//! Closing a pull request: abandon, reactivate and complete.

use super::PrContext;
use anyhow::Result;
use azure_devops_rust_api::git::models::{GitPullRequestUpdateOptions, PullRequestStatus};
use colored::Colorize;
use dialoguer::Confirm;

/// Builds an update payload that only changes the pull request status.
pub(super) fn build_status_options(status: PullRequestStatus) -> GitPullRequestUpdateOptions {
    GitPullRequestUpdateOptions {
        status: Some(status),
        ..Default::default()
    }
}

/// Abandons a pull request, asking for confirmation unless `skip_confirmation`.
pub(super) async fn abandon_pull_request(
    project: Option<&str>,
    repo: &str,
    id: &str,
    skip_confirmation: bool,
) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;
    let pull_request = ctx.get_pull_request().await?;

    println!("Abandoning pull request:");
    println!("  Repository: {repo}");
    println!("  ID: {}", pull_request.pull_request_id);
    println!(
        "  Title: {}",
        pull_request.title.clone().unwrap_or_default()
    );

    if !skip_confirmation
        && !Confirm::new()
            .with_prompt("Do you want to abandon this pull request?")
            .default(false)
            .interact()?
    {
        println!("Abandon cancelled.");
        return Ok(());
    }

    ctx.set_status(PullRequestStatus::Abandoned).await?;
    println!("{}", "✅ Pull request abandoned.".green());
    Ok(())
}

/// Reactivates a previously abandoned pull request.
pub(super) async fn reactivate_pull_request(
    project: Option<&str>,
    repo: &str,
    id: &str,
) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;

    ctx.set_status(PullRequestStatus::Active).await?;
    println!("{}", "✅ Pull request reactivated.".green());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_status_options_sets_only_status() {
        let options = build_status_options(PullRequestStatus::Abandoned);
        assert_eq!(options.status, Some(PullRequestStatus::Abandoned));
        assert_eq!(options.title, None);
        assert_eq!(options.description, None);
    }
}
