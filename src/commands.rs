use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Clone)]
pub enum SubCommands {
    /// Create a new resource
    Create {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// List all resources
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Delete a resource
    Delete {
        /// ID of the resource to delete
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Show details of a resource
    Show {
        /// ID of the resource to show
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Update a resource
    Update {
        /// ID of the resource to update
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
}

pub async fn handle_boards_command(subcommand: &SubCommands) -> Result<()> {
    crate::boards::handle_command(subcommand).await
}

pub async fn handle_artifacts_command(subcommand: &SubCommands) -> Result<()> {
    crate::artifacts::handle_command(subcommand).await
}
