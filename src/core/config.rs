//! The teacher's settings (Java `NarratorConfig`), the subset the port reads, under the original
//! Gson field names and defaults. A key missing from the file keeps its default. Keys this port
//! does not read are kept and written back unchanged, so no setting is lost between versions.
use super::gson::{self, JsonObject};
use mod_api::serde_json::Value;

macro_rules! config_fields {
    (@ty bool) => { bool };
    (@ty int) => { i32 };
    (@ty float) => { f32 };
    (@ty string) => { String };
    (@default string, $default:expr) => { String::from($default) };
    (@default $ty:ident, $default:expr) => { $default };
    (@read bool, $value:expr) => { gson::as_bool($value) };
    (@read int, $value:expr) => { gson::as_i32($value) };
    (@read float, $value:expr) => { gson::as_f32($value) };
    (@read string, $value:expr) => { gson::as_string($value) };
    (@write bool, $value:expr) => { Value::Bool($value) };
    (@write int, $value:expr) => { Value::from($value) };
    (@write float, $value:expr) => { gson::float_value($value) };
    (@write string, $value:expr) => { Value::String($value.clone()) };
    ($($ty:ident $field:ident $name:literal = $default:expr;)*) => {
        /// Field names follow the file's camelCase keys; see the module docs.
        #[derive(Clone, Debug, PartialEq)]
        pub struct Config {
            $(pub $field: config_fields!(@ty $ty),)*
            /// Keys this port does not read, written back unchanged.
            pub(crate) extra: JsonObject,
        }

        impl Default for Config {
            fn default() -> Self {
                Self {
                    $($field: config_fields!(@default $ty, $default),)*
                    extra: JsonObject::new(),
                }
            }
        }

        impl Config {
            /// Reads the config file. A missing or `null` key keeps its default; a key of the wrong
            /// type is an error, and the caller then falls back to [`Config::default`], as Gson's
            /// failed read does.
            pub fn from_json(value: &Value) -> Result<Config, String> {
                let object = value.as_object().ok_or("the config is not a JSON object")?;
                let mut config = Config::default();
                for (name, value) in object {
                    if value.is_null() {
                        continue;
                    }
                    match name.as_str() {
                        $($name => {
                            config.$field = config_fields!(@read $ty, value)
                                .map_err(|e| [$name, ": ", e.as_str()].concat())?;
                        })*
                        _ => {
                            config.extra.insert(name.clone(), value.clone());
                        }
                    }
                }
                Ok(config)
            }

            /// The config file's JSON.
            pub fn to_json(&self) -> Value {
                let mut out = self.extra.clone();
                $(out.insert($name.to_string(), config_fields!(@write $ty, self.$field));)*
                Value::Object(out)
            }
        }
    };
}

config_fields! {
    int config_version "configVersion" = 8;
    bool enabled "enabled" = true;
    string language "language" = "";
    bool guide_shown "guideShown" = false;
    string voice "voice" = "";
    string speed "speed" = "normal";
    float volume "volume" = 1.0;
    bool muted "muted" = false;
    float repeat_seconds "repeatSeconds" = 0.0;
    float same_again_seconds "sameAgainSeconds" = 10.0;
    float series_pause_seconds "seriesPauseSeconds" = 2.0;
    float movement_delay_seconds "movementDelaySeconds" = 1.5;
    float jump_cooldown_seconds "jumpCooldownSeconds" = 4.0;
    float same_event_cooldown_seconds "sameEventCooldownSeconds" = 0.6;
    int max_queue "maxQueue" = 2;
    bool say_movement "sayMovement" = false;
    bool say_combat "sayCombat" = true;
    bool say_blocks "sayBlocks" = true;
    bool say_items "sayItems" = true;
    bool say_world "sayWorld" = true;
    bool say_held_item "sayHeldItem" = true;
    bool say_weather_and_time "sayWeatherAndTime" = true;
    bool say_climate "sayClimate" = true;
    float climate_repeat_seconds "climateRepeatSeconds" = 0.0;
    bool say_kills "sayKills" = true;
    float weather_report_minutes "weatherReportMinutes" = 0.0;
    bool say_damage "sayDamage" = true;
    string quiz_mode "quizMode" = "choice";
    string hint_language "hintLanguage" = "auto";
    float quiz_repeat_after_minutes "quizRepeatAfterMinutes" = 5.0;
    bool rest_after_correct "restAfterCorrect" = true;
    int learned_after_correct "learnedAfterCorrect" = 1;
    float learned_rest_minutes "learnedRestMinutes" = 60.0;
    bool mute_learned "muteLearned" = false;
    string learned_mode "learnedMode" = "forever";
    float quiz_retry_after_seconds "quizRetryAfterSeconds" = 120.0;
    float quiz_interval_seconds "quizIntervalSeconds" = 60.0;
    float quiz_answer_seconds "quizAnswerSeconds" = 12.0;
    float exam_minutes "examMinutes" = 20.0;
    bool exam_auto "examAuto" = true;
    bool exam_choice "examChoice" = true;
    bool exam_typing "examTyping" = true;
    bool exam_penalties "examPenalties" = false;
    string quiz_penalty "quizPenalty" = "debuff";
    bool quiz_reward "quizReward" = true;
    bool quiz_pause_game "quizPauseGame" = true;
    int angry_after_wrong "angryAfterWrong" = 5;
    string angry_teacher "angryTeacher" = "drop";
    bool hud_enabled "hudEnabled" = true;
    string hud_font "hudFont" = "default";
    float hud_scale "hudScale" = 2.0;
    bool hud_show_english "hudShowEnglish" = true;
    float hud_hold_seconds "hudHoldSeconds" = 3.0;
    string hud_anchor "hudAnchor" = "bottom";
    int hud_offset "hudOffset" = 80;
    float hud_y "hudY" = 0.72;
    int hud_color_zh "hudColorZh" = -8054;
    int hud_color_en "hudColorEn" = -2236963;
    bool hud_shadow "hudShadow" = true;
    bool hud_colour_grammar "hudColourGrammar" = true;
    int hud_color_pronoun "hudColorPronoun" = -7419393;
    int hud_color_particle "hudColorParticle" = -4473925;
    int hud_color_verb "hudColorVerb" = -22195;
    int hud_color_adjective "hudColorAdjective" = -746814;
    int hud_color_object "hudColorObject" = -6560899;
    int hud_color_number "hudColorNumber" = -3758849;
    bool hud_held_label "hudHeldLabel" = true;
    string hud_portrait "hudPortrait" = "bottom_right";
    float hud_portrait_scale "hudPortraitScale" = 1.5;
    bool hud_backdrop "hudBackdrop" = true;
    bool tooltip_chinese "tooltipChinese" = true;
}

impl Config {
    /// Parses the config file's text; see [`Config::from_json`].
    pub fn parse(text: &str) -> Result<Config, String> {
        let value: Value = mod_api::serde_json::from_str(text).map_err(|e| e.to_string())?;
        Config::from_json(&value)
    }

    /// The config file's text, pretty printed as Gson writes it.
    pub fn to_json_string(&self) -> String {
        mod_api::serde_json::to_string_pretty(&self.to_json()).unwrap_or_default()
    }

    /// The teacher is on. Lesson-map worlds, which also silence him, are not ported.
    pub fn on(&self) -> bool {
        self.enabled
    }

    /// Whether the topic of an action is switched on.
    pub fn allows(&self, action: &str) -> bool {
        match action {
            "walk" | "sprint" | "sneak" | "swim" | "fly" | "climb" | "jump" | "ride" => {
                self.say_movement
            }
            "hit" | "attack" | "kill" | "kill_it" | "hurt" | "fall" | "die" | "burning" => {
                self.say_combat
            }
            "break" | "break_hand" | "place" | "open" | "close" | "use" => self.say_blocks,
            "hold" | "drop" | "pickup" | "eat" | "drink" => self.say_items,
            "word" => true,
            _ => self.say_world,
        }
    }

    /// The language whose hints replace `en`: `hintLanguage` unless it is blank or `auto`, else the
    /// game language without its region (`de_de` gives `de`).
    pub fn translation_language(&self, game_language: &str) -> String {
        if !self.hint_language.trim().is_empty() && self.hint_language != "auto" {
            return self.hint_language.clone();
        }
        game_language
            .split_once('_')
            .map_or(game_language, |(language, _)| language)
            .to_string()
    }

    /// The ARGB colour for a grammatical role, or `None` for a role with no colour of its own.
    pub fn role_colour(&self, role: &str) -> Option<i32> {
        match role {
            "pron" => Some(self.hud_color_pronoun),
            "verb" => Some(self.hud_color_verb),
            "obj" => Some(self.hud_color_object),
            "adj" => Some(self.hud_color_adjective),
            "num" => Some(self.hud_color_number),
            "part" => Some(self.hud_color_particle),
            _ => None,
        }
    }
}
