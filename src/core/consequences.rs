//! What a quiz answer does to the player (Java `Consequences` and `MobSpawns`), decided as plain
//! data for the integrated server to apply.
//!
//! Only with an integrated server (single player, a LAN host included): elsewhere nothing
//! happens. A Hardcore world always uses the `hardcore` penalty. A wrong answer brings the
//! configured penalty: nothing (`none`), two distinct debuffs (`debuff`), five hostile mobs
//! (`mobs`), one more lost heart (`hearts`) or death (`hardcore`). A right answer with
//! `quizReward` brings two distinct buffs, or five friendly mobs in the `mobs` mode, whatever the
//! penalty.
use super::config::Config;
use super::progress::Progress;
use super::rng::Rng;

/// The id of the permanent max-health modifier the `hearts` penalty sets.
pub const LOST_HEARTS_MODIFIER: &str = "zhnarrator:lost_hearts";
/// Mobs summoned per answer.
const MOBS: usize = 5;
const EFFECTS: usize = 2;
const CHARGED_CREEPER: &str = "charged_creeper";

/// A mob effect: the registry id, how long and how strong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub id: &'static str,
    pub seconds: i32,
    /// From 1.
    pub level: i32,
}

impl Effect {
    const fn new(id: &'static str, seconds: i32, level: i32) -> Effect {
        Effect { id, seconds, level }
    }

    /// The effect instance's duration.
    pub fn duration_ticks(&self) -> i32 {
        self.seconds * 20
    }

    /// The effect instance's amplifier: the level less one.
    pub fn amplifier(&self) -> i32 {
        self.level - 1
    }
}

pub(crate) const DEBUFFS: [Effect; 16] = [
    Effect::new("minecraft:slowness", 20, 5),
    Effect::new("minecraft:weakness", 30, 3),
    Effect::new("minecraft:mining_fatigue", 30, 3),
    Effect::new("minecraft:nausea", 30, 1),
    Effect::new("minecraft:blindness", 15, 1),
    Effect::new("minecraft:darkness", 20, 1),
    Effect::new("minecraft:hunger", 40, 3),
    Effect::new("minecraft:poison", 10, 2),
    Effect::new("minecraft:wither", 6, 1),
    Effect::new("minecraft:levitation", 5, 1),
    Effect::new("minecraft:glowing", 120, 1),
    Effect::new("minecraft:unluck", 300, 3),
    Effect::new("minecraft:infested", 90, 1),
    Effect::new("minecraft:oozing", 90, 1),
    Effect::new("minecraft:weaving", 90, 1),
    Effect::new("minecraft:wind_charged", 90, 1),
];

pub(crate) const BUFFS: [Effect; 18] = [
    Effect::new("minecraft:regeneration", 15, 3),
    Effect::new("minecraft:speed", 60, 3),
    Effect::new("minecraft:jump_boost", 60, 3),
    Effect::new("minecraft:haste", 60, 3),
    Effect::new("minecraft:strength", 60, 2),
    Effect::new("minecraft:resistance", 60, 2),
    Effect::new("minecraft:fire_resistance", 120, 1),
    Effect::new("minecraft:absorption", 60, 4),
    Effect::new("minecraft:health_boost", 90, 4),
    Effect::new("minecraft:saturation", 5, 1),
    Effect::new("minecraft:night_vision", 120, 1),
    Effect::new("minecraft:water_breathing", 120, 1),
    Effect::new("minecraft:slow_falling", 60, 1),
    Effect::new("minecraft:invisibility", 60, 1),
    Effect::new("minecraft:luck", 300, 3),
    Effect::new("minecraft:hero_of_the_village", 300, 1),
    Effect::new("minecraft:dolphins_grace", 60, 1),
    Effect::new("minecraft:conduit_power", 60, 1),
];

/// What the angry teacher does to the inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wipe {
    /// Everything is dropped on the ground (`Inventory.dropAll`).
    Drop,
    /// Everything is gone (`Inventory.clearContent`).
    Delete,
}

/// One thing the integrated server does to the player, on its own thread, in list order.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerAction {
    /// `addEffect(MobEffectInstance(id, duration_ticks, amplifier))`.
    Effect(Effect),
    /// Five mobs around the player, hostile or friendly: build the pool with [`mob_pool`] from
    /// the server's entity types, then place them with [`summons`] and this seed.
    Mobs { hostile: bool, seed: u64 },
    /// `lost` hearts are gone: set the permanent max-health modifier [`LOST_HEARTS_MODIFIER`]
    /// (add value) to `modifier`, then do what [`after_lost_hearts`] says.
    LostHearts { lost: i32, modifier: f64 },
    /// `kill` the player.
    Kill,
    /// The angry teacher's wipe, then `containerMenu.broadcastChanges()`.
    Wipe(Wipe),
}

/// What an answer does on the integrated server, in order; `None` without one (the original
/// logs "Quiz rewards and penalties only work in singleplayer for now"). Counts the lost heart
/// in this world's progress.
pub(crate) fn decide(
    correct: bool,
    integrated_server: bool,
    hardcore: bool,
    config: &Config,
    progress: &mut Progress,
    rng: &mut Rng,
) -> Option<Vec<ServerAction>> {
    if !integrated_server {
        return None;
    }
    let penalty = if hardcore {
        "hardcore"
    } else {
        config.quiz_penalty.as_str()
    };
    let actions = if !correct {
        match penalty {
            "hardcore" => vec![ServerAction::Kill],
            "hearts" => {
                let lost = progress.lose_heart();
                vec![ServerAction::LostHearts {
                    lost,
                    modifier: -2.0 * f64::from(lost),
                }]
            }
            "debuff" => effects(&DEBUFFS, rng),
            "mobs" => vec![ServerAction::Mobs {
                hostile: true,
                seed: rng.next_u64(),
            }],
            _ => Vec::new(),
        }
    } else if config.quiz_reward && penalty == "mobs" {
        vec![ServerAction::Mobs {
            hostile: false,
            seed: rng.next_u64(),
        }]
    } else if config.quiz_reward {
        effects(&BUFFS, rng)
    } else {
        Vec::new()
    };
    Some(actions)
}

/// Two distinct effects of the list.
fn effects(pool: &[Effect], rng: &mut Rng) -> Vec<ServerAction> {
    let mut picks = pool.to_vec();
    rng.shuffle(&mut picks);
    picks.truncate(EFFECTS);
    picks.into_iter().map(ServerAction::Effect).collect()
}

/// What the server does after the lost-hearts modifier is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeartsAfter {
    /// Max health is 0.5 or less: kill the player.
    Kill,
    /// Health is above the new max: set it to the max.
    SetHealth(f32),
    Keep,
}

/// `max_health` is the attribute's value with the modifier set, `health` the player's now.
pub fn after_lost_hearts(max_health: f64, health: f32) -> HeartsAfter {
    if max_health <= 0.5 {
        HeartsAfter::Kill
    } else if f64::from(health) > max_health {
        HeartsAfter::SetHealth(max_health as f32)
    } else {
        HeartsAfter::Keep
    }
}

/// The mobs an answer may bring, from the server's entity types as `(id, category)`: only the
/// types that can be summoned and are enabled in the world's features, with the category's
/// serialized name (`monster`, `creature`, ...). Hostile: monsters but the ender dragon, the
/// pufferfish, and a charged creeper. Friendly: creatures, ambient and water mobs, axolotls,
/// villagers and golems, never the pufferfish.
pub fn mob_pool<'a>(
    types: impl IntoIterator<Item = (&'a str, &'a str)>,
    hostile: bool,
) -> Vec<String> {
    const FRIENDLY: [&str; 6] = [
        "creature",
        "ambient",
        "axolotls",
        "underground_water_creature",
        "water_creature",
        "water_ambient",
    ];
    const FRIENDLY_EXTRA: [&str; 4] = [
        "minecraft:villager",
        "minecraft:iron_golem",
        "minecraft:snow_golem",
        "minecraft:copper_golem",
    ];
    const PUFFERFISH: &str = "minecraft:pufferfish";
    let mut pool: Vec<String> = types
        .into_iter()
        .filter(|&(id, category)| {
            let category = category.to_ascii_lowercase();
            if hostile {
                (category == "monster" && id != "minecraft:ender_dragon") || id == PUFFERFISH
            } else {
                (FRIENDLY.contains(&category.as_str()) || FRIENDLY_EXTRA.contains(&id))
                    && id != PUFFERFISH
            }
        })
        .map(|(id, _)| id.to_string())
        .collect();
    if hostile {
        pool.push(CHARGED_CREEPER.to_string());
    }
    pool
}

/// A mob to summon (`summon <entity> ~ ~ ~`, with `{powered:1b}` when charged) at `at`.
#[derive(Clone, Debug, PartialEq)]
pub struct Summon {
    pub entity: String,
    pub charged: bool,
    pub at: [f64; 3],
}

/// Five mobs from `pool`, picked with replacement, each placed 3 to 7 blocks from the player at a
/// random angle: the first spot of 24 tries, scanning from 3 above the player's block down to 4
/// below, with two blocks free of collision over one that is not; else the player's position.
/// `free` says whether a block has no collision shape.
pub fn summons(
    pool: &[String],
    seed: u64,
    block: [i32; 3],
    position: [f64; 3],
    mut free: impl FnMut([i32; 3]) -> bool,
) -> Vec<Summon> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    if pool.is_empty() {
        return out;
    }
    for _ in 0..MOBS {
        let id = &pool[rng.below(pool.len())];
        let at = spot(&mut rng, block, &mut free).unwrap_or(position);
        let charged = id == CHARGED_CREEPER;
        out.push(Summon {
            entity: if charged {
                "minecraft:creeper".to_string()
            } else {
                id.clone()
            },
            charged,
            at,
        });
    }
    out
}

fn spot(
    rng: &mut Rng,
    [bx, by, bz]: [i32; 3],
    free: &mut impl FnMut([i32; 3]) -> bool,
) -> Option<[f64; 3]> {
    // `Math.round`: halves go up.
    let round = |v: f64| (v + 0.5).floor() as i32;
    for _ in 0..24 {
        let angle = rng.next_f64() * std::f64::consts::PI * 2.0;
        let distance = f64::from(3 + rng.below(5) as i32);
        let x = bx + round(angle.cos() * distance);
        let z = bz + round(angle.sin() * distance);
        for dy in (-4..=3).rev() {
            let y = by + dy;
            if free([x, y, z]) && free([x, y + 1, z]) && !free([x, y - 1, z]) {
                return Some([f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5]);
            }
        }
    }
    None
}
