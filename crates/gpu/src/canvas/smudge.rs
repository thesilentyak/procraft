//! A smudge stroke's work (smudging and paint laid in place) done a window at a time: its compute
//! passes reach the layer through buffers that hold only the part of it they read and write,
//! copied in before them and back after, so that what is bound never grows with the canvas and a
//! stroke begins without copying its layer. What the stroke keeps of the layer (the layer as it
//! was, and wet paint's stroke as laid and how much each pixel has taken) is kept in tiles, each
//! made as the stroke first writes to it. A stroke laying paint into its layer's store (see
//! `store.rs`) works in the store's paint, and its tiles keep the layer and the height as they were
//! too.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use procraft_engine::geom::{IRect, Vec2};

use super::{
    Canvas, Kept, LAYER_FORMAT, SLOT, SMUDGE_UNIFORM_SIZE, SmudgeUniform, StampInstance, UniformData, copy_at, extent,
    smudge_params, texture,
};
use crate::Gpu;

/// The side of the tiles a smudge stroke of a big brush keeps (pixels). Each window copies each
/// tile it covers in and out, a copy at a time, and on OpenGL each copy costs the CPU a good deal:
/// with tiles half as wide, a big wet brush's frame took three times the copies (and a third more
/// time).
pub(super) const TILE: u32 = 512;
/// The side of a thin brush's tiles (see [`tile_for`]): in a big brush's tiles a thin stroke held
/// two to three times the memory (its tiles, and its marks' copies of them, see `rewind.rs`).
pub(super) const THIN_TILE: u32 = 256;
/// The widest brush (canvas pixels across) whose strokes keep thin tiles: a wider one's stroke, in
/// tiles half as wide, took longer to lay again as it ended (its tiles gone back over and put
/// together a copy at a time), a medium brush's half as long again on OpenGL.
const THIN_MOST: f32 = 32.0;

/// Where a copy from a texture may put its rows in a window's buffer: at a multiple of this many
/// bytes. Direct3D 12 without unrestricted copy pitches (on Windows 10, and with older drivers, as
/// many integrated GPUs have) places them only so (see `crate::placed`): a tile copied into a
/// window wider than it anywhere else wrote over its neighbours in the window with whatever
/// wgpu's buffer for the copy held (blocks of the layer gone, stale or doubled as the stroke went
/// on).
const PLACEMENT: u32 = crate::placed::PLACEMENT as u32;
const _: () = assert!(TILE.is_multiple_of(PLACEMENT / 4), "tiles start on a column copies can start at");
const _: () = assert!(THIN_TILE.is_multiple_of(PLACEMENT / 4), "tiles start on a column copies can start at");

/// The side of the tiles a stroke keeps, of a brush `diameter` canvas pixels across (none told: a
/// big brush's), on OpenGL (`gl`) a big brush's whatever the brush: each copy costs the CPU there.
pub(super) fn tile_for(diameter: Option<f32>, gl: bool) -> u32 {
    match diameter {
        Some(d) if d <= THIN_MOST && !gl => THIN_TILE,
        _ => TILE,
    }
}

/// The most bytes a window takes for work laid together in it: past it, the work goes in windows of
/// its own (only a piece that reaches further than this by itself takes one bigger).
const TOGETHER_MOST: u64 = 4 << 20;

/// All of a dispatch's cells or texels (see `part` in the shader).
pub(super) const WHOLE: u32 = 0xffff_0000;

/// The part of a dispatch's cells or texels from (x0, y0) up to (x1, y1), as the shader takes it.
fn part(x0: u32, y0: u32, x1: u32, y1: u32) -> u32 {
    x0.min(255) | (y0.min(255) << 8) | (x1.min(255) << 16) | (y1.min(255) << 24)
}

/// What a smudge stroke keeps besides the layer as it was: the colour its stamps mixed (wet paint
/// laid apart, at full strength: with the layer as it was and how much of the paint shows, the
/// stroke as laid), how much each pixel has taken (the modulated mode) and how much of the paint
/// shows (laid apart, the amount's second half).
#[derive(Clone, Copy)]
pub(super) struct Keeps {
    pub grained: bool,
    pub amount: bool,
    pub shown: bool,
}

impl Keeps {
    /// The halves the amount buffer has (each as big as the work buffer), at least one.
    fn halves(self) -> u64 {
        if self.shown { 2 } else { 1 }
    }
}

/// What a smudge stroke keeps of a tile of its layer once it has written there: the layer as it
/// was, and (as it keeps them) the colour the stroke mixed, how much each pixel has taken and how much of
/// the paint shows (these two f32s a pixel, as their bytes); and the textures it changes besides
/// the one it works in as they were (see [`Canvas::smudge_also`]).
pub(super) struct Tile {
    pub(super) before: wgpu::Texture,
    pub(super) grained: Option<wgpu::Texture>,
    pub(super) amount: Option<wgpu::Texture>,
    pub(super) shown: Option<wgpu::Texture>,
    pub(super) more: Vec<wgpu::Texture>,
}

/// What of a tile a kind of what the stroke keeps is in (none where the stroke doesn't keep it).
type KeptOf = fn(&Tile) -> Option<&wgpu::Texture>;

/// The buffers a smudge stroke's passes work in (see the module): the layer as the stroke has left
/// it, the colour the stroke mixed, the amounts (both halves), and the layer as it was (which wet
/// paint laid apart reads the stroke as laid from).
pub(super) struct Window {
    work: wgpu::Buffer,
    grained: Option<wgpu::Buffer>,
    amount: Option<wgpu::Buffer>,
    before: wgpu::Texture,
    before_view: wgpu::TextureView,
    /// Bytes the work buffer holds (each of the amount's halves as many).
    bytes: u64,
}

/// What a dispatch of a smudge stroke runs.
#[derive(Clone, Copy)]
pub(super) enum Pipe {
    Carry,
    Dull,
    BlurDown,
    BlurAcross,
    Deposit,
    /// A run of a pass's stamps laid at once (see `cs_deposit_run`).
    DepositRun,
    Synthesize,
    WetPlots,
}

/// A piece of a smudge stroke's work, done in one window: its dispatches in order (each with which
/// of its uniforms it reads, the window's fields filled in where it is laid, and its workgroups),
/// the pass of wet paint they read (an offset in its uniforms), the pixels they read and write,
/// whether they change the layer or what the stroke keeps, and which of a symmetry's copies of the
/// stroke it lays (whose carry it works in, see `Carry`).
pub(super) struct Piece {
    pub uniforms: Vec<SmudgeUniform>,
    pub dispatches: Vec<(Pipe, usize, [u32; 2])>,
    pub listed: u32,
    pub reach: IRect,
    pub writes: IRect,
    pub work: bool,
    pub kept: bool,
    pub carry: usize,
}

impl Piece {
    /// What of its copy's carry the piece reads from that copy's stamps before (its first dispatch
    /// to work in the carry: those after read what it left), and what of it the copy's next stamp
    /// reads once it is laid (none: as before); none if it doesn't work in a carry (paint laid apart
    /// put together).
    pub(super) fn carry_use(&self) -> Option<(Kept, Option<Kept>)> {
        let mut used: Option<(Kept, Option<Kept>)> = None;
        for &(pipe, which, _) in &self.dispatches {
            let Some(u) = self.uniforms.get(which) else { continue };
            let from_before = u.kept >= 0;
            let (needs, leaves) = match pipe {
                Pipe::Carry => {
                    (if from_before { Kept::Cells(u.kept) } else { Kept::Nothing }, Some(Kept::Cells(u.half)))
                }
                Pipe::WetPlots => {
                    let needs = if from_before {
                        Kept::Plot { which: u.index.min(1), texels: u.carried * u.carried }
                    } else {
                        Kept::Nothing
                    };
                    (needs, Some(Kept::Plot { which: 1 - u.index.min(1), texels: u.plot * u.plot }))
                }
                Pipe::Dull | Pipe::BlurDown | Pipe::BlurAcross | Pipe::Deposit | Pipe::DepositRun => {
                    (Kept::Nothing, None)
                }
                Pipe::Synthesize => continue,
            };
            used = Some(match used {
                Some((first, last)) => (first, leaves.or(last)),
                None => (needs, leaves),
            });
        }
        used
    }
}

/// Bytes a row of a window `width` pixels wide takes (rows start at multiples of [`PLACEMENT`]).
pub(super) fn row_bytes(width: u32) -> u32 {
    (width.max(1) * 4).div_ceil(PLACEMENT) * PLACEMENT
}

/// The window laid over `reach`: from a column a copy can start at (see [`PLACEMENT`]), so that each
/// tile's part of it (tiles start on such columns) starts at a multiple of it, as its rows do.
pub(super) fn window_reach(reach: IRect) -> IRect {
    if reach.is_empty() {
        return reach;
    }
    let step = (PLACEMENT / 4) as i32;
    IRect { x0: reach.x0 - reach.x0.rem_euclid(step), ..reach }
}

/// Bytes a window over `reach` takes as the stroke's work is cut into windows: its pixels, rows
/// 256 bytes apart (one laid in buffers takes a little more, see [`window_reach`]).
fn window_bytes(reach: IRect) -> u64 {
    u64::from((reach.width().max(1) * 4).div_ceil(256) * 256) * u64::from(reach.height().max(1))
}

/// Bytes a window laid in buffers over `reach` takes, rows [`row_bytes`] apart.
fn laid_bytes(reach: IRect) -> u64 {
    u64::from(row_bytes(reach.width())) * u64::from(reach.height().max(1))
}

/// Where `r` (within the window over `reach`) starts in the window's buffers, bytes.
fn in_window(reach: IRect, r: IRect) -> u64 {
    u64::from(row_bytes(reach.width())) * u64::from(r.y0.abs_diff(reach.y0)) + 4 * u64::from(r.x0.abs_diff(reach.x0))
}

/// The pixels the shader's `layer_at` reads for canvas points from `lo` to `hi` (bilinear, with a
/// pixel to spare each way for rounding), as it holds them to the canvas `full`.
fn read_reach(lo: Vec2, hi: Vec2, full: IRect) -> IRect {
    if !lo.is_finite() || !hi.is_finite() || full.is_empty() {
        return full;
    }
    let held = |v: f32, least: i32, most: i32| (v as i32).clamp(least, most);
    let (last_x, last_y) = (full.x1 - 1, full.y1 - 1);
    IRect::new(
        held((lo.x - 0.5).floor() - 1.0, full.x0, last_x),
        held((lo.y - 0.5).floor() - 1.0, full.y0, last_y),
        held((hi.x - 0.5).floor() + 2.0, full.x0, last_x) + 1,
        held((hi.y - 0.5).floor() + 2.0, full.y0, last_y) + 1,
    )
}

/// The pixels `cs_carry` reads for the cells `[i0, i1)` across and `[j0, j1)` down of a stamp's
/// window (`half` cells each way of its middle, at `anchor`, `scale` canvas pixels a cell).
fn carry_reach(u: &SmudgeUniform, [i0, j0, i1, j1]: [u32; 4], full: IRect) -> IRect {
    let at = |i: u32| (i as f32 - u.half as f32 + 0.5) * u.scale;
    let lo = Vec2::new(u.pick[0] + at(i0), u.pick[1] + at(j0));
    let hi = Vec2::new(u.pick[0] + at(i1.max(i0 + 1) - 1), u.pick[1] + at(j1.max(j0 + 1) - 1));
    read_reach(lo, hi, full)
}

/// The stamp point a plot's texel `i` stands for, in a plot `n` texels a side (as the shader's
/// `plot_t`).
fn plot_t(i: u32, n: u32) -> f32 {
    let n = n.max(1) as f32;
    ((i as f32 + 0.5) / n * 2.0 - 1.0) * (1.0 + 1.0 / n)
}

/// The pixels `cs_wet_plots` reads under a pass's stamp `s` for the texels `[i0, i1)` across and
/// `[j0, j1)` down of a plot `n` a side, or all of them (`None`: then by the stamp's reach
/// whatever its turn, the GPU's turn being near enough).
pub(super) fn plot_reach(s: &StampInstance, n: u32, texels: Option<[u32; 4]>, full: IRect) -> IRect {
    let ([x, y], radius, angle, roundness) = (s.center, s.radius.max(0.5), s.angle, s.roundness);
    let center = Vec2::new(x, y);
    // a little more than the turn's error at the size (the GPU's sine is coarse)
    let spare = 1.0 + radius * roundness.max(1.0) / 256.0;
    let Some([i0, j0, i1, j1]) = texels else {
        let r = radius * roundness.max(1.0) * std::f32::consts::SQRT_2 + spare;
        return read_reach(center - Vec2::new(r, r), center + Vec2::new(r, r), full);
    };
    let (sin, cos) = angle.sin_cos();
    let mut lo = Vec2::new(f32::INFINITY, f32::INFINITY);
    let mut hi = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for tx in [plot_t(i0, n), plot_t(i1.max(i0 + 1) - 1, n)] {
        for ty in [plot_t(j0, n), plot_t(j1.max(j0 + 1) - 1, n)] {
            let (lx, ly) = (tx * radius, ty * radius * roundness);
            let p = Vec2::new(x + lx * cos - ly * sin, y + lx * sin + ly * cos);
            lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
    }
    read_reach(lo - Vec2::new(spare, spare), hi + Vec2::new(spare, spare), full)
}

/// The end of the longest run `[from, end)` within `[from, hi)` that `fits` takes (one at least),
/// `fits` taking any run within one it takes.
pub(super) fn longest(from: u32, hi: u32, fits: impl Fn(u32, u32) -> bool) -> u32 {
    let (mut good, mut bad) = (from + 1, hi.max(from + 1) + 1);
    while good + 1 < bad {
        let mid = good + (bad - good) / 2;
        if fits(from, mid) {
            good = mid;
        } else {
            bad = mid;
        }
    }
    good
}

/// `[lo, hi)` cut into runs from its start, each the longest `fits` takes.
fn runs(lo: u32, hi: u32, fits: impl Fn(u32, u32) -> bool) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut from = lo;
    while from < hi {
        let end = longest(from, hi, &fits);
        out.push((from, end));
        from = end;
    }
    out
}

/// The tiles `side` pixels across that `rect` (canvas pixels, none negative) touches.
pub(super) fn tiles_of(rect: IRect, side: u32) -> impl Iterator<Item = (u32, u32)> {
    let side = side.max(1);
    let (x0, y0) = (rect.x0.max(0).unsigned_abs() / side, rect.y0.max(0).unsigned_abs() / side);
    let (x1, y1) = (rect.x1.max(1).unsigned_abs().div_ceil(side), rect.y1.max(1).unsigned_abs().div_ceil(side));
    let rows = if rect.is_empty() { 0..0 } else { y0..y1 };
    rows.flat_map(move |y| (x0..x1).map(move |x| (x, y)))
}

/// Where the tile at `(x, y)` of tiles `side` pixels across lies (canvas pixels).
pub(super) fn tile_rect((x, y): (u32, u32), side: u32) -> IRect {
    let at = |n: u32| i32::try_from(n.saturating_mul(side)).unwrap_or(i32::MAX);
    IRect::new(at(x), at(y), at(x + 1), at(y + 1))
}

/// A buffer for a window.
fn window_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A copy from or into a window buffer of rows `row` bytes apart, starting `offset` bytes in.
fn in_buffer(buffer: &wgpu::Buffer, offset: u64, row: u32, rows: u32) -> wgpu::TexelCopyBufferInfo<'_> {
    wgpu::TexelCopyBufferInfo {
        buffer,
        layout: wgpu::TexelCopyBufferLayout { offset, bytes_per_row: Some(row), rows_per_image: Some(rows.max(1)) },
    }
}

impl Canvas {
    /// The most a smudge stroke's window may take: what a buffer may be and a binding may hold.
    pub(super) fn smudge_room_for(device: &wgpu::Device) -> u64 {
        let limits = device.limits();
        limits.max_storage_buffer_binding_size.min(limits.max_buffer_size)
    }

    /// Whether a window over `reach` fits a binding, as it would for the open smudge stroke (its
    /// amount buffer can take two halves), within `most` bytes; and laid in buffers, rows
    /// [`row_bytes`] apart, within what a binding holds. (Its rows take up to twice its pixels', 256
    /// bytes apart: work cut close to what a binding holds by those alone wasn't laid.)
    fn window_fits(&self, reach: IRect, most: u64) -> bool {
        let halves = self.smudge.as_ref().map_or(1, |s| s.keeps.halves());
        window_bytes(reach).saturating_mul(halves) <= most
            && laid_bytes(reach).saturating_mul(halves) <= self.smudge_room
    }

    /// Whether a piece reaching `reach` can be laid in a window of its own.
    pub(super) fn fits_window(&self, reach: IRect) -> bool {
        self.window_fits(reach, self.smudge_most)
    }

    /// A smudge stamp's work (its uniform `u`, laying down over `bbox`): what the brush carries
    /// picked up, blurred if it `blurs`, and laid down. A stamp reaching further than a window holds
    /// picks up a band of rows of what it carries at a time, and lays down a band of rows at a time.
    pub(super) fn smudge_pieces(&self, u: SmudgeUniform, bbox: IRect, blurs: bool) -> Vec<Piece> {
        let full = self.full();
        let n = 2 * u.half.unsigned_abs() + 1;
        let cells = n.div_ceil(8);
        let blur = [(Pipe::BlurDown, 0, [cells, cells]), (Pipe::BlurAcross, 0, [cells, cells])];
        let blur = if blurs { &blur[..] } else { &[] };
        let piece = |uniform, dispatches: Vec<_>, reach, writes: IRect| Piece {
            uniforms: vec![uniform],
            dispatches,
            listed: 0,
            reach,
            writes,
            work: !writes.is_empty(),
            kept: false,
            carry: 0,
        };
        let reach = carry_reach(&u, [0, 0, n, n], full).union(bbox);
        // what it carries made one colour, a workgroup's work
        let dull = if u.dull > 0.0 { &[(Pipe::Dull, 0, [1, 1])][..] } else { &[] };
        if self.fits_window(reach) {
            let mut dispatches = vec![(Pipe::Carry, 0, [cells, cells])];
            dispatches.extend_from_slice(dull);
            dispatches.extend_from_slice(blur);
            dispatches.push((Pipe::Deposit, 0, [bbox.width().div_ceil(8), bbox.height().div_ceil(8)]));
            return vec![piece(u, dispatches, reach, bbox)];
        }
        let band = |j0, j1| carry_reach(&u, [0, j0, n, j1], full);
        let mut pieces: Vec<Piece> = runs(0, n, |j0, j1| self.fits_window(band(j0, j1)))
            .into_iter()
            .map(|(j0, j1)| {
                let carry = vec![(Pipe::Carry, 0, [cells, (j1 - j0).div_ceil(8)])];
                piece(SmudgeUniform { part: part(0, j0, 255, j1), ..u }, carry, band(j0, j1), IRect::EMPTY)
            })
            .collect();
        if !dull.is_empty() {
            pieces.push(piece(u, dull.to_vec(), IRect::EMPTY, IRect::EMPTY));
        }
        if blurs {
            pieces.push(piece(u, blur.to_vec(), IRect::EMPTY, IRect::EMPTY));
        }
        pieces.extend(self.bands(Pipe::Deposit, u, bbox, (true, false), 0));
        pieces
    }

    /// Each smudge stamp's work (its uniform, where it lays down, whether it blurs, and which of a
    /// symmetry's copies of the stroke it is on) as [`Canvas::smudge_pieces`] cuts it.
    pub(super) fn smudged_pieces(&self, laid: Vec<(SmudgeUniform, IRect, bool, usize)>) -> Vec<Piece> {
        laid.into_iter()
            .flat_map(|(uniform, bbox, blurs, copy)| {
                self.smudge_pieces(uniform, bbox, blurs).into_iter().map(move |piece| Piece { carry: copy, ..piece })
            })
            .collect()
    }

    /// A dispatch over `rect` (its uniform `u`, reading only the pixels it writes), in bands of rows
    /// that each fit a window: what it changes, the layer (`work`) or what the stroke keeps
    /// (`kept`), and the pass of wet paint it reads.
    pub(super) fn bands(
        &self,
        pipe: Pipe,
        u: SmudgeUniform,
        rect: IRect,
        (work, kept): (bool, bool),
        listed: u32,
    ) -> Vec<Piece> {
        let band = |a: u32, b: u32| {
            let at = |n: u32| rect.y0.saturating_add(i32::try_from(n).unwrap_or(i32::MAX));
            IRect::new(rect.x0, at(a), rect.x1, at(b))
        };
        runs(0, rect.height(), |a, b| self.fits_window(band(a, b)))
            .into_iter()
            .map(|(a, b)| {
                let r = band(a, b);
                Piece {
                    uniforms: vec![SmudgeUniform { bbox: [r.x0, r.y0, r.x1, r.y1], ..u }],
                    dispatches: vec![(pipe, 0, [r.width().div_ceil(8), r.height().div_ceil(8)])],
                    listed,
                    reach: r,
                    writes: r,
                    work,
                    kept,
                    carry: 0,
                }
            })
            .collect()
    }

    /// A pass of wet paint of one stamp `s` reaching further than a window holds (`pass` as one
    /// piece: its plots, then the stamp): the plot made a block of texels at a time, each block
    /// as big as fits, then the stamp laid a band of rows at a time.
    pub(super) fn wet_stamp_pieces(&self, s: &StampInstance, pass: Piece, changes: (bool, bool)) -> Vec<Piece> {
        let full = self.full();
        let Some(&plots) = pass.uniforms.first() else { return Vec::new() };
        let n = plots.plot.max(1);
        let blocks = |side: u32| {
            let count = n.div_ceil(side);
            (0..count).flat_map(move |y| {
                (0..count).map(move |x| [x * side, y * side, ((x + 1) * side).min(n), ((y + 1) * side).min(n)])
            })
        };
        let fit = |side| blocks(side).all(|b| self.fits_window(plot_reach(s, n, Some(b), full)));
        let mut side = n;
        while side > 1 && !fit(side) {
            side = side.div_ceil(2);
        }
        // a stamp no block of fits (none could be that big): as it is
        if !fit(side) {
            return vec![pass];
        }
        let mut pieces: Vec<Piece> = blocks(side)
            .map(|b| {
                let [x0, y0, x1, y1] = b;
                Piece {
                    uniforms: vec![SmudgeUniform { part: part(x0, y0, x1, y1), ..plots }],
                    dispatches: vec![(Pipe::WetPlots, 0, [(x1 - x0).div_ceil(8), (y1 - y0).div_ceil(8)])],
                    listed: pass.listed,
                    reach: plot_reach(s, n, Some(b), full),
                    writes: IRect::EMPTY,
                    work: false,
                    kept: false,
                    carry: pass.carry,
                }
            })
            .collect();
        if let Some(&stamp) = pass.uniforms.get(1) {
            let [x0, y0, x1, y1] = stamp.bbox;
            let bands = self.bands(Pipe::Deposit, stamp, IRect::new(x0, y0, x1, y1), changes, pass.listed);
            pieces.extend(bands.into_iter().map(|band| Piece { carry: pass.carry, ..band }));
        }
        pieces
    }

    /// Lays `pieces` in order, as many together in one window as fit: through the brush images
    /// `brush` and a dual brush's `second`.
    pub(super) fn lay_pieces(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        params: &mut UniformData,
        pieces: &[Piece],
        (brush, second): (&wgpu::BindGroup, Option<&wgpu::TextureView>),
    ) {
        self.ready_asides(gpu, pieces);
        let together = self.smudge_most.min(TOGETHER_MOST);
        let area = |r: IRect| u64::from(r.width()) * u64::from(r.height());
        let mut from = 0;
        while let Some(first) = pieces.get(from) {
            let mut reach = first.reach;
            let mut to = from + 1;
            while let Some(next) = pieces.get(to) {
                let both = reach.union(next.reach);
                // together while small, or (a big brush's pass, and its paint put together after)
                // where one window over both copies no more than one over each would: each window
                // copies the layer and every tile it keeps in and out, a copy at a time
                let overlapping = area(both) <= area(reach) + area(next.reach) && self.fits_window(both);
                if !self.window_fits(both, together) && !overlapping {
                    break;
                }
                reach = both;
                to += 1;
            }
            let Some(lot) = pieces.get(from..to) else { return };
            if !self.lay_window(gpu, encoder, params, lot, reach, (brush, second)) {
                return;
            }
            from = to;
        }
    }

    /// Lays `pieces` in one window over `reach`; false when it couldn't be made (the stroke then
    /// lays nothing more).
    fn lay_window(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        params: &mut UniformData,
        pieces: &[Piece],
        reach: IRect,
        (brush, second): (&wgpu::BindGroup, Option<&wgpu::TextureView>),
    ) -> bool {
        // without compute passes, in fragment passes (see `fragments`)
        if self.pipelines.compute.is_none() {
            return self.lay_window_in_fragments(gpu, encoder, params, pieces, reach, (brush, second));
        }
        let reach = if reach.is_empty() { IRect::new(0, 0, 1, 1) } else { reach }.intersect(self.full());
        self.windows_laid += 1;
        // laid out so that copies into it start where any API places them, where a binding holds
        // that (else as it is, its tiles' parts copied in a row at a time: only a window close to
        // the most a binding holds is that big)
        let room = self.smudge_room;
        let halves = self.smudge.as_ref().map_or(1, |s| s.keeps.halves());
        let fits = |r: IRect| laid_bytes(r).saturating_mul(halves) <= room;
        let reach = Some(window_reach(reach)).filter(|r| fits(*r)).unwrap_or(reach);
        // past what a binding holds (a piece the stroke couldn't cut smaller): not laid
        if reach.is_empty() || !fits(reach) {
            log::error!("smudging: {} by {} pixels is past what a binding holds", reach.width(), reach.height());
            return true;
        }
        let each: Vec<IRect> = pieces.iter().map(|p| p.writes.intersect(reach)).collect();
        let writes = each.iter().fold(IRect::EMPTY, |r, w| r.union(*w));
        let (work, kept) = pieces.iter().fold((false, false), |(w, k), p| (w || p.work, k || p.kept));
        let count: usize = pieces.iter().map(|p| p.uniforms.len()).sum();
        self.smudge_uniform_room(gpu, params, count);
        // the layer as it was is read by a glaze, paint laid apart and paint put together, else
        // not filled in
        let before = pieces.iter().any(|p| {
            p.uniforms.iter().any(|u| u.source_before == 1 || u.laid_apart == 2)
                || p.dispatches.iter().any(|d| matches!(d.0, Pipe::Synthesize))
        });
        if !self.window_in(gpu, encoder, (reach, before), &each) {
            return false;
        }
        let from_image = pieces.iter().any(|p| p.uniforms.iter().any(|u| u.source_image == 1));
        let Some(smudge) = self.smudge.as_ref() else { return false };
        let Some(window) = smudge.window.as_ref() else { return false };
        // (never begun without them: see `Canvas::smudges`)
        let Some(compute) = self.pipelines.compute.as_ref() else { return false };
        let Some(carry) = smudge.carry.as_ref() else { return false };
        // where the carry goes from one of a symmetry's copies to another's
        let (overs, resident) = super::carry_hand_overs(smudge.resident, pieces);
        let mut overs = overs.into_iter().peekable();
        let uniform = |binding, buffer, size| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: NonZeroU64::new(size),
            }),
        };
        let spare = &self.smudge_spare;
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("smudge"),
            layout: &compute.smudge_layout,
            entries: &[
                uniform(0, &self.smudge_params, SMUDGE_UNIFORM_SIZE),
                wgpu::BindGroupEntry { binding: 1, resource: window.work.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: carry.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: window.grained.as_ref().unwrap_or(&spare[0]).as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: window.amount.as_ref().unwrap_or(&spare[1]).as_entire_binding(),
                },
                uniform(5, &self.wet_params, super::WET_PASS_SIZE),
                // a dual brush's second (else anything: not read)
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(second.unwrap_or(&window.before_view)),
                },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&window.before_view) },
                // the layer as a smudge's last pass left it (else anything: not read)
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(
                        smudge.passed.as_ref().map_or(&window.before_view, |p| &p.1),
                    ),
                },
                // the image as last shown, for a stroke that picks up from it (else anything: not
                // read)
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(if from_image {
                        &self.composite_view
                    } else {
                        &window.before_view
                    }),
                },
                // the paper's tile, which wet paint takes
                wgpu::BindGroupEntry { binding: 11, resource: wgpu::BindingResource::TextureView(&self.paper_view) },
            ],
        });
        let stride = row_bytes(reach.width()) / 4;
        // the paper as it is now, which each wet stamp takes as it lays
        let paper = self.paper_look();
        let begin = |encoder: &mut wgpu::CommandEncoder| {
            let mut pass = encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("smudge"), timestamp_writes: None })
                .forget_lifetime();
            pass.set_bind_group(1, brush, &[]);
            pass
        };
        let mut pass = begin(encoder);
        for (k, piece) in pieces.iter().enumerate() {
            if let Some((_, out, into)) = overs.next_if(|o| o.0 == k) {
                drop(pass);
                hand_over(encoder, carry, &smudge.carries, out, into);
                pass = begin(encoder);
            }
            let offsets: Vec<u32> = piece
                .uniforms
                .iter()
                .map(|u| {
                    params.push(&SmudgeUniform {
                        origin: [reach.x0, reach.y0],
                        stride,
                        rows: reach.height(),
                        paper,
                        ..*u
                    })
                })
                .collect();
            for &(pipe, which, [x, y]) in &piece.dispatches {
                let Some(&offset) = offsets.get(which) else { continue };
                pass.set_bind_group(0, &bind, &[offset, piece.listed]);
                pass.set_pipeline(match pipe {
                    Pipe::Carry => &compute.smudge_carry,
                    Pipe::Dull => &compute.smudge_dull,
                    Pipe::BlurDown => &compute.smudge_blur_down,
                    Pipe::BlurAcross => &compute.smudge_blur_across,
                    Pipe::Deposit => &compute.smudge_deposit,
                    Pipe::DepositRun => &compute.smudge_deposit_run,
                    Pipe::Synthesize => &compute.smudge_synthesize,
                    Pipe::WetPlots => &compute.smudge_wet_plots,
                });
                pass.dispatch_workgroups(x, y, 1);
                self.dispatched.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        drop(pass);
        if let Some(smudge) = self.smudge.as_mut() {
            smudge.resident = resident;
        }
        self.window_out(gpu, encoder, reach, writes, (work, kept));
        true
    }

    /// Makes room to put aside the carries of the copies of the stroke that `pieces` work in (and of
    /// the one whose carry the stroke's holds), where more than one copy is among them. A copy whose
    /// carry can't be put aside (past the GPU's memory) works in the carry as the others leave it.
    fn ready_asides(&mut self, gpu: &Gpu, pieces: &[Piece]) {
        let device = &gpu.device;
        let in_fragments = self.pipelines.compute.is_none();
        let Some(smudge) = self.smudge.as_mut() else { return };
        let mut copies: Vec<usize> = pieces.iter().filter(|p| p.carry_use().is_some()).map(|p| p.carry).collect();
        copies.extend(smudge.resident.map(|r| r.0));
        copies.sort_unstable();
        copies.dedup();
        if copies.len() < 2 {
            return;
        }
        let wet = smudge.wet;
        for copy in copies {
            let Some(carry) = smudge.carries.get_mut(copy) else { continue };
            if carry.aside.is_some() || carry.unkept {
                continue;
            }
            let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            let invalid = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let aside = if in_fragments {
                let side = if wet { super::WET_PLOT_MOST } else { super::SMUDGE_GRID };
                super::Aside::Texture(super::fragments::Tex::new(
                    gpu,
                    "smudge carry aside",
                    side,
                    side,
                    super::fragments::CARRY_FORMAT,
                ))
            } else {
                super::Aside::Buffer(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("smudge carry aside"),
                    size: aside_bytes(wet),
                    usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }))
            };
            if let Some(why) = pollster::block_on(invalid.pop()).or(pollster::block_on(memory.pop())) {
                log::error!("smudging: a symmetry's copy's carry couldn't be put aside: {why}");
                carry.unkept = true;
                continue;
            }
            carry.aside = Some(aside);
        }
    }

    /// Room for `count` more smudge uniforms: when the buffer they go in is full, what it holds is
    /// written to it now (the passes laid with it read that) and the rest start a bigger one.
    pub(super) fn smudge_uniform_room(&mut self, gpu: &Gpu, params: &mut UniformData, count: usize) {
        let slots = SMUDGE_UNIFORM_SIZE.div_ceil(SLOT) * count as u64;
        if params.data.len() as u64 / SLOT + slots <= self.smudge_capacity {
            return;
        }
        if !params.data.is_empty() {
            gpu.queue.write_buffer(&self.smudge_params, 0, &params.data);
        }
        *params = UniformData::default();
        self.smudge_capacity = self.smudge_capacity.max(slots).next_power_of_two();
        self.smudge_params = smudge_params(&gpu.device, self.smudge_capacity);
    }

    /// Readies the open smudge stroke's window over `reach`, its passes writing within `writes`: the
    /// buffers made big enough, the tiles they write to made (the layer as it was kept first), and
    /// all of it copied in (the layer as it was if they read it, `before`). False if what it takes
    /// couldn't be made: refused if the stroke has laid nothing yet, else it lays nothing more (and
    /// either way it is told).
    fn window_in(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        (reach, before): (IRect, bool),
        writes: &[IRect],
    ) -> bool {
        let device = &gpu.device;
        // what the passes write, as it is before them, for the stroke's mark to go back to
        self.keep_for_mark(gpu, encoder, writes);
        let full = self.full();
        let also = self.smudge_also();
        let room = self.smudge_room;
        let Some(smudge) = self.smudge.as_mut() else { return false };
        let Some(layer) = self.layers.get(&smudge.into) else { return false };
        let also: Vec<&wgpu::Texture> = also.iter().filter_map(|id| self.layers.get(id)).map(|l| &l.texture).collect();
        if smudge.stalled {
            return false;
        }
        let (keeps, side) = (smudge.keeps, smudge.tile);
        let (width, height) = (reach.width(), reach.height());
        let row = row_bytes(width);
        let bytes = u64::from(row) * u64::from(height);
        let small = smudge
            .window
            .as_ref()
            .is_none_or(|w| w.bytes < bytes || w.before.width() < width || w.before.height() < height);
        let mut missing: Vec<(u32, u32)> =
            writes.iter().flat_map(|w| tiles_of(*w, side)).filter(|t| !smudge.tiles.contains_key(t)).collect();
        missing.sort_unstable();
        missing.dedup();
        if small || !missing.is_empty() {
            let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            let invalid = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let window = small.then(|| {
                // grown in steps, so a stroke that reaches further doesn't make them every frame
                let most = room / keeps.halves();
                let bytes = bytes.next_multiple_of(1 << 20).min(most).max(bytes);
                let side = |n: u32, of: u32| n.next_multiple_of(256).min(of).max(n);
                let before =
                    texture(gpu, "smudge window", side(width, self.width), side(height, self.height), LAYER_FORMAT);
                Window {
                    work: window_buffer(device, "smudge layer", bytes),
                    grained: keeps.grained.then(|| window_buffer(device, "smudge grained", bytes)),
                    amount: (keeps.amount || keeps.shown)
                        .then(|| window_buffer(device, "smudge amount", bytes * keeps.halves())),
                    before_view: before.create_view(&wgpu::TextureViewDescriptor::default()),
                    before,
                    bytes,
                }
            });
            let tile = |label| texture(gpu, label, side, side, LAYER_FORMAT);
            let made: Vec<((u32, u32), Tile)> = missing
                .iter()
                .map(|&at| {
                    let tile = Tile {
                        before: tile("smudge before"),
                        grained: keeps.grained.then(|| tile("smudge grained")),
                        amount: keeps.amount.then(|| tile("smudge amount")),
                        shown: keeps.shown.then(|| tile("smudge shown")),
                        more: also.iter().map(|_| tile("smudge kept")).collect(),
                    };
                    (at, tile)
                })
                .collect();
            let failed = pollster::block_on(invalid.pop()).or(pollster::block_on(memory.pop()));
            if let Some(e) = failed {
                log::error!("smudging: {e}");
                self.smudge_untold = true;
                if smudge.laid {
                    smudge.stalled = true;
                } else {
                    self.smudge = None;
                    self.smudge_refused = true;
                }
                return false;
            }
            if let Some(window) = window {
                smudge.window = Some(window);
            }
            for (at, tile) in made {
                // the layer there as it was: the stroke hasn't written to it yet
                let r = tile_rect(at, side).intersect(full);
                for into in std::iter::once(&tile.before).chain(&tile.grained) {
                    gpu.copy_texture_to_texture(
                        encoder,
                        copy_at(&layer.texture, r.x0, r.y0),
                        copy_at(into, 0, 0),
                        extent(r.width(), r.height()),
                    );
                }
                for (from, into) in also.iter().zip(&tile.more) {
                    gpu.copy_texture_to_texture(
                        encoder,
                        copy_at(from, r.x0, r.y0),
                        copy_at(into, 0, 0),
                        extent(r.width(), r.height()),
                    );
                }
                smudge.tiles.insert(at, tile);
            }
        }
        smudge.laid = true;
        let Some(window) = smudge.window.as_ref() else { return false };
        let size = extent(width, height);
        // the layer as the stroke has left it, and as it was where the stroke hasn't been
        let from_layer = copy_at(&layer.texture, reach.x0, reach.y0);
        gpu.copy_texture_to_buffer(encoder, from_layer, in_buffer(&window.work, 0, row, height), size);
        if let Some(amount) = &window.amount {
            encoder.clear_buffer(amount, 0, Some(bytes * keeps.halves()));
        }
        // the stroke's tiles under the window: each one's part of it (in the window), and where
        // that starts in the tile
        let parts: Vec<(&Tile, IRect, (i32, i32))> = tiles_of(reach, side)
            .filter_map(|at| {
                let tile = smudge.tiles.get(&at)?;
                let t = tile_rect(at, side);
                let r = t.intersect(reach);
                (!r.is_empty()).then_some((tile, r, (r.x0 - t.x0, r.y0 - t.y0)))
            })
            .collect();
        // what the stroke keeps where it has been, a kind at a time: put together in the window's
        // texture first (the layer, or nothing, with each tile's part over it), then copied into
        // its buffer whole rows at a time. A copy from a texture into the middle of a buffer's rows
        // can write over the rest of them, as Direct3D 12 without unrestricted copy pitches does
        // (see `crate::placed`), and as a driver that writes a copy's rows whole would: tiles'
        // parts copied straight in wrote over their neighbours' (on some phones, blocks of a
        // tile's size came out in solid colour, #357). The texture ends up holding the layer as it
        // was, as the passes read it.
        let at_window = |r: IRect| copy_at(&window.before, r.x0 - reach.x0, r.y0 - reach.y0);
        let area = |r: IRect| u64::from(r.width()) * u64::from(r.height());
        let whole = u64::from(width) * u64::from(height);
        let kinds: [(Option<&wgpu::Buffer>, u64, KeptOf, bool); 3] = [
            (window.grained.as_ref(), 0, |t| t.grained.as_ref(), true),
            (window.amount.as_ref(), 0, |t| t.amount.as_ref(), false),
            (window.amount.as_ref(), bytes, |t| t.shown.as_ref(), false),
        ];
        for (buffer, start, kept, from_the_layer) in kinds {
            let Some(buffer) = buffer else { continue };
            let theirs: Vec<(&wgpu::Texture, IRect, (i32, i32))> =
                parts.iter().filter_map(|&(tile, r, at)| Some((kept(tile)?, r, at))).collect();
            if theirs.is_empty() {
                // (the amounts are cleared already)
                if from_the_layer {
                    gpu.copy_texture_to_buffer(encoder, from_layer, in_buffer(buffer, start, row, height), size);
                }
                continue;
            }
            // (under what the tiles' parts don't cover)
            if theirs.iter().map(|t| area(t.1)).sum::<u64>() < whole {
                if from_the_layer {
                    gpu.copy_texture_to_texture(encoder, from_layer, copy_at(&window.before, 0, 0), size);
                } else {
                    drop(super::render_pass(
                        encoder,
                        "smudge window cleared",
                        &window.before_view,
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    ));
                }
            }
            for (kept, r, (x, y)) in theirs {
                gpu.copy_texture_to_texture(encoder, copy_at(kept, x, y), at_window(r), extent(r.width(), r.height()));
            }
            gpu.copy_texture_to_buffer(
                encoder,
                copy_at(&window.before, 0, 0),
                in_buffer(buffer, start, row, height),
                size,
            );
        }
        if before {
            if parts.iter().map(|p| area(p.1)).sum::<u64>() < whole {
                gpu.copy_texture_to_texture(encoder, from_layer, copy_at(&window.before, 0, 0), size);
            }
            for &(tile, r, (x, y)) in &parts {
                gpu.copy_texture_to_texture(
                    encoder,
                    copy_at(&tile.before, x, y),
                    at_window(r),
                    extent(r.width(), r.height()),
                );
            }
        }
        true
    }

    /// Puts what the passes in the window over `reach` changed within `changed` back: into the
    /// layer if they changed it (`work`), and into the stroke's tiles if they changed what it
    /// keeps (`kept`).
    fn window_out(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        reach: IRect,
        changed: IRect,
        (work, kept): (bool, bool),
    ) {
        if changed.is_empty() {
            return;
        }
        let Some(smudge) = self.smudge.as_ref() else { return };
        let (Some(window), Some(layer)) = (smudge.window.as_ref(), self.layers.get(&smudge.into)) else { return };
        let row = row_bytes(reach.width());
        let bytes = u64::from(row) * u64::from(reach.height());
        let offset = |r: IRect| in_window(reach, r);
        if work {
            gpu.copy_buffer_to_texture(
                encoder,
                in_buffer(&window.work, offset(changed), row, changed.height()),
                copy_at(&layer.texture, changed.x0, changed.y0),
                extent(changed.width(), changed.height()),
            );
            self.dirty = self.dirty.union(changed);
        }
        if !kept {
            return;
        }
        for at in tiles_of(changed, smudge.tile) {
            let Some(tile) = smudge.tiles.get(&at) else { continue };
            let t = tile_rect(at, smudge.tile);
            let r = t.intersect(changed);
            if r.is_empty() {
                continue;
            }
            let (x, y, size) = (r.x0 - t.x0, r.y0 - t.y0, extent(r.width(), r.height()));
            let halves = [
                (&tile.grained, &window.grained, 0),
                (&tile.amount, &window.amount, 0),
                (&tile.shown, &window.amount, bytes),
            ];
            for (kept, buffer, start) in halves {
                if let (Some(kept), Some(buffer)) = (kept, buffer) {
                    gpu.copy_buffer_to_texture(
                        encoder,
                        in_buffer(buffer, start + offset(r), row, r.height()),
                        copy_at(kept, x, y),
                        size,
                    );
                }
            }
        }
    }

    /// Puts the open smudge stroke's layer back as it was wherever the stroke has written (and what
    /// it changes besides, as they were).
    pub(super) fn smudge_put_back(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder) {
        let full = self.full();
        let also = self.smudge_also();
        let Some(smudge) = self.smudge.as_ref() else { return };
        let Some(layer) = self.layers.get(&smudge.into) else { return };
        let also: Vec<Option<&wgpu::Texture>> = also.iter().map(|id| self.layers.get(id).map(|l| &l.texture)).collect();
        for (&at, tile) in &smudge.tiles {
            let r = tile_rect(at, smudge.tile).intersect(full);
            let back =
                std::iter::once((&tile.before, Some(&layer.texture))).chain(tile.more.iter().zip(also.iter().copied()));
            for (kept, to) in back {
                let Some(to) = to else { continue };
                gpu.copy_texture_to_texture(
                    encoder,
                    copy_at(kept, 0, 0),
                    copy_at(to, r.x0, r.y0),
                    extent(r.width(), r.height()),
                );
            }
        }
    }

    /// The open smudge stroke's layer as it was over `rect` (within the canvas), into `into`.
    pub(super) fn smudge_before(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        rect: IRect,
        into: &wgpu::Texture,
    ) {
        self.smudge_before_of(gpu, encoder, rect, into, None);
    }

    /// What the open smudge stroke works in as it was over `rect` (within the canvas), into `into`;
    /// or (`also`) the one of the textures it changes besides (see [`Canvas::smudge_also`]).
    pub(super) fn smudge_before_of(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        rect: IRect,
        into: &wgpu::Texture,
        also: Option<usize>,
    ) {
        let Some(smudge) = self.smudge.as_ref() else { return };
        let id = match also {
            Some(k) => self.smudge_also().get(k).copied(),
            None => Some(smudge.into),
        };
        let Some(layer) = id.and_then(|id| self.layers.get(&id)) else { return };
        // as it is where the stroke hasn't written, then its tiles
        gpu.copy_texture_to_texture(
            encoder,
            copy_at(&layer.texture, rect.x0, rect.y0),
            copy_at(into, 0, 0),
            extent(rect.width(), rect.height()),
        );
        for at in tiles_of(rect, smudge.tile) {
            let Some(tile) = smudge.tiles.get(&at) else { continue };
            let kept = match also {
                Some(k) => tile.more.get(k),
                None => Some(&tile.before),
            };
            let Some(kept) = kept else { continue };
            let t = tile_rect(at, smudge.tile);
            let r = t.intersect(rect);
            if r.is_empty() {
                continue;
            }
            gpu.copy_texture_to_texture(
                encoder,
                copy_at(kept, r.x0 - t.x0, r.y0 - t.y0),
                copy_at(into, r.x0 - rect.x0, r.y0 - rect.y0),
                extent(r.width(), r.height()),
            );
        }
    }

    /// The textures the open smudge stroke changes besides the one it works in: laying paint into
    /// its layer's store, the layer itself and the store's height.
    pub(super) fn smudge_also(&self) -> Vec<procraft_engine::document::LayerId> {
        let Some(smudge) = self.smudge.as_ref() else { return Vec::new() };
        smudge.store.as_ref().map_or_else(Vec::new, |s| vec![smudge.layer, s.planes[1]])
    }

    /// Makes the open smudge stroke's tiles under `rects` that it hasn't made yet (keeping what is
    /// under them as it was), before it writes there other than through a window; false if they
    /// couldn't be made (the stroke then lays nothing more).
    pub(super) fn smudge_keep(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, rects: &[IRect]) -> bool {
        let device = &gpu.device;
        let full = self.full();
        let also = self.smudge_also();
        let in_fragments = self.pipelines.compute.is_none();
        let Some(smudge) = self.smudge.as_mut() else { return false };
        if smudge.stalled {
            return false;
        }
        let Some(layer) = self.layers.get(&smudge.into) else { return false };
        let also: Vec<&wgpu::Texture> = also.iter().filter_map(|id| self.layers.get(id)).map(|l| &l.texture).collect();
        let (keeps, side) = (smudge.keeps, smudge.tile);
        let mut missing: Vec<(u32, u32)> = rects
            .iter()
            .flat_map(|w| tiles_of(w.intersect(full), side))
            .filter(|t| !smudge.tiles.contains_key(t))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        if missing.is_empty() {
            return true;
        }
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let invalid = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let tile = |label| texture(gpu, label, side, side, LAYER_FORMAT);
        // (the amounts' bits as words in fragment passes, as their bytes elsewhere)
        let amounts = if in_fragments { super::fragments::AMOUNT_FORMAT } else { LAYER_FORMAT };
        let amount_tile = |label| texture(gpu, label, side, side, amounts);
        let made: Vec<((u32, u32), Tile)> = missing
            .iter()
            .map(|&at| {
                let tile = Tile {
                    before: tile("smudge before"),
                    grained: keeps.grained.then(|| tile("smudge grained")),
                    amount: keeps.amount.then(|| amount_tile("smudge amount")),
                    shown: keeps.shown.then(|| amount_tile("smudge shown")),
                    more: also.iter().map(|_| tile("smudge kept")).collect(),
                };
                (at, tile)
            })
            .collect();
        if let Some(e) = pollster::block_on(invalid.pop()).or(pollster::block_on(memory.pop())) {
            log::error!("smudging: {e}");
            self.smudge_untold = true;
            smudge.stalled = true;
            return false;
        }
        smudge.laid = true;
        for (at, tile) in made {
            let r = tile_rect(at, side).intersect(full);
            let from = std::iter::once((&layer.texture, &tile.before))
                .chain(tile.grained.iter().map(|g| (&layer.texture, g)))
                .chain(also.iter().copied().zip(&tile.more));
            for (from, into) in from {
                gpu.copy_texture_to_texture(
                    encoder,
                    copy_at(from, r.x0, r.y0),
                    copy_at(into, 0, 0),
                    extent(r.width(), r.height()),
                );
            }
            smudge.tiles.insert(at, tile);
        }
        true
    }
}

/// Bytes a copy's carry takes put aside: wet paint's carried-over plot, or a smudge's grid.
fn aside_bytes(wet: bool) -> u64 {
    let side = if wet { super::WET_PLOT_MOST } else { super::SMUDGE_GRID };
    u64::from(side * side) * 16
}

/// Where what is kept of a copy's carry lies (see [`Kept`]): bytes into the stroke's carry buffer
/// and into the copy's put aside, and how many; none for nothing.
fn kept_bytes(kept: Kept) -> Option<(u64, u64, u64)> {
    match kept {
        Kept::Nothing => None,
        // the grid's rows within that many cells of its middle
        Kept::Cells(cells) => {
            let cells = cells.clamp(0, super::SMUDGE_REACH);
            let row = u64::from(super::SMUDGE_GRID) * 16;
            let at = u64::from((super::SMUDGE_GRID as i32 / 2 - cells).unsigned_abs()) * row;
            Some((at, at, (2 * cells.unsigned_abs() as u64 + 1) * row))
        }
        Kept::Plot { which, texels } => {
            let carried = u64::from(super::WET_PLOT_MOST * super::WET_PLOT_MOST);
            Some((u64::from(which.min(1)) * carried * 16, 0, u64::from(texels).min(carried) * 16))
        }
    }
}

/// Hands the stroke's carry buffer `carry` from copy `out` (what of its carry is kept goes aside) to
/// copy `into` (what of its carry its piece reads comes back).
fn hand_over(
    encoder: &mut wgpu::CommandEncoder,
    carry: &wgpu::Buffer,
    carries: &[super::Carry],
    out: Option<(usize, Kept)>,
    (into, needs): (usize, Kept),
) {
    let aside = |copy: usize| match carries.get(copy).and_then(|c| c.aside.as_ref()) {
        Some(super::Aside::Buffer(buffer)) => Some(buffer),
        _ => None,
    };
    if let Some((copy, kept)) = out
        && let (Some(aside), Some((at, aside_at, bytes))) = (aside(copy), kept_bytes(kept))
    {
        encoder.copy_buffer_to_buffer(carry, at, aside, aside_at, bytes);
    }
    if let (Some(aside), Some((at, aside_at, bytes))) = (aside(into), kept_bytes(needs)) {
        encoder.copy_buffer_to_buffer(aside, aside_at, carry, at, bytes);
    }
}

/// A smudge stroke's tiles, by where they are.
pub(super) type Tiles = BTreeMap<(u32, u32), Tile>;

impl super::Smudge {
    /// The textures the stroke holds: its tiles' and its window's (and in fragment passes, what
    /// the brush carries, and what its copies' carries are put aside in).
    pub(super) fn textures(&self) -> Vec<&wgpu::Texture> {
        let tiles = self.tiles.values().flat_map(|t| {
            std::iter::once(&t.before)
                .chain(t.grained.iter())
                .chain(t.amount.iter())
                .chain(t.shown.iter())
                .chain(t.more.iter())
        });
        let asides = self.carries.iter().filter_map(|c| match &c.aside {
            Some(super::Aside::Texture(tex)) => Some(&tex.texture),
            _ => None,
        });
        let fragments = self
            .held
            .iter()
            .flat_map(|h| h.textures())
            .chain(self.frag_window.iter().flat_map(|w| w.textures()))
            .chain(asides);
        tiles
            .chain(self.window.as_ref().map(|w| &w.before))
            .chain(self.passed.as_ref().map(|p| &p.0))
            .chain(fragments)
            .chain(self.rewind.iter().flat_map(super::rewind::Rewind::textures))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_the_longest_that_fits_and_never_none() {
        assert_eq!(runs(0, 10, |a, b| b - a <= 4), [(0, 4), (4, 8), (8, 10)]);
        assert_eq!(runs(3, 5, |_, _| false), [(3, 4), (4, 5)]);
        assert_eq!(runs(0, 7, |_, _| true), [(0, 7)]);
        assert!(runs(4, 4, |_, _| true).is_empty());
    }

    #[test]
    fn a_reach_is_held_to_the_canvas_as_the_shader_holds_its_reads() {
        let full = IRect::from_size(100, 50);
        // bilinear around 10..20, a pixel to spare
        assert_eq!(read_reach(Vec2::new(10.0, 10.0), Vec2::new(20.0, 12.0), full), IRect::new(8, 8, 22, 14));
        // wholly off the canvas: the edge pixels the shader clamps to
        assert_eq!(read_reach(Vec2::new(-40.0, 70.0), Vec2::new(-30.0, 80.0), full), IRect::new(0, 49, 1, 50));
        assert_eq!(read_reach(Vec2::new(f32::NAN, 0.0), Vec2::new(1.0, 1.0), full), full);
        // the tiles a rect touches
        let t = TILE as i32;
        let tiles: Vec<_> = tiles_of(IRect::new(t - 6, 10, t + 4, 2 * t + 88), TILE).collect();
        assert_eq!(tiles, [(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)]);
        assert_eq!(tiles_of(IRect::EMPTY, TILE).count(), 0);
        // a thin brush's, half as wide
        let thin: Vec<_> = tiles_of(IRect::new(t - 6, 10, t + 4, 300), THIN_TILE).collect();
        assert_eq!(thin, [(1, 0), (2, 0), (1, 1), (2, 1)]);
        assert_eq!(tile_rect((2, 1), THIN_TILE), IRect::new(512, 256, 768, 512));
        assert_eq!((tile_for(Some(20.0), false), tile_for(Some(600.0), false)), (THIN_TILE, TILE));
        assert_eq!((tile_for(None, false), tile_for(Some(20.0), true)), (TILE, TILE));
        assert_eq!(part(0, 0, 255, 255), WHOLE);
    }

    #[test]
    fn each_tile_s_part_of_a_window_starts_where_a_copy_can_place_it() {
        // windows from any column, wide and narrow, over tiles they cross part of
        let windows = [(0, 0, 1, 1), (3, 7, 900, 300), (250, 250, 20, 700), (257, 1, 1500, 2), (1023, 511, 513, 257)];
        for (x0, y0, w, h) in windows.into_iter().chain([(130, 129, 4000, 9), (5000, 3, 77, 600)]) {
            let reach = window_reach(IRect::new(x0, y0, x0 + w, y0 + h));
            assert!(reach.x0 <= x0 && x0 - reach.x0 < 128, "{reach:?}");
            assert_eq!((reach.x1, reach.y0, reach.y1), (x0 + w, y0, y0 + h));
            let bytes = u64::from(row_bytes(reach.width())) * u64::from(reach.height());
            for at in tiles_of(reach, TILE) {
                let r = tile_rect(at, TILE).intersect(reach);
                // the amount's second half too
                for start in [0, bytes] {
                    assert_eq!((start + in_window(reach, r)) % u64::from(PLACEMENT), 0, "{reach:?} {at:?}");
                }
            }
        }
        assert_eq!(window_reach(IRect::EMPTY), IRect::EMPTY);
    }
}
