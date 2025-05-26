use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Clone)]
pub enum PipelinesSubCommands {
    /// List all repositories
    List {
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
    Run {
        /// ID of the repository to update
        #[clap(short, long)]
        id: String,
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
}

pub async fn handle_pipelines_command(subcommand: &PipelinesSubCommands) -> Result<()> {
    crate::pipelines::handle_command(subcommand).await
}

pub async fn handle_command(subcommand: &PipelinesSubCommands) -> Result<()> {
    match subcommand {
        PipelinesSubCommands::List { project } => {
            // Call the function to list pipelines
        }
        PipelinesSubCommands::Show { id, project } => {
            // Call the function to show pipeline details
        }
        PipelinesSubCommands::Run { id, project } => {
            // Call the function to run a pipeline
        }
    }

    Ok(())
}
