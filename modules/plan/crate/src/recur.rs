//! Recurrence — the RFC 5545 subset QuantPlan supports.
//!
//! `FREQ` (DAILY | WEEKLY | MONTHLY | YEARLY), `INTERVAL`, `BYDAY` (weekly),
//! `COUNT` and `UNTIL`. Hand-written rather than pulled from a crate because
//! the subset is small, the DST behaviour below is the part that actually
//! matters, and it is worth having under test in this repository.
//!
//! **Everything here works in local dates.** A weekly 09:00 event stays 09:00
//! local across a daylight-saving change; in UTC it moves by an hour. So the
//! series is expanded as a list of *dates* in the event's own timezone, and the
//! caller attaches the constant local time-of-day afterwards. Expanding in UTC
//! is the classic way to get a calendar that drifts twice a year.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub freq: Freq,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<NaiveDate>,
    /// Weekly only. Empty means "the weekday DTSTART falls on".
    pub byday: Vec<Weekday>,
}

impl Default for Rule {
    fn default() -> Self {
        Self { freq: Freq::Daily, interval: 1, count: None, until: None, byday: Vec::new() }
    }
}

/// A runaway rule (`INTERVAL=0`, or a window a century wide) must not hang the
/// IPC thread. Well past any real calendar, far short of a freeze.
const MAX_STEPS: u32 = 10_000;

fn weekday(token: &str) -> Option<Weekday> {
    match token.trim().to_ascii_uppercase().as_str() {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Parse an RRULE body — `FREQ=WEEKLY;BYDAY=MO,WE;INTERVAL=2`, with or without
/// the `RRULE:` prefix. Unknown parts are ignored rather than rejected: a rule
/// carrying a `BYSETPOS` we do not implement should still produce its weekly
/// occurrences instead of vanishing from the calendar.
pub fn parse(rrule: &str) -> Result<Rule, String> {
    let body = rrule.trim().strip_prefix("RRULE:").unwrap_or(rrule.trim());
    let mut rule = Rule::default();
    let mut saw_freq = false;

    for part in body.split(';').filter(|p| !p.is_empty()) {
        let (key, value) = part.split_once('=').ok_or_else(|| format!("Malformed RRULE part: {part}"))?;
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                rule.freq = match value.trim().to_ascii_uppercase().as_str() {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    other => return Err(format!("Unsupported FREQ: {other}")),
                };
                saw_freq = true;
            }
            "INTERVAL" => {
                let n: u32 = value.trim().parse().map_err(|_| format!("Bad INTERVAL: {value}"))?;
                // INTERVAL=0 is invalid and would step nowhere forever.
                rule.interval = n.max(1);
            }
            "COUNT" => {
                rule.count = Some(value.trim().parse().map_err(|_| format!("Bad COUNT: {value}"))?);
            }
            "UNTIL" => {
                // Both `20260901T090000Z` and `20260901` occur in the wild; the
                // date part is all this subset needs.
                let raw: String = value.trim().chars().take(8).collect();
                rule.until = Some(
                    NaiveDate::parse_from_str(&raw, "%Y%m%d")
                        .map_err(|_| format!("Bad UNTIL: {value}"))?,
                );
            }
            "BYDAY" => {
                rule.byday = value.split(',').filter_map(weekday).collect();
            }
            _ => {}
        }
    }

    if !saw_freq {
        return Err("RRULE is missing FREQ".into());
    }
    Ok(rule)
}

pub fn to_string(rule: &Rule) -> String {
    let mut out = String::from("FREQ=");
    out.push_str(match rule.freq {
        Freq::Daily => "DAILY",
        Freq::Weekly => "WEEKLY",
        Freq::Monthly => "MONTHLY",
        Freq::Yearly => "YEARLY",
    });
    if rule.interval > 1 {
        out.push_str(&format!(";INTERVAL={}", rule.interval));
    }
    if !rule.byday.is_empty() {
        let days: Vec<&str> = rule
            .byday
            .iter()
            .map(|d| match d {
                Weekday::Mon => "MO",
                Weekday::Tue => "TU",
                Weekday::Wed => "WE",
                Weekday::Thu => "TH",
                Weekday::Fri => "FR",
                Weekday::Sat => "SA",
                Weekday::Sun => "SU",
            })
            .collect();
        out.push_str(&format!(";BYDAY={}", days.join(",")));
    }
    if let Some(count) = rule.count {
        out.push_str(&format!(";COUNT={count}"));
    }
    if let Some(until) = rule.until {
        out.push_str(&format!(";UNTIL={}", until.format("%Y%m%d")));
    }
    out
}

/// Add `months` calendar months, keeping the day of month.
///
/// Returns `None` when the day does not exist in the target month — the 31st
/// in a 30-day month. That is RFC 5545's behaviour for a MONTHLY rule without
/// BYMONTHDAY: the occurrence is *skipped*, not clamped to the 30th. Clamping
/// is the intuitive-looking choice that silently invents a meeting.
fn add_months(date: NaiveDate, months: i32) -> Option<NaiveDate> {
    let total = date.year() * 12 + date.month0() as i32 + months;
    let year = total.div_euclid(12);
    let month0 = total.rem_euclid(12) as u32;
    NaiveDate::from_ymd_opt(year, month0 + 1, date.day())
}

fn add_years(date: NaiveDate, years: i32) -> Option<NaiveDate> {
    // Feb 29 in a non-leap year: skipped, same reasoning as add_months.
    NaiveDate::from_ymd_opt(date.year() + years, date.month(), date.day())
}

/// The Monday on or before `date` — this subset assumes `WKST=MO`, which is
/// the default everywhere outside the US.
fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

/// Every date the rule produces that falls inside `[window_from, window_to]`.
///
/// `COUNT` counts from `start`, not from the window, so the walk always begins
/// at DTSTART — a `COUNT=3` series never yields a fourth occurrence just
/// because the caller asked about a later month.
pub fn expand_dates(
    start: NaiveDate,
    rule: &Rule,
    window_from: NaiveDate,
    window_to: NaiveDate,
) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut emitted: u32 = 0;
    let interval = rule.interval.max(1) as i32;

    let push = |date: NaiveDate, emitted: &mut u32, out: &mut Vec<NaiveDate>| -> bool {
        if date < start {
            return true;
        }
        if let Some(until) = rule.until {
            if date > until {
                return false;
            }
        }
        if let Some(count) = rule.count {
            if *emitted >= count {
                return false;
            }
        }
        *emitted += 1;
        if date >= window_from && date <= window_to {
            out.push(date);
        }
        true
    };

    match rule.freq {
        Freq::Daily => {
            for step in 0..MAX_STEPS {
                let Some(date) = start.checked_add_signed(Duration::days((step as i32 * interval) as i64))
                else {
                    break;
                };
                if date > window_to && rule.count.is_none() {
                    break;
                }
                if !push(date, &mut emitted, &mut out) {
                    break;
                }
                if date > window_to && rule.count.is_some() && out.len() > 1 {
                    break;
                }
            }
        }
        Freq::Weekly => {
            let days: Vec<Weekday> =
                if rule.byday.is_empty() { vec![start.weekday()] } else { rule.byday.clone() };
            let base = week_start(start);
            'weeks: for step in 0..MAX_STEPS {
                let Some(monday) =
                    base.checked_add_signed(Duration::days((step as i32 * interval * 7) as i64))
                else {
                    break;
                };
                if monday > window_to && rule.count.is_none() {
                    break;
                }
                // Within a week the days come out in calendar order, whatever
                // order BYDAY listed them in.
                let mut in_week: Vec<NaiveDate> =
                    days.iter().map(|d| monday + Duration::days(d.num_days_from_monday() as i64)).collect();
                in_week.sort_unstable();
                in_week.dedup();
                for date in in_week {
                    if !push(date, &mut emitted, &mut out) {
                        break 'weeks;
                    }
                }
                if monday > window_to {
                    break;
                }
            }
        }
        Freq::Monthly => {
            for step in 0..MAX_STEPS {
                let Some(date) = add_months(start, step as i32 * interval) else {
                    // A skipped month is not the end of the series: February
                    // has no 31st, March does.
                    continue;
                };
                if date > window_to && rule.count.is_none() {
                    break;
                }
                if !push(date, &mut emitted, &mut out) {
                    break;
                }
                if date > window_to {
                    break;
                }
            }
        }
        Freq::Yearly => {
            for step in 0..MAX_STEPS {
                let Some(date) = add_years(start, step as i32 * interval) else {
                    continue;
                };
                if date > window_to && rule.count.is_none() {
                    break;
                }
                if !push(date, &mut emitted, &mut out) {
                    break;
                }
                if date > window_to {
                    break;
                }
            }
        }
    }

    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn parses_a_weekly_rule() {
        let rule = parse("FREQ=WEEKLY;BYDAY=MO,WE,FR;INTERVAL=2").unwrap();
        assert_eq!(rule.freq, Freq::Weekly);
        assert_eq!(rule.interval, 2);
        assert_eq!(rule.byday, vec![Weekday::Mon, Weekday::Wed, Weekday::Fri]);
    }

    #[test]
    fn round_trips_through_to_string() {
        let rule = parse("FREQ=MONTHLY;INTERVAL=3;COUNT=5").unwrap();
        assert_eq!(to_string(&rule), "FREQ=MONTHLY;INTERVAL=3;COUNT=5");
    }

    #[test]
    fn rejects_a_rule_without_freq() {
        assert!(parse("INTERVAL=2").is_err());
    }

    #[test]
    fn ignores_parts_it_does_not_implement() {
        let rule = parse("FREQ=WEEKLY;BYSETPOS=-1;WKST=SU").unwrap();
        assert_eq!(rule.freq, Freq::Weekly);
    }

    #[test]
    fn interval_zero_does_not_hang() {
        let rule = parse("FREQ=DAILY;INTERVAL=0").unwrap();
        assert_eq!(rule.interval, 1);
        let dates = expand_dates(d(2026, 1, 1), &rule, d(2026, 1, 1), d(2026, 1, 5));
        assert_eq!(dates.len(), 5);
    }

    #[test]
    fn daily_every_third_day() {
        let rule = parse("FREQ=DAILY;INTERVAL=3").unwrap();
        let dates = expand_dates(d(2026, 1, 1), &rule, d(2026, 1, 1), d(2026, 1, 10));
        assert_eq!(dates, vec![d(2026, 1, 1), d(2026, 1, 4), d(2026, 1, 7), d(2026, 1, 10)]);
    }

    #[test]
    fn weekly_byday_lands_on_every_named_day() {
        // 2026-01-05 is a Monday.
        let rule = parse("FREQ=WEEKLY;BYDAY=MO,WE,FR").unwrap();
        let dates = expand_dates(d(2026, 1, 5), &rule, d(2026, 1, 5), d(2026, 1, 11));
        assert_eq!(dates, vec![d(2026, 1, 5), d(2026, 1, 7), d(2026, 1, 9)]);
    }

    #[test]
    fn weekly_interval_skips_the_odd_weeks() {
        let rule = parse("FREQ=WEEKLY;BYDAY=MO;INTERVAL=2").unwrap();
        let dates = expand_dates(d(2026, 1, 5), &rule, d(2026, 1, 5), d(2026, 2, 2));
        assert_eq!(dates, vec![d(2026, 1, 5), d(2026, 1, 19), d(2026, 2, 2)]);
    }

    #[test]
    fn count_is_measured_from_dtstart_not_from_the_window() {
        let rule = parse("FREQ=DAILY;COUNT=3").unwrap();
        // The window opens after the series has already ended.
        let dates = expand_dates(d(2026, 1, 1), &rule, d(2026, 1, 4), d(2026, 1, 31));
        assert!(dates.is_empty(), "COUNT=3 must not produce a fourth occurrence, got {dates:?}");
    }

    #[test]
    fn until_is_inclusive() {
        let rule = parse("FREQ=DAILY;UNTIL=20260103").unwrap();
        let dates = expand_dates(d(2026, 1, 1), &rule, d(2026, 1, 1), d(2026, 1, 31));
        assert_eq!(dates, vec![d(2026, 1, 1), d(2026, 1, 2), d(2026, 1, 3)]);
    }

    #[test]
    fn monthly_skips_months_without_a_31st() {
        let rule = parse("FREQ=MONTHLY").unwrap();
        let dates = expand_dates(d(2026, 1, 31), &rule, d(2026, 1, 1), d(2026, 5, 31));
        // February and April have no 31st: skipped, not clamped.
        assert_eq!(dates, vec![d(2026, 1, 31), d(2026, 3, 31), d(2026, 5, 31)]);
    }

    #[test]
    fn yearly_skips_feb_29_in_common_years() {
        let rule = parse("FREQ=YEARLY").unwrap();
        let dates = expand_dates(d(2024, 2, 29), &rule, d(2024, 1, 1), d(2029, 12, 31));
        assert_eq!(dates, vec![d(2024, 2, 29), d(2028, 2, 29)]);
    }

    #[test]
    fn a_window_before_dtstart_yields_nothing() {
        let rule = parse("FREQ=DAILY").unwrap();
        let dates = expand_dates(d(2026, 6, 1), &rule, d(2026, 1, 1), d(2026, 5, 31));
        assert!(dates.is_empty());
    }
}
