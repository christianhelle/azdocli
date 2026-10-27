//! Listing pull requests and their commits.

use super::{create::branch_ref, create_git_client, identity, parse_pr_id};
use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::{get_credentials, Credentials};
use crate::project::get_project_or_default;
use anyhow::Result;
use azure_devops_rust_api::git;
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

/// The project, and optionally the repository, a listing runs against.
struct ListContext {
    creds: Credentials,
    client: git::Client,
    project: String,
    repository_id: Option<String>,
}

impl ListContext {
    async fn new(project: Option<&str>, repo: Option<&str>) -> Result<Self> {
        let project = get_project_or_default(project)?;
        let creds = get_credentials()?;
        let client = CredentialClientFactory::new(&creds)?.build_git();
        let repository_id = match repo {
            Some(repo) => Some(crate::repos::get_repo(&project, repo).await?.id),
            None => None,
        };

        Ok(Self {
            creds,
            client,
            project,
            repository_id,
        })
    }

    /// Fetches the pull requests matching the filters, narrowed to an
    /// optional creator and reviewer identity ID.
    async fn query(
        &self,
        filters: &ListFilters<'_>,
        creator_id: Option<String>,
        reviewer_id: Option<String>,
    ) -> Result<Vec<GitPullRequest>> {
        let mut request = self
            .client
            .pull_requests_client()
            .get_pull_requests_by_project(&self.creds.organization, &self.project)
            .search_criteria_status(filters.status.as_api_value());

        if let Some(repository_id) = &self.repository_id {
            request = request.search_criteria_repository_id(repository_id);
        }
        if let Some(creator_id) = creator_id {
            request = request.search_criteria_creator_id(creator_id);
        }
        if let Some(reviewer_id) = reviewer_id {
            request = request.search_criteria_reviewer_id(reviewer_id);
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

        Ok(request.await?.value)
    }
}

/// Lists pull requests for a repository, or for the whole project when no
/// repository is given, filtered server-side.
pub(super) async fn list_pull_requests(
    project: Option<&str>,
    repo: Option<&str>,
    filters: &ListFilters<'_>,
) -> Result<()> {
    let ctx = ListContext::new(project, repo).await?;
    let columns = Columns {
        repository: repo.is_none(),
    };

    let creator_id = match filters.creator {
        Some(creator) => Some(identity::resolve_identity(&ctx.creds, creator).await?),
        None => None,
    };
    let reviewer_id = match filters.reviewer {
        Some(reviewer) => Some(identity::resolve_identity(&ctx.creds, reviewer).await?),
        None => None,
    };
    let pull_requests = ctx.query(filters, creator_id, reviewer_id).await?;

    if pull_requests.is_empty() {
        match repo {
            Some(repo) => println!(
                "No pull requests found for repository '{repo}' in project '{}'",
                ctx.project
            ),
            None => println!("No pull requests found in project '{}'", ctx.project),
        }
        return Ok(());
    }

    display_pull_requests(&pull_requests, &columns);
    Ok(())
}

/// Strips the `refs/heads/` prefix so branch columns stay readable.
pub(super) fn short_branch(ref_name: &str) -> &str {
    ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name)
}

/// The optional columns a listing shows.
#[derive(Copy, Clone)]
struct Columns {
    /// Show which repository each pull request belongs to, for project-wide
    /// listings.
    repository: bool,
}

/// Marks draft pull requests, which the web portal badges in its list.
fn draft_marker(pr: &GitPullRequest) -> &'static str {
    if pr.is_draft {
        "draft"
    } else {
        ""
    }
}

/// The day a pull request was opened, as `YYYY-MM-DD`.
fn created_on(pr: &GitPullRequest) -> String {
    pr.creation_date.date().to_string()
}

/// Prints one row per pull request.
fn display_pull_requests(pull_requests: &[GitPullRequest], columns: &Columns) {
    let mut header = format!(
        "{:<8} {:<11} {:<6} {:<50} ",
        "ID".bold(),
        "Status".bold(),
        "Draft".bold(),
        "Title".bold()
    );
    if columns.repository {
        header.push_str(&format!("{:<24} ", "Repository".bold()));
    }
    header.push_str(&format!("{:<24} ", "Author".bold()));
    header.push_str(&format!("{:<11} {}", "Created".bold(), "Branches".bold()));
    println!("{header}");
    println!("{}", "-".repeat(header_width(columns)));

    for pr in pull_requests {
        let author = pr
            .created_by
            .graph_subject_base
            .display_name
            .as_deref()
            .unwrap_or("-");

        let mut row = format!(
            "{:<8} {:<11} {:<6} {:<50} ",
            pr.pull_request_id,
            format!("{:?}", pr.status).to_lowercase(),
            draft_marker(pr),
            pr.title.as_deref().unwrap_or("-")
        );
        if columns.repository {
            row.push_str(&format!("{:<24} ", pr.repository.name));
        }
        row.push_str(&format!("{author:<24} "));
        row.push_str(&format!(
            "{:<11} {} -> {}",
            created_on(pr),
            short_branch(&pr.source_ref_name),
            short_branch(&pr.target_ref_name)
        ));
        println!("{row}");
    }
}

/// How wide the separator under the header should be for these columns.
fn header_width(columns: &Columns) -> usize {
    let mut width = 135;
    if columns.repository {
        width += 25;
    }
    width
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

    /// Builds a minimal pull request by deserializing the API shape, which
    /// avoids depending on the SDK's date type in tests.
    fn pull_request(id: i32) -> GitPullRequest {
        serde_json::from_value(serde_json::json!({
            "pullRequestId": id,
            "createdBy": { "id": "author" },
            "creationDate": "2026-10-20T13:45:00Z",
            "isDraft": false,
            "repository": {
                "id": "repo-id",
                "name": "repo",
                "url": "url",
                "project": { "name": "project", "visibility": "private" }
            },
            "sourceRefName": "refs/heads/feature",
            "targetRefName": "refs/heads/main",
            "status": "active",
            "url": "url"
        }))
        .expect("fixture should deserialize")
    }

    #[test]
    fn draft_marker_only_marks_drafts() {
        let mut pr = pull_request(1);
        assert_eq!(draft_marker(&pr), "");
        pr.is_draft = true;
        assert_eq!(draft_marker(&pr), "draft");
    }

    #[test]
    fn created_on_shows_the_day() {
        assert_eq!(created_on(&pull_request(1)), "2026-10-20");
    }

    #[test]
    fn short_branch_strips_the_ref_prefix() {
        assert_eq!(short_branch("refs/heads/feature/x"), "feature/x");
        assert_eq!(short_branch("feature/x"), "feature/x");
        assert_eq!(short_branch("refs/pull/1/merge"), "refs/pull/1/merge");
    }
}
