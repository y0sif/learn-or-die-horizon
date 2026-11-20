//! The dictionary (the model of Java `DictionaryScreen`): the pack's `dictionary.json`, the learned
//! phrases it lists in its two tabs, its search, and what its detail pane shows for a phrase.
//! Text the game translates comes out as lang keys.
//!
//! `dictionary.json` is `{"entries": {"<phrase id>": {"h": [one hint code per word], "n": [[note
//! type, args...]], "f": [[text, reading, form type]]}}}`.
//!
//! Learned: a phrase with an entry and at least `learnedAfterCorrect` (1 or more) right answers
//! across all worlds, newest right answer first; the "this world" tab also needs it learned in
//! this world, and goes by this world's last right answer.
use super::config::Config;
use super::gson::{self, field};
use super::pack::{self, Pack, Part, Phrase};
use super::progress::Progress;
use mod_api::serde_json::{self, Value};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Default, PartialEq)]
struct Entry {
    hints: Vec<String>,
    notes: Vec<Vec<String>>,
    forms: Vec<Vec<String>>,
}

/// The pack's `dictionary.json`.
#[derive(Clone, Debug, Default)]
pub struct Dictionary {
    entries: HashMap<String, Entry>,
}

/// The learned phrases of the two tabs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Learned<'a> {
    /// The "all worlds" tab, by the last right answer anywhere, newest first.
    pub all: Vec<&'a Phrase>,
    /// The "this world" tab, by this world's last right answer, newest first.
    pub this_world: Vec<&'a Phrase>,
}

/// A piece of text in the detail pane.
#[derive(Clone, Debug, PartialEq)]
pub enum Label {
    /// A lang key to translate.
    Key(String),
    /// Shown as it is.
    Text(String),
    /// A word's hint only: the thing the phrase is about, then ` · <suffix>` when there is one
    /// (the German article).
    Item {
        name: ItemName,
        suffix: Option<String>,
    },
}

/// The name of the thing a phrase is about.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemName {
    /// The pack's own word for it (its `word.<key>` translation): the translation language is
    /// English or one of the pack's hint languages.
    Text(String),
    /// The game's name: the entity type's when `minecraft:<key>` is an entity and not an item,
    /// else the hover name of the item the pack draws for the key ([`Pack::picture`]), else
    /// `fallback`.
    Game { key: String, fallback: String },
}

/// A grammar note: a lang key filled with its arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub key: String,
    pub args: Vec<Label>,
}

/// Another form of the sentence.
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    pub text: String,
    /// Drawn after the text in the reading colour (`hudColorPinyin`); empty for German.
    pub reading: String,
    /// The lang key of what form it is, drawn under it.
    pub label: String,
}

/// What the detail pane shows for a phrase, besides its picture, translation and Play buttons.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    /// Each word with the hint drawn small above it; `None` past the entry's hints. The word's
    /// colour is its role's ([`Config::role_colour`], else `hudColorZh`).
    pub words: Vec<(Part, Option<Label>)>,
    pub notes: Vec<Note>,
    pub forms: Vec<Form>,
}

fn strings(value: &Value) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{value} is not a list"))?
        .iter()
        .map(gson::as_string)
        .collect()
}

fn lists(value: &Value) -> Result<Vec<Vec<String>>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{value} is not a list"))?
        .iter()
        .map(strings)
        .collect()
}

impl Dictionary {
    /// Parses `dictionary.json`. On an error the caller goes on with an empty dictionary, which
    /// the screen shows as "no dictionary yet".
    pub fn parse(text: &str) -> Result<Dictionary, String> {
        let root: Value =
            serde_json::from_str(text).map_err(|e| format!("dictionary.json: {e}"))?;
        let entries = root
            .as_object()
            .and_then(|root| field(root, "entries"))
            .and_then(Value::as_object)
            .ok_or("dictionary.json has no entries")?;
        let mut out = HashMap::new();
        for (id, value) in entries {
            let object = value
                .as_object()
                .ok_or_else(|| format!("dictionary.json: {id} is not an object"))?;
            let part = |name: &str| {
                field(object, name).ok_or_else(|| format!("dictionary.json: {id} has no {name}"))
            };
            let entry = Entry {
                hints: strings(part("h")?)?,
                notes: lists(part("n")?)?,
                forms: lists(part("f")?)?,
            };
            out.insert(id.clone(), entry);
        }
        Ok(Dictionary { entries: out })
    }

    /// No entries: the screen says the language has no dictionary yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The learned phrases with an entry; see the module docs.
    pub fn learned<'a>(&self, pack: &'a Pack, progress: &Progress, config: &Config) -> Learned<'a> {
        let need = config.learned_after_correct.max(1);
        let mut learned = Learned::default();
        for (id, &total) in &progress.correct_total {
            let Some(phrase) = pack.phrase(id) else {
                continue;
            };
            if total >= need && self.entries.contains_key(id) {
                learned.all.push(phrase);
                if progress.is_learned(id, config) {
                    learned.this_world.push(phrase);
                }
            }
        }
        let newest = |at: &BTreeMap<String, i64>, phrase: &Phrase| {
            Reverse(at.get(&phrase.id).copied().unwrap_or(0))
        };
        learned
            .all
            .sort_by_key(|phrase| newest(&progress.last_correct_at, phrase));
        let here = &progress.world().correct_at;
        learned
            .this_world
            .sort_by_key(|phrase| newest(here, phrase));
        learned
    }

    /// The detail pane's words, notes and forms for a phrase, or `None` when it has no entry.
    /// `language` is the translation language the pack was loaded with
    /// ([`Config::translation_language`]).
    pub fn detail(&self, pack: &Pack, phrase: &Phrase, language: &str) -> Option<Detail> {
        let entry = self.entries.get(&phrase.id)?;
        let ns = &pack.meta.namespace;
        let words = phrase
            .parts
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let hint = entry
                    .hints
                    .get(index)
                    .map(|code| hint(code, phrase, pack, language));
                (part.clone(), hint)
            })
            .collect();
        let notes = entry
            .notes
            .iter()
            .filter_map(|note| note_of(note, ns))
            .collect();
        let forms = entry
            .forms
            .iter()
            .map(|form| {
                let at = |index: usize| form.get(index).cloned().unwrap_or_default();
                Form {
                    text: at(0),
                    reading: at(1),
                    label: format!("{ns}.dict.form.{}", at(2)),
                }
            })
            .collect();
        Some(Detail {
            words,
            notes,
            forms,
        })
    }
}

/// The search box: phrases whose text, reading and translation, as one line, hold the query,
/// ignoring case and the spaces around the query.
pub fn search<'a>(phrases: &[&'a Phrase], query: &str) -> Vec<&'a Phrase> {
    let query = query.to_lowercase();
    let query = query.trim_matches(|c: char| c <= ' ');
    phrases
        .iter()
        .copied()
        .filter(|phrase| {
            query.is_empty()
                || format!("{} {} {}", phrase.text, phrase.reading, phrase.en)
                    .to_lowercase()
                    .contains(query)
        })
        .collect()
}

/// `DictionaryScreen.hint`: `w.<word>` is the pack's lang key, `color.<c>` the game's colour name,
/// `item` and `item|<suffix>` the thing's name; anything else is shown as it is.
fn hint(code: &str, phrase: &Phrase, pack: &Pack, language: &str) -> Label {
    if code.starts_with("w.") {
        Label::Key(format!("{}.dict.{code}", pack.meta.namespace))
    } else if let Some(colour) = code.strip_prefix("color.") {
        Label::Key(format!("color.minecraft.{colour}"))
    } else if code.starts_with("item") {
        Label::Item {
            name: item_name(phrase, pack, language),
            suffix: code.split_once('|').map(|(_, suffix)| suffix.to_string()),
        }
    } else {
        Label::Text(code.to_string())
    }
}

/// `DictionaryScreen.itemName`.
fn item_name(phrase: &Phrase, pack: &Pack, language: &str) -> ItemName {
    let key = phrase.key.as_deref().unwrap_or_default();
    let word = pack.phrase(&format!("word.{key}"));
    let known = language == "en" || pack.meta.hint_languages.iter().any(|l| l == language);
    match word {
        Some(word) if known => ItemName::Text(pack::bare(&word.en).to_string()),
        _ => ItemName::Game {
            key: key.to_string(),
            fallback: word.map_or(key, |word| pack::bare(&word.en)).to_string(),
        },
    }
}

/// `DictionaryScreen.note`: the note's lang key `<ns>.dict.note.<type>` and its arguments, `w.`
/// words as the pack's lang keys. An empty note is skipped.
fn note_of(note: &[String], ns: &str) -> Option<Note> {
    let (kind, args) = note.split_first()?;
    let arg = |index: usize| args.get(index).cloned().unwrap_or_default();
    let args = match kind.as_str() {
        "verb" => vec![
            Label::Text(arg(0)),
            Label::Text(arg(1)),
            Label::Key(format!("{ns}.dict.w.{}", arg(2))),
        ],
        "de" => vec![Label::Text(arg(0))],
        "measure" => vec![
            Label::Text(arg(0)),
            Label::Text(arg(1)),
            Label::Text(arg(2)),
            Label::Key(format!("{ns}.dict.measure.{}", arg(3))),
            Label::Text(arg(4)),
        ],
        _ => args
            .iter()
            .map(|arg| {
                if arg.starts_with("w.") {
                    Label::Key(format!("{ns}.dict.{arg}"))
                } else {
                    Label::Text(arg.clone())
                }
            })
            .collect(),
    };
    Some(Note {
        key: format!("{ns}.dict.note.{kind}"),
        args,
    })
}
