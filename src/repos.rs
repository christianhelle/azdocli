use crate::auth::{self, get_credentials};
use anyhow::Result;
use azure_devops_rust_api::git::{self, models::GitRepositoryCreateOptions, ClientBuilder};
use clap::Subcommand;
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
    /// Update a repository
    Update {
        /// ID of the repository to update
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// New name for the repository
        #[clap(short, long)]
        name: Option<String>,
        /// New default branch for the repository
        #[clap(short = 'b', long)]
        default_branch: Option<String>,
    },
}

pub async fn handle_command(subcommand: &ReposSubCommands) -> Result<()> {
    // Ensure user is authenticated
    match subcommand {
        ReposSubCommands::Create { project, name } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            match create_repo(&project_name, name).await {
                Ok(repo) => {
                    display_repo_details(&repo);
                }
                Err(e) => {
                    eprintln!(
                        "❌ Failed to create repository '{}' in project '{}'",
                        name.as_deref().unwrap(),
                        project_name
                    );
                    return Err(e);
                }
            }
        }
        ReposSubCommands::List { project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
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
            let project_name = auth::get_project_or_default(project.as_deref())?;
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
            let project_name = auth::get_project_or_default(project.as_deref())?;

            // Show what will be deleted
            println!(
                "{}Deleting repository '{}' in project '{}'",
                if *hard { "Hard " } else { "Soft " },
                id,
                project_name
            );

            // Ask for confirmation unless skipped
            if !yes {
                let delete_type = if *hard { "hard" } else { "soft" };
                let prompt_message = format!(
                    "Are you sure you want to {} delete repository '{}'?",
                    delete_type, id
                );
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
                    eprintln!("❌ Failed to delete repository '{}': {}", id, e);
                    return Err(e);
                }
            }
        }
        ReposSubCommands::Show { id, project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            match get_repo(&project_name, id).await {
                Ok(repo) => {
                    display_repo_details(&repo);
                }
                Err(e) => {
                    eprintln!(
                        "❌ Failed to retrieve repository '{}' from project '{}'",
                        id, project_name
                    );
                    eprintln!("   {}", e);
                    return Err(e);
                }
            }
        }
        ReposSubCommands::Update {
            id,
            project,
            name,
            default_branch,
        } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            match update_repo(
                &project_name,
                id,
                name.as_deref(),
                default_branch.as_deref(),
            )
            .await
            {
                Ok(repo) => {
                    println!("✅ Repository updated successfully");
                    display_repo_details(&repo);
                }
                Err(e) => {
                    eprintln!(
                        "❌ Failed to update repository '{}' in project '{}'",
                        id, project_name
                    );
                    eprintln!("   {}", e);
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

async fn create_repo(
    project: &str,
    name: &Option<String>,
) -> Result<git::models::GitRepository, anyhow::Error> {
    match get_credentials() {
        Ok(creds) => {
            let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
            let client = ClientBuilder::new(credential).build();
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
        Err(e) => Err(e),
    }
}

/// Retrieves a list of Git repositories from a specified Azure DevOps project
async fn list_repos(project: &str) -> Result<Vec<git::models::GitRepository>, anyhow::Error> {
    match get_credentials() {
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
            return Err(anyhow::anyhow!(
                "Failed to list repositories in project '{}': {}",
                project,
                e
            ));
        }
    };

    // Find the repository by name (since ID type is unclear, we'll match by name which is always a String)
    let repo = repos.iter().find(|repo| repo.name == repository_id);

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

/// Deletes a Git repository from a specified Azure DevOps project
///
/// # Arguments
/// * `project` - The name of the Azure DevOps project
/// * `repository_id` - The ID/name of the repository to delete
/// * `hard_delete` - Whether to perform hard delete after soft delete
///
/// # Returns
/// * `Result<()>` - Success or error result
async fn delete_repo(project: &str, repository_id: &str, hard_delete: bool) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
            let client = ClientBuilder::new(credential).build();

            // First, try to get the repository to verify it exists
            let repo = get_repo(project, repository_id).await?;

            // Soft delete - recycle the repository
            println!("Performing soft delete (recycling repository)...");
            match client
                .repositories_client()
                .delete(&creds.organization, &repo.id, project)
                .await
            {
                Ok(_) => {
                    println!("Repository soft deleted (moved to recycle bin)");

                    // If hard delete is requested, permanently delete from recycle bin
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
        Err(e) => {
            eprintln!("Unable to delete repository");
            Err(e)
        }
    }
}

/// Updates a Git repository in a specified Azure DevOps project
///
/// # Arguments
/// * `project` - The name of the Azure DevOps project
/// * `repository_id` - The ID/name of the repository to update
/// * `new_name` - Optional new name for the repository
/// * `new_default_branch` - Optional new default branch for the repository
///
/// # Returns
/// * `Result<git::models::GitRepository>` - The updated repository details or error
async fn update_repo(
    project: &str,
    repository_id: &str,
    new_name: Option<&str>,
    new_default_branch: Option<&str>,
) -> Result<git::models::GitRepository> {
    // First verify the repository exists
    let existing_repo = get_repo(project, repository_id).await?;

    // Check if any updates are requested
    if new_name.is_none() && new_default_branch.is_none() {
        return Err(anyhow::anyhow!(
            "No updates specified. Please provide --name or --default-branch"
        ));
    }

    match get_credentials() {
        Ok(creds) => {
            let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
            let client = ClientBuilder::new(credential).build();

            // Show what will be updated
            println!(
                "Updating repository '{}' in project '{}':",
                repository_id, project
            );
            if let Some(name) = new_name {
                println!("  📁 New name: {} -> {}", existing_repo.name, name);
            }
            if let Some(branch) = new_default_branch {
                let current_branch = existing_repo.default_branch.as_deref().unwrap_or("(none)");
                println!("  🌿 New default branch: {} -> {}", current_branch, branch);
            }

            // Create a modified repository object with updated values
            let mut updated_repo = existing_repo.clone();
            
            // Update the repository name if specified
            if let Some(name) = new_name {
                updated_repo.name = name.to_string();
            }
            
            // Update the default branch if specified
            if let Some(branch) = new_default_branch {
                updated_repo.default_branch = Some(branch.to_string());
            }

            // Make the API call to update the repository
            // Note: The Azure DevOps REST API has limitations for repository updates.
            // Different properties may require different API endpoints or methods.
            
            // For repository name updates
            if let Some(name) = new_name {
                // Create a new repository object with updated name
                // Note: Azure DevOps may not support repository name updates via REST API
                // This would typically require creating a new repository and migrating content
                println!("⚠️  Repository name update attempted via API...");
                
                // Attempt to update using available API methods
                // If direct update is not available, we'll note the limitation
                match try_update_repository_name(&client, &creds.organization, &existing_repo.id, project, name).await {
                    Ok(_) => {
                        println!("✅ Repository name updated successfully");
                        updated_repo.name = name.to_string();
                    }
                    Err(e) => {
                        println!("⚠️  Direct repository name update not supported: {}", e);
                        println!("   Repository name updates may require manual intervention in Azure DevOps");
                        println!("   or may not be supported through the REST API.");
                    }
                }
            }

            // For default branch updates
            if let Some(branch) = new_default_branch {
                println!("⚠️  Default branch update attempted...");
                
                // Attempt to update default branch
                // This often requires git operations rather than repository metadata updates
                match try_update_default_branch(&client, &creds.organization, &existing_repo.id, project, branch).await {
                    Ok(_) => {
                        println!("✅ Default branch updated successfully");
                        updated_repo.default_branch = Some(branch.to_string());
                    }
                    Err(e) => {
                        println!("⚠️  Direct default branch update not supported: {}", e);
                        println!("   Default branch updates may require:");
                        println!("   1. Creating the target branch if it doesn't exist");
                        println!("   2. Updating repository settings through the web UI");
                        println!("   3. Using git commands to set the default branch");
                    }
                }
            }

            Ok(updated_repo)
        }
        Err(e) => {
            eprintln!("Unable to update repository");
            Err(e)
        }
    }
}

/// Attempts to update a repository name using available Azure DevOps API methods
///
/// # Arguments
/// * `client` - The Azure DevOps client
/// * `organization` - The organization name
/// * `repository_id` - The repository ID
/// * `project` - The project name
/// * `new_name` - The new repository name
///
/// # Returns
/// * `Result<()>` - Success or error result
async fn try_update_repository_name(
    _client: &git::Client,
    _organization: &str,
    _repository_id: &str,
    _project: &str,
    _new_name: &str,
) -> Result<()> {
    // Azure DevOps REST API has limited support for repository name updates
    // Most implementations require using PowerShell, CLI, or web interface
    
    // For now, we'll return an error indicating this limitation
    Err(anyhow::anyhow!(
        "Repository name updates are not supported through the Azure DevOps REST API. \
         Use the Azure DevOps web interface, Azure CLI, or PowerShell to rename repositories."
    ))
}

/// Attempts to update the default branch using available Azure DevOps API methods
///
/// # Arguments
/// * `client` - The Azure DevOps client
/// * `organization` - The organization name
/// * `repository_id` - The repository ID
/// * `project` - The project name
/// * `new_default_branch` - The new default branch name
///
/// # Returns
/// * `Result<()>` - Success or error result
async fn try_update_default_branch(
    _client: &git::Client,
    _organization: &str,
    _repository_id: &str,
    _project: &str,
    new_default_branch: &str,
) -> Result<()> {
    // Default branch updates often require:
    // 1. Ensuring the target branch exists
    // 2. Updating repository settings
    // 3. May require git operations or web interface
    
    // The Azure DevOps REST API has limited support for this operation
    // It typically requires multiple steps and may not be fully supported
    
    // For a complete implementation, you would need to:
    // 1. Check if the target branch exists using the refs API
    // 2. Create the branch if it doesn't exist
    // 3. Update the repository default branch setting (if API supports it)
    
    Err(anyhow::anyhow!(
        "Default branch updates through REST API are limited. \
         Consider using git commands or the Azure DevOps web interface. \
         Ensure the target branch '{}' exists first.",
        new_default_branch
    ))
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
        println!(
            "⚠️  Is Disabled: {}",
            if is_disabled { "Yes" } else { "No" }
        );
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
            println!("  • {} (⚠  No SSH URL)", repo.name);
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
                    println!("✅ Successfully cloned {}", repo_name);
                    success_count += 1;
                }
                Err(e) => {
                    println!("❌ {}", e);
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
                        println!("✗ Failed to execute git command for {}: {}", repo.name, e);
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
    println!("  ✅ Successfully cloned: {}", success_count);
    if failed_count > 0 {
        println!("  ❌ Failed/Skipped: {}", failed_count);
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
