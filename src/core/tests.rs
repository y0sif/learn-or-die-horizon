//! The core's tests: one per behavior, each case a row. The fixture pack is invented for these
//! tests; expected values follow the original mod's rules.
use super::detector::{Detector, Speech, break_phrase};
use super::quiz::choose_options;
use super::rng::Rng;
use super::*;
use mod_api::serde_json::{Value, json};

const PHRASES: &str = r#"{
  "meta": {"name": "Testisch", "ui": {"correct": "Ja!", "wrong": "Nein!"}, "portrait": {"frames": 9}},
  "voices": ["v1"],
  "speeds": ["normal", "slow"],
  "words": [
    {"key": "bloop", "kind": "block", "patterns": ["bloop", "*_bloop"], "measure": "m"},
    {"key": "zing", "kind": "block", "patterns": ["zing"]},
    {"key": "zang", "kind": "block", "patterns": ["zang"]},
    {"key": "stein", "kind": "block", "patterns": ["stone"]},
    {"key": "allstein", "kind": "block", "patterns": ["*stone*"]},
    {"key": "kerzenkuchen", "kind": "block", "patterns": ["*candle*cake"]},
    {"key": "quux", "kind": "item", "patterns": ["quux"]},
    {"key": "palin", "kind": "item", "patterns": ["ab*ba"]}
  ],
  "phrases": [
    {"id": "break.bloop", "action": "break", "key": "bloop", "text": "Ich zerbreche den Bloop.", "en": "I break the bloop.",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "zerbreche", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Bloop", "role": "obj"}],
     "hints": {"es": "Rompo el bloop."}},
    {"id": "place.bloop", "action": "place", "key": "bloop", "text": "Ich setze den Bloop.", "en": "I set the bloop."},
    {"id": "pickup.bloop", "action": "pickup", "key": "bloop", "text": "Ich nehme den Bloop.", "en": "I take the bloop."},
    {"id": "drop.bloop", "action": "drop", "key": "bloop", "text": "Ich werfe den Bloop.", "en": "I throw the bloop."},
    {"id": "hold.bloop", "action": "hold", "key": "bloop", "text": "der Bloop", "en": "bloop"},
    {"id": "word.bloop", "action": "word", "key": "bloop", "text": "der Bloop", "en": "bloop"},
    {"id": "break.zing", "action": "break", "key": "zing", "text": "Ich zerbreche den Zing.", "en": "I break the zing."},
    {"id": "break_hand.zing", "action": "break_hand", "key": "zing", "text": "Ich zerbreche den Zing mit der Hand.", "en": "I break the zing by hand."},
    {"id": "use.zing", "action": "use", "key": "zing", "text": "J'zing den Zing.", "en": "I zing the zing.",
     "parts": [{"text": "J'", "role": "pron"}, {"text": "zing", "role": "verb"}, {"text": "den", "role": "part"}, {"text": "Zing", "role": "other"}]},
    {"id": "break.zang", "action": "break", "key": "zang", "text": "Ich zerbreche den Zing.", "en": "I break the zang."},
    {"id": "hold.zing", "action": "hold", "key": "zing", "text": "der Zing", "en": "zing"},
    {"id": "hold.zang", "action": "hold", "key": "zang", "text": "die Zang", "en": "zang"},
    {"id": "hold.quux", "action": "hold", "key": "quux", "text": "das Quux", "en": "quux"},
    {"id": "pickup.quux", "action": "pickup", "key": "quux", "text": "Ich nehme das Quux.", "en": "I take the quux."},
    {"id": "jump", "action": "jump", "key": null, "text": "Ich hüpfe.", "en": "I hop."}
  ]
}"#;

const LIPSYNC: &str = r#"{"window": 40, "clips": {
  "v1.normal.pickup.bloop": "1221000",
  "v1.normal.hold.bloop": "12"
}}"#;

const WORLD: &str = "save:Testwelt:2a";
const START: i64 = 1_000_000;
const HEARD: &[&str] = &["break.bloop", "hold.quux", "pickup.quux"];

fn fixture_pack(language: &str) -> Pack {
    Pack::parse("zhnarrator_xx", PHRASES, Some(LIPSYNC), language).expect("the fixture pack parses")
}

fn phrase(pack: &Pack, id: &str) -> Phrase {
    pack.phrase(id)
        .cloned()
        .unwrap_or_else(|| panic!("fixture phrase {id}"))
}

/// A teacher in [`WORLD`] whose progress has heard `heard`, joined at [`START`].
fn teacher_with(config: Config, heard: &[&str]) -> Teacher {
    let mut progress = Progress::default();
    progress.switch_world(WORLD);
    for id in heard {
        progress.heard(id);
    }
    let mut teacher = Teacher::new(fixture_pack("en"), config, progress, 7);
    teacher.joined_level(START, WORLD, "minecraft:overworld", "minecraft:air");
    teacher
}

/// A calm player on the ground who looks around a little every tick.
fn calm(now: i64) -> WorldTick<'static> {
    WorldTick {
        now,
        held_item: "minecraft:air",
        health: 20.0,
        on_ground: true,
        yaw: (now % 3600) as f32,
        ..WorldTick::default()
    }
}

/// Ticks every 100 ms until a question is asked; returns when.
fn ask(teacher: &mut Teacher) -> i64 {
    for step in 1..=50 {
        let now = START + step * 100;
        if teacher.world_tick(&calm(now)) == Some(QuizEvent::Ask) {
            return now;
        }
    }
    panic!("no question within 5 s");
}

#[derive(Clone, Copy)]
enum Lookup {
    /// The word key for this kind.
    Key(&'static str),
    /// The item's phrase for this action.
    Item(&'static str),
    /// The break phrase while holding this item.
    Break(&'static str),
    /// The phrase a right click says instead of placing.
    Use,
    /// The translation of this phrase id with this hint language.
    Hint(&'static str),
}

#[test]
fn registry_ids_find_their_word_and_phrase() {
    let pack = fixture_pack("en");
    #[rustfmt::skip]
    let rows: [(&str, Lookup, &str, Option<&str>); 22] = [
        ("an exact pattern", Lookup::Key("block"), "minecraft:bloop", Some("bloop")),
        ("a star prefix", Lookup::Key("block"), "minecraft:red_bloop", Some("bloop")),
        ("the star keeps the underscore", Lookup::Key("block"), "minecraft:redbloop", None),
        ("an id without namespace", Lookup::Key("block"), "zing", Some("zing")),
        ("the first word wins over a later wider glob", Lookup::Key("block"), "minecraft:stone", Some("stein")),
        ("the wider glob takes the rest", Lookup::Key("block"), "minecraft:mossy_stone_bricks", Some("allstein")),
        ("two stars in order", Lookup::Key("block"), "minecraft:white_candle_cake", Some("kerzenkuchen")),
        ("a star matches nothing", Lookup::Key("block"), "minecraft:candlecake", Some("kerzenkuchen")),
        ("two stars out of order", Lookup::Key("block"), "minecraft:cake_candle", None),
        ("prefix and suffix may not overlap", Lookup::Key("item"), "minecraft:aba", None),
        ("prefix and suffix meet", Lookup::Key("item"), "minecraft:abba", Some("palin")),
        ("the kind is respected", Lookup::Key("item"), "minecraft:bloop", None),
        ("an item word", Lookup::Item("pickup"), "minecraft:quux", Some("pickup.quux")),
        ("a block item falls back to the block word", Lookup::Item("hold"), "minecraft:bloop", Some("hold.bloop")),
        ("no phrase for the action", Lookup::Item("pickup"), "minecraft:zing", None),
        ("breaking by hand", Lookup::Break("minecraft:air"), "minecraft:zing", Some("break_hand.zing")),
        ("breaking with an axe", Lookup::Break("minecraft:iron_axe"), "minecraft:zing", Some("break.zing")),
        ("no break_hand phrase", Lookup::Break("minecraft:air"), "minecraft:bloop", Some("break.bloop")),
        ("a block with a use phrase", Lookup::Use, "minecraft:zing", Some("use.zing")),
        ("a block with no use phrase", Lookup::Use, "minecraft:bloop", None),
        ("a hint replaces the English", Lookup::Hint("es"), "break.bloop", Some("Rompo el bloop.")),
        ("no hint keeps the English", Lookup::Hint("pt"), "break.bloop", Some("I break the bloop.")),
    ];
    for (name, lookup, id, expected) in rows {
        let got: Option<String> = match lookup {
            Lookup::Key(kind) => pack.key_for(kind, id).map(str::to_string),
            Lookup::Item(action) => pack.for_item(action, id).map(|p| p.id.clone()),
            Lookup::Break(held) => break_phrase(&pack, held, id).map(|p| p.id.clone()),
            Lookup::Use => pack.use_phrase(id, false).map(|p| p.id.clone()),
            Lookup::Hint(language) => fixture_pack(language).phrase(id).map(|p| p.en.clone()),
        };
        assert_eq!(got.as_deref(), expected, "{name}");
    }
}

#[test]
fn distractors_are_four_distinct_texts_with_the_question() {
    let pack = fixture_pack("en");
    #[rustfmt::skip]
    let rows: [(&str, &str, &[&str], &[&str]); 4] = [
        ("a sentence first takes sentences about the same thing", "break.bloop",
            &["break.bloop", "place.bloop", "pickup.bloop", "drop.bloop"], &[]),
        ("then the same action, skipping a repeated text", "break.zing",
            &["break.zing", "break_hand.zing", "use.zing", "break.bloop"], &["break.zang"]),
        ("a noun takes other nouns", "hold.zing",
            &["hold.zing", "hold.bloop", "hold.zang", "hold.quux"], &[]),
        ("with nothing alike, any phrase but a fragment", "jump", &["jump"], &["word.bloop"]),
    ];
    for (name, id, must, never) in rows {
        let question = phrase(&pack, id);
        for seed in 0..32u64 {
            let options = choose_options(&pack, &question, &mut Rng::new(seed));
            let ids: Vec<&str> = options.iter().map(|p| p.id.as_str()).collect();
            let row = format!("{name}, seed {seed}, options {ids:?}");
            let mut texts: Vec<&str> = options.iter().map(|p| p.text.as_str()).collect();
            texts.sort_unstable();
            texts.dedup();
            assert_eq!(texts.len(), 4, "{row}: four distinct texts");
            let asked = ids.iter().filter(|&&option| option == id).count();
            assert_eq!(asked, 1, "{row}: the question once");
            assert!(
                options.iter().all(|p| !p.is_fragment()),
                "{row}: no fragment"
            );
            assert!(
                must.iter().all(|wanted| ids.contains(wanted)),
                "{row}: has {must:?}"
            );
            assert!(
                !never.iter().any(|unwanted| ids.contains(unwanted)),
                "{row}: no {never:?}"
            );
        }
    }
}

fn describe(speech: &Speech) -> String {
    match speech {
        Speech::New(p) => format!("new {}", p.id),
        Speech::Repeat(p) => format!("repeat {}", p.id),
        Speech::Ongoing { phrase, repeat } if *repeat => format!("again {}", phrase.id),
        Speech::Ongoing { phrase, .. } => format!("ongoing {}", phrase.id),
        Speech::DropQueued(id) => format!("drop {id}"),
        Speech::HeldLabel(p) => format!("label {}", p.id),
    }
}

/// Each step is `<ms> <verb> [<name> [<count>]]`; every verb but `break` and `place` is a tick,
/// `unread` one whose inventory could not be read. The expected text lists what the step says,
/// comma separated.
#[test]
fn freshness_decides_when_a_phrase_is_said_again() {
    let pack = fixture_pack("en");
    // (name, repeatSeconds, steps as (step, said))
    type Row = (&'static str, f32, &'static [(&'static str, &'static str)]);
    #[rustfmt::skip]
    let rows: [Row; 11] = [
        ("a first break is new, more with the button held are silent", 0.0, &[
            ("0 attack", ""), ("100 break bloop", "new break.bloop"), ("200 tick", ""),
            ("300 break bloop", ""),
        ]),
        ("pressing again is new only once 10 s passed since it was said", 0.0, &[
            ("0 attack", ""), ("100 break bloop", "new break.bloop"), ("200 release", ""),
            ("300 attack", ""), ("400 break bloop", ""), ("10500 release", ""), ("10600 attack", ""),
            ("10700 break bloop", "new break.bloop"),
        ]),
        ("repeatSeconds brings a held break back 200 ms early", 3.0, &[
            ("0 attack", ""), ("100 break bloop", "new break.bloop"), ("2800 break bloop", ""),
            ("2900 break bloop", "repeat break.bloop"),
        ]),
        ("a second place with the use key held is silent", 0.0, &[
            ("0 use", ""), ("100 place bloop", "new place.bloop"), ("200 place bloop", ""),
        ]),
        ("a pickup is new again after a 2 s pause and 10 s since it was said", 0.0, &[
            ("0 tick", ""), ("100 get bloop 1", "new pickup.bloop"), ("1000 get bloop 1", ""),
            ("11500 get bloop 1", "new pickup.bloop"),
        ]),
        ("a pickup with the attack key held from before is silent", 0.0, &[
            ("0 attack", ""), ("100 get bloop 1", "new pickup.bloop"), ("11500 get bloop 1", ""),
        ]),
        ("a new press makes a pickup new again", 0.0, &[
            ("0 tick", ""), ("100 get bloop 1", "new pickup.bloop"), ("10900 attack", ""),
            ("11000 get bloop 1", "new pickup.bloop"),
        ]),
        ("two items at once wait for counted sentences", 0.0, &[
            ("0 tick", ""), ("100 get quux 2", ""),
        ]),
        ("a tick that could not read the inventory is no pickup", 0.0, &[
            ("0 get bloop 1", ""), ("100 unread", ""), ("200 tick", ""),
        ]),
        ("the held item is said on every change of item", 0.0, &[
            ("0 hold quux", "label hold.quux, repeat hold.quux"), ("100 tick", ""),
            ("200 hold bloop", "label hold.bloop, repeat hold.bloop"),
            ("300 hold quux", "label hold.quux, repeat hold.quux"), ("400 hold air", ""),
            ("500 hold zang", "label hold.zang, repeat hold.zang"),
        ]),
        ("mining says the break as a state; the break itself is then silent", 0.0, &[
            ("0 attack", ""), ("50 mine zing", "ongoing break_hand.zing"), ("100 break zing", ""),
            ("150 stop", ""), ("200 release", "drop break_hand.zing"),
        ]),
    ];
    for (name, repeat_seconds, steps) in rows {
        let config = Config {
            repeat_seconds,
            ..Config::default()
        };
        let mut detector = Detector::default();
        detector.reset("minecraft:air");
        let (mut attack, mut use_key) = (false, false);
        let mut breaking: Option<String> = None;
        let mut held = "minecraft:air".to_string();
        let mut inventory: Vec<(String, i32)> = Vec::new();
        for &(step, expected) in steps {
            let words: Vec<&str> = step.split_whitespace().collect();
            let at: i64 = words[0].parse().expect("a time");
            let target = words.get(2).map(|name| format!("minecraft:{name}"));
            let block = || target.clone().expect("the step names a thing");
            let said: Vec<Speech> = match words[1] {
                "break" => detector
                    .block_broken(&block(), &held, at, &pack, &config)
                    .into_iter()
                    .collect(),
                "place" => detector
                    .block_placed(&block(), at, &pack, &config)
                    .into_iter()
                    .collect(),
                verb => {
                    match verb {
                        "attack" => attack = true,
                        "release" => attack = false,
                        "use" => use_key = true,
                        "mine" => breaking = Some(block()),
                        "stop" => breaking = None,
                        "hold" => held = block(),
                        "get" => inventory.push((block(), words[3].parse().expect("a count"))),
                        "tick" | "unread" => {}
                        other => panic!("{name}: unknown verb {other}"),
                    }
                    let slots: Vec<(&str, i32)> = inventory
                        .iter()
                        .map(|(item, n)| (item.as_str(), *n))
                        .collect();
                    let input = WorldTick {
                        now: at,
                        attack_down: attack,
                        use_down: use_key,
                        breaking: breaking.as_deref(),
                        held_item: &held,
                        held_count: 1,
                        inventory: (verb != "unread").then_some(&slots[..]),
                        ..WorldTick::default()
                    };
                    detector.tick(&input, &pack, &config)
                }
            };
            let got: Vec<String> = said.iter().map(describe).collect();
            assert_eq!(got.join(", "), expected, "{name}: '{step}'");
        }
    }
}

/// How a schedule row changes the calm player's tick.
type Change = fn(&mut WorldTick<'static>);

#[test]
fn questions_wait_for_the_interval_and_a_calm_player() {
    // (name, heard something, teacher on, singleplayer, change, asked)
    #[rustfmt::skip]
    let rows: [(&str, bool, bool, bool, Change, bool); 8] = [
        ("a calm player is asked", true, true, false, |_| {}, true),
        ("nothing heard in this world", false, true, false, |_| {}, false),
        ("the teacher is off", true, false, false, |_| {}, false),
        ("a screen is open", true, true, false, |t| t.screen_open = true, false),
        ("mining is busy", true, true, true, |t| t.breaking = Some("minecraft:bloop"), false),
        ("in water on a server", true, true, false, |t| t.in_water = true, false),
        ("in water in singleplayer is asked anyway", true, true, true, |t| t.in_water = true, true),
        ("away from the keys, even in singleplayer", true, true, true, |t| t.yaw = 0.0, false),
    ];
    for (name, heard, teacher_on, singleplayer, change, asked) in rows {
        let config = Config {
            quiz_interval_seconds: 1.0,
            enabled: teacher_on,
            ..Config::default()
        };
        let mut teacher = teacher_with(config, if heard { HEARD } else { &HEARD[..0] });
        let mut first_ask = None;
        for step in 1..=30 {
            let mut input = calm(START + step * 100);
            input.singleplayer = singleplayer;
            change(&mut input);
            if teacher.world_tick(&input) == Some(QuizEvent::Ask) && first_ask.is_none() {
                first_ask = Some(step * 100);
            }
        }
        let expected = if asked { Some(1000) } else { None };
        assert_eq!(
            first_ask, expected,
            "{name}: asked one interval after joining, or never"
        );
    }
}

#[derive(Clone, Copy)]
enum Act {
    /// Picks the right option or a wrong one, `at` ms after the question opened.
    Choose {
        at: i64,
        right: bool,
        expect: Option<bool>,
    },
    /// The question screen ticks.
    Screen { at: i64, expect: Option<bool> },
    /// Esc, keeping `left` ms.
    Suspend { at: i64, left: i64 },
    /// A world tick with a screen open (`screen`), the question screen showing or not.
    World {
        at: i64,
        screen: bool,
        question_screen: bool,
        expect: Option<QuizEvent>,
    },
}

#[test]
fn an_open_question_runs_on_its_clock() {
    use Act::{Choose, Screen, Suspend, World};
    // (name, acts, answers recorded)
    #[rustfmt::skip]
    let rows: [(&str, &[Act], i32); 6] = [
        ("answers in the first 700 ms are ignored", &[
            Choose { at: 699, right: true, expect: None },
            Choose { at: 700, right: true, expect: Some(true) },
        ], 1),
        ("a wrong option is wrong", &[Choose { at: 800, right: false, expect: Some(false) }], 1),
        ("time-out counts wrong", &[
            Screen { at: 11_999, expect: None },
            Screen { at: 12_000, expect: Some(false) },
        ], 1),
        ("Esc keeps the time left and brings the question back with a new lockout", &[
            Suspend { at: 5_000, left: 7_000 },
            Screen { at: 30_000, expect: None },
            World { at: 30_000, screen: true, question_screen: false, expect: None },
            World { at: 40_000, screen: false, question_screen: false, expect: Some(QuizEvent::Resume) },
            Choose { at: 40_699, right: true, expect: None },
            Screen { at: 46_999, expect: None },
            Screen { at: 47_000, expect: Some(false) },
        ], 1),
        ("Esc in the last second keeps one second", &[Suspend { at: 11_800, left: 1_000 }], 0),
        ("another screen drops the question unanswered", &[
            World { at: 2_000, screen: true, question_screen: false, expect: Some(QuizEvent::Dropped) },
            Choose { at: 2_100, right: true, expect: None },
        ], 0),
    ];
    for (name, acts, answers) in rows {
        let config = Config {
            quiz_interval_seconds: 1.0,
            ..Config::default()
        };
        let mut teacher = teacher_with(config, HEARD);
        let opened = ask(&mut teacher);
        let question = teacher.question().expect("a question").question.id.clone();
        for &act in acts {
            match act {
                Choose { at, right, expect } => {
                    let index = teacher
                        .question()
                        .and_then(|open| {
                            let answer = &open.question.id;
                            open.options.iter().position(|p| (&p.id == answer) == right)
                        })
                        .unwrap_or(0);
                    let got = teacher.choose(index, opened + at).map(|o| o.correct);
                    assert_eq!(got, expect, "{name}: choose at {at}");
                }
                Screen { at, expect } => {
                    let got = teacher.question_tick(opened + at).map(|o| o.correct);
                    assert_eq!(got, expect, "{name}: screen tick at {at}");
                }
                Suspend { at, left } => {
                    assert!(
                        teacher.suspend_question(opened + at),
                        "{name}: suspend at {at}"
                    );
                    let kept = teacher.question().map(|open| open.left_ms(opened + at));
                    assert_eq!(kept, Some(left), "{name}: time kept at {at}");
                }
                World {
                    at,
                    screen,
                    question_screen,
                    expect,
                } => {
                    let mut input = calm(opened + at);
                    input.screen_open = screen;
                    input.question_screen_open = question_screen;
                    let got = teacher.world_tick(&input);
                    assert_eq!(got, expect, "{name}: world tick at {at}");
                }
            }
        }
        let recorded = teacher.progress.asked(&question);
        assert_eq!(recorded, answers, "{name}: answers recorded");
    }
}

#[test]
fn lipsync_gives_the_mouth_and_the_lengths() {
    let lipsync = Lipsync::parse(r#"{"clips": {"a": "1221000", "b": "000"}}"#).expect("parses");
    // (name, clip, ms, mouth, length, spoken); the window defaults to 40 ms.
    type Row = (&'static str, &'static str, i64, i32, Option<i64>, i64);
    #[rustfmt::skip]
    let rows: [Row; 10] = [
        ("the first window", "a", 0, 1, Some(280), 160),
        ("the end of the first window", "a", 39, 1, Some(280), 160),
        ("the second window", "a", 40, 2, Some(280), 160),
        ("the fourth window", "a", 120, 1, Some(280), 160),
        ("trailing silence", "a", 160, 0, Some(280), 160),
        ("after the track", "a", 280, 0, Some(280), 160),
        ("just before the start rounds to the first window", "a", -1, 1, Some(280), 160),
        ("a window before the start", "a", -40, 0, Some(280), 160),
        ("a silent clip", "b", 0, 0, Some(120), 0),
        ("a clip with no track", "missing", 0, -1, None, -1),
    ];
    for (name, clip, ms, mouth, length, spoken) in rows {
        assert_eq!(lipsync.mouth(clip, ms), mouth, "{name}: mouth");
        assert_eq!(lipsync.length_ms(clip), length, "{name}: length");
        assert_eq!(lipsync.spoken_ms(clip), spoken, "{name}: spoken");
    }
}

/// Two clips in a chain: `pickup.bloop` (7 windows, speech over after 4) then `hold.bloop`
/// (2 windows). The second starts at 50 + 160 + 40 ms; it ends 50 + 80 ms after it started, and
/// the text stays 3 s more.
#[test]
fn the_speaker_times_clips_by_their_lipsync() {
    let mut teacher = teacher_with(Config::default(), &[]);
    let display = phrase(&teacher.pack, "pickup.bloop");
    let clips = [display.clone(), phrase(&teacher.pack, "hold.bloop")];
    teacher.say_sequence(&display, &clips, true, START);
    // (ms, clips started, speaking, mouth, portrait frame, text shown)
    type Row = (i64, &'static [&'static str], bool, i32, i32, bool);
    #[rustfmt::skip]
    let rows: [Row; 10] = [
        (0, &["pickup.bloop"], true, 0, 0, true),
        (100, &[], true, 2, 2, true),
        (210, &[], true, 0, 0, true),
        (249, &[], true, 0, 0, true),
        (250, &["hold.bloop"], true, 0, 0, true),
        (300, &[], true, 1, 1, true),
        (379, &[], true, 2, 2, true),
        (380, &[], false, 0, 0, true),
        (3380, &[], false, 0, 0, true),
        (3381, &[], false, 0, 0, false),
    ];
    for (at, plays, speaking, mouth, frame, shown) in rows {
        let now = START + at;
        let played: Vec<String> = teacher
            .speaker_tick(now, false)
            .into_iter()
            .filter_map(|command| match command {
                SoundCommand::Play { phrase_id, .. } => Some(phrase_id),
                SoundCommand::Stop => None,
            })
            .collect();
        assert_eq!(played, plays, "{at} ms: clips started");
        assert_eq!(teacher.is_speaking(), speaking, "{at} ms: speaking");
        assert_eq!(teacher.mouth(now), mouth, "{at} ms: mouth");
        assert_eq!(
            teacher.portrait_frame(now),
            frame,
            "{at} ms: portrait frame"
        );
        assert_eq!(
            teacher.hud_text(now).is_some(),
            shown,
            "{at} ms: text shown"
        );
    }
    let heard = &teacher.progress.world().heard;
    let counts = (heard.get("pickup.bloop"), heard.get("hold.bloop"));
    assert_eq!(counts, (Some(&1), Some(&1)), "both clips count as heard");
}

#[test]
fn phrases_split_into_role_colours() {
    const PRON: i32 = -7_419_393;
    const VERB: i32 = -22_195;
    const PART: i32 = -4_473_925;
    const OBJ: i32 = -6_560_899;
    const WHITE: i32 = 0x00FF_FFFF;
    let pack = fixture_pack("en");
    // (name, phrase, grammar colours on, spans)
    type Row = (
        &'static str,
        &'static str,
        bool,
        &'static [(&'static str, i32)],
    );
    #[rustfmt::skip]
    let rows: [Row; 4] = [
        ("parts in their colours, the full stop as a particle", "break.bloop", true,
            &[("Ich ", PRON), ("zerbreche ", VERB), ("den ", PART), ("Bloop", OBJ), (".", PART)]),
        ("no space after an elided word, other roles in the fallback", "use.zing", true,
            &[("J'", PRON), ("zing ", VERB), ("den ", PART), ("Zing", WHITE), (".", PART)]),
        ("no parts", "jump", true, &[("Ich hüpfe.", WHITE)]),
        ("grammar colours off", "break.bloop", false, &[("Ich zerbreche den Bloop.", WHITE)]),
    ];
    for (name, id, grammar, expected) in rows {
        let config = Config {
            hud_colour_grammar: grammar,
            ..Config::default()
        };
        let got: Vec<(String, i32)> = spans(&phrase(&pack, id), " ", &config, WHITE)
            .into_iter()
            .map(|span| (span.text, span.colour))
            .collect();
        let expected: Vec<(String, i32)> = expected
            .iter()
            .map(|&(text, colour)| (text.to_string(), colour))
            .collect();
        assert_eq!(got, expected, "{name}");
    }
}

#[test]
fn progress_file_keeps_the_original_shape() {
    assert_eq!(
        singleplayer_world_key("Testwelt", -1),
        "save:Testwelt:ffffffffffffffff",
        "a world key writes the seed as Java's Long.toHexString"
    );
    let mut written = Progress::default();
    written.switch_world("save:Testwelt:ff");
    written.heard("break.bloop");
    written.heard("break.bloop");
    written.heard("hold.quux");
    written.answered("break.bloop", true, 1_700_000_000_000);
    written.answered("hold.quux", false, 1_700_000_000_500);
    let older = Progress::parse(
        r#"{"worlds": {"server:example.org": {"heard": {"jump": 3}, "quiz": {"jump": [2, 1]},
            "exams": [{"at": 5, "minutes": 20.0, "type": "choice", "right": 1, "total": 2}]}},
            "deaths": 1}"#,
    )
    .expect("an older file parses");
    let rows: [(&str, Progress, Value); 2] = [
        (
            "what the port writes",
            written,
            json!({
                "worlds": {"save:Testwelt:ff": {
                    "heard": {"break.bloop": 2, "hold.quux": 1},
                    "quiz": {"break.bloop": [1, 1], "hold.quux": [1, 0]},
                    "correctAt": {"break.bloop": 1_700_000_000_000i64},
                    "heartsLost": 0,
                    "deaths": 0,
                    "exams": []
                }},
                "correctTotal": {"break.bloop": 1},
                "lastCorrectAt": {"break.bloop": 1_700_000_000_000i64},
                "deaths": 0
            }),
        ),
        (
            "a file with missing keys and an exam keeps the exam",
            older,
            json!({
                "worlds": {"server:example.org": {
                    "heard": {"jump": 3},
                    "quiz": {"jump": [2, 1]},
                    "correctAt": {},
                    "heartsLost": 0,
                    "deaths": 0,
                    "exams": [{"at": 5, "minutes": 20.0, "type": "choice", "right": 1, "total": 2}]
                }},
                "correctTotal": {},
                "lastCorrectAt": {},
                "deaths": 1
            }),
        ),
    ];
    for (name, progress, expected) in rows {
        assert_eq!(progress.to_json(), expected, "{name}: written");
        let back = Progress::parse(&progress.to_json_string()).expect("the file reads back");
        assert_eq!(back.to_json(), expected, "{name}: read back");
    }
}

#[test]
fn config_file_keeps_names_defaults_and_unknown_keys() {
    let config = Config::parse(
        r#"{"quizIntervalSeconds": 30, "muted": "true", "laterSetting": [1, 2], "voice": null}"#,
    )
    .expect("the config parses");
    let written = config.to_json();
    #[rustfmt::skip]
    let rows: [(&str, &str, Value); 7] = [
        ("a number read as a float", "quizIntervalSeconds", json!(30.0)),
        ("a boolean written as a string", "muted", json!(true)),
        ("a key this port does not read", "laterSetting", json!([1, 2])),
        ("null keeps the default", "voice", json!("")),
        ("a missing float written as Gson writes it", "sameEventCooldownSeconds", json!(0.6)),
        ("a missing colour", "hudColorZh", json!(-8054)),
        ("the config version", "configVersion", json!(8)),
    ];
    for (name, key, expected) in rows {
        assert_eq!(written.get(key), Some(&expected), "{name}: {key}");
    }
    let wrong_type = Config::parse(r#"{"muted": [true]}"#);
    assert!(
        wrong_type.is_err(),
        "a key of the wrong type fails the read"
    );
}
