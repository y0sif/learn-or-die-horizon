//! Loading the language pack after each client resource reload: `phrases.json`, `lipsync.json`
//! and `dictionary.json` under `zhnarrator_de` (the original jar's layer), the sounds checked
//! against the game's sound events.
//!
//! A reload listener can only read the files of its own directory, and the pack's files sit at
//! the namespace root, so the listener only counts reloads; the next client tick reads the files
//! with the host and builds the teacher.
use super::client::Client;
use super::files;
use crate::core::{Dictionary, Pack, Pictures, Teacher};
use mod_api::{
    ClientHost, ClientReloadContext, ClientReloadListener, ClientReloadListeners,
    FileToIdConverter, Identifier, ReloadApply, ReloadResourceError, Text,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// The pack's resource namespace.
pub(crate) const NAMESPACE: &str = "zhnarrator_de";

static RELOADS: AtomicU64 = AtomicU64::new(0);

struct ReloadCounter;

impl ClientReloadListener for ReloadCounter {
    fn prepare(
        &self,
        _context: &ClientReloadContext,
    ) -> Result<Box<dyn ReloadApply>, ReloadResourceError> {
        Ok(<dyn ReloadApply>::new(|| {
            RELOADS.fetch_add(1, Ordering::Relaxed);
        }))
    }
}

pub(crate) fn add_listener(listeners: &mut dyn ClientReloadListeners) {
    // A directory no pack has: the listener reads nothing, it only hears the reload.
    let (Ok(id), Ok(converter)) = (
        Identifier::parse("learnordie:pack"),
        FileToIdConverter::json("learnordie_reload"),
    ) else {
        return;
    };
    if let Err(e) = listeners.add_listener(id, converter, Arc::new(ReloadCounter)) {
        super::log_warn(format!(
            "could not register the pack reload listener: {e:?}"
        ));
    }
}

/// How many client resource reloads have finished.
pub(crate) fn generation() -> u64 {
    RELOADS.load(Ordering::Relaxed)
}

/// The game language (`de_de`), from the vanilla lang key that names it.
fn game_language(host: &dyn ClientHost, io: &dyn Client) -> String {
    let code = io.tr(host, &Text::translate("language.code", vec![]));
    if code.is_empty() || code == "language.code" {
        "en_us".to_string()
    } else {
        code
    }
}

fn text(host: &dyn ClientHost, io: &dyn Client, name: &str) -> Result<Option<String>, String> {
    let Some(bytes) = io.resource(host, &format!("{NAMESPACE}:{name}")) else {
        return Ok(None);
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|e| format!("{NAMESPACE}:{name}: {e}"))
}

/// The pack's `dictionary.json` (Java `DictionaryScreen.load`): empty when the pack has none, and
/// when it cannot be read, which the game's log says.
fn dictionary(host: &dyn ClientHost, io: &dyn Client) -> Dictionary {
    let read = text(host, io, "dictionary.json")
        .and_then(|text| text.map(|text| Dictionary::parse(&text)).transpose());
    match read {
        Ok(Some(dictionary)) => {
            super::log_info(format!("{NAMESPACE}: dictionary.json read"));
            dictionary
        }
        Ok(None) => Dictionary::default(),
        Err(e) => {
            super::log_warn(format!(
                "dictionary.json of {NAMESPACE} could not be read: {e}"
            ));
            Dictionary::default()
        }
    }
}

/// What a reload builds: the teacher, the item registry indexed for the pictures, and the
/// translation language the pack was read with (the dictionary's hints need it).
pub(crate) struct Loaded {
    pub(crate) teacher: Teacher,
    pub(crate) pictures: Pictures,
    pub(crate) language: String,
}

/// The teacher for the pack in the resources now. A teacher already running hands over its config
/// and progress (it keeps them when the pack fails to load); otherwise both are read from their
/// files.
pub(crate) fn load(
    host: &dyn ClientHost,
    io: &dyn Client,
    previous: Option<&mut Teacher>,
    now: i64,
) -> Result<Loaded, String> {
    let phrases = text(host, io, "phrases.json")?.ok_or_else(|| {
        format!("no {NAMESPACE}:phrases.json in the resources (is the original jar referenced?)")
    })?;
    let lipsync = text(host, io, "lipsync.json")?;
    let config = match &previous {
        Some(teacher) => teacher.config.clone(),
        None => files::load_config(host, io),
    };
    let hints = config.translation_language(&game_language(host, io));
    let mut pack = Pack::parse(NAMESPACE, &phrases, lipsync.as_deref(), &hints)?;
    let events = io.sound_events(host);
    let (resolved, total) = pack.check_sounds(|event| events.contains(event));
    super::log_info(format!(
        "{NAMESPACE}: {} phrases loaded, {resolved} of {total} sound events resolved",
        pack.phrases().len()
    ));
    let progress = match previous {
        Some(teacher) => std::mem::take(&mut teacher.progress),
        None => files::load_progress(host, io, &pack.meta.language),
    };
    let pictures = pack.pictures(io.item_ids(host));
    let seed = (now as u64) ^ 0x9E37_79B9_7F4A_7C15;
    let mut teacher = Teacher::new(pack, config, progress, seed);
    teacher.dictionary = dictionary(host, io);
    Ok(Loaded {
        teacher,
        pictures,
        language: hints,
    })
}
