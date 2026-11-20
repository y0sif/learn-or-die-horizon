//! The guide (Java `GuideScreen`, with the port's own short pages: the original's text and
//! pictures are not available to it), on this mod's screen: U opens it, the settings' main page
//! has a button for it, and it opens on its own 3 s into the first world ever (`guide_cue`).
//!
//! Each page a title, the page count and its text in a 330 px panel; the angry teacher's page
//! shows his angry face (portrait frame 5) under the text, his waist at most 72 px tall. Back
//! is off on the first page, Skip is gone on the last, where Next reads Done and closes. Opened
//! from the settings, closing goes back to them. Opened on its own, closing sends the chat hint:
//! the guide and settings keys, then a few settings worth a look. It pauses single player, as
//! the original's.
use super::client::{Client, WidgetHost};
use super::hud::panel;
use super::keys::{DICTIONARY, GUIDE, MUTE, SETTINGS, SPEED, TOGGLE, WHAT_IS_THIS};
use super::screen::scaled;
use super::settings::{Page, Settings};
use super::{Game, log_warn, number, teacher_says};
use crate::core::{GUIDE_PAGES, GuidePage, Teacher, angry_page};
use mod_api::client::widgets::{Button, Outcome as Input, WidgetId, Widgets};
use mod_api::{ClientHost, DrawContext, Flow, Text};

const W: f32 = 330.0;
/// The text's width inside the panel.
const TEXT_W: f32 = 306.0;
const LINE_H: f32 = 11.0;
const PORTRAIT_H: f32 = 72.0;
const TITLE: u32 = 0xFFFF_E08A;
const COUNT: u32 = 0xFFBB_BBBB;
const WHITE: u32 = 0xFFFF_FFFF;
/// `§7`.
const GREY: u32 = 0xAA_AAAA;

/// The guide showing.
pub(crate) struct GuideScreen {
    page: usize,
    /// Opened from the settings' main page: closing goes back there.
    from_settings: bool,
    /// Opened on its own in the first world: closing sends the chat hint.
    pub(crate) first_show: bool,
    /// Skip, or Done on the last page: the next client tick closes the screen.
    pub(crate) close: bool,
    buttons: Buttons,
}

/// A page's buttons (`refresh`): Back, off on the first page; Skip, gone on the last; Next, which
/// reads Done there.
struct Buttons {
    widgets: Widgets,
    back: WidgetId,
    skip: Option<WidgetId>,
    next: WidgetId,
}

impl Buttons {
    fn new(page: usize) -> Buttons {
        let last = page + 1 >= GUIDE_PAGES.len();
        let mut widgets = Widgets::new();
        let button = |key: &str| Button::new([0.0; 4], Text::translate(key, vec![]));
        let mut back = button("learnordie.guide.back");
        back.active = page > 0;
        let back = widgets.add_button(back);
        let skip = (!last).then(|| widgets.add_button(button("learnordie.guide.skip")));
        let next = if last {
            "learnordie.guide.done"
        } else {
            "learnordie.guide.next"
        };
        let next = widgets.add_button(button(next));
        Buttons {
            widgets,
            back,
            skip,
            next,
        }
    }
}

/// The settings' key, by its mapping, as Controls names it.
fn key(host: &dyn ClientHost, io: &dyn Client, id: &str) -> Text {
    io.key_name(host, id)
}

/// The page's text (`pageText`): the keys page names the keys, the angry page says what his
/// temper does after how many wrong answers in a row, the others point to the settings.
fn page_text(host: &dyn ClientHost, io: &dyn Client, teacher: &Teacher, page: GuidePage) -> Text {
    let key = |id: &str| key(host, io, id);
    match page {
        GuidePage::Keys => Text::translate(
            "learnordie.guide.keys.text",
            [
                SETTINGS,
                DICTIONARY,
                WHAT_IS_THIS,
                SPEED,
                MUTE,
                TOGGLE,
                GUIDE,
            ]
            .into_iter()
            .map(key)
            .collect(),
        ),
        GuidePage::Angry => {
            let (mode, after) = angry_page(&teacher.config);
            Text::translate(
                format!("learnordie.guide.angry.text.{mode}"),
                vec![Text::literal(after.to_string()), key(SETTINGS)],
            )
        }
        page => Text::translate(
            format!("learnordie.guide.{}.text", page.name()),
            vec![key(SETTINGS)],
        ),
    }
}

/// `GuideScreen.hint`, then `tip`: the chat lines after the guide's first show, and in every
/// other world 3 s in.
pub(crate) fn hint(host: &dyn ClientHost, io: &dyn Client) -> Vec<Text> {
    let key = |id: &str| key(host, io, id);
    let mut lines = vec![
        teacher_says("learnordie.guide.reopen", vec![key(GUIDE), key(SETTINGS)]),
        teacher_says("learnordie.tip.open", vec![key(SETTINGS)]),
    ];
    for tip in ["same_again", "question_every", "penalty", "quiz"] {
        let mut line = Text::literal("  \u{2022} ")
            .append(Text::translate(format!("learnordie.tip.{tip}"), vec![]));
        line.style.color = Some(format!("#{GREY:06X}"));
        lines.push(line);
    }
    lines
}

impl GuideScreen {
    pub(crate) fn new(from_settings: bool, first_show: bool) -> GuideScreen {
        GuideScreen {
            page: 0,
            from_settings,
            first_show,
            close: false,
            buttons: Buttons::new(0),
        }
    }

    /// `turn`: false past the last page, where the guide closes.
    fn turn(&mut self, by: isize) -> bool {
        let to = self.page as isize + by;
        if to >= GUIDE_PAGES.len() as isize {
            return false;
        }
        self.page = to.max(0) as usize;
        self.buttons = Buttons::new(self.page);
        true
    }

    /// `extractRenderState`: the panel sized to the page, its title and count, the text, the
    /// angry face on the angry page, and the buttons at its bottom.
    pub(crate) fn render(
        &mut self,
        host: &dyn ClientHost,
        io: &dyn Client,
        draw: &mut dyn DrawContext,
        teacher: &Teacher,
        mouse: [f32; 2],
    ) {
        let [w, h] = draw.size();
        io.blur(host);
        draw.draw_menu_background([0.0, 0.0, w, h], true);
        let page = GUIDE_PAGES[self.page.min(GUIDE_PAGES.len() - 1)];
        let text = io.tr(host, &page_text(host, io, teacher, page));
        let lines = super::dictionary::wrap(draw, &text, TEXT_W);
        let text_h = lines.len() as f32 * LINE_H;
        let portrait_h = if page.portrait_frame().is_some() {
            (h - 20.0 - 28.0 - text_h - 46.0 - 8.0).clamp(0.0, PORTRAIT_H)
        } else {
            0.0
        };
        let picture = if portrait_h > 0.0 {
            portrait_h + 8.0
        } else {
            0.0
        };
        let panel_h = 28.0 + text_h + picture + 46.0;
        let x0 = (w / 2.0).floor() - W / 2.0;
        let top = ((h / 2.0).floor() - (panel_h / 2.0).floor()).max(10.0);
        panel(draw, [x0, top, x0 + W, top + panel_h], 245);
        let title = Text::translate(format!("learnordie.guide.{}.title", page.name()), vec![]);
        scaled(draw, title, [x0 + 12.0, top + 10.0], 1.0, TITLE);
        let count = Text::translate(
            "learnordie.guide.page",
            vec![number(self.page + 1), number(GUIDE_PAGES.len())],
        );
        let count_w = draw.text_width(&io.tr(host, &count));
        scaled(
            draw,
            count,
            [x0 + W - 12.0 - count_w, top + 10.0],
            1.0,
            COUNT,
        );
        let mut y = top + 28.0;
        for line in &lines {
            draw.text(line, [x0 + 12.0, y], WHITE, true);
            y += LINE_H;
        }
        if let Some(frame) = page.portrait_frame().filter(|_| portrait_h > 0.0) {
            let meta = &teacher.pack.meta;
            let sprite = &meta.sprite;
            let (fw, waist) = (sprite.frame_width as f32, sprite.waist.max(1) as f32);
            let s = portrait_h / waist;
            let name = format!("{}:gui/{}", meta.namespace, sprite.file);
            let src = [frame as f32 * fw, 0.0, fw, waist];
            let dst = [x0 + W / 2.0 - fw * s / 2.0, y + 4.0, fw * s, waist * s];
            draw.sprite(&name, src, dst, None);
        }
        let buttons_y = top + panel_h - 32.0;
        let b = &mut self.buttons;
        let places = [
            (Some(b.back), [x0 + 12.0, buttons_y, 100.0, 20.0]),
            (b.skip, [x0 + W / 2.0 - 40.0, buttons_y, 80.0, 20.0]),
            (Some(b.next), [x0 + W - 112.0, buttons_y, 100.0, 20.0]),
        ];
        for (id, rect) in places {
            if let Some(button) = id.and_then(|id| b.widgets.button_mut(id)) {
                button.rect = rect;
            }
        }
        if let Err(e) = b.widgets.draw(draw, &WidgetHost { io, host }, mouse) {
            log_warn(format!("could not draw the guide: {e:?}"));
        }
    }

    pub(crate) fn released(&mut self, button: i32) -> bool {
        self.buttons.widgets.mouse_released(button).handled()
    }
}

/// `onClose`: opened from the settings, back to their main page; else the screen closes (and a
/// first show sends the hint as it goes, `super::panels_tick`). True when the screen stays.
fn leave(teacher: &Teacher, game: &mut Game) -> bool {
    let Some(page) = game.guide.as_mut() else {
        return false;
    };
    if page.from_settings {
        game.guide = None;
        game.settings = Some(Settings::new(Page::Main, &teacher.pack));
        true
    } else {
        page.close = true;
        false
    }
}

/// What a button made of an input: Back and Next turn the page, Skip and the last page's Done
/// close the guide.
fn act(teacher: &Teacher, game: &mut Game, input: Input) -> Flow {
    let Some(page) = game.guide.as_mut() else {
        return Flow::Continue;
    };
    let b = &page.buttons;
    let leaves = match input {
        Input::Pressed(id) if id == b.back => !page.turn(-1),
        Input::Pressed(id) if Some(id) == b.skip => true,
        Input::Pressed(id) if id == b.next => !page.turn(1),
        _ => false,
    };
    if leaves {
        leave(teacher, game);
    }
    if input.handled() {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// Esc: opened from the settings, back to them; else Escape closes the screen. The other keys
/// reach the buttons.
pub(super) fn key_pressed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    game: &mut Game,
    key: &str,
    modifiers: u32,
) -> Flow {
    let Some(page) = game.guide.as_mut() else {
        return Flow::Continue;
    };
    if key == "key.keyboard.escape" {
        if page.from_settings && leave(teacher, game) {
            return Flow::Cancel;
        }
        return Flow::Continue;
    }
    let input = page
        .buttons
        .widgets
        .key_pressed(&WidgetHost { io, host }, key, modifiers);
    act(teacher, game, input)
}

pub(super) fn mouse_pressed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    game: &mut Game,
    at: [f32; 2],
    modifiers: u32,
) -> Flow {
    let Some(page) = game.guide.as_mut() else {
        return Flow::Continue;
    };
    let input = page
        .buttons
        .widgets
        .mouse_pressed(&WidgetHost { io, host }, at, 0, modifiers);
    act(teacher, game, input)
}
