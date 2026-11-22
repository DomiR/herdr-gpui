//! The approval step's fleet line for another computer's Sheprd
//! (`docs/relay-plan.md`, Adding a nearby Sheprd from another network):
//! the claim asked to join this computer's fleet, or invited this
//! computer into its own. A switch, ticked by default, when Allow can do
//! it; a note when it cannot, because both are in that fleet already or
//! this computer is in another. Allow sends the fleet only when the
//! switch showed ticked, and the daemon refuses one that moved since.

use super::*;
use crate::{fleet::Fleet, toggles::switch};
use sheprd_remote_control::control::fleet::{View, bluetooth::Asked};

/// What the line says for a computer's claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// It joins this computer's `fleet` when allowed.
    Add { fleet: String },
    /// This computer, in no fleet, joins the claimant's `fleet`.
    Join { fleet: String },
    /// Both are in that fleet already.
    Same,
    /// This computer holds another fleet: Allow pairs only.
    Other,
}

impl Line {
    /// The line for `asked`, from this computer's `view` and the fleet it
    /// is a live member of. A join for a fleet this computer is no longer
    /// in shows nothing: Allow pairs only.
    pub(crate) fn of(asked: &Asked, view: Option<&View>, member: Option<&str>) -> Option<Self> {
        match asked {
            Asked::Join { fleet } => (member == Some(fleet.as_str())).then(|| Self::Add {
                fleet: fleet.clone(),
            }),
            Asked::Invite(hint) => match (view, member) {
                (None, _) => Some(Self::Join {
                    fleet: hint.fleet.clone(),
                }),
                (Some(_), Some(ours)) if ours.eq_ignore_ascii_case(&hint.fleet) => Some(Self::Same),
                (Some(_), _) => Some(Self::Other),
            },
        }
    }

    /// The fleet an Allow agrees to with the switch ticked.
    pub(crate) fn agrees(&self) -> Option<&str> {
        match self {
            Self::Add { fleet } | Self::Join { fleet } => Some(fleet),
            Self::Same | Self::Other => None,
        }
    }
}

/// The line for the claim the dialog shows, if it is a computer's that
/// asked something of the fleet.
pub(crate) fn line(phase: &Phase, cx: &App) -> Option<Line> {
    let Phase::Claimed {
        ssh: true,
        fleet: Some(asked),
        ..
    } = phase
    else {
        return None;
    };
    // Until the daemon has said, this computer's fleet is unknown: no
    // switch promises what Allow might not do.
    if !Fleet::read(cx) {
        return None;
    }
    Line::of(asked, Fleet::view(cx), Fleet::member_of(cx).as_deref())
}

impl SheprdWindow {
    /// The line under the matching code, usable until the user answers.
    pub(super) fn pairing_computer_fleet(
        &self,
        device: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let dialog = self.menu.remote_pairing.as_ref()?;
        let line = line(dialog.pairing.phase(), cx)?;
        let theme = &self.theme;
        let (label, note) = match &line {
            Line::Add { .. } => (
                fl!("remote-fleet-add-computer", device = device),
                fl!("remote-fleet-add-computer-note"),
            ),
            Line::Join { .. } => (
                fl!("remote-fleet-join-computer", device = device),
                fl!("remote-fleet-join-computer-note"),
            ),
            Line::Same => {
                return Some(note_line(
                    theme,
                    fl!("remote-fleet-same-computer", device = device),
                ));
            }
            Line::Other => {
                return Some(note_line(
                    theme,
                    fl!("remote-fleet-other-computer", device = device),
                ));
            }
        };
        let on = dialog.join_fleet;
        let toggle = div()
            .id("remote-pair-computer-fleet")
            .debug_selector(|| "remote-pair-computer-fleet".into())
            .role(Role::Switch)
            .aria_label(SharedString::from(label.clone()))
            .aria_toggled(if on { Toggled::True } else { Toggled::False })
            .flex()
            .items_start()
            .gap(px(8.))
            .px(px(4.))
            .py(px(3.))
            .rounded(px(crate::config::corners::CONTROL))
            .when(enabled, |row| {
                row.cursor_pointer()
                    .hover(|row| row.bg(rgb(theme.active)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_pairing_fleet(cx);
                    }))
            })
            .when(!enabled, |row| row.opacity(0.5))
            .child(switch(theme, 16., on))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(label)
                    .child(div().text_color(rgb(theme.subtext())).child(note)),
            );
        Some(div().child(toggle))
    }
}

fn note_line(theme: &Theme, text: String) -> Div {
    div()
        .debug_selector(|| "remote-pair-computer-fleet-note".into())
        .text_color(rgb(theme.subtext()))
        .child(text)
}
