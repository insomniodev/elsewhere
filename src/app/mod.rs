use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Axis, Context, Entity, Pixels, ScrollHandle, SharedString, Subscription, Task, Window, actions,
    div, prelude::*, px, rgb,
};
use jiff::{RoundMode, Span, Unit, Zoned, ZonedRound};

mod button;
mod drag;
mod footer;
mod grid;
mod here;
mod map;
mod notice;
mod resize;
mod row;
mod scrollbar;
pub(crate) mod search;
pub(crate) mod titlebar;
mod tooltip;

use crate::cities;
use crate::clock;
use crate::geo;
use crate::here::Here;
use crate::land;
use crate::saved;
use crate::search::SearchIndex;
use crate::settings::{self, Settings};
use crate::theme;
use crate::vendor::text_input::TextInput;
use crate::zones;
use notice::{Kind, Notice};
use scrollbar::scrollbar;
use search::{SearchEvent, SearchPicker};

// The vendored input claims neither key, so they bubble up to the
// "PinEditor" wrapper.
actions!(pin_editor, [Commit, Cancel]);

// Window-wide. The search picker lets escape through only when it has
// nothing of its own to clear.
actions!(elsewhere, [Unpin, Later, Earlier]);

/// One ctrl+wheel notch over the map, or one press of Later or Earlier.
const STEP_MINUTES: i64 = 15;

/// How long the window must hold still before its bounds are written, so a
/// drag writes once rather than every frame.
const BOUNDS_SETTLE: Duration = Duration::from_millis(500);

/// Between the window's stacked parts. The map reads it, with the banner's
/// and footer's heights, to know the room it has without them.
const GAP: Pixels = px(12.);
/// Under the footer.
const PAD_BOTTOM: Pixels = px(16.);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum View {
    #[default]
    List,
    Map,
    Grid,
}

/// A what-if time in effect.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pin {
    /// The row the time was typed into. `None` for a scrub on the map.
    pub(crate) anchor: Option<u32>,
    /// The instant every row reads from, held in home's zone.
    pub(crate) at: Zoned,
}

/// A what-if time's readings, banner fill and banner edge; the last two carry
/// their alpha.
pub(crate) struct Accent {
    pub(crate) text: u32,
    pub(crate) tint: u32,
    pub(crate) edge: u32,
}

impl Pin {
    /// A pin outlives its row's removal by reading as here, which keeps the
    /// instant the user was looking at rather than dropping back to now.
    pub(crate) fn forget(&mut self, geonameid: u32) {
        if self.anchor == Some(geonameid) {
            self.anchor = None;
        }
    }

    /// Typed and scrubbed times differ in color so they read apart at a glance.
    pub(crate) fn accent(&self) -> Accent {
        match self.anchor {
            Some(_) => Accent {
                text: theme::YELLOW,
                tint: theme::YELLOW_TINT,
                edge: theme::YELLOW_EDGE,
            },
            None => Accent {
                text: theme::MAUVE,
                tint: theme::MAUVE_TINT,
                edge: theme::MAUVE_EDGE,
            },
        }
    }
}

pub struct Elsewhere {
    picker: Entity<SearchPicker>,
    index: Rc<SearchIndex>,
    /// Geonameids, in the order the user arranged.
    saved: Vec<u32>,
    /// Why the saved file could not be read. It exists, so writing `saved`
    /// over it would lose its contents.
    saved_unreadable: Option<SharedString>,
    /// Likewise for the settings file.
    settings_unreadable: Option<SharedString>,
    /// What the settings file holds, so only a change is written.
    settings_written: Settings,
    /// Restarted by each move or resize; dropping it cancels the write.
    settings_settle: Option<Task<()>>,
    _bounds: Subscription,
    notice: Option<Notice>,
    /// Dropping it keeps the notice up.
    notice_timer: Option<Task<()>>,
    zones: clock::Zones,
    /// What every view calls this machine's zone, refreshed each render.
    here: Here,
    /// The row being dragged, by geonameid.
    drag: Option<u32>,
    /// The row washed to show where it is, until `reveal_timer` clears it.
    revealed: Option<u32>,
    /// Whether the list still has to scroll to `revealed`, which it can only
    /// do from the frame's order.
    reveal_pending: bool,
    reveal_timer: Option<Task<()>>,
    // View preferences, kept in the settings file with the window's bounds.
    view: View,
    /// Ordered by longitude rather than by hand.
    westward: bool,
    format: clock::Format,
    /// `None` follows the real clock.
    pinned: Option<Pin>,
    /// `pinned` as it was when the editor opened, live time included, so
    /// cancelling puts back exactly that. `None` while no editor is open.
    pre_edit: Option<Option<Pin>>,
    /// The row being typed over, and its input.
    editing: Option<(u32, Entity<TextInput>)>,
    /// Doubles as the list's bounds, which the scrollbar reads.
    saved_scroll: ScrollHandle,
    /// Shared because the map's canvas closures outlive the render borrow.
    land: Rc<Vec<Vec<geo::LonLat>>>,
    /// Shared for the same reason as `land`.
    zone_borders: Rc<Vec<Vec<geo::LonLat>>>,
    /// Written by the map canvas in prepaint, the only place the letterbox is
    /// known, for whatever is laid over the map to read back.
    map_layout: Rc<Cell<Option<map::Layout>>>,
    map_scroll: ScrollHandle,
    /// The map's zoom and middle, kept between fits. `None` fits it to the
    /// cities on the next frame. Shared for the same reason as `map_layout`.
    map_frame: Rc<Cell<Option<map::Frame>>>,
    /// A drag on the map's body in progress. Shared with the listeners that
    /// follow the pointer outside the map.
    map_pan: Rc<Cell<Option<map::Pan>>>,
    grid_scroll: ScrollHandle,
    /// The grid's vertical scroll through the day.
    grid_hours: ScrollHandle,
    /// Whether the hours have scrolled to the shown instant since the grid
    /// opened. Shared because the grid's canvas sets it.
    grid_at_now: Rc<Cell<bool>>,
    /// Wheel motion over the map that has not yet added up to a whole notch.
    scrub_carry: f32,
    /// Whether a plain wheel over the map has said how to scrub, which it
    /// does once a session.
    scrub_hinted: bool,
    /// The system's mono face for the times, if it has one.
    mono: Option<SharedString>,
    /// Dropping it stops the clock.
    _tick: Task<()>,
}

impl Elsewhere {
    /// `settings` comes from `main`, which read it to place the window.
    pub fn new(
        settings: Result<Settings, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let index = Rc::new(SearchIndex::new(cities::load()));
        let picker = cx.new(|cx| SearchPicker::new(index.clone(), window, cx));
        cx.subscribe(&picker, Self::picked).detach();

        let loaded = saved::load();
        let saved_unreadable = loaded.as_ref().err().map(|e| unreadable(e, "changes"));
        let mut saved = loaded.unwrap_or_default();
        let settings_unreadable = settings
            .as_ref()
            .err()
            .map(|e| unreadable(e, "preferences"));
        let settings = settings.unwrap_or_default();
        // A regenerated dataset may have dropped a city saved by an older run.
        saved.retain(|id| index.city(*id).is_some());
        // Up front, so restored rows show a time on the first frame.
        let mut zones = clock::Zones::new();
        for geonameid in &saved {
            if let Some(city) = index.city(*geonameid) {
                zones.resolve(&city.timezone);
            }
        }

        let here = here::locate(&index, &saved, Zoned::now().time_zone().clone(), None);

        let bounds = cx.observe_window_bounds(window, |this, window, cx| {
            this.settings_settle = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(BOUNDS_SETTLE).await;
                this.update_in(cx, |this, window, cx| this.persist_settings(window, cx))
                    .ok();
            }));
        });
        // The list matters more, so its warning wins the one slot.
        let notice = saved_unreadable
            .clone()
            .or_else(|| settings_unreadable.clone())
            .map(|text| Notice::new(Kind::Failure, text));

        Self {
            picker,
            index,
            saved,
            saved_unreadable,
            settings_unreadable,
            view: settings.view,
            westward: settings.westward,
            format: settings.format,
            settings_written: settings,
            settings_settle: None,
            _bounds: bounds,
            notice,
            notice_timer: None,
            zones,
            here,
            drag: None,
            revealed: None,
            reveal_pending: false,
            reveal_timer: None,
            pinned: None,
            pre_edit: None,
            editing: None,
            saved_scroll: ScrollHandle::new(),
            land: Rc::new(land::load()),
            zone_borders: Rc::new(zones::load()),
            map_layout: Rc::new(Cell::new(None)),
            map_scroll: ScrollHandle::new(),
            map_frame: Rc::new(Cell::new(None)),
            map_pan: Rc::new(Cell::new(None)),
            grid_scroll: ScrollHandle::new(),
            grid_hours: ScrollHandle::new(),
            grid_at_now: Rc::new(Cell::new(false)),
            scrub_carry: 0.,
            scrub_hinted: false,
            mono: row::mono(cx),
            _tick: tick(cx),
        }
    }

    fn picked(
        &mut self,
        _picker: Entity<SearchPicker>,
        event: &SearchEvent,
        cx: &mut Context<Self>,
    ) {
        let SearchEvent::Picked(geonameid) = event;
        self.save(*geonameid, cx);
    }

    /// Writes the list to disk, unless the file could not be read at startup,
    /// in which case the warning comes back if it was dismissed.
    pub(super) fn persist(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = &self.saved_unreadable {
            if self.notice.is_none() {
                self.show_notice(Notice::new(Kind::Failure, text.clone()), cx);
            }
            return;
        }
        if let Err(e) = saved::save(&self.saved) {
            let notice =
                Notice::new(Kind::Failure, e).action("retry", |this, _window, cx| this.persist(cx));
            self.show_notice(notice, cx);
        }
    }

    /// Writes the view preferences and the window's bounds, if they changed
    /// since the last write. Guarded like `persist`.
    pub(super) fn persist_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The restore bounds while maximized, which is what a relaunch wants.
        let bounds = window.window_bounds().get_bounds();
        let settings = Settings {
            view: self.view,
            westward: self.westward,
            format: self.format,
            size: Some(bounds.size),
            origin: Some(bounds.origin),
        };
        if settings == self.settings_written {
            return;
        }
        if let Some(text) = &self.settings_unreadable {
            if self.notice.is_none() {
                self.show_notice(Notice::new(Kind::Failure, text.clone()), cx);
            }
            return;
        }
        match settings::save(&settings) {
            Ok(()) => self.settings_written = settings,
            Err(e) => {
                let notice = Notice::new(Kind::Failure, e).action("retry", |this, window, cx| {
                    this.persist_settings(window, cx)
                });
                self.show_notice(notice, cx);
            }
        }
    }

    fn save(&mut self, geonameid: u32, cx: &mut Context<Self>) {
        if !self.saved.contains(&geonameid) {
            self.saved.push(geonameid);
            if let Some(city) = self.index.city(geonameid) {
                self.zones.resolve(&city.timezone);
            }
            self.persist(cx);
        }
        // New or already there, a pick shows where its row is: westward drops
        // a new one wherever its longitude falls, and a saved one may be off
        // screen.
        self.reveal_row(geonameid, cx);
    }

    /// Moves the shown instant `steps` quarter hours. A pin moves by whole
    /// quarters from where it is; from the real clock the first step snaps to
    /// the quarter boundary in the direction of travel, so from 12:04 Later
    /// lands on 12:15 and Earlier on 12:00, and the rest follow 15 minutes
    /// apart. The map's scrub steps through here too. A step has no row
    /// behind it, so it keeps any anchor already pinned and otherwise reads
    /// as here.
    pub(super) fn nudge(&mut self, steps: i64, cx: &mut Context<Self>) {
        let anchor = self.pinned.as_ref().and_then(|pin| pin.anchor);
        let at = match &self.pinned {
            Some(pin) => pin
                .at
                .checked_add(Span::new().minutes(steps * STEP_MINUTES))
                .ok(),
            None => from_live(&Zoned::now(), steps),
        };
        let Some(at) = at else {
            return;
        };
        self.pinned = Some(Pin { anchor, at });
        cx.notify();
    }

    /// Taken as an editor opens, for `restore_pin`.
    pub(super) fn snapshot_pin(&mut self) {
        self.pre_edit = Some(self.pinned.clone());
    }

    /// Puts back the pin, or live time, from before the editor opened.
    pub(super) fn restore_pin(&mut self) {
        if let Some(pinned) = self.pre_edit.take() {
            self.pinned = pinned;
        }
    }

    /// An open editor would keep showing the typed time, so it closes first.
    fn step(&mut self, steps: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.close_editor(window, cx);
        self.nudge(steps, cx);
    }
}

/// The warning for a file that exists but could not be read, saying what is
/// at stake.
fn unreadable(error: &str, what: &str) -> SharedString {
    format!("{error}; {what} won't be saved").into()
}

/// `steps` quarter hours from the real clock, the first of them to the next
/// quarter boundary that way: from 12:04, +1 is 12:15 and −1 is 12:00. A time
/// already on a boundary moves a whole quarter, so a step always moves.
fn from_live(now: &Zoned, steps: i64) -> Option<Zoned> {
    let quarter = |n: i64| Span::new().minutes(n * STEP_MINUTES);
    let floor = now
        .round(
            ZonedRound::new()
                .smallest(Unit::Minute)
                .increment(STEP_MINUTES)
                .mode(RoundMode::Trunc),
        )
        .ok()?;
    let boundary = match steps.signum() {
        0 => return Some(now.clone()),
        1 => floor.checked_add(quarter(1)).ok()?,
        _ if floor == *now => floor.checked_sub(quarter(1)).ok()?,
        _ => floor,
    };
    boundary.checked_add(quarter(steps - steps.signum())).ok()
}

/// Re-renders on each minute boundary rather than polling.
fn tick(cx: &mut Context<Elsewhere>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(clock::until_next_minute(&Zoned::now()))
                .await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                // The window is gone.
                break;
            }
        }
    })
}

impl Render for Elsewhere {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // gpui refreshes the window once a drag ends, however it ended, so this
        // is where a cancelled drag is forgotten.
        let dragging = cx.has_active_drag();
        if !dragging {
            self.drag = None;
        }

        // One reading for the whole pass, so the rows agree. A pin simply
        // stands in for the clock.
        let now = self
            .pinned
            .as_ref()
            .map_or_else(Zoned::now, |pin| pin.at.clone());
        self.refresh_here(&now);
        // Derived here so the stored order stays the hand-arranged one.
        let order: Vec<u32> = if self.westward {
            saved::westward(
                self.saved
                    .iter()
                    .filter_map(|geonameid| {
                        let city = self.index.city(*geonameid)?;
                        Some((*geonameid, city.longitude))
                    })
                    .collect(),
            )
        } else {
            self.saved.clone()
        };
        // A search in progress floats its results over the list, so the hint
        // waits until the input is clear again.
        let empty = order.is_empty() && !self.picker.read(cx).searching();
        self.picker
            .update(cx, |picker, _| picker.set_saved(&self.saved));
        // A saved city in the zone stands in for here's row.
        let here_row = self.here.city.is_none();
        if self.reveal_pending {
            self.reveal_pending = false;
            if let Some(i) = self
                .revealed
                .and_then(|id| order.iter().position(|o| *o == id))
            {
                // Rows follow here's row, if it is shown. Held by the handle
                // until the list is next laid out, should another view be up.
                self.saved_scroll.scroll_to_item(i + usize::from(here_row));
            }
        }
        div()
            .key_context("Elsewhere")
            .on_action(cx.listener(|this, _: &Unpin, window, cx| this.unpin(window, cx)))
            .on_action(cx.listener(|this, _: &Later, window, cx| this.step(1, window, cx)))
            .on_action(cx.listener(|this, _: &Earlier, window, cx| this.step(-1, window, cx)))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .gap(GAP)
            // The title bar sits in what would be the top padding.
            .px_4()
            .pt_1()
            .pb(PAD_BOTTOM)
            .bg(rgb(theme::BASE))
            .text_color(rgb(theme::TEXT))
            .child(self.render_titlebar(window, cx))
            .child(self.picker.clone())
            .children(self.render_banner(&now, cx))
            // Scrolls on its own so the input and toolbar stay put. `min_h_0`
            // lets this flex child shrink below its content and overflow.
            .child(div().relative().flex_1().min_h_0().map(|region| {
                match self.view {
                    View::Map => region.child(self.render_map(
                        &self.saved,
                        &now,
                        empty,
                        dragging,
                        window,
                        cx,
                    )),
                    View::Grid => region.child(self.render_grid(&order, &now, empty, dragging, cx)),
                    View::List => {
                        region
                            .child(
                                div()
                                    .id("saved")
                                    .size_full()
                                    .flex()
                                    .flex_col()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.saved_scroll)
                                    .when(here_row, |list| list.child(self.render_here_row(&now)))
                                    .when(empty, |list| list.child(self.render_empty()))
                                    .children(order.iter().enumerate().filter_map(
                                        |(i, geonameid)| {
                                            let first = i == 0 && !here_row;
                                            let last = i + 1 == order.len();
                                            self.render_row(
                                                *geonameid, &now, dragging, first, last, cx,
                                            )
                                        },
                                    ))
                                    .when(
                                        !self.westward && dragging && self.drag.is_some(),
                                        |list| list.child(self.render_drop_tail(cx)),
                                    ),
                            )
                            // Hung out in the window padding, clear of the times.
                            .child(scrollbar(
                                "saved-bar",
                                &self.saved_scroll,
                                Axis::Vertical,
                                px(-6.),
                            ))
                    }
                }
            }))
            .children(self.render_notice(cx))
            .child(self.render_footer(dragging, cx))
            // Last, so they lie over everything else.
            .children(resize::handles(window))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(datetime: &str) -> Zoned {
        format!("{datetime}[America/Guayaquil]")
            .parse()
            .expect("a valid zoned datetime")
    }

    fn time(zoned: Option<Zoned>) -> String {
        zoned.expect("a step").time().to_string()
    }

    #[test]
    fn the_first_step_from_live_lands_on_the_next_quarter_that_way() {
        let now = at("2026-11-01T12:04:30");
        assert_eq!(time(from_live(&now, 1)), "12:15:00");
        assert_eq!(time(from_live(&now, -1)), "12:00:00");
        // The rest are whole quarters on from there.
        assert_eq!(time(from_live(&now, 3)), "12:45:00");
        assert_eq!(time(from_live(&now, -3)), "11:30:00");
    }

    #[test]
    fn a_live_time_on_a_boundary_still_moves_a_quarter() {
        let now = at("2026-11-01T12:00:00");
        assert_eq!(time(from_live(&now, 1)), "12:15:00");
        assert_eq!(time(from_live(&now, -1)), "11:45:00");
    }

    #[test]
    fn the_first_step_crosses_midnight() {
        let now = at("2026-11-01T23:50:00");
        let next = from_live(&now, 1).expect("a step");
        assert_eq!(
            next.to_string(),
            "2026-11-02T00:00:00-05:00[America/Guayaquil]"
        );
    }

    #[test]
    fn removing_the_anchor_keeps_the_instant_as_here() {
        let at = at("2026-11-02T01:45:00");
        let mut pin = Pin {
            anchor: Some(3652462),
            at: at.clone(),
        };
        pin.forget(2643743);
        assert_eq!(pin.anchor, Some(3652462));
        pin.forget(3652462);
        assert_eq!(pin, Pin { anchor: None, at });
    }
}
