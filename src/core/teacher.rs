//! The teacher: the one entry point the game layer calls. It owns the pack, the config, the
//! progress and all narration and quiz state, takes game facts as plain data and hands back what
//! to play, show and ask.
//!
//! Per client tick, in this order: [`Teacher::world_tick`] (in a world and not paused only), then
//! [`Teacher::speaker_tick`] (always). Events between ticks: [`Teacher::block_broken`],
//! [`Teacher::block_placed`], [`Teacher::block_used`], [`Teacher::entity_attacked`],
//! [`Teacher::entity_used`] and the keys ([`Teacher::what_is_this`] and the others). Call [`Teacher::joined_level`] whenever the level object changes,
//! before the first world tick in it. A new pack means a new `Teacher`; after a resource reload
//! the new one goes on where the last one stood ([`Teacher::carry_on`]).
use super::config::Config;
use super::consequences::{self, ServerAction, Wipe};
use super::detector::{Detector, Speech};
use super::dictionary::Dictionary;
use super::exam::ExamStage;
use super::hud::{self, HudText, Portrait, Span};
use super::pack::{Pack, Phrase, path_of};
use super::progress::Progress;
use super::quiz::{Answer, OpenQuestion, QuestionKind, Quiz, QuizEvent};
use super::rng::Rng;
use super::speaker::{SoundCommand, Speaker};
use super::typed::{PackWords, Verdict};

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
    /// Any integrated server, open to LAN or not (`getSingleplayerServer() != null`): answers
    /// have consequences only then.
    pub integrated_server: bool,
    /// The world is a Hardcore world: the penalty is always `hardcore`.
    pub hardcore: bool,
    pub dead: bool,
    pub health: f32,
    /// The food level, 0 to 20.
    pub food: i32,
    pub sleeping: bool,
    pub in_vehicle: bool,
    /// The vehicle's entity id, for `ride.<entity>`; a sheep is `<colour>_sheep`.
    pub vehicle: Option<&'a str>,
    pub in_water: bool,
    pub on_ground: bool,
    pub fall_flying: bool,
    /// Creative flight (`getAbilities().flying`).
    pub flying: bool,
    /// `onClimbable()`.
    pub on_climbable: bool,
    /// `isSprinting()`.
    pub sprinting: bool,
    /// `isShiftKeyDown()`.
    pub sneaking: bool,
    /// `isOnFire() || isInLava()`.
    pub on_fire: bool,
    /// A fishing hook is out.
    pub fishing: bool,
    /// The item being used, when it is food or a consumable (`isUsingItem()` and the use item
    /// has the FOOD or CONSUMABLE component).
    pub eating: Option<&'a str>,
    /// A movement key (forward, back, left, right) is down.
    pub move_keys: bool,
    /// The jump key is down.
    pub jump_key: bool,
    /// The drop key is down.
    pub drop_key: bool,
    /// `getDeltaMovement()`, blocks per tick.
    pub velocity: [f64; 3],
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// `level.isDarkOutside()`.
    pub dark: bool,
    /// `level.isRaining()` and `level.isThundering()`.
    pub raining: bool,
    pub thundering: bool,
    /// The biome at the player has precipitation.
    pub biome_rains: bool,
    /// The biome is cold enough to snow at the player (`coldEnoughToSnow(pos, seaLevel)`).
    pub biome_snows: bool,
    /// The biome's base temperature.
    pub biome_temperature: f32,
    /// Living entities, not the player, hurt this very tick (`hurtTime == hurtDuration > 0`)
    /// within 3 blocks of one of the player's projectiles: `(entity, entity id)`, watched as a
    /// melee hit is. Only needed while `sayKills` is on. The game layer gives the kills the
    /// integrated server credits to the player, dying in `entity_fates` too.
    pub projectile_hits: &'a [(u64, &'a str)],
    /// Of [`Teacher::watched_entities`], those dying now or gone. One not listed is alive.
    pub entity_fates: &'a [(u64, Fate)],
}

/// What became of an entity the teacher watches for a kill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// `isDeadOrDying()`.
    Dying,
    /// `isRemoved()`, or no longer in the level.
    Removed,
}

/// What the crosshair is on, for the "what is this" key: a block's or an entity's id (a sheep is
/// `<colour>_sheep`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Looked<'a> {
    Block(&'a str),
    Entity(&'a str),
}

/// A typed answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Typed {
    /// What was typed, without surrounding whitespace. Not EXACT: the chat says it back
    /// (`learnordie.quiz.typed`, `learnordie.quiz.typed_close` for CLOSE, or
    /// `learnordie.quiz.typed_nothing`).
    pub text: String,
    pub verdict: Verdict,
}

/// A finished question.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub question: Phrase,
    pub correct: bool,
    /// This answer made the phrase learned; a `hold` phrase is a word, any other a phrase.
    pub learned: bool,
    /// A typed question: what was typed and how close it was.
    pub typed: Option<Typed>,
    /// What the integrated server applies, in order. `None` with no integrated server: nothing
    /// happens, and the original logs "Quiz rewards and penalties only work in singleplayer for
    /// now". An exam answer without `examPenalties` has none: `Some` of an empty list.
    pub server: Option<Vec<ServerAction>>,
    /// The teacher got angry at this answer: his face for 8 s and his angry line, queued after
    /// the question's sentence. The wipe itself is in `server`, when there is a server.
    pub angry: Option<Wipe>,
}

pub struct Teacher {
    pub pack: Pack,
    pub config: Config,
    pub progress: Progress,
    /// The pack's `dictionary.json`, empty until the game layer sets it.
    pub dictionary: Dictionary,
    speaker: Speaker,
    detector: Detector,
    quiz: Quiz,
    portrait: Portrait,
    rng: Rng,
    held_label: Option<(Phrase, i64)>,
    last_dimension: Option<String>,
    last_world_key: Option<String>,
    /// Built on the first typed answer.
    typed_words: Option<PackWords>,
    integrated_server: bool,
    hardcore: bool,
}

impl Teacher {
    pub fn new(pack: Pack, config: Config, progress: Progress, seed: u64) -> Teacher {
        Teacher {
            pack,
            config,
            progress,
            dictionary: Dictionary::default(),
            speaker: Speaker::default(),
            detector: Detector::default(),
            quiz: Quiz::default(),
            portrait: Portrait::default(),
            rng: Rng::new(seed),
            held_label: None,
            last_dimension: None,
            last_world_key: None,
            typed_words: None,
            integrated_server: false,
            hardcore: false,
        }
    }

    /// A resource reload built this teacher from the pack again: he goes on where `previous`
    /// stood, as in the original, which rebuilds nothing on a reload that keeps the pack's
    /// language (`ZhNarratorClient.onLanguagePacksScanned`). He keeps the quiz with its open
    /// question and its exam, what he was about to say, the trackers, and the world and dimension
    /// he is in, so the next join of the same level is no new world. The clip playing is cut, as
    /// a reload stops the game's sounds; the rest of the queue follows.
    pub fn carry_on(&mut self, previous: &mut Teacher, now: i64) {
        self.quiz = std::mem::take(&mut previous.quiz);
        self.speaker = std::mem::take(&mut previous.speaker);
        self.speaker.cut(now);
        self.detector = std::mem::take(&mut previous.detector);
        self.portrait = std::mem::take(&mut previous.portrait);
        self.held_label = previous.held_label.take();
        self.last_dimension = previous.last_dimension.take();
        self.last_world_key = previous.last_world_key.take();
        self.integrated_server = previous.integrated_server;
        self.hardcore = previous.hardcore;
    }

    /// The level object changed: a world was joined, or the dimension changed. Says the new
    /// dimension when it differs from the last level's, resets the trackers, switches the
    /// progress world when the key changed, and restarts the quiz interval in a new world or
    /// dimension.
    pub fn joined_level(&mut self, now: i64, world_key: &str, dimension: &str, held_item: &str) {
        let new_key = self.last_world_key.as_deref() != Some(world_key);
        let new_world = new_key || self.last_dimension.as_deref() != Some(dimension);
        if let Some(last) = &self.last_dimension
            && path_of(last) != path_of(dimension)
        {
            let id = match path_of(dimension) {
                "the_nether" => "dim_nether",
                "the_end" => "dim_end",
                _ => "dim_overworld",
            };
            if let Some(phrase) = self.pack.phrase(id) {
                self.speaker.say_now(phrase, false, &self.config, now);
            }
        }
        self.last_dimension = Some(dimension.to_string());
        self.detector.reset(held_item, now);
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

    /// A main-hand right click on a block, not sneaking: says its close, open or use phrase.
    /// False when it has none, and the click may place a block.
    pub fn block_used(&mut self, now: i64, block: &str, is_open: bool) -> bool {
        let Some(speech) = self
            .detector
            .block_used(block, is_open, now, &self.pack, &self.config)
        else {
            return false;
        };
        self.apply(speech, now);
        true
    }

    /// The player attacked entity `id` (`entity` is its type id; a sheep is `<colour>_sheep`).
    /// `living`: a living entity, then watched 6 s for the kill; `dying`: it is dying already,
    /// and nothing is said.
    pub fn entity_attacked(&mut self, now: i64, id: u64, entity: &str, living: bool, dying: bool) {
        let speech = self
            .detector
            .entity_attacked(id, entity, living, dying, now, &self.pack);
        self.apply(speech, now);
    }

    /// A main-hand right click on an entity, holding `held_item`; `held_is_food` when it has the
    /// FOOD component.
    pub fn entity_used(&mut self, now: i64, entity: &str, held_item: &str, held_is_food: bool) {
        let speech = self.detector.entity_used(
            entity,
            held_item,
            held_is_food,
            now,
            &self.pack,
            &self.config,
        );
        self.apply(speech, now);
    }

    /// The entities watched for a kill: report those dying or gone in
    /// [`WorldTick::entity_fates`].
    pub fn watched_entities(&self) -> impl Iterator<Item = u64> + '_ {
        self.detector.watched()
    }

    /// One client tick in a world, not paused: narration for the tick's state, then the quiz.
    pub fn world_tick(&mut self, input: &WorldTick<'_>) -> Option<QuizEvent> {
        self.integrated_server = input.integrated_server;
        self.hardcore = input.hardcore;
        let speech = self.detector.tick(input, &self.pack, &self.config);
        self.apply(speech, input.now);
        let event = self.quiz.tick(
            input,
            &self.pack,
            &self.config,
            &self.progress,
            &mut self.rng,
        );
        match &event {
            Some(QuizEvent::Ask | QuizEvent::Resume) => self.say_if_listening(input.now),
            Some(QuizEvent::ExamOver(result)) => self.progress.exam_done(result, input.now),
            _ => {}
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

    /// The speed key: the pack's next speed. The next clips play its sound events. Returns the
    /// speed now.
    pub fn cycle_speed(&mut self) -> &str {
        let speeds = &self.pack.speeds;
        if !speeds.is_empty() {
            let current = self.pack.speed(&self.config.speed);
            let next = speeds
                .iter()
                .position(|speed| speed == current)
                .map_or(0, |index| (index + 1) % speeds.len());
            self.config.speed = speeds[next].clone();
        }
        &self.config.speed
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

    /// The question screen's tick: a choice answers wrong at its time-out, a typed question
    /// submits what [`Teacher::set_typed`] last gave.
    pub fn question_tick(&mut self, now: i64) -> Option<Outcome> {
        let typing = self
            .quiz
            .open()
            .is_some_and(|open| open.kind == QuestionKind::Typing);
        let answer = if typing {
            let words = words(&mut self.typed_words, &self.pack);
            self.quiz.screen_tick(now, words)
        } else {
            self.quiz.screen_tick(now, &PackWords::default())
        }?;
        Some(self.answered(answer, now))
    }

    /// The typed question's edit box changed. What it holds is submitted at the time-out and
    /// kept while the question is suspended ([`OpenQuestion::typed`]).
    pub fn set_typed(&mut self, typed: &str) {
        self.quiz.set_typed(typed);
    }

    /// Enter or the Check button on a typed question. `None` in the 400 ms lockout and with an
    /// empty box.
    pub fn submit_typed(&mut self, now: i64, typed: &str) -> Option<Outcome> {
        let words = words(&mut self.typed_words, &self.pack);
        let answer = self.quiz.submit_typed(typed, now, words)?;
        Some(self.answered(answer, now))
    }

    /// Esc on the question screen: the question waits with its time left while the pause menu
    /// is open, and [`Teacher::world_tick`] brings it back once no screen is open.
    pub fn suspend_question(&mut self, now: i64) -> bool {
        self.quiz.suspend(now)
    }

    /// The exam page's Start: an exam whose questions start at once, about everything that came up
    /// in this world. It replaces any exam running.
    pub fn start_exam(&mut self, now: i64) {
        let exam = self.quiz.exam_mut();
        exam.start(self.progress.world(), &self.config, false, now);
    }

    /// The exam page's Stop. False when no exam runs. Else the exam is dropped and `examAuto` is
    /// off: save the config and say `learnordie.exam.stopped`.
    pub fn stop_exam(&mut self) -> bool {
        if self.quiz.exam().is_off() {
            return false;
        }
        self.quiz.exam_mut().stop();
        self.config.exam_auto = false;
        true
    }

    /// Where the exam stands: the exam page's Start or Stop button, and a question's header
    /// (`learnordie.exam.header`) while it asks.
    pub fn exam(&self, now: i64) -> ExamStage {
        self.quiz.exam().stage(now)
    }

    /// The HUD's exam line, while the teacher is on: the countdown (`learnordie.exam.timer`), or
    /// between the exam's questions the one that comes next (`learnordie.exam.timer_asking`).
    pub fn exam_hud(&self, now: i64) -> Option<ExamStage> {
        if !self.config.on() {
            return None;
        }
        match self.exam(now) {
            ExamStage::Off => None,
            ExamStage::Asking { number, total } if number > total || self.quiz.open().is_some() => {
                None
            }
            stage => Some(stage),
        }
    }

    /// The "what is this" key: says the word for the block or entity under the crosshair
    /// (`word.<key>`) at once, even while he is muted. False when the pack has no word for it or
    /// the teacher is off.
    pub fn what_is_this(&mut self, now: i64, looked: Looked<'_>) -> bool {
        let word = match looked {
            Looked::Block(id) => self.pack.for_block("word", id),
            Looked::Entity(id) => self.pack.for_entity("word", id),
        };
        let Some(word) = word.cloned() else {
            return false;
        };
        if !self.config.on() {
            return false;
        }
        self.speaker.say_now(&word, true, &self.config, now);
        true
    }

    /// The second line of an item's tooltip, while `tooltipChinese` is on: the item's noun with
    /// its article (its `hold` phrase without the sentence end, then its reading when it has one).
    /// Drawn in `0xFFE08A` in the pack's font. Shown with the teacher off too.
    pub fn tooltip_line(&self, item: &str) -> Option<String> {
        if !self.config.tooltip_chinese || item.is_empty() || path_of(item) == "air" {
            return None;
        }
        let phrase = self.pack.for_item("hold", item)?;
        Some(if phrase.reading.is_empty() {
            phrase.bare().to_string()
        } else {
            format!("{}  {}", phrase.bare(), phrase.reading)
        })
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
        self.portrait.frame(
            now,
            &self.pack.meta.sprite,
            mouth,
            speaking,
            self.quiz.hop_at(),
            self.quiz.angry(now),
            self.quiz.frowning(now),
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
                Speech::Now(phrase) => self.speaker.say_now(&phrase, false, &self.config, now),
                Speech::DropQueued(id) => self.speaker.drop_queued(&id),
                Speech::HeldLabel(phrase) => self.held_label = Some((phrase, now)),
                Speech::Counted {
                    display,
                    clips,
                    joined: true,
                    fresh,
                } => {
                    if let [start, noun] = clips.as_slice() {
                        self.speaker
                            .say_joined(&display, start, noun, fresh, &self.config, now);
                    }
                }
                Speech::Counted {
                    display,
                    clips,
                    joined: false,
                    fresh,
                } => {
                    self.speaker.say_sequence(
                        &display,
                        &clips,
                        fresh,
                        &self.config,
                        &self.progress,
                        now,
                    );
                }
                Speech::Died => self.progress.record_death(self.hardcore, &self.config),
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

    /// Records the answer, says the learned line when it made the phrase learned, decides the
    /// consequences, says the right sentence at once, and counts the answer toward the angry
    /// teacher, whose line comes after it. An exam answer without `examPenalties` has no
    /// consequences, and a wrong one does not count toward the angry teacher.
    fn answered(&mut self, answer: Answer, now: i64) -> Outcome {
        let Answer {
            question,
            correct,
            typed,
            consequences,
        } = answer;
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
        let mut server = if consequences {
            consequences::decide(
                correct,
                self.integrated_server,
                self.hardcore,
                &self.config,
                &mut self.progress,
                &mut self.rng,
            )
        } else {
            Some(Vec::new())
        };
        self.speaker.say_now(&question, false, &self.config, now);
        let counts = correct || consequences;
        let angry = (counts && self.quiz.angry_after(correct, &self.config, now)).then(|| {
            if self.config.angry_teacher == "drop" {
                Wipe::Drop
            } else {
                Wipe::Delete
            }
        });
        if let Some(wipe) = angry {
            if let Some(phrase) = self.pack.phrase("angry") {
                self.speaker.say(phrase, &self.config, &self.progress, now);
            }
            if let Some(actions) = &mut server {
                actions.push(ServerAction::Wipe(wipe));
            }
        }
        Outcome {
            question,
            correct,
            learned,
            typed: typed.map(|(text, verdict)| Typed { text, verdict }),
            server,
            angry,
        }
    }

    /// Drains the speaker's queue, for the core's tests.
    #[cfg(test)]
    pub(crate) fn take_queued(&mut self) -> Vec<String> {
        self.speaker.take_queued()
    }
}

/// The pack's words for judging typed answers, built once.
fn words<'a>(cache: &'a mut Option<PackWords>, pack: &Pack) -> &'a PackWords {
    cache.get_or_insert_with(|| PackWords::new(pack.phrases().iter().map(|p| p.text.as_str())))
}
