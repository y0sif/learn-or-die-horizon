//! The seam between the game layer and the client host: every client read and write the layer
//! makes goes through [`Client`]. [`HostClient`] answers from the game; the wiring test installs
//! its own, because the SDK's fake host has no client backend and panics on client calls.
//!
//! The SDK's widgets (the typed question's edit box, the settings buttons, the dictionary's tabs
//! and search box) ask their host for the game's clock, the clipboard and the click sound:
//! [`WidgetHost`] answers those through the seam too.
use super::{MOD_ID, detect};
use crate::core::{Fate, ServerAction};
use mod_api::prelude::{ClientInputClientHost, EffectsClientHost};
use mod_api::text::TextContent;
use mod_api::{
    BlockBreakingState, BlockClick, ClientHost, ConfigFileError, ConfigFiles, ConfigPath,
    CrosshairTarget, EntityRef, EquipmentSlot, Hand, Host, Identifier, ItemStack, LevelRef,
    PackType, SoundPlay, SoundRef, SoundState, Text,
};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where a file lives: `config/` beside `options.txt`, or the mod's bulk data folder
/// (`horizon-mod-data/learnordie/`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Store {
    Config,
    Data,
}

/// What the layer reads of the local player and the window each client tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// The client level, as a hash of its handle: a new value is a new level object.
    pub level: u64,
    /// The local player's UUID, by which the integrated server's half knows its host.
    pub uuid: u128,
    pub dimension: String,
    /// This window owns the integrated server it plays on (open to LAN or not).
    pub singleplayer: bool,
    /// That integrated server is open to LAN or has split-screen players joined, so it does not
    /// pause (as the `quiz_pause` mixin last saw it).
    pub shared: bool,
    /// The save's folder under `saves/`, in single player.
    pub save_folder: Option<String>,
    /// The joined server's address, elsewhere.
    pub address: Option<String>,
    /// The game is paused (the pause menu, or the quiz screen in single player).
    pub paused: bool,
    /// Any screen is over the world.
    pub screen_open: bool,
    /// The open screen is this mod's: the question screen.
    pub our_screen: bool,
    pub attack_down: bool,
    pub use_down: bool,
    pub using_item: bool,
    /// The block the player is mining (the client's predicted breaking target).
    pub mining: Option<[i32; 3]>,
    /// The main-hand item id, empty when the hand is empty, and its count.
    pub held_item: String,
    pub held_count: i32,
    /// Every stack the player carries as `(item id, count)`: the main inventory, the off hand and
    /// the worn armour (Java `Inventory.getContainerSize`). `None` when it could not be read.
    pub inventory: Option<Vec<(String, i32)>>,
    pub health: f32,
    pub sleeping: bool,
    /// Riding something; a client reads no vehicle, so only the integrated server's facts
    /// (`detect::merge_events`) set it.
    pub in_vehicle: bool,
    pub in_water: bool,
    /// Standing on a block (`Entity.onGround`): a client reads no ground, so it is a block's
    /// collision box right under the player's box.
    pub on_ground: bool,
    pub fall_flying: bool,
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// A Hardcore world, as the health bar last drew it (no other read tells a client).
    pub hardcore: bool,
    /// The food level, 0 to 20.
    pub food: i32,
    /// The vehicle's entity type id, from the integrated server's facts.
    pub vehicle: Option<String>,
    /// Creative flight.
    pub flying: bool,
    pub on_climbable: bool,
    /// Sprinting: the movement speed carries the `minecraft:sprinting` modifier, which
    /// `setSprinting` adds and takes off on the client too.
    pub sprinting: bool,
    pub sneaking: bool,
    /// On fire, or in lava: lava, a fire block where the player stands, the fire drawn over the
    /// view, or the integrated server's facts.
    pub on_fire: bool,
    /// A fishing hook of the player's is out, from the integrated server's facts: the client
    /// keeps its own hook outside the entities a mod can list.
    pub fishing: bool,
    /// The item in use, when it is food or a consumable.
    pub eating: Option<String>,
    /// A movement key, the jump key, the drop key is down. The drop key comes from the
    /// `key_mapping_pre` hook (`detect::merge_events`).
    pub move_keys: bool,
    pub jump_key: bool,
    pub drop_key: bool,
    /// Blocks per tick.
    pub velocity: [f64; 3],
    /// `isDarkOutside`, `isRaining`, `isThundering`.
    pub dark: bool,
    pub raining: bool,
    pub thundering: bool,
    /// The biome at the player has precipitation, is cold enough to snow there, and its base
    /// temperature.
    pub biome_rains: bool,
    pub biome_snows: bool,
    pub biome_temperature: f32,
}

/// The entity an attack hit, as the teacher reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Struck {
    /// Its network id.
    pub id: u64,
    /// Its entity type id.
    pub kind: String,
    /// A living entity (a mob, a player, an armour stand).
    pub living: bool,
    /// It is dying already.
    pub dying: bool,
}

/// What the crosshair is on, for the "what is this" key.
#[derive(Clone, Debug, PartialEq)]
pub enum Aim {
    /// A block, by its id.
    Block(String),
    /// An entity, by its network id and its type id.
    Entity { id: u32, kind: String },
}

/// A right click on a block, as the place check reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Click {
    pub main_hand: bool,
    pub sneaking: bool,
    /// The block clicked.
    pub target: String,
    /// The clicked block's `open` property is true (a door or trapdoor).
    pub open: bool,
    /// The block the clicking hand's item places, if it is a block item.
    pub places: Option<String>,
    pub pos: [i32; 3],
    /// The position on the clicked face's side.
    pub in_front: [i32; 3],
}

/// The client reads and writes the game layer makes.
pub trait Client: Send + Sync {
    /// Wall-clock time in ms since the epoch, the clock the core and the progress file use.
    fn now(&self) -> i64;
    /// Whose game the hook runs for: 0 for this window's own, 1 for split-screen P2 and so on.
    fn seat(&self) -> usize;
    /// The local player and the window this tick; `None` outside a world.
    fn view(&self, host: &dyn ClientHost) -> Option<View>;
    /// The block id at `pos` in the client level; `None` for an unloaded chunk.
    fn block_at(&self, host: &dyn ClientHost, pos: [i32; 3]) -> Option<String>;
    /// What an attack acts on now: the block under the crosshair, and the main-hand item id
    /// (empty for an empty hand). `None` when the crosshair is not on a block.
    fn attack_target(&self, host: &dyn ClientHost) -> Option<([i32; 3], String)>;
    fn click(&self, host: &dyn ClientHost, click: &BlockClick<'_>) -> Option<Click>;
    /// This mod's screen is the one open.
    fn our_screen(&self, host: &dyn ClientHost) -> bool;
    /// Asks for the menu blur over the world and the HUD this frame, as an options screen has.
    fn blur(&self, host: &dyn ClientHost);
    fn open_screen(&self, host: &mut dyn ClientHost) -> bool;
    fn close_screen(&self, host: &mut dyn ClientHost);
    /// Takes every click waiting on the mod's key mapping `key`.
    fn clicks(&self, host: &mut dyn ClientHost, key: &str) -> u32;
    fn play(&self, host: &dyn ClientHost, sound: &SoundPlay) -> Option<SoundRef>;
    /// The sound waits to start or plays.
    fn sound_active(&self, host: &dyn ClientHost, sound: SoundRef) -> bool;
    fn stop(&self, host: &dyn ClientHost, sound: SoundRef);
    fn chat(&self, host: &mut dyn ClientHost, line: Text);
    /// The text in the current language.
    fn tr(&self, host: &dyn ClientHost, text: &Text) -> String;
    /// A client resource's bytes, by `namespace:path`.
    fn resource(&self, host: &dyn ClientHost, id: &str) -> Option<Vec<u8>>;
    /// Every sound event the client's resources have, as `namespace:path`.
    fn sound_events(&self, host: &dyn ClientHost) -> HashSet<String>;
    /// Every item id in registry order.
    fn item_ids(&self, host: &dyn ClientHost) -> Vec<String>;
    /// A file's bytes; `Ok(None)` when there is none.
    fn read(
        &self,
        host: &dyn ClientHost,
        store: Store,
        name: &str,
    ) -> Result<Option<Vec<u8>>, String>;
    fn write(
        &self,
        host: &dyn ClientHost,
        store: Store,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String>;
    /// The game's own clock in ms, which the widgets' caret and double clicks run by.
    fn millis(&self, host: &dyn ClientHost) -> u64;
    fn clipboard(&self, host: &dyn ClientHost) -> Option<String>;
    fn set_clipboard(&self, host: &dyn ClientHost, text: &str) -> bool;
    /// Sends an answer's consequences to the integrated server; true when queued.
    fn send_consequences(&self, host: &mut dyn ClientHost, actions: &[ServerAction]) -> bool;
    /// The entity an attack acts on.
    fn struck(&self, host: &dyn ClientHost, entity: EntityRef) -> Option<Struck>;
    /// The item of a right click on an entity, empty for an empty hand, and whether it is food.
    fn used_item(&self, host: &dyn ClientHost, stack: Option<&ItemStack>) -> (String, bool);
    /// Of the entities `watched` (network ids), those dying now or gone.
    fn entity_fates(&self, host: &dyn ClientHost, watched: &[u64]) -> Vec<(u64, Fate)>;
    /// The block or entity under the crosshair (`Minecraft.hitResult`).
    fn crosshair(&self, host: &dyn ClientHost) -> Option<Aim>;
    /// The key the mod's mapping `id` is bound to, as Controls names it (`keyName`).
    fn key_name(&self, host: &dyn ClientHost, id: &str) -> Text;
    /// The game's name for a dictionary hint's thing (`DictionaryScreen.itemName`): the entity
    /// type's when `minecraft:<key>` is an entity type and not an item, else the hover name of
    /// `picture`, the item drawn for the key. `None` for neither.
    fn game_name(&self, host: &dyn ClientHost, key: &str, picture: Option<&str>) -> Option<String>;
}

/// A key's name as Controls shows it (`InputConstants.Key.getDisplayName`): a letter or digit key
/// as its character in upper case, any other by its lang key (`key.keyboard.right.shift`).
pub(crate) fn key_display(key: &str) -> Text {
    let rest = key.strip_prefix("key.keyboard.").unwrap_or(key);
    let mut chars = rest.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Text::literal(c.to_uppercase().to_string());
    }
    let mut text = Text::translate(key, vec![]);
    if let TextContent::Translatable { fallback, .. } = &mut text.content {
        *fallback = Some(rest.to_string());
    }
    text
}

/// The host the SDK's widgets run with: their clock, the clipboard and the button click go
/// through the seam. They ask nothing else of it.
pub(crate) struct WidgetHost<'a> {
    pub(crate) io: &'a dyn Client,
    pub(crate) host: &'a dyn ClientHost,
}

impl Host for WidgetHost<'_> {}
mod_api::impl_host_areas!(WidgetHost<'_>);
impl ClientHost for WidgetHost<'_> {}
mod_api::impl_client_host_areas!(WidgetHost<'_>, except(client_input, effects));

impl ClientInputClientHost for WidgetHost<'_> {
    fn millis(&self) -> u64 {
        self.io.millis(self.host)
    }
    fn clipboard_text(&self) -> Option<String> {
        self.io.clipboard(self.host)
    }
    fn set_clipboard_text(&self, text: &str) -> bool {
        self.io.set_clipboard(self.host, text)
    }
}

impl EffectsClientHost for WidgetHost<'_> {
    fn play_local_sound(&self, sound: &SoundPlay, _after: u32) -> Option<SoundRef> {
        self.io.play(self.host, sound)
    }
}

/// Entity types that can be attacked but are not living (`LivingEntity`): vehicles, hanging
/// things, crystals and the projectiles a hit deflects.
fn is_living(kind: &str) -> bool {
    let path = kind.rsplit(':').next().unwrap_or(kind);
    !(path.ends_with("boat")
        || path.ends_with("raft")
        || path.contains("minecart")
        || matches!(
            path,
            "item_frame"
                | "glow_item_frame"
                | "painting"
                | "end_crystal"
                | "leash_knot"
                | "interaction"
                | "fireball"
                | "small_fireball"
                | "dragon_fireball"
                | "wither_skull"
                | "shulker_bullet"
                | "wind_charge"
                | "breeze_wind_charge"
        ))
}

/// The sea level the snow line is reckoned from: a client cannot read it, and only the overworld,
/// where it is 63, has biomes with precipitation.
const SEA_LEVEL: i32 = 63;

/// `Biome.coldEnoughToSnow` without its temperature noise: the base temperature, cooled above
/// the sea level plus 17 by 0.05 every 40 blocks, below 0.15.
fn snows(temperature: f32, y: i32) -> bool {
    let above = y - (SEA_LEVEL + 17);
    let at = if above > 0 {
        temperature - above as f32 * 0.05 / 40.0
    } else {
        temperature
    };
    at < 0.15
}

/// How far under the player's box the ground is looked for.
const GROUND_PROBE: f64 = 1.0e-3;

/// `Entity.onGround`, which a client cannot read: a block's collision box lies right under the
/// player's predicted box. True where the box or the blocks under it cannot be read.
fn on_ground(host: &dyn ClientHost, level: LevelRef, entity: EntityRef) -> bool {
    let Some([[x0, y0, z0], [x1, _, z1]]) = entity.bounding_box(host) else {
        return true;
    };
    level
        .has_collisions(host, [x0, y0 - GROUND_PROBE, z0, x1, y0, z1])
        .unwrap_or(true)
}

/// A fire block where the player stands, at the feet or the head (`BaseFireBlock.entityInside`
/// sets a player inside one on fire).
fn in_fire(host: &dyn ClientHost, level: LevelRef, [x, y, z]: [i32; 3]) -> bool {
    [y, y + 1].into_iter().any(|y| {
        level.block_state(host, [x, y, z]).is_some_and(|state| {
            matches!(
                state.block().id(host).as_str(),
                "minecraft:fire" | "minecraft:soul_fire"
            )
        })
    })
}

/// [`Client`] answered by the game.
#[derive(Default)]
pub struct HostClient;

fn files(host: &dyn ClientHost, store: Store) -> &'static dyn ConfigFiles {
    match store {
        Store::Config => host.config_files(),
        Store::Data => host.data_files(),
    }
}

/// The slots beside the main inventory that `Inventory.getContainerSize` counts.
const WORN: [EquipmentSlot; 5] = [
    EquipmentSlot::OffHand,
    EquipmentSlot::Feet,
    EquipmentSlot::Legs,
    EquipmentSlot::Chest,
    EquipmentSlot::Head,
];

fn main_hand_id(host: &dyn ClientHost, entity: EntityRef) -> String {
    host.living(entity)
        .and_then(|living| living.hand_item(host, Hand::Main))
        .map_or_else(String::new, |stack| stack.item().id(host))
}

/// A property's value in a serialized state, `minecraft:oak_door[facing=north,open=true]`.
pub(crate) fn property<'a>(state: &'a str, name: &str) -> Option<&'a str> {
    let inner = state.split_once('[')?.1.strip_suffix(']')?;
    inner
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

impl Client for HostClient {
    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    fn seat(&self) -> usize {
        super::seat()
    }

    fn view(&self, host: &dyn ClientHost) -> Option<View> {
        let player = host.local_player()?;
        let level = player.level(host)?;
        let entity = player.entity();
        let living = host.living(entity);
        let held = living.and_then(|l| l.hand_item(host, Hand::Main));
        let inventory = player.inventory(host).map(|main| {
            let worn = WORN
                .iter()
                .filter_map(|&slot| living.and_then(|l| l.equipment(host, slot)));
            main.into_iter()
                .chain(worn)
                .map(|stack| (stack.item().id(host), stack.count() as i32))
                .collect()
        });
        let mut hasher = DefaultHasher::new();
        level.hash(&mut hasher);
        let server = host.client_session().client_server();
        let down = |name: &str| host.mapping(name).ok().flatten().is_some_and(|m| m.down);
        let mining = match player.block_breaking(host) {
            Some(BlockBreakingState::Breaking { position, .. }) => {
                Some([position.x, position.y, position.z])
            }
            _ => None,
        };
        let [yaw, pitch] = entity.rotation(host).unwrap_or([0.0; 2]);
        let fluids = entity.fluid_heights(host).unwrap_or_default();
        let in_fluid = |name: &str| {
            fluids
                .iter()
                .any(|(fluid, height)| fluid.ends_with(name) && *height > 0.0)
        };
        let in_water = in_fluid("water");
        let position = entity.position(host).unwrap_or([0.0; 3]);
        let block = position.map(|c| c.floor() as i32);
        let sprinting = living
            .and_then(|l| l.attribute_modifiers(host, "minecraft:movement_speed"))
            .is_some_and(|modifiers| {
                modifiers
                    .iter()
                    .any(|m| m.id.namespace() == "minecraft" && m.id.path() == "sprinting")
            });
        let eating = host
            .local_item_use()
            .and_then(|using| living?.hand_item(host, using.hand))
            .filter(|stack| {
                ["minecraft:food", "minecraft:consumable"]
                    .iter()
                    .any(|c| host.item_default_component(stack.item(), c).is_some())
            })
            .map(|stack| stack.item().id(host));
        let fixed_time = level
            .dimension_type(host)
            .is_some_and(|dimension| dimension.has_fixed_time);
        let (rain, thunder) = level.weather(host).unwrap_or((0.0, 0.0));
        let biome = level
            .client_biome(host, block)
            .and_then(|biome| level.biome_properties(host, &biome));
        Some(View {
            level: hasher.finish(),
            uuid: player.uuid(),
            dimension: player
                .dimension(host)
                .map_or_else(|| host.dimension().to_string(), |d| d.to_string()),
            singleplayer: server.as_ref().is_some_and(|s| s.integrated_local),
            shared: super::shared(),
            save_folder: server.as_ref().and_then(|s| s.save_folder.clone()),
            address: server.and_then(|s| s.address),
            paused: host.is_paused(),
            screen_open: !host.playing(),
            our_screen: host.mod_screen() == Some(MOD_ID),
            attack_down: down("key.attack"),
            use_down: down("key.use"),
            using_item: host.local_item_use().is_some(),
            mining,
            held_item: held
                .as_ref()
                .map_or_else(String::new, |stack| stack.item().id(host)),
            held_count: held.as_ref().map_or(0, |stack| stack.count() as i32),
            inventory,
            health: living.and_then(|l| l.health(host)).unwrap_or(20.0),
            sleeping: living.and_then(|l| l.is_sleeping(host)).unwrap_or(false),
            in_vehicle: false,
            in_water,
            on_ground: on_ground(host, level, entity),
            fall_flying: living.and_then(|l| l.is_gliding(host)).unwrap_or(false),
            position,
            yaw,
            pitch,
            hardcore: detect::hardcore(),
            food: player.food(host).map_or(20, |food| food.level),
            vehicle: None,
            flying: player.abilities(host).is_some_and(|a| a.flying),
            on_climbable: living.and_then(|l| l.is_climbing(host)).unwrap_or(false),
            sprinting,
            sneaking: entity.is_shift_key_down(host).unwrap_or(false),
            on_fire: in_fluid("lava") || in_fire(host, level, block),
            fishing: false,
            eating,
            move_keys: ["key.forward", "key.back", "key.left", "key.right"]
                .into_iter()
                .any(down),
            jump_key: down("key.jump"),
            drop_key: false,
            velocity: entity.velocity(host).unwrap_or([0.0; 3]),
            dark: !fixed_time && level.get_sky_darken(host).unwrap_or(0) >= 4,
            raining: rain > 0.2,
            thundering: thunder > 0.9,
            biome_rains: biome.as_ref().is_some_and(|b| b.has_precipitation),
            biome_snows: biome
                .as_ref()
                .is_some_and(|b| snows(b.temperature, block[1])),
            biome_temperature: biome.as_ref().map_or(0.8, |b| b.temperature),
        })
    }

    fn block_at(&self, host: &dyn ClientHost, pos: [i32; 3]) -> Option<String> {
        let level = host.local_player()?.level(host)?;
        Some(level.block_state(host, pos)?.block().id(host))
    }

    fn attack_target(&self, host: &dyn ClientHost) -> Option<([i32; 3], String)> {
        let Some(CrosshairTarget::Block { pos, .. }) = host.crosshair_target() else {
            return None;
        };
        let player = host.local_player()?;
        Some(([pos.x, pos.y, pos.z], main_hand_id(host, player.entity())))
    }

    fn click(&self, host: &dyn ClientHost, click: &BlockClick<'_>) -> Option<Click> {
        let player = host.local_player()?;
        let state = player.level(host)?.block_state(host, click.pos)?;
        let open = host
            .block_state_string(state)
            .is_some_and(|text| property(&text, "open") == Some("true"));
        let places = click
            .stack
            .and_then(|stack| host.item_block(stack.item()))
            .map(|block| block.id(host));
        let [x, y, z] = click.pos;
        let [dx, dy, dz] = click.hit.face;
        Some(Click {
            main_hand: click.hand == Hand::Main,
            sneaking: player.entity().is_shift_key_down(host).unwrap_or(false),
            target: state.block().id(host),
            open,
            places,
            pos: click.pos,
            in_front: [x + dx, y + dy, z + dz],
        })
    }

    fn our_screen(&self, host: &dyn ClientHost) -> bool {
        host.mod_screen() == Some(MOD_ID)
    }

    fn blur(&self, host: &dyn ClientHost) {
        host.apply_screen_blur();
    }

    fn open_screen(&self, host: &mut dyn ClientHost) -> bool {
        host.mod_screen_open(MOD_ID)
    }

    fn close_screen(&self, host: &mut dyn ClientHost) {
        host.mod_screen_close(MOD_ID);
    }

    fn clicks(&self, host: &mut dyn ClientHost, key: &str) -> u32 {
        let mut count = 0;
        while host.consume_click(MOD_ID, key) {
            count += 1;
        }
        count
    }

    fn play(&self, host: &dyn ClientHost, sound: &SoundPlay) -> Option<SoundRef> {
        host.play_local_sound(sound, 0)
    }

    fn sound_active(&self, host: &dyn ClientHost, sound: SoundRef) -> bool {
        matches!(
            sound.state(host),
            Some(SoundState::Waiting | SoundState::Playing)
        )
    }

    fn stop(&self, host: &dyn ClientHost, sound: SoundRef) {
        sound.stop(host);
    }

    fn chat(&self, host: &mut dyn ClientHost, line: Text) {
        if let Err(e) = host.chat_line(line) {
            super::log_warn(format!("could not show a chat line: {e:?}"));
        }
    }

    fn tr(&self, host: &dyn ClientHost, text: &Text) -> String {
        host.text_string(text).unwrap_or_default()
    }

    fn resource(&self, host: &dyn ClientHost, id: &str) -> Option<Vec<u8>> {
        host.resource(PackType::ClientResources, &Identifier::parse(id).ok()?)
    }

    fn sound_events(&self, host: &dyn ClientHost) -> HashSet<String> {
        host.sound_events()
            .into_iter()
            .map(|id| format!("{}:{}", id.namespace(), id.path()))
            .collect()
    }

    fn item_ids(&self, host: &dyn ClientHost) -> Vec<String> {
        host.item_keys()
            .into_iter()
            .map(|key| key.id(host))
            .collect()
    }

    fn read(
        &self,
        host: &dyn ClientHost,
        store: Store,
        name: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        let path = ConfigPath::new(name).map_err(|e| e.to_string())?;
        match files(host, store).read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(ConfigFileError::NotFound) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn write(
        &self,
        host: &dyn ClientHost,
        store: Store,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let path = ConfigPath::new(name).map_err(|e| e.to_string())?;
        files(host, store)
            .write(&path, bytes)
            .map_err(|e| e.to_string())
    }

    fn millis(&self, host: &dyn ClientHost) -> u64 {
        host.millis()
    }

    fn clipboard(&self, host: &dyn ClientHost) -> Option<String> {
        host.clipboard_text()
    }

    fn set_clipboard(&self, host: &dyn ClientHost, text: &str) -> bool {
        host.set_clipboard_text(text)
    }

    fn send_consequences(&self, host: &mut dyn ClientHost, actions: &[ServerAction]) -> bool {
        super::payload::send_consequences(host, actions)
    }

    fn struck(&self, host: &dyn ClientHost, entity: EntityRef) -> Option<Struck> {
        let kind = entity.kind(host)?.id(host);
        Some(Struck {
            id: u64::from(entity.id()),
            living: is_living(&kind),
            dying: detect::seen_dying(entity.id()),
            kind,
        })
    }

    fn used_item(&self, host: &dyn ClientHost, stack: Option<&ItemStack>) -> (String, bool) {
        stack.map_or_else(
            || (String::new(), false),
            |stack| {
                let food = host.item_default_component(stack.item(), "minecraft:food");
                (stack.item().id(host), food.is_some())
            },
        )
    }

    /// Dying: drawn with a death time since the last tick (`render_living_pre`; a client keeps
    /// no health of another entity). Gone: the client level no longer has it.
    fn entity_fates(&self, host: &dyn ClientHost, watched: &[u64]) -> Vec<(u64, Fate)> {
        watched
            .iter()
            .filter_map(|&id| {
                let network = u32::try_from(id).ok()?;
                if detect::seen_dying(network) {
                    Some((id, Fate::Dying))
                } else {
                    host.entity(network)
                        .is_none()
                        .then_some((id, Fate::Removed))
                }
            })
            .collect()
    }

    fn crosshair(&self, host: &dyn ClientHost) -> Option<Aim> {
        match host.crosshair_target()? {
            CrosshairTarget::Block { state, .. } => Some(Aim::Block(state.block().id(host))),
            CrosshairTarget::Entity { entity, .. } => Some(Aim::Entity {
                id: entity.id(),
                kind: entity.kind(host)?.id(host),
            }),
            _ => None,
        }
    }

    fn key_name(&self, host: &dyn ClientHost, id: &str) -> Text {
        host.mod_key(MOD_ID, id)
            .map_or_else(|| Text::literal("?"), |(key, _)| key_display(&key))
    }

    fn game_name(&self, host: &dyn ClientHost, key: &str, picture: Option<&str>) -> Option<String> {
        let id = format!("minecraft:{key}");
        let item = Identifier::parse(&id)
            .ok()
            .and_then(|id| host.item_key(&id));
        let entity = host
            .entity_type_keys()
            .iter()
            .any(|kind| kind.id(host) == id);
        if entity && item.is_none() {
            let name = Text::translate(format!("entity.minecraft.{key}"), vec![]);
            return Some(self.tr(host, &name));
        }
        let item = host.item_key(&Identifier::parse(picture?).ok()?)?;
        let name = host.hover_name(&ItemStack::of(item));
        (!name.is_empty()).then_some(name)
    }
}
