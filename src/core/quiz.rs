//! The quiz (Java `QuizManager`, `QuizScreen` and `TypingScreen`): when a question is due, which
//! phrase is asked, as a choice of four or to type, and the clock of the open question.
//!
//! Modes: `choice` asks with buttons, `typing` asks to type, `mixed` rolls one in three for
//! typing. Voice answers are not ported, so `speech`, and the voice third of `mixed`, ask with
//! buttons. While an exam asks its questions they take the place of normal ones; see
//! [`super::exam`].
//!
//! Schedule: a question comes every `quizIntervalSeconds` of play. Time with a screen open, or a
//! tick gap over a second, does not count. Nothing is asked before the teacher said something in
//! this world, and the first question comes a full interval after that. The player must be calm;
//! in singleplayer with `quizPauseGame` most reasons are waived, but never `afk`, `dead` or `busy`.
//!
//! Clock: a choice has `quizAnswerSeconds` and ignores answers for the first 700 ms; time-out
//! counts as wrong. A typed question has `max(20, 2 x quizAnswerSeconds)` and ignores input for
//! the first 400 ms; an empty submit does nothing, and time-out submits what was typed. Esc
//! suspends the question with the time left (at least 1 s) and what was typed; when no screen is
//! open any more it comes back with them and a new lockout.
use super::config::Config;
use super::exam::{self, Exam, ExamResult};
use super::pack::{Pack, Phrase};
use super::progress::Progress;
use super::rng::Rng;
use super::teacher::WorldTick;
use super::typed::{self, PackWords, Verdict, java_strip, java_whitespace};

const CALM_AFTER_DAMAGE_MS: i64 = 5000;
const AFK_AFTER_MS: i64 = 20_000;
const BUSY_MS: i64 = 1500;
const PICK_RETRY_MS: i64 = 1000;
const ACCEPT_AFTER_MS: i64 = 700;
const TYPED_ACCEPT_AFTER_MS: i64 = 400;
const MIN_TYPING_SECONDS: f32 = 20.0;
const ANGRY_MS: i64 = 8000;
const MIN_SUSPENDED_MS: i64 = 1000;
const FROWN_MS: i64 = 2500;
const OPTIONS: usize = 4;
const NOUNS_ONLY_CHANCE: f64 = 0.4;

/// What the game layer does after a quiz tick.
#[derive(Clone, Debug, PartialEq)]
pub enum QuizEvent {
    /// A new question: play the ding and open the question screen.
    Ask,
    /// The suspended question is back: open the question screen again.
    Resume,
    /// The question screen was replaced by another one: the question is dropped, no penalty.
    Dropped,
    /// The exam's questions begin: say `learnordie.exam.begin`. The first comes 1.5 s later.
    ExamBegins { questions: usize },
    /// The exam's time is up, but nothing came up to ask about: say so. The exam is off.
    ExamEmpty,
    /// The exam is over: say `learnordie.exam.result` and show the result screen, which pauses
    /// the game.
    /// Progress has the result already.
    ExamOver(ExamResult),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Clock {
    Showing {
        deadline: i64,
        accept_after: i64,
        total_ms: i64,
    },
    Suspended {
        remaining_ms: i64,
    },
}

impl Clock {
    fn showing(now: i64, total_ms: i64, kind: QuestionKind) -> Clock {
        let lockout = match kind {
            QuestionKind::Choice => ACCEPT_AFTER_MS,
            QuestionKind::Typing => TYPED_ACCEPT_AFTER_MS,
        };
        Clock::Showing {
            deadline: now + total_ms,
            accept_after: now + lockout,
            total_ms,
        }
    }
}

/// How the question is answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionKind {
    /// Four options, keys 1 to 4 or a click.
    Choice,
    /// An edit box and a Check button; Enter submits.
    Typing,
}

/// The question being asked, with its options in screen order (keys 1 to 4); a typed question
/// has none.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenQuestion {
    pub question: Phrase,
    pub options: Vec<Phrase>,
    /// The English prompt is hidden and the phrase is played instead: its `en` is blank or the
    /// same as its text.
    pub listening: bool,
    pub kind: QuestionKind,
    typed: String,
    clock: Clock,
}

impl OpenQuestion {
    /// Suspended by Esc: the screen is closed until the question comes back.
    pub fn is_suspended(&self) -> bool {
        matches!(self.clock, Clock::Suspended { .. })
    }

    /// What the edit box holds: kept while the question is suspended, to fill the box again.
    pub fn typed(&self) -> &str {
        &self.typed
    }

    /// Past the lockout and showing: keys, characters and clicks count.
    pub fn accepts_input(&self, now: i64) -> bool {
        matches!(self.clock, Clock::Showing { accept_after, .. } if now >= accept_after)
    }

    /// Time left to answer, in ms.
    pub fn left_ms(&self, now: i64) -> i64 {
        match self.clock {
            Clock::Showing { deadline, .. } => (deadline - now).max(0),
            Clock::Suspended { remaining_ms } => remaining_ms,
        }
    }

    /// The timer bar: the share of this screen's time that is left, from 1 to 0.
    pub fn fraction_left(&self, now: i64) -> f32 {
        match self.clock {
            Clock::Showing { total_ms, .. } if total_ms > 0 => {
                (self.left_ms(now) as f32 / total_ms as f32).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }
}

/// A finished question, for the teacher to record and say.
pub(crate) struct Answer {
    pub(crate) question: Phrase,
    pub(crate) correct: bool,
    /// A typed answer: what was typed (stripped) and its verdict.
    pub(crate) typed: Option<(String, Verdict)>,
    /// Rewards and penalties apply: always, but for an exam answer only with its penalties.
    pub(crate) consequences: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Quiz {
    last_quiz_at: i64,
    last_tick_at: i64,
    last_damage_at: i64,
    last_active_at: i64,
    last_health: Option<f32>,
    last_position: [f64; 3],
    last_yaw: f32,
    last_pitch: f32,
    busy_until: i64,
    nothing_to_ask: bool,
    last_pick_try_at: i64,
    open: Option<OpenQuestion>,
    given: String,
    hop_at: i64,
    frown_until: i64,
    wrong_streak: i32,
    angry_until: i64,
    exam: Exam,
}

impl Quiz {
    /// A new world or dimension: the interval starts again, and any open question and any exam
    /// are dropped.
    pub(crate) fn on_joined_world(&mut self, now: i64, progress: &Progress) {
        self.last_quiz_at = now;
        self.last_tick_at = now;
        self.nothing_to_ask = progress.world().heard.is_empty();
        self.last_pick_try_at = 0;
        self.open = None;
        self.exam.stop();
    }

    pub(crate) fn exam(&self) -> &Exam {
        &self.exam
    }

    pub(crate) fn exam_mut(&mut self) -> &mut Exam {
        &mut self.exam
    }

    pub(crate) fn open(&self) -> Option<&OpenQuestion> {
        self.open.as_ref()
    }

    /// When the last answer was right, for the portrait's hop.
    pub(crate) fn hop_at(&self) -> i64 {
        self.hop_at
    }

    /// The teacher frowns for 2.5 s after a wrong answer.
    pub(crate) fn frowning(&self, now: i64) -> bool {
        now < self.frown_until
    }

    /// The angry face shows for 8 s after the angry teacher struck.
    pub(crate) fn angry(&self, now: i64) -> bool {
        now < self.angry_until
    }

    /// Counts an answer that had consequences toward the angry teacher: `angryAfterWrong` wrong
    /// answers in a row, unless `angryTeacher` is `off`. A right one starts the count again.
    /// True when he gets angry now.
    pub(crate) fn angry_after(&mut self, correct: bool, config: &Config, now: i64) -> bool {
        if correct {
            self.wrong_streak = 0;
            return false;
        }
        self.wrong_streak += 1;
        if config.angry_teacher != "off" && self.wrong_streak >= config.angry_after_wrong.max(1) {
            self.wrong_streak = 0;
            self.angry_until = now + ANGRY_MS;
            return true;
        }
        false
    }

    /// One client tick in a world, not paused.
    pub(crate) fn tick(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
        progress: &Progress,
        rng: &mut Rng,
    ) -> Option<QuizEvent> {
        let now = input.now;
        let gap = if self.last_tick_at == 0 {
            0
        } else {
            now - self.last_tick_at
        };
        self.last_tick_at = now;
        if gap > 0 && (input.screen_open || gap > 1000) {
            self.last_quiz_at += gap;
            self.exam.add_back(gap, now);
        }
        if let Some(last) = self.last_health
            && input.health < last - 0.5
        {
            self.last_damage_at = now;
        }
        self.last_health = Some(input.health);
        let moved: f64 = input
            .position
            .iter()
            .zip(self.last_position)
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
        if moved > 1.0e-4 || input.yaw != self.last_yaw || input.pitch != self.last_pitch {
            self.last_active_at = now;
        }
        self.last_position = input.position;
        self.last_yaw = input.yaw;
        self.last_pitch = input.pitch;

        let clock = self.open.as_ref().map(|open| open.clock);
        match clock {
            Some(Clock::Suspended { remaining_ms }) => {
                if input.screen_open {
                    return None;
                }
                if let Some(open) = &mut self.open {
                    open.clock = Clock::showing(now, remaining_ms, open.kind);
                }
                return Some(QuizEvent::Resume);
            }
            Some(Clock::Showing { .. }) => {
                if input.question_screen_open {
                    return None;
                }
                self.open = None;
                self.last_quiz_at = now;
                return Some(QuizEvent::Dropped);
            }
            None => {}
        }

        if !config.on() || input.screen_open {
            return None;
        }
        if let Some(event) = self.exam_tick(pack, config, progress, rng, now) {
            return Some(event);
        }
        let asking = self.exam.asking();
        if asking {
            if self.exam.current().is_none() || now - self.last_quiz_at < exam::GAP_MS {
                return None;
            }
        } else {
            if config.quiz_mode == "off" {
                return None;
            }
            if self.nothing_to_ask {
                if now - self.last_pick_try_at < PICK_RETRY_MS {
                    return None;
                }
                self.last_pick_try_at = now;
                if has_candidates(pack, config, progress, now) {
                    self.nothing_to_ask = false;
                    self.last_quiz_at = now;
                }
                return None;
            }
            if ((now - self.last_quiz_at) as f32) < config.quiz_interval_seconds * 1000.0 {
                return None;
            }
        }
        if let Some(reason) = self.calm_reason(input, now) {
            let asked_anyway = config.quiz_pause_game
                && input.singleplayer
                && !matches!(reason, "afk" | "dead" | "busy");
            if !asked_anyway {
                return None;
            }
        }
        let typing = if asking {
            self.exam.kind(rng) == QuestionKind::Typing
        } else {
            match config.quiz_mode.as_str() {
                "typing" => true,
                "mixed" => rng.below(3) == 1,
                _ => false,
            }
        };
        let question = if asking {
            self.exam.current().cloned()
        } else {
            pick(pack, config, progress, now, rng)
        };
        let Some(question) = question else {
            self.nothing_to_ask = true;
            self.last_pick_try_at = now;
            return None;
        };
        self.exam.on_asked(&question.id);
        let (kind, options, seconds) = if typing {
            let seconds = MIN_TYPING_SECONDS.max(config.quiz_answer_seconds * 2.0);
            (QuestionKind::Typing, Vec::new(), seconds)
        } else {
            let options: Vec<Phrase> = choose_options(pack, &question, rng)
                .into_iter()
                .cloned()
                .collect();
            (QuestionKind::Choice, options, config.quiz_answer_seconds)
        };
        self.given.clear();
        self.last_quiz_at = now;
        let total_ms = (seconds * 1000.0) as i64;
        self.open = Some(OpenQuestion {
            listening: question.en.trim().is_empty() || question.en == question.text,
            question,
            options,
            kind,
            typed: String::new(),
            clock: Clock::showing(now, total_ms, kind),
        });
        Some(QuizEvent::Ask)
    }

    /// The exam's own steps, before any question: an automatic exam dropped when `examAuto` went
    /// off, one started when none runs, the questions begun when the countdown is over, and the
    /// result 2.5 s after the last answer.
    fn exam_tick(
        &mut self,
        pack: &Pack,
        config: &Config,
        progress: &Progress,
        rng: &mut Rng,
        now: i64,
    ) -> Option<QuizEvent> {
        if self.exam.playing() && self.exam.auto() && !config.exam_auto {
            self.exam.stop();
        }
        if self.exam.is_off() && config.exam_auto && config.quiz_mode != "off" {
            self.exam.start(progress.world(), config, true, now);
        }
        if self.exam.due(now) {
            let questions = self.exam.begin(pack, progress.world(), rng);
            if questions == 0 {
                return Some(QuizEvent::ExamEmpty);
            }
            self.last_quiz_at = now - exam::GAP_MS + exam::FIRST_AFTER_MS;
            return Some(QuizEvent::ExamBegins { questions });
        }
        if self.exam.asking()
            && self.exam.current().is_none()
            && now - self.last_quiz_at >= exam::GAP_MS
        {
            self.last_quiz_at = now;
            return Some(QuizEvent::ExamOver(self.exam.finish()));
        }
        None
    }

    /// Why the player is not calm, if they are not: `dead`, `busy` (mining, attacking or using an
    /// item, and 1.5 s after), `hurt` (damage in the last 5 s), `sleep`, `vehicle`, `water`, `air`
    /// or `afk` (no move or look for 20 s).
    fn calm_reason(&mut self, input: &WorldTick<'_>, now: i64) -> Option<&'static str> {
        if input.dead {
            return Some("dead");
        }
        if input.breaking.is_some() || input.attack_down || input.using_item {
            self.busy_until = now + BUSY_MS;
            return Some("busy");
        }
        if now < self.busy_until {
            Some("busy")
        } else if now - self.last_damage_at <= CALM_AFTER_DAMAGE_MS {
            Some("hurt")
        } else if input.sleeping {
            Some("sleep")
        } else if input.in_vehicle {
            Some("vehicle")
        } else if input.in_water {
            Some("water")
        } else if !input.on_ground || input.fall_flying {
            Some("air")
        } else if now - self.last_active_at >= AFK_AFTER_MS {
            Some("afk")
        } else {
            None
        }
    }

    /// Picks option `index` (key `index + 1`). Ignored in the lockout and while suspended.
    pub(crate) fn choose(&mut self, index: usize, now: i64) -> Option<Answer> {
        let open = self.open.as_ref()?;
        let Clock::Showing { accept_after, .. } = open.clock else {
            return None;
        };
        if now < accept_after {
            return None;
        }
        let option = open.options.get(index)?;
        let correct = option.id == open.question.id;
        self.given = option.bare().to_string();
        self.finish(correct, None, now)
    }

    /// The edit box changed: what it holds is kept for a time-out and for Esc.
    pub(crate) fn set_typed(&mut self, typed: &str) {
        if let Some(open) = &mut self.open
            && open.kind == QuestionKind::Typing
        {
            open.typed = typed.to_string();
        }
    }

    /// Enter or the Check button on a typed question. Ignored in the lockout, while suspended and
    /// with an empty box.
    pub(crate) fn submit_typed(
        &mut self,
        typed: &str,
        now: i64,
        words: &PackWords,
    ) -> Option<Answer> {
        let open = self.open.as_ref()?;
        if open.kind != QuestionKind::Typing
            || !open.accepts_input(now)
            || typed.chars().all(java_whitespace)
        {
            return None;
        }
        self.judge_typed(typed, now, words)
    }

    fn judge_typed(&mut self, typed: &str, now: i64, words: &PackWords) -> Option<Answer> {
        let text = &self.open.as_ref()?.question.text;
        let verdict = typed::judge(typed, text, words);
        let given = java_strip(typed).to_string();
        self.given = given.clone();
        self.finish(verdict.is_right(), Some((given, verdict)), now)
    }

    /// The screen's tick: a choice answers wrong at its time-out, a typed question submits what
    /// was typed. `words` judges a typed answer.
    pub(crate) fn screen_tick(&mut self, now: i64, words: &PackWords) -> Option<Answer> {
        let open = self.open.as_ref()?;
        let Clock::Showing { deadline, .. } = open.clock else {
            return None;
        };
        if now < deadline {
            return None;
        }
        match open.kind {
            QuestionKind::Choice => self.finish(false, None, now),
            QuestionKind::Typing => {
                let typed = open.typed.clone();
                self.judge_typed(&typed, now, words)
            }
        }
    }

    /// Esc: suspends the question with the time left. False when no question is showing.
    pub(crate) fn suspend(&mut self, now: i64) -> bool {
        let Some(open) = &mut self.open else {
            return false;
        };
        let Clock::Showing { deadline, .. } = open.clock else {
            return false;
        };
        open.clock = Clock::Suspended {
            remaining_ms: (deadline - now).max(MIN_SUSPENDED_MS),
        };
        true
    }

    fn finish(
        &mut self,
        correct: bool,
        typed: Option<(String, Verdict)>,
        now: i64,
    ) -> Option<Answer> {
        let open = self.open.take()?;
        self.last_quiz_at = now;
        self.last_tick_at = now;
        let consequences = self.exam.on_answer(&open.question, correct, &self.given);
        if !self.given.is_empty() {
            self.last_active_at = now;
        }
        self.given.clear();
        if correct {
            self.hop_at = now;
        } else {
            self.frown_until = now + FROWN_MS;
        }
        Some(Answer {
            question: open.question,
            correct,
            typed,
            consequences,
        })
    }
}

/// Phrases that may be asked, with their weights, and the fallback: the recently asked phrase
/// asked longest ago. A candidate was heard in this world, is not a fragment and is not resting.
/// Phrases asked within `quizRetryAfterSeconds` are only the fallback; the others weigh
/// `(1 + 2 wrong) / (1 + correct) * min(heard, 5)`.
fn pools<'a>(
    pack: &'a Pack,
    config: &Config,
    progress: &Progress,
    now: i64,
) -> (Vec<(&'a Phrase, f64)>, Option<&'a Phrase>) {
    let world = progress.world();
    let mut fresh = Vec::new();
    let mut recent: Option<(&Phrase, i64)> = None;
    for phrase in pack.phrases() {
        let heard = world.heard.get(&phrase.id).copied().unwrap_or(0);
        if phrase.is_fragment() || heard == 0 || progress.rest_left(&phrase.id, now, config) > 0 {
            continue;
        }
        let since = progress.since_asked_ms(&phrase.id, now);
        if (since as f32) < config.quiz_retry_after_seconds * 1000.0 {
            if recent.is_none_or(|(_, longest)| since > longest) {
                recent = Some((phrase, since));
            }
        } else {
            let correct = progress.correct(&phrase.id);
            let wrong = progress.asked(&phrase.id) - correct;
            let weight =
                f64::from(1 + wrong * 2) / f64::from(1 + correct) * f64::from(heard.min(5));
            fresh.push((phrase, weight));
        }
    }
    (fresh, recent.map(|(phrase, _)| phrase))
}

fn has_candidates(pack: &Pack, config: &Config, progress: &Progress, now: i64) -> bool {
    let (fresh, recent) = pools(pack, config, progress, now);
    !fresh.is_empty() || recent.is_some()
}

/// The question: a weighted pick from the candidates. When they mix nouns (`hold`) and sentences,
/// 40% of picks look at the nouns only.
fn pick(
    pack: &Pack,
    config: &Config,
    progress: &Progress,
    now: i64,
    rng: &mut Rng,
) -> Option<Phrase> {
    let (mut pool, recent) = pools(pack, config, progress, now);
    if pool.is_empty() {
        return recent.cloned();
    }
    let nouns = pool
        .iter()
        .filter(|(phrase, _)| phrase.action == "hold")
        .count();
    if nouns > 0 && nouns < pool.len() && rng.next_f64() < NOUNS_ONLY_CHANCE {
        pool.retain(|(phrase, _)| phrase.action == "hold");
    }
    let total: f64 = pool.iter().map(|(_, weight)| weight).sum();
    let mut roll = rng.next_f64() * total;
    for (phrase, weight) in &pool {
        roll -= weight;
        if roll <= 0.0 {
            return Some(Phrase::clone(phrase));
        }
    }
    pool.last().map(|(phrase, _)| Phrase::clone(phrase))
}

/// Four options with distinct texts, the question among them, in random order. For a sentence
/// about a thing, first other sentences about the same thing (so the verb decides), then phrases
/// with the same action, then any phrase that is not a fragment.
pub(crate) fn choose_options<'a>(
    pack: &'a Pack,
    question: &'a Phrase,
    rng: &mut Rng,
) -> Vec<&'a Phrase> {
    let mut chosen = vec![question];
    if let Some(key) = &question.key
        && question.action != "hold"
        && !question.is_fragment()
    {
        let mut same_thing: Vec<&Phrase> = pack
            .phrases()
            .iter()
            .filter(|p| {
                p.key.as_ref() == Some(key)
                    && p.action != question.action
                    && p.action != "hold"
                    && !p.is_fragment()
            })
            .collect();
        rng.shuffle(&mut same_thing);
        add_distinct(&mut chosen, &same_thing, false);
    }
    let mut same_action: Vec<&Phrase> = pack
        .by_action(&question.action)
        .filter(|p| p.id != question.id)
        .collect();
    rng.shuffle(&mut same_action);
    add_distinct(&mut chosen, &same_action, false);
    if chosen.len() < OPTIONS {
        let mut any: Vec<&Phrase> = pack.phrases().iter().collect();
        rng.shuffle(&mut any);
        add_distinct(&mut chosen, &any, true);
    }
    rng.shuffle(&mut chosen);
    chosen
}

/// Adds candidates until there are four options, skipping any whose text is already an option's.
fn add_distinct<'a>(chosen: &mut Vec<&'a Phrase>, candidates: &[&'a Phrase], skip_fragments: bool) {
    for &candidate in candidates {
        if chosen.len() == OPTIONS {
            break;
        }
        if skip_fragments && candidate.is_fragment() {
            continue;
        }
        if chosen.iter().all(|option| option.text != candidate.text) {
            chosen.push(candidate);
        }
    }
}
