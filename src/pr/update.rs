//! Updating the title and description of a pull request.

use super::parse_pr_id;
use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::repos;
use anyhow::Result;
use azure_devops_rust_api::git;
use colored::Colorize;

/// Builds the Azure DevOps update payload for a pull request.
pub(super) fn build_update_options(
    title: Option<&str>,
    description: Option<&str>,
) -> git::models::GitPullRequestUpdateOptions {
    git::models::GitPullRequestUpdateOptions {
        title: title.map(|t| t.to_string()),
        description: description.map(|d| d.to_string()),
        ..Default::default()
    }
}

/// Validates that at least one mutable field is provided.
pub(super) fn validate_update_has_changes(
    title: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    if title.is_none() && description.is_none() {
        return Err(anyhow::anyhow!(
            "At least one of --title or --description/--description-file must be provided"
        ));
    }
    Ok(())
}

/// Updates a pull request's title and/or description via the Azure DevOps Git API.
pub(super) async fn update_pull_request(
    project: &str,
    repo: &str,
    id: &str,
    title: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    validate_update_has_changes(title, description)?;

    match get_credentials() {
        Ok(creds) => {
            let factory = CredentialClientFactory::new(&creds)?;
            let client = factory.build_git();
            let repository = repos::get_repo(project, repo).await?;
            let pr_client = client.pull_requests_client();

            let pr_id = parse_pr_id(id)?;

            let options = build_update_options(title, description);

            println!("Updating pull request:");
            println!("  Repository: {repo}");
            println!("  ID: {id}");
            if let Some(t) = title {
                println!("  New Title: {t}");
            }
            if let Some(d) = description {
                if !d.is_empty() {
                    println!("  New Description: {d}");
                } else {
                    println!("  New Description: <empty> (clearing description)");
                }
            }

            match pr_client
                .update(&creds.organization, &repository.id, project, pr_id, options)
                .await
            {
                Ok(updated_pr) => {
                    println!("{}", "✓ Pull request updated successfully!".green());
                    println!("  ID: {}", updated_pr.pull_request_id);
                    println!("  Title: {}", updated_pr.title.unwrap_or_default());
                    println!("  URL: {}", updated_pr.url);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("{}", format!("❌ Failed to update pull request: {e}").red());
                    Err(anyhow::anyhow!("Failed to update pull request: {}", e))
                }
            }
        }
        Err(e) => {
            eprintln!("Unable to update pull request: {e}");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_update_options_maps_title_and_description() {
        let opts = build_update_options(Some("My Title"), Some("My Desc"));
        assert_eq!(opts.title, Some("My Title".to_string()));
        assert_eq!(opts.description, Some("My Desc".to_string()));
    }

    #[test]
    fn build_update_options_maps_none_fields() {
        let opts = build_update_options(None, None);
        assert_eq!(opts.title, None);
        assert_eq!(opts.description, None);
    }

    #[test]
    fn validate_update_requires_at_least_one_field() {
        assert!(validate_update_has_changes(None, None).is_err());
        assert!(validate_update_has_changes(Some("title"), None).is_ok());
        assert!(validate_update_has_changes(None, Some("desc")).is_ok());
        assert!(validate_update_has_changes(Some("t"), Some("d")).is_ok());
    }
}
