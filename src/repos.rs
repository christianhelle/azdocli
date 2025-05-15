use anyhow::Result;
use crate::auth;
use crate::commands::SubCommands;

pub async fn handle_command(subcommand: &SubCommands) -> Result<()> {
    // Ensure user is authenticated
    let credentials = auth::get_credentials()?;
    match subcommand {
        SubCommands::Create => {
            println!("Creating a repository...");
            // Implementation would go here
        }
        SubCommands::List => {
            println!("Listing all repos for organization: {}", credentials.organization);
            // Implementation would go here
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
