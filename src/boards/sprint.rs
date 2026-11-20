//! A team's current sprint at a glance: its work items by state, with effort
//! and remaining work totals.

use super::capacity;
use super::iteration;
use super::views::WorkItemList;
use super::EFFORT_FIELDS;
use crate::output::{self, md_table, OutputFormat, Render};
use crate::project::get_project_or_default;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::models::WorkItem;
use clap::Subcommand;
use serde::Serialize;
use std::io::{self, Write};

#[derive(Subcommand, Clone)]
pub enum SprintSubCommands {
    /// Show a team's current sprint: its work items by state, with effort and
    /// remaining work totals
    Show {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Team whose sprint to show (default: the project's default team)
        #[clap(long)]
        team: Option<String>,
    },
    /// Show each member's capacity for the rest of a team's current sprint
    /// against the remaining work assigned to them
    Capacity {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Team whose sprint to show (default: the project's default team)
        #[clap(long)]
        team: Option<String>,
    },
    /// Set a member's capacity, days off, or both, in a team's current
    /// sprint, then show the team's capacity
    #[clap(group(
        clap::ArgGroup::new("change")
            .required(true)
            .multiple(true)
            .args(["per_day", "days_off"])
    ))]
    SetCapacity {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Team whose sprint to change (default: the project's default team)
        #[clap(long)]
        team: Option<String>,
        /// The member: their email or unique name, display name, or @me
        #[clap(long)]
        member: String,
        /// Hours a day the member can work; replaces their activities
        #[clap(long)]
        per_day: Option<f64>,
        /// The activity the hours are for, such as Development
        #[clap(long, requires = "per_day")]
        activity: Option<String>,
        /// A day off, 2026-11-24, or a run of days off, 2026-11-24..2026-11-26;
        /// repeat for more. Replaces the member's days off
        #[clap(long = "day-off", value_parser = capacity::DaysOff::parse)]
        days_off: Vec<capacity::DaysOff>,
    },
}

pub(super) async fn handle_command(
    subcommand: &SprintSubCommands,
    format: OutputFormat,
) -> Result<()> {
    match subcommand {
        SprintSubCommands::Show { project, team } => {
            let project = get_project_or_default(project.as_deref())?;
            let (team, current) = iteration::current_iteration(&project, team.as_deref()).await?;
            let id = current.id.clone().unwrap_or_default();
            let mut items = iteration::iteration_work_items(&project, &team, &id).await?;
            sort_by_state(&mut items);
            let info = SprintInfo::from(&current);
            output::emit(&SprintOverview::new(&team, info, &items), format)
        }
        SprintSubCommands::Capacity { project, team } => {
            let project = get_project_or_default(project.as_deref())?;
            let (team, current) = iteration::current_iteration(&project, team.as_deref()).await?;
            show_capacity(&project, &team, &current, format).await
        }
        SprintSubCommands::SetCapacity {
            project,
            team,
            member,
            per_day,
            activity,
            days_off,
        } => {
            let project = get_project_or_default(project.as_deref())?;
            let (team, current) = iteration::current_iteration(&project, team.as_deref()).await?;
            let member = super::resolve_assignee(Some(member))
                .await?
                .unwrap_or_default();
            let members = capacity::team_members(&project, &team).await?;
            let member = capacity::find_member(&members, &member, &team)?;
            let id = current.id.clone().unwrap_or_default();
            let patch = capacity::capacity_patch(*per_day, activity.as_deref(), days_off);
            capacity::set_member_capacity(&project, &team, &id, &member.id, patch).await?;
            if format == OutputFormat::Text {
                println!(
                    "✅ Set the capacity of {} in {}\n",
                    member.name,
                    current.name.as_deref().unwrap_or_default()
                );
            }
            show_capacity(&project, &team, &current, format).await
        }
    }
}

/// Each member's capacity for the rest of the sprint `current`.
async fn show_capacity(
    project: &str,
    team: &str,
    current: &azure_devops_rust_api::work::models::TeamSettingsIteration,
    format: OutputFormat,
) -> Result<()> {
    let info = SprintInfo::from(current);
    let date = |date: &Option<String>| {
        date.as_deref()
            .and_then(|d| d.parse::<chrono::NaiveDate>().ok())
    };
    let (Some(start), Some(finish)) = (date(&info.start_date), date(&info.finish_date)) else {
        return Err(anyhow!(
            "Sprint '{}' has no start and finish dates, so its capacity cannot be counted",
            info.name
        ));
    };
    let id = current.id.clone().unwrap_or_default();
    let settings = capacity::team_capacity(project, team, &id).await?;
    let items = iteration::iteration_work_items(project, team, &id).await?;
    // Only the days still ahead count, today included, by the local
    // calendar so a working day is not dropped early.
    let today = chrono::Local::now().date_naive();
    let period = capacity::Period {
        from: start.max(today),
        to: finish,
        weekdays: &settings.weekdays,
        team_days_off: &settings.team_days_off,
    };
    let overview = capacity::CapacityOverview::new(team, info, &period, &settings.members, &items);
    output::emit(&overview, format)
}

/// The sprint an overview is about.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct SprintInfo {
    pub name: String,
    pub path: String,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
}

impl From<&azure_devops_rust_api::work::models::TeamSettingsIteration> for SprintInfo {
    fn from(iteration: &azure_devops_rust_api::work::models::TeamSettingsIteration) -> Self {
        let attributes = iteration.attributes.as_ref();
        Self {
            name: iteration.name.clone().unwrap_or_default(),
            path: iteration.path.clone().unwrap_or_default(),
            start_date: attributes
                .and_then(|a| a.start_date)
                .map(|date| date.date().to_string()),
            finish_date: attributes
                .and_then(|a| a.finish_date)
                .map(|date| date.date().to_string()),
        }
    }
}

/// A sprint's work items, grouped by state with totals.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SprintOverview<'a> {
    team: &'a str,
    iteration: SprintInfo,
    states: Vec<StateSummary>,
    total: StateSummary,
    work_items: &'a [WorkItem],
}

impl<'a> SprintOverview<'a> {
    pub fn new(team: &'a str, iteration: SprintInfo, work_items: &'a [WorkItem]) -> Self {
        let states = summarize(work_items);
        let total = totals(&states);
        Self {
            team,
            iteration,
            states,
            total,
            work_items,
        }
    }

    fn heading(&self) -> String {
        format!("{} ({})", self.iteration.name, self.iteration.path)
    }

    fn dates(&self) -> String {
        match (&self.iteration.start_date, &self.iteration.finish_date) {
            (None, None) => "No dates set".into(),
            (start, finish) => {
                let date = |date: &Option<String>| date.clone().unwrap_or_else(|| "?".into());
                format!("{} → {}", date(start), date(finish))
            }
        }
    }

    fn items(&self) -> WorkItemList<'_> {
        WorkItemList {
            heading: "Sprint Work Items",
            work_items: self.work_items,
        }
    }
}

/// Whole numbers without decimals, others as they are.
pub(super) fn amount(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

impl Render for SprintOverview<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "🏃 {}", self.heading())?;
        writeln!(w, "📅 {} · 👥 {}", self.dates(), self.team)?;
        writeln!(w)?;
        writeln!(
            w,
            "{:<20} {:>6} {:>8} {:>10}",
            "State", "Items", "Effort", "Remaining"
        )?;
        writeln!(w, "{}", "-".repeat(47))?;
        let row = |w: &mut dyn Write, row: &StateSummary| {
            writeln!(
                w,
                "{:<20} {:>6} {:>8} {:>10}",
                row.state,
                row.items,
                amount(row.effort),
                amount(row.remaining_work)
            )
        };
        for state in &self.states {
            row(w, state)?;
        }
        writeln!(w, "{}", "-".repeat(47))?;
        row(w, &self.total)?;
        self.items().text(w)
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "## {}\n", self.heading())?;
        writeln!(w, "{} · Team: {}\n", self.dates(), self.team)?;
        let mut rows: Vec<Vec<String>> = self
            .states
            .iter()
            .map(|row| {
                vec![
                    row.state.clone(),
                    row.items.to_string(),
                    amount(row.effort),
                    amount(row.remaining_work),
                ]
            })
            .collect();
        rows.push(vec![
            "**Total**".into(),
            self.total.items.to_string(),
            amount(self.total.effort),
            amount(self.total.remaining_work),
        ]);
        writeln!(
            w,
            "{}",
            md_table(&["State", "Items", "Effort", "Remaining work"], &rows)
        )?;
        self.items().markdown(w)
    }
}

/// States in the order the built-in processes move through them.
const STATE_ORDER: [&str; 14] = [
    "New",
    "Proposed",
    "To Do",
    "Approved",
    "Active",
    "Committed",
    "In Progress",
    "Doing",
    "Resolved",
    "Done",
    "Closed",
    "Completed",
    "Inactive",
    "Removed",
];

/// Work items in one state, with their effort and remaining work added up.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct StateSummary {
    pub state: String,
    pub items: usize,
    pub effort: f64,
    pub remaining_work: f64,
}

fn number(item: &WorkItem, field: &str) -> Option<f64> {
    item.fields.get(field).and_then(serde_json::Value::as_f64)
}

/// An item's size in whichever effort field its process uses.
fn effort(item: &WorkItem) -> f64 {
    EFFORT_FIELDS
        .iter()
        .find_map(|field| number(item, field))
        .unwrap_or(0.0)
}

/// Known states first in workflow order, then any others alphabetically.
fn state_rank(state: &str) -> (usize, String) {
    let known = STATE_ORDER
        .iter()
        .position(|known| known.eq_ignore_ascii_case(state))
        .unwrap_or(STATE_ORDER.len());
    (known, state.to_lowercase())
}

fn state_of(item: &WorkItem) -> &str {
    item.fields
        .get("System.State")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Unknown")
}

/// Orders the work items by state, in the same order as the summary; items
/// in one state keep their order.
pub(super) fn sort_by_state(items: &mut [WorkItem]) {
    items.sort_by_cached_key(|item| state_rank(state_of(item)));
}

/// The work items grouped by state.
pub(super) fn summarize(items: &[WorkItem]) -> Vec<StateSummary> {
    let mut summary: Vec<StateSummary> = Vec::new();
    for item in items {
        let state = state_of(item);
        let index = match summary.iter().position(|row| row.state == state) {
            Some(index) => index,
            None => {
                summary.push(StateSummary {
                    state: state.to_string(),
                    items: 0,
                    effort: 0.0,
                    remaining_work: 0.0,
                });
                summary.len() - 1
            }
        };
        let row = &mut summary[index];
        row.items += 1;
        row.effort += effort(item);
        row.remaining_work +=
            number(item, "Microsoft.VSTS.Scheduling.RemainingWork").unwrap_or(0.0);
    }
    summary.sort_by_key(|row| state_rank(&row.state));
    summary
}

/// Every state added together.
pub(super) fn totals(summary: &[StateSummary]) -> StateSummary {
    summary.iter().fold(
        StateSummary {
            state: "Total".into(),
            items: 0,
            effort: 0.0,
            remaining_work: 0.0,
        },
        |mut total, row| {
            total.items += row.items;
            total.effort += row.effort;
            total.remaining_work += row.remaining_work;
            total
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: i32, state: &str, fields: serde_json::Value) -> WorkItem {
        let mut value = json!({
            "id": id,
            "url": format!("https://dev.azure.com/org/_apis/wit/workItems/{id}"),
            "fields": {"System.State": state, "System.Title": format!("Item {id}")}
        });
        for (name, field) in fields.as_object().unwrap() {
            value["fields"][name] = field.clone();
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn summarize_groups_by_state_in_workflow_order() {
        let items = [
            item(1, "Done", json!({"Microsoft.VSTS.Scheduling.Effort": 3})),
            item(
                2,
                "New",
                json!({"Microsoft.VSTS.Scheduling.StoryPoints": 5.0}),
            ),
            item(
                3,
                "Committed",
                json!({"Microsoft.VSTS.Scheduling.RemainingWork": 4.5}),
            ),
            item(
                4,
                "New",
                json!({"Microsoft.VSTS.Scheduling.RemainingWork": 2}),
            ),
        ];
        assert_eq!(
            summarize(&items),
            [
                StateSummary {
                    state: "New".into(),
                    items: 2,
                    effort: 5.0,
                    remaining_work: 2.0
                },
                StateSummary {
                    state: "Committed".into(),
                    items: 1,
                    effort: 0.0,
                    remaining_work: 4.5
                },
                StateSummary {
                    state: "Done".into(),
                    items: 1,
                    effort: 3.0,
                    remaining_work: 0.0
                },
            ]
        );
    }

    #[test]
    fn items_sort_by_workflow_state_keeping_their_order_within_one() {
        let mut items = [
            item(1, "Done", json!({})),
            item(2, "New", json!({})),
            item(3, "Active", json!({})),
            item(4, "New", json!({})),
        ];
        sort_by_state(&mut items);
        let ids: Vec<_> = items.iter().map(|item| item.id).collect();
        assert_eq!(ids, [2, 4, 3, 1]);
    }

    #[test]
    fn unknown_states_follow_the_known_ones_alphabetically() {
        let items = [
            item(1, "Zeta", json!({})),
            item(2, "Closed", json!({})),
            item(3, "Alpha", json!({})),
        ];
        let states: Vec<_> = summarize(&items).into_iter().map(|s| s.state).collect();
        assert_eq!(states, ["Closed", "Alpha", "Zeta"]);
    }

    fn overview(items: &[WorkItem]) -> SprintOverview<'_> {
        SprintOverview::new(
            "Web Team",
            SprintInfo {
                name: "Sprint 3".into(),
                path: "Proj\\Sprint 3".into(),
                start_date: Some("2026-11-02".into()),
                finish_date: Some("2026-11-13".into()),
            },
            items,
        )
    }

    fn rendered(render: impl Fn(&mut Vec<u8>) -> std::io::Result<()>) -> String {
        let mut out = Vec::new();
        render(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn overview_text_shows_the_sprint_states_and_items() {
        let items = [
            item(1, "New", json!({"Microsoft.VSTS.Scheduling.Effort": 5})),
            item(
                2,
                "Done",
                json!({"Microsoft.VSTS.Scheduling.RemainingWork": 1.5}),
            ),
        ];
        let overview = overview(&items);
        let text = rendered(|w| overview.text(w));
        for expected in [
            "🏃 Sprint 3 (Proj\\Sprint 3)\n",
            "📅 2026-11-02 → 2026-11-13 · 👥 Web Team\n",
            &format!("{:<20} {:>6} {:>8} {:>10}\n", "New", 1, "5", "0"),
            &format!("{:<20} {:>6} {:>8} {:>10}\n", "Done", 1, "0", "1.5"),
            &format!("{:<20} {:>6} {:>8} {:>10}\n", "Total", 2, "5", "1.5"),
            "Item 2",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in\n{text}");
        }
    }

    #[test]
    fn overview_without_dates_says_so() {
        let mut overview = overview(&[]);
        overview.iteration.start_date = None;
        overview.iteration.finish_date = None;
        assert!(rendered(|w| overview.text(w)).contains(
            "📅 No dates set · 👥 Web Team
"
        ));
    }

    #[test]
    fn overview_markdown_has_a_state_table() {
        let items = [item(1, "New", json!({}))];
        let overview = overview(&items);
        let markdown = rendered(|w| overview.markdown(w));
        assert!(markdown.starts_with("## Sprint 3 (Proj\\Sprint 3)\n"));
        assert!(markdown.contains("| State | Items | Effort | Remaining work |\n"));
        assert!(markdown.contains("| **Total** | 1 | 0 | 0 |\n"));
    }

    #[test]
    fn overview_serializes_the_summary_and_items() {
        let items = [item(7, "New", json!({}))];
        let value = serde_json::to_value(overview(&items)).unwrap();
        assert_eq!(value["team"], "Web Team");
        assert_eq!(value["iteration"]["path"], "Proj\\Sprint 3");
        assert_eq!(value["iteration"]["startDate"], "2026-11-02");
        assert_eq!(value["states"][0]["state"], "New");
        assert_eq!(value["total"]["items"], 1);
        assert_eq!(value["workItems"][0]["id"], 7);
    }

    #[test]
    fn totals_add_up_every_state() {
        let summary = [
            StateSummary {
                state: "New".into(),
                items: 2,
                effort: 5.0,
                remaining_work: 2.0,
            },
            StateSummary {
                state: "Done".into(),
                items: 1,
                effort: 3.0,
                remaining_work: 0.5,
            },
        ];
        assert_eq!(
            totals(&summary),
            StateSummary {
                state: "Total".into(),
                items: 3,
                effort: 8.0,
                remaining_work: 2.5
            }
        );
    }
}
