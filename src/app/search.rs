use std::rc::Rc;

use gpui::{
    App, Axis, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseDownEvent, ScrollHandle,
    Window, actions, deferred, div, prelude::*, px, rgb,
};

use super::scrollbar::scrollbar;
use crate::search::SearchIndex;
use crate::theme;
use crate::vendor::text_input::TextInput;

// The vendored input claims none of these keys, so they bubble up to the
// "SearchPicker" wrapper.
actions!(search, [Confirm, Clear, MoveUp, MoveDown]);

pub(super) const RESULTS: usize = 8;

pub(super) struct SearchPicker {
    input: Entity<TextInput>,
    index: Rc<SearchIndex>,
    /// The last text seen, to tell keystrokes from the input's other
    /// notifications.
    query: String,
    /// Geonameids, best first.
    results: Vec<u32>,
    /// Into `results`; meaningless while it is empty.
    selected: usize,
    /// Doubles as the panel's bounds, which the dismiss guard and the
    /// scrollbar read.
    scroll: ScrollHandle,
}

pub(super) enum SearchEvent {
    /// The picker has already reset itself.
    Picked(u32),
}

impl EventEmitter<SearchEvent> for SearchPicker {}

impl SearchPicker {
    pub(super) fn new(index: Rc<SearchIndex>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new("search for a city...", cx));
        window.focus(&input.focus_handle(cx));
        cx.observe(&input, Self::search).detach();

        Self {
            input,
            index,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
        }
    }

    /// The input has no change event, so this runs on every notification,
    /// cursor moves included, and re-searches only when the text changed.
    fn search(&mut self, input: Entity<TextInput>, cx: &mut Context<Self>) {
        if input.read(cx).text() == self.query {
            return;
        }
        self.query = input.read(cx).text().to_string();
        self.results = self
            .index
            .search(&self.query, RESULTS)
            .iter()
            .map(|city| city.geonameid)
            .collect();
        // Back to the top hit, so plain Enter saves the best match.
        self.selected = 0;
        cx.notify();
    }

    /// Leaves `query` alone: clearing it would let the next input notification
    /// reopen the panel.
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.results.clear();
        cx.notify();
    }

    /// `query` is cleared with the input text so `search()` stays quiet.
    fn reset(&mut self, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.reset(cx));
        self.query.clear();
        self.results.clear();
        self.selected = 0;
    }

    /// The query, not the results: a dismissed panel leaves the text in the
    /// input, and the list still sits under it.
    pub(super) fn searching(&self) -> bool {
        !self.query.is_empty()
    }

    fn pick(&mut self, geonameid: u32, cx: &mut Context<Self>) {
        self.reset(cx);
        cx.emit(SearchEvent::Picked(geonameid));
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(&geonameid) = self.results.get(self.selected) {
            self.pick(geonameid, cx);
        }
    }

    fn clear(&mut self, _: &Clear, _window: &mut Window, cx: &mut Context<Self>) {
        // Nothing to clear, so the window gets the key.
        if self.query.is_empty() && self.results.is_empty() {
            cx.propagate();
            return;
        }
        self.reset(cx);
        cx.notify();
    }

    fn move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.results.len() - 1);
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }
}

impl Focusable for SearchPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for SearchPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The results float over the saved list so the rows below hold still
        // while the user types.
        div()
            .relative()
            .key_context("SearchPicker")
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::clear))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .child(self.input.clone())
            .children((!self.results.is_empty()).then(|| {
                deferred(
                    div()
                        .occlude()
                        .absolute()
                        .top_full()
                        .left_0()
                        .right_0()
                        .mt_1()
                        .p_1()
                        .rounded_md()
                        .bg(rgb(theme::MANTLE))
                        .border_1()
                        .border_color(rgb(theme::SURFACE0))
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(
                            |this, event: &MouseDownEvent, _window, cx| {
                                // A click on the input above keeps it open.
                                if event.position.y < this.scroll.bounds().top() {
                                    return;
                                }
                                this.dismiss(cx)
                            },
                        ))
                        .child(scrollbar(&self.scroll, Axis::Vertical, px(2.)))
                        .child(
                            div()
                                .id("results")
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .max_h(window.viewport_size().height - px(80.))
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll)
                                .children(self.results.iter().enumerate().filter_map(
                                    |(i, geonameid)| {
                                        let city = self.index.city(*geonameid)?;
                                        Some(
                                            div()
                                                .id(("result", *geonameid as usize))
                                                .px_2()
                                                .py_1()
                                                .rounded_md()
                                                // The overflow `truncate`
                                                // sets lets a flex item shrink
                                                // below its content, so in a
                                                // short window rows would
                                                // squash instead of scrolling.
                                                .flex_shrink_0()
                                                .truncate()
                                                .cursor_pointer()
                                                .when(i == self.selected, |row| {
                                                    row.bg(rgb(theme::SURFACE0))
                                                })
                                                .hover(|style| style.bg(rgb(theme::SURFACE0)))
                                                .active(|style| style.opacity(0.8))
                                                .on_click(cx.listener({
                                                    let geonameid = *geonameid;
                                                    move |this, _event, _window, cx| {
                                                        this.pick(geonameid, cx)
                                                    }
                                                }))
                                                .child(self.index.label(city)),
                                        )
                                    },
                                )),
                        ),
                )
            }))
    }
}
