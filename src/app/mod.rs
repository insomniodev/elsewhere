use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Axis, Context, Entity, ScrollHandle, SharedString, Task, Window, actions, div, prelude::*, px,
    rgb,
};
use jiff::{RoundMode, Span, Unit, Zoned, ZonedRound};

mod drag;
mod footer;
mod grid;
mod map;
mod resize;
mod row;
mod scrollbar;
pub(crate) mod search;
pub(crate) mod titlebar;
mod tooltip;

use crate::cities;
use crate::clock;
use crate::geo;
use crate::land;
use crate::saved;
use crate::search::SearchIndex;
use crate::theme;
use crate::vendor::text_input::TextInput;
use crate::zones;
use scrollbar::scrollbar;
use search::{SearchEvent, SearchPicker};

// The vendored input claims neither key, so they bubble up to the
// "PinEditor" wrapper.
actions!(pin_editor, [Commit, Cancel]);

// Window-wide. The search picker lets escape through only when it has
// nothing of its own to clear.
actions!(elsewhere, [Unpin, Later, Earlier]);

/// One wheel notch over the map, or one press of Later or Earlier.
const STEP_MINUTES: i64 = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum View {
    #[default]
    List,
    Map,
    Grid,
}

/// A what-if time in effect.
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
    /// The saved file exists but could not be read, so writing `saved` over it
    /// would lose its contents.
    saved_unreadable: bool,
    zones: clock::Zones,
    /// The row being dragged, by geonameid.
    drag: Option<u32>,
    // View preferences, deliberately not persisted.
    view: View,
    /// Ordered by longitude rather than by hand.
    westward: bool,
    format: clock::Format,
    /// `None` follows the real clock.
    pinned: Option<Pin>,
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
    grid_scroll: ScrollHandle,
    /// The grid's vertical scroll through the day.
    grid_hours: ScrollHandle,
    /// Whether the hours have scrolled to the shown instant since the grid
    /// opened. Shared because the grid's canvas sets it.
    grid_at_now: Rc<Cell<bool>>,
    /// Wheel motion over the map that has not yet added up to a whole notch.
    scrub_carry: f32,
    /// The system's mono face for the times, if it has one.
    mono: Option<SharedString>,
    /// Dropping it stops the clock.
    _tick: Task<()>,
}

impl Elsewhere {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let index = Rc::new(SearchIndex::new(cities::load()));
        let picker = cx.new(|cx| SearchPicker::new(index.clone(), window, cx));
        cx.subscribe(&picker, Self::picked).detach();

        let loaded = saved::load();
        let saved_unreadable = loaded.is_none();
        let mut saved = loaded.unwrap_or_default();
        // A regenerated dataset may have dropped a city saved by an older run.
        saved.retain(|id| index.city(*id).is_some());
        // Up front, so restored rows show a time on the first frame.
        let mut zones = clock::Zones::new();
        for geonameid in &saved {
            if let Some(city) = index.city(*geonameid) {
                zones.resolve(&city.timezone);
            }
        }

        Self {
            picker,
            index,
            saved,
            saved_unreadable,
            zones,
            drag: None,
            view: View::default(),
            westward: false,
            format: clock::Format::default(),
            pinned: None,
            editing: None,
            saved_scroll: ScrollHandle::new(),
            land: Rc::new(land::load()),
            zone_borders: Rc::new(zones::load()),
            map_layout: Rc::new(Cell::new(None)),
            map_scroll: ScrollHandle::new(),
            grid_scroll: ScrollHandle::new(),
            grid_hours: ScrollHandle::new(),
            grid_at_now: Rc::new(Cell::new(false)),
            scrub_carry: 0.,
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

    /// Writes the list to disk, unless the file could not be read at startup.
    pub(super) fn persist(&self) {
        if !self.saved_unreadable {
            saved::save(&self.saved);
        }
    }

    fn save(&mut self, geonameid: u32, cx: &mut Context<Self>) {
        if !self.saved.contains(&geonameid) {
            self.saved.push(geonameid);
            if let Some(city) = self.index.city(geonameid) {
                self.zones.resolve(&city.timezone);
            }
            self.persist();
            // Only the hand-arranged order appends at the end; westward drops
            // the new row wherever its longitude falls.
            if !self.westward {
                self.saved_scroll.scroll_to_bottom();
            }
        }
        cx.notify();
    }

    /// Moves the shown instant `steps` quarter hours. A step has no row behind
    /// it, so it keeps any anchor already pinned and otherwise reads as here.
    pub(super) fn nudge(&mut self, steps: i64, cx: &mut Context<Self>) {
        let anchor = self.pinned.as_ref().and_then(|pin| pin.anchor);
        let from = match &self.pinned {
            Some(pin) => pin.at.clone(),
            None => quarter_hour(&Zoned::now()),
        };
        let Ok(at) = from.checked_add(Span::new().minutes(steps * STEP_MINUTES)) else {
            return;
        };
        self.pinned = Some(Pin { anchor, at });
        cx.notify();
    }

    /// An open editor would keep showing the typed time, so it closes first.
    fn step(&mut self, steps: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.close_editor(window, cx);
        self.nudge(steps, cx);
    }
}

/// Truncated to the quarter hour, so steps from the real clock land on round
/// times.
fn quarter_hour(at: &Zoned) -> Zoned {
    at.round(
        ZonedRound::new()
            .smallest(Unit::Minute)
            .increment(STEP_MINUTES)
            .mode(RoundMode::Trunc),
    )
    .expect("a quarter hour is a valid rounding increment")
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
        div()
            .key_context("Elsewhere")
            .on_action(cx.listener(|this, _: &Unpin, window, cx| this.unpin(window, cx)))
            .on_action(cx.listener(|this, _: &Later, window, cx| this.step(1, window, cx)))
            .on_action(cx.listener(|this, _: &Earlier, window, cx| this.step(-1, window, cx)))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            // The title bar sits in what would be the top padding.
            .px_4()
            .pt_2()
            .pb_4()
            .bg(rgb(theme::BASE))
            .text_color(rgb(theme::TEXT))
            .child(self.render_titlebar(window, cx))
            .child(self.picker.clone())
            .children(self.render_banner(&now, cx))
            // Scrolls on its own so the input and toolbar stay put. `min_h_0`
            // lets this flex child shrink below its content and overflow.
            .child(div().relative().flex_1().min_h_0().map(|region| {
                match self.view {
                    View::Map => {
                        region.child(self.render_map(&self.saved, &now, dragging, window, cx))
                    }
                    View::Grid => region.child(self.render_grid(&order, &now, dragging, cx)),
                    View::List => region
                        .child(
                            div()
                                .id("saved")
                                .size_full()
                                .flex()
                                .flex_col()
                                .overflow_y_scroll()
                                .track_scroll(&self.saved_scroll)
                                .when(empty, |list| list.child(self.render_empty()))
                                .children(order.iter().enumerate().filter_map(|(i, geonameid)| {
                                    let first = i == 0;
                                    let last = i + 1 == order.len();
                                    self.render_row(*geonameid, &now, dragging, first, last, cx)
                                }))
                                .when(!self.westward && dragging && self.drag.is_some(), |list| {
                                    list.child(self.render_drop_tail(cx))
                                }),
                        )
                        // Hung out in the window padding, clear of the times.
                        .child(scrollbar(&self.saved_scroll, Axis::Vertical, px(-6.))),
                }
            }))
            .child(self.render_footer(dragging, cx))
            // Last, so they lie over everything else.
            .children(resize::handles(window))
    }
}
