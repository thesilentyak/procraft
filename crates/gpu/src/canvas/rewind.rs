//! Going back over the end of a stroke painting its layer in place. Such a stroke changes its layer
//! as it goes, so one laid again as it ends (its end taper reaching back over what it has laid) was
//! laid again from its start: on a slow GPU a long stroke's end went on being laid for a second or
//! two after the pen lifted, the picture held meanwhile. The stroke keeps marks to go back to
//! instead: at each, what it carries, and the tiles its stamps after it write to as they were when
//! first written (or that they were made after it). Laid again, it goes back to the newest mark
//! before the first stamp that changed where a display frame's stamps begin, and lays only what
//! follows: that leaves what laying it all again from its start would.

use crate::Gpu;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use procraft_engine::geom::IRect;

use super::smudge::{tile_rect, tiles_of};
use super::{Canvas, Carry, Kept, LAYER_FORMAT, Op, PacedStamp, copy_at, extent, texture, wet_pass};

/// Marks are at least this far apart in the stroke's input time (ms): two display frames.
pub(super) const MARK_EVERY_MS: f64 = 2.0 * super::WET_FRAME_MS;
/// Marks are kept this far back in input time from the newest (ms), the oldest at least this far
/// back: past where an end taper reaches (its time is at most `TAPER_TIME_MS`, and it is eased over
/// a point either side, and walked again from a little before).
pub(super) const MARKS_REACH_MS: f64 = 240.0;
/// The most the marks' pictures may take (bytes), and on a device of little memory: past it the
/// oldest go, and a stroke whose end reaches back past those left is laid again from its start.
pub(super) const MARKS_MOST: u64 = 160 << 20;
const MARKS_MOST_LOW: u64 = 48 << 20;
/// The bytes of the plots wet paint carries from one pass to the next (two, taking turns, at the
/// start of the stroke's carry buffer; see `CARRIED` in the shader).
const CARRIED_BYTES: u64 = 2 * (super::WET_PLOT_MOST as u64 * super::WET_PLOT_MOST as u64) * 16;
/// The most stamps a stroke keeps as they were given to go back over (past it, it can't).
pub(super) const GIVEN_MOST: usize = 1 << 17;

/// What a stroke painting in place keeps to go back to (see the module).
pub(super) struct Rewind {
    /// The stamps given to the stroke so far, as given (laid, and waiting for the rest of their
    /// display frame), and how many of them are laid.
    given: Vec<PacedStamp>,
    laid: usize,
    /// The marks, oldest first.
    marks: VecDeque<Mark>,
    /// Tile pictures and carry copies kept to be taken again, and the bytes those and the marks' take.
    free: Vec<wgpu::Texture>,
    free_carries: Vec<wgpu::Buffer>,
    bytes: u64,
    most: u64,
    /// The side of the stroke's tiles (pixels), and so of the marks' pictures of them.
    side: u32,
    /// Whether it keeps what going back takes (not past the most stamps it keeps, nor with a
    /// symmetry's copies of the stroke, which take turns in what they carry).
    on: bool,
}

/// A place in the stroke to go back to: before its `at`th stamp (input at `time_ms`).
struct Mark {
    at: usize,
    time_ms: f64,
    /// What each copy of the stroke carried, and which one's the carry buffer held (and its plots
    /// carried over, copied).
    carries: Vec<Carry>,
    resident: Option<(usize, Kept)>,
    carry: Option<wgpu::Buffer>,
    /// The tiles written to since, as they were at the mark: the layer's pixels and what the stroke
    /// keeps of them (the stroke as laid, how much each pixel has taken, how much of the paint
    /// shows), each as the tile keeps it; and the tiles made since.
    saved: BTreeMap<(u32, u32), Saved>,
    made: BTreeSet<(u32, u32)>,
}

struct Saved {
    layer: wgpu::Texture,
    kept: [Option<wgpu::Texture>; 3],
}

impl Saved {
    fn textures(self) -> impl Iterator<Item = wgpu::Texture> {
        std::iter::once(self.layer).chain(self.kept.into_iter().flatten())
    }
}

impl Mark {
    fn let_go(self, rewind_free: &mut Vec<wgpu::Texture>, carries: &mut Vec<wgpu::Buffer>) {
        carries.extend(self.carry);
        rewind_free.extend(self.saved.into_values().flat_map(Saved::textures));
    }
}

/// Whether two stamps painted in place are the same: as drawn, and where they come in the stroke.
pub(super) fn same((a, aw, ap): &PacedStamp, (b, bw, bp): &PacedStamp) -> bool {
    bytemuck::bytes_of(a) == bytemuck::bytes_of(b)
        && aw.iter().zip(bw).all(|(x, y)| x.to_bits() == y.to_bits())
        && ap.time_ms.to_bits() == bp.time_ms.to_bits()
        && ap.distance.to_bits() == bp.distance.to_bits()
        && ap.copy == bp.copy
}

impl Rewind {
    /// Kept for a stroke beginning whose tiles are `side` pixels across, its marks' pictures within
    /// `most` bytes (none: as much as a device of little memory, or not, gives them).
    pub(super) fn new(low_memory: bool, most: Option<u64>, side: u32) -> Rewind {
        Rewind {
            given: Vec::new(),
            laid: 0,
            marks: VecDeque::new(),
            free: Vec::new(),
            free_carries: Vec::new(),
            bytes: 0,
            most: most.unwrap_or(if low_memory { MARKS_MOST_LOW } else { MARKS_MOST }),
            side,
            on: true,
        }
    }

    /// Stamps given to the stroke, to be laid.
    pub(super) fn give(&mut self, stamps: impl Iterator<Item = PacedStamp>) {
        if !self.on {
            return;
        }
        let before = self.given.len();
        self.given.extend(stamps);
        let copies = self.given.get(before..).unwrap_or_default().iter().any(|s| s.2.copy != 0);
        if copies || self.given.len() > GIVEN_MOST {
            self.stop();
        }
    }

    /// `count` more of the stamps given are laid.
    pub(super) fn laid(&mut self, count: usize) {
        self.laid += count;
    }

    /// The stroke starts again from scratch: nothing laid, nothing to go back to.
    pub(super) fn restart(&mut self) {
        self.given.clear();
        self.laid = 0;
        while let Some(mark) = self.marks.pop_front() {
            mark.let_go(&mut self.free, &mut self.free_carries);
        }
    }

    /// It keeps nothing more to go back to.
    fn stop(&mut self) {
        self.on = false;
        self.given = Vec::new();
        self.marks.clear();
        self.free.clear();
        self.free_carries.clear();
        self.bytes = 0;
    }

    /// The marks' pictures and carries, and those kept to be taken again (bytes).
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The textures the marks' pictures are kept in, and those kept to be taken again.
    pub(super) fn textures(&self) -> impl Iterator<Item = &wgpu::Texture> {
        let saved = self.marks.iter().flat_map(|m| m.saved.values());
        let saved = saved.flat_map(|s| std::iter::once(&s.layer).chain(s.kept.iter().flatten()));
        saved.chain(&self.free)
    }

    /// A tile picture to keep a mark's in: one let go of, else a new one if the most allows,
    /// letting go of the oldest marks (not the newest) for room.
    fn take(&mut self, gpu: &Gpu) -> Option<wgpu::Texture> {
        loop {
            if let Some(t) = self.free.pop() {
                return Some(t);
            }
            // (each texture a mark keeps of a tile, its bytes)
            let tile_bytes = u64::from(self.side) * u64::from(self.side) * 4;
            if self.bytes + tile_bytes <= self.most {
                self.bytes += tile_bytes;
                return Some(texture(gpu, "stroke mark", self.side, self.side, LAYER_FORMAT));
            }
            if self.marks.len() < 2 {
                return None;
            }
            let oldest = self.marks.pop_front()?;
            oldest.let_go(&mut self.free, &mut self.free_carries);
        }
    }

    /// The newest mark, given up (it couldn't keep all it takes).
    fn drop_newest(&mut self) {
        if let Some(mark) = self.marks.pop_back() {
            mark.let_go(&mut self.free, &mut self.free_carries);
        }
    }

    /// The newest mark before the first of `stamps` (the stroke's stamps from its start, laid again)
    /// that differs from those given, where a display frame's stamps begin in them: its place in
    /// the marks, and how many of the stamps lie before it. `given` more were given after those it
    /// keeps, yet to be laid (in the work queued before these).
    fn point(&self, stamps: &[PacedStamp], given: &[PacedStamp], preview: bool) -> Option<(usize, usize)> {
        if !self.on {
            return None;
        }
        let kept = self.given.iter().chain(given);
        let unchanged = stamps.iter().zip(kept).take_while(|(a, b)| same(a, b)).count();
        let frame_starts = |at: usize| match (at.checked_sub(1).and_then(|i| stamps.get(i)), stamps.get(at)) {
            (Some(before), Some(first)) => wet_pass(before.2.time_ms, preview) != wet_pass(first.2.time_ms, preview),
            (_, None) => true,
            (None, Some(_)) => false,
        };
        self.marks
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| m.at > 0 && m.at <= unchanged && m.at <= self.laid && frame_starts(m.at))
            .map(|(k, m)| (k, m.at))
    }
}

impl Canvas {
    /// Whether [`Canvas::mark_in_place`] marks the open stroke before stamps input from `time_ms`
    /// on (it is time for one).
    pub(super) fn marks_in_place(&self, time_ms: f64) -> bool {
        self.smudge
            .as_ref()
            .and_then(|s| s.rewind.as_ref())
            .filter(|r| r.on && r.laid > 0)
            .is_some_and(|r| r.marks.back().is_none_or(|m| time_ms - m.time_ms >= MARK_EVERY_MS))
    }

    /// Marks the open stroke painting in place to go back to, before stamps input from `time_ms`
    /// on are laid (if it is time for one).
    pub(super) fn mark_in_place(&mut self, encoder: &mut wgpu::CommandEncoder, device: &wgpu::Device, time_ms: f64) {
        let Some(smudge) = self.smudge.as_mut() else { return };
        let Some(rewind) = smudge.rewind.as_mut().filter(|r| r.on && r.laid > 0) else { return };
        if rewind.marks.back().is_some_and(|m| time_ms - m.time_ms < MARK_EVERY_MS) {
            return;
        }
        // what it carries over between passes, copied
        let carry = match (smudge.carry.as_ref(), smudge.wet) {
            (Some(from), true) => {
                let into = rewind.free_carries.pop().or_else(|| {
                    (rewind.bytes + CARRIED_BYTES <= rewind.most).then(|| {
                        rewind.bytes += CARRIED_BYTES;
                        device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("stroke mark carry"),
                            size: CARRIED_BYTES,
                            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        })
                    })
                });
                let Some(into) = into else { return };
                encoder.copy_buffer_to_buffer(from, 0, &into, 0, CARRIED_BYTES);
                Some(into)
            }
            _ => None,
        };
        rewind.marks.push_back(Mark {
            at: rewind.laid,
            time_ms,
            carries: smudge.carries.iter().map(Carry::plain).collect(),
            resident: smudge.resident,
            carry,
            saved: BTreeMap::new(),
            made: BTreeSet::new(),
        });
        // the oldest let go once the one after it is far enough back
        while rewind.marks.get(1).is_some_and(|m| time_ms - m.time_ms >= MARKS_REACH_MS) {
            if let Some(oldest) = rewind.marks.pop_front() {
                oldest.let_go(&mut rewind.free, &mut rewind.free_carries);
            }
        }
        self.marks_made += 1;
    }

    /// The tiles `writes` reach, about to be written by the open stroke painting in place, kept as
    /// they are for its newest mark if this is their first write since (those not made yet noted as
    /// made since: as they were before the stroke).
    pub(super) fn keep_for_mark(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, writes: &[IRect]) {
        let full = self.full();
        let Some(smudge) = self.smudge.as_mut() else { return };
        let Some(layer) = self.layers.get(&smudge.into) else { return };
        let Some(rewind) = smudge.rewind.as_mut().filter(|r| r.on) else { return };
        let Some(mark) = rewind.marks.back_mut() else { return };
        let side = smudge.tile;
        let mut touched: Vec<(u32, u32)> = writes.iter().flat_map(|w| tiles_of(*w, side)).collect();
        touched.sort_unstable();
        touched.dedup();
        touched.retain(|t| !mark.made.contains(t) && !mark.saved.contains_key(t));
        mark.made.extend(touched.iter().filter(|t| !smudge.tiles.contains_key(t)));
        for at in touched {
            let Some(tile) = smudge.tiles.get(&at) else { continue };
            let kept = [&tile.grained, &tile.amount, &tile.shown];
            let wanted = 1 + kept.iter().filter(|k| k.is_some()).count();
            let mut taken = Vec::with_capacity(wanted);
            for _ in 0..wanted {
                match rewind.take(gpu) {
                    Some(t) => taken.push(t),
                    None => break,
                }
            }
            // past the most it keeps: this mark can't be gone back to
            if taken.len() < wanted {
                rewind.free.extend(taken);
                rewind.drop_newest();
                return;
            }
            let r = tile_rect(at, side).intersect(full);
            let mut taken = taken.into_iter();
            let Some(picture) = taken.next() else { return };
            gpu.copy_texture_to_texture(
                encoder,
                copy_at(&layer.texture, r.x0, r.y0),
                copy_at(&picture, 0, 0),
                extent(r.width(), r.height()),
            );
            let mut saved = Saved { layer: picture, kept: [None, None, None] };
            for (from, into) in kept.into_iter().zip(saved.kept.iter_mut()) {
                if let Some(from) = from {
                    let Some(t) = taken.next() else { break };
                    gpu.copy_texture_to_texture(encoder, copy_at(from, 0, 0), copy_at(&t, 0, 0), extent(side, side));
                    *into = Some(t);
                }
            }
            // (the newest mark is still there: none was let go for room but older ones)
            if let Some(mark) = rewind.marks.back_mut() {
                mark.saved.insert(at, saved);
            }
        }
    }

    /// The open stroke painting in place, laid again from its start in `ops` (the work queued for a
    /// render), goes back to its newest mark before what changed instead, and lays only what
    /// follows: `ops` begin with [`Op::RewindInPlace`] to the mark, the stamps given since it was
    /// laid that they would lay first are left out (laid again from the mark), and the stroke's
    /// stamps from scratch become those after the mark.
    pub(super) fn rewound(&self, ops: Vec<Op>) -> Vec<Op> {
        let Some(smudge) = self.smudge.as_ref() else { return ops };
        let layer = smudge.layer;
        let Some(rewind) = smudge.rewind.as_ref().filter(|r| r.on && !r.marks.is_empty()) else { return ops };
        if self.pipelines.compute.is_none() || smudge.stalled {
            return ops;
        }
        let is_own = |op: &Op| {
            matches!(op, Op::Stamps { second: false, pencil: false, layer: on, style, .. }
                if *on == layer && style.paints_in_place())
        };
        let Some(from) = ops.iter().position(|op| is_own(op) && matches!(op, Op::Stamps { clear: true, .. })) else {
            return ops;
        };
        // a stroke begun or ended before it isn't this one
        if ops.iter().take(from).any(|op| matches!(op, Op::Commit(_) | Op::Cancel)) {
            return ops;
        }
        let given: Vec<PacedStamp> = ops
            .iter()
            .take(from)
            .filter(|op| is_own(op))
            .flat_map(|op| match op {
                Op::Stamps { instances, wet, times, .. } => {
                    instances.iter().zip(wet).zip(times).map(|((s, w), t)| (*s, *w, *t)).collect::<Vec<_>>()
                }
                _ => Vec::new(),
            })
            .collect();
        let Some(Op::Stamps { instances, wet, times, style, .. }) = ops.get(from) else { return ops };
        let stamps: Vec<PacedStamp> = instances.iter().zip(wet).zip(times).map(|((s, w), t)| (*s, *w, *t)).collect();
        let Some((_, at)) = rewind.point(&stamps, &given, style.preview) else { return ops };
        // gone back first, before any other work: nothing else then makes a mark or lets one go (the
        // work before it in the queue is the stroke's second's, which doesn't read the first: what
        // is put together of the two is put together over the whole stroke again as it ends)
        let mut out = Vec::with_capacity(ops.len() + 1);
        out.push(Op::RewindInPlace(at));
        for (k, op) in ops.into_iter().enumerate() {
            match op {
                // laid again from the mark
                _ if k < from && is_own(&op) => {}
                Op::Stamps { second, instances, wet, picks, times, style, layer, pencil, carry, .. } if k == from => {
                    out.push(Op::Stamps {
                        second,
                        instances: instances.into_iter().skip(at).collect(),
                        wet: wet.into_iter().skip(at).collect(),
                        picks: picks.into_iter().skip(at).collect(),
                        times: times.into_iter().skip(at).collect(),
                        style,
                        clear: false,
                        again: true,
                        layer,
                        pencil,
                        carry,
                    });
                }
                op => out.push(op),
            }
        }
        out
    }

    /// Goes back to the open stroke's mark before its `at`th stamp (see [`Canvas::rewound`]): the
    /// tiles written since as they were then (those made since as they were before the stroke, and
    /// no longer kept), and what it carried; the stamps waiting for the rest of their frame dropped.
    pub(super) fn rewind_in_place(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, at: usize) {
        let full = self.full();
        let Some(smudge) = self.smudge.as_mut() else { return };
        let Some(layer) = self.layers.get(&smudge.into) else { return };
        let side = smudge.tile;
        let Some(rewind) = smudge.rewind.as_mut() else { return };
        let Some(k) = rewind.marks.iter().position(|m| m.at == at) else {
            log::error!("a stroke's mark at {at} went before it was gone back to");
            return;
        };
        let since: Vec<Mark> = rewind.marks.drain(k..).collect();
        let made: BTreeSet<(u32, u32)> = since.iter().flat_map(|m| m.made.iter().copied()).collect();
        // each tile as its first picture since the mark has it
        let mut first: BTreeMap<(u32, u32), &Saved> = BTreeMap::new();
        for mark in &since {
            for (at, saved) in &mark.saved {
                if !made.contains(at) {
                    first.entry(*at).or_insert(saved);
                }
            }
        }
        let mut changed = IRect::EMPTY;
        for at in &made {
            let Some(tile) = smudge.tiles.remove(at) else { continue };
            let r = tile_rect(*at, side).intersect(full);
            gpu.copy_texture_to_texture(
                encoder,
                copy_at(&tile.before, 0, 0),
                copy_at(&layer.texture, r.x0, r.y0),
                extent(r.width(), r.height()),
            );
            changed = changed.union(r);
        }
        for (at, saved) in first {
            let Some(tile) = smudge.tiles.get(&at) else { continue };
            let r = tile_rect(at, side).intersect(full);
            gpu.copy_texture_to_texture(
                encoder,
                copy_at(&saved.layer, 0, 0),
                copy_at(&layer.texture, r.x0, r.y0),
                extent(r.width(), r.height()),
            );
            for (from, into) in saved.kept.iter().zip([&tile.grained, &tile.amount, &tile.shown]) {
                if let (Some(from), Some(into)) = (from, into) {
                    gpu.copy_texture_to_texture(encoder, copy_at(from, 0, 0), copy_at(into, 0, 0), extent(side, side));
                }
            }
            changed = changed.union(r);
        }
        let mut since = since.into_iter();
        let Some(mark) = since.next() else { return };
        for later in since {
            later.let_go(&mut rewind.free, &mut rewind.free_carries);
        }
        if let (Some(from), Some(into)) = (mark.carry.as_ref(), smudge.carry.as_ref()) {
            encoder.copy_buffer_to_buffer(from, 0, into, 0, CARRIED_BYTES);
        }
        smudge.carries = mark.carries.iter().map(Carry::plain).collect();
        smudge.resident = mark.resident;
        smudge.pending.clear();
        rewind.given.truncate(at);
        rewind.laid = at;
        // (the mark stays: what it saved is still as the stroke was there)
        rewind.marks.push_back(mark);
        // (paint laid apart, put together there with its second as it then was)
        smudge.stale = smudge.stale.union(changed);
        self.dirty = self.dirty.union(changed);
        self.rewound += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::super::StampInstance;
    use super::*;

    fn stamp(x: f32, time_ms: f64) -> PacedStamp {
        let s = StampInstance { center: [x, 0.0], radius: 4.0, ..bytemuck::Zeroable::zeroed() };
        (s, [0.0; 8], super::super::Paced { time_ms, distance: x, copy: 0 })
    }

    fn rewind_with(marks: &[usize], given: &[PacedStamp]) -> Rewind {
        let mut r = Rewind::new(false, None, super::super::smudge::TILE);
        r.give(given.iter().copied());
        r.laid(given.len());
        for &at in marks {
            r.marks.push_back(Mark {
                at,
                time_ms: 0.0,
                carries: Vec::new(),
                resident: None,
                carry: None,
                saved: BTreeMap::new(),
                made: BTreeSet::new(),
            });
        }
        r
    }

    #[test]
    fn a_stroke_goes_back_to_the_newest_mark_before_what_changed_where_a_frame_begins() {
        // two stamps a display frame (17 ms)
        let given: Vec<PacedStamp> = (0..20).map(|k| stamp(k as f32, k as f64 * 8.4)).collect();
        let r = rewind_with(&[4, 8, 12, 16], &given);
        let mut again = given.clone();
        // the 13th changes: back to the mark at 12 (a frame begins at 12: 12 × 8.4 = 100.8 ms)
        again[13].0.radius = 2.0;
        assert_eq!(r.point(&again, &[], false), Some((2, 12)));
        // the 12th changes: still back to 12 (the mark is before it); the 11th: back to 8
        again[12].0.radius = 2.0;
        assert_eq!(r.point(&again, &[], false), Some((2, 12)));
        again[11].0.radius = 2.0;
        assert_eq!(r.point(&again, &[], false), Some((1, 8)));
        // nothing before the first mark is unchanged: none
        again[2].0.radius = 2.0;
        assert_eq!(r.point(&again, &[], false), None);
    }

    #[test]
    fn a_mark_in_the_middle_of_a_display_frame_isnt_gone_back_to() {
        let given: Vec<PacedStamp> = (0..20).map(|k| stamp(k as f32, k as f64 * 8.4)).collect();
        let r = rewind_with(&[4, 8], &given);
        let mut again = given.clone();
        again[10].0.radius = 2.0;
        // a stamp laid again now comes in the frame of the one before the mark at 8
        again[8].2.time_ms = again[7].2.time_ms;
        assert_eq!(r.point(&again, &[], false), Some((0, 4)));
    }

    #[test]
    fn stamps_given_but_not_yet_laid_count_as_given() {
        let given: Vec<PacedStamp> = (0..20).map(|k| stamp(k as f32, k as f64 * 8.4)).collect();
        let r = rewind_with(&[4, 8], &given[..10]);
        let mut again = given.clone();
        again[15].0.radius = 2.0;
        // (no mark past the 10 laid)
        assert_eq!(r.point(&again, &given[10..], false), Some((1, 8)));
        // a symmetry's copies keep nothing to go back to
        let mut copies = rewind_with(&[4], &given[..10]);
        let mut copy = stamp(1.0, 90.0);
        copy.2.copy = 1;
        copies.give(std::iter::once(copy));
        assert_eq!(copies.point(&again, &[], false), None);
    }
}
