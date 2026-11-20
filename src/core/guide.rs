//! The guide (Java `GuideScreen`, with the port's own pages, since the original's text and
//! pictures are not available to it): the page order, the page with the angry face, the angry
//! page's text variants, and when the guide opens on its own.
//!
//! The screen: Back is off on the first page, Skip is hidden on the last, Next reads Done there and
//! closes the guide. Opened on its own, closing it sends the chat hint (the guide and settings
//! keys, then the tips).
use super::config::Config;

/// The client tick in a world at which the guide opens the first time, or the hint comes (3 s).
pub const GUIDE_AFTER_TICKS: u32 = 60;

/// A page of the guide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuidePage {
    /// The keys.
    Keys,
    /// The teacher's questions.
    Questions,
    /// What a right answer brings and a wrong one costs.
    Consequences,
    /// The angry teacher.
    Angry,
}

/// The pages in order.
pub const GUIDE_PAGES: [GuidePage; 4] = [
    GuidePage::Keys,
    GuidePage::Questions,
    GuidePage::Consequences,
    GuidePage::Angry,
];

impl GuidePage {
    /// The page's name in its lang keys: `<prefix>.guide.<name>.title` and `.text`. The angry
    /// page's text has a variant per mode: see [`angry_page`].
    pub fn name(self) -> &'static str {
        match self {
            GuidePage::Keys => "keys",
            GuidePage::Questions => "questions",
            GuidePage::Consequences => "consequences",
            GuidePage::Angry => "angry",
        }
    }

    /// The portrait frame drawn under the text: the angry face (frame 5) on the angry page.
    pub fn portrait_frame(self) -> Option<i32> {
        (self == GuidePage::Angry).then_some(5)
    }
}

/// The angry page's text variant: the angry teacher's mode (`drop`, `delete` or `off`; any other
/// value reads as `drop`), and after how many wrong answers in a row he gets angry (at least 1).
pub fn angry_page(config: &Config) -> (&'static str, i32) {
    let mode = match config.angry_teacher.as_str() {
        "delete" => "delete",
        "off" => "off",
        _ => "drop",
    };
    (mode, config.angry_after_wrong.max(1))
}

/// What the guide does now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideCue {
    /// Open the guide: the first time ever. `guideShown` is set now: save the config.
    Open,
    /// Send the chat hint instead.
    Hint,
}

/// `ZhNarratorClient.tick`'s first-run guide. `ticks_in_world` counts client ticks, paused or
/// not, while a pack is loaded and a player exists, from 1, and starts again at 0 whenever there
/// is no player. At [`GUIDE_AFTER_TICKS`], with the teacher on: the guide opens if it was never
/// shown and no screen is open, else the hint comes. Every world gets one or the other.
pub fn guide_cue(ticks_in_world: u32, screen_open: bool, config: &mut Config) -> Option<GuideCue> {
    if ticks_in_world != GUIDE_AFTER_TICKS || !config.on() {
        return None;
    }
    if !config.guide_shown && !screen_open {
        config.guide_shown = true;
        Some(GuideCue::Open)
    } else {
        Some(GuideCue::Hint)
    }
}
