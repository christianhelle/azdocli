use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use azure_devops_rust_api::wiki::models::{
    wiki_create_base_parameters, WikiCreateParametersV2, WikiV2,
};
use std::path::Path;
use std::process::Command;

use crate::auth::factory::ClientFactory;
use crate::auth::url::normalize_base_url;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct WikisPhase;

#[async_trait]
impl Phase for WikisPhase {
    fn name(&self) -> &'static str {
        "wikis"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let source_client = ctx.source_factory()?.build_wiki();
        let target_client = ctx.target_factory()?.build_wiki();

        let source_wikis = ctx
            .executor
            .retry(|| async {
                source_client
                    .wikis_client()
                    .list(&ctx.source_creds.organization, &ctx.opts.source_project)
                    .await
                    .map(|w| w.value)
                    .map_err(|e| anyhow!("Listing source wikis: {e}"))
            })
            .await?;

        let Some(source_wiki) = source_wikis
            .iter()
            .find(|wiki| is_project_wiki(wiki))
            .cloned()
        else {
            return Ok(PhaseSummary::default());
        };

        let mut summary = PhaseSummary {
            items_total: 1,
            ..Default::default()
        };

        if ctx.opts.dry_run {
            println!(
                "  ⓘ would migrate project wiki '{}'.",
                wiki_name(&source_wiki, &ctx.opts.source_project)
            );
            summary.record_success();
            return Ok(summary);
        }

        let tmp_root = ctx.output_dir.join("wikis-tmp");
        std::fs::create_dir_all(&tmp_root)
            .with_context(|| format!("Creating wiki temp dir '{}'", tmp_root.display()))?;

        match migrate_project_wiki(ctx, &target_client, &source_wiki, &tmp_root).await {
            Ok(target_wiki) => {
                ctx.state.id_map_mut("wikis").map.insert(
                    wiki_id_or_name(&source_wiki, &ctx.opts.source_project),
                    wiki_id_or_name(&target_wiki, &ctx.opts.target_project),
                );
                summary.record_success();
            }
            Err(e) => summary.record_failure(format!(
                "Project wiki '{}': {e:#}",
                wiki_name(&source_wiki, &ctx.opts.source_project)
            )),
        }

        Ok(summary)
    }
}

async fn migrate_project_wiki(
    ctx: &MigrationContext,
    target_client: &azure_devops_rust_api::wiki::Client,
    source_wiki: &WikiV2,
    tmp_root: &Path,
) -> Result<WikiV2> {
    let target_wiki = ensure_target_project_wiki(ctx, target_client).await?;
    let source_repo = wiki_repo_name(source_wiki, &ctx.opts.source_project);
    let target_repo = wiki_repo_name(&target_wiki, &ctx.opts.target_project);

    let target_url = ado_git_url(
        ctx.target_base_url(),
        &ctx.target_creds.organization,
        &ctx.opts.target_project,
        &target_repo,
        &ctx.target_creds.pat,
    );

    if !remote_is_empty(&target_url, &ctx.target_creds.pat)? {
        return Err(anyhow!(
            "target project wiki backing repository '{}' is not empty; refusing destructive mirror push",
            target_repo
        ));
    }

    let source_url = ado_git_url(
        ctx.source_base_url(),
        &ctx.source_creds.organization,
        &ctx.opts.source_project,
        &source_repo,
        &ctx.source_creds.pat,
    );
    let tmp_dir = tmp_root.join(format!(
        "{}-{}.git",
        sanitize_path_segment(&source_repo),
        sanitize_path_segment(&wiki_id_or_name(source_wiki, &ctx.opts.source_project))
    ));

    mirror_push(
        &source_url,
        &target_url,
        &tmp_dir,
        &ctx.source_creds.pat,
        &ctx.target_creds.pat,
    )?;
    Ok(target_wiki)
}

async fn ensure_target_project_wiki(
    ctx: &MigrationContext,
    target_client: &azure_devops_rust_api::wiki::Client,
) -> Result<WikiV2> {
    let target_wikis = ctx
        .executor
        .retry(|| async {
            target_client
                .wikis_client()
                .list(&ctx.target_creds.organization, &ctx.opts.target_project)
                .await
                .map(|w| w.value)
                .map_err(|e| anyhow!("Listing target wikis: {e}"))
        })
        .await?;

    if let Some(wiki) = target_wikis.into_iter().find(is_project_wiki) {
        return Ok(wiki);
    }

    let mut body = WikiCreateParametersV2::new();
    body.wiki_create_base_parameters.name = Some(ctx.opts.target_project.clone());
    body.wiki_create_base_parameters.type_ = Some(wiki_create_base_parameters::Type::ProjectWiki);

    ctx.executor
        .retry(|| async {
            target_client
                .wikis_client()
                .create(
                    &ctx.target_creds.organization,
                    body.clone(),
                    &ctx.opts.target_project,
                )
                .await
                .map_err(|e| anyhow!("Creating target project wiki: {e}"))
        })
        .await
}

fn is_project_wiki(wiki: &WikiV2) -> bool {
    matches!(
        wiki.wiki_create_base_parameters.type_,
        Some(wiki_create_base_parameters::Type::ProjectWiki)
    )
}

fn wiki_id_or_name(wiki: &WikiV2, project: &str) -> String {
    wiki.id.clone().unwrap_or_else(|| wiki_name(wiki, project))
}

fn wiki_name(wiki: &WikiV2, project: &str) -> String {
    wiki.wiki_create_base_parameters
        .name
        .clone()
        .unwrap_or_else(|| project.to_string())
}

fn wiki_repo_name(wiki: &WikiV2, project: &str) -> String {
    let name = wiki_name(wiki, project);
    if name.ends_with(".wiki") {
        name
    } else {
        format!("{project}.wiki")
    }
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
    let output = run_git(&mut cmd, "checking target wiki refs", &[pat])?;
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

fn ado_git_url(base_url: &str, organization: &str, project: &str, repo: &str, pat: &str) -> String {
    let normalized = normalize_base_url(base_url);
    let scheme = if normalized.starts_with("http://") {
        "http"
    } else {
        "https"
    };
    let host_and_path = normalized
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    format!(
        "{scheme}://azdocli:{}@{}/{}/{}/_git/{}",
        percent_encode(pat),
        host_and_path,
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

#[cfg(test)]
mod tests {
    use super::ado_git_url;

    #[test]
    fn ado_git_url_preserves_https_scheme() {
        let url = ado_git_url(
            "https://dev.azure.com",
            "myorg",
            "myproject",
            "myrepo.wiki",
            "pat",
        );
        assert_eq!(
            url,
            "https://azdocli:pat@dev.azure.com/myorg/myproject/_git/myrepo.wiki"
        );
    }

    #[test]
    fn ado_git_url_preserves_http_scheme() {
        let url = ado_git_url(
            "http://azure-devops.company.local",
            "myorg",
            "myproject",
            "myrepo.wiki",
            "pat",
        );
        assert_eq!(
            url,
            "http://azdocli:pat@azure-devops.company.local/myorg/myproject/_git/myrepo.wiki"
        );
    }
}
