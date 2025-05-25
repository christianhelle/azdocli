use anyhow::Result;
use crate::auth;
use crate::commands::SubCommands;

pub async fn handle_command(subcommand: &SubCommands) -> Result<()> {
    // Ensure user is authenticated
    let credentials = auth::get_credentials()?;    match subcommand {
        SubCommands::Create => {
            println!("Creating a pipeline...");
            // Implementation would go here
        }
        SubCommands::List => {
            println!("Listing all pipelines for organization: {}", credentials.organization);
            // Implementation would go here
        }
        SubCommands::Delete { id } => {
            println!("Deleting pipeline with id: {}", id);
            // Implementation would go here
        }
        SubCommands::Show { id } => {
            println!("Showing pipeline with id: {}", id);
            // Implementation would go here
        }
        SubCommands::Update { id } => {
            println!("Updating pipeline with id: {}", id);
            // Implementation would go here
        }
    }
    Ok(())
}
