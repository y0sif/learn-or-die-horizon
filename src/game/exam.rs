//! The exam's result (Java `ExamResultScreen`), on this mod's screen when an exam is over: the
//! score as the title (green at 70% or more, else gold), then a row per question in the order
//! asked, a tick or a cross, the German, the translation and, under a wrong one, what was given.
//! The rows shrink to 85% before they scroll; the wheel scrolls them 18 px a notch, with marks
//! for more above or below. Done or Esc closes it; it pauses single player, as the original's.
//!
//! The chat line with the score and the exam's other lines come from the client tick
//! (`super::world_tick`).
use super::client::{Client, WidgetHost};
use super::hud::{fill, panel};
use super::screen::{centred_text, coloured, scaled};
use super::{log_warn, number};
use crate::core::{ExamResult, ExamRow};
use mod_api::client::widgets::{Button, Outcome as Input, WidgetId, Widgets};
use mod_api::{ClientHost, DrawContext, Flow, Text};

const ROW_H: f32 = 11.0;
const NOTE_H: f32 = 9.0;
const GAP: f32 = 2.0;
const CARD_W: f32 = 420.0;
const TOP: f32 = 10.0;
/// What a notch of the wheel scrolls.
const NOTCH: f64 = 18.0;
const CARD: u32 = 0xF20C_0C10;
const PASSED: u32 = 0xFF9B_E08A;
const SHORT: u32 = 0xFFFF_E08A;
const RIGHT: u32 = 0x9B_E08A;
const WRONG: u32 = 0xFF_7A7A;
const WHITE: u32 = 0xFF_FFFF;
const READING: u32 = 0xB0_E0FF;
const TRANSLATION: u32 = 0xBB_BBBB;
const NOTE: u32 = 0x9A_9A9A;

/// The result showing.
pub(crate) struct ResultScreen {
    result: ExamResult,
    /// How far the rows are scrolled, in GUI pixels, and how far they can be.
    scroll: i32,
    max_scroll: i32,
    widgets: Widgets,
    done: WidgetId,
    /// Done: the next client tick closes the screen.
    pub(crate) close: bool,
}

/// A row's line: the mark, the German in white, its reading, a long dash and the translation.
fn row_line(row: &ExamRow) -> (Text, String) {
    let (mark, colour) = if row.correct {
        ("\u{2713} ", RIGHT)
    } else {
        ("\u{2717} ", WRONG)
    };
    let phrase = &row.phrase;
    let mut plain = format!("{mark}{}", phrase.bare());
    let mut line = Text::literal("")
        .append(coloured(mark, colour))
        .append(coloured(phrase.bare(), WHITE));
    if !phrase.reading.is_empty() {
        let reading = format!("  {}", phrase.reading);
        plain.push_str(&reading);
        line = line.append(coloured(reading, READING));
    }
    let translation = format!("  \u{2014} {}", row.translation());
    plain.push_str(&translation);
    (line.append(coloured(translation, TRANSLATION)), plain)
}

/// `draw`: the line at `scale`, smaller when it would be wider than `max_w`.
fn fitted(
    draw: &mut dyn DrawContext,
    (line, plain): (Text, &str),
    at: [f32; 2],
    scale: f32,
    max_w: f32,
) {
    let s = scale.min(max_w / draw.text_width(plain).max(1.0));
    scaled(draw, line, at, s, 0xFFFF_FFFF);
}

impl ResultScreen {
    pub(crate) fn new(result: ExamResult) -> ResultScreen {
        let mut widgets = Widgets::new();
        let label = Text::translate("gui.done", vec![]);
        let done = widgets.add_button(Button::new([0.0; 4], label));
        ResultScreen {
            result,
            scroll: 0,
            max_scroll: 0,
            widgets,
            done,
            close: false,
        }
    }

    /// `extractRenderState`.
    pub(crate) fn render(
        &mut self,
        host: &dyn ClientHost,
        io: &dyn Client,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
    ) {
        let [w, h] = draw.size();
        io.blur(host);
        draw.draw_menu_background([0.0, 0.0, w, h], true);
        let card_w = (w - 24.0).min(CARD_W);
        let x0 = (w / 2.0).floor() - (card_w / 2.0).floor();
        let list_top = TOP + 30.0;
        let needed: f32 = self
            .result
            .rows
            .iter()
            .map(|row| ROW_H + if row.correct { 0.0 } else { NOTE_H } + GAP)
            .sum();
        let room = h - 38.0 - 6.0 - list_top;
        let s = (room / needed.max(1.0)).clamp(0.85, 1.0);
        let full = (needed * s).round();
        let list_h = room.min(full);
        self.max_scroll = (full - room).max(0.0) as i32;
        self.scroll = self.scroll.clamp(0, self.max_scroll);
        let bottom = list_top + list_h + 6.0;
        fill(draw, [x0, TOP, x0 + card_w, bottom], CARD);
        panel(draw, [x0, TOP, x0 + card_w, bottom], 255);
        let (right, total) = (self.result.right(), self.result.total());
        let title = Text::translate(
            "learnordie.exam.result_title",
            vec![number(right), number(total)],
        );
        let title = io.tr(host, &title);
        let colour = if self.result.passed() { PASSED } else { SHORT };
        centred_text(draw, &title, [w / 2.0, TOP + 8.0], [1.6, card_w], colour);
        let max_w = card_w - 20.0;
        let clip = [x0, list_top - 2.0, card_w, list_h + 4.0];
        let clipped = draw.push_clip(clip).is_ok();
        let mut y = list_top - self.scroll as f32;
        for row in &self.result.rows {
            let (line, plain) = row_line(row);
            fitted(draw, (line, &plain), [x0 + 10.0, y], s, max_w);
            y += ROW_H * s;
            if !row.correct {
                let note = match row.answer() {
                    Some(given) => {
                        Text::translate("learnordie.exam.your_answer", vec![Text::literal(given)])
                    }
                    None => Text::translate("learnordie.exam.no_answer", vec![]),
                };
                let plain = io.tr(host, &note);
                let mut note = note;
                note.style.color = Some(format!("#{NOTE:06X}"));
                fitted(draw, (note, &plain), [x0 + 22.0, y], s * 0.9, max_w - 12.0);
                y += NOTE_H * s;
            }
            y += GAP * s;
        }
        if clipped && let Err(e) = draw.pop_clip() {
            log_warn(format!("could not close the result's clip: {e:?}"));
        }
        if self.max_scroll > 0 {
            let up = if self.scroll > 0 { "\u{25B2}" } else { " " };
            let down = if self.scroll < self.max_scroll {
                "\u{25BC}"
            } else {
                " "
            };
            let marks = format!("{up}{down}");
            let marks_w = draw.text_width(&marks);
            let at = [x0 + card_w - 8.0 - marks_w, bottom - 11.0];
            draw.text(&marks, at, SHORT, true);
        }
        if let Some(done) = self.widgets.button_mut(self.done) {
            done.rect = [(w / 2.0).floor() - 60.0, h - 30.0, 120.0, 20.0];
        }
        if let Err(e) = self.widgets.draw(draw, &WidgetHost { io, host }, mouse) {
            log_warn(format!("could not draw the exam's result: {e:?}"));
        }
    }

    /// What a widget made of an input: Done closes.
    fn act(&mut self, input: Input) -> Flow {
        if input == Input::Pressed(self.done) {
            self.close = true;
        }
        if input.handled() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// Esc lets Escape close the screen; Tab, Enter and Space reach Done.
    pub(crate) fn key_pressed(
        &mut self,
        host: &dyn ClientHost,
        io: &dyn Client,
        key: &str,
        modifiers: u32,
    ) -> Flow {
        if key == "key.keyboard.escape" {
            return Flow::Continue;
        }
        let input = self
            .widgets
            .key_pressed(&WidgetHost { io, host }, key, modifiers);
        self.act(input)
    }

    pub(crate) fn mouse_pressed(
        &mut self,
        host: &dyn ClientHost,
        io: &dyn Client,
        at: [f32; 2],
        modifiers: u32,
    ) -> Flow {
        let input = self
            .widgets
            .mouse_pressed(&WidgetHost { io, host }, at, 0, modifiers);
        self.act(input)
    }

    pub(crate) fn released(&mut self, button: i32) -> bool {
        self.widgets.mouse_released(button).handled()
    }

    /// `mouseScrolled`: 18 px a notch, while the rows do not fit.
    pub(crate) fn scrolled(&mut self, dy: f64) -> bool {
        if self.max_scroll <= 0 {
            return false;
        }
        let by = (dy * NOTCH).round() as i32;
        self.scroll = (self.scroll - by).clamp(0, self.max_scroll);
        true
    }
}
