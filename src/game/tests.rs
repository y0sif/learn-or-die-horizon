//! The wiring: one scripted session dispatches the client hooks through the SDK's test support
//! and checks what the teacher played, drew, asked and saved. The client seam answers the client
//! reads and writes the fake host cannot; the pack is invented for the test.
use super::client::{Click, Client, Store, View};
use super::hud::LAYER;
use super::{files, keys, quiz_pauses, set_io, take_escape_pause, with_state};
use mod_api::serde_json::{self, Value};
use mod_api::testing::{FakeHost, with_mods};
use mod_api::text::TextContent;
use mod_api::{
    BlockClick, ClientHost, DrawContext, GuiDraw, ItemKey, ModEntry, ScreenView, SoundPlay,
    SoundRef, SoundSource, Text, TextFormError,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

const PHRASES: &str = r#"{
  "meta": {"name": "Testisch", "ui": {"correct": "Ja!", "wrong": "Nein!"}, "portrait": {"frames": 9}},
  "voices": ["v1"],
  "speeds": ["normal"],
  "words": [
    {"key": "bloop", "kind": "block", "patterns": ["bloop"]},
    {"key": "quux", "kind": "item", "patterns": ["quux"]}
  ],
  "phrases": [
    {"id": "break.bloop", "action": "break", "key": "bloop", "text": "Ich zerbreche den Bloop.", "en": "I break the bloop.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "zerbreche", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Bloop", "role": "obj"}]},
    {"id": "place.bloop", "action": "place", "key": "bloop", "text": "Ich setze den Bloop.", "en": "I set the bloop."},
    {"id": "pickup.bloop", "action": "pickup", "key": "bloop", "text": "Ich nehme den Bloop.", "en": "I take the bloop."},
    {"id": "hold.bloop", "action": "hold", "key": "bloop", "text": "der Bloop", "en": "bloop"},
    {"id": "hold.quux", "action": "hold", "key": "quux", "text": "das Quux", "en": "quux"},
    {"id": "pickup.quux", "action": "pickup", "key": "quux", "text": "Ich nehme das Quux.", "en": "I take the quux."}
  ]
}"#;

const LIPSYNC: &str = r#"{"window": 40, "clips": {"v1.normal.hold.quux": "1221"}}"#;

const IDS: [&str; 6] = [
    "break.bloop",
    "place.bloop",
    "pickup.bloop",
    "hold.bloop",
    "hold.quux",
    "pickup.quux",
];
const START: i64 = 1_700_000_000_000;
const BLOCK: [i32; 3] = [4, 64, 4];

/// What the scripted client holds and what the mod did to it.
#[derive(Default)]
struct Session {
    now: i64,
    view: Option<View>,
    blocks: HashMap<[i32; 3], String>,
    clicks: Vec<&'static str>,
    screen: bool,
    opened: usize,
    played: Vec<SoundPlay>,
    chat: Vec<String>,
    files: HashMap<(Store, String), Vec<u8>>,
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

    /// The voice played this phrase.
    fn said(&self, id: &str) -> bool {
        let event = format!("zhnarrator_de:v1.normal.{id}");
        self.session()
            .played
            .iter()
            .any(|sound| sound.identifier == event && sound.source == SoundSource::Voice)
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
    fn click(&self, _: &dyn ClientHost, _: &BlockClick<'_>) -> Option<Click> {
        None
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
        Ok(self
            .session()
            .files
            .get(&(store, name.to_string()))
            .cloned())
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
    buttons: Vec<String>,
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
        _: [f32; 4],
        label: &Text,
        _: bool,
        _: bool,
    ) -> Result<(), TextFormError> {
        self.buttons.push(plain(label));
        Ok(())
    }
}

fn tick(host: &mut FakeHost, io: &TestClient, ms: i64) {
    io.session().now += ms;
    mod_api::dispatch_client_tick_post(host);
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
    set_io(io.clone());
    let mods = vec![ModEntry::new("learnordie", crate::hooks())];
    with_mods(mods.into(), || {
        let mut host = FakeHost::new();
        let screen: ScreenView<'static> = mod_api::__private::built::ScreenView {
            id: "mod",
            menu: None,
            title: None,
            rect: [0.0, 0.0, 480.0, 270.0],
            slots: 0,
            mouse: [0.0, 0.0],
            partial_tick: 0.0,
        }
        .into();

        // The title screen: the pack loads, the default config is written.
        tick(&mut host, &io, 50);
        let phrases = with_state(|teacher, _| teacher.map(|t| t.pack.phrases().len()));
        assert_eq!(phrases, Some(IDS.len()));
        let config = io
            .file(Store::Config, files::CONFIG_FILE)
            .expect("config written");
        assert_eq!(config["enabled"], Value::Bool(true));

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
        let progress = io.file(Store::Data, &files::progress_file("de"));
        assert!(progress.is_some_and(|p| p["worlds"].get("server:test.server").is_some()));

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

        // A question a full interval after she first spoke: the ding, the screen, the pause.
        for step in 0..200 {
            io.update(|view| view.yaw = step as f32);
            tick(&mut host, &io, 500);
            if io.session().opened > 0 {
                break;
            }
        }
        assert_eq!(io.session().opened, 1, "no question within 100 s");
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
        assert!(canvas.buttons[0].starts_with("1. "));

        // The right key after the lockout: the screen closes, the chat and the progress say so.
        let (id, right) = with_state(|teacher, _| {
            let open = teacher
                .and_then(|t| t.question().cloned())
                .expect("a question");
            let right = open.options.iter().position(|o| o.id == open.question.id);
            (open.question.id, right.expect("the answer is an option"))
        });
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

        // The toggle key turns her off, and the config file keeps it.
        io.session().clicks.push(keys::TOGGLE);
        tick(&mut host, &io, 50);
        let config = io
            .file(Store::Config, files::CONFIG_FILE)
            .expect("config written");
        assert_eq!(config["enabled"], Value::Bool(false));
    });
}
