//! Creating pull requests.

use super::create_git_client;
use crate::auth::get_credentials;
use crate::repos;
use anyhow::Result;
use azure_devops_rust_api::git;

/// Creates a new pull request in the specified repository.
pub(super) async fn create_pull_request(
    project: &str,
    repo: &str,
    title: Option<&str>,
    description: Option<&str>,
    source: &str,
    target: &str,
) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;

            let repository = repos::get_repo(project, repo).await?;

            let pr_client = client.pull_requests_client();

            let source_ref = branch_ref(source);
            let target_ref = branch_ref(target);

            println!("Creating pull request:");
            println!("  Repository: {repo}");
            println!("  Source branch: {source}");
            println!("  Target branch: {target}");
            println!("  Title: {}", title.unwrap_or("Default title"));

            let pr_options = build_create_options(title, description, &source_ref, &target_ref);

            match pr_client
                .create(&creds.organization, &repository.id, project, pr_options)
                .await
            {
                Ok(created_pr) => {
                    println!("✅ Pull request created successfully!");
                    println!("  ID: {}", created_pr.pull_request_id);
                    println!("  Title: {}", created_pr.title.unwrap_or_default());
                    println!("  URL: {}", created_pr.url);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Failed to create pull request: {e}");
                    Err(anyhow::anyhow!("Failed to create pull request: {}", e))
                }
            }
        }
        Err(e) => {
            eprintln!("Unable to create pull request");
            Err(e)
        }
    }
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
pub(super) fn build_create_options(
    title: Option<&str>,
    description: Option<&str>,
    source_ref: &str,
    target_ref: &str,
) -> git::models::GitPullRequestCreateOptions {
    git::models::GitPullRequestCreateOptions {
        source_ref_name: source_ref.to_string(),
        target_ref_name: target_ref.to_string(),
        title: title.unwrap_or("Pull Request").to_string(),
        description: description.map(|d| d.to_string()),
        is_draft: Some(false),
        labels: Vec::new(),
        merge_options: None,
        completion_options: None,
        work_item_refs: Vec::new(),
        reviewers: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let options = build_create_options(
            Some("My title"),
            Some("My description"),
            "refs/heads/feature",
            "refs/heads/main",
        );

        assert_eq!(options.title, "My title");
        assert_eq!(options.description, Some("My description".to_string()));
        assert_eq!(options.source_ref_name, "refs/heads/feature");
        assert_eq!(options.target_ref_name, "refs/heads/main");
    }

    #[test]
    fn build_create_options_defaults_title() {
        let options = build_create_options(None, None, "refs/heads/feature", "refs/heads/main");

        assert_eq!(options.title, "Pull Request");
        assert_eq!(options.description, None);
    }
}
