//! Wall clock triggers: cues that start at a time of day.

use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime};
use mazzalab_core::{WallClockTrigger, Weekday};

/// The longest gap between two looks at the clock over which triggers
/// still fire: a machine that slept through a trigger's time does not
/// start the cue hours late when it wakes.
pub const MAX_GAP: Duration = Duration::seconds(10);

/// Whether the clock went past the trigger's time, on one of its days,
/// between `before` (excluded) and `now` (included).
pub fn passed(trigger: &WallClockTrigger, before: NaiveDateTime, now: NaiveDateTime) -> bool {
    if now <= before || now - before > MAX_GAP {
        return false;
    }
    let Some(time) = trigger
        .seconds()
        .and_then(|seconds| NaiveTime::from_num_seconds_from_midnight_opt(seconds, 0))
    else {
        return false;
    };
    // The look may span midnight: try the time on both days.
    let mut day = before.date();
    loop {
        let at = day.and_time(time);
        let weekday = Weekday::from_monday(day.weekday().num_days_from_monday());
        if before < at && at <= now && trigger.on(weekday) {
            return true;
        }
        if day >= now.date() {
            return false;
        }
        day = day.succ_opt().unwrap_or(day);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(day: u32, hour: u32, minute: u32, second: u32) -> NaiveDateTime {
        // November 2026: the 16th is a Monday.
        NaiveDate::from_ymd_opt(2026, 11, day)
            .unwrap()
            .and_hms_opt(hour, minute, second)
            .unwrap()
    }

    fn trigger(time: &str, days: &[Weekday]) -> WallClockTrigger {
        WallClockTrigger {
            time: time.into(),
            days: days.to_vec(),
        }
    }

    #[test]
    fn a_trigger_fires_once_as_the_clock_goes_past_it() {
        let opening = trigger("19:30", &[]);
        assert!(passed(&opening, at(16, 19, 29, 59), at(16, 19, 30, 0)));
        // Exactly at the time on the previous look: already fired.
        assert!(!passed(&opening, at(16, 19, 30, 0), at(16, 19, 30, 1)));
        assert!(!passed(&opening, at(16, 19, 28, 0), at(16, 19, 28, 1)));
        // Seconds count.
        assert!(passed(
            &trigger("19:30:05", &[]),
            at(16, 19, 30, 4),
            at(16, 19, 30, 5)
        ));
    }

    #[test]
    fn only_on_its_days_and_across_midnight() {
        let weekend = trigger("00:00", &[Weekday::Saturday, Weekday::Sunday]);
        // Friday 20th into Saturday 21st.
        assert!(passed(&weekend, at(20, 23, 59, 59), at(21, 0, 0, 0)));
        // Sunday 22nd into Monday 23rd.
        assert!(!passed(&weekend, at(22, 23, 59, 59), at(23, 0, 0, 1)));
    }

    #[test]
    fn a_long_sleep_or_a_clock_set_back_fires_nothing() {
        let opening = trigger("19:30", &[]);
        assert!(!passed(&opening, at(16, 18, 0, 0), at(16, 20, 0, 0)));
        assert!(!passed(&opening, at(16, 19, 31, 0), at(16, 19, 29, 0)));
        assert!(!passed(
            &trigger("25:00", &[]),
            at(16, 0, 0, 0),
            at(16, 0, 0, 5)
        ));
    }
}
