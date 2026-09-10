//! Managing the work items linked to a pull request.
//!
//! Azure DevOps models the link as an `ArtifactLink` relation on the *work
//! item*, not as a field on the pull request: `GitPullRequestUpdateOptions` has
//! no `workItemRefs`, and the SDK's `pull_request_work_items_client` only
//! exposes `list`. Reading the links therefore goes through the Git API, while
//! adding and removing them goes through the work item tracking API.

use super::PrContext;
use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::models::{json_patch_operation::Op, JsonPatchOperation, WorkItem};
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;

#[derive(Subcommand, Clone)]
pub enum WorkItemSubCommands {
    /// List the work items linked to a pull request
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request
        #[clap(short, long)]
        id: String,
    },
    /// Link one or more work items to a pull request
    Add {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request
        #[clap(short, long)]
        id: String,

        /// ID of a work item to link (repeat, or separate with commas)
        #[clap(long, required = true, value_delimiter = ',')]
        work_item: Vec<i32>,
    },
}

/// Routes work item subcommands to their handlers.
pub(super) async fn handle_command(subcommand: &WorkItemSubCommands) -> Result<()> {
    match subcommand {
        WorkItemSubCommands::List { project, repo, id } => {
            list_work_items(project.as_deref(), repo, id).await
        }
        WorkItemSubCommands::Add {
            project,
            repo,
            id,
            work_item,
        } => add_work_items(project.as_deref(), repo, id, work_item).await,
    }
}

/// The artifact URI Azure DevOps uses to identify a pull request.
///
/// The separators are percent-encoded because the whole `{project}/{repo}/{id}`
/// triple is a single artifact identifier, not three path segments.
fn artifact_link_url(project_id: &str, repository_id: &str, pull_request_id: i32) -> String {
    format!("vstfs:///Git/PullRequestId/{project_id}%2F{repository_id}%2F{pull_request_id}")
}

/// Builds the patch that attaches a pull request to a work item.
fn add_relation_patch(artifact_url: &str) -> Vec<JsonPatchOperation> {
    vec![JsonPatchOperation {
        from: None,
        op: Some(Op::Add),
        path: Some("/relations/-".to_string()),
        value: Some(json!({
            "rel": "ArtifactLink",
            "url": artifact_url,
            "attributes": { "name": "Pull Request" },
        })),
    }]
}

/// Links work items to a pull request by adding an artifact link to each one.
async fn add_work_items(
    project: Option<&str>,
    repo: &str,
    id: &str,
    work_items: &[i32],
) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;
    let client = CredentialClientFactory::new(&ctx.creds)?.build_wit();
    let artifact_url = artifact_link_url(&ctx.project_id, &ctx.repository_id, ctx.pull_request_id);

    for work_item in work_items {
        client
            .work_items_client()
            .update(
                &ctx.creds.organization,
                add_relation_patch(&artifact_url),
                *work_item,
                &ctx.project,
            )
            .await
            .map_err(|e| anyhow!("Linking work item {work_item} to the pull request: {e}"))?;

        println!(
            "{}",
            format!(
                "✅ Linked work item {work_item} to pull request {}",
                ctx.pull_request_id
            )
            .green()
        );
    }

    Ok(())
}

/// Lists the work items linked to a pull request, with their type and state.
async fn list_work_items(project: Option<&str>, repo: &str, id: &str) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;
    let ids = linked_work_item_ids(&ctx).await?;

    if ids.is_empty() {
        crate::boards::display_empty_work_items_table("Linked Work Items");
        return Ok(());
    }

    let work_items = fetch_work_items(&ctx, &ids).await?;
    crate::boards::display_work_items_list("Linked Work Items", &work_items);
    Ok(())
}

/// Returns the IDs of the work items linked to the pull request.
async fn linked_work_item_ids(ctx: &PrContext) -> Result<Vec<i32>> {
    let refs = ctx
        .client
        .pull_request_work_items_client()
        .list(
            &ctx.creds.organization,
            &ctx.repository_id,
            ctx.pull_request_id,
            &ctx.project,
        )
        .await?
        .value;

    Ok(refs
        .iter()
        .filter_map(|item| item.id.as_deref())
        .filter_map(|id| id.parse::<i32>().ok())
        .collect())
}

/// Fetches the full work items behind a set of IDs, skipping any that fail.
async fn fetch_work_items(ctx: &PrContext, ids: &[i32]) -> Result<Vec<WorkItem>> {
    let client = CredentialClientFactory::new(&ctx.creds)?.build_wit();

    let mut work_items = Vec::with_capacity(ids.len());
    for id in ids {
        match client
            .work_items_client()
            .get_work_item(&ctx.creds.organization, *id, &ctx.project)
            .await
        {
            Ok(work_item) => work_items.push(work_item),
            Err(e) => eprintln!("❌ Failed to get details for work item {id}: {e}"),
        }
    }

    Ok(work_items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_devops_rust_api::wit::models::json_patch_operation::Op;

    #[test]
    fn artifact_link_url_encodes_the_separators() {
        assert_eq!(
            artifact_link_url("proj-guid", "repo-guid", 123),
            "vstfs:///Git/PullRequestId/proj-guid%2Frepo-guid%2F123"
        );
    }

    #[test]
    fn add_relation_patch_adds_a_named_artifact_link() {
        let patch = add_relation_patch("vstfs:///Git/PullRequestId/p%2Fr%2F1");

        assert_eq!(patch.len(), 1);
        assert_eq!(patch[0].op, Some(Op::Add));
        assert_eq!(patch[0].path.as_deref(), Some("/relations/-"));

        let value = patch[0].value.as_ref().expect("a relation value");
        assert_eq!(value["rel"], "ArtifactLink");
        assert_eq!(value["url"], "vstfs:///Git/PullRequestId/p%2Fr%2F1");
        assert_eq!(value["attributes"]["name"], "Pull Request");
    }
}
