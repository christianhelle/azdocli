//! A team's product backlog in backlog order, and moving items within it.

use super::iteration::{self, WORK_ITEMS_PER_REQUEST};
use super::{create_wit_client, create_work_client, EFFORT_FIELDS};
use crate::auth::get_credentials;
use crate::output::{md_table, Render};
use crate::text::escape_control_characters;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::models::WorkItem;
use clap::Subcommand;
use serde::Serialize;
use std::io::{self, Write};

/// The backlog level of requirements: product backlog items, user stories or
/// requirements, depending on the process.
const REQUIREMENT_BACKLOG: &str = "Microsoft.RequirementCategory";

/// States of work items that are finished and no longer on the backlog.
const FINISHED_STATES: [&str; 4] = ["Closed", "Done", "Completed", "Removed"];

#[derive(Subcommand, Clone)]
pub enum BacklogSubCommands {
    /// List a team's product backlog in backlog order
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Team whose backlog to list (default: the project's default team)
        #[clap(long)]
        team: Option<String>,
        /// Only list the first N items of the backlog
        #[clap(long, value_parser = clap::value_parser!(i32).range(1..))]
        top: Option<i32>,
    },
}

pub(super) async fn handle_command(
    subcommand: &BacklogSubCommands,
    format: crate::output::OutputFormat,
) -> Result<()> {
    match subcommand {
        BacklogSubCommands::List { project, team, top } => {
            let project = crate::project::get_project_or_default(project.as_deref())?;
            let team = team_or_default(&project, team.as_deref()).await?;
            let mut items = backlog_items(&project, &team).await?;
            if let Some(top) = top {
                items.truncate(*top as usize);
            }
            crate::output::emit(&BacklogList::new(&team, &items), format)
        }
    }
}

pub(super) async fn team_or_default(project: &str, team: Option<&str>) -> Result<String> {
    match team {
        Some(team) => Ok(team.to_string()),
        None => iteration::default_team(project).await,
    }
}

/// The open work items on a team's requirement backlog, in backlog order.
pub(super) async fn backlog_items(project: &str, team: &str) -> Result<Vec<WorkItem>> {
    let creds = get_credentials()?;
    let links = create_work_client()?
        .backlogs_client()
        .get_backlog_level_work_items(
            creds.organization.clone(),
            project,
            team,
            REQUIREMENT_BACKLOG,
        )
        .await
        .map_err(|e| anyhow!("Failed to get the backlog of team '{team}': {e}"))?
        .work_items;
    let ids: Vec<i32> = links
        .iter()
        .filter_map(|link| link.target.as_ref()?.id)
        .collect();

    let client = create_wit_client()?;
    let mut fetched = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(WORK_ITEMS_PER_REQUEST) {
        let chunk = chunk
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        fetched.extend(
            client
                .work_items_client()
                .list(creds.organization.clone(), chunk, project)
                .await?
                .value,
        );
    }
    Ok(in_backlog_order(&ids, fetched))
}

/// `items` arranged as `order` says, without the finished ones.
fn in_backlog_order(order: &[i32], mut items: Vec<WorkItem>) -> Vec<WorkItem> {
    items.retain(|item| !is_finished(item));
    items.sort_by_key(|item| {
        order
            .iter()
            .position(|id| *id == item.id)
            .unwrap_or(usize::MAX)
    });
    items
}

fn field_str<'a>(item: &'a WorkItem, field: &str) -> Option<&'a str> {
    item.fields.get(field).and_then(serde_json::Value::as_str)
}

fn is_finished(item: &WorkItem) -> bool {
    field_str(item, "System.State").is_some_and(|state| {
        FINISHED_STATES
            .iter()
            .any(|s| s.eq_ignore_ascii_case(state))
    })
}

fn assignee(item: &WorkItem) -> &str {
    let value = item.fields.get("System.AssignedTo");
    value
        .and_then(|v| v.get("displayName").or(Some(v)))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

fn effort(item: &WorkItem) -> String {
    EFFORT_FIELDS
        .iter()
        .find_map(|field| item.fields.get(*field).and_then(serde_json::Value::as_f64))
        .map(super::sprint::amount)
        .unwrap_or_default()
}

/// A team's backlog, in backlog order.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BacklogList<'a> {
    team: &'a str,
    work_items: &'a [WorkItem],
}

impl<'a> BacklogList<'a> {
    pub fn new(team: &'a str, work_items: &'a [WorkItem]) -> Self {
        Self { team, work_items }
    }

    fn rows(&self) -> Vec<Vec<String>> {
        self.work_items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                vec![
                    (index + 1).to_string(),
                    item.id.to_string(),
                    field_str(item, "System.WorkItemType")
                        .unwrap_or("Unknown")
                        .to_string(),
                    field_str(item, "System.State")
                        .unwrap_or("Unknown")
                        .to_string(),
                    effort(item),
                    assignee(item).to_string(),
                    field_str(item, "System.Title")
                        .unwrap_or("No Title")
                        .to_string(),
                ]
            })
            .collect()
    }
}

const HEADERS: [&str; 7] = ["#", "ID", "Type", "State", "Effort", "Assigned to", "Title"];

impl Render for BacklogList<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(
            w,
            "📋 Backlog of {} ({} items)",
            escape_control_characters(self.team),
            self.work_items.len()
        )?;
        writeln!(w, "{}", "=".repeat(100))?;
        let row = |w: &mut dyn Write, cells: &[String]| {
            writeln!(
                w,
                "{:>4} {:<8} {:<20} {:<12} {:>6} {:<20} {}",
                cells[0],
                cells[1],
                escape_control_characters(&cells[2]),
                escape_control_characters(&cells[3]),
                cells[4],
                escape_control_characters(&cells[5]),
                escape_control_characters(&cells[6]),
            )
        };
        row(w, &HEADERS.map(String::from))?;
        writeln!(w, "{}", "-".repeat(100))?;
        if self.work_items.is_empty() {
            writeln!(w, "No work items found.")?;
        }
        for cells in self.rows() {
            row(w, &cells)?;
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(
            w,
            "## Backlog of {} ({} items)\n",
            escape_control_characters(self.team),
            self.work_items.len()
        )?;
        write!(w, "{}", md_table(&HEADERS, &self.rows()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(super) fn item(id: i32, state: &str, extra: serde_json::Value) -> WorkItem {
        let mut value = json!({
            "id": id,
            "url": format!("https://dev.azure.com/org/_apis/wit/workItems/{id}"),
            "fields": {
                "System.State": state,
                "System.Title": format!("Item {id}"),
                "System.WorkItemType": "User Story"
            }
        });
        for (name, field) in extra.as_object().unwrap() {
            value["fields"][name] = field.clone();
        }
        serde_json::from_value(value).unwrap()
    }

    fn rendered(render: impl Fn(&mut Vec<u8>) -> io::Result<()>) -> String {
        let mut out = Vec::new();
        render(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn items_follow_the_backlog_order_without_finished_ones() {
        let items = vec![
            item(1, "New", json!({})),
            item(2, "Closed", json!({})),
            item(3, "Active", json!({})),
            item(4, "Removed", json!({})),
        ];
        let ordered = in_backlog_order(&[3, 2, 4, 1], items);
        let ids: Vec<_> = ordered.iter().map(|i| i.id).collect();
        assert_eq!(ids, [3, 1]);
    }

    #[test]
    fn text_shows_position_effort_and_assignee() {
        let items = [item(
            7,
            "Active",
            json!({
                "Microsoft.VSTS.Scheduling.StoryPoints": 5.0,
                "System.AssignedTo": {"displayName": "Ada Lovelace"}
            }),
        )];
        let list = BacklogList::new("Web Team", &items);
        let text = rendered(|w| list.text(w));
        assert!(text.contains("Backlog of Web Team (1 items)"), "{text}");
        let line = text.lines().find(|l| l.contains("Item 7")).unwrap();
        for part in ["1", "7", "User Story", "Active", "5", "Ada Lovelace"] {
            assert!(line.contains(part), "missing {part} in {line}");
        }
    }

    #[test]
    fn text_escapes_control_characters_in_titles() {
        let mut items = [item(1, "New", json!({}))];
        items[0].fields["System.Title"] = json!("bad\u{1b}[31m");
        let list = BacklogList::new("Web Team", &items);
        assert!(!rendered(|w| list.text(w)).contains('\u{1b}'));
    }

    #[test]
    fn markdown_has_a_table() {
        let items = [item(1, "New", json!({}))];
        let list = BacklogList::new("Web Team", &items);
        let markdown = rendered(|w| list.markdown(w));
        assert!(markdown.starts_with("## Backlog of Web Team (1 items)\n"));
        assert!(markdown.contains("| # | ID | Type | State | Effort | Assigned to | Title |\n"));
        assert!(markdown.contains("| 1 | 1 | User Story | New |  |  | Item 1 |\n"));
    }

    #[test]
    fn json_has_team_and_work_items() {
        let items = [item(7, "New", json!({}))];
        let value = serde_json::to_value(BacklogList::new("Web Team", &items)).unwrap();
        assert_eq!(value["team"], "Web Team");
        assert_eq!(value["workItems"][0]["id"], 7);
    }
}
