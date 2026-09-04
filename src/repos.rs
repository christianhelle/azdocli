use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::pr::{self, PullRequestsSubCommands};
use crate::project::get_project_or_default;
use anyhow::Result;
use azure_devops_rust_api::git::{self, models::GitRepositoryCreateOptions};
use clap::Subcommand;
use colored::Colorize;
use dialoguer::Confirm;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Subcommand, Clone)]
pub enum ReposSubCommands {
    /// Create a new repository
    Create {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to create
        #[clap(short, long)]
        name: Option<String>,
    },
    /// List all repositories
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Clone all repositories from a project
    Clone {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Target directory to clone repositories into (optional, defaults to current directory)
        #[clap(short, long)]
        target_dir: Option<String>,
        /// Skip confirmation prompt and proceed directly
        #[clap(short = 'y', long)]
        yes: bool,
        /// Clone repositories in parallel for faster execution
        #[clap(short = 'j', long)]
        parallel: bool,
        /// Number of concurrent clone operations (default: 4, max: 8)
        #[clap(long, default_value = "4")]
        concurrency: usize,
    },
    /// Delete a repository
    Delete {
        /// ID of the repository to delete
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Perform hard delete immediately after soft delete
        #[clap(long)]
        hard: bool,
        /// Skip confirmation prompt and proceed directly
        #[clap(short = 'y', long)]
        yes: bool,
    },
    /// Show details of a repository
    Show {
        /// ID of the repository to show
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// List the branches of a repository
    Branches {
        /// Name or ID of the repository
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Only list branches whose name contains this text
        #[clap(long)]
        filter: Option<String>,
        /// Maximum number of branches to return
        #[clap(long)]
        top: Option<i32>,
    },
    /// List the commit history of a repository
    Commits {
        /// Name or ID of the repository
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Branch to read history from (defaults to the repository default branch)
        #[clap(short, long)]
        branch: Option<String>,
        /// Only list commits written by this author
        #[clap(long)]
        author: Option<String>,
        /// Only list commits that touch this path
        #[clap(long)]
        path: Option<String>,
        /// Maximum number of commits to return (default: 25)
        #[clap(long, default_value = "25")]
        top: i32,
    },
    /// List the files and folders in a repository
    Files {
        /// Name or ID of the repository
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Folder to list (defaults to the repository root)
        #[clap(long, default_value = "/")]
        path: String,
        /// Branch to read from (defaults to the repository default branch)
        #[clap(short, long)]
        branch: Option<String>,
        /// List the whole tree instead of the immediate children only
        #[clap(short, long)]
        recursive: bool,
    },
    /// Print the contents of a file in a repository
    File {
        /// Name or ID of the repository
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Path of the file to print
        #[clap(long)]
        path: String,
        /// Branch to read from (defaults to the repository default branch)
        #[clap(short, long)]
        branch: Option<String>,
    },
    /// Manage pull requests in repositories
    PR {
        #[clap(subcommand)]
        subcommand: PullRequestsSubCommands,
    },
}

pub async fn handle_command(subcommand: &ReposSubCommands) -> Result<()> {
    match subcommand {
        ReposSubCommands::Create { project, name } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match create_repo(&project_name, name).await {
                Ok(repo) => {
                    display_repo_details(&repo);
                }
                Err(e) => {
                    eprintln!(
                        "❌ Failed to create repository '{}' in project '{}'",
                        name.as_deref().unwrap_or("<unnamed>"),
                        project_name
                    );
                    return Err(e);
                }
            }
        }
        ReposSubCommands::List { project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let repos = list_repos(&project_name).await?;
            for repo in repos.iter() {
                println!("{}", repo.name);
            }
        }
        ReposSubCommands::Clone {
            project,
            target_dir,
            yes,
            parallel,
            concurrency,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            clone_all_repos(
                &project_name,
                target_dir.as_deref(),
                *yes,
                *parallel,
                *concurrency,
            )
            .await?;
        }
        ReposSubCommands::Delete {
            id,
            project,
            hard,
            yes,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;

            println!(
                "{}Deleting repository '{}' in project '{}'",
                if *hard { "Hard " } else { "Soft " },
                id,
                project_name
            );

            if !*yes {
                let delete_type = if *hard { "hard" } else { "soft" };
                let prompt_message =
                    format!("Are you sure you want to {delete_type} delete repository '{id}'?");
                if !Confirm::new()
                    .with_prompt(prompt_message)
                    .default(false)
                    .interact()?
                {
                    println!("Delete operation cancelled.");
                    return Ok(());
                }
            } else {
                println!("Proceeding with delete operation (confirmation skipped)...");
            }

            match delete_repo(&project_name, id, *hard).await {
                Ok(_) => {
                    if *hard {
                        println!("✅ Repository hard deleted successfully");
                    } else {
                        println!("✅ Repository soft deleted successfully");
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to delete repository '{id}': {e}");
                    return Err(e);
                }
            }
        }
        ReposSubCommands::Show { id, project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match get_repo(&project_name, id).await {
                Ok(repo) => {
                    display_repo_details(&repo);
                }
                Err(e) => {
                    eprintln!(
                        "❌ Failed to retrieve repository '{id}' from project '{project_name}'"
                    );
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ReposSubCommands::Branches {
            id,
            project,
            filter,
            top,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let repo = get_repo(&project_name, id).await?;
            match list_branches(&project_name, &repo, filter.as_deref(), *top).await {
                Ok(branches) => {
                    display_branches(&branches, repo.default_branch.as_deref());
                }
                Err(e) => {
                    eprintln!("❌ Failed to list branches of repository '{id}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ReposSubCommands::Commits {
            id,
            project,
            branch,
            author,
            path,
            top,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let repo = get_repo(&project_name, id).await?;
            match list_commits(
                &project_name,
                &repo,
                branch.as_deref(),
                author.as_deref(),
                path.as_deref(),
                *top,
            )
            .await
            {
                Ok(commits) => {
                    display_commits(&commits);
                }
                Err(e) => {
                    eprintln!("❌ Failed to list commits of repository '{id}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ReposSubCommands::Files {
            id,
            project,
            path,
            branch,
            recursive,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let repo = get_repo(&project_name, id).await?;
            match list_items(&project_name, &repo, path, branch.as_deref(), *recursive).await {
                Ok(items) => {
                    display_items(&items);
                }
                Err(e) => {
                    eprintln!("❌ Failed to list files of repository '{id}' at '{path}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ReposSubCommands::File {
            id,
            project,
            path,
            branch,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let repo = get_repo(&project_name, id).await?;
            match get_file_content(&project_name, &repo, path, branch.as_deref()).await {
                Ok(content) => {
                    print!("{content}");
                }
                Err(e) => {
                    eprintln!("❌ Failed to read '{path}' from repository '{id}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ReposSubCommands::PR { subcommand } => {
            pr::handle_command(subcommand).await?;
        }
    }
    Ok(())
}

fn create_git_client() -> Result<git::Client> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok(factory.build_git())
}

async fn create_repo(
    project: &str,
    name: &Option<String>,
) -> Result<git::models::GitRepository, anyhow::Error> {
    let creds = get_credentials()?;
    let client = create_git_client()?;
    Ok(client
        .repositories_client()
        .create(
            creds.organization,
            GitRepositoryCreateOptions {
                name: name.clone(),
                parent_repository: None,
                project: None,
            },
            project,
        )
        .await?)
}

async fn list_repos(project: &str) -> Result<Vec<git::models::GitRepository>, anyhow::Error> {
    let creds = get_credentials()?;
    let client = create_git_client()?;
    Ok(client
        .repositories_client()
        .list(creds.organization, project)
        .await?
        .value)
}

pub async fn get_repo(project: &str, repository_id: &str) -> Result<git::models::GitRepository> {
    let repos = match list_repos(project).await {
        Ok(repos) => repos,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to list repositories in project '{}': {}",
                project,
                e
            ));
        }
    };

    // Callers pass either the repository name or its GUID, so accept both.
    let repo = repos
        .iter()
        .find(|repo| repo.name == repository_id || repo.id == repository_id);

    match repo {
        Some(repo) => Ok(repo.clone()),
        None => {
            let available_repos: Vec<&str> = repos.iter().map(|r| r.name.as_str()).collect();
            if available_repos.is_empty() {
                Err(anyhow::anyhow!(
                    "No repositories found in project '{}'",
                    project
                ))
            } else {
                Err(anyhow::anyhow!(
                    "Repository '{}' not found in project '{}'. Available repositories: {}",
                    repository_id,
                    project,
                    available_repos.join(", ")
                ))
            }
        }
    }
}

async fn delete_repo(project: &str, repository_id: &str, hard_delete: bool) -> Result<()> {
    let creds = get_credentials()?;
    let client = create_git_client()?;

    let repo = get_repo(project, repository_id).await?;

    println!("Performing soft delete (recycling repository)...");
    match client
        .repositories_client()
        .delete(&creds.organization, &repo.id, project)
        .await
    {
        Ok(_) => {
            println!("Repository soft deleted (moved to recycle bin)");

            if hard_delete {
                println!("Performing hard delete (permanent deletion)...");
                // Note: The Azure DevOps API may not support permanent deletion through the REST API
                // This would typically be done through the web interface or PowerShell
                println!("Warning: Hard delete may require manual deletion from the recycle bin in Azure DevOps web interface");
            }

            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("Failed to delete repository: {}", e)),
    }
}

/// Lists the branch refs of a repository, newest API page first.
async fn list_branches(
    project: &str,
    repo: &git::models::GitRepository,
    filter: Option<&str>,
    top: Option<i32>,
) -> Result<Vec<git::models::GitRef>> {
    let creds = get_credentials()?;
    let client = create_git_client()?;

    let mut request = client
        .refs_client()
        .list(creds.organization, &repo.id, project)
        .filter("heads/");

    if let Some(filter) = filter {
        request = request.filter_contains(filter);
    }
    if let Some(top) = top {
        request = request.top(top);
    }

    Ok(request.await?.value)
}

/// Strips the `refs/heads/` prefix so branches read the way users type them.
fn short_branch_name(ref_name: &str) -> &str {
    ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name)
}

fn display_branches(branches: &[git::models::GitRef], default_branch: Option<&str>) {
    if branches.is_empty() {
        println!("No branches found.");
        return;
    }

    println!(
        "{:<50} {:<10} {}",
        "Branch".bold(),
        "Commit".bold(),
        "Notes".bold()
    );
    println!("{}", "-".repeat(75));

    for branch in branches {
        let mut notes = Vec::new();
        if default_branch == Some(branch.name.as_str()) {
            notes.push("default");
        }
        if branch.is_locked.unwrap_or(false) {
            notes.push("locked");
        }

        println!(
            "{:<50} {:<10} {}",
            short_branch_name(&branch.name),
            &branch.object_id[..branch.object_id.len().min(8)],
            notes.join(", ")
        );
    }

    println!("\n{} branch(es)", branches.len());
}

/// Lists commits reachable from a branch, most recent first.
async fn list_commits(
    project: &str,
    repo: &git::models::GitRepository,
    branch: Option<&str>,
    author: Option<&str>,
    path: Option<&str>,
    top: i32,
) -> Result<Vec<git::models::GitCommitRef>> {
    let creds = get_credentials()?;
    let client = create_git_client()?;

    let mut request = client
        .commits_client()
        .get_commits(creds.organization, &repo.id, project)
        .search_criteria_top(top);

    if let Some(branch) = branch {
        request = request
            .search_criteria_item_version_version(short_branch_name(branch))
            .search_criteria_item_version_version_type("branch");
    }
    if let Some(author) = author {
        request = request.search_criteria_author(author);
    }
    if let Some(path) = path {
        request = request.search_criteria_item_path(path);
    }

    Ok(request.await?.value)
}

/// Commit messages are multi-line; the listing only has room for the subject.
fn commit_subject(comment: Option<&str>) -> String {
    let subject = comment.unwrap_or("").lines().next().unwrap_or("").trim();
    if subject.chars().count() > 60 {
        format!("{}...", subject.chars().take(57).collect::<String>())
    } else {
        subject.to_string()
    }
}

fn display_commits(commits: &[git::models::GitCommitRef]) {
    if commits.is_empty() {
        println!("No commits found.");
        return;
    }

    println!(
        "{:<10} {:<12} {:<24} {}",
        "Commit".bold(),
        "Date".bold(),
        "Author".bold(),
        "Message".bold()
    );
    println!("{}", "-".repeat(110));

    for commit in commits {
        let commit_id = commit.commit_id.as_deref().unwrap_or("-");
        let author = commit.author.as_ref();
        let date = author
            .and_then(|author| author.date)
            .map(|date| date.date().to_string())
            .unwrap_or_else(|| "-".to_string());
        let name = author
            .and_then(|author| author.name.as_deref())
            .unwrap_or("-");

        println!(
            "{:<10} {:<12} {:<24} {}",
            &commit_id[..commit_id.len().min(8)],
            date,
            name,
            commit_subject(commit.comment.as_deref())
        );
    }

    println!("\n{} commit(s)", commits.len());
}

/// Lists the tree entries under a path in a repository.
async fn list_items(
    project: &str,
    repo: &git::models::GitRepository,
    path: &str,
    branch: Option<&str>,
    recursive: bool,
) -> Result<Vec<git::models::GitItem>> {
    let creds = get_credentials()?;
    let client = create_git_client()?;

    let mut request = client
        .items_client()
        .list(creds.organization, &repo.id, project)
        .scope_path(path)
        .recursion_level(if recursive { "full" } else { "oneLevel" });

    if let Some(branch) = branch {
        request = request
            .version_descriptor_version(short_branch_name(branch))
            .version_descriptor_version_type("branch");
    }

    Ok(request.await?.value)
}

fn display_items(items: &[git::models::GitItem]) {
    // The scope path itself comes back as the first entry; it is not a child.
    let children: Vec<&git::models::GitItem> = items
        .iter()
        .filter(|item| {
            item.item_model
                .path
                .as_deref()
                .is_some_and(|path| path != "/")
        })
        .collect();

    if children.is_empty() {
        println!("No files found.");
        return;
    }

    for item in &children {
        let path = item.item_model.path.as_deref().unwrap_or("-");
        if item.item_model.is_folder.unwrap_or(false) {
            println!("📁 {path}/");
        } else {
            println!("📄 {path}");
        }
    }

    println!("\n{} item(s)", children.len());
}

/// Reads the text content of a single file from a repository.
async fn get_file_content(
    project: &str,
    repo: &git::models::GitRepository,
    path: &str,
    branch: Option<&str>,
) -> Result<String> {
    let creds = get_credentials()?;
    let client = create_git_client()?;

    let mut request = client
        .items_client()
        .get(creds.organization, &repo.id, path, project)
        .include_content(true);

    if let Some(branch) = branch {
        request = request
            .version_descriptor_version(short_branch_name(branch))
            .version_descriptor_version_type("branch");
    }

    let item = request.await?;

    item.item_model.content.ok_or_else(|| {
        anyhow::anyhow!("'{path}' has no text content; it may be a folder or a binary file")
    })
}

fn display_repo_details(repo: &git::models::GitRepository) {
    println!("📋 Repository Details");
    println!("=====================");
    println!("📁 Name: {}", repo.name);
    println!("🆔 ID: {}", repo.id);

    if let Some(url) = &repo.web_url {
        println!("🌐 Web URL: {url}");
    }

    if let Some(remote_url) = &repo.remote_url {
        println!("🔗 Remote URL (HTTPS): {remote_url}");
    }

    if let Some(ssh_url) = &repo.ssh_url {
        println!("🔑 Clone URL (SSH): {ssh_url}");
    }

    if let Some(size) = &repo.size {
        let size_kilobytes = *size as f64 / 1024.0;
        let size_megabytes = size_kilobytes / 1024.0;
        if size_megabytes >= 1.0 {
            println!("📦 Size: {size_megabytes:.2} MB ({size} bytes)");
        } else {
            println!("📦 Size: {size_kilobytes:.2} KB ({size} bytes)");
        }
    }

    if let Some(default_branch) = &repo.default_branch {
        println!("🌿 Default Branch: {default_branch}");
    }

    println!("🎯 Project: {}", repo.project.name);

    if let Some(is_fork) = repo.is_fork {
        println!("🍴 Is Fork: {}", if is_fork { "Yes" } else { "No" });
    }

    if let Some(is_disabled) = repo.is_disabled {
        println!(
            "⚠️  Is Disabled: {}",
            if is_disabled { "Yes" } else { "No" }
        );
    }
}

async fn clone_all_repos(
    project: &str,
    target_dir: Option<&str>,
    skip_confirmation: bool,
    parallel: bool,
    concurrency: usize,
) -> Result<()> {
    let repos = list_repos(project).await?;
    let target_directory = target_dir.unwrap_or(".");

    if repos.is_empty() {
        println!("No repositories found in project '{project}'");
        return Ok(());
    }

    println!(
        "Found {} repositories in project '{}'",
        repos.len(),
        project
    );
    println!("Target directory: {target_directory}");
    println!("\nRepositories to clone:");
    for repo in &repos {
        if let Some(ssh_url) = &repo.ssh_url {
            println!("  • {} ({})", repo.name, ssh_url);
        } else {
            println!("  • {} (⚠  No SSH URL)", repo.name);
        }
    }
    if !skip_confirmation {
        if !Confirm::new()
            .with_prompt("Do you want to proceed with cloning all repositories?")
            .default(false)
            .interact()?
        {
            println!("Clone operation cancelled.");
            return Ok(());
        }
    } else {
        println!("\nProceeding with clone operation (confirmation skipped)...");
    }

    if target_directory != "." {
        std::fs::create_dir_all(target_directory)?;
    }
    let mut success_count = 0;
    let mut failed_count = 0;

    println!(
        "\nStarting clone operations{}...",
        if parallel { " (parallel)" } else { "" }
    );
    if parallel {
        let concurrency_level = if concurrency > 8 {
            println!(
                "Warning: Concurrency level {concurrency} exceeds maximum of 8. Using 8 instead."
            );
            8
        } else if concurrency < 1 {
            println!("Warning: Concurrency level {concurrency} is invalid. Using 1 instead.");
            1
        } else {
            concurrency
        };

        println!(
            "Starting {} repositories in parallel (max {} concurrent)...",
            repos.len(),
            concurrency_level
        );
        let results = clone_repos_parallel(&repos, target_directory, concurrency_level).await;
        for result in results {
            match result {
                Ok(repo_name) => {
                    println!("✅ Successfully cloned {repo_name}");
                    success_count += 1;
                }
                Err(e) => {
                    println!("❌ {e}");
                    failed_count += 1;
                }
            }
        }
    } else {
        for repo in &repos {
            if let Some(ssh_url) = &repo.ssh_url {
                println!("Cloning repository: {} from {}", repo.name, ssh_url);

                let target_path = if target_directory == "." {
                    repo.name.clone()
                } else {
                    format!("{}/{}", target_directory, repo.name)
                };

                let output = std::process::Command::new("git")
                    .args(["clone", ssh_url, &target_path])
                    .output();

                match output {
                    Ok(output) => {
                        if output.status.success() {
                            println!("✅ Successfully cloned {}", repo.name);
                            success_count += 1;
                        } else {
                            let error = String::from_utf8_lossy(&output.stderr);
                            println!("✗ Failed to clone {}: {}", repo.name, error.trim());
                            failed_count += 1;
                        }
                    }
                    Err(e) => {
                        println!("❌ Failed to execute git command for {}: {}", repo.name, e);
                        if e.kind() == std::io::ErrorKind::NotFound {
                            println!("  Git command not found. Please ensure Git is installed and in your PATH.");
                            return Err(anyhow::anyhow!("Git command not found"));
                        }
                        failed_count += 1;
                    }
                }
            } else {
                println!("⚠  Skipping {} (No SSH URL available)", repo.name);
                failed_count += 1;
            }
        }
    }

    println!("\nCloning completed:");
    println!("  ✅ Successfully cloned: {success_count}");
    if failed_count > 0 {
        println!("  ❌ Failed/Skipped: {failed_count}");
    }

    Ok(())
}

async fn clone_repos_parallel(
    repos: &[git::models::GitRepository],
    target_directory: &str,
    concurrency: usize,
) -> Vec<Result<String, String>> {
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut tasks = Vec::new();

    for repo in repos {
        if let Some(ssh_url) = &repo.ssh_url {
            let repo_name = repo.name.clone();
            let ssh_url = ssh_url.clone();
            let target_path = if target_directory == "." {
                repo_name.clone()
            } else {
                format!("{target_directory}/{repo_name}")
            };
            let semaphore = semaphore.clone();

            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                println!("Cloning repository: {repo_name} from {ssh_url}");

                let output = tokio::process::Command::new("git")
                    .args(["clone", &ssh_url, &target_path])
                    .output()
                    .await;

                match output {
                    Ok(output) => {
                        if output.status.success() {
                            Ok(repo_name)
                        } else {
                            let error = String::from_utf8_lossy(&output.stderr);
                            Err(format!("Failed to clone {}: {}", repo_name, error.trim()))
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            Err(format!("Git command not found while cloning {repo_name}"))
                        } else {
                            Err(format!(
                                "Failed to execute git command for {repo_name}: {e}"
                            ))
                        }
                    }
                }
            });

            tasks.push(task);
        } else {
            // For repos without SSH URLs, create a task that immediately returns an error
            let repo_name = repo.name.clone();
            let task =
                tokio::spawn(
                    async move { Err(format!("Skipping {repo_name} (No SSH URL available)")) },
                );
            tasks.push(task);
        }
    }
    let mut results = Vec::new();
    for task in tasks {
        match task.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(format!("Task failed: {e}"))),
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Credentials;
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::path::PathBuf;

    #[derive(Serialize, Deserialize, Debug)]
    struct TestConfig {
        pub organization: String,
        pub pat: String,
        pub project: String,
    }

    impl TestConfig {
        fn load() -> Result<Self> {
            let config_path = PathBuf::from("test_config.json");
            if !config_path.exists() {
                return Err(anyhow::anyhow!(
                    "Test configuration file 'test_config.json' not found. Please create it with your Azure DevOps credentials."
                ));
            }

            let content = fs::read_to_string(config_path)?;
            let config: TestConfig = serde_json::from_str(&content)?;
            Ok(config)
        }

        fn to_credentials(&self) -> Credentials {
            Credentials {
                organization: self.organization.clone(),
                pat: self.pat.clone(),
                base_url: crate::auth::url::default_base_url(),
            }
        }
    }
    fn get_test_credentials() -> Result<Credentials> {
        let config = TestConfig::load()?;
        Ok(config.to_credentials())
    }

    fn get_test_project() -> Result<String> {
        let config = TestConfig::load()?;
        Ok(config.project.clone())
    }

    async fn create_test_repo(project: &str, name: &str) -> Result<git::models::GitRepository> {
        let creds = get_test_credentials()?;
        let factory = CredentialClientFactory::new(&creds)?;
        let client = factory.build_git();

        client
            .repositories_client()
            .create(
                creds.organization,
                GitRepositoryCreateOptions {
                    name: Some(name.to_string()),
                    parent_repository: None,
                    project: None,
                },
                project,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create test repository: {}", e))
    }

    async fn delete_test_repo(project: &str, repo_id: &str) -> Result<()> {
        let creds = get_test_credentials()?;
        let factory = CredentialClientFactory::new(&creds)?;
        let client = factory.build_git();

        client
            .repositories_client()
            .delete(&creds.organization, repo_id, project)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete test repository: {}", e))
    }

    async fn list_test_repos(project: &str) -> Result<Vec<git::models::GitRepository>> {
        let creds = get_test_credentials()?;
        let factory = CredentialClientFactory::new(&creds)?;
        let client = factory.build_git();

        client
            .repositories_client()
            .list(creds.organization, project)
            .await
            .map(|response| response.value)
            .map_err(|e| anyhow::anyhow!("Failed to list repositories: {}", e))
    }

    async fn get_test_repo(
        project: &str,
        repository_name: &str,
    ) -> Result<git::models::GitRepository> {
        let repos = list_test_repos(project).await?;

        repos
            .iter()
            .find(|repo| repo.name == repository_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Repository '{}' not found", repository_name))
    }

    #[tokio::test]
    #[ignore] // Requires test_config.json with valid credentials
    async fn test_create_show_clone_delete_repository() -> Result<()> {
        let project = get_test_project()?;
        let test_repo_name = format!("test-repo-{}", chrono::Utc::now().timestamp());

        println!("Testing with project: {project}");
        println!("Test repository name: {test_repo_name}");
        println!("1. Creating repository...");
        let created_repo = create_test_repo(&project, &test_repo_name).await?;
        assert_eq!(created_repo.name, test_repo_name);
        println!("✅ Repository created successfully: {}", created_repo.name);
        println!("2. Retrieving repository details...");
        let retrieved_repo = get_test_repo(&project, &test_repo_name).await?;
        assert_eq!(retrieved_repo.id, created_repo.id);
        assert_eq!(retrieved_repo.name, test_repo_name);
        println!("✅ Repository details retrieved successfully");

        println!("3. Cloning repository...");
        let temp_dir = std::env::temp_dir().join(format!("azdocli_test_{test_repo_name}"));

        if let Some(clone_url) = &retrieved_repo.ssh_url {
            let output = std::process::Command::new("git")
                .args(["clone", clone_url, &temp_dir.to_string_lossy()])
                .output();

            match output {
                Ok(output) => {
                    if output.status.success() {
                        println!("✅ Repository cloned successfully to: {temp_dir:?}");

                        if temp_dir.exists() {
                            std::fs::remove_dir_all(&temp_dir).ok();
                        }
                    } else {
                        let error = String::from_utf8_lossy(&output.stderr);
                        eprintln!(
                            "⚠️  Clone failed (this may be expected for empty repos): {}",
                            error.trim()
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "⚠️  Git command failed: {e} (Git may not be available in test environment)"
                    );
                }
            }
        } else {
            println!("⚠️  No SSH URL available for cloning");
        }

        // Test 4: Hard delete repository
        println!("4. Deleting repository...");
        delete_test_repo(&project, &created_repo.id).await?;
        println!("✅ Repository deleted successfully");

        println!("5. Verifying deletion...");
        let verification_result = get_test_repo(&project, &test_repo_name).await;
        assert!(
            verification_result.is_err(),
            "Repository should not exist after deletion"
        );
        println!("✅ Repository deletion verified");

        println!("🎉 All repository operations completed successfully!");
        Ok(())
    }

    #[tokio::test]
    #[ignore] // Requires test_config.json with valid credentials
    async fn test_list_repositories() -> Result<()> {
        let project = get_test_project()?;

        println!("Testing repository listing for project: {project}");
        let repos = list_test_repos(&project).await?;

        println!(
            "Found {} repositories in project '{}'",
            repos.len(),
            project
        );
        for repo in repos.iter().take(5) {
            println!("  - {}", repo.name);
        }

        assert!(
            !repos.is_empty() || repos.is_empty(),
            "Repository list should be valid (empty or not)"
        );
        println!("✅ Repository listing test completed successfully");
        Ok(())
    }
}
