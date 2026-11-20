//! HUD decisions with no drawing (Java `NarratorHud`): the coloured pieces of a phrase, the
//! teacher portrait's frame and hop, and the fades of the text and the held-item label.
//!
//! Portrait frames: 0 idle, 1 and 2 mouth open, 3 blink, 4 frown, 8 happy (from 9 frames on).
//! The angry and shooting frames belong to the consequences, which come later.
use super::config::Config;
use super::pack::{Phrase, Sprite};

const BLINK_MS: i64 = 150;
const HAPPY_MS: i64 = 1200;
const HELD_LABEL_MS: i64 = 2500;
const HELD_FADE_FROM_MS: i64 = 2000;

/// The phrase the HUD shows now.
#[derive(Clone, Debug, PartialEq)]
pub struct HudText<'a> {
    pub phrase: &'a Phrase,
    /// 16 to 255: the text fades over `hudHoldSeconds` after the clip ends.
    pub alpha: i32,
    /// The translation line: off for held-item nouns and when it equals the text.
    pub show_translation: bool,
}

/// A run of text in one colour (ARGB).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub colour: i32,
}

/// The phrase in coloured pieces: one per part, joined by the pack's word separator (none after
/// an elided word ending in an apostrophe), then the text after the last part, such as the full
/// stop, in the particle colour. A part whose role has no colour, and the whole text when there
/// are no parts or grammar colours are off, take `fallback`.
pub fn spans(phrase: &Phrase, separator: &str, config: &Config, fallback: i32) -> Vec<Span> {
    if phrase.parts.is_empty() || !config.hud_colour_grammar {
        return vec![Span {
            text: phrase.text.clone(),
            colour: fallback,
        }];
    }
    let last = phrase.parts.len() - 1;
    let mut joined = String::new();
    let mut out = Vec::with_capacity(phrase.parts.len() + 1);
    for (index, part) in phrase.parts.iter().enumerate() {
        let elided = part.text.ends_with('\'') || part.text.ends_with('\u{2019}');
        let mut text = part.text.clone();
        if index < last && !elided {
            text.push_str(separator);
        }
        joined.push_str(&text);
        out.push(Span {
            text,
            colour: config.role_colour(&part.role).unwrap_or(fallback),
        });
    }
    if let Some(tail) = phrase.text.strip_prefix(joined.as_str())
        && !tail.is_empty()
    {
        out.push(Span {
            text: tail.to_string(),
            colour: config.hud_color_particle,
        });
    }
    out
}

/// The portrait's blink timer.
#[derive(Debug, Default)]
pub(crate) struct Portrait {
    next_blink_at: i64,
}

impl Portrait {
    /// The frame to draw now. `mouth` is the speaker's mouth level, `hop_at` the last right
    /// answer, `frowning` the 2.5 s after a wrong one.
    pub(crate) fn frame(
        &mut self,
        now: i64,
        sprite: &Sprite,
        mouth: i32,
        speaking: bool,
        hop_at: i64,
        frowning: bool,
    ) -> i32 {
        let mut blink = false;
        if self.next_blink_at == 0 || now > self.next_blink_at + BLINK_MS {
            self.next_blink_at = now + 2500 + now % 2500;
        } else if now >= self.next_blink_at {
            blink = true;
        }
        let frames = sprite.frames;
        if frames > 8 && now - hop_at < HAPPY_MS {
            if mouth > 0 && frames > 8 + mouth {
                8 + mouth
            } else {
                8
            }
        } else if frowning {
            if mouth > 0 && frames > 10 + mouth {
                10 + mouth
            } else {
                4
            }
        } else if speaking && mouth > 0 {
            mouth_frame(sprite, mouth, now)
        } else if blink {
            3
        } else {
            0
        }
    }
}

/// The frame for an open mouth: the sprite's own mouth frames in turn every 170 ms, else the
/// level itself.
fn mouth_frame(sprite: &Sprite, level: i32, now: i64) -> i32 {
    let shapes = if level == 1 {
        &sprite.mouth_small
    } else {
        &sprite.mouth_wide
    };
    if shapes.is_empty() {
        return level;
    }
    let index = (now / 170).rem_euclid(shapes.len() as i64) as usize;
    shapes[index]
}

/// How high the portrait jumps after a right answer: up for 300 ms and a small bounce for
/// 220 ms, both slower for a taller hop. In sprite pixels.
pub(crate) fn hop_lift(now: i64, hop_at: i64, height: f32) -> f32 {
    let t = now - hop_at;
    let slower = (height / 6.0).cbrt();
    let up = 300.0 * slower;
    let bounce = 220.0 * slower;
    if t < 0 || t as f32 >= up + bounce {
        return 0.0;
    }
    let t = t as f64;
    let (up, bounce, height) = (f64::from(up), f64::from(bounce), f64::from(height));
    let lift = if t < up {
        height * (std::f64::consts::PI * t / up).sin()
    } else {
        height / 2.0 * (std::f64::consts::PI * (t - up) / bounce).sin()
    };
    lift as f32
}

/// The held-item label's alpha `age_ms` after it appeared: full for 2 s, then fading over 0.5 s.
/// `None` once it is gone.
pub(crate) fn held_label_alpha(age_ms: i64) -> Option<i32> {
    if !(0..=HELD_LABEL_MS).contains(&age_ms) {
        return None;
    }
    if age_ms > HELD_FADE_FROM_MS {
        Some((255 * (HELD_LABEL_MS - age_ms) / 500).max(20) as i32)
    } else {
        Some(255)
    }
}
