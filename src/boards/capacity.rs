//! A team's capacity for the rest of its current sprint, set against the
//! remaining work assigned to each member.

use chrono::{Datelike, NaiveDate, Weekday};

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

    #[test]
    fn a_period_that_ended_has_no_working_days() {
        assert_eq!(
            working_days(date("2026-11-13"), date("2026-11-02"), &WEEKDAYS, &[]),
            0
        );
    }
}
