use clap::Subcommand;
use anyhow::Result;

#[derive(Subcommand, Clone)]
pub enum SubCommands {
    /// Create a new resource
    Create,
    /// List all resources
    List,
    /// Delete a resource
    Delete {
        /// ID of the resource to delete
        #[clap(short, long)]
        id: String,
    },
    /// Show details of a resource
    Show {
        /// ID of the resource to show
        #[clap(short, long)]
        id: String,
    },
    /// Update a resource
    Update {
        /// ID of the resource to update
        #[clap(short, long)]
        id: String,
    },
}

#[derive(Subcommand, Clone)]
pub enum ReposSubCommands {
    /// Create a new repository
    Create {
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// List all repositories
    List {
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
    /// Delete a repository
    Delete {
        /// ID of the repository to delete
        #[clap(short, long)]
        id: String,
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
    Update {
        /// ID of the repository to update
        #[clap(short, long)]
        id: String,
        /// Team project name
        #[clap(short, long)]
        project: String,
    },
}

pub async fn handle_pipelines_command(subcommand: &SubCommands) -> Result<()> {
    crate::pipelines::handle_command(subcommand).await
}

pub async fn handle_boards_command(subcommand: &SubCommands) -> Result<()> {
    crate::boards::handle_command(subcommand).await
}

pub async fn handle_repos_command(subcommand: &ReposSubCommands) -> Result<()> {
    crate::repos::handle_command(subcommand).await
}

pub async fn handle_artifacts_command(subcommand: &SubCommands) -> Result<()> {
    crate::artifacts::handle_command(subcommand).await
}
