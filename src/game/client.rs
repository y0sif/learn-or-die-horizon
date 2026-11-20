//! The seam between the game layer and the client host: every client read and write the layer
//! makes goes through [`Client`]. [`HostClient`] answers from the game; the wiring test installs
//! its own, because the SDK's fake host has no client backend and panics on client calls.
use super::MOD_ID;
use mod_api::{
    BlockBreakingState, BlockClick, ClientHost, ConfigFileError, ConfigFiles, ConfigPath,
    CrosshairTarget, EntityRef, EquipmentSlot, Hand, Identifier, PackType, SoundPlay, SoundRef,
    SoundState, Text,
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
    pub in_vehicle: bool,
    pub in_water: bool,
    pub on_ground: bool,
    pub fall_flying: bool,
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
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
        let in_water = entity
            .fluid_heights(host)
            .unwrap_or_default()
            .iter()
            .any(|(fluid, height)| fluid.ends_with("water") && *height > 0.0);
        Some(View {
            level: hasher.finish(),
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
            in_vehicle: entity.vehicle(host).is_some(),
            in_water,
            on_ground: entity.on_ground(host).unwrap_or(true),
            fall_flying: living.and_then(|l| l.is_gliding(host)).unwrap_or(false),
            position: entity.position(host).unwrap_or([0.0; 3]),
            yaw,
            pitch,
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
}
