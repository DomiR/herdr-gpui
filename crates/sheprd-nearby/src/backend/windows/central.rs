//! Scanning with WinRT: a `BluetoothLEAdvertisementWatcher` filtered to
//! the nearby service, then `BluetoothLEDevice` and its
//! `GattDeviceService` to read `info` and carry `pair`. WinRT's
//! asynchronous calls are waited for on a thread per action, which reports
//! back through the [`Inbox`]; its events arrive on its thread pool.
//! Computers are named by their Bluetooth address in hex.

use super::{ATT_HEADER, buffer, bytes, guid, state};
use crate::{
    Error, Result,
    backend::{INFO, PAIR, SERVICE},
    central::{Action, Inbox, Input, Link},
    framing::MIN_CHUNK,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    thread,
};
use windows::{
    Devices::{
        Bluetooth::{
            Advertisement::{
                BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
                BluetoothLEScanningMode,
            },
            BluetoothAdapter, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice,
            GenericAttributeProfile::{
                GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue,
                GattCommunicationStatus, GattDeviceService, GattSession, GattValueChangedEventArgs,
                GattWriteOption,
            },
        },
        Radios::Radio as WinRadio,
    },
    Foundation::TypedEventHandler,
    core::{IInspectable, Ref},
};

/// A computer connected to read its `info`, or to pair.
struct Peer {
    device: BluetoothLEDevice,
    /// Held so Windows keeps the link up between calls.
    session: Option<GattSession>,
    service: Option<GattDeviceService>,
    info: Option<GattCharacteristic>,
    pair: Option<GattCharacteristic>,
}

impl Peer {
    fn close(&self) {
        if let Some(service) = &self.service {
            let _ = service.Close();
        }
        if let Some(session) = &self.session {
            let _ = session.Close();
        }
        let _ = self.device.Close();
    }
}

type Peers = Arc<Mutex<HashMap<String, Peer>>>;

pub(crate) struct Scanner {
    watcher: BluetoothLEAdvertisementWatcher,
    peers: Peers,
    inbox: Inbox,
    /// Kept so its state handler stays registered.
    _radio: WinRadio,
}

impl Scanner {
    /// Checks the adapter can scan, reports its radio, and prepares the
    /// watcher; discovery starts with the first [`Action::Discover`].
    pub(crate) fn open(inbox: Inbox) -> Result<Self> {
        let adapter = BluetoothAdapter::GetDefaultAsync()?.join()?;
        if !adapter.IsCentralRoleSupported()? {
            return Err(Error::Gatt {
                step: "be a Bluetooth central",
                code: 0,
            });
        }
        let radio = adapter.GetRadioAsync()?.join()?;
        inbox.send(Input::Radio(state(radio.State()?)));
        let reporter = inbox.clone();
        radio.StateChanged(&TypedEventHandler::<WinRadio, IInspectable>::new(
            move |radio: Ref<WinRadio>, _| {
                reporter.send(Input::Radio(state(radio.ok()?.State()?)));
                Ok(())
            },
        ))?;

        let watcher = BluetoothLEAdvertisementWatcher::new()?;
        watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;
        watcher
            .AdvertisementFilter()?
            .Advertisement()?
            .ServiceUuids()?
            .Append(guid(SERVICE))?;
        let finder = inbox.clone();
        watcher.Received(&TypedEventHandler::<
            BluetoothLEAdvertisementWatcher,
            BluetoothLEAdvertisementReceivedEventArgs,
        >::new(move |_, args| {
            let address = args.ok()?.BluetoothAddress()?;
            finder.send(Input::Discovered(key(address)));
            Ok(())
        }))?;
        Ok(Self {
            watcher,
            peers: Arc::default(),
            inbox,
            _radio: radio,
        })
    }

    /// `device`'s characteristic, chosen by `pick`, when resolved.
    fn characteristic(
        &self,
        device: &str,
        pick: impl FnOnce(&Peer) -> Option<&GattCharacteristic>,
    ) -> Option<GattCharacteristic> {
        lock(&self.peers).get(device).and_then(pick).cloned()
    }
}

impl Link for Scanner {
    fn act(&self, action: Action) {
        let inbox = self.inbox.clone();
        match action {
            Action::Discover(on) => {
                let result = if on {
                    self.watcher.Start()
                } else {
                    self.watcher.Stop()
                };
                if let Err(error) = result {
                    inbox.send(Input::DiscoveryFailed {
                        on,
                        failure: error.to_string(),
                    });
                }
            }
            Action::Connect(device) => {
                let peers = self.peers.clone();
                thread::spawn(move || match connect(&device, &inbox) {
                    Ok(found) => {
                        if let Some(old) = lock(&peers).insert(device.clone(), found) {
                            old.close();
                        }
                        inbox.send(Input::Connected(device));
                    }
                    Err(_) => inbox.send(Input::Disconnected(device)),
                });
            }
            Action::Resolve(device) => {
                let peers = self.peers.clone();
                thread::spawn(move || {
                    let Some(handle) = lock(&peers).get(&device).map(|peer| peer.device.clone())
                    else {
                        inbox.send(Input::Disconnected(device));
                        return;
                    };
                    let resolved = resolve(&handle, &device, &inbox).unwrap_or_default();
                    let (info, pair) = (resolved.info.is_some(), resolved.pair.is_some());
                    let chunk = resolved.chunk;
                    if let Some(peer) = lock(&peers).get_mut(&device) {
                        peer.session = resolved.session;
                        peer.service = resolved.service;
                        peer.info = resolved.info;
                        peer.pair = resolved.pair;
                    }
                    inbox.send(Input::Resolved {
                        device,
                        info,
                        pair,
                        chunk,
                    });
                });
            }
            Action::ReadInfo { device, attempt } => {
                let Some(info) = self.characteristic(&device, |peer| peer.info.as_ref()) else {
                    return;
                };
                thread::spawn(move || {
                    let value = (|| -> windows::core::Result<Option<Vec<u8>>> {
                        let read = info
                            .ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?
                            .join()?;
                        if read.Status()? != GattCommunicationStatus::Success {
                            return Ok(None);
                        }
                        Ok(Some(bytes(&read.Value()?)?))
                    })()
                    .ok()
                    .flatten();
                    inbox.send(Input::Info {
                        device,
                        attempt,
                        value,
                    });
                });
            }
            Action::Subscribe { device, on } => {
                let Some(pair) = self.characteristic(&device, |peer| peer.pair.as_ref()) else {
                    return;
                };
                thread::spawn(move || {
                    let value = if on {
                        GattClientCharacteristicConfigurationDescriptorValue::Notify
                    } else {
                        GattClientCharacteristicConfigurationDescriptorValue::None
                    };
                    let done = pair
                        .WriteClientCharacteristicConfigurationDescriptorAsync(value)
                        .and_then(|operation| operation.join())
                        .is_ok_and(|status| status == GattCommunicationStatus::Success);
                    inbox.send(Input::Subscribed {
                        device,
                        notifying: on && done,
                    });
                });
            }
            Action::Write { device, chunk } => {
                let Some(pair) = self.characteristic(&device, |peer| peer.pair.as_ref()) else {
                    inbox.send(Input::Written {
                        device,
                        failure: Some("not connected".to_owned()),
                    });
                    return;
                };
                thread::spawn(move || {
                    let status = (|| -> windows::core::Result<GattCommunicationStatus> {
                        pair.WriteValueWithResultAndOptionAsync(
                            &buffer(&chunk)?,
                            GattWriteOption::WriteWithResponse,
                        )?
                        .join()?
                        .Status()
                    })();
                    let failure = match status {
                        Ok(GattCommunicationStatus::Success) => None,
                        Ok(status) => Some(format!("status {}", status.0)),
                        Err(error) => Some(error.to_string()),
                    };
                    inbox.send(Input::Written { device, failure });
                });
            }
            Action::Disconnect(device) => {
                if let Some(peer) = lock(&self.peers).remove(&device) {
                    peer.close();
                }
                inbox.send(Input::Disconnected(device));
            }
        }
    }
}

fn lock(peers: &Peers) -> MutexGuard<'_, HashMap<String, Peer>> {
    peers.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A computer's key: its Bluetooth address in hex.
fn key(address: u64) -> String {
    format!("{address:012X}")
}

/// Opens the computer at `device`'s address and follows its link.
fn connect(device: &str, inbox: &Inbox) -> Result<Peer> {
    let address = u64::from_str_radix(device, 16).map_err(|_| Error::Gatt {
        step: "read a Bluetooth address",
        code: 0,
    })?;
    let handle = BluetoothLEDevice::FromBluetoothAddressAsync(address)?.join()?;
    let (reporter, key) = (inbox.clone(), device.to_owned());
    handle.ConnectionStatusChanged(&TypedEventHandler::<BluetoothLEDevice, IInspectable>::new(
        move |handle: Ref<BluetoothLEDevice>, _| {
            if handle.ok()?.ConnectionStatus()? == BluetoothConnectionStatus::Disconnected {
                reporter.send(Input::Disconnected(key.clone()));
            }
            Ok(())
        },
    ))?;
    Ok(Peer {
        device: handle,
        session: None,
        service: None,
        info: None,
        pair: None,
    })
}

/// What [`resolve`] found.
#[derive(Default)]
struct Resolved {
    session: Option<GattSession>,
    service: Option<GattDeviceService>,
    info: Option<GattCharacteristic>,
    pair: Option<GattCharacteristic>,
    chunk: usize,
}

/// Finds the nearby service's `info` and `pair`, follows `pair`'s
/// notifications, and learns the largest write the link takes.
fn resolve(handle: &BluetoothLEDevice, device: &str, inbox: &Inbox) -> Result<Resolved> {
    let session = GattSession::FromDeviceIdAsync(&handle.BluetoothDeviceId()?)?.join()?;
    session.SetMaintainConnection(true)?;
    let chunk = session
        .MaxPduSize()
        .map_or(MIN_CHUNK, |pdu| usize::from(pdu.saturating_sub(ATT_HEADER)));
    let services = handle
        .GetGattServicesForUuidWithCacheModeAsync(guid(SERVICE), BluetoothCacheMode::Uncached)?
        .join()?;
    if services.Status()? != GattCommunicationStatus::Success {
        return Ok(Resolved::default());
    }
    let Some(service) = services.Services()?.into_iter().next() else {
        return Ok(Resolved::default());
    };
    let find = |uuid: &str| -> Result<Option<GattCharacteristic>> {
        let found = service
            .GetCharacteristicsForUuidWithCacheModeAsync(guid(uuid), BluetoothCacheMode::Uncached)?
            .join()?;
        if found.Status()? != GattCommunicationStatus::Success {
            return Ok(None);
        }
        Ok(found.Characteristics()?.into_iter().next())
    };
    let info = find(INFO)?;
    let pair = find(PAIR)?;
    if let Some(pair) = &pair {
        let (reporter, key) = (inbox.clone(), device.to_owned());
        pair.ValueChanged(&TypedEventHandler::<
            GattCharacteristic,
            GattValueChangedEventArgs,
        >::new(move |_, args| {
            let value = bytes(&args.ok()?.CharacteristicValue()?)?;
            reporter.send(Input::Notified {
                device: key.clone(),
                value,
            });
            Ok(())
        }))?;
    }
    Ok(Resolved {
        session: Some(session),
        service: Some(service),
        info,
        pair,
        chunk,
    })
}
