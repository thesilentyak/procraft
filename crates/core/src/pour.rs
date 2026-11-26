//! Dropping a colour on the canvas: the colour poured into the area joined to a point whose colour
//! is near the point's own, measured on the reference layer if there is one. While the finger stays
//! down the threshold can change and the pour is redone from the layer as it was; it ends as one
//! undo step. Recolor keeps a pour open while its point moves and its colour changes. A plugin on
//! the fill hook has its say as a pour is let go, on a thread of its own: the pour stays open, its
//! layer held, until it has.

use std::sync::{Arc, Condvar, Mutex};

use procraft_engine::document::LayerId;
use procraft_engine::fill::{self, Flood};
use procraft_engine::geom::{IRect, Vec2};
use procraft_engine::selection::Mask;

use crate::hooks::Hooks;
use crate::plugin_run::MOST_FILL;
use crate::reveal::{Reveal, Shake};
use crate::{CanvasSession, Shared, State};

/// What is said of a fill with more in it than can go through a plugin.
const TOO_BIG: &str = "This fill is too big for a plugin: it stays as it is.";

/// The side of the part of the canvas about its point a pour is measured on first: a small fill on
/// a large canvas reads, measures and pours only there. One that comes within [`POUR_MARGIN`] of
/// that part's edge (short of the canvas's own) is measured again on the whole canvas.
const POUR_AREA: u32 = 1024;

/// How near the edge of the part of the canvas read a fill's flood may come before the rest of the
/// canvas is read: what lies past that edge could join it, or shape its soft edge (followed
/// [`fill::EDGE_MOST`] pixels on).
const POUR_MARGIN: u32 = fill::EDGE_MOST as u32 + 2;

/// What a pour is measured on besides its own layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Measured {
    Own,
    Reference(LayerId),
    AllLayers,
}

/// A pour's flood came near the edge of the part of the canvas read for it (see [`POUR_AREA`]).
struct Outgrown;

/// A rectangle of a layer (x, y, width, height, canvas pixels) and its pixels, poured.
type Poured = ([u32; 4], Vec<u8>);

#[cfg(test)]
thread_local! {
    /// Pours on this thread read the whole canvas at once: a test checks the part read first
    /// changes nothing.
    pub(crate) static WHOLE_AT_ONCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether a pour reads the whole canvas at once (see [`WHOLE_AT_ONCE`]).
fn whole_at_once() -> bool {
    #[cfg(test)]
    return WHOLE_AT_ONCE.with(std::cell::Cell::get);
    #[cfg(not(test))]
    false
}

/// A pour a plugin is finishing: when it has, and whoever is waiting for that.
#[derive(Default)]
pub(crate) struct Finishing {
    done: Mutex<bool>,
    ended: Condvar,
}

impl Finishing {
    fn wait(&self) {
        let mut done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        while !*done {
            done = self.ended.wait(done).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn end(&self) {
        *self.done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.ended.notify_all();
    }
}

/// A pour under way.
pub(crate) struct Pour {
    layer: LayerId,
    /// The canvas's width and height, and the part of it the pour reads (x, y, width, height):
    /// `before`, `elsewhere`, the flood and the masks are that part's, until the pour outgrows it
    /// and reads the whole canvas (see [`State::widen_pour`]).
    canvas: (u32, u32),
    area: [u32; 4],
    /// The whole canvas couldn't be read: it pours within the part it has.
    kept_to_area: bool,
    /// What it is measured on.
    measured_on: Measured,
    /// The layer's pixels as the pour began.
    before: Vec<u8>,
    /// The area measured from the point; none while the point is off the canvas.
    flood: Option<Flood>,
    /// The reference layer's pixels when measured on it, so poured over this one.
    elsewhere: Option<Vec<u8>>,
    colour: [f32; 3],
    /// The selection it pours within, the whole canvas's.
    selection: Option<Mask>,
    alpha_lock: bool,
    /// The threshold last poured, in thousandths, and where the layer was last poured into (x, y,
    /// width, height): outside it the layer is as it was.
    shown: i32,
    shown_region: Option<[u32; 4]>,
    /// What it last poured, as the flood reached (within the selection).
    shown_mask: Option<Mask>,
    /// The flood as a plugin left it, once one has.
    closed: Option<Mask>,
    /// Being finished by a plugin: nothing more changes it.
    finishing: Option<Arc<Finishing>>,
}

impl Pour {
    /// The colour poured at `threshold`, as the rectangle of the layer it changes now joined with
    /// the one it changed last (put back as it was where it no longer pours): that rectangle (x, y,
    /// width, height) and its pixels; none if neither changes anything. With `same_flood` (only
    /// the threshold changed since it was shown) only the rectangle where what it pours changed.
    /// Remembered as shown.
    /// Outgrown, and nothing changed, where its flood comes near the edge of the part of the canvas
    /// read for it.
    fn pour_region(&mut self, threshold: f32, same_flood: bool) -> Result<Option<Poured>, Outgrown> {
        // the mask, and a rectangle it takes nothing outside of where that is known (a small fill
        // on a large canvas is worked on there alone)
        let mut mask = None;
        if let Some(flood) = self.flood.as_ref() {
            let (mut m, mut within) = match &self.closed {
                Some(closed) => (closed.clone(), None),
                None => {
                    let (m, within) = flood.mask_bounded(threshold);
                    if within.or_else(|| m.bounds()).is_some_and(|r| self.outgrows(r)) {
                        return Err(Outgrown);
                    }
                    (m, within)
                }
            };
            // poured into what is see-through, it goes on under the line it stopped at
            if flood.colour[3] == 0 {
                let pixels = self.elsewhere.as_deref().unwrap_or(&self.before);
                match within {
                    Some(bounds) => {
                        fill::under_edges_within(&mut m, pixels, bounds);
                        within = Some(grown(bounds, fill::EDGE_MOST as u32, m.width, m.height));
                    }
                    None => fill::under_edges(&mut m, pixels),
                }
            }
            // within the selection (the whole canvas's)
            if let Some(sel) = &self.selection {
                let [x, y, w, h] = within.unwrap_or([0, 0, m.width, m.height]).map(|v| v as usize);
                let (ax, ay) = (self.area[0] as usize, self.area[1] as usize);
                let (stride, across) = (m.width as usize, sel.width as usize);
                for row in y..y + h {
                    let (at, from) = (row * stride + x, (ay + row) * across + ax + x);
                    let (Some(taken), Some(s)) = (m.data.get_mut(at..at + w), sel.data.get(from..from + w)) else {
                        continue;
                    };
                    for (t, &s) in taken.iter_mut().zip(s) {
                        *t = ((u16::from(*t) * u16::from(s) + 127) / 255) as u8;
                    }
                }
            }
            mask = Some((m, within));
        }
        let (mask, within) = match mask {
            Some((mask, within)) => (Some(mask), within),
            None => (None, None),
        };
        let now = mask.as_ref().and_then(|m| match within {
            Some(within) => m.bounds_within(within),
            None => m.bounds(),
        });
        // the same colour poured from the same seed colour: the pixels change only where the
        // flood reaches differently (a threshold dragged over a large fill changes its edges);
        // looked for only where either takes anything, when both are known
        let changed = match (&mask, &self.shown_mask) {
            (Some(m), Some(old)) if same_flood && m.data.len() == old.data.len() => {
                // (what was shown takes nothing outside where it was shown)
                let both = within.map(|a| self.shown_region.map_or(a, |b| join(a, b)));
                Some(differ(m, old, both))
            }
            _ => None,
        };
        let region = match (changed, now, self.shown_region) {
            (Some(Some(r)), ..) => r,
            (Some(None), ..) => {
                self.shown_region = now;
                self.shown_mask = mask;
                return Ok(None);
            }
            (None, Some(a), Some(b)) => join(a, b),
            (None, a, b) => match a.or(b) {
                Some(r) => r,
                None => return Ok(None),
            },
        };
        self.shown_region = now;
        let [x, y, w, h] = region.map(|v| v as usize);
        let stride = self.area[2] as usize;
        // the layer as it was there, and what of the flood falls there (a row at a time)
        let cut = |all: &[u8], px: usize| -> Vec<u8> {
            let mut out = Vec::with_capacity(w * h * px);
            for r in y..y + h {
                out.extend_from_slice(all.get((r * stride + x) * px..(r * stride + x + w) * px).unwrap_or(&[]));
            }
            out
        };
        let mut pixels = cut(&self.before, 4);
        if let (Some(mask), Some(flood)) = (&mask, &self.flood) {
            let data = cut(&mask.data, 1);
            let part = Mask { width: region[2], height: region[3], data };
            if self.elsewhere.is_some() {
                fill::pour_over(&mut pixels, &part, self.colour, self.alpha_lock);
            } else {
                fill::pour(&mut pixels, &part, flood.colour, self.colour, self.alpha_lock);
            }
        }
        self.shown_mask = mask;
        let [ax, ay, ..] = self.area;
        Ok(Some(([region[0] + ax, region[1] + ay, region[2], region[3]], pixels)))
    }

    /// Whether a flood reaching `r` (x, y, width, height, in the part read) comes within
    /// [`POUR_MARGIN`] of an edge of the part of the canvas read that isn't the canvas's own.
    fn outgrows(&self, r: [u32; 4]) -> bool {
        let ([x, y, w, h], [ax, ay, aw, ah], (cw, ch)) = (r, self.area, self.canvas);
        let m = POUR_MARGIN;
        !self.kept_to_area
            && ((ax > 0 && x < m)
                || (ay > 0 && y < m)
                || (ax + aw < cw && x + w + m > aw)
                || (ay + ah < ch && y + h + m > ah))
    }

    /// Whether canvas point `seed` is in the part of the canvas read.
    fn covers(&self, seed: (u32, u32)) -> bool {
        let [ax, ay, aw, ah] = self.area;
        (ax..ax + aw).contains(&seed.0) && (ay..ay + ah).contains(&seed.1)
    }

    /// Where the layer was last poured into, on the canvas (x, y, width, height).
    fn shown_on_canvas(&self) -> Option<[u32; 4]> {
        let [ax, ay, ..] = self.area;
        self.shown_region.map(|[x, y, w, h]| [x + ax, y + ay, w, h])
    }

    /// The area measured again from canvas point `seed` (in the part read), on the reference
    /// layer or the layer as it was.
    fn measure(&mut self, seed: (u32, u32)) {
        let [ax, ay, aw, ah] = self.area;
        let sample = self.elsewhere.as_deref().unwrap_or(&self.before);
        self.flood = match (seed.0.checked_sub(ax), seed.1.checked_sub(ay)) {
            (Some(x), Some(y)) => Flood::new(aw, ah, sample, (x, y)),
            _ => None,
        };
    }

    /// The point moved to canvas point `seed` (in the part read): true if the pour stays as it
    /// is, the new point being of the same colour inside what it poured (the area joined to it
    /// then the same), so only the seed moves; otherwise measured again from it, and false.
    fn moved(&mut self, seed: (u32, u32)) -> bool {
        let [ax, ay, aw, _] = self.area;
        let (x, y) = (seed.0.saturating_sub(ax), seed.1.saturating_sub(ay));
        let sample = self.elsewhere.as_deref().unwrap_or(&self.before);
        let i = y as usize * aw as usize + x as usize;
        let colour: Option<[u8; 4]> = sample.get(i * 4..i * 4 + 4).and_then(|c| c.try_into().ok());
        let inside = self.shown_mask.as_ref().and_then(|m| m.data.get(i)).is_some_and(|&v| v > 0);
        if let Some(flood) = self.flood.as_mut()
            && inside
            && colour == Some(flood.colour)
            && flood.reseed((x, y))
        {
            return true;
        }
        self.measure(seed);
        false
    }

    /// The pour as it now is, put on its layer in `canvas` where it changed (`same_flood`: only
    /// its threshold changed since it was last shown). Outgrown, and nothing shown, where its
    /// flood comes near the edge of the part of the canvas read.
    fn show(&mut self, canvas: Option<&mut procraft_gpu::Canvas>, same_flood: bool) -> Result<(), Outgrown> {
        let threshold = self.shown as f32 / 1000.0;
        let Some(([x, y, w, h], pixels)) = self.pour_region(threshold, same_flood)? else { return Ok(()) };
        if let Some(canvas) = canvas {
            canvas.put_layer_region(self.layer, x, y, w, h, pixels);
        }
        Ok(())
    }

    /// The pour measured on the whole canvas from now on, from the point it has: `before` the
    /// whole layer as the pour began, `elsewhere` the whole of what it is measured on besides.
    /// Where it poured stays known, to be put back where it no longer pours.
    fn widen(&mut self, before: Vec<u8>, elsewhere: Option<Vec<u8>>) {
        let [ax, ay, ..] = self.area;
        let seed = self.flood.as_ref().map(|f| (f.seed().0 + ax, f.seed().1 + ay));
        self.shown_region = self.shown_on_canvas();
        self.shown_mask = None;
        self.area = [0, 0, self.canvas.0, self.canvas.1];
        self.before = before;
        self.elsewhere = elsewhere;
        match seed {
            Some(seed) => self.measure(seed),
            None => self.flood = None,
        }
    }
}

/// The part of a `width` x `height` canvas a pour from `seed` reads first (x, y, width, height):
/// [`POUR_AREA`] square about it, within the canvas.
fn pour_area(seed: (u32, u32), (width, height): (u32, u32)) -> [u32; 4] {
    let side = |at: u32, of: u32| {
        let size = POUR_AREA.min(of);
        (at.saturating_sub(POUR_AREA / 2).min(of - size), size)
    };
    let ((x, w), (y, h)) = (side(seed.0, width), side(seed.1, height));
    [x, y, w, h]
}

/// The canvas rectangle of `area` (x, y, width, height).
fn area_rect([x, y, w, h]: [u32; 4]) -> IRect {
    let n = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    IRect::new(n(x), n(y), n(x.saturating_add(w)), n(y.saturating_add(h)))
}

/// `mask`, a fill from `seed`, as the plugins on the fill hook leave it: its box goes through with
/// a pixel round it, which, in the picture, is a pixel of no fill, and past the picture's edge
/// nothing. None if they change nothing, or it is too big to go through. Takes a while: nothing
/// is held while it does.
pub(crate) fn closed_by(hooks: &Hooks, mut mask: Mask, seed: (u32, u32)) -> Option<Mask> {
    let [x, y, w, h] = mask.bounds()?;
    let (left, top) = (x.saturating_sub(1), y.saturating_sub(1));
    let (right, bottom) = ((x + w + 1).min(mask.width), (y + h + 1).min(mask.height));
    let (width, height) = ((right - left) as usize, (bottom - top) as usize);
    if (width * height) as u64 > MOST_FILL {
        hooks.say(TOO_BIG);
        return None;
    }
    let (stride, words) = (mask.width as usize, width.div_ceil(64));
    let mut rows = vec![0_u64; words * height];
    for (from, to) in mask.data.chunks_exact(stride).skip(top as usize).zip(rows.chunks_exact_mut(words)) {
        for (i, &v) in from.iter().skip(left as usize).take(width).enumerate() {
            if let Some(word) = to.get_mut(i / 64).filter(|_| v > 0) {
                *word |= 1 << (i % 64);
            }
        }
    }
    let from = (seed.0.checked_sub(left)? as usize, seed.1.checked_sub(top)? as usize);
    if !hooks.fill_rows(&mut rows, width, height, from) {
        return None;
    }
    for (to, left_of) in mask.data.chunks_exact_mut(stride).skip(top as usize).zip(rows.chunks_exact(words)) {
        for (i, v) in to.iter_mut().skip(left as usize).take(width).enumerate() {
            if left_of.get(i / 64).is_some_and(|word| word >> (i % 64) & 1 == 0) {
                *v = 0;
            }
        }
    }
    Some(mask)
}

/// Ends the open pour: `keep` makes it one undo step, called `name` if given (else "ColorDrop");
/// otherwise (or with nothing poured) the layer goes back to how it was.
fn end_pour(shared: &Shared, st: &mut State, keep: bool, name: Option<&str>) {
    let Some(pour) = st.pour.take() else { return };
    let keep = keep && pour.flood.is_some();
    if let Some(canvas) = st.canvas.as_mut() {
        if keep {
            canvas.name_next_step(name.unwrap_or("ColorDrop"));
        }
        canvas.release_layer(keep, None);
    }
    st.revision += 1;
    shared.frame(st);
}

impl State {
    /// The open pour shown as it now is (see [`Pour::show`]); measured again on the whole canvas
    /// first if it outgrew the part read for it.
    fn show_pour(&mut self, same_flood: bool) {
        let Some(pour) = self.pour.as_mut() else { return };
        if pour.show(self.canvas.as_deref_mut(), same_flood).is_ok() {
            return;
        }
        self.widen_pour();
        if let Some(pour) = self.pour.as_mut() {
            let _ = pour.show(self.canvas.as_deref_mut(), false);
        }
    }

    /// The open pour measured on the whole canvas from now on (see [`Pour::widen`]): the whole
    /// layer read, as it was where the pour has poured. Kept to the part it has if the canvas
    /// can't be read.
    fn widen_pour(&mut self) {
        let Some((layer, area, measured, (cw, ch))) =
            self.pour.as_ref().map(|p| (p.layer, p.area, p.measured_on, p.canvas))
        else {
            return;
        };
        if area == [0, 0, cw, ch] {
            return;
        }
        // (the pour's work so far run first: read, the layer has it)
        self.flush();
        let read = |st: &mut State, id: LayerId| {
            let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) else { return None };
            canvas.read_layer(gpu, id).ok()
        };
        let before = read(self, layer);
        let elsewhere = match measured {
            Measured::Reference(r) => read(self, r).map(Some),
            Measured::Own => Some(None),
            Measured::AllLayers => None,
        };
        let Some(pour) = self.pour.as_mut() else { return };
        let (Some(mut before), Some(elsewhere)) = (before, elsewhere) else {
            pour.kept_to_area = true;
            return;
        };
        // the part read, as it was before the pour (it has poured there since)
        let [ax, ay, aw, _] = area.map(|v| v as usize);
        let row = aw * 4;
        for (j, part) in pour.before.chunks_exact(row.max(1)).enumerate() {
            let at = ((ay + j) * cw as usize + ax) * 4;
            if let Some(to) = before.get_mut(at..at + row) {
                to.copy_from_slice(part);
            }
        }
        pour.widen(before, elsewhere);
    }

    /// A dropped colour's fill over `region` (x, y, width, height, canvas pixels), the colour
    /// landed on window point `point`, starts spreading in: the canvas as it was is kept to show
    /// over the fill but for a hole that grows. Not with Reduce Motion, nor on a model.
    fn start_reveal(&mut self, point: Vec2, region: [u32; 4]) {
        self.end_reveal();
        if self.reduce_motion || self.scene_shown() || self.sphere.is_some() {
            return;
        }
        let Some(view) = self.view else { return };
        let [x, y, w, h] = region.map(|v| i32::try_from(v).unwrap_or(i32::MAX));
        let rect = IRect::new(x, y, x.saturating_add(w), y.saturating_add(h));
        let mut shake = Shake::from_clock();
        let Some(reveal) = Reveal::new(point, rect, &view, self.window_size, self.display_scale, &mut shake) else {
            return;
        };
        // only what the window shows of it is veiled, at no more pixels than it is shown with
        let (ww, wh) = (self.window_size.0 as f32, self.window_size.1 as f32);
        let seen = [(0.0, 0.0), (ww, 0.0), (0.0, wh), (ww, wh)].map(|(x, y)| view.screen_to_canvas(Vec2::new(x, y)));
        let seen = seen.iter().flatten().fold(None, |b: Option<(Vec2, Vec2)>, &q| {
            let (lo, hi) = b.unwrap_or((q, q));
            Some((Vec2::new(lo.x.min(q.x), lo.y.min(q.y)), Vec2::new(hi.x.max(q.x), hi.y.max(q.y))))
        });
        let Some((lo, hi)) = seen.filter(|(lo, hi)| lo.is_finite() && hi.is_finite()) else { return };
        let seen = IRect::new(
            lo.x.floor() as i32 - 2,
            lo.y.floor() as i32 - 2,
            hi.x.ceil() as i32 + 2,
            hi.y.ceil() as i32 + 2,
        );
        let scale = view.to_screen().scale_factor();
        if let (Some(canvas), Some(gpu)) = (self.canvas.as_mut(), self.gpu.as_ref()) {
            canvas.veil_begin(gpu, rect.intersect(seen), scale);
            if canvas.veil_rect().is_some() {
                self.reveal = Some(reveal);
            }
        }
    }

    /// A fill spreading in shows all of itself now.
    pub(crate) fn end_reveal(&mut self) {
        if self.reveal.take().is_some() {
            if let Some(canvas) = self.canvas.as_mut() {
                canvas.veil_end();
            }
            self.needs_frame = true;
        }
    }
}

/// The smallest rectangle (x, y, width, height) holding every pixel where masks `a` and `b` (of
/// the same size) differ; none where they don't. Looked for only `within` a rectangle (x, y,
/// width, height) outside which neither takes anything, if one is given.
fn differ(a: &Mask, b: &Mask, within: Option<[u32; 4]>) -> Option<[u32; 4]> {
    let w = (a.width as usize).max(1);
    let [x, y, bw, bh] = within.unwrap_or([0, 0, a.width, a.height]).map(|v| v as usize);
    let (x0, x1) = (x.min(w), x.saturating_add(bw).min(w));
    let (mut top, mut bottom, mut left, mut right) = (None, 0, usize::MAX, 0);
    let rows = a.data.chunks(w).zip(b.data.chunks(w)).enumerate().skip(y).take(bh);
    for (y, (p, q)) in rows {
        let (p, q) = (p.get(x0..x1).unwrap_or_default(), q.get(x0..x1).unwrap_or_default());
        if p == q {
            continue;
        }
        let Some(first) = p.iter().zip(q).position(|(u, v)| u != v) else { continue };
        let last = p.iter().zip(q).rposition(|(u, v)| u != v).unwrap_or(first);
        top.get_or_insert(y);
        bottom = y;
        left = left.min(x0 + first);
        right = right.max(x0 + last);
    }
    let top = top?;
    let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    Some([n(left), n(top), n(right - left + 1), n(bottom - top + 1)])
}

/// Rectangle `r` (x, y, width, height) grown by `by` each way, kept to a `width` x `height` picture.
fn grown(r: [u32; 4], by: u32, width: u32, height: u32) -> [u32; 4] {
    let [x, y, w, h] = r;
    let (x0, y0) = (x.saturating_sub(by), y.saturating_sub(by));
    let (x1, y1) =
        (x.saturating_add(w).saturating_add(by).min(width), y.saturating_add(h).saturating_add(by).min(height));
    [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
}

/// The smallest rectangle (x, y, width, height) holding both `a` and `b`.
fn join(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    let [ax, ay, aw, ah] = a;
    let [bx, by, bw, bh] = b;
    let (x, y) = (ax.min(bx), ay.min(by));
    [x, y, (ax + aw).max(bx + bw) - x, (ay + ah).max(by + bh) - y]
}

fn thousandths(threshold: f32) -> i32 {
    (threshold.clamp(0.0, 1.0) * 1000.0).round() as i32
}

fn unit(c: f32) -> f32 {
    if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.0 }
}

/// The canvas pixel under window point (`x`, `y`), if it is on the canvas.
/// What a pour goes by, read with the lock let go (see [`CanvasSession::read_ahead`]): the point,
/// the layer, the part of the canvas and the reference layer it was read for, the artwork's edits
/// and those layers' versions then, their pixels there, and the pour measured on them.
struct ReadAhead {
    seed: (u32, u32),
    current: LayerId,
    area: [u32; 4],
    reference: Option<LayerId>,
    seen: (u64, u64, Option<u64>),
    before: Vec<u8>,
    elsewhere: Option<Vec<u8>>,
    flood: Option<Flood>,
}

impl ReadAhead {
    /// The artwork's edits and the versions of `current` and `reference` as they are now.
    fn seen(st: &State, current: LayerId, reference: Option<LayerId>) -> Option<(u64, u64, Option<u64>)> {
        let canvas = st.canvas.as_ref()?;
        Some((st.edits(), canvas.layer_version(current), reference.map(|r| canvas.layer_version(r))))
    }

    /// Whether what was read is as the canvas is now (nothing laid or undone since).
    fn still(&self, st: &State) -> bool {
        Self::seen(st, self.current, self.reference) == Some(self.seen)
    }
}

fn seed_at(st: &crate::State, x: f32, y: f32) -> Option<(u32, u32)> {
    st.window_texel(x, y)
}

/// What a fill at a window point lands on: nothing it fills (off the artwork, or a layer that takes
/// none), text (which takes none until it is rasterized), a plugin's layer (which makes of it what
/// it will, as one step), or a layer it pours into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PourTarget {
    Nothing,
    Text,
    Plugin,
    Layer,
}

/// What a fill at window point (`x`, `y`) lands on, as [`CanvasSession::pour_begin`] goes by it
/// (whether a plugin's layer takes it is for its plugin to say).
pub(crate) fn pour_target(st: &State, x: f32, y: f32) -> PourTarget {
    match layer_target(st) {
        PourTarget::Text => PourTarget::Text,
        _ if seed_at(st, x, y).is_none() => PourTarget::Nothing,
        target => target,
    }
}

/// What the current layer is to a fill (see [`PourTarget`]); a plugin's layer whatever its plugin
/// would say.
fn layer_target(st: &State) -> PourTarget {
    let current = st.doc.current();
    let Some(layer) = st.doc.layer(current) else { return PourTarget::Nothing };
    if layer.text.is_some() {
        PourTarget::Text
    } else if st.doc.is_locked(current) {
        PourTarget::Nothing
    } else if layer.plugin.is_some() {
        PourTarget::Plugin
    } else {
        PourTarget::Layer
    }
}

/// Where the artwork is on the screen as last seen (its view and its size; none while a model is
/// shown, whose surface goes by more than that), and what its current layer is to a fill: whether
/// a dragged colour lands on it, and what Continue filling's touch fills, are told from this while
/// a pour holds the canvas, which a whole layer's does for a while.
#[derive(Default)]
pub(crate) struct LandsOn(parking_lot::Mutex<Option<Seen>>);

/// The view the artwork was last seen through, its size, and what its current layer is to a fill.
type Seen = (procraft_engine::view::View, (u32, u32), PourTarget);

impl LandsOn {
    /// Notes where the artwork is as `st` has it.
    pub(crate) fn note(&self, st: &State) {
        let seen =
            st.view.filter(|_| !st.scene_shown()).map(|v| (v, (st.doc.width(), st.doc.height()), layer_target(st)));
        *self.0.lock() = seen;
    }

    /// Whether window point (`x`, `y`) is on the artwork as last noted; none if that isn't known.
    fn lands(&self, x: f32, y: f32) -> Option<bool> {
        let (view, (w, h), _) = (*self.0.lock())?;
        let p = view.screen_to_canvas(Vec2::new(x, y))?;
        Some(p.x >= 0.0 && p.y >= 0.0 && p.x < w as f32 && p.y < h as f32)
    }

    /// What a fill at window point (`x`, `y`) lands on as last noted (see [`pour_target`]); none if
    /// that isn't known.
    pub(crate) fn target(&self, x: f32, y: f32) -> Option<PourTarget> {
        let target = self.0.lock().map(|(.., target)| target)?;
        Some(match target {
            PourTarget::Text => PourTarget::Text,
            _ if !self.lands(x, y)? => PourTarget::Nothing,
            target => target,
        })
    }
}

#[uniffi::export]
impl CanvasSession {
    /// Whether window point (`x`, `y`) is on the artwork, where a dragged colour can come to rest.
    /// Asked while the colour is dragged, it never waits on a pour holding the canvas (an earlier
    /// drop's still being made): it goes by where the artwork was last seen.
    pub fn pour_lands(&self, x: f32, y: f32) -> bool {
        if let Some(st) = self.shared.try_lock() {
            self.shared.lands_on.note(&st);
            return seed_at(&st, x, y).is_some();
        }
        match self.shared.lands_on.lands(x, y) {
            Some(lands) => lands,
            None => seed_at(&self.shared.lock(), x, y).is_some(),
        }
    }

    /// Pours colour (`r`, `g`, `b`, 0..1) at window point (`x`, `y`) with `threshold` (0..1) on
    /// the current layer, within the selection if there is one. False if nothing was poured (off
    /// the canvas, or the layer locked or text). The pour stays open for
    /// [`CanvasSession::pour_threshold`] until [`CanvasSession::pour_end`].
    pub fn pour_begin(&self, x: f32, y: f32, r: f32, g: f32, b: f32, threshold: f32) -> bool {
        self.begin_pour(x, y, [r, g, b], threshold, false)
    }

    /// Pours as [`CanvasSession::pour_begin`] does a colour dragged out and let go, or come to
    /// rest, at window point (`x`, `y`): the fill is made at once, and shows spreading in from the
    /// point over a moment (at once with Reduce Motion).
    pub fn pour_drop(&self, x: f32, y: f32, r: f32, g: f32, b: f32, threshold: f32) -> bool {
        self.begin_pour(x, y, [r, g, b], threshold, true)
    }

    /// The open pour redone from window point (`x`, `y`) instead, from the layer as it was: off
    /// the canvas it pours nothing until it comes back. False if no pour is open.
    pub fn pour_move(&self, x: f32, y: f32) -> bool {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let seed = seed_at(st, x, y);
        let Some(pour) = st.pour.as_ref().filter(|p| p.finishing.is_none()) else { return false };
        // moved off the part of the canvas read: all of it read
        if seed.is_some_and(|seed| !pour.covers(seed)) {
            st.widen_pour();
        }
        let Some(pour) = st.pour.as_mut() else { return false };
        let same = match seed {
            Some(seed) if pour.covers(seed) => pour.moved(seed),
            _ => {
                pour.flood = None;
                false
            }
        };
        // moved within what it filled, from the same colour: nothing changes
        if !same {
            st.show_pour(false);
            self.shared.frame(st);
        }
        true
    }

    /// The open pour redone with colour (`r`, `g`, `b`, 0..1).
    pub fn pour_colour(&self, r: f32, g: f32, b: f32) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let Some(pour) = st.pour.as_mut().filter(|p| p.finishing.is_none()) else { return };
        let colour = [r, g, b].map(unit);
        if colour == pour.colour {
            return;
        }
        pour.colour = colour;
        st.show_pour(false);
        self.shared.frame(st);
    }

    /// The open pour redone at `threshold` (0..1).
    pub fn pour_threshold(&self, threshold: f32) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let Some(pour) = st.pour.as_mut().filter(|p| p.finishing.is_none()) else { return };
        let t = thousandths(threshold);
        if t == pour.shown {
            return;
        }
        pour.shown = t;
        st.show_pour(true);
        self.shared.frame(st);
    }

    /// Ends the open pour: `keep` makes it one undo step, called `name` if given (else
    /// "ColorDrop"); otherwise (or with nothing poured) the layer goes back to how it was. Kept,
    /// with a plugin on the fill hook, it ends when that has had its say, on a thread of its own:
    /// [`CanvasSession::pour_finishing`] until then.
    pub fn pour_end(&self, keep: bool, name: Option<String>) {
        self.await_pour();
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        if keep && self.finish_pour(st, &name) {
            return;
        }
        end_pour(&self.shared, st, keep, name.as_deref());
    }

    /// Whether a fill let go is still being finished by a plugin: nothing should be started until
    /// it is. The pour's own calls wait for it.
    pub fn pour_finishing(&self) -> bool {
        self.shared.lock().pour.as_ref().is_some_and(|pour| pour.finishing.is_some())
    }
}

impl CanvasSession {
    /// Pours `colour` at window point (`x`, `y`) with `threshold` (see
    /// [`CanvasSession::pour_begin`]); `spread`: shown spreading in from the point.
    fn begin_pour(&self, x: f32, y: f32, colour: [f32; 3], threshold: f32, spread: bool) -> bool {
        self.pour_end(true, None);
        self.await_pour();
        let ahead = self.read_ahead(x, y);
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        // a fill still spreading in shows all of itself
        st.end_reveal();
        let [r, g, b] = colour;
        let Some(seed) = seed_at(st, x, y) else { return false };
        let (w, h) = (st.doc.width(), st.doc.height());
        let current = st.doc.current();
        let Some(layer) = st.doc.layer(current).cloned() else { return false };
        // a plugin's layer that takes a dropped colour makes of it what it will, as one step
        if layer.plugin.is_some() && !st.doc.is_locked(current) && st.plugin_layer_fills(current) {
            let at = Vec2::new(seed.0 as f32 + 0.5, seed.1 as f32 + 0.5);
            let done = st.fill_plugin_layer(current, at, [r, g, b], threshold);
            if done {
                self.shared.frame(st);
            }
            return done;
        }
        if st.doc.is_locked(current) || layer.drawn_from_data() {
            return false;
        }
        // measured on every layer seen while a plugin says so (as they are over no background, as
        // a layer's own pixels are, so a faint line stops it there as it does on its own layer),
        // else on the reference layer when another layer is it
        let all_layers = st.hooks.as_ref().is_some_and(|hooks| hooks.fill_all_layers());
        let reference = st.doc.reference().filter(|&r| r != current);
        // first only the part of the canvas about the point (a fill that comes near its edge reads
        // the rest)
        let full = [0, 0, w, h];
        let area = if all_layers || whole_at_once() { full } else { pour_area(seed, (w, h)) };
        let read = |st: &mut State, id: LayerId| {
            if area == full { st.layer_pixels(id) } else { st.layer_part(id, area_rect(area)) }
        };
        // read (and measured) with the lock let go, if nothing changed meanwhile; else now
        let ahead = ahead
            .filter(|a| (a.seed, a.current, a.area, a.reference) == (seed, current, area, reference) && a.still(st));
        let (before, elsewhere, flood) = match ahead {
            Some(a) => (a.before, a.elsewhere, Some(a.flood)),
            None => {
                let Some(before) = read(st, current) else { return false };
                (before, None, None)
            }
        };
        let within = st.reach_selection();
        let elsewhere = match (all_layers, elsewhere) {
            (true, _) => st.seen_layers_pixels(),
            (false, Some(read_ahead)) => Some(read_ahead),
            (false, None) => reference.and_then(|r| read(st, r)),
        };
        let measured_on = match (&elsewhere, reference) {
            (None, _) => Measured::Own,
            (Some(_), _) if all_layers => Measured::AllLayers,
            (Some(_), Some(r)) => Measured::Reference(r),
            (Some(_), None) => Measured::Own,
        };
        let mut pour = Pour {
            layer: current,
            canvas: (w, h),
            area,
            kept_to_area: false,
            measured_on,
            before,
            flood: None,
            elsewhere,
            colour: [r, g, b].map(unit),
            selection: within,
            alpha_lock: layer.alpha_lock,
            shown: thousandths(threshold),
            shown_region: None,
            shown_mask: None,
            closed: None,
            finishing: None,
        };
        match flood {
            Some(flood) if !all_layers => pour.flood = flood,
            _ => pour.measure(seed),
        }
        if pour.flood.is_none() {
            return false;
        }
        if let Some(canvas) = st.canvas.as_mut() {
            canvas.hold_layer(current);
        }
        st.pour = Some(pour);
        st.show_pour(false);
        let region = st.pour.as_ref().and_then(Pour::shown_on_canvas);
        if let Some(region) = region.filter(|_| spread) {
            st.start_reveal(Vec2::new(x, y), region);
        }
        self.shared.frame(st);
        true
    }

    /// The pixels a pour at window point (`x`, `y`) goes by, read as [`CanvasSession::begin_pour`]
    /// would read them but with the lock let go while the GPU copies them back: frames and touches
    /// waited on that copy, and on all the work queued before it, a fifth of a second or more on a
    /// large canvas. None where it reads otherwise (on every layer seen, or not at all).
    fn read_ahead(&self, x: f32, y: f32) -> Option<ReadAhead> {
        let (mut ahead, reads) = {
            let mut guard = self.shared.lock();
            let st = &mut *guard;
            let seed = seed_at(st, x, y)?;
            let (w, h) = (st.doc.width(), st.doc.height());
            let current = st.doc.current();
            let layer = st.doc.layer(current)?;
            if layer.plugin.is_some() || st.doc.is_locked(current) || layer.drawn_from_data() {
                return None;
            }
            if st.hooks.as_ref().is_some_and(|hooks| hooks.fill_all_layers()) {
                return None;
            }
            let reference = st.doc.reference().filter(|&r| r != current);
            let area = if whole_at_once() { [0, 0, w, h] } else { pour_area(seed, (w, h)) };
            let before = st.start_layer_part(current, area_rect(area))?;
            let other = match reference {
                Some(r) => Some(st.start_layer_part(r, area_rect(area))?),
                None => None,
            };
            let ahead = ReadAhead {
                seed,
                current,
                area,
                reference,
                seen: ReadAhead::seen(st, current, reference)?,
                before: Vec::new(),
                elsewhere: None,
                flood: None,
            };
            (ahead, (before, other))
        };
        ahead.before = reads.0.pixels().ok()?;
        ahead.elsewhere = match reads.1 {
            Some(read) => Some(read.pixels().ok()?),
            None => None,
        };
        // and measured, as the pour measures it
        let [ax, ay, aw, ah] = ahead.area;
        let sample = ahead.elsewhere.as_deref().unwrap_or(&ahead.before);
        ahead.flood = match (ahead.seed.0.checked_sub(ax), ahead.seed.1.checked_sub(ay)) {
            (Some(x), Some(y)) => Flood::new(aw, ah, sample, (x, y)),
            _ => None,
        };
        Some(ahead)
    }

    /// Waits for a fill a plugin is finishing, if there is one.
    pub(crate) fn await_pour(&self) {
        let finishing = self.shared.lock().pour.as_ref().and_then(|pour| pour.finishing.clone());
        if let Some(finishing) = finishing {
            finishing.wait();
        }
    }

    /// Hands the open pour to a thread of its own, to end when the plugins on the fill hook have
    /// had their say; false if none wants to, and it is to end now.
    fn finish_pour(&self, st: &mut State, name: &Option<String>) -> bool {
        let Some(hooks) = st.hooks.clone().filter(|hooks| hooks.fill_mask()) else { return false };
        let Some(pour) = st.pour.as_mut() else { return false };
        let threshold = pour.shown as f32 / 1000.0;
        let Some(flood) = pour.flood.as_ref() else { return false };
        let (mask, seed) = (flood.mask(threshold), flood.seed());
        let finishing = Arc::new(Finishing::default());
        let (shared, ended, name) = (Arc::clone(&self.shared), Arc::clone(&finishing), name.clone());
        let work = move || {
            let closed = closed_by(&hooks, mask, seed);
            let mut guard = shared.lock();
            let st = &mut *guard;
            if let (Some(pour), Some(closed)) = (st.pour.as_mut(), closed) {
                pour.closed = Some(closed);
                let _ = pour.show(st.canvas.as_deref_mut(), false);
            }
            end_pour(&shared, st, true, name.as_deref());
            drop(guard);
            ended.end();
        };
        if std::thread::Builder::new().name("fill".to_owned()).spawn(work).is_err() {
            return false;
        }
        pour.finishing = Some(finishing);
        true
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A mask filled all over.
    pub(crate) fn mask_of_box(width: u32, height: u32) -> Mask {
        Mask { width, height, data: vec![255; width as usize * height as usize] }
    }

    #[test]
    fn a_fill_goes_through_a_row_of_bits_and_comes_back_by_where_it_was() {
        use crate::plugin_run::tests::{FILL_AT, fill_module};
        use crate::plugin_run::{Hook, Running};
        // takes out the fifth pixel of every row it is handed
        let out = "(i64.store (i32.const 65536) (i64.const 16))";
        let hooks = Hooks::default();
        let module = fill_module(FILL_AT, "(i32.const 1)", out);
        hooks.put_on("fill".to_owned(), Running::start(&module, &[]).unwrap(), &[Hook::FillMask]);
        // a block of 10 x 3 at (20, 5) in a mask of 100 x 50, with soft values
        let mut mask = Mask { width: 100, height: 50, data: vec![0; 100 * 50] };
        for y in 5..8 {
            mask.data[y * 100 + 20..y * 100 + 30].fill(90);
        }
        let closed = closed_by(&hooks, mask.clone(), (22, 6)).unwrap();
        // its box and a pixel round it start at (19, 4): the fifth pixel is at 23
        for y in 5..8 {
            assert_eq!(closed.data[y * 100 + 23], 0, "{y}");
            assert_eq!(closed.data[y * 100 + 22], 90, "what stays keeps its value");
            assert_eq!(closed.data[y * 100 + 24], 90);
        }
        assert_eq!(closed.data.iter().filter(|&&v| v > 0).count(), 27);
    }

    #[test]
    fn a_box_at_the_picture_s_edge_has_nothing_past_it() {
        use crate::plugin_run::tests::fill_module;
        use crate::plugin_run::{Hook, Running};
        // it must be told 3 wide and 2 tall (nothing left or above, a pixel right and below) and
        // the seed at (1, 0), or it crashes
        let begin = "(if (i32.or (i32.ne (local.get 0) (i32.const 3)) (i32.or (i32.ne (local.get 1) (i32.const 2))
                (i32.or (i32.ne (local.get 2) (i32.const 1)) (i32.ne (local.get 3) (i32.const 0)))))
                (then (unreachable)))
             (i32.const 65536)";
        let hooks = Hooks::default();
        let module = fill_module(begin, "(i32.const 0)", "");
        hooks.put_on("fill".to_owned(), Running::start(&module, &[]).unwrap(), &[Hook::FillMask]);
        let mut mask = Mask { width: 100, height: 50, data: vec![0; 100 * 50] };
        mask.data[0..2].fill(255);
        assert!(closed_by(&hooks, mask, (1, 0)).is_none());
        assert_eq!(hooks.take_said(), None);
    }
}
