//! Learn or Die's game-independent core: the language pack, the teacher's narration rules, the
//! choice quiz, the per-world progress and the config. Ported from Learn or Die by Laren, All
//! Rights Reserved, with permission.
//!
//! Game facts come in as plain data: registry ids such as `minecraft:oak_log` (lookups match the
//! path after the colon), wall-clock times in ms, and a seed for the random source. Decisions come
//! out as plain data: sound commands, quiz events, answer outcomes and what the HUD shows.
//! [`Teacher`] is the entry point and owns all state; nothing here calls the game.
mod config;
mod detector;
mod gson;
mod hud;
mod pack;
mod progress;
mod quiz;
mod rng;
mod speaker;
mod teacher;
#[cfg(test)]
mod tests;

pub use config::Config;
pub use hud::{HudText, Span, spans};
pub use pack::{Lipsync, Meta, Pack, Part, Phrase, Sprite, Word};
pub use progress::{Progress, REST_FOREVER_MS, World, server_world_key, singleplayer_world_key};
pub use quiz::{OpenQuestion, QuizEvent};
pub use speaker::SoundCommand;
pub use teacher::{Outcome, Teacher, WorldTick};
