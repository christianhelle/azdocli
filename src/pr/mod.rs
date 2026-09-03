//! Pull request commands for `azdocli repos pr`.
//!
//! The subcommand enum and routing live here; each group of operations lives in
//! its own submodule so no single file has to hold the whole surface.

mod create;
mod list;
mod show;
mod update;

use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
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
    },
    /// List pull requests
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to list pull requests from
        #[clap(short, long)]
        repo: String,
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
            )
            .await?;
        }
        PullRequestsSubCommands::List { project, repo } => {
            let project_name = get_project_or_default(project.as_deref())?;
            list::list_pull_requests(&project_name, repo).await?;
        }
        PullRequestsSubCommands::Show { project, repo, id } => {
            let project_name = get_project_or_default(project.as_deref())?;
            show::show_pull_request(&project_name, repo, id).await?;
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
    }
    Ok(())
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
    use std::io::Write;

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
