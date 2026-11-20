//! The world key the progress is filed under, as the original makes it: the save folder and the
//! overworld seed in single player, the server address elsewhere. The seed is server-only, so the
//! integrated server's half reads it when it starts; both halves run in the one process.
use super::client::View;
use super::lock;
use crate::core::{server_world_key, singleplayer_world_key};
use mod_api::ServerHost;
use std::sync::Mutex;

/// How long a single-player join waits for the seed before filing under seed 0.
const SEED_WAIT_MS: i64 = 5000;

/// The integrated server's overworld seed, while one runs.
pub(super) static SEED: Mutex<Option<i64>> = Mutex::new(None);

/// The integrated server started: keep its overworld seed for the client half.
pub(crate) fn server_started(host: &mut dyn ServerHost, dedicated: bool) {
    if dedicated {
        return;
    }
    let seed = host
        .level("minecraft:overworld")
        .and_then(|level| level.seed(&*host));
    *lock(&SEED) = seed;
}

pub(crate) fn server_stopped() {
    *lock(&SEED) = None;
}

/// The key for the world in view, or `None` while a single-player join still waits for its seed.
/// `waiting_since` is when the wait began.
pub(crate) fn key(view: &View, now: i64, waiting_since: i64) -> Option<String> {
    let folder = view.save_folder.as_deref().filter(|_| view.singleplayer);
    let Some(folder) = folder else {
        return Some(server_world_key(view.address.as_deref()));
    };
    match *lock(&SEED) {
        Some(seed) => Some(singleplayer_world_key(folder, seed)),
        None if now - waiting_since >= SEED_WAIT_MS => {
            super::log_warn(
                "no seed from the integrated server; filing progress under seed 0".into(),
            );
            Some(singleplayer_world_key(folder, 0))
        }
        None => None,
    }
}
