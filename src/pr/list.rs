//! Listing pull requests and their commits.

use super::create_git_client;
use super::parse_pr_id;
use crate::auth::get_credentials;
use anyhow::Result;

/// Lists pull requests for a repository.
pub(super) async fn list_pull_requests(project: &str, repo: &str) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;
            let pr_client = client.pull_requests_client();

            let pull_requests = pr_client
                .get_pull_requests_by_project(creds.organization, project)
                .await?;

            let filtered_prs: Vec<_> = pull_requests
                .value
                .into_iter()
                .filter(|pr| pr.repository.name == repo)
                .collect();

            if filtered_prs.is_empty() {
                println!("No pull requests found for repository '{repo}' in project '{project}'");
            } else {
                println!("Pull requests for repository '{repo}' in project '{project}':");
                for pr in filtered_prs {
                    println!(
                        "  #{} - {}",
                        pr.pull_request_id,
                        pr.title.unwrap_or_default()
                    );
                }
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to retrieve pull requests");
            Err(e)
        }
    }
}

/// Lists commits in a pull request.
pub(super) async fn list_pull_request_commits(
    repo: &String,
    id: &String,
    project_name: String,
) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;
            let pr_client = client.pull_request_commits_client();

            let pr_id = parse_pr_id(id)?;

            let commits = pr_client
                .get_pull_request_commits(creds.organization, repo, pr_id, project_name)
                .await?;

            if commits.value.is_empty() {
                println!("No commits found for pull request ID {id}");
            } else {
                println!("Commits in pull request ID {id}:");
                for commit in commits.value {
                    if let Some(commit_id) = commit.commit_id {
                        println!("  - Commit ID: {commit_id}");
                    }
                    if let Some(message) = commit.comment {
                        println!("    Message: {message}");
                    }
                    if let Some(author) = commit.author {
                        println!(
                            "    Author: {} ({})",
                            author.name.unwrap_or_else(|| "Unknown".to_string()),
                            author.email.unwrap_or_default()
                        );
                    }
                }
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to retrieve commits: {e}");
            Err(e)
        }
    }
}
