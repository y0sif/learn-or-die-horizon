//! The teacher's HUD layer (Java `NarratorHud`, the pilot's parts): her portrait in a corner with
//! the lipsync mouth, the held item's noun over the hotbar, and the phrase she says in words
//! coloured by their grammatical role with its translation under it.
//!
//! Scaled text goes through one `gui_paint` batch per block, as the SDK's pose (`GuiDraw::Push`)
//! holds only inside one batch. The portrait is drawn with `DrawContext::sprite`, which takes the
//! pack's `textures/gui/teacher.png` as the GUI atlas sprite `zhnarrator_de:gui/teacher`.
use super::{STATE, State, lock};
use crate::core::Teacher;
use mod_api::client::hud_layout::PoseOp;
use mod_api::{
    ClientHost, DrawContext, GuiDraw, GuiLayer, GuiLayerDrawEvent, GuiLayerPlacement,
    RegisterGuiLayersEvent, Text,
};

pub const LAYER: &str = "learnordie:teacher";

/// `PANEL_BG` and `GOLD`, without alpha.
pub(crate) const PANEL: u32 = 0x0014_161A;
pub(crate) const GOLD: u32 = 0x00E6_B422;
/// The held label's text colour.
const LABEL: u32 = 0x00FF_E08A;

pub(crate) fn register(event: &mut RegisterGuiLayersEvent<'_>) {
    event.layers.push(GuiLayer {
        id: LAYER.into(),
        placement: GuiLayerPlacement::Above("minecraft:chat".into()),
    });
}

/// `alpha << 24 | argb & 0xFFFFFF`.
pub(crate) fn with_alpha(argb: u32, alpha: i32) -> u32 {
    ((alpha.clamp(0, 255) as u32) << 24) | (argb & 0x00FF_FFFF)
}

/// `Font.width`.
pub(crate) fn width(draw: &dyn DrawContext, text: &str) -> f32 {
    draw.text_width(text).ceil()
}

/// `fill(x0, y0, x1, y1)`.
pub(crate) fn fill(draw: &mut dyn DrawContext, [x0, y0, x1, y1]: [f32; 4], argb: u32) {
    draw.fill([x0, y0, x1 - x0, y1 - y0], argb);
}

/// `NarratorHud.frame`: a one-pixel outline inside the box.
pub(crate) fn frame(draw: &mut dyn DrawContext, [x0, y0, x1, y1]: [f32; 4], argb: u32) {
    fill(draw, [x0, y0, x1, y0 + 1.0], argb);
    fill(draw, [x0, y1 - 1.0, x1, y1], argb);
    fill(draw, [x0, y0 + 1.0, x0 + 1.0, y1 - 1.0], argb);
    fill(draw, [x1 - 1.0, y0 + 1.0, x1, y1 - 1.0], argb);
}

/// `NarratorHud.panel`: a dark box with a double gold outline.
pub(crate) fn panel(draw: &mut dyn DrawContext, [x0, y0, x1, y1]: [f32; 4], alpha: i32) {
    let a = alpha as u32;
    fill(draw, [x0, y0, x1, y1], ((a * 150 / 255) << 24) | PANEL);
    frame(
        draw,
        [x0 - 1.0, y0 - 1.0, x1 + 1.0, y1 + 1.0],
        ((a * 30 / 255) << 24) | GOLD,
    );
    frame(draw, [x0, y0, x1, y1], ((a * 95 / 255) << 24) | GOLD);
}

pub(crate) fn draw(_host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
    if event.layer != LAYER {
        return;
    }
    let now = super::io().now();
    let mut state = lock(&STATE);
    let State { teacher, game } = &mut *state;
    let Some(teacher) = teacher.as_mut() else {
        return;
    };
    if game.level.is_none() {
        return;
    }
    let draw = &mut *event.draw;
    let [w, h] = draw.size();
    portrait(draw, teacher, now, [w, h]);
    held_label(draw, teacher, now, [w, h]);
    spoken(draw, teacher, now, [w, h]);
}

/// `renderPortrait`: the frame the core picks, at `hudPortraitScale` in half steps, standing on the
/// screen's edge from her waist up, showing more of her while she hops.
fn portrait(draw: &mut dyn DrawContext, teacher: &mut Teacher, now: i64, [w, h]: [f32; 2]) {
    let corner = teacher.config.hud_portrait.clone();
    if !teacher.config.on() || corner == "off" {
        return;
    }
    let scale = ((teacher.config.hud_portrait_scale * 2.0).round() / 2.0).max(0.5);
    let frame = teacher.portrait_frame(now);
    let hop = teacher.hop_lift(now);
    let sprite = &teacher.pack.meta.sprite;
    let fw = sprite.frame_width as f32;
    let waist = sprite.waist.max(1);
    let units = sprite.screen_height / waist as f32;
    let x = if corner.ends_with("right") {
        w - 6.0 - fw * units * scale
    } else {
        6.0
    };
    let y = if corner.starts_with("bottom") {
        h - waist as f32 * units * scale
    } else {
        6.0
    };
    let lift = (hop * scale).min(y);
    let shown = sprite
        .frame_height
        .min(waist + (lift / (units * scale)).ceil() as i32) as f32;
    let s = units * scale;
    let name = format!("{}:gui/{}", teacher.pack.meta.namespace, sprite.file);
    draw.sprite(
        &name,
        [frame as f32 * fw, 0.0, fw, shown],
        [x, y - lift, fw * s, shown * s],
        None,
    );
}

/// `renderHeldLabel`: the held item's noun in a small panel over the hotbar.
fn held_label(draw: &mut dyn DrawContext, teacher: &Teacher, now: i64, [w, h]: [f32; 2]) {
    let Some((phrase, alpha)) = teacher.held_label(now) else {
        return;
    };
    let tw = width(draw, &phrase.text);
    let x0 = (w / 2.0 - tw / 2.0).round() - 7.0;
    let y0 = h - 75.0;
    panel(draw, [x0, y0, x0 + tw + 14.0, y0 + 15.0], alpha);
    draw.text(
        &phrase.text,
        [x0 + 7.0, y0 + 3.0],
        with_alpha(LABEL, alpha),
        true,
    );
}

/// The phrase on the HUD: the text at `hudScale` in role colours, the translation under it at 0.6
/// of that, over a backdrop, at the `hudAnchor`.
fn spoken(draw: &mut dyn DrawContext, teacher: &Teacher, now: i64, [w, h]: [f32; 2]) {
    let Some(hud) = teacher.hud_text(now) else {
        return;
    };
    let config = &teacher.config;
    let (phrase, alpha) = (hud.phrase, hud.alpha);
    let scale = config.hud_scale.max(0.5);
    let small = scale * 0.6;
    let cx = w / 2.0;
    let small_lines = if hud.show_translation { 1.0 } else { 0.0 };
    let block = 11.0 * scale + small_lines * 10.0 * small;
    let y = match config.hud_anchor.as_str() {
        "top" => config.hud_offset as f32,
        "fraction" => h * config.hud_y,
        _ => h - config.hud_offset as f32 - block,
    };
    let spans = teacher.spans(phrase, config.hud_color_zh);
    let widths: Vec<f32> = spans.iter().map(|span| width(draw, &span.text)).collect();
    let total: f32 = widths.iter().sum();
    let en_width = width(draw, &phrase.en);
    if config.hud_backdrop {
        let mut bw = (total * scale).round();
        if hud.show_translation {
            bw = bw.max((en_width * small).round());
        }
        let bx0 = (cx - bw / 2.0).round() - 8.0;
        let by0 = y.round() - 3.0;
        let colour = (((alpha as u32) * 105 / 255) << 24) | PANEL;
        fill(
            draw,
            [bx0, by0, bx0 + bw + 16.0, (y + block).round() + 1.0],
            colour,
        );
    }
    let shadow = config.hud_shadow;
    let mut ops = vec![GuiDraw::Push(vec![
        PoseOp::Translate(cx, y),
        PoseOp::Scale(scale, scale),
    ])];
    let mut x = -(total / 2.0).floor();
    for (span, span_width) in spans.iter().zip(&widths) {
        ops.push(GuiDraw::Text {
            text: Text::literal(span.text.clone()),
            x: x as i32,
            y: 0,
            color: with_alpha(span.colour as u32, alpha),
            shadow,
        });
        x += span_width;
    }
    ops.push(GuiDraw::Pop);
    if hud.show_translation {
        ops.push(GuiDraw::Push(vec![
            PoseOp::Translate(cx, y + 11.0 * scale),
            PoseOp::Scale(small, small),
        ]));
        ops.push(GuiDraw::Text {
            text: Text::literal(phrase.en.clone()),
            x: -(en_width / 2.0).floor() as i32,
            y: 0,
            color: with_alpha(config.hud_color_en as u32, alpha),
            shadow,
        });
        ops.push(GuiDraw::Pop);
    }
    if let Err(e) = draw.gui_paint(&ops) {
        super::log_warn(format!("could not draw the phrase: {e:?}"));
    }
}
