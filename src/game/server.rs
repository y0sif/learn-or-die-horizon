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
//! same `MobEffectInstance` and make the same `kill(level)` call the original makes directly. The
//! lost hearts and the inventory use the API.
use super::payload::{Action, Consequences};
use super::{lock, log_info, log_warn};
use crate::core::{HeartsAfter, LOST_HEARTS_MODIFIER, after_lost_hearts, mob_pool, summons};
use mod_api::{
    AttributeModifier, AttributeOperation, BlockShapeKind, Identifier, ModifierLifetime, PlayerRef,
    ServerHost,
};
use std::sync::Mutex;

/// The local player of this process's client, by UUID, set as it sends consequences.
pub(crate) static HOST_PLAYER: Mutex<Option<u128>> = Mutex::new(None);

const MAX_HEALTH: &str = "minecraft:max_health";

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
            "vex",
            "vindicator",
            "warden",
            "witch",
            "wither",
            "wither_skeleton",
            "zoglin",
            "zombie",
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
            "trader_llama",
            "turtle",
            "wandering_trader",
            "wolf",
            "zombie_horse",
        ],
    ),
    ("ambient", &["bat"]),
    ("axolotls", &["axolotl"]),
    ("underground_water_creature", &["glow_squid"]),
    ("water_creature", &["dolphin", "nautilus", "squid"]),
    (
        "water_ambient",
        &["cod", "pufferfish", "salmon", "tadpole", "tropical_fish"],
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
    name: String,
    dimension: String,
}

/// The payload's handler: applies `payload` to `player` when they are this process's own client
/// player, else nothing.
pub(crate) fn receive(host: &mut dyn ServerHost, player: PlayerRef, payload: &Consequences) {
    if *lock(&HOST_PLAYER) != Some(player.uuid()) {
        log_info("consequences from a player who is not the host ignored".into());
        return;
    }
    let (Some(name), Some(dimension)) = (player.name(&*host), player.dimension(&*host)) else {
        return;
    };
    let target = Target {
        player,
        name,
        dimension: dimension.to_string(),
    };
    let mut effects = Vec::new();
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
            Action::Wipe { delete } => wipe(host, &target, *delete),
        }
    }
    if !effects.is_empty() {
        log_info(format!("effects: {}", effects.join(", ")));
    }
}

/// `addEffect(new MobEffectInstance(effect, seconds * 20, amplifier))`, through `/effect give`,
/// which builds that instance. False for an id that is not one.
fn effect(host: &mut dyn ServerHost, target: &Target, id: &str, seconds: i32, amp: i32) -> bool {
    if Identifier::parse(id).is_err() {
        log_warn(format!("not an effect id: {id:?}"));
        return false;
    }
    let line = format!("effect give {} {id} {seconds} {amp}", target.name);
    host.commands().run_command(&line)
}

/// `p.kill(p.level())`: `/kill` in the player's own level.
fn kill(host: &mut dyn ServerHost, target: &Target) {
    let line = format!("execute in {} run kill {}", target.dimension, target.name);
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

/// `Inventory.dropAll()` (each stack thrown at random, without the thrower's name) or
/// `clearContent()`, over every slot, armour and off hand included. Each emptied slot reaches the
/// client with the tick's inventory sync (`containerMenu.broadcastChanges()`).
fn wipe(host: &mut dyn ServerHost, target: &Target, delete: bool) {
    let player = target.player;
    let living = host.living(player.entity());
    for (slot, stack) in player.inventory_slots(&*host).unwrap_or_default() {
        if player.set_inventory_slot(host, slot, None).is_err() || delete {
            continue;
        }
        if let Some(living) = living {
            let _ = living.drop(host, stack, true, false);
        }
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
