//! The two files the teacher keeps: the config in `config/learnordie.json` (the original's field
//! names and defaults) and the progress of every world in the mod's data folder, one file per
//! language (`progress-de.json`), as data may outgrow the 4 MiB config cap. A file that cannot be
//! read is set aside as `<name>.invalid-<ms>.bak` before the defaults replace it.
use super::client::{Client, Store};
use crate::core::{Config, Progress};
use mod_api::ClientHost;

pub(crate) const CONFIG_FILE: &str = "learnordie.json";

pub(crate) fn progress_file(language: &str) -> String {
    format!("progress-{language}.json")
}

/// Reads a file's text; `Ok(None)` when there is none.
fn read_text(
    host: &dyn ClientHost,
    io: &dyn Client,
    store: Store,
    name: &str,
) -> Result<Option<(String, Vec<u8>)>, String> {
    let Some(bytes) = io.read(host, store, name)? else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(Some((text, bytes)))
}

fn back_up(host: &dyn ClientHost, io: &dyn Client, store: Store, name: &str, bytes: &[u8]) {
    let backup = format!("{name}.invalid-{}.bak", io.now());
    if let Err(e) = io.write(host, store, &backup, bytes) {
        super::log_warn(format!("could not set {name} aside as {backup}: {e}"));
    }
}

/// The config file, or the defaults (written) when it is missing or unreadable.
pub(crate) fn load_config(host: &dyn ClientHost, io: &dyn Client) -> Config {
    match read_text(host, io, Store::Config, CONFIG_FILE) {
        Ok(Some((text, bytes))) => match Config::parse(&text) {
            Ok(config) => return config,
            Err(e) => {
                super::log_warn(format!("invalid {CONFIG_FILE}, using the defaults: {e}"));
                back_up(host, io, Store::Config, CONFIG_FILE, &bytes);
            }
        },
        Ok(None) => {}
        Err(e) => {
            super::log_warn(format!("could not read {CONFIG_FILE}: {e}"));
            return Config::default();
        }
    }
    let config = Config::default();
    save_config(host, io, &config);
    config
}

pub(crate) fn save_config(host: &dyn ClientHost, io: &dyn Client, config: &Config) {
    let text = config.to_json_string();
    if let Err(e) = io.write(host, Store::Config, CONFIG_FILE, text.as_bytes()) {
        super::log_warn(format!("could not write {CONFIG_FILE}: {e}"));
    }
}

/// The progress file of `language`, or an empty progress when it is missing or unreadable.
pub(crate) fn load_progress(host: &dyn ClientHost, io: &dyn Client, language: &str) -> Progress {
    let name = progress_file(language);
    match read_text(host, io, Store::Data, &name) {
        Ok(Some((text, bytes))) => Progress::parse(&text).unwrap_or_else(|e| {
            super::log_warn(format!("invalid {name}, starting afresh: {e}"));
            back_up(host, io, Store::Data, &name, &bytes);
            Progress::default()
        }),
        Ok(None) => Progress::default(),
        Err(e) => {
            super::log_warn(format!("could not read {name}: {e}"));
            Progress::default()
        }
    }
}

/// Writes the progress and marks it saved. A failed write is logged and tried again at the next
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
    if let Err(e) = io.write(host, Store::Data, &name, text.as_bytes()) {
        super::log_warn(format!("could not write {name}: {e}"));
    }
    progress.saved(now);
}
