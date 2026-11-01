//! One message at a time, in a bar above the footer: what went wrong on disk
//! now, or an undo. A new notice replaces the old.

use std::rc::Rc;
use std::time::Duration;

use gpui::{Context, SharedString, Window, div, prelude::*, px, rgb};

use super::Elsewhere;
use super::button::Button;
use super::tooltip::Tooltip;
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Info,
    /// Something the user should know did not happen.
    Failure,
}

type Run = Rc<dyn Fn(&mut Elsewhere, &mut Window, &mut Context<Elsewhere>)>;

pub(super) struct Notice {
    text: SharedString,
    kind: Kind,
    /// A button's word and what it does; the notice closes first, so the
    /// action can raise another.
    action: Option<(SharedString, Run)>,
    /// `None` stays until dismissed.
    timeout: Option<Duration>,
}

impl Notice {
    pub(super) fn new(kind: Kind, text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            kind,
            action: None,
            timeout: None,
        }
    }

    pub(super) fn action(
        mut self,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Elsewhere, &mut Window, &mut Context<Elsewhere>) + 'static,
    ) -> Self {
        self.action = Some((label.into(), Rc::new(run)));
        self
    }

    pub(super) fn has_action(&self) -> bool {
        self.action.is_some()
    }

    /// For news that needs no answer; failures should stay until read.
    pub(super) fn timeout(mut self, after: Duration) -> Self {
        self.timeout = Some(after);
        self
    }
}

impl Elsewhere {
    pub(super) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        // Replacing the task cancels the previous notice's timer.
        self.notice_timer = notice.timeout.map(|after| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(after).await;
                this.update(cx, |this, cx| {
                    this.notice = None;
                    cx.notify();
                })
                .ok();
            })
        });
        self.notice = Some(notice);
        cx.notify();
    }

    pub(super) fn dismiss_notice(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        self.notice_timer = None;
        cx.notify();
    }

    /// Above the footer rather than under the search: the list keeps its top
    /// edge and only loses room at the bottom, so no row moves when a notice
    /// comes or goes. Wraps rather than truncates, since a path and an error
    /// seldom fit one line at the window's narrowest.
    pub(super) fn render_notice(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let notice = self.notice.as_ref()?;
        let failure = notice.kind == Kind::Failure;
        let action = notice.action.as_ref().map(|(label, run)| {
            let run = run.clone();
            Button::label("notice-action", label.clone())
                .focus(self.focus.notice_action.clone())
                .rest(theme::TEXT)
                .wash(rgb(theme::SURFACE1))
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.dismiss_notice(cx);
                    run(this, window, cx);
                }))
                .build()
        });
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .min_h_6()
                .py_0p5()
                .pl_2()
                .pr_0p5()
                .rounded_md()
                .bg(rgb(theme::SURFACE0))
                // A stripe rather than a frame: enough to tell a failure
                // without shouting over the list.
                .when(failure, |bar| {
                    bar.border_l_2().border_color(rgb(theme::RED))
                })
                .text_xs()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .line_clamp(3)
                        .text_color(rgb(if failure {
                            theme::TEXT
                        } else {
                            theme::SUBTEXT0
                        }))
                        .child(notice.text.clone()),
                )
                .children(action)
                .child(
                    Button::new("notice-dismiss", "icons/x.svg", px(12.))
                        .focus(self.focus.notice_dismiss.clone())
                        .rest(theme::SUBTEXT0)
                        .hover(theme::TEXT)
                        .wash(rgb(theme::SURFACE1))
                        .tooltip(Tooltip::text("dismiss"))
                        .on_click(cx.listener(|this, _event, _window, cx| this.dismiss_notice(cx)))
                        .build(),
                ),
        )
    }
}
