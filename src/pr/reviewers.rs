//! Managing the reviewers of a pull request.

use super::{identity, PrContext};
use anyhow::Result;
use azure_devops_rust_api::git::models::{IdentityRef, IdentityRefWithVote};
use clap::{Subcommand, ValueEnum};
use colored::Colorize;

#[derive(Subcommand, Clone)]
pub enum ReviewersSubCommands {
    /// List the reviewers of a pull request
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request
        #[clap(short, long)]
        id: String,
    },
    /// Add one or more reviewers to a pull request
    Add {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request
        #[clap(short, long)]
        id: String,

        /// Reviewer to add, as an email address, identity ID, or '@me'
        /// (repeat for multiple reviewers)
        #[clap(long, required = true)]
        reviewer: Vec<String>,

        /// Mark the reviewers as required rather than optional
        #[clap(long)]
        required: bool,
    },
    /// Remove a reviewer from a pull request
    Remove {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request
        #[clap(short, long)]
        id: String,

        /// Reviewer to remove, as an email address, identity ID, or '@me'
        #[clap(long)]
        reviewer: String,
    },
    /// Cast your vote on a pull request
    Vote {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request
        #[clap(short, long)]
        id: String,

        /// The vote to cast
        #[clap(long, value_enum)]
        vote: VoteArg,
    },
}

/// The votes a reviewer can cast on a pull request.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum VoteArg {
    /// Approve the pull request
    #[value(name = "approve")]
    Approve,
    /// Approve, but leave suggestions for the author
    #[value(name = "approve-with-suggestions")]
    ApproveWithSuggestions,
    /// Clear a previous vote
    #[value(name = "reset")]
    Reset,
    /// Ask the author for changes
    #[value(name = "wait-for-author")]
    WaitForAuthor,
    /// Reject the pull request
    #[value(name = "reject")]
    Reject,
}

impl VoteArg {
    /// The numeric value Azure DevOps uses for this vote.
    fn as_api_value(self) -> i64 {
        match self {
            VoteArg::Approve => 10,
            VoteArg::ApproveWithSuggestions => 5,
            VoteArg::Reset => 0,
            VoteArg::WaitForAuthor => -5,
            VoteArg::Reject => -10,
        }
    }
}

/// Renders a reviewer's vote as the label the web UI uses.
pub(super) fn vote_label(vote: Option<i64>) -> &'static str {
    match vote {
        Some(10) => "approved",
        Some(5) => "approved with suggestions",
        Some(-5) => "waiting for author",
        Some(-10) => "rejected",
        _ => "no vote",
    }
}

/// Returns the most useful display name available for a reviewer.
pub(super) fn reviewer_name(reviewer: &IdentityRefWithVote) -> &str {
    reviewer
        .identity_ref
        .graph_subject_base
        .display_name
        .as_deref()
        .or(reviewer.identity_ref.unique_name.as_deref())
        .unwrap_or(reviewer.identity_ref.id.as_str())
}

/// Routes reviewer subcommands to their handlers.
pub(super) async fn handle_command(subcommand: &ReviewersSubCommands) -> Result<()> {
    match subcommand {
        ReviewersSubCommands::List { project, repo, id } => {
            list_reviewers(project.as_deref(), repo, id).await
        }
        ReviewersSubCommands::Add {
            project,
            repo,
            id,
            reviewer,
            required,
        } => add_reviewers(project.as_deref(), repo, id, reviewer, *required).await,
        ReviewersSubCommands::Remove {
            project,
            repo,
            id,
            reviewer,
        } => remove_reviewer(project.as_deref(), repo, id, reviewer).await,
        ReviewersSubCommands::Vote {
            project,
            repo,
            id,
            vote,
        } => cast_vote(project.as_deref(), repo, id, *vote).await,
    }
}

/// Lists the reviewers assigned to a pull request.
async fn list_reviewers(project: Option<&str>, repo: &str, id: &str) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;

    let reviewers = ctx
        .client
        .pull_request_reviewers_client()
        .list(
            &ctx.creds.organization,
            &ctx.repository_id,
            ctx.pull_request_id,
            &ctx.project,
        )
        .await?
        .value;

    display_reviewers(&reviewers);
    Ok(())
}

/// Prints reviewers with their vote and whether they are required.
fn display_reviewers(reviewers: &[IdentityRefWithVote]) {
    if reviewers.is_empty() {
        println!("No reviewers assigned.");
        return;
    }

    println!(
        "{:<40} {:<28} {:<38} {}",
        "Name".bold(),
        "Vote".bold(),
        "ID".bold(),
        "Required".bold()
    );
    println!("{}", "-".repeat(118));

    for reviewer in reviewers {
        println!(
            "{:<40} {:<28} {:<38} {}",
            reviewer_name(reviewer),
            vote_label(reviewer.vote),
            reviewer.identity_ref.id,
            if reviewer.is_required.unwrap_or(false) {
                "yes"
            } else {
                "no"
            }
        );
    }
}

/// Adds reviewers to a pull request, resolving each argument to an identity.
async fn add_reviewers(
    project: Option<&str>,
    repo: &str,
    id: &str,
    reviewers: &[String],
    required: bool,
) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;

    for reviewer in reviewers {
        let identity_id = identity::resolve_identity(&ctx.creds, reviewer).await?;
        let mut body = IdentityRefWithVote::new(IdentityRef::new(identity_id.clone()));
        body.is_required = Some(required);

        ctx.client
            .pull_request_reviewers_client()
            .create_pull_request_reviewer(
                &ctx.creds.organization,
                body,
                &ctx.repository_id,
                ctx.pull_request_id,
                &identity_id,
                &ctx.project,
            )
            .await?;

        println!("{}", format!("✅ Added reviewer {reviewer}").green());
    }

    Ok(())
}

/// Removes a single reviewer from a pull request.
async fn remove_reviewer(
    project: Option<&str>,
    repo: &str,
    id: &str,
    reviewer: &str,
) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;
    let identity_id = identity::resolve_identity(&ctx.creds, reviewer).await?;

    ctx.client
        .pull_request_reviewers_client()
        .delete(
            &ctx.creds.organization,
            &ctx.repository_id,
            ctx.pull_request_id,
            &identity_id,
            &ctx.project,
        )
        .await?;

    println!("{}", format!("✅ Removed reviewer {reviewer}").green());
    Ok(())
}

/// Casts the signed-in user's vote on a pull request.
async fn cast_vote(project: Option<&str>, repo: &str, id: &str, vote: VoteArg) -> Result<()> {
    let ctx = PrContext::new(project, repo, id).await?;
    let identity_id = identity::resolve_identity(&ctx.creds, "@me").await?;

    let mut body = IdentityRefWithVote::new(IdentityRef::new(identity_id.clone()));
    body.vote = Some(vote.as_api_value());

    ctx.client
        .pull_request_reviewers_client()
        .create_pull_request_reviewer(
            &ctx.creds.organization,
            body,
            &ctx.repository_id,
            ctx.pull_request_id,
            &identity_id,
            &ctx.project,
        )
        .await?;

    println!(
        "{}",
        format!(
            "✅ Vote recorded: {}",
            vote_label(Some(vote.as_api_value()))
        )
        .green()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn votes_map_to_the_api_values() {
        assert_eq!(VoteArg::Approve.as_api_value(), 10);
        assert_eq!(VoteArg::ApproveWithSuggestions.as_api_value(), 5);
        assert_eq!(VoteArg::Reset.as_api_value(), 0);
        assert_eq!(VoteArg::WaitForAuthor.as_api_value(), -5);
        assert_eq!(VoteArg::Reject.as_api_value(), -10);
    }

    #[test]
    fn vote_labels_match_the_web_ui() {
        assert_eq!(vote_label(Some(10)), "approved");
        assert_eq!(vote_label(Some(5)), "approved with suggestions");
        assert_eq!(vote_label(Some(-5)), "waiting for author");
        assert_eq!(vote_label(Some(-10)), "rejected");
        assert_eq!(vote_label(Some(0)), "no vote");
        assert_eq!(vote_label(None), "no vote");
    }

    #[test]
    fn reviewer_name_prefers_the_display_name() {
        let mut reviewer = IdentityRefWithVote::new(IdentityRef::new("the-id".to_string()));
        reviewer.identity_ref.unique_name = Some("someone@example.com".to_string());
        reviewer.identity_ref.graph_subject_base.display_name = Some("Someone".to_string());

        assert_eq!(reviewer_name(&reviewer), "Someone");
    }

    #[test]
    fn reviewer_name_falls_back_to_unique_name_then_id() {
        let mut reviewer = IdentityRefWithVote::new(IdentityRef::new("the-id".to_string()));
        assert_eq!(reviewer_name(&reviewer), "the-id");

        reviewer.identity_ref.unique_name = Some("someone@example.com".to_string());
        assert_eq!(reviewer_name(&reviewer), "someone@example.com");
    }
}
