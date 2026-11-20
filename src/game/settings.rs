//! The settings pages (Java `NarratorSettingsScreen`, `NarratorAdvancedScreen`,
//! `NarratorTopicsScreen`), on this mod's screen: I opens the main page, whose last button leads
//! to the advanced page, whose last leads to the topics.
//!
//! The rows are the core's ([`crate::core::settings`]), laid out as the original's options list:
//! two columns of `Label: Value` buttons, a row the original gives the whole width on a line of
//! its own, the page button under them and Done in the footer. A click on a switch or a choice
//! moves it on (Shift moves it back); a slider takes the value under the pointer and follows a
//! drag, and the arrow keys move a focused slider a step. Every change is saved to
//! `config/learnordie.json` at once and the teacher reads it from there on; a new translation
//! language loads the pack again, as the original reloads its resources.
//!
//! Esc or Done goes back a page; on the main page it closes the screen. Like an options screen,
//! a page pauses single player and blurs the world and the HUD behind it.
use super::client::{Client, View, WidgetHost};
use super::screen::inside;
use super::{Game, files, keys};
pub(crate) use crate::core::Page;
use crate::core::{Config, Pack, Setting, Shown, Teacher, settings as rows};
use mod_api::client::widgets::{Button, Outcome as Input, WidgetId, Widgets};
use mod_api::{ClientHost, DrawContext, Flow, Text};

/// A small button's width (`Button.SMALL_WIDTH`), and the room between the two columns.
const SMALL_W: f32 = 150.0;
const GAP: f32 = 10.0;
/// Kept free at the screen's sides.
const SIDE: f32 = 10.0;
const BUTTON_H: f32 = 20.0;
/// The first row's top, under the title, and the rows' pitch (shrunk to fit a short window).
const TOP: f32 = 32.0;
const PITCH: f32 = 24.0;
const MIN_PITCH: f32 = 21.0;
/// Done in the footer, from the bottom.
const FOOTER: f32 = 27.0;
const DONE_W: f32 = 200.0;
const WHITE: u32 = 0xFFFF_FFFF;
/// Shift in the screen hooks' modifiers.
const SHIFT: u32 = 1;
const SLIDER: &str = "gui/sprites/widget/slider";
const SLIDER_FOCUSED: &str = "gui/sprites/widget/slider_highlighted";
const HANDLE: &str = "gui/sprites/widget/slider_handle";
const HANDLE_HOVERED: &str = "gui/sprites/widget/slider_handle_highlighted";

/// The rows the original adds at full width (`addBig`). `enabled` shared its line with the
/// language row, which the port does not have.
fn is_wide(name: &str) -> bool {
    matches!(name, "enabled" | "hint_language" | "english" | "backdrop")
}

/// The page a page's last button leads to, and the one Esc and Done go back to.
fn next(page: Page) -> Option<Page> {
    match page {
        Page::Main => Some(Page::Advanced),
        Page::Advanced => Some(Page::Topics),
        Page::Topics => None,
    }
}

fn parent(page: Page) -> Option<Page> {
    match page {
        Page::Main => None,
        Page::Advanced => Some(Page::Main),
        Page::Topics => Some(Page::Advanced),
    }
}

fn title(page: Page) -> &'static str {
    match page {
        Page::Main => "learnordie.options.title",
        Page::Advanced => "learnordie.options.advanced_title",
        Page::Topics => "learnordie.options.topics_title",
    }
}

/// The rows' lines: each row index alone or with the next one.
fn lines(rows: &[Setting]) -> Vec<Vec<usize>> {
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let pairs = lines
            .last()
            .is_some_and(|line| line.len() == 1 && !is_wide(rows[line[0]].name));
        if !is_wide(row.name) && pairs {
            if let Some(line) = lines.last_mut() {
                line.push(index);
            }
        } else {
            lines.push(vec![index]);
        }
    }
    lines
}

/// A row's label: `<name>: <value>` (`options.generic_value`, as `CycleButton` and the original's
/// sliders show it).
fn label(row: &Setting, config: &Config) -> Text {
    let name = Text::translate(format!("learnordie.options.{}", row.name), vec![]);
    let value = match row.shown(config) {
        Shown::Flag(on) => Text::translate(if on { "options.on" } else { "options.off" }, vec![]),
        Shown::Choice(value) => keys::value_text(&value),
        Shown::Text(text) => Text::literal(text),
        Shown::Key("auto") => Text::translate("learnordie.options.hint_language.auto", vec![]),
        Shown::Key(name) => Text::translate(format!("learnordie.value.{name}"), vec![]),
        Shown::Rest(minutes) => Text::translate(
            "learnordie.value.rest",
            vec![Text::literal(minutes.to_string())],
        ),
    };
    Text::translate("options.generic_value", vec![name, value])
}

/// `OptionInstance.IntRange.toSliderValue`: where a slider's handle stands.
fn handle_at(position: usize, positions: usize) -> f32 {
    let last = positions.saturating_sub(1);
    if position == 0 || last == 0 {
        0.0
    } else if position >= last {
        1.0
    } else {
        (position as f32 + 0.5) / positions as f32
    }
}

/// `AbstractSliderButton.setValueFromMouse`, then `IntRange.fromSliderValue`: the position under
/// the pointer at `x` on a slider at `rect`.
fn position_at(x: f32, [rx, _, rw, _]: [f32; 4], positions: usize) -> usize {
    let value = ((x - (rx + 4.0)) / (rw - 8.0)).clamp(0.0, 1.0);
    let last = positions.saturating_sub(1);
    if value >= 1.0 {
        last
    } else {
        ((value * positions as f32).floor() as usize).min(last)
    }
}

/// A settings page showing.
pub(crate) struct Settings {
    page: Page,
    rows: Vec<Setting>,
    widgets: Widgets,
    /// Each row's button, in the rows' order, then the page button and Done.
    buttons: Vec<WidgetId>,
    link: Option<(WidgetId, Page)>,
    done: WidgetId,
    /// Each button's place when last drawn.
    rects: Vec<[f32; 4]>,
    /// The slider row a drag holds.
    dragging: Option<usize>,
    /// Done or Esc on the main page: the next client tick closes the screen.
    close: bool,
}

impl Settings {
    pub(crate) fn new(page: Page, pack: &Pack) -> Settings {
        let rows = rows(page, pack);
        let mut widgets = Widgets::new();
        let blank = || Button::new([0.0; 4], Text::literal(""));
        let buttons = rows.iter().map(|_| widgets.add_button(blank())).collect();
        let link = next(page).map(|to| {
            let key = match to {
                Page::Topics => "learnordie.options.topics",
                _ => "learnordie.options.advanced",
            };
            let label = Text::translate(key, vec![]);
            (widgets.add_button(Button::new([0.0; 4], label)), to)
        });
        let done = widgets.add_button(Button::new([0.0; 4], Text::translate("gui.done", vec![])));
        Settings {
            page,
            rows,
            widgets,
            buttons,
            link,
            done,
            rects: Vec::new(),
            dragging: None,
            close: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn page(&self) -> Page {
        self.page
    }

    /// The buttons where they go in a screen of `size`, each label showing its value.
    fn place(&mut self, [w, h]: [f32; 2], config: &Config) {
        let lines = lines(&self.rows);
        let count = lines.len() + usize::from(self.link.is_some());
        let small = SMALL_W.min(((w - SIDE * 2.0 - GAP) / 2.0).floor()).max(1.0);
        let wide = small * 2.0 + GAP;
        let left = (w / 2.0).floor() - small - GAP / 2.0;
        let done_y = h - FOOTER;
        let room = done_y - 4.0 - TOP;
        let pitch = (room / count.max(1) as f32).floor().clamp(MIN_PITCH, PITCH);
        let mut rects = vec![[0.0; 4]; self.rows.len()];
        for (line, rows) in lines.iter().enumerate() {
            let y = TOP + line as f32 * pitch;
            match rows.as_slice() {
                [a, b] => {
                    rects[*a] = [left, y, small, BUTTON_H];
                    rects[*b] = [left + small + GAP, y, small, BUTTON_H];
                }
                [a] => rects[*a] = [left, y, wide, BUTTON_H],
                _ => {}
            }
        }
        for (index, rect) in rects.iter().enumerate() {
            let text = label(&self.rows[index], config);
            if let Some(button) = self.widgets.button_mut(self.buttons[index]) {
                button.rect = *rect;
                button.label = text;
            }
        }
        if let Some((id, _)) = self.link
            && let Some(button) = self.widgets.button_mut(id)
        {
            button.rect = [left, TOP + lines.len() as f32 * pitch, wide, BUTTON_H];
            rects.push(button.rect);
        }
        let done_w = DONE_W.min(w - SIDE * 2.0);
        if let Some(button) = self.widgets.button_mut(self.done) {
            button.rect = [
                (w / 2.0).floor() - (done_w / 2.0).floor(),
                done_y,
                done_w,
                BUTTON_H,
            ];
            rects.push(button.rect);
        }
        self.rects = rects;
    }

    /// The row a widget is, if it is one.
    fn row_of(&self, id: WidgetId) -> Option<usize> {
        self.buttons.iter().position(|button| *button == id)
    }

    /// A mouse button let go: a drag ends.
    pub(crate) fn released(&mut self, button: i32) -> bool {
        let dragging = self.dragging.take().is_some();
        self.widgets.mouse_released(button).handled() || dragging
    }
}

/// From the client tick: a page closed by Esc is forgotten, and one Done or Esc left is closed.
pub(crate) fn tick(host: &mut dyn ClientHost, io: &dyn Client, game: &mut Game, view: &View) {
    if !view.our_screen {
        game.settings = None;
    } else if game.settings.as_ref().is_some_and(|page| page.close) {
        io.close_screen(host);
        game.settings = None;
    }
}

/// A row's value changed: saved at once; a new translation language loads the pack again on the
/// next client tick (the teacher keeps his config and progress).
fn changed(host: &dyn ClientHost, io: &dyn Client, teacher: &Teacher, game: &mut Game, row: &str) {
    files::save_config(host, io, &teacher.config);
    if row == "hint_language" {
        game.loaded = None;
    }
}

/// Moves row `index` with `step`: a click, Shift-click, an arrow key, or the pointer's position
/// on a slider. Saved when the value changed, which a value between a slider's steps does on a
/// click at the step it shows at.
fn set(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    index: usize,
    step: Step,
) {
    let Some(page) = game.settings.as_ref() else {
        return;
    };
    let row = page.rows[index].clone();
    let before = teacher.config.clone();
    match step {
        Step::By(delta) => row.cycle(&mut teacher.config, delta),
        Step::To(position) => row.set_position(&mut teacher.config, position),
    }
    if teacher.config != before {
        changed(host, io, teacher, game, row.name);
    }
}

#[derive(Clone, Copy)]
enum Step {
    By(i32),
    To(usize),
}

/// Goes back a page; from the main page, the screen closes. Done and Esc.
fn back(teacher: &Teacher, game: &mut Game) {
    let Some(page) = game.settings.as_mut() else {
        return;
    };
    match parent(page.page) {
        Some(to) => *page = Settings::new(to, &teacher.pack),
        None => page.close = true,
    }
}

/// The page: the world and the HUD blurred and the in-world menu background over them
/// (`Screen.renderBackground` in a world, as the original's `OptionsSubScreen` has it), the title,
/// the buttons, and over each slider row the slider's track, handle and label (the buttons stay
/// under them for the narrator, a controller and horizon-ctl's `ui`).
pub(super) fn render(
    host: &dyn ClientHost,
    io: &dyn Client,
    draw: &mut dyn DrawContext,
    teacher: &Teacher,
    game: &mut Game,
    mouse: [f32; 2],
) {
    let Some(page) = game.settings.as_mut() else {
        return;
    };
    let size = draw.size();
    let [w, h] = size;
    io.blur(host);
    draw.draw_menu_background([0.0, 0.0, w, h], true);
    let title = io.tr(host, &Text::translate(title(page.page), vec![]));
    let title_w = draw.text_width(&title);
    draw.text(&title, [((w - title_w) / 2.0).floor(), 13.0], WHITE, true);
    page.place(size, &teacher.config);
    let widgets_host = WidgetHost { io, host };
    if let Err(e) = page.widgets.draw(draw, &widgets_host, mouse) {
        super::log_warn(format!("could not draw the settings: {e:?}"));
    }
    let focused = page.widgets.focused();
    for (index, row) in page.rows.iter().enumerate() {
        let Some(&rect) = page.rects.get(index).filter(|_| row.is_slider()) else {
            continue;
        };
        let [x, y, rw, rh] = rect;
        let focus = focused == Some(page.buttons[index]);
        let held = page.dragging == Some(index);
        let track = if focus && !held {
            SLIDER_FOCUSED
        } else {
            SLIDER
        };
        draw.gui_nine_slice(track, rect, WHITE);
        let at = handle_at(row.position(&teacher.config), row.positions());
        let handle = if inside(mouse, rect) || held {
            HANDLE_HOVERED
        } else {
            HANDLE
        };
        let handle_x = x + (at * (rw - 8.0)).floor();
        draw.gui_nine_slice(handle, [handle_x, y, 8.0, rh], WHITE);
        let text = io.tr(host, &label(row, &teacher.config));
        let text_w = draw.text_width(&text);
        draw.text(
            &text,
            [x + ((rw - text_w) / 2.0).floor(), y + 6.0],
            WHITE,
            true,
        );
    }
}

/// Esc goes back a page (the main page lets Escape close the screen); the arrows move a focused
/// slider; the other keys go to the buttons (Tab moves the focus, Enter or Space presses).
pub(super) fn key_pressed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    key: &str,
    modifiers: u32,
) -> Flow {
    let Some(page) = game.settings.as_mut() else {
        return Flow::Continue;
    };
    if key == "key.keyboard.escape" {
        if parent(page.page).is_none() {
            return Flow::Continue;
        }
        back(teacher, game);
        return Flow::Cancel;
    }
    let slider = page
        .widgets
        .focused()
        .and_then(|id| page.row_of(id))
        .filter(|&index| page.rows[index].is_slider());
    let arrow = match key {
        "key.keyboard.left" => Some(-1),
        "key.keyboard.right" => Some(1),
        _ => None,
    };
    if let (Some(index), Some(delta)) = (slider, arrow) {
        set(host, io, teacher, game, index, Step::By(delta));
        return Flow::Cancel;
    }
    let input = page
        .widgets
        .key_pressed(&WidgetHost { io, host }, key, modifiers);
    if let Input::Pressed(id) = input {
        press(host, io, teacher, game, id, None, modifiers);
    }
    if input.handled() {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// A left click on a button: see [`press`].
pub(super) fn mouse_pressed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    at: [f32; 2],
    modifiers: u32,
) -> Flow {
    let Some(page) = game.settings.as_mut() else {
        return Flow::Continue;
    };
    let input = page
        .widgets
        .mouse_pressed(&WidgetHost { io, host }, at, 0, modifiers);
    if let Input::Pressed(id) = input {
        press(host, io, teacher, game, id, Some(at), modifiers);
    }
    if input.handled() {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// A button pressed: a switch or a choice moves on (back with Shift), a slider clicked takes the
/// value under the pointer and holds the drag (Enter on a slider does nothing), the page button
/// opens its page and Done goes back.
fn press(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    id: WidgetId,
    at: Option<[f32; 2]>,
    modifiers: u32,
) {
    let Some(page) = game.settings.as_mut() else {
        return;
    };
    if id == page.done {
        back(teacher, game);
        return;
    }
    if let Some((link, to)) = page.link
        && id == link
    {
        *page = Settings::new(to, &teacher.pack);
        return;
    }
    let Some(index) = page.row_of(id) else {
        return;
    };
    let row = &page.rows[index];
    if !row.is_slider() {
        let delta = if modifiers & SHIFT != 0 { -1 } else { 1 };
        set(host, io, teacher, game, index, Step::By(delta));
        return;
    }
    let (Some([x, _]), Some(&rect)) = (at, page.rects.get(index)) else {
        return;
    };
    let position = position_at(x, rect, row.positions());
    page.dragging = Some(index);
    set(host, io, teacher, game, index, Step::To(position));
}

/// The pointer moved with the left button held on a slider: the value follows it.
pub(super) fn dragged(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &mut Teacher,
    game: &mut Game,
    at: [f32; 2],
    button: i32,
) -> bool {
    let Some(page) = game.settings.as_mut() else {
        return false;
    };
    let Some(index) = page.dragging.filter(|_| button == 0) else {
        return page
            .widgets
            .mouse_dragged(&WidgetHost { io, host }, at, button)
            .handled();
    };
    let Some(&rect) = page.rects.get(index) else {
        return false;
    };
    let position = position_at(at[0], rect, page.rows[index].positions());
    set(host, io, teacher, game, index, Step::To(position));
    true
}
