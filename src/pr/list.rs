//! Listing pull requests and their commits.

use super::reviewers::vote_label;
use super::{create::branch_ref, create_git_client, http, identity, parse_pr_id};
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

/// How many pull requests a non-active listing returns when `--top` is not
/// given, since completed and abandoned histories can be very long.
const DEFAULT_HISTORY_TOP: i32 = 50;

/// The filters `repos pr list` accepts.
pub(super) struct ListFilters<'a> {
    pub status: StatusFilter,
    pub mine: bool,
    pub creator: Option<&'a str>,
    pub reviewer: Option<&'a str>,
    pub source: Option<&'a str>,
    pub target: Option<&'a str>,
    pub top: Option<i32>,
}

impl ListFilters<'_> {
    /// The page size to request: what the user asked for, or a cap for
    /// listings that reach into history.
    fn effective_top(&self) -> Option<i32> {
        self.top.or(match self.status {
            StatusFilter::Active => None,
            _ => Some(DEFAULT_HISTORY_TOP),
        })
    }
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
        if let Some(top) = filters.effective_top() {
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
        vote_of: None,
    };

    if filters.mine {
        return list_my_pull_requests(&ctx, filters, columns).await;
    }

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

/// Lists the signed-in user's pull requests the way the web portal's "Mine"
/// tab does: the ones they created, then the ones they were asked to review.
async fn list_my_pull_requests(
    ctx: &ListContext,
    filters: &ListFilters<'_>,
    columns: Columns<'_>,
) -> Result<()> {
    let me = http::authenticated_user_id(&ctx.creds).await?;

    let created = ctx.query(filters, Some(me.clone()), None).await?;
    let assigned =
        without_pull_requests_in(ctx.query(filters, None, Some(me.clone())).await?, &created);

    display_section("Created by me", &created, &columns);
    println!();
    display_section(
        "Assigned to me",
        &assigned,
        &Columns {
            vote_of: Some(&me),
            ..columns
        },
    );
    Ok(())
}

/// Drops the pull requests that already appear in `seen`, so a pull request
/// the user both created and reviews is only listed once.
fn without_pull_requests_in(
    pull_requests: Vec<GitPullRequest>,
    seen: &[GitPullRequest],
) -> Vec<GitPullRequest> {
    pull_requests
        .into_iter()
        .filter(|pr| {
            !seen
                .iter()
                .any(|other| other.pull_request_id == pr.pull_request_id)
        })
        .collect()
}

/// Strips the `refs/heads/` prefix so branch columns stay readable.
pub(super) fn short_branch(ref_name: &str) -> &str {
    ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name)
}

/// The optional columns a listing shows.
#[derive(Copy, Clone)]
struct Columns<'a> {
    /// Show which repository each pull request belongs to, for project-wide
    /// listings.
    repository: bool,
    /// Show the vote this identity cast on each pull request.
    vote_of: Option<&'a str>,
}

/// The vote `identity_id` cast on a pull request.
fn vote_of(pr: &GitPullRequest, identity_id: &str) -> &'static str {
    vote_label(
        pr.reviewers
            .iter()
            .find(|reviewer| reviewer.identity_ref.id.eq_ignore_ascii_case(identity_id))
            .and_then(|reviewer| reviewer.vote),
    )
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

/// Prints a titled group of pull requests, or "none" when it is empty.
fn display_section(title: &str, pull_requests: &[GitPullRequest], columns: &Columns) {
    println!("{}", format!("{title} ({})", pull_requests.len()).bold());
    if pull_requests.is_empty() {
        println!("  none");
    } else {
        display_pull_requests(pull_requests, columns);
    }
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
    if columns.vote_of.is_some() {
        header.push_str(&format!("{:<26} ", "My vote".bold()));
    }
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
        if let Some(identity_id) = columns.vote_of {
            row.push_str(&format!("{:<26} ", vote_of(pr, identity_id)));
        }
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
    if columns.vote_of.is_some() {
        width += 27;
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
            "url": "url",
            "reviewers": [
                { "id": "AAAA-me", "vote": -5 },
                { "id": "someone-else", "vote": 10 }
            ]
        }))
        .expect("fixture should deserialize")
    }

    fn filters(status: StatusFilter, top: Option<i32>) -> ListFilters<'static> {
        ListFilters {
            status,
            mine: false,
            creator: None,
            reviewer: None,
            source: None,
            target: None,
            top,
        }
    }

    #[test]
    fn active_listings_are_uncapped_by_default() {
        assert_eq!(filters(StatusFilter::Active, None).effective_top(), None);
    }

    #[test]
    fn history_listings_are_capped_by_default() {
        for status in [
            StatusFilter::Completed,
            StatusFilter::Abandoned,
            StatusFilter::All,
        ] {
            assert_eq!(
                filters(status, None).effective_top(),
                Some(DEFAULT_HISTORY_TOP)
            );
        }
    }

    #[test]
    fn an_explicit_top_wins_over_the_default() {
        assert_eq!(
            filters(StatusFilter::Completed, Some(5)).effective_top(),
            Some(5)
        );
        assert_eq!(
            filters(StatusFilter::Active, Some(5)).effective_top(),
            Some(5)
        );
    }

    #[test]
    fn pull_requests_already_seen_are_dropped() {
        let created = vec![pull_request(1), pull_request(2)];
        let assigned = vec![pull_request(2), pull_request(3)];

        let ids: Vec<i32> = without_pull_requests_in(assigned, &created)
            .iter()
            .map(|pr| pr.pull_request_id)
            .collect();
        assert_eq!(ids, vec![3]);
    }

    #[test]
    fn vote_of_finds_the_identity_ignoring_case() {
        let pr = pull_request(1);
        assert_eq!(vote_of(&pr, "aaaa-ME"), "waiting for author");
        assert_eq!(vote_of(&pr, "someone-else"), "approved");
    }

    #[test]
    fn vote_of_an_identity_not_reviewing_is_no_vote() {
        assert_eq!(vote_of(&pull_request(1), "stranger"), "no vote");
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
