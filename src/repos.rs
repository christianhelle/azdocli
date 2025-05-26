use crate::auth;
use anyhow::Result;
use azure_devops_rust_api::git::{self, ClientBuilder};
use clap::Subcommand;
use std::process::Command;

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
        ReposSubCommands::Clone { project, target_dir } => {
            clone_all_repos(project, target_dir.as_deref()).await?;
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

async fn clone_all_repos(project: &str, target_dir: Option<&str>) -> Result<()> {
    let repos = list_repos(project).await?;
    let target_directory = target_dir.unwrap_or(".");
    
    println!("Found {} repositories in project '{}'", repos.len(), project);
    println!("Cloning repositories to directory: {}", target_directory);
    
    for repo in repos.iter() {
        if let Some(ssh_url) = &repo.ssh_url {
            println!("Cloning repository: {}", repo.name);
            let output = Command::new("git")
                .args(&["clone", ssh_url, &format!("{}/{}", target_directory, repo.name)])
                .output()?;
                
            if output.status.success() {
                println!("✓ Successfully cloned {}", repo.name);
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                println!("✗ Failed to clone {}: {}", repo.name, error);
            }
        } else {
            println!("⚠ No SSH URL available for repository: {}", repo.name);
        }
    }
    
    Ok(())
}

async fn clone_all_repos(
    project: &str,
    target_dir: Option<&str>,
) -> Result<(), anyhow::Error> {
    let repos = list_repos(project).await?;
    for repo in repos {
        let repo_name = &repo.name;
        let repo_url = &repo.remote_url;
        let target = match target_dir {
            Some(dir) => format!("{}/{}", dir, repo_name),
            None => repo_name.clone(),
        };
        println!("Cloning {} into {}", repo_url, target);
        // Here you would add the actual git clone command, e.g., using std::process::Command
    }
    Ok(())
}
