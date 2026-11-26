//! How the brush cursor's outline turns, and how big it is while painting: as the stamps it
//! stands for.
//!
//! While a stroke is painted it is the newest stamp's footprint, as big as that stamp is laid (its
//! pressure and start taper) and turned as it is (the stroke's way for a brush that follows it, the
//! pen's azimuth and barrel for one turned by the pen, a random turn or scatter included). Hovering, it turns as a stamp laid there would: the way the
//! pointer has been going, smoothed, stands for the stroke's way, so a brush that follows the stroke
//! shows which way it would lie as the pen moves over the canvas; and as the pen is held.
//!
//! Both are read without waiting on the canvas (a frame laying a stroke's paint holds it): the
//! newest stamp's are noted as the stamps are laid, the hover's turn as the pointer moves.

use std::sync::atomic::{AtomicU32, Ordering};

use procraft_engine::stroke::{HeldPen, hover_angle};

use crate::{CanvasSession, Shared, State};

/// A hovering pointer's move this long (points) or shorter turns its way only as far as the move
/// agrees with it; each [`HEADING_REACH`] more pulls it further, up to [`HEADING_MOST`].
const HEADING_STILL: f32 = 3.0;
const HEADING_REACH: f32 = 8.0;
const HEADING_MOST: f32 = 0.3;
/// A move agreeing with the way pulls it this much more (a move against it, none): a pen held
/// near still, its moves this way and that, doesn't swing it about.
const HEADING_AGREEING: f32 = 0.1;
/// How fast that falls off as the move turns from the way (per unit of their difference).
const HEADING_DISAGREEING: f32 = 0.75;

/// A pen held over the canvas: how it is turned (radians), its lean's way as its samples' azimuth
/// (orientation) and its barrel (0 for a pen that doesn't say).
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct CursorPen {
    pub azimuth: f32,
    pub roll: f32,
}

/// The way a hovering pointer has been going.
#[derive(Debug, Default)]
struct Heading {
    /// Where it last was (points); none since it left.
    last: Option<(f32, f32)>,
    /// The way it goes, smoothed (zero before it has gone anywhere).
    way: (f32, f32),
}

impl Heading {
    /// It moved on to `at` (points).
    fn moved(&mut self, at: (f32, f32)) {
        if !(at.0.is_finite() && at.1.is_finite()) {
            return;
        }
        let Some(from) = self.last.replace(at) else { return };
        let (dx, dy) = (at.0 - from.0, at.1 - from.1);
        let far = dx.hypot(dy);
        // a still pointer keeps its way
        if far.is_nan() || far <= 1e-4 {
            return;
        }
        let (ux, uy) = (dx / far, dy / far);
        let (ox, oy) = (ux - self.way.0, uy - self.way.1);
        let agrees = (1.0 - HEADING_DISAGREEING * ox.hypot(oy)).max(0.0);
        let pull = ((far - HEADING_STILL) / HEADING_REACH).clamp(0.0, 1.0) * HEADING_MOST;
        let pull = (pull + HEADING_AGREEING * agrees).min(1.0);
        self.way = (self.way.0 + pull * ox, self.way.1 + pull * oy);
    }

    /// The way (radians on the screen, clockwise from the right), none before it has gone anywhere.
    fn angle(&self) -> Option<f32> {
        let (x, y) = self.way;
        (x.hypot(y) > 1e-4).then(|| y.atan2(x))
    }
}

/// The brush cursor's turns, kept beside the canvas's lock.
pub(crate) struct CursorTurns {
    /// The newest stamp's while a stroke is painted (an f32's bits; NaN before it has one), and
    /// its radius on the screen (window pixels).
    painted: AtomicU32,
    painted_radius: AtomicU32,
    /// As last worked out where the pointer hovers.
    hovered: AtomicU32,
    heading: parking_lot::Mutex<Heading>,
    /// Window pixels a point, as last seen.
    points: AtomicU32,
    /// The plugins strokes go through, for where the cursor shows (see
    /// [`CanvasSession::cursor_point`]): kept beside the canvas's lock, so it never waits on it.
    pub(crate) hooks: parking_lot::Mutex<Option<std::sync::Arc<crate::hooks::Hooks>>>,
}

impl Default for CursorTurns {
    fn default() -> CursorTurns {
        CursorTurns {
            painted: AtomicU32::new(f32::NAN.to_bits()),
            painted_radius: AtomicU32::new(f32::NAN.to_bits()),
            hovered: AtomicU32::new(0f32.to_bits()),
            heading: parking_lot::Mutex::new(Heading::default()),
            points: AtomicU32::new(1f32.to_bits()),
            hooks: parking_lot::Mutex::new(None),
        }
    }
}

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

fn store(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

impl State {
    /// The canvas's turn on the screen (radians, clockwise).
    fn view_turn(&self) -> f32 {
        self.view.map_or(0.0, |v| v.rotation())
    }

    /// How far the cursor's outline turns past the current brush's own rotation (which
    /// [`CanvasSession::brush_outline`] has turned it by already) for a stamp turned `angle` on the
    /// canvas: on the screen, with the canvas.
    fn cursor_turn_for(&self, angle: f32) -> f32 {
        let turn = angle - self.brush(self.tool).shape.rotation + self.view_turn();
        if turn.is_finite() { turn } else { 0.0 }
    }

    /// The cursor's turn and radius on the screen (window pixels) for the newest stamp of the
    /// stroke painted, none before it has one.
    fn painted_stamp(&self) -> Option<(f32, f32)> {
        let stamp = self.stroke.as_ref()?.newest_stamp()?;
        let zoom = self.view.map_or(1.0, |v| v.zoom());
        let radius = stamp.radius * zoom;
        Some((self.cursor_turn_for(stamp.angle), if radius.is_finite() { radius.max(0.0) } else { f32::NAN }))
    }

    /// The cursor's turn hovering, going `heading` (radians on the screen; none before it has
    /// moved), the pen held as `pen` says (none: a pointer that doesn't tell).
    fn hover_turn(&self, heading: Option<f32>, pen: Option<CursorPen>) -> f32 {
        let view = self.view_turn();
        let pen = pen.map(|p| HeldPen { azimuth: p.azimuth, roll: p.roll });
        // a way on the screen is that much less turned on the canvas
        let angle = hover_angle(self.brush(self.tool), heading.map(|h| h - view), pen, view);
        self.cursor_turn_for(angle)
    }
}

impl Shared {
    /// Notes the newest stamp's turn and size for the cursor, the stroke's stamps laid (as `st`
    /// now is).
    pub(crate) fn note_painted_turn(&self, st: &State) {
        let (turn, radius) = st.painted_stamp().unwrap_or((f32::NAN, f32::NAN));
        store(&self.cursor.painted, turn);
        store(&self.cursor.painted_radius, radius);
    }

    /// No stroke is painted: the cursor turns as it hovers, as big as the brush.
    pub(crate) fn forget_painted_turn(&self) {
        store(&self.cursor.painted, f32::NAN);
        store(&self.cursor.painted_radius, f32::NAN);
    }
}

#[uniffi::export]
impl CanvasSession {
    /// The pen, or a mouse, hovers over the canvas at (`x`, `y`), window pixels, held as `pen` says
    /// (none for a pointer that doesn't tell how it is turned). The way it goes is noted; returns how
    /// far the brush cursor's outline turns there (see [`CanvasSession::cursor_turn`]).
    pub fn cursor_hovers(&self, x: f32, y: f32, pen: Option<CursorPen>) -> f32 {
        let turns = &self.shared.cursor;
        // (the canvas held by a frame: the scale and the turn as last seen)
        let st = self.shared.try_lock();
        if let Some(st) = &st {
            store(&turns.points, st.display_scale.max(1e-3));
        }
        let points = load(&turns.points).max(1e-3);
        let heading = {
            let mut h = turns.heading.lock();
            h.moved((x / points, y / points));
            h.angle()
        };
        if let Some(st) = &st {
            store(&turns.hovered, st.hover_turn(heading, pen));
        }
        load(&turns.hovered)
    }

    /// The hovering pointer left the canvas: its next move begins afresh from where it comes back
    /// (the way it was going is kept).
    pub fn cursor_hover_gone(&self) {
        self.shared.cursor.heading.lock().last = None;
    }

    /// How far the brush cursor's outline turns on the screen past the brush's own rotation (radians,
    /// clockwise), as the stamps it stands for: while a stroke is painted, as its newest stamp is;
    /// else as one laid where the pointer last hovered would be. Read without waiting on the canvas.
    pub fn cursor_turn(&self) -> f32 {
        let painted = load(&self.shared.cursor.painted);
        if painted.is_finite() { painted } else { load(&self.shared.cursor.hovered) }
    }

    /// Where the brush cursor shows for a pointer at (`x`, `y`), window pixels: where a stroke's
    /// sample there lands, as the plugins on the `position` hook move it (there, with none on), so
    /// the cursor stands where the stroke goes, hovering and painting alike. Read without waiting on
    /// the canvas.
    pub fn cursor_point(&self, x: f32, y: f32) -> Vec<f32> {
        let hooks = self.shared.cursor.hooks.lock().clone();
        let (x, y) = hooks.map_or((x, y), |hooks| hooks.position(x, y));
        vec![x, y]
    }

    /// How big the brush cursor's outline is while a stroke is painted: its newest stamp's radius
    /// on the screen (window pixels), as it is laid (its pressure and start taper in); none before
    /// it has one, or not painting (the brush's own size then). Read without waiting on the canvas.
    pub fn cursor_radius(&self) -> Option<f32> {
        Some(load(&self.shared.cursor.painted_radius)).filter(|r| r.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hovering_pointer_s_way_follows_its_moves_and_holds_when_it_stops() {
        let mut h = Heading::default();
        assert_eq!(h.angle(), None);
        // across the screen, a few points at a time: it comes round to the right
        for i in 0..40 {
            h.moved((i as f32 * 6.0, 100.0));
        }
        let right = h.angle().unwrap_or(f32::NAN);
        assert!(right.abs() < 0.02, "{right}");
        // then down: it turns to follow, not at once
        h.moved((240.0, 106.0));
        let first = h.angle().unwrap_or(f32::NAN);
        assert!(first > 0.0 && first < 1.0, "{first}");
        for i in 2..40 {
            h.moved((240.0, 100.0 + i as f32 * 6.0));
        }
        let down = h.angle().unwrap_or(f32::NAN);
        assert!((down - std::f32::consts::FRAC_PI_2).abs() < 0.02, "{down}");
        // held still, and wobbling back and forth by a point: it stays as it was
        for _ in 0..20 {
            h.moved((240.0, 334.0));
        }
        for i in 0..20 {
            h.moved((240.0 + if i % 2 == 0 { 1.0 } else { -1.0 }, 334.0));
        }
        let held = h.angle().unwrap_or(f32::NAN);
        assert!((held - down).abs() < 0.2, "{down} -> {held}");
        // gone and back elsewhere: the jump there is no move
        h.last = None;
        h.moved((900.0, 0.0));
        assert_eq!(h.angle(), Some(held));
    }
}
