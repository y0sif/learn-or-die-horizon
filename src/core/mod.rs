//! Learn or Die's game-independent core: the language pack, the teacher's narration rules, the
//! quiz (choice and typed), the consequences of an answer, the per-world progress, the config and
//! its settings pages. Ported from Learn or Die by Laren, All Rights Reserved, with permission.
//!
//! Game facts come in as plain data: registry ids such as `minecraft:oak_log` (lookups match the
//! path after the colon), wall-clock times in ms, and a seed for the random source. Decisions come
//! out as plain data: sound commands, quiz events, answer outcomes (with what the integrated
//! server applies) and what the HUD shows.
//! [`Teacher`] is the entry point and owns all state; nothing here calls the game.
mod config;
mod consequences;
mod detector;
mod gson;
mod hud;
mod pack;
mod progress;
mod quiz;
mod rng;
mod settings;
mod speaker;
mod teacher;
#[cfg(test)]
mod tests;
mod typed;

pub use config::Config;
pub use consequences::{
    Effect, HeartsAfter, LOST_HEARTS_MODIFIER, ServerAction, Summon, Wipe, after_lost_hearts,
    mob_pool, summons,
};
pub use hud::{HudText, Span, spans};
pub use pack::{Lipsync, Meta, Pack, Part, Phrase, Pictures, Sprite, Word};
pub use progress::{Progress, REST_FOREVER_MS, World, server_world_key, singleplayer_world_key};
pub use quiz::{OpenQuestion, QuestionKind, QuizEvent};
pub use settings::{Page, Setting, Shown, settings};
pub use speaker::SoundCommand;
pub use teacher::{Fate, Outcome, Teacher, Typed, WorldTick};
pub use typed::Verdict;
