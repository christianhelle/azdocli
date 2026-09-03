//! Showing details of a single pull request.

use super::comments::thread_status_label;
use super::list::short_branch;
use super::reviewers::{reviewer_name, vote_label};
use super::PrContext;
use crate::auth::url::web_pull_request_url;
use anyhow::Result;
use azure_devops_rust_api::git::models::GitPullRequest;
use colored::Colorize;

/// Shows details of a specific pull request.
pub(super) async fn show_pull_request(
    project: Option<&str>,
    repo: &str,
    id: &str,
    web: bool,
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
    display_pull_request(&pull_request);

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

    if !work_items.is_empty() {
        let ids = work_items
            .iter()
            .filter_map(|item| item.id.as_deref())
            .collect::<Vec<_>>();
        println!("  Work Items: {}", ids.join(", "));
    }

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

    let open_threads = super::comments::visible_threads(&threads, false)
        .into_iter()
        .filter(|thread| thread_status_label(thread.comment_thread.status.as_ref()) == "active")
        .count();

    println!("  Open comment threads: {open_threads}");

    Ok(())
}

/// Prints the details of a pull request, including reviewers and labels.
fn display_pull_request(pull_request: &GitPullRequest) {
    println!("📋 Pull Request Details");
    println!("=======================");
    println!("  ID: {}", pull_request.pull_request_id);
    println!(
        "  Title: {}",
        pull_request.title.clone().unwrap_or_default()
    );

    if let Some(description) = pull_request.description.as_deref() {
        if !description.is_empty() {
            println!("  Description: {description}");
        }
    }

    println!("  Status: {:?}", pull_request.status);
    if pull_request.is_draft {
        println!("  Draft: {}", "yes".yellow());
    }
    if let Some(merge_status) = pull_request.merge_status.as_ref() {
        println!("  Merge status: {merge_status:?}");
    }
    if let Some(message) = pull_request.merge_failure_message.as_deref() {
        println!("  Merge failure: {}", message.red());
    }
    if pull_request.auto_complete_set_by.is_some() {
        println!("  Auto-complete: enabled");
    }

    println!(
        "  Source Branch: {}",
        short_branch(&pull_request.source_ref_name)
    );
    println!(
        "  Target Branch: {}",
        short_branch(&pull_request.target_ref_name)
    );
    println!(
        "  Created by: {}",
        pull_request
            .created_by
            .graph_subject_base
            .display_name
            .as_deref()
            .unwrap_or("-")
    );
    println!("  Created: {}", pull_request.creation_date);

    if !pull_request.labels.is_empty() {
        let labels = pull_request
            .labels
            .iter()
            .filter_map(|label| label.name.as_deref())
            .collect::<Vec<_>>();
        println!("  Labels: {}", labels.join(", "));
    }

    if pull_request.reviewers.is_empty() {
        println!("  Reviewers: none");
    } else {
        println!("  Reviewers:");
        for reviewer in &pull_request.reviewers {
            println!(
                "    • {} ({}){}",
                reviewer_name(reviewer),
                vote_label(reviewer.vote),
                if reviewer.is_required.unwrap_or(false) {
                    ", required"
                } else {
                    ""
                }
            );
        }
    }
}
