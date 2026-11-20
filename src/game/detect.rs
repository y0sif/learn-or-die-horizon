//! What the player did, read on the client as the original reads it: the block they broke (no
//! client event, so polled), the block they placed (a right click with a block item, then the
//! block showing up within 500 ms), and the tick's state for the core's detector (held item,
//! inventory for pickups, the block being mined).
use super::client::{Client, View};
use super::{STATE, State, lock};
use crate::core::{Teacher, WorldTick};
use mod_api::{BlockClick, ClientHost};

const PLACE_WAIT_MS: i64 = 500;

/// The block the player went for last tick, and what they held.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    pos: [i32; 3],
    block: String,
    held: String,
    /// The client was mining it (not only aiming at it with attack held).
    mining: bool,
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
    )
}

/// The block the player broke since the last tick, and the item they held: last tick's target is
/// air or a fluid now. The target is the block being mined, or the block aimed at while attack is
/// held, which covers blocks that break at once (grass, torches, creative mode).
pub(crate) fn poll_break(
    host: &dyn ClientHost,
    io: &dyn Client,
    view: &View,
    target: &mut Option<Target>,
) -> Option<(String, String)> {
    let broken = target.take().filter(|last| {
        io.block_at(host, last.pos)
            .is_some_and(|now| now != last.block && is_gone(&now))
    });
    let next = match view.mining {
        Some(pos) => Some((pos, true)),
        None => view
            .aimed
            .filter(|_| view.attack_down)
            .map(|pos| (pos, false)),
    };
    *target = next.and_then(|(pos, mining)| {
        let block = io.block_at(host, pos).filter(|block| !is_gone(block))?;
        Some(Target {
            pos,
            block,
            held: view.held_item.clone(),
            mining,
        })
    });
    broken.map(|last| (last.block, last.held))
}

/// The block being mined, for the core's ongoing state.
pub(crate) fn mining(target: Option<&Target>) -> Option<&str> {
    target
        .filter(|target| target.mining)
        .map(|target| target.block.as_str())
}

/// A right click on a block, on the client (Java `onUseBlock`): a click the pack has a use
/// phrase for (main hand, not sneaking) places nothing; any other with a block item waits for
/// its block.
pub(crate) fn right_click_block(host: &dyn ClientHost, click: &BlockClick<'_>) {
    let io = super::io();
    let Some(click) = io.click(host, click) else {
        return;
    };
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_ref() else {
        return;
    };
    if click.main_hand
        && !click.sneaking
        && teacher.pack.use_phrase(&click.target, click.open).is_some()
    {
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

/// The core's input for one world tick.
pub(crate) fn world_tick<'a>(
    view: &'a View,
    inventory: &'a [(&'a str, i32)],
    breaking: Option<&'a str>,
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
        singleplayer: view.singleplayer,
        dead: view.health <= 0.0,
        health: view.health,
        sleeping: view.sleeping,
        in_vehicle: view.in_vehicle,
        in_water: view.in_water,
        on_ground: view.on_ground,
        fall_flying: view.fall_flying,
        position: view.position,
        yaw: view.yaw,
        pitch: view.pitch,
    }
}
