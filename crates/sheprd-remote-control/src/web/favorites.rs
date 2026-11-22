//! This computer's favorite agents (`control::favorites`) as the daemon
//! holds them, and `POST /api/v2/favorites`, a phone's way to star or
//! unstar one. Home marks each favorite row, so a change reaches every
//! phone connected.

use super::{ApiError, Device, Shared};
use crate::{
    app,
    control::favorites::{Change, Favorites},
};
use axum::{Json, extract::State};
use std::{
    path::PathBuf,
    sync::{Mutex, PoisonError},
};
use tokio::sync::watch;

pub(crate) struct Kept {
    file: PathBuf,
    current: watch::Sender<Favorites>,
    /// Held across a whole change, so two cannot both write the partial
    /// file or finish in the wrong order.
    changing: Mutex<()>,
}

impl Kept {
    /// The favorites saved in `file`. Blocking.
    pub(crate) fn load(file: PathBuf) -> Self {
        let current = watch::Sender::new(app::favorites::load(&file));
        Self {
            file,
            current,
            changing: Mutex::default(),
        }
    }

    pub(crate) fn current(&self) -> Favorites {
        self.current.borrow().clone()
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<Favorites> {
        self.current.subscribe()
    }

    /// Makes `change` and returns what stands. Blocking: a change is saved
    /// before anyone hears of it, and one that cannot be saved is refused.
    /// The file counts too, as a change that found no socket while this
    /// daemon started wrote there after it was read.
    pub(crate) fn change(&self, change: Change) -> app::Result<Favorites> {
        self.update(|saved| app::favorites::changed(saved, change))
    }

    /// Follows the primary Herdr 1 session's `boot`
    /// (`Favorites::follow_boot`). Blocking.
    pub(crate) fn follow_boot(&self, boot: &str) -> app::Result<Favorites> {
        self.update(|mut saved| {
            saved.follow_boot(boot);
            Ok(saved)
        })
    }

    fn update(
        &self,
        work: impl FnOnce(Favorites) -> app::Result<Favorites>,
    ) -> app::Result<Favorites> {
        let _changing = self.changing.lock().unwrap_or_else(PoisonError::into_inner);
        app::edited::locked(&self.file, || {
            let saved = app::favorites::load(&self.file);
            let next = work(saved.clone())?;
            if next != saved {
                app::edited::save(&self.file, &next)?;
            }
            self.current.send_if_modified(|current| {
                let modified = *current != next;
                if modified {
                    current.clone_from(&next);
                }
                modified
            });
            Ok(next)
        })
    }
}

/// Watches the primary Herdr 1 session and forgets its panes' stars when
/// it restarts, as its pane ids start over.
pub(crate) async fn follow_boot(state: Shared) {
    let mut primary = state.hub.watch_primary();
    loop {
        let boot = {
            let layout = primary.borrow_and_update();
            (layout.connected && !layout.boot.is_empty()).then(|| layout.boot.clone())
        };
        if let Some(boot) = boot
            && state.favorites.current().boot.as_deref() != Some(boot.as_str())
        {
            let state = state.clone();
            let followed =
                tokio::task::spawn_blocking(move || state.favorites.follow_boot(&boot)).await;
            if let Ok(Err(error)) = followed {
                tracing::warn!(%error, "could not save favorites after Herdr restarted");
            }
        }
        if primary.changed().await.is_err() {
            return;
        }
    }
}

pub(super) async fn post(
    State(state): State<Shared>,
    _device: Device,
    Json(change): Json<Change>,
) -> Result<Json<Favorites>, ApiError> {
    let favorites = tokio::task::spawn_blocking(move || state.favorites.change(change))
        .await
        .map_err(|_| ApiError::Invalid("the request was interrupted"))??;
    Ok(Json(favorites))
}
