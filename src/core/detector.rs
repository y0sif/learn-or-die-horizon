//! Which phrase an action calls for, and whether it is said now (Java `ActionDetector`).
//!
//! Ongoing states (sleeping, riding, mining, eating or drinking, fishing, flying, swimming,
//! climbing, sprinting, sneaking, walking) are said when they start, movements only after
//! `movementDelaySeconds`, and again every `repeatSeconds` while they last. Events (hit, feed,
//! use, place, break, pickup, drop, held item, jump, damage, burning, kills, hunger, day and
//! night, weather, climate) are said by their freshness.
//!
//! Freshness: a phrase is NEW the first time, and again once its driver was let go and taken up
//! again: a key (attack, use) or a state (moving, sprinting, sneaking, water, flight, climbing,
//! vehicle, bed, rod, fire). A SERIES (damage) is NEW after a `seriesPauseSeconds` pause. A
//! RESULT (pickup, kill) is NEW after a new attack or use press, or after a `seriesPauseSeconds`
//! pause with both keys up. Otherwise it repeats only every `repeatSeconds` (never by default),
//! and a NEW within `sameAgainSeconds` of the last time it was said stays silent. The held item
//! and drops are said on every change. Topic switches: the speaker drops a phrase whose topic is
//! off; held item, kills, damage, weather and time, and climate are switched off here.
use super::config::Config;
use super::pack::{Pack, Part, Phrase, path_of};
use super::teacher::{Fate, WorldTick};
use std::collections::HashMap;

const STATE_GRACE_MS: i64 = 1000;
const CLIMATE_DEBOUNCE_MS: i64 = 3000;
const KILL_CREDIT_MS: i64 = 6000;
const WEATHER_AFTER_JOIN_MS: i64 = 3000;
/// How long after the last tick of a fast fall a health drop is still the fall's. The original
/// looks one tick back, but a client hears its health from the server, a tick or more after the
/// landing it reported.
const FALL_HURT_MS: i64 = 1000;
const HOT_TEMPERATURE: f32 = 1.5;
const AIR: &str = "minecraft:air";

/// Items an animal eats, beside anything with a food component.
const ANIMAL_FOOD: [&str; 44] = [
    "wheat",
    "wheat_seeds",
    "beetroot_seeds",
    "melon_seeds",
    "pumpkin_seeds",
    "torchflower_seeds",
    "carrot",
    "golden_carrot",
    "potato",
    "apple",
    "golden_apple",
    "hay_block",
    "sugar",
    "sweet_berries",
    "glow_berries",
    "bamboo",
    "cactus",
    "dandelion",
    "seagrass",
    "cod",
    "salmon",
    "tropical_fish",
    "bone",
    "rotten_flesh",
    "honeycomb",
    "cake",
    "beef",
    "porkchop",
    "chicken",
    "mutton",
    "rabbit",
    "cooked_beef",
    "cooked_porkchop",
    "cooked_chicken",
    "cooked_mutton",
    "cooked_rabbit",
    "cooked_cod",
    "cooked_salmon",
    "egg",
    "kelp",
    "spider_eye",
    "slime_ball",
    "warped_fungus",
    "crimson_fungus",
];

/// English nouns the counted sentence's translation keeps as they are.
const NO_PLURAL: [&str; 67] = [
    "bread",
    "wheat",
    "paper",
    "string",
    "leather",
    "coal",
    "iron",
    "copper",
    "gold",
    "sugar",
    "snow",
    "ice",
    "sand",
    "dirt",
    "gravel",
    "glass",
    "wool",
    "stone",
    "wood",
    "planks",
    "mud",
    "clay",
    "moss",
    "redstone",
    "quartz",
    "netherite",
    "flint",
    "bone meal",
    "gunpowder",
    "kelp",
    "honey",
    "milk",
    "soup",
    "watermelon",
    "steak",
    "raw beef",
    "raw pork",
    "cooked pork",
    "raw chicken",
    "cooked chicken",
    "raw mutton",
    "cooked mutton",
    "cooked rabbit",
    "cooked fish",
    "raw fish",
    "rotten flesh",
    "dried kelp",
    "seeds",
    "boots",
    "leggings",
    "shears",
    "scaffolding",
    "hay",
    "lapis lazuli",
    "cobblestone",
    "deepslate",
    "sandstone",
    "terracotta",
    "amethyst",
    "glowstone",
    "bricks",
    "stone bricks",
    "obsidian",
    "netherrack",
    "soul sand",
    "end stone",
    "concrete",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fresh {
    New,
    Repeat,
    Same,
}

/// What a phrase waits on before it is new again. The first twelve are held or let go each tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Driver {
    Attack,
    Use,
    Move,
    Sprint,
    Sneak,
    Water,
    Flight,
    Climb,
    Vehicle,
    Bed,
    Rod,
    Fire,
    Series,
    Result,
    #[default]
    None,
}

const HELD_DRIVERS: usize = 12;

/// What the detector asks of the speaker, the HUD and the progress.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Speech {
    /// Said now, past the same-event cooldown.
    New(Phrase),
    /// Said unless it was said within the same-event cooldown.
    Repeat(Phrase),
    /// Said at once, ahead of everything queued.
    Now(Phrase),
    /// An ongoing state started (`repeat` false) or is due again (`repeat` true).
    Ongoing { phrase: Phrase, repeat: bool },
    /// The state ended: its queued repeat goes.
    DropQueued(String),
    /// Show the held item's name above the hotbar.
    HeldLabel(Phrase),
    /// A counted sentence shown as `display`: the count start and the plural as one `joined`
    /// clip, else (no counted pieces) the whole sentence then the amount as two clips.
    Counted {
        display: Box<Phrase>,
        clips: Vec<Phrase>,
        joined: bool,
        fresh: bool,
    },
    /// The player died.
    Died,
}

#[derive(Debug, Default)]
pub(crate) struct Detector {
    tick_no: i64,
    press_tick: i64,
    attack_down: bool,
    use_down: bool,
    held: [bool; HELD_DRIVERS],
    free_tick: [i64; HELD_DRIVERS],
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
    /// The first tick in a level reads the player's health, ground and food without a word.
    level_unread: bool,
    joined_at: i64,
    drop_was_down: bool,
    prev_held: Option<String>,
    drop_check: Option<String>,
    last_health: f32,
    was_dead: bool,
    /// The last tick the player fell faster than 0.6 blocks a tick, off the ground.
    fell_at: Option<i64>,
    was_on_ground: bool,
    hungry_said: bool,
    last_jump_at: i64,
    was_dark: Option<bool>,
    precipitation: Option<&'static str>,
    climate: Option<&'static str>,
    pending_climate: Option<&'static str>,
    pending_climate_since: i64,
    climate_said_at: i64,
    /// Living things the player hit, for the kill: `(id, entity id, when)`.
    recently_hit: Vec<(u64, String, i64)>,
    last_weather_report_at: i64,
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
        "walk" => Driver::Move,
        "sprint" => Driver::Sprint,
        "sneak" => Driver::Sneak,
        "swim" => Driver::Water,
        "fly" => Driver::Flight,
        "climb" => Driver::Climb,
        "ride" => Driver::Vehicle,
        "sleep" => Driver::Bed,
        "fish" => Driver::Rod,
        _ => Driver::None,
    }
}

fn riding(input: &WorldTick<'_>) -> bool {
    input.in_vehicle || input.vehicle.is_some()
}

fn moving(input: &WorldTick<'_>) -> bool {
    let [x, _, z] = input.velocity;
    x * x + z * z > 1.0e-4
}

/// The state the player is in, in the original's order of precedence.
fn current_state<'a>(input: &WorldTick<'_>, pack: &'a Pack) -> Option<&'a Phrase> {
    if input.sleeping {
        return pack.phrase("sleep");
    }
    if let Some(vehicle) = input.vehicle {
        return pack.for_entity("ride", vehicle);
    }
    if input.in_vehicle {
        return None;
    }
    if let Some(phrase) = input
        .breaking
        .and_then(|block| break_phrase(pack, input.held_item, block))
    {
        return Some(phrase);
    }
    if let Some(phrase) = input.eating.and_then(|item| {
        pack.for_item("drink", item)
            .or_else(|| pack.for_item("eat", item))
    }) {
        return Some(phrase);
    }
    if input.fishing {
        return pack.phrase("fish");
    }
    let moving = moving(input);
    if input.fall_flying || (input.flying && moving) {
        return pack.phrase("fly");
    }
    if input.in_water && !input.on_ground && (moving || input.jump_key) {
        return pack.phrase("swim");
    }
    if input.on_climbable && !input.on_ground {
        pack.phrase("climb")
    } else if input.sprinting && moving {
        pack.phrase("sprint")
    } else if input.sneaking {
        pack.phrase("sneak")
    } else if input.move_keys && moving && input.on_ground {
        pack.phrase("walk")
    } else {
        None
    }
}

/// `none`, `rain`, `snow` or `thunder` where the player is.
fn precipitation(input: &WorldTick<'_>) -> &'static str {
    if !input.raining || !input.biome_rains {
        "none"
    } else if input.biome_snows {
        "snow"
    } else if input.thundering {
        "thunder"
    } else {
        "rain"
    }
}

/// `cold` where it snows, `hot` from a base temperature of 1.5, else `mild`.
fn climate(input: &WorldTick<'_>) -> &'static str {
    if input.biome_snows {
        "cold"
    } else if input.biome_temperature >= HOT_TEMPERATURE {
        "hot"
    } else {
        "mild"
    }
}

/// The text without its sentence end.
fn trim_end(text: &str) -> &str {
    text.trim_end_matches(|c: char| "。！？.!?".contains(c))
}

/// The English plural of a noun for `amount` things.
pub(crate) fn plural(en: &str, amount: i32) -> String {
    if amount == 1 || NO_PLURAL.contains(&en) || en.ends_with('s') {
        en.to_string()
    } else if en.ends_with('y') && !en.ends_with("ey") {
        format!("{}ies", &en[..en.len() - 1])
    } else if en.ends_with("ch") || en.ends_with("sh") || en.ends_with('x') {
        format!("{en}es")
    } else {
        format!("{en}s")
    }
}

/// The translation the counted sentence builds: "I picked up 14 logs." and so on.
fn counted_en(action: &str, amount: i32, noun_en: &str, pack: &Pack) -> String {
    if pack.meta.language == "en" {
        return String::new();
    }
    match action {
        "pickup" => format!("I picked up {amount} {noun_en}."),
        "drop" => format!("I dropped {amount} {noun_en}."),
        _ => format!("{amount} {noun_en}"),
    }
}

impl Detector {
    /// A new level: forget every tracker. The item in hand now is not announced, and the next
    /// tick reads the player's health, ground and food without a word.
    pub(crate) fn reset(&mut self, held_item: &str, now: i64) {
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
        self.level_unread = true;
        self.joined_at = now;
        self.was_dark = None;
        self.precipitation = None;
        self.climate = None;
        self.pending_climate = None;
        self.recently_hit.clear();
        self.last_weather_report_at = now;
    }

    /// The living things the player hit lately, whose death would be his kill.
    pub(crate) fn watched(&self) -> impl Iterator<Item = u64> + '_ {
        self.recently_hit.iter().map(|(id, _, _)| *id)
    }

    /// A press that came before the tick saw its key.
    fn pressed(&mut self, was_down: bool) {
        if !was_down {
            self.press_tick = self.tick_no + 1;
        }
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
        self.pressed(self.attack_down);
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

    /// A right click on a block with the main hand, not sneaking (Java `onUseBlock`): its close,
    /// open or use phrase. `None` when the block has none, and the click may place a block.
    pub(crate) fn block_used(
        &mut self,
        block: &str,
        is_open: bool,
        now: i64,
        pack: &Pack,
        config: &Config,
    ) -> Option<Option<Speech>> {
        let phrase = pack.use_phrase(block, is_open)?;
        self.pressed(self.use_down);
        Some(self.say(phrase, Driver::Use, now, config))
    }

    /// The player attacked an entity (Java `onAttackEntity`): `hit.<entity>`, else `attack`, said
    /// at once. A living one that is not dying yet is watched 6 s for the kill.
    pub(crate) fn entity_attacked(
        &mut self,
        id: u64,
        entity: &str,
        living: bool,
        dying: bool,
        now: i64,
        pack: &Pack,
    ) -> Option<Speech> {
        if living && dying {
            return None;
        }
        self.pressed(false);
        let phrase = pack
            .for_entity("hit", entity)
            .or_else(|| pack.phrase("attack"))
            .cloned();
        if living {
            self.watch(id, entity, now);
        }
        phrase.map(Speech::New)
    }

    /// A main-hand right click on an entity (Java `onUseEntity`): `feed.<entity>` when the hand
    /// holds food, or an item from the animal food list.
    pub(crate) fn entity_used(
        &mut self,
        entity: &str,
        held_item: &str,
        held_is_food: bool,
        now: i64,
        pack: &Pack,
        config: &Config,
    ) -> Option<Speech> {
        let food = held_is_food || ANIMAL_FOOD.contains(&path_of(held_item));
        if is_empty_item(held_item, 1) || !food {
            return None;
        }
        self.pressed(self.use_down);
        let phrase = pack.for_entity("feed", entity)?;
        self.say(phrase, Driver::Use, now, config)
    }

    fn watch(&mut self, id: u64, entity: &str, now: i64) {
        match self
            .recently_hit
            .iter_mut()
            .find(|(seen, _, _)| *seen == id)
        {
            Some(entry) => {
                entry.1 = entity.to_string();
                entry.2 = now;
            }
            None => self.recently_hit.push((id, entity.to_string(), now)),
        }
    }

    /// One client tick in a world, not paused (Java `ActionDetector.tick`).
    pub(crate) fn tick(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
    ) -> Vec<Speech> {
        let now = input.now;
        let mut out = Vec::new();
        if self.level_unread {
            self.level_unread = false;
            self.last_health = input.health;
            self.was_dead = input.dead;
            self.was_on_ground = input.on_ground;
            self.fell_at = None;
            self.hungry_said = input.food <= 6;
        }
        self.read_drivers(input);
        self.ongoing_state(input, pack, config, &mut out);
        self.drop_key(input);
        if self.held[Driver::Fire as usize]
            && let Some(phrase) = pack.phrase("burning")
        {
            out.extend(self.say(phrase, Driver::Fire, now, config));
        }
        self.inventory(input, pack, config, &mut out);
        if config.say_held_item {
            let held = if is_empty_item(input.held_item, input.held_count) {
                AIR
            } else {
                input.held_item
            };
            if self.last_held.as_deref() != Some(held) {
                self.last_held = Some(held.to_string());
                if held != AIR {
                    self.say_counted("hold", held, input.held_count, now, pack, config, &mut out);
                }
            }
        }
        self.jump(input, pack, config, &mut out);
        if config.say_damage {
            self.damage(input, pack, config, &mut out);
        }
        self.kills(input, pack, config, &mut out);
        let food = input.food;
        if food <= 6 && !self.hungry_said {
            self.hungry_said = true;
            out.extend(pack.phrase("hungry").cloned().map(Speech::Repeat));
        } else if food >= 10 {
            self.hungry_said = false;
        }
        if config.say_weather_and_time && now - self.joined_at > WEATHER_AFTER_JOIN_MS {
            self.weather(input, pack, config, &mut out);
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
        let flying = input.fall_flying || input.flying;
        let other = flying
            || (input.in_water && !input.on_ground)
            || (input.on_climbable && !input.on_ground)
            || riding(input)
            || input.sleeping;
        let states = [
            (Driver::Attack, input.attack_down),
            (Driver::Use, input.use_down || input.using_item),
            (
                Driver::Move,
                input.move_keys && !other && !input.sprinting && !input.sneaking,
            ),
            (Driver::Sprint, input.sprinting && !other),
            (Driver::Sneak, input.sneaking && !other && !input.sprinting),
            (Driver::Water, input.in_water),
            (Driver::Flight, flying),
            (Driver::Climb, input.on_climbable),
            (Driver::Vehicle, riding(input)),
            (Driver::Bed, input.sleeping),
            (Driver::Rod, input.fishing),
            (Driver::Fire, input.on_fire),
        ];
        for (driver, on) in states {
            self.held[driver as usize] = on;
            if !on {
                self.free_tick[driver as usize] = self.tick_no;
            }
        }
    }

    fn held(&self, driver: Driver) -> bool {
        (driver as usize) < HELD_DRIVERS && self.held[driver as usize]
    }

    /// The ongoing state: said when it starts, and again every `repeatSeconds` while it lasts.
    fn ongoing_state(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
        out: &mut Vec<Speech>,
    ) {
        let now = input.now;
        let state = current_state(input, pack);
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

    /// The drop key went down with no screen open: the item held last tick is checked for a drop
    /// on the next inventory read.
    fn drop_key(&mut self, input: &WorldTick<'_>) {
        if input.drop_key && !self.drop_was_down && !input.screen_open && self.prev_held.is_some() {
            self.drop_check = self.prev_held.clone();
        }
        self.drop_was_down = input.drop_key;
        self.prev_held = (!is_empty_item(input.held_item, input.held_count))
            .then(|| input.held_item.to_string());
    }

    /// Items whose total count rose since the last tick are picked up, and a checked drop whose
    /// item went down is dropped, while no screen is open. A tick whose inventory could not be
    /// read is skipped, keeping the last counts.
    fn inventory(
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
        let count_of = |list: &[(String, i32)], item: &str| {
            list.iter()
                .find(|(name, _)| name == item)
                .map_or(0, |entry| entry.1)
        };
        let mut gained = Vec::new();
        let mut lost = None;
        if let Some(last) = &self.last_counts
            && !input.screen_open
        {
            for (item, total) in &counts {
                let gain = total - count_of(last, item);
                if gain > 0 {
                    gained.push((item.clone(), gain));
                }
            }
            if let Some(item) = self.drop_check.take() {
                let gone = count_of(last, &item) - count_of(&counts, &item);
                if gone > 0 {
                    lost = Some((item, gone));
                }
            }
        }
        self.last_counts = Some(counts);
        for (item, amount) in gained {
            self.say_counted("pickup", &item, amount, input.now, pack, config, out);
        }
        if let Some((item, amount)) = lost {
            self.say_counted("drop", &item, amount, input.now, pack, config, out);
        }
    }

    /// A pickup, drop or held item (Java `sayCounted`). One thing says `<action>.<key>`. More, the
    /// counted sentence: the count start and the plural as one joined clip, else the whole
    /// sentence and the amount, else the whole sentence alone. A pickup goes by its freshness,
    /// the others are said on every change.
    #[allow(clippy::too_many_arguments)]
    fn say_counted(
        &mut self,
        action: &str,
        item: &str,
        amount: i32,
        now: i64,
        pack: &Pack,
        config: &Config,
        out: &mut Vec<Speech>,
    ) {
        let (Some(noun), Some(whole)) = (pack.for_item("hold", item), pack.for_item(action, item))
        else {
            return;
        };
        let fresh = if action == "pickup" {
            self.freshness(&whole.id, Driver::Result, now, config)
        } else {
            Fresh::Repeat
        };
        if fresh == Fresh::Same {
            return;
        }
        let is_new = fresh == Fresh::New;
        let key = noun.key.as_deref().unwrap_or_default();
        let measure = pack.measure_of(key);
        if amount > 1 {
            let start = pack.count_start(action, amount, measure);
            let head_first = pack.meta.count_head_first && action != "hold";
            let other = if head_first {
                pack.phrase(&format!("head.{key}"))
            } else {
                pack.count_noun(action, key)
            };
            if let (Some(start), Some(other)) = (start, other) {
                let (first, second) = if head_first {
                    (other, start)
                } else {
                    (start, other)
                };
                let end = &whole.text[trim_end(&whole.text).len()..];
                let noun_en = match pack.count_en(key) {
                    Some(counted) if !counted.is_empty() => counted.to_string(),
                    _ => plural(&noun.en, amount),
                };
                let mut parts = first.parts.clone();
                parts.extend(second.parts.iter().cloned());
                let display = Phrase {
                    id: whole.id.clone(),
                    action: whole.action.clone(),
                    key: whole.key.clone(),
                    text: format!(
                        "{}{}{}{end}",
                        first.text,
                        pack.meta.word_separator,
                        trim_end(&second.text)
                    ),
                    reading: String::new(),
                    en: counted_en(action, amount, &noun_en, pack),
                    parts,
                };
                if action == "hold" {
                    out.push(Speech::HeldLabel(display.clone()));
                }
                out.push(Speech::Counted {
                    display: Box::new(display),
                    clips: vec![first.clone(), second.clone()],
                    joined: true,
                    fresh: is_new,
                });
                return;
            }
        }
        let unit = if amount > 1 {
            pack.count_unit(amount, measure)
        } else {
            None
        };
        let Some(unit) = unit else {
            if action == "hold" {
                out.push(Speech::HeldLabel(whole.clone()));
            }
            out.push(if is_new {
                Speech::New(whole.clone())
            } else {
                Speech::Repeat(whole.clone())
            });
            return;
        };
        let separator = if pack.meta.word_separator.is_empty() {
            "\u{FF0C}"
        } else {
            ", "
        };
        let mut parts: Vec<Part> = whole.parts.clone();
        if let Some(last) = parts.last_mut() {
            last.text
                .push_str(if separator == ", " { "," } else { separator });
        }
        parts.extend(unit.parts.iter().cloned());
        let display = Phrase {
            id: whole.id.clone(),
            action: action.to_string(),
            key: whole.key.clone(),
            text: format!("{}{separator}{}", trim_end(&whole.text), unit.text),
            reading: String::new(),
            en: counted_en(action, amount, &plural(&noun.en, amount), pack),
            parts,
        };
        if action == "hold" {
            out.push(Speech::HeldLabel(display.clone()));
        }
        out.push(Speech::Counted {
            display: Box::new(display),
            clips: vec![whole.clone(), unit.clone()],
            joined: false,
            fresh: is_new,
        });
    }

    /// A jump: the jump key held as the player left the ground, not in water, a vehicle or a
    /// sprint, at most every `jumpCooldownSeconds`.
    fn jump(&mut self, input: &WorldTick<'_>, pack: &Pack, config: &Config, out: &mut Vec<Speech>) {
        let now = input.now;
        if input.jump_key
            && self.was_on_ground
            && !input.on_ground
            && !input.in_water
            && !riding(input)
            && !input.sprinting
            && ((now - self.last_jump_at) as f32) > config.jump_cooldown_seconds * 1000.0
        {
            self.last_jump_at = now;
            out.extend(pack.phrase("jump").cloned().map(Speech::Repeat));
        }
        self.was_on_ground = input.on_ground;
    }

    /// Death (said at once and recorded), and damage over half a heart: `fall` within
    /// `FALL_HURT_MS` of a fast fall, else `hurt`, as a series. Water ends a fall unhurt.
    fn damage(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
        out: &mut Vec<Speech>,
    ) {
        let health = input.health;
        if input.dead && !self.was_dead {
            out.extend(pack.phrase("die").cloned().map(Speech::Now));
            out.push(Speech::Died);
        } else if !input.dead && self.last_health >= 0.0 && health < self.last_health - 0.5 {
            let fell = self
                .fell_at
                .take()
                .is_some_and(|at| input.now - at <= FALL_HURT_MS);
            let id = if fell { "fall" } else { "hurt" };
            if let Some(phrase) = pack.phrase(id) {
                out.extend(self.say(phrase, Driver::Series, input.now, config));
            }
        }
        self.was_dead = input.dead;
        self.last_health = health;
        if !input.on_ground && input.velocity[1] < -0.6 {
            self.fell_at = Some(input.now);
        } else if input.in_water {
            self.fell_at = None;
        }
    }

    /// A watched entity that is dying now is the player's kill: `kill.<entity>`, else `kill_it`.
    /// One hit by the player's projectile this tick is watched too. A watch ends after 6 s or
    /// when the entity is gone.
    fn kills(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
        out: &mut Vec<Speech>,
    ) {
        let now = input.now;
        if config.say_kills {
            for &(id, entity) in input.projectile_hits {
                self.watch(id, entity, now);
            }
        }
        let fate = |id: u64| {
            input
                .entity_fates
                .iter()
                .find(|(seen, _)| *seen == id)
                .map(|(_, fate)| *fate)
        };
        let mut killed = Vec::new();
        self.recently_hit
            .retain(|(id, entity, at)| match fate(*id) {
                Some(Fate::Dying) => {
                    killed.push(entity.clone());
                    false
                }
                Some(Fate::Removed) => false,
                None => now - at <= KILL_CREDIT_MS,
            });
        if !config.say_kills {
            return;
        }
        for entity in killed {
            let phrase = pack
                .for_entity("kill", &entity)
                .or_else(|| pack.phrase("kill_it"));
            if let Some(phrase) = phrase {
                out.extend(self.say(phrase, Driver::Result, now, config));
            }
        }
    }

    /// Day and night, the weather where the player is, the climate after 3 s in it, and the
    /// weather report every `weatherReportMinutes`.
    fn weather(
        &mut self,
        input: &WorldTick<'_>,
        pack: &Pack,
        config: &Config,
        out: &mut Vec<Speech>,
    ) {
        let now = input.now;
        let mut say = |id: &str| out.extend(pack.phrase(id).cloned().map(Speech::Repeat));
        let dark = input.dark;
        if self.was_dark.is_some_and(|was| was != dark) {
            say(if dark { "night" } else { "day" });
        }
        self.was_dark = Some(dark);
        let now_falling = precipitation(input);
        if let Some(before) = self.precipitation
            && before != now_falling
        {
            say(match now_falling {
                "none" if before == "snow" => "snow_stop",
                "none" => "rain_stop",
                "rain" if before == "none" => "rain",
                "rain" => "weather_rain",
                "snow" if before == "none" => "snow",
                "snow" => "weather_snow",
                _ => "thunder",
            });
        }
        self.precipitation = Some(now_falling);
        let here = climate(input);
        if self.pending_climate != Some(here) {
            self.pending_climate = Some(here);
            self.pending_climate_since = now;
        }
        match self.climate {
            None => {
                self.climate = Some(here);
                self.climate_said_at = now;
            }
            Some(before)
                if config.say_climate
                    && before != here
                    && now - self.pending_climate_since >= CLIMATE_DEBOUNCE_MS =>
            {
                self.climate = Some(here);
                self.climate_said_at = now;
                say(match here {
                    "hot" | "cold" => here,
                    _ if before == "hot" => "not_hot",
                    _ => "not_cold",
                });
            }
            Some(before)
                if config.say_climate
                    && config.climate_repeat_seconds > 0.0
                    && before != "mild"
                    && ((now - self.climate_said_at) as f32)
                        >= config.climate_repeat_seconds * 1000.0 =>
            {
                self.climate_said_at = now;
                say(before);
            }
            Some(_) => {}
        }
        if config.weather_report_minutes > 0.0
            && ((now - self.last_weather_report_at) as f32)
                >= config.weather_report_minutes * 60_000.0
        {
            self.last_weather_report_at = now;
            say(match now_falling {
                "rain" => "weather_rain",
                "snow" => "weather_snow",
                "thunder" => "weather_thunder",
                _ if dark => "weather_night",
                _ => "weather_clear",
            });
            if config.say_climate && here != "mild" {
                say(here);
            }
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
                    Driver::Series => now - at >= series,
                    Driver::Result => {
                        self.press_tick > tick
                            || (!self.attack_down && !self.use_down && now - at >= series)
                    }
                    Driver::None => true,
                    held => self.free_tick[held as usize] > tick,
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
