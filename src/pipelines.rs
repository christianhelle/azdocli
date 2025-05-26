use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Clone)]
pub enum PipelinesSubCommands {
    /// List all pipelines in a project
    List {
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// Show builds of a pipeline
    Runs {
        /// ID of the pipeline to show runs for
        #[clap(short, long)]
        id: String,
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// Show details of a pipeline
    Show {
        /// ID of the pipeline to show
        #[clap(short, long)]
        id: String,
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// Run a pipeline
    Run {
        /// ID of the pipeline to start
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
        PipelinesSubCommands::Runs { id, project } => {
            // Call the function to show pipeline details
        }
        PipelinesSubCommands::Run { id, project } => {
            // Call the function to run a pipeline
        }
        PipelinesSubCommands::Show { id, project } => {
            // Call the function to run a pipeline
        }
    }

    Ok(())
}
