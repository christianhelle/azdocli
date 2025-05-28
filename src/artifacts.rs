use crate::auth;
use crate::commands::SubCommands;
use anyhow::Result;

pub async fn handle_command(subcommand: &SubCommands) -> Result<()> {
    // Ensure user is authenticated
    let credentials = auth::get_credentials()?;
    match subcommand {
        SubCommands::Create { project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!("Creating an artifact in project: {}", project_name);
            // Implementation would go here
        }
        SubCommands::List { project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!(
                "Listing all artifacts for organization: {} in project: {}",
                credentials.organization, project_name
            );
            // Implementation would go here
        }
        SubCommands::Delete { id, project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!(
                "Deleting artifact with id: {} in project: {}",
                id, project_name
            );
            // Implementation would go here
        }
        SubCommands::Show { id, project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!(
                "Showing artifact with id: {} in project: {}",
                id, project_name
            );
            // Implementation would go here
        }
        SubCommands::Update { id, project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!(
                "Updating artifact with id: {} in project: {}",
                id, project_name
            );
            // Implementation would go here
        }
    }
    Ok(())
}
