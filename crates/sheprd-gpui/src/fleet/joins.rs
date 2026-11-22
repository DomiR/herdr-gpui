//! Phones asking to join through this computer's daemon (`Ask::Joins`).
//!
//! A join is admitted only with a person's approval here: either the
//! pairing dialog's Also add to your fleet, given for one device id and
//! good for [`EXPECT`], or the second click on Admit in Settings > Fleet.
//! Nothing else ever admits a join.

use super::Fleet;
use gpui::{App, BorrowAppContext};
use sheprd_remote_control::control::{
    DeviceId, Request, Response,
    fleet::{
        Method,
        sync::{Answer, Ask, Join},
    },
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// How long a pairing's Also add to your fleet waits for the phone's join;
/// the daemon keeps a join as long.
pub(super) const EXPECT: Duration = Duration::from_secs(600);
/// How often the daemon is asked while a join is expected.
pub(super) const WATCH: Duration = Duration::from_secs(5);

/// An approval given at pairing: the fleet it was given for, and when.
pub(super) type Expected = HashMap<DeviceId, (String, Instant)>;

/// Splits `joins` into those an approval in `expected` covers for `fleet`,
/// this computer's fleet now, and the rest, which wait for a person. An
/// approval is used once, and dropped once it is older than [`EXPECT`] or
/// was given for another fleet.
pub(super) fn settle(
    expected: &mut Expected,
    fleet: &str,
    joins: Vec<Join>,
    now: Instant,
) -> (Vec<Join>, Vec<Join>) {
    expected.retain(|_, (given, at)| given == fleet && now.duration_since(*at) < EXPECT);
    joins
        .into_iter()
        .partition(|join| expected.remove(&join.device_id).is_some())
}

/// What admits `join`.
pub(crate) fn admit(join: &Join) -> Ask {
    Ask::Admit {
        bundle: join.bundle.clone(),
        signature: join.signature.clone(),
        // A phone joins only once it is paired, and pairing is the QR
        // code or nearby code comparison.
        method: Method::Qr,
    }
}

impl Fleet {
    /// Admits the phone `device` to `fleet` when it asks to join within
    /// [`EXPECT`]: the person ticked Also add to your fleet, shown for
    /// that fleet, when approving it.
    pub(crate) fn expect(cx: &mut App, device: DeviceId, fleet: String) {
        if !cx.has_global::<Self>() {
            return;
        }
        cx.update_global::<Self, _>(|this, _| {
            this.expected.insert(device, (fleet, Instant::now()));
        });
        Self::refresh(cx);
    }

    /// The approvals given at pairing and not yet spent.
    #[cfg(test)]
    pub(crate) fn expected(cx: &App) -> Vec<(DeviceId, String)> {
        cx.try_global::<Self>()
            .map(|fleet| {
                fleet
                    .expected
                    .iter()
                    .map(|(device, (given, _))| (device.clone(), given.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Sets the view as the daemon would have answered it.
    #[cfg(test)]
    pub(crate) fn set_view(
        cx: &mut App,
        view: Option<sheprd_remote_control::control::fleet::View>,
    ) {
        cx.update_global::<Self, _>(|this, _| {
            this.view = view;
            this.read = true;
        });
    }

    /// The joins waiting for a person, oldest first.
    pub(crate) fn joins(cx: &App) -> &[Join] {
        cx.try_global::<Self>()
            .map_or(&[], |fleet| fleet.joins.as_slice())
    }

    /// How soon the view is read again.
    pub(super) fn pace(&self) -> Duration {
        if self.expected.is_empty() {
            super::VIEW
        } else {
            WATCH
        }
    }

    /// Reads the joins waiting, admits those approved at pairing, and
    /// keeps the rest for the page.
    pub(super) fn read_joins(cx: &mut App) {
        let Some(client) = crate::remote::Supervisor::client(cx) else {
            return;
        };
        let task = client.request(Request::Fleet(Box::new(Ask::Joins)));
        cx.spawn(async move |cx| {
            let answer = task.await;
            cx.update(|cx| {
                let joins = match answer {
                    Ok(Response::Fleet(answer)) => match *answer {
                        Answer::Joins { joins } => joins,
                        _ => return,
                    },
                    // An older daemon has no joins to list.
                    _ => return,
                };
                let admit = cx.update_global::<Self, _>(|this, _| {
                    let fleet = this.member_fleet().map(str::to_owned).unwrap_or_default();
                    let (admit, rest) = settle(&mut this.expected, &fleet, joins, Instant::now());
                    this.joins = rest;
                    admit
                });
                for join in &admit {
                    Self::act(cx, self::admit(join), |failure, cx| {
                        if failure.is_some() {
                            cx.update_global::<Self, _>(|this, _| this.error = failure);
                        }
                    });
                }
                cx.refresh_windows();
            });
        })
        .detach();
    }
}
