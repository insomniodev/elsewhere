use std::time::Duration;

use gpui::{
    App, Context, Div, ElementId, Entity, EntityInputHandler, Focusable, Pixels, SharedString,
    Window, deferred, div, prelude::*, px, rgb, rgba,
};
use jiff::Zoned;

use super::button::{self, Button};
use super::drag::{DragRow, drop_target};
use super::here::{hint, tag};
use super::notice::{Kind, Notice};
use super::tooltip::Tooltip;
use super::{Cancel, Commit, Elsewhere, Pin};
use crate::clock;
use crate::theme;
use crate::vendor::text_input::TextInput;

/// Fixed so the reading and the editor share a box and a row does not reflow
/// as its digits change. The longest reading in a 0.6em mono at 16px, which
/// every face on the list is, plus the vendored input's 6px padding either
/// side, so the editor hugs the digits.
fn time_width(format: clock::Format) -> Pixels {
    px(format.glyphs() as f32 * 9.6 + 12.)
}

/// Also the row's height: the vendored input's, reserved even while it is
/// closed so opening it does not push the list down. The honest fix is a
/// shorter input.
const TIME_HEIGHT: Pixels = px(36.);

/// The label's inset from the hover wash's edge. The wash cannot hang out
/// into the window padding: the list is a scroll container and clips on both
/// axes, which would square off the row's left corners.
const WASH: Pixels = px(8.);

/// Either side of the rule between rows.
const GAP: Pixels = px(6.);

/// Long enough to notice a removal was a mistake, short enough that the
/// notice is not still up long after.
const UNDO: Duration = Duration::from_secs(8);

/// How long a revealed row keeps its wash.
const REVEAL: Duration = Duration::from_millis(1500);

/// Best first. Mono rather than `tnum`, which several of gpui's fallback sans
/// faces lack, and which cannot be asked for without naming a family anyway.
const MONO: [&str; 8] = [
    "JetBrains Mono",
    "IBM Plex Mono",
    "SF Mono",
    "Menlo",
    "Cascadia Mono",
    "Adwaita Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
];

/// gpui matches a family name exactly and silently falls back to the UI font,
/// so the list is checked against what is installed.
pub(super) fn mono(cx: &App) -> Option<SharedString> {
    let installed = cx.text_system().all_font_names();
    MONO.iter()
        .find(|family| installed.iter().any(|name| name == *family))
        .map(|family| SharedString::from(*family))
}

impl Elsewhere {
    /// Written at once, with an undo that puts the city back where it stood
    /// in the hand order. Notices are single, so a second removal replaces
    /// the first's notice, and only the latest can be undone.
    pub(super) fn delete(&mut self, geonameid: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.saved.iter().position(|id| *id == geonameid) else {
            return;
        };
        self.saved.remove(at);
        if let Some(pin) = &mut self.pinned {
            pin.forget(geonameid);
        }
        let name = self
            .index
            .city(geonameid)
            .map_or_else(String::new, |city| city.name.clone());
        // Before the write, so a failure to write replaces it.
        self.show_notice(
            Notice::new(Kind::Info, format!("removed {name}"))
                .action("undo", move |this, _window, cx| {
                    this.restore(geonameid, at, cx)
                })
                .timeout(UNDO),
            cx,
        );
        self.persist(cx);
        if self
            .editing
            .as_ref()
            .is_some_and(|(id, _)| *id == geonameid)
        {
            self.close_editor(window, cx);
        }
        cx.notify();
    }

    /// Unless the city came back some other way meanwhile.
    fn restore(&mut self, geonameid: u32, at: usize, cx: &mut Context<Self>) {
        if self.saved.contains(&geonameid) {
            return;
        }
        self.saved.insert(at.min(self.saved.len()), geonameid);
        self.persist(cx);
        self.reveal_row(geonameid, cx);
    }

    /// Scrolls the list to the row on its next frame and washes it briefly,
    /// so a row that comes back, or is picked, is found where it landed.
    pub(super) fn reveal_row(&mut self, geonameid: u32, cx: &mut Context<Self>) {
        self.revealed = Some(geonameid);
        self.reveal_pending = true;
        // Replacing the task cancels an earlier reveal's fade.
        self.reveal_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REVEAL).await;
            this.update(cx, |this, cx| {
                this.revealed = None;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Opens on the current reading, as shown, selected: typing replaces it
    /// and an arrow key drops the cursor in to nudge it. It is the
    /// placeholder too, for when the field is cleared.
    pub(super) fn edit(
        &mut self,
        geonameid: u32,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            let mut input = TextInput::new(current.clone(), cx);
            // Marking then unmarking is the vendored input's only way to set
            // text with a selection. It notifies before `retime` observes, so
            // opening pins nothing.
            let all = current.encode_utf16().count();
            input.replace_and_mark_text_in_range(None, &current, Some(0..all), window, cx);
            input.unmark_text(window, cx);
            input
        });
        window.focus(&input.focus_handle(cx));
        // Not over one already open, which is still the editor's own pin.
        if self.editing.is_none() {
            self.snapshot_pin();
        }
        cx.observe(&input, Self::retime).detach();
        self.editing = Some((geonameid, input));
        cx.notify();
    }

    /// Re-pins on each keystroke that leaves a whole time, so the other rows
    /// follow as the user types; other text keeps the last pin. On the pin's
    /// day, if there is one, so a typed time keeps the day being explored.
    fn retime(&mut self, input: Entity<TextInput>, cx: &mut Context<Self>) {
        let Some((geonameid, editor)) = &self.editing else {
            return;
        };
        if *editor != input {
            return;
        }
        let reference = self
            .pinned
            .as_ref()
            .map_or_else(Zoned::now, |pin| pin.at.clone());
        if let Some(city) = self.index.city(*geonameid)
            && let Some(zone) = self.zones.get(&city.timezone)
            && let Some(pinned) = clock::pin(input.read(cx).text(), zone, &reference)
        {
            self.pinned = Some(Pin {
                anchor: Some(*geonameid),
                at: pinned,
            });
        }
        // Even unpinned, for the hint.
        cx.notify();
    }

    /// Keeps the pin typed. Cleared text backs out as escape does; text that
    /// is not a time keeps the editor open under its hint.
    fn commit(&mut self, _: &Commit, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, input)) = &self.editing else {
            return;
        };
        let text = input.read(cx).text();
        if text.trim().is_empty() {
            self.cancel(&Cancel, window, cx);
        } else if clock::wall(text).is_some() {
            self.close_editor(window, cx);
        }
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.restore_pin();
        self.close_editor(window, cx);
    }

    pub(super) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.pre_edit = None;
        window.focus(&self.picker.focus_handle(cx));
        cx.notify();
    }

    /// Enter and escape fall through the vendored input, so the wrapper
    /// carries the context that catches them. The map opens the same editor.
    pub(super) fn render_editor(&self, input: &Entity<TextInput>, cx: &mut Context<Self>) -> Div {
        let text = input.read(cx).text();
        let invalid = !text.trim().is_empty() && clock::wall(text).is_none();
        div()
            .relative()
            .key_context("PinEditor")
            .on_action(cx.listener(Self::commit))
            .on_action(cx.listener(Self::cancel))
            .w(time_width(self.format))
            .h(TIME_HEIGHT)
            .child(
                div()
                    .size_full()
                    .when_some(self.mono.clone(), Styled::font_family)
                    .child(input.clone()),
            )
            // Floated so the rows hold still: it overlaps the next row's
            // right end for as long as the text is wrong, and deferred so it
            // paints over that row and past the list's clip under the last.
            .when(invalid, |editor| {
                editor.child(deferred(
                    div()
                        .absolute()
                        .top_full()
                        .right_0()
                        .px_1()
                        .rounded_sm()
                        .bg(rgb(theme::BASE))
                        .whitespace_nowrap()
                        .text_xs()
                        .text_color(rgb(theme::RED))
                        .child("try 9:30 pm or 21:30"),
                ))
            })
    }

    /// Set where the first saved row will go.
    pub(super) fn render_empty(&self) -> impl IntoElement + use<> {
        div()
            .h(TIME_HEIGHT)
            .flex()
            .items_center()
            .pl(WASH)
            .child(hint())
    }

    /// Here's own row while no saved city is in the zone: first, and neither
    /// a handle nor a drop target, since it is not one of the cities. Its
    /// time reads like a row's but does not open the editor, which edits
    /// saved cities.
    pub(super) fn render_here_row(&self, now: &Zoned) -> impl IntoElement + use<> {
        let time = clock::reading(now, &self.here.zone, self.format).time;
        div().pb(GAP).child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .pl(WASH)
                .child(self.render_name(
                    "name-here".into(),
                    self.here.name.clone(),
                    self.here_detail(now),
                    true,
                    now,
                ))
                .child(
                    div()
                        .w(time_width(self.format))
                        .h(TIME_HEIGHT)
                        .flex()
                        .items_center()
                        .justify_end()
                        .px(px(6.))
                        .when_some(self.mono.clone(), Styled::font_family)
                        .when_some(
                            self.pinned.as_ref().map(|pin| pin.accent().text),
                            |cell, text| cell.text_color(rgb(text)),
                        )
                        .child(time),
                )
                // The remove button's room, so the times line up.
                .child(div().flex_none().w(button::WIDTH)),
        )
    }

    /// The name with the "here" mark beside it when it is here. The mark goes
    /// next to the name rather than out by the time with the day tag, as the
    /// name is what it describes. Hovering the name gives `detail`, and shows
    /// the whole label when it is cut short.
    fn render_name(
        &self,
        id: ElementId,
        name: String,
        detail: String,
        here: bool,
        now: &Zoned,
    ) -> impl IntoElement + use<> {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id(id)
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme::SUBTEXT0))
                    .tooltip(Tooltip::text(detail))
                    .child(name),
            )
            .when(here, |name| name.child(self.here_tag("here-row", now)))
    }

    /// `use<>`: the row borrows none of the arguments, so `render` can build
    /// the list inside a closure that holds `cx`.
    pub(super) fn render_row(
        &self,
        geonameid: u32,
        now: &Zoned,
        dragging: bool,
        first: bool,
        last: bool,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let city = self.index.city(geonameid)?;
        let label = self.index.label(city);
        let zone = self.zones.get(&city.timezone);
        let detail = clock::city_detail(&label, zone, now);
        let reading = zone.map(|zone| clock::reading(now, zone, self.format));
        // Only a row whose zone resolved can anchor a pin.
        let known = reading.is_some();
        let (time, day) = match reading {
            Some(reading) => (reading.time, reading.day),
            None => (clock::UNKNOWN.to_string(), None),
        };
        // The rule above a row doubles as the insertion line, live only in the
        // hand-arranged order; westward rows can still be dragged to the bin,
        // but show no grab cursor, as a drop between rows moves nothing.
        // The first row wears it transparent, so a drop before it has a line
        // to show. No inset at the list's ends, with nothing to space from.
        let target = drop_target(div(), !self.westward)
            .when(!first, |row| row.border_color(rgb(theme::SURFACE0)))
            .py(GAP)
            .when(first, |row| row.pt_0())
            .when(last, |row| row.pb_0())
            .when(!self.westward, |row| {
                row.on_drop(cx.listener(move |this, row: &DragRow, _window, cx| {
                    this.drop(row.geonameid, Some(geonameid), cx)
                }))
            });
        Some(
            target.child(
                div()
                    .id(("row", geonameid as usize))
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_center()
                    .group("row")
                    .pl(WASH)
                    .rounded_md()
                    // Not a click target: the wash shows the row is live for
                    // its remove button and as a drag handle, so only the time
                    // and the button take a pointer cursor. The name's tooltip
                    // says what the row is.
                    .hover(|style| style.bg(rgba(theme::WASH)))
                    .when(self.revealed == Some(geonameid), |row| {
                        row.bg(rgba(theme::WASH))
                    })
                    // The whole row is the handle; there is no grip.
                    .when(!self.westward, |row| row.cursor_grab())
                    .on_drag(
                        DragRow::new(geonameid, label.clone(), !self.westward),
                        Self::drag_preview(cx),
                    )
                    .when(dragging && self.drag == Some(geonameid), |row| {
                        row.opacity(0.4)
                    })
                    // A flex item's automatic minimum is its content, so a long
                    // label would shove the time past the right edge. `min_w_0`
                    // lets it give way and truncate instead.
                    .child(self.render_name(
                        ("name", geonameid as usize).into(),
                        label,
                        detail,
                        self.here.city == Some(geonameid),
                        now,
                    ))
                    .children(day.map(tag))
                    .child(match &self.editing {
                        Some((editing, input)) if *editing == geonameid => {
                            self.render_editor(input, cx).into_any_element()
                        }
                        // The cell is fixed so the column lines up; the pill
                        // hugs the digits so the hover wash sits under them, at
                        // the full height the editor opens into.
                        _ => div()
                            .w(time_width(self.format))
                            .h(TIME_HEIGHT)
                            .flex()
                            .justify_end()
                            .child(
                                div()
                                    .id(("time", geonameid as usize))
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    // The vendored input's inset, so the digits
                                    // hold still when the editor opens.
                                    .px(px(6.))
                                    .rounded_md()
                                    .when_some(self.mono.clone(), Styled::font_family)
                                    .when_some(
                                        self.pinned.as_ref().map(|pin| pin.accent().text),
                                        |cell, text| cell.text_color(rgb(text)),
                                    )
                                    .when(known, |cell| {
                                        cell.cursor_pointer()
                                            .hover(|style| style.bg(rgb(theme::SURFACE0)))
                                            .on_click(cx.listener({
                                                let time = time.clone();
                                                move |this, _event, window, cx| {
                                                    this.edit(geonameid, time.clone(), window, cx)
                                                }
                                            }))
                                    })
                                    .child(time.clone()),
                            )
                            .into_any_element(),
                    })
                    .child(self.render_remove(geonameid, cx)),
            ),
        )
    }

    /// Shown only while the row is hovered, but always laid out, so the
    /// times hold still. Invisible rather than absent, it is only ever
    /// clicked with the row under the pointer, which shows it.
    fn render_remove(&self, geonameid: u32, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        Button::new("remove", "icons/x.svg", px(12.))
            .key(geonameid as usize)
            .rest(theme::SUBTEXT0)
            .hover(theme::RED)
            .tooltip(Tooltip::text("remove"))
            .on_click(
                cx.listener(move |this, _event, window, cx| this.delete(geonameid, window, cx)),
            )
            .build()
            .flex_none()
            .opacity(0.)
            .group_hover("row", |style| style.opacity(1.))
    }
}
