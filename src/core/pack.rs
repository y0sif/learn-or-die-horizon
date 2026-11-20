//! The language pack: `phrases.json` (meta, voices, speeds, the word list and the phrases) and
//! `lipsync.json`, parsed into typed values, with the lookups the teacher makes on them.
//!
//! Invariants: phrase ids are unique, and a repeated id replaces the earlier phrase in place.
//! Words keep file order, so the first word of a kind whose glob matches a registry path wins.
//! Lookups take registry ids such as `minecraft:oak_log` and match only the path after the colon.
use super::gson::{self, JsonObject, field};
use mod_api::serde_json::{self, Value};
use std::collections::{HashMap, HashSet};

/// The path of a registry id: `minecraft:oak_log` gives `oak_log`; an id with no namespace is its
/// own path.
pub(crate) fn path_of(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, path)| path)
}

/// One word of a phrase, with its grammatical role (`pron`, `verb`, `obj`, `adj`, `num`, `part`).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub text: String,
    pub reading: String,
    pub role: String,
}

/// One recorded sentence or word. The id is `<action>.<key>` for phrases about a thing, or a fixed
/// name such as `jump`. `en` is the translation shown under the text, already replaced by the
/// hint language when the pack has one.
#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub id: String,
    pub action: String,
    pub key: Option<String>,
    pub text: String,
    pub reading: String,
    pub en: String,
    pub parts: Vec<Part>,
}

impl Phrase {
    /// Never quizzed: bare words, nouns, plurals and the counted pieces.
    pub fn is_fragment(&self) -> bool {
        matches!(self.action.as_str(), "word" | "noun" | "plural") || self.action.ends_with("count")
    }

    /// Movement states wait before they are spoken.
    pub fn is_movement(&self) -> bool {
        matches!(
            self.action.as_str(),
            "walk" | "sprint" | "sneak" | "swim" | "fly" | "climb"
        )
    }

    /// The clip name inside the pack: `<voice>.<speed>.<id>`. It is both the sound event path and
    /// the lipsync key.
    pub fn sound_path(&self, voice: &str, speed: &str) -> String {
        format!("{voice}.{speed}.{}", self.id)
    }

    /// The text without its sentence end (`.!?` and the CJK marks) or a leading `¡¿`.
    pub fn bare(&self) -> &str {
        bare(&self.text)
    }
}

/// `Phrase.bare`: a text without its sentence end or a leading `¡¿`.
pub(crate) fn bare(text: &str) -> &str {
    text.trim_end_matches(|c: char| "。！？.!?".contains(c))
        .trim_start_matches(|c: char| "¡¿".contains(c))
}

/// A thing in the world and the registry paths it covers. Patterns are globs where `*` matches
/// any run of characters, including none.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub key: String,
    /// `block`, `item` or `entity`.
    pub kind: String,
    pub patterns: Vec<String>,
    /// The grammatical gender for German (`m`, `f`, `n`); `ge` when the pack gives none.
    pub measure: String,
    pub count_en: Option<String>,
}

impl Word {
    pub fn matches(&self, path: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| glob_matches(pattern, path))
    }
}

/// A glob with `*` as the only wildcard, anchored at both ends.
fn glob_matches(glob: &str, path: &str) -> bool {
    let mut pieces = glob.split('*');
    let first = pieces.next().unwrap_or("");
    let Some(mut rest) = path.strip_prefix(first) else {
        return false;
    };
    let later: Vec<&str> = pieces.collect();
    let Some((last, middle)) = later.split_last() else {
        return rest.is_empty();
    };
    for piece in middle {
        match rest.find(*piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    rest.ends_with(*last)
}

/// The item registry indexed for [`Pack::picture`] by [`Pack::pictures`], so opening a question
/// matches no globs.
#[derive(Clone, Debug, Default)]
pub struct Pictures {
    /// Item ids in registry order.
    items: Vec<String>,
    /// Each word key's first item (its registry index) whose path maps to the key, as an item or
    /// as a block.
    by_key: HashMap<String, usize>,
    /// Each registry path's first item (its registry index).
    by_path: HashMap<String, usize>,
}

/// The teacher's sprite sheet: frames laid out side by side.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    /// The texture is `<namespace>:textures/gui/<file>.png`.
    pub file: String,
    pub frame_width: i32,
    pub frame_height: i32,
    pub frames: i32,
    pub screen_height: f32,
    pub waist: i32,
    pub hop: f32,
    pub mouth_small: Vec<i32>,
    pub mouth_wide: Vec<i32>,
}

impl Sprite {
    fn read(file: &str, portrait: &JsonObject) -> Sprite {
        let int = |name: &str, fallback: i32| {
            field(portrait, name)
                .and_then(|value| gson::as_i32(value).ok())
                .unwrap_or(fallback)
        };
        let float = |name: &str, fallback: f32| {
            field(portrait, name)
                .and_then(|value| gson::as_f32(value).ok())
                .unwrap_or(fallback)
        };
        let ints = |name: &str| -> Vec<i32> {
            field(portrait, name)
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(|v| gson::as_i32(v).ok()).collect())
                .unwrap_or_default()
        };
        let frame_height = int("frameHeight", 56);
        Sprite {
            file: file.to_string(),
            frame_width: int("frameWidth", 48),
            frame_height,
            frames: int("frames", 8),
            screen_height: float("screenHeight", 56.0),
            waist: int("waist", frame_height - int("padBottom", 0)),
            hop: float("hop", 6.0),
            mouth_small: ints("mouthSmall"),
            mouth_wide: ints("mouthWide"),
        }
    }
}

/// The pack's `meta` block, with the original's defaults for missing keys.
#[derive(Clone, Debug, PartialEq)]
pub struct Meta {
    /// The resource namespace, such as `zhnarrator_de`.
    pub namespace: String,
    /// The namespace without its `zhnarrator_` prefix, such as `de`.
    pub language: String,
    pub name: String,
    pub native_name: String,
    pub font: String,
    pub word_separator: String,
    pub has_reading: bool,
    pub ui_correct: String,
    pub ui_wrong: String,
    /// The languages whose hints the pack has, for the translation setting.
    pub hint_languages: Vec<String>,
    /// `countNoun: "hold"`: a counted sentence with no plural uses the held-item noun.
    pub count_noun_is_hold: bool,
    /// `countOrder: "head"`: a counted pickup or drop says the noun (`head.<key>`) first.
    pub count_head_first: bool,
    /// The first portrait the pack lists.
    pub sprite: Sprite,
}

impl Meta {
    fn read(namespace: &str, meta: &JsonObject) -> Meta {
        let language = namespace
            .strip_prefix("zhnarrator_")
            .unwrap_or(namespace)
            .to_string();
        let text = |object: &JsonObject, name: &str, fallback: &str| {
            field(object, name)
                .and_then(|value| gson::as_string(value).ok())
                .unwrap_or_else(|| fallback.to_string())
        };
        let empty = JsonObject::new();
        let ui = field(meta, "ui")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let listed = field(meta, "portraits")
            .and_then(Value::as_array)
            .and_then(|list| list.first())
            .and_then(Value::as_object);
        let sprite = match listed {
            Some(portrait) => Sprite::read(&text(portrait, "file", "teacher"), portrait),
            None => Sprite::read(
                "teacher",
                field(meta, "portrait")
                    .and_then(Value::as_object)
                    .unwrap_or(&empty),
            ),
        };
        Meta {
            name: text(meta, "name", language.as_str()),
            native_name: text(meta, "nativeName", language.as_str()),
            font: text(meta, "font", "minecraft:default"),
            word_separator: text(meta, "wordSeparator", " "),
            has_reading: field(meta, "hasReading")
                .is_some_and(|value| gson::as_bool(value).unwrap_or(false)),
            ui_correct: text(ui, "correct", "\u{2713}"),
            ui_wrong: text(ui, "wrong", "\u{2717}"),
            hint_languages: field(meta, "hintLanguages")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|v| gson::as_string(v).ok())
                        .collect()
                })
                .unwrap_or_default(),
            count_noun_is_hold: text(meta, "countNoun", "") == "hold",
            count_head_first: text(meta, "countOrder", "") == "head",
            namespace: namespace.to_string(),
            language,
            sprite,
        }
    }
}

/// `lipsync.json`: one digit per window of a clip (`0` closed, `1` small, `2` wide), keyed by the
/// clip name `<voice>.<speed>.<id>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Lipsync {
    window_ms: i64,
    clips: HashMap<String, Vec<u8>>,
}

impl Default for Lipsync {
    fn default() -> Self {
        Self {
            window_ms: 40,
            clips: HashMap::new(),
        }
    }
}

impl Lipsync {
    pub fn parse(text: &str) -> Result<Lipsync, String> {
        let root: Value = serde_json::from_str(text).map_err(|e| format!("lipsync.json: {e}"))?;
        let root = root.as_object().ok_or("lipsync.json is not an object")?;
        let window_ms = match field(root, "window") {
            Some(value) => gson::as_i64(value)?,
            None => 40,
        };
        let clips = field(root, "clips")
            .and_then(Value::as_object)
            .ok_or("lipsync.json has no clips")?
            .iter()
            .map(|(key, value)| -> Result<(String, Vec<u8>), String> {
                Ok((key.clone(), gson::as_string(value)?.into_bytes()))
            })
            .collect::<Result<HashMap<_, _>, String>>()?;
        Ok(Lipsync {
            window_ms: window_ms.max(1),
            clips,
        })
    }

    /// The clip's length: its digit count times the window. `None` when the pack has no track.
    pub fn length_ms(&self, clip: &str) -> Option<i64> {
        self.clips
            .get(clip)
            .map(|levels| levels.len() as i64 * self.window_ms)
    }

    /// The mouth level `ms` into the clip: 0, 1 or 2, and 0 outside the track. -1 when the pack has
    /// no track for the clip.
    pub fn mouth(&self, clip: &str, ms: i64) -> i32 {
        let Some(levels) = self.clips.get(clip) else {
            return -1;
        };
        match usize::try_from(ms / self.window_ms)
            .ok()
            .and_then(|index| levels.get(index))
        {
            Some(&level) => i32::from(level) - i32::from(b'0'),
            None => 0,
        }
    }

    /// Where the speech ends: the track up to its last non-zero digit. -1 with no track.
    pub fn spoken_ms(&self, clip: &str) -> i64 {
        let Some(levels) = self.clips.get(clip) else {
            return -1;
        };
        let spoken = levels
            .iter()
            .rposition(|&level| level != b'0')
            .map_or(0, |last| last + 1);
        spoken as i64 * self.window_ms
    }
}

/// A parsed language pack.
#[derive(Clone, Debug)]
pub struct Pack {
    pub meta: Meta,
    pub voices: Vec<String>,
    pub speeds: Vec<String>,
    pub lipsync: Lipsync,
    words: Vec<Word>,
    phrases: Vec<Phrase>,
    by_id: HashMap<String, usize>,
    missing_sounds: HashSet<String>,
}

impl Pack {
    /// Parses `phrases.json` and, when the pack has one, `lipsync.json`. `hint_language` is the
    /// translation language from [`super::Config::translation_language`]: a phrase with a hint in
    /// it shows that hint instead of `en`.
    pub fn parse(
        namespace: &str,
        phrases_json: &str,
        lipsync_json: Option<&str>,
        hint_language: &str,
    ) -> Result<Pack, String> {
        let root: Value =
            serde_json::from_str(phrases_json).map_err(|e| format!("phrases.json: {e}"))?;
        let root = root.as_object().ok_or("phrases.json is not an object")?;
        let empty = JsonObject::new();
        let meta = Meta::read(
            namespace,
            field(root, "meta")
                .and_then(Value::as_object)
                .unwrap_or(&empty),
        );
        let voices = list(root, "voices")?
            .iter()
            .map(gson::as_string)
            .collect::<Result<Vec<_>, _>>()?;
        let speeds = list(root, "speeds")?
            .iter()
            .map(gson::as_string)
            .collect::<Result<Vec<_>, _>>()?;
        let words = list(root, "words")?
            .iter()
            .map(word)
            .collect::<Result<Vec<_>, _>>()?;
        let mut phrases: Vec<Phrase> = Vec::new();
        let mut by_id = HashMap::new();
        for value in list(root, "phrases")? {
            let parsed = phrase(value, hint_language)?;
            let existing = by_id.get(&parsed.id).copied();
            match existing {
                Some(index) => phrases[index] = parsed,
                None => {
                    by_id.insert(parsed.id.clone(), phrases.len());
                    phrases.push(parsed);
                }
            }
        }
        let lipsync = match lipsync_json {
            Some(text) => Lipsync::parse(text)?,
            None => Lipsync::default(),
        };
        Ok(Pack {
            meta,
            voices,
            speeds,
            lipsync,
            words,
            phrases,
            by_id,
            missing_sounds: HashSet::new(),
        })
    }

    /// Every phrase, in file order.
    pub fn phrases(&self) -> &[Phrase] {
        &self.phrases
    }

    /// Every word, in file order.
    pub fn words(&self) -> &[Word] {
        &self.words
    }

    pub fn phrase(&self, id: &str) -> Option<&Phrase> {
        self.by_id.get(id).map(|&index| &self.phrases[index])
    }

    pub fn by_action<'a>(&'a self, action: &'a str) -> impl Iterator<Item = &'a Phrase> + 'a {
        self.phrases.iter().filter(move |p| p.action == action)
    }

    /// The key of the first word of `kind` whose glob matches the id's path.
    pub fn key_for(&self, kind: &str, id: &str) -> Option<&str> {
        let path = path_of(id);
        self.words
            .iter()
            .find(|word| word.kind == kind && word.matches(path))
            .map(|word| word.key.as_str())
    }

    fn for_kind(&self, kind: &str, action: &str, id: &str) -> Option<&Phrase> {
        let key = self.key_for(kind, id)?;
        self.phrase(&format!("{action}.{key}"))
    }

    pub fn for_block(&self, action: &str, id: &str) -> Option<&Phrase> {
        self.for_kind("block", action, id)
    }

    /// An item's phrase, else the phrase of the block with the same path (block items).
    pub fn for_item(&self, action: &str, id: &str) -> Option<&Phrase> {
        self.for_kind("item", action, id)
            .or_else(|| self.for_block(action, id))
    }

    pub fn for_entity(&self, action: &str, id: &str) -> Option<&Phrase> {
        self.for_kind("entity", action, id)
    }

    /// The phrase a right click on the block says instead of placing: `close` when the block is
    /// open and has one, else `open`, else `use`. A click with one of these places nothing.
    pub fn use_phrase(&self, block: &str, is_open: bool) -> Option<&Phrase> {
        if is_open && let Some(close) = self.for_block("close", block) {
            return Some(close);
        }
        self.for_block("open", block)
            .or_else(|| self.for_block("use", block))
    }

    /// The grammatical gender of a word key: the last word with the key decides, `ge` when it
    /// gives none.
    pub fn measure_of(&self, key: &str) -> &str {
        self.words
            .iter()
            .rev()
            .find(|word| word.key == key)
            .map_or("ge", |word| word.measure.as_str())
    }

    /// The English plural of a word key for counted sentences (`countEn`), if the pack has one.
    pub fn count_en(&self, key: &str) -> Option<&str> {
        self.words
            .iter()
            .rev()
            .filter(|word| word.key == key)
            .find_map(|word| word.count_en.as_deref())
    }

    /// The start of a counted sentence, such as "I pick up fourteen": `count.<action>.<n>.<measure>`,
    /// else `count.<action>.<n>`.
    pub fn count_start(&self, action: &str, n: i32, measure: &str) -> Option<&Phrase> {
        self.phrase(&format!("count.{action}.{n}.{measure}"))
            .or_else(|| self.phrase(&format!("count.{action}.{n}")))
    }

    /// The counted noun: `plural.<key>.<action>`, else `plural.<key>`, else (with
    /// `countNoun: "hold"`) `hold.<key>`.
    pub fn count_noun(&self, action: &str, key: &str) -> Option<&Phrase> {
        self.phrase(&format!("plural.{key}.{action}"))
            .or_else(|| self.phrase(&format!("plural.{key}")))
            .or_else(|| {
                self.meta
                    .count_noun_is_hold
                    .then(|| self.phrase(&format!("hold.{key}")))
                    .flatten()
            })
    }

    /// The amount on its own, such as "fourteen pieces.": `count.<n>.<measure>`.
    pub fn count_unit(&self, n: i32, measure: &str) -> Option<&Phrase> {
        self.phrase(&format!("count.{n}.{measure}"))
    }

    /// The raw patterns of the last word with this kind and key.
    fn patterns_of(&self, kind: &str, key: &str) -> &[String] {
        self.words
            .iter()
            .rev()
            .find(|word| word.kind == kind && word.key == key)
            .map(|word| word.patterns.as_slice())
            .unwrap_or_default()
    }

    /// Indexes the item registry (`items`, in registry order) for [`Pack::picture`]. This is the
    /// one pass that matches every item against the word globs, once per pack and registry.
    pub fn pictures(&self, items: Vec<String>) -> Pictures {
        let mut by_key = HashMap::new();
        let mut by_path = HashMap::new();
        for (index, item) in items.iter().enumerate() {
            for kind in ["item", "block"] {
                if let Some(key) = self.key_for(kind, item) {
                    by_key.entry(key.to_string()).or_insert(index);
                }
            }
            by_path.entry(path_of(item).to_string()).or_insert(index);
        }
        Pictures {
            items,
            by_key,
            by_path,
        }
    }

    /// The item to draw for a word key, from the registry `pictures` indexes. First the first
    /// item in registry order whose path maps to the key as an item or a block, then an item
    /// named by one of the key's exact patterns, then the spawn egg of the entity.
    pub fn picture<'a>(&self, key: &str, pictures: &'a Pictures) -> Option<&'a str> {
        let item = |index: usize| pictures.items.get(index).map(String::as_str);
        if let Some(&index) = pictures.by_key.get(key) {
            return item(index);
        }
        for kind in ["item", "block"] {
            for pattern in self.patterns_of(kind, key) {
                if pattern.contains('*') {
                    continue;
                }
                if let Some(&index) = pictures.by_path.get(pattern.as_str()) {
                    return item(index);
                }
            }
        }
        let entity = if key.ends_with("_sheep") {
            "sheep"
        } else {
            key
        };
        let exact = self
            .patterns_of("entity", key)
            .iter()
            .map(String::as_str)
            .filter(|pattern| !pattern.contains('*'));
        std::iter::once(entity)
            .chain(exact)
            .filter_map(|name| pictures.by_path.get(&format!("{name}_spawn_egg")).copied())
            .min()
            .and_then(item)
    }

    /// The configured voice when the pack has it, else the pack's first.
    pub fn voice<'a>(&'a self, wanted: &'a str) -> &'a str {
        pick(wanted, &self.voices)
    }

    /// The configured speed when the pack has it, else the pack's first.
    pub fn speed<'a>(&'a self, wanted: &'a str) -> &'a str {
        pick(wanted, &self.speeds)
    }

    /// The sound event id of a phrase, as the pack's `sounds.json` names it:
    /// `<namespace>:<voice>.<speed>.<id>`.
    pub fn sound_event(&self, phrase_id: &str, voice: &str, speed: &str) -> String {
        format!("{}:{voice}.{speed}.{phrase_id}", self.meta.namespace)
    }

    /// Every sound event the pack should have: each phrase at each voice and speed.
    pub fn sound_events(&self) -> Vec<String> {
        let mut out =
            Vec::with_capacity(self.voices.len() * self.speeds.len() * self.phrases.len());
        for voice in &self.voices {
            for speed in &self.speeds {
                for phrase in &self.phrases {
                    out.push(self.sound_event(&phrase.id, voice, speed));
                }
            }
        }
        out
    }

    /// Checks every sound event against the game's. Missing ones are shown as text only. Returns
    /// `(resolved, total)`.
    pub fn check_sounds(&mut self, exists: impl Fn(&str) -> bool) -> (usize, usize) {
        let events = self.sound_events();
        let total = events.len();
        self.missing_sounds = events
            .into_iter()
            .filter(|event| !exists(event.as_str()))
            .collect();
        (total - self.missing_sounds.len(), total)
    }

    pub fn has_sound(&self, event: &str) -> bool {
        !self.missing_sounds.contains(event)
    }
}

fn pick<'a>(wanted: &'a str, available: &'a [String]) -> &'a str {
    match available.first() {
        Some(first) if !available.iter().any(|name| name == wanted) => first.as_str(),
        _ => wanted,
    }
}

fn list<'a>(root: &'a JsonObject, name: &str) -> Result<&'a Vec<Value>, String> {
    field(root, name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("phrases.json has no {name} list"))
}

fn required(object: &JsonObject, name: &str) -> Result<String, String> {
    gson::as_string(field(object, name).ok_or_else(|| format!("{name} is missing"))?)
}

fn optional(object: &JsonObject, name: &str) -> Option<String> {
    field(object, name).and_then(|value| gson::as_string(value).ok())
}

fn word(value: &Value) -> Result<Word, String> {
    let object = value.as_object().ok_or("a word is not an object")?;
    let patterns = field(object, "patterns")
        .and_then(Value::as_array)
        .ok_or("a word has no patterns")?
        .iter()
        .map(gson::as_string)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Word {
        key: required(object, "key")?,
        kind: required(object, "kind")?,
        patterns,
        measure: optional(object, "measure").unwrap_or_else(|| "ge".to_string()),
        count_en: optional(object, "countEn"),
    })
}

fn phrase(value: &Value, hint_language: &str) -> Result<Phrase, String> {
    let object = value.as_object().ok_or("a phrase is not an object")?;
    let id = required(object, "id")?;
    let hint = field(object, "hints")
        .and_then(Value::as_object)
        .and_then(|hints| field(hints, hint_language));
    let en = match hint {
        Some(hint) => gson::as_string(hint)?,
        None => required(object, "en").map_err(|e| format!("{id}: {e}"))?,
    };
    let mut parts = Vec::new();
    if let Some(list) = field(object, "parts").and_then(Value::as_array) {
        for part in list {
            let part = part
                .as_object()
                .ok_or_else(|| format!("{id}: a part is not an object"))?;
            parts.push(Part {
                text: required(part, "text")?,
                reading: optional(part, "reading").unwrap_or_default(),
                role: required(part, "role")?,
            });
        }
    }
    Ok(Phrase {
        action: required(object, "action").map_err(|e| format!("{id}: {e}"))?,
        key: optional(object, "key"),
        text: required(object, "text").map_err(|e| format!("{id}: {e}"))?,
        reading: optional(object, "reading").unwrap_or_default(),
        en,
        parts,
        id,
    })
}
