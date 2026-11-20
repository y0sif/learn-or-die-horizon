//! The integrated server's half of an answer's consequences (Java `Consequences.onAnswer`'s
//! `server.execute` body, `apply`, `wipeInventory` and `MobSpawns.spawn`): what the client half
//! sent ([`super::payload`]) is applied to the host's own player, on the server thread, in order.
//!
//! Only the player whose client runs in this process counts: the integrated server's host, in
//! single player or on a LAN world (the original's `getSingleplayerServer()`). A dedicated server
//! has no client in its process and a LAN guest is another player, so both are ignored.
//!
//! Effects, kills and mobs go through the server's command dispatcher as the console (permission
//! 4, output suppressed), as the original summons its mobs; `/effect give` and `/kill` build the
//! same `MobEffectInstance` and make the same `kill(level)` call the original makes directly. A
//! command names the player by UUID, never by name: an offline name may read as a selector
//! (`@a`). The lost hearts and the inventory use the API. The drop waits for the host's own tick
//! ([`host_tick`]), where their state is in place.
//!
//! The same process's client reads what only this server knows of its host, through shared state
//! ([`super::detect`]): each tick of the host's player their vehicle, fishing hook, fire and the
//! colours of the sheep within reach ([`host_tick`]), and each death the host dealt the killing
//! blow to ([`kill_credit`]). A tick reads little: one look at the entities near the player, whose
//! types and sheep colours are kept from tick to tick, and the player's hook once found ([`Seen`]).
use super::detect::{self, HostFacts};
use super::payload::{Action, Consequences};
use super::{lock, log_info, log_warn};
use crate::core::{HeartsAfter, LOST_HEARTS_MODIFIER, Rng, after_lost_hearts, mob_pool, summons};
use mod_api::{
    AttributeModifier, AttributeOperation, BlockShapeKind, EntityRef, Hand, Identifier, ItemStack,
    ModifierLifetime, PlayerRef, ServerHost, Tag,
};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The local player of this process's client, by UUID: the client half sets it in single player
/// and as it sends consequences.
pub(crate) static HOST_PLAYER: Mutex<Option<u128>> = Mutex::new(None);

/// The angry teacher's drop waits for the host's next tick ([`host_tick`]).
static DROP_WAITING: Mutex<bool> = Mutex::new(false);

/// What the host's ticks keep from one to the next ([`ticked`]).
static SEEN: Mutex<Seen> = Mutex::new(Seen::new());

const MAX_HEALTH: &str = "minecraft:max_health";
const SHEEP: &str = "minecraft:sheep";
const FISHING_ROD: &str = "minecraft:fishing_rod";

/// `MobEffect.isInstantenous`: vanilla's `InstantenousMobEffect`s.
const INSTANT_EFFECTS: [&str; 3] = [
    "minecraft:instant_health",
    "minecraft:instant_damage",
    "minecraft:saturation",
];

/// `DyeColor` by id, as a sheep's `Color` names it.
const DYE_COLOURS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// How far from the player's eyes the entities are looked at: past the creative interaction
/// range (5) and a sheep's box, and where a hook just cast is in its first ticks of flight.
const NEAR_REACH: f64 = 8.0;

/// Vanilla's entity types by `MobCategory` (`EntityType.getCategory`), as far as the mob pool asks:
/// the API reads an entity's category but no type's. Every other type counts as `misc`, so a type
/// this list does not name (a mod's, or one newer than 26.3) is left out of both pools.
const CATEGORIES: [(&str, &[&str]); 7] = [
    (
        "monster",
        &[
            "blaze",
            "bogged",
            "breeze",
            "camel_husk",
            "cave_spider",
            "creaking",
            "creeper",
            "drowned",
            "elder_guardian",
            "ender_dragon",
            "enderman",
            "endermite",
            "evoker",
            "ghast",
            "giant",
            "guardian",
            "hoglin",
            "husk",
            "illusioner",
            "magma_cube",
            "parched",
            "phantom",
            "piglin",
            "piglin_brute",
            "pillager",
            "ravager",
            "shulker",
            "silverfish",
            "skeleton",
            "slime",
            "spider",
            "stray",
            "sulfur_cube",
            "vex",
            "vindicator",
            "warden",
            "witch",
            "wither",
            "wither_skeleton",
            "zoglin",
            "zombie",
            "zombie_horse",
            "zombie_nautilus",
            "zombie_villager",
            "zombified_piglin",
        ],
    ),
    (
        "creature",
        &[
            "allay",
            "armadillo",
            "bee",
            "camel",
            "cat",
            "chicken",
            "cow",
            "donkey",
            "fox",
            "frog",
            "goat",
            "happy_ghast",
            "horse",
            "llama",
            "mooshroom",
            "mule",
            "ocelot",
            "panda",
            "parrot",
            "pig",
            "polar_bear",
            "rabbit",
            "sheep",
            "skeleton_horse",
            "sniffer",
            "strider",
            "tadpole",
            "trader_llama",
            "turtle",
            "wandering_trader",
            "wolf",
        ],
    ),
    ("ambient", &["bat"]),
    ("axolotls", &["axolotl"]),
    ("underground_water_creature", &["glow_squid"]),
    ("water_creature", &["dolphin", "nautilus", "squid"]),
    (
        "water_ambient",
        &["cod", "pufferfish", "salmon", "tropical_fish"],
    ),
];

/// A vanilla entity type's category's serialized name, `misc` for any other.
fn category(id: &str) -> &'static str {
    let Some(path) = id.strip_prefix("minecraft:") else {
        return "misc";
    };
    CATEGORIES
        .iter()
        .find(|(_, types)| types.contains(&path))
        .map_or("misc", |&(category, _)| category)
}

/// The player the actions land on, as commands name them.
struct Target {
    player: PlayerRef,
    /// `UUID.toString`, which the entity argument reads as that entity alone.
    uuid: String,
    dimension: String,
}

/// `UUID.toString`: lower-case hex in groups of 8, 4, 4, 4 and 12 digits.
fn uuid_text(uuid: u128) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        uuid >> 96,
        (uuid >> 80) & 0xffff,
        (uuid >> 64) & 0xffff,
        (uuid >> 48) & 0xffff,
        uuid & 0xffff_ffff_ffff
    )
}

/// The payload's handler: [`receive`], and a drop it leaves waits for the host's next tick.
pub(crate) fn handle(host: &mut dyn ServerHost, player: PlayerRef, payload: &Consequences) {
    if receive(host, player, payload) {
        *lock(&DROP_WAITING) = true;
    }
}

/// Applies `payload` to `player` when they are this process's own client player, else nothing.
/// True when the angry teacher's drop is left for the player's next tick ([`ticked`]): the
/// handler runs outside any player's turn, and a drop thrown there left no item in game.
pub(crate) fn receive(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    payload: &Consequences,
) -> bool {
    if *lock(&HOST_PLAYER) != Some(player.uuid()) {
        log_info("consequences from a player who is not the host ignored".into());
        return false;
    }
    let Some(dimension) = player.dimension(&*host) else {
        return false;
    };
    let target = Target {
        player,
        uuid: uuid_text(player.uuid()),
        dimension: dimension.to_string(),
    };
    let mut effects = Vec::new();
    let mut drop_waits = false;
    for action in &payload.actions {
        match action {
            Action::Effect {
                id,
                seconds,
                amplifier,
            } => {
                if effect(host, &target, id, *seconds, *amplifier) {
                    effects.push(format!("{id} {}", amplifier + 1));
                }
            }
            Action::Mobs { hostile, seed } => mobs(host, &target, *hostile, *seed as u64),
            Action::LostHearts { lost, modifier } => lost_hearts(host, &target, *lost, *modifier),
            Action::Kill => kill(host, &target),
            Action::Wipe { delete: true } => delete_all(host, target.player),
            Action::Wipe { delete: false } => drop_waits = true,
        }
    }
    if !effects.is_empty() {
        log_info(format!("effects: {}", effects.join(", ")));
    }
    drop_waits
}

/// `addEffect(new MobEffectInstance(effect, seconds * 20, amplifier))`, through `/effect give`,
/// which builds that instance. It reads an instant effect's seconds as ticks
/// (`EffectCommands`' `computeDurationInTicks`), so an instant one is given `seconds * 20` there.
/// False for an id that is not one.
fn effect(host: &mut dyn ServerHost, target: &Target, id: &str, seconds: i32, amp: i32) -> bool {
    if Identifier::parse(id).is_err() {
        log_warn(format!("not an effect id: {id:?}"));
        return false;
    }
    let seconds = if INSTANT_EFFECTS.contains(&id) {
        seconds.saturating_mul(20)
    } else {
        seconds
    };
    let line = format!("effect give {} {id} {seconds} {amp}", target.uuid);
    host.commands().run_command(&line)
}

/// `p.kill(p.level())`: `/kill` in the player's own level.
fn kill(host: &mut dyn ServerHost, target: &Target) {
    let line = format!("execute in {} run kill {}", target.dimension, target.uuid);
    host.commands().run_command(&line);
}

/// `MobSpawns.spawn`: five mobs from the server's entity types, each summoned where
/// [`summons`] places it, from a source at that spot in the player's level.
fn mobs(host: &mut dyn ServerHost, target: &Target, hostile: bool, seed: u64) {
    let ids: Vec<String> = host
        .entity_type_keys()
        .into_iter()
        .map(|key| key.id(&*host))
        .collect();
    let pool = mob_pool(ids.iter().map(|id| (id.as_str(), category(id))), hostile);
    let entity = target.player.entity();
    let (Some(block), Some(position), Some(level)) = (
        entity.block_position(&*host),
        entity.position(&*host),
        target.player.level(&*host),
    ) else {
        return;
    };
    let reader = &*host;
    // `getCollisionShape(level, pos).isEmpty()`; a block the level does not answer for (out of
    // the world's height) is void air.
    let free = |pos: [i32; 3]| match level.block_state(reader, pos) {
        Some(state) => level
            .block_shape(reader, state, pos, BlockShapeKind::Collision)
            .is_none_or(|boxes| boxes.is_empty()),
        None => true,
    };
    let came = summons(&pool, seed, block, position, free);
    for mob in &came {
        let [x, y, z] = mob.at;
        let nbt = if mob.charged { " {powered:1b}" } else { "" };
        let line = format!(
            "execute in {} positioned {x} {y} {z} run summon {} ~ ~ ~{nbt}",
            target.dimension, mob.entity
        );
        host.commands().run_command(&line);
    }
    let names: Vec<&str> = came
        .iter()
        .map(|mob| mob.entity.trim_start_matches("minecraft:"))
        .collect();
    let kind = if hostile { "hostile" } else { "friendly" };
    log_info(format!("{kind} mobs: {names:?}"));
}

/// The `hearts` penalty: the permanent max-health modifier set to `modifier` (add value), then
/// a kill at 0.5 or less, else the health brought down to the new max.
fn lost_hearts(host: &mut dyn ServerHost, target: &Target, lost: i32, modifier: f64) {
    let Some(living) = host.living(target.player.entity()) else {
        return;
    };
    let Ok(id) = Identifier::parse(LOST_HEARTS_MODIFIER) else {
        return;
    };
    let modifier = AttributeModifier::new(id, modifier, AttributeOperation::AddValue);
    let lifetime = ModifierLifetime::Permanent;
    if !living.replace_attribute_modifier(host, MAX_HEALTH, modifier, lifetime) {
        log_warn("the lost hearts modifier was refused".into());
        return;
    }
    let (Some(max), Some(health)) = (
        living.attribute_value(&*host, MAX_HEALTH),
        living.health(&*host),
    ) else {
        return;
    };
    log_info(format!("hearts lost: {lost}, max health {max}"));
    match after_lost_hearts(max, health) {
        HeartsAfter::Kill => kill(host, target),
        HeartsAfter::SetHealth(health) => {
            living.set_health(host, health);
        }
        HeartsAfter::Keep => {}
    }
}

/// `Inventory.clearContent()`, over every slot, armour and off hand included. Each emptied slot
/// reaches the client with the tick's inventory sync (`containerMenu.broadcastChanges()`).
fn delete_all(host: &mut dyn ServerHost, player: PlayerRef) {
    for (slot, _) in player.inventory_slots(&*host).unwrap_or_default() {
        let _ = player.set_inventory_slot(host, slot, None);
    }
}

/// `Inventory.dropAll()`: each stack thrown from the eyes at random, without the thrower's name
/// (`player.drop(stack, true, false)`), then its slot emptied; a stack nothing threw stays in its
/// slot. Armour and off hand included.
fn drop_all(host: &mut dyn ServerHost, player: PlayerRef) {
    let living = host.living(player.entity());
    let mut rng = Rng::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64),
    );
    let (mut by_game, mut by_hand, mut kept) = (0, 0, 0);
    for (slot, stack) in player.inventory_slots(&*host).unwrap_or_default() {
        let thrown = if living.is_some_and(|living| {
            matches!(living.drop(host, stack.clone(), true, false), Ok(Some(_)))
        }) {
            by_game += 1;
            true
        } else if throw(host, player, stack, &mut rng) {
            by_hand += 1;
            true
        } else {
            false
        };
        if !thrown || player.set_inventory_slot(host, slot, None).is_err() {
            kept += 1;
        }
    }
    let line = format!("angry drop: {by_game} stacks thrown, {by_hand} by hand, {kept} kept");
    if kept > 0 {
        log_warn(line);
    } else {
        log_info(line);
    }
}

/// `Player.drop(stack, true, false)` by hand, where `living.drop` threw nothing: an item at the
/// eyes less 0.3, thrown up 0.2 and across up to 0.5 a tick at a random angle, picked up after
/// 40 ticks. The spread comes from the mod's random, not the player's.
fn throw(host: &mut dyn ServerHost, player: PlayerRef, stack: ItemStack, rng: &mut Rng) -> bool {
    let (Some([x, y, z]), Some(level)) =
        (player.entity().eye_position(&*host), player.level(&*host))
    else {
        return false;
    };
    // `Random.nextFloat`: 24 bits.
    let mut next_float = || (rng.next_u64() >> 40) as f32 / (1u32 << 24) as f32;
    let spread = next_float() * 0.5;
    let angle = next_float() * std::f32::consts::TAU;
    let motion = [
        f64::from(-angle.sin() * spread),
        f64::from(0.2f32),
        f64::from(angle.cos() * spread),
    ];
    let y = y - f64::from(0.3f32);
    matches!(
        level.spawn_item_with_motion(host, x, y, z, stack, motion, 40),
        Ok(Some(_))
    )
}

/// The integrated server reset (started or stopped): no drop waits, nothing seen is kept, and the
/// client hears no more of its host.
pub(crate) fn reset() {
    *lock(&DROP_WAITING) = false;
    *lock(&SEEN) = Seen::new();
    detect::set_host_facts(None);
    detect::clear_host_kills();
}

/// A player right-clicked the entity `entity` on the server (`entity_interact`, before the item
/// acts): a sheep's colour is read again at the host's next tick, as a dye may change it.
pub(crate) fn entity_used(entity: i32) {
    if let Ok(id) = u32::try_from(entity) {
        lock(&SEEN).near.retain(|&(seen, _)| seen != id);
    }
}

/// `player_tick_post` on the integrated server: for this process's own client player, [`ticked`]
/// with the drop the payload left; `None` for any other player.
pub(crate) fn host_tick(host: &mut dyn ServerHost, player: PlayerRef) -> Option<HostFacts> {
    if *lock(&HOST_PLAYER) != Some(player.uuid()) {
        return None;
    }
    let drop_waits = std::mem::take(&mut *lock(&DROP_WAITING));
    Some(ticked(host, player, drop_waits))
}

/// The host's player ticked, their state in place: the angry teacher's waiting drop is thrown,
/// then what a client cannot read of them (Java's client reads `getVehicle`, `player.fishing`
/// and `isOnFire`), and the colours of the sheep within reach for the hit and feed sentences.
pub(crate) fn ticked(host: &mut dyn ServerHost, player: PlayerRef, drop_waits: bool) -> HostFacts {
    if drop_waits {
        drop_all(host, player);
    }
    let entity = player.entity();
    let near = match (entity.eye_position(&*host), player.level(&*host)) {
        (Some(eye), Some(level)) => {
            let (min, max) = (eye.map(|c| c - NEAR_REACH), eye.map(|c| c + NEAR_REACH));
            level.entities_in(&*host, min, max).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    let mut seen = lock(&SEEN);
    let sheep = seen.look(host, &near);
    let fishing = seen.fishing(&*host, player, &near);
    let vehicle = entity
        .vehicle(&*host)
        .and_then(|vehicle| seen.entity_path(host, vehicle));
    HostFacts {
        vehicle,
        fishing,
        on_fire: entity.fire_ticks(&*host).is_some_and(|ticks| ticks > 0),
        sheep,
    }
}

/// What the host's ticks keep, so that each reads little: the entities near the player's eyes,
/// each a sheep's colour or none (an entity's type is read as it comes near, and a sheep's saved
/// data once, until a right click may have dyed it), and the player's fishing hook once found.
struct Seen {
    near: Vec<(u32, Option<&'static str>)>,
    hook: Option<EntityRef>,
}

impl Seen {
    const fn new() -> Seen {
        Seen {
            near: Vec::new(),
            hook: None,
        }
    }

    /// `entity`'s colour if it is a sheep: kept, or read and kept.
    fn colour(&mut self, host: &mut dyn ServerHost, entity: EntityRef) -> Option<&'static str> {
        let id = entity.id();
        if let Some(&(_, colour)) = self.near.iter().find(|&&(seen, _)| seen == id) {
            return colour;
        }
        let sheep = entity
            .kind(&*host)
            .is_some_and(|kind| kind.id(&*host) == SHEEP);
        let colour = sheep.then(|| sheep_colour(host, entity)).flatten();
        self.near.push((id, colour));
        colour
    }

    /// The entities `near` now: the sheep among them, by entity id, with their colours. What is
    /// no longer near is forgotten.
    fn look(&mut self, host: &mut dyn ServerHost, near: &[EntityRef]) -> Vec<(u32, &'static str)> {
        let sheep: Vec<(u32, &'static str)> = near
            .iter()
            .filter_map(|&entity| Some((entity.id(), self.colour(host, entity)?)))
            .collect();
        self.near
            .retain(|&(id, _)| near.iter().any(|entity| entity.id() == id));
        sheep
    }

    /// `player.fishing != null`: a cast hook of the player's is out. A hook goes once neither hand
    /// holds a rod (`FishingHook.shouldStopFishing`), so one is looked for only while one does:
    /// the hook kept from the last tick, else one `near`, where a cast starts (at the eyes).
    fn fishing(&mut self, host: &dyn ServerHost, player: PlayerRef, near: &[EntityRef]) -> bool {
        let entity = player.entity();
        let rod = host.living(entity).is_some_and(|living| {
            Hand::BOTH.into_iter().any(|hand| {
                living
                    .hand_item(host, hand)
                    .is_some_and(|stack| stack.item().id(host) == FISHING_ROD)
            })
        });
        if !rod {
            self.hook = None;
            return false;
        }
        let his = |hook: EntityRef| {
            host.fishing_hook(hook)
                .and_then(|hook| hook.player_owner(host))
                .is_some_and(|owner| owner.id() == entity.id())
        };
        if !self.hook.is_some_and(his) {
            self.hook = near.iter().copied().find(|&near| his(near));
        }
        self.hook.is_some()
    }

    /// An entity's type id as the original names it (`ActionDetector.entityPath`): a sheep as
    /// `<colour>_sheep`.
    fn entity_path(&mut self, host: &mut dyn ServerHost, entity: EntityRef) -> Option<String> {
        if let Some(colour) = self.colour(host, entity) {
            return Some(format!("minecraft:{colour}_sheep"));
        }
        Some(entity.kind(&*host)?.id(&*host))
    }
}

/// A sheep's colour, from its saved `Color` (`DyeColor` by id).
fn sheep_colour(host: &mut dyn ServerHost, sheep: EntityRef) -> Option<&'static str> {
    match Tag::Compound(sheep.nbt(host)?).get("Color") {
        Some(&Tag::Byte(id)) => Some(DYE_COLOURS[(id as u8 & 15) as usize]),
        _ => None,
    }
}

/// `living_death`: the host's player dealt the killing blow (the source's `getEntity`: their own
/// hand, or a projectile they own), so the entity `entity` of type `kind` is their kill: its id
/// and type, a sheep as `<colour>_sheep`. `None` for any other death. The original credits the
/// player's own projectiles only, by their owner, which a client cannot read.
pub(crate) fn kill_credit(
    host: &mut dyn ServerHost,
    entity: u32,
    kind: &str,
    causing: i32,
) -> Option<(u32, String)> {
    let uuid = (*lock(&HOST_PLAYER))?;
    let own = host.player(uuid)?.entity().id();
    if causing <= 0 || causing as u32 != own || entity == own {
        return None;
    }
    let colour = match host.entity(entity) {
        Some(dead) if kind == SHEEP => sheep_colour(host, dead),
        _ => None,
    };
    let path = colour.map_or_else(|| kind.to_string(), |c| format!("minecraft:{c}_sheep"));
    Some((entity, path))
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
