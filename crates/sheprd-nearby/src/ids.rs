//! The ids Sheprd names devices by: shaped like the UUID Core Bluetooth
//! gives a phone or a computer, made from the platform's own key (a BlueZ
//! device path, a Windows device id or address) the first time it is seen,
//! and different between runs.

use std::{
    collections::HashMap,
    hash::{BuildHasher, Hasher},
};

#[derive(Debug)]
pub(crate) struct Ids {
    seed: u64,
    /// The id for each platform key.
    ids: HashMap<String, String>,
    /// The platform key for each id.
    devices: HashMap<String, String>,
}

impl Ids {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            seed,
            ids: HashMap::new(),
            devices: HashMap::new(),
        }
    }

    /// The id `device` is known by, made the first time it is seen.
    pub(crate) fn id(&mut self, device: &str) -> String {
        if let Some(id) = self.ids.get(device) {
            return id.clone();
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write_u64(self.seed);
        hasher.write(device.as_bytes());
        let high = hasher.finish();
        hasher.write_u64(high);
        let low = hasher.finish();
        let id = format!(
            "{:08X}-{:04X}-4{:03X}-8{:03X}-{:012X}",
            high >> 32,
            (high >> 16) & 0xffff,
            high & 0xfff,
            (low >> 48) & 0xfff,
            low & 0xffff_ffff_ffff,
        );
        self.ids.insert(device.to_owned(), id.clone());
        self.devices.insert(id.clone(), device.to_owned());
        id
    }

    /// The platform key of the device Sheprd calls `id`.
    pub(crate) fn device(&self, id: &str) -> Option<&str> {
        self.devices.get(id).map(String::as_str)
    }
}

/// A seed no other run shares, so ids differ between runs.
pub(crate) fn random_seed() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.finish()
}
