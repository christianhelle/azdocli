use crate::auth;
use anyhow::Result;
use azure_devops_rust_api::git::{self, ClientBuilder};
use clap::Subcommand;
use dialoguer::Confirm;

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
    },    /// Clone all repositories from a project
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
        }        ReposSubCommands::Clone { project, target_dir, yes } => {
            clone_all_repos(project, target_dir.as_deref(), *yes).await?;
        }
        ReposSubCommands::Delete { id, project } => {
            println!("Deleting repo with id: {} in project: {}", id, project);
            // Implementation would go here
        }
        ReposSubCommands::Show { id, project } => {
            println!("Showing repo with id: {} in project: {}", id, project);
            // Implementation would go here
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
            Ok(ClientBuilder::new(credential)
                .build()
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

/// Clones all repositories from a specified Azure DevOps project to a target directory
/// 
/// # Arguments
/// * `project` - The name of the Azure DevOps project
/// * `target_dir` - Optional target directory (defaults to current directory)
/// * `skip_confirmation` - Whether to skip the confirmation prompt
/// 
/// # Returns
/// * `Result<()>` - Success or error result
async fn clone_all_repos(project: &str, target_dir: Option<&str>, skip_confirmation: bool) -> Result<()> {
    let repos = list_repos(project).await?;
    let target_directory = target_dir.unwrap_or(".");
    
    if repos.is_empty() {
        println!("No repositories found in project '{}'", project);
        return Ok(());
    }
    
    println!("Found {} repositories in project '{}'", repos.len(), project);
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
    
    println!("\nStarting clone operations...");
    
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
    
    println!("\nCloning completed:");
    println!("  ✓ Successfully cloned: {}", success_count);
    if failed_count > 0 {
        println!("  ✗ Failed/Skipped: {}", failed_count);
    }
    
    Ok(())
}
