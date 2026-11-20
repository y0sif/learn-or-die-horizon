//! The wiring: one scripted session dispatches the client hooks through the SDK's test support
//! and checks what the teacher played, drew, asked and saved. The client seam answers the client
//! reads and writes the fake host cannot; the pack is invented for the test.
use super::client::{Aim, Click, Client, Store, Struck, View};
use super::hud::LAYER;
use super::settings::Page;
use super::{
    NO_SERVER_LINE, files, keys, lock, quiz_pauses, set_io, take_escape_pause, take_log_lines,
    with_state, world,
};
use crate::core::{ExamStage, Fate, Phrase, QuestionKind, ServerAction};
use mod_api::client::widgets::{TextFieldLayout, TextFieldView};
use mod_api::serde_json::{self, Value};
use mod_api::testing::{FakeHost, with_mods};
use mod_api::text::TextContent;
use mod_api::{
    BlockClick, BlockHit, ClientHost, DrawContext, EntityRef, GuiClipRefusal, GuiDraw, Hand,
    HostSide, InteractionAttempt, InteractionKey, InteractionResult, ItemKey, ItemStack, KeyAction,
    LevelDimension, LogicalSide, ModEntry, ScreenView, SoundPlay, SoundRef, SoundSource, Text,
    TextFormError, TriState,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

/// Every phrase has an umlaut, so any one asked to type can be answered with it folded. The
/// sentences have their words, for the dictionary; the nouns (`hold`) are one piece.
const PHRASES: &str = r#"{
  "meta": {"name": "Testisch", "ui": {"correct": "Ja!", "wrong": "Nein!"}, "portrait": {"frames": 9}},
  "voices": ["v1"],
  "speeds": ["normal", "slow"],
  "words": [
    {"key": "bloop", "kind": "block", "patterns": ["bloop"]},
    {"key": "quux", "kind": "item", "patterns": ["quux"]},
    {"key": "zap", "kind": "block", "patterns": ["zap"]},
    {"key": "zing", "kind": "block", "patterns": ["zing"]}
  ],
  "phrases": [
    {"id": "break.bloop", "action": "break", "key": "bloop", "text": "Ich zerbreche den Blööp.", "en": "I break the bloop.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "zerbreche", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Blööp", "role": "obj"}]},
    {"id": "place.bloop", "action": "place", "key": "bloop", "text": "Ich setze den Blööp.", "en": "I set the bloop.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "setze", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Blööp", "role": "obj"}]},
    {"id": "pickup.bloop", "action": "pickup", "key": "bloop", "text": "Ich nehme den Blööp.", "en": "I take the bloop.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "nehme", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Blööp", "role": "obj"}]},
    {"id": "hold.bloop", "action": "hold", "key": "bloop", "text": "der Blööp", "en": "bloop"},
    {"id": "hold.quux", "action": "hold", "key": "quux", "text": "das Quäx", "en": "quux"},
    {"id": "pickup.quux", "action": "pickup", "key": "quux", "text": "Ich nehme das Quäx.", "en": "I take the quux.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "nehme", "role": "verb"}, {"text": "das", "role": "part"}, {"text": "Quäx", "role": "obj"}]},
    {"id": "break.zap", "action": "break", "key": "zap", "text": "Ich zerbreche den Zäp.", "en": "I break the zap.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "zerbreche", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Zäp", "role": "obj"}]},
    {"id": "use.zing", "action": "use", "key": "zing", "text": "Ich benütze das Zing.", "en": "I use the zing.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "benütze", "role": "verb"}, {"text": "das", "role": "part"}, {"text": "Zing", "role": "obj"}]},
    {"id": "hungry", "action": "hungry", "text": "Ich möchte essen.", "en": "I want to eat.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "möchte", "role": "verb"}, {"text": "essen", "role": "part"}]},
    {"id": "drop.bloop", "action": "drop", "key": "bloop", "text": "Ich lasse den Blööp fallen.", "en": "I let the bloop fall.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "lasse", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Blööp", "role": "obj"}, {"text": "fallen", "role": "adj"}]},
    {"id": "burning", "action": "burning", "text": "Ich glühe.", "en": "I glow.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "glühe", "role": "verb"}]},
    {"id": "fish", "action": "fish", "text": "Ich fänge Fische.", "en": "I catch fish.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "fänge", "role": "verb"}, {"text": "Fische", "role": "part"}]},
    {"id": "word.bloop", "action": "word", "key": "bloop", "text": "Blööp", "en": "bloop"}
  ]
}"#;

/// Every phrase but the word has an entry, so whichever the session learns is listed.
const DICTIONARY: &str = r#"{"entries": {
  "break.bloop": {"h": ["w.wer", "w.tun", "w.den"], "n": [["fall", "w.akk"]], "f": [["Er zerbricht ihn.", "", "er"]]},
  "place.bloop": {"h": ["w.wer"], "n": [["fall", "w.akk"]], "f": [["Formel", "", "er"]]},
  "pickup.bloop": {"h": ["w.wer"], "n": [["fall", "w.akk"]], "f": [["Formel", "", "er"]]},
  "hold.bloop": {"h": ["w.der"], "n": [["fall", "w.nom"]], "f": [["Formel", "", "er"]]},
  "hold.quux": {"h": ["w.das"], "n": [["fall", "w.nom"]], "f": [["Formel", "", "er"]]},
  "pickup.quux": {"h": ["w.wer"], "n": [["fall", "w.akk"]], "f": [["Formel", "", "er"]]},
  "break.zap": {"h": ["w.wer"], "n": [["fall", "w.akk"]], "f": [["Formel", "", "er"]]},
  "use.zing": {"h": ["w.wer"], "n": [["fall", "w.akk"]], "f": [["Formel", "", "er"]]},
  "hungry": {"h": ["w.wer"], "n": [["fall", "w.nom"]], "f": [["Formel", "", "er"]]},
  "drop.bloop": {"h": ["w.wer"], "n": [["fall", "w.akk"]], "f": [["Formel", "", "er"]]},
  "burning": {"h": ["w.wer"], "n": [["fall", "w.nom"]], "f": [["Formel", "", "er"]]},
  "fish": {"h": ["w.wer"], "n": [["fall", "w.akk"]], "f": [["Formel", "", "er"]]}
}}"#;

const LIPSYNC: &str = r#"{"window": 40, "clips": {"v1.normal.hold.quux": "1221"}}"#;

const IDS: [&str; 13] = [
    "break.bloop",
    "place.bloop",
    "pickup.bloop",
    "hold.bloop",
    "hold.quux",
    "pickup.quux",
    "break.zap",
    "use.zing",
    "hungry",
    "drop.bloop",
    "burning",
    "fish",
    "word.bloop",
];
const SPEEDS: [&str; 2] = ["normal", "slow"];
const START: i64 = 1_700_000_000_000;
const BLOCK: [i32; 3] = [4, 64, 4];
const ZAP: [i32; 3] = [5, 64, 4];
const ABOVE: [i32; 3] = [4, 65, 4];
/// A progress file from an earlier session.
const KEPT: &[u8] = br#"{"worlds": {"server:earlier": {}}}"#;
/// A config from an earlier session that does not parse.
const BAD_CONFIG: &[u8] = br#"{"enabled": fals"#;

/// The integrated server's half keeps process-wide state, which this session (the host's UUID,
/// the server stopping) and the server test both write: each holds this for its whole run.
pub(super) static SERVER_STATE: Mutex<()> = Mutex::new(());

/// What the scripted client holds and what the mod did to it.
#[derive(Default)]
struct Session {
    now: i64,
    /// Whose game the hooks run for: 0 this window's own, 1 split-screen P2.
    seat: usize,
    view: Option<View>,
    blocks: HashMap<[i32; 3], String>,
    /// The block under the crosshair, for the attack key.
    aim: Option<[i32; 3]>,
    /// What the crosshair is on, for the "what is this" key.
    looked: Option<Aim>,
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
    /// The consequences the answers sent to the integrated server.
    sent: Vec<Vec<ServerAction>>,
    /// Frames that asked for the menu blur.
    blurred: usize,
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
        self.plays_at("normal", id)
    }

    /// How often the voice played this phrase at this speed.
    fn plays_at(&self, speed: &str, id: &str) -> usize {
        let event = format!("zhnarrator_de:v1.{speed}.{id}");
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

/// A component's characters, translation keys standing for their text, with their arguments as
/// `key[a|b]`.
fn plain(text: &Text) -> String {
    let mut out = match &text.content {
        TextContent::Literal(literal) => literal.clone(),
        TextContent::Translatable { key, arguments, .. } if !arguments.is_empty() => {
            let arguments: Vec<String> = arguments.iter().map(plain).collect();
            format!("{key}[{}]", arguments.join("|"))
        }
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
    fn blur(&self, _: &dyn ClientHost) {
        self.session().blurred += 1;
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
            "zhnarrator_de:dictionary.json" => Some(DICTIONARY.into()),
            _ => None,
        }
    }
    fn sound_events(&self, _: &dyn ClientHost) -> HashSet<String> {
        SPEEDS
            .iter()
            .flat_map(|speed| IDS.map(|id| format!("zhnarrator_de:v1.{speed}.{id}")))
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
    fn millis(&self, _: &dyn ClientHost) -> u64 {
        self.session().now as u64
    }
    fn clipboard(&self, _: &dyn ClientHost) -> Option<String> {
        None
    }
    fn set_clipboard(&self, _: &dyn ClientHost, _: &str) -> bool {
        false
    }
    fn send_consequences(&self, _: &mut dyn ClientHost, actions: &[ServerAction]) -> bool {
        self.session().sent.push(actions.to_vec());
        true
    }
    fn struck(&self, _: &dyn ClientHost, _: EntityRef) -> Option<Struck> {
        None
    }
    fn used_item(&self, _: &dyn ClientHost, _: Option<&ItemStack>) -> (String, bool) {
        (String::new(), false)
    }
    fn entity_fates(&self, _: &dyn ClientHost, _: &[u64]) -> Vec<(u64, Fate)> {
        Vec::new()
    }
    fn crosshair(&self, _: &dyn ClientHost) -> Option<Aim> {
        self.session().looked.clone()
    }
    /// A key by its mapping's id, so chat lines name it plainly.
    fn key_name(&self, _: &dyn ClientHost, id: &str) -> Text {
        Text::literal(id)
    }
    fn game_name(&self, _: &dyn ClientHost, _: &str, _: Option<&str>) -> Option<String> {
        None
    }
}

/// Records what the HUD and the screen draw.
#[derive(Default)]
struct Canvas {
    /// The GUI's size, when not 480 by 270.
    size: Option<[f32; 2]>,
    texts: Vec<String>,
    sprites: Vec<String>,
    /// Each button's label and rectangle.
    buttons: Vec<(String, [f32; 4])>,
    /// The labels of the buttons drawn greyed out.
    inactive: Vec<String>,
    /// Each text field's text and rectangle.
    fields: Vec<(String, [f32; 4])>,
}

impl Canvas {
    fn sized(size: [f32; 2]) -> Canvas {
        Canvas {
            size: Some(size),
            ..Canvas::default()
        }
    }

    /// The rectangle of the button whose label holds `part`.
    fn button(&self, part: &str) -> [f32; 4] {
        let found = self.buttons.iter().find(|(label, _)| label.contains(part));
        found
            .unwrap_or_else(|| panic!("no {part} in {:?}", self.buttons))
            .1
    }

    /// Every button and text field lies inside the screen (`ui` lists only those).
    fn all_on_screen(&self) {
        let [sw, sh] = self.size();
        let widgets = self.buttons.iter().chain(&self.fields);
        for (label, [x, y, w, h]) in widgets {
            let on_screen = *x >= 0.0 && *y >= 0.0 && x + w <= sw && y + h <= sh;
            assert!(on_screen, "{label} at {:?}", [x, y, w, h]);
        }
    }

    fn shows(&self, text: &str) -> bool {
        self.texts.iter().any(|t| t == text)
    }
}

impl DrawContext for Canvas {
    fn size(&self) -> [f32; 2] {
        self.size.unwrap_or([480.0, 270.0])
    }
    fn draw_menu_background(&mut self, _: [f32; 4], _: bool) {}
    fn gui_nine_slice(&mut self, _: &str, _: [f32; 4], _: u32) {}
    fn widget_text_field(&mut self, field: &TextFieldView) -> TextFieldLayout {
        self.fields.push((field.value.clone(), field.rect));
        TextFieldLayout::new(field.scroll, Vec::new())
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
        active: bool,
        _: bool,
    ) -> Result<(), TextFormError> {
        if !active {
            self.inactive.push(plain(label));
        }
        self.buttons.push((plain(label), rect));
        Ok(())
    }
    fn push_clip(&mut self, _: [f32; 4]) -> Result<(), GuiClipRefusal> {
        Ok(())
    }
    fn pop_clip(&mut self) -> Result<(), GuiClipRefusal> {
        Ok(())
    }
    fn gui_tooltip(&mut self, lines: &[Text], _: [f32; 2]) -> Result<(), TextFormError> {
        self.texts.extend(lines.iter().map(plain));
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

/// A character typed at the screen; whether a widget took it.
fn type_char(host: &FakeHost, screen: &ScreenView<'_>, c: char) -> bool {
    let (mut field, mut cancelled) = (None, false);
    mod_api::dispatch_screen_char_typed_pre(host, screen, c, 0, &mut field, &mut cancelled);
    cancelled
}

/// A left click at the centre of `rect`; whether the screen took it.
fn click_at(host: &FakeHost, [x, y, w, h]: [f32; 4]) -> bool {
    let mut cancelled = false;
    let at = screen_at([x + w / 2.0, y + h / 2.0]);
    mod_api::dispatch_screen_mouse_button_pressed_pre(host, &at, 0, 0, &mut cancelled);
    cancelled
}

/// What the typed question's box holds, as the teacher keeps it.
fn kept_typed() -> String {
    with_state(|teacher, _| {
        let open = teacher.and_then(|t| t.question().cloned());
        open.map_or_else(String::new, |open| open.typed().to_string())
    })
}

/// The open question's kind and its text.
fn question_text() -> (QuestionKind, String) {
    with_state(|teacher, _| {
        let open = teacher
            .and_then(|t| t.question().cloned())
            .expect("a question");
        (open.kind, open.question.text)
    })
}

/// The settings page showing, if one is.
fn settings_page() -> Option<Page> {
    with_state(|_, game| game.settings.as_ref().map(|page| page.page()))
}

/// Which of this mod's pages shows: the guide, the dictionary, the exam's result.
fn pages() -> [bool; 3] {
    with_state(|_, game| {
        [
            game.guide.is_some(),
            game.dictionary.is_some(),
            game.result.is_some(),
        ]
    })
}

/// The screen drawn in a `size` window, the mouse at `mouse`.
fn drawn(host: &FakeHost, size: [f32; 2], mouse: [f32; 2]) -> Canvas {
    let mut canvas = Canvas::sized(size);
    mod_api::dispatch_screen_render_post(host, &mut canvas, &screen_at(mouse));
    canvas
}

/// The chat lines equal to `line`.
fn said_in_chat(io: &TestClient, line: &str) -> usize {
    io.session()
        .chat
        .iter()
        .filter(|seen| *seen == line)
        .count()
}

const HINT: &str = "[Teacher] learnordie.guide.reopen[guide|settings]";
/// The GUI size every page must fit.
const SMALL: [f32; 2] = [350.0, 405.0];

#[test]
fn hooks_drive_the_teacher() {
    let _server = lock(&SERVER_STATE);
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
        let (n, events) = (IDS.len(), IDS.len() * SPEEDS.len());
        let line = format!(
            "zhnarrator_de: {n} phrases loaded, {events} of {events} sound events resolved"
        );
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
            food: 20,
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
            canvas.texts.iter().any(|t| t == "das Quäx"),
            "{:?}",
            canvas.texts
        );
        assert!(
            canvas
                .sprites
                .iter()
                .any(|s| s == "zhnarrator_de:gui/teacher")
        );
        // The exam counts down on its own from the world's first tick, its seconds rounded up.
        assert!(
            canvas.shows("learnordie.exam.timer[20:00]"),
            "{:?}",
            canvas.texts
        );

        // 3 s into the first world ever the guide opens on its own, `guideShown` saved at once.
        // It pauses and fits the small window; Back is off on the first page, the last has no
        // Skip, his angry face and Done, which closes it and leaves the hint in chat.
        let opened = io.session().opened;
        for _ in 0..60 {
            if io.session().opened > opened {
                break;
            }
            tick(&mut host, &io, 50);
        }
        assert_eq!(pages(), [true, false, false], "the guide opens on its own");
        let config = io.file(Store::Config, files::CONFIG_FILE);
        assert_eq!(
            config.expect("config written")["guideShown"],
            Value::Bool(true)
        );
        tick(&mut host, &io, 50);
        assert!(quiz_pauses());
        for page in 1..=4 {
            let canvas = drawn(&host, SMALL, [0.0, 0.0]);
            canvas.all_on_screen();
            assert!(canvas.shows(&format!("learnordie.guide.page[{page}|4]")));
            let skip = canvas
                .buttons
                .iter()
                .any(|(l, _)| l == "learnordie.guide.skip");
            assert_eq!(skip, page < 4, "Skip on page {page}");
            let back_off = canvas.inactive.iter().any(|l| l == "learnordie.guide.back");
            assert_eq!(back_off, page == 1, "Back on page {page}");
            let face = canvas
                .sprites
                .iter()
                .any(|s| s == "zhnarrator_de:gui/teacher");
            assert_eq!(face, page == 4, "the angry face on page {page}");
            let next = if page < 4 { "guide.next" } else { "guide.done" };
            assert!(click_at(&host, canvas.button(next)));
        }
        tick(&mut host, &io, 50);
        assert!(!io.session().screen);
        assert_eq!(pages(), [false; 3]);
        assert_eq!(said_in_chat(&io, HINT), 1);
        assert_eq!(
            said_in_chat(&io, "[Teacher] learnordie.tip.open[settings]"),
            1
        );
        // U opens it again: not a first show, so Esc closes it with no hint.
        io.session().clicks.push(keys::GUIDE);
        tick(&mut host, &io, 50);
        assert_eq!(pages(), [true, false, false], "U opens the guide");
        assert!(!press(&host, &screen, "key.keyboard.escape"));
        io.session().screen = false;
        tick(&mut host, &io, 50);
        assert_eq!(pages(), [false; 3]);
        assert_eq!(said_in_chat(&io, HINT), 1);

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

        // Placing: a click the pack has a use phrase for says it and places nothing, a block that
        // has not shown up within 500 ms is given up, and one that shows up in front of the face
        // is said.
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
        assert!(io.said("use.zing"));
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
        let opened = io.session().opened;
        next_question(&mut host, &io);
        assert_eq!(io.session().opened, opened + 1);
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
        assert_eq!(io.session().opened, opened + 1);
        io.update(|view| {
            view.screen_open = false;
            view.paused = false;
        });
        tick(&mut host, &io, 50);
        assert_eq!(io.session().opened, opened + 2);

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

        // Off single player the answers send nothing, and the game's log says why once for the
        // world.
        assert!(io.session().sent.is_empty());
        let no_server = |lines: &[(bool, String)]| {
            let line = (true, NO_SERVER_LINE.to_string());
            lines.iter().filter(|seen| **seen == line).count()
        };
        assert_eq!(no_server(&take_log_lines()), 1);

        // K: the config and chat say the slow speed.
        io.update(|view| view.paused = false);
        io.session().clicks.push(keys::SPEED);
        tick(&mut host, &io, 50);
        let config = io.file(Store::Config, files::CONFIG_FILE);
        assert_eq!(
            config.expect("config written")["speed"],
            Value::from("slow")
        );
        let chat = io.session().chat.clone();
        let speed = "learnordie.notice.speed[learnordie.value.slow]".to_string();
        assert!(chat.contains(&speed), "{chat:?}");
        io.session().clicks.push(keys::SPEED);
        tick(&mut host, &io, 50);

        // Hunger, a fact of the tick: a food level of 6 or less is said once.
        io.update(|view| view.food = 5);
        for _ in 0..10 {
            tick(&mut host, &io, 100);
        }
        assert_eq!(io.plays("hungry"), 1);
        io.update(|view| view.food = 20);

        // The drop key, told by its key hook for the window's key and by the key event alone for
        // the control socket's: the held bloop one fewer is a drop. A key the window's hook told
        // for another mapping is no drop key.
        io.update(|view| {
            view.held_item = "minecraft:bloop".into();
            view.inventory = Some(vec![("minecraft:bloop".into(), 4)]);
        });
        tick(&mut host, &io, 100);
        let q = |host: &mut FakeHost, mapping: Option<&str>, action| {
            if let Some(mapping) = mapping {
                let pressed = action != KeyAction::Release;
                mod_api::dispatch_key_mapping_pre(host, mapping, pressed, &mut false);
            }
            mod_api::dispatch_input_key(host, "key.keyboard.q", None, 0, action, 0);
        };
        for (mapping, left) in [(Some("key.drop"), 3), (None, 2), (Some("key.use"), 1)] {
            q(&mut host, mapping, KeyAction::Press);
            io.update(|view| view.inventory = Some(vec![("minecraft:bloop".into(), left)]));
            tick(&mut host, &io, 100);
            q(&mut host, mapping, KeyAction::Release);
            for _ in 0..10 {
                tick(&mut host, &io, 100);
            }
        }
        assert_eq!(
            io.plays("drop.bloop"),
            2,
            "the window's and the socket's drop key only"
        );

        // The fire drawn over the view: he burns, once while the frames show it.
        super::detect::fire_drawn();
        for _ in 0..5 {
            tick(&mut host, &io, 100);
        }
        assert_eq!(io.plays("burning"), 1);

        // I opens the settings' main page, which pauses as an options screen does, every button
        // on screen in a 350 by 405 window. A click on Questions moves it on to typed questions,
        // saved at once. The page button opens the advanced page; Esc goes back, and on the main
        // page lets Escape close the screen.
        io.session().clicks.push(keys::SETTINGS);
        tick(&mut host, &io, 50);
        assert!(io.session().screen);
        assert_eq!(settings_page(), Some(Page::Main));
        tick(&mut host, &io, 50);
        assert!(quiz_pauses());
        with_state(|teacher, _| {
            teacher.expect("the teacher").config.quiz_interval_seconds = 20.0;
        });
        let blurred = io.session().blurred;
        let mut canvas = Canvas::sized([350.0, 405.0]);
        mod_api::dispatch_screen_render_post(&host, &mut canvas, &screen);
        assert_eq!(
            io.session().blurred,
            blurred + 1,
            "the page blurs what is behind it"
        );
        for (label, [x, y, w, h]) in &canvas.buttons {
            let on_screen = *x >= 0.0 && *y >= 0.0 && x + w <= 350.0 && y + h <= 405.0;
            assert!(on_screen, "{label} at {:?}", [x, y, w, h]);
        }
        assert!(click_at(&host, canvas.button("options.quiz_mode")));
        let config = io.file(Store::Config, files::CONFIG_FILE);
        assert_eq!(
            config.expect("config written")["quizMode"],
            Value::from("typing")
        );
        // Ask every 20 s shows at its nearest step, 15 s; a click on that step saves 15 s.
        let [x, y, _, h] = canvas.button("options.quiz_interval");
        let at = screen_at([x + 4.0, y + h / 2.0]);
        mod_api::dispatch_screen_mouse_button_pressed_pre(&host, &at, 0, 0, &mut false);
        mod_api::dispatch_screen_mouse_button_released_pre(&host, &at, 0, 0, &mut false);
        let config = io.file(Store::Config, files::CONFIG_FILE);
        assert_eq!(
            config.expect("config written")["quizIntervalSeconds"],
            Value::from(15.0)
        );
        assert!(click_at(&host, canvas.button("options.advanced")));
        assert_eq!(settings_page(), Some(Page::Advanced));
        assert!(press(&host, &screen, "key.keyboard.escape"));
        assert_eq!(settings_page(), Some(Page::Main));
        assert!(!press(&host, &screen, "key.keyboard.escape"));
        io.session().screen = false;
        tick(&mut host, &io, 50);
        assert_eq!(settings_page(), None);

        // So the next question is one to type, pausing as a choice does. For 400 ms the box
        // takes nothing, Enter included. Then it takes ä, and the answer typed with its umlauts
        // folded is close enough: right, with the spelling note in chat.
        next_question(&mut host, &io);
        let (kind, text) = question_text();
        assert_eq!(kind, QuestionKind::Typing);
        assert!(quiz_pauses());
        let mut canvas = Canvas::default();
        mod_api::dispatch_screen_render_post(&host, &mut canvas, &screen);
        assert_eq!(canvas.fields.len(), 1, "{:?}", canvas.fields);
        assert!(type_char(&host, &screen, 'x'));
        assert!(press(&host, &screen, "key.keyboard.enter"));
        assert_eq!(kept_typed(), "");
        io.update(|view| view.paused = true);
        tick(&mut host, &io, 500);
        assert!(io.session().screen);
        assert!(type_char(&host, &screen, 'ä'));
        assert_eq!(kept_typed(), "ä");
        assert!(press(&host, &screen, "key.keyboard.backspace"));
        let folded: String = text
            .chars()
            .map(|c| match c {
                'ä' => 'a',
                'ö' => 'o',
                'ü' => 'u',
                c => c,
            })
            .collect();
        assert_ne!(folded, text);
        for c in folded.chars() {
            assert!(type_char(&host, &screen, c));
        }
        assert_eq!(kept_typed(), folded);
        assert!(press(&host, &screen, "key.keyboard.enter"));
        tick(&mut host, &io, 50);
        assert!(!io.session().screen);
        let chat = io.session().chat.clone();
        let close = format!("learnordie.quiz.typed_close[{folded}]");
        assert!(chat.contains(&close), "{chat:?}");
        let right = chat.iter().filter(|line| line.starts_with("Ja!")).count();
        assert_eq!(right, 3, "{chat:?}");

        // Single player: the progress is filed under the save folder and the integrated server's
        // seed; with no seed within 5 s, under seed 0.
        let solo = |level| View {
            level,
            singleplayer: true,
            save_folder: Some("Testwelt".into()),
            health: 20.0,
            food: 20,
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

        // There, a wrong answer sends its consequences (the default penalty, two effects) to
        // the integrated server, and chat says back what was typed. Check submits too.
        io.update(|view| {
            view.held_item = "minecraft:quux".into();
            view.held_count = 1;
        });
        next_question(&mut host, &io);
        assert_eq!(question_text().0, QuestionKind::Typing);
        io.update(|view| view.paused = true);
        tick(&mut host, &io, 500);
        let mut canvas = Canvas::default();
        mod_api::dispatch_screen_render_post(&host, &mut canvas, &screen);
        for c in "falsch".chars() {
            assert!(type_char(&host, &screen, c));
        }
        assert!(click_at(&host, canvas.button("learnordie.quiz.check")));
        tick(&mut host, &io, 50);
        let sent = io.session().sent.clone();
        let effects = |actions: &[ServerAction]| {
            actions.len() == 2 && actions.iter().all(|a| matches!(a, ServerAction::Effect(_)))
        };
        assert!(sent.len() == 1 && effects(&sent[0]), "{sent:?}");
        let chat = io.session().chat.clone();
        assert!(
            chat.contains(&"learnordie.quiz.typed[falsch]".to_string()),
            "{chat:?}"
        );
        assert!(
            chat.iter().any(|line| line.starts_with("Nein!")),
            "{chat:?}"
        );

        // The integrated server tells what a client cannot read of its host's player: a cast
        // fishing hook is fishing, said once the answer's lines are over.
        io.update(|view| view.paused = false);
        super::detect::set_host_facts(Some(super::detect::HostFacts {
            fishing: true,
            ..Default::default()
        }));
        for _ in 0..40 {
            tick(&mut host, &io, 100);
        }
        super::detect::set_host_facts(None);
        assert_eq!(io.plays("fish"), 1);

        // Every later world gets the hint 3 s in, not the guide.
        assert_eq!(said_in_chat(&io, HINT), 2);

        // V names the block under the crosshair at once, even while he is muted.
        io.session().clicks.push(keys::MUTE);
        tick(&mut host, &io, 50);
        io.session().looked = Some(Aim::Block("minecraft:bloop".into()));
        io.session().clicks.push(keys::WHAT_IS_THIS);
        tick(&mut host, &io, 50);
        assert_eq!(io.plays("word.bloop"), 1);
        io.session().clicks.push(keys::MUTE);
        tick(&mut host, &io, 50);

        // An item's tooltip gets its German noun as the second line, in the original's gold (what
        // the `item_tooltip` hook hands on with the stack's item id).
        let mut lines = vec![
            (Text::literal("Quux"), u32::MAX),
            (Text::literal("minecraft:quux"), 0xFF55_5555),
        ];
        super::tooltip("minecraft:quux", &mut lines);
        let second = lines
            .get(1)
            .map(|(text, _)| (plain(text), text.style.color.clone()));
        let gold = Some("#FFE08A".to_string());
        assert_eq!(second, Some(("das Quäx".to_string(), gold)), "{lines:?}");
        assert_eq!(lines.len(), 3);

        // The exam page, from the main page's link, fits the small window. Stop while the exam
        // counts down turns `examAuto` off and says so; Start closes the screen and the questions
        // begin at once, each with its header, the HUD naming the next between them.
        io.session().clicks.push(keys::SETTINGS);
        tick(&mut host, &io, 50);
        let canvas = drawn(&host, SMALL, [0.0, 0.0]);
        canvas.all_on_screen();
        assert!(click_at(
            &host,
            canvas.button("learnordie.options.exam.on[20]")
        ));
        assert_eq!(settings_page(), Some(Page::Exam));
        let canvas = drawn(&host, SMALL, [0.0, 0.0]);
        canvas.all_on_screen();
        assert!(click_at(&host, canvas.button("options.exam_typing")));
        let [x, y, w, h] = canvas.button("learnordie.exam.stop[");
        let hovered = drawn(&host, SMALL, [x + w / 2.0, y + h / 2.0]);
        assert!(hovered.shows("learnordie.exam.start.tooltip[20]"));
        assert!(click_at(&host, [x, y, w, h]));
        let config = io
            .file(Store::Config, files::CONFIG_FILE)
            .expect("config written");
        assert_eq!(config["examTyping"], Value::Bool(false));
        assert_eq!(config["examAuto"], Value::Bool(false));
        tick(&mut host, &io, 50);
        assert_eq!(said_in_chat(&io, "[Teacher] learnordie.exam.stopped"), 1);
        let canvas = drawn(&host, SMALL, [0.0, 0.0]);
        assert!(click_at(&host, canvas.button("learnordie.exam.start")));
        tick(&mut host, &io, 50);
        assert!(!io.session().screen);
        tick(&mut host, &io, 50);
        let now = io.now();
        let stage = with_state(|teacher, _| teacher.map(|t| t.exam(now)));
        let Some(ExamStage::Asking { number: 1, total }) = stage else {
            panic!("the exam asks at once: {stage:?}");
        };
        let begins = format!("[Teacher] learnordie.exam.begin[{total}]");
        assert_eq!(said_in_chat(&io, &begins), 1);
        let mut given = String::new();
        for n in 1..=total {
            next_question(&mut host, &io);
            io.update(|view| view.paused = true);
            tick(&mut host, &io, 800);
            let canvas = drawn(&host, [480.0, 270.0], [0.0, 0.0]);
            assert!(canvas.shows(&format!("learnordie.exam.header[{n}|{total}]")));
            // The first answer is wrong: the result says what was picked.
            let (_, right) = right_option();
            let pick = if n == 1 { (right + 1) % 4 } else { right };
            if n == 1 {
                given = with_state(|teacher, _| {
                    let open = teacher.and_then(|t| t.question().cloned());
                    open.map(|open| open.options[pick].bare().to_string())
                })
                .expect("a question");
            }
            assert!(press(&host, &screen, &format!("key.keyboard.{}", pick + 1)));
            tick(&mut host, &io, 50);
            io.update(|view| view.paused = false);
            tick(&mut host, &io, 100);
            let mut canvas = Canvas::default();
            mod_api::dispatch_gui_layer_draw(&host, LAYER, &mut canvas);
            let next = format!("learnordie.exam.timer_asking[{}|{total}]", n + 1);
            assert_eq!(canvas.shows(&next), n < total, "{:?}", canvas.texts);
        }
        // 2.5 s after the last answer: the score in chat, and the result screen, which pauses.
        for _ in 0..30 {
            tick(&mut host, &io, 100);
        }
        assert_eq!(pages(), [false, false, true], "the result shows");
        let score = format!("{}|{total}", total - 1);
        assert_eq!(
            said_in_chat(&io, &format!("[Teacher] learnordie.exam.result[{score}]")),
            1
        );
        tick(&mut host, &io, 50);
        assert!(quiz_pauses());
        let canvas = drawn(&host, SMALL, [0.0, 0.0]);
        canvas.all_on_screen();
        assert!(canvas.shows(&format!("learnordie.exam.result_title[{score}]")));
        let rows = |mark: &str| canvas.texts.iter().filter(|t| t.starts_with(mark)).count();
        assert_eq!((rows("\u{2713} "), rows("\u{2717} ")), (total - 1, 1));
        assert!(canvas.shows(&format!("learnordie.exam.your_answer[{given}]")));
        assert!(click_at(&host, canvas.button("gui.done")));
        tick(&mut host, &io, 50);
        assert!(!io.session().screen);
        assert_eq!(pages(), [false; 3]);
        let progress = io.file(Store::Data, &files::progress_file("de"));
        let exams = progress.map(|p| p["worlds"]["save:Testwelt:0"]["exams"].clone());
        let exam = exams.as_ref().and_then(|exams| exams.get(0));
        let kept = exam.map(|e| (e["right"].clone(), e["total"].clone()));
        assert_eq!(
            kept,
            Some((Value::from(total - 1), Value::from(total))),
            "{exams:?}"
        );

        // J opens the dictionary on this world's learned phrases, the portrait hidden, every
        // widget on screen in the small window. The other tab lists every world's; the search
        // box finds one phrase, whose words, hints, notes and forms show; Play and Slow play it.
        let learned = with_state(|teacher, _| {
            let t = teacher.expect("the teacher");
            let learned = t.dictionary.learned(&t.pack, &t.progress, &t.config);
            let ids = |list: &[&Phrase]| list.iter().map(|p| (*p).clone()).collect::<Vec<_>>();
            (ids(&learned.this_world), ids(&learned.all))
        });
        let (here, all) = learned;
        assert!(
            !here.is_empty() && all.len() >= here.len(),
            "{here:?} {all:?}"
        );
        io.session().clicks.push(keys::DICTIONARY);
        tick(&mut host, &io, 50);
        assert_eq!(pages(), [false, true, false], "J opens the dictionary");
        tick(&mut host, &io, 50);
        assert!(quiz_pauses());
        let mut hud = Canvas::default();
        mod_api::dispatch_gui_layer_draw(&host, LAYER, &mut hud);
        assert!(
            hud.sprites.is_empty(),
            "the portrait hides: {:?}",
            hud.sprites
        );
        drawn(&host, SMALL, [0.0, 0.0]).all_on_screen();
        // The rows' text is cut to fit: the list's content is read in a wide window.
        const WIDE: [f32; 2] = [960.0, 540.0];
        let canvas = drawn(&host, WIDE, [0.0, 0.0]);
        let tabs = (
            format!("learnordie.dict.tab.world[{}]", here.len()),
            format!("learnordie.dict.tab.all[{}]", all.len()),
        );
        assert!(canvas.inactive.contains(&tabs.0), "this world's tab shows");
        let listed = |canvas: &Canvas, list: &[Phrase]| list.iter().all(|p| canvas.shows(p.bare()));
        assert!(listed(&canvas, &here));
        assert!(click_at(&host, canvas.button(&tabs.1)));
        let canvas = drawn(&host, WIDE, [0.0, 0.0]);
        assert!(canvas.inactive.contains(&tabs.1) && listed(&canvas, &all));
        let target = all
            .iter()
            .find(|p| !p.parts.is_empty())
            .expect("a sentence");
        assert!(click_at(&host, canvas.fields[0].1));
        for c in target.en.to_lowercase().chars() {
            assert!(type_char(&host, &screen, c));
        }
        drawn(&host, SMALL, [0.0, 0.0]).all_on_screen();
        let canvas = drawn(&host, WIDE, [0.0, 0.0]);
        let others = all.iter().filter(|p| p.text != target.text);
        assert!(
            others.clone().all(|p| !canvas.shows(p.bare())),
            "the search finds one"
        );
        assert!(canvas.shows(target.bare()) && canvas.shows(&target.en));
        assert!(target.parts.iter().all(|part| canvas.shows(&part.text)));
        assert!(
            canvas.shows("zhnarrator_de.dict.w.wer"),
            "the first word's hint"
        );
        let note = canvas
            .texts
            .iter()
            .any(|t| t.starts_with("zhnarrator_de.dict.note.fall["));
        assert!(
            note && canvas.shows("zhnarrator_de.dict.form.er"),
            "{:?}",
            canvas.texts
        );
        let played = |speed: &str| io.plays_at(speed, &target.id);
        let before = (played("normal"), played("slow"));
        assert!(click_at(&host, canvas.button("learnordie.dict.play")));
        assert!(click_at(&host, canvas.button("learnordie.dict.slow")));
        assert_eq!(
            (played("normal"), played("slow")),
            (before.0 + 1, before.1 + 1)
        );
        assert!(!press(&host, &screen, "key.keyboard.escape"));
        io.session().screen = false;
        tick(&mut host, &io, 50);
        assert_eq!(pages(), [false; 3]);

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
