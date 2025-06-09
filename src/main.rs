use clap::{CommandFactory, Parser, Subcommand};
use crate::auth::{login, logout};
use crate::project::{get_default_project, save_default_project};

mod auth;
mod boards;
mod pipelines;
mod pr;
mod repos;
mod project;
mod config;

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
    /// Set or view the default project
    Project {
        /// Project name to set as default (if not provided, shows current default)
        project_name: Option<String>,
    },
    /// Manage Azure DevOps pipelines
    Pipelines {
        #[clap(subcommand)]
        subcommand: pipelines::PipelinesSubCommands,
    },
    /// Manage Azure DevOps boards
    Boards {
        #[clap(subcommand)]
        subcommand: boards::BoardsSubCommands,
    },
    /// Manage Azure DevOps repos
    Repos {
        #[clap(subcommand)]
        subcommand: repos::ReposSubCommands,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::Login) => {
            login().await?;
        }
        Some(Commands::Logout) => {
            logout()?;
        }
        Some(Commands::Project { project_name }) => match project_name {
            Some(project) => {
                save_default_project(project)?;
                println!("✅ Default project set to: {}", project);
            }
            None => match get_default_project() {
                Ok(project) => println!("Current default project: {}", project),
                Err(_) => println!("No default project configured"),
            },
        },
        Some(Commands::Pipelines { subcommand }) => {
            pipelines::handle_command(subcommand).await?;
        }
        Some(Commands::Boards { subcommand }) => {
            boards::handle_command(subcommand).await?;
        }
        Some(Commands::Repos { subcommand }) => {
            repos::handle_command(subcommand).await?;
        }
        None => {
            Cli::command().print_help()?;
        }
    }

    Ok(())
}
