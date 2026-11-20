//! The server half: each row sends one answer's consequences through the payload's codec to its
//! handler and checks what reached the player and the level.
//!
//! The host is the SDK's fake host (the player, their name, level and inventory slots, the blocks
//! and the command log) in a double that adds what the fake does not model: the player's max
//! health, health and position, the blocks' collision shapes, the server's entity types and the
//! stacks thrown. Where the game would act on a command (effects, kills, summons) or on the
//! attribute map, the row checks what the mod asked the host to do.
use super::super::payload::{Action, Consequences};
use super::{HOST_PLAYER, receive};
use crate::core::{Effect, ServerAction, Wipe};
use mod_api::__private::game::{BackendMutRefOf, BackendRefOf};
use mod_api::prelude::*;
use mod_api::testing::FakeHost;
use mod_api::{
    AttributeModifier, AttributeOperation, Backend, BackendMut, BackendMutRef, BackendRef,
    BlockShapeKind, BlockState, Commands, EntityRef, EntityTypeKey, Host, Identifier, ItemStack,
    LevelRef, LivingRef, ModifierLifetime, PAYLOAD_DECODE_BUDGET, PlayerDataRefusal, PlayerRef,
    ServerHost, StackWriteError, decode_payload, encode_payload,
};
use std::collections::BTreeMap;

const PLAYER: u128 = 7;
const GUEST: u128 = 8;
const ENTITY: u32 = 10;
/// The player stands on stone at y 64, far from the world's spawn.
const BLOCK: [i32; 3] = [100, 65, -41];
const POSITION: [f64; 3] = [100.3, 65.0, -40.7];
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

/// The fake host and what the double adds to it.
#[derive(Clone)]
struct Server {
    fake: FakeHost,
    types: Vec<EntityTypeKey>,
    /// The player's max-health modifiers and their lifetimes (base 20).
    modifiers: Vec<(AttributeModifier, ModifierLifetime)>,
    health: f32,
    /// The stacks thrown, with `throw_randomly` and `include_thrower_name`.
    thrown: Vec<(ItemStack, bool, bool)>,
}

impl Server {
    fn max_health(&self) -> f64 {
        20.0 + self.modifiers.iter().map(|(m, _)| m.amount).sum::<f64>()
    }
    fn is_player(&self, entity: EntityRef) -> bool {
        entity.id() == ENTITY
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
mod_api::impl_host_areas!(Server, except(keys, living));
mod_api::impl_server_host_areas!(Server, except(services));
mod_api::impl_backend_areas!(Server, except(entities, keys, levels, living, players));
mod_api::impl_backend_mut_areas!(Server, except(living, players));

impl LivingClientBackend for Server {}
impl PlayersClientBackend for Server {}

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
}
impl EntitiesBackend for Server {
    fn entity_position(&self, entity: EntityRef) -> Option<[f64; 3]> {
        self.is_player(entity).then_some(POSITION)
    }
    fn entity_block_position(&self, entity: EntityRef) -> Option<[i32; 3]> {
        self.is_player(entity).then_some(BLOCK)
    }
}
impl LevelsBackend for Server {
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
        self.thrown
            .push((stack, throw_randomly, include_thrower_name));
        Ok(None)
    }
}

/// The player on stone, carrying [`STACKS`], with the server's entity types [`TYPES`].
fn server() -> Server {
    let mut fake = FakeHost::new();
    let items = STACKS.map(|(_, id, _)| Identifier::parse(id).unwrap());
    fake.registries.resolve_items(items.clone()).unwrap();
    for (n, kind) in TYPES.into_iter().enumerate() {
        fake.level.entity(100 + n as u32, kind);
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
    Server {
        fake,
        // TYPES and the player's.
        types: mod_api::__private::entity_type_keys(),
        modifiers: Vec::new(),
        health: 20.0,
        thrown: Vec::new(),
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
    /// The inventory slots left (all by default), and the stacks thrown.
    kept: Option<usize>,
    thrown: usize,
}

#[test]
fn consequences_reach_the_host_player() {
    let effect = |id, seconds, level| ServerAction::Effect(Effect { id, seconds, level });
    let lost = |lost: i32| ServerAction::LostHearts {
        lost,
        modifier: -2.0 * f64::from(lost),
    };
    let kill = "execute in minecraft:overworld run kill Player10";
    let rows = [
        Row {
            name: "two effects, each for its seconds at its level less one",
            actions: vec![
                effect("minecraft:slowness", 20, 5),
                effect("minecraft:regeneration", 15, 3),
            ],
            commands: vec![
                "effect give Player10 minecraft:slowness 20 4",
                "effect give Player10 minecraft:regeneration 15 2",
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
            name: "the last heart kills",
            actions: vec![lost(10)],
            commands: vec![kill],
            modifier: Some(-20.0),
            ..Row::default()
        },
        Row {
            name: "hardcore kills",
            actions: vec![ServerAction::Kill],
            commands: vec![kill],
            ..Row::default()
        },
        Row {
            name: "the angry teacher throws everything on the ground",
            actions: vec![ServerAction::Wipe(Wipe::Drop)],
            kept: Some(0),
            thrown: STACKS.len(),
            ..Row::default()
        },
        Row {
            name: "or deletes it",
            actions: vec![ServerAction::Wipe(Wipe::Delete)],
            kept: Some(0),
            ..Row::default()
        },
        Row {
            name: "a dedicated server ignores it",
            sender: Sender::NoClient,
            actions: vec![ServerAction::Kill, ServerAction::Wipe(Wipe::Delete)],
            ..Row::default()
        },
        Row {
            name: "a LAN guest's answers are ignored",
            sender: Sender::Guest,
            actions: vec![lost(1), ServerAction::Wipe(Wipe::Drop)],
            ..Row::default()
        },
    ];
    let template = server();
    let player = template.fake.player(PLAYER).unwrap();
    for row in rows {
        let name = row.name;
        *super::lock(&HOST_PLAYER) = match row.sender {
            Sender::Host => Some(PLAYER),
            Sender::NoClient => None,
            Sender::Guest => Some(GUEST),
        };
        let mut host = template.clone();
        let health = row.health.unwrap_or(20.0);
        host.health = health;
        let sent = Consequences {
            actions: row.actions.iter().map(Action::from).collect(),
        };
        let body = encode_payload(&sent, PAYLOAD_DECODE_BUDGET).unwrap();
        let received = decode_payload::<Consequences>(&body).unwrap();
        receive(&mut host, player, &received);

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
        assert_eq!(host.thrown.len(), row.thrown, "{name}");
        for (_, randomly, named) in &host.thrown {
            assert_eq!((*randomly, *named), (true, false), "{name}");
        }
    }
}
