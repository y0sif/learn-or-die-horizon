//! Which phrase an action calls for, and whether it is said now (Java `ActionDetector`, for the
//! pilot actions: break, place, pickup of one item, and the held item).
//!
//! Freshness: a phrase is NEW the first time, and again once its driver was let go and pressed
//! again (the attack key for breaking, the use key for placing). A pickup is a RESULT: NEW after a
//! new attack or use press, or after a `seriesPauseSeconds` pause with both keys up. Otherwise it
//! repeats only every `repeatSeconds` (never by default), and a NEW within `sameAgainSeconds` of
//! the last time it was said stays silent. The held item is said on every change of item.
use super::config::Config;
use super::pack::{Pack, Phrase, path_of};
use super::teacher::WorldTick;
use std::collections::HashMap;

const STATE_GRACE_MS: i64 = 1000;
const AIR: &str = "minecraft:air";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fresh {
    New,
    Repeat,
    Same,
}

/// What a phrase waits on before it is new again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Driver {
    Attack,
    Use,
    Result,
    #[default]
    None,
}

/// What the detector asks of the speaker and the HUD.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Speech {
    /// Said now, past the same-event cooldown.
    New(Phrase),
    /// Said unless it was said within the same-event cooldown.
    Repeat(Phrase),
    /// An ongoing state started (`repeat` false) or is due again (`repeat` true).
    Ongoing { phrase: Phrase, repeat: bool },
    /// The state ended: its queued repeat goes.
    DropQueued(String),
    /// Show the held item's name above the hotbar.
    HeldLabel(Phrase),
}

#[derive(Debug, Default)]
pub(crate) struct Detector {
    tick_no: i64,
    press_tick: i64,
    attack_down: bool,
    use_down: bool,
    attack_held: bool,
    use_held: bool,
    attack_free_tick: i64,
    use_free_tick: i64,
    /// Per phrase: `[tick of the last check, time of the last check, time last said]`.
    asked: HashMap<String, [i64; 3]>,
    state_id: Option<String>,
    state_driver: Driver,
    state_said_at: i64,
    state_seen_at: i64,
    pending_id: Option<String>,
    pending_since: i64,
    last_counts: Option<Vec<(String, i32)>>,
    last_held: Option<String>,
}

fn is_empty_item(item: &str, count: i32) -> bool {
    count <= 0 || item.is_empty() || item == AIR
}

/// `break_hand.<key>` unless the held item is an axe, else `break.<key>`.
pub(crate) fn break_phrase<'a>(pack: &'a Pack, held_item: &str, block: &str) -> Option<&'a Phrase> {
    if !path_of(held_item).ends_with("_axe")
        && let Some(by_hand) = pack.for_block("break_hand", block)
    {
        return Some(by_hand);
    }
    pack.for_block("break", block)
}

fn driver_of(state: &Phrase) -> Driver {
    match state.action.as_str() {
        "break" | "break_hand" => Driver::Attack,
        "eat" | "drink" => Driver::Use,
        _ => Driver::None,
    }
}

impl Detector {
    /// A new level: forget every tracker. The item in hand now is not announced.
    pub(crate) fn reset(&mut self, held_item: &str) {
        self.state_id = None;
        self.state_driver = Driver::None;
        self.asked.clear();
        self.last_counts = None;
        let held = if is_empty_item(held_item, 1) {
            AIR
        } else {
            held_item
        };
        self.last_held = Some(held.to_string());
    }

    /// The player broke a block (Java `onBlockBroken`). Silent when the break is the ongoing state
    /// already spoken while mining.
    pub(crate) fn block_broken(
        &mut self,
        block: &str,
        held_item: &str,
        now: i64,
        pack: &Pack,
        config: &Config,
    ) -> Option<Speech> {
        if !self.attack_down {
            self.press_tick = self.tick_no + 1;
        }
        let phrase = break_phrase(pack, held_item, block)?;
        if self.state_id.as_deref() == Some(phrase.id.as_str()) {
            return None;
        }
        self.say(phrase, Driver::Attack, now, config)
    }

    /// A block the player placed appeared in the world (Java's pending place check).
    pub(crate) fn block_placed(
        &mut self,
        block: &str,
        now: i64,
        pack: &Pack,
        config: &Config,
    ) -> Option<Speech> {
        let phrase = pack.for_block("place", block)?;
        self.say(phrase, Driver::Use, now, config)
    }

    /// One client tick in a world, not paused (Java `ActionDetector.tick`, pilot parts).
    pub(crate) fn tick(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
    ) -> Vec<Speech> {
        let mut out = Vec::new();
        self.read_drivers(input);
        self.ongoing_state(input, pack, config, &mut out);
        self.pickups(input, pack, config, &mut out);
        if config.say_held_item {
            let held = if is_empty_item(input.held_item, input.held_count) {
                AIR
            } else {
                input.held_item
            };
            if self.last_held.as_deref() != Some(held) {
                self.last_held = Some(held.to_string());
                if held != AIR
                    && let Some(phrase) = pack.for_item("hold", held)
                {
                    out.push(Speech::HeldLabel(phrase.clone()));
                    out.push(Speech::Repeat(phrase.clone()));
                }
            }
        }
        out
    }

    fn read_drivers(&mut self, input: &WorldTick<'_>) {
        self.tick_no += 1;
        if (input.attack_down && !self.attack_down) || (input.use_down && !self.use_down) {
            self.press_tick = self.tick_no;
        }
        self.attack_down = input.attack_down;
        self.use_down = input.use_down;
        self.attack_held = input.attack_down;
        if !self.attack_held {
            self.attack_free_tick = self.tick_no;
        }
        self.use_held = input.use_down || input.using_item;
        if !self.use_held {
            self.use_free_tick = self.tick_no;
        }
    }

    fn held(&self, driver: Driver) -> bool {
        match driver {
            Driver::Attack => self.attack_held,
            Driver::Use => self.use_held,
            Driver::Result | Driver::None => false,
        }
    }

    /// The state the player is in: in the pilot, mining a block. It is spoken when it starts, and
    /// again every `repeatSeconds` while it lasts.
    fn ongoing_state(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
        out: &mut Vec<Speech>,
    ) {
        let now = input.now;
        let state = input
            .breaking
            .and_then(|block| break_phrase(pack, input.held_item, block));
        let id = state.map(|phrase| phrase.id.as_str());
        let left = self
            .state_id
            .as_deref()
            .is_some_and(|current| Some(current) != id && !self.held(self.state_driver));
        if left {
            if let Some(old) = self.state_id.take() {
                out.push(Speech::DropQueued(old));
            }
            self.pending_id = None;
        }
        let Some(state) = state else {
            if now - self.state_seen_at > STATE_GRACE_MS {
                if let Some(old) = self.state_id.take() {
                    out.push(Speech::DropQueued(old));
                }
                self.pending_id = None;
            }
            return;
        };
        self.state_seen_at = now;
        if self.state_id.as_deref() == Some(state.id.as_str()) {
            if config.repeat_seconds > 0.0
                && ((now - self.state_said_at) as f32) >= config.repeat_seconds * 1000.0
            {
                self.state_said_at = now;
                out.push(Speech::Ongoing {
                    phrase: state.clone(),
                    repeat: true,
                });
            }
            return;
        }
        if self.pending_id.as_deref() != Some(state.id.as_str()) {
            self.pending_id = Some(state.id.clone());
            self.pending_since = now;
        }
        let delay = if state.is_movement() {
            (config.movement_delay_seconds * 1000.0) as i64
        } else {
            0
        };
        if now - self.pending_since >= delay {
            self.state_id = Some(state.id.clone());
            self.state_driver = driver_of(state);
            self.state_said_at = now;
            if self.freshness(&state.id, self.state_driver, now, config) != Fresh::Same {
                out.push(Speech::Ongoing {
                    phrase: state.clone(),
                    repeat: false,
                });
            }
        }
    }

    /// Items whose total count rose since the last tick, while no screen is open. A tick whose
    /// inventory could not be read is skipped, keeping the last counts.
    fn pickups(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
        out: &mut Vec<Speech>,
    ) {
        let Some(inventory) = input.inventory else {
            return;
        };
        let mut counts: Vec<(String, i32)> = Vec::new();
        for &(item, count) in inventory {
            if is_empty_item(item, count) {
                continue;
            }
            match counts.iter_mut().find(|(name, _)| name.as_str() == item) {
                Some(entry) => entry.1 += count,
                None => counts.push((item.to_string(), count)),
            }
        }
        let mut gained = Vec::new();
        if let Some(last) = &self.last_counts
            && !input.screen_open
        {
            for (item, total) in &counts {
                let before = last
                    .iter()
                    .find(|(name, _)| name == item)
                    .map_or(0, |entry| entry.1);
                if total - before > 0 {
                    gained.push((item.clone(), total - before));
                }
            }
        }
        self.last_counts = Some(counts);
        for (item, amount) in gained {
            if let Some(speech) = self.pickup(&item, amount, input.now, pack, config) {
                out.push(speech);
            }
        }
    }

    /// `pickup.<key>` when the item has a word. Counted sentences for more than one item come
    /// later: their freshness is still recorded, so the rules stay the original's.
    fn pickup(
        &mut self,
        item: &str,
        amount: i32,
        now: i64,
        pack: &Pack,
        config: &Config,
    ) -> Option<Speech> {
        pack.for_item("hold", item)?;
        let whole = pack.for_item("pickup", item)?;
        let fresh = self.freshness(&whole.id, Driver::Result, now, config);
        if amount > 1 {
            return None;
        }
        match fresh {
            Fresh::New => Some(Speech::New(whole.clone())),
            Fresh::Repeat => Some(Speech::Repeat(whole.clone())),
            Fresh::Same => None,
        }
    }

    fn say(
        &mut self,
        phrase: &Phrase,
        driver: Driver,
        now: i64,
        config: &Config,
    ) -> Option<Speech> {
        match self.freshness(&phrase.id, driver, now, config) {
            Fresh::New => Some(Speech::New(phrase.clone())),
            Fresh::Repeat => Some(Speech::Repeat(phrase.clone())),
            Fresh::Same => None,
        }
    }

    /// NEW, REPEAT or SAME for the phrase now, and records the check.
    fn freshness(&mut self, id: &str, driver: Driver, now: i64, config: &Config) -> Fresh {
        let series = (config.series_pause_seconds * 1000.0) as i64;
        let previous = self.asked.get(id).copied();
        let fresh = match previous {
            None => Fresh::New,
            Some([tick, at, said]) => {
                let is_new = match driver {
                    Driver::Result => {
                        self.press_tick > tick
                            || (!self.attack_down && !self.use_down && now - at >= series)
                    }
                    Driver::None => true,
                    Driver::Attack => self.attack_free_tick > tick,
                    Driver::Use => self.use_free_tick > tick,
                };
                let repeat_ms = (config.repeat_seconds * 1000.0) as i64;
                let same_again_ms = (config.same_again_seconds * 1000.0) as i64;
                let said_lately = config.same_again_seconds > 0.0 && now - said < same_again_ms;
                if is_new && !said_lately {
                    Fresh::New
                } else if !is_new && config.repeat_seconds > 0.0 && now - said >= repeat_ms - 200 {
                    Fresh::Repeat
                } else {
                    Fresh::Same
                }
            }
        };
        let said = match (fresh, previous) {
            (Fresh::Same, Some([_, _, said])) => said,
            _ => now,
        };
        self.asked.insert(id.to_string(), [self.tick_no, now, said]);
        fresh
    }
}
