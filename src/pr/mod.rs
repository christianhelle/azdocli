//! Pull request commands for `azdocli repos pr`.
//!
//! The subcommand enum and routing live here; each group of operations lives in
//! its own submodule so no single file has to hold the whole surface.

mod comments;
mod complete;
mod create;
mod http;
mod identity;
mod list;
mod reviewers;
mod show;
mod update;

use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::{get_credentials, Credentials};
use crate::project::get_project_or_default;
use anyhow::{Context, Result};
use azure_devops_rust_api::git;
use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Clone)]
pub enum PullRequestsSubCommands {
    /// Create new pull request
    Create {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to create a pull request in
        #[clap(short, long)]
        repo: String,

        /// Title of the pull request
        #[clap(short, long)]
        title: Option<String>,

        /// Description of the pull request
        #[clap(short, long)]
        description: Option<String>,

        /// Path to a markdown file containing the pull request description
        #[clap(long, value_name = "PATH")]
        description_file: Option<PathBuf>,

        /// Source branch for the pull request (e.g., 'feature/my-feature')
        #[clap(short, long)]
        source: String,

        /// Target branch for the pull request (defaults to 'main')
        #[clap(long, default_value = "main")]
        target: String,

        /// Create the pull request as a draft
        #[clap(long)]
        draft: bool,

        /// Reviewer to assign, as an email address, identity ID, or '@me'
        /// (repeat for multiple reviewers)
        #[clap(long)]
        reviewer: Vec<String>,

        /// Work item to link to the pull request (repeat for multiple items)
        #[clap(long)]
        work_item: Vec<i32>,

        /// Label to apply to the pull request (repeat for multiple labels)
        #[clap(long)]
        label: Vec<String>,

        /// Complete the pull request automatically once policies pass
        #[clap(long)]
        auto_complete: bool,

        /// Delete the source branch when the pull request completes
        #[clap(long)]
        delete_source_branch: bool,
    },
    /// List pull requests
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to list pull requests from
        #[clap(short, long)]
        repo: String,

        /// Only list pull requests in this state
        #[clap(long, value_enum, default_value = "active")]
        status: list::StatusFilter,

        /// Only list pull requests opened by this user
        /// (email address, identity ID, or '@me')
        #[clap(long)]
        creator: Option<String>,

        /// Only list pull requests this user is reviewing
        /// (email address, identity ID, or '@me')
        #[clap(long)]
        reviewer: Option<String>,

        /// Only list pull requests from this source branch
        #[clap(short, long)]
        source: Option<String>,

        /// Only list pull requests targeting this branch
        #[clap(long)]
        target: Option<String>,

        /// Maximum number of pull requests to return
        #[clap(long)]
        top: Option<i32>,
    },
    /// Show details of a specific pull request
    Show {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to show pull requests from
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request to show
        #[clap(short, long)]
        id: String,

        /// Open the pull request in a browser instead of printing it
        #[clap(long)]
        web: bool,
    },
    /// Show commits in a pull request
    Commits {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to show commits from
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request to show commits for
        #[clap(short, long)]
        id: String,
    },
    /// Update an existing pull request
    Update {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request to update
        #[clap(short, long)]
        id: String,

        /// New title for the pull request
        #[clap(short, long)]
        title: Option<String>,

        /// New description for the pull request
        #[clap(short, long)]
        description: Option<String>,

        /// Path to a markdown file containing the pull request description
        #[clap(long, value_name = "PATH")]
        description_file: Option<PathBuf>,
    },
    /// Complete (merge) a pull request
    Complete {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request to complete
        #[clap(short, long)]
        id: String,

        /// Strategy used to merge the source branch
        #[clap(long, value_enum)]
        merge_strategy: Option<complete::MergeStrategyArg>,

        /// Delete the source branch after merging
        #[clap(long)]
        delete_source_branch: bool,

        /// Message for the merge commit
        #[clap(long)]
        merge_commit_message: Option<String>,

        /// Complete even when branch policies are not satisfied
        #[clap(long)]
        bypass_policy: bool,

        /// Reason recorded when bypassing branch policies
        #[clap(long, requires = "bypass_policy")]
        bypass_reason: Option<String>,

        /// Set the pull request to complete automatically once policies pass,
        /// instead of completing it now
        #[clap(long)]
        auto_complete: bool,

        /// Skip the confirmation prompt
        #[clap(short = 'y', long = "yes")]
        skip_confirmation: bool,
    },
    /// Show the comment threads on a pull request
    Threads {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request
        #[clap(short, long)]
        id: String,

        /// Include system-generated and deleted threads
        #[clap(long)]
        all: bool,
    },
    /// Comment on a pull request
    Comment {
        #[clap(subcommand)]
        subcommand: comments::CommentSubCommands,
    },
    /// Manage the reviewers of a pull request
    Reviewers {
        #[clap(subcommand)]
        subcommand: reviewers::ReviewersSubCommands,
    },
    /// Abandon a pull request
    Abandon {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request to abandon
        #[clap(short, long)]
        id: String,

        /// Skip the confirmation prompt
        #[clap(short = 'y', long = "yes")]
        skip_confirmation: bool,
    },
    /// Reactivate an abandoned pull request
    Reactivate {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository containing the pull request
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request to reactivate
        #[clap(short, long)]
        id: String,
    },
}

/// Creates an authenticated Azure DevOps Git client.
fn create_git_client() -> Result<git::Client> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok(factory.build_git())
}

/// Parses a pull request ID supplied on the command line.
fn parse_pr_id(id: &str) -> Result<i32> {
    id.parse::<i32>()
        .map_err(|_| anyhow::anyhow!("Invalid pull request ID, must be a number"))
}

/// Routes pull-request subcommands to their handlers.
pub async fn handle_command(subcommand: &PullRequestsSubCommands) -> anyhow::Result<()> {
    match subcommand {
        PullRequestsSubCommands::Create {
            project,
            repo,
            title,
            description,
            description_file,
            source,
            target,
            draft,
            reviewer,
            work_item,
            label,
            auto_complete,
            delete_source_branch,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let description =
                resolve_description(description.as_deref(), description_file.as_deref()).await?;
            create::create_pull_request(
                &project_name,
                repo,
                title.as_deref(),
                description.as_deref(),
                source,
                target,
                *draft,
                reviewer,
                work_item,
                label,
                *auto_complete,
                *delete_source_branch,
            )
            .await?;
        }
        PullRequestsSubCommands::List {
            project,
            repo,
            status,
            creator,
            reviewer,
            source,
            target,
            top,
        } => {
            let filters = list::ListFilters {
                status: *status,
                creator: creator.as_deref(),
                reviewer: reviewer.as_deref(),
                source: source.as_deref(),
                target: target.as_deref(),
                top: *top,
            };
            list::list_pull_requests(project.as_deref(), repo, &filters).await?;
        }
        PullRequestsSubCommands::Show {
            project,
            repo,
            id,
            web,
        } => {
            show::show_pull_request(project.as_deref(), repo, id, *web).await?;
        }
        PullRequestsSubCommands::Commits {
            ref project,
            repo,
            id,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            list::list_pull_request_commits(repo, id, project_name).await?;
        }
        PullRequestsSubCommands::Update {
            project,
            repo,
            id,
            title,
            description,
            description_file,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let description =
                resolve_description(description.as_deref(), description_file.as_deref()).await?;
            update::update_pull_request(
                &project_name,
                repo,
                id,
                title.as_deref(),
                description.as_deref(),
            )
            .await?;
        }
        PullRequestsSubCommands::Complete {
            project,
            repo,
            id,
            merge_strategy,
            delete_source_branch,
            merge_commit_message,
            bypass_policy,
            bypass_reason,
            auto_complete,
            skip_confirmation,
        } => {
            let settings = complete::CompletionSettings {
                merge_strategy: *merge_strategy,
                delete_source_branch: *delete_source_branch,
                merge_commit_message: merge_commit_message.as_deref(),
                bypass_policy: *bypass_policy,
                bypass_reason: bypass_reason.as_deref(),
            };
            complete::complete_pull_request(
                project.as_deref(),
                repo,
                id,
                &settings,
                *auto_complete,
                *skip_confirmation,
            )
            .await?;
        }
        PullRequestsSubCommands::Threads {
            project,
            repo,
            id,
            all,
        } => {
            comments::list_threads(project.as_deref(), repo, id, *all).await?;
        }
        PullRequestsSubCommands::Comment { subcommand } => {
            comments::handle_command(subcommand).await?;
        }
        PullRequestsSubCommands::Reviewers { subcommand } => {
            reviewers::handle_command(subcommand).await?;
        }
        PullRequestsSubCommands::Abandon {
            project,
            repo,
            id,
            skip_confirmation,
        } => {
            complete::abandon_pull_request(project.as_deref(), repo, id, *skip_confirmation)
                .await?;
        }
        PullRequestsSubCommands::Reactivate { project, repo, id } => {
            complete::reactivate_pull_request(project.as_deref(), repo, id).await?;
        }
    }
    Ok(())
}

/// Everything a pull-request subcommand needs: resolved credentials, a Git
/// client, and the resolved repository and pull request identifiers.
struct PrContext {
    creds: Credentials,
    client: git::Client,
    project: String,
    repository_id: String,
    pull_request_id: i32,
}

impl PrContext {
    /// Resolves the project and repository for a subcommand that works on a
    /// repository rather than a single pull request.
    async fn for_repo(project: Option<&str>, repo: &str) -> Result<Self> {
        let project = get_project_or_default(project)?;
        let creds = get_credentials()?;
        let factory = CredentialClientFactory::new(&creds)?;
        let client = factory.build_git();
        let repository = crate::repos::get_repo(&project, repo).await?;

        Ok(Self {
            creds,
            client,
            project,
            repository_id: repository.id,
            pull_request_id: 0,
        })
    }

    /// Resolves the project, repository and pull request ID for a subcommand.
    async fn new(project: Option<&str>, repo: &str, id: &str) -> Result<Self> {
        let mut ctx = Self::for_repo(project, repo).await?;
        ctx.pull_request_id = parse_pr_id(id)?;
        Ok(ctx)
    }

    /// Fetches the pull request this context points at.
    async fn get_pull_request(&self) -> Result<git::models::GitPullRequest> {
        Ok(self
            .client
            .pull_requests_client()
            .get_pull_request(
                &self.creds.organization,
                &self.repository_id,
                self.pull_request_id,
                &self.project,
            )
            .await?)
    }

    /// Changes the status of the pull request.
    async fn set_status(&self, status: git::models::PullRequestStatus) -> Result<()> {
        self.client
            .pull_requests_client()
            .update(
                &self.creds.organization,
                &self.repository_id,
                &self.project,
                self.pull_request_id,
                complete::build_status_options(status),
            )
            .await?;
        Ok(())
    }
}

/// Resolves a pull request description from inline text or a markdown file.
///
/// When `description_file` is provided, its contents take precedence over
/// `description`, matching the behavior of `repos pr create`.
async fn resolve_description(
    description: Option<&str>,
    description_file: Option<&Path>,
) -> Result<Option<String>> {
    match description_file {
        Some(path) => {
            let path = path.to_path_buf();
            let path_display = path.display().to_string();
            let content = tokio::task::spawn_blocking(move || std::fs::read_to_string(&path))
                .await
                .context("Failed to read description file in blocking task")?
                .with_context(|| format!("Failed to read description file '{}'", path_display))?;
            Ok(Some(content))
        }
        None => Ok(description.map(|d| d.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::io::Write;

    /// A parser over just the pull request subcommands, so the argument wiring
    /// can be exercised without going through the whole CLI.
    #[derive(Parser)]
    struct TestCli {
        #[clap(subcommand)]
        command: PullRequestsSubCommands,
    }

    fn parse(args: &[&str]) -> Result<PullRequestsSubCommands, clap::Error> {
        TestCli::try_parse_from(std::iter::once("pr").chain(args.iter().copied()))
            .map(|cli| cli.command)
    }

    #[test]
    fn list_defaults_to_active_pull_requests() {
        let command = parse(&["list", "--repo", "r"]).unwrap();

        let PullRequestsSubCommands::List { status, top, .. } = command else {
            panic!("expected List");
        };
        assert_eq!(status, list::StatusFilter::Active);
        assert_eq!(top, None);
    }

    #[test]
    fn list_accepts_every_filter() {
        let command = parse(&[
            "list",
            "--repo",
            "r",
            "--status",
            "all",
            "--creator",
            "@me",
            "--reviewer",
            "someone@example.com",
            "--source",
            "feature",
            "--target",
            "main",
            "--top",
            "5",
        ])
        .unwrap();

        let PullRequestsSubCommands::List {
            status,
            creator,
            reviewer,
            source,
            target,
            top,
            ..
        } = command
        else {
            panic!("expected List");
        };
        assert_eq!(status, list::StatusFilter::All);
        assert_eq!(creator.as_deref(), Some("@me"));
        assert_eq!(reviewer.as_deref(), Some("someone@example.com"));
        assert_eq!(source.as_deref(), Some("feature"));
        assert_eq!(target.as_deref(), Some("main"));
        assert_eq!(top, Some(5));
    }

    #[test]
    fn create_collects_repeated_reviewers_work_items_and_labels() {
        let command = parse(&[
            "create",
            "--repo",
            "r",
            "--source",
            "feature",
            "--draft",
            "--reviewer",
            "a@example.com",
            "--reviewer",
            "b@example.com",
            "--work-item",
            "11",
            "--work-item",
            "22",
            "--label",
            "bug",
        ])
        .unwrap();

        let PullRequestsSubCommands::Create {
            draft,
            reviewer,
            work_item,
            label,
            target,
            ..
        } = command
        else {
            panic!("expected Create");
        };
        assert!(draft);
        assert_eq!(reviewer, vec!["a@example.com", "b@example.com"]);
        assert_eq!(work_item, vec![11, 22]);
        assert_eq!(label, vec!["bug"]);
        assert_eq!(target, "main");
    }

    #[test]
    fn comment_add_requires_file_and_line_together() {
        assert!(parse(&[
            "comment",
            "add",
            "--repo",
            "r",
            "--id",
            "1",
            "--message",
            "m",
            "--file",
            "/a.rs",
        ])
        .is_err());

        assert!(parse(&[
            "comment",
            "add",
            "--repo",
            "r",
            "--id",
            "1",
            "--message",
            "m",
            "--line",
            "3",
        ])
        .is_err());

        assert!(parse(&[
            "comment",
            "add",
            "--repo",
            "r",
            "--id",
            "1",
            "--message",
            "m",
            "--file",
            "/a.rs",
            "--line",
            "3",
        ])
        .is_ok());
    }

    #[test]
    fn comment_resolve_defaults_to_fixed() {
        let command = parse(&[
            "comment", "resolve", "--repo", "r", "--id", "1", "--thread", "9",
        ])
        .unwrap();

        let PullRequestsSubCommands::Comment { subcommand } = command else {
            panic!("expected Comment");
        };
        let comments::CommentSubCommands::Resolve { status, thread, .. } = subcommand else {
            panic!("expected Resolve");
        };
        assert_eq!(status, comments::ThreadStatusArg::Fixed);
        assert_eq!(thread, 9);
    }

    #[test]
    fn complete_requires_bypass_policy_for_a_bypass_reason() {
        assert!(parse(&[
            "complete",
            "--repo",
            "r",
            "--id",
            "1",
            "--bypass-reason",
            "hotfix",
        ])
        .is_err());

        assert!(parse(&[
            "complete",
            "--repo",
            "r",
            "--id",
            "1",
            "--bypass-policy",
            "--bypass-reason",
            "hotfix",
            "--merge-strategy",
            "squash",
        ])
        .is_ok());
    }

    #[test]
    fn reviewers_add_requires_at_least_one_reviewer() {
        assert!(parse(&["reviewers", "add", "--repo", "r", "--id", "1"]).is_err());

        let command = parse(&[
            "reviewers",
            "add",
            "--repo",
            "r",
            "--id",
            "1",
            "--reviewer",
            "a@example.com",
            "--required",
        ])
        .unwrap();

        let PullRequestsSubCommands::Reviewers { subcommand } = command else {
            panic!("expected Reviewers");
        };
        let reviewers::ReviewersSubCommands::Add {
            reviewer, required, ..
        } = subcommand
        else {
            panic!("expected Add");
        };
        assert_eq!(reviewer, vec!["a@example.com"]);
        assert!(required);
    }

    #[test]
    fn reviewers_vote_parses_every_option() {
        for (arg, expected) in [
            ("approve", reviewers::VoteArg::Approve),
            (
                "approve-with-suggestions",
                reviewers::VoteArg::ApproveWithSuggestions,
            ),
            ("reset", reviewers::VoteArg::Reset),
            ("wait-for-author", reviewers::VoteArg::WaitForAuthor),
            ("reject", reviewers::VoteArg::Reject),
        ] {
            let command = parse(&[
                "reviewers",
                "vote",
                "--repo",
                "r",
                "--id",
                "1",
                "--vote",
                arg,
            ])
            .unwrap();

            let PullRequestsSubCommands::Reviewers { subcommand } = command else {
                panic!("expected Reviewers");
            };
            let reviewers::ReviewersSubCommands::Vote { vote, .. } = subcommand else {
                panic!("expected Vote");
            };
            assert_eq!(vote, expected, "for --vote {arg}");
        }
    }

    #[test]
    fn abandon_supports_skipping_confirmation() {
        let command = parse(&["abandon", "--repo", "r", "--id", "1", "-y"]).unwrap();

        let PullRequestsSubCommands::Abandon {
            skip_confirmation, ..
        } = command
        else {
            panic!("expected Abandon");
        };
        assert!(skip_confirmation);
    }

    #[test]
    fn threads_hides_system_threads_unless_asked() {
        let command = parse(&["threads", "--repo", "r", "--id", "1"]).unwrap();

        let PullRequestsSubCommands::Threads { all, .. } = command else {
            panic!("expected Threads");
        };
        assert!(!all);
    }

    #[tokio::test]
    async fn resolve_description_returns_inline_when_no_file() {
        assert_eq!(
            resolve_description(Some("inline description"), None)
                .await
                .unwrap(),
            Some("inline description".to_string())
        );
    }

    #[tokio::test]
    async fn resolve_description_returns_none_when_nothing_provided() {
        assert_eq!(resolve_description(None, None).await.unwrap(), None);
    }

    #[tokio::test]
    async fn resolve_description_reads_file_contents() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        write!(temp, "# Markdown description\n\nWith **bold** text.").unwrap();

        let result = resolve_description(None, Some(temp.path())).await.unwrap();

        assert_eq!(
            result,
            Some("# Markdown description\n\nWith **bold** text.".to_string())
        );
    }

    #[tokio::test]
    async fn resolve_description_file_wins_over_inline() {
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        write!(temp, "file contents").unwrap();

        let result = resolve_description(Some("inline"), Some(temp.path()))
            .await
            .unwrap();

        assert_eq!(result, Some("file contents".to_string()));
    }

    #[tokio::test]
    async fn resolve_description_returns_error_for_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("does/not/exist.md");

        let result = resolve_description(None, Some(&missing)).await;

        assert!(result.is_err());
        let message = format!("{}", result.unwrap_err());
        assert!(message.contains("Failed to read description file"));
        assert!(message.contains(missing.display().to_string().as_str()));
    }

    #[tokio::test]
    async fn resolve_description_reads_empty_file() {
        let temp = tempfile::NamedTempFile::new().unwrap();

        let result = resolve_description(None, Some(temp.path())).await.unwrap();

        assert_eq!(result, Some("".to_string()));
    }

    #[test]
    fn parse_pr_id_accepts_numbers() {
        assert_eq!(parse_pr_id("42").unwrap(), 42);
    }

    #[test]
    fn parse_pr_id_rejects_non_numbers() {
        assert!(parse_pr_id("abc").is_err());
    }

    #[test]
    fn description_file_accepts_pathbuf() {
        let path = PathBuf::from("description.md");
        let command = PullRequestsSubCommands::Create {
            project: None,
            repo: "repo".to_string(),
            title: None,
            description: None,
            description_file: Some(path.clone()),
            source: "feature".to_string(),
            target: "main".to_string(),
            draft: false,
            reviewer: Vec::new(),
            work_item: Vec::new(),
            label: Vec::new(),
            auto_complete: false,
            delete_source_branch: false,
        };

        if let PullRequestsSubCommands::Create {
            description_file: Some(actual),
            ..
        } = command
        {
            assert_eq!(actual, path);
        } else {
            panic!("expected Create with description_file");
        }
    }

    #[test]
    fn update_variant_accepts_title_and_description() {
        let command = PullRequestsSubCommands::Update {
            project: None,
            repo: "my-repo".to_string(),
            id: "123".to_string(),
            title: Some("New title".to_string()),
            description: Some("New description".to_string()),
            description_file: None,
        };

        if let PullRequestsSubCommands::Update {
            repo,
            id,
            title,
            description,
            ..
        } = command
        {
            assert_eq!(repo, "my-repo");
            assert_eq!(id, "123");
            assert_eq!(title, Some("New title".to_string()));
            assert_eq!(description, Some("New description".to_string()));
        } else {
            panic!("expected Update variant");
        }
    }

    #[test]
    fn update_variant_supports_description_file() {
        let path = PathBuf::from("desc.md");
        let command = PullRequestsSubCommands::Update {
            project: Some("proj".to_string()),
            repo: "repo".to_string(),
            id: "42".to_string(),
            title: None,
            description: None,
            description_file: Some(path.clone()),
        };

        if let PullRequestsSubCommands::Update {
            description_file: Some(actual),
            ..
        } = command
        {
            assert_eq!(actual, path);
        } else {
            panic!("expected Update with description_file");
        }
    }
}
