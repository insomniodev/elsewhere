//! The home day as hour rows, kept gpui-free so it can be tested.

use jiff::{Span, Zoned};

/// Rows add whole hours in absolute time, so a day the clocks change has 23
/// or 25 of them.
pub struct Day {
    start: Zoned,
    /// From this midnight to the next, which is not always 24 hours.
    seconds: i64,
}

impl Day {
    pub fn of(now: &Zoned) -> Day {
        let start = now.start_of_day().expect("a real zone has a start of day");
        let end = start
            .tomorrow()
            .and_then(|tomorrow| tomorrow.start_of_day())
            .expect("a real zone has a next day");
        let seconds = end.timestamp().duration_since(start.timestamp()).as_secs();
        Day { start, seconds }
    }

    /// Rounded up, so a day that changes by half an hour keeps its last
    /// partial hour.
    pub fn rows(&self) -> usize {
        (self.seconds as usize).div_ceil(3600)
    }

    pub fn row(&self, i: usize) -> Zoned {
        self.start
            .checked_add(Span::new().hours(i as i64))
            .expect("a day's hours past midnight never overflow a zone")
    }

    /// Compares instants rather than wall times, so DST days answer right.
    pub fn index(&self, at: &Zoned) -> Option<usize> {
        let elapsed = at
            .timestamp()
            .duration_since(self.start.timestamp())
            .as_secs();
        (0..self.seconds)
            .contains(&elapsed)
            .then_some((elapsed / 3600) as usize)
    }
}

#[cfg(test)]
mod tests {
    use jiff::tz::TimeZone;

    use super::*;

    /// An instant, read from the given zone as home.
    fn at(instant: &str, home: &str) -> Zoned {
        instant
            .parse::<jiff::Timestamp>()
            .expect("a valid timestamp")
            .to_zoned(TimeZone::get(home).expect("a zone this machine knows"))
    }

    #[test]
    fn rows_of_a_plain_day_are_an_hour_apart() {
        let now = at("2026-08-15T14:30:00Z", "UTC");
        let day = Day::of(&now);
        assert_eq!(day.row(0).datetime().to_string(), "2026-08-15T00:00:00");
        assert_eq!(day.row(1).datetime().to_string(), "2026-08-15T01:00:00");
        assert_eq!(day.row(23).datetime().to_string(), "2026-08-15T23:00:00");
    }

    #[test]
    fn index_covers_the_start_last_minute_and_just_outside() {
        let now = at("2026-08-15T00:00:00Z", "UTC");
        let day = Day::of(&now);
        assert_eq!(day.index(&day.row(0)), Some(0));
        let last_minute = at("2026-08-15T23:59:00Z", "UTC");
        assert_eq!(day.index(&last_minute), Some(23));
        let just_before = at("2026-08-14T23:59:59Z", "UTC");
        assert_eq!(day.index(&just_before), None);
        let just_after = at("2026-08-16T00:00:00Z", "UTC");
        assert_eq!(day.index(&just_after), None);
    }

    #[test]
    fn index_switches_rows_exactly_on_the_hour() {
        let now = at("2026-08-15T00:00:00Z", "UTC");
        let day = Day::of(&now);
        assert_eq!(day.index(&day.row(1)), Some(1));
        let just_before = day
            .row(1)
            .checked_sub(Span::new().seconds(1))
            .expect("a second before row 1");
        assert_eq!(day.index(&just_before), Some(0));
    }

    #[test]
    fn a_plain_day_has_24_rows() {
        let now = at("2026-08-15T14:30:00Z", "America/New_York");
        assert_eq!(Day::of(&now).rows(), 24);
    }

    #[test]
    fn a_spring_forward_day_has_23_rows_ending_on_its_own_last_hour() {
        // New York springs forward on 2026-03-08.
        let now = at("2026-03-08T12:00:00Z", "America/New_York");
        let day = Day::of(&now);
        assert_eq!(day.rows(), 23);
        assert_eq!(day.row(2).hour(), 3);
        assert_eq!(day.row(22).datetime().to_string(), "2026-03-08T23:00:00");
        let last_minute = at("2026-03-09T03:59:00Z", "America/New_York");
        assert_eq!(day.index(&last_minute), Some(22));
    }

    #[test]
    fn a_fall_back_day_has_25_rows_repeating_the_doubled_hour() {
        // New York falls back on 2026-11-01.
        let now = at("2026-11-01T12:00:00Z", "America/New_York");
        let day = Day::of(&now);
        assert_eq!(day.rows(), 25);
        assert_eq!(day.row(1).hour(), 1);
        assert_eq!(day.row(2).hour(), 1);
        assert_eq!(day.row(24).datetime().to_string(), "2026-11-01T23:00:00");
        let late = at("2026-11-02T04:30:00Z", "America/New_York");
        assert_eq!(day.index(&late), Some(24));
    }
}
