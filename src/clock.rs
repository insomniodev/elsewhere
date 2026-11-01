//! Turning an instant into what a saved city shows, and the grid's labels.

use std::collections::HashMap;
use std::time::Duration;

use jiff::civil::Date;
use jiff::tz::{Offset, TimeZone};
use jiff::{Unit, Zoned};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    TwentyFour,
    Twelve,
}

impl Format {
    /// The widest reading, so a column sized to it never reflows as the
    /// digits change.
    pub fn glyphs(self) -> usize {
        match self {
            Format::TwentyFour => 5,
            Format::Twelve => 8,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Reading {
    pub time: String,
    /// Set when the date there differs from home's.
    pub day: Option<String>,
}

/// For a city whose zone this machine's tzdb lacks; it still keeps its row.
pub const UNKNOWN: &str = "–:–";

/// So a timer firing a hair early still lands in the next minute rather than
/// waking again milliseconds later.
const PAD: Duration = Duration::from_millis(50);

/// `now`'s own zone is home.
pub fn reading(now: &Zoned, zone: &TimeZone, format: Format) -> Reading {
    let there = now.with_time_zone(zone.clone());
    let (hour, minute) = (there.hour(), there.minute());
    let time = match format {
        Format::TwentyFour => format!("{hour:02}:{minute:02}"),
        Format::Twelve => {
            let (hour, half) = twelve(hour);
            format!("{hour}:{minute:02} {half}")
        }
    };
    Reading {
        time,
        day: day(now.date(), there.date()),
    }
}

/// `(9, "pm")` for 21.
fn twelve(hour: i8) -> (i8, &'static str) {
    let half = if hour < 12 { "am" } else { "pm" };
    let hour = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    (hour, half)
}

/// "9 am" or "09:00", as a calendar gutter writes it. 12-hour labels show
/// minutes only for zones off the hour ("5:30 am"), and use the same lower-case
/// half as `reading`.
pub fn hour_label(hour: i8, minute: i8, format: Format) -> String {
    match format {
        Format::TwentyFour => format!("{hour:02}:{minute:02}"),
        Format::Twelve => {
            let (hour, half) = twelve(hour);
            if minute == 0 {
                format!("{hour} {half}")
            } else {
                format!("{hour}:{minute:02} {half}")
            }
        }
    }
}

/// "UTC", "+1", "−5", "+5:30" (U+2212 minus): the one notation for offsets
/// everywhere. From UTC only for the map's nautical marks and here's own zone;
/// everything else is from here, through `shift_label`.
pub fn offset_label(offset: Offset) -> String {
    signed(offset.seconds()).unwrap_or_else(|| "UTC".to_string())
}

/// "+14", "−1", "±0": how far `zone` runs ahead of here, which is `at`'s
/// zone, in `offset_label`'s notation.
pub fn shift_label(zone: &TimeZone, at: &Zoned) -> String {
    signed(shift(zone, at)).unwrap_or_else(|| "±0".to_string())
}

/// "14 hours ahead of here", "45 minutes behind here": `shift_label` spelled
/// out for a tooltip, which has room to name the reference.
pub fn shift_detail(zone: &TimeZone, at: &Zoned) -> String {
    let seconds = shift(zone, at);
    if seconds == 0 {
        return "same time as here".to_string();
    }
    let side = if seconds < 0 { "behind" } else { "ahead of" };
    let seconds = seconds.abs();
    let (hours, minutes) = (seconds / 3600, (seconds % 3600) / 60);
    let unit = |n: i32, one: &str| match n {
        1 => format!("1 {one}"),
        n => format!("{n} {one}s"),
    };
    let span = match (hours, minutes) {
        (0, minutes) => unit(minutes, "minute"),
        (hours, 0) => unit(hours, "hour"),
        (hours, minutes) => format!("{} {}", unit(hours, "hour"), unit(minutes, "minute")),
    };
    format!("{span} {side} here")
}

/// Whether `zone` is here, `at`'s zone. By name where both have one, since a
/// zone loaded from `TZ` and the same one looked up by name need not compare
/// equal.
pub fn is_here(zone: &TimeZone, at: &Zoned) -> bool {
    match (zone.iana_name(), at.time_zone().iana_name()) {
        (Some(zone), Some(here)) => zone == here,
        _ => zone == at.time_zone(),
    }
}

fn shift(zone: &TimeZone, at: &Zoned) -> i32 {
    zone.to_offset(at.timestamp()).seconds() - at.offset().seconds()
}

/// `None` for zero, which each caller names its own way.
fn signed(seconds: i32) -> Option<String> {
    if seconds == 0 {
        return None;
    }
    let sign = if seconds < 0 { '\u{2212}' } else { '+' };
    let seconds = seconds.abs();
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    Some(if minutes == 0 {
        format!("{sign}{hours}")
    } else {
        format!("{sign}{hours}:{minutes:02}")
    })
}

/// "America/New_York · EDT · 1 hour ahead of here · daylight time", where
/// here is `at`'s zone; the IANA name is left out for a zone without one.
/// Here's own zone has nothing to be ahead of, so it names its offset from
/// UTC instead, in `offset_label`'s notation. tzdb abbreviates many zones as a
/// bare "-05", which is dropped rather than shown beside it.
pub fn zone_detail(zone: &TimeZone, at: &Zoned) -> String {
    let info = zone.to_offset_info(at.timestamp());
    let kind = if info.dst().is_dst() {
        "daylight"
    } else {
        "standard"
    };
    let offset = match offset_label(info.offset()) {
        zero if zero == "UTC" => zero,
        label => format!("UTC{label}"),
    };
    let abbreviation = info.abbreviation();
    let numeric = abbreviation.starts_with(['+', '-']);
    let name = zone.iana_name();
    let mut parts = Vec::new();
    parts.extend(name);
    if !numeric && abbreviation != offset && Some(abbreviation) != name {
        parts.push(abbreviation);
    }
    let shift = shift_detail(zone, at);
    if !is_here(zone, at) {
        parts.push(&shift);
    } else if Some(offset.as_str()) != name {
        parts.push(&offset);
    }
    let tail = format!("{kind} time");
    parts.push(&tail);
    parts.join(" · ")
}

fn day(here: Date, there: Date) -> Option<String> {
    let days = there.since((Unit::Day, here)).ok()?.get_days();
    match days {
        0 => None,
        1 => Some("+1 day".to_string()),
        -1 => Some("−1 day".to_string()),
        days => Some(format!("{days:+} days")),
    }
}

/// "tomorrow", "yesterday" or "Tue 4 Nov": `date` as said on `today`. The
/// year is left out; a what-if rarely strays that far.
pub fn day_name(date: Date, today: Date) -> String {
    match date.since((Unit::Day, today)).map(|span| span.get_days()) {
        Ok(0) => "today".to_string(),
        Ok(1) => "tomorrow".to_string(),
        Ok(-1) => "yesterday".to_string(),
        _ => date.strftime("%a %-d %b").to_string(),
    }
}

/// The day a pinned reading in `zone` falls on, named from `real`, the real
/// clock held in here's zone. `None` while the pin is on today here, where
/// the reading speaks for itself. Named in `zone` so the date matches the
/// time beside it, even when here has already moved on.
pub fn pinned_day(pinned: &Zoned, zone: &TimeZone, real: &Zoned) -> Option<String> {
    let today = real.with_time_zone(pinned.time_zone().clone()).date();
    if pinned.date() == today {
        return None;
    }
    Some(day_name(pinned.with_time_zone(zone.clone()).date(), today))
}

/// When `zone`'s wall clock reads `text` on the day `reference` reads over
/// there, held in `reference`'s zone, home, so day hints stay relative to
/// home. The reference is the pin being edited, else now, so typing a time
/// keeps the day being explored. Both "HH:MM" and "h:mm am" are accepted
/// whatever the display format, so typing need not match the row.
pub fn pin(text: &str, zone: &TimeZone, reference: &Zoned) -> Option<Zoned> {
    let (hour, minute) = wall(text)?;
    let date = reference.with_time_zone(zone.clone()).date();
    // A time a DST change skips or repeats resolves to jiff's compatible
    // choice rather than failing.
    let there = date.at(hour, minute, 0, 0).to_zoned(zone.clone()).ok()?;
    Some(there.with_time_zone(reference.time_zone().clone()))
}

/// As a 24-hour clock. A trailing "am" or "pm", in any case and with or
/// without a space, makes the hour a 12-hour one.
fn wall(text: &str) -> Option<(i8, i8)> {
    let text = text.trim().to_ascii_lowercase();
    let (text, half) = match text.strip_suffix("am") {
        Some(rest) => (rest, Some(false)),
        None => match text.strip_suffix("pm") {
            Some(rest) => (rest, Some(true)),
            None => (text.as_str(), None),
        },
    };
    let (hour, minute) = text.split_once(':')?;
    let hour: i8 = hour.trim().parse().ok()?;
    let minute: i8 = minute.trim().parse().ok()?;
    let hour = match half {
        None => hour,
        Some(_) if !(1..=12).contains(&hour) => return None,
        Some(pm) => hour % 12 + if pm { 12 } else { 0 },
    };
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) {
        return None;
    }
    Some((hour, minute))
}

pub fn until_next_minute(now: &Zoned) -> Duration {
    // A leap second reads as :60, which is still inside the minute.
    let second = now.second().clamp(0, 59) as u64;
    let nanosecond = now.subsec_nanosecond().max(0) as u32;
    // The nanoseconds carry into a whole second when there are none to drop.
    Duration::new(59 - second, 1_000_000_000 - nanosecond) + PAD
}

/// A tzdb lookup cache. A zone this machine does not know is cached as `None`
/// rather than retried every minute.
#[derive(Default)]
pub struct Zones(HashMap<String, Option<TimeZone>>);

impl Zones {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn resolve(&mut self, timezone: &str) -> Option<&TimeZone> {
        if !self.0.contains_key(timezone) {
            self.0
                .insert(timezone.to_string(), TimeZone::get(timezone).ok());
        }
        self.get(timezone)
    }

    /// Only answers for zones already `resolve`d.
    pub fn get(&self, timezone: &str) -> Option<&TimeZone> {
        self.0.get(timezone).and_then(|zone| zone.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Span;

    /// An instant, read from the given zone as home.
    fn at(instant: &str, home: &str) -> Zoned {
        instant
            .parse::<jiff::Timestamp>()
            .expect("a valid timestamp")
            .to_zoned(zone(home))
    }

    fn zone(name: &str) -> TimeZone {
        TimeZone::get(name).expect("a zone this machine knows")
    }

    #[test]
    fn reads_the_time_in_another_zone() {
        let now = at("2026-08-15T12:34:56Z", "UTC");
        assert_eq!(
            reading(&now, &zone("UTC"), Format::TwentyFour).time,
            "12:34"
        );
        // +09:00, year round.
        assert_eq!(
            reading(&now, &zone("Asia/Tokyo"), Format::TwentyFour).time,
            "21:34"
        );
        // -07:00 in August.
        assert_eq!(
            reading(&now, &zone("America/Los_Angeles"), Format::TwentyFour).time,
            "05:34"
        );
    }

    #[test]
    fn pads_the_hour_and_the_minute() {
        let now = at("2026-08-15T09:05:00Z", "UTC");
        assert_eq!(
            reading(&now, &zone("UTC"), Format::TwentyFour).time,
            "09:05"
        );
        assert_eq!(
            reading(&now, &zone("Europe/London"), Format::TwentyFour).time,
            "10:05"
        );
    }

    #[test]
    fn reads_a_twelve_hour_clock() {
        for (instant, expected) in [
            ("2026-08-15T00:05:00Z", "12:05 am"),
            ("2026-08-15T09:05:00Z", "9:05 am"),
            ("2026-08-15T11:59:00Z", "11:59 am"),
            ("2026-08-15T12:00:00Z", "12:00 pm"),
            ("2026-08-15T21:34:00Z", "9:34 pm"),
            ("2026-08-15T23:59:00Z", "11:59 pm"),
        ] {
            let now = at(instant, "UTC");
            assert_eq!(reading(&now, &zone("UTC"), Format::Twelve).time, expected);
        }
    }

    #[test]
    fn the_same_date_needs_no_hint() {
        let now = at("2026-08-15T12:00:00Z", "UTC");
        assert_eq!(
            reading(&now, &zone("Europe/Paris"), Format::TwentyFour).day,
            None
        );
    }

    #[test]
    fn a_zone_far_enough_east_is_a_day_ahead() {
        // 22:00 UTC is already the 16th at +14:00.
        let now = at("2026-08-15T22:00:00Z", "UTC");
        let there = reading(&now, &zone("Pacific/Kiritimati"), Format::TwentyFour);
        assert_eq!(there.time, "12:00");
        assert_eq!(there.day.as_deref(), Some("+1 day"));
    }

    #[test]
    fn a_zone_far_enough_west_is_a_day_behind() {
        // Just past midnight UTC is still the 14th in California.
        let now = at("2026-08-15T00:30:00Z", "UTC");
        let there = reading(&now, &zone("America/Los_Angeles"), Format::TwentyFour);
        assert_eq!(there.time, "17:30");
        assert_eq!(there.day.as_deref(), Some("−1 day"));
    }

    #[test]
    fn the_hint_is_relative_to_home_not_to_utc() {
        // Home in Tokyo: it is the 16th here, so London is the one behind and
        // Kiritimati, a day ahead of UTC, is level with us.
        let now = at("2026-08-15T22:00:00Z", "Asia/Tokyo");
        assert_eq!(now.date().to_string(), "2026-08-16");
        assert_eq!(
            reading(&now, &zone("Europe/London"), Format::TwentyFour)
                .day
                .as_deref(),
            Some("−1 day")
        );
        assert_eq!(
            reading(&now, &zone("Pacific/Kiritimati"), Format::TwentyFour).day,
            None
        );
    }

    #[test]
    fn a_fixed_offset_zone_reads_the_same_way() {
        let now = at("2026-08-15T23:15:00Z", "UTC");
        let there = reading(
            &now,
            &TimeZone::fixed(jiff::tz::offset(5)),
            Format::TwentyFour,
        );
        assert_eq!(there.time, "04:15");
        assert_eq!(there.day.as_deref(), Some("+1 day"));
    }

    #[test]
    fn an_unknown_zone_does_not_resolve() {
        assert!(TimeZone::get("Mars/Olympus_Mons").is_err());
    }

    #[test]
    fn pins_a_time_in_another_zone() {
        // 12:00Z is 21:00 in Tokyo, so "today there" is still the 15th; 09:30
        // that morning in Tokyo was 00:30Z.
        let now = at("2026-08-15T12:00:00Z", "UTC");
        let pinned = pin("09:30", &zone("Asia/Tokyo"), &now).expect("a valid pin");
        assert_eq!(
            reading(&pinned, &zone("Asia/Tokyo"), Format::TwentyFour).time,
            "09:30"
        );
        assert_eq!(
            reading(&pinned, &zone("UTC"), Format::TwentyFour).time,
            "00:30"
        );
    }

    #[test]
    fn a_pin_keeps_home_as_the_day_reference() {
        // Kiritimati is already on the 16th; pinning 09:00 there lands at
        // 19:00Z on the 15th, so from home it reads as a day ahead.
        let now = at("2026-08-15T12:00:00Z", "UTC");
        let pinned = pin("09:00", &zone("Pacific/Kiritimati"), &now).expect("a valid pin");
        let there = reading(&pinned, &zone("Pacific/Kiritimati"), Format::TwentyFour);
        assert_eq!(there.time, "09:00");
        assert_eq!(there.day.as_deref(), Some("+1 day"));
    }

    #[test]
    fn a_time_skipped_by_dst_still_pins() {
        // New York's clocks jump from 02:00 to 03:00 on 2026-03-08; 02:30
        // resolves to the other side of the gap instead of failing.
        let now = at("2026-03-08T12:00:00Z", "UTC");
        let pinned = pin("02:30", &zone("America/New_York"), &now).expect("a valid pin");
        assert_eq!(
            reading(&pinned, &zone("America/New_York"), Format::TwentyFour).time,
            "03:30"
        );
    }

    #[test]
    fn pins_a_twelve_hour_time() {
        let now = at("2026-08-15T12:00:00Z", "UTC");
        for (text, expected) in [
            ("9:30 am", "09:30"),
            ("9:30 pm", "21:30"),
            ("12:00 am", "00:00"),
            ("12:00 pm", "12:00"),
            ("9:30PM", "21:30"),
            (" 9:30 Pm ", "21:30"),
        ] {
            let pinned = pin(text, &zone("UTC"), &now).expect(text);
            assert_eq!(
                reading(&pinned, &zone("UTC"), Format::TwentyFour).time,
                expected,
                "{text:?}"
            );
        }
    }

    #[test]
    fn text_that_is_not_a_time_does_not_pin() {
        let now = at("2026-08-15T12:00:00Z", "UTC");
        for text in [
            "", "noon", "12", "25:00", "12:60", "-1:30", "1:2:3", "0:30 am", "13:00 pm", "9:30 xm",
        ] {
            assert_eq!(pin(text, &zone("UTC"), &now), None, "{text:?}");
        }
    }

    #[test]
    fn a_pin_keeps_the_reference_day() {
        // Pinned on tomorrow, 2026-11-02 at 06:45 here; 09:00 typed into
        // Quito stays on the 2nd rather than jumping back to the 1st.
        let reference = at("2026-11-02T11:45:00Z", "America/Guayaquil");
        let pinned = pin("09:00", &zone("America/Guayaquil"), &reference).expect("a valid pin");
        assert_eq!(
            pinned.to_string(),
            "2026-11-02T09:00:00-05:00[America/Guayaquil]"
        );
        // The day is the reference's over there: 23:00Z on the 1st is
        // already the 2nd in Tokyo, so 09:00 there is the 2nd's.
        let reference = at("2026-11-01T23:00:00Z", "America/Guayaquil");
        let pinned = pin("09:00", &zone("Asia/Tokyo"), &reference).expect("a valid pin");
        assert_eq!(pinned.timestamp().to_string(), "2026-11-02T00:00:00Z");
    }

    #[test]
    fn days_near_today_are_named_and_the_rest_dated() {
        let today = jiff::civil::date(2026, 11, 1);
        let day = |days: i64| today.checked_add(Span::new().days(days)).expect("a date");
        assert_eq!(day_name(day(0), today), "today");
        assert_eq!(day_name(day(1), today), "tomorrow");
        assert_eq!(day_name(day(-1), today), "yesterday");
        assert_eq!(day_name(day(3), today), "Wed 4 Nov");
        assert_eq!(day_name(day(-2), today), "Fri 30 Oct");
        assert_eq!(day_name(day(40), today), "Fri 11 Dec");
    }

    #[test]
    fn a_pin_on_today_here_names_no_day() {
        let real = at("2026-11-01T16:45:00Z", "America/Guayaquil");
        let pinned = at("2026-11-02T04:00:00Z", "America/Guayaquil");
        // 23:00 here on the 1st, already the 2nd in Tokyo: still today here.
        assert_eq!(pinned_day(&pinned, &zone("Asia/Tokyo"), &real), None);
        assert_eq!(pinned_day(&pinned, &zone("America/Guayaquil"), &real), None);
    }

    #[test]
    fn a_pin_past_midnight_here_names_its_day() {
        let real = at("2026-11-01T16:45:00Z", "America/Guayaquil");
        // 01:45 here on the 2nd.
        let pinned = at("2026-11-02T06:45:00Z", "America/Guayaquil");
        let here = zone("America/Guayaquil");
        assert_eq!(
            pinned_day(&pinned, &here, &real).as_deref(),
            Some("tomorrow")
        );
        // Read in Tokyo it is 15:45 on the 2nd: named by Tokyo's date.
        assert_eq!(
            pinned_day(&pinned, &zone("Asia/Tokyo"), &real).as_deref(),
            Some("tomorrow")
        );
        // Three days on, dated.
        let pinned = at("2026-11-04T14:00:00Z", "America/Guayaquil");
        assert_eq!(
            pinned_day(&pinned, &here, &real).as_deref(),
            Some("Wed 4 Nov")
        );
        // A day back.
        let pinned = at("2026-11-01T02:00:00Z", "America/Guayaquil");
        assert_eq!(
            pinned_day(&pinned, &here, &real).as_deref(),
            Some("yesterday")
        );
    }

    #[test]
    fn the_wait_runs_to_the_top_of_the_next_minute() {
        let now = at("2026-08-15T12:00:00Z", "UTC");
        assert_eq!(until_next_minute(&now), Duration::from_millis(60_050));

        let now = at("2026-08-15T12:00:59Z", "UTC");
        assert_eq!(until_next_minute(&now), Duration::from_millis(1_050));

        let now = at("2026-08-15T12:00:30.250Z", "UTC");
        assert_eq!(until_next_minute(&now), Duration::from_millis(29_800));
    }

    #[test]
    fn hour_labels_drop_round_minutes_only_in_twelve_hour_time() {
        for (hour, minute, expected) in [(0, 0, "00:00"), (9, 30, "09:30"), (23, 0, "23:00")] {
            assert_eq!(hour_label(hour, minute, Format::TwentyFour), expected);
        }
        for (hour, minute, expected) in [
            (0, 0, "12 am"),
            (9, 0, "9 am"),
            (5, 30, "5:30 am"),
            (12, 0, "12 pm"),
            (13, 45, "1:45 pm"),
            (23, 0, "11 pm"),
        ] {
            assert_eq!(hour_label(hour, minute, Format::Twelve), expected);
        }
    }

    #[test]
    fn offsets_read_as_signed_hours_with_minutes_when_off_the_hour() {
        assert_eq!(
            offset_label(Offset::from_seconds(0).expect("a valid offset")),
            "UTC"
        );
        assert_eq!(
            offset_label(Offset::from_seconds(3600).expect("a valid offset")),
            "+1"
        );
        assert_eq!(
            offset_label(Offset::from_seconds(-5 * 3600).expect("a valid offset")),
            "\u{2212}5"
        );
        assert_eq!(
            offset_label(Offset::from_seconds(5 * 3600 + 1800).expect("a valid offset")),
            "+5:30"
        );
        assert_eq!(
            offset_label(Offset::from_seconds(-(3 * 3600 + 1800)).expect("a valid offset")),
            "\u{2212}3:30"
        );
    }

    #[test]
    fn zone_detail_names_the_abbreviation_and_daylight_status() {
        let july = at("2026-07-15T12:00:00Z", "America/New_York");
        assert_eq!(
            zone_detail(&zone("America/New_York"), &july),
            "America/New_York · EDT · UTC\u{2212}4 · daylight time"
        );
        let january = at("2026-01-15T12:00:00Z", "America/New_York");
        assert_eq!(
            zone_detail(&zone("America/New_York"), &january),
            "America/New_York · EST · UTC\u{2212}5 · standard time"
        );
    }

    #[test]
    fn zone_detail_drops_a_numeric_abbreviation_for_the_offset() {
        // tzdb calls Ecuador "-05"; the detail reads as the grid head does.
        let now = at("2026-07-15T12:00:00Z", "America/Guayaquil");
        let detail = zone_detail(&zone("America/Guayaquil"), &now);
        assert_eq!(detail, "America/Guayaquil · UTC\u{2212}5 · standard time");
        let now = at("2026-07-15T12:00:00Z", "Asia/Kathmandu");
        assert_eq!(
            zone_detail(&zone("Asia/Kathmandu"), &now),
            "Asia/Kathmandu · UTC+5:45 · standard time"
        );
    }

    #[test]
    fn zone_detail_does_not_repeat_utc() {
        let now = at("2026-07-15T12:00:00Z", "UTC");
        assert_eq!(zone_detail(&TimeZone::UTC, &now), "UTC · standard time");
    }

    #[test]
    fn other_zones_are_named_from_here() {
        let now = at("2026-07-15T12:00:00Z", "America/Guayaquil");
        assert_eq!(
            zone_detail(&zone("Asia/Kathmandu"), &now),
            "Asia/Kathmandu · 10 hours 45 minutes ahead of here · standard time"
        );
        assert_eq!(
            zone_detail(&zone("America/Los_Angeles"), &now),
            "America/Los_Angeles · PDT · 2 hours behind here · daylight time"
        );
        // Level with here in July, but a different zone, so not here.
        assert_eq!(
            zone_detail(&zone("America/Chicago"), &now),
            "America/Chicago · CDT · same time as here · daylight time"
        );
    }

    #[test]
    fn heads_read_hours_from_here() {
        let now = at("2026-07-15T12:00:00Z", "America/Guayaquil");
        assert_eq!(shift_label(&zone("Pacific/Kiritimati"), &now), "+19");
        assert_eq!(shift_label(&zone("Asia/Kathmandu"), &now), "+10:45");
        assert_eq!(shift_label(&zone("America/Los_Angeles"), &now), "\u{2212}2");
        assert_eq!(shift_label(&zone("America/Guayaquil"), &now), "±0");
        assert_eq!(
            shift_detail(&zone("America/St_Johns"), &now),
            "2 hours 30 minutes ahead of here"
        );
        assert_eq!(
            shift_detail(
                &zone("Asia/Tokyo"),
                &at("2026-07-15T12:00:00Z", "Asia/Seoul")
            ),
            "same time as here"
        );
    }

    #[test]
    fn a_zone_loaded_by_name_is_here_when_the_names_match() {
        let now = at("2026-07-15T12:00:00Z", "America/Guayaquil");
        assert!(is_here(&zone("America/Guayaquil"), &now));
        assert!(!is_here(&zone("America/Bogota"), &now));
    }

    #[test]
    fn twelve_hour_halves_are_lower_case_everywhere() {
        let now = at("2026-08-15T21:00:00Z", "UTC");
        let list = reading(&now, &zone("UTC"), Format::Twelve).time;
        assert_eq!(list, "9:00 pm");
        assert_eq!(hour_label(21, 0, Format::Twelve), "9 pm");
        assert_eq!(hour_label(21, 30, Format::Twelve), "9:30 pm");
    }
}
