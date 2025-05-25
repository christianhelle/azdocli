use clap::{CommandFactory, Parser, Subcommand};
mod artifacts;
mod auth;
mod boards;
mod commands;
mod pipelines;
mod repos;
mod utils;

#[derive(Parser)]
#[clap(about, version)]
struct Cli {
    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Login to Azure DevOps with a Personal Access Token (PAT)
    Login,
    /// Logout from Azure DevOps
    Logout,
    /// Manage Azure DevOps pipelines
    Pipelines {
        #[clap(subcommand)]
        subcommand: commands::SubCommands,
    },
    /// Manage Azure DevOps boards
    Boards {
        #[clap(subcommand)]
        subcommand: commands::SubCommands,
    },
    /// Manage Azure DevOps repos
    Repos {
        #[clap(subcommand)]
        subcommand: commands::SubCommands,
    },
    /// Manage Azure DevOps artifacts
    Artifacts {
        #[clap(subcommand)]
        subcommand: commands::SubCommands,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Login) => {
            auth::login().await?;
        }
        Some(Commands::Logout) => {
            auth::logout()?;
        }
        Some(Commands::Pipelines { subcommand }) => {
            commands::handle_pipelines_command(subcommand).await?;
        }
        Some(Commands::Boards { subcommand }) => {
            commands::handle_boards_command(subcommand).await?;
        }
        Some(Commands::Repos { subcommand }) => {
            commands::handle_repos_command(subcommand).await?;
        }
        Some(Commands::Artifacts { subcommand }) => {
            commands::handle_artifacts_command(subcommand).await?;
        }
        None => {
            // When no command is provided, print the help information like --help would do
            Cli::command().print_help()?;
        }
    }

    Ok(())
}
