//! What Sheprd asks the daemon about the fleet (`control::fleet::sync::Ask`).
//! Every operation this computer signs is one a person here asked for in
//! Sheprd's own dialog; the daemon only signs, keeps, and passes them on.
//! The one exception is `Seen`: a shared host's newer record, which its own
//! certificate authority signed, carried as a member carries its own.

use super::{Error, Fleet, Result, me};
use crate::control::fleet::{
    Access, Admit, Body, Bundle, HostIntro, HostSeen, Remove, View,
    sync::{self, Answer, Ask},
};
use crate::web::State;
use std::collections::BTreeSet;

/// Answers `ask` at `now_ms`. Blocking.
pub(crate) fn answer(state: &State, fleet: &Fleet, ask: Ask, now_ms: u64) -> Result<Answer> {
    match ask {
        Ask::Status => Ok(Answer::Status {
            view: view(fleet).map(Box::new),
        }),
        Ask::Found => {
            fleet.found(me::bundle(state, fleet, now_ms)?, now_ms)?;
            Ok(Answer::Done)
        }
        Ask::Bundle { fleet: id } => {
            if fleet.in_fleet()? {
                return Err(Error::InFleet);
            }
            let id = sync::fleet(&id)?;
            let bundle = me::bundle(state, fleet, now_ms)?;
            let message = bundle
                .join_message(&id)
                .ok_or(crate::control::fleet::Error::Invalid)?;
            let signature: p256::ecdsa::Signature =
                p256::ecdsa::signature::Signer::sign(fleet.key(), &message);
            Ok(Answer::Bundle {
                bundle: sync::text(&bundle.to_bytes()?),
                signature: sync::text(&signature.to_bytes()),
            })
        }
        Ask::Joining { fleet: id, pairing } => {
            if fleet.in_fleet()? {
                return Err(Error::InFleet);
            }
            let id = sync::fleet(&id)?;
            let pairing: [u8; 32] = sync::bytes(&pairing)?
                .try_into()
                .map_err(|_| crate::control::fleet::Error::Encoding)?;
            let bundle = me::bundle(state, fleet, now_ms)?;
            Ok(Answer::Joining(crate::control::fleet::bluetooth::joining(
                fleet.key(),
                &id,
                &pairing,
                &bundle,
            )?))
        }
        Ask::Admit {
            bundle,
            signature,
            method,
        } => {
            let bundle = Bundle::from_bytes(&sync::bytes(&bundle)?)?;
            // Admitted already with this very bundle, as when an invitation
            // was cut off before the log was handed over: nothing to sign,
            // so the handover can resume.
            let admitted = fleet.inner().log.as_ref().is_some_and(|log| {
                log.state()
                    .members
                    .get(&bundle.id())
                    .is_some_and(|member| member.bundle == bundle)
            });
            if admitted {
                return Ok(Answer::Done);
            }
            let member = bundle.id();
            // Auth, released, then joins: the two locks never nest.
            let paired = paired(state);
            fleet.joins().admissible(&member, &paired)?;
            let joiner_signature = sync::bytes(&signature)?
                .try_into()
                .map_err(|_| crate::control::fleet::Error::JoinSignature)?;
            fleet.sign(
                Body::Admit(Admit {
                    bundle,
                    method,
                    joiner_signature,
                }),
                now_ms,
            )?;
            fleet.joins().admitted(&member);
            Ok(Answer::Done)
        }
        Ask::Remove {
            member,
            also_members,
            also_hosts,
            reason,
        } => {
            let body = Body::Remove(Remove {
                member: sync::member(&member)?,
                also_members: also_members
                    .iter()
                    .map(|member| sync::member(member))
                    .collect::<crate::control::fleet::Result<_>>()?,
                also_hosts: also_hosts
                    .iter()
                    .map(|host| sync::host(host))
                    .collect::<crate::control::fleet::Result<_>>()?,
                reason,
            });
            fleet.sign(body, now_ms)?;
            Ok(Answer::Done)
        }
        Ask::AddHost { intro, deny } => {
            let intro = HostIntro::from_bytes(&sync::bytes(&intro)?)?;
            let host = intro.id()?;
            let deny = deny
                .iter()
                .map(|member| sync::member(member))
                .collect::<crate::control::fleet::Result<BTreeSet<_>>>()?;
            // The host and its exclusions land together or not at all: a
            // host kept without them would be shared wider than asked.
            fleet.sign_all(now_ms, |log| {
                std::iter::once(Body::HostAdd(intro))
                    .chain(deny.into_iter().map(|member| {
                        Body::Access(Access {
                            host,
                            member,
                            allow: false,
                            rev: sync::next_rev(log.state(), host, member),
                        })
                    }))
                    .collect()
            })?;
            Ok(Answer::Done)
        }
        Ask::Access {
            host,
            member,
            allow,
        } => {
            access(
                fleet,
                sync::host(&host)?,
                sync::member(&member)?,
                allow,
                now_ms,
            )?;
            Ok(Answer::Done)
        }
        Ask::RemoveHost { host } => {
            fleet.sign(Body::HostRemove(sync::host(&host)?), now_ms)?;
            Ok(Answer::Done)
        }
        Ask::Intro => {
            let mut intro = me::intro(state, now_ms)?;
            intro.kind = crate::control::fleet::HostKind::Sheprd;
            Ok(Answer::Intro {
                intro: sync::text(&intro.to_bytes()?),
            })
        }
        Ask::Scrub { session } => {
            super::scrub::live(state, fleet, session)?;
            Ok(Answer::Done)
        }
        Ask::AgentAccess { host, suspended } => {
            let host = sync::host(&host)?;
            let asked = crate::control::fleet::access::asked(&suspended);
            let file = fleet.agent_access(&host, asked, now_ms)?;
            let bytes = file
                .to_bytes()
                .map_err(crate::control::fleet::Error::from)?;
            Ok(Answer::AgentAccess {
                access: String::from_utf8_lossy(&bytes).into_owned(),
            })
        }
        Ask::Seen { intro } => {
            let intro = HostIntro::from_bytes(&sync::bytes(&intro)?)?;
            let id = intro.id()?;
            let carry = {
                let inner = fleet.inner();
                let log = inner.log.as_ref().ok_or(Error::NoFleet)?;
                let state = log.state();
                // Only a host the fleet holds, that is no member (a member
                // signs its own), and only a newer record its own
                // certificate authority signed.
                state.hosts.get(&id).is_some_and(|held| {
                    intro.record.host == id
                        && held.intro.ca.is_some()
                        && !state.members.values().any(|member| {
                            member
                                .bundle
                                .host
                                .as_ref()
                                .is_some_and(|own| own.id().ok() == Some(id))
                        })
                        && intro.record.rev > held.record.rev
                        && me::moved(&held.record, &intro.record)
                        && held
                            .intro
                            .vouches(&intro.record, intro.record_signature.as_ref())
                })
            };
            if carry {
                fleet.sign(
                    Body::HostSeen(HostSeen {
                        record: intro.record,
                        signature: intro.record_signature,
                    }),
                    now_ms,
                )?;
            }
            Ok(Answer::Done)
        }
        Ask::RemovedHosts => {
            let inner = fleet.inner();
            let hosts = inner
                .log
                .as_ref()
                .map(sync::removed_hosts)
                .unwrap_or_default();
            Ok(Answer::Hosts { hosts })
        }
        Ask::RemovalFor { host, heads } => {
            let host = sync::host(&host)?;
            let heads = heads
                .iter()
                .map(|head| crate::control::fleet::OpId::from_hex(head))
                .collect::<Option<Vec<_>>>()
                .ok_or(crate::control::fleet::Error::Encoding)?;
            let inner = fleet.inner();
            let log = inner.log.as_ref().ok_or(Error::NoFleet)?;
            Ok(Answer::Applied(sync::removal_for(log, &host, &heads)))
        }
        Ask::Apply(apply) => {
            let mut applied = fleet.apply(&apply, now_ms)?;
            if let (Some(nonce), Some(authority)) = (&apply.nonce, &state.authority) {
                let message = sync::proof_message(nonce, &applied.fleet, &applied.heads);
                if let (Some(key), Some(signature)) =
                    (authority.public_key(), authority.sign(&message))
                {
                    applied.proof = Some(sync::Proof {
                        certificate: sync::text(authority.ca_der()),
                        key: sync::text(&key),
                        signature: sync::text(&signature),
                    });
                }
            }
            Ok(Answer::Applied(applied))
        }
        Ask::Joins => {
            let paired = paired(state);
            Ok(Answer::Joins {
                joins: fleet.joins().list(now_ms, &paired),
            })
        }
    }
}

/// The devices paired here now, read before the joins are locked so the
/// two locks never nest.
fn paired(state: &State) -> std::collections::HashSet<crate::control::DeviceId> {
    state.auth().devices().ids().cloned().collect()
}

fn access(
    fleet: &Fleet,
    host: crate::control::fleet::HostId,
    member: crate::control::fleet::MemberId,
    allow: bool,
    now_ms: u64,
) -> Result<()> {
    let rev = {
        let inner = fleet.inner();
        let log = inner.log.as_ref().ok_or(Error::NoFleet)?;
        sync::next_rev(log.state(), host, member)
    };
    fleet.sign(
        Body::Access(Access {
            host,
            member,
            allow,
            rev,
        }),
        now_ms,
    )?;
    Ok(())
}

/// The fleet as this computer sees it, when it is in one.
pub(crate) fn view(fleet: &Fleet) -> Option<View> {
    let suspended = fleet.suspended();
    let mut inner = fleet.inner();
    // A fleet this computer left, once the removal was passed on, is gone.
    if let Err(error) = fleet.release_if_left(&mut inner) {
        tracing::warn!(%error, "could not forget a fleet this computer left");
    }
    inner.log.as_ref().map(|log| View {
        hosted: fleet
            .host()
            .filter(|host| log.state().hosts.contains_key(host))
            .map(|host| host.to_string()),
        ..View::of(log, &fleet.me(), &suspended)
    })
}
