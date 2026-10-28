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
use std::future::Future;

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

    /// Fetches up to `top` pull requests matching the filters, narrowed to an
    /// optional creator and reviewer identity ID, after skipping `skip`.
    async fn query(
        &self,
        filters: &ListFilters<'_>,
        creator_id: Option<String>,
        reviewer_id: Option<String>,
        top: Option<i32>,
        skip: i32,
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
        if let Some(top) = top {
            request = request.top(top);
        }
        if skip > 0 {
            request = request.skip(skip);
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
    let pull_requests = ctx
        .query(filters, creator_id, reviewer_id, filters.effective_top(), 0)
        .await?;

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

    let top = filters.effective_top();
    let created = ctx.query(filters, Some(me.clone()), None, top, 0).await?;
    let assigned = assigned_to(ctx, filters, &me, top).await?;

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

/// Fetches up to `top` pull requests `me` is reviewing but did not create.
///
/// Azure DevOps cannot exclude a creator server-side, so this pages through
/// the reviewer results until enough remain after dropping the user's own.
async fn assigned_to(
    ctx: &ListContext,
    filters: &ListFilters<'_>,
    me: &str,
    top: Option<i32>,
) -> Result<Vec<GitPullRequest>> {
    let Some(top) = top else {
        let reviewing = ctx
            .query(filters, None, Some(me.to_string()), None, 0)
            .await?;
        return Ok(not_created_by(reviewing, me));
    };

    fill_not_created_by(me, top, |skip| {
        ctx.query(filters, None, Some(me.to_string()), Some(top), skip)
    })
    .await
}

/// Requests pages of `top` pull requests, skipping ahead each time, until `top`
/// of them were not created by `me` or the results run out.
async fn fill_not_created_by<F, Fut>(
    me: &str,
    top: i32,
    mut fetch_page: F,
) -> Result<Vec<GitPullRequest>>
where
    F: FnMut(i32) -> Fut,
    Fut: Future<Output = Result<Vec<GitPullRequest>>>,
{
    let limit = top.max(0) as usize;
    let mut kept = Vec::new();
    let mut skip = 0;
    while kept.len() < limit {
        let page = fetch_page(skip).await?;
        let fetched = page.len();
        kept.extend(not_created_by(page, me));
        if fetched < limit {
            break;
        }
        skip += fetched as i32;
    }
    kept.truncate(limit);
    Ok(kept)
}

/// Drops the pull requests `identity_id` created, so a pull request the user
/// both created and reviews is only listed under "Created by me".
fn not_created_by(pull_requests: Vec<GitPullRequest>, identity_id: &str) -> Vec<GitPullRequest> {
    pull_requests
        .into_iter()
        .filter(|pr| !pr.created_by.id.eq_ignore_ascii_case(identity_id))
        .collect()
}

/// Strips the `refs/heads/` prefix so branch columns stay readable.
pub(super) fn short_branch(ref_name: &str) -> &str {
    ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name)
}

/// The optional columns a listing shows.
#[derive(Copy, Clone)]
pub(super) struct Columns<'a> {
    /// Show which repository each pull request belongs to, for project-wide
    /// listings.
    pub repository: bool,
    /// Show the vote this identity cast on each pull request.
    pub vote_of: Option<&'a str>,
}

/// The vote `identity_id` cast on a pull request.
pub(super) fn vote_of(pr: &GitPullRequest, identity_id: &str) -> &'static str {
    vote_label(
        pr.reviewers
            .iter()
            .find(|reviewer| reviewer.identity_ref.id.eq_ignore_ascii_case(identity_id))
            .and_then(|reviewer| reviewer.vote),
    )
}

/// Marks draft pull requests, which the web portal badges in its list.
pub(super) fn draft_marker(pr: &GitPullRequest) -> &'static str {
    if pr.is_draft {
        "draft"
    } else {
        ""
    }
}

/// The day a pull request was opened, as `YYYY-MM-DD`.
pub(super) fn created_on(pr: &GitPullRequest) -> String {
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
pub(super) fn header_width(columns: &Columns) -> usize {
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

    fn created_by(id: i32, creator_id: &str) -> GitPullRequest {
        let mut pr = pull_request(id);
        pr.created_by.id = creator_id.to_string();
        pr
    }

    fn ids(pull_requests: &[GitPullRequest]) -> Vec<i32> {
        pull_requests.iter().map(|pr| pr.pull_request_id).collect()
    }

    /// Serves `top` pull requests at a time from `all`, like the API's
    /// `$top` and `$skip` paging.
    async fn fill_from(all: &[GitPullRequest], top: i32) -> Vec<GitPullRequest> {
        fill_not_created_by("me", top, |skip| {
            let page = all
                .iter()
                .skip(skip as usize)
                .take(top as usize)
                .cloned()
                .collect();
            std::future::ready(Ok(page))
        })
        .await
        .unwrap()
    }

    #[test]
    fn pull_requests_the_user_created_are_dropped_ignoring_case() {
        let reviewing = vec![created_by(1, "ME"), created_by(2, "someone-else")];
        assert_eq!(ids(&not_created_by(reviewing, "me")), vec![2]);
    }

    #[tokio::test]
    async fn the_limit_applies_after_dropping_the_users_own_pull_requests() {
        // With --top 1, a first result the user created must not leave the
        // section empty when later pages hold pull requests they review.
        let all = vec![
            created_by(1, "me"),
            created_by(2, "me"),
            created_by(3, "someone-else"),
            created_by(4, "someone-else"),
        ];
        assert_eq!(ids(&fill_from(&all, 1).await), vec![3]);
        assert_eq!(ids(&fill_from(&all, 3).await), vec![3, 4]);
    }

    #[tokio::test]
    async fn filling_stops_when_the_results_run_out() {
        let all = vec![created_by(1, "me"), created_by(2, "someone-else")];
        assert_eq!(ids(&fill_from(&all, 5).await), vec![2]);
        assert!(fill_from(&all, 0).await.is_empty());
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
