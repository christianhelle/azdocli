use super::classification::{self, NodePath, StructureGroup};
use super::create_work_client;
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::work::models::{TeamFieldValue, TeamFieldValues, TeamFieldValuesPatch};
use clap::Subcommand;
use colored::Colorize;

const GROUP: StructureGroup = StructureGroup::Areas;

#[derive(Subcommand, Clone)]
pub enum AreaSubCommands {
    /// Manage the area paths of a project
    Project {
        #[clap(subcommand)]
        subcommand: AreaProjectSubCommands,
    },
    /// Manage the area paths a team owns
    Team {
        #[clap(subcommand)]
        subcommand: AreaTeamSubCommands,
    },
}

#[derive(Subcommand, Clone)]
pub enum AreaProjectSubCommands {
    /// List the area paths of a project as a tree
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Area path to list from, e.g. '\MyProject\Area\Team A' (default: the project's root area)
        #[clap(long)]
        path: Option<String>,
        /// How many levels of child areas to fetch
        #[clap(long, default_value = "1", value_parser = clap::value_parser!(i32).range(0..))]
        depth: i32,
    },
    /// Show the details of an area path
    Show {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Area path, e.g. '\MyProject\Area\Team A' or 'MyProject\Team A'
        #[clap(long)]
        path: String,
    },
    /// Create an area path
    Create {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name of the new area
        #[clap(long)]
        name: String,
        /// Path of the parent area (default: the project's root area)
        #[clap(long)]
        path: Option<String>,
    },
    /// Delete an area path
    Delete {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Area path to delete
        #[clap(long)]
        path: String,
        /// ID of the area to move the deleted area's work items to
        #[clap(long)]
        reclassify_id: Option<i32>,
    },
    /// Rename an area path, or move another area path under it
    #[clap(group = clap::ArgGroup::new("changes").required(true).multiple(true).args(["name", "child_id"]))]
    Update {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Area path to update
        #[clap(long)]
        path: String,
        /// New name of the area
        #[clap(long)]
        name: Option<String>,
        /// ID of an existing area to move, with its children, under this area
        #[clap(long)]
        child_id: Option<i32>,
    },
}

#[derive(Subcommand, Clone)]
pub enum AreaTeamSubCommands {
    /// List the area paths a team owns
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
    },
    /// Give a team an area path
    Add {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        /// Area path, e.g. '\MyProject\Area\Team A' or 'MyProject\Team A'
        #[clap(long)]
        path: String,
        /// Include the area's sub-areas (true or false; the flag alone means true)
        #[clap(long, num_args = 0..=1, default_missing_value = "true")]
        include_sub_areas: Option<bool>,
        /// Make this area the team's default area
        #[clap(long)]
        set_as_default: bool,
    },
    /// Take an area path away from a team
    Remove {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        /// Area path to remove
        #[clap(long)]
        path: String,
    },
    /// Change whether a team's area path includes sub-areas, or make it the team's default
    #[clap(group = clap::ArgGroup::new("changes").required(true).multiple(true).args(["include_sub_areas", "set_as_default"]))]
    Update {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Name or ID of the team
        #[clap(long)]
        team: String,
        /// Area path to update
        #[clap(long)]
        path: String,
        /// Include the area's sub-areas (true or false; the flag alone means true)
        #[clap(long, num_args = 0..=1, default_missing_value = "true")]
        include_sub_areas: Option<bool>,
        /// Make this area the team's default area
        #[clap(long)]
        set_as_default: bool,
    },
}

pub(super) async fn handle_command(subcommand: &AreaSubCommands) -> Result<()> {
    let result = match subcommand {
        AreaSubCommands::Project { subcommand } => handle_project_command(subcommand).await,
        AreaSubCommands::Team { subcommand } => handle_team_command(subcommand).await,
    };

    if let Err(e) = &result {
        eprintln!("❌ {e}");
        if let Some(cause) = e.chain().nth(1) {
            eprintln!("   {cause}");
        }
    }
    result
}

async fn handle_project_command(subcommand: &AreaProjectSubCommands) -> Result<()> {
    match subcommand {
        AreaProjectSubCommands::List {
            project,
            path,
            depth,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let path = NodePath::parse_or_root(path.as_deref(), &project, GROUP);
            classification::get_node(&project, GROUP, &path, Some(*depth))
                .await
                .map(|node| classification::display_tree(GROUP, &node))
                .map_err(|e| e.context("Failed to list areas"))
        }
        AreaProjectSubCommands::Show { project, path } => {
            let project = get_project_or_default(project.as_deref())?;
            let path = NodePath::parse(path, &project, GROUP);
            classification::get_node(&project, GROUP, &path, None)
                .await
                .map(|node| classification::display_node(GROUP, &node))
                .map_err(|e| e.context("Failed to show area"))
        }
        AreaProjectSubCommands::Create {
            project,
            name,
            path,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let parent = NodePath::parse_or_root(path.as_deref(), &project, GROUP);
            match classification::create_node(&project, GROUP, &parent, name, None).await {
                Ok(node) => {
                    println!("{}", "✅ Area created successfully!".green());
                    classification::display_node(GROUP, &node);
                    Ok(())
                }
                Err(e) => Err(e.context("Failed to create area")),
            }
        }
        AreaProjectSubCommands::Delete {
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
                        format!("✅ Area '{path}' deleted successfully").green()
                    )
                })
                .map_err(|e| e.context("Failed to delete area"))
        }
        AreaProjectSubCommands::Update {
            project,
            path,
            name,
            child_id,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let node_path = NodePath::parse(path, &project, GROUP);
            update_area(&project, &node_path, name.as_deref(), *child_id)
                .await
                .map_err(|e| e.context("Failed to update area"))
        }
    }
}

/// Moves `child_id` under the area first, since renaming changes its path.
async fn update_area(
    project: &str,
    path: &NodePath,
    name: Option<&str>,
    child_id: Option<i32>,
) -> Result<()> {
    if let Some(child_id) = child_id {
        classification::move_node(project, GROUP, path, child_id).await?;
        println!(
            "{}",
            format!("✅ Moved area {child_id} under '{}'", path.logical(project)).green()
        );
    }

    let node = match name {
        Some(name) => classification::update_node(project, GROUP, path, Some(name), None).await?,
        None => classification::get_node(project, GROUP, path, None).await?,
    };

    println!("{}", "✅ Area updated successfully!".green());
    classification::display_node(GROUP, &node);
    Ok(())
}

async fn handle_team_command(subcommand: &AreaTeamSubCommands) -> Result<()> {
    match subcommand {
        AreaTeamSubCommands::List { project, team } => {
            let project = get_project_or_default(project.as_deref())?;
            get_team_areas(&project, team)
                .await
                .map(|areas| display_team_areas(team, &areas))
                .map_err(|e| e.context(format!("Failed to list the areas of team '{team}'")))
        }
        AreaTeamSubCommands::Add {
            project,
            team,
            path,
            include_sub_areas,
            set_as_default,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let path = NodePath::parse(path, &project, GROUP).logical(&project);
            change_team_areas(&project, team, |current| {
                add_team_area(
                    current,
                    &path,
                    include_sub_areas.unwrap_or(false),
                    *set_as_default,
                )
            })
            .await
            .map(|areas| {
                println!(
                    "{}",
                    format!("✅ Added area '{path}' to team '{team}'").green()
                );
                display_team_areas(team, &areas);
            })
            .map_err(|e| e.context(format!("Failed to add an area to team '{team}'")))
        }
        AreaTeamSubCommands::Remove {
            project,
            team,
            path,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let path = NodePath::parse(path, &project, GROUP).logical(&project);
            change_team_areas(&project, team, |current| remove_team_area(current, &path))
                .await
                .map(|areas| {
                    println!(
                        "{}",
                        format!("✅ Removed area '{path}' from team '{team}'").green()
                    );
                    display_team_areas(team, &areas);
                })
                .map_err(|e| e.context(format!("Failed to remove an area from team '{team}'")))
        }
        AreaTeamSubCommands::Update {
            project,
            team,
            path,
            include_sub_areas,
            set_as_default,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let path = NodePath::parse(path, &project, GROUP).logical(&project);
            change_team_areas(&project, team, |current| {
                update_team_area(current, &path, *include_sub_areas, *set_as_default)
            })
            .await
            .map(|areas| {
                println!(
                    "{}",
                    format!("✅ Updated area '{path}' of team '{team}'").green()
                );
                display_team_areas(team, &areas);
            })
            .map_err(|e| e.context(format!("Failed to update an area of team '{team}'")))
        }
    }
}

async fn get_team_areas(project: &str, team: &str) -> Result<TeamFieldValues> {
    let creds = get_credentials()?;
    Ok(create_work_client()?
        .teamfieldvalues_client()
        .get(creds.organization, project, team)
        .await?)
}

/// Reads a team's areas, changes them with `change` and writes them back.
async fn change_team_areas(
    project: &str,
    team: &str,
    change: impl FnOnce(&TeamFieldValues) -> Result<TeamFieldValuesPatch>,
) -> Result<TeamFieldValues> {
    let current = get_team_areas(project, team).await?;
    let patch = change(&current)?;

    let creds = get_credentials()?;
    Ok(create_work_client()?
        .teamfieldvalues_client()
        .update(creds.organization, patch, project, team)
        .await?)
}

fn position_of(values: &[TeamFieldValue], path: &str) -> Option<usize> {
    values.iter().position(|value| {
        value
            .value
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(path))
    })
}

fn is_default(current: &TeamFieldValues, path: &str) -> bool {
    current
        .default_value
        .as_deref()
        .is_some_and(|default| default.eq_ignore_ascii_case(path))
}

fn add_team_area(
    current: &TeamFieldValues,
    path: &str,
    include_sub_areas: bool,
    set_as_default: bool,
) -> Result<TeamFieldValuesPatch> {
    if position_of(&current.values, path).is_some() {
        return Err(anyhow!("'{path}' is already one of the team's areas"));
    }

    let mut values = current.values.clone();
    values.push(TeamFieldValue {
        include_children: Some(include_sub_areas),
        value: Some(path.to_string()),
    });

    // A team needs a default area, so the first one it gets becomes it.
    let default_value = if set_as_default || current.default_value.is_none() {
        Some(path.to_string())
    } else {
        current.default_value.clone()
    };

    Ok(TeamFieldValuesPatch {
        default_value,
        values,
    })
}

fn remove_team_area(current: &TeamFieldValues, path: &str) -> Result<TeamFieldValuesPatch> {
    let index = position_of(&current.values, path)
        .ok_or_else(|| anyhow!("'{path}' is not one of the team's areas"))?;

    if is_default(current, path) {
        return Err(anyhow!(
            "'{path}' is the team's default area; make another area the default before removing it"
        ));
    }

    let mut values = current.values.clone();
    values.remove(index);

    Ok(TeamFieldValuesPatch {
        default_value: current.default_value.clone(),
        values,
    })
}

fn update_team_area(
    current: &TeamFieldValues,
    path: &str,
    include_sub_areas: Option<bool>,
    set_as_default: bool,
) -> Result<TeamFieldValuesPatch> {
    let index = position_of(&current.values, path)
        .ok_or_else(|| anyhow!("'{path}' is not one of the team's areas"))?;

    let mut values = current.values.clone();
    if let Some(include_sub_areas) = include_sub_areas {
        values[index].include_children = Some(include_sub_areas);
    }

    let default_value = if set_as_default {
        values[index].value.clone()
    } else {
        current.default_value.clone()
    };

    Ok(TeamFieldValuesPatch {
        default_value,
        values,
    })
}

fn display_team_areas(team: &str, areas: &TeamFieldValues) {
    println!(
        "{}",
        format!("🗂️  Areas of team '{}'", escape_control_characters(team)).bold()
    );
    println!("{}", "=".repeat(80));

    if areas.values.is_empty() {
        println!("The team has no areas.");
        return;
    }

    println!("{:<60} {}", "Path".bold(), "Include sub-areas".bold());
    println!("{}", "-".repeat(80));

    for area in &areas.values {
        let path = area.value.as_deref().unwrap_or("-");
        let label = if is_default(areas, path) {
            format!("{} (default)", escape_control_characters(path))
        } else {
            escape_control_characters(path)
        };
        let include = if area.include_children.unwrap_or(false) {
            "yes"
        } else {
            "no"
        };
        println!("{label:<60} {include}");
    }

    println!("\n{} area(s)", areas.values.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[clap(subcommand)]
        command: AreaSubCommands,
    }

    fn parse(args: &[&str]) -> Result<AreaSubCommands, clap::Error> {
        TestCli::try_parse_from(std::iter::once("area").chain(args.iter().copied()))
            .map(|cli| cli.command)
    }

    fn area(path: &str, include_children: bool) -> TeamFieldValue {
        TeamFieldValue {
            include_children: Some(include_children),
            value: Some(path.to_string()),
        }
    }

    fn team_areas(default: &str, values: Vec<TeamFieldValue>) -> TeamFieldValues {
        TeamFieldValues {
            default_value: Some(default.to_string()),
            values,
            ..Default::default()
        }
    }

    #[test]
    fn project_list_defaults_to_one_level_from_the_root() {
        let AreaSubCommands::Project {
            subcommand: AreaProjectSubCommands::List { path, depth, .. },
        } = parse(&["project", "list"]).unwrap()
        else {
            panic!("expected project list");
        };
        assert_eq!(path, None);
        assert_eq!(depth, 1);
    }

    #[test]
    fn project_update_needs_a_change() {
        assert!(parse(&["project", "update", "--path", "Team A"]).is_err());
        assert!(parse(&["project", "update", "--path", "Team A", "--name", "Team B"]).is_ok());
        assert!(parse(&["project", "update", "--path", "Team A", "--child-id", "7"]).is_ok());
    }

    #[test]
    fn team_add_accepts_include_sub_areas_without_a_value() {
        let AreaSubCommands::Team {
            subcommand:
                AreaTeamSubCommands::Add {
                    include_sub_areas,
                    set_as_default,
                    ..
                },
        } = parse(&[
            "team",
            "add",
            "--team",
            "Team A",
            "--path",
            "Team A",
            "--include-sub-areas",
            "--set-as-default",
        ])
        .unwrap()
        else {
            panic!("expected team add");
        };
        assert_eq!(include_sub_areas, Some(true));
        assert!(set_as_default);
    }

    #[test]
    fn team_update_accepts_include_sub_areas_false() {
        let AreaSubCommands::Team {
            subcommand:
                AreaTeamSubCommands::Update {
                    include_sub_areas, ..
                },
        } = parse(&[
            "team",
            "update",
            "--team",
            "Team A",
            "--path",
            "Team A",
            "--include-sub-areas",
            "false",
        ])
        .unwrap()
        else {
            panic!("expected team update");
        };
        assert_eq!(include_sub_areas, Some(false));
    }

    #[test]
    fn team_update_needs_a_change() {
        assert!(parse(&["team", "update", "--team", "Team A", "--path", "Team A"]).is_err());
    }

    #[test]
    fn adding_an_area_keeps_the_existing_ones() {
        let current = team_areas("P\\A", vec![area("P\\A", false)]);
        let patch = add_team_area(&current, "P\\B", true, false).unwrap();

        assert_eq!(patch.default_value.as_deref(), Some("P\\A"));
        assert_eq!(patch.values, vec![area("P\\A", false), area("P\\B", true)]);
    }

    #[test]
    fn adding_an_area_can_make_it_the_default() {
        let current = team_areas("P\\A", vec![area("P\\A", false)]);
        let patch = add_team_area(&current, "P\\B", false, true).unwrap();
        assert_eq!(patch.default_value.as_deref(), Some("P\\B"));
    }

    #[test]
    fn the_first_area_becomes_the_default() {
        let patch = add_team_area(&TeamFieldValues::default(), "P\\A", false, false).unwrap();
        assert_eq!(patch.default_value.as_deref(), Some("P\\A"));
    }

    #[test]
    fn adding_an_area_twice_fails() {
        let current = team_areas("P\\A", vec![area("P\\A", false)]);
        assert!(add_team_area(&current, "p\\a", false, false).is_err());
    }

    #[test]
    fn removing_an_area_keeps_the_others() {
        let current = team_areas("P\\A", vec![area("P\\A", false), area("P\\B", true)]);
        let patch = remove_team_area(&current, "P\\B").unwrap();
        assert_eq!(patch.values, vec![area("P\\A", false)]);
        assert_eq!(patch.default_value.as_deref(), Some("P\\A"));
    }

    #[test]
    fn removing_the_default_area_fails() {
        let current = team_areas("P\\A", vec![area("P\\A", false), area("P\\B", true)]);
        assert!(remove_team_area(&current, "P\\A").is_err());
    }

    #[test]
    fn removing_an_unknown_area_fails() {
        let current = team_areas("P\\A", vec![area("P\\A", false)]);
        assert!(remove_team_area(&current, "P\\B").is_err());
    }

    #[test]
    fn updating_an_area_changes_only_what_was_asked() {
        let current = team_areas("P\\A", vec![area("P\\A", false), area("P\\B", false)]);

        let patch = update_team_area(&current, "P\\B", Some(true), false).unwrap();
        assert_eq!(patch.values, vec![area("P\\A", false), area("P\\B", true)]);
        assert_eq!(patch.default_value.as_deref(), Some("P\\A"));

        let patch = update_team_area(&current, "p\\b", None, true).unwrap();
        assert_eq!(patch.values, current.values);
        assert_eq!(patch.default_value.as_deref(), Some("P\\B"));
    }
}
