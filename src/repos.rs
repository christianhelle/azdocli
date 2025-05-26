use crate::auth;
use anyhow::Result;
use azure_devops_rust_api::git::{self, ClientBuilder};
use clap::Subcommand;
use dialoguer::Confirm;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Subcommand, Clone)]
pub enum ReposSubCommands {
    /// Create a new repository
    Create {
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// List all repositories
    List {
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// Clone all repositories from a project
    Clone {
        /// Team project name
        #[clap(short, long)]
        project: String,
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
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// Show details of a repository
    Show {
        /// ID of the repository to show
        #[clap(short, long)]
        id: String,
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// Update a repository
    Update {
        /// ID of the repository to update
        #[clap(short, long)]
        id: String,
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
}

pub async fn handle_command(subcommand: &ReposSubCommands) -> Result<()> {
    // Ensure user is authenticated
    match subcommand {
        ReposSubCommands::Create { project } => {
            println!("Creating a repository in project: {}", project);
            // Implementation would go here
        }
        ReposSubCommands::List { project } => {
            let repos = list_repos(project).await?;
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
            clone_all_repos(
                project,
                target_dir.as_deref(),
                *yes,
                *parallel,
                *concurrency,
            )
            .await?;
        }
        ReposSubCommands::Delete { id, project } => {
            println!("Deleting repo with id: {} in project: {}", id, project);
            // Implementation would go here
        }        ReposSubCommands::Show { id, project } => {
            match get_repo(project, id).await {
                Ok(repo) => {
                    display_repo_details(&repo);
                }
                Err(e) => {
                    eprintln!("❌ Failed to retrieve repository '{}' from project '{}'", id, project);
                    eprintln!("   {}", e);
                    return Err(e);
                }
            }
        }
        ReposSubCommands::Update { id, project } => {
            println!("Updating repo with id: {} in project: {}", id, project);
            // Implementation would go here
        }
    }
    Ok(())
}

/// Retrieves a list of Git repositories from a specified Azure DevOps project
async fn list_repos(project: &str) -> Result<Vec<git::models::GitRepository>, anyhow::Error> {
    match auth::get_credentials() {
        Ok(creds) => {
            let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
            let client = ClientBuilder::new(credential).build();
            Ok(client
                .repositories_client()
                .list(creds.organization, project)
                .await?
                .value)
        }
        Err(e) => {
            println!("Unable to retrieve repositories");
            Err(e)
        }
    }
}

/// Retrieves a single Git repository by ID from a specified Azure DevOps project
///
/// # Arguments
/// * `project` - The name of the Azure DevOps project
/// * `repository_id` - The ID of the repository to retrieve
///
/// # Returns
/// * `Result<git::models::GitRepository>` - The repository details or error
async fn get_repo(project: &str, repository_id: &str) -> Result<git::models::GitRepository> {
    let repos = match list_repos(project).await {
        Ok(repos) => repos,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to list repositories in project '{}': {}", project, e));
        }
    };
    
    // Find the repository by name (since ID type is unclear, we'll match by name which is always a String)
    let repo = repos.iter().find(|repo| repo.name == repository_id);

    match repo {
        Some(repo) => Ok(repo.clone()),
        None => {
            let available_repos: Vec<&str> = repos.iter().map(|r| r.name.as_str()).collect();
            if available_repos.is_empty() {
                Err(anyhow::anyhow!("No repositories found in project '{}'", project))
            } else {
                Err(anyhow::anyhow!("Repository '{}' not found in project '{}'. Available repositories: {}", 
                    repository_id, project, available_repos.join(", ")))
            }
        }
    }
}

/// Displays detailed information about a repository
///
/// # Arguments
/// * `repo` - The GitRepository object to display
fn display_repo_details(repo: &git::models::GitRepository) {
    println!("📋 Repository Details");
    println!("=====================");
    println!("📁 Name: {}", repo.name);
    println!("🆔 ID: {}", repo.id);
    
    if let Some(url) = &repo.web_url {
        println!("🌐 Web URL: {}", url);
    }
    
    if let Some(remote_url) = &repo.remote_url {
        println!("🔗 Remote URL (HTTPS): {}", remote_url);
    }
    
    if let Some(ssh_url) = &repo.ssh_url {
        println!("🔑 Clone URL (SSH): {}", ssh_url);
    }
    
    if let Some(size) = &repo.size {
        let size_kb = *size as f64 / 1024.0;
        let size_mb = size_kb / 1024.0;
        if size_mb >= 1.0 {
            println!("📦 Size: {:.2} MB ({} bytes)", size_mb, size);
        } else {
            println!("📦 Size: {:.2} KB ({} bytes)", size_kb, size);
        }
    }
    
    if let Some(default_branch) = &repo.default_branch {
        println!("🌿 Default Branch: {}", default_branch);
    }
    
    println!("🎯 Project: {}", repo.project.name);
    
    if let Some(is_fork) = repo.is_fork {
        println!("🍴 Is Fork: {}", if is_fork { "Yes" } else { "No" });
    }
    
    if let Some(is_disabled) = repo.is_disabled {
        println!("⚠️  Is Disabled: {}", if is_disabled { "Yes" } else { "No" });
    }
}

/// Clones all repositories from a specified Azure DevOps project to a target directory
///
/// # Arguments
/// * `project` - The name of the Azure DevOps project
/// * `target_dir` - Optional target directory (defaults to current directory)
/// * `skip_confirmation` - Whether to skip the confirmation prompt
/// * `parallel` - Whether to clone repositories in parallel
/// * `concurrency` - Number of concurrent operations (only used if parallel is true)
///
/// # Returns
/// * `Result<()>` - Success or error result
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
        println!("No repositories found in project '{}'", project);
        return Ok(());
    }

    println!(
        "Found {} repositories in project '{}'",
        repos.len(),
        project
    );
    println!("Target directory: {}", target_directory);
    println!("\nRepositories to clone:");
    for repo in repos.iter() {
        if let Some(ssh_url) = &repo.ssh_url {
            println!("  • {} ({})", repo.name, ssh_url);
        } else {
            println!("  • {} (⚠ No SSH URL)", repo.name);
        }
    }

    // Ask for confirmation unless skipped
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

    // Create target directory if it doesn't exist
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
        // Validate concurrency level
        let concurrency_level = if concurrency > 8 {
            println!(
                "Warning: Concurrency level {} exceeds maximum of 8. Using 8 instead.",
                concurrency
            );
            8
        } else if concurrency < 1 {
            println!(
                "Warning: Concurrency level {} is invalid. Using 1 instead.",
                concurrency
            );
            1
        } else {
            concurrency
        };

        // Parallel cloning implementation
        println!(
            "Starting {} repositories in parallel (max {} concurrent)...",
            repos.len(),
            concurrency_level
        );
        let results = clone_repos_parallel(&repos, target_directory, concurrency_level).await;
        for result in results {
            match result {
                Ok(repo_name) => {
                    println!("✓ Successfully cloned {}", repo_name);
                    success_count += 1;
                }
                Err(e) => {
                    println!("✗ {}", e);
                    failed_count += 1;
                }
            }
        }
    } else {
        // Sequential cloning implementation (existing logic)
        for repo in repos.iter() {
            if let Some(ssh_url) = &repo.ssh_url {
                println!("Cloning repository: {} from {}", repo.name, ssh_url);

                let target_path = if target_directory == "." {
                    repo.name.clone()
                } else {
                    format!("{}/{}", target_directory, repo.name)
                };

                let output = std::process::Command::new("git")
                    .args(&["clone", ssh_url, &target_path])
                    .output();

                match output {
                    Ok(output) => {
                        if output.status.success() {
                            println!("✓ Successfully cloned {}", repo.name);
                            success_count += 1;
                        } else {
                            let error = String::from_utf8_lossy(&output.stderr);
                            println!("✗ Failed to clone {}: {}", repo.name, error.trim());
                            failed_count += 1;
                        }
                    }
                    Err(e) => {
                        println!("✗ Failed to execute git command for {}: {}", repo.name, e);
                        if e.kind() == std::io::ErrorKind::NotFound {
                            println!("  Git command not found. Please ensure Git is installed and in your PATH.");
                            return Err(anyhow::anyhow!("Git command not found"));
                        }
                        failed_count += 1;
                    }
                }
            } else {
                println!("⚠ Skipping {} (No SSH URL available)", repo.name);
                failed_count += 1;
            }
        }
    }

    println!("\nCloning completed:");
    println!("  ✓ Successfully cloned: {}", success_count);
    if failed_count > 0 {
        println!("  ✗ Failed/Skipped: {}", failed_count);
    }

    Ok(())
}

/// Clones repositories in parallel using tokio tasks
///
/// # Arguments
/// * `repos` - Vector of GitRepository objects to clone
/// * `target_directory` - Target directory for cloning
/// * `concurrency` - Number of concurrent clone operations
///
/// # Returns
/// * `Vec<Result<String, String>>` - Results for each clone operation
async fn clone_repos_parallel(
    repos: &[git::models::GitRepository],
    target_directory: &str,
    concurrency: usize,
) -> Vec<Result<String, String>> {
    let semaphore = Arc::new(Semaphore::new(concurrency)); // Use provided concurrency level
    let mut tasks = Vec::new();

    for repo in repos {
        if let Some(ssh_url) = &repo.ssh_url {
            let repo_name = repo.name.clone();
            let ssh_url = ssh_url.clone();
            let target_path = if target_directory == "." {
                repo_name.clone()
            } else {
                format!("{}/{}", target_directory, repo_name)
            };
            let semaphore = semaphore.clone();

            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                println!("Cloning repository: {} from {}", repo_name, ssh_url);

                let output = tokio::process::Command::new("git")
                    .args(&["clone", &ssh_url, &target_path])
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
                            Err(format!("Git command not found while cloning {}", repo_name))
                        } else {
                            Err(format!(
                                "Failed to execute git command for {}: {}",
                                repo_name, e
                            ))
                        }
                    }
                }
            });

            tasks.push(task);
        } else {
            // For repos without SSH URLs, create a task that immediately returns an error
            let repo_name = repo.name.clone();
            let task = tokio::spawn(async move {
                Err(format!("Skipping {} (No SSH URL available)", repo_name))
            });
            tasks.push(task);
        }
    }

    // Wait for all tasks to complete
    let mut results = Vec::new();
    for task in tasks {
        match task.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(format!("Task failed: {}", e))),
        }
    }

    results
}
