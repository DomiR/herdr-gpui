use super::*;

#[cfg(unix)]
use crate::setup_relay::gui::{self, Prompt, Session};

#[cfg(unix)]
pub(in crate::menu::devices) struct Progress {
    session: Session,
    pub(super) response: Entity<SearchInput>,
    output: String,
    scroll: ScrollHandle,
    waiting: bool,
}

impl SheprdWindow {
    #[cfg(windows)]
    pub(super) fn start_device_setup(&mut self, cx: &mut Context<Self>) {
        self.set_device_error(Some(
            super::super::Error::TerminalSetupUnavailable.to_string(),
        ));
        cx.notify();
    }

    #[cfg(unix)]
    pub(super) fn start_device_setup(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        let Step::Confirm(request, offer) = &form.step else {
            return;
        };
        let (request, offer) = (request.clone(), *offer);
        let claim = form.claim.take();
        form.step = Step::Verifying(request.clone(), offer);
        form.error = None;
        let background = cx.background_executor().spawn(async move {
            let claim = match claim {
                Some(claim) => setup::verify_unsaved(&request, &claim).map(|()| claim),
                None => setup::claim(&request),
            }?;
            setup::native_setup(&request, offer == Offer::InstallKey, claim)
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| this.device_setup_started(result, cx));
        }));
        cx.notify();
    }

    #[cfg(unix)]
    pub(in crate::menu::devices) fn device_setup_started(
        &mut self,
        result: crate::Result<Session>,
        cx: &mut Context<Self>,
    ) {
        let Some(Setup {
            step: Step::Verifying(request, offer),
            ..
        }) = &self.menu.device_setup
        else {
            return;
        };
        let (request, offer) = (request.clone(), *offer);
        let session = match result {
            Ok(session) => session,
            Err(error) => {
                if !self.refuse_duplicate(&error, cx) {
                    if let Some(form) = &mut self.menu.device_setup {
                        form.step = Step::Confirm(request, offer);
                        form.error = Some(error.to_string());
                    }
                    cx.notify();
                }
                return;
            }
        };
        let response = cx.new(SearchInput::new);
        response.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            input.set_placeholder_with(|| fl!("devices-setup-response"), cx);
            input.set_masked(true, cx);
        });
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Running(
                request,
                Progress {
                    session,
                    response,
                    output: String::new(),
                    scroll: ScrollHandle::new(),
                    waiting: false,
                },
            );
        }
        cx.notify();
    }

    #[cfg(unix)]
    pub(super) fn poll_device_progress(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        let Step::Running(_, progress) = &mut form.step else {
            return;
        };
        let Some(update) = progress.session.poll() else {
            return;
        };
        progress.output = update.output;
        progress.scroll.scroll_to_bottom();
        progress.waiting = false;
        let visible = self.menu.page == Some(Page::AddDevice);
        if visible {
            let focus = match gui::prompt(&progress.output) {
                Prompt::Confirm => self.menu.focus.clone(),
                Prompt::Response => progress.response.read(cx).focus.clone(),
            };
            if !focus.is_focused(window) {
                window.focus(&focus, cx);
            }
        }
        if let Some(result) = update.result {
            let error = match result {
                Ok(0) => {
                    self.device_save_succeeded(setup::SaveOutcome::Ready, cx);
                    return;
                }
                Ok(code) => {
                    use std::os::unix::process::ExitStatusExt;
                    super::super::Error::Setup {
                        status: std::process::ExitStatus::from_raw(code.clamp(1, 255) << 8),
                        detail: setup::last_line(progress.output.as_bytes()),
                    }
                    .into()
                }
                Err(error) => error,
            };
            form.step = Step::Form;
            form.error = Some(error.to_string());
        }
        cx.notify();
    }

    #[cfg(unix)]
    pub(super) fn respond_to_device_setup(
        &mut self,
        confirmation: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let Some(Setup {
            step: Step::Running(_, progress),
            ..
        }) = &mut self.menu.device_setup
        else {
            return;
        };
        if progress.waiting {
            return;
        }
        let prompt = gui::prompt(&progress.output);
        let response = match (prompt, confirmation) {
            (Prompt::Confirm, Some(true)) => "yes",
            (Prompt::Confirm, Some(false)) => "no",
            (Prompt::Response, None) => progress.response.read(cx).text(),
            _ => return,
        };
        if progress.session.respond(response) {
            progress
                .response
                .update(cx, |input, cx| input.set_text_selected("", cx));
            progress.waiting = true;
            cx.notify();
        }
    }

    #[cfg(unix)]
    pub(super) fn render_device_progress(
        &self,
        request: &setup::Request,
        progress: &Progress,
        cx: &mut Context<Self>,
    ) -> Div {
        let button = |id: &'static str, label: String| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .p(px(8.))
                .rounded(px(crate::config::corners::CONTROL))
                .bg(rgb(self.theme.active))
                .cursor_pointer()
                .hover(|style| {
                    style
                        .bg(rgb(self.theme.active).blend(rgba((self.theme.foreground << 8) | 0x20)))
                })
                .child(label)
        };
        let mut controls = div().flex().flex_wrap().justify_end().gap(px(8.)).child(
            button("device-progress-cancel", fl!("add-device-cancel"))
                .on_click(cx.listener(|this, _, _, cx| this.cancel_device_step(cx))),
        );
        if !progress.waiting {
            controls = match gui::prompt(&progress.output) {
                Prompt::Confirm => controls
                    .child(
                        button("device-progress-no", fl!("devices-setup-no")).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.respond_to_device_setup(Some(false), cx)
                            }),
                        ),
                    )
                    .child(
                        button("device-progress-yes", fl!("devices-setup-yes")).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.respond_to_device_setup(Some(true), cx)
                            }),
                        ),
                    ),
                Prompt::Response => controls.child(
                    button("device-progress-send", fl!("devices-setup-send")).on_click(
                        cx.listener(|this, _, _, cx| this.respond_to_device_setup(None, cx)),
                    ),
                ),
            };
        }
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .p(px(16.))
                    .flex_none()
                    .child(fl!("devices-setup-progress", target = request.target())),
            )
            .child(
                div()
                    .id("device-progress-output")
                    .debug_selector(|| "device-progress-output".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&progress.scroll)
                    .p(px(16.))
                    .text_sm()
                    .children(
                        progress
                            .output
                            .lines()
                            .map(|line| div().whitespace_normal().child(line.to_owned())),
                    ),
            )
            .child(
                div()
                    .p(px(16.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .when(gui::prompt(&progress.output) == Prompt::Response, |body| {
                        body.child(
                            div()
                                .text_color(rgb(self.theme.muted))
                                .child(fl!("devices-setup-response-hint")),
                        )
                        .child(progress.response.clone())
                    })
                    .child(controls),
            )
    }
}
