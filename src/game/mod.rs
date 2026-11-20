//! The game layer: the core's teacher wired to Horizon's client hooks. Detection, the voice, the
//! HUD, the question screen, the keys, the files and the world key each have a module; this one
//! keeps the state and runs the client tick.
//!
//! State lives in one process-wide mutex. The two native mixins (`src/mixins/`) run inside game
//! functions with no host, so what they share with the layer is kept in statics of its own: the
//! pause and shared flags, the queued log lines and the seat reader. Every client read and write
//! goes through [`client::Client`], one seam the wiring test answers, as the SDK's fake host has no
//! client backend.
//!
//! Split-screen players each run a `Game` of their own in this process, so they share these
//! statics and the one teacher. The client hooks act only for this window's own game, seat 0
//! ([`hosting`]); the other seats' games get nothing from the mod.
pub mod client;
pub(crate) mod detect;
mod files;
pub mod hud;
pub(crate) mod keys;
pub(crate) mod loading;
pub(crate) mod payload;
pub(crate) mod screen;
pub(crate) mod server;
pub(crate) mod settings;
mod sound;
#[cfg(test)]
mod tests;
pub(crate) mod typing;
pub(crate) mod world;

use crate::core::{Outcome, Pictures, QuizEvent, Teacher, Verdict, Wipe};
use client::{Client, HostClient, View};
use mod_api::{ClientHost, SoundRef, Text};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock, RwLock};

pub const MOD_ID: &str = "learnordie";

/// True while this mod's screen asks to pause: the question screen with `quizPauseGame` on, or the
/// settings pages (options screens pause); the `quiz_pause` mixin adds it to
/// `Game::pauses_game`, which pauses single player only.
static QUIZ_PAUSES: AtomicBool = AtomicBool::new(false);
/// Esc has just suspended the question: the `quiz_escape` mixin opens the pause menu once Escape
/// has closed the screen, as the original's `onClose` does. Cleared every client tick.
static ESCAPE_PAUSE: AtomicBool = AtomicBool::new(false);
/// The integrated server is open to LAN (Java `IntegratedServer.isPublished`) or has split-screen
/// players joined, as the `quiz_pause` mixin sees it every frame (no API tells a mod): the world
/// then runs on behind a pause screen.
static SHARED: AtomicBool = AtomicBool::new(false);

/// For the `quiz_pause` mixin: the question screen asks the game to pause.
pub fn quiz_pauses() -> bool {
    QUIZ_PAUSES.load(Ordering::Relaxed)
}

/// For the `quiz_escape` mixin: whether the screen that just closed was the question's, left by
/// Esc. True once.
pub fn take_escape_pause() -> bool {
    ESCAPE_PAUSE.swap(false, Ordering::Relaxed)
}

/// For the `quiz_pause` mixin: whether the integrated server is open to LAN or has split-screen
/// players joined. Only this window's own game's frame sets it.
pub fn set_shared(shared: bool) {
    SHARED.store(shared, Ordering::Relaxed);
}

pub(crate) fn shared() -> bool {
    SHARED.load(Ordering::Relaxed)
}

/// The engine's `horizon_hooks::seat`: whose turn it is on the game's thread, 0 for this window's
/// own player, 1 for split-screen P2 and so on. The mod's crate cannot name the engine's crates;
/// the `quiz_pause` mixin, compiled against them, hands it over on the first frame.
static SEAT: OnceLock<fn() -> usize> = OnceLock::new();

/// For the `quiz_pause` mixin: the engine's reader of the current seat.
pub fn set_seat(seat: fn() -> usize) {
    let _ = SEAT.set(seat);
}

/// The seat whose turn it is; 0 until the first frame, before any split-screen player can join.
pub(crate) fn seat() -> usize {
    SEAT.get().map_or(0, |seat| seat())
}

/// The hook runs for this window's own game, not a split-screen player's.
pub(crate) fn hosting() -> bool {
    io().seat() == 0
}

/// The game's log (`logs/latest.log`) is written by the game's own `log` crate. A mod's `log`
/// dependency is a separate copy (registry crates are never the SDK's), whose logger the game
/// never sets, and stderr is not in that log. So the mod queues its lines, `(warning, text)`, and the
/// `quiz_pause` mixin, compiled against the game's crates, logs them every frame under the
/// target `learnordie`: `[learnordie] <text>`.
static LOG_LINES: Mutex<Vec<(bool, String)>> = Mutex::new(Vec::new());
/// Lines kept while nothing takes them (a dedicated server has no frames): the newest win.
const LOG_LINES_KEPT: usize = 256;

fn log_line(warning: bool, text: String) {
    let mut lines = lock(&LOG_LINES);
    if lines.len() >= LOG_LINES_KEPT {
        lines.remove(0);
    }
    lines.push((warning, text));
}

/// An info line for the game's log.
pub(crate) fn log_info(text: String) {
    log_line(false, text);
}

/// A warning for the game's log.
pub(crate) fn log_warn(text: String) {
    log_line(true, text);
}

/// For the `quiz_pause` mixin: the lines waiting for the game's log, oldest first, each
/// `(warning, text)`.
pub fn take_log_lines() -> Vec<(bool, String)> {
    std::mem::take(&mut *lock(&LOG_LINES))
}

/// What the layer keeps beside the teacher.
#[derive(Default)]
pub(crate) struct Game {
    /// The reload the teacher was built after (or failed at).
    loaded: Option<u64>,
    /// The item registry, indexed for the question's picture.
    pictures: Pictures,
    /// The level joined, by its view hash; `None` outside a world and while a join waits.
    level: Option<u64>,
    /// When a single-player join began waiting for the seed.
    seed_wait: Option<i64>,
    /// The block the player was mining last tick.
    target: Option<detect::Target>,
    /// The blocks the attack key acted on this tick, for its end.
    attacked: Vec<detect::Target>,
    pending_place: Option<detect::PendingPlace>,
    /// The teacher's last sound.
    sound: Option<SoundRef>,
    /// Answers the screen's hooks took, for the next client tick's chat lines.
    outcomes: Vec<Outcome>,
    /// The question id and the item drawn for it.
    picture: Option<(String, Option<String>)>,
    /// The question screen's size when last drawn, for its clicks.
    screen_size: [f32; 2],
    /// The typed question's edit box and buttons, while one shows.
    typing: Option<typing::Typing>,
    /// The settings page this mod's screen shows instead of a question.
    settings: Option<settings::Settings>,
    /// The world key of the world joined.
    world_key: Option<String>,
    /// The world the "no consequences" line was logged for.
    warned_world: Option<String>,
}

pub(crate) struct State {
    /// `None` until a pack loads.
    teacher: Option<Teacher>,
    game: Game,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| {
    Mutex::new(State {
        teacher: None,
        game: Game::default(),
    })
});

static IO: LazyLock<RwLock<Arc<dyn Client>>> = LazyLock::new(|| RwLock::new(Arc::new(HostClient)));

/// The client seam in use.
pub(crate) fn io() -> Arc<dyn Client> {
    IO.read().unwrap_or_else(|e| e.into_inner()).clone()
}

#[cfg(test)]
pub(crate) fn set_io(io: Arc<dyn Client>) {
    *IO.write().unwrap_or_else(|e| e.into_inner()) = io;
}

/// Locks a mutex, carrying on past a poisoned one (a panicking hook must not stop the others).
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn question_up(teacher: &Teacher) -> bool {
    teacher.question().is_some_and(|open| !open.is_suspended())
}

/// One client tick, then what the game drew since the last one is forgotten.
pub(crate) fn client_tick(host: &mut dyn ClientHost) {
    tick(host);
    detect::end_tick();
}

/// One client tick (Java `ZhNarratorClient.tick`): the pack after a reload, the settings page,
/// the keys, the level, the detector and the quiz while the game runs, the question's clock,
/// the speaker, the progress file, then what the answers left to do.
fn tick(host: &mut dyn ClientHost) {
    ESCAPE_PAUSE.store(false, Ordering::Relaxed);
    let io = io();
    let io = &*io;
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    reload(&*host, io, teacher, game, now);
    let Some(teacher) = teacher.as_mut() else {
        QUIZ_PAUSES.store(false, Ordering::Relaxed);
        return;
    };
    let Some(mut view) = io.view(&*host) else {
        leave(&*host, io, teacher, game, now);
        QUIZ_PAUSES.store(false, Ordering::Relaxed);
        return;
    };
    detect::merge_events(&mut view, now);
    settings::tick(host, io, game, &view);
    keys::handle(host, io, teacher, game, now);
    if game.settings.is_some() {
        // The settings page is up: a screen is open, and it is not the question's.
        view.screen_open = true;
        view.our_screen = false;
    }
    let mut opened = false;
    if join(teacher, game, &view, now) && !view.paused {
        opened = world_tick(host, io, teacher, game, &view, now);
    }
    game.attacked.clear();
    if view.our_screen
        && question_up(teacher)
        && let Some(outcome) = teacher.question_tick(now)
    {
        game.outcomes.push(outcome);
    }
    let active = game
        .sound
        .is_some_and(|sound| io.sound_active(&*host, sound));
    let commands = teacher.speaker_tick(now, active);
    sound::run(&*host, io, commands, &mut game.sound);
    if teacher.progress.needs_save(now) {
        let language = &teacher.pack.meta.language;
        files::save_progress(&*host, io, &mut teacher.progress, language, now);
    }
    finish_answers(host, io, teacher, game, view.our_screen);
    if !question_up(teacher) {
        game.typing = None;
    }
    let up = question_up(teacher) && (view.our_screen || opened);
    let pauses = (up && teacher.config.quiz_pause_game) || game.settings.is_some();
    QUIZ_PAUSES.store(pauses, Ordering::Relaxed);
}

/// The client is about to disconnect: the progress is saved.
pub(crate) fn logging_out(host: &mut dyn ClientHost) {
    let io = io();
    let io = &*io;
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    if let Some(teacher) = teacher.as_mut() {
        leave(&*host, io, teacher, game, now);
    }
    QUIZ_PAUSES.store(false, Ordering::Relaxed);
}

/// A resource reload finished: the teacher is built again from the pack, keeping his config and
/// progress, and joins the level again.
fn reload(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Option<Teacher>,
    game: &mut Game,
    now: i64,
) {
    let generation = loading::generation();
    if game.loaded == Some(generation) {
        return;
    }
    game.loaded = Some(generation);
    match loading::load(host, io, teacher.as_mut(), now) {
        Ok((fresh, pictures)) => {
            if let Some(sound) = game.sound.take() {
                io.stop(host, sound);
            }
            *teacher = Some(fresh);
            game.pictures = pictures;
            game.level = None;
            game.target = None;
            game.attacked.clear();
            game.pending_place = None;
            game.picture = None;
            game.typing = None;
        }
        Err(e) => log_warn(format!("the teacher stays away: {e}")),
    }
}

/// The player left the world: the progress is saved, his voice stopped and the trackers
/// forgotten.
fn leave(host: &dyn ClientHost, io: &dyn Client, teacher: &mut Teacher, game: &mut Game, now: i64) {
    game.seed_wait = None;
    if game.level.take().is_none() {
        return;
    }
    sound::run(host, io, teacher.left_world(now), &mut game.sound);
    let language = &teacher.pack.meta.language;
    files::save_progress(host, io, &mut teacher.progress, language, now);
    game.target = None;
    game.attacked.clear();
    game.pending_place = None;
    game.outcomes.clear();
    game.picture = None;
    game.typing = None;
    game.settings = None;
    game.world_key = None;
    detect::set_hardcore(false);
}

/// The level object changed (a world joined, another dimension, a respawn into a new level):
/// the teacher starts over in it, under its world key. False while a single-player join waits
/// for the seed.
fn join(teacher: &mut Teacher, game: &mut Game, view: &View, now: i64) -> bool {
    if game.level == Some(view.level) {
        return true;
    }
    let since = *game.seed_wait.get_or_insert(now);
    let Some(key) = world::key(view, now, since) else {
        return false;
    };
    game.seed_wait = None;
    game.level = Some(view.level);
    game.target = None;
    game.attacked.clear();
    game.pending_place = None;
    teacher.joined_level(now, &key, &view.dimension, &view.held_item);
    game.world_key = Some(key);
    true
}

/// The detector and the quiz, while the game runs (Java `detector.tick` and `quiz.tick`). True
/// when the question screen opened.
fn world_tick(
    host: &mut dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    view: &View,
    now: i64,
) -> bool {
    let broken = detect::poll_break(&*host, io, view, &mut game.target, &mut game.attacked);
    for (block, held) in broken {
        teacher.block_broken(now, &block, &held);
    }
    detect::check_place(&*host, io, &mut game.pending_place, teacher, now);
    let inventory: Option<Vec<(&str, i32)>> = view.inventory.as_ref().map(|stacks| {
        stacks
            .iter()
            .map(|(item, count)| (item.as_str(), *count))
            .collect()
    });
    let breaking = detect::mining(game.target.as_ref());
    let fates = detect::fates(&*host, io, teacher);
    let hits = detect::hits(&*host, io, teacher);
    let hits: Vec<(u64, &str)> = hits.iter().map(|(id, kind)| (*id, kind.as_str())).collect();
    let input = detect::world_tick(view, inventory.as_deref(), breaking, &hits, &fates, now);
    let event = teacher.world_tick(&input);
    match event {
        Some(QuizEvent::Ask) => {
            sound::ding(&*host, io);
            open_question(host, io, teacher, game)
        }
        Some(QuizEvent::Resume) => open_question(host, io, teacher, game),
        Some(QuizEvent::Dropped) | None => false,
    }
}

/// Opens the question screen, working out the question's picture first.
fn open_question(
    host: &mut dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    game: &mut Game,
) -> bool {
    let Some(open) = teacher.question() else {
        return false;
    };
    let id = &open.question.id;
    if game.picture.as_ref().is_none_or(|(known, _)| known != id) {
        let item = open
            .question
            .key
            .as_deref()
            .and_then(|key| teacher.pack.picture(key, &game.pictures))
            .map(str::to_string);
        game.picture = Some((id.clone(), item));
    }
    // A typed question's box is made for it when its screen first draws: empty for a new
    // question, with the kept text for one coming back.
    game.typing = None;
    io.open_screen(host)
}

/// What the original logs when an answer has no integrated server to act on, once per world
/// here (the original logs it at every answer).
pub(crate) const NO_SERVER_LINE: &str =
    "answers bring no rewards or penalties here: they work in single player only";

/// What the answers left to do (Java `QuizManager.answerTyped` and `answer`): the chat lines in
/// the original's order (what was typed, a learned phrase, the angry teacher, the verdict), the
/// consequences sent to the integrated server, and the question screen closed once it has
/// nothing left to show.
fn finish_answers(
    host: &mut dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    game: &mut Game,
    our_screen: bool,
) {
    for outcome in std::mem::take(&mut game.outcomes) {
        if let Some(line) = typed_line(&outcome) {
            io.chat(host, line);
        }
        if outcome.learned {
            io.chat(host, learned(&outcome));
        }
        if let Some(wipe) = outcome.angry {
            io.chat(host, angry(teacher, wipe));
        }
        io.chat(host, verdict(teacher, &outcome));
        match &outcome.server {
            Some(actions) => {
                io.send_consequences(host, actions);
            }
            None => {
                if game.warned_world != game.world_key {
                    game.warned_world = game.world_key.clone();
                    log_warn(NO_SERVER_LINE.into());
                }
            }
        }
    }
    if our_screen && !question_up(teacher) {
        io.close_screen(host);
    }
}

/// `§7`, the grey of the original's side notes.
const GREY: u32 = 0xAA_AAAA;
/// `§c`.
const RED: u32 = 0xFF_5555;

/// A typed answer that was not exact, said back in grey: nothing typed, close enough with a
/// spelling note, or wrong.
fn typed_line(outcome: &Outcome) -> Option<Text> {
    let typed = outcome
        .typed
        .as_ref()
        .filter(|t| t.verdict != Verdict::Exact)?;
    let key = if typed.text.trim().is_empty() {
        "learnordie.quiz.typed_nothing"
    } else if typed.verdict == Verdict::Close {
        "learnordie.quiz.typed_close"
    } else {
        "learnordie.quiz.typed"
    };
    let mut line = Text::translate(key, vec![Text::literal(typed.text.clone())]);
    line.style.color = Some(format!("#{GREY:06X}"));
    Some(line)
}

/// The angry teacher's line: his angry phrase in red, then what he did in grey (said even with no
/// integrated server to do it, as the original).
fn angry(teacher: &Teacher, wipe: Wipe) -> Text {
    let phrase = teacher
        .pack
        .phrase("angry")
        .map_or_else(String::new, |p| p.text.clone());
    let key = match wipe {
        Wipe::Drop => "learnordie.angry.drop",
        Wipe::Delete => "learnordie.angry.delete",
    };
    let mut what = Text::translate(key, vec![]);
    what.style.color = Some(format!("#{GREY:06X}"));
    Text::literal("")
        .append(screen::coloured(format!("{phrase} "), RED))
        .append(what)
}

/// `Richtig! ✓ <text>  (<translation>)`, green, or the wrong one in red.
fn verdict(teacher: &Teacher, outcome: &Outcome) -> Text {
    let meta = &teacher.pack.meta;
    let question = &outcome.question;
    let (mark, rgb) = if outcome.correct {
        (format!("{} \u{2713}", meta.ui_correct), 0x55_FF55)
    } else {
        (format!("{} \u{2717}", meta.ui_wrong), 0xFF_5555)
    };
    let reading = if question.reading.is_empty() {
        String::new()
    } else {
        format!("  {}", question.reading)
    };
    let line = format!("{mark} {}{reading}  ({})", question.text, question.en);
    screen::coloured(line, rgb)
}

/// The line for a phrase that this answer made learned, in gold.
fn learned(outcome: &Outcome) -> Text {
    let question = &outcome.question;
    let key = if question.action == "hold" {
        "learnordie.learned.word"
    } else {
        "learnordie.learned.phrase"
    };
    let en = question.en.strip_suffix(['.', '!']).unwrap_or(&question.en);
    let mut line = Text::translate(
        key,
        vec![Text::literal(format!("{} ({en})", question.bare()))],
    );
    line.style.color = Some("#FFAA00".to_string());
    line
}

/// Runs `f` on the teacher and the layer's state, for the wiring test.
#[cfg(test)]
pub(crate) fn with_state<R>(f: impl FnOnce(Option<&mut Teacher>, &mut Game) -> R) -> R {
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    f(teacher.as_mut(), game)
}
