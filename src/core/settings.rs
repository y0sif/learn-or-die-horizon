//! The settings pages (Java `NarratorSettingsScreen`, `NarratorAdvancedScreen` and
//! `NarratorTopicsScreen`): which rows each page has for a German pack, what each row shows, and
//! how a click moves its value. Choices and switches go round; numbers stop at their ends.
//!
//! Left out: rows for packs with readings or several looks, voice answers and the exam page, and
//! rows of features the port does not have (the score counter, the question timer line, the
//! learned jingle, tooltips, and the font, which is the same either way for German). The original
//! offers the `speech` quiz mode; here a `speech` config asks with buttons and the row skips it.
use super::config::Config;
use super::pack::Pack;

/// A settings page. The main page links to the advanced one, which links to the topics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Advanced,
    Topics,
}

/// What a row shows for its value.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// On or off.
    Flag(bool),
    /// A choice, labelled by the lang key `<prefix>.value.<choice>`, else by the choice itself.
    Choice(String),
    /// A number with its unit, or a language's own name.
    Text(String),
    /// A word with no value: `never` and `at_once` (lang key `<prefix>.value.<name>`), and the
    /// translation row's `auto` (`<prefix>.options.hint_language.auto`).
    Key(&'static str),
    /// The learned mode `rest`, labelled with its minutes (`<prefix>.value.rest`).
    Rest(i32),
}

#[derive(Clone, Copy)]
enum Steps {
    /// Every `step` from `min` to `max`, shown with `unit`.
    Even {
        min: f32,
        max: f32,
        step: f32,
        unit: &'static str,
    },
    /// The given values; `0` shows `zero`.
    Listed {
        values: &'static [f32],
        zero: &'static str,
        minutes: bool,
    },
    /// 0 to 100%.
    Percent,
}

#[derive(Clone)]
enum Kind {
    Switch {
        get: fn(&Config) -> bool,
        set: fn(&mut Config, bool),
    },
    Choice {
        values: Vec<String>,
        get: fn(&Config) -> &str,
        set: fn(&mut Config, &str),
    },
    Number {
        steps: Steps,
        get: fn(&Config) -> f32,
        set: fn(&mut Config, f32),
    },
}

/// One row of a settings page.
#[derive(Clone)]
pub struct Setting {
    /// The original's option name: its label is `<prefix>.options.<name>`. `hint_language`
    /// changes the translations, which needs the pack loaded again.
    pub name: &'static str,
    kind: Kind,
}

fn switch(name: &'static str, get: fn(&Config) -> bool, set: fn(&mut Config, bool)) -> Setting {
    Setting {
        name,
        kind: Kind::Switch { get, set },
    }
}

fn choice(
    name: &'static str,
    values: &[&str],
    get: fn(&Config) -> &str,
    set: fn(&mut Config, &str),
) -> Setting {
    Setting {
        name,
        kind: Kind::Choice {
            values: values.iter().map(|v| v.to_string()).collect(),
            get,
            set,
        },
    }
}

fn number(
    name: &'static str,
    steps: Steps,
    get: fn(&Config) -> f32,
    set: fn(&mut Config, f32),
) -> Setting {
    Setting {
        name,
        kind: Kind::Number { steps, get, set },
    }
}

fn even(min: f32, max: f32, step: f32, unit: &'static str) -> Steps {
    Steps::Even {
        min,
        max,
        step,
        unit,
    }
}

/// The rows of a page, in the original's order.
pub fn settings(page: Page, pack: &Pack) -> Vec<Setting> {
    match page {
        Page::Main => main_page(pack),
        Page::Advanced => vec![
            choice(
                "portrait",
                &[
                    "bottom_right",
                    "bottom_left",
                    "top_right",
                    "top_left",
                    "off",
                ],
                |c| &c.hud_portrait,
                |c, v| c.hud_portrait = v.to_string(),
            ),
            number(
                "portrait_scale",
                even(0.5, 4.0, 0.5, "\u{D7}"),
                |c| c.hud_portrait_scale,
                |c, v| c.hud_portrait_scale = v,
            ),
            choice(
                "position",
                &["bottom", "top"],
                |c| &c.hud_anchor,
                |c, v| c.hud_anchor = v.to_string(),
            ),
            choice(
                "learned_mode",
                &["forever", "until_death", "rest"],
                |c| &c.learned_mode,
                |c, v| c.learned_mode = v.to_string(),
            ),
            switch("quiz_reward", |c| c.quiz_reward, |c, v| c.quiz_reward = v),
            switch(
                "quiz_pause",
                |c| c.quiz_pause_game,
                |c, v| c.quiz_pause_game = v,
            ),
            choice(
                "angry_teacher",
                &["drop", "delete", "off"],
                |c| &c.angry_teacher,
                |c, v| c.angry_teacher = v.to_string(),
            ),
            number(
                "angry_after",
                even(1.0, 10.0, 1.0, ""),
                |c| c.angry_after_wrong.max(1) as f32,
                |c, v| c.angry_after_wrong = v.round() as i32,
            ),
            switch("backdrop", |c| c.hud_backdrop, |c, v| c.hud_backdrop = v),
        ],
        Page::Topics => vec![
            switch(
                "say_movement",
                |c| c.say_movement,
                |c, v| c.say_movement = v,
            ),
            switch("say_combat", |c| c.say_combat, |c, v| c.say_combat = v),
            switch("say_blocks", |c| c.say_blocks, |c, v| c.say_blocks = v),
            switch("say_items", |c| c.say_items, |c, v| c.say_items = v),
            switch("say_world", |c| c.say_world, |c, v| c.say_world = v),
            switch("say_held", |c| c.say_held_item, |c, v| c.say_held_item = v),
            switch("say_kills", |c| c.say_kills, |c, v| c.say_kills = v),
            switch("say_damage", |c| c.say_damage, |c, v| c.say_damage = v),
        ],
    }
}

fn main_page(pack: &Pack) -> Vec<Setting> {
    let mut rows = vec![switch("enabled", |c| c.enabled, |c, v| c.enabled = v)];
    if !pack.meta.hint_languages.is_empty() {
        let mut values = vec!["auto".to_string()];
        values.extend(pack.meta.hint_languages.iter().cloned());
        rows.push(Setting {
            name: "hint_language",
            kind: Kind::Choice {
                values,
                get: |c| &c.hint_language,
                set: |c, v| c.hint_language = v.to_string(),
            },
        });
    }
    if pack.voices.len() > 1 {
        rows.push(Setting {
            name: "voice",
            kind: Kind::Choice {
                values: pack.voices.clone(),
                get: |c| &c.voice,
                set: |c, v| c.voice = v.to_string(),
            },
        });
    }
    rows.push(Setting {
        name: "speed",
        kind: Kind::Choice {
            values: pack.speeds.clone(),
            get: |c| &c.speed,
            set: |c, v| c.speed = v.to_string(),
        },
    });
    rows.extend([
        number("volume", Steps::Percent, |c| c.volume, |c, v| c.volume = v),
        switch("sound", |c| !c.muted, |c, v| c.muted = !v),
        number(
            "repeat",
            Steps::Listed {
                values: &[0.0, 3.0, 5.0, 10.0, 20.0, 30.0, 60.0],
                zero: "never",
                minutes: false,
            },
            |c| c.repeat_seconds,
            |c, v| c.repeat_seconds = v,
        ),
        number(
            "same_again",
            Steps::Listed {
                values: &[0.0, 10.0, 30.0, 60.0, 300.0],
                zero: "at_once",
                minutes: true,
            },
            |c| c.same_again_seconds,
            |c, v| c.same_again_seconds = v,
        ),
        number(
            "text_scale",
            even(1.0, 3.0, 0.25, "\u{D7}"),
            |c| c.hud_scale,
            |c, v| c.hud_scale = v,
        ),
        switch(
            "english",
            |c| c.hud_show_english,
            |c, v| c.hud_show_english = v,
        ),
        choice(
            "quiz_mode",
            &["off", "choice", "typing", "mixed"],
            |c| &c.quiz_mode,
            |c, v| c.quiz_mode = v.to_string(),
        ),
        choice(
            "quiz_penalty",
            &["none", "debuff", "mobs", "hearts", "hardcore"],
            |c| &c.quiz_penalty,
            |c, v| c.quiz_penalty = v.to_string(),
        ),
        number(
            "quiz_interval",
            even(15.0, 300.0, 15.0, " s"),
            |c| c.quiz_interval_seconds,
            |c, v| c.quiz_interval_seconds = v,
        ),
        number(
            "quiz_answer",
            even(5.0, 30.0, 1.0, " s"),
            |c| c.quiz_answer_seconds,
            |c, v| c.quiz_answer_seconds = v,
        ),
    ]);
    rows
}

/// The original's number format: whole numbers without a point, else up to two decimals.
fn format(v: f64) -> String {
    if v == v.round() {
        format!("{}", v as i64)
    } else {
        let text = format!("{v:.2}");
        text.trim_end_matches('0').to_string()
    }
}

/// A language's own name, for the translation row.
fn language_name(code: &str) -> &str {
    match code {
        "zh" => "\u{4E2D}\u{6587}",
        "es" => "Espa\u{F1}ol",
        "de" => "Deutsch",
        "pt" => "Portugu\u{EA}s",
        "ja" => "\u{65E5}\u{672C}\u{8A9E}",
        "en" => "English",
        "fr" => "Fran\u{E7}ais",
        other => other,
    }
}

impl Steps {
    fn positions(self) -> usize {
        match self {
            Steps::Even { min, max, step, .. } => ((max - min) / step).round() as usize + 1,
            Steps::Listed { values, .. } => values.len(),
            Steps::Percent => 101,
        }
    }

    fn value(self, index: usize) -> f32 {
        match self {
            Steps::Even { min, step, .. } => min + index as f32 * step,
            Steps::Listed { values, .. } => values[index],
            Steps::Percent => index as f32 / 100.0,
        }
    }

    /// The position a value shows at: the nearest step (the first of two as near).
    fn position(self, value: f32) -> usize {
        let last = self.positions() - 1;
        match self {
            Steps::Even { min, step, .. } => {
                let at = ((value - min) / step + 0.5).floor();
                (at.max(0.0) as usize).min(last)
            }
            Steps::Listed { values, .. } => {
                let mut best = 0;
                for (index, step) in values.iter().enumerate() {
                    if (step - value).abs() < (values[best] - value).abs() {
                        best = index;
                    }
                }
                best
            }
            Steps::Percent => ((value * 100.0 + 0.5).floor().max(0.0) as usize).min(last),
        }
    }

    fn shown(self, index: usize) -> Shown {
        let value = self.value(index);
        match self {
            Steps::Even { unit, .. } => Shown::Text(format!("{}{unit}", format(f64::from(value)))),
            Steps::Listed { zero, .. } if value == 0.0 => Shown::Key(zero),
            Steps::Listed { minutes, .. } if minutes && value >= 60.0 => {
                Shown::Text(format!("{} min", format(f64::from(value / 60.0))))
            }
            Steps::Listed { .. } => Shown::Text(format!("{} s", format(f64::from(value)))),
            Steps::Percent => Shown::Text(format!("{index}%")),
        }
    }
}

impl Setting {
    /// How many values the row has.
    pub fn positions(&self) -> usize {
        match &self.kind {
            Kind::Switch { .. } => 2,
            Kind::Choice { values, .. } => values.len().max(1),
            Kind::Number { steps, .. } => steps.positions(),
        }
    }

    /// The value's position: off is 0 and on 1; a choice the config does not list shows the
    /// first; a number shows at its nearest step.
    pub fn position(&self, config: &Config) -> usize {
        match &self.kind {
            Kind::Switch { get, .. } => usize::from(get(config)),
            Kind::Choice { values, get, .. } => {
                let current = get(config);
                values.iter().position(|v| v == current).unwrap_or(0)
            }
            Kind::Number { steps, get, .. } => steps.position(get(config)),
        }
    }

    /// Sets the value at `index`, kept within the row's positions.
    pub fn set_position(&self, config: &mut Config, index: usize) {
        let index = index.min(self.positions() - 1);
        match &self.kind {
            Kind::Switch { set, .. } => set(config, index == 1),
            Kind::Choice { values, set, .. } => {
                if let Some(value) = values.get(index) {
                    set(config, value);
                }
            }
            Kind::Number { steps, set, .. } => set(config, steps.value(index)),
        }
    }

    /// A click: `delta` steps on. Switches and choices go round; numbers stop at their ends.
    pub fn cycle(&self, config: &mut Config, delta: i32) {
        let count = self.positions() as i64;
        let at = self.position(config) as i64 + i64::from(delta);
        let index = match self.kind {
            Kind::Number { .. } => at.clamp(0, count - 1),
            _ => at.rem_euclid(count),
        };
        self.set_position(config, index as usize);
    }

    /// What the row shows for the config's value.
    pub fn shown(&self, config: &Config) -> Shown {
        let index = self.position(config);
        match &self.kind {
            Kind::Switch { get, .. } => Shown::Flag(get(config)),
            Kind::Choice { values, .. } => {
                let value = values.get(index).cloned().unwrap_or_default();
                match (self.name, value.as_str()) {
                    ("hint_language", "auto") => Shown::Key("auto"),
                    ("hint_language", code) => Shown::Text(language_name(code).to_string()),
                    ("learned_mode", "rest") => {
                        Shown::Rest(config.learned_rest_minutes.round() as i32)
                    }
                    _ => Shown::Choice(value),
                }
            }
            Kind::Number { steps, .. } => steps.shown(index),
        }
    }
}
