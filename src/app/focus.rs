//! Keyboard focus. The window walks its own list of stops rather than gpui's
//! tab order, which cannot take the vendored search input as a stop, and
//! which would put the notice and the banner where they paint: before the
//! footer, shifting its every stop each time they come and go.

use std::collections::HashMap;

use gpui::{App, Context, Div, FocusHandle, Focusable, MouseButton, Window, div, prelude::*, rgb};

use super::{Elsewhere, View};
use crate::theme;

/// Handles outlive the elements that track them, which are rebuilt each frame.
pub(super) struct Focus {
    pub(super) views: [FocusHandle; 3],
    pub(super) fit: FocusHandle,
    pub(super) format: FocusHandle,
    pub(super) by_clock: FocusHandle,
    pub(super) unpin: FocusHandle,
    pub(super) notice_action: FocusHandle,
    pub(super) notice_dismiss: FocusHandle,
    /// One stop for the whole grid, whose arrows move a cell cursor.
    pub(super) grid: FocusHandle,
    /// A row's, which its dot on the map shares. Kept after a removal, so
    /// focus left on a removed row is still known to have been there.
    rows: HashMap<u32, FocusHandle>,
    /// The stops as last rendered, each with the list item to scroll to.
    stops: Vec<(FocusHandle, Option<usize>)>,
    /// Where focus last stood in `stops`, for when its stop goes away.
    at: Option<usize>,
    /// What has focus this frame, for the rows, dots and grid to ring.
    focused: Option<FocusHandle>,
    /// What had focus when the editor opened, which gets it back on close.
    pub(super) before_edit: Option<FocusHandle>,
}

impl Focus {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            views: [cx.focus_handle(), cx.focus_handle(), cx.focus_handle()],
            fit: cx.focus_handle(),
            format: cx.focus_handle(),
            by_clock: cx.focus_handle(),
            unpin: cx.focus_handle(),
            notice_action: cx.focus_handle(),
            notice_dismiss: cx.focus_handle(),
            grid: cx.focus_handle(),
            rows: HashMap::new(),
            stops: Vec::new(),
            at: None,
            focused: None,
            before_edit: None,
        }
    }

    /// Every saved city has one once `tab_stops` has run.
    pub(super) fn row(&self, geonameid: u32) -> Option<&FocusHandle> {
        self.rows.get(&geonameid)
    }

    /// Whether `handle` has focus this frame.
    pub(super) fn on(&self, handle: &FocusHandle) -> bool {
        self.focused.as_ref() == Some(handle)
    }
}

/// A press does not move focus: a click on a toggle leaves the search field
/// typing, and draws no ring that only a keyboard user wants.
pub(super) fn keyboard_only<E: InteractiveElement>(element: E) -> E {
    element.on_mouse_down(MouseButton::Left, |_event, window, _cx| {
        window.prevent_default()
    })
}

/// The ring a focused row or cell wears, laid over it so it takes no room.
pub(super) fn ring() -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded_md()
        .border_1()
        .border_color(rgb(theme::BLUE))
}

impl Elsewhere {
    /// The stops in order: search, the view's cities, the footer, then the
    /// banner's and notice's, which come and go without moving the rest.
    /// `order` is the list's; `here_row` whether here's row tops it. Also
    /// puts focus back on a stop when its own has gone away.
    pub(super) fn tab_stops(
        &mut self,
        order: &[u32],
        here_row: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for geonameid in &self.saved {
            self.focus
                .rows
                .entry(*geonameid)
                .or_insert_with(|| cx.focus_handle());
        }
        let mut stops = vec![(self.picker.focus_handle(cx), None)];
        match self.view {
            View::List => stops.extend(order.iter().enumerate().filter_map(|(i, geonameid)| {
                let handle = self.focus.row(*geonameid)?.clone();
                Some((handle, Some(i + usize::from(here_row))))
            })),
            // In the saved order, as the dots are drawn.
            View::Map => stops.extend(
                self.saved
                    .iter()
                    .filter_map(|geonameid| Some((self.focus.row(*geonameid)?.clone(), None))),
            ),
            View::Grid => stops.push((self.focus.grid.clone(), None)),
        }
        stops.extend(self.footer_stops().into_iter().map(|handle| (handle, None)));
        if self.pinned.is_some() {
            stops.push((self.focus.unpin.clone(), None));
        }
        if let Some(notice) = &self.notice {
            if notice.has_action() {
                stops.push((self.focus.notice_action.clone(), None));
            }
            stops.push((self.focus.notice_dismiss.clone(), None));
        }
        self.focus.stops = stops;
        self.focus.focused = window.focused(cx);

        if self.editor_focused(window, cx) {
            return;
        }
        let at = self.focus.focused.as_ref().and_then(|focused| {
            self.focus
                .stops
                .iter()
                .position(|(handle, _)| handle == focused)
        });
        match at {
            Some(at) => self.focus.at = Some(at),
            // Its stop is gone: a removed row, a closed notice, a toggle
            // gone disabled. The stop now in its place takes over.
            None => {
                let at = self
                    .focus
                    .at
                    .map_or(0, |at| at.min(self.focus.stops.len() - 1));
                self.focus.at = Some(at);
                let handle = self.focus.stops[at].0.clone();
                cx.defer_in(window, move |_, window, cx| window.focus(&handle, cx));
            }
        }
    }

    fn editor_focused(&self, window: &Window, cx: &App) -> bool {
        self.editing
            .as_ref()
            .is_some_and(|(_, input)| input.focus_handle(cx).is_focused(window))
    }

    /// Tab and shift+tab. An open editor counts as its row's stop, and keeps
    /// a whole time typed as it is left.
    pub(super) fn focus_step(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let len = self.focus.stops.len();
        if len == 0 {
            return;
        }
        let editing = self
            .editing
            .as_ref()
            .and_then(|(geonameid, _)| self.focus.row(*geonameid));
        let at = self
            .focus
            .stops
            .iter()
            .position(|(handle, _)| handle.is_focused(window) || Some(handle) == editing);
        let next = match (at, forward) {
            (Some(at), true) => (at + 1) % len,
            (Some(at), false) => (at + len - 1) % len,
            (None, true) => 0,
            (None, false) => len - 1,
        };
        if self.editing.is_some() {
            self.settle_editor(window, cx);
        }
        let (handle, item) = self.focus.stops[next].clone();
        if let Some(item) = item {
            self.saved_scroll.scroll_to_item(item);
        }
        self.focus.at = Some(next);
        window.focus(&handle, cx);
        cx.notify();
    }
}
