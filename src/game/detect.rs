//! What the player did, read on the client as the original reads it: the block they broke (no
//! client break event: the blocks the attack key acted on, and the block being mined, gone by
//! the tick's end), the block they placed (a right click with a block item, then the block
//! showing up within 500 ms), the block or entity they used, the entity they attacked, and the
//! tick's state for the core's detector (the player's state, the keys, the weather and the
//! biome, the inventory for pickups, the block being mined, the fates of the entities he hit).
//!
//! A client reads no health of another entity, so whether one is dying comes from how the game
//! draws it (`render_living_pre`: its death time), kept for one tick.
//!
//! Some of the player's own state reaches the layer through hooks rather than reads, and is put
//! into the tick's view by [`merge_events`]: the drop key (`key_mapping_pre`: the `mapping` read
//! answers the movement keys, attack and use only), the fire over the view (a client reads no
//! fire ticks), and what the integrated server knows of its host's player ([`HostFacts`]).
//!
//! Kills: the player's melee hits are watched (`entity_attacked`) and one drawn dying within 6 s
//! is his kill, as the original. A client cannot read a projectile's owner, so the kills by the
//! player's own projectiles come only from the integrated server ([`host_killed`], single
//! player): off it, no projectile counts. In single player a sheep is named by its colour, also
//! from the integrated server.
use super::client::{Client, View};
use super::{STATE, State, lock};
use crate::core::{Fate, Teacher, WorldTick};
use mod_api::game::MobRender;
use mod_api::{BlockClick, ClientHost, EntityRef, InteractionKey, ItemStack};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

const PLACE_WAIT_MS: i64 = 500;
const SHEEP: &str = "minecraft:sheep";

/// The Hardcore flag of the world, as the health bar's inputs last showed it.
static HARDCORE: AtomicBool = AtomicBool::new(false);

/// The drop key (`key.drop`) is held, and it went down since the last tick read it (a tap
/// between two ticks counts too).
static DROP_HELD: AtomicBool = AtomicBool::new(false);
static DROP_PRESSED: AtomicBool = AtomicBool::new(false);
/// When a frame last drew the fire over the first-person view, in the seam's ms.
static FIRE_DRAWN_AT: AtomicI64 = AtomicI64::new(i64::MIN);
/// How long a drawn fire keeps the player burning: a few frames, so a slow frame between two
/// ticks does not put the fire out.
const FIRE_FRAME_MS: i64 = 250;

/// What the integrated server knows of its host's own player that a client cannot read
/// (`Entity.getVehicle`, `Player.fishing`, `Entity.isOnFire` in either camera, a sheep's colour).
/// The server's half sets it every tick of that player, in single player and on the host of a
/// LAN world; the client reads it only there.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HostFacts {
    /// The vehicle's entity type id (a sheep is `<colour>_sheep`).
    pub(crate) vehicle: Option<String>,
    /// A cast fishing hook of the player's is out.
    pub(crate) fishing: bool,
    pub(crate) on_fire: bool,
    /// The sheep within the player's reach, by entity id, and their colours (`DyeColor`'s
    /// names), for the hit and feed sentences.
    pub(crate) sheep: Vec<(u32, &'static str)>,
}

static HOST_FACTS: Mutex<Option<HostFacts>> = Mutex::new(None);

/// The kills the integrated server credited to its host's player since the client last took them:
/// `(entity id, entity type id)`, a sheep as `<colour>_sheep`. At most [`HOST_KILLS_KEPT`].
static HOST_KILLS: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());
const HOST_KILLS_KEPT: usize = 64;

/// The window owns the integrated server it plays on, as the last tick's view showed it: the
/// hooks between ticks read the server's facts only then.
static SINGLEPLAYER: AtomicBool = AtomicBool::new(false);

/// From the integrated server's half: its host's player now, `None` once it stops.
pub(crate) fn set_host_facts(facts: Option<HostFacts>) {
    *lock(&HOST_FACTS) = facts;
}

/// From the integrated server's half (`living_death`): its host's player dealt the killing blow
/// to the entity `entity` of type `kind`.
pub(crate) fn host_killed(entity: u32, kind: String) {
    let mut kills = lock(&HOST_KILLS);
    if kills.len() >= HOST_KILLS_KEPT {
        kills.remove(0);
    }
    kills.push((entity, kind));
}

/// From the integrated server's half as it starts or stops: no kill of its is said any more.
pub(crate) fn clear_host_kills() {
    lock(&HOST_KILLS).clear();
}

/// The kills the integrated server credited to this window's player since the last world tick,
/// as `(entity, entity type id)`; none off single player.
pub(crate) fn credited_kills(view: &View) -> Vec<(u64, String)> {
    let kills = std::mem::take(&mut *lock(&HOST_KILLS));
    if !view.singleplayer {
        return Vec::new();
    }
    kills
        .into_iter()
        .map(|(entity, kind)| (u64::from(entity), kind))
        .collect()
}

/// The entity type id the teacher names an entity by (Java `entityPath`): in single player a
/// sheep the integrated server told the colour of as `<colour>_sheep`, else `kind`.
fn entity_path(entity: u32, kind: &str) -> String {
    if kind == SHEEP
        && SINGLEPLAYER.load(Ordering::Relaxed)
        && let Some(facts) = lock(&HOST_FACTS).as_ref()
        && let Some((_, colour)) = facts.sheep.iter().find(|(id, _)| *id == entity)
    {
        return format!("minecraft:{colour}_sheep");
    }
    kind.to_string()
}

/// From the `key_mapping_pre` hook: a physical key or button bound to `mapping` went down or up.
pub(crate) fn key_mapping(mapping: &str, pressed: bool) {
    if mapping != "key.drop" {
        return;
    }
    DROP_HELD.store(pressed, Ordering::Relaxed);
    if pressed {
        DROP_PRESSED.store(true, Ordering::Relaxed);
    }
}

/// From the `extract_block_screen_effect` hook: a frame drew the fire over the view, which the
/// game does while the player burns (`isOnFire`), in first person.
pub(crate) fn fire_drawn() {
    FIRE_DRAWN_AT.store(super::io().now(), Ordering::Relaxed);
}

/// Puts what the hooks told since the last tick into the tick's view: the drop key, the fire
/// drawn over the view, and in single player the integrated server's facts of its host's player.
pub(crate) fn merge_events(view: &mut View, now: i64) {
    let pressed = DROP_PRESSED.swap(false, Ordering::Relaxed);
    if view.screen_open {
        // A screen lets go of every key (`KeyMapping.releaseAll`), and no release is told then.
        DROP_HELD.store(false, Ordering::Relaxed);
    } else {
        view.drop_key |= pressed || DROP_HELD.load(Ordering::Relaxed);
    }
    let since_fire = now.saturating_sub(FIRE_DRAWN_AT.load(Ordering::Relaxed));
    view.on_fire |= (0..=FIRE_FRAME_MS).contains(&since_fire);
    SINGLEPLAYER.store(view.singleplayer, Ordering::Relaxed);
    if !view.singleplayer {
        lock(&HOST_KILLS).clear();
        return;
    }
    // The integrated server's half knows its host by this. A view without a player's UUID
    // names no one.
    if view.uuid != 0 {
        *lock(&super::server::HOST_PLAYER) = Some(view.uuid);
    }
    if let Some(facts) = lock(&HOST_FACTS).clone() {
        view.vehicle = view.vehicle.take().or(facts.vehicle);
        view.in_vehicle |= view.vehicle.is_some();
        view.fishing |= facts.fishing;
        view.on_fire |= facts.on_fire;
    }
}

/// Living entities drawn dying (with a death time) since the last client tick, by network id.
static DYING: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// From the `health_hud` hook: whether the world is a Hardcore one.
pub(crate) fn set_hardcore(hardcore: bool) {
    HARDCORE.store(hardcore, Ordering::Relaxed);
}

pub(crate) fn hardcore() -> bool {
    HARDCORE.load(Ordering::Relaxed)
}

/// From the `render_living_pre` hook, for every living entity drawn: one dying is kept until the
/// tick's end. Most are not, and cost one comparison.
pub(crate) fn drawn(mob: &MobRender<'_>) {
    if mob.death_time <= 0 || !super::hosting() {
        return;
    }
    let mut dying = lock(&DYING);
    if !dying.contains(&mob.id) {
        dying.push(mob.id);
    }
}

/// The entity was drawn dying since the last tick.
pub(crate) fn seen_dying(id: u32) -> bool {
    lock(&DYING).contains(&id)
}

/// The tick is over: what was drawn before it is forgotten.
pub(crate) fn end_tick() {
    lock(&DYING).clear();
}

/// A block the player went for, and what they held.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    pos: [i32; 3],
    block: String,
    held: String,
}

/// A right click with a block item, waiting for its block (Java `PendingPlace`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingPlace {
    block: String,
    clicked: [i32; 3],
    in_front: [i32; 3],
    until: i64,
}

/// What a broken block leaves behind.
fn is_gone(block: &str) -> bool {
    matches!(
        block,
        "minecraft:air"
            | "minecraft:cave_air"
            | "minecraft:void_air"
            | "minecraft:water"
            | "minecraft:lava"
            | "minecraft:bubble_column"
    )
}

/// The attack key acts on the crosshair's block this tick (Java's client break event has no
/// counterpart): a click, or the key held digging on. The block is kept for the tick's end,
/// which counts it if it is gone, so a block that breaks at once is heard too (creative mode,
/// and grass, flowers, torches or crops by hand), though it never starts mining.
pub(crate) fn attack(host: &dyn ClientHost, key: InteractionKey) {
    if key != InteractionKey::Attack {
        return;
    }
    let io = super::io();
    let Some((pos, held)) = io.attack_target(host) else {
        return;
    };
    let Some(block) = io.block_at(host, pos).filter(|block| !is_gone(block)) else {
        return;
    };
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    if teacher.is_none() || game.level.is_none() {
        return;
    }
    if !game.attacked.iter().any(|target| target.pos == pos) {
        game.attacked.push(Target { pos, block, held });
    }
}

/// The blocks the player broke this tick, each with the item they held: a block the attack key
/// acted on this tick, or the block mined last tick, that is air or a fluid now. A block both
/// mined and attacked counts once. Then the block being mined now becomes the next target.
pub(crate) fn poll_break(
    host: &dyn ClientHost,
    io: &dyn Client,
    view: &View,
    target: &mut Option<Target>,
    attacked: &mut Vec<Target>,
) -> Vec<(String, String)> {
    let mut broken: Vec<Target> = Vec::new();
    for last in target.take().into_iter().chain(attacked.drain(..)) {
        let gone = io
            .block_at(host, last.pos)
            .is_some_and(|now| now != last.block && is_gone(&now));
        if gone && !broken.iter().any(|seen| seen.pos == last.pos) {
            broken.push(last);
        }
    }
    *target = view.mining.and_then(|pos| {
        let block = io.block_at(host, pos).filter(|block| !is_gone(block))?;
        Some(Target {
            pos,
            block,
            held: view.held_item.clone(),
        })
    });
    broken
        .into_iter()
        .map(|last| (last.block, last.held))
        .collect()
}

/// The block being mined, for the core's ongoing state.
pub(crate) fn mining(target: Option<&Target>) -> Option<&str> {
    target.map(|target| target.block.as_str())
}

/// A right click on a block, on the client (Java `onUseBlock`): a click the pack has a close,
/// open or use phrase for (main hand, not sneaking) says it and places nothing; any other with a
/// block item waits for its block.
pub(crate) fn right_click_block(host: &dyn ClientHost, click: &BlockClick<'_>) {
    let io = super::io();
    let Some(click) = io.click(host, click) else {
        return;
    };
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_mut() else {
        return;
    };
    if game.level.is_none() {
        return;
    }
    if click.main_hand && !click.sneaking && teacher.block_used(now, &click.target, click.open) {
        return;
    }
    if let Some(block) = click.places {
        game.pending_place = Some(PendingPlace {
            block,
            clicked: click.pos,
            in_front: click.in_front,
            until: now + PLACE_WAIT_MS,
        });
    }
}

/// The pending place's block showed up in front of the clicked face (or at the clicked block):
/// the teacher says so. Given up after 500 ms.
pub(crate) fn check_place(
    host: &dyn ClientHost,
    io: &dyn Client,
    pending: &mut Option<PendingPlace>,
    teacher: &mut Teacher,
    now: i64,
) {
    let Some(place) = pending.as_ref() else {
        return;
    };
    let is_there = |pos| io.block_at(host, pos).as_deref() == Some(place.block.as_str());
    if is_there(place.in_front) || is_there(place.clicked) {
        let block = place.block.clone();
        *pending = None;
        teacher.block_placed(now, &block);
    } else if now > place.until {
        *pending = None;
    }
}

/// The attack key hit an entity (Java `AttackEntityCallback`, from `client_attack_target`):
/// the crosshair's entity, the first target.
pub(crate) fn attacked(host: &dyn ClientHost, target: EntityRef) {
    let io = super::io();
    let Some(struck) = io.struck(host, target) else {
        return;
    };
    let kind = u32::try_from(struck.id)
        .map_or_else(|_| struck.kind.clone(), |id| entity_path(id, &struck.kind));
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    if let Some(teacher) = teacher.as_mut()
        && game.level.is_some()
    {
        teacher.entity_attacked(now, struck.id, &kind, struck.living, struck.dying);
    }
}

/// A main-hand right click on the entity `id` of type `kind` (Java `UseEntityCallback`, from the
/// client side of `entity_interact`).
pub(crate) fn entity_used(host: &dyn ClientHost, id: i32, kind: &str, stack: Option<&ItemStack>) {
    let io = super::io();
    let (item, food) = io.used_item(host, stack);
    let entity = u32::try_from(id).map_or_else(|_| kind.to_string(), |id| entity_path(id, kind));
    let entity = entity.as_str();
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    if let Some(teacher) = teacher.as_mut()
        && game.level.is_some()
    {
        teacher.entity_used(now, entity, &item, food);
    }
}

/// The integrated server's `credited` kills dying, then what became of the entities the teacher
/// watches for a kill. A credited kill comes first, so it is said even when the client no
/// longer has the entity.
pub(crate) fn fates(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    credited: &[(u64, String)],
) -> Vec<(u64, Fate)> {
    let mut fates: Vec<(u64, Fate)> = credited.iter().map(|&(id, _)| (id, Fate::Dying)).collect();
    let watched: Vec<u64> = teacher.watched_entities().collect();
    if !watched.is_empty() {
        fates.extend(io.entity_fates(host, &watched));
    }
    fates
}

/// The core's input for one world tick. The integrated server's `credited` kills are the hits
/// the core watches, and are dying in `entity_fates`, so each is said once.
pub(crate) fn world_tick<'a>(
    view: &'a View,
    inventory: Option<&'a [(&'a str, i32)]>,
    breaking: Option<&'a str>,
    credited: &'a [(u64, &'a str)],
    entity_fates: &'a [(u64, Fate)],
    now: i64,
) -> WorldTick<'a> {
    WorldTick {
        now,
        attack_down: view.attack_down,
        use_down: view.use_down,
        using_item: view.using_item,
        breaking,
        held_item: &view.held_item,
        held_count: view.held_count,
        inventory,
        screen_open: view.screen_open,
        question_screen_open: view.our_screen,
        singleplayer: view.singleplayer && !view.shared,
        // The window owns the integrated server: single player, or the host of a LAN world. The
        // server half acts for that host only (`server::receive`).
        integrated_server: view.singleplayer,
        hardcore: view.hardcore,
        dead: view.health <= 0.0,
        health: view.health,
        food: view.food,
        sleeping: view.sleeping,
        in_vehicle: view.in_vehicle,
        vehicle: view.vehicle.as_deref(),
        in_water: view.in_water,
        on_ground: view.on_ground,
        fall_flying: view.fall_flying,
        flying: view.flying,
        on_climbable: view.on_climbable,
        sprinting: view.sprinting,
        sneaking: view.sneaking,
        on_fire: view.on_fire,
        fishing: view.fishing,
        eating: view.eating.as_deref(),
        move_keys: view.move_keys,
        jump_key: view.jump_key,
        drop_key: view.drop_key,
        velocity: view.velocity,
        position: view.position,
        yaw: view.yaw,
        pitch: view.pitch,
        dark: view.dark,
        raining: view.raining,
        thundering: view.thundering,
        biome_rains: view.biome_rains,
        biome_snows: view.biome_snows,
        biome_temperature: view.biome_temperature,
        projectile_hits: credited,
        entity_fates,
    }
}
