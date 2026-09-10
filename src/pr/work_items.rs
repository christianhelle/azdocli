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
use azure_devops_rust_api::wit::models::{
    json_patch_operation::Op, JsonPatchOperation, WorkItem, WorkItemRelation,
};
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
    /// Unlink one or more work items from a pull request
    Remove {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request
        #[clap(short, long)]
        id: String,

        /// ID of a work item to unlink (repeat, or separate with commas)
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
        WorkItemSubCommands::Remove {
            project,
            repo,
            id,
            work_item,
        } => remove_work_items(project.as_deref(), repo, id, work_item).await,
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

/// Builds the patch that detaches the relation at `index` from a work item.
fn remove_relation_patch(index: usize) -> Vec<JsonPatchOperation> {
    vec![JsonPatchOperation {
        from: None,
        op: Some(Op::Remove),
        path: Some(format!("/relations/{index}")),
        value: None,
    }]
}

/// Finds the position of the artifact link pointing at `artifact_url`.
///
/// Azure DevOps does not guarantee the casing of the GUIDs it stores, and links
/// created by older clients record the separators unencoded, so both forms are
/// normalized before comparing.
fn find_artifact_relation_index(
    relations: &[WorkItemRelation],
    artifact_url: &str,
) -> Option<usize> {
    let wanted = normalize_artifact_url(artifact_url);

    relations.iter().position(|relation| {
        relation.link.rel == "ArtifactLink" && normalize_artifact_url(&relation.link.url) == wanted
    })
}

/// Reduces an artifact URI to a form that can be compared for equality.
fn normalize_artifact_url(url: &str) -> String {
    url.to_lowercase().replace("%2f", "/")
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
        // Azure DevOps rejects a duplicate relation outright, so an item that is
        // already linked is skipped rather than failing the whole command.
        let existing = client
            .work_items_client()
            .get_work_item(&ctx.creds.organization, *work_item, &ctx.project)
            .expand("Relations")
            .await
            .map_err(|e| anyhow!("Fetching work item {work_item}: {e}"))?;

        if find_artifact_relation_index(&existing.relations, &artifact_url).is_some() {
            println!(
                "{}",
                format!(
                    "⚠ Work item {work_item} is already linked to pull request {}",
                    ctx.pull_request_id
                )
                .yellow()
            );
            continue;
        }

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

/// Unlinks work items from a pull request by removing their artifact link.
async fn remove_work_items(
    project: Option<&str>,
    repo: &str,
    id: &str,
    work_items: &[i32],
) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;
    let client = CredentialClientFactory::new(&ctx.creds)?.build_wit();
    let artifact_url = artifact_link_url(&ctx.project_id, &ctx.repository_id, ctx.pull_request_id);

    for work_item in work_items {
        // Relations are only returned when they are explicitly expanded.
        let existing = client
            .work_items_client()
            .get_work_item(&ctx.creds.organization, *work_item, &ctx.project)
            .expand("Relations")
            .await
            .map_err(|e| anyhow!("Fetching work item {work_item}: {e}"))?;

        let Some(index) = find_artifact_relation_index(&existing.relations, &artifact_url) else {
            return Err(anyhow!(
                "Work item {work_item} is not linked to pull request {}",
                ctx.pull_request_id
            ));
        };

        client
            .work_items_client()
            .update(
                &ctx.creds.organization,
                remove_relation_patch(index),
                *work_item,
                &ctx.project,
            )
            .await
            .map_err(|e| anyhow!("Unlinking work item {work_item} from the pull request: {e}"))?;

        println!(
            "{}",
            format!(
                "✅ Unlinked work item {work_item} from pull request {}",
                ctx.pull_request_id
            )
            .green()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_devops_rust_api::wit::models::{json_patch_operation::Op, Link, WorkItemRelation};

    #[test]
    fn artifact_link_url_encodes_the_separators() {
        assert_eq!(
            artifact_link_url("proj-guid", "repo-guid", 123),
            "vstfs:///Git/PullRequestId/proj-guid%2Frepo-guid%2F123"
        );
    }

    fn relation(rel: &str, url: &str) -> WorkItemRelation {
        WorkItemRelation::new(Link::new(json!({}), rel.to_string(), url.to_string()))
    }

    #[test]
    fn find_artifact_relation_index_locates_the_pull_request_link() {
        let relations = vec![
            relation("AttachedFile", "https://example.test/attachments/a"),
            relation("ArtifactLink", "vstfs:///Git/PullRequestId/p%2Fr%2F1"),
        ];

        assert_eq!(
            find_artifact_relation_index(&relations, "vstfs:///Git/PullRequestId/p%2Fr%2F1"),
            Some(1)
        );
    }

    #[test]
    fn find_artifact_relation_index_ignores_casing_and_separator_encoding() {
        // Azure DevOps does not guarantee the casing of the GUIDs it stores, and
        // older links are recorded with unencoded separators.
        let relations = vec![relation(
            "ArtifactLink",
            "vstfs:///Git/PullRequestId/ABC/DEF/1",
        )];

        assert_eq!(
            find_artifact_relation_index(&relations, "vstfs:///Git/PullRequestId/abc%2Fdef%2F1"),
            Some(0)
        );
    }

    #[test]
    fn find_artifact_relation_index_does_not_match_another_pull_request() {
        let relations = vec![
            relation("ArtifactLink", "vstfs:///Git/PullRequestId/p%2Fr%2F2"),
            relation(
                "System.LinkTypes.Related",
                "vstfs:///Git/PullRequestId/p%2Fr%2F1",
            ),
        ];

        assert_eq!(
            find_artifact_relation_index(&relations, "vstfs:///Git/PullRequestId/p%2Fr%2F1"),
            None
        );
    }

    #[test]
    fn a_link_added_by_the_add_patch_is_found_again() {
        // The URL written on add and the URL matched on remove have to agree,
        // otherwise a link could be created but never detected or deleted.
        let url = artifact_link_url("proj-guid", "repo-guid", 123);
        let patch = add_relation_patch(&url);
        let written = patch[0].value.as_ref().unwrap();
        let relations = vec![relation(
            written["rel"].as_str().unwrap(),
            written["url"].as_str().unwrap(),
        )];

        assert_eq!(find_artifact_relation_index(&relations, &url), Some(0));
    }

    #[test]
    fn remove_relation_patch_removes_the_relation_at_the_index() {
        let patch = remove_relation_patch(2);

        assert_eq!(patch.len(), 1);
        assert_eq!(patch[0].op, Some(Op::Remove));
        assert_eq!(patch[0].path.as_deref(), Some("/relations/2"));
        assert!(patch[0].value.is_none());
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
