//! A team's capacity for the rest of its current sprint, set against the
//! remaining work assigned to each member.

use super::create_work_client;
use super::sprint::{amount, SprintInfo};
use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::output::{md_table, Render};
use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::models::WorkItem;
use azure_devops_rust_api::work::models::{
    Activity, CapacityPatch, DateRange, TeamSettingsDaysOffPatch,
};
use chrono::{Datelike, NaiveDate, Weekday};
use serde::Serialize;
use std::io::{self, Write};

fn days_off(ranges: &[DateRange]) -> Vec<DaysOff> {
    ranges
        .iter()
        .filter_map(|range| {
            // The day each API date falls on.
            let start = range.start?;
            let end = range.end?;
            Some(DaysOff {
                start: NaiveDate::from_yo_opt(start.year(), u32::from(start.ordinal()))?,
                end: NaiveDate::from_yo_opt(end.year(), u32::from(end.ordinal()))?,
            })
        })
        .collect()
}

/// The team's working days, as the team settings name them; Monday to
/// Friday when none are set.
pub(super) fn parse_weekdays(names: &[serde_json::Value]) -> Vec<Weekday> {
    let days: Vec<Weekday> = names
        .iter()
        .filter_map(|name| name.as_str()?.parse().ok())
        .collect();
    if days.is_empty() {
        vec![
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ]
    } else {
        days
    }
}

/// What the team settings say about a sprint's capacity.
pub(super) struct TeamCapacity {
    pub members: Vec<Member>,
    pub weekdays: Vec<Weekday>,
    pub team_days_off: Vec<DaysOff>,
}

pub(super) async fn team_capacity(
    project: &str,
    team: &str,
    iteration_id: &str,
) -> Result<TeamCapacity> {
    let creds = get_credentials()?;
    let client = create_work_client()?;
    let capacity = client
        .capacities_client()
        .get_capacities_with_identity_ref_and_totals(
            creds.organization.clone(),
            project,
            iteration_id,
            team,
        )
        .await
        .map_err(|e| anyhow!("Failed to read the capacity of team '{team}': {e}"))?;
    let team_days_off = client
        .teamdaysoff_client()
        .get(creds.organization.clone(), project, iteration_id, team)
        .await?
        .days_off;
    let settings = client
        .teamsettings_client()
        .get(creds.organization.clone(), project, team)
        .await?;
    let members = capacity
        .team_members
        .iter()
        .filter_map(|member| {
            let identity = member.team_member.as_ref()?;
            let unique_name = identity.unique_name.clone()?;
            let name = identity
                .graph_subject_base
                .display_name
                .clone()
                .unwrap_or_else(|| unique_name.clone());
            let base = &member.capacity_contract_base;
            Some(Member {
                name,
                unique_name,
                capacity_per_day: base
                    .activities
                    .iter()
                    .filter_map(|activity| activity.capacity_per_day)
                    .map(f64::from)
                    .sum(),
                days_off: days_off(&base.days_off),
            })
        })
        .collect();
    Ok(TeamCapacity {
        members,
        weekdays: parse_weekdays(&settings.working_days),
        team_days_off: days_off(&team_days_off),
    })
}

/// A member of a team, as capacity is set for them.
#[derive(Debug, PartialEq)]
pub(super) struct TeamMember {
    pub id: String,
    pub unique_name: String,
    pub name: String,
}

pub(super) async fn team_members(project: &str, team: &str) -> Result<Vec<TeamMember>> {
    let creds = get_credentials()?;
    let members = CredentialClientFactory::new(&creds)?
        .build_core()
        .teams_client()
        .get_team_members_with_extended_properties(creds.organization.clone(), project, team)
        .await
        .map_err(|e| anyhow!("Failed to list the members of team '{team}': {e}"))?
        .value;
    Ok(members
        .into_iter()
        .filter_map(|member| {
            let identity = member.identity?;
            let unique_name = identity.unique_name?;
            Some(TeamMember {
                id: identity.id?,
                name: identity
                    .graph_subject_base
                    .display_name
                    .unwrap_or_else(|| unique_name.clone()),
                unique_name,
            })
        })
        .collect())
}

/// The member `wanted` names, by unique name (such as an email address) or
/// else by display name, ignoring case.
pub(super) fn find_member<'a>(
    members: &'a [TeamMember],
    wanted: &str,
    team: &str,
) -> Result<&'a TeamMember> {
    // Names compare by their lowercase form, which folds beyond ASCII.
    let key = wanted.to_lowercase();
    if let Some(member) = members.iter().find(|m| m.unique_name.to_lowercase() == key) {
        return Ok(member);
    }
    let named: Vec<_> = members
        .iter()
        .filter(|m| m.name.to_lowercase() == key)
        .collect();
    match named.as_slice() {
        [member] => Ok(member),
        [] => Err(anyhow!(
            "Team '{team}' has no member '{wanted}'; its members are {}",
            members
                .iter()
                .map(|m| format!("{} ({})", m.name, m.unique_name))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        several => Err(anyhow!(
            "Several members of team '{team}' are called '{wanted}'; use one of {}",
            several
                .iter()
                .map(|m| m.unique_name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Reads hours a day, from 0 to 24, so the value survives the API's f32.
pub(crate) fn parse_hours_a_day(value: &str) -> Result<f64, String> {
    match value.trim().parse::<f64>() {
        Ok(hours) if (0.0..=24.0).contains(&hours) => Ok(hours),
        _ => Err(format!("'{value}' is not a number of hours from 0 to 24")),
    }
}

/// The change to a member's capacity: hours a day for one activity, days
/// off, or both. What is not given is left as it is.
pub(super) fn capacity_patch(
    per_day: Option<f64>,
    activity: Option<&str>,
    days_off: &[DaysOff],
) -> CapacityPatch {
    let mut patch = CapacityPatch::new();
    if let Some(per_day) = per_day {
        let mut work = Activity::new();
        // The API holds capacity per day as f32.
        work.capacity_per_day = Some(per_day as f32);
        work.name = Some(activity.unwrap_or_default().to_string());
        patch.activities = vec![work];
    }
    patch.days_off = days_off.iter().map(|off| off.to_date_range()).collect();
    patch
}

/// The team's days off, replacing the ones set before.
pub(super) fn team_days_off_patch(days_off: &[DaysOff]) -> TeamSettingsDaysOffPatch {
    let mut patch = TeamSettingsDaysOffPatch::new();
    patch.days_off = days_off.iter().map(|off| off.to_date_range()).collect();
    patch
}

pub(super) async fn set_team_days_off(
    project: &str,
    team: &str,
    iteration_id: &str,
    days_off: &[DaysOff],
) -> Result<()> {
    let creds = get_credentials()?;
    create_work_client()?
        .teamdaysoff_client()
        .update(
            creds.organization.clone(),
            team_days_off_patch(days_off),
            project,
            iteration_id,
            team,
        )
        .await
        .map_err(|e| anyhow!("Failed to set the days off of team '{team}': {e}"))?;
    Ok(())
}

pub(super) async fn set_member_capacity(
    project: &str,
    team: &str,
    iteration_id: &str,
    member_id: &str,
    patch: CapacityPatch,
) -> Result<()> {
    let creds = get_credentials()?;
    create_work_client()?
        .capacities_client()
        .update(
            creds.organization.clone(),
            patch,
            project,
            iteration_id,
            member_id,
            team,
        )
        .await
        .map_err(|e| anyhow!("Failed to set the capacity: {e}"))?;
    Ok(())
}

/// A team's capacity for what is left of its sprint.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CapacityOverview<'a> {
    pub team: &'a str,
    pub iteration: SprintInfo,
    /// The first and last day counted.
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// The team's working days left, less the team's days off but before
    /// anyone's own.
    pub working_days: u32,
    #[serde(flatten)]
    pub plan: CapacityPlan,
    pub total: MemberCapacity,
}

impl<'a> CapacityOverview<'a> {
    pub fn new(
        team: &'a str,
        iteration: SprintInfo,
        period: &Period,
        members: &[Member],
        items: &[WorkItem],
    ) -> Self {
        let plan = plan(members, period, items);
        let mut total = plan.members.iter().fold(
            MemberCapacity {
                name: "Total".into(),
                capacity_per_day: 0.0,
                days: 0,
                capacity: 0.0,
                assigned: 0.0,
                left: 0.0,
            },
            |mut total, row| {
                total.capacity_per_day += row.capacity_per_day;
                total.capacity += row.capacity;
                total.assigned += row.assigned;
                total
            },
        );
        total.capacity_per_day = hours(total.capacity_per_day);
        total.capacity = hours(total.capacity);
        total.assigned = hours(total.assigned);
        total.left = hours(total.capacity - total.assigned);
        Self {
            team,
            iteration,
            from: period.from,
            to: period.to,
            working_days: working_days(
                period.from,
                period.to,
                period.weekdays,
                period.team_days_off,
            ),
            plan,
            total,
        }
    }

    fn heading(&self) -> String {
        format!("{} ({})", self.iteration.name, self.iteration.path)
    }

    fn period(&self) -> String {
        format!(
            "{} → {} · {} working day(s) left",
            self.from, self.to, self.working_days
        )
    }

    /// Each member, the total, then the work no member's capacity covers.
    fn rows(&self) -> Vec<[String; 6]> {
        let member = |row: &MemberCapacity| {
            [
                row.name.clone(),
                amount(row.capacity_per_day),
                row.days.to_string(),
                amount(row.capacity),
                amount(row.assigned),
                left(row.left),
            ]
        };
        let mut rows: Vec<_> = self.plan.members.iter().map(member).collect();
        let mut total = member(&self.total);
        // Days differ per member, so they do not add up.
        total[2] = String::new();
        rows.push(total);
        let others = &self.plan.other_work;
        rows.extend(others.iter().map(|other| {
            let shared = |name: &String| {
                others
                    .iter()
                    .filter(|o| o.assigned_to.as_ref() == Some(name))
                    .count()
                    > 1
            };
            let name = match (&other.assigned_to, &other.unique_name) {
                // People with the same name are told apart by unique name.
                (Some(name), Some(unique_name)) if shared(name) => {
                    format!("{name} <{unique_name}> (no capacity)")
                }
                (Some(name), _) => format!("{name} (no capacity)"),
                (None, _) => "Unassigned".into(),
            };
            let blank = String::new;
            [
                name,
                blank(),
                blank(),
                blank(),
                amount(other.remaining_work),
                blank(),
            ]
        }));
        rows
    }
}

/// Capacity left, flagged when the assigned work is more than the capacity.
fn left(value: f64) -> String {
    if value < 0.0 {
        format!("⚠️ {}", amount(value))
    } else {
        amount(value)
    }
}

const HEADERS: [&str; 6] = ["Member", "Per day", "Days", "Capacity", "Assigned", "Left"];

impl Render for CapacityOverview<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "🏃 {}", self.heading())?;
        writeln!(w, "📅 {} · 👥 {}", self.period(), self.team)?;
        writeln!(w)?;
        if self.plan.members.is_empty() {
            writeln!(w, "No capacity is set for this sprint.\n")?;
        }
        let rows = self.rows();
        // The name column fits the longest name.
        let width = rows
            .iter()
            .map(|row| row[0].chars().count())
            .max()
            .unwrap_or(0)
            .max(30);
        let line = |w: &mut dyn Write, row: &[String]| {
            writeln!(
                w,
                "{:<width$} {:>8} {:>5} {:>9} {:>9} {:>9}",
                row[0], row[1], row[2], row[3], row[4], row[5]
            )
        };
        let rule = "-".repeat(width + 45);
        line(w, &HEADERS.map(String::from))?;
        writeln!(w, "{rule}")?;
        let members = self.plan.members.len();
        for (index, row) in rows.iter().enumerate() {
            // Rules set off the total, between the members and other work.
            if (index == members && members > 0) || index == members + 1 {
                writeln!(w, "{rule}")?;
            }
            line(w, row)?;
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "## {}\n", self.heading())?;
        writeln!(w, "{} · Team: {}\n", self.period(), self.team)?;
        if self.plan.members.is_empty() {
            writeln!(w, "No capacity is set for this sprint.\n")?;
        }
        let mut rows: Vec<Vec<String>> = self.rows().into_iter().map(Vec::from).collect();
        let total = self.plan.members.len();
        rows[total][0] = "**Total**".into();
        writeln!(w, "{}", md_table(&HEADERS, &rows))
    }
}

/// A team member's capacity settings for a sprint.
#[derive(Debug)]
pub(super) struct Member {
    pub name: String,
    pub unique_name: String,
    /// Hours a day, all activities added together.
    pub capacity_per_day: f64,
    pub days_off: Vec<DaysOff>,
}

/// One member's capacity for the days left, against the remaining work
/// assigned to them.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct MemberCapacity {
    pub name: String,
    pub capacity_per_day: f64,
    /// Working days left that the member is not off.
    pub days: u32,
    pub capacity: f64,
    pub assigned: f64,
    /// Capacity not yet taken by assigned work; negative when over.
    pub left: f64,
}

/// Remaining work in the sprint assigned to someone without capacity on the
/// team, or to no one.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct OtherWork {
    /// The assignee's display name; `None` for unassigned work.
    pub assigned_to: Option<String>,
    /// The assignee's unique name, which tells apart people with the same
    /// display name.
    pub unique_name: Option<String>,
    pub remaining_work: f64,
}

/// Every member's capacity and the work the members do not cover.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct CapacityPlan {
    pub members: Vec<MemberCapacity>,
    pub other_work: Vec<OtherWork>,
}

/// The days a sprint's capacity is counted over.
pub(super) struct Period<'a> {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub weekdays: &'a [Weekday],
    pub team_days_off: &'a [DaysOff],
}

fn assignee(item: &WorkItem) -> Option<(&str, &str)> {
    let assigned = item.fields.get("System.AssignedTo")?;
    let unique_name = assigned.get("uniqueName").and_then(|v| v.as_str())?;
    let name = assigned
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or(unique_name);
    Some((unique_name, name))
}

fn remaining_work(item: &WorkItem) -> f64 {
    item.fields
        .get("Microsoft.VSTS.Scheduling.RemainingWork")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
}

/// Hours to the nearest millionth, so sums of fractional estimates do not
/// show rounding errors, such as a tiny overload when work fills capacity.
fn hours(value: f64) -> f64 {
    let rounded = (value * 1e6).round() / 1e6;
    // Avoid -0, which serializes as "-0.0".
    rounded + 0.0
}

/// Each member's capacity over `period` against the remaining work of the
/// sprint's `items` assigned to them.
pub(super) fn plan(members: &[Member], period: &Period, items: &[WorkItem]) -> CapacityPlan {
    let mut members: Vec<(&Member, MemberCapacity)> = members
        .iter()
        .map(|member| {
            let days_off: Vec<DaysOff> = period
                .team_days_off
                .iter()
                .chain(&member.days_off)
                .copied()
                .collect();
            let days = working_days(period.from, period.to, period.weekdays, &days_off);
            // The API holds capacity per day as f32; rounding drops what
            // widening it adds, such as 7.6 becoming 7.5999999.
            let capacity_per_day = hours(member.capacity_per_day);
            let capacity = capacity_per_day * f64::from(days);
            let row = MemberCapacity {
                name: member.name.clone(),
                capacity_per_day,
                days,
                capacity,
                assigned: 0.0,
                left: capacity,
            };
            (member, row)
        })
        .collect();
    let mut other_work: Vec<OtherWork> = Vec::new();
    for item in items {
        let work = remaining_work(item);
        let assignee = assignee(item);
        let member = assignee.and_then(|(unique_name, _)| {
            members
                .iter_mut()
                .find(|(member, _)| member.unique_name.eq_ignore_ascii_case(unique_name))
        });
        if let Some((_, row)) = member {
            row.assigned += work;
            continue;
        }
        let unique_name = assignee.map(|(unique_name, _)| unique_name);
        let same_person = |other: &&mut OtherWork| match (&other.unique_name, unique_name) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            (None, None) => true,
            _ => false,
        };
        match other_work.iter_mut().find(same_person) {
            Some(other) => other.remaining_work += work,
            None => other_work.push(OtherWork {
                assigned_to: assignee.map(|(_, name)| name.to_string()),
                unique_name: unique_name.map(str::to_string),
                remaining_work: work,
            }),
        }
    }
    for (_, row) in &mut members {
        row.capacity = hours(row.capacity);
        row.assigned = hours(row.assigned);
        row.left = hours(row.capacity - row.assigned);
    }
    for other in &mut other_work {
        other.remaining_work = hours(other.remaining_work);
    }
    CapacityPlan {
        members: members.into_iter().map(|(_, row)| row).collect(),
        other_work,
    }
}

/// A run of days off, both ends included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DaysOff {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DaysOff {
    fn contains(&self, day: NaiveDate) -> bool {
        self.start <= day && day <= self.end
    }

    /// Reads one day, `2026-11-24`, or a run of days, `2026-11-24..2026-11-26`.
    pub fn parse(value: &str) -> Result<Self, String> {
        let date = |text: &str| {
            text.trim()
                .parse::<NaiveDate>()
                .map_err(|_| format!("'{text}' is not a date like 2026-11-24"))
        };
        let (start, end) = match value.split_once("..") {
            Some((start, end)) => (date(start)?, date(end)?),
            None => (date(value)?, date(value)?),
        };
        if end < start {
            return Err(format!("'{value}' ends before it starts"));
        }
        Ok(Self { start, end })
    }

    /// The range as the API takes it.
    pub fn to_date_range(self) -> DateRange {
        let midnight = |day: NaiveDate| format!("{day}T00:00:00Z");
        serde_json::from_value(serde_json::json!({
            "start": midnight(self.start),
            "end": midnight(self.end),
        }))
        .expect("a date range of two RFC 3339 dates")
    }
}

/// The days from `from` to `to`, both included, that fall on one of the
/// team's working days and in none of the runs of days off.
pub(super) fn working_days(
    from: NaiveDate,
    to: NaiveDate,
    weekdays: &[Weekday],
    days_off: &[DaysOff],
) -> u32 {
    from.iter_days()
        .take_while(|day| *day <= to)
        .filter(|day| weekdays.contains(&day.weekday()))
        .filter(|day| !days_off.iter().any(|off| off.contains(*day)))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEEKDAYS: [Weekday; 5] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
    ];

    fn date(day: &str) -> NaiveDate {
        day.parse().unwrap()
    }

    fn team() -> Vec<TeamMember> {
        [
            ("1", "ann@one.com", "Ann"),
            ("2", "alex@one.com", "Alex"),
            ("3", "alex@two.com", "Alex"),
        ]
        .into_iter()
        .map(|(id, unique_name, name)| TeamMember {
            id: id.into(),
            unique_name: unique_name.into(),
            name: name.into(),
        })
        .collect()
    }

    #[test]
    fn members_are_found_by_unique_or_display_name() {
        let team = team();
        assert_eq!(find_member(&team, "ANN@one.com", "T").unwrap().id, "1");
        assert_eq!(find_member(&team, "ann", "T").unwrap().id, "1");
        assert_eq!(find_member(&team, "alex@two.com", "T").unwrap().id, "3");
    }

    #[test]
    fn display_names_match_ignoring_case_beyond_ascii() {
        let mut team = team();
        team.push(TeamMember {
            id: "4".into(),
            unique_name: "jorg@one.com".into(),
            name: "Jörg".into(),
        });
        assert_eq!(find_member(&team, "JÖRG", "T").unwrap().id, "4");
        team.push(TeamMember {
            id: "5".into(),
            unique_name: "jorg@two.com".into(),
            name: "JÖRG".into(),
        });
        assert!(find_member(&team, "jörg", "T")
            .unwrap_err()
            .to_string()
            .starts_with("Several members of team 'T' are called 'jörg'"));
    }

    #[test]
    fn unknown_or_shared_names_are_refused() {
        let team = team();
        assert_eq!(
            find_member(&team, "Bob", "Web").unwrap_err().to_string(),
            "Team 'Web' has no member 'Bob'; its members are Ann (ann@one.com), Alex (alex@one.com), Alex (alex@two.com)"
        );
        assert_eq!(
            find_member(&team, "alex", "Web").unwrap_err().to_string(),
            "Several members of team 'Web' are called 'alex'; use one of alex@one.com, alex@two.com"
        );
    }

    #[test]
    fn hours_a_day_must_be_a_number_from_0_to_24() {
        assert_eq!(parse_hours_a_day("6.5"), Ok(6.5));
        assert_eq!(parse_hours_a_day("0"), Ok(0.0));
        assert_eq!(parse_hours_a_day("24"), Ok(24.0));
        for bad in ["NaN", "inf", "1e39", "-1", "24.5", "six"] {
            assert_eq!(
                parse_hours_a_day(bad),
                Err(format!("'{bad}' is not a number of hours from 0 to 24")),
            );
        }
    }

    #[test]
    fn capacity_patch_sets_what_is_given() {
        let off = DaysOff::parse("2026-11-24").unwrap();
        assert_eq!(
            serde_json::to_value(capacity_patch(Some(6.5), Some("Development"), &[off])).unwrap(),
            serde_json::json!({
                "activities": [{"capacityPerDay": 6.5, "name": "Development"}],
                "daysOff": [{"start": "2026-11-24T00:00:00Z", "end": "2026-11-24T00:00:00Z"}],
            })
        );
        assert_eq!(
            serde_json::to_value(capacity_patch(Some(4.0), None, &[])).unwrap(),
            serde_json::json!({"activities": [{"capacityPerDay": 4.0, "name": ""}]})
        );
        assert_eq!(
            serde_json::to_value(capacity_patch(None, None, &[off])).unwrap()["activities"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn working_days_left_leave_out_team_days_off() {
        let team_off = [DaysOff::parse("2026-11-12..2026-11-13").unwrap()];
        let period = Period {
            from: date("2026-11-09"),
            to: date("2026-11-13"),
            weekdays: &WEEKDAYS,
            team_days_off: &team_off,
        };
        let info = SprintInfo {
            name: "Sprint 3".into(),
            path: "Proj\\Sprint 3".into(),
            start_date: None,
            finish_date: None,
        };
        let overview = CapacityOverview::new("Web Team", info, &period, &[], &[]);
        assert_eq!(overview.working_days, 3);
    }

    #[test]
    fn team_days_off_patch_lists_each_run() {
        let off = [
            DaysOff::parse("2026-11-24").unwrap(),
            DaysOff::parse("2026-12-24..2026-12-26").unwrap(),
        ];
        assert_eq!(
            serde_json::to_value(team_days_off_patch(&off)).unwrap(),
            serde_json::json!({"daysOff": [
                {"start": "2026-11-24T00:00:00Z", "end": "2026-11-24T00:00:00Z"},
                {"start": "2026-12-24T00:00:00Z", "end": "2026-12-26T00:00:00Z"},
            ]})
        );
    }

    #[test]
    fn days_off_parse_from_one_day_or_a_run() {
        assert_eq!(
            DaysOff::parse("2026-11-24"),
            Ok(DaysOff {
                start: date("2026-11-24"),
                end: date("2026-11-24"),
            })
        );
        assert_eq!(
            DaysOff::parse("2026-11-24..2026-11-26"),
            Ok(DaysOff {
                start: date("2026-11-24"),
                end: date("2026-11-26"),
            })
        );
    }

    #[test]
    fn days_off_refuse_bad_dates_and_backward_runs() {
        assert_eq!(
            DaysOff::parse("tomorrow"),
            Err("'tomorrow' is not a date like 2026-11-24".into())
        );
        assert_eq!(
            DaysOff::parse("2026-11-26..2026-11-24"),
            Err("'2026-11-26..2026-11-24' ends before it starts".into())
        );
    }

    #[test]
    fn days_off_round_trip_through_the_api_range() {
        let off = DaysOff::parse("2026-11-24..2026-11-26").unwrap();
        let range = off.to_date_range();
        assert_eq!(days_off(&[range]), [off]);
        let json = serde_json::to_value(off.to_date_range()).unwrap();
        assert_eq!(json["start"], "2026-11-24T00:00:00Z");
        assert_eq!(json["end"], "2026-11-26T00:00:00Z");
    }

    #[test]
    fn two_weeks_have_ten_working_days() {
        // Monday 2 November to Friday 13 November 2026
        assert_eq!(
            working_days(date("2026-11-02"), date("2026-11-13"), &WEEKDAYS, &[]),
            10
        );
    }

    #[test]
    fn days_off_on_working_days_are_left_out() {
        let off = [
            // Wednesday and Thursday
            DaysOff {
                start: date("2026-11-04"),
                end: date("2026-11-05"),
            },
            // A weekend, which is not counted anyway
            DaysOff {
                start: date("2026-11-07"),
                end: date("2026-11-08"),
            },
        ];
        assert_eq!(
            working_days(date("2026-11-02"), date("2026-11-13"), &WEEKDAYS, &off),
            8
        );
    }

    #[test]
    fn only_the_team_working_days_count() {
        assert_eq!(
            working_days(
                date("2026-11-02"),
                date("2026-11-08"),
                &[Weekday::Mon, Weekday::Sat],
                &[]
            ),
            2
        );
    }

    fn item(id: i32, assigned_to: Option<&str>, remaining: f64) -> WorkItem {
        let mut fields = serde_json::json!({
            "System.Title": format!("Item {id}"),
            "Microsoft.VSTS.Scheduling.RemainingWork": remaining,
        });
        if let Some(name) = assigned_to {
            fields["System.AssignedTo"] = serde_json::json!({
                "displayName": name,
                "uniqueName": format!("{}@example.com", name.to_lowercase()),
            });
        }
        serde_json::from_value(serde_json::json!({
            "id": id,
            "url": format!("https://dev.azure.com/org/_apis/wit/workItems/{id}"),
            "fields": fields,
        }))
        .unwrap()
    }

    fn member(name: &str, per_day: f64, days_off: Vec<DaysOff>) -> Member {
        Member {
            name: name.into(),
            unique_name: format!("{}@EXAMPLE.com", name.to_lowercase()),
            capacity_per_day: per_day,
            days_off,
        }
    }

    #[test]
    fn plan_sets_each_member_capacity_against_their_work() {
        let team_off = [DaysOff {
            start: date("2026-11-11"),
            end: date("2026-11-11"),
        }];
        let period = Period {
            // Monday to Friday, with the team off on Wednesday
            from: date("2026-11-09"),
            to: date("2026-11-13"),
            weekdays: &WEEKDAYS,
            team_days_off: &team_off,
        };
        let members = [
            member(
                "Ann",
                6.0,
                vec![DaysOff {
                    start: date("2026-11-13"),
                    end: date("2026-11-20"),
                }],
            ),
            member("Bob", 4.0, vec![]),
        ];
        let items = [
            item(1, Some("Ann"), 10.0),
            item(2, Some("Ann"), 12.5),
            item(3, Some("Bob"), 3.0),
            item(4, None, 2.0),
            item(5, Some("Eve"), 1.0),
            item(6, None, 0.5),
        ];
        assert_eq!(
            plan(&members, &period, &items),
            CapacityPlan {
                members: vec![
                    MemberCapacity {
                        name: "Ann".into(),
                        capacity_per_day: 6.0,
                        days: 3,
                        capacity: 18.0,
                        assigned: 22.5,
                        left: -4.5,
                    },
                    MemberCapacity {
                        name: "Bob".into(),
                        capacity_per_day: 4.0,
                        days: 4,
                        capacity: 16.0,
                        assigned: 3.0,
                        left: 13.0,
                    },
                ],
                other_work: vec![
                    OtherWork {
                        assigned_to: None,
                        unique_name: None,
                        remaining_work: 2.5,
                    },
                    OtherWork {
                        assigned_to: Some("Eve".into()),
                        unique_name: Some("eve@example.com".into()),
                        remaining_work: 1.0,
                    },
                ],
            }
        );
    }

    fn item_for(id: i32, name: &str, unique_name: &str, remaining: f64) -> WorkItem {
        let mut item = item(id, Some(name), remaining);
        item.fields["System.AssignedTo"] =
            serde_json::json!({"displayName": name, "uniqueName": unique_name});
        item
    }

    #[test]
    fn other_work_is_kept_apart_per_person_even_when_names_match() {
        let period = Period {
            from: date("2026-11-09"),
            to: date("2026-11-09"),
            weekdays: &WEEKDAYS,
            team_days_off: &[],
        };
        let items = [
            item_for(1, "Eve", "eve@one.com", 1.0),
            item_for(2, "Eve", "eve@two.com", 2.0),
            item_for(3, "Eve", "EVE@one.com", 4.0),
        ];
        let plan = plan(&[], &period, &items);
        assert_eq!(
            plan.other_work,
            [
                OtherWork {
                    assigned_to: Some("Eve".into()),
                    unique_name: Some("eve@one.com".into()),
                    remaining_work: 5.0,
                },
                OtherWork {
                    assigned_to: Some("Eve".into()),
                    unique_name: Some("eve@two.com".into()),
                    remaining_work: 2.0,
                },
            ]
        );
        let overview = CapacityOverview {
            team: "Web Team",
            iteration: SprintInfo {
                name: "Sprint 3".into(),
                path: "Proj\\Sprint 3".into(),
                start_date: None,
                finish_date: None,
            },
            from: period.from,
            to: period.to,
            working_days: 1,
            total: MemberCapacity {
                name: "Total".into(),
                capacity_per_day: 0.0,
                days: 0,
                capacity: 0.0,
                assigned: 0.0,
                left: 0.0,
            },
            plan,
        };
        let text = rendered(|w| overview.text(w));
        assert!(text.contains("Eve <eve@one.com> (no capacity)"), "{text}");
        assert!(text.contains("Eve <eve@two.com> (no capacity)"), "{text}");
    }

    #[test]
    fn capacity_read_as_f32_does_not_warn_when_work_fills_it() {
        let period = Period {
            // Monday to Friday
            from: date("2026-11-09"),
            to: date("2026-11-13"),
            weekdays: &WEEKDAYS,
            team_days_off: &[],
        };
        // The API holds capacity per day as f32, so 7.6 arrives as 7.5999999…
        let members = [member("Ann", f64::from(7.6_f32), vec![])];
        let plan_with = |work| plan(&members, &period, &[item(1, Some("Ann"), work)]);
        let full = plan_with(38.0);
        assert_eq!(full.members[0].capacity_per_day, 7.6);
        assert_eq!(full.members[0].capacity, 38.0);
        assert_eq!(full.members[0].left, 0.0);
        assert_eq!(plan_with(39.0).members[0].left, -1.0);
    }

    #[test]
    fn work_that_fills_the_capacity_leaves_nothing_rather_than_a_rounding_error() {
        let period = Period {
            // One Monday
            from: date("2026-11-09"),
            to: date("2026-11-09"),
            weekdays: &WEEKDAYS,
            team_days_off: &[],
        };
        let items = [0.1, 0.2, 0.3, 0.4, 0.1, 0.2]
            .iter()
            .enumerate()
            .map(|(id, work)| item(id as i32, Some("Ann"), *work))
            .collect::<Vec<_>>();
        let plan = plan(&[member("Ann", 1.3, vec![])], &period, &items);
        assert_eq!(plan.members[0].assigned, 1.3);
        assert_eq!(plan.members[0].left, 0.0);
    }

    #[test]
    fn weekdays_come_from_the_team_settings_or_default_to_monday_to_friday() {
        assert_eq!(
            parse_weekdays(&[serde_json::json!("monday"), serde_json::json!("saturday")]),
            [Weekday::Mon, Weekday::Sat]
        );
        assert_eq!(parse_weekdays(&[]), WEEKDAYS);
    }

    fn overview<'a>(members: &[Member], items: &[WorkItem]) -> CapacityOverview<'a> {
        let period = Period {
            from: date("2026-11-09"),
            to: date("2026-11-13"),
            weekdays: &WEEKDAYS,
            team_days_off: &[],
        };
        CapacityOverview::new(
            "Web Team",
            SprintInfo {
                name: "Sprint 3".into(),
                path: "Proj\\Sprint 3".into(),
                start_date: Some("2026-11-02".into()),
                finish_date: Some("2026-11-13".into()),
            },
            &period,
            members,
            items,
        )
    }

    fn rendered(render: impl Fn(&mut Vec<u8>) -> std::io::Result<()>) -> String {
        let mut out = Vec::new();
        render(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn sample<'a>() -> CapacityOverview<'a> {
        overview(
            &[member("Ann", 6.0, vec![]), member("Bob", 1.5, vec![])],
            &[
                item(1, Some("Ann"), 31.0),
                item(2, Some("Bob"), 2.0),
                item(3, None, 4.0),
                item(4, Some("Eve"), 1.0),
            ],
        )
    }

    #[test]
    fn overview_text_shows_each_member_the_total_and_other_work() {
        let text = rendered(|w| sample().text(w));
        let row = |cells: [&str; 6]| {
            format!(
                "{:<30} {:>8} {:>5} {:>9} {:>9} {:>9}\n",
                cells[0], cells[1], cells[2], cells[3], cells[4], cells[5]
            )
        };
        for expected in [
            "🏃 Sprint 3 (Proj\\Sprint 3)\n".to_string(),
            "📅 2026-11-09 → 2026-11-13 · 5 working day(s) left · 👥 Web Team\n".into(),
            row(["Ann", "6", "5", "30", "31", "⚠️ -1"]),
            row(["Bob", "1.5", "5", "7.5", "2", "5.5"]),
            row(["Total", "7.5", "", "37.5", "33", "4.5"]),
            row(["Unassigned", "", "", "", "4", ""]),
            row(["Eve (no capacity)", "", "", "", "1", ""]),
        ] {
            assert!(text.contains(&expected), "missing {expected:?} in\n{text}");
        }
        assert!(!text.contains("No capacity is set"));
    }

    #[test]
    fn overview_says_when_no_capacity_is_set() {
        let overview = overview(&[], &[item(1, None, 2.0)]);
        let text = rendered(|w| overview.text(w));
        assert!(text.contains("No capacity is set for this sprint.\n\n"));
        let rule = "-".repeat(75);
        assert!(
            !text.contains(&format!("{rule}\n{rule}")),
            "rules in a row in\n{text}"
        );
    }

    #[test]
    fn overview_name_column_fits_long_names() {
        let name = "Someone With A Very Long Display Name";
        let overview = overview(&[], &[item(1, Some(name), 2.0)]);
        let text = rendered(|w| overview.text(w));
        let label = format!("{name} (no capacity)");
        assert!(
            text.contains(&format!("{label} {:>8}", "")),
            "name not padded in\n{text}"
        );
        assert!(text.contains(&"-".repeat(label.len() + 45)));
    }

    #[test]
    fn overview_markdown_has_a_capacity_table() {
        let markdown = rendered(|w| sample().markdown(w));
        assert!(markdown.starts_with("## Sprint 3 (Proj\\Sprint 3)\n"));
        assert!(markdown.contains("| Member | Per day | Days | Capacity | Assigned | Left |\n"));
        assert!(markdown.contains("| **Total** | 7.5 |  | 37.5 | 33 | 4.5 |\n"));
    }

    #[test]
    fn overview_serializes_members_other_work_and_total() {
        let value = serde_json::to_value(sample()).unwrap();
        assert_eq!(value["team"], "Web Team");
        assert_eq!(value["from"], "2026-11-09");
        assert_eq!(value["workingDays"], 5);
        assert_eq!(value["members"][0]["name"], "Ann");
        assert_eq!(value["members"][0]["left"], -1.0);
        assert_eq!(value["otherWork"][0]["assignedTo"], serde_json::Value::Null);
        assert_eq!(value["total"]["capacity"], 37.5);
    }

    #[test]
    fn a_period_that_ended_has_no_working_days() {
        assert_eq!(
            working_days(date("2026-11-13"), date("2026-11-02"), &WEEKDAYS, &[]),
            0
        );
    }
}
