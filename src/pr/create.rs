//! Creating pull requests.

use super::{http, identity};
use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::{get_credentials, Credentials};
use crate::repos;
use anyhow::Result;
use azure_devops_rust_api::git::models::{
    GitPullRequestCompletionOptions, GitPullRequestCreateOptions, IdentityId, ResourceRef,
    WebApiCreateTagRequestData,
};
use colored::Colorize;
use serde_json::json;

/// The title given to a pull request created without `--title`.
const DEFAULT_TITLE: &str = "Pull Request";

/// Everything `repos pr create` accepts, after the description has been
/// resolved and the reviewers have been turned into identity IDs.
pub(super) struct CreateSettings<'a> {
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub source_ref: &'a str,
    pub target_ref: &'a str,
    pub draft: bool,
    pub reviewer_ids: &'a [String],
    pub work_items: &'a [i32],
    pub labels: &'a [String],
    pub delete_source_branch: bool,
}

/// Expands a branch name into a fully qualified ref name.
pub(super) fn branch_ref(branch: &str) -> String {
    if branch.starts_with("refs/heads/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    }
}

/// Builds the Azure DevOps create payload for a pull request.
pub(super) fn build_create_options(settings: &CreateSettings<'_>) -> GitPullRequestCreateOptions {
    let completion_options =
        settings
            .delete_source_branch
            .then(|| GitPullRequestCompletionOptions {
                delete_source_branch: Some(true),
                ..Default::default()
            });

    GitPullRequestCreateOptions {
        source_ref_name: settings.source_ref.to_string(),
        target_ref_name: settings.target_ref.to_string(),
        title: settings.title.unwrap_or(DEFAULT_TITLE).to_string(),
        description: settings.description.map(|d| d.to_string()),
        is_draft: Some(settings.draft),
        labels: settings
            .labels
            .iter()
            .map(|name| WebApiCreateTagRequestData { name: name.clone() })
            .collect(),
        merge_options: None,
        completion_options,
        work_item_refs: settings
            .work_items
            .iter()
            .map(|id| ResourceRef {
                id: Some(id.to_string()),
                url: None,
            })
            .collect(),
        reviewers: settings
            .reviewer_ids
            .iter()
            .map(|id| IdentityId { id: id.clone() })
            .collect(),
    }
}

/// Creates a new pull request in the specified repository.
#[allow(clippy::too_many_arguments)]
pub(super) async fn create_pull_request(
    project: &str,
    repo: &str,
    title: Option<&str>,
    description: Option<&str>,
    source: &str,
    target: &str,
    draft: bool,
    reviewers: &[String],
    work_items: &[i32],
    labels: &[String],
    auto_complete: bool,
    delete_source_branch: bool,
) -> Result<()> {
    let creds = match get_credentials() {
        Ok(creds) => creds,
        Err(e) => {
            eprintln!("Unable to create pull request");
            return Err(e);
        }
    };

    let client = CredentialClientFactory::new(&creds)?.build_git();
    let repository = repos::get_repo(project, repo).await?;

    let mut reviewer_ids = Vec::with_capacity(reviewers.len());
    for reviewer in reviewers {
        reviewer_ids.push(identity::resolve_identity(&creds, reviewer).await?);
    }

    let source_ref = branch_ref(source);
    let target_ref = branch_ref(target);

    println!("Creating pull request:");
    println!("  Repository: {repo}");
    println!("  Source branch: {source}");
    println!("  Target branch: {target}");
    println!("  Title: {}", title.unwrap_or(DEFAULT_TITLE));
    if draft {
        println!("  Draft: yes");
    }
    if !reviewers.is_empty() {
        println!("  Reviewers: {}", reviewers.join(", "));
    }
    if !work_items.is_empty() {
        println!(
            "  Work items: {}",
            work_items
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !labels.is_empty() {
        println!("  Labels: {}", labels.join(", "));
    }

    let pr_options = build_create_options(&CreateSettings {
        title,
        description,
        source_ref: &source_ref,
        target_ref: &target_ref,
        draft,
        reviewer_ids: &reviewer_ids,
        work_items,
        labels,
        delete_source_branch,
    });

    let created_pr = match client
        .pull_requests_client()
        .create(&creds.organization, &repository.id, project, pr_options)
        .await
    {
        Ok(created_pr) => created_pr,
        Err(e) => {
            eprintln!("{}", format!("❌ Failed to create pull request: {e}").red());
            return Err(anyhow::anyhow!("Failed to create pull request: {}", e));
        }
    };

    println!("{}", "✅ Pull request created successfully!".green());
    println!("  ID: {}", created_pr.pull_request_id);
    println!("  Title: {}", created_pr.title.clone().unwrap_or_default());
    println!("  URL: {}", created_pr.url);

    if auto_complete {
        arm_auto_complete(
            &creds,
            project,
            &repository.id,
            created_pr.pull_request_id,
            delete_source_branch,
        )
        .await?;
    }

    Ok(())
}

/// Sets a freshly created pull request to complete automatically.
///
/// `GitPullRequestCreateOptions` has no `autoCompleteSetBy` field, so this is a
/// follow-up update rather than part of the create call.
async fn arm_auto_complete(
    creds: &Credentials,
    project: &str,
    repository_id: &str,
    pull_request_id: i32,
    delete_source_branch: bool,
) -> Result<()> {
    let identity = http::authenticated_user_id(creds).await?;
    let mut completion_options = serde_json::Map::new();
    if delete_source_branch {
        completion_options.insert("deleteSourceBranch".to_string(), json!(true));
    }

    let body = json!({
        "autoCompleteSetBy": { "id": identity },
        "completionOptions": completion_options,
    });

    http::patch_pull_request(creds, project, repository_id, pull_request_id, &body).await?;
    println!("{}", "✅ Auto-complete enabled.".green());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> CreateSettings<'static> {
        CreateSettings {
            title: None,
            description: None,
            source_ref: "refs/heads/feature",
            target_ref: "refs/heads/main",
            draft: false,
            reviewer_ids: &[],
            work_items: &[],
            labels: &[],
            delete_source_branch: false,
        }
    }

    #[test]
    fn branch_ref_expands_short_names() {
        assert_eq!(branch_ref("feature/x"), "refs/heads/feature/x");
    }

    #[test]
    fn branch_ref_leaves_qualified_names_alone() {
        assert_eq!(branch_ref("refs/heads/main"), "refs/heads/main");
    }

    #[test]
    fn build_create_options_maps_title_and_description() {
        let options = build_create_options(&CreateSettings {
            title: Some("My title"),
            description: Some("My description"),
            ..settings()
        });

        assert_eq!(options.title, "My title");
        assert_eq!(options.description, Some("My description".to_string()));
        assert_eq!(options.source_ref_name, "refs/heads/feature");
        assert_eq!(options.target_ref_name, "refs/heads/main");
    }

    #[test]
    fn build_create_options_defaults_title_and_is_not_a_draft() {
        let options = build_create_options(&settings());

        assert_eq!(options.title, DEFAULT_TITLE);
        assert_eq!(options.description, None);
        assert_eq!(options.is_draft, Some(false));
        assert!(options.completion_options.is_none());
    }

    #[test]
    fn build_create_options_marks_drafts() {
        let options = build_create_options(&CreateSettings {
            draft: true,
            ..settings()
        });

        assert_eq!(options.is_draft, Some(true));
    }

    #[test]
    fn build_create_options_maps_reviewers_work_items_and_labels() {
        let reviewer_ids = vec!["id-1".to_string(), "id-2".to_string()];
        let labels = vec!["bug".to_string()];
        let work_items = vec![11, 22];

        let options = build_create_options(&CreateSettings {
            reviewer_ids: &reviewer_ids,
            work_items: &work_items,
            labels: &labels,
            ..settings()
        });

        assert_eq!(
            options
                .reviewers
                .iter()
                .map(|reviewer| reviewer.id.as_str())
                .collect::<Vec<_>>(),
            vec!["id-1", "id-2"]
        );
        assert_eq!(
            options
                .work_item_refs
                .iter()
                .map(|item| item.id.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["11", "22"]
        );
        assert_eq!(
            options
                .labels
                .iter()
                .map(|label| label.name.as_str())
                .collect::<Vec<_>>(),
            vec!["bug"]
        );
    }

    #[test]
    fn build_create_options_requests_source_branch_deletion() {
        let options = build_create_options(&CreateSettings {
            delete_source_branch: true,
            ..settings()
        });

        assert_eq!(
            options
                .completion_options
                .and_then(|completion| completion.delete_source_branch),
            Some(true)
        );
    }
}
