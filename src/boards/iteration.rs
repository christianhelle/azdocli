use super::classification::{self, NodePath, StructureGroup};
use super::{
    create_wit_client, create_work_client, display_empty_work_items_table, display_work_items_list,
};
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::work::models::{TeamSetting, TeamSettingsIteration, TeamSettingsPatch};
use chrono::NaiveDate;
use clap::{Args, Subcommand};
use colored::Colorize;

const GROUP: StructureGroup = StructureGroup::Iterations;

/// The work items API takes at most this many ids per request.
const WORK_ITEMS_PER_REQUEST: usize = 200;

#[derive(Subcommand, Clone)]
pub enum IterationSubCommands {
    /// Manage the iterations of a project
    Project {
        #[clap(subcommand)]
        subcommand: IterationProjectSubCommands,
    },
    /// Manage the iterations a team works in
    Team {
        #[clap(subcommand)]
        subcommand: IterationTeamSubCommands,
    },
}

#[derive(Subcommand, Clone)]
pub enum IterationProjectSubCommands {
    /// List the iterations of a project as a tree
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Iteration path to list from, e.g. '\MyProject\Iteration\Release 1' (default: the project's root iteration)
        #[clap(long)]
        path: Option<String>,
        /// How many levels of child iterations to fetch
        #[clap(long, default_value = "1", value_parser = clap::value_parser!(i32).range(0..))]
        depth: i32,
    },
    /// Show the details of an iteration
    Show {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Iteration path, e.g. '\MyProject\Iteration\Sprint 1' or 'MyProject\Sprint 1'
        #[clap(long)]
        path: String,
    },
    /// Create an iteration
    Create {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name of the new iteration
        #[clap(long)]
        name: String,
        /// Path of the parent iteration (default: the project's root iteration)
        #[clap(long)]
        path: Option<String>,
        /// Start date of the iteration, e.g. 2026-06-01
        #[clap(long, value_parser = classification::parse_date, requires = "finish_date")]
        start_date: Option<NaiveDate>,
        /// Finish date of the iteration, e.g. 2026-06-14
        #[clap(long, value_parser = classification::parse_date, requires = "start_date")]
        finish_date: Option<NaiveDate>,
    },
    /// Delete an iteration
    Delete {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Iteration path to delete
        #[clap(long)]
        path: String,
        /// ID of the iteration to move the deleted iteration's work items to
        #[clap(long)]
        reclassify_id: Option<i32>,
    },
    /// Rename an iteration, change its dates, or move another iteration under it
    #[clap(group = clap::ArgGroup::new("changes").required(true).multiple(true).args(["name", "child_id", "start_date"]))]
    Update {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Iteration path to update
        #[clap(long)]
        path: String,
        /// New name of the iteration
        #[clap(long)]
        name: Option<String>,
        /// ID of an existing iteration to move, with its children, under this iteration
        #[clap(long)]
        child_id: Option<i32>,
        /// New start date of the iteration, e.g. 2026-06-01
        #[clap(long, value_parser = classification::parse_date, requires = "finish_date")]
        start_date: Option<NaiveDate>,
        /// New finish date of the iteration, e.g. 2026-06-14
        #[clap(long, value_parser = classification::parse_date, requires = "start_date")]
        finish_date: Option<NaiveDate>,
    },
}

/// Picks a project iteration by its identifier or by its path.
#[derive(Args, Clone, Debug)]
#[group(required = true, multiple = false)]
pub struct IterationSelector {
    /// Identifier (GUID) of the iteration
    #[clap(long)]
    id: Option<String>,
    /// Path of the iteration, e.g. 'MyProject\Sprint 1'
    #[clap(long)]
    path: Option<String>,
}

#[derive(Subcommand, Clone)]
pub enum IterationTeamSubCommands {
    /// List the iterations a team works in
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        /// Only list the iterations in this timeframe
        #[clap(long, value_parser = ["current"])]
        timeframe: Option<String>,
    },
    /// Add an iteration to a team
    Add {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        #[clap(flatten)]
        iteration: IterationSelector,
    },
    /// Remove an iteration from a team
    Remove {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        #[clap(flatten)]
        iteration: IterationSelector,
    },
    /// List the work items in one of a team's iterations
    ListWorkItems {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        #[clap(flatten)]
        iteration: IterationSelector,
    },
    /// Show the iteration new work items of a team go to
    ShowDefaultIteration {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
    },
    /// Set the iteration new work items of a team go to
    #[clap(group = clap::ArgGroup::new("default").required(true).multiple(false).args(["id", "path", "default_iteration_macro"]))]
    SetDefaultIteration {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        /// Identifier (GUID) of the iteration
        #[clap(long)]
        id: Option<String>,
        /// Path of the iteration, e.g. 'MyProject\Sprint 1'
        #[clap(long)]
        path: Option<String>,
        /// Macro that picks the iteration, e.g. '@CurrentIteration'
        #[clap(long)]
        default_iteration_macro: Option<String>,
    },
    /// Show the iteration a team's backlog lives in
    ShowBacklogIteration {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
    },
    /// Set the iteration a team's backlog lives in
    SetBacklogIteration {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        #[clap(flatten)]
        iteration: IterationSelector,
    },
}

pub(super) async fn handle_command(subcommand: &IterationSubCommands) -> Result<()> {
    let result = match subcommand {
        IterationSubCommands::Project { subcommand } => handle_project_command(subcommand).await,
        IterationSubCommands::Team { subcommand } => handle_team_command(subcommand).await,
    };

    if let Err(e) = &result {
        eprintln!("❌ {e}");
        if let Some(cause) = e.chain().nth(1) {
            eprintln!("   {cause}");
        }
    }
    result
}

fn dates(start: Option<NaiveDate>, finish: Option<NaiveDate>) -> Result<Option<serde_json::Value>> {
    match (start, finish) {
        (Some(start), Some(finish)) => classification::iteration_dates(start, finish).map(Some),
        _ => Ok(None),
    }
}

async fn handle_project_command(subcommand: &IterationProjectSubCommands) -> Result<()> {
    match subcommand {
        IterationProjectSubCommands::List {
            project,
            path,
            depth,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let path = NodePath::parse_or_root(path.as_deref(), &project, GROUP);
            classification::get_node(&project, GROUP, &path, Some(*depth))
                .await
                .map(|node| classification::display_tree(GROUP, &node))
                .map_err(|e| e.context("Failed to list iterations"))
        }
        IterationProjectSubCommands::Show { project, path } => {
            let project = get_project_or_default(project.as_deref())?;
            let path = NodePath::parse(path, &project, GROUP);
            classification::get_node(&project, GROUP, &path, None)
                .await
                .map(|node| classification::display_node(GROUP, &node))
                .map_err(|e| e.context("Failed to show iteration"))
        }
        IterationProjectSubCommands::Create {
            project,
            name,
            path,
            start_date,
            finish_date,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let parent = NodePath::parse_or_root(path.as_deref(), &project, GROUP);
            let attributes = dates(*start_date, *finish_date)?;
            match classification::create_node(&project, GROUP, &parent, name, attributes).await {
                Ok(node) => {
                    println!("{}", "✅ Iteration created successfully!".green());
                    classification::display_node(GROUP, &node);
                    Ok(())
                }
                Err(e) => Err(e.context("Failed to create iteration")),
            }
        }
        IterationProjectSubCommands::Delete {
            project,
            path,
            reclassify_id,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let node_path = NodePath::parse(path, &project, GROUP);
            classification::delete_node(&project, GROUP, &node_path, *reclassify_id)
                .await
                .map(|_| {
                    println!(
                        "{}",
                        format!("✅ Iteration '{path}' deleted successfully").green()
                    )
                })
                .map_err(|e| e.context("Failed to delete iteration"))
        }
        IterationProjectSubCommands::Update {
            project,
            path,
            name,
            child_id,
            start_date,
            finish_date,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let node_path = NodePath::parse(path, &project, GROUP);
            let attributes = dates(*start_date, *finish_date)?;
            update_iteration(&project, &node_path, name.as_deref(), *child_id, attributes)
                .await
                .map_err(|e| e.context("Failed to update iteration"))
        }
    }
}

/// Moves `child_id` under the iteration first, since renaming changes its path.
async fn update_iteration(
    project: &str,
    path: &NodePath,
    name: Option<&str>,
    child_id: Option<i32>,
    attributes: Option<serde_json::Value>,
) -> Result<()> {
    if let Some(child_id) = child_id {
        classification::move_node(project, GROUP, path, child_id).await?;
        println!(
            "{}",
            format!(
                "✅ Moved iteration {child_id} under '{}'",
                path.logical(project)
            )
            .green()
        );
    }

    let node = if name.is_some() || attributes.is_some() {
        classification::update_node(project, GROUP, path, name, attributes).await?
    } else {
        classification::get_node(project, GROUP, path, None).await?
    };

    println!("{}", "✅ Iteration updated successfully!".green());
    classification::display_node(GROUP, &node);
    Ok(())
}

async fn handle_team_command(subcommand: &IterationTeamSubCommands) -> Result<()> {
    match subcommand {
        IterationTeamSubCommands::List {
            project,
            team,
            timeframe,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            list_team_iterations(&project, team, timeframe.as_deref())
                .await
                .map(|iterations| display_team_iterations(team, &iterations))
                .map_err(|e| e.context(format!("Failed to list the iterations of team '{team}'")))
        }
        IterationTeamSubCommands::Add {
            project,
            team,
            iteration,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            add_team_iteration(&project, team, iteration)
                .await
                .map(|added| {
                    println!("{}", format!("✅ Added iteration to team '{team}'").green());
                    display_team_iteration(&added);
                })
                .map_err(|e| e.context(format!("Failed to add an iteration to team '{team}'")))
        }
        IterationTeamSubCommands::Remove {
            project,
            team,
            iteration,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            remove_team_iteration(&project, team, iteration)
                .await
                .map(|_| {
                    println!(
                        "{}",
                        format!("✅ Removed iteration from team '{team}'").green()
                    )
                })
                .map_err(|e| e.context(format!("Failed to remove an iteration from team '{team}'")))
        }
        IterationTeamSubCommands::ListWorkItems {
            project,
            team,
            iteration,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            list_iteration_work_items(&project, team, iteration)
                .await
                .map_err(|e| e.context("Failed to list the work items of the iteration"))
        }
        IterationTeamSubCommands::ShowDefaultIteration { project, team } => {
            let project = get_project_or_default(project.as_deref())?;
            get_team_settings(&project, team)
                .await
                .map(|settings| display_default_iteration(team, &settings))
                .map_err(|e| {
                    e.context(format!(
                        "Failed to show the default iteration of team '{team}'"
                    ))
                })
        }
        IterationTeamSubCommands::SetDefaultIteration {
            project,
            team,
            id,
            path,
            default_iteration_macro,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let mut patch = TeamSettingsPatch::new();
            match default_iteration_macro {
                Some(default_iteration_macro) => {
                    patch.default_iteration_macro = Some(default_iteration_macro.clone())
                }
                None => {
                    let selector = IterationSelector {
                        id: id.clone(),
                        path: path.clone(),
                    };
                    patch.default_iteration = Some(resolve_iteration_id(&project, &selector).await?)
                }
            }
            update_team_settings(&project, team, patch)
                .await
                .map(|settings| {
                    println!(
                        "{}",
                        format!("✅ Set the default iteration of team '{team}'").green()
                    );
                    display_default_iteration(team, &settings);
                })
                .map_err(|e| {
                    e.context(format!(
                        "Failed to set the default iteration of team '{team}'"
                    ))
                })
        }
        IterationTeamSubCommands::ShowBacklogIteration { project, team } => {
            let project = get_project_or_default(project.as_deref())?;
            get_team_settings(&project, team)
                .await
                .map(|settings| display_backlog_iteration(team, &settings))
                .map_err(|e| {
                    e.context(format!(
                        "Failed to show the backlog iteration of team '{team}'"
                    ))
                })
        }
        IterationTeamSubCommands::SetBacklogIteration {
            project,
            team,
            iteration,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let mut patch = TeamSettingsPatch::new();
            patch.backlog_iteration = Some(resolve_iteration_id(&project, iteration).await?);
            update_team_settings(&project, team, patch)
                .await
                .map(|settings| {
                    println!(
                        "{}",
                        format!("✅ Set the backlog iteration of team '{team}'").green()
                    );
                    display_backlog_iteration(team, &settings);
                })
                .map_err(|e| {
                    e.context(format!(
                        "Failed to set the backlog iteration of team '{team}'"
                    ))
                })
        }
    }
}

/// The team settings APIs know iterations only by their identifier, so a path
/// is looked up first.
async fn resolve_iteration_id(project: &str, selector: &IterationSelector) -> Result<String> {
    if let Some(id) = &selector.id {
        return Ok(id.clone());
    }

    let path = selector
        .path
        .as_deref()
        .ok_or_else(|| anyhow!("Either --id or --path is required"))?;
    classification::get_node(project, GROUP, &NodePath::parse(path, project, GROUP), None)
        .await?
        .identifier
        .ok_or_else(|| anyhow!("Iteration '{path}' has no identifier"))
}

async fn list_team_iterations(
    project: &str,
    team: &str,
    timeframe: Option<&str>,
) -> Result<Vec<TeamSettingsIteration>> {
    let creds = get_credentials()?;
    let mut request =
        create_work_client()?
            .iterations_client()
            .list(creds.organization, project, team);

    if let Some(timeframe) = timeframe {
        request = request.timeframe(timeframe);
    }

    Ok(request.await?.value)
}

async fn add_team_iteration(
    project: &str,
    team: &str,
    selector: &IterationSelector,
) -> Result<TeamSettingsIteration> {
    let mut iteration = TeamSettingsIteration::new();
    iteration.id = Some(resolve_iteration_id(project, selector).await?);

    let creds = get_credentials()?;
    Ok(create_work_client()?
        .iterations_client()
        .post_team_iteration(creds.organization, iteration, project, team)
        .await?)
}

async fn remove_team_iteration(
    project: &str,
    team: &str,
    selector: &IterationSelector,
) -> Result<()> {
    let id = resolve_iteration_id(project, selector).await?;

    let creds = get_credentials()?;
    create_work_client()?
        .iterations_client()
        .delete(creds.organization, project, id, team)
        .await?;
    Ok(())
}

async fn list_iteration_work_items(
    project: &str,
    team: &str,
    selector: &IterationSelector,
) -> Result<()> {
    let id = resolve_iteration_id(project, selector).await?;

    let creds = get_credentials()?;
    let relations = create_work_client()?
        .iterations_client()
        .get_iteration_work_items(creds.organization.clone(), project, id, team)
        .await?
        .work_item_relations;

    // Child work items appear both as targets of their parent and on their own.
    let mut ids: Vec<i32> = Vec::new();
    for id in relations
        .iter()
        .filter_map(|relation| relation.target.as_ref()?.id)
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    let heading = "Iteration Work Items";
    if ids.is_empty() {
        display_empty_work_items_table(heading);
        return Ok(());
    }

    let client = create_wit_client()?;
    let mut work_items = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(WORK_ITEMS_PER_REQUEST) {
        let ids = chunk
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        work_items.extend(
            client
                .work_items_client()
                .list(creds.organization.clone(), ids, project)
                .await?
                .value,
        );
    }

    display_work_items_list(heading, &work_items);
    Ok(())
}

async fn get_team_settings(project: &str, team: &str) -> Result<TeamSetting> {
    let creds = get_credentials()?;
    Ok(create_work_client()?
        .teamsettings_client()
        .get(creds.organization, project, team)
        .await?)
}

async fn update_team_settings(
    project: &str,
    team: &str,
    patch: TeamSettingsPatch,
) -> Result<TeamSetting> {
    let creds = get_credentials()?;
    Ok(create_work_client()?
        .teamsettings_client()
        .update(creds.organization, patch, project, team)
        .await?)
}

/// Start date, finish date and timeframe of a team iteration, or `-`.
fn iteration_schedule(iteration: &TeamSettingsIteration) -> (String, String, String) {
    let attributes = iteration.attributes.as_ref();
    let start = attributes
        .and_then(|attributes| attributes.start_date)
        .map(|date| date.date().to_string())
        .unwrap_or_else(|| "-".to_string());
    let finish = attributes
        .and_then(|attributes| attributes.finish_date)
        .map(|date| date.date().to_string())
        .unwrap_or_else(|| "-".to_string());
    let timeframe = attributes
        .and_then(|attributes| attributes.time_frame.as_ref())
        .map(|time_frame| format!("{time_frame:?}"))
        .unwrap_or_else(|| "-".to_string());
    (start, finish, timeframe)
}

fn display_team_iterations(team: &str, iterations: &[TeamSettingsIteration]) {
    println!(
        "{}",
        format!(
            "🗓️  Iterations of team '{}'",
            escape_control_characters(team)
        )
        .bold()
    );
    println!("{}", "=".repeat(110));

    if iterations.is_empty() {
        println!("No iterations found.");
        return;
    }

    println!(
        "{:<36} {:<11} {:<11} {:<10} {}",
        "Path".bold(),
        "Start".bold(),
        "Finish".bold(),
        "Timeframe".bold(),
        "ID".bold()
    );
    println!("{}", "-".repeat(110));

    for iteration in iterations {
        let (start, finish, timeframe) = iteration_schedule(iteration);
        println!(
            "{:<36} {start:<11} {finish:<11} {timeframe:<10} {}",
            escape_control_characters(iteration.path.as_deref().unwrap_or("-")),
            iteration.id.as_deref().unwrap_or("-")
        );
    }

    println!("\n{} iteration(s)", iterations.len());
}

fn display_team_iteration(iteration: &TeamSettingsIteration) {
    let (start, finish, timeframe) = iteration_schedule(iteration);
    if let Some(name) = &iteration.name {
        println!("📝 Name: {}", escape_control_characters(name));
    }
    if let Some(path) = iteration.path.as_deref().filter(|path| !path.is_empty()) {
        println!("📂 Path: {}", escape_control_characters(path));
    }
    if let Some(id) = &iteration.id {
        println!("🔑 Identifier: {id}");
    }
    println!("📅 Start Date: {start}");
    println!("📅 Finish Date: {finish}");
    println!("⏱️  Timeframe: {timeframe}");
}

fn display_default_iteration(team: &str, settings: &TeamSetting) {
    println!(
        "{}",
        format!(
            "🗓️  Default iteration of team '{}'",
            escape_control_characters(team)
        )
        .bold()
    );
    if let Some(default_iteration_macro) = &settings.default_iteration_macro {
        println!(
            "🔣 Macro: {}",
            escape_control_characters(default_iteration_macro)
        );
    }
    match &settings.default_iteration {
        Some(iteration) => display_team_iteration(iteration),
        None => println!("The team has no default iteration."),
    }
}

fn display_backlog_iteration(team: &str, settings: &TeamSetting) {
    println!(
        "{}",
        format!(
            "🗓️  Backlog iteration of team '{}'",
            escape_control_characters(team)
        )
        .bold()
    );
    match &settings.backlog_iteration {
        Some(iteration) => display_team_iteration(iteration),
        None => println!("The team has no backlog iteration."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[clap(subcommand)]
        command: IterationSubCommands,
    }

    fn parse(args: &[&str]) -> Result<IterationSubCommands, clap::Error> {
        TestCli::try_parse_from(std::iter::once("iteration").chain(args.iter().copied()))
            .map(|cli| cli.command)
    }

    #[test]
    fn project_create_accepts_dates() {
        let IterationSubCommands::Project {
            subcommand:
                IterationProjectSubCommands::Create {
                    name,
                    start_date,
                    finish_date,
                    ..
                },
        } = parse(&[
            "project",
            "create",
            "--name",
            "Sprint 1",
            "--start-date",
            "2026-06-01",
            "--finish-date",
            "2026-06-14",
        ])
        .unwrap()
        else {
            panic!("expected project create");
        };
        assert_eq!(name, "Sprint 1");
        assert_eq!(start_date, NaiveDate::from_ymd_opt(2026, 6, 1));
        assert_eq!(finish_date, NaiveDate::from_ymd_opt(2026, 6, 14));
    }

    #[test]
    fn project_create_needs_both_dates() {
        assert!(parse(&[
            "project",
            "create",
            "--name",
            "Sprint 1",
            "--start-date",
            "2026-06-01"
        ])
        .is_err());
    }

    #[test]
    fn project_create_rejects_a_malformed_date() {
        assert!(parse(&[
            "project",
            "create",
            "--name",
            "Sprint 1",
            "--start-date",
            "01/06/2026",
            "--finish-date",
            "2026-06-14",
        ])
        .is_err());
    }

    #[test]
    fn project_update_needs_a_change() {
        assert!(parse(&["project", "update", "--path", "Sprint 1"]).is_err());
        assert!(parse(&[
            "project",
            "update",
            "--path",
            "Sprint 1",
            "--start-date",
            "2026-06-01",
            "--finish-date",
            "2026-06-14",
        ])
        .is_ok());
    }

    #[test]
    fn team_add_takes_an_id_or_a_path() {
        assert!(parse(&["team", "add", "--team", "Team A", "--id", "abc"]).is_ok());
        assert!(parse(&["team", "add", "--team", "Team A", "--path", "Sprint 1"]).is_ok());
        assert!(parse(&["team", "add", "--team", "Team A"]).is_err());
        assert!(
            parse(&["team", "add", "--team", "Team A", "--id", "abc", "--path", "Sprint 1"])
                .is_err()
        );
    }

    #[test]
    fn team_list_accepts_only_the_current_timeframe() {
        assert!(parse(&["team", "list", "--team", "Team A", "--timeframe", "current"]).is_ok());
        assert!(parse(&["team", "list", "--team", "Team A", "--timeframe", "past"]).is_err());
    }

    #[test]
    fn set_default_iteration_takes_one_of_id_path_or_macro() {
        assert!(parse(&[
            "team",
            "set-default-iteration",
            "--team",
            "Team A",
            "--default-iteration-macro",
            "@CurrentIteration",
        ])
        .is_ok());
        assert!(parse(&["team", "set-default-iteration", "--team", "Team A"]).is_err());
        assert!(parse(&[
            "team",
            "set-default-iteration",
            "--team",
            "Team A",
            "--id",
            "abc",
            "--default-iteration-macro",
            "@CurrentIteration",
        ])
        .is_err());
    }

    #[test]
    fn team_commands_use_kebab_case_names() {
        for command in [
            "list-work-items",
            "show-default-iteration",
            "show-backlog-iteration",
            "set-backlog-iteration",
        ] {
            let result = parse(&["team", command, "--team", "Team A", "--id", "abc"]);
            let accepts_id =
                command != "show-default-iteration" && command != "show-backlog-iteration";
            assert_eq!(result.is_ok(), accepts_id, "{command}");
        }
    }
}
