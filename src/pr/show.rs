//! Showing details of a single pull request.

use super::create_git_client;
use super::parse_pr_id;
use crate::auth::get_credentials;
use anyhow::Result;

/// Shows details of a specific pull request.
pub(super) async fn show_pull_request(project: &str, _repo: &str, id: &str) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;
            let pr_client = client.pull_requests_client();

            let pr_id = parse_pr_id(id)?;

            let pull_request = pr_client
                .get_pull_request_by_id(creds.organization, pr_id, project)
                .await?;

            println!("Pull Request Details:");
            println!("  ID: {}", pull_request.pull_request_id);
            println!("  Title: {}", pull_request.title.unwrap_or_default());
            if let Some(description) = pull_request.description {
                if !description.is_empty() {
                    println!("  Description: {description}");
                }
            }
            println!("  Status: {:?}", pull_request.status);
            println!("  Source Branch: {}", pull_request.source_ref_name);
            println!("  Target Branch: {}", pull_request.target_ref_name);
            println!("  Created: {}", pull_request.creation_date);

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to retrieve pull request");
            Err(e)
        }
    }
}
