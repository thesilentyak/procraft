//! The drawing guide: set from the guide editor, drawn over the canvas as lines, and followed by
//! strokes with assisted drawing on (kept to the guide's directions, or mirrored by symmetry).

use procraft_engine::geom::{Affine, Vec2};
use procraft_engine::guide::{Guide, GuideKind, LengthUnit, SymmetryKind, bar_color};
use procraft_engine::input::Sample;
use procraft_engine::stroke::Stamp;
use procraft_engine::view::View;
use procraft_gpu::{LiquifyMode, LiquifyStamp};

use crate::{CanvasSession, State, ViewCurve, ViewMotion};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GuideType {
    Grid,
    Isometric,
    Perspective,
    Symmetry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SymmetryType {
    Vertical,
    Horizontal,
    Quadrant,
    Radial,
}

/// The units a grid's size is typed and shown in, as its keypad lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GuideUnit {
    Millimeters,
    Centimeters,
    Inches,
    Pixels,
}

/// The sizes a grid can have, canvas pixels.
const SIZE_MIN: f32 = 4.0;
const SIZE_MAX: f32 = 1000.0;

/// The guide as the editor and the overlay show it. Points are window pixels, x and y in turn;
/// `lines` are segments (two points each); `color` is sRGB 0..1.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct GuideInfo {
    pub on: bool,
    pub kind: GuideType,
    pub opacity: f32,
    pub thickness: f32,
    /// The grid's size, canvas pixels; the unit it is shown in; and the size in that unit as its
    /// keypad starts with it ("71", "8", "0.5") and as its pill shows it ("71px", "8mm", "1″").
    pub size: f32,
    pub unit: GuideUnit,
    pub size_text: String,
    pub size_label: String,
    /// Where the colour sits on the colour bar (0..1), and the colour.
    pub hue: f32,
    pub color: Vec<f32>,
    pub symmetry: SymmetryType,
    pub rotational: bool,
    pub assisted: bool,
    pub centre: Vec<f32>,
    pub handle: Vec<f32>,
    pub vanishing: Vec<f32>,
    pub lines: Vec<f32>,
}

fn kind_of(t: GuideType) -> GuideKind {
    match t {
        GuideType::Grid => GuideKind::Grid,
        GuideType::Isometric => GuideKind::Isometric,
        GuideType::Perspective => GuideKind::Perspective,
        GuideType::Symmetry => GuideKind::Symmetry,
    }
}

fn type_of(k: GuideKind) -> GuideType {
    match k {
        GuideKind::Grid => GuideType::Grid,
        GuideKind::Isometric => GuideType::Isometric,
        GuideKind::Perspective => GuideType::Perspective,
        GuideKind::Symmetry => GuideType::Symmetry,
    }
}

fn length_unit(u: GuideUnit) -> LengthUnit {
    match u {
        GuideUnit::Millimeters => LengthUnit::Millimetres,
        GuideUnit::Centimeters => LengthUnit::Centimetres,
        GuideUnit::Inches => LengthUnit::Inches,
        GuideUnit::Pixels => LengthUnit::Pixels,
    }
}

fn guide_unit(u: LengthUnit) -> GuideUnit {
    match u {
        LengthUnit::Millimetres => GuideUnit::Millimeters,
        LengthUnit::Centimetres => GuideUnit::Centimeters,
        LengthUnit::Inches => GuideUnit::Inches,
        LengthUnit::Pixels => GuideUnit::Pixels,
    }
}

/// `px` canvas pixels in `unit` at `dpi`, as the grid size's keypad and pill show it: pixels
/// whole, the others to two decimals at most without trailing zeros ("71", "8", "0.31").
fn length_text(px: f32, unit: LengthUnit, dpi: f32) -> String {
    match unit {
        LengthUnit::Pixels => format!("{}", px.round() as i64),
        _ => crate::presets::preset_number(unit.from_pixels(px, dpi)),
    }
}

/// [`length_text`] and the unit's mark, without a space: "71px", "8mm", "10cm", "1″".
fn length_label(px: f32, unit: LengthUnit, dpi: f32) -> String {
    let mark = match unit {
        LengthUnit::Pixels => "px",
        LengthUnit::Millimetres => "mm",
        LengthUnit::Centimetres => "cm",
        LengthUnit::Inches => "\u{2033}",
    };
    format!("{}{mark}", length_text(px, unit, dpi))
}

/// `stamps` and the copies the guide's symmetry draws of them: each the stamp itself, the same
/// randomness and all, moved as the canvas is by the copy's mirror or turn, its shape and its
/// moving grain with it (the grain each copy's own reflection or turn of the stamp's, not read
/// afresh where the copy lands), and told which copy it is on (a smudge's copies each carry the
/// paint along their own paths). Without symmetry, the stamps themselves, not a copy (a long
/// stroke's end copied all of it, megabytes, as the pen lifted).
pub(crate) fn mirrored<'s>(guide: &Guide, stamps: &'s [Stamp]) -> std::borrow::Cow<'s, [Stamp]> {
    if guide.copies(procraft_engine::geom::Vec2::ZERO).is_empty() {
        return std::borrow::Cow::Borrowed(stamps);
    }
    let mut out = stamps.to_vec();
    for s in stamps {
        for (k, (at, turn, mirror)) in guide.copies(s.center).into_iter().enumerate() {
            let mut copy = *s;
            copy.center = at;
            copy.copy = u8::try_from(k + 1).unwrap_or(u8::MAX);
            if mirror {
                // mirrored across a line at half `turn`: the shape drawn at the turn less its own
                // and turned over top to bottom in its own frame; the grain's way from the middle
                // turned over on the canvas, then the patch turned by the line's turn on its own
                copy.angle = turn - s.angle;
                copy.flip = [s.flip[0], !s.flip[1]];
                copy.grain_turn = s.grain_turn + turn;
                copy.grain_mirrored = !s.grain_mirrored;
            } else {
                // turned: the shape with it, and the grain read turned back by as much
                copy.angle = s.angle + turn;
                copy.grain_turn = s.grain_turn - turn;
            }
            out.push(copy);
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Liquify `stamps` and the copies the guide's symmetry makes of them, as a stroke's are (with
/// Drawing Assist on the layer): each moved as the canvas is by the copy's mirror or turn, the way
/// it pushes with it, and a twirl mirrored turning the other way.
pub(crate) fn mirrored_liquify(guide: &Guide, stamps: &[LiquifyStamp]) -> Vec<LiquifyStamp> {
    let mut out = stamps.to_vec();
    for s in stamps {
        let [x, y] = s.way;
        for (at, turn, mirror) in guide.copies(Vec2::new(s.centre[0], s.centre[1])) {
            let (sin, cos) = turn.sin_cos();
            let (way, mode) = if mirror {
                let mode = match s.mode {
                    LiquifyMode::TwirlRight => LiquifyMode::TwirlLeft,
                    LiquifyMode::TwirlLeft => LiquifyMode::TwirlRight,
                    other => other,
                };
                ([x * cos + y * sin, x * sin - y * cos], mode)
            } else {
                ([x * cos - y * sin, x * sin + y * cos], s.mode)
            };
            out.push(LiquifyStamp { centre: [at.x, at.y], way, mode, ..*s });
        }
    }
    out
}

/// How far (window pixels) an assisted stroke goes before the direction it is kept to is settled.
const ASSIST_SETTLE: f32 = 10.0;

/// Vanishing points closer than this to the input (window pixels) are taken, not added.
const VANISHING_REACH: f32 = 30.0;

/// The guide editor shows the canvas fitted in this much of the window, centred, moving there
/// (and back as it closes) over this long, slowing to a stop.
const EDITOR_FIT: f32 = 0.6;
const EDITOR_EASE_SECONDS: f32 = 0.3;
/// A touch in the guide editor takes the handle it lands within this of (points).
const HANDLE_REACH: f32 = 30.0;

/// What a touch on the guide editor's canvas took: a perspective guide's point (by index), the
/// guide's centre, or its turn handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuideGrab {
    Point(u32),
    Centre,
    Turn,
}

impl State {
    /// Stroke input kept to the guide when assisted drawing is on: along the guide direction
    /// the stroke heads in once it has gone [`ASSIST_SETTLE`] window pixels, kept to its end,
    /// so a wavering hand doesn't switch it (before then, held where it began).
    pub(crate) fn assisted(&mut self, mut samples: Vec<Sample>) -> Vec<Sample> {
        if let Some(start) = self.assist_from {
            let forced = self.force_assist.then(|| {
                let mut g = self.doc.guide.clone();
                g.assisted = true;
                g
            });
            let layer_guide = self.doc.stroke_guide();
            let guide = forced.as_ref().unwrap_or(&layer_guide);
            let settle = ASSIST_SETTLE / self.view.map_or(1.0, |v| v.zoom()).max(1e-6);
            for s in &mut samples {
                let heading = match self.assist_angle {
                    Some(a) => a,
                    None => {
                        let Some(a) = guide.assist_direction(start, s.pos) else { continue };
                        if s.pos.distance(start) < settle {
                            // too near the start to tell the direction yet: held there
                            s.pos = start;
                            continue;
                        }
                        self.assist_angle = Some(a);
                        a
                    }
                };
                s.pos = procraft_engine::guide::assist_along(start, s.pos, heading);
            }
        }
        samples
    }

    /// The canvas's dots per inch, the usual where it has none that can be used.
    fn dpi(&self) -> f32 {
        let dpi = self.doc.dpi;
        if dpi.is_finite() && dpi > 0.0 { dpi } else { procraft_engine::formats::DPI as f32 }
    }

    fn guide_info(&self) -> GuideInfo {
        // radial symmetry's lines as a plugin has them, if one says
        let mut seen = self.doc.guide.clone();
        seen.radial = self.hooks.as_ref().and_then(|hooks| hooks.radial_segments());
        let g = &seen;
        let look = g.look();
        let dpi = self.dpi();
        let to_screen = self.view.map_or(Affine::scale(1.0), |v| v.to_screen());
        let point = |p: Vec2| {
            let w = to_screen.apply(p);
            [w.x, w.y]
        };
        let c = Vec2::new(g.centre[0], g.centre[1]);
        // the turn handle sits up from the centre, at the canvas's top edge when level
        let up = Vec2::new(g.angle.sin(), -g.angle.cos()) * c.y.max(1.0);
        GuideInfo {
            on: g.on,
            kind: type_of(g.kind),
            opacity: look.opacity,
            thickness: look.thickness,
            size: look.size,
            unit: guide_unit(look.unit),
            size_text: length_text(look.size, look.unit, dpi),
            size_label: length_label(look.size, look.unit, dpi),
            hue: look.color,
            color: bar_color(look.color).to_vec(),
            symmetry: match g.symmetry {
                SymmetryKind::Vertical => SymmetryType::Vertical,
                SymmetryKind::Horizontal => SymmetryType::Horizontal,
                SymmetryKind::Quadrant => SymmetryType::Quadrant,
                SymmetryKind::Radial => SymmetryType::Radial,
            },
            rotational: g.rotational,
            assisted: self.doc.is_assisted(self.doc.current()),
            centre: point(c).to_vec(),
            handle: point(c + up).to_vec(),
            vanishing: g.vanishing.iter().flat_map(|v| point(Vec2::new(v[0], v[1]))).collect(),
            lines: g
                .lines(self.doc.width(), self.doc.height())
                .into_iter()
                .flat_map(|(a, b)| point(a).into_iter().chain(point(b)))
                .collect(),
        }
    }

    fn to_canvas_point(&self, x: f32, y: f32) -> Option<Vec2> {
        self.view?.screen_to_canvas(Vec2::new(x, y)).filter(|p| p.is_finite())
    }
}

#[uniffi::export]
impl CanvasSession {
    pub fn guide(&self) -> GuideInfo {
        self.shared.lock().guide_info()
    }

    pub fn set_guide_on(&self, on: bool) -> GuideInfo {
        self.change_guide(|g| g.on = on)
    }

    pub fn set_guide_kind(&self, kind: GuideType) -> GuideInfo {
        self.change_guide(|g| g.kind = kind_of(kind))
    }

    /// The shown kind's opacity and thickness (0..1), grid size (canvas pixels) and colour (0..1
    /// along the colour bar).
    pub fn set_guide_look(&self, opacity: f32, thickness: f32, size: f32, hue: f32) -> GuideInfo {
        self.change_guide(|g| {
            let k = g.kind as usize;
            if let Some(look) = g.looks.get_mut(k) {
                let unit = |v: f32, old: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { old };
                look.opacity = unit(opacity, look.opacity);
                look.thickness = unit(thickness, look.thickness);
                look.color = unit(hue, look.color);
                if size.is_finite() {
                    look.size = size.clamp(SIZE_MIN, SIZE_MAX);
                }
            }
        })
    }

    /// The shown grid's size typed on its keypad: `typed` (what is typed so far) in `unit`, made
    /// canvas pixels by the canvas's DPI and kept to the sizes a grid can have. The unit is kept
    /// for the size's pill even while nothing (or no number above nothing) is typed.
    pub fn set_guide_length(&self, typed: String, unit: GuideUnit) -> GuideInfo {
        let dpi = self.shared.lock().dpi();
        let unit = length_unit(unit);
        let px = typed.trim().parse::<f32>().ok().filter(|v| v.is_finite() && *v > 0.0).map(|v| unit.to_pixels(v, dpi));
        self.change_guide(|g| {
            let k = g.kind as usize;
            if let Some(look) = g.looks.get_mut(k) {
                look.unit = unit;
                if let Some(px) = px.filter(|px| px.is_finite()) {
                    look.size = px.clamp(SIZE_MIN, SIZE_MAX);
                }
            }
        })
    }

    pub fn set_guide_symmetry(&self, kind: SymmetryType, rotational: bool) -> GuideInfo {
        self.change_guide(|g| {
            g.symmetry = match kind {
                SymmetryType::Vertical => SymmetryKind::Vertical,
                SymmetryType::Horizontal => SymmetryKind::Horizontal,
                SymmetryType::Quadrant => SymmetryKind::Quadrant,
                SymmetryType::Radial => SymmetryKind::Radial,
            };
            g.rotational = rotational;
        })
    }

    /// Strokes from now on follow the drawing guide (if it is on) even where drawing assist is
    /// off, until this is turned off again: □ + Touch or □ + Pen set to assisted drawing.
    pub fn set_stroke_assist(&self, on: bool) {
        self.shared.lock().force_assist = on;
    }

    /// Drawing assist on the current layer (a mask's layer, for a mask): strokes on it follow the
    /// guide. One step, named as measured ("Undo Drawing Assist on").
    pub fn set_guide_assisted(&self, assisted: bool) -> GuideInfo {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let current = st.doc.current();
        let owner = match st.doc.layer(current) {
            Some(l) if l.kind == procraft_engine::document::LayerKind::Mask => l.parent.unwrap_or(current),
            _ => current,
        };
        if st.doc.layer(owner).is_some_and(|l| l.assisted != assisted) {
            st.finish_stroke();
            st.settle();
            let before = st.doc.clone();
            st.doc.set_assisted(owner, assisted);
            if let Some(canvas) = st.canvas.as_mut() {
                canvas.name_next_step(if assisted { "Drawing Assist on" } else { "Drawing Assist off" });
                canvas.record_document(before);
            }
        }
        st.revision += 1;
        self.shared.frame(st);
        st.guide_info()
    }

    /// The guide's centre dragged to window point (`x`, `y`).
    pub fn move_guide_centre(&self, x: f32, y: f32) -> GuideInfo {
        let to = self.shared.lock().to_canvas_point(x, y);
        self.change_guide(|g| {
            if let Some(p) = to {
                g.centre = [p.x, p.y];
            }
        })
    }

    /// The guide's turn handle dragged to window point (`x`, `y`): it turns about its centre.
    pub fn turn_guide(&self, x: f32, y: f32) -> GuideInfo {
        let to = self.shared.lock().to_canvas_point(x, y);
        self.change_guide(|g| {
            if let Some(p) = to {
                let d = p - Vec2::new(g.centre[0], g.centre[1]);
                if d.length() > 1.0 {
                    g.angle = d.x.atan2(-d.y);
                }
            }
        })
    }

    /// Perspective: a tap at window point (`x`, `y`) adds a vanishing point (up to three); returns
    /// the index of the point there, to drag.
    pub fn touch_vanishing_point(&self, x: f32, y: f32) -> Option<u32> {
        self.vanishing_point_at(x, y, VANISHING_REACH, true)
    }
    pub fn move_vanishing_point(&self, index: u32, x: f32, y: f32) -> GuideInfo {
        let to = self.shared.lock().to_canvas_point(x, y);
        self.change_guide(|g| {
            if let (Some(p), Some(v)) = (to, g.vanishing.get_mut(index as usize)) {
                *v = [p.x, p.y];
            }
        })
    }

    pub fn remove_vanishing_point(&self, index: u32) -> GuideInfo {
        self.change_guide(|g| {
            if (index as usize) < g.vanishing.len() {
                g.vanishing.remove(index as usize);
            }
        })
    }

    /// The guide's centre back in the middle of the canvas and upright: what a tap on the centre
    /// of a grid, isometric or symmetry guide offers.
    pub fn reset_guide_place(&self) -> GuideInfo {
        let middle = {
            let st = self.shared.lock();
            [st.doc.width() as f32 / 2.0, st.doc.height() as f32 / 2.0]
        };
        self.change_guide(|g| {
            g.centre = middle;
            g.angle = 0.0;
        })
    }

    /// Opens the guide editor: the guide as it was is kept to go back to, and the canvas moves to
    /// fit upright in the middle 0.6 of the window, slowing to a stop over 0.3 s (the guide read
    /// meanwhile is where it has got to). Two fingers move and zoom it from there as on the canvas,
    /// a quick pinch in fitting it to the whole window (see [`CanvasSession::set_touch_guide_editing`]).
    pub fn guide_edit_begin(&self) -> GuideInfo {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        // (settling somewhere, the view goes back there)
        let before = st.view_motion.take().map(|m| m.to).or(st.view);
        st.guide_before = Some((st.doc.guide.clone(), before));
        if st.doc.guide.centre == [0.0, 0.0] {
            st.doc.guide.centre = [st.doc.width() as f32 / 2.0, st.doc.height() as f32 / 2.0];
        }
        let (w, h) = (st.window_size.0 as f32, st.window_size.1 as f32);
        let (side, band) = (w * (1.0 - EDITOR_FIT) / 2.0, h * (1.0 - EDITOR_FIT) / 2.0);
        let fitted = View::fit_inside((st.doc.width(), st.doc.height()), st.window_size, [side, band, side, band]);
        st.ease_view_to(fitted);
        self.shared.frame(st);
        st.guide_info()
    }

    /// Closes the guide editor: `keep` keeps the changes, otherwise the guide goes back. The view
    /// goes back as it was either way, as it came.
    pub fn guide_edit_end(&self, keep: bool) -> GuideInfo {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        if let Some((guide, view)) = st.guide_before.take() {
            if !keep {
                st.doc.guide = guide;
            }
            match view {
                Some(view) => st.ease_view_to(view),
                None => st.view = None,
            }
        }
        st.revision += 1;
        self.shared.frame(st);
        st.guide_info()
    }
}

impl State {
    /// The view moves to `to` over the guide editor's time, slowing to a stop.
    fn ease_view_to(&mut self, to: View) {
        let start = std::time::Instant::now();
        self.view_motion =
            self.view.map(|from| ViewMotion { from, to, start, curve: ViewCurve::EaseOut(EDITOR_EASE_SECONDS) });
        if self.view_motion.is_none() {
            self.view = Some(to);
        }
    }
}

impl CanvasSession {
    /// Perspective: a touch at window point (`x`, `y`) takes the vanishing point within `reach`
    /// window pixels, else (with `add`) adds one there (up to three); its index, to drag.
    fn vanishing_point_at(&self, x: f32, y: f32, reach: f32, add: bool) -> Option<u32> {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let p = st.to_canvas_point(x, y)?;
        let to_screen = st.view?.to_screen();
        let near = st
            .doc
            .guide
            .vanishing
            .iter()
            .position(|v| to_screen.apply(Vec2::new(v[0], v[1])).distance(Vec2::new(x, y)) <= reach);
        if let Some(i) = near {
            return Some(i as u32);
        }
        if !add || st.doc.guide.vanishing.len() >= 3 {
            return None;
        }
        st.doc.guide.vanishing.push([p.x, p.y]);
        st.revision += 1;
        self.shared.frame(st);
        Some(st.doc.guide.vanishing.len() as u32 - 1)
    }

    /// A tap on the guide editor's canvas at window point (`x`, `y`) away from its handles: a
    /// perspective guide with fewer than three points gets one there. Whether it did.
    pub(crate) fn guide_tap(&self, x: f32, y: f32) -> bool {
        if self.guide().kind != GuideType::Perspective {
            return false;
        }
        let before = self.shared.lock().doc.guide.vanishing.len();
        self.vanishing_point_at(x, y, 0.0, true).is_some() && self.shared.lock().doc.guide.vanishing.len() > before
    }

    /// Where handle `grab` is, window pixels: a tapped one's menu shows there.
    pub(crate) fn guide_handle_at(&self, grab: GuideGrab) -> Option<(f32, f32)> {
        let g = self.guide();
        let point = match grab {
            GuideGrab::Point(i) => g.vanishing.get(i as usize * 2..i as usize * 2 + 2),
            GuideGrab::Centre => g.centre.get(..2),
            GuideGrab::Turn => g.handle.get(..2),
        };
        match point {
            Some([x, y]) => Some((*x, *y)),
            _ => None,
        }
    }

    /// A touch on the guide editor's canvas at window point (`x`, `y`), `scale` window pixels a
    /// point: what it takes. A perspective guide's point there, else the turn handle or the centre
    /// it lands near (a point is added by a tap, [`CanvasSession::guide_tap`], not by a touch).
    pub(crate) fn guide_grab(&self, x: f32, y: f32, scale: f32) -> Option<GuideGrab> {
        let reach = HANDLE_REACH * scale;
        let g = self.guide();
        if g.kind == GuideType::Perspective {
            return self.vanishing_point_at(x, y, reach, false).map(GuideGrab::Point);
        }
        let near = |p: &[f32]| matches!(p, [px, py, ..] if (px - x).hypot(py - y) <= reach);
        if near(&g.handle) {
            Some(GuideGrab::Turn)
        } else if near(&g.centre) {
            Some(GuideGrab::Centre)
        } else {
            None
        }
    }

    /// What a touch took in the guide editor, dragged to window point (`x`, `y`).
    pub(crate) fn guide_drag(&self, grab: GuideGrab, x: f32, y: f32) {
        match grab {
            GuideGrab::Point(i) => _ = self.move_vanishing_point(i, x, y),
            GuideGrab::Centre => _ = self.move_guide_centre(x, y),
            GuideGrab::Turn => _ = self.turn_guide(x, y),
        }
    }
}

impl CanvasSession {
    fn change_guide(&self, f: impl FnOnce(&mut Guide)) -> GuideInfo {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        f(&mut st.doc.guide);
        // once on, it stays set up
        st.doc.guide.set_up |= st.doc.guide.on;
        st.revision += 1;
        self.shared.frame(st);
        st.guide_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_liquify_stamp_is_mirrored_or_turned_with_its_way_and_twirl() {
        let mut guide = Guide { on: true, kind: GuideKind::Symmetry, assisted: true, ..Guide::new(100, 100) };
        let stamp = |mode| LiquifyStamp { centre: [30.0, 40.0], way: [3.0, 1.0], radius: 9.0, amount: 0.5, mode };
        let near = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4;
        // mirrored across the upright line: its way too, and a twirl turns the other way
        let out = mirrored_liquify(&guide, &[stamp(LiquifyMode::Push), stamp(LiquifyMode::TwirlRight)]);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], stamp(LiquifyMode::Push));
        let copy = out[2];
        assert!(near(copy.centre, [70.0, 40.0]) && near(copy.way, [-3.0, 1.0]), "{copy:?}");
        assert_eq!((copy.radius, copy.amount, copy.mode), (9.0, 0.5, LiquifyMode::Push));
        assert_eq!(out[3].mode, LiquifyMode::TwirlLeft);
        // turned a half round: the way with it, the twirl as it was
        guide.rotational = true;
        let out = mirrored_liquify(&guide, &[stamp(LiquifyMode::TwirlLeft)]);
        assert_eq!(out.len(), 2);
        assert!(near(out[1].centre, [70.0, 60.0]) && near(out[1].way, [-3.0, -1.0]), "{:?}", out[1]);
        assert_eq!(out[1].mode, LiquifyMode::TwirlLeft);
        // without Drawing Assist, or another guide: only the stamp
        for g in [Guide { assisted: false, ..guide.clone() }, Guide { kind: GuideKind::Grid, ..guide.clone() }] {
            assert_eq!(mirrored_liquify(&g, &[stamp(LiquifyMode::Push)]), [stamp(LiquifyMode::Push)]);
        }
    }

    #[test]
    fn the_grid_size_shows_in_its_unit_whole_or_to_two_decimals() {
        // whole pixels; the lengths as measured, a whole number without decimals
        assert_eq!(length_label(71.0, LengthUnit::Pixels, 132.0), "71px");
        assert_eq!(length_label(71.4, LengthUnit::Pixels, 132.0), "71px");
        assert_eq!(length_label(LengthUnit::Millimetres.to_pixels(8.0, 132.0), LengthUnit::Millimetres, 132.0), "8mm");
        assert_eq!(
            length_label(LengthUnit::Centimetres.to_pixels(10.0, 132.0), LengthUnit::Centimetres, 132.0),
            "10cm"
        );
        assert_eq!(length_label(300.0, LengthUnit::Inches, 300.0), "1\u{2033}");
        // two decimals at most, no trailing zeros
        assert_eq!(length_text(150.0, LengthUnit::Inches, 300.0), "0.5");
        assert_eq!(length_text(71.0, LengthUnit::Millimetres, 132.0), "13.66");
        assert_eq!(length_text(71.0, LengthUnit::Inches, 132.0), "0.54");
    }

    #[test]
    fn a_typed_length_becomes_the_grid_size_by_the_dpi_and_cancel_undoes_it() {
        let s = CanvasSession::new(400, 300);
        s.shared.lock().doc.dpi = 132.0;
        s.set_guide_on(true);
        let before = s.guide_edit_begin();
        assert_eq!(
            (before.unit, before.size_text.as_str(), before.size_label.as_str()),
            (GuideUnit::Pixels, "71", "71px")
        );
        // a unit chosen with nothing typed keeps the size, shown in the unit
        let g = s.set_guide_length(String::new(), GuideUnit::Millimeters);
        assert_eq!((g.size, g.size_label.as_str()), (71.0, "13.66mm"));
        // typed: the size in canvas pixels, the pill in the unit
        let g = s.set_guide_length("8".into(), GuideUnit::Millimeters);
        assert!((g.size - 41.575).abs() < 1e-3 && g.size_label == "8mm" && g.size_text == "8", "{g:?}");
        let g = s.set_guide_length("10".into(), GuideUnit::Centimeters);
        assert!((g.size - 519.685).abs() < 1e-2 && g.size_label == "10cm", "{g:?}");
        assert_eq!(s.set_guide_length("1".into(), GuideUnit::Inches).size_label, "1\u{2033}");
        // what isn't a length above nothing leaves the size as it was
        for typed in [".", "0", "0.", "1.2.3"] {
            assert_eq!(s.set_guide_length(typed.into(), GuideUnit::Inches).size, 132.0, "{typed}");
        }
        assert_eq!(s.set_guide_length("1.".into(), GuideUnit::Inches).size, 132.0);
        assert_eq!(s.set_guide_length("0.5".into(), GuideUnit::Inches).size_label, "0.5\u{2033}");
        // kept to the sizes a grid can have, shown as kept
        let g = s.set_guide_length("100".into(), GuideUnit::Centimeters);
        assert_eq!((g.size, g.size_label.as_str()), (SIZE_MAX, "19.24cm"));
        let g = s.set_guide_length("1".into(), GuideUnit::Pixels);
        assert_eq!((g.size, g.size_label.as_str()), (SIZE_MIN, "4px"));
        // the slider keeps the unit
        assert_eq!(s.set_guide_look(g.opacity, g.thickness, 264.0, g.hue).size_label, "264px");
        s.set_guide_length("2".into(), GuideUnit::Inches);
        assert_eq!(s.set_guide_look(g.opacity, g.thickness, 66.0, g.hue).size_label, "0.5\u{2033}");
        // each kind keeps its own; Cancel puts size and unit back
        let iso = s.set_guide_kind(GuideType::Isometric);
        assert_eq!((iso.size, iso.unit), (82.0, GuideUnit::Pixels));
        s.set_guide_kind(GuideType::Grid);
        let back = s.guide_edit_end(false);
        assert_eq!((back.size, back.unit, back.size_label.as_str()), (71.0, GuideUnit::Pixels, "71px"));
        // Done keeps them
        s.guide_edit_begin();
        s.set_guide_length("3".into(), GuideUnit::Centimeters);
        assert_eq!(s.guide_edit_end(true).size_label, "3cm");
    }

    /// A symmetry's copies of a stroke are its own stamps moved as the canvas is, pixel for pixel:
    /// mirrored copies are the stroke mirrored and turned ones the stroke turned, shapes that read
    /// differently each way round, turned, squashed and flipped, and their moving grain with them
    /// (each copy's grain the stroke's own, mirrored or turned, not the grain where it lands).
    #[test]
    fn a_symmetry_s_copies_are_the_stroke_mirrored_or_turned_pixel_for_pixel() {
        use procraft_engine::brush::{Brush, GrainMode, Shape, Source};
        use procraft_engine::document::Document;
        use procraft_engine::textures::TEXTURE_SIZE;
        use procraft_gpu::{Canvas, Gpu, StrokeStyle, StrokeTool};
        let Ok(gpu) = Gpu::headless() else { return eprintln!("skipped: no GPU") };
        let side = 128u32;
        let n = TEXTURE_SIZE;
        // a shape with its ink in its top left and a fainter blot low right; a grain of uneven
        // bands that reads differently mirrored or turned
        let shape: Vec<u8> = (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as f32 / n as f32, (i / n) as f32 / n as f32);
                if x < 0.55 && y < 0.45 {
                    255
                } else if (x - 0.75).powi(2) + (y - 0.7).powi(2) < 0.015 {
                    150
                } else {
                    0
                }
            })
            .collect();
        let grain: Vec<u8> =
            (0..n * n).map(|i| ((i % n / 7 * 37 + i / n / 3 * 11 + i % n * (i / n) / 97) % 256) as u8).collect();
        let brush =
            Brush { shape: Shape { source: Source::new(shape), ..Brush::round_pen().shape }, ..Brush::round_pen() };
        let brush = Brush {
            grain: procraft_engine::brush::Grain {
                source: Source::new(grain),
                mode: GrainMode::Moving,
                ..brush.grain.clone()
            },
            ..brush
        };
        // stamps turned, squashed, flipped and reading the grain from places of their own
        let stamps: Vec<Stamp> = (0..7)
            .map(|i| {
                let t = i as f32;
                Stamp {
                    angle: 0.4 + 0.9 * t,
                    roundness: if i % 3 == 0 { 0.6 } else { 1.0 },
                    flip: [i % 2 == 1, i % 4 == 2],
                    grain_offset: Vec2::new(11.0 * t, 5.0 + 3.0 * t),
                    grain_turn: 0.3 * t,
                    grain_size: 0.8 + 0.1 * t,
                    ..Stamp::round(Vec2::new(18.0 + 5.0 * t, 20.0 + 11.0 * t), 9.0 + t, 0.7)
                }
            })
            .collect();
        let drawn = |guide: &Guide| {
            let mut doc = Document::new(side, side);
            let mut canvas = Canvas::new(&gpu, &doc).unwrap();
            let mut style = StrokeStyle {
                grain_mode: GrainMode::Moving,
                grain_depth: 1.0,
                grain_scale: 40.0,
                grain_blend: 1,
                ..StrokeStyle::new(StrokeTool::Paint, [0.1, 0.2, 0.3], 1.0)
            };
            canvas.use_sources(&gpu, &mut style, &brush);
            canvas.begin_stroke(doc.current(), style);
            canvas.add_stamps(&mirrored(guide, &stamps));
            canvas.end_stroke();
            canvas.render(&gpu, &mut doc, None);
            assert_eq!(gpu.errors(), 0, "GPU validation errors");
            canvas.read_layer(&gpu, doc.current()).unwrap()
        };
        let at = |px: &[u8], x: u32, y: u32| px.get(((y * side + x) * 4 + 3) as usize).copied().unwrap_or(0);
        let last = side - 1;
        let symmetry = |symmetry: SymmetryKind, rotational: bool| Guide {
            on: true,
            kind: GuideKind::Symmetry,
            assisted: true,
            symmetry,
            rotational,
            ..Guide::new(side, side)
        };
        // where a pixel lands mirrored across the upright line, the level one and the diagonals,
        // and turned a quarter and a half round (the guide's centre is the canvas's)
        type Map = fn(u32, u32, u32) -> (u32, u32);
        let across: Map = |l, x, y| (l - x, y);
        let down: Map = |l, x, y| (x, l - y);
        let diagonal: Map = |_, x, y| (y, x);
        let quarter: Map = |l, x, y| (l - y, x);
        let half: Map = |l, x, y| (l - x, l - y);
        // each guide, and the moves of the canvas its copies make of the stroke (whose ink is
        // all top left)
        type Way<'a> = (&'a str, Guide, &'a [(&'a str, Map)]);
        let ways: [Way<'_>; 6] = [
            ("vertical", symmetry(SymmetryKind::Vertical, false), &[("across", across)]),
            ("horizontal", symmetry(SymmetryKind::Horizontal, false), &[("down", down)]),
            (
                "quadrant",
                symmetry(SymmetryKind::Quadrant, false),
                &[("across", across), ("down", down), ("half", half)],
            ),
            ("vertical turned", symmetry(SymmetryKind::Vertical, true), &[("half", half)]),
            (
                "radial",
                symmetry(SymmetryKind::Radial, false),
                &[("across", across), ("down", down), ("diagonal", diagonal), ("quarter", quarter)],
            ),
            ("radial turned", symmetry(SymmetryKind::Radial, true), &[("quarter", quarter), ("half", half)]),
        ];
        for (name, guide, moves) in ways {
            let px = drawn(&guide);
            for (how, copy) in moves {
                let (mut off, mut inked, mut most) = (0, 0, 0u8);
                for y in 0..side {
                    for x in 0..side {
                        let (cx, cy) = copy(last, x, y);
                        let (a, b) = (at(&px, x, y), at(&px, cx, cy));
                        inked += usize::from(a > 0);
                        let d = a.abs_diff(b);
                        off += usize::from(d > 1);
                        most = most.max(d);
                    }
                }
                eprintln!("{name}, {how}: {inked} inked pixels, {off} off by more than 1, at most {most}");
                assert!(inked > 1000, "{name}: the stroke and its copies painted ({inked})");
                assert!(most <= 2 && off <= inked / 500, "{name}, {how}: {off} pixels off, at most {most}");
            }
        }
    }
}
