//! The teacher: the one entry point the game layer calls. It owns the pack, the config, the
//! progress and all narration and quiz state, takes game facts as plain data and hands back what
//! to play, show and ask.
//!
//! Per client tick, in this order: [`Teacher::world_tick`] (in a world and not paused only), then
//! [`Teacher::speaker_tick`] (always). Events between ticks: [`Teacher::block_broken`] and
//! [`Teacher::block_placed`]. Call [`Teacher::joined_level`] whenever the level object changes,
//! before the first world tick in it. A new pack means a new `Teacher`, as a language switch
//! rebuilds everything in the original.
use super::config::Config;
use super::detector::{Detector, Speech};
use super::hud::{self, HudText, Portrait, Span};
use super::pack::{Pack, Phrase};
use super::progress::Progress;
use super::quiz::{Answer, OpenQuestion, Quiz, QuizEvent};
use super::rng::Rng;
use super::speaker::{SoundCommand, Speaker};

/// What the game reports each tick while the player is in a world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldTick<'a> {
    /// Wall-clock time in ms.
    pub now: i64,
    /// The attack key is down.
    pub attack_down: bool,
    /// The use key is down.
    pub use_down: bool,
    /// `player.isUsingItem()`.
    pub using_item: bool,
    /// The block being mined right now (`gameMode.isDestroying()` and the block hit), if any.
    pub breaking: Option<&'a str>,
    /// The main-hand item id (`minecraft:air` or empty when the hand is empty) and its count.
    pub held_item: &'a str,
    pub held_count: i32,
    /// Every inventory slot as `(item id, count)`; empty slots may be left out. `None` when the
    /// game could not read it this tick: the pickup check waits for the next.
    pub inventory: Option<&'a [(&'a str, i32)]>,
    /// Any screen is open.
    pub screen_open: bool,
    /// The open screen is the question screen.
    pub question_screen_open: bool,
    /// An integrated server that is not open to LAN.
    pub singleplayer: bool,
    pub dead: bool,
    pub health: f32,
    pub sleeping: bool,
    pub in_vehicle: bool,
    pub in_water: bool,
    pub on_ground: bool,
    pub fall_flying: bool,
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}

/// A finished question.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub question: Phrase,
    pub correct: bool,
    /// This answer made the phrase learned; a `hold` phrase is a word, any other a phrase.
    pub learned: bool,
}

pub struct Teacher {
    pub pack: Pack,
    pub config: Config,
    pub progress: Progress,
    speaker: Speaker,
    detector: Detector,
    quiz: Quiz,
    portrait: Portrait,
    rng: Rng,
    held_label: Option<(Phrase, i64)>,
    last_dimension: Option<String>,
    last_world_key: Option<String>,
}

impl Teacher {
    pub fn new(pack: Pack, config: Config, progress: Progress, seed: u64) -> Teacher {
        Teacher {
            pack,
            config,
            progress,
            speaker: Speaker::default(),
            detector: Detector::default(),
            quiz: Quiz::default(),
            portrait: Portrait::default(),
            rng: Rng::new(seed),
            held_label: None,
            last_dimension: None,
            last_world_key: None,
        }
    }

    /// The level object changed: a world was joined, or the dimension changed. Resets the
    /// trackers, switches the progress world when the key changed, and restarts the quiz interval
    /// in a new world or dimension.
    pub fn joined_level(&mut self, now: i64, world_key: &str, dimension: &str, held_item: &str) {
        let new_key = self.last_world_key.as_deref() != Some(world_key);
        let new_world = new_key || self.last_dimension.as_deref() != Some(dimension);
        self.last_dimension = Some(dimension.to_string());
        self.detector.reset(held_item);
        if new_key {
            self.last_world_key = Some(world_key.to_string());
            self.progress.switch_world(world_key);
        }
        if new_world {
            self.quiz.on_joined_world(now, &self.progress);
        }
    }

    /// The player broke a block, holding `held_item`.
    pub fn block_broken(&mut self, now: i64, block: &str, held_item: &str) {
        let speech = self
            .detector
            .block_broken(block, held_item, now, &self.pack, &self.config);
        self.apply(speech, now);
    }

    /// A block the player placed showed up in the world. The game layer skips the click when
    /// [`Pack::use_phrase`] has a phrase for the clicked block (main hand, not sneaking), and gives
    /// up on a place it has not seen within 500 ms.
    pub fn block_placed(&mut self, now: i64, block: &str) {
        let speech = self
            .detector
            .block_placed(block, now, &self.pack, &self.config);
        self.apply(speech, now);
    }

    /// One client tick in a world, not paused: narration for the tick's state, then the quiz.
    pub fn world_tick(&mut self, input: &WorldTick<'_>) -> Option<QuizEvent> {
        let speech = self.detector.tick(input, &self.pack, &self.config);
        self.apply(speech, input.now);
        let event = self.quiz.tick(
            input,
            &self.pack,
            &self.config,
            &self.progress,
            &mut self.rng,
        );
        if matches!(event, Some(QuizEvent::Ask | QuizEvent::Resume)) {
            self.say_if_listening(input.now);
        }
        event
    }

    /// One client tick, paused or not. `sound_active` is the game's report on the last sound
    /// played. Returns the sound commands to run, in order.
    pub fn speaker_tick(&mut self, now: i64, sound_active: bool) -> Vec<SoundCommand> {
        self.speaker.tick(
            now,
            sound_active,
            &self.pack,
            &self.config,
            &mut self.progress,
        );
        self.speaker.take_commands()
    }

    /// The player left the world: his clip stops and what is queued is dropped, so none of it is
    /// said in the next world. Returns the sound commands to run.
    pub fn left_world(&mut self, now: i64) -> Vec<SoundCommand> {
        self.speaker.stop(now);
        self.speaker.take_commands()
    }

    /// Shows `display` while `clips` play one after the other. `fresh` skips the same-event
    /// cooldown.
    pub fn say_sequence(&mut self, display: &Phrase, clips: &[Phrase], fresh: bool, now: i64) {
        self.speaker
            .say_sequence(display, clips, fresh, &self.config, &self.progress, now);
    }

    /// The teacher key: on or off. Stops his sound. Returns whether he is on now.
    pub fn toggle_teacher(&mut self, now: i64) -> bool {
        self.config.enabled = !self.config.enabled;
        self.speaker.stop(now);
        self.config.enabled
    }

    /// The mute key: his sound off or on; text and questions stay. Returns whether he is muted.
    pub fn toggle_mute(&mut self, now: i64) -> bool {
        self.config.muted = !self.config.muted;
        if self.config.muted {
            self.speaker.stop(now);
        }
        self.config.muted
    }

    /// The question, open or suspended. Draw its screen while it is not suspended.
    pub fn question(&self) -> Option<&OpenQuestion> {
        self.quiz.open()
    }

    /// Key `index + 1` or a click on option `index`. `None` in the 700 ms lockout.
    pub fn choose(&mut self, index: usize, now: i64) -> Option<Outcome> {
        let answer = self.quiz.choose(index, now)?;
        Some(self.answered(answer, now))
    }

    /// The question screen's tick: a time-out answers wrong.
    pub fn question_tick(&mut self, now: i64) -> Option<Outcome> {
        let answer = self.quiz.screen_tick(now)?;
        Some(self.answered(answer, now))
    }

    /// Esc on the question screen: the question waits with its time left while the pause menu
    /// is open, and [`Teacher::world_tick`] brings it back once no screen is open.
    pub fn suspend_question(&mut self, now: i64) -> bool {
        self.quiz.suspend(now)
    }

    /// The replay button of a listening question.
    pub fn replay_question(&mut self, now: i64) {
        if let Some(question) = self.quiz.open().map(|open| open.question.clone()) {
            self.speaker.say_now(&question, true, &self.config, now);
        }
    }

    /// The phrase on the HUD now, if the HUD text is on.
    pub fn hud_text(&self, now: i64) -> Option<HudText<'_>> {
        if !self.config.hud_enabled {
            return None;
        }
        let phrase = self.speaker.shown(now, &self.config)?;
        Some(HudText {
            phrase,
            alpha: ((255.0 * self.speaker.visibility(now, &self.config)).round() as i32).max(16),
            show_translation: self.config.hud_show_english
                && phrase.action != "hold"
                && phrase.en != phrase.text,
        })
    }

    /// The phrase in coloured pieces; see [`hud::spans`].
    pub fn spans(&self, phrase: &Phrase, fallback: i32) -> Vec<Span> {
        hud::spans(
            phrase,
            &self.pack.meta.word_separator,
            &self.config,
            fallback,
        )
    }

    /// The mouth level now: 0 closed, 1 small, 2 wide.
    pub fn mouth(&self, now: i64) -> i32 {
        self.speaker.mouth(now, &self.pack)
    }

    /// A clip is playing or another of its chain waits.
    pub fn is_speaking(&self) -> bool {
        self.speaker.is_speaking()
    }

    /// The portrait frame to draw now. Call it once per drawn frame: it runs the blink timer.
    pub fn portrait_frame(&mut self, now: i64) -> i32 {
        let mouth = self.speaker.mouth(now, &self.pack);
        let speaking = self.speaker.is_speaking();
        let frowning = self.quiz.frowning(now);
        self.portrait.frame(
            now,
            &self.pack.meta.sprite,
            mouth,
            speaking,
            self.quiz.hop_at(),
            frowning,
        )
    }

    /// How high the portrait jumps now, in sprite pixels.
    pub fn hop_lift(&self, now: i64) -> f32 {
        hud::hop_lift(now, self.quiz.hop_at(), self.pack.meta.sprite.hop)
    }

    /// The held-item label and its alpha, while it shows.
    pub fn held_label(&self, now: i64) -> Option<(&Phrase, i32)> {
        if !self.config.hud_held_label {
            return None;
        }
        let (phrase, at) = self.held_label.as_ref()?;
        Some((phrase, hud::held_label_alpha(now - at)?))
    }

    fn apply(&mut self, speech: impl IntoIterator<Item = Speech>, now: i64) {
        for item in speech {
            match item {
                Speech::New(phrase) => {
                    self.speaker
                        .say_new(&phrase, &self.config, &self.progress, now);
                }
                Speech::Repeat(phrase) => {
                    self.speaker.say(&phrase, &self.config, &self.progress, now);
                }
                Speech::Ongoing { phrase, repeat } => {
                    self.speaker
                        .say_ongoing(&phrase, repeat, &self.config, &self.progress, now);
                }
                Speech::DropQueued(id) => self.speaker.drop_queued(&id),
                Speech::HeldLabel(phrase) => self.held_label = Some((phrase, now)),
            }
        }
    }

    /// A listening question plays its phrase when its screen opens.
    fn say_if_listening(&mut self, now: i64) {
        let listening = self
            .quiz
            .open()
            .filter(|open| open.listening)
            .map(|open| open.question.clone());
        if let Some(question) = listening {
            self.speaker.say_now(&question, true, &self.config, now);
        }
    }

    /// Records the answer, says the learned line when it made the phrase learned, then says the
    /// right sentence at once.
    fn answered(&mut self, answer: Answer, now: i64) -> Outcome {
        let Answer { question, correct } = answer;
        let was_learned = self.progress.is_learned(&question.id, &self.config);
        self.progress.answered(&question.id, correct, now);
        let learned =
            correct && !was_learned && self.progress.is_learned(&question.id, &self.config);
        if learned {
            let line = if question.action == "hold" {
                "learned_word"
            } else {
                "learned_phrase"
            };
            if let Some(phrase) = self.pack.phrase(line) {
                self.speaker.say(phrase, &self.config, &self.progress, now);
            }
        }
        self.speaker.say_now(&question, false, &self.config, now);
        Outcome {
            question,
            correct,
            learned,
        }
    }
}
