//! The two key mappings of the pilot, on the original's default keys and listed in Options >
//! Controls under the mod: N turns the teacher on or off, M mutes her voice (the text and the
//! questions stay). Each change is saved to the config and shown briefly over the hotbar.
use super::client::Client;
use super::{Game, files};
use crate::core::Teacher;
use mod_api::{ClientHost, KeyRegistration, Registrar, Text};

pub(crate) const TOGGLE: &str = "toggle";
pub(crate) const MUTE: &str = "mute";

pub(crate) fn register(r: &mut dyn Registrar) {
    r.register_key_mapping(
        KeyRegistration::new(TOGGLE, "key.keyboard.n").label("key.learnordie.toggle"),
    );
    r.register_key_mapping(
        KeyRegistration::new(MUTE, "key.keyboard.m").label("key.learnordie.mute"),
    );
}

/// The clicks since the last tick.
pub(crate) fn handle(
    host: &mut dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    now: i64,
) {
    let mut notice = None;
    for _ in 0..io.clicks(host, TOGGLE) {
        let on = teacher.toggle_teacher(now);
        notice = Some(if on {
            "learnordie.notice.on"
        } else {
            "learnordie.notice.off"
        });
    }
    for _ in 0..io.clicks(host, MUTE) {
        let muted = teacher.toggle_mute(now);
        notice = Some(if muted {
            "learnordie.notice.muted"
        } else {
            "learnordie.notice.unmuted"
        });
    }
    if let Some(key) = notice {
        files::save_config(&*host, io, &teacher.config);
        game.notice = Some((io.tr(&*host, &Text::translate(key, vec![])), now));
    }
}
