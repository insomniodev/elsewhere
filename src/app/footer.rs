use gpui::{
    AnyElement, Context, FocusHandle, Pixels, SharedString, Window, div, prelude::*, px, rgb, rgba,
    transparent_black,
};
use jiff::Zoned;
use jiff::tz::TimeZone;

use super::button::Button;
use super::focus::{Focus, keyboard_only};
use super::tooltip::Tooltip;
use super::{Elsewhere, View};
use crate::clock::{self, Format};
use crate::theme;

/// The banner's and footer's heights, which the map reads to hold still as
/// the banner comes and goes.
pub(super) const BANNER_HEIGHT: Pixels = px(24.);
pub(super) const FOOTER_HEIGHT: Pixels = px(24.);

/// Beside the way back, which esc also takes: how else to move the time.
/// Spelled out where there is room; cut a word short, it would read worse
/// than the bare keys.
const KEYS: [&str; 2] = ["alt ↑↓ · esc", "alt ↑↓ steps 15 min · esc"];

/// The window width from which `KEYS` spells the step out.
const KEYS_SPELLED: Pixels = px(460.);

/// What the window's shortcuts are held with, as the tooltips name it.
const MODIFIER: &str = if cfg!(target_os = "macos") {
    "cmd"
} else {
    "ctrl"
};

struct Toggle {
    id: &'static str,
    icon: &'static str,
    /// Glyphs fill their box unevenly: a circle runs edge to edge where the
    /// signpost leaves a margin, so drawn equal it looks bigger.
    size: Pixels,
    /// Off, then on.
    tips: [&'static str; 2],
    /// A plain fn, so the element can outlive the borrow that built it.
    click: fn(&mut Elsewhere, &mut Window, &mut Context<Elsewhere>),
    on: fn(&Elsewhere) -> bool,
    /// Why it does nothing now, when it doesn't apply. It stays in its slot
    /// either way, so the controls never jump between views.
    unavailable: fn(&Elsewhere) -> Option<&'static str>,
    focus: fn(&Focus) -> &FocusHandle,
}

const FORMAT: Toggle = Toggle {
    id: "format",
    icon: "icons/clock-12.svg",
    size: px(14.),
    tips: ["show 12-hour time", "showing 12-hour time"],
    click: Elsewhere::toggle_format,
    on: |this| this.format == Format::Twelve,
    // Every view shows here's time, even empty, except a map with no city to
    // mark here at.
    unavailable: |this| {
        (this.saved.is_empty() && this.view == View::Map && this.here.place().is_none())
            .then_some("add a city first")
    },
    focus: |focus| &focus.format,
};

struct Segment {
    view: View,
    id: &'static str,
    icon: &'static str,
    /// Another view shown, then this one.
    tips: [&'static str; 2],
    /// The digit that, held with `MODIFIER`, shows it.
    key: char,
}

const VIEWS: [Segment; 3] = [
    Segment {
        view: View::List,
        id: "show-list",
        icon: "icons/rows.svg",
        tips: ["show as a list", "showing as a list"],
        key: '1',
    },
    Segment {
        view: View::Map,
        id: "show-map",
        icon: "icons/map.svg",
        tips: ["show on a map", "showing on a map"],
        key: '2',
    },
    Segment {
        view: View::Grid,
        id: "show-grid",
        icon: "icons/columns.svg",
        tips: ["show as a day grid", "showing as a day grid"],
        key: '3',
    },
];

const BY_CLOCK: Toggle = Toggle {
    id: "by-clock",
    icon: "icons/milestone.svg",
    size: px(16.),
    tips: ["order by clock", "ordered earliest to latest"],
    click: Elsewhere::toggle_by_clock,
    on: |this| this.by_clock,
    // Not on the map at all, where there is no order: `render_order` gives
    // the slot to the fit control.
    unavailable: |this| (this.saved.len() < 2).then_some("add a city first"),
    focus: |focus| &focus.by_clock,
};

impl Elsewhere {
    pub(super) fn unpin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pinned = None;
        self.close_editor(window, cx);
    }

    pub(super) fn switch(&mut self, to: View, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == to {
            return;
        }
        // The fit and order controls trade places as the map comes and goes;
        // focus on the one leaving passes to the switch, not out of the window.
        if (self.focus.fit.is_focused(window) || self.focus.by_clock.is_focused(window))
            && let Some(i) = VIEWS.iter().position(|segment| segment.view == to)
        {
            window.focus(&self.focus.views[i].clone(), cx);
        }
        self.view = to;
        self.grid_cursor = None;
        // The map opens centred on the cities, wherever it was left.
        if to == View::Map {
            self.map_frame.set(None);
        }
        // The grid opens on now each time, wherever it was left.
        self.grid_at_now.set(false);
        // Not every view has a box for an open editor, and its focus would be
        // stranded where there is none.
        self.close_editor(window, cx);
        self.persist_settings(window, cx);
        cx.notify();
    }

    /// The footer's stops in order, leaving out the controls that do nothing
    /// now; the switch's current segment stays, so its neighbours keep their
    /// places as it changes.
    pub(super) fn footer_stops(&self) -> Vec<FocusHandle> {
        let mut stops = self.focus.views.to_vec();
        if (FORMAT.unavailable)(self).is_none() {
            stops.push(self.focus.format.clone());
        }
        if self.view == View::Map {
            stops.push(self.focus.fit.clone());
        } else if (BY_CLOCK.unavailable)(self).is_none() {
            stops.push(self.focus.by_clock.clone());
        }
        stops
    }

    fn toggle_by_clock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.by_clock = !self.by_clock;
        self.persist_settings(window, cx);
        cx.notify();
    }

    fn toggle_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.format = match self.format {
            Format::TwentyFour => Format::Twelve,
            Format::Twelve => Format::TwentyFour,
        };
        self.persist_settings(window, cx);
        cx.notify();
    }

    fn render_toggle(
        &self,
        toggle: &'static Toggle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Toggle {
            id,
            icon,
            size,
            tips,
            click,
            on,
            unavailable,
            focus,
        } = *toggle;
        let reason = unavailable(self);
        Button::new(id, icon, size)
            .focus(focus(&self.focus).clone())
            .on(on(self))
            .disabled(reason.is_some())
            .tooltip(Tooltip::live(cx.entity(), move |this| {
                unavailable(this).unwrap_or(tips[on(this) as usize]).into()
            }))
            .on_click(cx.listener(move |this, _event, window, cx| click(this, window, cx)))
            .build()
    }

    /// Each segment keeps the toggles' box, so the icons line up.
    fn render_switch(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .rounded_md()
            .bg(rgb(theme::MANTLE))
            .children(VIEWS.iter().zip(&self.focus.views).map(|(segment, focus)| {
                let Segment {
                    view,
                    id,
                    icon,
                    tips,
                    key,
                } = *segment;
                Button::new(id, icon, px(16.))
                    .focus(focus.clone())
                    .on(self.view == view)
                    .current(self.view == view)
                    .wash(rgba(theme::WASH))
                    .tooltip(Tooltip::live(cx.entity(), move |this| {
                        let tip = tips[(this.view == view) as usize];
                        SharedString::from(format!("{tip} · {MODIFIER}+{key}"))
                    }))
                    .on_click(
                        cx.listener(move |this, _event, window, cx| this.switch(view, window, cx)),
                    )
                    .build()
            }))
    }

    /// The last slot on the right: centring on the map, where there is no
    /// order, and ordering everywhere else. One button either way, so the
    /// format toggle beside it never moves.
    fn render_order(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.view != View::Map {
            return self.render_toggle(&BY_CLOCK, cx).into_any_element();
        }
        Button::new("fit", "icons/fit.svg", px(14.))
            .focus(self.focus.fit.clone())
            .tooltip(Tooltip::text("centre the map on your cities"))
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.map_frame.set(None);
                cx.notify();
            }))
            .build()
            .into_any_element()
    }

    /// How to move the time, in the banner's place until one is pinned: the
    /// room is the banner's either way, so nothing moves or resizes as the
    /// pin comes and goes, and unpinned it is not left empty. Names only what
    /// works now: here's row, all an empty list has, opens no editor.
    pub(super) fn render_time_hint(&self) -> Option<impl IntoElement + use<>> {
        if self.pinned.is_some() {
            return None;
        }
        let hint = match self.view {
            View::Map => "ctrl + scroll or alt + ↑/↓ moves the time",
            View::Grid => "click an hour or alt + ↑/↓ moves the time",
            View::List if self.saved.is_empty() => "alt + ↑/↓ moves the time",
            View::List => "click a time or alt + ↑/↓ moves the time",
        };
        Some(
            div()
                .flex()
                .items_center()
                .h(BANNER_HEIGHT)
                // The banner's inset and frame, so its label lands on this.
                // Filled like the view switch, frame included, so the hint
                // sits in a ghost of the banner's box that lines up with the
                // controls below rather than floating loose.
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(rgb(theme::MANTLE))
                .bg(rgb(theme::MANTLE))
                .text_xs()
                .text_color(rgb(theme::OVERLAY2))
                .child(div().min_w_0().truncate().child(hint)),
        )
    }

    /// A pin with no row reads here. Shown only while pinned, above the
    /// footer in the hint's place, so nothing moves as it comes and goes.
    /// Names the pinned day once it leaves today, since the rows' day tags
    /// are relative to it, and keeps the real time in view.
    pub(super) fn render_banner(
        &self,
        now: &Zoned,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let pin = self.pinned.as_ref()?;
        let keys = KEYS[usize::from(window.viewport_size().width >= KEYS_SPELLED)];
        let real = Zoned::now();
        let read = |zone: &TimeZone| {
            let time = clock::reading(now, zone, self.format).time;
            match clock::pinned_day(now, zone, &real) {
                Some(day) => format!("{time} {day}"),
                None => time,
            }
        };
        let label = match pin.anchor {
            Some(anchor) => {
                let city = self.index.city(anchor)?;
                let reading = self
                    .zones
                    .get(&city.timezone)
                    .map(read)
                    .unwrap_or_else(|| clock::UNKNOWN.to_string());
                format!("viewing {reading} in {}", city.name)
            }
            None => format!("viewing {} here", read(&self.here.zone)),
        };
        let live = clock::reading(&real, &self.here.zone, self.format).time;
        let accent = pin.accent();
        Some(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .gap_2()
                .items_center()
                .h(BANNER_HEIGHT)
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(rgba(accent.edge))
                .bg(rgba(accent.tint))
                .text_xs()
                .text_color(rgb(accent.text))
                // In a narrow window the real time gives way before the
                // what-if, and both before the way back.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(div().min_w_0().truncate().child(label))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .map(|mut live| {
                                    // Shrink is weighted by size, so a heavy
                                    // weight is what makes it go first.
                                    live.style().flex_shrink = Some(1000.);
                                    live
                                })
                                .text_color(rgb(theme::SUBTEXT0))
                                .child(format!("· now {live}")),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(
                            keyboard_only(
                                div()
                                    .id("unpin")
                                    .track_focus(&self.focus.unpin)
                                    .px_1()
                                    .rounded_md()
                                    // Framed always, so the focus ring
                                    // takes no room.
                                    .border_1()
                                    .border_color(transparent_black())
                                    .focus(|style| style.border_color(rgb(theme::BLUE)))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(theme::SURFACE0))),
                            )
                            .on_click(
                                cx.listener(|this, _event, window, cx| this.unpin(window, cx)),
                            )
                            .child("back to now"),
                        )
                        .child(div().text_color(rgb(theme::OVERLAY2)).child(keys)),
                ),
        )
    }

    /// Always there at a fixed height, so the list above never shifts as
    /// controls come and go.
    pub(super) fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .justify_between()
            .items_center()
            .h(FOOTER_HEIGHT)
            .text_xs()
            .child(self.render_switch(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(self.render_toggle(&FORMAT, cx))
                    .child(self.render_order(cx)),
            )
    }
}
