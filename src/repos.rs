use crate::auth;
use crate::commands::ReposSubCommands;
use anyhow::Result;
use azure_devops_rust_api::git::{self, ClientBuilder};

pub async fn handle_command(subcommand: &ReposSubCommands) -> Result<()> {
    // Ensure user is authenticated
    let credentials = auth::get_credentials()?;
    match subcommand {
        ReposSubCommands::Create { project } => {
            println!("Creating a repository in project: {}", project);
            // Implementation would go here
        }
        ReposSubCommands::List { project } => {
            println!(
                "Listing all repos for organization: {}, project: {}",
                credentials.organization, project
            );
            let repos = list_repos(project).await?;
            for repo in repos.iter() {
                println!("{}", repo.name);
            }
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
