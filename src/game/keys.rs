//! The key mappings, on the original's default keys and listed in Options > Controls under the
//! mod: N turns the teacher on or off, M mutes his voice (the text and the questions stay), K
//! cycles his speech speed, I opens the settings, V has him name what the crosshair is on (even
//! while muted), J opens the dictionary and U the guide. Each change is saved to the config and
//! told in chat, as the original's `actionBar` does (`sendSystemMessage`), in the port's own
//! words.
use super::client::{Client, View};
use super::dictionary::DictionaryScreen;
use super::guide::GuideScreen;
use super::{Game, detect, files, settings};
use crate::core::Teacher;
use mod_api::{ClientHost, KeyRegistration, Registrar, Text};

pub(crate) const TOGGLE: &str = "toggle";
pub(crate) const MUTE: &str = "mute";
pub(crate) const SPEED: &str = "speed";
/// The original's `reload` key, which has opened the settings for a while.
pub(crate) const SETTINGS: &str = "settings";
pub(crate) const WHAT_IS_THIS: &str = "what_is_this";
pub(crate) const DICTIONARY: &str = "dictionary";
pub(crate) const GUIDE: &str = "guide";

/// Each mapping and its default key, in the order Controls lists them.
const KEYS: [(&str, &str); 7] = [
    (TOGGLE, "n"),
    (MUTE, "m"),
    (SPEED, "k"),
    (SETTINGS, "i"),
    (WHAT_IS_THIS, "v"),
    (DICTIONARY, "j"),
    (GUIDE, "u"),
];

pub(crate) fn register(r: &mut dyn Registrar) {
    for (id, key) in KEYS {
        r.register_key_mapping(
            KeyRegistration::new(id, &format!("key.keyboard.{key}"))
                .label(&format!("key.learnordie.{id}")),
        );
    }
}

/// A value's label (`learnordie.value.<value>`), the value itself when the lang has none.
pub(crate) fn value_text(value: &str) -> Text {
    let mut text = Text::translate(format!("learnordie.value.{value}"), vec![]);
    if let mod_api::text::TextContent::Translatable { fallback, .. } = &mut text.content {
        *fallback = Some(value.to_string());
    }
    text
}

/// The clicks since the last tick. I opens the settings' main page, J the dictionary and U the
/// guide (clicks only come with no screen open); V names what the crosshair is on.
pub(crate) fn handle(
    host: &mut dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    view: &View,
    now: i64,
) {
    let mut notice = None;
    for _ in 0..io.clicks(host, TOGGLE) {
        let on = teacher.toggle_teacher(now);
        let key = if on {
            "learnordie.notice.on"
        } else {
            "learnordie.notice.off"
        };
        notice = Some(Text::translate(key, vec![]));
    }
    for _ in 0..io.clicks(host, MUTE) {
        let muted = teacher.toggle_mute(now);
        let key = if muted {
            "learnordie.notice.muted"
        } else {
            "learnordie.notice.unmuted"
        };
        notice = Some(Text::translate(key, vec![]));
    }
    for _ in 0..io.clicks(host, SPEED) {
        let speed = value_text(teacher.cycle_speed());
        notice = Some(Text::translate("learnordie.notice.speed", vec![speed]));
    }
    if let Some(line) = notice {
        files::save_config(&*host, io, &teacher.config);
        io.chat(host, line);
    }
    for _ in 0..io.clicks(host, WHAT_IS_THIS) {
        detect::what_is_this(&*host, io, teacher, now);
    }
    if io.clicks(host, SETTINGS) > 0 && io.open_screen(host) {
        game.settings = Some(settings::Settings::new(settings::Page::Main, &teacher.pack));
    }
    let free = !view.screen_open && !game.panel_open();
    if io.clicks(host, DICTIONARY) > 0 && free && io.open_screen(host) {
        game.dictionary = Some(DictionaryScreen::new(&*host, io, teacher, game.show_all));
    }
    let free = !view.screen_open && !game.panel_open();
    if io.clicks(host, GUIDE) > 0 && free && io.open_screen(host) {
        game.guide = Some(GuideScreen::new(false, false));
    }
}
