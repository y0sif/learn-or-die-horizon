//! The exam (Java `Exam`, and the exam parts of `QuizManager`): a countdown of `examMinutes` of
//! play, then up to 20 questions about the phrases that came up most, and the result.
//!
//! Stages: off, counting down, asking. With `examAuto` an exam starts on its own whenever none
//! runs and the quiz is not off. The countdown counts play only: a tick gap with a screen open,
//! or over a second, is added back, never more than the exam has run. Started by hand, it asks at
//! once. Stopping an exam turns `examAuto` off; switching `examAuto` off drops a coming automatic
//! exam. A new world or dimension drops any exam.
//!
//! Picking: every phrase that is not a fragment, by the times it came up (heard since the start,
//! plus asked as a quiz question during the countdown), ties by its wrong answers during the
//! countdown, the rest of a tie in random order, one phrase per text, up to 20. An exam started by
//! hand has no baseline: every hearing in this world counts.
//!
//! Asking: the first question 1.5 s after the questions begin, then one 2.5 s after each answer,
//! each by a kind picked at random among the enabled ones (buttons, typing; voice answers are not
//! ported). Normal questions wait meanwhile; during the countdown they come as usual. An exam
//! answer has consequences only with `examPenalties`, as it was when the exam started. The result
//! comes 2.5 s after the last answer.
use super::config::Config;
use super::pack::{Pack, Phrase};
use super::progress::World;
use super::quiz::QuestionKind;
use super::rng::Rng;
use super::typed::java_whitespace;
use std::collections::{BTreeMap, HashMap, HashSet};

/// The most questions an exam asks.
pub(crate) const QUESTIONS: usize = 20;
/// Between an answer and the next question, and after the last answer before the result.
pub(crate) const GAP_MS: i64 = 2500;
/// The first question comes this long after the questions begin.
pub(crate) const FIRST_AFTER_MS: i64 = 1500;

/// Where the exam stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExamStage {
    Off,
    /// Counting down: the questions start in `left`, the original's clock (`m:ss`, whole seconds
    /// rounded up).
    Playing {
        left: String,
    },
    /// Asking: question `number` of `total` comes next. Past `total`, every question is answered
    /// and the result comes 2.5 s after the last answer.
    Asking {
        number: usize,
        total: usize,
    },
}

/// One question of a finished exam.
#[derive(Clone, Debug, PartialEq)]
pub struct ExamRow {
    pub phrase: Phrase,
    pub correct: bool,
    /// What was given: the option's text or what was typed; empty after a time-out.
    pub given: String,
}

impl ExamRow {
    /// The translation shown on the row: the phrase's `en` without one final `.` or `!`.
    pub fn translation(&self) -> &str {
        let en = &self.phrase.en;
        en.strip_suffix(['.', '!']).unwrap_or(en)
    }

    /// The note under a wrong row: the answer given (`learnordie.exam.your_answer`), or `None`
    /// for `learnordie.exam.no_answer` when it is blank.
    pub fn answer(&self) -> Option<&str> {
        (!self.given.chars().all(java_whitespace)).then_some(self.given.as_str())
    }
}

/// A finished exam, for the chat line (`learnordie.exam.result`) and the result screen
/// (`learnordie.exam.result_title`). Progress has it already.
#[derive(Clone, Debug, PartialEq)]
pub struct ExamResult {
    /// The countdown's minutes; 0 for an exam started by hand.
    pub minutes: f32,
    /// The kinds asked, as the progress file names them: `choice`, `typing` or `choice+typing`.
    pub kinds: String,
    /// One row per question, in the order asked.
    pub rows: Vec<ExamRow>,
}

impl ExamResult {
    pub fn right(&self) -> usize {
        self.rows.iter().filter(|row| row.correct).count()
    }

    pub fn total(&self) -> usize {
        self.rows.len()
    }

    /// 70% right or more: the result screen's title is green (`0xFF9BE08A`), else gold
    /// (`0xFFFFE08A`).
    pub fn passed(&self) -> bool {
        self.right() * 10 >= self.total() * 7
    }
}

/// The original's clock: minutes and seconds, `m:ss`, the seconds rounded up.
pub(crate) fn clock(ms: i64) -> String {
    let seconds = ((ms + 999) / 1000).max(0);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Off,
    Playing,
    Asking,
}

#[derive(Debug, Default)]
pub(crate) struct Exam {
    stage: Stage,
    ends_at: i64,
    started_at: i64,
    minutes: f32,
    kinds: Vec<QuestionKind>,
    auto: bool,
    penalties: bool,
    heard_at_start: BTreeMap<String, i32>,
    asked: HashMap<String, i32>,
    wrong: HashMap<String, i32>,
    queue: Vec<Phrase>,
    rows: Vec<ExamRow>,
}

impl Exam {
    /// `Exam.start`: an automatic exam counts `examMinutes` down from now, with this world's
    /// hearings so far as its baseline; one started by hand asks at once, with no baseline.
    pub(crate) fn start(&mut self, world: &World, config: &Config, auto: bool, now: i64) {
        self.stop();
        self.stage = Stage::Playing;
        self.minutes = if auto { config.exam_minutes } else { 0.0 };
        self.kinds = [
            (config.exam_choice, QuestionKind::Choice),
            (config.exam_typing, QuestionKind::Typing),
        ]
        .into_iter()
        .filter_map(|(on, kind)| on.then_some(kind))
        .collect();
        if self.kinds.is_empty() {
            self.kinds.push(QuestionKind::Choice);
        }
        self.auto = auto;
        self.penalties = config.exam_penalties;
        self.started_at = now;
        self.ends_at = if auto {
            now + (config.exam_minutes * 60_000.0) as i64
        } else {
            now
        };
        if auto {
            self.heard_at_start = world.heard.clone();
        }
    }

    pub(crate) fn stop(&mut self) {
        self.stage = Stage::Off;
        self.asked.clear();
        self.wrong.clear();
        self.heard_at_start.clear();
        self.queue.clear();
        self.rows.clear();
    }

    pub(crate) fn is_off(&self) -> bool {
        self.stage == Stage::Off
    }

    pub(crate) fn playing(&self) -> bool {
        self.stage == Stage::Playing
    }

    pub(crate) fn asking(&self) -> bool {
        self.stage == Stage::Asking
    }

    /// Started on its own (by `examAuto`), not by hand.
    pub(crate) fn auto(&self) -> bool {
        self.auto
    }

    /// A tick gap that was not play (a screen open, or over a second): the countdown waits, but
    /// never longer than the exam has run.
    pub(crate) fn add_back(&mut self, gap: i64, now: i64) {
        if self.playing() {
            self.ends_at += gap.min((now - self.started_at).max(0));
        }
    }

    /// The countdown is over.
    pub(crate) fn due(&self, now: i64) -> bool {
        self.playing() && now >= self.ends_at
    }

    /// `beginExamQuestions`: picks the questions and starts asking. Returns how many; with none
    /// the exam is off.
    pub(crate) fn begin(&mut self, pack: &Pack, world: &World, rng: &mut Rng) -> usize {
        let top = self.top(pack, world, rng);
        if top.is_empty() {
            self.stop();
            return 0;
        }
        self.queue = top;
        self.stage = Stage::Asking;
        self.queue.len()
    }

    /// The phrases to ask, best first; see the module docs.
    fn top(&self, pack: &Pack, world: &World, rng: &mut Rng) -> Vec<Phrase> {
        let count = |map: &HashMap<String, i32>, id: &str| map.get(id).copied().unwrap_or(0);
        let mut scored: Vec<(&Phrase, i32, i32)> = pack
            .phrases()
            .iter()
            .filter(|phrase| !phrase.is_fragment())
            .filter_map(|phrase| {
                let id = phrase.id.as_str();
                let said = world.heard.get(id).copied().unwrap_or(0)
                    - self.heard_at_start.get(id).copied().unwrap_or(0);
                let times = said.max(0) + count(&self.asked, id);
                (times > 0).then(|| (phrase, times, count(&self.wrong, id)))
            })
            .collect();
        rng.shuffle(&mut scored);
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
        let mut texts = HashSet::new();
        scored
            .into_iter()
            .filter(|(phrase, ..)| texts.insert(phrase.text.as_str()))
            .take(QUESTIONS)
            .map(|(phrase, ..)| phrase.clone())
            .collect()
    }

    /// How the next exam question is asked.
    pub(crate) fn kind(&self, rng: &mut Rng) -> QuestionKind {
        if self.kinds.is_empty() {
            return QuestionKind::Choice;
        }
        self.kinds[rng.below(self.kinds.len())]
    }

    /// A quiz question was asked during the countdown: it came up once more.
    pub(crate) fn on_asked(&mut self, id: &str) {
        if self.playing() {
            *self.asked.entry(id.to_string()).or_insert(0) += 1;
        }
    }

    /// The question being asked, while asking and not all are answered.
    pub(crate) fn current(&self) -> Option<&Phrase> {
        if self.asking() {
            self.queue.get(self.rows.len())
        } else {
            None
        }
    }

    /// An answer: one to the exam's question is recorded with what was given, and has
    /// consequences only with the exam's penalties; a wrong one to a normal question during the
    /// countdown counts against its phrase. Returns whether the answer has consequences.
    pub(crate) fn on_answer(&mut self, question: &Phrase, correct: bool, given: &str) -> bool {
        if self
            .current()
            .is_some_and(|current| current.id == question.id)
        {
            self.rows.push(ExamRow {
                phrase: question.clone(),
                correct,
                given: given.to_string(),
            });
            return self.penalties;
        }
        if self.playing() && !correct {
            *self.wrong.entry(question.id.clone()).or_insert(0) += 1;
        }
        true
    }

    /// `finishExam`: the result, and the exam is off.
    pub(crate) fn finish(&mut self) -> ExamResult {
        let kinds: Vec<&str> = self
            .kinds
            .iter()
            .map(|kind| match kind {
                QuestionKind::Choice => "choice",
                QuestionKind::Typing => "typing",
            })
            .collect();
        let result = ExamResult {
            minutes: self.minutes,
            kinds: kinds.join("+"),
            rows: std::mem::take(&mut self.rows),
        };
        self.stop();
        result
    }

    pub(crate) fn stage(&self, now: i64) -> ExamStage {
        match self.stage {
            Stage::Off => ExamStage::Off,
            Stage::Playing => ExamStage::Playing {
                left: clock(self.ends_at - now),
            },
            Stage::Asking => ExamStage::Asking {
                number: self.rows.len() + 1,
                total: self.queue.len(),
            },
        }
    }

    /// The questions picked, in order, for the core's tests.
    #[cfg(test)]
    pub(crate) fn queue(&self) -> Vec<String> {
        self.queue.iter().map(|phrase| phrase.id.clone()).collect()
    }
}
