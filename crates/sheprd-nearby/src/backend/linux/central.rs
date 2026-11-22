//! Scanning with BlueZ over D-Bus: discovery filtered to the nearby
//! service on the first adapter, `org.bluez.Device1` to connect, and
//! `GattCharacteristic1` to read `info` and carry `pair`. Every call that
//! can wait on the radio runs on a thread of its own and reports back
//! through the [`Inbox`]; BlueZ's signals are followed on one more.
//! Discovery is the exception: starting and stopping it run on one worker,
//! in the order asked, so a late stop cannot undo a later start.

use super::{BLUEZ, CHARACTERISTIC, Managed, Properties};
use crate::{
    Error, Result,
    backend::{INFO, PAIR, SERVICE},
    central::{Action, Inbox, Input, Link, latest::Latest},
    framing::MIN_CHUNK,
    protocol::Radio,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    thread,
    time::{Duration, Instant},
};
use zbus::{
    MatchRule, blocking,
    message::Type,
    names::InterfaceName,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const GATT_SERVICE: &str = "org.bluez.GattService1";
/// How long BlueZ gets to look up a connected computer's services.
const RESOLVE_WAIT: Duration = Duration::from_secs(15);
const RESOLVE_POLL: Duration = Duration::from_millis(200);
/// What a write carries beyond the ATT header.
const ATT_HEADER: usize = 3;

/// The object paths of a computer's `info` and `pair`, once resolved.
#[derive(Clone, Debug, Default)]
struct Characteristics {
    info: Option<String>,
    pair: Option<String>,
}

/// Each computer's characteristics, by device path; shared with the
/// threads that resolve them and the one that follows notifications.
type Known = Arc<Mutex<HashMap<String, Characteristics>>>;

pub(crate) struct Scanner {
    conn: blocking::Connection,
    adapter: String,
    known: Known,
    inbox: Inbox,
    /// Discovery's latest change, for the worker that applies them.
    discovery: Arc<Latest<bool>>,
}

impl Scanner {
    /// Finds the first adapter, reports its power, and follows BlueZ.
    pub(crate) fn open(inbox: Inbox) -> Result<Self> {
        let conn = blocking::Connection::system()?;
        let Some((adapter, interfaces)) = managed(&conn)?
            .into_iter()
            .filter(|(_, interfaces)| interfaces.contains_key(ADAPTER))
            .min_by(|(a, _), (b, _)| a.as_str().cmp(b.as_str()))
        else {
            return Err(Error::Gatt {
                step: "find a Bluetooth adapter",
                code: 0,
            });
        };
        let powered = interfaces
            .get(ADAPTER)
            .and_then(|adapter| adapter.get("Powered"))
            .and_then(|value| bool::try_from(value).ok());
        inbox.send(Input::Radio(match powered {
            Some(true) => Radio::PoweredOn,
            Some(false) => Radio::PoweredOff,
            None => Radio::Unknown,
        }));
        let scanner = Self {
            conn,
            adapter: adapter.as_str().to_owned(),
            known: Arc::default(),
            inbox,
            discovery: Arc::default(),
        };
        scanner.watch()?;
        scanner.discover_in_order();
        Ok(scanner)
    }

    /// Applies discovery changes one at a time, each only after the one
    /// before it returned; a refusal is reported so the central asks again.
    fn discover_in_order(&self) {
        let (conn, adapter, inbox) = (self.conn.clone(), self.adapter.clone(), self.inbox.clone());
        let discovery = self.discovery.clone();
        thread::spawn(move || {
            while let Some(on) = discovery.take() {
                if let Err(error) = discover(&conn, &adapter, on) {
                    inbox.send(Input::DiscoveryFailed {
                        on,
                        failure: error.to_string(),
                    });
                }
            }
        });
    }

    /// Follows the adapter's power, computers appearing and leaving, and
    /// `pair` notifications, from BlueZ's signals.
    fn watch(&self) -> Result<()> {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(BLUEZ)?
            .build();
        let signals = blocking::MessageIterator::for_match_rule(rule, &self.conn, Some(256))?;
        let mut watcher = Watcher {
            conn: self.conn.clone(),
            prefix: format!("{}/", self.adapter),
            adapter: self.adapter.clone(),
            known: self.known.clone(),
            inbox: self.inbox.clone(),
            offers: HashMap::new(),
        };
        thread::spawn(move || {
            for message in signals {
                let Ok(message) = message else { continue };
                watcher.signal(&message);
            }
        });
        Ok(())
    }

    fn characteristics(&self, device: &str) -> Characteristics {
        lock(&self.known).get(device).cloned().unwrap_or_default()
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.discovery.close();
    }
}

impl Link for Scanner {
    fn act(&self, action: Action) {
        let (conn, inbox) = (self.conn.clone(), self.inbox.clone());
        match action {
            Action::Discover(on) => self.discovery.put(on),
            Action::Connect(device) => {
                thread::spawn(move || match call(&conn, &device, DEVICE, "Connect", &()) {
                    Ok(()) => inbox.send(Input::Connected(device)),
                    Err(error)
                        if method_error(&error) == Some("org.bluez.Error.AlreadyConnected") =>
                    {
                        inbox.send(Input::Connected(device));
                    }
                    Err(_) => inbox.send(Input::Disconnected(device)),
                });
            }
            Action::Resolve(device) => {
                let known = self.known.clone();
                thread::spawn(move || {
                    let found = resolve(&conn, &device).unwrap_or_default();
                    let chunk = found.1.unwrap_or(MIN_CHUNK);
                    let characteristics = found.0;
                    let (info, pair) = (
                        characteristics.info.is_some(),
                        characteristics.pair.is_some(),
                    );
                    lock(&known).insert(device.clone(), characteristics);
                    inbox.send(Input::Resolved {
                        device,
                        info,
                        pair,
                        chunk,
                    });
                });
            }
            Action::ReadInfo { device, attempt } => {
                let Some(path) = self.characteristics(&device).info else {
                    return;
                };
                thread::spawn(move || {
                    let value = conn
                        .call_method(
                            Some(BLUEZ),
                            path.as_str(),
                            Some(CHARACTERISTIC),
                            "ReadValue",
                            &(HashMap::<&str, Value<'_>>::new(),),
                        )
                        .ok()
                        .and_then(|reply| reply.body().deserialize::<Vec<u8>>().ok());
                    inbox.send(Input::Info {
                        device,
                        attempt,
                        value,
                    });
                });
            }
            Action::Subscribe { device, on } => {
                let Some(path) = self.characteristics(&device).pair else {
                    return;
                };
                thread::spawn(move || {
                    let method = if on { "StartNotify" } else { "StopNotify" };
                    let done = call(&conn, &path, CHARACTERISTIC, method, &()).is_ok();
                    inbox.send(Input::Subscribed {
                        device,
                        notifying: on && done,
                    });
                });
            }
            Action::Write { device, chunk } => {
                let Some(path) = self.characteristics(&device).pair else {
                    inbox.send(Input::Written {
                        device,
                        failure: Some("not connected".to_owned()),
                    });
                    return;
                };
                thread::spawn(move || {
                    let options = HashMap::from([("type", Value::from("request"))]);
                    let failure = conn
                        .call_method(
                            Some(BLUEZ),
                            path.as_str(),
                            Some(CHARACTERISTIC),
                            "WriteValue",
                            &(chunk, options),
                        )
                        .err()
                        .map(|error| error.to_string());
                    inbox.send(Input::Written { device, failure });
                });
            }
            Action::Disconnect(device) => {
                lock(&self.known).remove(&device);
                // BlueZ reports `Connected` false, which the watcher passes on.
                thread::spawn(move || {
                    let _ = call(&conn, &device, DEVICE, "Disconnect", &());
                });
            }
        }
    }
}

/// Follows BlueZ's signals for the scanner.
struct Watcher {
    conn: blocking::Connection,
    adapter: String,
    /// The adapter's path and a slash: its devices' paths start with it.
    prefix: String,
    known: Known,
    inbox: Inbox,
    /// Whether each device offers the nearby service, so an advertisement
    /// seen again costs no call.
    offers: HashMap<String, bool>,
}

impl Watcher {
    fn signal(&mut self, message: &zbus::Message) {
        let header = message.header();
        let (Some(path), Some(member)) = (header.path(), header.member()) else {
            return;
        };
        let path = path.as_str();
        match member.as_str() {
            "InterfacesAdded" => {
                let Ok((added, interfaces)) = message
                    .body()
                    .deserialize::<(OwnedObjectPath, HashMap<String, Properties>)>()
                else {
                    return;
                };
                let Some(device) = interfaces.get(DEVICE) else {
                    return;
                };
                let added = added.as_str();
                if added.starts_with(&self.prefix) && offers(device) {
                    self.offers.insert(added.to_owned(), true);
                    self.inbox.send(Input::Discovered(added.to_owned()));
                }
            }
            "InterfacesRemoved" => {
                let Ok((removed, interfaces)) = message
                    .body()
                    .deserialize::<(OwnedObjectPath, Vec<String>)>()
                else {
                    return;
                };
                if interfaces.iter().any(|interface| interface == DEVICE) {
                    self.offers.remove(removed.as_str());
                    self.inbox
                        .send(Input::Disconnected(removed.as_str().to_owned()));
                }
            }
            "PropertiesChanged" => {
                let Ok((interface, changed, _)) = message
                    .body()
                    .deserialize::<(String, Properties, Vec<String>)>()
                else {
                    return;
                };
                self.changed(path, &interface, &changed);
            }
            _ => {}
        }
    }

    fn changed(&mut self, path: &str, interface: &str, changed: &Properties) {
        let flag = |name: &str| {
            changed
                .get(name)
                .and_then(|value| bool::try_from(value).ok())
        };
        match interface {
            ADAPTER if path == self.adapter => {
                if let Some(powered) = flag("Powered") {
                    self.inbox.send(Input::Radio(if powered {
                        Radio::PoweredOn
                    } else {
                        Radio::PoweredOff
                    }));
                }
            }
            DEVICE if path.starts_with(&self.prefix) => {
                if flag("Connected") == Some(false) {
                    self.inbox.send(Input::Disconnected(path.to_owned()));
                    return;
                }
                if changed.contains_key("UUIDs") {
                    self.offers.insert(path.to_owned(), offers(changed));
                }
                // A cached computer is not added again: its signal strength
                // changing is how BlueZ says it is advertising now.
                if !changed.contains_key("RSSI") && !changed.contains_key("UUIDs") {
                    return;
                }
                let offered = match self.offers.get(path) {
                    Some(offered) => *offered,
                    None => {
                        let offered = property(&self.conn, path, DEVICE, "UUIDs")
                            .is_ok_and(|uuids| has_service(&uuids));
                        self.offers.insert(path.to_owned(), offered);
                        offered
                    }
                };
                if offered {
                    self.inbox.send(Input::Discovered(path.to_owned()));
                }
            }
            CHARACTERISTIC => {
                let Some(value) = changed.get("Value").map(bytes) else {
                    return;
                };
                let device = lock(&self.known)
                    .iter()
                    .find(|(_, known)| known.pair.as_deref() == Some(path))
                    .map(|(device, _)| device.clone());
                if let Some(device) = device {
                    self.inbox.send(Input::Notified { device, value });
                }
            }
            _ => {}
        }
    }
}

fn lock(known: &Known) -> std::sync::MutexGuard<'_, HashMap<String, Characteristics>> {
    known.lock().unwrap_or_else(PoisonError::into_inner)
}

fn managed(conn: &blocking::Connection) -> Result<Managed> {
    Ok(conn
        .call_method(
            Some(BLUEZ),
            "/",
            Some("org.freedesktop.DBus.ObjectManager"),
            "GetManagedObjects",
            &(),
        )?
        .body()
        .deserialize()?)
}

/// Calls a method whose answer carries nothing.
fn call(
    conn: &blocking::Connection,
    path: &str,
    interface: &str,
    method: &str,
    body: &(),
) -> zbus::Result<()> {
    conn.call_method(Some(BLUEZ), path, Some(interface), method, body)?;
    Ok(())
}

/// The D-Bus error name of a refused call.
fn method_error(error: &zbus::Error) -> Option<&str> {
    match error {
        zbus::Error::MethodError(name, _, _) => Some(name.as_str()),
        _ => None,
    }
}

fn property(
    conn: &blocking::Connection,
    path: &str,
    interface: &'static str,
    name: &str,
) -> Result<OwnedValue> {
    let proxy = blocking::fdo::PropertiesProxy::builder(conn)
        .destination(BLUEZ)?
        .path(path.to_owned())?
        .build()?;
    let interface = InterfaceName::from_static_str(interface).map_err(zbus::Error::from)?;
    Ok(proxy.get(interface, name)?)
}

/// Starts discovery of computers advertising the nearby service, or stops.
fn discover(conn: &blocking::Connection, adapter: &str, on: bool) -> zbus::Result<()> {
    if !on {
        return call(conn, adapter, ADAPTER, "StopDiscovery", &());
    }
    let filter = HashMap::from([
        ("UUIDs", Value::from(vec![SERVICE])),
        ("Transport", Value::from("le")),
    ]);
    conn.call_method(
        Some(BLUEZ),
        adapter,
        Some(ADAPTER),
        "SetDiscoveryFilter",
        &(filter,),
    )?;
    match call(conn, adapter, ADAPTER, "StartDiscovery", &()) {
        Err(error) if method_error(&error) == Some("org.bluez.Error.InProgress") => Ok(()),
        result => result,
    }
}

/// Waits for BlueZ to look up `device`'s services, then finds the nearby
/// service's `info` and `pair`, and the largest write `pair` takes.
fn resolve(conn: &blocking::Connection, device: &str) -> Result<(Characteristics, Option<usize>)> {
    let deadline = Instant::now() + RESOLVE_WAIT;
    loop {
        let resolved = property(conn, device, DEVICE, "ServicesResolved")
            .ok()
            .and_then(|value| bool::try_from(&value).ok());
        if resolved == Some(true) {
            break;
        }
        if Instant::now() >= deadline {
            return Ok((Characteristics::default(), None));
        }
        thread::sleep(RESOLVE_POLL);
    }
    let managed = managed(conn)?;
    let prefix = format!("{device}/");
    let uuid = |interfaces: &HashMap<String, Properties>, interface: &str| {
        interfaces
            .get(interface)
            .and_then(|properties| properties.get("UUID"))
            .and_then(|value| match &**value {
                Value::Str(uuid) => Some(uuid.as_str().to_ascii_lowercase()),
                _ => None,
            })
    };
    let Some(service) = managed
        .iter()
        .filter(|(path, _)| path.as_str().starts_with(&prefix))
        .find(|(_, interfaces)| uuid(interfaces, GATT_SERVICE).as_deref() == Some(SERVICE))
        .map(|(path, _)| format!("{}/", path.as_str()))
    else {
        return Ok((Characteristics::default(), None));
    };
    let mut found = Characteristics::default();
    let mut chunk = None;
    for (path, interfaces) in &managed {
        if !path.as_str().starts_with(&service) {
            continue;
        }
        match uuid(interfaces, CHARACTERISTIC).as_deref() {
            Some(INFO) => found.info = Some(path.as_str().to_owned()),
            Some(PAIR) => {
                found.pair = Some(path.as_str().to_owned());
                chunk = interfaces
                    .get(CHARACTERISTIC)
                    .and_then(|properties| properties.get("MTU"))
                    .and_then(|value| u16::try_from(value).ok())
                    .map(|mtu| usize::from(mtu).saturating_sub(ATT_HEADER));
            }
            _ => {}
        }
    }
    Ok((found, chunk))
}

/// Whether a device's properties list the nearby service.
fn offers(properties: &Properties) -> bool {
    properties.get("UUIDs").is_some_and(has_service)
}

fn has_service(uuids: &OwnedValue) -> bool {
    match &**uuids {
        Value::Array(array) => array.iter().any(|uuid| match uuid {
            Value::Str(uuid) => uuid.as_str().eq_ignore_ascii_case(SERVICE),
            _ => false,
        }),
        _ => false,
    }
}

/// A byte array property, such as a characteristic's `Value`.
fn bytes(value: &OwnedValue) -> Vec<u8> {
    match &**value {
        Value::Array(array) => array
            .iter()
            .filter_map(|byte| match byte {
                Value::U8(byte) => Some(*byte),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}
