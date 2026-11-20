//! The dictionary (Java `DictionaryScreen`), on this mod's screen: J opens it over the world.
//!
//! On the left, the learned phrases ([`crate::core::Dictionary::learned`]) of the tab chosen,
//! "this world" or "all worlds" (kept across openings), each row the thing's picture, the German
//! and the translation; a search box over them ([`crate::core::search`] on every change). The
//! list scrolls by the wheel, by its scrollbar (a click or a drag on it), by a drag along the rows,
//! by the middle button held (it scrolls toward the pointer), and the selection moves by the arrow
//! and Page keys; a left or right click on a row selects it.
//!
//! On the right, the selected phrase: its picture large, the sentence word by word with the hint
//! drawn small above each word, the translation, Play and Slow (the phrase on the voice source
//! at the voice volume, muted or not; a new one stops the last, and closing the screen stops
//! it), the grammar notes and the other forms. Their text is the pack's lang keys
//! (`zhnarrator_de.dict.*`), resolved through the client's lang once per selection.
//!
//! The teacher's portrait is hidden while it is open; it pauses single player, as the original's.
//! Esc closes it.
use super::client::{Client, WidgetHost};
use super::hud::{fill, panel};
use super::screen::{inside, scaled};
use super::{Game, log_warn, number, sound};
use crate::core::{ItemName, Label, Part, Phrase, Pictures, Teacher, bare, search};
use mod_api::client::widgets::{Button, Outcome as Input, TextField, WidgetId, Widgets};
use mod_api::{
    ClientHost, DrawContext, Flow, Identifier, ItemKey, SoundPlay, SoundRef, SoundSource, Text,
};

const ROW_H: f32 = 26.0;
const GOLD: u32 = 0xFFD4_AF37;
const GREY: u32 = 0xFF9A_9A9A;
const HINT: u32 = 0xFF9C_9C9C;
const TEXT: u32 = 0xFFE4_E4E4;
const WHITE: u32 = 0xFFFF_FFFF;
/// `hudColorPinyin`'s default: the port has no reading colour setting (German has no readings).
const READING: u32 = 0xFFB0_E0FF;
const BAR_W: f32 = 6.0;
/// `EditBox`'s default length.
const SEARCH_MAX: usize = 32;
/// Play and Slow.
const PLAY_W: f32 = 80.0;
const PLAY_H: f32 = 18.0;
/// A related form's size.
const FORM_SCALE: f32 = 1.3;

/// Where the screen's parts are, for a screen of its size (`init`).
#[derive(Clone, Copy, Debug)]
struct Layout {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    list_right: f32,
    list_top: f32,
    /// How many rows the list shows.
    rows: usize,
}

impl Layout {
    fn new([w, h]: [f32; 2]) -> Layout {
        let (left, top) = (8.0, 24.0);
        let bottom = h - 8.0;
        let list_top = top + 48.0;
        Layout {
            left,
            top,
            right: w - 8.0,
            bottom,
            list_right: left + (w / 3.0).floor().clamp(140.0, 220.0),
            list_top,
            rows: (((bottom - 4.0 - list_top) / ROW_H).floor() as usize).max(1),
        }
    }

    fn tab_w(&self) -> f32 {
        ((self.list_right - self.left - 16.0) / 2.0).floor()
    }

    /// Row `i`'s button.
    fn row(&self, i: usize) -> [f32; 4] {
        [
            self.left + 6.0,
            self.list_top + i as f32 * ROW_H,
            self.list_right - self.left - 20.0,
            24.0,
        ]
    }

    fn track_x(&self) -> f32 {
        self.list_right - BAR_W - 6.0
    }

    fn track_h(&self) -> f32 {
        self.rows as f32 * ROW_H - 2.0
    }
}

/// The middle button's scrolling: where it went down, where the pointer is, the rows owed.
#[derive(Clone, Copy, Debug)]
struct Auto {
    anchor: [f32; 2],
    pointer_y: f32,
    owed: f64,
}

/// The selected phrase's detail, its lang keys resolved.
#[derive(Clone, Debug)]
struct Resolved {
    id: String,
    /// Each word and the hint over it (empty past the entry's hints).
    words: Vec<(Part, String)>,
    notes: Vec<String>,
    /// Each form's text, reading and what form it is.
    forms: Vec<(String, String, String)>,
}

/// A line of text and its width.
type Line = (String, f32);

/// The selected phrase's pane measured where it is drawn (`detail_pane`), kept while the phrase
/// and the pane's edges stay the same: a frame measures and wraps nothing.
#[derive(Debug)]
struct Pane {
    id: String,
    /// The pane's left and right edges.
    at: [f32; 2],
    /// The translation's label, where the translation starts, and its lines.
    label: String,
    en_x: f32,
    en: Vec<Line>,
    /// Each word's width, its hint's and its reading's, unscaled.
    words: Vec<[f32; 3]>,
    grammar: String,
    forms_heading: String,
    notes: Vec<Vec<Line>>,
    /// Each form's lines (wrapped for its larger size) and its label's.
    forms: Vec<(Vec<Line>, Vec<Line>)>,
}

impl Pane {
    fn new(
        draw: &dyn DrawContext,
        (host, io): (&dyn ClientHost, &dyn Client),
        (phrase, detail): (&Phrase, &Resolved),
        [dx, d_right]: [f32; 2],
    ) -> Pane {
        let say = |key: &str| io.tr(host, &Text::translate(key, vec![]));
        let label = say("learnordie.dict.translation");
        let en_x = dx + draw.text_width(&label) + 6.0;
        let room = d_right - dx;
        let words = detail.words.iter().map(|(part, hint)| {
            let width = |text: &str| draw.text_width(text);
            [width(&part.text), width(hint), width(&part.reading)]
        });
        let forms = detail.forms.iter().map(|(text, _, label)| {
            (
                lines(draw, text, room / FORM_SCALE),
                lines(draw, label, room),
            )
        });
        Pane {
            id: detail.id.clone(),
            at: [dx, d_right],
            en: lines(draw, &phrase.en, d_right - en_x),
            label,
            en_x,
            words: words.collect(),
            grammar: say("learnordie.dict.grammar"),
            forms_heading: say("learnordie.dict.forms"),
            notes: detail.notes.iter().map(|n| lines(draw, n, room)).collect(),
            forms: forms.collect(),
        }
    }
}

/// The dictionary showing.
pub(crate) struct DictionaryScreen {
    all: Vec<String>,
    this_world: Vec<String>,
    show_all: bool,
    /// The phrases the list shows: the tab's, searched.
    shown: Vec<String>,
    query: String,
    selected: usize,
    scroll: usize,
    layout: Layout,
    widgets: Widgets,
    tab_world: WidgetId,
    tab_all: WidgetId,
    search: WidgetId,
    /// Play's and Slow's places when last drawn; `None` while hidden.
    play_at: Option<[[f32; 4]; 2]>,
    /// The scrollbar is held.
    dragging: bool,
    /// A drag along the rows: where it began and the scroll then.
    list_drag: Option<(f32, usize)>,
    auto: Option<Auto>,
    /// Each listed phrase's German and translation as its row draws them (`fit`), once drawn;
    /// kept while the list and the rows' width (`row_w`) stay the same.
    row_text: Vec<Option<(String, String)>>,
    row_w: f32,
    detail: Option<Resolved>,
    pane: Option<Pane>,
    playing: Option<SoundRef>,
}

/// The item a phrase's picture is (`WordIcons.iconFor`).
fn picture(
    host: &dyn ClientHost,
    teacher: &Teacher,
    pictures: &Pictures,
    phrase: &Phrase,
) -> Option<ItemKey> {
    let item = teacher.pack.picture(phrase.key.as_deref()?, pictures)?;
    host.item_key(&Identifier::parse(item).ok()?)
}

/// `slot`: a sunken inventory slot.
fn slot(draw: &mut dyn DrawContext, [x, y]: [f32; 2], size: f32) {
    fill(draw, [x, y, x + size, y + size], 0xFF8B_8B8B);
    fill(draw, [x, y, x + size, y + 1.0], 0xFF37_3737);
    fill(draw, [x, y, x + 1.0, y + size], 0xFF37_3737);
    fill(draw, [x, y + size - 1.0, x + size, y + size], WHITE);
    fill(draw, [x + size - 1.0, y, x + size, y + size], WHITE);
}

/// `fit`: the text cut to `width`, ending in an ellipsis when cut. The longest start that fits
/// is found by halving, as a longer start is never narrower.
fn fit(draw: &dyn DrawContext, text: &str, width: f32) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = |len: usize| chars[..len].iter().collect::<String>();
    let fits = |len: usize| draw.text_width(&start(len)) <= width;
    let len = if fits(chars.len()) {
        chars.len()
    } else {
        // The start of `fitting` characters fits (or is empty), that of `over` does not.
        let (mut fitting, mut over) = (0, chars.len());
        while over - fitting > 1 {
            let mid = (fitting + over) / 2;
            if fits(mid) {
                fitting = mid;
            } else {
                over = mid;
            }
        }
        fitting
    };
    if len < chars.len() && len > 1 {
        let mut cut = start(len - 1);
        cut.push('\u{2026}');
        cut
    } else {
        start(len)
    }
}

/// `Font.split`: the text in lines no wider than `width`, broken at spaces.
pub(crate) fn wrap(draw: &dyn DrawContext, text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let longer = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && draw.text_width(&longer) > width {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = longer;
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// [`wrap`]'s lines, each with its width.
fn lines(draw: &dyn DrawContext, text: &str, width: f32) -> Vec<Line> {
    let measured = |line: String| {
        let width = draw.text_width(&line);
        (line, width)
    };
    wrap(draw, text, width).into_iter().map(measured).collect()
}

/// `wrapped`: the lines 10 px apart from `y`, one wider than the room (a word) drawn smaller;
/// where the next line would go.
fn draw_lines(
    draw: &mut dyn DrawContext,
    lines: &[Line],
    [x, y]: [f32; 2],
    right: f32,
    colour: u32,
) -> f32 {
    let room = right - x;
    let mut y = y;
    for (line, width) in lines {
        if *width > room {
            scaled(
                draw,
                Text::literal(line.clone()),
                [x, y],
                room / width,
                colour,
            );
        } else {
            draw.text(line, [x, y], colour, true);
        }
        y += 10.0;
    }
    y
}

/// The text wrapped to the room and drawn as [`draw_lines`] draws it.
fn wrapped(
    draw: &mut dyn DrawContext,
    text: &str,
    [x, y]: [f32; 2],
    right: f32,
    colour: u32,
) -> f32 {
    let lines = lines(draw, text, right - x);
    draw_lines(draw, &lines, [x, y], right, colour)
}

/// A hint, a note's argument or a form's label as the client's lang has it.
fn label_text(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    pictures: &Pictures,
    label: &Label,
) -> String {
    match label {
        Label::Key(key) => io.tr(host, &Text::translate(key.clone(), vec![])),
        Label::Text(text) => text.clone(),
        Label::Item { name, suffix } => {
            let name = match name {
                ItemName::Text(text) => text.clone(),
                ItemName::Game { key, fallback } => {
                    let item = teacher.pack.picture(key, pictures);
                    io.game_name(host, key, item)
                        .unwrap_or_else(|| fallback.clone())
                }
            };
            match suffix {
                Some(suffix) => format!("{name} \u{B7} {suffix}"),
                None => name,
            }
        }
    }
}

/// The phrase's detail with its lang keys resolved; `None` when it has no entry.
fn resolve(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    pictures: &Pictures,
    language: &str,
    phrase: &Phrase,
) -> Option<Resolved> {
    let detail = teacher.dictionary.detail(&teacher.pack, phrase, language)?;
    let text = |label: &Label| label_text(host, io, teacher, pictures, label);
    let words = detail
        .words
        .into_iter()
        .map(|(part, hint)| {
            let hint = hint.as_ref().map_or_else(String::new, text);
            (part, hint)
        })
        .collect();
    let notes = detail
        .notes
        .iter()
        .map(|note| {
            let args = note
                .args
                .iter()
                .map(|arg| match arg {
                    Label::Key(key) => Text::translate(key.clone(), vec![]),
                    other => Text::literal(text(other)),
                })
                .collect();
            io.tr(host, &Text::translate(note.key.clone(), args))
        })
        .collect();
    let forms = detail
        .forms
        .into_iter()
        .map(|form| {
            let label = io.tr(host, &Text::translate(form.label, vec![]));
            (form.text, form.reading, label)
        })
        .collect();
    Some(Resolved {
        id: phrase.id.clone(),
        words,
        notes,
        forms,
    })
}

impl DictionaryScreen {
    /// The learned phrases now, on the tab chosen last.
    pub(crate) fn new(
        host: &dyn ClientHost,
        io: &dyn Client,
        teacher: &Teacher,
        show_all: bool,
    ) -> DictionaryScreen {
        let learned = teacher
            .dictionary
            .learned(&teacher.pack, &teacher.progress, &teacher.config);
        let ids = |list: &[&Phrase]| list.iter().map(|phrase| phrase.id.clone()).collect();
        let (all, this_world): (Vec<String>, Vec<String>) =
            (ids(&learned.all), ids(&learned.this_world));
        let mut widgets = Widgets::new();
        let tab = |key: &str, count: usize| {
            Button::new([0.0; 4], Text::translate(key, vec![number(count)]))
        };
        let tab_world = widgets.add_button(tab("learnordie.dict.tab.world", this_world.len()));
        let tab_all = widgets.add_button(tab("learnordie.dict.tab.all", all.len()));
        let mut field = TextField::new([0.0; 4], SEARCH_MAX);
        field.message = io.tr(host, &Text::translate("learnordie.dict.search", vec![]));
        let search = widgets.add_text_field(field);
        let mut page = DictionaryScreen {
            all,
            this_world,
            show_all,
            shown: Vec::new(),
            query: String::new(),
            selected: 0,
            scroll: 0,
            layout: Layout::new([0.0, 0.0]),
            widgets,
            tab_world,
            tab_all,
            search,
            play_at: None,
            dragging: false,
            list_drag: None,
            auto: None,
            row_text: Vec::new(),
            row_w: 0.0,
            detail: None,
            pane: None,
            playing: None,
        };
        page.refilter(teacher);
        page
    }

    /// The tab's phrases.
    fn learned(&self) -> &[String] {
        if self.show_all {
            &self.all
        } else {
            &self.this_world
        }
    }

    /// `refilter`: the list is the tab's phrases holding the query, the selection kept inside.
    fn refilter(&mut self, teacher: &Teacher) {
        let phrases: Vec<&Phrase> = self
            .learned()
            .iter()
            .filter_map(|id| teacher.pack.phrase(id))
            .collect();
        self.shown = search(&phrases, &self.query)
            .into_iter()
            .map(|phrase| phrase.id.clone())
            .collect();
        self.row_text.clear();
        self.selected = self.selected.min(self.shown.len().saturating_sub(1));
        self.update_rows();
    }

    /// `showAll`.
    fn show(&mut self, teacher: &Teacher, all: bool) {
        self.show_all = all;
        self.selected = 0;
        self.scroll = 0;
        self.refilter(teacher);
    }

    /// `updateRows`: the scroll kept inside the list; the tab showing is the one greyed out.
    fn update_rows(&mut self) {
        let last = self.shown.len().saturating_sub(self.layout.rows);
        self.scroll = self.scroll.min(last);
        let show_all = self.show_all;
        if let Some(tab) = self.widgets.button_mut(self.tab_world) {
            tab.active = show_all;
        }
        if let Some(tab) = self.widgets.button_mut(self.tab_all) {
            tab.active = !show_all;
        }
    }

    /// `scroll` moved by `by` rows, kept inside the list.
    fn scroll_by(&mut self, by: i64) {
        self.scroll = (self.scroll as i64 + by).max(0) as usize;
        self.update_rows();
    }

    /// `select`: the row comes into view.
    fn select(&mut self, index: usize) {
        if index >= self.shown.len() {
            return;
        }
        self.selected = index;
        let rows = self.layout.rows;
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + rows {
            self.scroll = self.selected + 1 - rows;
        }
        self.update_rows();
    }

    fn overflows(&self) -> bool {
        self.shown.len() > self.layout.rows
    }

    fn thumb_h(&self) -> f32 {
        let l = &self.layout;
        let h = (l.track_h() as usize * l.rows / self.shown.len().max(1)) as f32;
        h.max(12.0)
    }

    fn on_scrollbar(&self, [x, y]: [f32; 2]) -> bool {
        let l = &self.layout;
        self.overflows()
            && x >= l.track_x() - 3.0
            && x <= l.list_right - 2.0
            && y >= l.list_top - 2.0
            && y <= l.list_top + l.track_h() + 2.0
    }

    /// `scrollTo`: the thumb's middle follows the pointer.
    fn scroll_to(&mut self, y: f32) {
        let range = self.layout.track_h() - self.thumb_h();
        let f = if range <= 0.0 {
            0.0
        } else {
            (y - self.layout.list_top - self.thumb_h() / 2.0) / range
        };
        let last = self.shown.len().saturating_sub(self.layout.rows);
        self.scroll = (f.clamp(0.0, 1.0) * last as f32).round() as usize;
        self.update_rows();
    }

    /// `press`: a right click selects a row; the left or right button on the scrollbar holds it,
    /// on the rows of a long list starts a drag (and goes on to the row); the middle button
    /// starts scrolling toward the pointer. True when it took the click.
    fn press(&mut self, [x, y]: [f32; 2], button: i32) -> bool {
        let l = self.layout;
        let grab = button == 0 || button == 1;
        let on_rows = x >= l.left
            && x < l.track_x() - 3.0
            && y >= l.list_top
            && y < l.list_top + l.rows as f32 * ROW_H;
        if button == 1 && on_rows {
            self.select(self.scroll + ((y - l.list_top) / ROW_H) as usize);
        }
        if grab && self.on_scrollbar([x, y]) {
            self.dragging = true;
            self.scroll_to(y);
            true
        } else if grab && self.overflows() && on_rows {
            self.list_drag = Some((y, self.scroll));
            false
        } else if button == 2 && x < l.list_right && self.overflows() {
            self.auto = Some(Auto {
                anchor: [x, y],
                pointer_y: y,
                owed: 0.0,
            });
            true
        } else {
            false
        }
    }

    /// The pointer moved with a button held: the list follows a drag along it or on its
    /// scrollbar, and the middle button's scrolling follows the pointer.
    pub(crate) fn dragged(
        &mut self,
        host: &dyn ClientHost,
        io: &dyn Client,
        [x, y]: [f32; 2],
        button: i32,
    ) -> bool {
        if let Some((from, scroll)) = self.list_drag {
            let moved = ((y - from) / ROW_H).round() as i64;
            if moved != 0 || self.scroll != scroll {
                self.scroll = scroll;
                self.scroll_by(-moved);
            }
            return true;
        }
        if self.dragging {
            self.scroll_to(y);
            return true;
        }
        if let Some(auto) = self.auto.as_mut() {
            auto.pointer_y = y;
            return true;
        }
        self.widgets
            .mouse_dragged(&WidgetHost { io, host }, [x, y], button)
            .handled()
    }

    /// A mouse button let go: the drags end.
    pub(crate) fn released(&mut self, button: i32) -> bool {
        match button {
            0 | 1 => {
                self.dragging = false;
                self.list_drag = None;
            }
            2 => self.auto = None,
            _ => {}
        }
        self.widgets.mouse_released(button).handled()
    }

    /// The client tick: the middle button's scrolling, a row for every 30 px the pointer is
    /// away from where it went down, once it is more than 4 px away.
    pub(crate) fn tick(&mut self) {
        let Some(auto) = self.auto.as_mut() else {
            return;
        };
        let d = f64::from(auto.pointer_y - auto.anchor[1]);
        if d.abs() <= 4.0 {
            return;
        }
        auto.owed += d / 30.0;
        let steps = auto.owed.trunc();
        if steps != 0.0 {
            auto.owed -= steps;
            self.scroll_by(steps as i64);
        }
    }

    /// The wheel over the list moves it a row a notch.
    pub(crate) fn scrolled(&mut self, [x, _]: [f32; 2], dy: f64) -> bool {
        if x >= self.layout.list_right {
            return false;
        }
        if dy != 0.0 {
            self.scroll_by(-(dy.signum() as i64));
        }
        true
    }

    /// A widget's input: a tab shows its list, the search box's text filters it.
    fn act(&mut self, teacher: &Teacher, input: Input) -> Flow {
        match input {
            Input::Pressed(id) if id == self.tab_world => self.show(teacher, false),
            Input::Pressed(id) if id == self.tab_all => self.show(teacher, true),
            Input::Changed(id) if id == self.search => {
                self.query = self
                    .widgets
                    .text_field(self.search)
                    .map_or_else(String::new, |field| field.value().to_string());
                self.refilter(teacher);
            }
            _ => {}
        }
        if input.handled() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A character typed: the search box takes it when focused.
    pub(crate) fn char_typed(
        &mut self,
        host: &dyn ClientHost,
        io: &dyn Client,
        teacher: &Teacher,
        c: char,
    ) -> Flow {
        let input = self.widgets.char_typed(&WidgetHost { io, host }, c);
        self.act(teacher, input)
    }

    /// `play`: the selected phrase at `speed` when the pack has it, else at the speed set, on
    /// the voice source at the voice volume, if its sound exists. The last one stops.
    fn play(&mut self, host: &dyn ClientHost, io: &dyn Client, teacher: &Teacher, speed: &str) {
        let Some(id) = self.shown.get(self.selected) else {
            return;
        };
        let (pack, config) = (&teacher.pack, &teacher.config);
        let speed = if pack.speeds.iter().any(|s| s == speed) {
            speed
        } else {
            pack.speed(&config.speed)
        };
        let event = pack.sound_event(id, pack.voice(&config.voice), speed);
        self.stop(host, io);
        if pack.has_sound(&event) {
            let mut sound = SoundPlay::ui(&event).with_volume(config.volume);
            sound.source = SoundSource::Voice;
            self.playing = io.play(host, &sound);
        }
    }

    /// The screen closes (`removed`): its sound stops.
    pub(crate) fn stop(&mut self, host: &dyn ClientHost, io: &dyn Client) {
        if let Some(sound) = self.playing.take() {
            io.stop(host, sound);
        }
    }

    /// The rows in view have their German and translation cut to `max_w` (`row_text`), as
    /// `fit` cuts them.
    fn fit_rows(&mut self, draw: &dyn DrawContext, teacher: &Teacher, max_w: f32) {
        if self.row_w != max_w || self.row_text.len() != self.shown.len() {
            self.row_w = max_w;
            self.row_text = vec![None; self.shown.len()];
        }
        let end = self.shown.len().min(self.scroll + self.layout.rows);
        for i in self.scroll..end {
            if self.row_text[i].is_none()
                && let Some(phrase) = teacher.pack.phrase(&self.shown[i])
            {
                let en = fit(draw, bare(&phrase.en), max_w / 0.75);
                self.row_text[i] = Some((fit(draw, phrase.bare(), max_w), en));
            }
        }
    }

    /// `drawRows`: the middle button's anchor, the scrollbar, and each row's picture, German
    /// and translation over its button, the selected one marked.
    fn draw_rows(
        &mut self,
        draw: &mut dyn DrawContext,
        host: &dyn ClientHost,
        teacher: &Teacher,
        pictures: &Pictures,
    ) {
        let l = self.layout;
        let max_w = l.row(0)[2] - 32.0;
        self.fit_rows(draw, teacher, max_w);
        if let Some(auto) = self.auto {
            let [ax, ay] = auto.anchor;
            fill(draw, [ax - 1.0, ay - 6.0, ax + 2.0, ay + 7.0], 0xC0FF_FFFF);
            fill(draw, [ax - 4.0, ay - 1.0, ax + 5.0, ay + 2.0], 0xC0FF_FFFF);
        }
        if self.overflows() {
            let (tx, th) = (l.track_x(), self.thumb_h());
            let last = (self.shown.len() - l.rows) as f32;
            let ty = l.list_top + ((l.track_h() - th) * (self.scroll as f32 / last)).floor();
            fill(
                draw,
                [tx, l.list_top, tx + BAR_W, l.list_top + l.track_h()],
                0x6600_0000,
            );
            let (edge, face) = if self.dragging {
                (0xFFE0_E0E0, WHITE)
            } else {
                (0xFFA8_A8A8, 0xFFC6_C6C6)
            };
            fill(draw, [tx, ty, tx + BAR_W, ty + th], edge);
            fill(draw, [tx, ty, tx + BAR_W - 1.0, ty + th - 1.0], face);
        }
        for i in 0..l.rows {
            let Some(phrase) = self
                .shown
                .get(self.scroll + i)
                .and_then(|id| teacher.pack.phrase(id))
            else {
                break;
            };
            let [x, y, _, h] = l.row(i);
            if self.scroll + i == self.selected {
                fill(draw, [x - 4.0, y, x - 1.0, y + h], GOLD);
            }
            slot(draw, [x + 3.0, y + 2.0], 20.0);
            if let Some(item) = picture(host, teacher, pictures, phrase) {
                draw.item(item, [x + 5.0, y + (h - 16.0) / 2.0], 16.0);
            }
            let Some((german, en)) = &self.row_text[self.scroll + i] else {
                break;
            };
            let tx = x + 28.0;
            draw.text(german, [tx, y + 3.0], WHITE, true);
            scaled(draw, Text::literal(en.clone()), [tx, y + 14.0], 0.75, GREY);
        }
    }

    /// `sentence`: the picture in a large slot, then the words in columns, each with its hint
    /// small above it, wrapped inside the pane. A column wider than the pane is drawn smaller.
    /// Where the translation line goes.
    fn sentence(
        &self,
        draw: &mut dyn DrawContext,
        host: &dyn ClientHost,
        teacher: &Teacher,
        pictures: &Pictures,
        (phrase, detail, pane): (&Phrase, &Resolved, &Pane),
        [dx, d_right]: [f32; 2],
    ) -> f32 {
        let top0 = self.layout.top + 10.0;
        slot(draw, [dx, top0], 42.0);
        if let Some(item) = picture(host, teacher, pictures, phrase) {
            draw.item(item, [dx + 5.0, top0 + 5.0], 32.0);
        }
        let sx = dx + 54.0;
        let (mut x, mut y) = (sx, top0);
        let readings = !phrase.reading.is_empty();
        let line_h = if readings { 42.0 } else { 32.0 };
        let room = (d_right - sx).max(1.0);
        let config = &teacher.config;
        for ((part, hint), [word_w, hint_w, reading_w]) in detail.words.iter().zip(&pane.words) {
            let word_w = word_w * 2.0;
            let hint_w = hint_w * 0.7;
            let reading_w = reading_w * 0.9;
            let col_w = word_w.max(hint_w).max(reading_w).ceil();
            let shrink = if col_w > room { room / col_w } else { 1.0 };
            let col_w = col_w * shrink;
            if x + col_w > d_right && x > sx {
                x = sx;
                y += line_h;
            }
            let cx = x + col_w / 2.0;
            let hint_at = [cx - hint_w * shrink / 2.0, y];
            scaled(
                draw,
                Text::literal(hint.clone()),
                hint_at,
                0.7 * shrink,
                HINT,
            );
            let colour = config
                .role_colour(&part.role)
                .unwrap_or(config.hud_color_zh) as u32;
            let word_at = [cx - word_w * shrink / 2.0, y + 8.0];
            scaled(
                draw,
                Text::literal(part.text.clone()),
                word_at,
                2.0 * shrink,
                colour,
            );
            if readings {
                let at = [cx - reading_w * shrink / 2.0, y + 29.0];
                scaled(
                    draw,
                    Text::literal(part.reading.clone()),
                    at,
                    0.9 * shrink,
                    READING,
                );
            }
            x += col_w + 10.0;
        }
        (top0 + 48.0).max(y + line_h + 4.0)
    }

    /// The right pane: the sentence, the translation, Play and Slow (kept inside the pane), the
    /// grammar notes and the other forms, as far as they fit. The translation and the forms wrap
    /// inside the pane, as the notes do (the original draws them on one line, past a narrow
    /// window's edge). Their text is measured once for the phrase and the pane's edges (`pane`).
    #[allow(clippy::too_many_arguments)]
    fn detail_pane(
        &mut self,
        draw: &mut dyn DrawContext,
        host: &dyn ClientHost,
        io: &dyn Client,
        teacher: &Teacher,
        pictures: &Pictures,
        phrase: &Phrase,
        mouse: [f32; 2],
    ) {
        let l = self.layout;
        let (dx, d_right) = (l.list_right + 16.0, l.right - 10.0);
        if let Some(detail) = &self.detail
            && self
                .pane
                .as_ref()
                .is_none_or(|pane| pane.id != detail.id || pane.at != [dx, d_right])
        {
            let pane = Pane::new(draw, (host, io), (phrase, detail), [dx, d_right]);
            self.pane = Some(pane);
        }
        let shown = self.detail.as_ref().zip(self.pane.as_ref());
        let after = match shown {
            Some((detail, pane)) => self.sentence(
                draw,
                host,
                teacher,
                pictures,
                (phrase, detail, pane),
                [dx, d_right],
            ),
            None => l.top + 10.0,
        };
        let en_lines = shown.map_or(1, |(_, pane)| pane.en.len());
        let below = 14.0 + 10.0 * en_lines.saturating_sub(1) as f32;
        let buttons_y = (after + below).min(l.bottom - PLAY_H - 2.0);
        let play = [dx, buttons_y, PLAY_W, PLAY_H];
        let slow = [dx + 86.0, buttons_y, PLAY_W, PLAY_H];
        self.play_at = Some([play, slow]);
        for (rect, key) in [
            (play, "learnordie.dict.play"),
            (slow, "learnordie.dict.slow"),
        ] {
            let label = Text::translate(key, vec![]);
            if let Err(e) = draw.widget_button(rect, &label, true, inside(mouse, rect)) {
                log_warn(format!("could not draw the dictionary's {key}: {e:?}"));
            }
        }
        let Some((detail, pane)) = shown else {
            return;
        };
        draw.text(&pane.label, [dx, after], GREY, true);
        draw_lines(draw, &pane.en, [pane.en_x, after], d_right, TEXT);
        let mut y = buttons_y + 28.0;
        draw.text(&pane.grammar, [dx, y], GOLD, true);
        y += 13.0;
        for note in &pane.notes {
            if y > l.bottom - 12.0 {
                return;
            }
            y = draw_lines(draw, note, [dx, y], d_right, TEXT) + 3.0;
        }
        if pane.forms.is_empty() || y > l.bottom - 30.0 {
            return;
        }
        y += 5.0;
        draw.text(&pane.forms_heading, [dx, y], GOLD, true);
        y += 13.0;
        let room = d_right - dx;
        for ((_, reading, _), (texts, labels)) in detail.forms.iter().zip(&pane.forms) {
            let extra = 14.0 * (texts.len() - 1) as f32 + 10.0 * (labels.len() - 1) as f32;
            if y + extra > l.bottom - 22.0 {
                return;
            }
            for (i, (line, width)) in texts.iter().enumerate() {
                let s = FORM_SCALE.min(room / width.max(1.0));
                scaled(draw, Text::literal(line.clone()), [dx, y], s, TEXT);
                if i + 1 == texts.len() && !reading.is_empty() {
                    let at = [dx + (width * s).floor() + 8.0, y + 3.0];
                    draw.text(reading, at, READING, true);
                }
                y += 14.0;
            }
            y = draw_lines(draw, labels, [dx, y], d_right, GREY) + 3.0;
        }
    }
}

/// `extractRenderState`: the in-world menu background with the blur, the two panels, the title,
/// the tabs and the search box, then the list and the selected phrase, or why there is none.
pub(super) fn render(
    host: &dyn ClientHost,
    io: &dyn Client,
    draw: &mut dyn DrawContext,
    teacher: &Teacher,
    game: &mut Game,
    mouse: [f32; 2],
) {
    let Game {
        dictionary,
        pictures,
        language,
        ..
    } = game;
    let Some(page) = dictionary.as_mut() else {
        return;
    };
    let size = draw.size();
    let [w, h] = size;
    page.layout = Layout::new(size);
    page.update_rows();
    let l = page.layout;
    io.blur(host);
    draw.draw_menu_background([0.0, 0.0, w, h], true);
    panel(draw, [l.left, l.top, l.list_right, l.bottom], 235);
    panel(draw, [l.list_right + 6.0, l.top, l.right, l.bottom], 235);
    let title = io.tr(host, &Text::translate("learnordie.dict.title", vec![]));
    draw.text(&title, [l.left + 2.0, 9.0], GOLD, true);
    let tab_w = l.tab_w();
    let places = [
        (page.tab_world, [l.left + 6.0, l.top + 6.0, tab_w, 16.0]),
        (
            page.tab_all,
            [l.left + 10.0 + tab_w, l.top + 6.0, tab_w, 16.0],
        ),
    ];
    for (id, rect) in places {
        if let Some(button) = page.widgets.button_mut(id) {
            button.rect = rect;
        }
    }
    if let Some(field) = page.widgets.text_field_mut(page.search) {
        field.rect = [
            l.left + 6.0,
            l.top + 26.0,
            l.list_right - l.left - 12.0,
            16.0,
        ];
    }
    if let Err(e) = page.widgets.draw(draw, &WidgetHost { io, host }, mouse) {
        log_warn(format!("could not draw the dictionary: {e:?}"));
    }
    let shown = page.shown.len().saturating_sub(page.scroll).min(l.rows);
    for i in 0..shown {
        let rect = l.row(i);
        let blank = Text::literal("");
        if let Err(e) = draw.widget_button(rect, &blank, true, inside(mouse, rect)) {
            log_warn(format!("could not draw a dictionary row: {e:?}"));
        }
    }
    page.play_at = None;
    let dx = l.list_right + 16.0;
    let d_right = l.right - 10.0;
    let say = |key: &str| io.tr(host, &Text::translate(key, vec![]));
    if teacher.dictionary.is_empty() {
        wrapped(
            draw,
            &say("learnordie.dict.no_data"),
            [dx, l.top + 12.0],
            d_right,
            TEXT,
        );
        return;
    }
    if page.learned().is_empty() {
        let key = if !page.show_all && !page.all.is_empty() {
            "learnordie.dict.empty_world"
        } else {
            "learnordie.dict.empty"
        };
        wrapped(draw, &say(key), [dx, l.top + 12.0], d_right, TEXT);
        return;
    }
    page.draw_rows(draw, host, teacher, pictures);
    let Some(phrase) = page
        .shown
        .get(page.selected)
        .and_then(|id| teacher.pack.phrase(id))
    else {
        return;
    };
    if page.detail.as_ref().is_none_or(|d| d.id != phrase.id) {
        page.detail = resolve(host, io, teacher, pictures, language, phrase);
    }
    page.detail_pane(draw, host, io, teacher, pictures, phrase, mouse);
}

/// Esc lets Escape close the screen; Down, Up, Page Down and Page Up move the selection (by the
/// rows shown for a page); the other keys go to the tabs and the search box.
pub(super) fn key_pressed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    game: &mut Game,
    key: &str,
    modifiers: u32,
) -> Flow {
    let Some(page) = game.dictionary.as_mut() else {
        return Flow::Continue;
    };
    if key == "key.keyboard.escape" {
        return Flow::Continue;
    }
    let rows = page.layout.rows as i64;
    let step = match key {
        "key.keyboard.down" => 1,
        "key.keyboard.up" => -1,
        "key.keyboard.page.down" => rows,
        "key.keyboard.page.up" => -rows,
        _ => 0,
    };
    if step != 0 && !page.shown.is_empty() {
        let last = page.shown.len() as i64 - 1;
        page.select((page.selected as i64 + step).clamp(0, last) as usize);
        return Flow::Cancel;
    }
    let input = page
        .widgets
        .key_pressed(&WidgetHost { io, host }, key, modifiers);
    let flow = page.act(teacher, input);
    game.show_all = page.show_all;
    flow
}

/// A mouse button down: the list's own handling first (`press`), then the tabs and the search
/// box, a row's button (selects it) and Play and Slow, with the left button.
pub(super) fn mouse_pressed(
    host: &dyn ClientHost,
    io: &dyn Client,
    teacher: &Teacher,
    game: &mut Game,
    at: [f32; 2],
    button: i32,
) -> Flow {
    let Some(page) = game.dictionary.as_mut() else {
        return Flow::Continue;
    };
    if page.press(at, button) {
        return Flow::Cancel;
    }
    if button != 0 {
        return Flow::Continue;
    }
    let input = page
        .widgets
        .mouse_pressed(&WidgetHost { io, host }, at, 0, 0);
    if input.handled() {
        let flow = page.act(teacher, input);
        game.show_all = page.show_all;
        return flow;
    }
    let rows = page
        .shown
        .len()
        .saturating_sub(page.scroll)
        .min(page.layout.rows);
    if let Some(i) = (0..rows).find(|&i| inside(at, page.layout.row(i))) {
        sound::click(host, io);
        page.select(page.scroll + i);
        return Flow::Cancel;
    }
    let Some([play, slow]) = page.play_at else {
        return Flow::Continue;
    };
    for (rect, speed) in [(play, "normal"), (slow, "slow")] {
        if inside(at, rect) {
            sound::click(host, io);
            page.play(host, io, teacher, speed);
            return Flow::Cancel;
        }
    }
    Flow::Continue
}
