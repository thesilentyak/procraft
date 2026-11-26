//! Going back over the end of a dual brush's second. Its end taper reaching back over what it had
//! laid, a dual brush's second was laid again whole from its start as the stroke ended: thousands of
//! a big brush's stamps (Evergreen, Pansy, Wexcombe, Elfin Saddle), the end taking a tablet half a
//! second or more after the lift. The second keeps marks to go back to instead, as paint laid in
//! place does (see `rewind.rs`): at each, the tiles its stamps after it write to as they were when
//! first written since (or that nothing had written them yet). Laid again, it goes back to the
//! newest mark before the first stamp that changed and lays only what follows: bit for bit what
//! laying it whole again leaves, its stamps drawn over each other or kept strongest. Not a second
//! laid by its rule: going on from a mark, its lots would round its sums elsewhere, a level here and
//! there, and where its dual mode cuts at a threshold that put pixels on the other side (98 levels
//! apart for Pansy). Nor kept on a device of little memory, nor where laying whole is asked for.
//! Nothing it keeps changes how the stroke is drawn meanwhile.

use crate::Gpu;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::rewind::{GIVEN_MOST, MARK_EVERY_MS, MARKS_MOST, MARKS_REACH_MS, same};
use super::smudge::{tile_rect, tiles_of};
use procraft_engine::geom::IRect;

use super::{Canvas, Op, Paced, PacedStamp, StampInstance, copy_at, extent, quad_reach, texture};

/// The side of the second's tiles its marks keep (pixels).
const SIDE: u32 = 256;

/// What a dual brush's second keeps to go back to (see the module).
pub(super) struct SecondMarks {
    /// The stamps laid into the second since it was cleared, as laid.
    given: Vec<PacedStamp>,
    /// The marks, oldest first.
    marks: VecDeque<Mark>,
    /// The tiles written since the second was cleared (the rest are as cleared).
    written: BTreeSet<(u32, u32)>,
    /// Tile pictures kept to be taken again, a tile as cleared, and the bytes those and the marks'
    /// take, within the most they may.
    free: Vec<wgpu::Texture>,
    blank: Option<wgpu::Texture>,
    bytes: u64,
    most: u64,
    /// Whether it keeps what going back takes (not past the most stamps it keeps, nor with a
    /// symmetry's copies of the stroke, nor once the second is carried from lot to lot).
    on: bool,
}

/// A place in the second to go back to: before its `at`th stamp (input at `time_ms`).
struct Mark {
    at: usize,
    time_ms: f64,
    /// The tiles written to since, as they were at the mark, and those nothing had written then.
    saved: BTreeMap<(u32, u32), wgpu::Texture>,
    blank: BTreeSet<(u32, u32)>,
}

impl SecondMarks {
    /// Kept for a second cleared, its marks' pictures within `most` bytes (none: as many as a
    /// device of some memory gives them).
    pub(super) fn new(most: Option<u64>) -> SecondMarks {
        SecondMarks {
            given: Vec::new(),
            marks: VecDeque::new(),
            written: BTreeSet::new(),
            free: Vec::new(),
            blank: None,
            bytes: 0,
            most: most.unwrap_or(MARKS_MOST),
            on: true,
        }
    }

    /// The textures the marks' pictures are kept in, and those kept to be taken again.
    pub(super) fn textures(&self) -> impl Iterator<Item = &wgpu::Texture> {
        self.marks.iter().flat_map(|m| m.saved.values()).chain(&self.free).chain(&self.blank)
    }

    /// It keeps nothing more to go back to.
    fn stop(&mut self) {
        self.on = false;
        self.given = Vec::new();
        self.marks.clear();
        self.written.clear();
        self.free.clear();
        self.blank = None;
        self.bytes = 0;
    }

    /// The newest mark, given up (it couldn't keep all it takes).
    fn drop_newest(&mut self) {
        if let Some(mark) = self.marks.pop_back() {
            self.free.extend(mark.saved.into_values());
        }
    }

    /// A tile picture to keep a mark's in: one let go of, else a new one if the most allows,
    /// letting go of the oldest marks (not the newest) for room.
    fn take(&mut self, gpu: &Gpu, format: wgpu::TextureFormat) -> Option<wgpu::Texture> {
        loop {
            if let Some(t) = self.free.pop() {
                return Some(t);
            }
            let tile_bytes = u64::from(SIDE * SIDE) * u64::from(format.block_copy_size(None).unwrap_or(8));
            if self.bytes + tile_bytes <= self.most {
                self.bytes += tile_bytes;
                return Some(texture(gpu, "second mark", SIDE, SIDE, format));
            }
            if self.marks.len() < 2 {
                return None;
            }
            let oldest = self.marks.pop_front()?;
            self.free.extend(oldest.saved.into_values());
        }
    }
}

impl Canvas {
    /// Whether a dual brush's second cleared now keeps marks to go back to (see the module).
    fn keeps_second_marks(&self) -> bool {
        self.rewinds
            && self.second_may_lay_again
            && !self.ends_whole
            && !self.low_memory()
            && self.pipelines.compute.is_some()
            && self.pipelines.fragments.is_none()
    }

    /// The open stroke's second is cleared (it begins, or is laid from scratch): what it kept to go
    /// back to goes, and it keeps marks from here if it can.
    pub(super) fn second_cleared(&mut self) {
        let most = self.rewind_most;
        self.second_marks = self.keeps_second_marks().then(|| SecondMarks::new(most));
    }

    /// The open stroke's `stamps` (into its second, `into`; `by_rule`: laid by its rule, or in
    /// fragment passes) are about to be laid: the second marked before them if it is time for a mark, the tiles they
    /// write to kept as they are for the newest mark where this is their first write since, and
    /// the stamps noted as laid.
    pub(super) fn mark_second(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        into: &wgpu::Texture,
        (instances, wet, times): (&[StampInstance], &[[f32; 8]], &[Paced]),
        by_rule: bool,
    ) {
        let full = self.full();
        let format = self.pipelines.stroke_format;
        let whole = self.ends_whole;
        let Some(marks) = self.second_marks.as_mut().filter(|m| m.on) else { return };
        let copies = times.iter().any(|t| t.copy != 0);
        if by_rule || whole || copies || marks.given.len() + instances.len() > GIVEN_MOST {
            marks.stop();
            return;
        }
        if let Some(time_ms) = times.first().map(|t| t.time_ms)
            && !marks.given.is_empty()
            && marks.marks.back().is_none_or(|m| time_ms - m.time_ms >= MARK_EVERY_MS)
        {
            marks.marks.push_back(Mark {
                at: marks.given.len(),
                time_ms,
                saved: BTreeMap::new(),
                blank: BTreeSet::new(),
            });
            // the oldest let go once the one after it is far enough back
            while marks.marks.get(1).is_some_and(|m| time_ms - m.time_ms >= MARKS_REACH_MS) {
                if let Some(oldest) = marks.marks.pop_front() {
                    marks.free.extend(oldest.saved.into_values());
                }
            }
            self.second_marks_made += 1;
        }
        let mut touched: Vec<(u32, u32)> =
            instances.iter().flat_map(|s| tiles_of(quad_reach(s).intersect(full), SIDE)).collect();
        touched.sort_unstable();
        touched.dedup();
        if let Some(mark) = marks.marks.back_mut() {
            touched.retain(|t| !mark.saved.contains_key(t) && !mark.blank.contains(t));
            mark.blank.extend(touched.iter().filter(|t| !marks.written.contains(t)));
        }
        let kept: Vec<(u32, u32)> = match marks.marks.is_empty() {
            true => Vec::new(),
            false => touched.iter().copied().filter(|t| marks.written.contains(t)).collect(),
        };
        for at in kept {
            // past the most it keeps: this mark can't be gone back to
            let Some(picture) = marks.take(gpu, format) else {
                marks.drop_newest();
                break;
            };
            let r = tile_rect(at, SIDE).intersect(full);
            gpu.copy_texture_to_texture(
                encoder,
                copy_at(into, r.x0, r.y0),
                copy_at(&picture, 0, 0),
                extent(r.width(), r.height()),
            );
            // (the newest mark is still there: none was let go for room but older ones)
            if let Some(mark) = marks.marks.back_mut() {
                mark.saved.insert(at, picture);
            }
        }
        marks.written.extend(touched);
        marks.given.extend(instances.iter().zip(wet).zip(times).map(|((s, w), t)| (*s, *w, *t)));
    }

    /// The open stroke's second laid again from its start in `ops` (the work queued for a render)
    /// goes back to its newest mark before what changed instead, and lays only what follows: its
    /// stamps from scratch become [`Op::RewindSecond`] to the mark and the stamps after it.
    pub(super) fn second_rewound(&self, ops: Vec<Op>) -> Vec<Op> {
        let Some(marks) = self.second_marks.as_ref().filter(|m| m.on && !m.marks.is_empty()) else { return ops };
        if self.ends_whole {
            return ops;
        }
        let Some(from) = ops.iter().position(|op| matches!(op, Op::Stamps { second: true, clear: true, .. })) else {
            return ops;
        };
        // a stroke begun or ended before it, or a second cleared, isn't this one's
        let other = |op: &Op| matches!(op, Op::Commit(_) | Op::Cancel | Op::ClearSecond(_) | Op::DropSecond);
        if ops.iter().take(from).any(other) {
            return ops;
        }
        let paced = |op: &Op| -> Vec<PacedStamp> {
            match op {
                Op::Stamps { second: true, instances, wet, times, .. } => {
                    instances.iter().zip(wet).zip(times).map(|((s, w), t)| (*s, *w, *t)).collect()
                }
                _ => Vec::new(),
            }
        };
        // those queued before it are laid first (and kept for the newest mark as they are)
        let queued: Vec<PacedStamp> = ops.iter().take(from).flat_map(&paced).collect();
        let Some(op) = ops.get(from).filter(|op| matches!(op, Op::Stamps { carry: false, .. })) else { return ops };
        let stamps = paced(op);
        let unchanged = stamps.iter().zip(marks.given.iter().chain(&queued)).take_while(|(a, b)| same(a, b)).count();
        let Some(at) = marks.marks.iter().rev().map(|m| m.at).find(|&at| at > 0 && at <= unchanged) else {
            return ops;
        };
        let mut out = Vec::with_capacity(ops.len() + 1);
        for (k, op) in ops.into_iter().enumerate() {
            match op {
                Op::Stamps { second, instances, wet, picks, times, style, layer, pencil, carry, .. } if k == from => {
                    out.push(Op::RewindSecond(at));
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

    /// Goes back to the open stroke's second's mark before its `at`th stamp (see
    /// [`Canvas::second_rewound`]): the tiles written since as they were then (those nothing had
    /// written, cleared).
    pub(super) fn rewind_second(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, at: usize) {
        let full = self.full();
        let format = self.pipelines.stroke_format;
        let second = self.second_stroke.get(gpu).0.clone();
        let Some(marks) = self.second_marks.as_mut() else { return };
        let Some(k) = marks.marks.iter().position(|m| m.at == at) else {
            log::error!("a second's mark at {at} went before it was gone back to");
            return;
        };
        let since: Vec<Mark> = marks.marks.drain(k..).collect();
        let blank: BTreeSet<(u32, u32)> = since.iter().flat_map(|m| m.blank.iter().copied()).collect();
        // each tile as its first picture since the mark has it
        let mut first: BTreeMap<(u32, u32), &wgpu::Texture> = BTreeMap::new();
        for mark in &since {
            for (tile, saved) in &mark.saved {
                if !blank.contains(tile) {
                    first.entry(*tile).or_insert(saved);
                }
            }
        }
        let mut changed = IRect::EMPTY;
        if !blank.is_empty() {
            // (a texture of its own read before it is written is as cleared)
            let cleared = marks.blank.get_or_insert_with(|| texture(gpu, "second mark blank", SIDE, SIDE, format));
            for tile in &blank {
                let r = tile_rect(*tile, SIDE).intersect(full);
                gpu.copy_texture_to_texture(
                    encoder,
                    copy_at(cleared, 0, 0),
                    copy_at(&second, r.x0, r.y0),
                    extent(r.width(), r.height()),
                );
                changed = changed.union(r);
            }
        }
        for (tile, saved) in first {
            let r = tile_rect(tile, SIDE).intersect(full);
            gpu.copy_texture_to_texture(
                encoder,
                copy_at(saved, 0, 0),
                copy_at(&second, r.x0, r.y0),
                extent(r.width(), r.height()),
            );
            changed = changed.union(r);
        }
        let mut since = since.into_iter();
        let Some(mark) = since.next() else { return };
        for later in since {
            marks.free.extend(later.saved.into_values());
        }
        marks.written.retain(|t| !blank.contains(t));
        marks.given.truncate(at);
        // (the mark stays: what it saved is still as the second was there)
        marks.marks.push_back(mark);
        self.second_rewound_count += 1;
        self.second_rewound_stamps += at as u64;
        // shown again, and put together again with paint laid in place as the stroke ends
        self.dirty = self.dirty.union(changed);
        self.second_unsynthesized = self.second_unsynthesized.union(changed);
    }
}
