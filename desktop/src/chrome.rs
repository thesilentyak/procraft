//! The bars over the canvas, as the Android app's (ui/Chrome.kt): the top bar (Gallery, the
//! action buttons, the tools, Layers and the colour) and the side bar (brush size, the modify
//! button, opacity, undo and redo, and a slider's panel).

use std::sync::Arc;

use egui::emath::TSTransform;
use egui::layers::{PaintList, ShapeIdx};
use egui::{Align2, Color32, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use procraft_core::{
    CanvasInfo, CanvasSession, InterfacePart, SideBarDrag, SideBarDragState, SideBarShown, SideSliderTap, ToolKind,
    side_slider_drag_share, side_slider_shown_percent, side_slider_tap, side_thumb_shadow,
};
use resvg::tiny_skia;

use crate::icons;
use crate::look::{Look, Themed, Weight, argb, fill, highlight, mix, pc, round_rect, text, text_runs};

/// The top bar's buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BarButton {
    Gallery,
    Actions,
    Adjustments,
    Selection,
    Transform,
    /// Image generation's popover.
    #[cfg(feature = "generation")]
    Generate,
    Paint,
    Smudge,
    Erase,
    Layers,
    Colour,
}

/// Which of the top bar's buttons are lit.
#[derive(Default)]
pub struct Lit {
    pub actions: bool,
    pub adjustments: bool,
    pub selection: bool,
    pub transform: bool,
    pub layers: bool,
    /// Image generation's popover is open; a run's progress (0..1) while one goes.
    #[cfg(feature = "generation")]
    pub generate: bool,
    #[cfg(feature = "generation")]
    pub generating: Option<f32>,
    /// A panel takes the highlight from the current tool while open.
    pub panel_open: bool,
}

/// The button tapped on the top bar, if any, and where each one is (points); the one held down
/// (not dragged off), and since when (seconds, egui's clock). The colour button's touches are
/// ColorDrop's (see [`crate::colour_drop`]): it is never the one tapped here.
#[derive(Default)]
pub struct Bar {
    pub pressed: Option<BarButton>,
    pub rects: Vec<(BarButton, Rect)>,
    pub down: Option<(BarButton, f64)>,
}

impl Bar {
    pub fn rect(&self, button: BarButton) -> Option<Rect> {
        self.rects.iter().find(|(b, _)| *b == button).map(|(_, r)| *r)
    }
}

/// The colour button's brightness under which it is ringed on the dark bar.
const DARK_RING_BELOW: f32 = 0.35;

/// Adds `rect` (points, its corners `corner`) to `glass`: frost, or a window (window pixels: left,
/// top, right, bottom, corner, how much shows).
pub fn frost(glass: &mut Vec<f32>, rect: Rect, corner: f32, ppp: f32) {
    glass.extend([rect.left(), rect.top(), rect.right(), rect.bottom(), corner].map(|v| v * ppp));
    glass.push(1.0);
}

/// The bar across the top of the canvas: artwork actions on the left, painting tools on the right.
/// It shows at the size a plugin gives it: laid out at its own size, as wide as the window is at
/// that size, and drawn and touched that many times as big from the window's top left. Its
/// buttons are where they show.
pub fn top_bar(ui: &mut Ui, look: &mut Look, tool: ToolKind, colour: Color32, brightness: f32, lit: &Lit) -> Bar {
    let mut bar = Bar::default();
    let frame = egui::Frame::NONE.fill(argb(pc::BAR));
    let k = crate::panel::part_scale(ui.ctx(), InterfacePart::TopBar);
    egui::Panel::top("bar").exact_size(pc::BAR_HEIGHT * k).frame(frame).show(ui, |ui| {
        let shown = ui.max_rect();
        let r = Rect::from_min_size(shown.min, shown.size() / k);
        let to_window = scaled_from(r.min, k);
        let ctx = ui.ctx().clone();
        let first = ctx.graphics(|g| g.get(ui.layer_id()).map_or(ShapeIdx(0), PaintList::next_idx));
        look.set_zoom(ui.layer_id(), k);
        let (tool_gap, action_gap) = pc::bar_gaps(r.width());
        let cy = r.center().y;
        let button = |x: f32, width: f32| {
            Rect::from_min_size(pos2(x, cy - pc::BAR_BUTTON_HEIGHT / 2.0), vec2(width, pc::BAR_BUTTON_HEIGHT))
        };
        let mut tap = |ui: &Ui, which: BarButton, rect: Rect| {
            let rect = to_window.mul_rect(rect);
            bar.rects.push((which, rect));
            let response = ui.interact(rect, ui.scope_id().with(which), Sense::click());
            if response.clicked() {
                bar.pressed = Some(which);
            }
            let (since, dragging) = ui.input(|i| (i.pointer.press_start_time(), i.pointer.is_decidedly_dragging()));
            if response.is_pointer_button_down_on()
                && !dragging
                && let Some(since) = since
            {
                bar.down = Some((which, since));
            }
        };

        let mut x = r.left() + pc::BAR_MARGIN;
        let gallery = button(x, pc::GALLERY_WIDTH);
        tap(ui, BarButton::Gallery, gallery);
        text(
            ui.painter(),
            pos2(gallery.left(), cy - 1.0),
            Align2::LEFT_CENTER,
            "Gallery",
            18.0,
            Weight::Medium,
            argb(pc::BAR_TEXT),
        );
        x += pc::GALLERY_WIDTH + action_gap;
        // an action button: its glyph on a dark disc, which turns blue while its panel is open;
        // the glyphs are drawn in a 44 frame, shown `glyph` across
        let actions = [
            (BarButton::Actions, &icons::ACTIONS, 36.0, lit.actions),
            (BarButton::Adjustments, &icons::ADJUSTMENTS, 36.0, lit.adjustments),
            (BarButton::Selection, &icons::SELECTION, 33.0, lit.selection),
            (BarButton::Transform, &icons::TRANSFORM, 44.0, lit.transform),
        ];
        #[cfg(feature = "generation")]
        let actions = {
            let mut all = actions.to_vec();
            all.push((BarButton::Generate, &crate::generation::GENERATE, 40.0, lit.generate));
            all
        };
        for (which, icon, glyph, on) in actions {
            let rect = button(x, pc::BAR_BUTTON_WIDTH);
            tap(ui, which, rect);
            let l = highlight(&ctx, ui.scope_id().with(("lit", which)), on);
            let disc = mix(argb(pc::ACTION_DISC), argb(pc::BAR_GLYPH_SELECTED), l);
            ui.painter().circle_filled(rect.center(), pc::ACTION_DISC_SIZE / 2.0, disc);
            look.icon(ui, icon, rect.center(), glyph, mix(argb(pc::ACTION_GLYPH), Color32::WHITE, l));
            #[cfg(feature = "generation")]
            if which == BarButton::Generate
                && let Some(done) = lit.generating
            {
                progress_ring(ui, rect.center(), pc::ACTION_DISC_SIZE / 2.0 + 2.5, done);
            }
            x += pc::BAR_BUTTON_WIDTH + action_gap;
        }

        // from the colour leftwards
        let mut x = r.right() - pc::BAR_MARGIN - pc::BAR_BUTTON_WIDTH;
        let rect = button(x, pc::BAR_BUTTON_WIDTH);
        let colour_rect = rect;
        ui.painter().circle_filled(rect.center(), pc::COLOR_DOT / 2.0, colour);
        // ringed only where the colour would fade into the bar
        if brightness < DARK_RING_BELOW {
            ui.painter().circle_stroke(
                rect.center(),
                pc::COLOR_DOT / 2.0 - 0.5,
                Stroke::new(1.0, argb(pc::COLOR_RING)),
            );
        }
        let tools = [
            (BarButton::Layers, &icons::LAYERS, lit.layers),
            (BarButton::Erase, &icons::ERASE, !lit.panel_open && tool == ToolKind::Erase),
            (BarButton::Smudge, &icons::SMUDGE, !lit.panel_open && tool == ToolKind::Smudge),
            (BarButton::Paint, &icons::BRUSH, !lit.panel_open && tool == ToolKind::Paint),
        ];
        for (which, icon, on) in tools {
            x -= pc::BAR_BUTTON_WIDTH + tool_gap;
            let rect = button(x, pc::BAR_BUTTON_WIDTH);
            tap(ui, which, rect);
            let l = highlight(&ctx, ui.scope_id().with(("lit", which)), on);
            // a 44 frame shown as it is, over the 38 tall button
            look.icon(ui, icon, rect.center(), 44.0, mix(argb(pc::BAR_GLYPH), argb(pc::BAR_GLYPH_SELECTED), l));
        }
        bar.rects.push((BarButton::Colour, to_window.mul_rect(colour_rect)));
        look.set_zoom(ui.layer_id(), 1.0);
        // what it drew, at its scale, clipped to where it shows
        if k != 1.0 {
            ctx.graphics_mut(|g| {
                let list = g.entry(ui.layer_id());
                for i in first.0..list.next_idx().0 {
                    list.mutate_shape(ShapeIdx(i), |shape| {
                        let clip = shape.clip_rect;
                        shape.transform(to_window);
                        shape.clip_rect = clip;
                    });
                }
            });
        }
    });
    bar
}

/// A run's progress round a bar button: a thin accent arc from the top, clockwise.
#[cfg(feature = "generation")]
fn progress_ring(ui: &Ui, centre: egui::Pos2, radius: f32, done: f32) {
    let steps = 64;
    let end = (done.clamp(0.02, 1.0) * steps as f32).round() as usize;
    let points: Vec<egui::Pos2> = (0..=end)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 / steps as f32 * std::f32::consts::TAU;
            centre + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    ui.painter().add(egui::Shape::line(points, Stroke::new(2.0, argb(pc::ACCENT))));
}

/// The room at either end of half of the top bar moved by Edit layout, past its first and last
/// buttons (the bar's own points).
pub const HALF_END: f32 = 10.0;

/// How long (seconds) a moved bar's corners take to turn square or round as it comes to an edge or
/// leaves it.
const BAR_CORNER_TIME: f32 = 0.15;

/// A bar's corners as Edit layout leaves it: `radius` round all round, but square on each side of it
/// against the window's edges (`sides`), each easing over as the bar is dragged so none snaps (under
/// `id`). Top-left, top-right, bottom-right, bottom-left.
pub fn bar_corners(ctx: &egui::Context, id: Id, sides: procraft_core::BarSides, radius: f32) -> [f32; 4] {
    let square =
        [sides.left || sides.top, sides.top || sides.right, sides.right || sides.bottom, sides.bottom || sides.left];
    let mut corners = [0.0; 4];
    for (i, (corner, square)) in corners.iter_mut().zip(square).enumerate() {
        *corner = ctx.animate_value_with_time(id.with(i), if square { 0.0 } else { radius }, BAR_CORNER_TIME);
    }
    corners
}

/// [`bar_corners`] as egui rounds a rect's (top-left, top-right, bottom-right, bottom-left).
pub fn corner_radius(corners: [f32; 4]) -> egui::CornerRadius {
    let r = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    egui::CornerRadius { nw: r(corners[0]), ne: r(corners[1]), se: r(corners[2]), sw: r(corners[3]) }
}

/// `rect` reaching `past` further out on each of `sides` (those against the window's edges): its
/// frost, so those corners go off the window and only the free ones round.
pub fn past_edges(rect: Rect, sides: procraft_core::BarSides, past: f32) -> Rect {
    let by = |on: bool| if on { past } else { 0.0 };
    Rect::from_min_max(
        rect.min - vec2(by(sides.left), by(sides.top)),
        rect.max + vec2(by(sides.right), by(sides.bottom)),
    )
}

/// How long half of the top bar is (`left`, else the right) laid in a row, its gaps those of a bar
/// `width` wide (its own points): its buttons, gaps and ends.
pub fn half_length(left: bool, width: f32) -> f32 {
    let (tool_gap, action_gap) = pc::bar_gaps(width);
    if left {
        // (and image generation's button after Transform, when it's built in)
        HALF_END * 2.0
            + pc::GALLERY_WIDTH
            + (pc::BAR_BUTTON_WIDTH + action_gap) * if cfg!(feature = "generation") { 5.0 } else { 4.0 }
    } else {
        HALF_END * 2.0 + pc::BAR_BUTTON_WIDTH * 5.0 + tool_gap * 4.0
    }
}

/// The top bar's halves where Edit layout moved them, or as they slide into a side or out of it
/// (`places`, the left's then the right's: its top-left in the window, and whether it stands upright;
/// one `gone` into its side not drawn), each on glass of its own, which goes into
/// `glass`; as [`top_bar`] draws them, at the size a plugin gives the top bar, its buttons where they
/// show. Upright, each button stands in a column, Gallery's word reading up it. Round at its ends,
/// each is square on its sides against the window's edges (`edges`, as `places`).
#[allow(clippy::too_many_arguments)]
pub fn top_bar_halves(
    ctx: &egui::Context,
    look: &mut Look,
    tool: ToolKind,
    colour: Color32,
    brightness: f32,
    lit: &Lit,
    places: [procraft_core::BarPlace; 2],
    edges: [procraft_core::BarSides; 2],
    gone: [bool; 2],
    glass: &mut Vec<f32>,
) -> Bar {
    let mut bar = Bar::default();
    let k = crate::panel::part_scale(ctx, InterfacePart::TopBar);
    let width = ctx.content_rect().width() / k;
    let (tool_gap, action_gap) = pc::bar_gaps(width);
    for (((left, place), sides), gone) in [true, false].into_iter().zip(places).zip(edges).zip(gone) {
        // (collapsed into its side: out of sight, its tab there instead)
        if gone {
            continue;
        }
        let at = pos2(place.x, place.y);
        let length = half_length(left, width);
        let r = Rect::from_min_size(
            at,
            if place.vertical { vec2(pc::BAR_HEIGHT, length) } else { vec2(length, pc::BAR_HEIGHT) },
        );
        let to_window = scaled_from(at, k);
        let round = pc::BAR_HEIGHT / 2.0;
        let corners = bar_corners(ctx, Id::unique("top bar half corners").with(left), sides, round);
        frost(glass, to_window.mul_rect(past_edges(r, sides, round)), round * k, ctx.pixels_per_point());
        let gap = if left { action_gap } else { tool_gap };
        egui::Area::new(Id::unique("top bar half").with(left)).fixed_pos(at).order(Order::Middle).show(ctx, |ui| {
            ui.ctx().set_transform_layer(ui.layer_id(), to_window);
            look.set_zoom(ui.layer_id(), k);
            ui.painter().rect_filled(r, corner_radius(corners), argb(pc::BAR));
            crate::screen::claim(ui, r);
            let mut along = HALF_END;
            let mut next = |w: f32| {
                let rect = if place.vertical {
                    Rect::from_min_size(
                        pos2(r.center().x - pc::BAR_BUTTON_HEIGHT / 2.0, r.top() + along),
                        vec2(pc::BAR_BUTTON_HEIGHT, w),
                    )
                } else {
                    Rect::from_min_size(
                        pos2(r.left() + along, r.center().y - pc::BAR_BUTTON_HEIGHT / 2.0),
                        vec2(w, pc::BAR_BUTTON_HEIGHT),
                    )
                };
                along += w + gap;
                rect
            };
            let mut tap = |ui: &Ui, which: BarButton, rect: Rect| {
                bar.rects.push((which, to_window.mul_rect(rect)));
                let response = ui.interact(rect, ui.scope_id().with(which), Sense::click());
                if response.clicked() {
                    bar.pressed = Some(which);
                }
                let (since, dragging) = ui.input(|i| (i.pointer.press_start_time(), i.pointer.is_decidedly_dragging()));
                if response.is_pointer_button_down_on()
                    && !dragging
                    && let Some(since) = since
                {
                    bar.down = Some((which, since));
                }
            };
            if left {
                let gallery = next(pc::GALLERY_WIDTH);
                tap(ui, BarButton::Gallery, gallery);
                let ink = argb(pc::BAR_TEXT);
                if place.vertical {
                    // reading up the column
                    let galley =
                        crate::look::wrapped(ui.painter(), "Gallery", 18.0, Weight::Medium, ink, f32::INFINITY);
                    let size = galley.size();
                    let corner = pos2(gallery.center().x - size.y / 2.0, gallery.center().y + size.x / 2.0);
                    let shape =
                        egui::epaint::TextShape::new(corner, galley, ink).with_angle(-std::f32::consts::FRAC_PI_2);
                    ui.painter().add(shape);
                } else {
                    text(
                        ui.painter(),
                        pos2(gallery.left(), gallery.center().y - 1.0),
                        Align2::LEFT_CENTER,
                        "Gallery",
                        18.0,
                        Weight::Medium,
                        ink,
                    );
                }
                let actions = [
                    (BarButton::Actions, &icons::ACTIONS, 36.0, lit.actions),
                    (BarButton::Adjustments, &icons::ADJUSTMENTS, 36.0, lit.adjustments),
                    (BarButton::Selection, &icons::SELECTION, 33.0, lit.selection),
                    (BarButton::Transform, &icons::TRANSFORM, 44.0, lit.transform),
                ];
                for (which, icon, glyph, on) in actions {
                    let rect = next(pc::BAR_BUTTON_WIDTH);
                    tap(ui, which, rect);
                    let l = highlight(ui.ctx(), ui.scope_id().with(("lit", which)), on);
                    let disc = mix(argb(pc::ACTION_DISC), argb(pc::BAR_GLYPH_SELECTED), l);
                    ui.painter().circle_filled(rect.center(), pc::ACTION_DISC_SIZE / 2.0, disc);
                    look.icon(ui, icon, rect.center(), glyph, mix(argb(pc::ACTION_GLYPH), Color32::WHITE, l));
                }
            } else {
                let tools = [
                    (BarButton::Paint, &icons::BRUSH, !lit.panel_open && tool == ToolKind::Paint),
                    (BarButton::Smudge, &icons::SMUDGE, !lit.panel_open && tool == ToolKind::Smudge),
                    (BarButton::Erase, &icons::ERASE, !lit.panel_open && tool == ToolKind::Erase),
                    (BarButton::Layers, &icons::LAYERS, lit.layers),
                ];
                for (which, icon, on) in tools {
                    let rect = next(pc::BAR_BUTTON_WIDTH);
                    tap(ui, which, rect);
                    let l = highlight(ui.ctx(), ui.scope_id().with(("lit", which)), on);
                    look.icon(ui, icon, rect.center(), 44.0, mix(argb(pc::BAR_GLYPH), argb(pc::BAR_GLYPH_SELECTED), l));
                }
                let rect = next(pc::BAR_BUTTON_WIDTH);
                ui.painter().circle_filled(rect.center(), pc::COLOR_DOT / 2.0, colour);
                if brightness < DARK_RING_BELOW {
                    ui.painter().circle_stroke(
                        rect.center(),
                        pc::COLOR_DOT / 2.0 - 0.5,
                        Stroke::new(1.0, argb(pc::COLOR_RING)),
                    );
                }
                bar.rects.push((BarButton::Colour, to_window.mul_rect(rect)));
            }
            look.set_zoom(ui.layer_id(), 1.0);
        });
    }
    bar
}

/// What shows `k` times as big from `at`: from where it is laid out to where it shows.
pub fn scaled_from(at: Pos2, k: f32) -> TSTransform {
    TSTransform::from_translation(at.to_vec2())
        * TSTransform::from_scaling(k)
        * TSTransform::from_translation(-at.to_vec2())
}

/// Where the side bar goes in a window: its top left, its size laid out (its tracks fitted to the
/// window), how long each track is, and how many times as big it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideBarFit {
    pub at: Pos2,
    pub size: Vec2,
    pub track: f32,
    pub scale: f32,
}

impl SideBarFit {
    /// Where it is laid out: at its own size, from where it shows.
    pub fn laid_out(self) -> Rect {
        Rect::from_min_size(self.at, self.size)
    }

    /// Where it shows.
    pub fn shown(self) -> Rect {
        Rect::from_min_size(self.at, self.size * self.scale)
    }

    /// From where it is laid out to where it shows.
    pub fn to_window(self) -> TSTransform {
        scaled_from(self.at, self.scale)
    }
}

/// The side bar in `window`, against its right edge or its left, under a top bar reaching
/// `bar_bottom` down, shown `scale` times as big as the rest, its centre kept at `centre` (a share
/// of the window's height): fitted to the window as it measures the window at its own size.
pub fn fit_side_bar(window: Rect, right: bool, scale: f32, bar_bottom: f32, centre: f32) -> SideBarFit {
    let procraft_core::SideBarFit { top, track, .. } =
        procraft_core::side_bar_fit(window.height() / scale, bar_bottom / scale, centre);
    // shorter tracks bring what is under each up with them
    let shorter = pc::SLIDER_HEIGHT - track;
    let left = if right { window.right() - pc::SIDE_BAR_WIDTH * scale } else { window.left() };
    SideBarFit {
        at: pos2(left, window.top() + top * scale),
        size: vec2(pc::SIDE_BAR_WIDTH, pc::SIDE_BAR_HEIGHT - shorter * 2.0),
        track,
        scale,
    }
}

/// The side bar's sliders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slider {
    Size,
    Opacity,
}

/// What the side bar's buttons were asked: undo, redo, and the modify button tapped or held.
#[derive(Default)]
pub struct SideTaps {
    pub undo: bool,
    pub redo: bool,
    pub modify_tapped: bool,
    pub modify_down: bool,
    /// A slider was let go or a notch saved or taken away: the brush's settings changed.
    pub brush_changed: bool,
    /// The bar was moved by its modify button and let go: its centre, to be kept, a share of the
    /// window's height.
    pub centre: Option<f32>,
}

/// The side bar's own state: the slider pressed (its value before a notch pulls it in, when, and
/// whether it moved), the slider whose panel shows, whether the brush's settings changed this
/// frame, the modify button's press (which dragged out moves the bar), and the bar let go settling
/// into its width and place.
#[derive(Default)]
pub struct SideBar {
    pressed: Option<Press>,
    panel: Option<Slider>,
    changed: bool,
    /// How its slider tracks look as a plugin says (interface 11).
    pub slider_look: procraft_core::SliderLook,
    modify: Option<ModifyDrag>,
    settle: Option<Settle>,
    /// Where Edit layout moved it (its top-left in the window), standing upright there; none while
    /// it is against its edge.
    pub moved: Option<Pos2>,
    /// Moved, its sides against the window's edges (square; the rest round).
    pub edges: Option<procraft_core::BarSides>,
}

/// The modify button pressed: the bar's drag, and where the bar shows as it goes.
struct ModifyDrag {
    drag: Arc<SideBarDrag>,
    shown: SideBarShown,
    /// Where the pointer last was (the window's points).
    last: Pos2,
}

/// The side bar let go where its modify button dragged it: where it was then (its top and how far
/// it was stretched toward the pointer, its own points), and when (s).
#[derive(Clone, Copy)]
struct Settle {
    top: f32,
    stretch: f32,
    since: f64,
}

/// Let go, the side bar settles into its width and its place in this long (s), easing out.
const SETTLE_S: f64 = 0.15;

#[derive(Clone, Copy)]
struct Press {
    which: Slider,
    raw: f32,
    since: std::time::Instant,
    moved: bool,
    /// Held still long enough to bring up the panel: let go, it is no tap.
    held: bool,
}

/// A slider held still this long brings up its panel.
const HOLD: std::time::Duration = std::time::Duration::from_millis(400);
const NOTCH_WIDTH: f32 = 20.0;
const NOTCH_HEIGHT: f32 = 4.0;

const SLIDER_LEFT: f32 = 5.0;
const SIZE_TRACK_TOP: f32 = 6.0;
const OPACITY_TRACK_TOP: f32 = 248.0;
const SLIDER_PANEL_SIDE: f32 = 190.0;
const SLIDER_PANEL_GAP: f32 = 8.0;
const SLIDER_PANEL_RISE: f32 = 10.0;
const SLIDER_PANEL_FILL: Themed = Themed(0xC93E3E3E, 0xFFE7E7E7);
const SLIDER_PANEL_INK: Themed = Themed(0xFFF0F0F0, 0xFF393939);

/// The slider tracks' fill: the interface's own, or as see-through as a plugin says (`opacity`, 0 to 1).
fn track_colour(opacity: Option<f32>) -> Color32 {
    let ink = argb(pc::SLIDER_TRACK);
    match opacity.filter(|a| a.is_finite()) {
        Some(a) => {
            let [r, g, b, _] = ink.to_srgba_unmultiplied();
            Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
        }
        None => ink,
    }
}

impl SideBar {
    /// The side bar against the window's left edge (the right one, `right`-handed: everything
    /// mirrored but the glyphs); it and a slider's panel add their frost to `backdrops`, and its
    /// slider tracks their thin glass to `windows` (see [`frost`]). It shows at the size a plugin
    /// gives it, under the top bar as that shows: laid out at its own size, and drawn and touched
    /// that many times as big from its top left, a slider's panel with it. Its centre is kept at
    /// `centre` (a share of the window's height); its modify button dragged out from its edge and
    /// then up or down moves it, and let go it settles into its width there (and says where).
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        info: &CanvasInfo,
        modify_lit: bool,
        right: bool,
        centre: f32,
        backdrops: &mut Vec<f32>,
        windows: &mut Vec<f32>,
    ) -> SideTaps {
        let mut taps = SideTaps::default();
        let ctx = ui.ctx().clone();
        let window = ctx.content_rect();
        let scale = crate::panel::part_scale(&ctx, InterfacePart::SideBar);
        let bar_bottom = crate::panel::bar_shown(&ctx);
        let now = ctx.input(|i| i.time);
        let rest = fit_side_bar(window, right, scale, bar_bottom, centre);
        // where Edit layout moved it, if it did
        let rest = match self.moved {
            Some(at) => SideBarFit { at, ..rest },
            None => rest,
        };
        let fit = self.shown_at(&ctx, rest, right, now);
        // stretched toward the pointer: placed that far out, and reaching back to the window's edge
        let stretch = (fit.at.x - rest.at.x).abs() / scale;
        let track_height = fit.track;
        // shorter tracks bring what is under each up with them
        let shorter = pc::SLIDER_HEIGHT - track_height;
        let opacity_top = OPACITY_TRACK_TOP - shorter;
        let (bar, shown, to_window) = (fit.laid_out(), fit.shown(), fit.to_window());
        // a control's left edge in the bar, mirrored on the right
        let x = |left: f32, width: f32| if right { pc::SIDE_BAR_WIDTH - left - width } else { left };
        let at = |l: f32, y: f32, w: f32, h: f32| Rect::from_min_size(bar.min + vec2(x(l, w), y), vec2(w, h));
        let tracks = [(Slider::Size, SIZE_TRACK_TOP, info.size), (Slider::Opacity, opacity_top, info.opacity)];
        // its frost goes past the window's edge, so that side's corners are square
        let ppp = ctx.pixels_per_point();
        let body = side_bar_body(bar, stretch, right);
        // (moved by Edit layout: square only on its sides against the window's edges)
        let c = pc::SIDE_BAR_CORNER;
        let corners = match self.edges {
            Some(sides) => bar_corners(&ctx, Id::unique("side bar corners"), sides, c),
            None if right => [c, 0.0, 0.0, c],
            None => [0.0, c, c, 0.0],
        };
        let past = match self.edges {
            Some(sides) => past_edges(body, sides, c),
            None if right => body.with_max_x(body.right() + c),
            None => body.with_min_x(body.left() - c),
        };
        frost(backdrops, to_window.mul_rect(past), pc::SIDE_BAR_CORNER * scale, ppp);
        // the tracks' thin glass, unless a plugin has them frosted as the bar's glass under them is
        if self.slider_look.frosted != Some(true) {
            for (_, y, _) in tracks {
                let track = to_window.mul_rect(at(SLIDER_LEFT, y, pc::SLIDER_WIDTH, track_height));
                frost(windows, track, pc::SLIDER_CORNER * scale, ppp);
            }
        }
        let area = Id::unique("side bar");
        ctx.set_transform_layer(LayerId::new(Order::Middle, area), to_window);
        look.set_zoom(LayerId::new(Order::Middle, area), scale);
        // kept to the window, and clipped to it, as the bar measures it
        let measured = to_window.inverse().mul_rect(window);
        egui::Area::new(area).fixed_pos(bar.min).order(Order::Middle).constrain_to(measured).show(&ctx, |ui| {
            ui.set_min_size(bar.size());
            // translucent, with windows where the slider tracks are: they show what is behind;
            // square against the window's edge, reaching back to it however it is stretched
            let inset = bar.left() - body.left();
            let key = (right, track_height.to_bits(), stretch.to_bits(), corners.map(f32::to_bits));
            look.shape(ui, "side bar", key, body, |pixmap, t| {
                let mut pb = tiny_skia::PathBuilder::new();
                round_rect(&mut pb, Rect::from_min_size(pos2(0.0, 0.0), body.size()), corners);
                let windows = tracks.map(|(_, y, _)| {
                    let left = x(SLIDER_LEFT, pc::SLIDER_WIDTH) + inset;
                    Rect::from_min_size(pos2(left, y), vec2(pc::SLIDER_WIDTH, track_height))
                });
                for window in windows {
                    round_rect(&mut pb, window, [pc::SLIDER_CORNER; 4]);
                }
                fill(pixmap, pb.finish(), argb(pc::SIDE_BAR), t);
                // a darker hairline round each window
                let mut ring = tiny_skia::PathBuilder::new();
                for window in windows {
                    let edge = pc::SLIDER_EDGE_WIDTH;
                    round_rect(&mut ring, window.expand(edge), [pc::SLIDER_CORNER + edge; 4]);
                    round_rect(&mut ring, window, [pc::SLIDER_CORNER; 4]);
                }
                fill(pixmap, ring.finish(), argb(pc::SLIDER_EDGE), t);
            });
            for (which, y, value) in tracks {
                let rect = at(SLIDER_LEFT, y, pc::SLIDER_WIDTH, track_height);
                self.slider(ui, look, session, which, rect, value);
            }
            // a slider held still brings up its panel too
            if let Some(press) = self.pressed.as_mut().filter(|p| !p.moved && !p.held) {
                if press.since.elapsed() >= HOLD {
                    press.held = true;
                    self.panel = Some(press.which);
                } else {
                    ui.ctx().request_repaint_after(HOLD - press.since.elapsed());
                }
            }
            // the modify button: held, it changes what a canvas touch does; tapped, it does what
            // Tap □ is set to (opens the eyedropper)
            let modify = at(5.0, 191.0 - shorter, 40.0, 41.0);
            let response = ui.interact(modify, ui.scope_id().with("modify"), Sense::click_and_drag());
            let down = response.is_pointer_button_down_on();
            // dragged out from the bar's edge it moves the bar, and is no press
            let place = (window, scale, bar_bottom, centre);
            let moved = self.modify_drag(&ctx, down, right, place, now, &mut taps);
            taps.modify_down = down && !moved;
            taps.modify_tapped = response.clicked() && !moved;
            let square = egui::Rect::from_center_size(modify.center(), vec2(pc::MODIFY_SIZE, pc::MODIFY_SIZE));
            let w = pc::MODIFY_STROKE;
            ui.painter().rect_stroke(
                square.shrink(w / 2.0),
                pc::MODIFY_CORNER - w / 2.0,
                Stroke::new(w, if modify_lit { argb(pc::ACCENT) } else { argb(pc::SIDE_GLYPH) }),
                egui::StrokeKind::Middle,
            );
            for (icon, enabled, y, undo) in
                [(&icons::UNDO, info.can_undo, 430.5, true), (&icons::REDO, info.can_redo, 472.5, false)]
            {
                let rect = at(5.0, y - shorter * 2.0, 40.0, 41.0);
                let tapped = ui.interact(rect, ui.scope_id().with(("side", undo)), Sense::click()).clicked();
                if tapped && enabled {
                    if undo {
                        taps.undo = true;
                    } else {
                        taps.redo = true;
                    }
                }
                // a 50 frame shown 36 across; turning on or off (a stroke kept, the last step
                // undone) fades over 0.15 s either way, linear, as measured: the lit glyph over
                // the unlit one
                let lit = ui.ctx().animate_bool_with_time(ui.scope_id().with(("side lit", undo)), enabled, 0.15);
                if lit < 1.0 {
                    look.icon(ui, icon, rect.center(), 36.0, argb(pc::SIDE_GLYPH_DISABLED));
                }
                if lit > 0.0 {
                    look.icon(ui, icon, rect.center(), 36.0, argb(pc::SIDE_GLYPH).gamma_multiply(lit));
                }
            }
        });
        if let Some(which) = self.panel {
            let y = if which == Slider::Size { SIZE_TRACK_TOP } else { opacity_top };
            let left = x(SLIDER_LEFT + pc::SLIDER_WIDTH + SLIDER_PANEL_GAP, SLIDER_PANEL_SIDE);
            let min = bar.min + vec2(left, y - SLIDER_PANEL_RISE);
            let rect = Rect::from_min_size(min, vec2(SLIDER_PANEL_SIDE, SLIDER_PANEL_SIDE));
            self.changed |= slider_panel(&ctx, look, session, which, rect, info, to_window, backdrops);
            // it stays up after the slider is let go, for its + to be pressed, and goes at the next
            // press anywhere but on it and the side bar (which goes on to do what it does there)
            let panel = to_window.mul_rect(rect);
            let pressed_elsewhere = ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !panel.contains(p) && !shown.contains(p))
            });
            if pressed_elsewhere {
                self.panel = None;
            }
        }
        taps.brush_changed = std::mem::take(&mut self.changed);
        taps
    }

    /// Where the bar shows, `rest` where it is kept: as the modify button drags it (placed out by
    /// its stretch toward the pointer), settling after (at `now`, s), or there.
    fn shown_at(&mut self, ctx: &egui::Context, rest: SideBarFit, right: bool, now: f64) -> SideBarFit {
        let window = ctx.content_rect();
        let rest_top = (rest.at.y - window.top()) / rest.scale;
        let moving = self.modify.as_ref().map(|m| m.shown).filter(|s| s.state == SideBarDragState::Moving);
        let (top, stretch) = if let Some(shown) = moving {
            (shown.top, shown.stretch)
        } else if let Some(settle) = self.settle {
            let t = ((now - settle.since) / SETTLE_S) as f32;
            if t >= 1.0 {
                self.settle = None;
                (rest_top, 0.0)
            } else {
                ctx.request_repaint();
                let e = crate::look::cubic_bezier(0.0, 0.0, 0.58, 1.0, t);
                (settle.top + (rest_top - settle.top) * e, settle.stretch * (1.0 - e))
            }
        } else {
            (rest_top, 0.0)
        };
        let outward = if right { -1.0 } else { 1.0 };
        SideBarFit { at: pos2(rest.at.x + stretch * rest.scale * outward, window.top() + top * rest.scale), ..rest }
    }

    /// The modify button's press as it goes (`down` on it or not, at `now`, s), the bar in `place`
    /// (the window, how big the bar shows, the top bar's bottom, and its kept centre): the bar's
    /// drag started, told where the pointer goes (the bar's points), and let go; let go moved, the
    /// bar settles into its width and place and its centre goes in `taps`. Whether it moves the bar.
    fn modify_drag(
        &mut self,
        ctx: &egui::Context,
        down: bool,
        right: bool,
        place: (Rect, f32, f32, f32),
        now: f64,
        taps: &mut SideTaps,
    ) -> bool {
        let (window, scale, bar_bottom, centre) = place;
        let (origin, latest) = ctx.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
        let ms = now * 1000.0;
        // the window's points as the bar's
        let local = |p: Pos2| (p - window.min) / scale;
        match (&mut self.modify, down) {
            (None, true) => {
                let Some(at) = origin.or(latest) else { return false };
                let p = local(at);
                let (height, top_bar) = (window.height() / scale, bar_bottom / scale);
                let drag = SideBarDrag::new(p.x, p.y, ms, right, height, top_bar, centre);
                let shown = drag.now();
                self.modify = Some(ModifyDrag { drag, shown, last: at });
                self.settle = None;
            }
            (Some(m), true) => {
                if let Some(at) = latest.filter(|&at| at != m.last) {
                    let p = local(at);
                    m.shown = m.drag.moved(p.x, p.y, ms);
                    m.last = at;
                    ctx.request_repaint();
                }
            }
            (Some(_), false) => {
                if let Some(m) = self.modify.take() {
                    let kept = m.drag.let_go();
                    if kept.state == SideBarDragState::Moving {
                        taps.centre = Some(kept.centre);
                        self.settle = Some(Settle { top: m.shown.top, stretch: m.shown.stretch, since: now });
                        ctx.request_repaint();
                        return true;
                    }
                }
            }
            (None, false) => {}
        }
        self.modify.as_ref().is_some_and(|m| m.shown.state == SideBarDragState::Moving)
    }

    /// An upright slider moved by dragging anywhere on it: the thumb follows the pointer (not
    /// jumping to it), finer the farther out to the side the pointer goes, 1 at the top. Moved or
    /// held still, its panel comes up and stays up when it is let go; its thumb tapped brings the
    /// panel up or takes it away; a tap elsewhere on the track takes the thumb there (and the
    /// panel away). Its notches (saved values) show as marks on the track; a value dragged near
    /// one is pulled onto it, and a tap on one goes there. On a notch, the thumb shows a blue bar.
    fn slider(&mut self, ui: &Ui, look: &mut Look, session: &CanvasSession, which: Slider, rect: Rect, value: f32) {
        let size = which == Slider::Size;
        let travel = rect.height() - pc::THUMB_HEIGHT - pc::THUMB_INSET * 2.0;
        let response = ui.interact(rect, ui.scope_id().with(("slider", which)), Sense::click_and_drag());
        let set = |v: f32| match which {
            Slider::Size => session.set_brush_size(v),
            Slider::Opacity => session.set_brush_opacity(v),
        };
        if response.is_pointer_button_down_on() && self.pressed.is_none_or(|p| p.which != which) {
            self.pressed =
                Some(Press { which, raw: value, since: std::time::Instant::now(), moved: false, held: false });
        }
        if let Some(press) = self.pressed.as_mut().filter(|p| p.which == which) {
            let dy = response.drag_delta().y;
            if dy != 0.0 {
                if !press.moved {
                    self.panel = Some(which);
                }
                press.moved = true;
                // the farther out to the side the pointer is, the finer the thumb follows it
                let across = response.interact_pointer_pos().map_or(0.0, |p| p.x - rect.center().x);
                press.raw = (press.raw - dy / travel * side_slider_drag_share(across)).clamp(0.0, 1.0);
                set(session.slider_notch_at(size, press.raw).unwrap_or(press.raw));
            }
        }
        // a tap on the thumb brings up the panel (or takes it away); elsewhere the thumb goes
        // there (or onto a notch's mark there)
        if response.clicked()
            && !self.pressed.is_some_and(|p| p.held)
            && let Some(p) = response.interact_pointer_pos()
        {
            let at = 1.0 - (p.y - rect.top() - pc::THUMB_INSET - pc::THUMB_HEIGHT / 2.0) / travel;
            let reach = pc::THUMB_HEIGHT / 2.0 / travel;
            match side_slider_tap(at, value, reach, session.slider_notches(size), self.panel.is_some()) {
                SideSliderTap::Panel => self.panel = Some(which),
                SideSliderTap::ClosePanel => self.panel = None,
                SideSliderTap::Jump { value } => {
                    set(value);
                    self.panel = None;
                }
            }
        }
        // let go, the panel stays up
        if !response.is_pointer_button_down_on() && self.pressed.is_some_and(|p| p.which == which) {
            self.pressed = None;
            self.changed = true;
        }
        let notches = session.slider_notches(size);
        let on_notch = notches.iter().any(|n| (n - value).abs() < 1e-4);
        let slot = match which {
            Slider::Size => "size slider",
            Slider::Opacity => "opacity slider",
        };
        // the track as a plugin has it see-through, else as the interface has it
        let track_ink = track_colour(self.slider_look.track_opacity);
        let key = (
            (value * 10_000.0).round() as i32,
            notches.iter().map(|n| (n * 10_000.0) as i32).collect::<Vec<_>>(),
            track_ink.to_array(),
        );
        look.shape(ui, slot, key, rect, |pixmap, t| {
            let size = rect.size();
            let thumb = vec2(pc::THUMB_WIDTH, pc::THUMB_HEIGHT);
            let inset = pc::THUMB_INSET;
            let knob_at = pos2((size.x - thumb.x) / 2.0, inset + (size.y - thumb.y - inset * 2.0) * (1.0 - value));
            let knob = Rect::from_min_size(knob_at, thumb);
            let rim = pc::SLIDER_RIM_WIDTH;
            let outer = Rect::from_min_size(pos2(0.0, 0.0), size);
            let inside = outer.shrink(rim);
            let corner = pc::SLIDER_CORNER;
            // the track has a window where the thumb is, so the thumb shows over the bar alone;
            // a lighter rim runs round it
            let mut track = tiny_skia::PathBuilder::new();
            round_rect(&mut track, inside, [corner - rim; 4]);
            round_rect(&mut track, knob, [thumb.y / 2.0; 4]);
            fill(pixmap, track.finish(), track_ink, t);
            let mut ring = tiny_skia::PathBuilder::new();
            round_rect(&mut ring, outer, [corner; 4]);
            round_rect(&mut ring, inside, [corner - rim; 4]);
            fill(pixmap, ring.finish(), argb(pc::SLIDER_RIM), t);
            // the notches: short bars across the track
            let bar = vec2(NOTCH_WIDTH, NOTCH_HEIGHT);
            let travel = size.y - thumb.y - inset * 2.0;
            for n in &notches {
                let y = inset + thumb.y / 2.0 + travel * (1.0 - n);
                let mut mark = tiny_skia::PathBuilder::new();
                round_rect(&mut mark, Rect::from_center_size(pos2(size.x / 2.0, y), bar), [bar.y / 2.0; 4]);
                fill(pixmap, mark.finish(), argb(pc::NOTCH_MARK), t);
            }
            // the thumb's soft shadow, a point below it: under the thumb, only its rim shows
            for step in side_thumb_shadow() {
                let mut shade = tiny_skia::PathBuilder::new();
                let grown = knob.expand(step.grow).translate(vec2(0.0, step.down));
                round_rect(&mut shade, grown, [thumb.y / 2.0 + step.grow; 4]);
                fill(pixmap, shade.finish(), Color32::from_black_alpha((step.alpha * 255.0).round() as u8), t);
            }
            let mut thumb_path = tiny_skia::PathBuilder::new();
            round_rect(&mut thumb_path, knob, [thumb.y / 2.0; 4]);
            fill(pixmap, thumb_path.finish(), argb(pc::SLIDER_THUMB), t);
            // on a notch, the thumb shows a blue bar
            if on_notch {
                let mut mark = tiny_skia::PathBuilder::new();
                round_rect(&mut mark, Rect::from_center_size(knob.center(), bar), [bar.y / 2.0; 4]);
                fill(pixmap, mark.finish(), argb(pc::ACCENT), t);
            }
        });
    }
}

/// A slider's panel: its name and value ("Size 8%"), a button saving the value as a notch on the
/// slider ("+"; "-" takes away the notch it is on), and in the middle the brush's shape (a dot for
/// a round tip) as it would paint now, its size on the screen (and at its opacity). It shows as
/// the side bar does, through `to_window`.
#[allow(clippy::too_many_arguments)]
fn slider_panel(
    ctx: &egui::Context,
    look: &mut Look,
    session: &CanvasSession,
    which: Slider,
    rect: Rect,
    info: &CanvasInfo,
    to_window: TSTransform,
    backdrops: &mut Vec<f32>,
) -> bool {
    let mut toggled = false;
    let (ppp, scale) = (ctx.pixels_per_point(), to_window.scaling);
    let corner = 22.0;
    frost(backdrops, to_window.mul_rect(rect), corner * scale, ppp);
    let area = Id::unique("slider panel");
    ctx.set_transform_layer(LayerId::new(Order::Foreground, area), to_window);
    look.set_zoom(LayerId::new(Order::Foreground, area), scale);
    let measured = to_window.inverse().mul_rect(ctx.content_rect());
    egui::Area::new(area).fixed_pos(rect.min).order(Order::Foreground).constrain_to(measured).show(ctx, |ui| {
        ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, corner, argb(SLIDER_PANEL_FILL));
        // a hairline rim
        painter.rect_stroke(rect, corner, Stroke::new(1.0, argb(pc::GLASS_MENU_EDGE)), egui::StrokeKind::Inside);
        let (name, value) = match which {
            Slider::Size => ("Size", info.size),
            Slider::Opacity => ("Opacity", info.opacity),
        };
        let shown = format!(" {}%", side_slider_shown_percent(which == Slider::Size, value));
        let line = pos2(rect.left() + 16.0, rect.top() + 6.0 + 16.0);
        text_runs(
            painter,
            line,
            Align2::LEFT_CENTER,
            &[(name, Weight::Bold), (&shown, Weight::Regular)],
            19.0,
            argb(SLIDER_PANEL_INK),
        );
        // its size on the screen, however big the panel shows
        let diameter = info.brush_diameter * info.zoom / ppp / scale;
        let r = (diameter / 2.0).clamp(1.5, (SLIDER_PANEL_SIDE / 2.0 - 8.0).max(1.5));
        let alpha = if which == Slider::Size { 1.0 } else { info.opacity.clamp(0.02, 1.0) };
        // the brush's stamp as it paints (soft where it is soft) in a picture its size on the
        // screen, kept while it stays so
        let side = ((2.0 * r * ppp * scale).ceil() as usize).max(2);
        let stamp = session.brush_stamp(side as u32);
        let key = stamp_key(&stamp, side);
        let id = egui::Id::unique("size preview stamp");
        let kept = ctx.data(|d| d.get_temp::<(u64, egui::TextureHandle)>(id)).filter(|(k, _)| *k == key);
        let texture = kept.map(|(_, t)| t).unwrap_or_else(|| {
            let pixels = stamp.iter().map(|&a| Color32::from_white_alpha(a)).collect();
            let picture = egui::ColorImage { size: [side, side], pixels, ..Default::default() };
            let t = ctx.load_texture("size preview stamp", picture, egui::TextureOptions::LINEAR);
            ctx.data_mut(|d| d.insert_temp(id, (key, t.clone())));
            t
        });
        let shown = Rect::from_center_size(rect.center(), vec2(2.0 * r, 2.0 * r));
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        painter.image(texture.id(), shown, uv, Color32::WHITE.gamma_multiply(alpha));
        let size = which == Slider::Size;
        let value = if size { info.size } else { info.opacity };
        let on_notch = session.slider_notch_at(size, value).is_some();
        if on_notch || session.can_save_slider_notch(size) {
            let button = Rect::from_min_size(rect.min + vec2(150.0, 6.0), vec2(32.0, 32.0));
            let icon = if on_notch { &icons::NOTCH_REMOVE } else { &icons::NOTCH_ADD };
            look.icon(ui, icon, button.center(), 24.0, argb(SLIDER_PANEL_INK));
            if ui.interact(button, ui.scope_id().with("notch"), Sense::click()).clicked() {
                session.toggle_slider_notch(size);
                toggled = true;
            }
        }
    });
    toggled
}

/// What a stamp's picture (`side` pixels square) is known by, to make its texture once.
pub fn stamp_key(stamp: &[u8], side: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    side.hash(&mut h);
    stamp.hash(&mut h);
    h.finish()
}

/// The side bar's body laid out at `bar` (its own points, placed out from its edge by `stretch`
/// toward the pointer): it reaches back as far on its edge side (the left, or the right if `right`),
/// so that side stays against the window's edge.
fn side_bar_body(bar: Rect, stretch: f32, right: bool) -> Rect {
    let stretch = stretch.max(0.0);
    if right { bar.with_max_x(bar.right() + stretch) } else { bar.with_min_x(bar.left() - stretch) }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::emath::TSTransform;
    use egui::{Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
    use procraft_core::CanvasSession;

    use super::{
        HOLD, SIZE_TRACK_TOP, SLIDER_LEFT, SLIDER_PANEL_GAP, SLIDER_PANEL_RISE, SideBar, SideTaps, Slider,
        fit_side_bar, side_bar_body,
    };
    use crate::look::{Look, pc};

    /// The side bar over a canvas, with a pointer on it as the window's reach the interface: its
    /// centre kept where its modify button last left it, and what it was asked each frame.
    struct Hand {
        ctx: egui::Context,
        look: Look,
        side: SideBar,
        session: Arc<CanvasSession>,
        time: f64,
        centre: f32,
        taps: Vec<SideTaps>,
    }

    const WINDOW: egui::Vec2 = vec2(1200.0, 900.0);

    impl Hand {
        fn new() -> Hand {
            let session = CanvasSession::new(64, 64);
            let mut hand = Hand {
                ctx: egui::Context::default(),
                look: Look::default(),
                side: SideBar::default(),
                session,
                time: 0.0,
                centre: procraft_core::side_bar_first_centre(),
                taps: Vec::new(),
            };
            hand.frame(Vec::new());
            hand
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 1.0 / 60.0;
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, WINDOW)),
                time: Some(self.time),
                events,
                ..RawInput::default()
            };
            let Hand { ctx, look, side, session, centre, taps, .. } = self;
            let mut output = ctx.clone().run_ui(raw, |ui| {
                let info = session.info();
                let tapped =
                    side.show(ui, look, session, &info, false, false, *centre, &mut Vec::new(), &mut Vec::new());
                if let Some(kept) = tapped.centre {
                    *centre = kept;
                }
                taps.push(tapped);
            });
            output.textures_delta.clear();
        }

        /// The bar's frost and its tracks' thin glass, as one frame lays them.
        fn glass(&mut self) -> (Vec<f32>, Vec<f32>) {
            let (mut frost, mut windows) = (Vec::new(), Vec::new());
            let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, WINDOW)), ..RawInput::default() };
            let Hand { ctx, look, side, session, centre, .. } = self;
            let mut output = ctx.clone().run_ui(raw, |ui| {
                let info = session.info();
                side.show(ui, look, session, &info, false, false, *centre, &mut frost, &mut windows);
            });
            output.textures_delta.clear();
            (frost, windows)
        }

        /// The pointer, pressed, moved to `to` from `from` in `steps` frames.
        fn moves(&mut self, from: Pos2, to: Pos2, steps: u32) {
            for k in 1..=steps {
                let at = from + (to - from) * (k as f32 / steps as f32);
                self.frame(vec![Event::PointerMoved(at)]);
            }
        }

        /// Where the side bar shows now (its area, as drawn).
        fn shown(&self) -> Rect {
            let id = egui::Id::unique("side bar");
            self.ctx.memory(|m| m.area_rect(id)).expect("the side bar shows")
        }

        /// The middle of the modify button, where the bar shows now.
        fn modify(&self) -> Pos2 {
            let bar = self.shown();
            pos2(bar.left() + 25.0, bar.top() + 191.0 + 20.5)
        }

        fn button(at: Pos2, pressed: bool) -> Event {
            Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
        }

        fn down(&mut self, at: Pos2) {
            self.frame(vec![Event::PointerMoved(at), Self::button(at, true)]);
        }

        fn up(&mut self, at: Pos2) {
            self.frame(vec![Self::button(at, false)]);
            self.frame(Vec::new());
        }

        fn tap(&mut self, at: Pos2) {
            self.down(at);
            self.up(at);
        }

        /// Pressed at `from`, dragged to `to` in steps, and let go there.
        fn drag(&mut self, from: Pos2, to: Pos2) {
            self.down(from);
            for k in 1..=8 {
                let at = from + (to - from) * (k as f32 / 8.0);
                self.frame(vec![Event::PointerMoved(at)]);
            }
            self.up(to);
        }

        fn bar(&self) -> Rect {
            fit_side_bar(Rect::from_min_size(Pos2::ZERO, WINDOW), false, 1.0, pc::BAR_HEIGHT, self.centre).shown()
        }

        /// Where the size slider's thumb sits for `value`.
        fn on_size(&self, value: f32) -> Pos2 {
            let fit = fit_side_bar(Rect::from_min_size(Pos2::ZERO, WINDOW), false, 1.0, pc::BAR_HEIGHT, self.centre);
            let travel = fit.track - pc::THUMB_HEIGHT - pc::THUMB_INSET * 2.0;
            let top = self.bar().top() + SIZE_TRACK_TOP + pc::THUMB_INSET + pc::THUMB_HEIGHT / 2.0;
            pos2(self.bar().left() + SLIDER_LEFT + pc::SLIDER_WIDTH / 2.0, top + travel * (1.0 - value))
        }

        /// The size panel's button: "+" saves the size, "-" takes away the saved size it is on.
        fn panel_button(&self) -> Pos2 {
            let left = self.bar().left() + SLIDER_LEFT + pc::SLIDER_WIDTH + SLIDER_PANEL_GAP;
            pos2(left + 150.0 + 16.0, self.bar().top() + SIZE_TRACK_TOP - SLIDER_PANEL_RISE + 6.0 + 16.0)
        }

        fn size(&self) -> f32 {
            self.session.info().size
        }
    }

    /// The size slider dragged and let go: its panel stays up, and its "+" saves the size as a mark
    /// on the track. A press on the canvas takes the panel away (and goes on to the canvas); a tap
    /// on the mark goes back to the size; on it, the thumb tapped brings up the panel, whose "-"
    /// takes the saved size away; the thumb tapped again takes the panel away. Held still, the
    /// slider brings the panel up too, and let go it stays.
    #[test]
    fn a_plugin_s_slider_look_frosts_the_tracks_and_lets_them_show_through() {
        let mut hand = Hand::new();
        // as the app has them: the bar's frost and each track's thin glass over it
        let (frost, windows) = hand.glass();
        assert_eq!((frost.len(), windows.len()), (6, 12));
        assert_eq!(super::track_colour(None), crate::look::argb(pc::SLIDER_TRACK));
        // frosted: the bar's frost goes on under them, and no thin glass
        hand.side.slider_look = procraft_core::SliderLook { track_opacity: Some(0.25), frosted: Some(true) };
        let (frost, windows) = hand.glass();
        assert_eq!((frost.len(), windows.len()), (6, 0));
        // see-through as it says, the colour kept
        let [r, g, b, a] = super::track_colour(Some(0.25)).to_srgba_unmultiplied();
        let [r0, g0, b0, _] = crate::look::argb(pc::SLIDER_TRACK).to_srgba_unmultiplied();
        // (kept premultiplied: a channel comes back within a few levels)
        assert_eq!(a, 64);
        for (c, c0) in [(r, r0), (g, g0), (b, b0)] {
            assert!(c.abs_diff(c0) <= 3, "{c} {c0}");
        }
        assert_eq!(super::track_colour(Some(f32::NAN)), crate::look::argb(pc::SLIDER_TRACK));
    }

    #[test]
    fn a_size_is_saved_from_the_panel_left_up_after_a_drag_gone_back_to_and_taken_away() {
        let mut h = Hand::new();
        h.session.set_brush_size(0.5);
        h.frame(Vec::new());
        // dragged down and let go: a smaller size, and the panel still up
        h.drag(h.on_size(0.5), h.on_size(0.5) + vec2(0.0, 40.0));
        let saved = h.size();
        assert!(saved < 0.45, "the drag left the size at {saved}");
        assert_eq!(h.side.panel, Some(Slider::Size));
        // its "+" pressed: the size saved, and the panel stays
        h.tap(h.panel_button());
        assert_eq!(h.session.slider_notches(true), vec![saved]);
        assert_eq!(h.side.panel, Some(Slider::Size));
        // a press on the canvas takes it away
        h.tap(pos2(700.0, 500.0));
        assert_eq!(h.side.panel, None);
        // dragged back up, then a tap on the saved size's mark: back on it, and no panel
        h.drag(h.on_size(saved), h.on_size(saved) - vec2(0.0, 60.0));
        assert!(h.size() > saved + 0.1);
        assert_eq!(h.side.panel, Some(Slider::Size));
        h.tap(h.on_size(saved));
        assert!((h.size() - saved).abs() < 1e-6, "{} against {saved}", h.size());
        assert_eq!(h.side.panel, None);
        // on it, the thumb tapped: the panel, offering "-", which takes the saved size away
        h.tap(h.on_size(saved));
        assert_eq!(h.side.panel, Some(Slider::Size));
        assert!(!h.session.can_save_slider_notch(true));
        h.tap(h.panel_button());
        assert!(h.session.slider_notches(true).is_empty());
        // the thumb tapped with the panel up: it goes
        h.tap(h.on_size(saved));
        assert_eq!(h.side.panel, None);
        // held still: the panel comes up, and let go it stays
        h.down(h.on_size(saved));
        std::thread::sleep(HOLD + std::time::Duration::from_millis(50));
        h.frame(Vec::new());
        assert_eq!(h.side.panel, Some(Slider::Size));
        h.up(h.on_size(saved));
        assert_eq!(h.side.panel, Some(Slider::Size));
    }

    /// The modify button dragged out from the bar's edge and down: the bar follows (stretched a
    /// little toward the pointer, its edge side kept against the window's edge), the button neither
    /// held nor tapped once it moves; let go, it settles into its width where it was taken and is
    /// kept there. A press on the canvas dragged over the button,
    /// and a drag down the button first, move nothing; a tap on it is a tap.
    #[test]
    fn the_modify_button_dragged_out_and_down_moves_the_bar_and_a_stroke_from_the_canvas_never_does() {
        // within a pixel: the area lands on whole ones
        let near = |a: Rect, b: Rect| (a.min - b.min).length() < 0.6 && (a.max - b.max).length() < 0.6;
        let mut h = Hand::new();
        let rest = h.bar();
        assert!(near(h.shown(), rest), "{:?} against {rest:?}", h.shown());
        let first = h.centre;
        // a press on the canvas, dragged across the bar and the button and down: nothing moves
        let on_button = h.modify();
        h.down(pos2(700.0, on_button.y));
        h.moves(pos2(700.0, on_button.y), on_button, 12);
        h.moves(on_button, on_button + vec2(0.0, 120.0), 8);
        h.up(on_button + vec2(0.0, 120.0));
        assert!(near(h.shown(), rest));
        assert!(h.taps.iter().all(|t| t.centre.is_none() && !t.modify_down && !t.modify_tapped));
        assert_eq!(h.centre, first);
        // the button pressed and dragged down first: held, not moving the bar
        h.taps.clear();
        h.down(on_button);
        h.moves(on_button, on_button + vec2(0.0, 60.0), 6);
        assert!(h.taps.last().is_some_and(|t| t.modify_down));
        h.up(on_button + vec2(0.0, 60.0));
        assert!(near(h.shown(), rest));
        assert_eq!(h.centre, first);
        // tapped: a tap
        h.taps.clear();
        h.tap(on_button);
        assert!(h.taps.iter().any(|t| t.modify_tapped));
        // pressed, dragged out 20 and down 100: the bar follows down, stretched 5 toward the
        // pointer (the move begun at 15) with its edge side still against the window's, and the
        // press lets go
        h.taps.clear();
        h.down(on_button);
        h.moves(on_button, on_button + vec2(20.0, 0.0), 4);
        h.moves(on_button + vec2(20.0, 0.0), on_button + vec2(20.0, 100.0), 10);
        h.frame(Vec::new());
        let moved = h.shown();
        assert!(near(moved, rest.translate(vec2(5.0, 100.0))), "{moved:?} from {rest:?}");
        let stretch = moved.left() - rest.left();
        let body = side_bar_body(moved, stretch, false);
        eprintln!("dragged 20 out: stretched {stretch}, the body {:.1}..{:.1}", body.left(), body.right());
        assert!(body.left().abs() < 0.6 && stretch <= 8.0, "{body:?}");
        assert!((body.right() - (rest.right() + 5.0)).abs() < 0.6, "{body:?}");
        assert!(h.taps.last().is_some_and(|t| !t.modify_down));
        // let go: kept there, and back at its edge once it has slid
        h.up(on_button + vec2(20.0, 100.0));
        for _ in 0..12 {
            h.frame(Vec::new());
        }
        assert!(h.taps.iter().all(|t| !t.modify_tapped));
        assert_eq!(h.taps.iter().filter(|t| t.centre.is_some()).count(), 1);
        assert!((h.centre - (rest.top() + 100.0 + pc::SIDE_BAR_HEIGHT / 2.0) / WINDOW.y).abs() < 1e-4);
        assert!(near(h.bar(), rest.translate(vec2(0.0, 100.0))));
        assert!(near(h.shown(), h.bar()), "{:?}", h.shown());
    }

    #[test]
    fn the_side_bar_as_big_as_the_rest_is_where_it_was_measured() {
        for (height, right) in [(1133.0, false), (744.0, true), (500.0, false), (500.0, true)] {
            let window = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, height));
            let centre = procraft_core::side_bar_first_centre();
            let fit = fit_side_bar(window, right, 1.0, pc::BAR_HEIGHT, centre);
            let procraft_core::SideBarFit { top, track, .. } =
                procraft_core::side_bar_fit(height, pc::BAR_HEIGHT, centre);
            let left = if right { 1000.0 - pc::SIDE_BAR_WIDTH } else { 0.0 };
            let tall = pc::SIDE_BAR_HEIGHT - (pc::SLIDER_HEIGHT - track) * 2.0;
            let measured = Rect::from_min_size(pos2(left, top), vec2(pc::SIDE_BAR_WIDTH, tall));
            assert_eq!((fit.laid_out(), fit.shown(), fit.track), (measured, measured, track), "{height}");
            assert_eq!(fit.to_window(), TSTransform::IDENTITY);
        }
    }

    #[test]
    fn the_side_bar_bigger_or_smaller_keeps_to_its_edge_under_the_top_bar_as_it_shows_and_in_the_window() {
        for height in (480..1400).step_by(7) {
            let window = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, height as f32));
            for (scale, bar) in [(1.5, 1.5), (1.5, 1.0), (0.5, 1.5), (0.75, 0.5), (1.25, 1.25)] {
                for right in [false, true] {
                    let fit = fit_side_bar(window, right, scale, pc::BAR_HEIGHT * bar, 0.5);
                    let (shown, laid_out) = (fit.shown(), fit.laid_out());
                    let case = format!("{height} {scale} {bar} {right}");
                    // drawn at its scale from its top left
                    let drawn = fit.to_window().mul_rect(laid_out);
                    assert!(
                        (drawn.min - shown.min).length() < 1e-3 && (drawn.max - shown.max).length() < 1e-3,
                        "{case}"
                    );
                    assert!((shown.width() - pc::SIDE_BAR_WIDTH * scale).abs() < 1e-3, "{case}");
                    let edge = if right { window.right() - shown.right() } else { shown.left() - window.left() };
                    assert!(edge.abs() < 1e-3, "{case}");
                    assert!(shown.top() >= pc::BAR_HEIGHT * bar - 1e-3, "{case}");
                    assert!(shown.bottom() <= window.bottom() + 1e-3, "{case}");
                }
            }
        }
    }
}
