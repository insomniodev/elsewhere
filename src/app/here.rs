//! The pieces every view draws "here" and the empty state with, so they read
//! the same in the list, on the map and in the grid.

use gpui::{Div, SharedString, Stateful, div, prelude::*, rgb};
use jiff::Zoned;
use jiff::tz::TimeZone;

use super::Elsewhere;
use super::tooltip::Tooltip;
use crate::clock;
use crate::here::{self, Here};
use crate::search::SearchIndex;
use crate::theme;

pub(super) const HINT: &str = "search for a city to start your list";

/// `known` is the model this one replaces: its seat is reused while the zone
/// holds, since finding one scans the whole dataset.
pub(super) fn locate(
    index: &SearchIndex,
    saved: &[u32],
    zone: TimeZone,
    known: Option<&Here>,
) -> Here {
    let seat = match known {
        Some(known) if known.zone.iana_name() == zone.iana_name() => known.seat,
        _ => zone
            .iana_name()
            .and_then(|iana| here::seat(iana, index.cities())),
    };
    let saved = saved.iter().filter_map(|geonameid| index.city(*geonameid));
    Here::new(zone, saved, seat)
}

/// A day tag, or the "here" mark: small and stepped back beside a name.
pub(super) fn tag(text: impl Into<SharedString>) -> Div {
    div()
        .flex_none()
        .text_xs()
        .text_color(rgb(theme::OVERLAY2))
        .child(text.into())
}

/// Shown by every view while nothing is saved. The list sets it in a row's
/// place; the others lay it over their content with `hint_over`.
pub(super) fn hint() -> Div {
    div().text_color(rgb(theme::OVERLAY2)).child(HINT)
}

/// `hint` centred in whatever absolute box the caller gives it, on a `BASE`
/// pill so it stays legible over the map's land and night.
pub(super) fn hint_over(area: Div) -> Div {
    area.absolute()
        .flex()
        .items_center()
        .justify_center()
        .p_2()
        .child(
            hint()
                .text_center()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(theme::BASE)),
        )
}

impl Elsewhere {
    /// Re-read at the top of each render from the instant that render shows,
    /// so every view in the pass agrees and an added or removed city takes or
    /// gives up the face at once.
    pub(super) fn refresh_here(&mut self, now: &Zoned) {
        self.here = locate(
            &self.index,
            &self.saved,
            now.time_zone().clone(),
            Some(&self.here),
        );
    }

    /// The "here" mark, whose tooltip names the zone: the only place its IANA
    /// name and offset from UTC show when the face is a zone rather than a
    /// city.
    pub(super) fn here_tag(&self, id: &'static str, now: &Zoned) -> Stateful<Div> {
        tag("here")
            .id(id)
            .tooltip(Tooltip::text(self.here_detail(now)))
    }

    /// `city_detail` for here: under the face's full label when a saved city
    /// is the face, so its name says what that city's own does.
    pub(super) fn here_detail(&self, now: &Zoned) -> String {
        let name = match self.here.city.and_then(|id| self.index.city(id)) {
            Some(city) => self.index.label(city),
            None => self.here.name.clone(),
        };
        clock::city_detail(&name, Some(&self.here.zone), now)
    }
}
