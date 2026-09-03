//! Listing pull requests and their commits.

use super::{create::branch_ref, create_git_client, identity, parse_pr_id, PrContext};
use crate::auth::get_credentials;
use anyhow::Result;
use azure_devops_rust_api::git::models::GitPullRequest;
use clap::ValueEnum;
use colored::Colorize;

/// The pull request states `repos pr list` can filter on.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum StatusFilter {
    /// Open pull requests
    #[value(name = "active")]
    Active,
    /// Merged pull requests
    #[value(name = "completed")]
    Completed,
    /// Closed without merging
    #[value(name = "abandoned")]
    Abandoned,
    /// Every pull request, whatever its state
    #[value(name = "all")]
    All,
}

impl StatusFilter {
    /// The value Azure DevOps expects in `searchCriteria.status`.
    fn as_api_value(self) -> &'static str {
        match self {
            StatusFilter::Active => "active",
            StatusFilter::Completed => "completed",
            StatusFilter::Abandoned => "abandoned",
            StatusFilter::All => "all",
        }
    }
}

/// The filters `repos pr list` accepts.
pub(super) struct ListFilters<'a> {
    pub status: StatusFilter,
    pub creator: Option<&'a str>,
    pub reviewer: Option<&'a str>,
    pub source: Option<&'a str>,
    pub target: Option<&'a str>,
    pub top: Option<i32>,
}

/// Lists pull requests for a repository, filtered server-side.
pub(super) async fn list_pull_requests(
    project: Option<&str>,
    repo: &str,
    filters: &ListFilters<'_>,
) -> Result<()> {
    let ctx = PrContext::for_repo(project, repo).await?;

    let mut request = ctx
        .client
        .pull_requests_client()
        .get_pull_requests(&ctx.creds.organization, &ctx.repository_id, &ctx.project)
        .search_criteria_status(filters.status.as_api_value());

    if let Some(creator) = filters.creator {
        request = request
            .search_criteria_creator_id(identity::resolve_identity(&ctx.creds, creator).await?);
    }
    if let Some(reviewer) = filters.reviewer {
        request = request
            .search_criteria_reviewer_id(identity::resolve_identity(&ctx.creds, reviewer).await?);
    }
    if let Some(source) = filters.source {
        request = request.search_criteria_source_ref_name(branch_ref(source));
    }
    if let Some(target) = filters.target {
        request = request.search_criteria_target_ref_name(branch_ref(target));
    }
    if let Some(top) = filters.top {
        request = request.top(top);
    }

    let pull_requests = request.await?.value;

    if pull_requests.is_empty() {
        println!(
            "No pull requests found for repository '{repo}' in project '{}'",
            ctx.project
        );
        return Ok(());
    }

    display_pull_requests(&pull_requests);
    Ok(())
}

/// Strips the `refs/heads/` prefix so branch columns stay readable.
pub(super) fn short_branch(ref_name: &str) -> &str {
    ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name)
}

/// Prints one row per pull request.
fn display_pull_requests(pull_requests: &[GitPullRequest]) {
    println!(
        "{:<8} {:<11} {:<50} {:<24} {}",
        "ID".bold(),
        "Status".bold(),
        "Title".bold(),
        "Author".bold(),
        "Branches".bold()
    );
    println!("{}", "-".repeat(120));

    for pr in pull_requests {
        let author = pr
            .created_by
            .graph_subject_base
            .display_name
            .as_deref()
            .unwrap_or("-");

        println!(
            "{:<8} {:<11} {:<50} {:<24} {} -> {}",
            pr.pull_request_id,
            format!("{:?}", pr.status).to_lowercase(),
            pr.title.as_deref().unwrap_or("-"),
            author,
            short_branch(&pr.source_ref_name),
            short_branch(&pr.target_ref_name)
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_filters_map_to_the_api_values() {
        assert_eq!(StatusFilter::Active.as_api_value(), "active");
        assert_eq!(StatusFilter::Completed.as_api_value(), "completed");
        assert_eq!(StatusFilter::Abandoned.as_api_value(), "abandoned");
        assert_eq!(StatusFilter::All.as_api_value(), "all");
    }

    #[test]
    fn short_branch_strips_the_ref_prefix() {
        assert_eq!(short_branch("refs/heads/feature/x"), "feature/x");
        assert_eq!(short_branch("feature/x"), "feature/x");
        assert_eq!(short_branch("refs/pull/1/merge"), "refs/pull/1/merge");
    }
}
