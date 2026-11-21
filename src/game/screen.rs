//! This mod's screen. It shows the open question, as four options (Java `QuizScreen`, here) or
//! as an edit box ([`super::typing`]), or a settings page ([`super::settings`]), the exam's
//! result ([`super::exam`]), the dictionary ([`super::dictionary`]) or the guide
//! ([`super::guide`]); this module sends each screen hook to the one showing.
//!
//! The choice question: the item's picture, the English prompt (or a replay button for a
//! listening question), the timer bar and four German options as buttons; an exam's question has
//! `learnordie.exam.header` at the card's top. Keys 1 to 4 and a click answer (both ignored for the
//! first 700 ms); Esc suspends the question and the pause menu opens (the `quiz_escape` mixin),
//! and it comes back with the time left once no screen is open.
//!
//! The input hooks' host only reads, so an answer is recorded here and the next client tick
//! closes the screen and writes the chat line.
//!
//! A controller's A, right stick, B and Start, its on-screen keyboard and the D-pad on a slider
//! that A grabbed reach these hooks only through a separate stopgap mod (`padbridge`), as the
//! mouse and keys do, so this mod has no controller mixins of its own (each would arrive twice).
use super::hud::{self, GOLD, frame, panel, width};
use super::{
    ESCAPE_PAUSE, Game, QUIZ_PAUSES, STATE, State, dictionary, guide, lock, number, settings,
    sound, typing,
};
use crate::core::{ExamStage, OpenQuestion, Phrase, QuestionKind, Teacher};
use mod_api::client::hud_layout::PoseOp;
use mod_api::{
    ClientHost, DrawContext, Flow, GuiDraw, Identifier, ScreenCharTypedPreEvent,
    ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent, ScreenRenderPostEvent, Text,
};
use std::sync::atomic::Ordering;

/// The option buttons' width in a wide enough window; a narrower one shrinks them to fit.
const BUTTON_W: f32 = 190.0;
const BUTTON_H: f32 = 22.0;
const ROW_H: f32 = 44.0;
/// Between the two columns.
const GAP: f32 = 12.0;
/// Inside the card's edges.
pub(super) const PAD: f32 = 16.0;
/// Kept between the card and the screen's edges.
pub(super) const EDGE: f32 = 4.0;
pub(super) const PROMPT: u32 = 0xFFFF_E08A;
pub(super) const GREY: u32 = 0xFFBB_BBBB;
const HURRY: u32 = 0xFFFF_7A7A;
/// What the option text is brightened by (`NarratorHud.lift`, 0.35).
const LIFT: f32 = 0.35;

/// Where the screen's parts are, for a screen of this size. The card and its two columns of
/// options keep inside the screen's width: a narrow window gets narrower buttons.
struct Layout {
    w: f32,
    h: f32,
    button_w: f32,
}

impl Layout {
    fn new([w, h]: [f32; 2]) -> Self {
        let room = (w - EDGE * 2.0 - PAD * 2.0 - GAP) / 2.0;
        let button_w = room.floor().clamp(1.0, BUTTON_W);
        Self { w, h, button_w }
    }

    /// The card's width: the two columns and their padding.
    fn card_w(&self) -> f32 {
        self.button_w * 2.0 + GAP + PAD * 2.0
    }

    /// `buttonX`, `buttonY`: two columns of two.
    fn option(&self, index: usize) -> [f32; 4] {
        let column = (index % 2) as f32;
        let row = (index / 2) as f32;
        [
            (self.w / 2.0).floor() - self.button_w - GAP / 2.0 + column * (self.button_w + GAP),
            (self.h / 2.0).floor() + 2.0 + row * ROW_H,
            self.button_w,
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

pub(super) fn inside([x, y]: [f32; 2], [rx, ry, rw, rh]: [f32; 4]) -> bool {
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

/// A line of text `scale` times its size, centred on `at`, in one batch; scaled down further
/// when it would be wider than `room`.
pub(super) fn centred_text(
    draw: &mut dyn DrawContext,
    text: &str,
    [x, y]: [f32; 2],
    [scale, room]: [f32; 2],
    colour: u32,
) {
    let text_w = width(draw, text);
    let scale = if text_w * scale > room && text_w > 0.0 {
        room / text_w
    } else {
        scale
    };
    let offset = -(text_w / 2.0).floor();
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
        super::log_warn(format!("could not draw the question: {e:?}"));
    }
}

/// A text `scale` times its size with its top left at `at`, in one batch.
pub(super) fn scaled(
    draw: &mut dyn DrawContext,
    text: Text,
    [x, y]: [f32; 2],
    scale: f32,
    colour: u32,
) {
    let ops = [
        GuiDraw::Push(vec![PoseOp::Translate(x, y), PoseOp::Scale(scale, scale)]),
        GuiDraw::Text {
            text,
            x: 0,
            y: 0,
            color: colour,
            shadow: true,
        },
        GuiDraw::Pop,
    ];
    if let Err(e) = draw.gui_paint(&ops) {
        super::log_warn(format!("could not draw a text: {e:?}"));
    }
}

/// An exam question's header at the card's top left, `x0 + 8, top + 6` (Java
/// `QuizManager.questionHeader`, drawn by `QuizScreen` and `TypingScreen`).
pub(super) fn exam_header(
    draw: &mut dyn DrawContext,
    teacher: &Teacher,
    [x0, top]: [f32; 2],
    now: i64,
) {
    if let ExamStage::Asking { number: n, total } = teacher.exam(now)
        && n <= total
    {
        let header = Text::translate("learnordie.exam.header", vec![number(n), number(total)]);
        scaled(draw, header, [x0 + 8.0, top + 6.0], 1.0, PROMPT);
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
pub(super) fn showing(teacher: &Teacher) -> Option<&OpenQuestion> {
    teacher.question().filter(|open| !open.is_suspended())
}

/// The question's picture in its framed square at the card's top (`WordIcons.iconFor`).
pub(super) fn picture(
    draw: &mut dyn DrawContext,
    host: &dyn ClientHost,
    game: &Game,
    open: &OpenQuestion,
    [cx, mid]: [f32; 2],
) {
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
}

/// The time bar under the prompt and the seconds left beside it, across the card from `x0`.
pub(super) fn timer(
    draw: &mut dyn DrawContext,
    open: &OpenQuestion,
    [x0, card_w, mid]: [f32; 3],
    now: i64,
) {
    let left = open.left_ms(now);
    let timer = format!("{:.0} s", left as f64 / 1000.0);
    let bar_right = x0 + card_w - PAD - width(draw, &timer) - 6.0;
    time_bar(
        draw,
        [x0 + PAD, mid - 20.0, bar_right - (x0 + PAD)],
        open.fraction_left(now),
    );
    let colour = if left < 3000 { HURRY } else { GREY };
    draw.text(&timer, [bar_right + 6.0, mid - 24.0], colour, true);
}

/// `extractRenderState` of the screen showing.
pub(crate) fn render(host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
    let io = super::io();
    let io = &*io;
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
    let draw = &mut *event.draw;
    if game.settings.is_some() {
        settings::render(host, io, draw, teacher, game, mouse);
        return;
    }
    if let Some(page) = game.result.as_mut() {
        page.render(host, io, draw, mouse);
        return;
    }
    if game.dictionary.is_some() {
        dictionary::render(host, io, draw, teacher, game, mouse);
        return;
    }
    if let Some(page) = game.guide.as_mut() {
        page.render(host, io, draw, teacher, mouse);
        return;
    }
    let Some(open) = showing(teacher) else {
        return;
    };
    if open.kind == QuestionKind::Typing {
        typing::render(host, io, draw, teacher, game, mouse, now);
        return;
    }
    let size = draw.size();
    game.screen_size = size;
    let layout = Layout::new(size);
    let [w, h] = size;
    let (cx, mid) = (w / 2.0, (h / 2.0).floor());
    draw.fill_gradient([0.0, 0.0, w, h], 0xC010_1010, 0xD010_1010);
    let card_w = layout.card_w();
    let room = card_w - PAD * 2.0;
    let x0 = (cx - card_w / 2.0).round();
    let bottom = layout.option(3)[1] + BUTTON_H + 10.0;
    panel(draw, [x0, mid - 130.0, x0 + card_w, bottom], 255);
    picture(draw, host, game, open, [cx, mid]);
    exam_header(draw, teacher, [x0, mid - 130.0], now);

    let title = if open.listening {
        "learnordie.quiz.listen"
    } else {
        "learnordie.quiz.prompt"
    };
    let title = io.tr(host, &Text::translate(title, vec![]));
    centred_text(draw, &title, [cx, mid - 62.0], [1.0, room], GREY);
    if open.listening {
        let replay = layout.replay();
        let text = Text::translate("learnordie.quiz.replay", vec![]);
        let _ = draw.widget_button(replay, &text, true, inside(mouse, replay));
    } else {
        centred_text(
            draw,
            &open.question.en,
            [cx, mid - 48.0],
            [1.6, room],
            PROMPT,
        );
    }

    timer(draw, open, [x0, card_w, mid], now);

    for (index, option) in open.options.iter().enumerate() {
        let rect = layout.option(index);
        let text = label(teacher, index, option);
        let _ = draw.widget_button(rect, &text, true, inside(mouse, rect));
    }
}

/// Esc on a question: it is suspended and Escape closes the screen, after which the
/// `quiz_escape` mixin opens the pause menu.
pub(super) fn suspend(teacher: &mut Teacher, now: i64) {
    if teacher.suspend_question(now) {
        QUIZ_PAUSES.store(false, Ordering::Relaxed);
        ESCAPE_PAUSE.store(true, Ordering::Relaxed);
    }
}

/// The question's answer, for the next client tick to finish.
pub(super) fn answered(game: &mut Game, outcome: crate::core::Outcome) {
    QUIZ_PAUSES.store(false, Ordering::Relaxed);
    game.outcomes.push(outcome);
}

/// Keys 1 to 4 answer a choice question; Esc suspends it.
pub(crate) fn key_pressed(host: &dyn ClientHost, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
    let io = super::io();
    let io = &*io;
    if !io.our_screen(host) {
        return Flow::Continue;
    }
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_mut() else {
        return Flow::Continue;
    };
    let (key, modifiers) = (event.key, event.modifiers);
    if game.settings.is_some() {
        return settings::key_pressed(host, io, teacher, game, key, modifiers);
    }
    if let Some(page) = game.result.as_mut() {
        return page.key_pressed(host, io, key, modifiers);
    }
    if game.dictionary.is_some() {
        return dictionary::key_pressed(host, io, teacher, game, key, modifiers);
    }
    if game.guide.is_some() {
        return guide::key_pressed(host, io, teacher, game, key, modifiers);
    }
    if showing(teacher).is_some_and(|open| open.kind == QuestionKind::Typing) {
        return typing::key_pressed(host, io, teacher, game, key, modifiers, now);
    }
    let index = match key {
        "key.keyboard.escape" => {
            suspend(teacher, now);
            return Flow::Continue;
        }
        "key.keyboard.1" => 0,
        "key.keyboard.2" => 1,
        "key.keyboard.3" => 2,
        "key.keyboard.4" => 3,
        _ => return Flow::Continue,
    };
    if let Some(outcome) = teacher.choose(index, now) {
        answered(game, outcome);
    }
    Flow::Cancel
}

/// A typed character goes to the typed question's edit box.
pub(crate) fn char_typed(host: &dyn ClientHost, event: &mut ScreenCharTypedPreEvent<'_>) -> Flow {
    let io = super::io();
    let io = &*io;
    if !io.our_screen(host) {
        return Flow::Continue;
    }
    let now = io.now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_mut() else {
        return Flow::Continue;
    };
    if let Some(page) = game.dictionary.as_mut() {
        return page.char_typed(host, io, teacher, event.c);
    }
    if game.panel_open() || !showing(teacher).is_some_and(|open| open.kind == QuestionKind::Typing)
    {
        return Flow::Continue;
    }
    typing::char_typed(host, io, teacher, game, event.c, now)
}

/// A mouse button let go: a drag on the typed question's box, a settings slider or the
/// dictionary's list ends.
pub(crate) fn mouse_released(
    host: &dyn ClientHost,
    event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
) -> Flow {
    let io = super::io();
    if !io.our_screen(host) {
        return Flow::Continue;
    }
    let mut state = lock(&STATE);
    let game = &mut state.game;
    let input = if let Some(page) = game.settings.as_mut() {
        page.released(event.button)
    } else if let Some(page) = game.dictionary.as_mut() {
        page.released(event.button)
    } else if let Some(page) = game.result.as_mut() {
        page.released(event.button)
    } else if let Some(page) = game.guide.as_mut() {
        page.released(event.button)
    } else if let Some(typing) = game.typing.as_mut() {
        typing.released(event.button)
    } else {
        return Flow::Continue;
    };
    if input { Flow::Cancel } else { Flow::Continue }
}

/// The pointer moved with a button held: the box's selection, a settings slider or the
/// dictionary's list follows it.
pub(crate) fn mouse_dragged(
    host: &dyn ClientHost,
    event: &mut ScreenMouseDraggedPreEvent<'_>,
) -> Flow {
    let io = super::io();
    let io = &*io;
    if !io.our_screen(host) {
        return Flow::Continue;
    }
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_mut() else {
        return Flow::Continue;
    };
    let (at, button) = (event.screen.mouse, event.button);
    let taken = if game.settings.is_some() {
        settings::dragged(host, io, teacher, game, at, button)
    } else if let Some(page) = game.dictionary.as_mut() {
        page.dragged(host, io, at, button)
    } else if game.panel_open() {
        false
    } else if let Some(typing) = game.typing.as_mut() {
        typing.dragged(host, io, at, button)
    } else {
        false
    };
    if taken { Flow::Cancel } else { Flow::Continue }
}

/// A left click on an option answers it; on the replay button it plays the phrase again. The
/// dictionary takes the right and middle buttons too.
pub(crate) fn mouse_pressed(
    host: &dyn ClientHost,
    event: &mut ScreenMouseButtonPressedPreEvent<'_>,
) -> Flow {
    let io = super::io();
    let io = &*io;
    if !io.our_screen(host) {
        return Flow::Continue;
    }
    let now = io.now();
    let mouse = event.screen.mouse;
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_mut() else {
        return Flow::Continue;
    };
    if game.dictionary.is_some() {
        return dictionary::mouse_pressed(host, io, teacher, game, mouse, event.button);
    }
    if event.button != 0 {
        return Flow::Continue;
    }
    if game.settings.is_some() {
        return settings::mouse_pressed(host, io, teacher, game, mouse, event.modifiers);
    }
    if let Some(page) = game.result.as_mut() {
        return page.mouse_pressed(host, io, mouse, event.modifiers);
    }
    if game.guide.is_some() {
        return guide::mouse_pressed(host, io, teacher, game, mouse, event.modifiers);
    }
    let Some(open) = showing(teacher) else {
        return Flow::Continue;
    };
    if open.kind == QuestionKind::Typing {
        return typing::mouse_pressed(host, io, teacher, game, mouse, event.modifiers, now);
    }
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
        answered(game, outcome);
    }
    Flow::Cancel
}

/// The wheel over the screen: the exam's result or the dictionary's list scrolls.
pub(crate) fn mouse_scrolled(
    host: &dyn ClientHost,
    event: &mut ScreenMouseScrolledPreEvent<'_>,
) -> Flow {
    let io = super::io();
    if !io.our_screen(host) {
        return Flow::Continue;
    }
    let mut state = lock(&STATE);
    let game = &mut state.game;
    let dy = event.scroll[1];
    let taken = if let Some(page) = game.result.as_mut() {
        page.scrolled(dy)
    } else if let Some(page) = game.dictionary.as_mut() {
        page.scrolled(event.screen.mouse, dy)
    } else {
        false
    };
    if taken { Flow::Cancel } else { Flow::Continue }
}
