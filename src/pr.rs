use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use crate::repos;
use anyhow::Result;
use azure_devops_rust_api::git;
use clap::Subcommand;
use std::process::Command;

#[derive(Subcommand, Clone)]
pub enum PullRequestsSubCommands {
    /// Create new pull request
    Create {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to create a pull request in (auto-detected from git remote if omitted)
        #[clap(short, long)]
        repo: Option<String>,

        /// Title of the pull request
        #[clap(short, long)]
        title: Option<String>,

        /// Description of the pull request
        #[clap(short, long)]
        description: Option<String>,

        /// Source branch for the pull request (auto-detected from current git branch if omitted)
        #[clap(short, long)]
        source: Option<String>,

        /// Target branch for the pull request (auto-detected from upstream, then defaults to 'main')
        #[clap(long)]
        target: Option<String>,
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
    let factory = CredentialClientFactory::new(&creds);
    Ok(factory.build_git())
}

#[derive(Clone)]
struct DetectedRemoteContext {
    project: String,
    repo: String,
}

fn run_git_command(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run git {}: {}", args.join(" "), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(anyhow::anyhow!(
            "Git command failed (git {}): {}",
            args.join(" "),
            stderr
        ));
    }

    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_string())
        .map_err(|e| anyhow::anyhow!("Git output was not valid UTF-8: {}", e))
}

fn strip_refs_heads_prefix(branch: &str) -> &str {
    branch
        .trim()
        .strip_prefix("refs/heads/")
        .unwrap_or(branch.trim())
}

fn detect_current_branch() -> Result<String> {
    let branch = run_git_command(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch == "HEAD" {
        return Err(anyhow::anyhow!(
            "Unable to detect source branch from git: HEAD is detached. Pass --source explicitly."
        ));
    }
    Ok(branch)
}

fn detect_upstream_target_branch(source_branch: &str) -> Option<String> {
    let source = strip_refs_heads_prefix(source_branch);

    // Walk decorated commit ancestry to find the first remote branch that isn't
    // the source branch's own remote tracking ref.
    let log_output = run_git_command(&[
        "log",
        "--pretty=format:%D",
        "--simplify-by-decoration",
        "HEAD",
    ])
    .ok()?;

    for line in log_output.lines() {
        for decoration in line.split(',') {
            let decoration = decoration.trim();
            if decoration.is_empty()
                || decoration.starts_with("HEAD")
                || decoration.starts_with("tag:")
                || decoration.ends_with("/HEAD")
                || decoration == source
                || decoration.ends_with(&format!("/{source}"))
            {
                continue;
            }
            if let Some((_, branch_name)) = decoration.split_once('/') {
                return Some(branch_name.to_string());
            }
        }
    }

    None
}

fn strip_git_suffix(remote_url: &str) -> String {
    remote_url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string()
}

fn parse_azure_remote_context(remote_url: &str) -> Option<DetectedRemoteContext> {
    let normalized = strip_git_suffix(remote_url);

    if let Some(index) = normalized.find("dev.azure.com/") {
        let path = &normalized[index + "dev.azure.com/".len()..];
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() >= 4 && segments[2] == "_git" {
            return Some(DetectedRemoteContext {
                project: segments[1].to_string(),
                repo: segments[3].to_string(),
            });
        }
    }

    if let Some(index) = normalized.find(".visualstudio.com/") {
        let path = &normalized[index + ".visualstudio.com/".len()..];
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() >= 3 && segments[1] == "_git" {
            return Some(DetectedRemoteContext {
                project: segments[0].to_string(),
                repo: segments[2].to_string(),
            });
        }
    }

    if let Some(index) = normalized.find("ssh.dev.azure.com:v3/") {
        let path = &normalized[index + "ssh.dev.azure.com:v3/".len()..];
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() >= 3 {
            return Some(DetectedRemoteContext {
                project: segments[1].to_string(),
                repo: segments[2].to_string(),
            });
        }
    }

    None
}

fn detect_selected_azure_remote_context() -> Result<DetectedRemoteContext> {
    let remotes_output = run_git_command(&["remote"]).map_err(|_| {
        anyhow::anyhow!(
            "Unable to detect git remotes. Run this command inside a git repository or pass --repo and --source explicitly."
        )
    })?;

    let remotes: Vec<String> = remotes_output
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if remotes.is_empty() {
        return Err(anyhow::anyhow!(
            "No git remotes found. Pass --repo explicitly."
        ));
    }

    let mut azure_contexts: Vec<(String, DetectedRemoteContext)> = Vec::new();
    for remote in remotes {
        if let Ok(url) = run_git_command(&["remote", "get-url", &remote]) {
            if let Some(context) = parse_azure_remote_context(&url) {
                azure_contexts.push((remote, context));
            }
        }
    }

    if let Some((_, context)) = azure_contexts.iter().find(|(name, _)| name == "origin") {
        return Ok(context.clone());
    }

    match azure_contexts.len() {
        0 => Err(anyhow::anyhow!(
            "No Azure DevOps remotes found. Pass --repo explicitly."
        )),
        1 => Ok(azure_contexts.remove(0).1),
        _ => Err(anyhow::anyhow!(
            "Multiple Azure DevOps remotes found and 'origin' is not Azure DevOps. Pass --repo explicitly."
        )),
    }
}

fn resolve_repo_and_project(project: Option<&str>, repo: Option<&str>) -> Result<(String, String)> {
    let mut detected_context: Option<DetectedRemoteContext> = None;

    let resolved_repo = match repo {
        Some(repo_name) => repo_name.to_string(),
        None => {
            let context = detect_selected_azure_remote_context()?;
            let repo_name = context.repo.clone();
            detected_context = Some(context);
            repo_name
        }
    };

    let resolved_project = match project {
        Some(project_name) => project_name.to_string(),
        None => {
            if detected_context.is_none() {
                detected_context = detect_selected_azure_remote_context().ok();
            }

            if let Some(context) = detected_context {
                context.project
            } else {
                get_project_or_default(None)?
            }
        }
    };

    Ok((resolved_project, resolved_repo))
}

fn resolve_source_branch(source: Option<&str>) -> Result<String> {
    match source {
        Some(source_branch) => Ok(strip_refs_heads_prefix(source_branch).to_string()),
        None => detect_current_branch().map(|branch| strip_refs_heads_prefix(&branch).to_string()),
    }
}

fn resolve_target_branch(target: Option<&str>, source_branch: &str) -> String {
    target
        .map(|target_branch| strip_refs_heads_prefix(target_branch).to_string())
        .or_else(|| detect_upstream_target_branch(source_branch))
        .unwrap_or_else(|| "main".to_string())
}

pub async fn handle_command(subcommand: &PullRequestsSubCommands) -> anyhow::Result<()> {
    match subcommand {
        PullRequestsSubCommands::Create {
            project,
            repo,
            title,
            description,
            source,
            target,
        } => {
            create_pull_request(
                project.as_deref(),
                repo.as_deref(),
                title.as_deref(),
                description.as_deref(),
                source.as_deref(),
                target.as_deref(),
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

async fn create_pull_request(
    project: Option<&str>,
    repo: Option<&str>,
    title: Option<&str>,
    description: Option<&str>,
    source: Option<&str>,
    target: Option<&str>,
) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_git_client()?;
            let (project_name, repo_name) = resolve_repo_and_project(project, repo)?;
            let source_branch = resolve_source_branch(source)?;
            let target_branch = resolve_target_branch(target, &source_branch);

            let repository = repos::get_repo(&project_name, &repo_name).await?;

            let pr_client = client.pull_requests_client();

            let source_ref = if source_branch.starts_with("refs/heads/") {
                source_branch.to_string()
            } else {
                format!("refs/heads/{source_branch}")
            };

            let target_ref = if target_branch.starts_with("refs/heads/") {
                target_branch.to_string()
            } else {
                format!("refs/heads/{target_branch}")
            };
            println!("Creating pull request:");
            println!("  Project: {project_name}");
            println!("  Repository: {repo_name}");
            println!("  Source branch: {source_branch}");
            println!("  Target branch: {target_branch}");
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
                .create(&creds.organization, &repository.id, &project_name, pr_options)
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
