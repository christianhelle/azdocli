//! A team's product backlog in backlog order, and moving items within it.

use super::iteration::{self, WORK_ITEMS_PER_REQUEST};
use super::{create_wit_client, create_work_client, EFFORT_FIELDS};
use crate::auth::get_credentials;
use crate::connection::Connection;
use crate::output::{md_table, Render};
use crate::text::escape_control_characters;
use anyhow::{anyhow, bail, Result};
use azure_devops_rust_api::wit::models::WorkItem;
use azure_devops_rust_api::work::models::ReorderOperation;
use clap::Subcommand;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

/// The backlog level of requirements: product backlog items, user stories or
/// requirements, depending on the process.
const REQUIREMENT_BACKLOG: &str = "Microsoft.RequirementCategory";

/// State categories of work items that are finished and no longer on the
/// backlog.
const FINISHED_CATEGORIES: [&str; 2] = ["Completed", "Removed"];

/// The (work item type, state) pairs, lowercased, in a finished category.
type FinishedStates = HashSet<(String, String)>;

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
    conn: &Connection,
    subcommand: &BacklogSubCommands,
    format: crate::output::OutputFormat,
) -> Result<()> {
    match subcommand {
        BacklogSubCommands::List { project, team, top } => {
            let project = crate::project::get_project_or_default(project.as_deref())?;
            let team = team_or_default(conn, &project, team.as_deref()).await?;
            let mut items = backlog_items(conn, &project, &team).await?;
            if let Some(top) = top {
                items.truncate(*top as usize);
            }
            crate::output::emit(&BacklogList::new(&team, &items), format)
        }
    }
}

pub(super) async fn team_or_default(
    conn: &Connection,
    project: &str,
    team: Option<&str>,
) -> Result<String> {
    match team {
        Some(team) => Ok(team.to_string()),
        None => iteration::default_team(conn, project).await,
    }
}

/// The open work items on a team's requirement backlog, in backlog order.
pub(super) async fn backlog_items(
    conn: &Connection,
    project: &str,
    team: &str,
) -> Result<Vec<WorkItem>> {
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
    let finished = finished_states(conn, project, &fetched).await?;
    Ok(in_backlog_order(&ids, fetched, &finished))
}

/// The finished states of every work item type in `items`, by state category.
async fn finished_states(
    conn: &Connection,
    project: &str,
    items: &[WorkItem],
) -> Result<FinishedStates> {
    let mut types: Vec<&str> = items
        .iter()
        .filter_map(|item| field_str(item, "System.WorkItemType"))
        .collect();
    types.sort_unstable();
    types.dedup();
    let mut finished = FinishedStates::new();
    for work_item_type in types {
        for state in super::list_work_item_states(conn, project, work_item_type).await? {
            let category = state.category.as_deref().unwrap_or_default();
            if FINISHED_CATEGORIES
                .iter()
                .any(|c| c.eq_ignore_ascii_case(category))
            {
                finished.insert(finished_key(
                    work_item_type,
                    state.name.as_deref().unwrap_or_default(),
                ));
            }
        }
    }
    Ok(finished)
}

fn finished_key(work_item_type: &str, state: &str) -> (String, String) {
    (work_item_type.to_lowercase(), state.to_lowercase())
}

/// `items` arranged as `order` says, without the finished ones.
fn in_backlog_order(
    order: &[i32],
    mut items: Vec<WorkItem>,
    finished: &FinishedStates,
) -> Vec<WorkItem> {
    items.retain(|item| !is_finished(item, finished));
    let positions: HashMap<i32, usize> = order
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    items.sort_by_key(|item| positions.get(&item.id).copied().unwrap_or(usize::MAX));
    items
}

fn field_str<'a>(item: &'a WorkItem, field: &str) -> Option<&'a str> {
    item.fields.get(field).and_then(serde_json::Value::as_str)
}

fn is_finished(item: &WorkItem, finished: &FinishedStates) -> bool {
    match (
        field_str(item, "System.WorkItemType"),
        field_str(item, "System.State"),
    ) {
        (Some(work_item_type), Some(state)) => {
            finished.contains(&finished_key(work_item_type, state))
        }
        _ => false,
    }
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
                "{:>4} {:<8} {:<21} {:<12} {:>6} {:<20} {}",
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

/// Where `work-item move` puts an item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Placement {
    Top,
    Before(i32),
    After(i32),
}

/// The `(previous_id, next_id)` neighbours the reorder API wants to put `id`
/// at `placement` in the backlog `order`; 0 means the start or end.
pub(super) fn neighbours(order: &[i32], id: i32, placement: Placement) -> Result<(i32, i32)> {
    if let Placement::Before(target) | Placement::After(target) = placement {
        if target == id {
            bail!("Cannot move work item {id} relative to itself");
        }
        if !order.contains(&target) {
            bail!("Work item {target} is not on the backlog");
        }
    }
    if !order.contains(&id) {
        bail!("Work item {id} is not on the backlog");
    }
    let others: Vec<i32> = order.iter().copied().filter(|other| *other != id).collect();
    let at = |index: usize| others.get(index).copied().unwrap_or(0);
    let position = |target: i32| others.iter().position(|other| *other == target);
    Ok(match placement {
        Placement::Top => (0, at(0)),
        Placement::Before(target) => {
            let index = position(target).unwrap_or(0);
            (index.checked_sub(1).map_or(0, at), target)
        }
        Placement::After(target) => (target, at(position(target).unwrap_or(0) + 1)),
    })
}

/// The result of a move.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MoveResult<'a> {
    team: &'a str,
    position: usize,
    total: usize,
    work_item: &'a WorkItem,
}

impl Render for MoveResult<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(
            w,
            "✅ Moved work item {} '{}' to position {} of {} in the backlog of {}",
            self.work_item.id,
            escape_control_characters(field_str(self.work_item, "System.Title").unwrap_or("")),
            self.position,
            self.total,
            escape_control_characters(self.team)
        )
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(
            w,
            "Moved work item **{}** to position {} of {} in the backlog of {}",
            self.work_item.id,
            self.position,
            self.total,
            escape_control_characters(self.team)
        )
    }
}

/// Moves work item `id` within the team's backlog, then reports its position.
pub(super) async fn move_item(
    conn: &Connection,
    project: &str,
    team: Option<&str>,
    id: i32,
    placement: Placement,
    format: crate::output::OutputFormat,
) -> Result<()> {
    let team = team_or_default(conn, project, team).await?;
    let order: Vec<i32> = backlog_items(conn, project, &team)
        .await?
        .iter()
        .map(|item| item.id)
        .collect();
    let (previous, next) = neighbours(&order, id, placement)?;
    let creds = get_credentials()?;
    let mut body = ReorderOperation::new();
    body.ids = vec![id];
    body.previous_id = Some(previous);
    body.next_id = Some(next);
    body.parent_id = Some(0);
    create_work_client()?
        .workitemsorder_client()
        .reorder_backlog_work_items(creds.organization, body, project, &team)
        .await
        .map_err(|e| anyhow!("Failed to move work item {id}: {e}"))?;
    let items = backlog_items(conn, project, &team).await?;
    let index = items
        .iter()
        .position(|item| item.id == id)
        .ok_or_else(|| anyhow!("Work item {id} is no longer on the backlog"))?;
    crate::output::emit(
        &MoveResult {
            team: &team,
            position: index + 1,
            total: items.len(),
            work_item: &items[index],
        },
        format,
    )
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
    fn items_follow_the_backlog_order_without_finished_states() {
        let items = vec![
            item(1, "New", json!({})),
            item(2, "Closed", json!({})),
            item(3, "Active", json!({})),
            item(4, "Removed", json!({})),
        ];
        let finished: FinishedStates = [
            ("user story".to_string(), "closed".to_string()),
            ("user story".to_string(), "removed".to_string()),
        ]
        .into();
        let ordered = in_backlog_order(&[3, 2, 4, 1], items, &finished);
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

    #[test]
    fn top_goes_before_the_first_item() {
        assert_eq!(neighbours(&[1, 2, 3], 3, Placement::Top).unwrap(), (0, 1));
    }

    #[test]
    fn before_uses_the_item_above_the_target() {
        assert_eq!(
            neighbours(&[1, 2, 3], 3, Placement::Before(2)).unwrap(),
            (1, 2)
        );
        assert_eq!(
            neighbours(&[1, 2, 3], 3, Placement::Before(1)).unwrap(),
            (0, 1)
        );
    }

    #[test]
    fn before_skips_the_moved_item_when_it_is_above_the_target() {
        assert_eq!(
            neighbours(&[1, 2, 3], 1, Placement::Before(3)).unwrap(),
            (2, 3)
        );
    }

    #[test]
    fn after_uses_the_item_below_the_target() {
        assert_eq!(
            neighbours(&[1, 2, 3], 1, Placement::After(2)).unwrap(),
            (2, 3)
        );
        assert_eq!(
            neighbours(&[1, 2, 3], 1, Placement::After(3)).unwrap(),
            (3, 0)
        );
    }

    #[test]
    fn after_skips_the_moved_item_when_it_is_below_the_target() {
        assert_eq!(
            neighbours(&[1, 2, 3], 3, Placement::After(1)).unwrap(),
            (1, 2)
        );
    }

    #[test]
    fn moving_relative_to_itself_or_unknown_items_is_refused() {
        assert!(neighbours(&[1, 2], 1, Placement::Before(1)).is_err());
        assert!(neighbours(&[1, 2], 1, Placement::After(9)).is_err());
        assert!(neighbours(&[1, 2], 9, Placement::Top).is_err());
    }

    #[test]
    fn move_result_serializes_position_and_item() {
        let items = [item(7, "New", json!({}))];
        let result = MoveResult {
            team: "T",
            position: 2,
            total: 5,
            work_item: &items[0],
        };
        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(value["position"], 2);
        assert_eq!(value["workItem"]["id"], 7);
        let text = rendered(|w| result.text(w));
        assert!(text.contains("to position 2 of 5"), "{text}");
    }
}
