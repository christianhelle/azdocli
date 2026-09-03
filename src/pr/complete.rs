//! Closing a pull request: abandon, reactivate and complete.

use super::{http, PrContext};
use anyhow::{anyhow, Result};
use azure_devops_rust_api::git::models::{
    git_pull_request, GitPullRequest, GitPullRequestUpdateOptions, PullRequestStatus,
};
use clap::ValueEnum;
use colored::Colorize;
use dialoguer::Confirm;
use serde_json::{json, Map, Value};

/// Merge strategies Azure DevOps can use when completing a pull request.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum MergeStrategyArg {
    /// Create a merge commit (no fast-forward)
    #[value(name = "no-fast-forward")]
    NoFastForward,
    /// Squash the source branch into a single commit
    #[value(name = "squash")]
    Squash,
    /// Rebase the source commits onto the target branch
    #[value(name = "rebase")]
    Rebase,
    /// Rebase the source commits and then create a merge commit
    #[value(name = "rebase-merge")]
    RebaseMerge,
}

impl MergeStrategyArg {
    /// The value Azure DevOps expects in `completionOptions.mergeStrategy`.
    fn as_api_value(self) -> &'static str {
        match self {
            MergeStrategyArg::NoFastForward => "noFastForward",
            MergeStrategyArg::Squash => "squash",
            MergeStrategyArg::Rebase => "rebase",
            MergeStrategyArg::RebaseMerge => "rebaseMerge",
        }
    }
}

/// The completion options accepted by `repos pr complete`.
pub(super) struct CompletionSettings<'a> {
    pub merge_strategy: Option<MergeStrategyArg>,
    pub delete_source_branch: bool,
    pub merge_commit_message: Option<&'a str>,
    pub bypass_policy: bool,
    pub bypass_reason: Option<&'a str>,
}

/// Builds the `completionOptions` payload, omitting anything not requested.
pub(super) fn build_completion_options(settings: &CompletionSettings<'_>) -> Value {
    let mut options = Map::new();

    if let Some(strategy) = settings.merge_strategy {
        options.insert("mergeStrategy".to_string(), json!(strategy.as_api_value()));
    }
    if settings.delete_source_branch {
        options.insert("deleteSourceBranch".to_string(), json!(true));
    }
    if let Some(message) = settings.merge_commit_message {
        options.insert("mergeCommitMessage".to_string(), json!(message));
    }
    if settings.bypass_policy {
        options.insert("bypassPolicy".to_string(), json!(true));
    }
    if let Some(reason) = settings.bypass_reason {
        options.insert("bypassReason".to_string(), json!(reason));
    }

    Value::Object(options)
}

/// Rejects pull requests that cannot be completed, with a readable reason.
///
/// Returns the last merge source commit, which Azure DevOps requires on the
/// completion request so a stale pull request cannot be merged by accident.
pub(super) fn ensure_completable(pull_request: &GitPullRequest) -> Result<String> {
    if pull_request.status != git_pull_request::Status::Active {
        return Err(anyhow!(
            "Pull request #{} is {:?}; only active pull requests can be completed",
            pull_request.pull_request_id,
            pull_request.status
        ));
    }

    if pull_request.is_draft {
        return Err(anyhow!(
            "Pull request #{} is a draft; publish it before completing",
            pull_request.pull_request_id
        ));
    }

    match pull_request.merge_status {
        Some(git_pull_request::MergeStatus::Conflicts) => {
            return Err(anyhow!(
                "Pull request #{} has merge conflicts that must be resolved first",
                pull_request.pull_request_id
            ))
        }
        Some(git_pull_request::MergeStatus::RejectedByPolicy) => {
            return Err(anyhow!(
                "Pull request #{} was rejected by branch policy",
                pull_request.pull_request_id
            ))
        }
        Some(git_pull_request::MergeStatus::Failure) => {
            return Err(anyhow!(
                "Pull request #{} could not be merged: {}",
                pull_request.pull_request_id,
                pull_request
                    .merge_failure_message
                    .as_deref()
                    .unwrap_or("no reason reported")
            ))
        }
        _ => {}
    }

    pull_request
        .last_merge_source_commit
        .as_ref()
        .and_then(|commit| commit.commit_id.clone())
        .ok_or_else(|| {
            anyhow!(
                "Pull request #{} has no merge source commit yet; wait for the merge to be evaluated and try again",
                pull_request.pull_request_id
            )
        })
}

/// Completes a pull request, or arms auto-complete for it.
pub(super) async fn complete_pull_request(
    project: Option<&str>,
    repo: &str,
    id: &str,
    settings: &CompletionSettings<'_>,
    auto_complete: bool,
    skip_confirmation: bool,
) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;
    let pull_request = ctx.get_pull_request().await?;
    let completion_options = build_completion_options(settings);

    println!("Completing pull request:");
    println!("  Repository: {repo}");
    println!("  ID: {}", pull_request.pull_request_id);
    println!(
        "  Title: {}",
        pull_request.title.clone().unwrap_or_default()
    );
    println!(
        "  {} -> {}",
        pull_request.source_ref_name, pull_request.target_ref_name
    );
    if let Some(strategy) = settings.merge_strategy {
        println!("  Merge strategy: {}", strategy.as_api_value());
    }
    if settings.delete_source_branch {
        println!("  Source branch will be deleted");
    }

    let body = if auto_complete {
        let identity = http::authenticated_user_id(&ctx.creds).await?;
        json!({
            "autoCompleteSetBy": { "id": identity },
            "completionOptions": completion_options,
        })
    } else {
        let commit_id = ensure_completable(&pull_request)?;
        json!({
            "status": "completed",
            "lastMergeSourceCommit": { "commitId": commit_id },
            "completionOptions": completion_options,
        })
    };

    let prompt = if auto_complete {
        "Do you want this pull request to complete automatically?"
    } else {
        "Do you want to complete this pull request?"
    };

    if !skip_confirmation
        && !Confirm::new()
            .with_prompt(prompt)
            .default(false)
            .interact()?
    {
        println!("Completion cancelled.");
        return Ok(());
    }

    http::patch_pull_request(
        &ctx.creds,
        &ctx.project,
        &ctx.repository_id,
        ctx.pull_request_id,
        &body,
    )
    .await?;

    if auto_complete {
        println!("{}", "✅ Auto-complete enabled.".green());
    } else {
        println!("{}", "✅ Pull request completed.".green());
    }

    Ok(())
}

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

    /// Builds a minimal active pull request by deserializing the API shape,
    /// which avoids depending on the SDK's date type in tests.
    fn pull_request() -> GitPullRequest {
        serde_json::from_value(json!({
            "pullRequestId": 7,
            "createdBy": { "id": "author" },
            "creationDate": "2024-01-01T00:00:00Z",
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
            "mergeStatus": "succeeded",
            "lastMergeSourceCommit": { "commitId": "abc123" }
        }))
        .expect("fixture should deserialize")
    }

    fn settings() -> CompletionSettings<'static> {
        CompletionSettings {
            merge_strategy: None,
            delete_source_branch: false,
            merge_commit_message: None,
            bypass_policy: false,
            bypass_reason: None,
        }
    }

    #[test]
    fn build_completion_options_is_empty_by_default() {
        assert_eq!(build_completion_options(&settings()), json!({}));
    }

    #[test]
    fn build_completion_options_maps_every_flag() {
        let options = build_completion_options(&CompletionSettings {
            merge_strategy: Some(MergeStrategyArg::Squash),
            delete_source_branch: true,
            merge_commit_message: Some("merged"),
            bypass_policy: true,
            bypass_reason: Some("hotfix"),
        });

        assert_eq!(
            options,
            json!({
                "mergeStrategy": "squash",
                "deleteSourceBranch": true,
                "mergeCommitMessage": "merged",
                "bypassPolicy": true,
                "bypassReason": "hotfix",
            })
        );
    }

    #[test]
    fn merge_strategy_uses_api_casing() {
        assert_eq!(
            MergeStrategyArg::NoFastForward.as_api_value(),
            "noFastForward"
        );
        assert_eq!(MergeStrategyArg::RebaseMerge.as_api_value(), "rebaseMerge");
    }

    #[test]
    fn ensure_completable_returns_the_merge_source_commit() {
        assert_eq!(ensure_completable(&pull_request()).unwrap(), "abc123");
    }

    #[test]
    fn ensure_completable_rejects_inactive_pull_requests() {
        let mut pr = pull_request();
        pr.status = git_pull_request::Status::Completed;

        let message = ensure_completable(&pr).unwrap_err().to_string();
        assert!(message.contains("only active pull requests"), "{message}");
    }

    #[test]
    fn ensure_completable_rejects_drafts() {
        let mut pr = pull_request();
        pr.is_draft = true;

        let message = ensure_completable(&pr).unwrap_err().to_string();
        assert!(message.contains("draft"), "{message}");
    }

    #[test]
    fn ensure_completable_rejects_conflicts() {
        let mut pr = pull_request();
        pr.merge_status = Some(git_pull_request::MergeStatus::Conflicts);

        let message = ensure_completable(&pr).unwrap_err().to_string();
        assert!(message.contains("merge conflicts"), "{message}");
    }

    #[test]
    fn ensure_completable_requires_a_merge_source_commit() {
        let mut pr = pull_request();
        pr.last_merge_source_commit = None;

        let message = ensure_completable(&pr).unwrap_err().to_string();
        assert!(message.contains("no merge source commit"), "{message}");
    }

    #[test]
    fn build_status_options_sets_only_status() {
        let options = build_status_options(PullRequestStatus::Abandoned);
        assert_eq!(options.status, Some(PullRequestStatus::Abandoned));
        assert_eq!(options.title, None);
        assert_eq!(options.description, None);
    }
}
