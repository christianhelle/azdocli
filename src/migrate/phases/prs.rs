use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use azure_devops_rust_api::git::{
    models::{git_pull_request, GitPullRequest, GitPullRequestCreateOptions, GitRepository},
    Client, ClientBuilder as GitClientBuilder,
};
use serde_json::Value;

use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

const PAGE_SIZE: i32 = 100;

pub struct PrsPhase;

#[async_trait]
impl Phase for PrsPhase {
    fn name(&self) -> &'static str {
        "prs"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let source_client = GitClientBuilder::new(ctx.source_credential.clone()).build();
        let target_client = GitClientBuilder::new(ctx.target_credential.clone()).build();
        let repo_map = ctx
            .state
            .id_map("repos")
            .map(|m| m.map.clone())
            .ok_or_else(|| anyhow!("PR migration requires populated repos id-map"))?;

        let source_repos = ctx
            .executor
            .retry(|| async {
                source_client
                    .repositories_client()
                    .list(&ctx.source_creds.organization, &ctx.opts.source_project)
                    .await
                    .map(|r| r.value)
                    .map_err(|e| anyhow!("Listing source repositories for PR migration: {e}"))
            })
            .await?;

        let migrated_repos: Vec<_> = source_repos
            .into_iter()
            .filter(|repo| repo_map.contains_key(&repo.id))
            .collect();

        let mut summary = PhaseSummary::default();
        let mut archive = Vec::new();

        for source_repo in migrated_repos {
            let Some(target_repo_id) = repo_map.get(&source_repo.id).cloned() else {
                continue;
            };

            let pull_requests = list_all_pull_requests(ctx, &source_client, &source_repo).await?;
            summary.items_total += pull_requests.len() as u64;

            for pr in pull_requests {
                if pr.status == git_pull_request::Status::Active {
                    match migrate_active_pr(ctx, &target_client, &target_repo_id, &pr).await {
                        Ok(target_pr_id) => {
                            ctx.state
                                .id_map_mut("prs")
                                .map
                                .insert(pr.pull_request_id.to_string(), target_pr_id);
                            summary.record_success();
                        }
                        Err(e) => summary.record_failure(format!(
                            "Pull request !{} in '{}': {e:#}",
                            pr.pull_request_id, source_repo.name
                        )),
                    }
                } else {
                    let archived = archive_closed_pr(ctx, &source_client, &source_repo, &pr).await;
                    archive.push(archived);
                    ctx.state
                        .id_map_mut("prs")
                        .map
                        .insert(pr.pull_request_id.to_string(), "archived".to_string());
                    summary.record_success();
                }
            }
        }

        write_archive(ctx, &archive)?;
        Ok(summary)
    }
}

async fn list_all_pull_requests(
    ctx: &MigrationContext,
    client: &Client,
    repo: &GitRepository,
) -> Result<Vec<GitPullRequest>> {
    let pr_client = client.pull_requests_client();
    let mut all = Vec::new();
    let mut skip = 0;

    loop {
        let page = ctx
            .executor
            .retry(|| async {
                pr_client
                    .get_pull_requests(
                        &ctx.source_creds.organization,
                        &repo.id,
                        &ctx.opts.source_project,
                    )
                    .search_criteria_status("all")
                    .top(PAGE_SIZE)
                    .skip(skip)
                    .await
                    .map(|r| r.value)
                    .map_err(|e| anyhow!("Listing pull requests for '{}': {e}", repo.name))
            })
            .await?;

        let count = page.len();
        all.extend(page);
        if count < PAGE_SIZE as usize {
            break;
        }
        skip += PAGE_SIZE;
    }

    Ok(all)
}

async fn migrate_active_pr(
    ctx: &MigrationContext,
    target_client: &Client,
    target_repo_id: &str,
    pr: &GitPullRequest,
) -> Result<String> {
    if ctx.opts.dry_run {
        println!(
            "  ⓘ would recreate active PR !{} '{}'",
            pr.pull_request_id,
            pr.title.as_deref().unwrap_or_default()
        );
        return Ok("dry-run".to_string());
    }

    ensure_branch_exists(ctx, target_client, target_repo_id, &pr.source_ref_name).await?;
    ensure_branch_exists(ctx, target_client, target_repo_id, &pr.target_ref_name).await?;

    let create_options = GitPullRequestCreateOptions {
        source_ref_name: pr.source_ref_name.clone(),
        target_ref_name: pr.target_ref_name.clone(),
        title: pr
            .title
            .clone()
            .unwrap_or_else(|| "Pull Request".to_string()),
        description: Some(migrated_description(ctx, pr)),
        is_draft: Some(pr.is_draft),
        labels: Vec::new(),
        merge_options: None,
        completion_options: None,
        work_item_refs: Vec::new(),
        reviewers: Vec::new(),
    };

    let created = ctx
        .executor
        .retry(|| async {
            target_client
                .pull_requests_client()
                .create(
                    &ctx.target_creds.organization,
                    target_repo_id,
                    &ctx.opts.target_project,
                    create_options.clone(),
                )
                .await
                .map_err(|e| anyhow!("Creating target PR: {e}"))
        })
        .await?;

    Ok(created.pull_request_id.to_string())
}

async fn ensure_branch_exists(
    ctx: &MigrationContext,
    target_client: &Client,
    target_repo_id: &str,
    ref_name: &str,
) -> Result<()> {
    let expected = full_ref_name(ref_name);
    let filter = ref_filter(&expected);
    let refs = ctx
        .executor
        .retry(|| async {
            target_client
                .refs_client()
                .list(
                    &ctx.target_creds.organization,
                    target_repo_id,
                    &ctx.opts.target_project,
                )
                .filter(&filter)
                .top(1)
                .await
                .map(|r| r.value)
                .map_err(|e| anyhow!("Checking target branch '{expected}': {e}"))
        })
        .await?;

    if refs.iter().any(|r| r.name == expected) {
        Ok(())
    } else {
        Err(anyhow!("target branch '{}' does not exist", expected))
    }
}

async fn archive_closed_pr(
    ctx: &MigrationContext,
    source_client: &Client,
    source_repo: &GitRepository,
    pr: &GitPullRequest,
) -> Value {
    let mut value = serde_json::to_value(pr).unwrap_or_else(|e| {
        serde_json::json!({
            "pullRequestId": pr.pull_request_id,
            "serializationError": e.to_string()
        })
    });

    if let Value::Object(map) = &mut value {
        map.insert(
            "sourceRepositoryId".to_string(),
            Value::String(source_repo.id.clone()),
        );
        map.insert(
            "sourceRepositoryName".to_string(),
            Value::String(source_repo.name.clone()),
        );

        match source_client
            .pull_request_commits_client()
            .get_pull_request_commits(
                &ctx.source_creds.organization,
                &source_repo.id,
                pr.pull_request_id,
                &ctx.opts.source_project,
            )
            .await
        {
            Ok(commits) => {
                map.insert(
                    "commitsList".to_string(),
                    serde_json::to_value(commits.value).unwrap_or(Value::Null),
                );
            }
            Err(e) => {
                map.insert(
                    "commitsFetchError".to_string(),
                    Value::String(format!("{e}")),
                );
            }
        }
    }

    value
}

fn write_archive(ctx: &MigrationContext, archive: &[Value]) -> Result<()> {
    let path = ctx.output_dir.join("prs-closed-archive.json");
    let json = serde_json::to_string_pretty(archive)?;
    std::fs::write(&path, json).with_context(|| format!("Writing '{}'", path.display()))
}

fn migrated_description(ctx: &MigrationContext, pr: &GitPullRequest) -> String {
    let mut description = pr.description.clone().unwrap_or_default();
    description.push_str(&format!(
        "\n\n---\nMigrated from {}/{} PR !{}\nOriginally created by {} on {}\n",
        ctx.source_creds.organization,
        ctx.opts.source_project,
        pr.pull_request_id,
        pr.created_by
            .graph_subject_base
            .display_name
            .as_deref()
            .unwrap_or("unknown"),
        pr.creation_date
    ));
    description
}

fn full_ref_name(ref_name: &str) -> String {
    if ref_name.starts_with("refs/") {
        ref_name.to_string()
    } else {
        format!("refs/{ref_name}")
    }
}

fn ref_filter(ref_name: &str) -> String {
    full_ref_name(ref_name)
        .strip_prefix("refs/")
        .unwrap_or(ref_name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{full_ref_name, ref_filter};

    #[test]
    fn ref_filter_uses_azure_devops_refs_filter_format() {
        assert_eq!(ref_filter("refs/heads/main"), "heads/main");
        assert_eq!(ref_filter("heads/feature/x"), "heads/feature/x");
    }

    #[test]
    fn full_ref_name_preserves_existing_refs_prefix() {
        assert_eq!(full_ref_name("refs/heads/main"), "refs/heads/main");
        assert_eq!(full_ref_name("heads/main"), "refs/heads/main");
    }
}
