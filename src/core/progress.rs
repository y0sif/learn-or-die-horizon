//! The learning state (Java `Progress`): what was heard and answered in each world, and the right
//! answers across all worlds. The file keeps the original's shape and field names:
//! `{"worlds": {"<key>": {"heard", "quiz", "correctAt", "heartsLost", "deaths", "exams"}},
//! "correctTotal", "lastCorrectAt", "deaths"}`, with `quiz` values as `[asked, correct]`.
//!
//! Invariants: every count is per world except `correctTotal`, `lastCorrectAt` and `deaths`.
//! When a question was last asked is kept only in memory, as in the original, so it resets on load.
//! Until [`Progress::switch_world`] names a world, counts go to a detached world that is never
//! saved.
use super::config::Config;
use super::gson::{self, JsonObject, field};
use mod_api::serde_json::{self, Map, Value, json};
use std::collections::{BTreeMap, HashMap};

/// How long a learned phrase rests in the `forever` and `until_death` modes.
pub const REST_FOREVER_MS: i64 = i64::MAX / 2;
const SAVE_EVERY_MS: i64 = 15_000;

/// The world key of a singleplayer save: `save:<folder name>:<seed in hex>`, the seed written as
/// Java's `Long.toHexString` writes it.
pub fn singleplayer_world_key(folder: &str, seed: i64) -> String {
    format!("save:{folder}:{:x}", seed as u64)
}

/// The world key on a server: `server:<address>`, or `unknown` with no address.
pub fn server_world_key(address: Option<&str>) -> String {
    address.map_or_else(|| "unknown".to_string(), |ip| format!("server:{ip}"))
}

/// One world's learning state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    /// Times each phrase was played.
    pub heard: BTreeMap<String, i32>,
    /// `[asked, correct]` per phrase.
    pub quiz: BTreeMap<String, [i32; 2]>,
    /// When each phrase was last answered right, in epoch ms.
    pub correct_at: BTreeMap<String, i64>,
    pub hearts_lost: i32,
    pub deaths: i32,
    /// Exam results, kept as written: the exam is not ported yet.
    pub exams: Vec<Value>,
}

impl World {
    fn to_json(&self) -> Value {
        json!({
            "heard": self.heard,
            "quiz": self.quiz,
            "correctAt": self.correct_at,
            "heartsLost": self.hearts_lost,
            "deaths": self.deaths,
            "exams": self.exams,
        })
    }

    fn from_json(value: &Value) -> Result<World, String> {
        let Some(object) = value.as_object() else {
            return if value.is_null() {
                Ok(World::default())
            } else {
                Err(format!("{value} is not a world"))
            };
        };
        let mut world = World::default();
        if let Some(heard) = field(object, "heard") {
            world.heard = gson::map_of(heard, gson::as_i32)?.into_iter().collect();
        }
        if let Some(quiz) = field(object, "quiz") {
            world.quiz = gson::map_of(quiz, stats)?.into_iter().collect();
        }
        if let Some(correct_at) = field(object, "correctAt") {
            world.correct_at = gson::map_of(correct_at, gson::as_i64)?
                .into_iter()
                .collect();
        }
        world.hearts_lost = int(object, "heartsLost")?;
        world.deaths = int(object, "deaths")?;
        if let Some(exams) = field(object, "exams") {
            world.exams = exams
                .as_array()
                .ok_or_else(|| format!("{exams} is not a list"))?
                .clone();
        }
        Ok(world)
    }
}

/// `[asked, correct]`; missing entries read as 0.
fn stats(value: &Value) -> Result<[i32; 2], String> {
    let list = value
        .as_array()
        .ok_or_else(|| format!("{value} is not [asked, correct]"))?;
    let at = |index: usize| list.get(index).map_or(Ok(0), gson::as_i32);
    Ok([at(0)?, at(1)?])
}

fn int(object: &JsonObject, name: &str) -> Result<i32, String> {
    field(object, name).map_or(Ok(0), gson::as_i32)
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub worlds: BTreeMap<String, World>,
    /// Right answers per phrase across every world.
    pub correct_total: BTreeMap<String, i32>,
    /// The last right answer per phrase across every world, in epoch ms.
    pub last_correct_at: BTreeMap<String, i64>,
    /// Hardcore deaths across every world.
    pub deaths: i32,
    current: Option<String>,
    detached: World,
    last_asked_at: HashMap<String, i64>,
    dirty: bool,
    save_now: bool,
    last_save: i64,
}

impl Progress {
    /// Parses the progress file's text. On an error the caller starts from
    /// [`Progress::default`], as the original does with a file it cannot read.
    pub fn parse(text: &str) -> Result<Progress, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Progress::from_json(&value)
    }

    pub fn from_json(value: &Value) -> Result<Progress, String> {
        let object = value
            .as_object()
            .ok_or("the progress file is not a JSON object")?;
        let mut progress = Progress::default();
        if let Some(worlds) = field(object, "worlds") {
            for (key, world) in worlds
                .as_object()
                .ok_or_else(|| format!("{worlds} is not a map of worlds"))?
            {
                progress
                    .worlds
                    .insert(key.clone(), World::from_json(world)?);
            }
        }
        if let Some(total) = field(object, "correctTotal") {
            progress.correct_total = gson::map_of(total, gson::as_i32)?.into_iter().collect();
        }
        if let Some(last) = field(object, "lastCorrectAt") {
            progress.last_correct_at = gson::map_of(last, gson::as_i64)?.into_iter().collect();
        }
        progress.deaths = int(object, "deaths")?;
        Ok(progress)
    }

    pub fn to_json(&self) -> Value {
        let worlds: Map<String, Value> = self
            .worlds
            .iter()
            .map(|(key, world)| (key.clone(), world.to_json()))
            .collect();
        json!({
            "worlds": worlds,
            "correctTotal": self.correct_total,
            "lastCorrectAt": self.last_correct_at,
            "deaths": self.deaths,
        })
    }

    /// The progress file's text, pretty printed as Gson writes it.
    pub fn to_json_string(&self) -> String {
        serde_json::to_string_pretty(&self.to_json()).unwrap_or_default()
    }

    /// Makes `key` the current world, creating it when new, and asks for a save.
    pub fn switch_world(&mut self, key: &str) {
        self.worlds.entry(key.to_string()).or_default();
        self.current = Some(key.to_string());
        self.save_now = true;
    }

    /// The current world's key, once one is set.
    pub fn world_key(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn world(&self) -> &World {
        match &self.current {
            Some(key) => self.worlds.get(key).unwrap_or(&self.detached),
            None => &self.detached,
        }
    }

    fn world_mut(&mut self) -> &mut World {
        match &self.current {
            Some(key) => self.worlds.entry(key.clone()).or_default(),
            None => &mut self.detached,
        }
    }

    /// A phrase was played.
    pub fn heard(&mut self, id: &str) {
        *self.world_mut().heard.entry(id.to_string()).or_insert(0) += 1;
        self.dirty = true;
    }

    /// A question was answered; asks for a save.
    pub fn answered(&mut self, id: &str, correct: bool, now: i64) {
        let world = self.world_mut();
        let stats = world.quiz.entry(id.to_string()).or_insert([0, 0]);
        stats[0] += 1;
        if correct {
            stats[1] += 1;
            world.correct_at.insert(id.to_string(), now);
        }
        self.last_asked_at.insert(id.to_string(), now);
        if correct {
            self.last_correct_at.insert(id.to_string(), now);
            *self.correct_total.entry(id.to_string()).or_insert(0) += 1;
        }
        self.save_now = true;
    }

    /// Times the phrase was asked in this world.
    pub fn asked(&self, id: &str) -> i32 {
        self.world().quiz.get(id).map_or(0, |stats| stats[0])
    }

    /// Right answers for the phrase in this world.
    pub fn correct(&self, id: &str) -> i32 {
        self.world().quiz.get(id).map_or(0, |stats| stats[1])
    }

    /// Enough right answers in this world (`learnedAfterCorrect`, at least 1).
    pub fn is_learned(&self, id: &str, config: &Config) -> bool {
        self.correct(id) >= config.learned_after_correct.max(1)
    }

    /// How long the phrase stays silent and out of the quiz. A learned phrase rests for ever in
    /// the `forever` and `until_death` modes, else `learnedRestMinutes`; a phrase answered right
    /// but not yet learned rests `quizRepeatAfterMinutes`.
    pub fn rest_left(&self, id: &str, now: i64, config: &Config) -> i64 {
        let learned = self.is_learned(id, config);
        if learned && matches!(config.learned_mode.as_str(), "until_death" | "forever") {
            return REST_FOREVER_MS;
        }
        let mut at = self.world().correct_at.get(id).copied();
        if at.is_none() && self.correct(id) > 0 {
            at = self.last_correct_at.get(id).copied();
        }
        let Some(at) = at else {
            return 0;
        };
        let minutes = if learned {
            config.learned_rest_minutes
        } else {
            config.quiz_repeat_after_minutes
        };
        ((minutes * 60_000.0) as i64 - (now - at)).max(0)
    }

    /// Time since the phrase was last asked; [`REST_FOREVER_MS`] when not asked since load.
    pub fn since_asked_ms(&self, id: &str, now: i64) -> i64 {
        self.last_asked_at
            .get(id)
            .map_or(REST_FOREVER_MS, |&at| now - at)
    }

    /// The file should be written now: right after an answer or a world switch, else every 15 s
    /// while something changed. Call [`Progress::saved`] after writing.
    pub fn needs_save(&self, now: i64) -> bool {
        self.save_now || (self.dirty && now - self.last_save > SAVE_EVERY_MS)
    }

    pub fn saved(&mut self, now: i64) {
        self.dirty = false;
        self.save_now = false;
        self.last_save = now;
    }
}
