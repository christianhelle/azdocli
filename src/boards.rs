use crate::auth;
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Clone)]
pub enum BoardsSubCommands {
    /// Manage work items
    WorkItem {
        #[clap(subcommand)]
        subcommand: WorkItemSubCommands,
    },
}

#[derive(Subcommand, Clone)]
pub enum WorkItemSubCommands {
    /// Create a new work item
    Create {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Delete a work item
    Delete {
        /// ID of the work item to delete
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Show details of a work item
    Show {
        /// ID of the work item to show
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Update a work item
    Update {
        /// ID of the work item to update
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
}

pub async fn handle_command(subcommand: &BoardsSubCommands) -> Result<()> {
    // Ensure user is authenticated
    let _credentials = auth::get_credentials()?;
    match subcommand {
        BoardsSubCommands::WorkItem { subcommand } => handle_work_item_command(subcommand).await,
    }
}

async fn handle_work_item_command(subcommand: &WorkItemSubCommands) -> Result<()> {
    match subcommand {
        WorkItemSubCommands::Create { project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!("Creating a work item in project: {}", project_name);
            // Implementation would go here
        }
        WorkItemSubCommands::Delete { id, project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!(
                "Deleting work item with id: {} in project: {}",
                id, project_name
            );
            // Implementation would go here
        }
        WorkItemSubCommands::Show { id, project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!(
                "Showing work item with id: {} in project: {}",
                id, project_name
            );
            // Implementation would go here
        }
        WorkItemSubCommands::Update { id, project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!(
                "Updating work item with id: {} in project: {}",
                id, project_name
            );
            // Implementation would go here
        }
    }
    Ok(())
}
