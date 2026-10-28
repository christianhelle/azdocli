use crate::auth::{login, logout};
use crate::output::OutputFormat;
use crate::project::{get_default_project, save_default_project};
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};

mod artifacts;
mod auth;
mod boards;
mod browser;
mod config;
mod migrate;
mod output;
mod pipelines;
mod pr;
mod project;
mod projects;
mod releases;
mod repos;
mod text;
mod user;
mod wiki;

#[derive(Parser)]
#[clap(about, version)]
struct Cli {
    /// Output format
    #[clap(short, long, global = true, value_enum, default_value_t)]
    output: OutputFormat,

    #[clap(subcommand)]
    command: Option<Commands>,
}

/// Commands that print through a `Render` model and so support every format.
const STRUCTURED_OUTPUT_COMMANDS: &[&str] = &[
    "repos list",
    "repos show",
    "pipelines list",
    "pipelines show",
];

/// The subcommand names the user typed, e.g. `repos pr list`.
fn command_path(matches: &ArgMatches) -> String {
    let mut names = Vec::new();
    let mut current = matches;
    while let Some((name, sub)) = current.subcommand() {
        names.push(name);
        current = sub;
    }
    names.join(" ")
}

/// Fails early when `format` isn't text and `command` has no renderer yet.
fn check_output_support(format: OutputFormat, command: &str) -> anyhow::Result<()> {
    if STRUCTURED_OUTPUT_COMMANDS.contains(&command) {
        return Ok(());
    }
    output::require_text(format, command)
}

#[derive(Subcommand)]
enum Commands {
    /// Login to Azure DevOps with a Personal Access Token (PAT)
    Login {
        /// Optional named credential profile (used by `azdocli migrate`).
        /// Omit to use the legacy default credentials store.
        #[clap(long)]
        profile: Option<String>,
    },
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
    /// Manage Azure Artifacts feeds and Universal Packages
    Artifacts {
        #[clap(subcommand)]
        subcommand: artifacts::ArtifactsSubCommands,
    },
    /// Manage Azure DevOps classic releases
    Releases {
        #[clap(subcommand)]
        subcommand: releases::ReleasesSubCommands,
    },
    /// Manage Azure DevOps repos
    Repos {
        #[clap(subcommand)]
        subcommand: repos::ReposSubCommands,
    },
    /// List Azure DevOps projects
    Projects {
        #[clap(subcommand)]
        subcommand: projects::ProjectsSubCommands,
    },
    /// Manage users
    User {
        #[clap(subcommand)]
        subcommand: user::UserSubCommands,
    },
    /// Manage Azure DevOps wikis
    Wiki {
        #[clap(subcommand)]
        subcommand: wiki::WikiSubCommands,
    },
    /// Migrate one or more team projects between organizations (Experimental)
    Migrate {
        #[clap(subcommand)]
        subcommand: migrate::MigrateSubCommands,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    check_output_support(cli.output, &command_path(&matches))?;
    match &cli.command {
        Some(Commands::Login { profile }) => {
            login(profile.as_deref()).await?;
        }
        Some(Commands::Logout) => {
            logout()?;
        }
        Some(Commands::Project { project_name }) => match project_name {
            Some(project) => {
                save_default_project(project)?;
                println!("✅ Default project set to: {project}");
            }
            None => match get_default_project() {
                Ok(project) => println!("Current default project: {project}"),
                Err(_) => println!("No default project configured"),
            },
        },
        Some(Commands::Pipelines { subcommand }) => {
            pipelines::handle_command(subcommand, cli.output).await?;
        }
        Some(Commands::Boards { subcommand }) => {
            boards::handle_command(subcommand).await?;
        }
        Some(Commands::Artifacts { subcommand }) => {
            artifacts::handle_command(subcommand).await?;
        }
        Some(Commands::Releases { subcommand }) => {
            releases::handle_command(subcommand).await?;
        }
        Some(Commands::Repos { subcommand }) => {
            repos::handle_command(subcommand, cli.output).await?;
        }
        Some(Commands::Projects { subcommand }) => {
            projects::handle_command(subcommand).await?;
        }
        Some(Commands::User { subcommand }) => {
            user::handle_command(subcommand).await?;
        }
        Some(Commands::Wiki { subcommand }) => {
            wiki::handle_command(subcommand).await?;
        }
        Some(Commands::Migrate { subcommand }) => {
            migrate::handle_command(subcommand).await?;
        }
        None => {
            Cli::command().print_help()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputFormat;

    fn matches(args: &[&str]) -> clap::ArgMatches {
        Cli::command().try_get_matches_from(args).unwrap()
    }

    #[test]
    fn output_flag_is_accepted_after_the_subcommand() {
        let cli = Cli::try_parse_from(["azdocli", "repos", "list", "-o", "json"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Json);
    }

    #[test]
    fn output_flag_defaults_to_text() {
        let cli = Cli::try_parse_from(["azdocli", "repos", "list"]).unwrap();
        assert_eq!(cli.output, OutputFormat::Text);
    }

    #[test]
    fn command_path_names_the_nested_subcommand() {
        let matches = matches(&["azdocli", "repos", "pr", "list", "--output", "yaml"]);
        assert_eq!(command_path(&matches), "repos pr list");
    }

    #[test]
    fn commands_without_a_renderer_reject_structured_output() {
        let err = check_output_support(OutputFormat::Json, "wiki list").unwrap_err();
        assert_eq!(
            err.to_string(),
            "--output json is not supported for 'wiki list' yet"
        );
    }

    #[test]
    fn migrated_commands_accept_structured_output() {
        for command in [
            "repos list",
            "repos show",
            "pipelines list",
            "pipelines show",
        ] {
            assert!(check_output_support(OutputFormat::Yaml, command).is_ok());
        }
    }

    #[test]
    fn every_command_accepts_text_output() {
        assert!(check_output_support(OutputFormat::Text, "wiki list").is_ok());
    }
}
