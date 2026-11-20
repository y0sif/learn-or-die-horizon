//! The wiring: one scripted session dispatches the client hooks through the SDK's test support
//! and checks what the teacher played, drew, asked and saved. The client seam answers the client
//! reads and writes the fake host cannot; the pack is invented for the test.
use super::client::{Click, Client, Store, View};
use super::hud::LAYER;
use super::{
    files, keys, lock, quiz_pauses, set_io, take_escape_pause, take_log_lines, with_state, world,
};
use mod_api::serde_json::{self, Value};
use mod_api::testing::{FakeHost, with_mods};
use mod_api::text::TextContent;
use mod_api::{
    BlockClick, BlockHit, ClientHost, DrawContext, GuiDraw, Hand, HostSide, InteractionAttempt,
    InteractionKey, InteractionResult, ItemKey, LevelDimension, LogicalSide, ModEntry, ScreenView,
    SoundPlay, SoundRef, SoundSource, Text, TextFormError, TriState,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

const PHRASES: &str = r#"{
  "meta": {"name": "Testisch", "ui": {"correct": "Ja!", "wrong": "Nein!"}, "portrait": {"frames": 9}},
  "voices": ["v1"],
  "speeds": ["normal"],
  "words": [
    {"key": "bloop", "kind": "block", "patterns": ["bloop"]},
    {"key": "quux", "kind": "item", "patterns": ["quux"]},
    {"key": "zap", "kind": "block", "patterns": ["zap"]},
    {"key": "zing", "kind": "block", "patterns": ["zing"]}
  ],
  "phrases": [
    {"id": "break.bloop", "action": "break", "key": "bloop", "text": "Ich zerbreche den Bloop.", "en": "I break the bloop.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "zerbreche", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Bloop", "role": "obj"}]},
    {"id": "place.bloop", "action": "place", "key": "bloop", "text": "Ich setze den Bloop.", "en": "I set the bloop."},
    {"id": "pickup.bloop", "action": "pickup", "key": "bloop", "text": "Ich nehme den Bloop.", "en": "I take the bloop."},
    {"id": "hold.bloop", "action": "hold", "key": "bloop", "text": "der Bloop", "en": "bloop"},
    {"id": "hold.quux", "action": "hold", "key": "quux", "text": "das Quux", "en": "quux"},
    {"id": "pickup.quux", "action": "pickup", "key": "quux", "text": "Ich nehme das Quux.", "en": "I take the quux."},
    {"id": "break.zap", "action": "break", "key": "zap", "text": "Ich zerbreche den Zap.", "en": "I break the zap."},
    {"id": "use.zing", "action": "use", "key": "zing", "text": "Ich benutze das Zing.", "en": "I use the zing."}
  ]
}"#;

const LIPSYNC: &str = r#"{"window": 40, "clips": {"v1.normal.hold.quux": "1221"}}"#;

const IDS: [&str; 8] = [
    "break.bloop",
    "place.bloop",
    "pickup.bloop",
    "hold.bloop",
    "hold.quux",
    "pickup.quux",
    "break.zap",
    "use.zing",
];
const START: i64 = 1_700_000_000_000;
const BLOCK: [i32; 3] = [4, 64, 4];
const ZAP: [i32; 3] = [5, 64, 4];
const ABOVE: [i32; 3] = [4, 65, 4];
/// A progress file from an earlier session.
const KEPT: &[u8] = br#"{"worlds": {"server:earlier": {}}}"#;
/// A config from an earlier session that does not parse.
const BAD_CONFIG: &[u8] = br#"{"enabled": fals"#;

/// What the scripted client holds and what the mod did to it.
#[derive(Default)]
struct Session {
    now: i64,
    /// Whose game the hooks run for: 0 this window's own, 1 split-screen P2.
    seat: usize,
    view: Option<View>,
    blocks: HashMap<[i32; 3], String>,
    /// The block under the crosshair.
    aim: Option<[i32; 3]>,
    clicks: Vec<&'static str>,
    screen: bool,
    opened: usize,
    played: Vec<SoundPlay>,
    chat: Vec<String>,
    files: HashMap<(Store, String), Vec<u8>>,
    /// Files that are there but fail to read.
    unreadable: HashSet<(Store, String)>,
    /// What the next right click on a block reads as.
    click: Option<Click>,
}

#[derive(Default)]
struct TestClient(Mutex<Session>);

impl TestClient {
    fn session(&self) -> MutexGuard<'_, Session> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn update(&self, f: impl FnOnce(&mut View)) {
        if let Some(view) = self.session().view.as_mut() {
            f(view);
        }
    }

    /// How often the voice played this phrase.
    fn plays(&self, id: &str) -> usize {
        let event = format!("zhnarrator_de:v1.normal.{id}");
        self.session()
            .played
            .iter()
            .filter(|sound| sound.identifier == event && sound.source == SoundSource::Voice)
            .count()
    }

    fn said(&self, id: &str) -> bool {
        self.plays(id) > 0
    }

    /// A copy of `bytes` was set aside as `<name>.invalid-<ms>.bak`.
    fn set_aside(&self, store: Store, name: &str, bytes: &[u8]) -> bool {
        let prefix = format!("{name}.invalid-");
        self.session().files.iter().any(|((s, file), kept)| {
            *s == store && file.starts_with(&prefix) && file.ends_with(".bak") && kept == bytes
        })
    }

    fn file(&self, store: Store, name: &str) -> Option<Value> {
        let bytes = self
            .session()
            .files
            .get(&(store, name.to_string()))
            .cloned()?;
        serde_json::from_slice(&bytes).ok()
    }
}

/// A component's characters, translation keys standing for their text.
fn plain(text: &Text) -> String {
    let mut out = match &text.content {
        TextContent::Literal(literal) => literal.clone(),
        TextContent::Translatable { key, .. } => key.clone(),
        _ => String::new(),
    };
    for child in &text.children {
        out.push_str(&plain(child));
    }
    out
}

impl Client for TestClient {
    fn now(&self) -> i64 {
        self.session().now
    }
    fn seat(&self) -> usize {
        self.session().seat
    }
    fn view(&self, _: &dyn ClientHost) -> Option<View> {
        let session = self.session();
        let mut view = session.view.clone()?;
        view.our_screen = session.screen;
        view.screen_open |= session.screen;
        Some(view)
    }
    fn block_at(&self, _: &dyn ClientHost, pos: [i32; 3]) -> Option<String> {
        let session = self.session();
        let block = session
            .blocks
            .get(&pos)
            .map_or("minecraft:air", String::as_str);
        Some(block.to_string())
    }
    fn attack_target(&self, _: &dyn ClientHost) -> Option<([i32; 3], String)> {
        let session = self.session();
        let held = session.view.as_ref()?.held_item.clone();
        Some((session.aim?, held))
    }
    fn click(&self, _: &dyn ClientHost, _: &BlockClick<'_>) -> Option<Click> {
        self.session().click.clone()
    }
    fn our_screen(&self, _: &dyn ClientHost) -> bool {
        self.session().screen
    }
    fn open_screen(&self, _: &mut dyn ClientHost) -> bool {
        let mut session = self.session();
        session.screen = true;
        session.opened += 1;
        true
    }
    fn close_screen(&self, _: &mut dyn ClientHost) {
        self.session().screen = false;
    }
    fn clicks(&self, _: &mut dyn ClientHost, key: &str) -> u32 {
        let mut session = self.session();
        let before = session.clicks.len();
        session.clicks.retain(|waiting| *waiting != key);
        (before - session.clicks.len()) as u32
    }
    fn play(&self, _: &dyn ClientHost, sound: &SoundPlay) -> Option<SoundRef> {
        self.session().played.push(sound.clone());
        None
    }
    fn sound_active(&self, _: &dyn ClientHost, _: SoundRef) -> bool {
        false
    }
    fn stop(&self, _: &dyn ClientHost, _: SoundRef) {}
    fn chat(&self, _: &mut dyn ClientHost, line: Text) {
        self.session().chat.push(plain(&line));
    }
    fn tr(&self, _: &dyn ClientHost, text: &Text) -> String {
        plain(text)
    }
    fn resource(&self, _: &dyn ClientHost, id: &str) -> Option<Vec<u8>> {
        match id {
            "zhnarrator_de:phrases.json" => Some(PHRASES.into()),
            "zhnarrator_de:lipsync.json" => Some(LIPSYNC.into()),
            _ => None,
        }
    }
    fn sound_events(&self, _: &dyn ClientHost) -> HashSet<String> {
        IDS.iter()
            .map(|id| format!("zhnarrator_de:v1.normal.{id}"))
            .collect()
    }
    fn item_ids(&self, _: &dyn ClientHost) -> Vec<String> {
        Vec::new()
    }
    fn read(
        &self,
        _: &dyn ClientHost,
        store: Store,
        name: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        let session = self.session();
        let file = (store, name.to_string());
        if session.unreadable.contains(&file) {
            return Err("unreadable".into());
        }
        Ok(session.files.get(&file).cloned())
    }
    fn write(
        &self,
        _: &dyn ClientHost,
        store: Store,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        self.session()
            .files
            .insert((store, name.to_string()), bytes.to_vec());
        Ok(())
    }
}

/// Records what the HUD and the screen draw.
#[derive(Default)]
struct Canvas {
    texts: Vec<String>,
    sprites: Vec<String>,
    /// Each button's label and rectangle.
    buttons: Vec<(String, [f32; 4])>,
}

impl DrawContext for Canvas {
    fn size(&self) -> [f32; 2] {
        [480.0, 270.0]
    }
    fn fill(&mut self, _: [f32; 4], _: u32) {}
    fn fill_gradient(&mut self, _: [f32; 4], _: u32, _: u32) {}
    fn sprite(&mut self, name: &str, _: [f32; 4], _: [f32; 4], _: Option<u32>) -> bool {
        self.sprites.push(name.to_string());
        true
    }
    fn item(&mut self, _: ItemKey, _: [f32; 2], _: f32) -> bool {
        false
    }
    fn text(&mut self, text: &str, _: [f32; 2], _: u32, _: bool) {
        self.texts.push(text.to_string());
    }
    fn text_width(&self, text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }
    fn gui_paint(&mut self, draws: &[GuiDraw]) -> Result<(), TextFormError> {
        for draw in draws {
            if let GuiDraw::Text { text, .. } = draw {
                self.texts.push(plain(text));
            }
        }
        Ok(())
    }
    fn widget_button(
        &mut self,
        rect: [f32; 4],
        label: &Text,
        _: bool,
        _: bool,
    ) -> Result<(), TextFormError> {
        self.buttons.push((plain(label), rect));
        Ok(())
    }
}

fn tick(host: &mut FakeHost, io: &TestClient, ms: i64) {
    io.session().now += ms;
    mod_api::dispatch_client_tick_post(host);
}

/// The attack key acts on the crosshair's target, a click.
fn attack(host: &mut FakeHost) {
    let (mut swing, mut cancelled) = (true, false);
    mod_api::dispatch_interaction_key_mapping_triggered(
        host,
        InteractionKey::Attack,
        Hand::Main,
        InteractionAttempt::Press,
        &mut swing,
        &mut cancelled,
    );
}

/// A right click on a block, which the seam reads as `click`.
fn right_click(host: &mut FakeHost, io: &TestClient, click: Click) {
    let event: BlockClick<'static> = mod_api::__private::built::BlockClick {
        side: LogicalSide::Client,
        dimension: LevelDimension::Overworld,
        pos: click.pos,
        hit: BlockHit::new([0.5, 1.0, 0.5], [0, 1, 0], false),
        hand: Hand::Main,
        stack: None,
        player: 0,
        player_entity_id: 0,
    }
    .into();
    io.session().click = Some(click);
    let (mut block, mut item) = (TriState::Default, TriState::Default);
    let (mut result, mut cancelled) = (InteractionResult::Pass, false);
    let side = HostSide::Client(host);
    mod_api::dispatch_right_click_block(
        side,
        &event,
        &mut block,
        &mut item,
        &mut result,
        &mut cancelled,
    );
}

/// The question screen with the mouse at `mouse`.
fn screen_at(mouse: [f32; 2]) -> ScreenView<'static> {
    mod_api::__private::built::ScreenView {
        id: "mod",
        menu: None,
        title: None,
        rect: [0.0, 0.0, 480.0, 270.0],
        slots: 0,
        mouse,
        partial_tick: 0.0,
    }
    .into()
}

/// Plays on, moving, until the next question opens.
fn next_question(host: &mut FakeHost, io: &TestClient) {
    let opened = io.session().opened;
    for step in 0..200 {
        io.update(|view| view.yaw = step as f32);
        tick(host, io, 500);
        if io.session().opened > opened {
            return;
        }
    }
    panic!("no question within 100 s");
}

/// The open question's id and its right option.
fn right_option() -> (String, usize) {
    with_state(|teacher, _| {
        let open = teacher
            .and_then(|t| t.question().cloned())
            .expect("a question");
        let right = open.options.iter().position(|o| o.id == open.question.id);
        (open.question.id, right.expect("the answer is an option"))
    })
}

fn press(host: &FakeHost, screen: &ScreenView<'_>, key: &str) -> bool {
    let mut cancelled = false;
    mod_api::dispatch_screen_key_pressed_pre(host, screen, key, false, 0, &mut cancelled);
    cancelled
}

#[test]
fn hooks_drive_the_teacher() {
    let io = Arc::new(TestClient::default());
    io.session().now = START;
    // The progress of an earlier session, in a file that cannot be read now.
    let progress_file = (Store::Data, files::progress_file("de"));
    io.session()
        .files
        .insert(progress_file.clone(), KEPT.into());
    io.session().unreadable.insert(progress_file.clone());
    let config_file = (Store::Config, files::CONFIG_FILE.to_string());
    io.session().files.insert(config_file, BAD_CONFIG.into());
    set_io(io.clone());
    let mods = vec![ModEntry::new("learnordie", crate::hooks())];
    with_mods(mods.into(), || {
        let mut host = FakeHost::new();
        let screen = screen_at([0.0, 0.0]);

        // The title screen: the pack loads and says so in the game's log (the mixin adds the
        // `[learnordie]` target), the config that does not parse is set aside and the defaults
        // are written in its place.
        tick(&mut host, &io, 50);
        let phrases = with_state(|teacher, _| teacher.map(|t| t.pack.phrases().len()));
        assert_eq!(phrases, Some(IDS.len()));
        let n = IDS.len();
        let line = format!("zhnarrator_de: {n} phrases loaded, {n} of {n} sound events resolved");
        let lines = take_log_lines();
        assert!(lines.contains(&(false, line)), "{lines:?}");
        let config = io
            .file(Store::Config, files::CONFIG_FILE)
            .expect("config written");
        assert_eq!(config["enabled"], Value::Bool(true));
        let saved = io.session().files.clone();
        let aside = io.set_aside(Store::Config, files::CONFIG_FILE, BAD_CONFIG);
        assert!(aside, "{:?}", saved.keys());

        // A world: the item in hand on joining is not said; the next one is, on the voice.
        io.session().view = Some(View {
            level: 1,
            dimension: "minecraft:overworld".into(),
            address: Some("test.server".into()),
            held_item: "minecraft:bloop".into(),
            held_count: 1,
            health: 20.0,
            on_ground: true,
            ..View::default()
        });
        tick(&mut host, &io, 50);
        io.update(|view| view.held_item = "minecraft:quux".into());
        for _ in 0..3 {
            tick(&mut host, &io, 50);
        }
        assert!(!io.said("hold.bloop"));
        assert!(io.said("hold.quux"));
        // The join's save leaves the unreadable progress file as it was.
        let kept = io.session().files.get(&progress_file).cloned();
        assert_eq!(kept.as_deref(), Some(KEPT));
        io.session().unreadable.clear();

        // The HUD: the noun, the held label and the portrait.
        let mut canvas = Canvas::default();
        mod_api::dispatch_gui_layer_draw(&host, LAYER, &mut canvas);
        assert!(
            canvas.texts.iter().any(|t| t == "das Quux"),
            "{:?}",
            canvas.texts
        );
        assert!(
            canvas
                .sprites
                .iter()
                .any(|s| s == "zhnarrator_de:gui/teacher")
        );

        // A split-screen player's turn: its game shares this process, but its hooks leave the
        // teacher alone. Picking its profile it is in no world, which does not take him out of
        // this window's world, its Disconnect does not either, and its view gets no HUD.
        let own = io.session().view.take();
        io.session().seat = 1;
        tick(&mut host, &io, 50);
        mod_api::dispatch_client_player_logging_out(&mut host, None, None);
        let mut canvas = Canvas::default();
        mod_api::dispatch_gui_layer_draw(&host, LAYER, &mut canvas);
        io.session().seat = 0;
        io.session().view = own;
        assert_eq!(with_state(|_, game| game.level), Some(1), "left the world");
        assert!(canvas.sprites.is_empty(), "{:?}", canvas.sprites);

        // A block that breaks at once (creative, grass, torches): the attack key acts on it and
        // it is gone by the tick's end, though it was never mined. Said once.
        io.session().blocks.insert(ZAP, "minecraft:zap".into());
        io.session().aim = Some(ZAP);
        io.update(|view| view.attack_down = true);
        attack(&mut host);
        io.session().blocks.insert(ZAP, "minecraft:air".into());
        tick(&mut host, &io, 100);
        io.session().aim = None;
        io.update(|view| view.attack_down = false);
        for _ in 0..10 {
            tick(&mut host, &io, 100);
        }
        assert_eq!(io.plays("break.zap"), 1);

        // Breaking a block: mined for a while, then gone.
        for _ in 0..20 {
            tick(&mut host, &io, 100);
        }
        io.session().blocks.insert(BLOCK, "minecraft:bloop".into());
        io.update(|view| {
            view.mining = Some(BLOCK);
            view.attack_down = true;
        });
        for _ in 0..3 {
            tick(&mut host, &io, 100);
        }
        io.session().blocks.insert(BLOCK, "minecraft:air".into());
        io.update(|view| {
            view.mining = None;
            view.attack_down = false;
        });
        tick(&mut host, &io, 100);
        assert!(io.said("break.bloop"));

        // Placing: a click the pack has a use phrase for places nothing, a block that has not
        // shown up within 500 ms is given up, and one that shows up in front of the face is said.
        let place = |target: &str| Click {
            main_hand: true,
            target: target.into(),
            places: Some("minecraft:bloop".into()),
            pos: BLOCK,
            in_front: ABOVE,
            ..Click::default()
        };
        right_click(&mut host, &io, place("minecraft:zing"));
        io.session().blocks.insert(ABOVE, "minecraft:bloop".into());
        tick(&mut host, &io, 100);
        io.session().blocks.remove(&ABOVE);
        right_click(&mut host, &io, place("minecraft:stone"));
        tick(&mut host, &io, 600);
        io.session().blocks.insert(ABOVE, "minecraft:bloop".into());
        for _ in 0..20 {
            tick(&mut host, &io, 100);
        }
        assert!(!io.said("place.bloop"));
        io.session().blocks.remove(&ABOVE);
        right_click(&mut host, &io, place("minecraft:stone"));
        io.session().blocks.insert(ABOVE, "minecraft:bloop".into());
        for _ in 0..3 {
            tick(&mut host, &io, 100);
        }
        assert!(io.said("place.bloop"));

        // A question a full interval after he first spoke: the ding, the screen, the pause.
        next_question(&mut host, &io);
        assert_eq!(io.session().opened, 1);
        let ding = io
            .session()
            .played
            .iter()
            .any(|s| s.identifier == super::sound::DING);
        assert!(ding);
        assert!(quiz_pauses());

        // Esc suspends it while the pause menu is up; with no screen open it comes back.
        assert!(!press(&host, &screen, "key.keyboard.escape"));
        assert!(!quiz_pauses());
        assert!(take_escape_pause());
        io.session().screen = false;
        io.update(|view| {
            view.screen_open = true;
            view.paused = true;
        });
        tick(&mut host, &io, 1000);
        assert_eq!(io.session().opened, 1);
        io.update(|view| {
            view.screen_open = false;
            view.paused = false;
        });
        tick(&mut host, &io, 50);
        assert_eq!(io.session().opened, 2);

        // The screen: four option buttons.
        io.update(|view| view.paused = true);
        tick(&mut host, &io, 800);
        let mut canvas = Canvas::default();
        mod_api::dispatch_screen_render_post(&host, &mut canvas, &screen);
        assert_eq!(canvas.buttons.len(), 4, "{:?}", canvas.buttons);
        assert!(canvas.buttons[0].0.starts_with("1. "));

        // The right key after the lockout: the screen closes, the chat and the progress say so.
        let (id, right) = right_option();
        assert!(press(
            &host,
            &screen,
            &format!("key.keyboard.{}", right + 1)
        ));
        assert!(!quiz_pauses());
        tick(&mut host, &io, 50);
        assert!(!io.session().screen);
        let chat = io.session().chat.clone();
        assert!(chat.iter().any(|line| line.starts_with("Ja!")), "{chat:?}");
        let correct = with_state(|teacher, _| teacher.map(|t| t.progress.correct(&id)));
        assert_eq!(correct, Some(1));
        // The file reads again: the earlier progress was set aside before a save replaced it.
        let saved = io.session().files.clone();
        let aside = io.set_aside(Store::Data, &files::progress_file("de"), KEPT);
        assert!(aside, "{:?}", saved.keys());
        let filed = |key: &str| {
            let progress = io.file(Store::Data, &files::progress_file("de"));
            progress.is_some_and(|p| p["worlds"].get(key).is_some())
        };
        assert!(filed("server:test.server"));

        // A click on the right option answers the next question.
        io.update(|view| view.paused = false);
        next_question(&mut host, &io);
        io.update(|view| view.paused = true);
        tick(&mut host, &io, 800);
        let mut canvas = Canvas::default();
        mod_api::dispatch_screen_render_post(&host, &mut canvas, &screen);
        let [x, y, w, h] = canvas.buttons[right_option().1].1;
        let mut cancelled = false;
        let at = screen_at([x + w / 2.0, y + h / 2.0]);
        mod_api::dispatch_screen_mouse_button_pressed_pre(&host, &at, 0, 0, &mut cancelled);
        assert!(cancelled);
        tick(&mut host, &io, 50);
        assert!(!io.session().screen);
        let chat = io.session().chat.clone();
        let right = chat.iter().filter(|line| line.starts_with("Ja!")).count();
        assert_eq!(right, 2, "{chat:?}");

        // Single player: the progress is filed under the save folder and the integrated server's
        // seed; with no seed within 5 s, under seed 0.
        let solo = |level| View {
            level,
            singleplayer: true,
            save_folder: Some("Testwelt".into()),
            health: 20.0,
            on_ground: true,
            ..View::default()
        };
        io.session().view = None;
        tick(&mut host, &io, 50);
        *lock(&world::SEED) = Some(0x4d2);
        io.session().view = Some(solo(2));
        tick(&mut host, &io, 50);
        assert!(filed("save:Testwelt:4d2"));
        io.session().view = None;
        tick(&mut host, &io, 50);
        mod_api::dispatch_server_stopped(&mut host, false);
        io.session().view = Some(solo(3));
        tick(&mut host, &io, 50);
        assert!(!filed("save:Testwelt:0"), "a join waits for the seed");
        tick(&mut host, &io, 5000);
        assert!(filed("save:Testwelt:0"));

        // The toggle key turns him off, says so in chat, and the config file keeps it.
        io.session().clicks.push(keys::TOGGLE);
        tick(&mut host, &io, 50);
        let config = io
            .file(Store::Config, files::CONFIG_FILE)
            .expect("config written");
        assert_eq!(config["enabled"], Value::Bool(false));
        let chat = io.session().chat.clone();
        assert!(
            chat.iter().any(|line| line == "learnordie.notice.off"),
            "{chat:?}"
        );
    });
}
