//! The core's tests: one per behavior, each case a row. The fixture pack is invented for these
//! tests; expected values follow the original mod's rules.
use super::consequences::{BUFFS, DEBUFFS};
use super::detector::{Detector, Speech, break_phrase, plural};
use super::quiz::choose_options;
use super::rng::Rng;
use super::typed::{PackWords, judge};
use super::*;
use mod_api::serde_json::{Value, json};

const PHRASES: &str = r#"{
  "meta": {"name": "Testisch", "ui": {"correct": "Ja!", "wrong": "Nein!"}, "portrait": {"frames": 9},
           "hintLanguages": ["es", "pt"]},
  "voices": ["v1"],
  "speeds": ["normal", "slow"],
  "words": [
    {"key": "bloop", "kind": "block", "patterns": ["bloop", "*_bloop"], "measure": "m", "countEn": "bloop blocks"},
    {"key": "zing", "kind": "block", "patterns": ["zing"]},
    {"key": "zang", "kind": "block", "patterns": ["zang"]},
    {"key": "stein", "kind": "block", "patterns": ["stone"]},
    {"key": "allstein", "kind": "block", "patterns": ["*stone*"]},
    {"key": "fels", "kind": "block", "patterns": ["stone"]},
    {"key": "kerzenkuchen", "kind": "block", "patterns": ["*candle*cake"]},
    {"key": "quux", "kind": "item", "patterns": ["quux"]},
    {"key": "palin", "kind": "item", "patterns": ["ab*ba"]},
    {"key": "muh", "kind": "entity", "patterns": ["cow", "mooshroom"]},
    {"key": "tuer", "kind": "block", "patterns": ["*_door"], "measure": "f"},
    {"key": "knusper", "kind": "item", "patterns": ["cookie"]},
    {"key": "trank", "kind": "item", "patterns": ["potion"]},
    {"key": "wauz", "kind": "entity", "patterns": ["wolf"]},
    {"key": "gaul", "kind": "entity", "patterns": ["horse"]}
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
    {"id": "jump", "action": "jump", "key": null, "text": "Ich hüpfe.", "en": "I hop."},
    {"id": "hit.muh", "action": "hit", "key": "muh", "text": "Ich haue die Muh.", "en": "I bop the moo."},
    {"id": "kill.muh", "action": "kill", "key": "muh", "text": "Ich besiege die Muh.", "en": "I beat the moo."},
    {"id": "feed.muh", "action": "feed", "key": "muh", "text": "Ich füttere die Muh.", "en": "I feed the moo."},
    {"id": "ride.gaul", "action": "ride", "key": "gaul", "text": "Ich reite den Gaul.", "en": "I ride the nag."},
    {"id": "open.tuer", "action": "open", "key": "tuer", "text": "Ich öffne die Tuer.", "en": "I open the dor."},
    {"id": "close.tuer", "action": "close", "key": "tuer", "text": "Ich schließe die Tuer.", "en": "I shut the dor."},
    {"id": "eat.knusper", "action": "eat", "key": "knusper", "text": "Ich esse den süßen Knusperkeks.", "en": "I eat the sweet crunch biscuit."},
    {"id": "drink.trank", "action": "drink", "key": "trank", "text": "Ich trinke den Trank für Zing.", "en": "I drink the brew for zing."},
    {"id": "eat.trank", "action": "eat", "key": "trank", "text": "Ich esse den Trank.", "en": "I eat the brew."},
    {"id": "count.pickup.3", "action": "pickupcount", "key": null, "text": "Ich grapsche drei", "en": "I grab three",
     "parts": [{"text": "Ich", "role": "pron"}, {"text": "grapsche", "role": "verb"}, {"text": "drei", "role": "num"}]},
    {"id": "plural.bloop", "action": "plural", "key": "bloop", "text": "Bloopen", "en": "bloops",
     "parts": [{"text": "Bloopen", "role": "obj"}]},
    {"id": "plural.bloop.hold", "action": "plural", "key": "bloop", "text": "Bloopen da", "en": "bloops there"},
    {"id": "count.hold.3.m", "action": "holdcount", "key": null, "text": "drei", "en": "three"},
    {"id": "count.3.m", "action": "count", "key": null, "text": "drei Stück.", "en": "three pieces.",
     "parts": [{"text": "drei", "role": "num"}, {"text": "Stück", "role": "obj"}]},
    {"id": "count.3.ge", "action": "count", "key": null, "text": "drei Teile.", "en": "three bits."}
  ]
}"#;

/// The fixed phrases the action table says, each its own action, as in the original's packs.
const FIXED: [&str; 35] = [
    "attack",
    "kill_it",
    "sleep",
    "fish",
    "fly",
    "swim",
    "climb",
    "sprint",
    "sneak",
    "walk",
    "die",
    "hurt",
    "fall",
    "burning",
    "hungry",
    "night",
    "day",
    "rain",
    "rain_stop",
    "snow",
    "snow_stop",
    "thunder",
    "weather_clear",
    "weather_night",
    "weather_rain",
    "weather_snow",
    "weather_thunder",
    "hot",
    "cold",
    "not_hot",
    "not_cold",
    "dim_overworld",
    "dim_nether",
    "dim_end",
    "angry",
];

const LIPSYNC: &str = r#"{"window": 40, "clips": {
  "v1.normal.pickup.bloop": "1221000",
  "v1.normal.hold.bloop": "12",
  "v1.normal.count.pickup.3": "1210",
  "v1.normal.plural.bloop": "21",
  "v1.slow.pickup.bloop": "1"
}}"#;

const WORLD: &str = "save:Testwelt:2a";
const START: i64 = 1_000_000;
const HEARD: &[&str] = &["break.bloop", "hold.quux", "pickup.quux"];
/// An item registry, in registry order, for the questions' pictures.
const REGISTRY: [&str; 5] = [
    "minecraft:stone",
    "minecraft:red_bloop",
    "minecraft:bloop",
    "minecraft:mooshroom_spawn_egg",
    "minecraft:cow_spawn_egg",
];

fn fixture_pack(language: &str) -> Pack {
    let fixed: Vec<String> = FIXED
        .iter()
        .map(|id| format!(r#"{{"id": "{id}", "action": "{id}", "key": null, "text": "Satz {id}.", "en": "Line {id}."}},"#))
        .collect();
    let phrases = PHRASES.replacen(
        r#""phrases": ["#,
        &format!(r#""phrases": [{}"#, fixed.concat()),
        1,
    );
    Pack::parse("zhnarrator_xx", &phrases, Some(LIPSYNC), language)
        .expect("the fixture pack parses")
}

fn phrase(pack: &Pack, id: &str) -> Phrase {
    pack.phrase(id)
        .cloned()
        .unwrap_or_else(|| panic!("fixture phrase {id}"))
}

/// A teacher in [`WORLD`] whose progress has heard `heard`, joined at [`START`].
fn teacher_with(config: Config, heard: &[&str]) -> Teacher {
    teacher_seeded(config, heard, 7)
}

fn teacher_seeded(config: Config, heard: &[&str], seed: u64) -> Teacher {
    let mut progress = Progress::default();
    progress.switch_world(WORLD);
    for id in heard {
        progress.heard(id);
    }
    let mut teacher = Teacher::new(fixture_pack("en"), config, progress, seed);
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
    ask_after(teacher, START)
}

/// Ticks every 100 ms from `from` until a question is asked; returns when.
fn ask_after(teacher: &mut Teacher, from: i64) -> i64 {
    for step in 1..=50 {
        let now = from + step * 100;
        if teacher.world_tick(&calm(now)) == Some(QuizEvent::Ask) {
            return now;
        }
    }
    panic!("no question within 5 s of {from}");
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
    /// The item a question about this word key shows, from [`REGISTRY`].
    Picture,
}

#[test]
fn registry_ids_find_their_word_and_phrase() {
    let pack = fixture_pack("en");
    let pictures = pack.pictures(REGISTRY.map(str::to_string).to_vec());
    #[rustfmt::skip]
    let rows: [(&str, Lookup, &str, Option<&str>); 26] = [
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
        ("a picture is the first registry item the key maps", Lookup::Picture, "bloop", Some("minecraft:red_bloop")),
        ("an exact pattern names an item an earlier word took", Lookup::Picture, "fels", Some("minecraft:stone")),
        ("then the lowest spawn egg of the entity's names", Lookup::Picture, "muh", Some("minecraft:mooshroom_spawn_egg")),
        ("no item and no egg, no picture", Lookup::Picture, "zang", None),
    ];
    for (name, lookup, id, expected) in rows {
        let got: Option<String> = match lookup {
            Lookup::Key(kind) => pack.key_for(kind, id).map(str::to_string),
            Lookup::Item(action) => pack.for_item(action, id).map(|p| p.id.clone()),
            Lookup::Break(held) => break_phrase(&pack, held, id).map(|p| p.id.clone()),
            Lookup::Use => pack.use_phrase(id, false).map(|p| p.id.clone()),
            Lookup::Hint(language) => fixture_pack(language).phrase(id).map(|p| p.en.clone()),
            Lookup::Picture => pack.picture(id, &pictures).map(str::to_string),
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
        Speech::Now(p) => format!("now {}", p.id),
        Speech::Counted { display, .. } => format!("counted {}", display.id),
        Speech::Died => "died".to_string(),
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
    let rows: [Row; 10] = [
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
        detector.reset("minecraft:air", 0);
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

/// Ticks a calm player every 100 ms for 3 s; returns when the first question came and how it is
/// answered.
fn first_question(
    teacher: &mut Teacher,
    change: impl Fn(&mut WorldTick<'static>),
) -> Option<(i64, QuestionKind)> {
    let mut first = None;
    for step in 1..=30 {
        let mut input = calm(START + step * 100);
        change(&mut input);
        if teacher.world_tick(&input) == Some(QuizEvent::Ask) && first.is_none() {
            first = teacher.question().map(|open| (step * 100, open.kind));
        }
    }
    first
}

#[test]
fn questions_wait_for_the_interval_and_a_calm_player() {
    use QuestionKind::{Choice, Typing};
    // (name, heard something, teacher on, singleplayer, quiz mode, change, asked)
    type Row = (
        &'static str,
        bool,
        bool,
        bool,
        &'static str,
        Change,
        Option<QuestionKind>,
    );
    #[rustfmt::skip]
    let rows: [Row; 10] = [
        ("a calm player is asked", true, true, false, "choice", |_| {}, Some(Choice)),
        ("nothing heard in this world", false, true, false, "choice", |_| {}, None),
        ("the teacher is off", true, false, false, "choice", |_| {}, None),
        ("a screen is open", true, true, false, "choice", |t| t.screen_open = true, None),
        ("mining is busy", true, true, true, "choice", |t| t.breaking = Some("minecraft:bloop"), None),
        ("in water on a server", true, true, false, "choice", |t| t.in_water = true, None),
        ("in water in singleplayer is asked anyway", true, true, true, "choice", |t| t.in_water = true, Some(Choice)),
        ("away from the keys, even in singleplayer", true, true, true, "choice", |t| t.yaw = 0.0, None),
        ("the typing mode asks to type", true, true, false, "typing", |_| {}, Some(Typing)),
        ("voice answers are not ported: speech asks with buttons", true, true, false, "speech", |_| {}, Some(Choice)),
    ];
    for (name, heard, teacher_on, singleplayer, mode, change, asked) in rows {
        let config = Config {
            quiz_interval_seconds: 1.0,
            enabled: teacher_on,
            quiz_mode: mode.to_string(),
            ..Config::default()
        };
        let mut teacher = teacher_with(config, if heard { HEARD } else { &HEARD[..0] });
        let first = first_question(&mut teacher, |input| {
            input.singleplayer = singleplayer;
            change(input);
        });
        assert_eq!(
            first,
            asked.map(|kind| (1000, kind)),
            "{name}: asked one interval after joining, or never"
        );
    }
    let config = Config {
        quiz_interval_seconds: 1.0,
        quiz_mode: "mixed".to_string(),
        ..Config::default()
    };
    let typed = (0..300u64)
        .filter(|&seed| {
            let mut teacher = teacher_seeded(config.clone(), HEARD, seed);
            first_question(&mut teacher, |_| {}).map(|(_, kind)| kind) == Some(Typing)
        })
        .count();
    assert!(
        (70..=130).contains(&typed),
        "mixed asks to type one time in three: {typed} of 300"
    );
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
    /// The edit box of a typed question now holds this.
    Fill(Typing),
    /// Enter on a typed question with this in the box.
    Submit {
        at: i64,
        typed: Typing,
        expect: Option<Verdict>,
    },
    /// The question's box holds this, as the screen would show it.
    Kept(Typing),
}

/// What is typed: the question's own text, another text, or only spaces.
#[derive(Clone, Copy)]
enum Typing {
    Right,
    Other,
    Blank,
}

impl Typing {
    fn text(self, question: &Phrase) -> String {
        match self {
            Typing::Right => question.text.clone(),
            Typing::Other => "Etwas ganz anderes".to_string(),
            Typing::Blank => "  ".to_string(),
        }
    }
}

#[test]
fn an_open_question_runs_on_its_clock() {
    use Act::{Choose, Fill, Kept, Screen, Submit, Suspend, World};
    use Typing::{Blank, Other, Right};
    // (name, quiz mode, quizAnswerSeconds, acts, answers recorded)
    type Row = (&'static str, &'static str, f32, &'static [Act], i32);
    #[rustfmt::skip]
    let rows: [Row; 11] = [
        ("answers in the first 700 ms are ignored", "choice", 12.0, &[
            Choose { at: 699, right: true, expect: None },
            Choose { at: 700, right: true, expect: Some(true) },
        ], 1),
        ("a wrong option is wrong", "choice", 12.0, &[Choose { at: 800, right: false, expect: Some(false) }], 1),
        ("time-out counts wrong", "choice", 12.0, &[
            Screen { at: 11_999, expect: None },
            Screen { at: 12_000, expect: Some(false) },
        ], 1),
        ("Esc keeps the time left and brings the question back with a new lockout", "choice", 12.0, &[
            Suspend { at: 5_000, left: 7_000 },
            Screen { at: 30_000, expect: None },
            World { at: 30_000, screen: true, question_screen: false, expect: None },
            World { at: 40_000, screen: false, question_screen: false, expect: Some(QuizEvent::Resume) },
            Choose { at: 40_699, right: true, expect: None },
            Screen { at: 46_999, expect: None },
            Screen { at: 47_000, expect: Some(false) },
        ], 1),
        ("Esc in the last second keeps one second", "choice", 12.0, &[Suspend { at: 11_800, left: 1_000 }], 0),
        ("another screen drops the question unanswered", "choice", 12.0, &[
            World { at: 2_000, screen: true, question_screen: false, expect: Some(QuizEvent::Dropped) },
            Choose { at: 2_100, right: true, expect: None },
        ], 0),
        ("typed input waits 400 ms, and an empty box submits nothing", "typing", 12.0, &[
            Submit { at: 399, typed: Right, expect: None },
            Submit { at: 400, typed: Blank, expect: None },
            Choose { at: 400, right: true, expect: None },
            Submit { at: 400, typed: Right, expect: Some(Verdict::Exact) },
        ], 1),
        ("a typed question has twice the answer time; time-out submits the box", "typing", 12.0, &[
            Fill(Right),
            Screen { at: 23_999, expect: None },
            Screen { at: 24_000, expect: Some(true) },
        ], 1),
        ("at least 20 s, and time-out with nothing typed is wrong", "typing", 5.0, &[
            Screen { at: 19_999, expect: None },
            Screen { at: 20_000, expect: Some(false) },
        ], 1),
        ("Esc keeps the box and brings it back with a 400 ms lockout", "typing", 12.0, &[
            Fill(Other),
            Suspend { at: 5_000, left: 19_000 },
            World { at: 40_000, screen: false, question_screen: false, expect: Some(QuizEvent::Resume) },
            Kept(Other),
            Submit { at: 40_399, typed: Right, expect: None },
            Submit { at: 40_400, typed: Other, expect: Some(Verdict::Wrong) },
        ], 1),
        ("a wrong text is wrong", "typing", 12.0, &[Submit { at: 500, typed: Other, expect: Some(Verdict::Wrong) }], 1),
    ];
    for (name, mode, answer_seconds, acts, answers) in rows {
        let config = Config {
            quiz_interval_seconds: 1.0,
            quiz_mode: mode.to_string(),
            quiz_answer_seconds: answer_seconds,
            ..Config::default()
        };
        let mut teacher = teacher_with(config, HEARD);
        let opened = ask(&mut teacher);
        let asked = teacher.question().expect("a question").question.clone();
        let question = asked.id.clone();
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
                Fill(typed) => teacher.set_typed(&typed.text(&asked)),
                Submit { at, typed, expect } => {
                    let got = teacher
                        .submit_typed(opened + at, &typed.text(&asked))
                        .map(|o| o.typed.expect("a typed answer").verdict);
                    assert_eq!(got, expect, "{name}: submit at {at}");
                }
                Kept(typed) => {
                    let kept = teacher.question().map(|open| open.typed().to_string());
                    assert_eq!(kept, Some(typed.text(&asked)), "{name}: the box is kept");
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

/// Ticks a calm player with these inventory slots.
fn pick_up(teacher: &mut Teacher, now: i64, slots: &[(&str, i32)]) {
    let input = WorldTick {
        inventory: Some(slots),
        ..calm(now)
    };
    teacher.world_tick(&input);
}

/// Scenarios, each timed from [`START`]:
/// - a chain: `pickup.bloop` (7 windows, speech over after 4) then `hold.bloop` (2 windows). The
///   second starts at 50 + 160 + 40 ms; it ends 50 + 80 ms after it started, and the text stays
///   3 s more.
/// - a joined count: `count.pickup.3` (4 windows) then `plural.bloop` (2 windows), the second at
///   160 ms, where the first track ends. The mouth reads the joined track: the first track until
///   the second's starts 50 ms later.
/// - the speed key: the next clip plays the slow event.
#[test]
fn the_speaker_times_clips_by_their_lipsync() {
    // (ms, events started, speaking, mouth, portrait frame, text shown)
    type Row = (i64, &'static [&'static str], bool, i32, i32, bool);
    // (name, setup, rows, times heard)
    type Scenario = (
        &'static str,
        fn(&mut Teacher),
        &'static [Row],
        &'static [(&'static str, Option<i32>)],
    );
    #[rustfmt::skip]
    let scenarios: [Scenario; 3] = [
        ("two clips in a chain", |teacher| {
            let display = phrase(&teacher.pack, "pickup.bloop");
            let clips = [display.clone(), phrase(&teacher.pack, "hold.bloop")];
            teacher.say_sequence(&display, &clips, true, START);
        }, &[
            (0, &["normal.pickup.bloop"], true, 0, 0, true),
            (100, &[], true, 2, 2, true),
            (210, &[], true, 0, 0, true),
            (249, &[], true, 0, 0, true),
            (250, &["normal.hold.bloop"], true, 0, 0, true),
            (300, &[], true, 1, 1, true),
            (379, &[], true, 2, 2, true),
            (380, &[], false, 0, 0, true),
            (3380, &[], false, 0, 0, true),
            (3381, &[], false, 0, 0, false),
        ], &[("pickup.bloop", Some(1)), ("hold.bloop", Some(1))]),
        ("a counted pickup joins its two clips", |teacher| {
            pick_up(teacher, START, &[]);
            pick_up(teacher, START, &[("minecraft:bloop", 3)]);
        }, &[
            (0, &["normal.count.pickup.3"], true, 0, 0, true),
            (100, &[], true, 2, 2, true),
            (159, &[], true, 1, 1, true),
            (160, &["normal.plural.bloop"], true, 1, 1, true),
            (209, &[], true, 0, 0, true),
            (210, &[], true, 2, 2, true),
            (250, &[], true, 1, 1, true),
            (289, &[], true, 1, 1, true),
            (290, &[], false, 0, 0, true),
            (3291, &[], false, 0, 0, false),
        ], &[("pickup.bloop", Some(1)), ("count.pickup.3", None), ("plural.bloop", None)]),
        ("the speed key slows the next clip", |teacher| {
            assert_eq!(teacher.cycle_speed(), "slow", "K: normal to slow");
            assert_eq!(teacher.cycle_speed(), "normal", "K: slow back to normal");
            assert_eq!(teacher.cycle_speed(), "slow", "K: normal to slow again");
            let display = phrase(&teacher.pack, "pickup.bloop");
            teacher.say_sequence(&display, std::slice::from_ref(&display), true, START);
        }, &[
            (0, &["slow.pickup.bloop"], true, 0, 0, true),
            (50, &[], true, 1, 1, true),
            (90, &[], false, 0, 0, true),
        ], &[("pickup.bloop", Some(1))]),
    ];
    for (name, setup, rows, heard) in scenarios {
        let mut teacher = teacher_with(Config::default(), &[]);
        setup(&mut teacher);
        for &(at, plays, speaking, mouth, frame, shown) in rows {
            let now = START + at;
            let played: Vec<String> = teacher
                .speaker_tick(now, false)
                .into_iter()
                .filter_map(|command| match command {
                    SoundCommand::Play { event, .. } => {
                        event.strip_prefix("zhnarrator_xx:v1.").map(str::to_string)
                    }
                    SoundCommand::Stop => None,
                })
                .collect();
            assert_eq!(played, plays, "{name}, {at} ms: events started");
            assert_eq!(teacher.is_speaking(), speaking, "{name}, {at} ms: speaking");
            assert_eq!(teacher.mouth(now), mouth, "{name}, {at} ms: mouth");
            assert_eq!(
                teacher.portrait_frame(now),
                frame,
                "{name}, {at} ms: portrait frame"
            );
            assert_eq!(
                teacher.hud_text(now).is_some(),
                shown,
                "{name}, {at} ms: text shown"
            );
        }
        let counts = &teacher.progress.world().heard;
        for &(id, times) in heard {
            assert_eq!(counts.get(id).copied(), times, "{name}: {id} heard");
        }
    }
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
    let rows: [(&str, &str, Value); 11] = [
        ("a number read as a float", "quizIntervalSeconds", json!(30.0)),
        ("a boolean written as a string", "muted", json!(true)),
        ("a key this port does not read", "laterSetting", json!([1, 2])),
        ("null keeps the default", "voice", json!("")),
        ("a missing float written as Gson writes it", "sameEventCooldownSeconds", json!(0.6)),
        ("a missing colour", "hudColorZh", json!(-8054)),
        ("the config version", "configVersion", json!(8)),
        ("the penalty", "quizPenalty", json!("debuff")),
        ("the angry teacher's streak", "angryAfterWrong", json!(5)),
        ("the jump cooldown", "jumpCooldownSeconds", json!(4.0)),
        ("the climate repeat", "climateRepeatSeconds", json!(0.0)),
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

#[test]
fn typed_answers_follow_the_german_rules() {
    let pack = fixture_pack("en");
    let words = PackWords::new(pack.phrases().iter().map(|p| p.text.as_str()));
    use Verdict::{Close, Exact, Wrong};
    const KEKS: &str = "Ich esse den süßen Knusperkeks.";
    // (name, typed, the question's text, verdict)
    #[rustfmt::skip]
    let rows: [(&str, &str, &str, Verdict); 19] = [
        ("the text itself", KEKS, KEKS, Exact),
        ("case and the full stop do not count", "ICH ESSE DEN SÜßEN KNUSPERKEKS", KEKS, Exact),
        ("a decomposed umlaut is the umlaut", "Ich esse den su\u{308}ßen Knusperkeks", KEKS, Exact),
        ("ss for ß is close", "ich esse den süssen knusperkeks", KEKS, Close),
        ("an umlaut without its dots is close", "ich esse den sußen knusperkeks", KEKS, Close),
        ("two typos in a word of 8 letters or more are close", "ich esse den süßen knusprkeksx", KEKS, Close),
        ("three are wrong", "ich esse den süßen knusbrkekz", KEKS, Wrong),
        ("a missing article is wrong", "ich esse süßen knusperkeks", KEKS, Wrong),
        ("an extra word is wrong", "ich esse den süßen knusperkeks heute", KEKS, Wrong),
        ("spaces count", "ich esse den süßenknusperkeks", KEKS, Wrong),
        ("one typo in a word of 4 to 7 letters is close", "der Zinf", "der Zing", Close),
        ("a word of 3 letters takes no typo", "dir Zing", "der Zing", Wrong),
        ("a typo that is itself a word of the pack is wrong", "der Zang", "der Zing", Wrong),
        ("hyphens and dashes are spaces", "der-Zing", "der Zing", Exact),
        ("an apostrophe may be left out", "Jzing den Zing", "J'zing den Zing.", Exact),
        ("or read as a space", "j zing den zing", "J'zing den Zing.", Exact),
        ("the words split at the apostrophe take typos too", "j zinf den zing", "J'zing den Zing.", Close),
        ("u for ü in a short word folds the same", "ich trinke den trank fur zing", "Ich trinke den Trank für Zing.", Close),
        ("ue for ü in a short word is wrong", "ich trinke den trank fuer zing", "Ich trinke den Trank für Zing.", Wrong),
    ];
    for (name, typed, text, verdict) in rows {
        assert_eq!(judge(typed, text, &words), verdict, "{name}: '{typed}'");
    }
    for blank in ["", "  ", "?!", "\u{A0}"] {
        assert_eq!(
            judge(blank, "der Zing", &words),
            Wrong,
            "nothing typed: '{blank}'"
        );
    }
}

/// An answer's server actions in short: `debuff`, `buff`, `hostile mobs`, `friendly mobs`,
/// `hearts <lost> <modifier>`, `kill`, `drop`, `delete`; `nothing` for none, `offline` with no
/// integrated server; then `angry` when the teacher got angry.
fn consequence(outcome: &Outcome) -> String {
    let server = match &outcome.server {
        None => "offline".to_string(),
        Some(actions) if actions.is_empty() => "nothing".to_string(),
        Some(actions) => actions
            .iter()
            .map(|action| match action {
                ServerAction::Effect(effect) if DEBUFFS.contains(effect) => "debuff".to_string(),
                ServerAction::Effect(effect) if BUFFS.contains(effect) => "buff".to_string(),
                ServerAction::Effect(effect) => format!("unknown effect {}", effect.id),
                ServerAction::Mobs { hostile: true, .. } => "hostile mobs".to_string(),
                ServerAction::Mobs { .. } => "friendly mobs".to_string(),
                ServerAction::LostHearts { lost, modifier } => format!("hearts {lost} {modifier}"),
                ServerAction::Kill => "kill".to_string(),
                ServerAction::Wipe(Wipe::Drop) => "drop".to_string(),
                ServerAction::Wipe(Wipe::Delete) => "delete".to_string(),
            })
            .collect::<Vec<_>>()
            .join(" "),
    };
    let angry = if outcome.angry.is_some() {
        " angry"
    } else {
        ""
    };
    format!("{server}{angry}")
}

#[derive(Clone, Copy)]
enum Step {
    /// A question answered right or wrong, and its consequence in short.
    Answer(bool, &'static str),
    /// The player dies and comes back.
    Die,
    /// The last phrase answered right is no longer learned.
    Forgotten,
}

/// What the server half is told to do.
enum Case {
    /// Answers in this config, with or without an integrated server, in a Hardcore world or not;
    /// then the hearts lost in this world.
    Answers {
        config: fn(&mut Config),
        integrated: bool,
        hardcore: bool,
        steps: &'static [Step],
        hearts: i32,
    },
    /// The mob pool from these entity types.
    Pool {
        hostile: bool,
        expected: &'static [&'static str],
    },
    /// Five summons with nowhere free.
    Summons,
    /// After the lost-hearts modifier: (max health, health).
    Hearts(f64, f32, HeartsAfter),
}

#[test]
fn answers_bring_their_consequences() {
    use Step::{Answer, Die, Forgotten};
    const TYPES: [(&str, &str); 7] = [
        ("minecraft:zombie", "monster"),
        ("minecraft:ender_dragon", "monster"),
        ("minecraft:pufferfish", "water_ambient"),
        ("minecraft:cow", "creature"),
        ("minecraft:villager", "misc"),
        ("minecraft:bat", "AMBIENT"),
        ("minecraft:armor_stand", "misc"),
    ];
    #[rustfmt::skip]
    let rows: [(&str, Case); 18] = [
        ("a wrong answer brings two debuffs, a right one two buffs", Case::Answers {
            config: |_| {}, integrated: true, hardcore: false,
            steps: &[Answer(false, "debuff debuff"), Answer(true, "buff buff")], hearts: 0 }),
        ("no integrated server: nothing happens and no heart is counted", Case::Answers {
            config: |c| c.quiz_penalty = "hearts".into(), integrated: false, hardcore: false,
            steps: &[Answer(false, "offline"), Answer(true, "offline")], hearts: 0 }),
        ("mobs: hostile ones for a wrong answer, friendly ones for a right one", Case::Answers {
            config: |c| c.quiz_penalty = "mobs".into(), integrated: true, hardcore: false,
            steps: &[Answer(false, "hostile mobs"), Answer(true, "friendly mobs")], hearts: 0 }),
        ("no penalty, but a right answer is still rewarded", Case::Answers {
            config: |c| c.quiz_penalty = "none".into(), integrated: true, hardcore: false,
            steps: &[Answer(false, "nothing"), Answer(true, "buff buff")], hearts: 0 }),
        ("no rewards", Case::Answers {
            config: |c| c.quiz_reward = false, integrated: true, hardcore: false,
            steps: &[Answer(true, "nothing")], hearts: 0 }),
        ("hearts: one more lost for each wrong answer", Case::Answers {
            config: |c| c.quiz_penalty = "hearts".into(), integrated: true, hardcore: false,
            steps: &[Answer(false, "hearts 1 -2"), Answer(false, "hearts 2 -4"), Answer(true, "buff buff")],
            hearts: 2 }),
        ("a death gives the hearts back", Case::Answers {
            config: |c| c.quiz_penalty = "hearts".into(), integrated: true, hardcore: false,
            steps: &[Answer(false, "hearts 1 -2"), Answer(false, "hearts 2 -4"), Die, Answer(false, "hearts 1 -2")],
            hearts: 1 }),
        ("until_death: a death forgets this world's right answers", Case::Answers {
            config: |c| c.learned_mode = "until_death".into(), integrated: true, hardcore: false,
            steps: &[Answer(true, "buff buff"), Die, Forgotten], hearts: 0 }),
        ("hardcore: a wrong answer kills", Case::Answers {
            config: |c| c.quiz_penalty = "hardcore".into(), integrated: true, hardcore: false,
            steps: &[Answer(false, "kill")], hearts: 0 }),
        ("a Hardcore world always kills, and still rewards", Case::Answers {
            config: |c| c.quiz_penalty = "hearts".into(), integrated: true, hardcore: true,
            steps: &[Answer(false, "kill"), Answer(true, "buff buff")], hearts: 0 }),
        ("the angry teacher drops everything after five wrong in a row", Case::Answers {
            config: |c| c.quiz_penalty = "none".into(), integrated: true, hardcore: false,
            steps: &[Answer(false, "nothing"), Answer(false, "nothing"), Answer(false, "nothing"),
                Answer(false, "nothing"), Answer(false, "drop angry"), Answer(false, "nothing")], hearts: 0 }),
        ("a right answer starts the count again; delete wipes the inventory", Case::Answers {
            config: |c| {
                c.quiz_penalty = "none".into();
                c.quiz_reward = false;
                c.angry_after_wrong = 2;
                c.angry_teacher = "delete".into();
            }, integrated: true, hardcore: false,
            steps: &[Answer(false, "nothing"), Answer(true, "nothing"), Answer(false, "nothing"),
                Answer(false, "delete angry")], hearts: 0 }),
        ("an angry teacher that is off never gets angry", Case::Answers {
            config: |c| {
                c.quiz_penalty = "none".into();
                c.angry_after_wrong = 1;
                c.angry_teacher = "off".into();
            }, integrated: true, hardcore: false,
            steps: &[Answer(false, "nothing"), Answer(false, "nothing")], hearts: 0 }),
        ("with no integrated server he still gets angry, with nothing to wipe", Case::Answers {
            config: |c| c.angry_after_wrong = 1, integrated: false, hardcore: false,
            steps: &[Answer(false, "offline angry")], hearts: 0 }),
        ("hostile mobs: monsters but the dragon, the pufferfish and a charged creeper",
            Case::Pool { hostile: true, expected: &["minecraft:zombie", "minecraft:pufferfish", "charged_creeper"] }),
        ("friendly mobs: creatures, ambient and water mobs, villagers and golems",
            Case::Pool { hostile: false, expected: &["minecraft:cow", "minecraft:villager", "minecraft:bat"] }),
        ("with nowhere free they come at the player", Case::Summons),
        ("max health of half a heart or less kills", Case::Hearts(0.5, 20.0, HeartsAfter::Kill)),
    ];
    for (name, case) in rows {
        match case {
            Case::Answers {
                config: change,
                integrated,
                hardcore,
                steps,
                hearts,
            } => {
                let mut config = Config {
                    quiz_interval_seconds: 1.0,
                    ..Config::default()
                };
                change(&mut config);
                let mut teacher = teacher_with(config, HEARD);
                let mut now = START;
                let mut last_right = None;
                for (index, &step) in steps.iter().enumerate() {
                    let tick = |now: i64| WorldTick {
                        integrated_server: integrated,
                        hardcore,
                        ..calm(now)
                    };
                    match step {
                        Answer(right, expected) => {
                            let opened = loop {
                                now += 100;
                                assert!(
                                    now < START + 600_000,
                                    "{name}: a question for step {index}"
                                );
                                if teacher.world_tick(&tick(now)) == Some(QuizEvent::Ask) {
                                    break now;
                                }
                            };
                            let open = teacher.question().expect("a question");
                            let id = open.question.id.clone();
                            let pick = open
                                .options
                                .iter()
                                .position(|p| (p.id == id) == right)
                                .expect("an option");
                            now = opened + 1000;
                            let outcome = teacher.choose(pick, now).expect("an answer");
                            assert_eq!(consequence(&outcome), expected, "{name}: step {index}");
                            if let Some(ServerAction::Effect(first)) =
                                outcome.server.as_ref().and_then(|a| a.first())
                            {
                                let ids: Vec<&str> = outcome
                                    .server
                                    .iter()
                                    .flatten()
                                    .filter_map(|a| match a {
                                        ServerAction::Effect(effect) => Some(effect.id),
                                        _ => None,
                                    })
                                    .collect();
                                assert!(
                                    ids[0] == first.id && ids[0] != ids[1],
                                    "{name}: step {index}: two distinct effects"
                                );
                            }
                            if outcome.angry.is_some() {
                                let queued = teacher.take_queued();
                                assert_eq!(
                                    queued,
                                    [id.as_str(), "angry"],
                                    "{name}: step {index}: the angry line after the sentence"
                                );
                                assert_eq!(
                                    teacher.portrait_frame(now),
                                    5,
                                    "{name}: step {index}: the angry face"
                                );
                                assert_eq!(
                                    teacher.portrait_frame(now + 8000),
                                    0,
                                    "{name}: step {index}: for 8 s"
                                );
                            }
                            if right {
                                last_right = Some(id);
                            }
                        }
                        Die => {
                            for (at, dead) in [(100, true), (200, false)] {
                                let input = WorldTick {
                                    dead,
                                    health: if dead { 0.0 } else { 20.0 },
                                    ..tick(now + at)
                                };
                                teacher.world_tick(&input);
                            }
                            now += 200;
                        }
                        Forgotten => {
                            let id = last_right.clone().expect("a right answer before");
                            let config = teacher.config.clone();
                            assert!(
                                !teacher.progress.is_learned(&id, &config),
                                "{name}: {id} forgotten"
                            );
                            assert_eq!(
                                teacher.progress.correct_total.get(&id),
                                Some(&1),
                                "{name}: still counted across worlds"
                            );
                        }
                    }
                }
                assert_eq!(
                    teacher.progress.hearts_lost(),
                    hearts,
                    "{name}: hearts lost"
                );
            }
            Case::Pool { hostile, expected } => {
                assert_eq!(mob_pool(TYPES, hostile), expected, "{name}");
            }
            Case::Summons => {
                let pool = mob_pool(TYPES, true);
                let player = [0.3, 65.0, -0.2];
                let mobs = summons(&pool, 11, [0, 65, 0], player, |_| false);
                assert_eq!(mobs.len(), 5, "{name}: five mobs");
                for mob in &mobs {
                    assert_eq!(
                        mob.charged,
                        mob.entity == "minecraft:creeper",
                        "{name}: a creeper is charged"
                    );
                    assert!(
                        mob.entity == "minecraft:creeper" || pool.contains(&mob.entity),
                        "{name}: {} from the pool",
                        mob.entity
                    );
                    assert_eq!(mob.at, player, "{name}: at the player");
                }
            }
            Case::Hearts(max, health, after) => {
                assert_eq!(after_lost_hearts(max, health), after, "{name}");
            }
        }
    }
}

/// The player's state for the action table, built into each tick's facts.
struct Player {
    attack: bool,
    use_key: bool,
    held: String,
    inventory: Vec<(String, i32)>,
    mining: Option<String>,
    eating: Option<String>,
    health: f32,
    dead: bool,
    food: i32,
    sleeping: bool,
    vehicle: Option<String>,
    in_water: bool,
    on_ground: bool,
    fall_flying: bool,
    climbing: bool,
    sprinting: bool,
    sneaking: bool,
    on_fire: bool,
    fishing: bool,
    move_keys: bool,
    jump_key: bool,
    drop_key: bool,
    velocity: [f64; 3],
    dark: bool,
    raining: bool,
    thundering: bool,
    snowy: bool,
    temperature: f32,
}

impl Player {
    fn new() -> Player {
        Player {
            attack: false,
            use_key: false,
            held: "minecraft:air".into(),
            inventory: Vec::new(),
            mining: None,
            eating: None,
            health: 20.0,
            dead: false,
            food: 20,
            sleeping: false,
            vehicle: None,
            in_water: false,
            on_ground: true,
            fall_flying: false,
            climbing: false,
            sprinting: false,
            sneaking: false,
            on_fire: false,
            fishing: false,
            move_keys: false,
            jump_key: false,
            drop_key: false,
            velocity: [0.0; 3],
            dark: false,
            raining: false,
            thundering: false,
            snowy: false,
            temperature: 0.8,
        }
    }

    /// Changes the state by a step's verb. False for a verb that is not a state change.
    fn apply(&mut self, verb: &str, thing: Option<String>, number: Option<f32>) -> bool {
        let thing = || thing.clone().expect("the step names a thing");
        match verb {
            "tick" => {}
            "attack" => self.attack = true,
            "release" => self.attack = false,
            "use" => self.use_key = true,
            "hold" => self.held = thing(),
            "get" => self
                .inventory
                .push((thing(), number.expect("a count") as i32)),
            "throw" => {
                self.drop_key = true;
                let item = thing();
                if let Some(slot) = self.inventory.iter_mut().find(|(name, _)| *name == item) {
                    slot.1 -= 1;
                }
            }
            "undrop" => self.drop_key = false,
            "drop_key" => self.drop_key = true,
            "lose" => {
                let item = thing();
                if let Some(slot) = self.inventory.iter_mut().find(|(name, _)| *name == item) {
                    slot.1 -= 1;
                }
            }
            "eat" => {
                self.use_key = true;
                self.eating = Some(thing());
            }
            "full" => {
                self.use_key = false;
                self.eating = None;
            }
            "sleep" => self.sleeping = true,
            "wake" => self.sleeping = false,
            "ride" => self.vehicle = Some(thing()),
            "dismount" => self.vehicle = None,
            "fish" => self.fishing = true,
            "elytra" => {
                self.fall_flying = true;
                self.on_ground = false;
                self.velocity = [0.5, 0.0, 0.0];
            }
            "walk" | "sprint" | "sneak" | "swim" | "climb" | "rest" => {
                self.sprinting = verb == "sprint";
                self.sneaking = verb == "sneak";
                self.in_water = verb == "swim";
                self.climbing = verb == "climb";
                self.fall_flying = false;
                self.on_ground = !matches!(verb, "swim" | "climb");
                self.move_keys = verb != "rest";
                self.velocity = if verb == "rest" {
                    [0.0; 3]
                } else {
                    [0.2, 0.0, 0.0]
                };
            }
            "jump" => {
                self.jump_key = true;
                self.on_ground = false;
            }
            "drop_fast" => {
                self.on_ground = false;
                self.velocity = [0.0, -1.0, 0.0];
            }
            "land" => {
                self.jump_key = false;
                self.on_ground = true;
                self.velocity = [0.0; 3];
                if let Some(health) = number {
                    self.health = health;
                }
            }
            "health" => self.health = number.expect("health"),
            "die" => {
                self.dead = true;
                self.health = 0.0;
            }
            "food" => self.food = number.expect("food") as i32,
            "fire" => self.on_fire = true,
            "nofire" => self.on_fire = false,
            "dark" => self.dark = true,
            "light" => self.dark = false,
            "rain" | "thunder" => {
                self.raining = true;
                self.thundering = verb == "thunder";
            }
            "clear" => {
                self.raining = false;
                self.thundering = false;
            }
            "snowy" => self.snowy = true,
            "hot" => {
                self.snowy = false;
                self.temperature = 2.0;
            }
            "mild" => {
                self.snowy = false;
                self.temperature = 0.8;
            }
            _ => return false,
        }
        true
    }
}

/// Each step is `<ms> <verb> [<thing> [<number> | open | food]]`; things are `minecraft:` ids.
/// Events: `hit`/`hitdying <entity> <id>`, `feed <entity> <item> [food]`, `click <block> [open]`
/// (`places` when it has no use phrase), `dim <dimension>`; a tick with `arrow <entity> <id>`
/// (hit by the player's arrow), `dies <id>` or `gone <id>`. Any other verb changes the player and
/// ticks. The expected text lists what the speaker queued, comma separated.
#[test]
fn every_action_is_said_under_its_rules() {
    // (name, config change, steps as (step, said))
    type Row = (
        &'static str,
        fn(&mut Config),
        &'static [(&'static str, &'static str)],
    );
    #[rustfmt::skip]
    let rows: [Row; 26] = [
        ("a hit says the entity's phrase, else attack; a dying one nothing", |_| {}, &[
            ("0 tick", ""), ("100 hit cow 1", "hit.muh"), ("200 hit wolf 2", "attack"),
            ("300 hitdying cow 3", ""),
        ]),
        ("a kill within 6 s of the hit, else none", |_| {}, &[
            ("0 tick", ""), ("100 hit cow 1", "hit.muh"), ("1000 dies 1", "kill.muh"),
            ("1100 hit wolf 2", "attack"), ("7150 tick", ""), ("7200 dies 2", ""),
        ]),
        ("an arrow's hit counts for the kill, kill_it without a phrase", |_| {}, &[
            ("0 tick", ""), ("100 arrow wolf 5", ""), ("200 dies 5", "kill_it"),
        ]),
        ("a watch ends when the entity is gone", |_| {}, &[
            ("0 tick", ""), ("100 hit cow 1", "hit.muh"), ("200 gone 1", ""), ("300 dies 1", ""),
        ]),
        ("kills off: arrows go unwatched and no kill is said", |c| c.say_kills = false, &[
            ("0 tick", ""), ("100 arrow wolf 5", ""), ("200 hit cow 1", "hit.muh"), ("300 dies 1", ""),
            ("400 dies 5", ""),
        ]),
        ("combat off silences hits and kills", |c| c.say_combat = false, &[
            ("0 tick", ""), ("100 hit cow 1", ""), ("200 dies 1", ""),
        ]),
        ("feeding takes food in the hand", |_| {}, &[
            ("0 tick", ""), ("100 feed cow stone", ""), ("200 feed cow wheat", "feed.muh"),
            ("10300 tick", ""), ("10400 feed cow cookie food", "feed.muh"),
        ]),
        ("a right click says the block's use phrase, close when it is open", |_| {}, &[
            ("0 tick", ""), ("100 click zing", "use.zing"), ("200 click oak_door", "open.tuer"),
            ("300 click oak_door open", "close.tuer"), ("400 click bloop", "places"),
        ]),
        ("blocks off silences a click, which still places nothing", |c| c.say_blocks = false, &[
            ("0 tick", ""), ("100 click zing", ""),
        ]),
        ("sleeping, riding (a movement) and fishing are states", |c| c.say_movement = true, &[
            ("0 sleep", "sleep"), ("100 tick", ""), ("200 wake", ""), ("300 ride horse", "ride.gaul"),
            ("400 dismount", ""), ("500 fish", "fish"),
        ]),
        ("eating, drinking before eating", |_| {}, &[
            ("0 eat cookie", "eat.knusper"), ("100 full", ""), ("200 eat potion", "drink.trank"),
        ]),
        ("movement waits 1.5 s, and a new one replaces it", |c| c.say_movement = true, &[
            ("0 walk", ""), ("1400 walk", ""), ("1500 walk", "walk"), ("1600 sprint", ""),
            ("3100 sprint", "sprint"), ("3200 sneak", ""), ("4700 sneak", "sneak"),
        ]),
        ("flying, swimming and climbing", |c| c.say_movement = true, &[
            ("0 elytra", ""), ("1500 elytra", "fly"), ("1600 swim", ""), ("3100 swim", "swim"),
            ("3200 climb", ""), ("4700 climb", "climb"),
        ]),
        ("movement is off by default", |_| {}, &[
            ("0 walk", ""), ("1500 walk", ""), ("1600 jump", ""),
        ]),
        ("a jump leaves the ground, at most every 4 s", |c| c.say_movement = true, &[
            ("0 tick", ""), ("100 jump", "jump"), ("200 land", ""), ("300 jump", ""), ("4200 land", ""),
            ("4300 jump", "jump"),
        ]),
        ("damage: hurt, a pause before the next, a fall heard up to 1 s after it but not after water, death",
            |c| c.same_again_seconds = 0.0, &[
            ("0 tick", ""), ("100 health 18", "hurt"), ("1000 health 16", ""), ("3100 health 14", "hurt"),
            ("3200 drop_fast", ""), ("3250 land", ""), ("3400 health 12", "fall"),
            ("5500 drop_fast", ""), ("5550 land", ""), ("6600 health 10", "hurt"),
            ("8700 drop_fast", ""), ("8750 swim", ""), ("8800 health 9", "hurt"), ("8900 die", "die"),
        ]),
        ("damage off", |c| c.say_damage = false, &[
            ("0 tick", ""), ("100 health 18", ""), ("200 die", ""),
        ]),
        ("burning, again only after leaving the fire", |_| {}, &[
            ("0 fire", "burning"), ("100 fire", ""), ("200 nofire", ""), ("10300 fire", "burning"),
        ]),
        ("hunger once, again after eating up to 10", |_| {}, &[
            ("0 tick", ""), ("100 food 6", "hungry"), ("200 food 5", ""), ("300 food 9", ""),
            ("400 food 10", ""), ("1500 food 6", "hungry"),
        ]),
        ("the world topic off silences hunger", |c| c.say_world = false, &[
            ("0 tick", ""), ("100 food 6", ""),
        ]),
        ("night and day, from 3 s after joining", |_| {}, &[
            ("0 tick", ""), ("100 dark", ""), ("3100 dark", ""), ("3200 light", "day"), ("3300 dark", "night"),
        ]),
        ("rain, thunder, snow and their ends", |_| {}, &[
            ("3100 tick", ""), ("3200 rain", "rain"), ("3300 thunder", "thunder"), ("3400 rain", "weather_rain"),
            ("3500 clear", "rain_stop"), ("4200 snowy", ""), ("4300 rain", "snow"), ("4400 clear", "snow_stop"),
        ]),
        ("the climate after 3 s in it, and its repeat", |c| c.climate_repeat_seconds = 10.0, &[
            ("3100 tick", ""), ("3200 hot", ""), ("6100 hot", ""), ("6200 hot", "hot"), ("6300 mild", ""),
            ("9300 mild", "not_hot"), ("9400 snowy", ""), ("12400 snowy", "cold"), ("22400 snowy", "cold"),
        ]),
        ("the weather report", |c| c.weather_report_minutes = 1.0, &[
            ("3100 tick", ""), ("59999 tick", ""), ("60000 tick", "weather_clear"), ("60100 rain", "rain"),
            ("120100 tick", "weather_rain"),
        ]),
        ("a new dimension is said at once", |_| {}, &[
            ("0 tick", ""), ("100 dim the_nether", "dim_nether"), ("200 dim the_nether", ""),
            ("300 dim overworld", "dim_overworld"),
        ]),
        ("a pickup, the held item, then drops: the stack one fewer on the drop key's tick or up to three after",
            |_| {}, &[
            ("0 get bloop 3", ""), ("100 get quux 1", "pickup.quux"), ("200 hold bloop", "hold.bloop"),
            ("300 throw bloop", "drop.bloop"), ("400 undrop", ""),
            ("1000 drop_key", ""), ("1050 lose bloop", "drop.bloop"), ("1100 undrop", ""),
            ("2000 drop_key", ""), ("2050 tick", ""), ("2100 tick", ""), ("2150 tick", ""),
            ("2200 lose bloop", ""), ("2250 undrop", ""),
        ]),
    ];
    for (name, change, steps) in rows {
        let mut config = Config::default();
        change(&mut config);
        let mut teacher = teacher_with(config, &[]);
        let mut player = Player::new();
        for &(step, expected) in steps {
            let words: Vec<&str> = step.split_whitespace().collect();
            let now = START + words[0].parse::<i64>().expect("a time");
            let id = |word: Option<&&str>| word.map(|name| format!("minecraft:{name}"));
            let thing = id(words.get(2));
            let handle = || {
                words
                    .get(3)
                    .and_then(|n| n.parse::<u64>().ok())
                    .expect("an id")
            };
            let mut said = Vec::new();
            let mut arrows: Vec<(u64, String)> = Vec::new();
            let mut fates: Vec<(u64, Fate)> = Vec::new();
            let ticks = match words[1] {
                "hit" | "hitdying" => {
                    let entity = thing.clone().expect("an entity");
                    teacher.entity_attacked(now, handle(), &entity, true, words[1] == "hitdying");
                    false
                }
                "feed" => {
                    let item = id(words.get(3)).expect("an item");
                    let food = words.get(4) == Some(&"food");
                    teacher.entity_used(now, &thing.clone().expect("an entity"), &item, food);
                    false
                }
                "click" => {
                    let open = words.get(3) == Some(&"open");
                    if !teacher.block_used(now, &thing.clone().expect("a block"), open) {
                        said.push("places".to_string());
                    }
                    false
                }
                "dim" => {
                    teacher.joined_level(
                        now,
                        WORLD,
                        &thing.clone().expect("a dimension"),
                        &player.held,
                    );
                    false
                }
                "arrow" => {
                    arrows.push((handle(), thing.clone().expect("an entity")));
                    true
                }
                "dies" | "gone" => {
                    let who = words[2].parse::<u64>().expect("an id");
                    fates.push((
                        who,
                        if words[1] == "dies" {
                            Fate::Dying
                        } else {
                            Fate::Removed
                        },
                    ));
                    true
                }
                verb => {
                    let number = words.get(3).and_then(|n| n.parse::<f32>().ok());
                    let number =
                        number.or_else(|| words.get(2).and_then(|n| n.parse::<f32>().ok()));
                    assert!(
                        player.apply(verb, thing.clone(), number),
                        "{name}: unknown verb {verb}"
                    );
                    true
                }
            };
            if ticks {
                let slots: Vec<(&str, i32)> = player
                    .inventory
                    .iter()
                    .map(|(item, n)| (item.as_str(), *n))
                    .collect();
                let hits: Vec<(u64, &str)> =
                    arrows.iter().map(|(id, e)| (*id, e.as_str())).collect();
                let input = WorldTick {
                    now,
                    attack_down: player.attack,
                    use_down: player.use_key,
                    using_item: player.eating.is_some(),
                    breaking: player.mining.as_deref(),
                    held_item: &player.held,
                    held_count: 1,
                    inventory: Some(&slots),
                    dead: player.dead,
                    health: player.health,
                    food: player.food,
                    sleeping: player.sleeping,
                    vehicle: player.vehicle.as_deref(),
                    in_water: player.in_water,
                    on_ground: player.on_ground,
                    fall_flying: player.fall_flying,
                    on_climbable: player.climbing,
                    sprinting: player.sprinting,
                    sneaking: player.sneaking,
                    on_fire: player.on_fire,
                    fishing: player.fishing,
                    eating: player.eating.as_deref(),
                    move_keys: player.move_keys,
                    jump_key: player.jump_key,
                    drop_key: player.drop_key,
                    velocity: player.velocity,
                    dark: player.dark,
                    raining: player.raining,
                    thundering: player.thundering,
                    biome_rains: true,
                    biome_snows: player.snowy,
                    biome_temperature: player.temperature,
                    projectile_hits: &hits,
                    entity_fates: &fates,
                    ..WorldTick::default()
                };
                teacher.world_tick(&input);
            }
            said.extend(teacher.take_queued());
            assert_eq!(said.join(", "), expected, "{name}: '{step}'");
        }
    }
}

/// What a counted sentence comes out as: `<text> | <translation> | <clips>`, the clips joined
/// with `+` for one joined clip and `,` for two in a row; a plain phrase by its id; the held
/// label as `label <text>`.
fn counted(speech: &Speech) -> String {
    match speech {
        Speech::Counted {
            display,
            clips,
            joined,
            ..
        } => {
            let ids: Vec<&str> = clips.iter().map(|clip| clip.id.as_str()).collect();
            let glue = if *joined { "+" } else { "," };
            format!("{} | {} | {}", display.text, display.en, ids.join(glue))
        }
        Speech::HeldLabel(p) => format!("label {}", p.text),
        other => describe(other),
    }
}

#[test]
fn counted_sentences_say_the_amount() {
    let pack = fixture_pack("en");
    // (name, action, item, amount, said)
    #[rustfmt::skip]
    let rows: [(&str, &str, &str, i32, &str); 7] = [
        ("a pickup joins the count and the plural, in English by countEn", "pickup", "bloop", 3,
            "Ich grapsche drei Bloopen. | I picked up 3 bloop blocks. | count.pickup.3+plural.bloop"),
        ("the held count with the gender and the plural for holding", "hold", "bloop", 3,
            "label drei Bloopen da, drei Bloopen da | 3 bloop blocks | count.hold.3.m+plural.bloop.hold"),
        ("no count start: the whole sentence, then the amount", "drop", "bloop", 3,
            "Ich werfe den Bloop, drei Stück. | I dropped 3 bloops. | drop.bloop,count.3.m"),
        ("no plural: the same, with the amount for no gender", "pickup", "quux", 3,
            "Ich nehme das Quux, drei Teile. | I picked up 3 quuxes. | pickup.quux,count.3.ge"),
        ("no amount phrase either: the plain sentence", "pickup", "quux", 5, "new pickup.quux"),
        ("one thing: the plain sentence", "pickup", "bloop", 1, "new pickup.bloop"),
        ("one held thing: its noun", "hold", "bloop", 1, "label der Bloop, repeat hold.bloop"),
    ];
    for (name, action, item, amount, expected) in rows {
        let config = Config::default();
        let mut detector = Detector::default();
        detector.reset("minecraft:air", 0);
        let item = format!("minecraft:{item}");
        let item = item.as_str();
        let tick = |detector: &mut Detector,
                    at: i64,
                    held: &str,
                    slots: &[(&str, i32)],
                    drop_key: bool| {
            let input = WorldTick {
                now: at,
                held_item: held,
                held_count: if held == item { amount } else { 1 },
                inventory: Some(slots),
                drop_key,
                food: 20,
                ..WorldTick::default()
            };
            detector.tick(&input, &pack, &config)
        };
        let said = match action {
            "pickup" => {
                tick(&mut detector, 0, "minecraft:air", &[], false);
                tick(
                    &mut detector,
                    100,
                    "minecraft:air",
                    &[(item, amount)],
                    false,
                )
            }
            "hold" => tick(&mut detector, 100, item, &[], false),
            _ => {
                tick(&mut detector, 0, item, &[(item, amount)], false);
                tick(&mut detector, 100, "minecraft:air", &[], true)
            }
        };
        let got: Vec<String> = said.iter().map(counted).collect();
        assert_eq!(got.join(", "), expected, "{name}");
    }
    #[rustfmt::skip]
    let plurals: [(&str, &str, i32, &str); 7] = [
        ("an s", "bloop", 3, "bloops"),
        ("y after a consonant", "berry", 2, "berries"),
        ("y after e", "key", 2, "keys"),
        ("es after ch, sh and x", "box", 2, "boxes"),
        ("a word with no plural", "glass", 2, "glass"),
        ("a word already in the plural", "boots", 2, "boots"),
        ("one is not plural", "berry", 1, "berry"),
    ];
    for (name, en, amount, expected) in plurals {
        assert_eq!(
            plural(en, amount),
            expected,
            "the translation's plural: {name}"
        );
    }
}

#[test]
fn settings_rows_go_round_or_stop_at_their_ends() {
    let pack = fixture_pack("en");
    let text = |t: &str| Shown::Text(t.to_string());
    let choice = |t: &str| Shown::Choice(t.to_string());
    // (name, page, row, config change, clicks, shown before, shown after)
    type Row = (
        &'static str,
        Page,
        &'static str,
        fn(&mut Config),
        &'static [i32],
        Shown,
        Shown,
    );
    #[rustfmt::skip]
    let rows: [Row; 18] = [
        ("a switch flips", Page::Main, "enabled", |_| {}, &[1], Shown::Flag(true), Shown::Flag(false)),
        ("sound shows the voice on, and a click mutes it", Page::Main, "sound", |_| {}, &[1],
            Shown::Flag(true), Shown::Flag(false)),
        ("a choice goes round", Page::Main, "quiz_mode", |_| {}, &[1, 1, 1], choice("choice"), choice("off")),
        ("a value the row does not list shows the first, and steps on from it", Page::Advanced, "position",
            |c| c.hud_anchor = "middle".into(), &[1], choice("bottom"), choice("top")),
        ("speech asks with buttons: it shows as the button mode and steps on from it", Page::Main,
            "quiz_mode", |c| c.quiz_mode = "speech".into(), &[1], choice("choice"), choice("typing")),
        ("a number stops at its top", Page::Main, "quiz_interval", |_| {}, &[20], text("60 s"), text("300 s")),
        ("and at its bottom", Page::Main, "quiz_answer", |_| {}, &[-20], text("12 s"), text("5 s")),
        ("a value between steps shows the nearest", Page::Main, "text_scale", |c| c.hud_scale = 2.1, &[1],
            text("2\u{D7}"), text("2.25\u{D7}")),
        ("repeat: never, then its steps", Page::Main, "repeat", |_| {}, &[2], Shown::Key("never"), text("5 s")),
        ("of two steps as near, the first", Page::Main, "repeat", |c| c.repeat_seconds = 4.0, &[],
            text("3 s"), text("3 s")),
        ("the same phrase again: at once, seconds, then minutes, and no further", Page::Main, "same_again",
            |c| c.same_again_seconds = 0.0, &[3, 5], Shown::Key("at_once"), text("5 min")),
        ("the volume in percent", Page::Main, "volume", |_| {}, &[-1], text("100%"), text("99%")),
        ("the translation: auto, then the pack's languages by their own names", Page::Main, "hint_language",
            |_| {}, &[1], Shown::Key("auto"), text("Espa\u{F1}ol")),
        ("the speeds are the pack's", Page::Main, "speed", |_| {}, &[1], choice("normal"), choice("slow")),
        ("the penalty", Page::Main, "quiz_penalty", |_| {}, &[-2], choice("debuff"), choice("hardcore")),
        ("angry after at least one", Page::Advanced, "angry_after", |c| c.angry_after_wrong = 0, &[1],
            text("1"), text("2")),
        ("the rest mode shows its minutes", Page::Advanced, "learned_mode",
            |c| c.learned_mode = "rest".into(), &[1], Shown::Rest(60), choice("forever")),
        ("a topic", Page::Topics, "say_kills", |_| {}, &[-1], Shown::Flag(true), Shown::Flag(false)),
    ];
    for (name, page, row_name, change, clicks, before, after) in rows {
        let rows = settings(page, &pack);
        let row = rows
            .iter()
            .find(|row| row.name == row_name)
            .unwrap_or_else(|| panic!("{name}: no row {row_name}"));
        let mut config = Config::default();
        change(&mut config);
        assert_eq!(row.shown(&config), before, "{name}: before");
        for &delta in clicks {
            row.cycle(&mut config, delta);
        }
        assert_eq!(row.shown(&config), after, "{name}: after {clicks:?}");
    }
    let mut config = Config::default();
    let sound = settings(Page::Main, &pack)
        .into_iter()
        .find(|row| row.name == "sound")
        .expect("sound");
    sound.cycle(&mut config, 1);
    assert!(config.muted, "the sound row writes muted");
    #[rustfmt::skip]
    let pages: [(Page, &[&str]); 3] = [
        (Page::Main, &["enabled", "hint_language", "speed", "volume", "sound", "repeat", "same_again",
            "text_scale", "english", "quiz_mode", "quiz_penalty", "quiz_interval", "quiz_answer"]),
        (Page::Advanced, &["portrait", "portrait_scale", "position", "learned_mode", "quiz_reward",
            "quiz_pause", "angry_teacher", "angry_after", "backdrop"]),
        (Page::Topics, &["say_movement", "say_combat", "say_blocks", "say_items", "say_world", "say_held",
            "say_kills", "say_damage"]),
    ];
    for (page, names) in pages {
        let got: Vec<&str> = settings(page, &pack).iter().map(|row| row.name).collect();
        assert_eq!(got, names, "{page:?}: the rows for a German pack");
    }
}
