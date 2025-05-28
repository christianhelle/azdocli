use clap::{CommandFactory, Parser, Subcommand};
mod artifacts;
mod auth;
mod boards;
mod commands;
mod pipelines;
mod repos;

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
        subcommand: commands::SubCommands,
    },
    /// Manage Azure DevOps repos
    Repos {
        #[clap(subcommand)]
        subcommand: repos::ReposSubCommands,
    },
    /// Manage Azure DevOps artifacts
    Artifacts {
        #[clap(subcommand)]
        subcommand: commands::SubCommands,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();    match &cli.command {
        Some(Commands::Login) => {
            auth::login().await?;
        }
        Some(Commands::Logout) => {
            auth::logout()?;
        }
        Some(Commands::Project { project_name }) => {
            match project_name {
                Some(project) => {
                    auth::save_default_project(project)?;
                    println!("✓ Default project set to: {}", project);
                }
                None => {
                    match auth::get_default_project() {
                        Ok(project) => println!("Current default project: {}", project),
                        Err(_) => println!("No default project configured"),
                    }
                }
            }
        }
        Some(Commands::Pipelines { subcommand }) => {
            pipelines::handle_command(subcommand).await?;
        }
        Some(Commands::Boards { subcommand }) => {
            commands::handle_boards_command(subcommand).await?;
        }
        Some(Commands::Repos { subcommand }) => {
            repos::handle_command(subcommand).await?;
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
