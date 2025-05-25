use crate::auth;
use crate::commands::ReposSubCommands;
use anyhow::Result;
use azure_devops_rust_api::git::{self, ClientBuilder};

pub async fn handle_command(subcommand: &ReposSubCommands) -> Result<()> {
    // Ensure user is authenticated
    let credentials = auth::get_credentials()?;    match subcommand {
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
            println!("{} repos found", repos.len());
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
    let result = auth::get_credentials();
    let creds = result.unwrap();
    let organization = creds.organization;
    let pat = creds.pat;
    let credential = azure_devops_rust_api::Credential::Pat(pat);
    let repos = ClientBuilder::new(credential)
        .build()
        .repositories_client()
        .list(organization, project)
        .await?
        .value;
    for repo in repos.iter() {
        println!("{}", repo.name);
    }
    Ok(repos)
}
