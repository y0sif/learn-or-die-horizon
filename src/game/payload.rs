//! The consequences payload: what a quiz answer does on the integrated server, sent by the client
//! half ([`send_consequences`]) and applied by the server half ([`super::server::receive`]).
//!
//! The original reaches into the integrated server in its own process (`server.execute`). Horizon
//! keeps the halves apart, so the client sends the core's [`ServerAction`]s on a typed payload. It
//! is registered `optional()`: a server without the mod still lets the client join, and a send
//! there answers `UnsupportedPeer`.
use super::{lock, log_info, log_warn, server};
use crate::core::{ServerAction, Wipe};
use mod_api::{ClientHost, PayloadCodec, PayloadId, PayloadSendError, Registrar, ToServer};
use std::sync::OnceLock;

pub const CONSEQUENCES_ID: &str = "learnordie:consequences";

/// What one answer does, in order.
#[derive(Clone, Debug, PartialEq, PayloadCodec)]
pub struct Consequences {
    pub actions: Vec<Action>,
}

/// One [`ServerAction`] as it crosses to the server.
#[derive(Clone, Debug, PartialEq, PayloadCodec)]
pub enum Action {
    /// A mob effect (`effect.amplifier()`: the level less one) for `seconds`.
    Effect {
        id: String,
        seconds: i32,
        amplifier: i32,
    },
    /// Five mobs around the player; the seed's bits as an `i64`.
    Mobs {
        hostile: bool,
        seed: i64,
    },
    LostHearts {
        lost: i32,
        modifier: f64,
    },
    Kill,
    /// The angry teacher's wipe: delete everything, or drop it.
    Wipe {
        delete: bool,
    },
}

impl From<&ServerAction> for Action {
    fn from(action: &ServerAction) -> Self {
        match *action {
            ServerAction::Effect(effect) => Action::Effect {
                id: effect.id.to_string(),
                seconds: effect.seconds,
                amplifier: effect.amplifier(),
            },
            ServerAction::Mobs { hostile, seed } => Action::Mobs {
                hostile,
                seed: seed as i64,
            },
            ServerAction::LostHearts { lost, modifier } => Action::LostHearts { lost, modifier },
            ServerAction::Kill => Action::Kill,
            ServerAction::Wipe(wipe) => Action::Wipe {
                delete: wipe == Wipe::Delete,
            },
        }
    }
}

static TO_SERVER: OnceLock<ToServer<Consequences>> = OnceLock::new();

/// From `Hooks::register`: the payload and its server handler, on the server's main thread.
pub(crate) fn register(r: &mut dyn Registrar) {
    let mut payloads = r.payload_handlers();
    let mut view = payloads.registrar("1").optional();
    let token = view.play_to_server_typed::<Consequences>(
        PayloadId(CONSEQUENCES_ID.into()),
        |payload, cx| {
            let player = cx.player().map_err(|e| format!("{e:?}"))?;
            let host = cx.server_host().map_err(|e| format!("{e:?}"))?;
            server::receive(host, player, &payload);
            Ok(())
        },
    );
    let _ = TO_SERVER.set(token);
}

/// Sends an answer's [`Outcome.server`](crate::core::Outcome) actions to the integrated server,
/// for the local player. Nothing is sent for an empty list. Whether the payload was queued; a
/// failure is logged.
pub(crate) fn send_consequences(host: &mut dyn ClientHost, actions: &[ServerAction]) -> bool {
    if actions.is_empty() {
        return false;
    }
    let (Some(token), Some(player)) = (TO_SERVER.get(), host.local_player()) else {
        return false;
    };
    // The server half acts only for this process's own client player.
    *lock(&server::HOST_PLAYER) = Some(player.uuid());
    let payload = Consequences {
        actions: actions.iter().map(Action::from).collect(),
    };
    match token.send(host, &payload) {
        Ok(_) => true,
        Err(PayloadSendError::UnsupportedPeer(_)) => {
            log_info("the server does not have the mod: no rewards or penalties".into());
            false
        }
        Err(error) => {
            log_warn(format!("consequences not sent: {error:?}"));
            false
        }
    }
}
