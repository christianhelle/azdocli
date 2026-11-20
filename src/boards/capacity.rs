//! A team's capacity for the rest of its current sprint, set against the
//! remaining work assigned to each member.

use azure_devops_rust_api::wit::models::WorkItem;
use chrono::{Datelike, NaiveDate, Weekday};
use serde::Serialize;

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
    /// `None` for unassigned work.
    pub assigned_to: Option<String>,
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
            let capacity = member.capacity_per_day * f64::from(days);
            let row = MemberCapacity {
                name: member.name.clone(),
                capacity_per_day: member.capacity_per_day,
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
            row.left -= work;
            continue;
        }
        let assigned_to = assignee.map(|(_, name)| name.to_string());
        match other_work.iter_mut().find(|o| o.assigned_to == assigned_to) {
            Some(other) => other.remaining_work += work,
            None => other_work.push(OtherWork {
                assigned_to,
                remaining_work: work,
            }),
        }
    }
    CapacityPlan {
        members: members.into_iter().map(|(_, row)| row).collect(),
        other_work,
    }
}

/// A run of days off, both ends included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DaysOff {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DaysOff {
    fn contains(&self, day: NaiveDate) -> bool {
        self.start <= day && day <= self.end
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
                        remaining_work: 2.5,
                    },
                    OtherWork {
                        assigned_to: Some("Eve".into()),
                        remaining_work: 1.0,
                    },
                ],
            }
        );
    }

    #[test]
    fn a_period_that_ended_has_no_working_days() {
        assert_eq!(
            working_days(date("2026-11-13"), date("2026-11-02"), &WEEKDAYS, &[]),
            0
        );
    }
}
