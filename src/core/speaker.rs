//! The speech queue (Java `Speaker`): which phrase the teacher says next, which sound to play,
//! and when the phrase is over.
//!
//! A clip lasts as long as its lipsync track (plus the 50 ms lipsync delay), not as long as the
//! engine says the sound plays: with no audio output the engine reports a sound stopped at once.
//! Only a clip with no track ends when the game reports its sound stopped. One phrase plays at a
//! time, 250 ms apart, and none longer than 9 s. A chain of clips plays one after the other: the
//! next starts when the lipsync shows the previous clip's speech is over.
//!
//! A joined counted sentence (the count, then the plural) is one sound in the original, its PCM
//! joined and its lipsync the first track padded to the first clip's length, then the second.
//! Here the two clips play in turn, the second exactly where the first clip's track ends, so the
//! mouth follows that same joined track; it counts as heard once, under the sentence's id.
use super::config::Config;
use super::pack::{Pack, Phrase};
use super::progress::Progress;
use std::collections::{HashMap, VecDeque};

const MAX_PHRASE_MS: i64 = 9000;
const GAP_MS: i64 = 250;
const INTRA_GAP_MS: i64 = 60;
const JOIN_MS: i64 = 40;
const LIPSYNC_DELAY_MS: i64 = 50;

/// What the game layer does with the teacher's sound.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundCommand {
    /// Play a sound event on the voice source, not positional. `volume` is 0 while the teacher is
    /// muted: the clip still runs, so the text and timing stay. `length_ms` is the lipsync
    /// length that times the clip, when the pack has a track.
    Play {
        event: String,
        phrase_id: String,
        volume: f32,
        length_ms: Option<i64>,
    },
    /// Stop the teacher's sound.
    Stop,
}

#[derive(Clone, Debug)]
struct Queued {
    phrase: Phrase,
    repeat: bool,
    priority: bool,
    sounds: Vec<Phrase>,
    voiced: bool,
    joined: bool,
}

impl Queued {
    fn single(phrase: &Phrase, repeat: bool, priority: bool, voiced: bool) -> Queued {
        Queued {
            phrase: phrase.clone(),
            repeat,
            priority,
            sounds: vec![phrase.clone()],
            voiced,
            joined: false,
        }
    }
}

#[derive(Clone, Debug)]
struct Playing {
    clip_key: String,
    clip_started_at: i64,
    length_ms: Option<i64>,
    /// The first clip of a joined sentence: the next starts when this one's track ends.
    joined: bool,
    /// The second clip of a joined sentence: the first clip's name and start, whose track the
    /// mouth reads until this clip's own track starts.
    after: Option<(String, i64)>,
}

#[derive(Debug, Default)]
pub(crate) struct Speaker {
    queue: VecDeque<Queued>,
    pending_sounds: VecDeque<Phrase>,
    last_said: HashMap<String, i64>,
    playing: Option<Playing>,
    /// The phrase playing is a joined sentence: its clips are neither heard nor timed one by one.
    joined: bool,
    silent: bool,
    shown: Option<Phrase>,
    started_at: i64,
    finished_at: i64,
    commands: Vec<SoundCommand>,
}

/// Learned phrases stay silent: `muteLearned`, and `restAfterCorrect` while they rest.
fn resting(id: &str, config: &Config, progress: &Progress, now: i64) -> bool {
    (config.mute_learned && progress.is_learned(id, config))
        || (config.rest_after_correct && progress.rest_left(id, now, config) > 0)
}

impl Speaker {
    /// A phrase the same-event cooldown applies to.
    pub(crate) fn say(&mut self, p: &Phrase, config: &Config, progress: &Progress, now: i64) {
        self.enqueue(p, false, false, config, progress, now);
    }

    /// A fresh phrase: it skips the same-event cooldown.
    pub(crate) fn say_new(&mut self, p: &Phrase, config: &Config, progress: &Progress, now: i64) {
        self.enqueue(p, false, true, config, progress, now);
    }

    /// An ongoing state: replaces any queued repeat.
    pub(crate) fn say_ongoing(
        &mut self,
        p: &Phrase,
        repeat: bool,
        config: &Config,
        progress: &Progress,
        now: i64,
    ) {
        self.queue.retain(|q| !q.repeat);
        self.enqueue(p, repeat, !repeat, config, progress, now);
    }

    /// Drops the queued repeat of a state that ended.
    pub(crate) fn drop_queued(&mut self, id: &str) {
        self.queue.retain(|q| !(q.repeat && q.phrase.id == id));
    }

    /// Stops what plays and says `p` next, ahead of everything. `voiced` plays it even while the
    /// teacher is muted.
    pub(crate) fn say_now(&mut self, p: &Phrase, voiced: bool, config: &Config, now: i64) {
        if config.on() {
            self.stop(now);
            self.queue.push_back(Queued::single(p, false, true, voiced));
        }
    }

    /// Shows `display` while its clips play one after the other. The cooldown and the learned
    /// checks go by the last clip.
    pub(crate) fn say_sequence(
        &mut self,
        display: &Phrase,
        sounds: &[Phrase],
        fresh: bool,
        config: &Config,
        progress: &Progress,
        now: i64,
    ) {
        let Some(last) = sounds.last() else {
            return;
        };
        if !config.on()
            || !config.allows(&display.action)
            || self.cooling(&last.id, fresh, config, now)
            || self.is_queued(&display.id)
            || resting(&last.id, config, progress, now)
            || !self.make_room(config)
        {
            return;
        }
        self.queue.push_back(Queued {
            phrase: display.clone(),
            repeat: false,
            priority: false,
            sounds: sounds.to_vec(),
            voiced: false,
            joined: false,
        });
        self.last_said.insert(last.id.clone(), now);
    }

    /// Shows `display` while `start` and `noun` play as one joined clip. The cooldown and the
    /// queue go by the sentence's id; learned phrases are not checked, as in the original.
    pub(crate) fn say_joined(
        &mut self,
        display: &Phrase,
        start: &Phrase,
        noun: &Phrase,
        fresh: bool,
        config: &Config,
        now: i64,
    ) {
        if !config.on()
            || !config.allows(&display.action)
            || self.cooling(&display.id, fresh, config, now)
            || self.is_queued(&display.id)
            || !self.make_room(config)
        {
            return;
        }
        self.queue.push_back(Queued {
            phrase: display.clone(),
            repeat: false,
            priority: false,
            sounds: vec![start.clone(), noun.clone()],
            voiced: false,
            joined: true,
        });
        self.last_said.insert(display.id.clone(), now);
    }

    pub(crate) fn stop(&mut self, now: i64) {
        if self.playing.take().is_some() {
            self.commands.push(SoundCommand::Stop);
            self.finished_at = now;
        }
        self.pending_sounds.clear();
        self.queue.clear();
    }

    /// The game stopped every sound (a resource reload): the clip playing is over now, as when
    /// the game reports it stopped, and the rest of its sequence and the queue follow. A joined
    /// sentence is one sound in the original, so its second clip goes with it.
    pub(crate) fn cut(&mut self, now: i64) {
        if self.playing.take().is_some() {
            self.finished_at = now;
            if self.joined {
                self.pending_sounds.clear();
            }
        }
    }

    fn enqueue(
        &mut self,
        p: &Phrase,
        repeat: bool,
        fresh: bool,
        config: &Config,
        progress: &Progress,
        now: i64,
    ) {
        if !config.on()
            || !config.allows(&p.action)
            || self.cooling(&p.id, fresh, config, now)
            || resting(&p.id, config, progress, now)
            || self.is_queued(&p.id)
            || !self.make_room(config)
        {
            return;
        }
        self.queue
            .push_back(Queued::single(p, repeat, false, false));
        self.last_said.insert(p.id.clone(), now);
    }

    /// Said less than `sameEventCooldownSeconds` ago, and not fresh.
    fn cooling(&self, id: &str, fresh: bool, config: &Config, now: i64) -> bool {
        !fresh
            && self.last_said.get(id).is_some_and(|&last| {
                ((now - last) as f32) < config.same_event_cooldown_seconds * 1000.0
            })
    }

    fn is_queued(&self, id: &str) -> bool {
        self.queue.iter().any(|q| q.phrase.id == id)
    }

    /// Evicts the oldest phrases that are not priority until one more fits under `maxQueue`.
    /// False when only priority phrases are queued.
    fn make_room(&mut self, config: &Config) -> bool {
        let cap = config.max_queue.max(1) as usize;
        while self.queue.len() >= cap {
            match self.queue.iter().position(|q| !q.priority) {
                Some(index) => {
                    self.queue.remove(index);
                }
                None => return false,
            }
        }
        true
    }

    /// One client tick. `sound_active` is the game's report on the last sound played; it only
    /// times a clip with no lipsync track.
    pub(crate) fn tick(
        &mut self,
        now: i64,
        sound_active: bool,
        pack: &Pack,
        config: &Config,
        progress: &mut Progress,
    ) {
        let mut chain_next = false;
        let mut over = false;
        if let Some(playing) = &self.playing {
            if !self.pending_sounds.is_empty() {
                let elapsed = now - playing.clip_started_at;
                chain_next = match playing.length_ms {
                    Some(length) if playing.joined => elapsed >= length,
                    _ => {
                        let spoken = pack.lipsync.spoken_ms(&playing.clip_key);
                        spoken >= 0 && elapsed >= LIPSYNC_DELAY_MS + spoken + JOIN_MS
                    }
                };
            }
            let clip_done = match playing.length_ms {
                Some(length) => now - playing.clip_started_at >= LIPSYNC_DELAY_MS + length,
                None => !sound_active,
            };
            over = clip_done || now - self.started_at > MAX_PHRASE_MS;
        }
        if chain_next {
            if let Some(next) = self.pending_sounds.pop_front() {
                self.play_clip(&next, now, pack, config, progress);
            }
            return;
        }
        if over {
            self.playing = None;
            self.finished_at = now;
        }
        if self.playing.is_some() {
            return;
        }
        if !self.pending_sounds.is_empty() {
            if now - self.finished_at >= INTRA_GAP_MS
                && let Some(next) = self.pending_sounds.pop_front()
            {
                self.play_clip(&next, now, pack, config, progress);
            }
            return;
        }
        if now - self.started_at.max(self.finished_at) < GAP_MS {
            return;
        }
        let Some(queued) = self.queue.pop_front() else {
            return;
        };
        self.started_at = now;
        self.silent = config.muted && !queued.voiced;
        self.joined = false;
        if queued.joined {
            progress.heard(&queued.phrase.id);
            // The original falls back to two clips in a row when the first has no sound.
            let first = &queued.sounds[0];
            let event = pack.sound_event(
                &first.id,
                pack.voice(&config.voice),
                pack.speed(&config.speed),
            );
            if pack.has_sound(&event) {
                self.joined = true;
                for clip in &queued.sounds {
                    self.last_said.insert(clip.id.clone(), now);
                }
            }
        }
        self.shown = Some(queued.phrase);
        self.pending_sounds.extend(queued.sounds);
        if let Some(first) = self.pending_sounds.pop_front() {
            self.play_clip(&first, now, pack, config, progress);
        }
    }

    fn play_clip(
        &mut self,
        clip: &Phrase,
        now: i64,
        pack: &Pack,
        config: &Config,
        progress: &mut Progress,
    ) {
        let after = self
            .playing
            .as_ref()
            .filter(|before| before.joined)
            .map(|before| (before.clip_key.clone(), before.clip_started_at));
        let voice = pack.voice(&config.voice);
        let speed = pack.speed(&config.speed);
        let event = pack.sound_event(&clip.id, voice, speed);
        if pack.has_sound(&event) {
            let clip_key = clip.sound_path(voice, speed);
            let length_ms = pack.lipsync.length_ms(&clip_key);
            self.commands.push(SoundCommand::Play {
                event,
                phrase_id: clip.id.clone(),
                volume: if self.silent { 0.0 } else { config.volume },
                length_ms,
            });
            self.playing = Some(Playing {
                clip_key,
                clip_started_at: now,
                length_ms,
                joined: self.joined && !self.pending_sounds.is_empty(),
                after,
            });
            self.finished_at = 0;
        } else {
            self.finished_at = now;
        }
        if self.joined {
            return;
        }
        self.last_said.insert(clip.id.clone(), now);
        if clip.key.is_some() || !clip.action.ends_with("count") {
            progress.heard(&clip.id);
        }
    }

    /// Drains the queue, naming each phrase, with its clips when it has more than one: joined
    /// with `+`, in a row with `,`.
    #[cfg(test)]
    pub(crate) fn take_queued(&mut self) -> Vec<String> {
        self.queue
            .drain(..)
            .map(|q| {
                if q.sounds.len() < 2 {
                    return q.phrase.id;
                }
                let clips: Vec<&str> = q.sounds.iter().map(|clip| clip.id.as_str()).collect();
                let glue = if q.joined { "+" } else { "," };
                format!("{}[{}]", q.phrase.id, clips.join(glue))
            })
            .collect()
    }

    /// The commands since the last call, in order.
    pub(crate) fn take_commands(&mut self) -> Vec<SoundCommand> {
        std::mem::take(&mut self.commands)
    }

    /// A clip is playing or another of the chain waits.
    pub(crate) fn is_speaking(&self) -> bool {
        self.playing.is_some() || !self.pending_sounds.is_empty()
    }

    /// The mouth level now: the lipsync digit, 1 and 2 in turn every 120 ms for a clip with no
    /// track, and 0 while silent or quiet.
    pub(crate) fn mouth(&self, now: i64, pack: &Pack) -> i32 {
        match &self.playing {
            Some(playing) if !self.silent => {
                let into = now - playing.clip_started_at - LIPSYNC_DELAY_MS;
                let level = match &playing.after {
                    Some((before, started_at)) if into < 0 => pack
                        .lipsync
                        .mouth(before, now - started_at - LIPSYNC_DELAY_MS),
                    _ => pack.lipsync.mouth(&playing.clip_key, into),
                };
                if level >= 0 {
                    level
                } else if (now / 120) % 2 == 0 {
                    1
                } else {
                    2
                }
            }
            _ => 0,
        }
    }

    /// The phrase on the HUD: it stays `hudHoldSeconds` after its last clip ends.
    pub(crate) fn shown(&self, now: i64, config: &Config) -> Option<&Phrase> {
        let faded = self.pending_sounds.is_empty()
            && self.finished_at != 0
            && ((now - self.finished_at) as f32) > config.hud_hold_seconds * 1000.0;
        if faded { None } else { self.shown.as_ref() }
    }

    /// The HUD text's opacity from 1 down to 0 over `hudHoldSeconds` after the last clip ends.
    pub(crate) fn visibility(&self, now: i64, config: &Config) -> f32 {
        if self.finished_at != 0 && self.pending_sounds.is_empty() {
            let hold = config.hud_hold_seconds.max(0.1) * 1000.0;
            1.0 - (((now - self.finished_at) as f32) / hold).min(1.0)
        } else {
            1.0
        }
    }
}
