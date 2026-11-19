use crate::auth::{login, logout};
use crate::output::OutputFormat;
use crate::project::{get_default_project, save_default_project};
use anyhow::Context;
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use colored::Colorize;
use std::io::IsTerminal;

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
mod update;
mod user;
mod wiki;

#[derive(Parser)]
#[clap(about, version)]
struct Cli {
    /// Output format
    #[clap(short, long, global = true, value_enum, default_value_t)]
    output: OutputFormat,

    /// Don't check for a newer azdocli after a command
    /// (or set AZDOCLI_NO_UPDATE_CHECK)
    #[clap(long, global = true)]
    no_update_check: bool,

    #[clap(subcommand)]
    command: Option<Commands>,
}

/// Commands that print through a `Render` model and so support every format.
const STRUCTURED_OUTPUT_COMMANDS: &[&str] = &[
    "repos list",
    "repos show",
    "pipelines list",
    "pipelines show",
    "pipelines runs list",
    "repos pr list",
    "repos pr show",
    "boards work-item list",
    "boards work-item show",
    "boards work-item create",
    "boards work-item update",
    "boards work-item attachment list",
    "boards query",
    "boards sprint show",
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

/// Flags that open a browser instead of printing a result.
const BROWSER_FLAGS: &[&str] = &["web", "open"];

/// Fails when a browser flag is combined with a format other than text,
/// since opening a browser prints a status line instead of the result.
fn check_browser_flags(format: OutputFormat, matches: &ArgMatches) -> anyhow::Result<()> {
    let mut leaf = matches;
    while let Some((_, sub)) = leaf.subcommand() {
        leaf = sub;
    }
    for flag in BROWSER_FLAGS {
        if let Ok(Some(true)) = leaf.try_get_one::<bool>(flag) {
            if format != OutputFormat::Text {
                anyhow::bail!("--{flag} opens a browser and cannot be combined with --output");
            }
        }
    }
    Ok(())
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
    /// Check whether a newer azdocli has been released
    CheckUpdate,
    /// Install the latest azdocli release in place of this one
    Update,
}

/// Whether to tell the user, unasked, about a newer azdocli once `cli` has
/// run: only for a person reading text output, and not for the commands
/// that already deal with updates.
fn wants_update_notice(cli: &Cli, opted_out_by_env: bool, stderr_is_terminal: bool) -> bool {
    let update_command = matches!(
        cli.command,
        None | Some(Commands::CheckUpdate) | Some(Commands::Update)
    );
    !cli.no_update_check
        && !opted_out_by_env
        && stderr_is_terminal
        && cli.output == OutputFormat::Text
        && !update_command
}

/// Mentions a newer azdocli on stderr, if there is one. Never fails.
async fn notify_of_update() {
    let Ok(path) = update::check_path() else {
        return;
    };
    let current = update::current();
    if let Some(latest) = update::available(
        &update::http_client(),
        update::GITHUB_API,
        &path,
        &current,
        chrono::Utc::now(),
    )
    .await
    {
        eprintln!(
            "{}",
            format!(
                "azdocli {latest} is available (you have {current}). Run `azdocli update` to install it."
            )
            .yellow()
        );
    }
}

async fn check_update() -> anyhow::Result<()> {
    let release = update::check(
        &update::http_client(),
        update::GITHUB_API,
        &update::check_path()?,
        chrono::Utc::now(),
    )
    .await
    .context("could not check for updates")?;
    let current = update::current();
    if update::newer(&current, &release.version) {
        println!(
            "azdocli {} is available (you have {current}): {}",
            release.version, release.url
        );
        println!("Run `azdocli update` to install it.");
    } else {
        println!("azdocli {current} is up to date.");
    }
    Ok(())
}

async fn install_update() -> anyhow::Result<()> {
    let outcome = update::update_running(
        &update::http_client(),
        update::GITHUB_API,
        &update::check_path()?,
    )
    .await?;
    match outcome {
        update::Outcome::Installed(version) => {
            println!("✅ azdocli {version} is installed.");
        }
        update::Outcome::UpToDate(version) => {
            println!("azdocli {version} is up to date.");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    check_output_support(cli.output, &command_path(&matches))?;
    check_browser_flags(cli.output, &matches)?;
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
            boards::handle_command(subcommand, cli.output).await?;
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
        Some(Commands::CheckUpdate) => {
            check_update().await?;
        }
        Some(Commands::Update) => {
            install_update().await?;
        }
        None => {
            Cli::command().print_help()?;
        }
    }

    let opted_out_by_env = std::env::var_os("AZDOCLI_NO_UPDATE_CHECK").is_some();
    if wants_update_notice(&cli, opted_out_by_env, std::io::stderr().is_terminal()) {
        notify_of_update().await;
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
            "pipelines runs list",
            "repos pr list",
            "repos pr show",
            "boards work-item list",
            "boards work-item show",
            "boards work-item create",
            "boards work-item update",
            "boards work-item attachment list",
            "boards query",
            "boards sprint show",
        ] {
            assert!(check_output_support(OutputFormat::Yaml, command).is_ok());
        }
    }

    #[test]
    fn browser_flags_reject_structured_output() {
        for (args, flag) in [
            (
                &[
                    "azdocli",
                    "pipelines",
                    "show",
                    "--id",
                    "7",
                    "--open",
                    "-o",
                    "json",
                ][..],
                "--open",
            ),
            (
                &[
                    "azdocli", "repos", "pr", "show", "--repo", "api", "--id", "1", "--web", "-o",
                    "yaml",
                ][..],
                "--web",
            ),
            (
                &[
                    "azdocli",
                    "boards",
                    "work-item",
                    "show",
                    "--id",
                    "1",
                    "--web",
                    "-o",
                    "markdown",
                ][..],
                "--web",
            ),
        ] {
            let matches = matches(args);
            let format = Cli::from_arg_matches(&matches).unwrap().output;
            let err = check_browser_flags(format, &matches).unwrap_err();
            assert!(
                err.to_string().contains(flag),
                "{args:?} gave unexpected error: {err}"
            );
        }
    }

    #[test]
    fn browser_flags_are_fine_with_text_output() {
        let matches = matches(&["azdocli", "pipelines", "show", "--id", "7", "--open"]);
        assert!(check_browser_flags(OutputFormat::Text, &matches).is_ok());
    }

    #[test]
    fn structured_output_is_fine_without_browser_flags() {
        let matches = matches(&["azdocli", "repos", "list", "-o", "json"]);
        assert!(check_browser_flags(OutputFormat::Json, &matches).is_ok());
    }

    #[test]
    fn update_commands_parse() {
        let cli = Cli::try_parse_from(["azdocli", "check-update"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::CheckUpdate)));
        let cli = Cli::try_parse_from(["azdocli", "update"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Update)));
    }

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn a_person_running_a_command_hears_of_updates() {
        assert!(wants_update_notice(
            &cli(&["azdocli", "repos", "list"]),
            false,
            true
        ));
    }

    #[test]
    fn the_update_notice_can_be_turned_off() {
        let opted_out = cli(&["azdocli", "repos", "list", "--no-update-check"]);
        assert!(opted_out.no_update_check);
        assert!(!wants_update_notice(&opted_out, false, true));
        assert!(!wants_update_notice(
            &cli(&["azdocli", "repos", "list"]),
            true,
            true
        ));
    }

    #[test]
    fn scripts_and_structured_output_get_no_update_notice() {
        assert!(!wants_update_notice(
            &cli(&["azdocli", "repos", "list"]),
            false,
            false
        ));
        assert!(!wants_update_notice(
            &cli(&["azdocli", "repos", "list", "-o", "json"]),
            false,
            true
        ));
    }

    #[test]
    fn update_commands_and_help_get_no_update_notice() {
        for args in [
            &["azdocli"][..],
            &["azdocli", "check-update"],
            &["azdocli", "update"],
        ] {
            assert!(!wants_update_notice(&cli(args), false, true), "{args:?}");
        }
    }

    #[test]
    fn every_command_accepts_text_output() {
        assert!(check_output_support(OutputFormat::Text, "wiki list").is_ok());
    }
}
