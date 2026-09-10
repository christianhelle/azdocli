//! Managing the work items linked to a pull request.
//!
//! Azure DevOps models the link as an `ArtifactLink` relation on the *work
//! item*, not as a field on the pull request: `GitPullRequestUpdateOptions` has
//! no `workItemRefs`, and the SDK's `pull_request_work_items_client` only
//! exposes `list`. Reading the links therefore goes through the Git API, while
//! adding and removing them goes through the work item tracking API.

use super::PrContext;
use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use anyhow::Result;
use azure_devops_rust_api::wit::models::WorkItem;
use clap::Subcommand;

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
}

/// Routes work item subcommands to their handlers.
pub(super) async fn handle_command(subcommand: &WorkItemSubCommands) -> Result<()> {
    match subcommand {
        WorkItemSubCommands::List { project, repo, id } => {
            list_work_items(project.as_deref(), repo, id).await
        }
    }
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
