use crate::auth;
use crate::commands::SubCommands;
use anyhow::Result;

pub async fn handle_command(subcommand: &SubCommands) -> Result<()> {
    // Ensure user is authenticated
    let credentials = auth::get_credentials()?;
    match subcommand {
        SubCommands::Create => {
            println!("Creating an artifact...");
            // Implementation would go here
        }
        SubCommands::List => {
            println!(
                "Listing all artifacts for organization: {}",
                credentials.organization
            );
            // Implementation would go here
        }
        SubCommands::Delete { id } => {
            println!("Deleting artifact with id: {}", id);
            // Implementation would go here
        }
        SubCommands::Show { id } => {
            println!("Showing artifact with id: {}", id);
            // Implementation would go here
        }
        SubCommands::Update { id } => {
            println!("Updating artifact with id: {}", id);
            // Implementation would go here
        }
    }
    Ok(())
}
