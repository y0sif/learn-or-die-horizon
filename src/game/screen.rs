//! The question screen (Java `QuizScreen`), drawn on the mod's own screen: the item's picture,
//! the English prompt (or a replay button for a listening question), the timer bar and four
//! German options as buttons. Keys 1 to 4 and a click answer (both ignored for the first 700 ms);
//! Esc suspends the question and the pause menu opens (the `quiz_escape` mixin), and it comes back
//! with the time left once no screen is open.
//!
//! The input hooks' host only reads, so an answer is recorded here and the next client tick
//! closes the screen and writes the chat line.
use super::hud::{self, GOLD, frame, panel, width};
use super::{ESCAPE_PAUSE, QUIZ_PAUSES, STATE, State, lock, sound};
use crate::core::{OpenQuestion, Phrase, Teacher};
use mod_api::client::hud_layout::PoseOp;
use mod_api::{
    ClientHost, DrawContext, Flow, GuiDraw, Identifier, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenRenderPostEvent, Text,
};
use std::sync::atomic::Ordering;

const BUTTON_W: f32 = 190.0;
const BUTTON_H: f32 = 22.0;
const ROW_H: f32 = 44.0;
const PROMPT: u32 = 0xFFFF_E08A;
const GREY: u32 = 0xFFBB_BBBB;
const HURRY: u32 = 0xFFFF_7A7A;
/// What the option text is brightened by (`NarratorHud.lift`, 0.35).
const LIFT: f32 = 0.35;

/// Where the screen's parts are, for a screen of this size.
struct Layout {
    w: f32,
    h: f32,
}

impl Layout {
    fn new([w, h]: [f32; 2]) -> Self {
        Self { w, h }
    }

    /// `buttonX`, `buttonY`: two columns of two.
    fn option(&self, index: usize) -> [f32; 4] {
        let column = (index % 2) as f32;
        let row = (index / 2) as f32;
        [
            (self.w / 2.0).floor() - BUTTON_W - 6.0 + column * (BUTTON_W + 12.0),
            (self.h / 2.0).floor() + 2.0 + row * ROW_H,
            BUTTON_W,
            BUTTON_H,
        ]
    }

    fn replay(&self) -> [f32; 4] {
        [
            (self.w / 2.0).floor() - 60.0,
            (self.h / 2.0).floor() - 54.0,
            120.0,
            20.0,
        ]
    }
}

fn inside([x, y]: [f32; 2], [rx, ry, rw, rh]: [f32; 4]) -> bool {
    (rx..rx + rw).contains(&x) && (ry..ry + rh).contains(&y)
}

/// `NarratorHud.lift`: the colour moved `f` of the way to white.
fn lift(rgb: u32, f: f32) -> u32 {
    let channel = |shift: u32| {
        let c = ((rgb >> shift) & 0xFF) as f32;
        ((c + ((255.0 - c) * f).round()) as u32).min(255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// A literal in one colour (`0xRRGGBB`).
pub(crate) fn coloured(text: impl Into<String>, rgb: u32) -> Text {
    let mut out = Text::literal(text);
    out.style.color = Some(format!("#{:06X}", rgb & 0x00FF_FFFF));
    out
}

/// An option's button label: its number, then its words in their role colours, brightened.
fn label(teacher: &Teacher, index: usize, option: &Phrase) -> Text {
    let mut text = Text::literal("").append(coloured(format!("{}. ", index + 1), PROMPT));
    for span in teacher.spans(option, 0x00FF_FFFF) {
        text = text.append(coloured(span.text, lift(span.colour as u32, LIFT)));
    }
    text
}

/// A line of text `scale` times its size, centred on `at`, in one batch.
fn centred_text(draw: &mut dyn DrawContext, text: &str, [x, y]: [f32; 2], scale: f32, colour: u32) {
    let offset = -(width(draw, text) / 2.0).floor();
    let ops = [
        GuiDraw::Push(vec![PoseOp::Translate(x, y), PoseOp::Scale(scale, scale)]),
        GuiDraw::Text {
            text: Text::literal(text),
            x: offset as i32,
            y: 0,
            color: colour,
            shadow: true,
        },
        GuiDraw::Pop,
    ];
    if let Err(e) = draw.gui_paint(&ops) {
        eprintln!("[{}] could not draw the question: {e:?}", super::MOD_ID);
    }
}

/// `NarratorHud.timeBar`.
fn time_bar(draw: &mut dyn DrawContext, [x0, y, w]: [f32; 3], fraction: f32) {
    let f = fraction.clamp(0.0, 1.0);
    hud::fill(draw, [x0, y, x0 + w, y + 2.0], 0x3CFF_FFFF);
    let colour = if f < 0.25 { 0x00FF_5A5A } else { GOLD };
    hud::fill(
        draw,
        [x0, y, x0 + (w * f).round(), y + 2.0],
        0xFF00_0000 | colour,
    );
}

/// The open question while this mod's screen may show it.
fn showing(teacher: &Teacher) -> Option<&OpenQuestion> {
    teacher.question().filter(|open| !open.is_suspended())
}

/// `extractRenderState`.
pub(crate) fn render(host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
    let io = super::io();
    if !io.our_screen(host) {
        return;
    }
    let now = io.now();
    let mouse = event.screen.mouse;
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_ref() else {
        return;
    };
    let Some(open) = showing(teacher) else {
        return;
    };
    let draw = &mut *event.draw;
    let size = draw.size();
    game.screen_size = size;
    let layout = Layout::new(size);
    let [w, h] = size;
    let (cx, mid) = (w / 2.0, (h / 2.0).floor());
    draw.fill_gradient([0.0, 0.0, w, h], 0xC010_1010, 0xD010_1010);
    let card_w = BUTTON_W * 2.0 + 12.0 + 32.0;
    let x0 = (cx - card_w / 2.0).round();
    let bottom = layout.option(3)[1] + BUTTON_H + 10.0;
    panel(draw, [x0, mid - 130.0, x0 + card_w, bottom], 255);

    let picture = game
        .picture
        .as_ref()
        .filter(|(id, _)| *id == open.question.id)
        .and_then(|(_, item)| item.as_deref())
        .and_then(|item| Identifier::parse(item).ok())
        .and_then(|id| host.item_key(&id));
    if let Some(item) = picture {
        let (sx, sy) = (cx.round() - 28.0, mid - 123.0);
        hud::fill(draw, [sx, sy, sx + 56.0, sy + 56.0], 0x5000_0000);
        frame(draw, [sx, sy, sx + 56.0, sy + 56.0], 0x40E6_B422);
        draw.item(item, [sx + 4.0, sy + 4.0], 48.0);
    }

    let title = if open.listening {
        "learnordie.quiz.listen"
    } else {
        "learnordie.quiz.prompt"
    };
    let title = io.tr(host, &Text::translate(title, vec![]));
    centred_text(draw, &title, [cx, mid - 62.0], 1.0, GREY);
    if open.listening {
        let replay = layout.replay();
        let text = Text::translate("learnordie.quiz.replay", vec![]);
        let _ = draw.widget_button(replay, &text, true, inside(mouse, replay));
    } else {
        centred_text(draw, &open.question.en, [cx, mid - 48.0], 1.6, PROMPT);
    }

    let left = open.left_ms(now);
    let timer = format!("{:.0} s", left as f64 / 1000.0);
    let bar_right = x0 + card_w - 16.0 - width(draw, &timer) - 6.0;
    time_bar(
        draw,
        [x0 + 16.0, mid - 20.0, bar_right - (x0 + 16.0)],
        open.fraction_left(now),
    );
    let colour = if left < 3000 { HURRY } else { GREY };
    draw.text(&timer, [bar_right + 6.0, mid - 24.0], colour, true);

    for (index, option) in open.options.iter().enumerate() {
        let rect = layout.option(index);
        let text = label(teacher, index, option);
        let _ = draw.widget_button(rect, &text, true, inside(mouse, rect));
    }
}

/// Keys 1 to 4 answer. Esc suspends the question and lets Escape close the screen, after which
/// the `quiz_escape` mixin opens the pause menu.
pub(crate) fn key_pressed(host: &dyn ClientHost, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
    let io = super::io();
    if !io.our_screen(host) {
        return Flow::Continue;
    }
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_mut() else {
        return Flow::Continue;
    };
    let index = match event.key {
        "key.keyboard.escape" => {
            if teacher.suspend_question(now) {
                QUIZ_PAUSES.store(false, Ordering::Relaxed);
                ESCAPE_PAUSE.store(true, Ordering::Relaxed);
            }
            return Flow::Continue;
        }
        "key.keyboard.1" => 0,
        "key.keyboard.2" => 1,
        "key.keyboard.3" => 2,
        "key.keyboard.4" => 3,
        _ => return Flow::Continue,
    };
    if let Some(outcome) = teacher.choose(index, now) {
        QUIZ_PAUSES.store(false, Ordering::Relaxed);
        game.outcomes.push(outcome);
    }
    Flow::Cancel
}

/// A left click on an option answers it; on the replay button it plays the phrase again.
pub(crate) fn mouse_pressed(
    host: &dyn ClientHost,
    event: &mut ScreenMouseButtonPressedPreEvent<'_>,
) -> Flow {
    let io = super::io();
    let io = &*io;
    if event.button != 0 || !io.our_screen(host) {
        return Flow::Continue;
    }
    let now = io.now();
    let mouse = event.screen.mouse;
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_mut() else {
        return Flow::Continue;
    };
    let Some(open) = showing(teacher) else {
        return Flow::Continue;
    };
    let (listening, count) = (open.listening, open.options.len());
    let layout = Layout::new(game.screen_size);
    if listening && inside(mouse, layout.replay()) {
        sound::click(host, io);
        teacher.replay_question(now);
        return Flow::Cancel;
    }
    let Some(index) = (0..count).find(|&index| inside(mouse, layout.option(index))) else {
        return Flow::Continue;
    };
    sound::click(host, io);
    if let Some(outcome) = teacher.choose(index, now) {
        QUIZ_PAUSES.store(false, Ordering::Relaxed);
        game.outcomes.push(outcome);
    }
    Flow::Cancel
}
