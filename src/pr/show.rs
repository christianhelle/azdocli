//! Showing details of a single pull request.

use super::comments::thread_status_label;
use super::views::PullRequestDetails;
use super::PrContext;
use crate::auth::url::web_pull_request_url;
use crate::output::{self, OutputFormat};
use anyhow::Result;

/// Shows details of a specific pull request.
pub(super) async fn show_pull_request(
    project: Option<&str>,
    repo: &str,
    id: &str,
    web: bool,
    format: OutputFormat,
) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;

    if web {
        let url = web_pull_request_url(
            &ctx.creds.base_url,
            &ctx.creds.organization,
            &ctx.project,
            repo,
            ctx.pull_request_id,
        );
        println!("Opening pull request in browser: {url}");
        return crate::browser::open_url(&url);
    }

    let pull_request = ctx.get_pull_request().await?;

    let work_items = ctx
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

    let threads = ctx
        .client
        .pull_request_threads_client()
        .list(
            &ctx.creds.organization,
            &ctx.repository_id,
            ctx.pull_request_id,
            &ctx.project,
        )
        .await?
        .value;

    let open_comment_threads = super::comments::visible_threads(&threads, false)
        .into_iter()
        .filter(|thread| thread_status_label(thread.comment_thread.status.as_ref()) == "active")
        .count();

    let details = PullRequestDetails {
        pull_request: &pull_request,
        work_item_ids: work_items
            .iter()
            .filter_map(|item| item.id.clone())
            .collect(),
        open_comment_threads,
    };
    output::emit(&details, format)
}
