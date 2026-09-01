use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use crate::repos;
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
}

fn create_git_client() -> Result<git::Client> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok(factory.build_git())
}

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
            create_pull_request(
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
            list_pull_requests(&project_name, repo).await?;
        }
        PullRequestsSubCommands::Show { project, repo, id } => {
            let project_name = get_project_or_default(project.as_deref())?;
            show_pull_request(&project_name, repo, id).await?;
        }
        PullRequestsSubCommands::Commits {
            ref project,
            repo,
            id,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            list_pull_request_commits(repo, id, project_name).await?;
        }
    }
    Ok(())
}

async fn list_pull_request_commits(repo: &String, id: &String, project_name: String) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;
            let pr_client = client.pull_request_commits_client();

            let pr_id = id
                .parse::<i32>()
                .map_err(|_| anyhow::anyhow!("Invalid pull request ID, must be a number"))?;

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

async fn create_pull_request(
    project: &str,
    repo: &str,
    title: Option<&str>,
    description: Option<&str>,
    source: &str,
    target: &str,
) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;

            let repository = repos::get_repo(project, repo).await?;

            let pr_client = client.pull_requests_client();

            let source_ref = if source.starts_with("refs/heads/") {
                source.to_string()
            } else {
                format!("refs/heads/{source}")
            };

            let target_ref = if target.starts_with("refs/heads/") {
                target.to_string()
            } else {
                format!("refs/heads/{target}")
            };
            println!("Creating pull request:");
            println!("  Repository: {repo}");
            println!("  Source branch: {source}");
            println!("  Target branch: {target}");
            println!("  Title: {}", title.unwrap_or("Default title"));

            let pr_options = git::models::GitPullRequestCreateOptions {
                source_ref_name: source_ref.clone(),
                target_ref_name: target_ref.clone(),
                title: title.unwrap_or("Pull Request").to_string(),
                description: description.map(|d| d.to_string()),
                is_draft: Some(false),
                labels: Vec::new(),
                merge_options: None,
                completion_options: None,
                work_item_refs: Vec::new(),
                reviewers: Vec::new(),
            };

            match pr_client
                .create(&creds.organization, &repository.id, project, pr_options)
                .await
            {
                Ok(created_pr) => {
                    println!("✅ Pull request created successfully!");
                    println!("  ID: {}", created_pr.pull_request_id);
                    println!("  Title: {}", created_pr.title.unwrap_or_default());
                    println!("  URL: {}", created_pr.url);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Failed to create pull request: {e}");
                    Err(anyhow::anyhow!("Failed to create pull request: {}", e))
                }
            }
        }
        Err(e) => {
            eprintln!("Unable to create pull request");
            Err(e)
        }
    }
}

async fn list_pull_requests(project: &str, repo: &str) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;
            let pr_client = client.pull_requests_client();

            let pull_requests = pr_client
                .get_pull_requests_by_project(creds.organization, project)
                .await?;

            let filtered_prs: Vec<_> = pull_requests
                .value
                .into_iter()
                .filter(|pr| pr.repository.name == repo)
                .collect();

            if filtered_prs.is_empty() {
                println!("No pull requests found for repository '{repo}' in project '{project}'");
            } else {
                println!("Pull requests for repository '{repo}' in project '{project}':");
                for pr in filtered_prs {
                    println!(
                        "  #{} - {}",
                        pr.pull_request_id,
                        pr.title.unwrap_or_default()
                    );
                }
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to retrieve pull requests");
            Err(e)
        }
    }
}

async fn show_pull_request(project: &str, _repo: &str, id: &str) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;
            let pr_client = client.pull_requests_client();

            let pr_id = id
                .parse::<i32>()
                .map_err(|_| anyhow::anyhow!("Invalid pull request ID, must be a number"))?;

            let pull_request = pr_client
                .get_pull_request_by_id(creds.organization, pr_id, project)
                .await?;

            println!("Pull Request Details:");
            println!("  ID: {}", pull_request.pull_request_id);
            println!("  Title: {}", pull_request.title.unwrap_or_default());
            if let Some(description) = pull_request.description {
                if !description.is_empty() {
                    println!("  Description: {description}");
                }
            }
            println!("  Status: {:?}", pull_request.status);
            println!("  Source Branch: {}", pull_request.source_ref_name);
            println!("  Target Branch: {}", pull_request.target_ref_name);
            println!("  Created: {}", pull_request.creation_date);

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to retrieve pull request");
            Err(e)
        }
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

    #[test]
    fn build_update_options_maps_title_and_description() {
        let opts = build_update_options(Some("My Title"), Some("My Desc"));
        assert_eq!(opts.title, Some("My Title".to_string()));
        assert_eq!(opts.description, Some("My Desc".to_string()));
    }

    #[test]
    fn build_update_options_maps_none_fields() {
        let opts = build_update_options(None, None);
        assert_eq!(opts.title, None);
        assert_eq!(opts.description, None);
    }

    #[test]
    fn validate_update_requires_at_least_one_field() {
        assert!(validate_update_has_changes(None, None).is_err());
        assert!(validate_update_has_changes(Some("title"), None).is_ok());
        assert!(validate_update_has_changes(None, Some("desc")).is_ok());
        assert!(validate_update_has_changes(Some("t"), Some("d")).is_ok());
    }
}
