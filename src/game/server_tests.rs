//! The server half: each row sends one answer's consequences through the payload's codec to its
//! handler and checks what reached the player and the level.
//!
//! The host is the SDK's fake host (the player, their name, level and inventory slots, the blocks,
//! the entities' kinds and the command log) in a double that adds what the fake does not model:
//! the player's max health, health, position, eyes, hands, vehicle and fire, the blocks'
//! collision shapes, the server's entity types, the entities around, a sheep's colour, a fishing
//! hook's owner, and the item entities thrown or spawned. Where the game would act on a command
//! (effects, kills, summons) or on the attribute map, the row checks what the mod asked the host
//! to do.
use super::super::detect::HostFacts;
use super::super::payload::{Action, Consequences};
use super::{HOST_PLAYER, SEEN, Seen, kill_credit, lock, receive, ticked};
use crate::core::{Effect, ServerAction, Wipe};
use mod_api::__private::game::{BackendMutRefOf, BackendRefOf};
use mod_api::prelude::*;
use mod_api::testing::{FakeHost, with_mods};
use mod_api::{
    AttributeModifier, AttributeOperation, Backend, BackendMut, BackendMutRef, BackendRef,
    BlockShapeKind, BlockState, Commands, Compound, EntityRef, EntityTypeKey, FishingHookRef, Hand,
    Host, HostSide, Identifier, ItemKey, ItemStack, LevelRef, LivingRef, ModEntry,
    ModifierLifetime, PAYLOAD_DECODE_BUDGET, PlayerDataRefusal, PlayerRef, ServerHost,
    StackWriteError, Tag, decode_payload, encode_payload,
};
use std::collections::BTreeMap;

/// The host's UUID: commands name it `01234567-89ab-cdef-0fed-cba987654321` (`UUID.toString`).
const PLAYER: u128 = 0x0123_4567_89ab_cdef_0fed_cba9_8765_4321;
const GUEST: u128 = 8;
const ENTITY: u32 = 10;
/// The guest's entity, online beside the host.
const GUEST_ENTITY: u32 = 11;
/// The player stands on stone at y 64, far from the world's spawn.
const BLOCK: [i32; 3] = [100, 65, -41];
const POSITION: [f64; 3] = [100.3, 65.0, -40.7];
const EYES: [f64; 3] = [100.3, 66.62, -40.7];
/// Around the player: a red sheep, a fishing hook, a skeleton.
const SHEEP: u32 = 200;
const HOOK: u32 = 201;
const SKELETON: u32 = 202;
const AROUND: [(u32, &str); 3] = [
    (SHEEP, "minecraft:sheep"),
    (HOOK, "minecraft:fishing_bobber"),
    (SKELETON, "minecraft:skeleton"),
];
/// `DyeColor.RED`'s and `DyeColor.BLUE`'s ids.
const RED: i8 = 14;
const BLUE: i8 = 11;
/// The server's entity types (and the player's). The friendly ones are friendly by category alone.
const TYPES: [&str; 6] = [
    "minecraft:zombie",
    "minecraft:ender_dragon",
    "minecraft:pufferfish",
    "minecraft:cow",
    "minecraft:bat",
    "minecraft:item",
];
/// Inventory slots by the fake's numbers: the hotbar's first, the main inventory's last, the
/// head and the off hand.
const STACKS: [(i32, &str, u32); 4] = [
    (0, "minecraft:stone", 5),
    (35, "minecraft:apple", 3),
    (103, "minecraft:iron_helmet", 1),
    (-106, "minecraft:shield", 1),
];

/// The host's player as they tick after the payload.
#[derive(Clone, Copy, Default)]
struct Tick {
    riding: bool,
    fire_ticks: i32,
    /// A fishing rod in the main hand.
    rod: bool,
    /// Whose the fishing hook around is: none's by default.
    hook_owner: Option<u32>,
    /// The sheep is dyed this colour by a right click, then the player ticks again.
    dyed: Option<i8>,
}

/// An item entity in the level: thrown by `living.drop` with `throw_randomly` and
/// `include_thrower_name`, or spawned at a spot with a motion and a pickup delay.
#[derive(Clone)]
struct Item {
    stack: ItemStack,
    thrown: Option<(bool, bool)>,
    spawned: Option<([f64; 3], [f64; 3], u32)>,
}

/// The fake host and what the double adds to it.
#[derive(Clone)]
struct Server {
    fake: FakeHost,
    types: Vec<EntityTypeKey>,
    /// The player's max-health modifiers and their lifetimes (base 20).
    modifiers: Vec<(AttributeModifier, ModifierLifetime)>,
    health: f32,
    tick: Tick,
    rod: ItemStack,
    /// The sheep's saved `Color`, and how many times its saved data was read.
    colour: i8,
    nbt_reads: usize,
    /// `living.drop` throws nothing, and neither does a spawned item.
    drop_refused: bool,
    spawn_refused: bool,
    items: Vec<Item>,
}

impl Server {
    /// Within the attribute's range (`RangedAttribute`, 1 to 1024), as the game's value.
    fn max_health(&self) -> f64 {
        (20.0 + self.modifiers.iter().map(|(m, _)| m.amount).sum::<f64>()).clamp(1.0, 1024.0)
    }
    fn is_player(&self, entity: EntityRef) -> bool {
        entity.id() == ENTITY
    }
    /// A new item entity's handle.
    fn item(&mut self, item: Item) -> EntityRef {
        self.items.push(item);
        let player = self.fake.entity(ENTITY).expect("the player");
        EntityRef::new(player.epoch(), 1000 + self.items.len() as u32)
    }
}

/// `Inventory`'s slot numbers (the API's) as the fake numbers them.
fn fake_slot(slot: u32) -> Option<i32> {
    match slot {
        0..36 => Some(slot as i32),
        36..40 => Some(100 + slot as i32 - 36),
        40 => Some(-106),
        _ => None,
    }
}

impl Host for Server {
    fn backend(&self) -> Option<BackendRef<'_>> {
        Some(BackendRef::of(self))
    }
}
impl ServerHost for Server {
    fn backend_mut(&mut self) -> Option<BackendMutRef<'_>> {
        Some(BackendMutRef::of(self))
    }
}
impl Backend for Server {}
impl BackendMut for Server {}
mod_api::impl_host_areas!(Server, except(entities, keys, living, players));
mod_api::impl_server_host_areas!(Server, except(projectiles, services));
mod_api::impl_backend_areas!(
    Server,
    except(entities, keys, levels, living, players, projectiles)
);
mod_api::impl_backend_mut_areas!(Server, except(entities, levels, living, players));

impl LivingClientBackend for Server {}
impl PlayersClientBackend for Server {}

impl EntitiesHost for Server {
    fn entity(&self, id: u32) -> Option<EntityRef> {
        EntitiesHost::entity(&self.fake, id)
    }
}
impl PlayersHost for Server {
    fn player(&self, uuid: u128) -> Option<PlayerRef> {
        PlayersHost::player(&self.fake, uuid)
    }
}
impl ProjectilesServerHost for Server {
    fn fishing_hook(&self, entity: EntityRef) -> Option<FishingHookRef> {
        (entity.id() == HOOK).then(|| FishingHookRef::new(entity))
    }
}
impl ProjectilesBackend for Server {
    fn fishing_hook_player_owner(&self, _: FishingHookRef) -> Option<EntityRef> {
        EntitiesHost::entity(&self.fake, self.tick.hook_owner?)
    }
}
impl KeysHost for Server {
    fn entity_type_keys(&self) -> Vec<EntityTypeKey> {
        self.types.clone()
    }
}
impl LivingHost for Server {
    fn living(&self, entity: EntityRef) -> Option<LivingRef> {
        self.is_player(entity).then(|| LivingRef::new(entity))
    }
}
impl ServicesServerHost for Server {
    fn commands(&mut self) -> &mut dyn Commands {
        &mut self.fake.commands
    }
}
impl KeysBackend for Server {
    fn entity_type_key_id(&self, key: EntityTypeKey) -> String {
        KeysBackend::entity_type_key_id(&self.fake, key)
    }
    fn item_key_id(&self, key: ItemKey) -> String {
        KeysBackend::item_key_id(&self.fake, key)
    }
}
impl EntitiesBackend for Server {
    fn entity_kind(&self, entity: EntityRef) -> Option<EntityTypeKey> {
        EntitiesBackend::entity_kind(&self.fake, entity)
    }
    fn entity_position(&self, entity: EntityRef) -> Option<[f64; 3]> {
        self.is_player(entity).then_some(POSITION)
    }
    fn entity_block_position(&self, entity: EntityRef) -> Option<[i32; 3]> {
        self.is_player(entity).then_some(BLOCK)
    }
    fn entity_eye_position(&self, entity: EntityRef) -> Option<[f64; 3]> {
        self.is_player(entity).then_some(EYES)
    }
    /// The player rides the sheep when the tick says so.
    fn entity_vehicle(&self, entity: EntityRef) -> Option<EntityRef> {
        let riding = self.is_player(entity) && self.tick.riding;
        riding.then(|| EntitiesHost::entity(&self.fake, SHEEP))?
    }
    fn entity_fire_ticks(&self, entity: EntityRef) -> Option<i32> {
        self.is_player(entity).then_some(self.tick.fire_ticks)
    }
}
impl EntitiesBackendMut for Server {
    /// The sheep's saved data: its colour.
    fn entity_nbt(&mut self, entity: EntityRef) -> Option<Compound> {
        let Tag::Compound(nbt) = Tag::compound([("Color", Tag::Byte(self.colour))]) else {
            unreachable!()
        };
        self.nbt_reads += 1;
        (entity.id() == SHEEP).then_some(nbt)
    }
}
impl LevelsBackendMut for Server {
    fn level_spawn_item_with_motion(
        &mut self,
        _: LevelRef,
        x: f64,
        y: f64,
        z: f64,
        stack: ItemStack,
        motion: [f64; 3],
        pickup_delay: u32,
    ) -> Result<Option<EntityRef>, StackWriteError> {
        if self.spawn_refused {
            return Ok(None);
        }
        let spawned = Some(([x, y, z], motion, pickup_delay));
        Ok(Some(self.item(Item {
            stack,
            thrown: None,
            spawned,
        })))
    }
}
impl LevelsBackend for Server {
    /// The player and the entities [`AROUND`] them, for a box about the player.
    fn level_entities_in(
        &self,
        _: LevelRef,
        min: [f64; 3],
        max: [f64; 3],
    ) -> Option<Vec<EntityRef>> {
        assert!((0..3).all(|i| min[i] < POSITION[i] && POSITION[i] < max[i]));
        let ids = std::iter::once(ENTITY).chain(AROUND.map(|(id, _)| id));
        Some(
            ids.filter_map(|id| EntitiesHost::entity(&self.fake, id))
                .collect(),
        )
    }
    fn level_block_state(&self, level: LevelRef, pos: [i32; 3]) -> Option<BlockState> {
        LevelsBackend::level_block_state(&self.fake, level, pos)
    }
    /// Every block the fake holds is a full cube.
    fn level_block_shape(
        &self,
        _: LevelRef,
        _: BlockState,
        _: [i32; 3],
        kind: BlockShapeKind,
    ) -> Option<Vec<[f64; 6]>> {
        assert_eq!(kind, BlockShapeKind::Collision);
        Some(vec![[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]])
    }
}
impl PlayersBackend for Server {
    fn player_name(&self, player: PlayerRef) -> Option<String> {
        PlayersBackend::player_name(&self.fake, player)
    }
    fn player_level(&self, player: PlayerRef) -> Option<LevelRef> {
        PlayersBackend::player_level(&self.fake, player)
    }
    fn player_dimension(&self, player: PlayerRef) -> Option<Identifier> {
        PlayersBackend::player_dimension(&self.fake, player)
    }
    fn player_inventory_slots(&self, player: PlayerRef) -> Option<Vec<(u32, ItemStack)>> {
        let slots = &self.fake.players.get(player.uuid())?.slots;
        Some(
            (0..43)
                .filter_map(|slot| Some((slot, slots.get(&fake_slot(slot)?)?.clone())))
                .collect(),
        )
    }
}
impl PlayersBackendMut for Server {
    fn player_set_inventory_slot(
        &mut self,
        player: PlayerRef,
        slot: u32,
        stack: Option<ItemStack>,
    ) -> Result<(), PlayerDataRefusal> {
        let held = self.fake.players.get_mut(player.uuid());
        let held = held.ok_or(PlayerDataRefusal::Stale)?;
        let slot = fake_slot(slot).expect("a slot the fake numbers");
        match stack {
            Some(stack) => held.slots.insert(slot, stack),
            None => held.slots.remove(&slot),
        };
        Ok(())
    }
}
impl LivingBackend for Server {
    fn living_health(&self, _: LivingRef) -> Option<f32> {
        Some(self.health)
    }
    fn living_hand_item(&self, _: LivingRef, hand: Hand) -> Option<ItemStack> {
        (self.tick.rod && hand == Hand::Main).then(|| self.rod.clone())
    }
    fn living_attribute_value(&self, _: LivingRef, id: &str) -> Option<f64> {
        (id == "minecraft:max_health").then(|| self.max_health())
    }
}
impl LivingBackendMut for Server {
    fn living_replace_attribute_modifier(
        &mut self,
        _: LivingRef,
        attribute: &str,
        modifier: AttributeModifier,
        lifetime: ModifierLifetime,
    ) -> bool {
        assert_eq!(attribute, "minecraft:max_health");
        self.modifiers.retain(|(m, _)| m.id != modifier.id);
        self.modifiers.push((modifier, lifetime));
        true
    }
    /// Cut to the max, as the game's.
    fn living_set_health(&mut self, _: LivingRef, health: f32) -> bool {
        self.health = health.min(self.max_health() as f32);
        true
    }
    fn living_drop(
        &mut self,
        _: LivingRef,
        stack: ItemStack,
        throw_randomly: bool,
        include_thrower_name: bool,
    ) -> Result<Option<EntityRef>, StackWriteError> {
        if self.drop_refused {
            return Ok(None);
        }
        let thrown = Some((throw_randomly, include_thrower_name));
        Ok(Some(self.item(Item {
            stack,
            thrown,
            spawned: None,
        })))
    }
}

/// The player on stone, carrying [`STACKS`], with the server's entity types [`TYPES`] and the
/// entities [`AROUND`], and the guest online beside them.
fn server() -> Server {
    let mut fake = FakeHost::new();
    let items = STACKS.map(|(_, id, _)| Identifier::parse(id).unwrap());
    let rod = Identifier::parse("minecraft:fishing_rod").unwrap();
    let resolved = items.iter().cloned().chain([rod.clone()]);
    fake.registries.resolve_items(resolved).unwrap();
    for (n, kind) in TYPES.into_iter().enumerate() {
        fake.level.entity(100 + n as u32, kind);
    }
    for (id, kind) in AROUND {
        fake.level.entity(id, kind);
    }
    fake.resolve_entity_types().unwrap();
    mod_api::__private::resolve_block_keys(["minecraft:stone"], |_| {
        horizon_mod_runtime::block_definitions::BlockDefinition::new(&[&BTreeMap::new()], 0)
            .map_err(|e| format!("{e:?}"))
    })
    .unwrap();
    let [bx, by, bz] = BLOCK;
    for x in bx - 8..=bx + 8 {
        for z in bz - 8..=bz + 8 {
            fake.level.set_block([x, by - 1, z], "minecraft:stone");
        }
    }
    let stacks: Vec<_> = STACKS
        .iter()
        .zip(&items)
        .map(|(&(slot, _, count), id)| {
            let key = fake.item_key(id).unwrap();
            (slot, ItemStack::new(key, count).unwrap())
        })
        .collect();
    let player = fake.join(PLAYER, ENTITY);
    player.slots.extend(stacks);
    fake.join(GUEST, GUEST_ENTITY);
    // TYPES and the player's: not the entities around.
    let types = mod_api::__private::entity_type_keys()
        .into_iter()
        .filter(|&key| {
            let id = KeysBackend::entity_type_key_id(&fake, key);
            !AROUND.iter().any(|(_, kind)| *kind == id)
        })
        .collect();
    let rod = ItemStack::new(fake.item_key(&rod).unwrap(), 1).unwrap();
    Server {
        fake,
        types,
        modifiers: Vec::new(),
        health: 20.0,
        tick: Tick::default(),
        rod,
        colour: RED,
        nbt_reads: 0,
        drop_refused: false,
        spawn_refused: false,
        items: Vec::new(),
    }
}

/// A summon command's spot and entity, its NBT after it.
fn summoned(line: &str) -> ([f64; 3], &str, &str) {
    let rest = line
        .strip_prefix("execute in minecraft:overworld positioned ")
        .unwrap_or_else(|| panic!("not in the player's level: {line}"));
    let (spot, rest) = rest.split_once(" run summon ").unwrap();
    let spot: Vec<f64> = spot.split(' ').map(|n| n.parse().unwrap()).collect();
    let (entity, nbt) = rest.split_once(" ~ ~ ~").unwrap();
    ([spot[0], spot[1], spot[2]], entity, nbt)
}

/// Who sends the payload.
#[derive(Default)]
enum Sender {
    /// The host: the player whose client runs in the server's process.
    #[default]
    Host,
    /// A player on a dedicated server, whose process has no client.
    NoClient,
    /// A LAN guest: the host is another player.
    Guest,
}

#[derive(Default)]
struct Row {
    name: &'static str,
    sender: Sender,
    /// The player's health before.
    health: Option<f32>,
    actions: Vec<ServerAction>,
    /// The command lines run (the mob rows check theirs apart).
    commands: Vec<&'static str>,
    /// Five summons, hostile or friendly, of these entities.
    mobs: Option<&'static [&'static str]>,
    /// The lost-hearts modifier's amount.
    modifier: Option<f64>,
    health_after: Option<f32>,
    /// The game's `living.drop` throws nothing, and neither does a spawned item.
    drop_refused: bool,
    spawn_refused: bool,
    /// The host's player ticks after the payload, and what that tells the client.
    tick: Option<Tick>,
    facts: Option<HostFacts>,
    /// The inventory slots left (all by default), and the item entities in the level.
    kept: Option<usize>,
    items: usize,
    /// Deaths, `(entity, kind, the source's causing entity)`, and the kills credited to the host.
    deaths: &'static [(u32, &'static str, i32)],
    credited: &'static [(u32, &'static str)],
}

#[test]
fn consequences_reach_the_host_player() {
    let _server = lock(&crate::game::tests::SERVER_STATE);
    let effect = |id, seconds, level| ServerAction::Effect(Effect { id, seconds, level });
    let lost = |lost: i32| ServerAction::LostHearts {
        lost,
        modifier: -2.0 * f64::from(lost),
    };
    let kill = "execute in minecraft:overworld run kill 01234567-89ab-cdef-0fed-cba987654321";
    let around = || vec![(SHEEP, "red")];
    let rows = [
        Row {
            name: "two effects, each for its seconds at its level less one",
            actions: vec![
                effect("minecraft:slowness", 20, 5),
                effect("minecraft:regeneration", 15, 3),
            ],
            commands: vec![
                "effect give 01234567-89ab-cdef-0fed-cba987654321 minecraft:slowness 20 4",
                "effect give 01234567-89ab-cdef-0fed-cba987654321 minecraft:regeneration 15 2",
            ],
            ..Row::default()
        },
        Row {
            name: "an instant effect's seconds are given as its ticks: Saturation for 100",
            actions: vec![effect("minecraft:saturation", 5, 1)],
            commands: vec![
                "effect give 01234567-89ab-cdef-0fed-cba987654321 minecraft:saturation 100 0",
            ],
            ..Row::default()
        },
        Row {
            name: "five hostile mobs on the ground around the player",
            actions: vec![ServerAction::Mobs {
                hostile: true,
                seed: 3,
            }],
            mobs: Some(&[
                "minecraft:zombie",
                "minecraft:pufferfish",
                "minecraft:creeper",
            ]),
            ..Row::default()
        },
        Row {
            name: "five friendly mobs on the ground around the player",
            actions: vec![ServerAction::Mobs {
                hostile: false,
                seed: 3,
            }],
            mobs: Some(&["minecraft:cow", "minecraft:bat"]),
            ..Row::default()
        },
        Row {
            name: "a lost heart brings full health down to the new max",
            actions: vec![lost(1)],
            modifier: Some(-2.0),
            health_after: Some(18.0),
            ..Row::default()
        },
        Row {
            name: "health under the new max stays",
            health: Some(10.0),
            actions: vec![lost(2)],
            modifier: Some(-4.0),
            health_after: Some(10.0),
            ..Row::default()
        },
        Row {
            name: "the tenth lost heart leaves half a heart: max health stops at its floor, 1",
            actions: vec![lost(10)],
            modifier: Some(-20.0),
            health_after: Some(1.0),
            ..Row::default()
        },
        Row {
            name: "hardcore kills",
            actions: vec![ServerAction::Kill],
            commands: vec![kill],
            ..Row::default()
        },
        Row {
            name: "the angry teacher throws everything on the ground at the host's next tick",
            actions: vec![ServerAction::Wipe(Wipe::Drop)],
            tick: Some(Tick::default()),
            kept: Some(0),
            items: STACKS.len(),
            ..Row::default()
        },
        Row {
            name: "where the game throws nothing, each stack is thrown from the eyes by hand",
            actions: vec![ServerAction::Wipe(Wipe::Drop)],
            drop_refused: true,
            tick: Some(Tick::default()),
            kept: Some(0),
            items: STACKS.len(),
            ..Row::default()
        },
        Row {
            name: "a stack nothing throws stays in its slot",
            actions: vec![ServerAction::Wipe(Wipe::Drop)],
            drop_refused: true,
            spawn_refused: true,
            tick: Some(Tick::default()),
            ..Row::default()
        },
        Row {
            name: "or deletes it",
            actions: vec![ServerAction::Wipe(Wipe::Delete)],
            kept: Some(0),
            ..Row::default()
        },
        Row {
            name: "a dedicated server ignores it, and credits no kill",
            sender: Sender::NoClient,
            actions: vec![ServerAction::Kill, ServerAction::Wipe(Wipe::Delete)],
            deaths: &[(100, "minecraft:zombie", ENTITY as i32)],
            ..Row::default()
        },
        Row {
            name: "a LAN guest's answers are ignored, and their kills are not the host's",
            sender: Sender::Guest,
            actions: vec![lost(1), ServerAction::Wipe(Wipe::Drop)],
            deaths: &[(100, "minecraft:zombie", ENTITY as i32)],
            ..Row::default()
        },
        Row {
            name: "the host riding a sheep, burning and fishing: the client is told, by colour",
            tick: Some(Tick {
                riding: true,
                fire_ticks: 5,
                rod: true,
                hook_owner: Some(ENTITY),
                ..Tick::default()
            }),
            facts: Some(HostFacts {
                vehicle: Some("minecraft:red_sheep".into()),
                fishing: true,
                on_fire: true,
                sheep: around(),
            }),
            ..Row::default()
        },
        Row {
            name: "a sheep's colour is read as it comes near, and again after a right click dyes it",
            tick: Some(Tick {
                dyed: Some(BLUE),
                ..Tick::default()
            }),
            facts: Some(HostFacts {
                sheep: vec![(SHEEP, "blue")],
                ..HostFacts::default()
            }),
            ..Row::default()
        },
        Row {
            name: "another player's hook is not his fishing",
            tick: Some(Tick {
                rod: true,
                hook_owner: Some(GUEST_ENTITY),
                ..Tick::default()
            }),
            facts: Some(HostFacts {
                sheep: around(),
                ..HostFacts::default()
            }),
            ..Row::default()
        },
        Row {
            name: "his own hook with no rod in hand is not fishing either",
            tick: Some(Tick {
                hook_owner: Some(ENTITY),
                ..Tick::default()
            }),
            facts: Some(HostFacts {
                sheep: around(),
                ..HostFacts::default()
            }),
            ..Row::default()
        },
        Row {
            name: "the host's killing blows are his kills, a sheep by colour; no one else's are",
            deaths: &[
                (SHEEP, "minecraft:sheep", ENTITY as i32),
                (100, "minecraft:zombie", ENTITY as i32),
                (100, "minecraft:zombie", SKELETON as i32),
                (100, "minecraft:zombie", GUEST_ENTITY as i32),
                (100, "minecraft:zombie", 0),
                (ENTITY, "minecraft:player", ENTITY as i32),
            ],
            credited: &[(SHEEP, "minecraft:red_sheep"), (100, "minecraft:zombie")],
            ..Row::default()
        },
    ];
    let template = server();
    let player = template.fake.player(PLAYER).unwrap();
    for row in rows {
        let name = row.name;
        *lock(&SEEN) = Seen::new();
        *lock(&HOST_PLAYER) = match row.sender {
            Sender::Host => Some(PLAYER),
            Sender::NoClient => None,
            Sender::Guest => Some(GUEST),
        };
        let mut host = template.clone();
        let health = row.health.unwrap_or(20.0);
        host.health = health;
        host.drop_refused = row.drop_refused;
        host.spawn_refused = row.spawn_refused;
        let sent = Consequences {
            actions: row.actions.iter().map(Action::from).collect(),
        };
        let body = encode_payload(&sent, PAYLOAD_DECODE_BUDGET).unwrap();
        let received = decode_payload::<Consequences>(&body).unwrap();
        let drop_waits = receive(&mut host, player, &received);
        let host_drops = matches!(row.sender, Sender::Host)
            && row.actions.contains(&ServerAction::Wipe(Wipe::Drop));
        assert_eq!(
            drop_waits, host_drops,
            "{name}: the drop waits for the tick"
        );
        assert!(
            host.items.is_empty(),
            "{name}: nothing thrown before the tick"
        );
        if let Some(tick) = row.tick {
            host.tick = tick;
            let mut facts = ticked(&mut host, player, drop_waits);
            if let Some(colour) = tick.dyed {
                ticked(&mut host, player, false);
                host.colour = colour;
                // The right click goes through the mod's hooks, on the server.
                let mods = vec![ModEntry::new("learnordie", crate::hooks())];
                with_mods(mods.into(), || {
                    mod_api::dispatch_entity_interact(
                        HostSide::Server(&mut host),
                        "minecraft:overworld",
                        Hand::Main,
                        SHEEP as i32,
                        "minecraft:sheep",
                        None,
                        &mut None,
                    );
                });
                facts = ticked(&mut host, player, false);
                assert_eq!(host.nbt_reads, 2, "{name}: read twice in three ticks");
            }
            if let Some(expected) = &row.facts {
                assert_eq!(facts, *expected, "{name}");
            }
        }
        let credited: Vec<_> = row
            .deaths
            .iter()
            .filter_map(|&(entity, kind, causing)| kill_credit(&mut host, entity, kind, causing))
            .collect();
        let expected: Vec<_> = row
            .credited
            .iter()
            .map(|&(id, kind)| (id, kind.into()))
            .collect();
        assert_eq!(credited, expected, "{name}");

        let ran = &host.fake.commands.ran;
        match row.mobs {
            None => assert_eq!(*ran, row.commands, "{name}"),
            Some(entities) => {
                assert_eq!(ran.len(), 5, "{name}: {ran:?}");
                let mut charged = false;
                for line in ran {
                    let ([x, y, z], entity, nbt) = summoned(line);
                    assert!(entities.contains(&entity), "{name}: {line}");
                    let creeper = entity == "minecraft:creeper";
                    assert_eq!(nbt, if creeper { " {powered:1b}" } else { "" }, "{name}");
                    charged |= creeper;
                    // On the stone, 3 to 7 blocks out (rounded), in a block's middle.
                    assert_eq!(y, f64::from(BLOCK[1]), "{name}: {line}");
                    let (dx, dz) = (x - 0.5 - f64::from(BLOCK[0]), z - 0.5 - f64::from(BLOCK[2]));
                    assert_eq!((dx.fract(), dz.fract()), (0.0, 0.0), "{name}: {line}");
                    let distance = dx.hypot(dz);
                    assert!((2.0..=7.5).contains(&distance), "{name}: {line}");
                }
                let hostile = entities.contains(&"minecraft:creeper");
                assert!(charged || !hostile, "{name}: no charged creeper in {ran:?}");
            }
        }
        let modifiers: Vec<_> = host
            .modifiers
            .iter()
            .map(|(m, lifetime)| {
                let id = m.id.to_string();
                assert_eq!(id, "zhnarrator:lost_hearts", "{name}");
                assert_eq!(m.operation, AttributeOperation::AddValue, "{name}");
                assert_eq!(*lifetime, ModifierLifetime::Permanent, "{name}");
                m.amount
            })
            .collect();
        assert_eq!(modifiers, Vec::from_iter(row.modifier), "{name}");
        assert_eq!(host.health, row.health_after.unwrap_or(health), "{name}");
        let kept = host.fake.players.get(PLAYER).unwrap().slots.len();
        assert_eq!(kept, row.kept.unwrap_or(STACKS.len()), "{name}");
        // Each stack is in the level once: thrown at random without a thrower, or from the eyes
        // less 0.3, up 0.2 and across up to 0.5 a tick, picked up after 40 ticks.
        let counts: Vec<_> = host.items.iter().map(|item| item.stack.count()).collect();
        let all = STACKS.map(|(_, _, count)| count);
        assert_eq!(counts, all[..row.items], "{name}");
        for item in &host.items {
            assert_eq!(item.thrown.is_some(), !row.drop_refused, "{name}");
            if let Some(flags) = item.thrown {
                assert_eq!(flags, (true, false), "{name}");
            }
            if let Some(([x, y, z], [dx, dy, dz], delay)) = item.spawned {
                assert_eq!(
                    [x, y, z],
                    [EYES[0], EYES[1] - f64::from(0.3f32), EYES[2]],
                    "{name}"
                );
                assert_eq!((dy, delay), (f64::from(0.2f32), 40), "{name}");
                assert!(dx.hypot(dz) <= 0.5, "{name}: {dx} {dz}");
            }
        }
    }
}
