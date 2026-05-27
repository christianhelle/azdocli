use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use azure_devops_rust_api::git::{
    models::{GitRepository, GitRepositoryCreateOptions},
    ClientBuilder as GitClientBuilder,
};
use std::path::Path;
use std::process::Command;

use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct ReposPhase;

#[async_trait]
impl Phase for ReposPhase {
    fn name(&self) -> &'static str {
        "repos"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let source_client = GitClientBuilder::new(ctx.source_credential.clone()).build();
        let target_client = GitClientBuilder::new(ctx.target_credential.clone()).build();

        let source_repos = ctx
            .executor
            .retry(|| async {
                source_client
                    .repositories_client()
                    .list(&ctx.source_creds.organization, &ctx.opts.source_project)
                    .await
                    .map(|r| r.value)
                    .map_err(|e| anyhow!("Listing source repositories: {e}"))
            })
            .await?;

        let mut target_repos = ctx
            .executor
            .retry(|| async {
                target_client
                    .repositories_client()
                    .list(&ctx.target_creds.organization, &ctx.opts.target_project)
                    .await
                    .map(|r| r.value)
                    .map_err(|e| anyhow!("Listing target repositories: {e}"))
            })
            .await?;

        let mut summary = PhaseSummary {
            items_total: source_repos.len() as u64,
            ..Default::default()
        };

        let tmp_root = ctx.output_dir.join("repos-tmp");
        if !ctx.opts.dry_run {
            std::fs::create_dir_all(&tmp_root)
                .with_context(|| format!("Creating repo temp dir '{}'", tmp_root.display()))?;
        }

        for source_repo in source_repos {
            let _permit = ctx.executor.permit().await;

            if ctx.opts.dry_run {
                println!("  ⓘ would migrate repository '{}'", source_repo.name);
                summary.record_success();
                continue;
            }

            match migrate_repository(
                ctx,
                &target_client,
                &mut target_repos,
                &source_repo,
                &tmp_root,
            )
            .await
            {
                Ok(target_repo) => {
                    ctx.state
                        .id_map_mut("repos")
                        .map
                        .insert(source_repo.id.clone(), target_repo.id.clone());
                    summary.record_success();
                }
                Err(e) => {
                    summary.record_failure(format!("Repository '{}': {e:#}", source_repo.name))
                }
            }
        }

        Ok(summary)
    }
}

async fn migrate_repository(
    ctx: &MigrationContext,
    target_client: &azure_devops_rust_api::git::Client,
    target_repos: &mut Vec<GitRepository>,
    source_repo: &GitRepository,
    tmp_root: &Path,
) -> Result<GitRepository> {
    let target_repo =
        ensure_target_repo(ctx, target_client, target_repos, &source_repo.name).await?;
    let target_url = ado_git_url(
        &ctx.target_creds.organization,
        &ctx.opts.target_project,
        &target_repo.name,
        &ctx.target_creds.pat,
    );

    if !remote_is_empty(&target_url, &ctx.target_creds.pat)? {
        return Err(anyhow!(
            "target repository '{}' is not empty; refusing destructive mirror push",
            target_repo.name
        ));
    }

    let source_url = ado_git_url(
        &ctx.source_creds.organization,
        &ctx.opts.source_project,
        &source_repo.name,
        &ctx.source_creds.pat,
    );
    let tmp_dir = tmp_root.join(format!(
        "{}-{}.git",
        sanitize_path_segment(&source_repo.name),
        sanitize_path_segment(&source_repo.id)
    ));

    mirror_push(
        &source_url,
        &target_url,
        &tmp_dir,
        &ctx.source_creds.pat,
        &ctx.target_creds.pat,
    )?;
    Ok(target_repo)
}

async fn ensure_target_repo(
    ctx: &MigrationContext,
    target_client: &azure_devops_rust_api::git::Client,
    target_repos: &mut Vec<GitRepository>,
    repo_name: &str,
) -> Result<GitRepository> {
    if let Some(repo) = target_repos.iter().find(|repo| repo.name == repo_name) {
        return Ok(repo.clone());
    }

    let created = ctx
        .executor
        .retry(|| async {
            target_client
                .repositories_client()
                .create(
                    &ctx.target_creds.organization,
                    GitRepositoryCreateOptions {
                        name: Some(repo_name.to_string()),
                        parent_repository: None,
                        project: None,
                    },
                    &ctx.opts.target_project,
                )
                .await
                .map_err(|e| anyhow!("Creating target repository '{repo_name}': {e}"))
        })
        .await?;

    target_repos.push(created.clone());
    Ok(created)
}

fn mirror_push(
    source_url: &str,
    target_url: &str,
    tmp_dir: &Path,
    source_pat: &str,
    target_pat: &str,
) -> Result<()> {
    if tmp_dir.exists() {
        std::fs::remove_dir_all(tmp_dir)
            .with_context(|| format!("Removing stale temp dir '{}'", tmp_dir.display()))?;
    }

    let mut clone = Command::new("git");
    clone
        .arg("clone")
        .arg("--mirror")
        .arg(source_url)
        .arg(tmp_dir);
    run_git(&mut clone, "mirror clone", &[source_pat, target_pat])?;

    let mut push = Command::new("git");
    push.current_dir(tmp_dir)
        .arg("push")
        .arg("--mirror")
        .arg(target_url);
    run_git(&mut push, "mirror push", &[source_pat, target_pat])?;

    std::fs::remove_dir_all(tmp_dir)
        .with_context(|| format!("Cleaning temp dir '{}'", tmp_dir.display()))?;
    Ok(())
}

fn remote_is_empty(remote_url: &str, pat: &str) -> Result<bool> {
    let mut cmd = Command::new("git");
    cmd.arg("ls-remote").arg(remote_url);
    let output = run_git(&mut cmd, "checking target repository refs", &[pat])?;
    Ok(output.trim().is_empty())
}

fn run_git(cmd: &mut Command, operation: &str, secrets: &[&str]) -> Result<String> {
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow!("Git command not found. Please ensure Git is installed and in your PATH.")
        } else {
            anyhow!("Failed to execute git {operation}: {e}")
        }
    })?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    let stderr = sanitize_secrets(&String::from_utf8_lossy(&output.stderr), secrets);
    let stdout = sanitize_secrets(&String::from_utf8_lossy(&output.stdout), secrets);
    let details = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };
    Err(anyhow!("git {operation} failed: {details}"))
}

fn ado_git_url(organization: &str, project: &str, repo: &str, pat: &str) -> String {
    format!(
        "https://azdocli:{}@dev.azure.com/{}/{}/_git/{}",
        percent_encode(pat),
        percent_encode(organization),
        percent_encode(project),
        percent_encode(repo)
    )
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn sanitize_secrets(value: &str, secrets: &[&str]) -> String {
    let mut sanitized = value.to_string();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        sanitized = sanitized.replace(secret, "***");
        sanitized = sanitized.replace(&percent_encode(secret), "***");
    }
    sanitized
}

fn sanitize_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
