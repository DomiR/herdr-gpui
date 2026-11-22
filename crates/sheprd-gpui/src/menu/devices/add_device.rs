//! The Add Device form and background host checks and saves.

use super::{Page, discover::Discovery, setup};

mod offer;
mod progress;

use crate::i18n::fl;
use crate::{
    SheprdWindow,
    search_input::{Changed, SearchInput},
};
use gpui::{prelude::*, *};
pub(super) use offer::Offer;
use sheprd_client::protocol::{ClientKeyCode, ClientKeyKind, ClientPaneInputEvent};
use sheprd_client::{HostKeys, HostProbe, NewHostKey, SshFailure};

pub(crate) fn enter() -> ClientPaneInputEvent {
    ClientPaneInputEvent::Key {
        code: ClientKeyCode::Enter,
        modifiers: 0,
        kind: ClientKeyKind::Press,
        repeat_count: 1,
        shifted_codepoint: None,
        generated_text: None,
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    }
}

pub(in crate::menu) struct Setup {
    pub(super) fields: [Entity<SearchInput>; 3],
    pub(super) step: Step,
    /// This process's claim on the host being added, from the first check
    /// until the device is saved or the dialog closes.
    pub(super) claim: Option<setup::Claim>,
    pub(super) task: Option<Task<()>>,
    /// The search for hosts to suggest, from when the form opens.
    pub(super) discovery: Option<Discovery>,
    /// The label a suggestion filled in, until the user edits it. Only that
    /// label is replaced when another suggestion is chosen.
    pub(super) suggested_label: Option<String>,
    /// Clears `suggested_label` once the label field holds anything else, and
    /// refilters the suggestions as the target changes.
    pub(super) _edits: Vec<Subscription>,
    /// The suggestion Up and Down moved to, by the names and addresses that
    /// identify its machine, with the target text it was chosen for: typing
    /// anything else drops it.
    pub(super) highlight: Option<(String, Vec<String>)>,
    /// Why the last check or save failed. Each source of Add Device keeps
    /// its own, so a job finishing while another source shows stays here.
    pub(super) error: Option<String>,
    pub(super) suggestion_scroll: ScrollHandle,
    /// Whether the host's key is checked; see `host_key_choice`.
    pub(super) host_keys: HostKeys,
}

/// Where adding a device stands. Each step after `Form` belongs to the request
/// that was submitted, not to whatever the fields hold now.
pub(super) enum Step {
    Form,
    Checking(setup::Request),
    /// macOS refused the host check the local network; waiting for the user
    /// to allow Herdr, then checking again (see `local_network`).
    AwaitingLocalNetwork(setup::Request),
    /// Herdr is present, so the CLI saves the device without a terminal. A
    /// stopped server is started by that same command.
    Saving(setup::Request, HostProbe),
    /// Saved; the next frame closes the dialog (see `poll_device_setup`).
    Saved,
    /// Already saved; keep the recovery advice readable until dismissed.
    SavedWarning(&'static str),
    /// SSH refused an unknown host key; fetching it for the user to approve.
    FetchingKey(setup::Request),
    /// The user compares the host key's fingerprint before trusting it.
    TrustKey(setup::Request, NewHostKey),
    /// Setup needs prompts, so the user decides whether to run it locally.
    Confirm(setup::Request, Offer),
    // Only interactive setup verifies, and Windows cannot host it yet.
    #[cfg(unix)]
    Verifying(setup::Request, Offer),
    #[cfg(unix)]
    Running(setup::Request, progress::Progress),
}

impl SheprdWindow {
    /// Set the SSH form's error, which stays with that form.
    pub(super) fn set_device_error(&mut self, error: Option<String>) {
        if let Some(setup) = &mut self.menu.device_setup {
            setup.error = error;
        }
    }

    /// Show an empty Add Device form with the target field focused.
    pub(in crate::menu) fn open_add_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fields = std::array::from_fn(|index| {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| {
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
                match index {
                    0 => input.set_placeholder_with(|| fl!("add-device-target-placeholder"), cx),
                    1 => input.set_placeholder_with(|| fl!("add-device-label-placeholder"), cx),
                    // Herdr's default session's name.
                    _ => input.set_placeholder("default", cx), // i18n: data
                }
            });
            input
        });
        window.focus(&fields[0].read(cx).focus.clone(), cx);
        let label_edits = cx.subscribe(&fields[1], |this, field, _: &Changed, cx| {
            if let Some(setup) = &mut this.menu.device_setup
                && setup.suggested_label.as_deref() != Some(field.read(cx).text())
            {
                setup.suggested_label = None;
            }
        });
        // An offer runs for the target that was checked, so changing the
        // target withdraws it: the next submit checks the new target.
        let target_edits = cx.subscribe(&fields[0], |this, field, _: &Changed, cx| {
            if let Some(setup) = &mut this.menu.device_setup
                && let Step::Confirm(request, _) = &setup.step
                && request.target() != field.read(cx).text().trim()
            {
                setup.step = Step::Form;
                setup.claim = None;
                setup.error = None;
            }
            cx.notify();
        });
        self.menu.device_setup = Some(Setup {
            fields,
            step: Step::Form,
            claim: None,
            task: None,
            discovery: None,
            suggested_label: None,
            _edits: vec![label_edits, target_edits],
            highlight: None,
            suggestion_scroll: ScrollHandle::new(),
            error: None,
            host_keys: HostKeys::Verify,
        });
        self.menu.page = Some(Page::AddDevice);
        self.start_device_discovery(cx);
    }

    /// Route a key to the Add Device form. Returns `false` when the key is
    /// left to propagate: during composition, or when the form ignores it.
    pub(super) fn add_device_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = &self.menu.device_setup else {
            return false;
        };
        #[cfg(unix)]
        if let Step::Running(_, progress) = &form.step {
            if progress.response.read(cx).is_composing() {
                return false;
            }
            match event.keystroke.key.as_str() {
                "escape" => self.dismiss_menu(window, cx),
                "enter" => self.respond_to_device_setup(None, cx),
                _ => return false,
            }
            return true;
        }
        if matches!(form.step, Step::SavedWarning(_)) {
            if matches!(event.keystroke.key.as_str(), "enter" | "escape") {
                self.dismiss_menu(window, cx);
            }
            // The completed form has no editable fields or further setup action.
            return true;
        }
        if form
            .fields
            .iter()
            .any(|field| field.read(cx).is_composing())
        {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let in_target = form.fields[0].read(cx).focus.is_focused(window);
        let plain = !(modifiers.shift || modifiers.platform || modifiers.control || modifiers.alt);
        if in_target && plain && self.complete_device_target(key, window, cx) {
            return true;
        }
        let Some(form) = &self.menu.device_setup else {
            return false;
        };
        match key {
            "tab" => {
                let index = form
                    .fields
                    .iter()
                    .position(|field| field.read(cx).focus.is_focused(window))
                    .unwrap_or(0);
                let next = (index + if modifiers.shift { 2 } else { 1 }) % 3;
                window.focus(&form.fields[next].read(cx).focus.clone(), cx);
            }
            "enter" => self.submit_device_setup(cx),
            "escape" => self.dismiss_menu(window, cx),
            _ => return false,
        }
        true
    }

    pub(in crate::menu) fn render_add_device(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let frame = |pane: Div, cx: &mut Context<Self>| {
            self.add_device_frame(
                "device-setup-dialog",
                "device-setup-close",
                compact,
                pane,
                cx,
            )
        };
        let Some(setup) = &self.menu.device_setup else {
            return frame(div(), cx);
        };
        #[cfg(unix)]
        if let Step::Running(request, progress) = &setup.step {
            return frame(self.render_device_progress(request, progress, cx), cx);
        }
        if let Step::SavedWarning(warning) = setup.step {
            return frame(self.render_device_saved_warning(warning, cx), cx);
        }
        let ghost = matches!(setup.step, Step::Form)
            .then(|| {
                let text = setup.fields[0].read(cx).text().to_owned();
                self.device_completion(setup, &text).ghost(&text)
            })
            .flatten();
        setup.fields[0].update(cx, |field, cx| field.set_completion(ghost, cx));
        let mut body = div()
            .id("device-setup-body")
            .debug_selector(|| "device-setup-body".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("add-device-intro")),
            );
        for (index, (label, field)) in [
            fl!("add-device-target"),
            fl!("add-device-label"),
            fl!("add-device-session"),
        ]
        .into_iter()
        .zip(&setup.fields)
        .enumerate()
        {
            body = body.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label)
                    .child(field.clone()),
            );
            // Suggestions fill the target, so they sit right under it, and
            // only while the form can still take one.
            if index == 0
                && matches!(setup.step, Step::Form)
                && let Some(discovery) = &setup.discovery
            {
                body = body.child(self.render_device_suggestions(setup, discovery, cx));
            }
        }
        body = body.child(self.render_host_key_choice(setup, cx));
        if let Some(error) = &setup.error {
            body = body.child(
                div()
                    .flex_none()
                    .text_color(crate::menu::danger(theme))
                    .child(error.clone()),
            );
        }
        let status = match &setup.step {
            Step::Form => None,
            Step::Checking(request) => Some(fl!("add-device-checking", target = request.target())),
            Step::AwaitingLocalNetwork(request) => {
                Some(super::local_network::waiting_status(request.target()))
            }
            Step::Saving(request, HostProbe::Stopped) => {
                Some(fl!("add-device-starting", target = request.target()))
            }
            Step::Saving(request, _) => Some(fl!("add-device-saving", target = request.target())),
            Step::Saved | Step::SavedWarning(_) => None,
            Step::FetchingKey(request) => {
                Some(fl!("add-device-fetching-key", target = request.target()))
            }
            Step::TrustKey(request, key) => {
                body = body.child(self.render_host_key(request.target(), key));
                None
            }
            Step::Confirm(request, offer) => Some(offer.question(
                request.target(),
                setup.claim.as_ref().map(setup::Claim::user),
            )),
            #[cfg(unix)]
            Step::Verifying(request, _) => {
                Some(fl!("add-device-verifying", target = request.target()))
            }
            #[cfg(unix)]
            Step::Running(..) => None,
        };
        if let Some(status) = status {
            body = body.child(
                div()
                    .debug_selector(|| "device-setup-status".into())
                    .flex_none()
                    .text_color(rgb(theme.muted))
                    .child(status),
            );
        }
        let button = |id: &'static str, label: SharedString, enabled: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .p(px(8.))
                .rounded(px(crate::config::corners::CONTROL))
                .bg(rgb(theme.active))
                .when(enabled, |button| {
                    button.cursor_pointer().hover(|s| {
                        s.bg(rgb(theme.active).blend(rgba((theme.foreground << 8) | 0x20)))
                    })
                })
                .when(!enabled, |button| button.text_color(rgb(theme.muted)))
                .child(label)
        };
        let mut footer = div()
            .debug_selector(|| "device-setup-footer".into())
            .flex_none()
            .p(px(16.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .flex()
            .justify_end()
            .gap(px(8.));
        let cancel = button("device-setup-cancel", fl!("add-device-cancel").into(), true)
            .on_click(cx.listener(|this, _, _, cx| this.cancel_device_step(cx)));
        footer = match &setup.step {
            Step::Confirm(..) if cfg!(windows) => footer.child(cancel),
            Step::Confirm(_, offer) => footer.child(cancel).child(
                button("device-setup-submit", offer.action().into(), true)
                    .on_click(cx.listener(|this, _, _, cx| this.start_device_setup(cx))),
            ),
            Step::AwaitingLocalNetwork(_) => footer
                .children(self.local_network_settings_button("device-setup-settings", cx))
                .child(cancel),
            // Enter does not trust a key: only this button does.
            Step::TrustKey(..) => footer.child(cancel).child(
                button(
                    "device-setup-trust",
                    fl!("add-device-trust-key").into(),
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| this.trust_host_key(cx))),
            ),
            step => {
                let ready = matches!(step, Step::Form);
                footer.child(
                    button(
                        "device-setup-submit",
                        if ready {
                            fl!("add-device-submit")
                        } else {
                            fl!("add-device-submit-checking")
                        }
                        .into(),
                        ready,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.submit_device_setup(cx))),
                )
            }
        };
        frame(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(body)
                .child(footer),
            cx,
        )
    }

    /// Refuse a host already in the catalog. Checked again before each step
    /// that saves, since another window or the CLI can add it meanwhile. A
    /// device is matched by its saved entry: the sessions list may have attached
    /// it to another session, and that does not add one to the catalog.
    pub(super) fn ensure_new_device(&self, request: &setup::Request) -> crate::Result<()> {
        match self.endpoints.iter().find(|endpoint| {
            endpoint
                .saved_ssh()
                .is_some_and(|(target, session)| request.same_host(target, session))
        }) {
            Some(endpoint) => Err(super::Error::Exists(endpoint.label.clone()).into()),
            None => Ok(()),
        }
    }

    fn submit_device_setup(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        match &form.step {
            Step::Form => {}
            // Windows offers no terminal setup; the question says what to run.
            Step::Confirm(..) if cfg!(windows) => return,
            Step::Confirm(..) => return self.start_device_setup(cx),
            _ => return,
        }
        let request = setup::Request::new(
            form.fields[0].read(cx).text(),
            form.fields[1].read(cx).text(),
            form.fields[2].read(cx).text(),
        )
        .map(|request| request.with_host_keys(form.host_keys));
        let request = match request.and_then(|request| {
            self.ensure_new_device(&request)?;
            Ok(request)
        }) {
            Ok(request) => request,
            Err(error) => {
                tracing::info!(category = "device_setup", "Device form refused");
                self.set_device_error(Some(error.to_string()));
                cx.notify();
                return;
            }
        };
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        tracing::info!(category = "device_setup", "Checking device");
        form.step = Step::Checking(request.clone());
        form.error = None;
        // Claim before probing: a second add of this host, from any window of
        // this process, is refused from here on rather than after a slow probe.
        let background = cx.background_executor().spawn(async move {
            let claim = setup::claim(&request)?;
            Ok::<_, crate::Error>((claim, request.probe()))
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| this.device_probed(result, cx));
        }));
        cx.notify();
    }

    /// Return to the form when the host is already saved or being added:
    /// offering a terminal would only add it again.
    fn refuse_duplicate(&mut self, error: &crate::Error, cx: &mut Context<Self>) -> bool {
        if !matches!(
            error,
            crate::Error::Devices(super::Error::Exists(_) | super::Error::Adding)
        ) {
            return false;
        }
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Form;
            form.claim = None;
        }
        self.set_device_error(Some(error.to_string()));
        cx.notify();
        true
    }

    pub(super) fn device_probed(
        &mut self,
        result: crate::Result<(setup::Claim, crate::Result<HostProbe>)>,
        cx: &mut Context<Self>,
    ) {
        let (request, waited) = match &self.menu.device_setup {
            Some(Setup {
                step: Step::Checking(request),
                ..
            }) => (request.clone(), false),
            Some(Setup {
                step: Step::AwaitingLocalNetwork(request),
                ..
            }) => (request.clone(), true),
            _ => return,
        };
        let (claim, probe) = match result {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(category = "device_setup", "Could not claim device");
                if !self.refuse_duplicate(&error, cx) {
                    self.set_device_error(Some(fl!(
                        "add-device-check-failed",
                        target = request.target(),
                        error = error.to_string(),
                    )));
                    if let Some(form) = &mut self.menu.device_setup {
                        form.step = Step::Confirm(request, Offer::Terminal);
                    }
                    cx.notify();
                }
                return;
            }
        };
        match &probe {
            Ok(probe) => tracing::info!(
                category = "device_setup",
                probe = ?probe,
                "Device probed"
            ),
            Err(_) => tracing::warn!(category = "device_setup", "Device probe failed"),
        }
        // An unknown key can be approved here; a changed one is refused while
        // fetching it, and the terminal is offered instead.
        if let Ok(HostProbe::SshFailed(SshFailure::HostKey)) = probe {
            return self.fetch_host_key(request, claim, cx);
        }
        // A terminal cannot help: the permission belongs to this app. The
        // first refusal can be the system still asking, so wait for the user;
        // a refusal after the wait returns to the form, and the claim drops.
        if let Ok(HostProbe::SshFailed(failure @ SshFailure::LocalNetworkDenied)) = probe {
            if !waited {
                return self.await_local_network(request, claim, cx);
            }
            self.set_device_error(Some(failure.to_string()));
            if let Some(form) = &mut self.menu.device_setup {
                form.step = Step::Form;
            }
            cx.notify();
            return;
        }
        let offer = match probe {
            Ok(probe) => match Offer::for_probe(probe) {
                None => return self.save_device(request, probe, claim, cx),
                Some(offer) => {
                    // Say what SSH refused, since the terminal may not need to.
                    // The key offer already says it, without the advice
                    // about agents that does not apply to a new host.
                    if let HostProbe::SshFailed(failure) = probe
                        && offer != Offer::InstallKey
                    {
                        self.set_device_error(Some(fl!(
                            "add-device-ssh-failed",
                            target = request.target(),
                            error = failure.to_string(),
                        )));
                    }
                    offer
                }
            },
            Err(error) => {
                self.set_device_error(Some(fl!(
                    "add-device-check-failed",
                    target = request.target(),
                    error = error.to_string(),
                )));
                Offer::Terminal
            }
        };
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Confirm(request, offer);
            form.claim = Some(claim);
        }
        cx.notify();
    }

    fn save_device(
        &mut self,
        request: setup::Request,
        probe: HostProbe,
        claim: setup::Claim,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        form.step = Step::Saving(request.clone(), probe);
        // The claim travels with the save and comes back, so a failure that
        // offers a terminal still holds the host.
        let background = cx.background_executor().spawn(async move {
            let result = setup::save(&request, &claim, probe);
            (result, claim)
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let (result, claim) = background.await;
            let _ = this.update(cx, |this, cx| {
                let Some(Setup {
                    step: Step::Saving(request, _),
                    ..
                }) = &this.menu.device_setup
                else {
                    return;
                };
                let request = request.clone();
                match &result {
                    Ok(_) => tracing::info!(category = "device_setup", "Device saved"),
                    Err(_) => tracing::warn!(category = "device_setup", "Device save failed"),
                }
                let error = match result {
                    Ok(outcome) => {
                        this.device_save_succeeded(outcome, cx);
                        return;
                    }
                    Err(error) => error,
                };
                if this.refuse_duplicate(&error, cx) {
                    return;
                }
                this.set_device_error(Some(error.to_string()));
                let offer = match error {
                    crate::Error::Devices(super::Error::ServerRestart) => Offer::Restart,
                    _ => Offer::Terminal,
                };
                if let Some(form) = &mut this.menu.device_setup {
                    form.step = Step::Confirm(request, offer);
                    form.claim = Some(claim);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Back to the form, giving up the host: the next submit, for this host or
    /// another, claims it again.
    pub(super) fn cancel_device_step(&mut self, cx: &mut Context<Self>) {
        if let Some(setup) = &mut self.menu.device_setup {
            setup.step = Step::Form;
            setup.claim = None;
            // Ends a wait for Local Network permission, which holds a claim.
            setup.task = None;
            setup.error = None;
            cx.notify();
        }
    }

    pub(crate) fn poll_device_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Windows has no interactive setup to poll once nothing was saved.
        if !self.poll_saved_device(window, cx) {
            #[cfg(unix)]
            self.poll_device_progress(window, cx);
        }
    }
}
