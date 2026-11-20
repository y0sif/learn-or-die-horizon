//! The typed question (Java `TypingScreen`), on this mod's screen: the picture, the English
//! prompt (or a replay button for a listening question), the timer bar, an edit box of up to 160
//! characters and a Check button, laid out as the original's; an exam's question has its header.
//!
//! The box is the SDK's text field, edited as the game's own `EditBox`; it takes every character
//! a chat line may hold, so ä ö ü ß too. Enter or keypad Enter submits wherever the focus is, and
//! so does Check; an empty box does nothing. For the first 400 ms every key and character is
//! ignored, Esc included. Esc suspends the question as for a choice (the pause menu opens, the
//! box's text is kept), and the time-out submits what the box holds. Every change of the box is
//! handed to the teacher, who keeps it for both.
use super::Game;
use super::client::{Client, WidgetHost};
use super::hud::panel;
use super::screen::{self, EDGE, GREY, PROMPT, centred_text, showing};
use crate::core::{OpenQuestion, Teacher};
use mod_api::client::widgets::{Button, Outcome as Input, TextField, WidgetId, Widgets};
use mod_api::{ClientHost, DrawContext, Flow, Text};

/// `setMaxLength(160)`.
const BOX_MAX: usize = 160;
const CARD_W: f32 = 344.0;
const BOX_W: f32 = 300.0;
const BUTTON_W: f32 = 120.0;
const BUTTON_H: f32 = 20.0;

/// The box and buttons of the typed question showing.
pub(crate) struct Typing {
    widgets: Widgets,
    field: WidgetId,
    check: WidgetId,
    replay: Option<WidgetId>,
}

impl Typing {
    /// `init`: the box with what was typed before (a question coming back after Esc), focused;
    /// the Check button; the replay button for a listening question.
    fn new(host: &dyn ClientHost, io: &dyn Client, open: &OpenQuestion) -> Typing {
        let mut widgets = Widgets::new();
        let mut field = TextField::new([0.0; 4], BOX_MAX);
        field.message = io.tr(host, &Text::translate("learnordie.quiz.answer", vec![]));
        field.set_value(open.typed());
        let field = widgets.add_text_field(field);
        let check = Text::translate("learnordie.quiz.check", vec![]);
        let check = widgets.add_button(Button::new([0.0; 4], check));
        let replay = open.listening.then(|| {
            let label = Text::translate("learnordie.quiz.replay", vec![]);
            widgets.add_button(Button::new([0.0; 4], label))
        });
        widgets.set_focus(&WidgetHost { io, host }, Some(field));
        Typing {
            widgets,
            field,
            check,
            replay,
        }
    }

    /// What the box holds.
    pub(crate) fn value(&self) -> &str {
        self.widgets
            .text_field(self.field)
            .map_or("", TextField::value)
    }

    /// The widgets where the original puts them for a screen of `size` with a card `card_w`
    /// wide: the box under the middle, Check under it, the replay button over the prompt.
    fn place(&mut self, [w, h]: [f32; 2], card_w: f32) {
        let (cx, mid) = ((w / 2.0).floor(), (h / 2.0).floor());
        let box_w = BOX_W.min(card_w - 44.0).max(1.0);
        if let Some(field) = self.widgets.text_field_mut(self.field) {
            field.rect = [cx - (box_w / 2.0).floor(), mid + 4.0, box_w, BUTTON_H];
        }
        let button = |y: f32| [cx - BUTTON_W / 2.0, y, BUTTON_W, BUTTON_H];
        if let Some(check) = self.widgets.button_mut(self.check) {
            check.rect = button(mid + 44.0);
        }
        if let Some(replay) = self.replay.and_then(|id| self.widgets.button_mut(id)) {
            replay.rect = button(mid - 54.0);
        }
    }

    /// A mouse button let go. True when a widget took it.
    pub(crate) fn released(&mut self, button: i32) -> bool {
        self.widgets.mouse_released(button).handled()
    }

    /// The pointer moved with a button held: the box's selection follows it.
    pub(crate) fn dragged(
        &mut self,
        host: &dyn ClientHost,
        io: &dyn Client,
        at: [f32; 2],
        button: i32,
    ) -> bool {
        let widgets_host = WidgetHost { io, host };
        self.widgets
            .mouse_dragged(&widgets_host, at, button)
            .handled()
    }
}

/// The question's box and buttons, made for it if they are not yet.
fn typing<'a>(
    game: &'a mut Game,
    host: &dyn ClientHost,
    io: &dyn Client,
    open: &OpenQuestion,
) -> &'a mut Typing {
    game.typing
        .get_or_insert_with(|| Typing::new(host, io, open))
}

/// The card's width in a screen `w` wide.
fn card_w(w: f32) -> f32 {
    CARD_W.min(w - EDGE * 2.0)
}

/// `extractRenderState`.
pub(super) fn render(
    host: &dyn ClientHost,
    io: &dyn Client,
    draw: &mut dyn DrawContext,
    teacher: &Teacher,
    game: &mut Game,
    mouse: [f32; 2],
    now: i64,
) {
    let Some(open) = showing(teacher) else {
        return;
    };
    let size = draw.size();
    game.screen_size = size;
    let [w, h] = size;
    let (cx, mid) = (w / 2.0, (h / 2.0).floor());
    draw.fill_gradient([0.0, 0.0, w, h], 0xC010_1010, 0xD010_1010);
    let card_w = card_w(w);
    let x0 = (cx - card_w / 2.0).round();
    panel(draw, [x0, mid - 130.0, x0 + card_w, mid + 74.0], 255);
    screen::picture(draw, host, game, open, [cx, mid]);
    screen::exam_header(draw, teacher, [x0, mid - 130.0], now);
    let title = if open.listening {
        Text::translate("learnordie.quiz.type_heard", vec![])
    } else {
        let language = Text::literal(teacher.pack.meta.name.clone());
        Text::translate("learnordie.quiz.type", vec![language])
    };
    let title = io.tr(host, &title);
    centred_text(draw, &title, [cx, mid - 62.0], [1.0, card_w - 24.0], GREY);
    if !open.listening {
        let prompt = &open.question.en;
        centred_text(draw, prompt, [cx, mid - 48.0], [1.6, card_w - 24.0], PROMPT);
    }
    screen::timer(draw, open, [x0, card_w, mid], now);
    let typing = typing(game, host, io, open);
    typing.place(size, card_w);
    if let Err(e) = typing.widgets.draw(draw, &WidgetHost { io, host }, mouse) {
        super::log_warn(format!("could not draw the typed question: {e:?}"));
    }
}

/// Enter or Check: the box is judged, unless it is blank or the lockout runs.
fn submit(teacher: &mut Teacher, game: &mut Game, text: &str, now: i64) {
    teacher.set_typed(text);
    if let Some(outcome) = teacher.submit_typed(now, text) {
        screen::answered(game, outcome);
    }
}

/// What a widget made of an input: the box's new text is handed on, Check submits, the replay
/// button plays the phrase again. True when a widget took it.
fn act(teacher: &mut Teacher, game: &mut Game, input: Input, now: i64) -> bool {
    let Some(typing) = game.typing.as_ref() else {
        return false;
    };
    let (field, check, replay) = (typing.field, typing.check, typing.replay);
    let text = typing.value().to_string();
    match input {
        Input::Changed(id) if id == field => teacher.set_typed(&text),
        Input::Submitted(id) if id == field => submit(teacher, game, &text, now),
        Input::Pressed(id) if id == check => submit(teacher, game, &text, now),
        Input::Pressed(id) if Some(id) == replay => teacher.replay_question(now),
        _ => {}
    }
    input.handled()
}

/// `keyPressed`: nothing in the lockout; Esc suspends; Enter and keypad Enter submit; every
/// other key goes to the box or the focused button.
pub(super) fn key_pressed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    key: &str,
    modifiers: u32,
    now: i64,
) -> Flow {
    let Some(open) = showing(teacher).cloned() else {
        return Flow::Continue;
    };
    if !open.accepts_input(now) {
        return Flow::Cancel;
    }
    match key {
        "key.keyboard.escape" => {
            screen::suspend(teacher, now);
            game.typing = None;
            return Flow::Continue;
        }
        "key.keyboard.enter" | "key.keyboard.keypad.enter" => {
            let text = typing(game, host, io, &open).value().to_string();
            submit(teacher, game, &text, now);
            return Flow::Cancel;
        }
        _ => {}
    }
    let input =
        typing(game, host, io, &open)
            .widgets
            .key_pressed(&WidgetHost { io, host }, key, modifiers);
    if act(teacher, game, input, now) {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// `charTyped`: nothing in the lockout, else the box takes it.
pub(super) fn char_typed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    c: char,
    now: i64,
) -> Flow {
    let Some(open) = showing(teacher).cloned() else {
        return Flow::Continue;
    };
    if !open.accepts_input(now) {
        return Flow::Cancel;
    }
    let input = typing(game, host, io, &open)
        .widgets
        .char_typed(&WidgetHost { io, host }, c);
    if act(teacher, game, input, now) {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// A left click: the box takes the focus and the caret, a button is pressed (with the widgets'
/// own click, `AbstractButton.playDownSound`). Clicks are not locked out (the original locks
/// keys and characters only); Check still is, by the teacher.
pub(super) fn mouse_pressed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    at: [f32; 2],
    modifiers: u32,
    now: i64,
) -> Flow {
    let Some(open) = showing(teacher).cloned() else {
        return Flow::Continue;
    };
    let input = typing(game, host, io, &open).widgets.mouse_pressed(
        &WidgetHost { io, host },
        at,
        0,
        modifiers,
    );
    if act(teacher, game, input, now) {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}
