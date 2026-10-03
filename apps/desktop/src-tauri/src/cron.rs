//! A five-field cron expression: `minute hour day-of-month month day-of-week`.
//!
//! Hand-written rather than a dependency because the surface is small, the rules
//! are fiddly in exactly one place (day-of-month and day-of-week combine with
//! OR when both are restricted), and a scheduler that mis-fires is a scheduler
//! that quietly runs agents at the wrong time (ticket 019).
//!
//! Supported per field: `*`, a number, `a-b`, `a,b,c`, and `/n` steps on `*` or
//! a range. Names (`MON`, `JAN`) and `@daily`-style shorthands are not.
//! Day-of-week is 0-7 with both 0 and 7 meaning Sunday. Times are the user's
//! local time, because "every weekday at 9" means their 9.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDateTime, TimeZone, Timelike};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CronError {
    #[error("a schedule has five fields: minute hour day-of-month month day-of-week")]
    FieldCount,
    #[error("`{0}` is not a valid value for the {1} field")]
    Bad(String, &'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    minutes: u64,
    hours: u64,
    days: u64,
    months: u64,
    weekdays: u64,
    day_restricted: bool,
    weekday_restricted: bool,
}

fn parse_field(text: &str, lo: u32, hi: u32, name: &'static str) -> Result<u64, CronError> {
    let bad = || CronError::Bad(text.to_owned(), name);
    let mut bits = 0u64;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(bad)?),
            None => (part, 1),
        };
        let (from, to) = if range == "*" {
            (lo, hi)
        } else if let Some((a, b)) = range.split_once('-') {
            (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?)
        } else {
            let n: u32 = range.parse().map_err(|_| bad())?;
            // `5/10` means 5, 15, 25 … to the end, as in vixie cron.
            (n, if part.contains('/') { hi } else { n })
        };
        if from < lo || to > hi || from > to {
            return Err(bad());
        }
        let mut v = from;
        while v <= to {
            bits |= 1 << v;
            v += step;
        }
    }
    if bits == 0 { Err(bad()) } else { Ok(bits) }
}

impl Cron {
    /// Parse an expression.
    ///
    /// # Errors
    /// [`CronError`] naming the field that is wrong.
    pub fn parse(expr: &str) -> Result<Self, CronError> {
        let f: Vec<&str> = expr.split_whitespace().collect();
        let [min, hour, dom, mon, dow] = f[..] else {
            return Err(CronError::FieldCount);
        };
        let mut weekdays = parse_field(dow, 0, 7, "day-of-week")?;
        if weekdays & (1 << 7) != 0 {
            weekdays = (weekdays & !(1 << 7)) | 1; // 7 is Sunday too
        }
        Ok(Self {
            minutes: parse_field(min, 0, 59, "minute")?,
            hours: parse_field(hour, 0, 23, "hour")?,
            days: parse_field(dom, 1, 31, "day-of-month")?,
            months: parse_field(mon, 1, 12, "month")?,
            weekdays,
            day_restricted: dom != "*",
            weekday_restricted: dow != "*",
        })
    }

    fn day_matches<T: Datelike>(&self, t: &T) -> bool {
        let dom = self.days & (1 << t.day()) != 0;
        let dow = self.weekdays & (1 << t.weekday().num_days_from_sunday()) != 0;
        // The one fiddly rule: restrict both and either may match.
        match (self.day_restricted, self.weekday_restricted) {
            (true, true) => dom || dow,
            (true, false) => dom,
            (false, true) => dow,
            (false, false) => true,
        }
    }

    fn matches_naive(&self, t: &NaiveDateTime) -> bool {
        self.minutes & (1 << t.minute()) != 0
            && self.hours & (1 << t.hour()) != 0
            && self.months & (1 << t.month()) != 0
            && self.day_matches(t)
    }

    /// The first fire strictly after `after`, in `tz`, or `None` if there is none
    /// within about five years (an expression like `0 0 30 2 *`).
    ///
    /// Walks wall-clock minutes in naive local time and resolves each candidate
    /// back into the zone, so a daylight-saving gap simply has no candidate and
    /// an overlap fires once.
    pub fn next_after<Tz: TimeZone>(&self, after: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let tz = after.timezone();
        let mut t = after.naive_local().with_second(0)?.with_nanosecond(0)? + Duration::minutes(1);
        let limit = t + Duration::days(366 * 5);
        while t < limit {
            if self.months & (1 << t.month()) == 0 {
                // Skip to the first of next month.
                let (y, m) = if t.month() == 12 { (t.year() + 1, 1) } else { (t.year(), t.month() + 1) };
                t = chrono::NaiveDate::from_ymd_opt(y, m, 1)?.and_hms_opt(0, 0, 0)?;
                continue;
            }
            if !self.day_matches(&t) {
                t = (t.date() + Duration::days(1)).and_hms_opt(0, 0, 0)?;
                continue;
            }
            if self.hours & (1 << t.hour()) == 0 {
                t = t.with_minute(0)? + Duration::hours(1);
                continue;
            }
            if self.matches_naive(&t) {
                match tz.from_local_datetime(&t) {
                    LocalResult::Single(d) => return Some(d),
                    // An overlap: the earlier instant is the one that fires.
                    LocalResult::Ambiguous(first, _) => return Some(first),
                    LocalResult::None => {}
                }
            }
            t += Duration::minutes(1);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn next(expr: &str, after: &str) -> String {
        Cron::parse(expr).unwrap().next_after(&at(after)).unwrap().to_rfc3339()
    }

    #[test]
    fn a_daily_time_fires_today_if_it_is_still_ahead_and_tomorrow_if_not() {
        assert_eq!(next("30 9 * * *", "2026-03-10T08:00:00Z"), "2026-03-10T09:30:00+00:00");
        assert_eq!(next("30 9 * * *", "2026-03-10T09:30:00Z"), "2026-03-11T09:30:00+00:00");
    }

    #[test]
    fn steps_lists_and_ranges() {
        assert_eq!(next("*/15 * * * *", "2026-03-10T08:07:00Z"), "2026-03-10T08:15:00+00:00");
        assert_eq!(next("0 9-17/4 * * *", "2026-03-10T10:00:00Z"), "2026-03-10T13:00:00+00:00");
        assert_eq!(next("5,35 * * * *", "2026-03-10T08:10:00Z"), "2026-03-10T08:35:00+00:00");
        assert_eq!(next("5/20 * * * *", "2026-03-10T08:10:00Z"), "2026-03-10T08:25:00+00:00");
    }

    #[test]
    fn weekdays_only_skip_the_weekend() {
        // 2026-03-13 is a Friday.
        assert_eq!(next("0 9 * * 1-5", "2026-03-13T10:00:00Z"), "2026-03-16T09:00:00+00:00");
    }

    #[test]
    fn sunday_is_both_zero_and_seven() {
        let a = next("0 0 * * 0", "2026-03-10T00:00:00Z");
        let b = next("0 0 * * 7", "2026-03-10T00:00:00Z");
        assert_eq!(a, b);
        assert_eq!(a, "2026-03-15T00:00:00+00:00");
    }

    #[test]
    fn restricting_day_and_weekday_together_means_either() {
        // The 13th, or any Friday. After Thu 12th, the 13th (a Friday) is next;
        // after that, the next Friday is the 20th, not the next 13th.
        assert_eq!(next("0 0 13 * 5", "2026-03-12T00:00:00Z"), "2026-03-13T00:00:00+00:00");
        assert_eq!(next("0 0 13 * 5", "2026-03-13T00:00:00Z"), "2026-03-20T00:00:00+00:00");
    }

    #[test]
    fn month_end_rolls_over_and_short_months_are_skipped() {
        assert_eq!(next("0 0 31 * *", "2026-04-01T00:00:00Z"), "2026-05-31T00:00:00+00:00");
        assert_eq!(next("0 0 29 2 *", "2026-03-01T00:00:00Z"), "2028-02-29T00:00:00+00:00");
    }

    #[test]
    fn an_impossible_date_has_no_next_fire() {
        assert!(Cron::parse("0 0 31 2 *").unwrap().next_after(&at("2026-01-01T00:00:00Z")).is_none());
    }

    #[test]
    fn bad_expressions_name_what_is_wrong() {
        assert_eq!(Cron::parse("* * * *"), Err(CronError::FieldCount));
        assert!(matches!(Cron::parse("61 * * * *"), Err(CronError::Bad(_, "minute"))));
        assert!(matches!(Cron::parse("* 24 * * *"), Err(CronError::Bad(_, "hour"))));
        assert!(matches!(Cron::parse("* * 0 * *"), Err(CronError::Bad(_, "day-of-month"))));
        assert!(matches!(Cron::parse("* * * 13 *"), Err(CronError::Bad(_, "month"))));
        assert!(matches!(Cron::parse("* * * * 8"), Err(CronError::Bad(_, "day-of-week"))));
        assert!(matches!(Cron::parse("*/0 * * * *"), Err(CronError::Bad(_, "minute"))));
        assert!(matches!(Cron::parse("5-2 * * * *"), Err(CronError::Bad(_, "minute"))));
        assert!(matches!(Cron::parse("MON * * * *"), Err(CronError::Bad(_, "minute"))));
    }

    #[test]
    fn local_time_is_what_the_user_means_including_across_a_dst_gap() {
        use chrono::FixedOffset;
        let tz = FixedOffset::east_opt(2 * 3600).unwrap();
        let after = DateTime::parse_from_rfc3339("2026-03-10T00:00:00+02:00").unwrap().with_timezone(&tz);
        let n = Cron::parse("0 9 * * *").unwrap().next_after(&after).unwrap();
        assert_eq!(n.to_rfc3339(), "2026-03-10T09:00:00+02:00");
        assert_eq!(n.with_timezone(&Utc).to_rfc3339(), "2026-03-10T07:00:00+00:00");
    }
}
