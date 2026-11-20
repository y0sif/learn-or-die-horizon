//! The two files the teacher keeps: the config in `config/learnordie.json` (the original's field
//! names and defaults) and the progress of every world in the mod's data folder, one file per
//! language (`progress-de.json`), as data may outgrow the 4 MiB config cap.
//!
//! A file that exists but cannot be read or parsed is set aside as `<name>.invalid-<ms>.bak`
//! before anything new is written to it. The file API has no rename, so setting aside is a copy,
//! and a copy needs the bytes: a file that cannot be read (an I/O error, too large) is held, and
//! every save to it is skipped until a save can read it and copy it aside. So a save never
//! writes the defaults or an empty progress over a file it could not look at.
use super::client::{Client, Store};
use super::lock;
use crate::core::{Config, Progress};
use mod_api::ClientHost;
use std::sync::Mutex;

pub(crate) const CONFIG_FILE: &str = "learnordie.json";

/// The files not set aside yet: no save may write to them.
static HELD: Mutex<Vec<(Store, String)>> = Mutex::new(Vec::new());

pub(crate) fn progress_file(language: &str) -> String {
    format!("progress-{language}.json")
}

fn hold(store: Store, name: &str, held: bool) {
    let mut files = lock(&HELD);
    files.retain(|(s, n)| !(*s == store && n == name));
    if held {
        files.push((store, name.to_string()));
    }
}

/// Copies the file's bytes to its backup name. False when the copy failed.
fn back_up(host: &dyn ClientHost, io: &dyn Client, store: Store, name: &str, bytes: &[u8]) -> bool {
    let backup = format!("{name}.invalid-{}.bak", io.now());
    match io.write(host, store, &backup, bytes) {
        Ok(()) => true,
        Err(e) => {
            super::log_warn(format!("could not set {name} aside as {backup}: {e}"));
            false
        }
    }
}

/// Reads a file and parses it. `None` when it is missing or bad: a bad one is set aside, or held
/// when it cannot be.
fn load<T>(
    host: &dyn ClientHost,
    io: &dyn Client,
    store: Store,
    name: &str,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Option<T> {
    let bytes = match io.read(host, store, name) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            hold(store, name, false);
            return None;
        }
        Err(e) => {
            super::log_warn(format!(
                "could not read {name}, kept as it is and not saved until it can be set aside: {e}"
            ));
            hold(store, name, true);
            return None;
        }
    };
    match parse(&String::from_utf8_lossy(&bytes)) {
        Ok(value) => {
            hold(store, name, false);
            Some(value)
        }
        Err(e) => {
            super::log_warn(format!("invalid {name}, starting afresh: {e}"));
            let aside = back_up(host, io, store, name, &bytes);
            hold(store, name, !aside);
            None
        }
    }
}

/// Writes a file, once a held one could be set aside.
fn save(
    host: &dyn ClientHost,
    io: &dyn Client,
    store: Store,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let held = lock(&HELD).iter().any(|(s, n)| *s == store && n == name);
    if held {
        let aside = match io.read(host, store, name) {
            Ok(Some(old)) => back_up(host, io, store, name, &old),
            Ok(None) => true,
            Err(_) => false,
        };
        if !aside {
            return Ok(());
        }
        hold(store, name, false);
    }
    io.write(host, store, name, bytes)
}

/// The config file, or the defaults (written, unless the file is held) when it is missing or bad.
pub(crate) fn load_config(host: &dyn ClientHost, io: &dyn Client) -> Config {
    if let Some(config) = load(host, io, Store::Config, CONFIG_FILE, Config::parse) {
        return config;
    }
    let config = Config::default();
    save_config(host, io, &config);
    config
}

pub(crate) fn save_config(host: &dyn ClientHost, io: &dyn Client, config: &Config) {
    let text = config.to_json_string();
    if let Err(e) = save(host, io, Store::Config, CONFIG_FILE, text.as_bytes()) {
        super::log_warn(format!("could not write {CONFIG_FILE}: {e}"));
    }
}

/// The progress file of `language`, or an empty progress when it is missing or bad.
pub(crate) fn load_progress(host: &dyn ClientHost, io: &dyn Client, language: &str) -> Progress {
    let name = progress_file(language);
    load(host, io, Store::Data, &name, Progress::parse).unwrap_or_default()
}

/// Writes the progress and marks it saved. A failed or held write is tried again at the next
/// save, not every tick.
pub(crate) fn save_progress(
    host: &dyn ClientHost,
    io: &dyn Client,
    progress: &mut Progress,
    language: &str,
    now: i64,
) {
    let name = progress_file(language);
    let text = progress.to_json_string();
    if let Err(e) = save(host, io, Store::Data, &name, text.as_bytes()) {
        super::log_warn(format!("could not write {name}: {e}"));
    }
    progress.saved(now);
}
