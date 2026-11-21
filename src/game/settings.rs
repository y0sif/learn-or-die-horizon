//! The settings pages (Java `NarratorSettingsScreen`, `NarratorAdvancedScreen`,
//! `NarratorTopicsScreen`, `ExamScreen`), on this mod's screen: I opens the main page, whose last
//! buttons lead to the exam page, the advanced page (whose last leads to the topics) and the
//! guide.
//!
//! The rows are the core's ([`crate::core::settings`]), laid out as the original's options list:
//! two columns of `Label: Value` buttons, a row the original gives the whole width on a line of
//! its own, the page buttons under them and Done in the footer. A click on a switch or a choice
//! moves it on (Shift moves it back); a slider takes the value under the pointer and follows a
//! drag, and the arrow keys move a focused slider a step. Every change is saved to
//! `config/learnordie.json` at once and the teacher reads it from there on; a new translation
//! language loads the pack again, as the original reloads its resources.
//!
//! The exam page ends with Start, which saves the config, starts an exam whose questions begin at
//! once and closes the screen, or Stop while one runs, which drops it, turns `examAuto` off and
//! says so in chat. Not in a world, the button is off.
//!
//! A page with more lines than fit above Done scrolls a line at a time, as the original's options
//! list scrolls: by the wheel, by its scrollbar (a click or a drag on it), and to the button Tab
//! moves to.
//!
//! Esc or Done goes back a page; on the main page it closes the screen. Like an options screen,
//! a page pauses single player and blurs the world and the HUD behind it.
use super::client::{Client, WidgetHost};
use super::dictionary::wrap;
use super::guide::GuideScreen;
use super::screen::{inside, scrollbar};
use super::{Game, files, keys, log_info, number, teacher_says};
pub(crate) use crate::core::Page;
use crate::core::{Config, ExamStage, Pack, Setting, Shown, Teacher, exam_link, settings as rows};
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
/// A line scrolled out of the page: its buttons wait far above the screen, where nothing shows
/// them, no click reaches them and horizon-ctl's `ui` does not list them.
const HIDDEN_Y: f32 = -1000.0;
/// The scrollbar's width, and the gap between it and the rows.
const BAR_W: f32 = 6.0;
const BAR_GAP: f32 = 6.0;
/// A tooltip's lines wrap at this width (`Tooltip.MAX_WIDTH`).
const TOOLTIP_W: f32 = 170.0;
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

/// What a button under a page's rows does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link {
    /// Opens that page.
    Page(Page),
    /// Opens the guide; closing it comes back to the main page.
    Guide,
    /// The exam page's Start or Stop.
    Exam,
}

/// The buttons under a page's rows, each on a line of its own: the main page's exam page,
/// advanced page and guide, the advanced page's topics, the exam page's Start or Stop.
fn links(page: Page) -> Vec<Link> {
    match page {
        Page::Main => vec![
            Link::Page(Page::Exam),
            Link::Page(Page::Advanced),
            Link::Guide,
        ],
        Page::Advanced => vec![Link::Page(Page::Topics)],
        Page::Topics => Vec::new(),
        Page::Exam => vec![Link::Exam],
    }
}

/// How many questions an exam asks at most, for the Start button's tooltip.
const EXAM_QUESTIONS: usize = 20;

fn parent(page: Page) -> Option<Page> {
    match page {
        Page::Main => None,
        Page::Advanced => Some(Page::Main),
        Page::Topics => Some(Page::Advanced),
        Page::Exam => Some(Page::Main),
    }
}

fn title(page: Page) -> &'static str {
    match page {
        Page::Main => "learnordie.options.title",
        Page::Advanced => "learnordie.options.advanced_title",
        Page::Topics => "learnordie.options.topics_title",
        Page::Exam => "learnordie.options.exam_title",
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

/// How a page's lines stood when last placed.
#[derive(Clone, Copy, Debug, Default)]
struct Window {
    /// Every line: the rows' and the links'.
    lines: usize,
    /// How many fit above Done.
    fit: usize,
    pitch: f32,
    /// The scrollbar's left edge.
    bar_x: f32,
}

impl Window {
    fn overflows(&self) -> bool {
        self.lines > self.fit
    }

    /// The last line the page can scroll to show first.
    fn last(&self) -> usize {
        self.lines.saturating_sub(self.fit)
    }

    /// The scrollbar's height: from the first line shown to the end of the last.
    fn track_h(&self) -> f32 {
        self.fit.saturating_sub(1) as f32 * self.pitch + BUTTON_H
    }

    fn thumb_h(&self) -> f32 {
        let h = self.track_h() * self.fit as f32 / self.lines.max(1) as f32;
        h.floor().max(12.0)
    }
}

/// A settings page showing.
pub(crate) struct Settings {
    page: Page,
    rows: Vec<Setting>,
    widgets: Widgets,
    /// Each row's button, in the rows' order.
    buttons: Vec<WidgetId>,
    /// The buttons under the rows.
    links: Vec<(WidgetId, Link)>,
    done: WidgetId,
    /// Each button's place when last drawn.
    rects: Vec<[f32; 4]>,
    /// The slider row a drag holds.
    dragging: Option<usize>,
    /// The first line shown, and how the lines stood when last placed.
    scroll: usize,
    window: Window,
    /// The scrollbar is held.
    bar_held: bool,
    /// Done or Esc on the main page: the next client tick closes the screen.
    close: bool,
}

impl Settings {
    pub(crate) fn new(page: Page, pack: &Pack) -> Settings {
        let rows = rows(page, pack);
        let mut widgets = Widgets::new();
        let blank = || Button::new([0.0; 4], Text::literal(""));
        let buttons = rows.iter().map(|_| widgets.add_button(blank())).collect();
        let links = links(page)
            .into_iter()
            .map(|link| (widgets.add_button(blank()), link))
            .collect();
        let done = widgets.add_button(Button::new([0.0; 4], Text::translate("gui.done", vec![])));
        Settings {
            page,
            rows,
            widgets,
            buttons,
            links,
            done,
            rects: Vec::new(),
            dragging: None,
            scroll: 0,
            window: Window::default(),
            bar_held: false,
            close: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn page(&self) -> Page {
        self.page
    }

    /// Done or Esc on the main page, or the exam's Start: the next client tick closes the screen.
    pub(crate) fn closing(&self) -> bool {
        self.close
    }

    /// The buttons where they go in a screen of `size`, each label showing its value; the exam's
    /// Start or Stop as the exam stands, off outside a world. The lines that do not fit above
    /// Done at the smallest pitch scroll: those before `scroll` and after the last that fits wait
    /// at [`HIDDEN_Y`].
    fn place(&mut self, [w, h]: [f32; 2], teacher: &Teacher, now: i64, in_world: bool) {
        let config = &teacher.config;
        let lines = lines(&self.rows);
        let count = lines.len() + self.links.len();
        let small = SMALL_W.min(((w - SIDE * 2.0 - GAP) / 2.0).floor()).max(1.0);
        let wide = small * 2.0 + GAP;
        let left = (w / 2.0).floor() - small - GAP / 2.0;
        let done_y = h - FOOTER;
        let room = done_y - 4.0 - TOP;
        let pitch = (room / count.max(1) as f32).floor().clamp(MIN_PITCH, PITCH);
        let fit = ((room - BUTTON_H) / pitch).floor().max(0.0) as usize + 1;
        self.window = Window {
            lines: count,
            fit,
            pitch,
            bar_x: (left + wide + BAR_GAP).min(w - BAR_W - 1.0),
        };
        self.scroll = self.scroll.min(self.window.last());
        let first = self.scroll;
        let line_y = |line: usize| match line.checked_sub(first) {
            Some(shown) if shown < fit => TOP + shown as f32 * pitch,
            _ => HIDDEN_Y,
        };
        let mut rects = vec![[0.0; 4]; self.rows.len()];
        for (line, rows) in lines.iter().enumerate() {
            let y = line_y(line);
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
        for (index, &(id, link)) in self.links.iter().enumerate() {
            let (label, active) = link_label(link, teacher, now, in_world);
            if let Some(button) = self.widgets.button_mut(id) {
                button.rect = [left, line_y(lines.len() + index), wide, BUTTON_H];
                button.label = label;
                button.active = active;
                rects.push(button.rect);
            }
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

    /// The line a widget stands on: a row's or a link's (Done stays in the footer).
    fn line_of(&self, id: WidgetId) -> Option<usize> {
        let lines = lines(&self.rows);
        if let Some(index) = self.row_of(id) {
            return lines.iter().position(|line| line.contains(&index));
        }
        let link = self.links.iter().position(|(link, _)| *link == id)?;
        Some(lines.len() + link)
    }

    /// The focused button's line scrolled into view, as `AbstractSelectionList` scrolls to the
    /// entry Tab moves to.
    fn reveal(&mut self) {
        let fit = self.window.fit;
        let Some(line) = self.widgets.focused().and_then(|id| self.line_of(id)) else {
            return;
        };
        if fit == 0 {
            return;
        }
        if line < self.scroll {
            self.scroll = line;
        } else if line >= self.scroll + fit {
            self.scroll = line + 1 - fit;
        }
    }

    fn on_scrollbar(&self, [x, y]: [f32; 2]) -> bool {
        let w = &self.window;
        w.overflows()
            && x >= w.bar_x - 2.0
            && x <= w.bar_x + BAR_W + 2.0
            && y >= TOP - 2.0
            && y <= TOP + w.track_h() + 2.0
    }

    /// The thumb's middle follows the pointer.
    fn scroll_to(&mut self, y: f32) {
        let w = self.window;
        let range = w.track_h() - w.thumb_h();
        let f = if range <= 0.0 {
            0.0
        } else {
            (y - TOP - w.thumb_h() / 2.0) / range
        };
        self.scroll = (f.clamp(0.0, 1.0) * w.last() as f32).round() as usize;
    }

    /// The wheel moves a page that overflows a line a notch.
    pub(crate) fn scrolled(&mut self, dy: f64) -> bool {
        if !self.window.overflows() {
            return false;
        }
        if dy != 0.0 {
            let by = -(dy.signum() as i64);
            let to = (self.scroll as i64 + by).clamp(0, self.window.last() as i64);
            self.scroll = to as usize;
        }
        true
    }

    /// The scrollbar, when the page overflows (as [`super::dictionary`] draws its list's).
    fn draw_scrollbar(&self, draw: &mut dyn DrawContext) {
        let w = self.window;
        if !w.overflows() {
            return;
        }
        let (track, thumb) = (w.track_h(), w.thumb_h());
        let at = (track - thumb) * self.scroll as f32 / w.last() as f32;
        let track = [w.bar_x, TOP, BAR_W, track];
        scrollbar(draw, track, [TOP + at.floor(), thumb], self.bar_held);
    }

    /// A mouse button let go: a drag ends.
    pub(crate) fn released(&mut self, button: i32) -> bool {
        let dragging = self.dragging.take().is_some();
        let bar = std::mem::take(&mut self.bar_held) && button == 0;
        self.widgets.mouse_released(button).handled() || dragging || bar
    }
}

/// A link's label, and whether it can be pressed: the exam page's as the exam stands
/// (`ExamScreen.addOptions`), off outside a world.
fn link_label(link: Link, teacher: &Teacher, now: i64, in_world: bool) -> (Text, bool) {
    let key = |key: &str| Text::translate(key, vec![]);
    match link {
        Link::Page(Page::Exam) => match exam_link(&teacher.config) {
            Some(minutes) => (
                Text::translate("learnordie.options.exam.on", vec![Text::literal(minutes)]),
                true,
            ),
            None => (key("learnordie.options.exam.off"), true),
        },
        Link::Page(Page::Topics) => (key("learnordie.options.topics"), true),
        Link::Page(_) => (key("learnordie.options.advanced"), true),
        Link::Guide => (key("learnordie.options.guide"), true),
        Link::Exam if !in_world => (key("learnordie.exam.need_world"), false),
        Link::Exam => match teacher.exam(now) {
            ExamStage::Off => (key("learnordie.exam.start"), true),
            ExamStage::Playing { left } => (
                Text::translate("learnordie.exam.stop", vec![Text::literal(left)]),
                true,
            ),
            ExamStage::Asking { .. } => (key("learnordie.exam.stop_asking"), true),
        },
    }
}

/// The exam page's Start or Stop (`ExamScreen`'s button): Start saves the config, starts an
/// exam whose questions begin at once and closes the screen; Stop drops the running exam, turns
/// `examAuto` off, saves it, says so in chat and shows the page again.
fn exam_button(host: &dyn ClientHost, io: &dyn Client, teacher: &mut Teacher, game: &mut Game) {
    if game.level.is_none() {
        return;
    }
    let now = io.now();
    if teacher.exam(now) == ExamStage::Off {
        files::save_config(host, io, &teacher.config);
        teacher.start_exam(now);
        log_info("exam started by the player".into());
        if let Some(page) = game.settings.as_mut() {
            page.close = true;
        }
    } else {
        teacher.stop_exam();
        files::save_config(host, io, &teacher.config);
        log_info("exam stopped by the player".into());
        game.notices
            .push(teacher_says("learnordie.exam.stopped", vec![]));
        game.settings = Some(Settings::new(Page::Exam, &teacher.pack));
    }
}

/// A row's value changed: saved at once; a new translation language loads the pack again on the
/// next client tick (the teacher goes on where he stood: config, progress, quiz and exam).
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
    page.place(size, teacher, io.now(), game.level.is_some());
    let widgets_host = WidgetHost { io, host };
    if let Err(e) = page.widgets.draw(draw, &widgets_host, mouse) {
        super::log_warn(format!("could not draw the settings: {e:?}"));
    }
    page.draw_scrollbar(draw);
    let focused = page.widgets.focused();
    for (index, row) in page.rows.iter().enumerate() {
        let shown = |rect: &&[f32; 4]| row.is_slider() && rect[1] != HIDDEN_Y;
        let Some(&rect) = page.rects.get(index).filter(shown) else {
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
    let start = page.links.iter().find(|(_, link)| *link == Link::Exam);
    if let Some(button) = start.and_then(|(id, _)| page.widgets.button(*id))
        && inside(mouse, button.rect)
    {
        let tip = Text::translate(
            "learnordie.exam.start.tooltip",
            vec![number(EXAM_QUESTIONS)],
        );
        // `Tooltip.create`: the text wrapped as `Font.split` wraps it.
        let tip = wrap(&*draw, &io.tr(host, &tip), TOOLTIP_W);
        let tip: Vec<Text> = tip.into_iter().map(Text::literal).collect();
        if let Err(e) = draw.gui_tooltip(&tip, mouse) {
            super::log_warn(format!("could not draw the exam's tooltip: {e:?}"));
        }
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
    page.reveal();
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
    if page.on_scrollbar(at) {
        page.bar_held = true;
        page.scroll_to(at[1]);
        return Flow::Cancel;
    }
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
/// value under the pointer and holds the drag (Enter on a slider does nothing), a page button
/// opens its page, the guide's opens the guide, the exam's starts or stops it, and Done goes
/// back.
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
    if let Some(&(_, link)) = page.links.iter().find(|(link, _)| *link == id) {
        match link {
            Link::Page(to) => *page = Settings::new(to, &teacher.pack),
            Link::Guide => {
                game.settings = None;
                game.guide = Some(GuideScreen::new(true, false));
            }
            Link::Exam => exam_button(host, io, teacher, game),
        }
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
    if page.bar_held && button == 0 {
        page.scroll_to(at[1]);
        return true;
    }
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
