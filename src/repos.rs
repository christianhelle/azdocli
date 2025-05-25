use crate::auth;
use crate::commands::SubCommands;
use anyhow::Result;
use azure_devops_rust_api::git::{self, ClientBuilder};

pub async fn handle_command(subcommand: &SubCommands) -> Result<()> {
    // Ensure user is authenticated
    let credentials = auth::get_credentials()?;
    match subcommand {
        SubCommands::Create => {
            println!("Creating a repository...");
            // Implementation would go here
        }
        SubCommands::List => {
            println!(
                "Listing all repos for organization: {}",
                credentials.organization
            );

            let repos = list_repos().await?;
            println!("{} repos found", repos.len());
        }
        SubCommands::Delete { id } => {
            println!("Deleting repo with id: {}", id);
            // Implementation would go here
        }
        SubCommands::Show { id } => {
            println!("Showing repo with id: {}", id);
            // Implementation would go here
        }
        SubCommands::Update { id } => {
            println!("Updating repo with id: {}", id);
            // Implementation would go here
        }
    }
    Ok(())
}

async fn list_repos() -> Result<Vec<git::models::GitRepository>, anyhow::Error> {
    let result = auth::get_credentials();
    let creds = result.unwrap();
    let organization = creds.organization;
    let pat = creds.pat;
    let project = "[team project name]";
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
