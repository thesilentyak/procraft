//! Brush engines of plugins' own (interface 4). A plugin that lists `brush_engines` reads brush
//! files itself and draws the brushes it makes of them its own way: the app hands it a stroke's
//! input and lays the dabs it answers, on the GPU, as it lays its own stamps.
//!
//! - **Import.** A brush file whose name ends as an engine on says (`accepts`) goes to it as well as
//!   through the app's own reading. Each brush it makes, a name and data of its own (a preview and
//!   a size if it gives them), joins the brush the app read of the same name, or comes in on a
//!   round brush if there is none: [`Brush::engine`] keeps the engine's name and the data. With the
//!   engine off or gone, the brush draws as the app read it, so the app is as it was.
//! - **Look.** Before a stroke, the app asks the engine how the brush looks ([`BrushLook`]): its tips
//!   (grey pictures, one picked per dab), whether its dabs build up over each other or towards their
//!   opacity, its blend mode, whether it erases, a texture dabs read where they land, and a second
//!   stroke whose coverage combines with the first (a dual brush). Kept for the brush's data while
//!   the plugin stays on, and on the disk with the plugin, so the next start needn't ask again: the
//!   engine then only reads the brush to draw it.
//! - **Strokes.** A stroke's samples go to the engine as they come ([`EngineHalf`]), and the dabs it
//!   answers become the stroke's stamps: each its place, size, angle, squash, opacity, flow, tip,
//!   mirroring, its own colour if it has one, and how strongly the texture shows in it.
//!
//! Records are the layer items' (see `plugin_layers.rs`): fields of a 16-bit id, a 32-bit length
//! and the bytes, little-endian. What each holds is in `docs/plugins.md`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use procraft_engine::brush::Rendering;
use procraft_engine::brush::{Brush, OutsideBrush};
use procraft_engine::geom::Vec2;
use procraft_engine::input::Sample;
use procraft_engine::stroke::{OutsideEngine, Stamp, Tint};
use procraft_gpu::{SecondStroke, StrokeStyle, StrokeTool, TipKind};

use crate::CanvasSession;
use crate::hooks::{Hooks, Shared, at_work};
use crate::plugin_layers::{Record, fields};
use crate::plugin_run::{DAB_BYTES, Following, ImportLimits, Stop};

/// A brush engine a plugin has: its key (`<id>.<key>`), what it is called and the endings of the
/// names of the brush files it reads.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BrushEngine {
    pub key: String,
    pub label: String,
    pub accepts: Vec<String>,
    /// The action (`<id>.<key>`) that is its brushes' studio, if it has one.
    pub studio: Option<String>,
}

/// A brush an engine made of a file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MadeBrush {
    pub engine: String,
    pub name: String,
    pub data: Vec<u8>,
    pub thumbnail: Option<Vec<u8>>,
}

/// The ids of the records' fields.
mod field {
    /// The file handed to an engine: its name, its bytes.
    pub const FILE_NAME: u16 = 1;
    pub const FILE_BYTES: u16 = 2;
    /// The brushes made of it, each a record of its name, data and preview.
    pub const MADE: u16 = 1;
    pub const MADE_NAME: u16 = 1;
    pub const MADE_DATA: u16 = 2;
    pub const MADE_THUMBNAIL: u16 = 3;
    /// A look: its tips (each width, height, pixels), how its dabs build up, its blend mode,
    /// whether it erases, its texture and its second stroke.
    pub const TIP: u16 = 1;
    pub const TIP_WIDTH: u16 = 1;
    pub const TIP_HEIGHT: u16 = 2;
    pub const TIP_PIXELS: u16 = 3;
    pub const BUILD: u16 = 2;
    pub const BLEND: u16 = 3;
    pub const ERASES: u16 = 4;
    pub const TEXTURE: u16 = 5;
    pub const TEXTURE_SCALE: u16 = 4;
    pub const TEXTURE_MODE: u16 = 5;
    pub const SECOND: u16 = 6;
    pub const SECOND_COMBINE: u16 = 3;
    /// What its tips are (u8, the GPU's tip kinds; masks if it doesn't say).
    pub const TIP_KIND: u16 = 7;
    /// How often it lays a dab while the pen holds still (f32 ms), if it does (an airbrush).
    pub const AIRBRUSH: u16 = 8;
    /// Its dabs move paint (u8 1): a smudge whose stamps pick up behind them.
    pub const SMUDGES: u16 = 9;
    /// What its dabs pick up is the image as shown (every layer), not their layer alone (u8 1).
    pub const FROM_IMAGE: u16 = 10;
    /// Its strokes want the colours of their layer under where they start, a square this many
    /// pixels a side (u32).
    pub const UNDER_START: u16 = 11;
    /// What its strokes keep with their layer between strokes (u8, a layer store's kind: 1 the paint
    /// they lay and a height lighting it), with field 9.
    pub const STORE: u16 = 12;
    /// A load: the brush's id and data, and whether the app holds its look already.
    pub const LOAD_ID: u16 = 1;
    pub const LOAD_DATA: u16 = 2;
    pub const LOAD_LOOK_HELD: u16 = 3;
    /// A stroke starting: the brush's id, the colour and the secondary colour, the size now and
    /// where it starts, the opacity slider, the tool, the pointer, a seed and the view's turn.
    pub const BEGIN_BRUSH: u16 = 1;
    pub const BEGIN_COLOUR: u16 = 2;
    pub const BEGIN_SECONDARY: u16 = 3;
    pub const BEGIN_DIAMETER: u16 = 4;
    pub const BEGIN_OPACITY: u16 = 5;
    pub const BEGIN_TOOL: u16 = 6;
    pub const BEGIN_POINTER: u16 = 7;
    pub const BEGIN_SEED: u16 = 8;
    pub const BEGIN_TURN: u16 = 9;
    pub const BEGIN_START_DIAMETER: u16 = 10;
    /// The colours under where the stroke starts, if its look asks: the square's left and top
    /// (i32, canvas pixels), its side (u32), then its pixels (RGBA8, straight, rows from the top).
    pub const BEGIN_UNDER: u16 = 11;
    /// The canvas's width and height (f32, canvas pixels).
    pub const BEGIN_CANVAS: u16 = 12;
}

/// A dab's flags.
const FLIP_ACROSS: u32 = 1;
const FLIP_DOWN: u32 = 2;
const OWN_COLOUR: u32 = 4;
const FOR_SECOND: u32 = 8;
/// A dab that moves the paint under where the stroke's last lay; that paint made one colour first;
/// that weighed by its tip; the colour it mixes in clear (taking paint away); what it moves laid over
/// what is there (not in its place); picking up where it lies itself.
const MOVES_PAINT: u32 = 16;
const DULLS: u32 = 32;
const WEIGHED: u32 = 64;
const CLEAR_PAINT: u32 = 128;
const LAID_OVER: u32 = 256;
const PICKS_HERE: u32 = 512;
/// It picks up from elsewhere: as far from where it lies as words 20 and 21 say.
const PICKS_FROM: u32 = 1024;
/// What it picks up comes through a warp: its kind (word 23), amount (22) and shift (20, 21).
const WARPS: u32 = 2048;
/// What it picks up is blurred (word 22, 0..1) and then sharpened (word 23) before it is laid.
const FILTERS: u32 = 4096;
/// It picks up from the layer as it was before the stroke.
const PICKS_BEFORE: u32 = 8192;
/// It lays nothing: its engine is told the colours of the layer under it, as the stroke has laid
/// it by then, before the engine's next samples (word 7 says how they are weighed).
const TELLS_UNDER: u32 = 16384;
/// Its own colour (flags 4 and 16) is as opaque as word 22 says: laid in place, it thins what is
/// there towards that.
const SEE_THROUGH: u32 = 32768;

/// The most dabs a stroke may ask about at once, and the most pixels read for them at once.
const MOST_ASKED: usize = 4096;
const MOST_READ: i64 = 1 << 20;
/// Into its layer's store: its tip's grey goes over the height there, as opaque as word 20 says.
const LAYS_HEIGHT: u32 = 65536;

/// The most tips, and the largest side of one, and of a texture.
const MOST_TIP_SIDE: u32 = 4096;
/// The side tips are drawn into on the GPU at most, and the bytes a set may take there (before its
/// mip levels).
const MOST_SET_SIDE: u32 = 1024;
const MOST_SET_BYTES: usize = 48 << 20;
const MOST_TEXTURE_SIDE: u32 = 2048;
/// The looks each plugin keeps by the brushes' data.
pub(crate) const LOOKS_KEPT: usize = 16;

/// The tips a stroke picks from, fitted into squares a power of two a side for the GPU, what they
/// are (a byte a pixel for masks, else RGBA, straight), and their id there.
#[derive(Debug, PartialEq)]
pub(crate) struct TipSet {
    pub id: u64,
    pub side: u32,
    pub kind: TipKind,
    pub layers: Vec<Vec<u8>>,
}

/// How a stroke's stamps build up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Build {
    /// Each laid over those before.
    Over,
    /// Each towards its opacity by its flow, never past it nor back.
    Towards,
}

/// A texture the dabs read where they land, as a grain: a square, one tile of it `scale` canvas
/// pixels to one of its own, combined with each dab by `mode`.
#[derive(Debug, PartialEq)]
pub(crate) struct LookTexture {
    pub id: u64,
    pub side: u32,
    pub pixels: Vec<u8>,
    pub tile: f32,
    pub mode: u32,
}

/// How a brush an engine draws looks (see the module's notes).
#[derive(Debug, PartialEq)]
pub(crate) struct BrushLook {
    /// Every how many milliseconds its engine is handed the pen again while it holds still.
    pub airbrush: Option<f32>,
    /// Its dabs move paint (as a smudge's stamps do, picking up behind them).
    pub smudges: bool,
    /// What they pick up is the image as shown, every layer.
    pub from_image: bool,
    /// How big a square of the layer's colours under where a stroke starts it wants (0 none).
    pub under_start: u32,
    /// What its strokes keep with their layer between strokes (a layer store's kind, 0 none).
    pub store: u32,
    pub tips: TipSet,
    pub build: Build,
    pub blend: Option<u32>,
    pub erases: bool,
    pub texture: Option<LookTexture>,
    pub second: Option<SecondLook>,
}

/// A brush look's second stroke: its tips, how they build up, and how its coverage combines with
/// the first's (a dual brush's mode).
#[derive(Debug, PartialEq)]
pub(crate) struct SecondLook {
    pub tips: TipSet,
    pub build: Build,
    pub combine: u32,
}

fn u32_of(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(*bytes.first_chunk::<4>()?))
}

fn f32_of(bytes: &[u8]) -> Option<f32> {
    Some(f32::from_le_bytes(*bytes.first_chunk::<4>()?)).filter(|v| v.is_finite())
}

/// The record `bytes` as a look, the tips' set given `id` (and the second's and the texture's
/// near it); none if it doesn't hold up.
pub(crate) fn read_look(bytes: &[u8], id: u64) -> Option<BrushLook> {
    let kind = fields(bytes).find(|(f, _)| *f == field::TIP_KIND).map_or(TipKind::Mask, |(_, v)| match v.first() {
        Some(1) => TipKind::Picture,
        Some(2) => TipKind::Lightness,
        Some(3) => TipKind::Gradient,
        _ => TipKind::Mask,
    });
    let mut tips = Vec::new();
    let (mut build, mut blend, mut erases, mut texture, mut second) = (Build::Over, None, false, None, None);
    let (mut airbrush, mut smudges, mut from_image, mut under_start, mut store) = (None, false, false, 0, 0);
    for (f, value) in fields(bytes) {
        match f {
            field::STORE => store = store_kind(value),
            field::SMUDGES => smudges = value.first() == Some(&1),
            field::FROM_IMAGE => from_image = value.first() == Some(&1),
            field::UNDER_START => under_start = u32_of(value).unwrap_or(0).min(MOST_UNDER_START),
            field::AIRBRUSH => airbrush = f32_of(value).filter(|ms| *ms > 0.0).map(|ms| ms.max(MOST_AIRBRUSH_MS)),
            field::TIP => tips.push(read_tip(value, kind.channels() as usize)?),
            field::BUILD => build = read_build(value),
            field::BLEND => blend = u32_of(value),
            field::ERASES => erases = value.first().is_some_and(|&b| b != 0),
            field::TEXTURE => texture = read_texture(value, id ^ 0x7e47_0001),
            field::SECOND => second = read_second(value, id ^ 0x5eed_0002),
            _ => {}
        }
    }
    Some(BrushLook {
        airbrush,
        smudges,
        from_image,
        under_start,
        // (only for a brush whose dabs move paint)
        store: if smudges { store } else { 0 },
        tips: tip_set(&tips, id, kind)?,
        build,
        blend,
        erases,
        texture,
        second,
    })
}

/// A look's store's kind, as its field says: 0 for none, or for one there isn't.
fn store_kind(value: &[u8]) -> u32 {
    value.first().map(|&k| u32::from(k)).filter(|&k| procraft_gpu::LayerStore::planes_of(k) > 0).unwrap_or(0)
}

/// The most samples a brush engine is handed at once.
const SAMPLES_A_CALL: usize = 4;

/// The pen is handed again at most this often (ms), however often an airbrush lays its dabs.
const MOST_AIRBRUSH_MS: f32 = 4.0;

/// A set's tips, as the GPU takes them.
fn layers(set: &TipSet) -> Vec<&[u8]> {
    set.layers.iter().map(Vec::as_slice).collect()
}

fn read_build(value: &[u8]) -> Build {
    if value.first().is_some_and(|&b| b == 1) { Build::Towards } else { Build::Over }
}

/// A tip: its width, height and pixels, `channels` bytes each (a grey, 255 painting; or RGBA).
fn read_tip(bytes: &[u8], channels: usize) -> Option<(u32, u32, Vec<u8>)> {
    let (mut w, mut h, mut pixels) = (None, None, None);
    for (f, value) in fields(bytes) {
        match f {
            field::TIP_WIDTH => w = u32_of(value),
            field::TIP_HEIGHT => h = u32_of(value),
            field::TIP_PIXELS => pixels = Some(value),
            _ => {}
        }
    }
    let (w, h, pixels) = (w?, h?, pixels?);
    let fits = (1..=MOST_TIP_SIDE).contains(&w) && (1..=MOST_TIP_SIDE).contains(&h);
    (fits && pixels.len() == (w as usize) * (h as usize) * channels).then(|| (w, h, pixels.to_vec()))
}

fn read_second(bytes: &[u8], id: u64) -> Option<SecondLook> {
    let mut tips = Vec::new();
    let (mut build, mut combine) = (Build::Towards, 1);
    for (f, value) in fields(bytes) {
        match f {
            field::TIP => tips.push(read_tip(value, 1)?),
            field::BUILD => build = read_build(value),
            field::SECOND_COMBINE => combine = u32_of(value)?,
            _ => {}
        }
    }
    Some(SecondLook { tips: tip_set(&tips, id, TipKind::Mask)?, build, combine })
}

fn read_texture(bytes: &[u8], id: u64) -> Option<LookTexture> {
    let (mut scale, mut mode) = (1.0, 1);
    // its picture's fields are a tip's
    let mut parts = Record::default();
    for (f, value) in fields(bytes) {
        match f {
            field::TIP_WIDTH | field::TIP_HEIGHT | field::TIP_PIXELS => {
                parts.field(f, value);
            }
            field::TEXTURE_SCALE => scale = f32_of(value)?,
            field::TEXTURE_MODE => mode = u32_of(value)?,
            _ => {}
        }
    }
    let (w, h, pixels) = read_tip(&parts.bytes(), 1)?;
    if !(0.01..=1000.0).contains(&scale) {
        return None;
    }
    // a square a power of two a side, as a grain is read from: one tile its longer side
    let side = w.max(h).next_power_of_two().clamp(16, MOST_TEXTURE_SIDE);
    let pixels = stretched(w, h, &pixels, side);
    Some(LookTexture { id, side, pixels, tile: w.max(h) as f32 * scale, mode })
}

/// The tips as a set for the GPU: each fitted into a square, all one side (a power of two, as small
/// as keeps the largest whole, held within what a set may take).
fn tip_set(tips: &[(u32, u32, Vec<u8>)], id: u64, kind: TipKind) -> Option<TipSet> {
    let most = procraft_gpu::MOST_TIPS as usize;
    if tips.is_empty() || tips.len() > most {
        return None;
    }
    let channels = kind.channels() as usize;
    let largest = tips.iter().map(|(w, h, _)| *w.max(h)).max().unwrap_or(1);
    let mut side = largest.next_power_of_two().clamp(16, MOST_SET_SIDE);
    while side > 16 && (side as usize).pow(2) * tips.len() * channels > MOST_SET_BYTES {
        side /= 2;
    }
    let layers = tips
        .iter()
        .map(
            |(w, h, pixels)| {
                if channels == 1 { fitted(*w, *h, pixels, side) } else { fitted_rgba(*w, *h, pixels, side) }
            },
        )
        .collect();
    Some(TipSet { id, side, kind, layers })
}

/// RGBA `pixels` (`w` x `h`, straight) fitted as [`fitted`] fits a grey picture: each channel as
/// it covers, so that clear pixels lend none of their colour; straight again.
fn fitted_rgba(w: u32, h: u32, pixels: &[u8], side: u32) -> Vec<u8> {
    let px = pixels.as_chunks::<4>().0;
    let channel = |c: usize| -> Vec<u8> {
        let plane: Vec<u8> = px
            .iter()
            .map(|&[r, g, b, a]| {
                let v = match c {
                    0 => r,
                    1 => g,
                    2 => b,
                    _ => return a,
                };
                ((u32::from(v) * u32::from(a) + 127) / 255) as u8
            })
            .collect();
        fitted(w, h, &plane, side)
    };
    let [r, g, b, a] = [channel(0), channel(1), channel(2), channel(3)];
    let straight =
        |v: u8, a: u8| if a == 0 { 0 } else { ((u32::from(v) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8 };
    r.iter()
        .zip(&g)
        .zip(&b)
        .zip(&a)
        .flat_map(|(((&r, &g), &b), &a)| [straight(r, a), straight(g, a), straight(b, a), a])
        .collect()
}

/// `pixels` (`w` x `h`) fitted into a square `side` a side, its longer side across all of it,
/// centred: each pixel the average of those it covers, or between the four nearest where it is
/// drawn larger.
fn fitted(w: u32, h: u32, pixels: &[u8], side: u32) -> Vec<u8> {
    let tip = procraft_engine::abr::AbrTip {
        name: String::new(),
        spacing: None,
        width: w,
        height: h,
        pixels: pixels.to_vec(),
    };
    crate::brush_files::fitted_at(&tip, side as usize)
}

/// `pixels` (`w` x `h`) drawn over a square `side` a side, stretched to fill it (a texture's tile).
fn stretched(w: u32, h: u32, pixels: &[u8], side: u32) -> Vec<u8> {
    let (w, h, side) = (w as usize, h as usize, side as usize);
    let get = |x: usize, y: usize| f32::from(pixels.get(y.min(h - 1) * w + x.min(w - 1)).copied().unwrap_or(0));
    let mut out = vec![0_u8; side * side];
    for y in 0..side {
        let v = ((y as f32 + 0.5) * h as f32 / side as f32 - 0.5).max(0.0);
        let (y0, fy) = (v.floor() as usize, v.fract());
        for x in 0..side {
            let u = ((x as f32 + 0.5) * w as f32 / side as f32 - 0.5).max(0.0);
            let (x0, fx) = (u.floor() as usize, u.fract());
            // wrapping round, as a tile does
            let (x1, y1) = ((x0 + 1) % w, (y0 + 1) % h);
            let top = get(x0, y0) * (1.0 - fx) + get(x1, y0) * fx;
            let bottom = get(x0, y1) * (1.0 - fx) + get(x1, y1) * fx;
            if let Some(v) = out.get_mut(y * side + x) {
                *v = (top * (1.0 - fy) + bottom * fy).round() as u8;
            }
        }
    }
    out
}

/// A look as it is kept on the disk: as the GPU takes it (its tips fitted, its texture a square),
/// so that a start only reads it. Fields: 1 its version, 2 its tips (a side, then each layer), 3
/// how they build up, 4 its blend, 5 whether it erases, 6 its texture (a side, its pixels, its
/// tile, its mode) and 7 its second (its tips, how they build up, how it combines); 8 to 12 as a
/// look's own.
const KEPT_LOOK_VERSION: u32 = 7;

/// The largest square of colours under a stroke's start an engine may ask for.
const MOST_UNDER_START: u32 = 512;

fn kept_tips(set: &TipSet) -> Vec<u8> {
    let mut r = Record::default();
    r.field(1, &set.side.to_le_bytes()).field(3, &[set.kind as u8]);
    for layer in &set.layers {
        r.field(2, layer);
    }
    r.bytes()
}

fn kept_build(build: Build) -> u8 {
    u8::from(build == Build::Towards)
}

/// `look` as it is kept on the disk.
pub(crate) fn kept_look(look: &BrushLook) -> Vec<u8> {
    let mut r = Record::default();
    r.field(1, &KEPT_LOOK_VERSION.to_le_bytes()).field(2, &kept_tips(&look.tips)).field(3, &[kept_build(look.build)]);
    if let Some(blend) = look.blend {
        r.field(4, &blend.to_le_bytes());
    }
    r.field(5, &[u8::from(look.erases)]);
    if let Some(ms) = look.airbrush {
        r.field(8, &ms.to_le_bytes());
    }
    r.field(9, &[u8::from(look.smudges)]);
    r.field(10, &[u8::from(look.from_image)]);
    r.field(11, &look.under_start.to_le_bytes());
    r.field(12, &[u8::try_from(look.store).unwrap_or(0)]);
    if let Some(t) = &look.texture {
        let mut tex = Record::default();
        tex.field(1, &t.side.to_le_bytes())
            .field(2, &t.pixels)
            .field(3, &t.tile.to_le_bytes())
            .field(4, &t.mode.to_le_bytes());
        r.field(6, &tex.bytes());
    }
    if let Some(s) = &look.second {
        let mut second = Record::default();
        second.field(1, &kept_tips(&s.tips)).field(2, &[kept_build(s.build)]).field(3, &s.combine.to_le_bytes());
        r.field(7, &second.bytes());
    }
    r.bytes()
}

/// A tip set as kept: a side a power of two the GPU takes, each layer that square.
fn tips_kept(bytes: &[u8], id: u64) -> Option<TipSet> {
    let (mut side, mut kind) = (None, TipKind::Mask);
    let mut layers = Vec::new();
    for (f, value) in fields(bytes) {
        match f {
            1 => side = u32_of(value),
            2 => layers.push(value.to_vec()),
            3 => {
                kind = match value.first() {
                    Some(1) => TipKind::Picture,
                    Some(2) => TipKind::Lightness,
                    Some(3) => TipKind::Gradient,
                    _ => TipKind::Mask,
                }
            }
            _ => {}
        }
    }
    let side = side.filter(|s| s.is_power_of_two() && (16..=MOST_SET_SIDE).contains(s))?;
    let fits = !layers.is_empty()
        && layers.len() <= procraft_gpu::MOST_TIPS as usize
        && layers.iter().all(|l| l.len() == (side as usize).pow(2) * kind.channels() as usize);
    fits.then_some(TipSet { id, side, kind, layers })
}

/// The look kept on the disk as `bytes`, its tips' set given `id` (and the rest near it, as
/// [`read_look`] gives them); none if it doesn't hold up, or was kept by another version.
pub(crate) fn look_kept_as(bytes: &[u8], id: u64) -> Option<BrushLook> {
    let (mut version, mut tips, mut build, mut blend, mut erases, mut texture, mut second) =
        (None, None, Build::Over, None, false, None, None);
    let (mut airbrush, mut smudges, mut from_image, mut under_start, mut store) = (None, false, false, 0, 0);
    for (f, value) in fields(bytes) {
        match f {
            1 => version = u32_of(value),
            12 => store = store_kind(value),
            9 => smudges = value.first() == Some(&1),
            10 => from_image = value.first() == Some(&1),
            11 => under_start = u32_of(value).unwrap_or(0).min(MOST_UNDER_START),
            8 => airbrush = f32_of(value).filter(|ms| *ms >= MOST_AIRBRUSH_MS),
            2 => tips = Some(tips_kept(value, id)?),
            3 => build = read_build(value),
            4 => blend = u32_of(value),
            5 => erases = value.first().is_some_and(|&b| b != 0),
            6 => {
                let (mut side, mut pixels, mut tile, mut mode) = (None, None, None, None);
                for (f, value) in fields(value) {
                    match f {
                        1 => side = u32_of(value),
                        2 => pixels = Some(value.to_vec()),
                        3 => tile = f32_of(value),
                        4 => mode = u32_of(value),
                        _ => {}
                    }
                }
                let (side, pixels) = (side?, pixels?);
                let square =
                    side.is_power_of_two() && side <= MOST_TEXTURE_SIDE && pixels.len() == (side as usize).pow(2);
                if !square {
                    return None;
                }
                texture = Some(LookTexture { id: id ^ 0x7e47_0001, side, pixels, tile: tile?, mode: mode? });
            }
            7 => {
                let (mut tips, mut build, mut combine) = (None, Build::Towards, None);
                for (f, value) in fields(value) {
                    match f {
                        1 => tips = Some(tips_kept(value, id ^ 0x5eed_0002)?),
                        2 => build = read_build(value),
                        3 => combine = u32_of(value),
                        _ => {}
                    }
                }
                second = Some(SecondLook { tips: tips?, build, combine: combine? });
            }
            _ => {}
        }
    }
    (version == Some(KEPT_LOOK_VERSION)).then_some(())?;
    let store = if smudges { store } else { 0 };
    Some(BrushLook {
        airbrush,
        smudges,
        from_image,
        under_start,
        store,
        tips: tips?,
        build,
        blend,
        erases,
        texture,
        second,
    })
}

/// What a brush's load hands its engine: the brush's id and its data, and, if the app holds its
/// look, that it needn't answer one.
fn load_record(id: u64, data: &[u8], look_held: bool) -> Vec<u8> {
    let mut r = Record::default();
    r.field(field::LOAD_ID, &id.to_le_bytes()).field(field::LOAD_DATA, data);
    if look_held {
        r.field(field::LOAD_LOOK_HELD, &[1]);
    }
    r.bytes()
}

/// The id of a brush's data: what its look is kept by, here and on the GPU.
pub(crate) fn data_id(data: &[u8]) -> u64 {
    crate::project::pixels_hash(data) ^ (data.len() as u64).rotate_left(17)
}

/// Where a brush file's bytes are: in memory, or in its file on the disk, read as they are handed to
/// an engine.
#[derive(Clone, Debug)]
pub enum BrushFileBytes {
    Memory(Arc<[u8]>),
    Disk(std::path::PathBuf),
}

/// The record a file called `name` (`file`) is handed to an engine as: its name and its bytes, and
/// where its bytes are in it (from, how many); its bytes read straight into it, never held twice.
/// None if it can't be read, or is over `most` bytes (then the second is true).
fn file_record(name: &str, file: &BrushFileBytes, most: usize) -> Result<(Vec<u8>, (usize, usize)), bool> {
    use std::io::Read;
    let len = match file {
        BrushFileBytes::Memory(bytes) => bytes.len(),
        BrushFileBytes::Disk(path) => {
            usize::try_from(std::fs::metadata(path).map_err(|_| false)?.len()).map_err(|_| true)?
        }
    };
    if len > most || u32::try_from(len).is_err() {
        return Err(true);
    }
    let head = |record: &mut Vec<u8>, id: u16, len: usize| {
        record.extend_from_slice(&id.to_le_bytes());
        record.extend_from_slice(&u32::try_from(len).unwrap_or(0).to_le_bytes());
    };
    let mut record = Vec::new();
    record.try_reserve_exact(12 + name.len() + len).map_err(|_| true)?;
    head(&mut record, field::FILE_NAME, name.len());
    record.extend_from_slice(name.as_bytes());
    head(&mut record, field::FILE_BYTES, len);
    let from = record.len();
    match file {
        BrushFileBytes::Memory(bytes) => record.extend_from_slice(bytes),
        BrushFileBytes::Disk(path) => {
            let mut f = std::fs::File::open(path).map_err(|_| false)?;
            f.read_to_end(&mut record).map_err(|_| false)?;
        }
    }
    if record.len() != from + len {
        return Err(false);
    }
    Ok((record, (from, len)))
}

/// What making a brush's look may take: an import's memory, and less time than a file's (a look
/// is made as the brush is first drawn).
const LOOK_LIMITS: ImportLimits = ImportLimits {
    memory: crate::plugin_run::MOST_IMPORT_MEMORY,
    steps: 40_000_000_000,
    time: std::time::Duration::from_secs(120),
};

/// Why an engine couldn't make its brushes of a file: too big for what an import may have on this
/// device, too long, it crashed or answered nonsense, or the import was cancelled. The plugin stays
/// on whichever: the file's brushes come in as the app read them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportTrouble {
    TooBig,
    TooSlow,
    Failed,
    Cancelled,
}

impl ImportTrouble {
    fn of(stop: Stop) -> ImportTrouble {
        match stop {
            Stop::Memory => ImportTrouble::TooBig,
            Stop::TooSlow => ImportTrouble::TooSlow,
            Stop::Cancelled => ImportTrouble::Cancelled,
            Stop::Crashed | Stop::Nonsense | Stop::Damaged => ImportTrouble::Failed,
        }
    }
}

/// What the engines on made of a brush file: the brushes; if one couldn't, its label and why;
/// whether the file is a pack of brushes (a zip) rather than one; and the most memory an import's
/// copy of a plugin's code came to (bytes).
#[derive(Debug, Default)]
pub(crate) struct EnginesMade {
    pub made: Vec<MadeBrush>,
    pub trouble: Option<(String, ImportTrouble)>,
    pub pack: bool,
    pub memory: usize,
}

/// The brushes the engines on make of a file called `name` (`bytes`): those whose `accepts` has the
/// ending of its name, or every engine for a name without one.
#[cfg(test)]
pub(crate) fn make_brushes(hooks: &Hooks, name: &str, bytes: &[u8]) -> Vec<MadeBrush> {
    let file = BrushFileBytes::Memory(bytes.into());
    let made = make_brushes_following(hooks, name, &file, &Following::default(), ImportLimits::now());
    made.map(|m| m.made).unwrap_or_default()
}

/// What the engines on make of a file called `name` (`file`), as [`make_brushes`] says; none if no
/// engine on reads it. `following` stops it and is told how far it has come.
///
/// Each engine gets the file on a copy of its plugin's code of its own, started with `limits`'s
/// memory and let go after, and may spend its steps and take its time: what drawing may have is
/// far less. A file that is too much for that (too big for the memory, too long) or that the
/// engine fails on comes in as the app read it, and the plugin stays on: the trouble is told, not
/// held against it.
pub(crate) fn make_brushes_following(
    hooks: &Hooks,
    name: &str,
    file: &BrushFileBytes,
    following: &Following,
    limits: ImportLimits,
) -> Option<EnginesMade> {
    let ending = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let engines = hooks.engines_reading(ending.as_deref());
    if engines.is_empty() {
        return None;
    }
    let cancelled = || following.cancel.as_ref().is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed));
    let mut out = EnginesMade::default();
    for (plugin, running, index, key) in engines {
        let label = hooks.engine_label(&key).unwrap_or_default();
        if cancelled() {
            out.trouble = Some((label, ImportTrouble::Cancelled));
            break;
        }
        let (record, span) = match file_record(name, file, limits.memory) {
            Ok(record) => record,
            Err(too_big) => {
                log::warn!("{name} couldn't be handed to {key} (too big: {too_big})");
                let why = if too_big { ImportTrouble::TooBig } else { ImportTrouble::Failed };
                out.trouble = Some((label, why));
                continue;
            }
        };
        out.pack |= record.get(span.0..span.0 + 4) == Some(&b"PK\x03\x04"[..]);
        let started = std::time::Instant::now();
        let answer = match hooks.copier(&plugin) {
            Some(copy) => copy(limits.memory).and_then(|mut copied| {
                let answer = copied.brush_import(index, record, span, (limits.steps, limits.time), following);
                out.memory = out.memory.max(copied.memory_used());
                log::info!(
                    "{key} had {name}: {} steps in {:?}, its memory at most {} MB",
                    copied.spent(),
                    started.elapsed(),
                    copied.memory_used() >> 20
                );
                answer
            }),
            // (code put on its hooks alone, as tests do: on that code, which stops if it fails)
            None => {
                let answer =
                    at_work(&running).brush_import(index, record, span, (limits.steps, limits.time), following);
                if let Err(stop) = answer
                    && stop != Stop::Cancelled
                {
                    hooks.stopped(&plugin, stop);
                }
                answer
            }
        };
        match answer {
            Ok(answer) => out.made.extend(read_made(&answer, &key)),
            Err(stop) => {
                log::warn!("{key} couldn't make brushes of {name}: {stop:?}");
                out.trouble = Some((label, ImportTrouble::of(stop)));
            }
        }
    }
    Some(out)
}

/// The brushes in the record an engine `key` answered an import with.
pub(crate) fn read_made(record: &[u8], key: &str) -> Vec<MadeBrush> {
    let mut made = Vec::new();
    for (f, value) in fields(record) {
        if f != field::MADE {
            continue;
        }
        let (mut name, mut data, mut thumbnail) = (None, None, None);
        for (f, value) in fields(value) {
            match f {
                field::MADE_NAME => name = std::str::from_utf8(value).ok().map(|n| n.chars().take(256).collect()),
                field::MADE_DATA => data = Some(value.to_vec()),
                field::MADE_THUMBNAIL => thumbnail = Some(value.to_vec()).filter(|t| t.starts_with(b"\x89PNG")),
                _ => {}
            }
        }
        if let (Some(name), Some(data)) = (name, data) {
            made.push(MadeBrush { engine: key.to_owned(), name, data, thumbnail });
        }
    }
    made
}

/// `brushes` (as the app read a file) with what engines `made` of it: each such brush joins the one
/// of its name not yet joined, or comes in on a round brush after them.
pub(crate) fn join_made(brushes: &mut Vec<Brush>, made: Vec<MadeBrush>) {
    let mut joined = vec![false; brushes.len()];
    for m in made {
        let engine = Some(OutsideBrush { engine: m.engine, data: m.data.into() });
        let at = brushes.iter().zip(&joined).position(|(b, &j)| !j && b.name.trim() == m.name.trim());
        match at.and_then(|i| brushes.get_mut(i).map(|b| (i, b))) {
            Some((i, b)) => {
                b.engine = engine;
                if b.thumbnail.is_none() {
                    b.thumbnail = m.thumbnail.map(Into::into);
                }
                if let Some(j) = joined.get_mut(i) {
                    *j = true;
                }
            }
            None => {
                brushes.push(Brush {
                    name: m.name,
                    thumbnail: m.thumbnail.map(Into::into),
                    engine,
                    ..Brush::round_pen()
                });
                joined.push(true);
            }
        }
    }
}

/// What a stroke tells its engine as it starts, besides its brush.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Starting {
    /// Its colour and secondary colour (sRGB 0..1), its size (canvas pixels) now and where the
    /// brush's size starts, the opacity slider (0..1).
    pub colour: [f32; 3],
    pub secondary: [f32; 3],
    pub diameter: f32,
    pub start_diameter: f32,
    pub opacity: f32,
    /// 0 paint, 1 erase, 2 smudge; 0 finger, 1 pen, 2 mouse.
    pub tool: u8,
    pub pointer: u8,
    pub seed: u32,
    /// How far the canvas is turned on screen (radians).
    pub turn: f32,
    /// The layer the stroke is laid on and where it starts (canvas pixels), for an engine that
    /// wants the colours there; none where it isn't a stroke of the pen's.
    pub under_at: Option<(procraft_engine::document::LayerId, f32, f32)>,
}

/// A look being made on a thread of its own, which those asking for it may wait on.
#[derive(Default)]
pub(crate) struct Making {
    done: Mutex<bool>,
    ended: std::sync::Condvar,
}

impl Making {
    fn wait(&self) {
        let mut done = self.done.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*done {
            done = self.ended.wait(done).unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn end(&self) {
        *self.done.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.ended.notify_all();
    }
}

/// The look of `brush` drawn by its engine, if that is on (asked of it once for its data, then
/// kept, in memory and on the disk), and the id it goes by. A look that can't be had (too much for
/// its call, or one the app can't take) isn't asked for again, and the brush draws as the app read
/// it.
///
/// Making a brush's look decodes every picture it uses, once for good (the app keeps it): the last
/// of bringing the brush in, which can take seconds. So it goes as an import does, on a copy of the
/// plugin's code with an import's limits (see [`ImportLimits`]), one too much even for those
/// leaving the plugin on; and on a thread of its own, once however many ask. Unless `wait`, a look
/// not had yet is none: a stroke never waits (the interface's thread starts it), and draws as the
/// app read the brush meanwhile; a preview, drawn off the interface's threads, waits. Its strokes
/// go on the plugin's own code, with what drawing always had.
pub(crate) fn look_of(hooks: &Arc<Hooks>, brush: &Brush, wait: bool) -> Option<(Arc<BrushLook>, u64)> {
    let outside = brush.engine.as_ref()?;
    let (plugin, running, index) = hooks.engine(&outside.engine)?;
    let id = data_id(&outside.data);
    if let Some(look) = hooks.look_kept(&outside.engine, id) {
        return Some((look, id));
    }
    if hooks.has_no_look(&outside.engine, id) {
        return None;
    }
    // kept on the disk by an earlier start (one that doesn't hold up is asked for again)
    let kept = hooks.look_on_disk(&plugin, &outside.engine, id).and_then(|bytes| look_kept_as(&bytes, id));
    if let Some(look) = kept {
        let look = Arc::new(look);
        hooks.keep_look(&outside.engine, id, Arc::clone(&look));
        return Some((look, id));
    }
    let Some(copy) = hooks.copier(&plugin) else {
        // (code put on its hooks alone, as tests do: made here, on that code, which stops if it fails)
        let answer = at_work(&running).brush_load(index, &load_record(id, &outside.data, false));
        return made_look(hooks, &plugin, &outside.engine, id, &brush.name, answer);
    };
    let (making, starts) = hooks.making_look(&outside.engine, id)?;
    if starts {
        let (on, key, data, name, made) = (
            Arc::clone(hooks),
            outside.engine.clone(),
            Arc::clone(&outside.data),
            brush.name.clone(),
            Arc::clone(&making),
        );
        let thread = std::thread::Builder::new().name("brush look".into());
        let started = thread.spawn(move || {
            let limits = LOOK_LIMITS.memory(ImportLimits::now().memory);
            let load = load_record(id, &data, false);
            let answer = copy(limits.memory)
                .and_then(|mut copied| copied.brush_load_within(index, &load, (limits.steps, limits.time)));
            match answer {
                Ok(answer) => {
                    made_look(&on, &plugin, &key, id, &name, Ok(answer));
                }
                Err(stop) => {
                    log::warn!("{key} couldn't make the look of {name}: {stop:?}");
                    on.no_look(&key, id);
                }
            }
            on.made_look(&key, id);
            made.end();
        });
        if started.is_err() {
            hooks.no_look(&outside.engine, id);
            hooks.made_look(&outside.engine, id);
            making.end();
        }
    }
    if !wait {
        return None;
    }
    making.wait();
    look_of(hooks, brush, false)
}

/// What engine `key` of plugin `plugin` answered asked for the look of the brush whose data is
/// `id` (`name`): kept, in memory and on the disk, if it holds up; one that doesn't, or a refusal,
/// not asked for again; a stop of the plugin's own code switches it off.
fn made_look(
    hooks: &Hooks,
    plugin: &str,
    key: &str,
    id: u64,
    name: &str,
    answer: Result<Option<Vec<u8>>, Stop>,
) -> Option<(Arc<BrushLook>, u64)> {
    let record = match answer {
        Ok(Some(record)) => record,
        // (refused)
        Ok(None) => {
            hooks.no_look(key, id);
            return None;
        }
        Err(stop) => {
            hooks.stopped(plugin, stop);
            return None;
        }
    };
    let Some(look) = read_look(&record, id) else {
        log::warn!("a brush engine's look of {name} doesn't hold up");
        hooks.no_look(key, id);
        return None;
    };
    hooks.keep_look_on_disk(plugin, key, id, &kept_look(&look));
    let look = Arc::new(look);
    hooks.keep_look(key, id, Arc::clone(&look));
    Some((look, id))
}

/// A stroke of `brush` drawn by its engine: the halves its first and second strokes take; none if
/// its engine is off or doesn't start it.
pub(crate) fn begin_stroke(
    hooks: &Arc<Hooks>,
    brush: &Brush,
    look: Arc<BrushLook>,
    id: u64,
    starting: Starting,
    under: Option<(i32, i32, u32, Vec<u8>)>,
    (width, height): (u32, u32),
) -> Option<(EngineHalf, EngineHalf)> {
    let outside = brush.engine.as_ref()?;
    let (plugin, running, index) = hooks.engine(&outside.engine)?;
    let mut record = Record::default();
    let floats = |v: &[f32]| -> Vec<u8> { v.iter().flat_map(|f| f.to_le_bytes()).collect() };
    record
        .field(field::BEGIN_BRUSH, &id.to_le_bytes())
        .field(field::BEGIN_COLOUR, &floats(&starting.colour))
        .field(field::BEGIN_SECONDARY, &floats(&starting.secondary))
        .field(field::BEGIN_DIAMETER, &floats(&[starting.diameter]))
        .field(field::BEGIN_START_DIAMETER, &floats(&[starting.start_diameter]))
        .field(field::BEGIN_OPACITY, &floats(&[starting.opacity]))
        .field(field::BEGIN_TOOL, &[starting.tool])
        .field(field::BEGIN_POINTER, &[starting.pointer])
        .field(field::BEGIN_SEED, &starting.seed.to_le_bytes())
        .field(field::BEGIN_TURN, &floats(&[starting.turn]))
        .field(field::BEGIN_CANVAS, &floats(&[width as f32, height as f32]));
    if let Some((x0, y0, side, pixels)) = under {
        let mut bytes = Vec::with_capacity(12 + pixels.len());
        bytes.extend_from_slice(&x0.to_le_bytes());
        bytes.extend_from_slice(&y0.to_le_bytes());
        bytes.extend_from_slice(&side.to_le_bytes());
        bytes.extend_from_slice(&pixels);
        record.field(field::BEGIN_UNDER, &bytes);
    }
    let record = record.bytes();
    let begun = (|| {
        let mut code = at_work(&running);
        if let Some(number) = code.brush_begin(&record)? {
            return Ok(Some(number));
        }
        // its brush first (its look is held), then again
        if code.brush_load(index, &load_record(id, &outside.data, true))?.is_none() {
            return Ok(None);
        }
        code.brush_begin(&record)
    })();
    let number = match begun {
        Ok(number) => number?,
        Err(stop) => {
            hooks.stopped(&plugin, stop);
            return None;
        }
    };
    let core = Arc::new(Mutex::new(StrokeCore {
        hooks: Arc::clone(hooks),
        plugin,
        running,
        number: Some(number),
        colour: starting.colour,
        look,
        first_ms: None,
        fed: 0,
        ended: false,
        laid: [VecDeque::new(), VecDeque::new()],
        layer: starting.under_at.map(|(layer, ..)| layer),
        asked: Vec::new(),
        told: Vec::new(),
    }));
    let half = |second| EngineHalf { core: Arc::clone(&core), second, seen: 0 };
    Some((half(false), half(true)))
}

/// The open stroke an engine draws, as the app tells it the colours under the dabs it asks about
/// (see [`crate::State::tell_under`]).
#[derive(Clone, Debug, Default)]
pub(crate) struct Asking(std::sync::Weak<Mutex<StrokeCore>>);

impl EngineHalf {
    /// Its stroke, to be told the colours under its dabs.
    pub(crate) fn asking(&self) -> Asking {
        Asking(Arc::downgrade(&self.core))
    }
}

/// The colours of `pixels` (premultiplied RGBA8, the canvas's `rect` of them) under `a`, weighed as
/// it asks, as four f32: premultiplied red, green and blue, and alpha (what lies off them clear).
fn colours_under(a: &Asked, pixels: &[u8], rect: procraft_engine::geom::IRect) -> [f32; 4] {
    let r = a.radius.max(0.5);
    let (x0, y0, side) = match a.weighing {
        // the square from its corner, as wide as the disc (whole pixels)
        1 => ((a.x - r) as i32, (a.y - r) as i32, ((2.0 * r) as i32).max(1)),
        _ => ((a.x - r).floor() as i32, (a.y - r).floor() as i32, (2.0 * r).ceil() as i32 + 1),
    };
    let (mut sum, mut total) = ([0.0_f64; 4], 0.0_f64);
    for py in y0..y0 + side {
        for px in x0..x0 + side {
            let (dx, dy) = (px as f32 + 0.5 - a.x, py as f32 + 0.5 - a.y);
            let rr = (dx * dx + dy * dy) / (r * r);
            let w = match a.weighing {
                1 => {
                    let (cx, cy) = (px as f32 - a.x, py as f32 - a.y);
                    if cx * cx + cy * cy <= (r + 1.0) * (r + 1.0) { ((1.0 - rr) * 255.0).round() / 255.0 } else { 1.0 }
                }
                _ if rr <= 1.0 => 1.0 - rr,
                _ => continue,
            };
            let w = f64::from(w);
            total += w;
            let inside = px >= rect.x0 && px < rect.x1 && py >= rect.y0 && py < rect.y1;
            if !inside {
                continue;
            }
            let at = (((py - rect.y0) * rect.width() as i32 + (px - rect.x0)) * 4) as usize;
            if let Some(p) = pixels.get(at..at + 4) {
                for (s, v) in sum.iter_mut().zip(p) {
                    *s += w * f64::from(*v) / 255.0;
                }
            }
        }
    }
    if total <= 0.0 {
        return [0.0; 4];
    }
    sum.map(|s| ((s / total) as f32).clamp(0.0, 1.0))
}

impl crate::State {
    /// The open stroke's engine told the colours under the dabs it has asked about, as the stroke
    /// has laid its layer by now: before its next samples, so that it sees them as near as the GPU
    /// allows (what it laid in the batch of samples with them, the dabs after them included).
    pub(crate) fn tell_under(&mut self) {
        let Some(core) = self.engine_asking.0.upgrade() else { return };
        let (asked, layer) = {
            let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
            (std::mem::take(&mut c.asked), c.layer)
        };
        let (Some(layer), false) = (layer, asked.is_empty()) else { return };
        let (Some(canvas), Some(gpu)) = (self.canvas.as_mut(), self.gpu.as_ref()) else { return };
        // what is still to be laid laid first
        canvas.render(gpu, &mut self.doc, None);
        let (width, height) = canvas.size();
        let full = procraft_engine::geom::IRect::from_size(width, height);
        let reach = |a: &Asked| {
            let r = a.radius.ceil() as i32 + 2;
            let (x, y) = (a.x.floor() as i32, a.y.floor() as i32);
            procraft_engine::geom::IRect::new(x - r, y - r, x + r + 1, y + r + 1)
        };
        // the pixels under all of them at once, unless that is too many (then each its own)
        let all = asked.iter().map(reach).reduce(|a, b| a.union(b)).unwrap_or(full).intersect(full);
        let mut told = Vec::with_capacity(asked.len() * 16);
        let together = i64::from(all.width()) * i64::from(all.height()) <= MOST_READ;
        let read_all = if together { canvas.read_layer_part(gpu, layer, all).ok() } else { None };
        for a in &asked {
            let colours = match &read_all {
                Some(pixels) => colours_under(a, pixels, all),
                None => {
                    let rect = reach(a).intersect(full);
                    canvas.read_layer_part(gpu, layer, rect).map_or([0.0; 4], |p| colours_under(a, &p, rect))
                }
            };
            told.extend(colours.iter().flat_map(|c| c.to_le_bytes()));
        }
        core.lock().unwrap_or_else(|e| e.into_inner()).told.extend(told);
    }
}

/// A stroke an engine draws: what both its halves share.
struct StrokeCore {
    hooks: Arc<Hooks>,
    plugin: String,
    running: Shared,
    /// Its number with the engine, until it ends or the plugin stops.
    number: Option<i32>,
    colour: [f32; 3],
    look: Arc<BrushLook>,
    /// When its first sample came (ms).
    first_ms: Option<f64>,
    /// How many samples have gone to the engine, and whether it has ended.
    fed: usize,
    ended: bool,
    /// The stamps each half has yet to take: the first stroke's, the second's.
    laid: [VecDeque<Stamp>; 2],
    /// The layer it is laid on, if the colours under its dabs can be read there; the dabs it has
    /// asked about since it was last told, and what it is to be told before its next samples.
    layer: Option<procraft_engine::document::LayerId>,
    asked: Vec<Asked>,
    told: Vec<u8>,
}

/// A dab a stroke asked about: where (canvas pixels), its radius and how the colours are weighed
/// (0: the disc, each pixel by 1 - (d / r)^2 at its middle; 1: the square from the corner, the
/// pixels whose corners lie within a pixel past the rim so, less than nothing past it, the rest
/// fully).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Asked {
    x: f32,
    y: f32,
    radius: f32,
    weighing: u32,
}

impl StrokeCore {
    /// `samples` handed to the engine, its dabs kept for the halves.
    fn feed(&mut self, samples: &[Sample]) {
        // a few at a time, so that an engine laying many dabs for each (a line for each of a
        // bristle brush's bristles) stays within what a call may answer
        // what it asked about first
        if !self.told.is_empty()
            && let Some(number) = self.number
        {
            let told = std::mem::take(&mut self.told);
            if let Err(stop) = at_work(&self.running).brush_under(number, &told) {
                self.number = None;
                self.hooks.stopped(&self.plugin, stop);
                return;
            }
        }
        for some in samples.chunks(SAMPLES_A_CALL) {
            self.feed_some(some);
        }
    }

    fn feed_some(&mut self, samples: &[Sample]) {
        let Some(number) = self.number else { return };
        let first = *self.first_ms.get_or_insert(samples.first().map_or(0.0, |s| s.time_ms));
        let mut bytes = Vec::with_capacity(samples.len() * 32);
        for s in samples {
            for v in [s.pos.x, s.pos.y, s.pressure, s.tilt, s.azimuth, s.roll] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes.extend_from_slice(&(s.time_ms - first).max(0.0).to_le_bytes());
        }
        let dabs = at_work(&self.running).brush_add(number, &bytes);
        self.take(dabs);
    }

    fn end(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        let Some(number) = self.number else { return };
        let dabs = at_work(&self.running).brush_end(number);
        self.number = None;
        self.take(dabs);
    }

    fn take(&mut self, dabs: Result<Vec<u8>, Stop>) {
        let dabs = match dabs {
            Ok(dabs) => dabs,
            Err(stop) => {
                // off, and the stroke ends where it got to
                self.number = None;
                self.hooks.stopped(&self.plugin, stop);
                return;
            }
        };
        let first_ms = self.first_ms.unwrap_or(0.0);
        for dab in dabs.as_chunks::<DAB_BYTES>().0 {
            let flags = u32_of(dab.get(32..).unwrap_or_default()).unwrap_or(0);
            if flags & TELLS_UNDER != 0 {
                let word = |i: usize| u32_of(dab.get(i * 4..).unwrap_or_default()).unwrap_or(0);
                let [x, y, diameter] = [0, 1, 2].map(|i| f32::from_bits(word(i)));
                if self.layer.is_some()
                    && self.asked.len() < MOST_ASKED
                    && x.is_finite()
                    && y.is_finite()
                    && diameter.is_finite()
                {
                    let radius = (diameter * 0.5).clamp(0.5, 1024.0);
                    self.asked.push(Asked { x, y, radius, weighing: word(7).min(1) });
                }
                continue;
            }
            let second = flags & FOR_SECOND != 0;
            let look = &*self.look;
            let (tips, build) = match (second, &look.second) {
                (false, _) => (look.tips.layers.len(), look.build),
                (true, Some(s)) => (s.tips.layers.len(), s.build),
                (true, None) => continue,
            };
            if let (Some(stamp), Some(laid)) =
                (stamp_of(dab, self.colour, tips, build, first_ms), self.laid.get_mut(usize::from(second)))
            {
                laid.push_back(stamp);
            }
        }
    }
}

impl Drop for StrokeCore {
    fn drop(&mut self) {
        // the engine lets go of a stroke the app let go of unended
        if let Some(number) = self.number.take() {
            let _ = at_work(&self.running).brush_end(number);
        }
    }
}

/// A dab (see the module's notes) as a stamp of a stroke whose brush's colour is `colour`, whose
/// tips number `tips` and whose stamps `build` up so: none if it isn't one.
fn stamp_of(dab: &[u8; DAB_BYTES], colour: [f32; 3], tips: usize, build: Build, first_ms: f64) -> Option<Stamp> {
    let word = |i: usize| -> [u8; 4] { dab.get(i * 4..i * 4 + 4).and_then(|b| b.try_into().ok()).unwrap_or([0; 4]) };
    let float = |i: usize| f32::from_le_bytes(word(i));
    let [x, y, diameter, angle, ratio, opacity, flow] = [0, 1, 2, 3, 4, 5, 6].map(float);
    let (tip, flags) = (u32::from_le_bytes(word(7)), u32::from_le_bytes(word(8)));
    let [r, g, b, strength, tx, ty, lightness, threshold, softness, share, radius, w20, w21, w22] =
        [9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22].map(float);
    let w23 = u32::from_le_bytes(word(23));
    if ![x, y, diameter].iter().all(|v| v.is_finite()) || diameter <= 0.0 || tips == 0 {
        return None;
    }
    let unit = |v: f32, none: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { none };
    let (opacity, flow) = (unit(opacity, 1.0), unit(flow, 1.0));
    let center = Vec2::new(x, y);
    let mut s = Stamp::round(center, (diameter * 0.5).min(16_384.0), 1.0);
    (s.alpha, s.transfer) = match build {
        Build::Over => (opacity * flow, 1.0),
        Build::Towards => (flow, opacity),
    };
    s.angle = if angle.is_finite() { angle } else { 0.0 };
    s.roundness = if ratio.is_finite() { ratio.clamp(0.05, 1.0) } else { 1.0 };
    s.flip = [flags & FLIP_ACROSS != 0, flags & FLIP_DOWN != 0];
    s.tip = tip.min(tips as u32 - 1);
    if flags & OWN_COLOUR != 0 {
        s.tint = tint_to(colour, [unit(r, 0.0), unit(g, 0.0), unit(b, 0.0)]);
    }
    // the texture read where the dab lands on the canvas, moved as far as the engine says
    let offset = if tx.is_finite() && ty.is_finite() { Vec2::new(tx, ty) } else { Vec2::ZERO };
    s.grain_offset = center + offset;
    s.grain_depth = unit(strength, 1.0);
    s.lightness = unit(lightness, 1.0);
    if flags & MOVES_PAINT != 0 {
        let own = flags & OWN_COLOUR != 0;
        s.pick_up = Some(procraft_engine::stroke::PickUp {
            paint_share: unit(share, 0.0),
            paint: own.then(|| [unit(r, 0.0), unit(g, 0.0), unit(b, 0.0)]),
            dull: (flags & DULLS != 0)
                .then(|| (if radius.is_finite() { radius.clamp(0.0, 3.0) } else { 0.0 }, flags & WEIGHED != 0)),
            clear: flags & CLEAR_PAINT != 0,
            thin: if own && flags & SEE_THROUGH != 0 && flags & (WARPS | FILTERS) == 0 {
                1.0 - unit(w22, 1.0)
            } else {
                0.0
            },
            height: (flags & LAYS_HEIGHT != 0).then(|| unit(w20, 0.0)),
            over: flags & LAID_OVER != 0,
            here: flags & PICKS_HERE != 0,
            from: (flags & PICKS_FROM != 0 && w20.is_finite() && w21.is_finite()).then_some([w20, w21]),
            before: flags & PICKS_BEFORE != 0,
            warp: (flags & WARPS != 0).then(|| warp_of(w23, w22, [w20, w21])).flatten(),
            blur: if flags & FILTERS != 0 && flags & WARPS == 0 { unit(w22, 0.0) } else { 0.0 },
            sharpen: if flags & FILTERS != 0 && flags & WARPS == 0 {
                f32::from_bits(w23).clamp(0.0, 16.0)
            } else {
                0.0
            },
        });
    }
    // made sharp, if it says (a threshold of 0 says not)
    s.threshold =
        if threshold.is_finite() && threshold > 0.0 { [threshold.min(1.0), unit(softness, 0.0)] } else { [0.0; 2] };
    s.time_ms = first_ms;
    Some(s)
}

/// A dab's warp: its kind (the low four bits; 16 added for the nearest pixel), amount and shift.
fn warp_of(kind: u32, amount: f32, shift: [f32; 2]) -> Option<procraft_engine::stroke::Warp> {
    use procraft_engine::stroke::{Warp, WarpKind};
    let nearest = kind & 16 != 0;
    let kind = match kind & 15 {
        1 => WarpKind::Scale,
        2 => WarpKind::Turn,
        3 => WarpKind::Shift,
        4 => WarpKind::LensOut,
        5 => WarpKind::LensIn,
        6 => WarpKind::Scatter,
        _ => return None,
    };
    let finite = |v: f32| if v.is_finite() { v.clamp(-4096.0, 4096.0) } else { 0.0 };
    Some(Warp { kind, amount: finite(amount), shift: shift.map(finite), nearest })
}
fn hsb([r, g, b]: [f32; 3]) -> [f32; 3] {
    let hi = r.max(g).max(b);
    let d = hi - r.min(g).min(b);
    if d <= 1e-5 {
        return [0.0, 0.0, hi];
    }
    let h = if hi == r {
        (g - b) / d
    } else if hi == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [(h / 6.0).rem_euclid(1.0), d / hi, hi]
}

/// The tint taking `from` to `to` exactly: the hue turned to it, the saturation and brightness set.
fn tint_to(from: [f32; 3], to: [f32; 3]) -> Tint {
    let ([h0, ..], [h1, s1, b1]) = (hsb(from), hsb(to));
    Tint { hue: h1 - h0, saturation: [0.0, s1], brightness: [0.0, b1] }
}

/// One of a stroke's two halves (its first stroke, or its second), as the builder of each takes its
/// stamps: the samples go to the engine once, whichever half has them first.
#[derive(Debug)]
pub(crate) struct EngineHalf {
    core: Arc<Mutex<StrokeCore>>,
    second: bool,
    /// How many samples this half has been handed.
    seen: usize,
}

impl std::fmt::Debug for StrokeCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrokeCore").field("plugin", &self.plugin).field("number", &self.number).finish_non_exhaustive()
    }
}

impl EngineHalf {
    fn core(&self) -> MutexGuard<'_, StrokeCore> {
        self.core.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl OutsideEngine for EngineHalf {
    fn add(&mut self, samples: &[Sample]) -> Vec<Stamp> {
        let seen = self.seen + samples.len();
        let second = self.second;
        let mut core = self.core();
        // what the other half hasn't handed over yet
        if core.fed < seen {
            let new = samples.get(samples.len().saturating_sub(seen - core.fed)..).unwrap_or_default();
            core.feed(new);
            core.fed = seen;
        }
        let laid = core.laid.get_mut(usize::from(second)).map(std::mem::take).unwrap_or_default();
        drop(core);
        self.seen = seen;
        laid.into()
    }

    fn finish(&mut self) -> Vec<Stamp> {
        let second = self.second;
        let mut core = self.core();
        core.end();
        core.laid.get_mut(usize::from(second)).map(std::mem::take).unwrap_or_default().into()
    }
}

/// A stroke a plugin's engine draws: how its first stroke and its second (if it has one) are laid,
/// and the halves of the engine's stroke each builder takes.
pub(crate) struct OutsideStroke {
    /// Every how many milliseconds its engine wants the pen again while it holds still.
    pub airbrush: Option<f32>,
    /// What its look keeps with the layer between strokes (a layer store's kind, 0 none): its style
    /// paints into that only where the caller says (a plain layer's stroke of the user's own).
    pub store: u32,
    pub style: StrokeStyle,
    pub second: Option<SecondStroke>,
    pub first: EngineHalf,
    pub second_half: EngineHalf,
}

/// How a stroke of a look is laid on `canvas` with `tool`, as `starting` says (its colours and
/// opacity): its style and its second's, its tips and texture on the GPU. None if the GPU can't
/// hold its tips.
fn look_styles(
    look: &BrushLook,
    tool: StrokeTool,
    starting: &Starting,
    (alpha_lock, preview): (bool, bool),
    canvas: &mut procraft_gpu::Canvas,
    gpu: &procraft_gpu::Gpu,
) -> Option<(StrokeStyle, Option<SecondStroke>)> {
    // a brush whose dabs move paint smudges with the paint tool (an erasing one taking paint away
    // as it does), a brush that erases erases with it (its preview showing what it takes away or
    // moves: there is nothing under it there)
    let tool = match tool {
        StrokeTool::Paint if look.smudges && !preview => StrokeTool::Smudge,
        StrokeTool::Paint if look.erases && !preview => StrokeTool::Erase,
        other => other,
    };
    let paint = |build: Build, opacity: f32| {
        let mut style = StrokeStyle::new(tool, starting.colour, opacity);
        style.preserve_alpha = alpha_lock;
        style.secondary = starting.secondary;
        style.preview = preview;
        style.wetness = StrokeStyle::wetness_at(opacity);
        style.rendering = match build {
            Build::Over => Rendering::BuildUp,
            Build::Towards => Rendering::Capped,
        };
        style
    };
    let mut style = paint(look.build, starting.opacity);
    if tool == StrokeTool::Smudge && look.smudges {
        // what each stamp picks up behind it, laid as it says: nothing kept, nothing blurred; from
        // the image as shown if it says
        style.pull = 0.0;
        style.blur = 0.0;
        style.rendering = Rendering::BuildUp;
        style.from_image = look.from_image;
    }
    if let Some(blend) = look.blend {
        style.blend = blend.min(procraft_engine::brush::BRUSH_BEHIND);
    }
    let mut second = look.second.as_ref().map(|s| SecondStroke {
        style: paint(s.build, 1.0).as_second(),
        mode: s.combine,
        threshold: -1.0,
        classic: false,
    });
    canvas.use_tip_set(gpu, &mut style, look.tips.id, look.tips.side, &layers(&look.tips), look.tips.kind);
    // tips the GPU can't hold leave the brush as the app read it
    style.full_shape?;
    if let Some(t) = &look.texture {
        canvas.use_full_grain(gpu, &mut style, t.id, t.side, &t.pixels);
        style.grain_mode = procraft_engine::brush::GrainMode::Moving;
        style.grain_scale = t.tile.max(1.0);
        style.grain_depth = 1.0;
        style.grain_blend = t.mode;
    }
    if let (Some(s), Some(look)) = (second.as_mut(), look.second.as_ref()) {
        canvas.use_tip_set(gpu, &mut s.style, look.tips.id, look.tips.side, &layers(&look.tips), TipKind::Mask);
        if s.style.full_shape.is_none() {
            second = None;
        }
    }
    Some((style, second))
}

/// A stroke of `brush` drawn by the brush engine that made it, if that is on, on `canvas` with
/// `tool`, as `starting` says (`look`: the layer's alpha lock, and whether it is a preview's); one
/// whose look isn't had yet is none unless `wait` (see [`look_of`]).
pub(crate) fn engine_stroke(
    hooks: &Arc<Hooks>,
    brush: &Brush,
    tool: StrokeTool,
    starting: Starting,
    look: (bool, bool),
    canvas: &mut procraft_gpu::Canvas,
    gpu: &procraft_gpu::Gpu,
    wait: bool,
) -> Option<OutsideStroke> {
    brush.engine.as_ref()?;
    let (brush_look, id) = look_of(hooks, brush, wait)?;
    let (style, second) = look_styles(&brush_look, tool, &starting, look, canvas, gpu)?;
    let airbrush = brush_look.airbrush;
    let store = if style.tool == StrokeTool::Smudge { brush_look.store } else { 0 };
    // the colours under where it starts, if the engine wants them
    let under = starting.under_at.filter(|_| brush_look.under_start > 0).and_then(|(layer, x, y)| {
        let side = brush_look.under_start as i32;
        let (x0, y0) = (x.round() as i32 - side / 2, y.round() as i32 - side / 2);
        let rect = procraft_engine::geom::IRect::new(x0, y0, x0 + side, y0 + side);
        let (width, height) = canvas.size();
        let got = rect.intersect(procraft_engine::geom::IRect::from_size(width, height));
        let pixels = canvas.read_layer_part(gpu, layer, got).ok()?;
        let mut square = vec![0_u8; (side * side * 4) as usize];
        for (row, line) in pixels.chunks_exact((got.width() * 4) as usize).enumerate() {
            for (column, p) in line.as_chunks::<4>().0.iter().enumerate() {
                let (sx, sy) = (got.x0 - x0 + column as i32, got.y0 - y0 + row as i32);
                let at = ((sy * side + sx) * 4) as usize;
                // straight colours
                let a = u32::from(p[3]);
                let straight = |c: u8| (u32::from(c) * 255 + a / 2).checked_div(a).map_or(0, |v| v.min(255) as u8);
                if let Some(out) = square.get_mut(at..at + 4) {
                    out.copy_from_slice(&[straight(p[0]), straight(p[1]), straight(p[2]), p[3]]);
                }
            }
        }
        Some((x0, y0, side as u32, square))
    });
    let (width, height) = canvas.size();
    let (first, second_half) = begin_stroke(hooks, brush, brush_look, id, starting, under, (width, height))?;
    Some(OutsideStroke { airbrush, store, style, second, first, second_half })
}

impl crate::State {
    /// `builder` (not fed yet) drawn by its brush's engine as a fresh stroke seeded with `seed`, if
    /// the brush is an engine's and that is on (a builder's copy draws as the app reads the brush).
    pub(crate) fn engine_again(
        &mut self,
        builder: procraft_engine::stroke::StrokeBuilder,
        layer: procraft_engine::document::LayerId,
        seed: u32,
    ) -> procraft_engine::stroke::StrokeBuilder {
        if builder.brush().engine.is_none() || builder.is_outside() {
            return builder;
        }
        let brush = builder.brush().clone();
        let pointer = builder.pointer().into();
        match self.outside_stroke(&brush, self.tool, layer, pointer, builder.diameter(), seed, None) {
            Some(o) => builder.with_engine(Box::new(o.first)),
            None => builder,
        }
    }

    /// A stroke of `brush` with `tool` on `layer`, drawn by the brush engine that made the brush if
    /// that is on, at `diameter` canvas pixels (the size slider's, the brush scale in it): its
    /// style laid by what the engine says the brush looks like, its tips and texture on the GPU.
    /// None draws it as the app read it.
    pub(crate) fn outside_stroke(
        &mut self,
        brush: &Brush,
        tool: crate::ToolKind,
        layer: procraft_engine::document::LayerId,
        pointer: crate::PointerKind,
        diameter: f32,
        seed: u32,
        first: Option<(f32, f32)>,
    ) -> Option<OutsideStroke> {
        brush.engine.as_ref()?;
        let hooks = self.hooks.clone()?;
        let starting = Starting {
            colour: self.color,
            secondary: self.secondary,
            diameter,
            start_diameter: brush.diameter(self.starting_sliders(tool).size),
            opacity: self.sliders(tool).opacity.clamp(0.0, 1.0),
            tool: match tool.stroke_tool() {
                StrokeTool::Erase => 1,
                StrokeTool::Smudge => 2,
                _ => 0,
            },
            pointer: match pointer {
                crate::PointerKind::Finger => 0,
                crate::PointerKind::Mouse => 2,
                _ => 1,
            },
            seed,
            turn: self.view.map_or(0.0, |v| v.rotation()),
            under_at: first.map(|(x, y)| (layer, x, y)),
        };
        let look = (self.doc.layer(layer).is_some_and(|l| l.alpha_lock), self.pad_strokes.is_some());
        let (canvas, gpu) = (self.canvas.as_mut()?, self.gpu.as_ref()?);
        // an engine wanting the colours under where the stroke starts reads the layer as it is now,
        // what is still to be laid on it laid first
        if first.is_some() && look_of(&hooks, brush, false).is_some_and(|(l, _)| l.under_start > 0) {
            canvas.render(gpu, &mut self.doc, None);
        }
        // (never waiting for a look being made: this is the interface's thread)
        let mut stroke = engine_stroke(&hooks, brush, tool.stroke_tool(), starting, look, canvas, gpu, false);
        // a stroke of the pen's on the canvas is told the colours under the dabs it asks about
        if first.is_some() {
            self.engine_asking = stroke.as_ref().map(|o| o.first.asking()).unwrap_or_default();
        }
        // what its look keeps with the layer, on a layer of plain pixels
        let plain = self
            .doc
            .layer(layer)
            .is_some_and(|l| l.kind == procraft_engine::document::LayerKind::Paint && !l.drawn_from_data());
        if let Some(o) = stroke.as_mut().filter(|_| plain) {
            o.style.store = o.store;
        }
        stroke
    }
}

#[uniffi::export]
impl CanvasSession {
    /// The plugin action that edits the current brush in place of the brush studio: the brush
    /// studio of the brush engine that made it, while that is on. None opens the brush studio as
    /// ever. The plugin's studio then reads and changes that brush (see the `brush.data`
    /// permission) until another opens.
    pub fn brush_studio_action(&self) -> Option<String> {
        let mut st = self.shared.lock();
        let tool = st.tool;
        let at = st.brush_ref(tool);
        let engine = st.brush(tool).engine.as_ref()?.engine.clone();
        let action = st.hooks.as_ref()?.engine_studio(&engine)?;
        st.engine_studio = Some(at);
        Some(action)
    }

    /// Whether a plugin changed a brush of the library, or gave it the set of brushes it comes with,
    /// since this was last asked (the library to be read again and kept).
    pub fn take_brushes_changed(&self) -> bool {
        std::mem::take(&mut self.shared.lock().brushes_changed)
    }
}

impl CanvasSession {
    /// The brush a plugin's studio edits (else the current brush), if the brush engine of plugin
    /// `owner` made it.
    fn edited_brush(st: &crate::State, owner: &str) -> Option<(usize, usize)> {
        let at = st.engine_studio.unwrap_or_else(|| st.brush_ref(st.tool));
        let brush = st.library.get(at.0)?.brushes.get(at.1)?;
        let engine = brush.engine.as_ref()?;
        engine.engine.strip_prefix(owner)?.starts_with('.').then_some(at)
    }

    /// The data of the brush plugin `owner`'s studio edits, as its engine keeps it.
    pub(crate) fn edited_brush_data(&self, owner: &str) -> Option<Arc<[u8]>> {
        let st = self.shared.lock();
        let (set, index) = Self::edited_brush(&st, owner)?;
        let brush = st.library.get(set)?.brushes.get(index)?;
        brush.engine.as_ref().map(|e| Arc::clone(&e.data))
    }

    /// The brush plugin `owner`'s studio edits keeps `data` from now on; false if there is none.
    pub(crate) fn set_edited_brush_data(&self, owner: &str, data: Vec<u8>) -> bool {
        let mut st = self.shared.lock();
        let Some((set, index)) = Self::edited_brush(&st, owner) else { return false };
        let Some(brush) = st.library.get_mut(set).and_then(|s| s.brushes.get_mut(index)) else { return false };
        let Some(engine) = brush.engine.as_mut() else { return false };
        engine.data = data.into();
        // its look, new with its data, made now on a thread of its own, for its next stroke
        let changed = brush.clone();
        if let Some(hooks) = st.hooks.clone() {
            look_of(&hooks, &changed, false);
        }
        st.brushes_changed = true;
        true
    }
}

#[cfg(test)]
impl CanvasSession {
    /// Waits for the look of the current tool's brush, if a plugin's brush engine is making it (a
    /// stroke started before draws as the app read the brush).
    pub(crate) fn wait_for_look(&self) {
        let (hooks, brush) = {
            let st = self.shared.lock();
            (st.hooks.clone(), st.brush(st.tool).clone())
        };
        if let Some(hooks) = hooks {
            look_of(&hooks, &brush, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tip(w: u32, h: u32) -> Vec<u8> {
        let mut t = Record::default();
        t.field(field::TIP_WIDTH, &w.to_le_bytes())
            .field(field::TIP_HEIGHT, &h.to_le_bytes())
            .field(field::TIP_PIXELS, &vec![255; (w * h) as usize]);
        t.bytes()
    }

    /// A Krita preset of a hard square tip 20 pixels across, spaced 2 diameters apart: a PNG with
    /// only its settings in it.
    pub(crate) fn square_preset() -> Vec<u8> {
        preset_of("")
    }

    /// The square preset with `more` params besides.
    fn preset_of(more: &str) -> Vec<u8> {
        let xml = format!(
            r#"<Preset name="Squares" paintopid="paintbrush"><param name="brush_definition" type="string"><![CDATA[<Brush type="auto_brush" spacing="2" useAutoSpacing="0" angle="0" density="1" randomness="0"><MaskGenerator type="rect" id="default" diameter="20" ratio="1" hfade="1" vfade="1" spikes="2" antialiasEdges="0"/></Brush>]]></param><param name="PaintOpAction" type="string"><![CDATA[1]]></param><param name="CompositeOp" type="string"><![CDATA[normal]]></param><param name="PressureSize" type="string"><![CDATA[false]]></param><param name="OpacityUseCurve" type="string"><![CDATA[false]]></param><param name="FlowUseCurve" type="string"><![CDATA[false]]></param>{more}</Preset>"#
        );
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(kind);
            c.extend_from_slice(data);
            // the reader doesn't check sums
            c.extend_from_slice(&[0; 4]);
            c
        };
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = Vec::new();
        for v in [1_u32, 1] {
            header.extend_from_slice(&v.to_be_bytes());
        }
        header.extend_from_slice(&[8, 0, 0, 0, 0]);
        png.extend(chunk(b"IHDR", &header));
        let mut text = b"preset\0".to_vec();
        text.extend_from_slice(xml.as_bytes());
        png.extend(chunk(b"tEXt", &text));
        png.extend(chunk(b"IEND", &[]));
        png
    }

    /// The Krita plugin the app comes with (or the one at `KRITA_PLUGIN`) drawing a preset's squares
    /// where Krita lays them, its studio changing their size; with it off, the brush draws as read.
    #[test]
    fn a_dab_picks_up_from_elsewhere_through_a_warp_or_blurred_as_its_words_say() {
        let dab = |flags: u32, words: [u32; 4]| {
            let mut d = [0_u8; DAB_BYTES];
            let mut put = |i: usize, w: u32| d[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
            for (i, v) in [10.0_f32, 20.0, 8.0, 0.0, 1.0, 1.0, 1.0].iter().enumerate() {
                put(i, v.to_bits());
            }
            put(8, flags);
            for (i, w) in words.iter().enumerate() {
                put(20 + i, *w);
            }
            stamp_of(&d, [0.0; 3], 1, Build::Over, 0.0).unwrap().pick_up.unwrap()
        };
        let f = f32::to_bits;
        let clone = dab(MOVES_PAINT | PICKS_FROM | PICKS_BEFORE, [f(-30.0), f(4.0), 0, 0]);
        assert_eq!((clone.from, clone.before, clone.warp, clone.blur), (Some([-30.0, 4.0]), true, None, 0.0));
        let warp = dab(MOVES_PAINT | WARPS, [f(2.0), f(0.0), f(0.5), 3 + 16]).warp.unwrap();
        assert_eq!(
            warp,
            procraft_engine::stroke::Warp {
                kind: procraft_engine::stroke::WarpKind::Shift,
                amount: 0.5,
                shift: [2.0, 0.0],
                nearest: true
            }
        );
        let blurred = dab(MOVES_PAINT | FILTERS, [0, 0, f(0.25), f(1.5)]);
        assert_eq!((blurred.blur, blurred.sharpen, blurred.from), (0.25, 1.5, None));
        // without flag 16 none of it
        let mut plain = [0_u8; DAB_BYTES];
        plain[8..12].copy_from_slice(&8.0_f32.to_bits().to_le_bytes());
        plain[32..36].copy_from_slice(&(PICKS_FROM | WARPS).to_le_bytes());
        assert!(stamp_of(&plain, [0.0; 3], 1, Build::Over, 0.0).unwrap().pick_up.is_none());
        // its own colour see-through as word 22 says (not with a warp's or a blur's words)
        let own = MOVES_PAINT | OWN_COLOUR | PICKS_HERE;
        assert_eq!(dab(own | SEE_THROUGH, [0, 0, f(0.25), 0]).thin, 0.75);
        assert_eq!(dab(own, [0, 0, f(0.25), 0]).thin, 0.0);
        assert_eq!(dab(own | SEE_THROUGH | FILTERS, [0, 0, f(0.25), 0]).thin, 0.0);
    }

    #[test]
    fn the_colours_under_a_dab_asked_about_are_weighed_as_it_asks() {
        use procraft_engine::geom::IRect;
        // a 20 x 20 layer, its left half opaque red, its right half clear
        let rect = IRect::new(0, 0, 20, 20);
        let pixels: Vec<u8> =
            (0..400).flat_map(|i| if i % 20 < 10 { [255, 0, 0, 255] } else { [0, 0, 0, 0] }).collect();
        let disc = |x: f32, weighing: u32| colours_under(&Asked { x, y: 10.0, radius: 4.0, weighing }, &pixels, rect);
        assert_eq!(disc(5.0, 0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(disc(15.0, 0), [0.0; 4]);
        // across the edge: half and half, premultiplied
        let [r, g, _, a] = disc(10.0, 0);
        assert!((r - 0.5).abs() < 0.02 && g == 0.0 && (a - r).abs() < 1e-6, "{r} {a}");
        // the square from its corner, its corners counted fully: the same across the edge
        let [r, _, _, a] = disc(10.0, 1);
        assert!((r - 0.5).abs() < 0.05 && (a - r).abs() < 1e-6, "{r} {a}");
        // off the layer it is clear
        let off = colours_under(&Asked { x: -10.0, y: 10.0, radius: 4.0, weighing: 0 }, &pixels, rect);
        assert_eq!(off, [0.0; 4]);
        let half_off = colours_under(&Asked { x: 0.0, y: 10.0, radius: 4.0, weighing: 0 }, &pixels, rect);
        assert!((half_off[3] - 0.5).abs() < 0.05, "{half_off:?}");
    }

    #[test]
    fn krita_squares_lie_where_the_engine_spaces_them() {
        let packed = match std::env::var("KRITA_PLUGIN") {
            Ok(path) => std::fs::read(path).unwrap(),
            Err(_) => crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1.to_vec(),
        };
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let s = crate::CanvasSession::new(200, 120);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 120);
            st.view = Some(procraft_engine::view::View::fit((200, 120), (200, 120)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        let dir = std::env::temp_dir().join(format!("procraft-krita-plugin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        assert_eq!(plugins.install(packed), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id.clone(), true));
        s.use_plugins(Arc::clone(&plugins));
        // read off the interface's thread, the engine making its brushes there too, then added
        let read = crate::read_brushes(square_preset(), Some("Squares.kpp".to_owned())).unwrap();
        s.make_engine_brushes(Arc::clone(&read), None);
        let set = s.add_brushes_read(read, 0).unwrap();
        assert!(s.shared.lock().library[set as usize].brushes[0].engine.is_some(), "made by the engine too");
        assert!(s.select_brush(set, 0));
        s.wait_for_look();
        s.set_color(0.0, 0.0, 0.0);
        let draw = |s: &crate::CanvasSession| {
            let layer = s.add_layer();
            let at = |x: f32| crate::PenSample {
                x,
                y: 60.0,
                pressure: 1.0,
                tilt: 0.0,
                orientation: 0.0,
                time_ms: f64::from(x),
                roll: 0.0,
            };
            s.stroke_begin(crate::PointerKind::Stylus, vec![at(20.0)]);
            s.stroke_add((21..=180).map(|x| at(x as f32)).collect());
            s.stroke_end(vec![]);
            s.shared.lock().layer_pixels(procraft_engine::document::LayerId(layer)).unwrap()
        };
        let alpha = |rgba: &[u8], x: usize, y: usize| rgba[(y * 200 + x) * 4 + 3];
        let on = draw(&s);
        // a square at the start, 20 across; the next 40 on (twice its size), none between
        assert!(alpha(&on, 20, 60) > 240 && alpha(&on, 28, 52) > 200, "the first square");
        assert!(alpha(&on, 60, 60) > 240, "the next, 40 on");
        assert_eq!(alpha(&on, 40, 60), 0, "nothing between them");
        // held still, the line it drew snaps to a line, laid again by the engine: squares 40 apart
        let layer = s.add_layer();
        let at = |x: f32, t: f64| crate::PenSample {
            x,
            y: 60.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: t,
            roll: 0.0,
        };
        s.stroke_begin(crate::PointerKind::Stylus, vec![at(20.0, 0.0)]);
        s.stroke_add((21..=180).map(|x| at(x as f32, f64::from(x))).collect());
        assert!(s.quickshape_hold().is_some(), "a line");
        s.stroke_end(vec![]);
        s.quickshape_done();
        let shape = s.shared.lock().layer_pixels(procraft_engine::document::LayerId(layer)).unwrap();
        assert!(alpha(&shape, 20, 60) > 240 && alpha(&shape, 60, 60) > 240, "squares along the shape");
        assert_eq!(alpha(&shape, 40, 60), 0, "spaced as the engine spaces them");
        // its preview drawn by the engine: squares too, not the preset's picture
        let preview = s.brush_preview(set, 0, 400, 120).expect("a preview");
        assert!(preview.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
        // an eraser's preview shows what it takes away
        let read = crate::read_brushes(
            preset_of(r#"<param name="EraserMode"><![CDATA[true]]></param>"#),
            Some("Eraser.kpp".to_owned()),
        )
        .unwrap();
        s.make_engine_brushes(Arc::clone(&read), None);
        let erasers = s.add_brushes_read(read, 0).unwrap();
        let preview = s.brush_preview(erasers, 0, 400, 120).expect("an eraser's preview");
        assert!(preview.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0), "an eraser's preview draws");
        // written out as a brush file and read back, it keeps its engine and data
        let file = s.brush_as_file(set, 0).unwrap();
        let back = s.import_brushes(file).unwrap();
        let engine = |set: u32| s.shared.lock().library[set as usize].brushes[0].engine.clone();
        assert!(engine(back).is_some());
        assert_eq!(engine(back), engine(set));
        // its studio, the plugin's own: the tip made 40 across on its first page
        let studio = s.brush_studio_action().expect("its plugin's studio");
        assert!(plugins.run_action(studio, Arc::clone(&s)));
        let panel = plugins.panel().expect("the studio's panel");
        assert_eq!(panel.title, "Squares");
        let row = panel
            .rows
            .iter()
            .position(|r| matches!(r, crate::PanelRow::Slider { label, .. } if label == "Diameter"))
            .unwrap();
        assert!(plugins.panel_touched(row as u32, 40.0, Arc::clone(&s)));
        assert!(s.take_brushes_changed(), "the brush changed");
        assert!(!s.take_brushes_changed());
        s.wait_for_look();
        let bigger = draw(&s);
        assert!(alpha(&bigger, 38, 60) > 240, "the first square, 40 across");
        assert!(alpha(&bigger, 100, 60) > 240 && alpha(&bigger, 60, 60) == 0, "the next, 80 on");
        assert!(plugins.set_on(id, false));
        assert_eq!(s.brush_studio_action(), None, "with its plugin off, the brush studio as ever");
        // as the app read it: paint, the app's own way
        let off = draw(&s);
        assert!(off.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
        assert_ne!(off, bigger);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Krita plugin on draws its brushes through the app's Pressure and Smoothing, as the app
    /// draws any brush: the pressure curve sizes and lightens the dabs (the preset's own pressure
    /// sensors following what the curve makes of the pen), and stabilisation and motion filtering
    /// smooth a shaky line's path.
    #[test]
    fn a_krita_brush_takes_the_apps_pressure_and_smoothing() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let (w, h) = (400_usize, 160_usize);
        let s = crate::CanvasSession::new(w as u32, h as u32);
        {
            let mut st = s.shared.lock();
            st.window_size = (w as u32, h as u32);
            st.view = Some(procraft_engine::view::View::fit((w as u32, h as u32), (w as u32, h as u32)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        let dir = std::env::temp_dir().join(format!("procraft-krita-smoothing-{}", std::process::id()));
        let (plugins, ..) = krita_on(&dir);
        s.use_plugins(Arc::clone(&plugins));
        // a soft round tip 20 pixels across, sized and made opaque by the pressure, washed
        let round = r#"<Brush type="auto_brush" spacing="0.05" useAutoSpacing="0" angle="0" density="1" randomness="0"><MaskGenerator type="circle" id="default" diameter="20" ratio="1" hfade="1" vfade="1" spikes="2" antialiasEdges="1"/></Brush>"#;
        let more = r#"<param name="PaintOpAction" type="string"><![CDATA[2]]></param><param name="PressureSize" type="string"><![CDATA[true]]></param><param name="SizeSensor" type="string"><![CDATA[<params id="pressure"/>]]></param><param name="PressureOpacity" type="string"><![CDATA[true]]></param><param name="OpacitySensor" type="string"><![CDATA[<params id="pressure"/>]]></param>"#;
        let brush = preset_png(&format!(
            r#"<Preset name="Round" paintopid="paintbrush"><param name="brush_definition" type="string"><![CDATA[{round}]]></param>{more}</Preset>"#
        ));
        let read = crate::read_brushes(brush, Some("Round.kpp".to_owned())).unwrap();
        s.make_engine_brushes(Arc::clone(&read), None);
        let set = s.add_brushes_read(read, 0).unwrap();
        assert!(s.shared.lock().library[set as usize].brushes[0].engine.is_some(), "made by the engine");
        assert!(s.select_brush(set, 0));
        s.wait_for_look();
        s.set_color(0.0, 0.0, 0.0);
        // a shaky line along y = 80, 6 pixels off it either way three samples at a time, pressed
        // from nothing to all the way, at 240 samples a second
        let draw = |s: &crate::CanvasSession| {
            let layer = s.add_layer();
            let at = |i: usize| crate::PenSample {
                x: 40.0 + 3.0 * i as f32,
                y: 80.0 + if (i / 3).is_multiple_of(2) { 6.0 } else { -6.0 },
                pressure: i as f32 / 106.0,
                tilt: 0.0,
                orientation: 0.0,
                time_ms: 1000.0 + i as f64 * 1000.0 / 240.0,
                roll: 0.0,
            };
            s.stroke_begin(crate::PointerKind::Stylus, vec![at(0)]);
            for i in (1..=106).step_by(4) {
                s.stroke_add((i..(i + 4).min(107)).map(at).collect());
            }
            s.stroke_end(vec![]);
            s.shared.lock().layer_pixels(procraft_engine::document::LayerId(layer)).unwrap()
        };
        // a column's ink: how many pixels across, how far its middle lies off the line (none for a
        // column without ink), its darkest
        let column = |rgba: &[u8], x: usize| {
            let alphas: Vec<f32> = (0..h).map(|y| f32::from(rgba[(y * w + x) * 4 + 3])).collect();
            let across = alphas.iter().filter(|a| **a > 32.0).count();
            let total: f32 = alphas.iter().sum();
            let middle = alphas.iter().enumerate().map(|(y, a)| y as f32 * a).sum::<f32>() / total.max(1.0);
            let darkest = alphas.iter().fold(0.0_f32, |m, a| m.max(*a));
            (across, (total > 255.0).then_some((middle - 80.0).abs()), darkest)
        };
        // (pressed a fifth of the way at x 110, nearly all the way at 340; the shake measured from
        // a tenth to two thirds of the way, short of its end, which runs on to where the pen lifted)
        let measure = |what: &str, rgba: &[u8]| {
            let off: Vec<f32> = (75..280).filter_map(|x| column(rgba, x).1).collect();
            let shake = off.iter().sum::<f32>() / off.len().max(1) as f32;
            let (light, heavy) = (column(rgba, 110), column(rgba, 340));
            eprintln!(
                "{what}: {shake:.2} px off the line on average; at a fifth of the pressure {} px across, \
                 alpha {}; at nearly all {} px across, alpha {}",
                light.0, light.2, heavy.0, heavy.2
            );
            (shake, light, heavy)
        };
        let set_input = |stabilization: f32, motion: f32, curve: &[f32]| {
            s.set_input_settings(stabilization, motion, 0.0, 0.0, true, curve.to_vec());
        };
        set_input(0.0, 0.0, &[]);
        let (raw_shake, raw_light, raw_heavy) = measure("as it comes", &draw(&s));
        assert!(raw_shake > 1.0, "the shake drawn: {raw_shake}");
        assert!(raw_light.0 < raw_heavy.0 && raw_light.2 < raw_heavy.2, "size and opacity follow the pressure");
        // the app's pressure curve: pressed all the way is pressed half as far, the dabs half as big
        // and half as opaque
        set_input(0.0, 0.0, &[0.0, 0.0, 1.0, 0.5]);
        let (_, _, half) = measure("pressure curve to a half", &draw(&s));
        let (across, dark) = (half.0 as f32 / raw_heavy.0 as f32, half.2 / raw_heavy.2);
        assert!((0.35..0.7).contains(&across), "{} px across, {} at full", half.0, raw_heavy.0);
        assert!((0.35..0.7).contains(&dark), "alpha {}, {} at full", half.2, raw_heavy.2);
        // stabilisation and motion filtering take out the shake, each on its own
        for (what, stabilization, motion) in [("stabilization 50%", 0.5, 0.0), ("motion filtering 50%", 0.0, 0.5)] {
            set_input(stabilization, motion, &[]);
            let (shake, light, heavy) = measure(what, &draw(&s));
            assert!(shake < raw_shake / 3.0, "{what}: {shake} px off the line, {raw_shake} as it comes");
            // (the shake no longer widens the light stroke's ink)
            assert!(light.0 < raw_light.0, "{what}: {} px across, {} as it comes", light.0, raw_light.0);
            assert!(light.0 < heavy.0 && light.2 < heavy.2, "{what}: size and opacity still follow the pressure");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Krita colour smudge brush moves the paint it goes over: dragged from red into the clear, it
    /// lays the red where there was nothing, whatever the colour; within a selection, only inside it.
    #[test]
    fn a_krita_colour_smudge_moves_the_paint_it_goes_over() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let s = crate::CanvasSession::new(200, 120);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 120);
            st.view = Some(procraft_engine::view::View::fit((200, 120), (200, 120)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        let dir = std::env::temp_dir().join(format!("procraft-colour-smudge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        let packed = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1;
        assert_eq!(plugins.install(packed.to_vec()), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id, true));
        s.use_plugins(Arc::clone(&plugins));
        let xml = r#"<Preset name="Smear" paintopid="colorsmudge"><param name="brush_definition"><![CDATA[<Brush type="auto_brush" spacing="0.1" BrushVersion="2"><MaskGenerator type="circle" id="default" diameter="20" ratio="1" hfade="1" vfade="1"/></Brush>]]></param><param name="PressureSize"><![CDATA[false]]></param><param name="PressureSmudgeRate"><![CDATA[false]]></param><param name="SmudgeRateValue"><![CDATA[1]]></param><param name="SmudgeRateMode"><![CDATA[0]]></param></Preset>"#;
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(kind);
            c.extend_from_slice(data);
            c.extend_from_slice(&[0; 4]);
            c
        };
        let mut kpp = b"\x89PNG\r\n\x1a\n".to_vec();
        kpp.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0]));
        kpp.extend(chunk(b"tEXt", &[b"preset\0".as_slice(), xml.as_bytes()].concat()));
        kpp.extend(chunk(b"IEND", &[]));
        let read = crate::read_brushes(kpp, Some("Smear.kpp".to_owned())).unwrap();
        let set = s.add_brushes_read(read, 0).unwrap();
        assert!(s.select_brush(set, 0));
        s.wait_for_look();
        // red on the left half
        let layer = s.add_layer();
        {
            let mut st = s.shared.lock();
            let rect = procraft_engine::geom::IRect::new(0, 0, 100, 120);
            st.rewrite_layer(procraft_engine::document::LayerId(layer), rect, [255, 0, 0, 255].repeat(100 * 120));
        }
        s.set_color(0.0, 0.0, 1.0);
        let at = |x: f32| crate::PenSample {
            x,
            y: 60.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: f64::from(x),
            roll: 0.0,
        };
        s.stroke_begin(crate::PointerKind::Stylus, vec![at(60.0)]);
        s.stroke_add((61..=160).map(|x| at(x as f32)).collect());
        s.stroke_end(vec![]);
        let px = s.shared.lock().layer_pixels(procraft_engine::document::LayerId(layer)).unwrap();
        let at_x = |x: usize| &px[(60 * 200 + x) * 4..(60 * 200 + x) * 4 + 4];
        assert!(at_x(130)[0] > 128 && at_x(130)[3] > 128 && at_x(130)[2] < 64, "red dragged out: {:?}", at_x(130));
        assert_eq!(at_x(190)[3], 0, "past the stroke, still clear");
        // with the left 120 pixels selected, a stroke across its edge moves nothing outside it
        s.select_shape(false, 0.0, 0.0, 120.0, 120.0, crate::SelectionJoin::Add);
        let coverage = s.shared.lock().selection.as_ref().unwrap().data.clone();
        let at = |x: f32| crate::PenSample { y: 25.0, ..at(x) };
        s.stroke_begin(crate::PointerKind::Stylus, vec![at(60.0)]);
        s.stroke_add((61..=160).map(|x| at(x as f32)).collect());
        s.stroke_end(vec![]);
        let after = s.shared.lock().layer_pixels(procraft_engine::document::LayerId(layer)).unwrap();
        let moved = |i: usize| px[i * 4..i * 4 + 4] != after[i * 4..i * 4 + 4];
        assert!(moved(25 * 200 + 110), "dragged inside");
        let outside = (0..200 * 120).filter(|&i| coverage[i] == 0 && moved(i)).count();
        assert_eq!(outside, 0, "pixels outside the selection changed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session on `gpu` with the Krita plugin on (the one at `KRITA_PLUGIN`, else the bundled one)
    /// and a bundle's MyPaint brushes brought in: one that smudges fully, its dabs hard and close,
    /// and one that only paints. The session, the plugins, the plugin's id, the set and its folder.
    fn mypaint_session(
        gpu: procraft_gpu::Gpu,
        tag: &str,
    ) -> (Arc<crate::CanvasSession>, Arc<crate::Plugins>, String, u32, std::path::PathBuf) {
        let packed = match std::env::var("KRITA_PLUGIN") {
            Ok(path) => std::fs::read(path).unwrap(),
            Err(_) => crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1.to_vec(),
        };
        let s = crate::CanvasSession::new(200, 120);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 120);
            st.view = Some(procraft_engine::view::View::fit((200, 120), (200, 120)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        let dir = std::env::temp_dir().join(format!("procraft-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        assert_eq!(plugins.install(packed), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id.clone(), true));
        s.use_plugins(Arc::clone(&plugins));
        let brush = |smudge: f32| {
            format!(
                r#"{{"version": 3, "settings": {{
                "radius_logarithmic": {{"base_value": 1.6, "inputs": {{}}}},
                "hardness": {{"base_value": 0.9, "inputs": {{}}}},
                "opaque_multiply": {{"base_value": 1.0, "inputs": {{}}}},
                "dabs_per_actual_radius": {{"base_value": 4.0, "inputs": {{}}}},
                "smudge": {{"base_value": {smudge}, "inputs": {{}}}},
                "smudge_length": {{"base_value": 0.5, "inputs": {{}}}}
            }}}}"#
            )
        };
        let mut zip = procraft_engine::zip::ZipWriter::new();
        zip.add("mimetype", b"application/x-krita-resourcebundle");
        zip.add("paintoppresets/Wet_Drag.myb", brush(1.0).as_bytes());
        zip.add("paintoppresets/Plain_Ink.myb", brush(0.0).as_bytes());
        let read = crate::read_brushes(zip.finish(), Some("Drag.bundle".to_owned())).unwrap();
        s.make_engine_brushes(Arc::clone(&read), None);
        let set = s.add_brushes_read(read, 0).expect("the engine's brushes come in");
        (s, plugins, id, set, dir)
    }

    /// The pen dragged across the middle of a 200 x 120 canvas, from x = 60 to 150, in batches of
    /// three samples as its moves come.
    fn drag_across(s: &crate::CanvasSession) {
        let at = |x: f32| crate::PenSample {
            x,
            y: 60.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: f64::from(x),
            roll: 0.0,
        };
        s.stroke_begin(crate::PointerKind::Stylus, vec![at(60.0)]);
        for from in (61..=150).step_by(3) {
            s.stroke_add((from..from + 3).map(|x| at(x as f32)).collect());
        }
        s.stroke_end(vec![]);
    }

    /// The brush of `set` called `name`, chosen.
    fn choose(s: &crate::CanvasSession, set: u32, name: &str) {
        let index = s.shared.lock().library[set as usize].brushes.iter().position(|b| b.name == name).unwrap();
        assert!(s.shared.lock().library[set as usize].brushes[index].engine.is_some());
        assert!(s.select_brush(set, index as u32));
        s.wait_for_look();
    }

    /// A brush engine that asks what lies under its dabs is told, a batch of samples later: a Krita
    /// MyPaint brush that smudges, dragged from red into the clear, carries the red there in place of
    /// its blue (picking up the red it laid itself as it goes on).
    #[test]
    fn an_engine_is_told_the_colours_under_the_dabs_it_asks_about() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let (s, plugins, id, set, dir) = mypaint_session(gpu, "told-under");
        choose(&s, set, "Wet Drag");
        // red on the left half
        let layer = s.add_layer();
        {
            let mut st = s.shared.lock();
            let rect = procraft_engine::geom::IRect::new(0, 0, 100, 120);
            st.rewrite_layer(procraft_engine::document::LayerId(layer), rect, [255, 0, 0, 255].repeat(100 * 120));
        }
        s.set_color(0.0, 0.0, 1.0);
        drag_across(&s);
        assert!(plugins.plugins().iter().any(|p| p.id == id && p.on && p.problem.is_none()), "the engine still on");
        let px = s.shared.lock().layer_pixels(procraft_engine::document::LayerId(layer)).unwrap();
        let at_x = |x: usize| &px[(60 * 200 + x) * 4..(60 * 200 + x) * 4 + 4];
        // just past the red, the red it carries (none of its blue: it smudges fully)
        let [r, _, b, a] = at_x(110).try_into().unwrap();
        assert!(r > 128 && b < 64 && a > 64, "red carried out: {:?}", at_x(110));
        // further on, still red: what it laid itself is what it picks up
        let [r, _, b, _] = at_x(145).try_into().unwrap();
        assert!(r > 128 && b < 64, "red carried on: {:?}", at_x(145));
        assert_eq!(at_x(190)[3], 0, "past the stroke, still clear");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// On an old GPU (no compute passes: see `Gpu::headless_old`) an engine whose dabs ask what lies
    /// under them and move paint smudges as on any other (within a step of 8 bits a channel here and
    /// there), nothing said and the engine still on; one that only paints paints the same.
    #[test]
    fn an_engine_asking_on_an_old_gpu_smudges_as_on_others() {
        let (Ok(full), Ok(old)) = (procraft_gpu::Gpu::headless(), procraft_gpu::Gpu::headless_old()) else {
            return eprintln!("skipped: no GPU");
        };
        let laid = [(full, "told-under-full"), (old, "told-under-old")].map(|(gpu, tag)| {
            let (s, plugins, id, set, dir) = mypaint_session(gpu, tag);
            let layer = procraft_engine::document::LayerId(s.add_layer());
            s.set_color(0.0, 0.0, 1.0);
            choose(&s, set, "Plain Ink");
            drag_across(&s);
            let painted = s.shared.lock().layer_pixels(layer).unwrap();
            choose(&s, set, "Wet Drag");
            s.set_color(1.0, 0.0, 0.0);
            drag_across(&s);
            let smudged = s.shared.lock().layer_pixels(layer).unwrap();
            assert_eq!(s.take_failure(), None);
            assert!(plugins.plugins().iter().any(|p| p.id == id && p.on && p.problem.is_none()), "the engine still on");
            let _ = std::fs::remove_dir_all(&dir);
            (painted, smudged)
        });
        let [(painted, smudged), (old_painted, old_smudged)] = laid;
        let blue = |px: &[u8], x: usize| px[(60 * 200 + x) * 4 + 2];
        assert!(blue(&old_painted, 100) > 200, "it paints");
        assert_ne!(old_smudged, old_painted, "it smudges");
        let most = |a: &[u8], b: &[u8]| a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        let (p, m) = (most(&painted, &old_painted), most(&smudged, &old_smudged));
        eprintln!("old GPU against a full one: painted {p} at most, smudged {m}");
        assert!(p <= 1 && m <= 2, "{p} {m}");
    }

    /// A look's store is kept with it (only for dabs that move paint), and a dab with flag 65536 lays
    /// its grey over the height as opaque as word 20 says.
    #[test]
    fn a_look_keeps_paint_with_the_layer_and_a_dab_lays_its_height() {
        let mut r = Record::default();
        r.field(field::TIP, &tip(2, 2)).field(field::SMUDGES, &[1]).field(field::STORE, &[1]);
        let look = read_look(&r.bytes(), 9).unwrap();
        assert_eq!(look.store, procraft_gpu::STORE_LIT);
        assert_eq!(look_kept_as(&kept_look(&look), 9).map(|l| l.store), Some(procraft_gpu::STORE_LIT));
        // not for dabs that don't move paint, nor a kind there isn't
        let mut still = Record::default();
        still.field(field::TIP, &tip(2, 2)).field(field::STORE, &[1]);
        assert_eq!(read_look(&still.bytes(), 9).unwrap().store, 0);
        let mut other = Record::default();
        other.field(field::TIP, &tip(2, 2)).field(field::SMUDGES, &[1]).field(field::STORE, &[7]);
        assert_eq!(read_look(&other.bytes(), 9).unwrap().store, 0);
        let dab = |flags: u32| {
            let mut dab = [0_u8; DAB_BYTES];
            let words = [
                (2, 10.0_f32.to_bits()),
                (5, 1.0_f32.to_bits()),
                (8, flags),
                (15, 0.4_f32.to_bits()),
                (20, 0.75_f32.to_bits()),
            ];
            for (i, v) in words {
                dab[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            dab
        };
        let stamp = stamp_of(&dab(MOVES_PAINT | LAYS_HEIGHT), [0.0; 3], 1, Build::Over, 0.0).unwrap();
        assert_eq!(stamp.pick_up.and_then(|p| p.height), Some(0.75));
        assert!((stamp.lightness - 0.4).abs() < 1e-6);
        let without = stamp_of(&dab(MOVES_PAINT), [0.0; 3], 1, Build::Over, 0.0).unwrap();
        assert_eq!(without.pick_up.and_then(|p| p.height), None);
    }

    /// A Krita colour smudge brush with a lightness tip keeps its paint with a plain layer between
    /// strokes, the layer showing it lit by the tip's grey; undone, the layer keeps nothing.
    #[test]
    fn a_krita_lightness_smudge_keeps_its_paint_with_the_layer_lit_by_its_thickness() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let s = crate::CanvasSession::new(200, 120);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 120);
            st.view = Some(procraft_engine::view::View::fit((200, 120), (200, 120)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        let dir = std::env::temp_dir().join(format!("procraft-thickness-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        let packed = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1;
        assert_eq!(plugins.install(packed.to_vec()), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id, true));
        s.use_plugins(Arc::clone(&plugins));
        // a tip 32 pixels a side, a disc (a picture, as it is see-through), its grey in bands 8 rows
        // each, light and dark
        let side = 32_usize;
        let pixels: Vec<u8> = (0..side * side)
            .flat_map(|i| {
                let (x, y) = ((i % side) as f32 - 15.5, (i / side) as f32 - 15.5);
                let a = if x * x + y * y < 15.5 * 15.5 { 255 } else { 0 };
                if (i / side / 8).is_multiple_of(2) { [220, 220, 220, a] } else { [40, 40, 40, a] }
            })
            .collect();
        let png = crate::project::png_of(side as u32, side as u32, &pixels).unwrap();
        let tip = procraft_engine::base64::encode(&png);
        let xml = format!(
            r#"<Preset name="Thick" paintopid="colorsmudge"><resources><resource name="bands" filename="bands.png" type="brushes"><![CDATA[{tip}]]></resource></resources><param name="brush_definition"><![CDATA[<Brush type="png_brush" filename="bands.png" spacing="0.1" BrushVersion="2" brushApplication="2" scale="1" angle="0" useAutoSpacing="0"/>]]></param><param name="PressureSize"><![CDATA[false]]></param><param name="PressureSmudgeRate"><![CDATA[false]]></param><param name="SmudgeRateValue"><![CDATA[1]]></param><param name="SmudgeRateMode"><![CDATA[0]]></param><param name="SmudgeRateUseNewEngine"><![CDATA[true]]></param><param name="PressureColorRate"><![CDATA[true]]></param><param name="ColorRateValue"><![CDATA[1]]></param></Preset>"#
        );
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(kind);
            c.extend_from_slice(data);
            c.extend_from_slice(&[0; 4]);
            c
        };
        let mut kpp = b"\x89PNG\r\n\x1a\n".to_vec();
        kpp.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0]));
        kpp.extend(chunk(b"tEXt", &[b"preset\0".as_slice(), xml.as_bytes()].concat()));
        kpp.extend(chunk(b"IEND", &[]));
        let read = crate::read_brushes(kpp, Some("Thick.kpp".to_owned())).unwrap();
        let set = s.add_brushes_read(read, 0).unwrap();
        assert!(s.select_brush(set, 0));
        s.wait_for_look();
        let layer = procraft_engine::document::LayerId(s.add_layer());
        // a mid grey all over: its lightness 0.5 goes lighter and darker as the tip's grey says
        s.set_color(0.5, 0.5, 0.5);
        let at = |x: f32| crate::PenSample {
            x,
            y: 60.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: f64::from(x),
            roll: 0.0,
        };
        s.stroke_begin(crate::PointerKind::Stylus, vec![at(40.0)]);
        s.stroke_add((41..=160).map(|x| at(x as f32)).collect());
        s.stroke_end(vec![]);
        let store = |s: &crate::CanvasSession| {
            let mut guard = s.shared.lock();
            let st = &mut *guard;
            let (canvas, gpu) = (st.canvas.as_mut().unwrap(), st.gpu.as_ref().unwrap());
            canvas.render(gpu, &mut st.doc, None);
            canvas.store(layer).map(|s| s.kind)
        };
        assert_eq!(store(&s), Some(procraft_gpu::STORE_LIT));
        let px = s.shared.lock().layer_pixels(layer).unwrap();
        // down the middle of the stroke, the bands of the tip lit light and dark
        let column: Vec<u8> = (48..72).map(|y| px[(y * 200 + 100) * 4]).collect();
        let (lo, hi) = (column.iter().min().copied().unwrap(), column.iter().max().copied().unwrap());
        assert!(lo < 90 && hi > 170, "{column:?}");
        s.undo();
        assert_eq!(store(&s), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A brush engine that asks for the colours under where a stroke starts is handed them: a Krita
    /// bristle brush soaking up its ink paints in the red it starts on, whatever the colour.
    #[test]
    fn an_engine_is_handed_the_colours_under_where_a_stroke_starts() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let s = crate::CanvasSession::new(200, 120);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 120);
            st.view = Some(procraft_engine::view::View::fit((200, 120), (200, 120)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        let dir = std::env::temp_dir().join(format!("procraft-soak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        let packed = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1;
        assert_eq!(plugins.install(packed.to_vec()), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id, true));
        s.use_plugins(Arc::clone(&plugins));
        let xml = r#"<Preset name="Soak" paintopid="hairybrush"><param name="brush_definition"><![CDATA[<Brush type="auto_brush" spacing="0.1" BrushVersion="2"><MaskGenerator type="circle" id="default" diameter="6" ratio="1" hfade="1" vfade="1"/></Brush>]]></param><param name="HairyBristle/scale"><![CDATA[1]]></param><param name="HairyBristle/random"><![CDATA[0]]></param><param name="HairyBristle/isConnected"><![CDATA[true]]></param><param name="HairyInk/enabled"><![CDATA[true]]></param><param name="HairyInk/soak"><![CDATA[true]]></param><param name="HairyInk/useOpacity"><![CDATA[false]]></param></Preset>"#;
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(kind);
            c.extend_from_slice(data);
            c.extend_from_slice(&[0; 4]);
            c
        };
        let mut kpp = b"\x89PNG\r\n\x1a\n".to_vec();
        kpp.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0]));
        kpp.extend(chunk(b"tEXt", &[b"preset\0".as_slice(), xml.as_bytes()].concat()));
        kpp.extend(chunk(b"IEND", &[]));
        let read = crate::read_brushes(kpp, Some("Soak.kpp".to_owned())).unwrap();
        let set = s.add_brushes_read(read, 0).unwrap();
        assert!(s.select_brush(set, 0));
        s.wait_for_look();
        // red on the left half of the layer
        let layer = s.add_layer();
        {
            let mut st = s.shared.lock();
            let rect = procraft_engine::geom::IRect::new(0, 0, 100, 120);
            st.rewrite_layer(procraft_engine::document::LayerId(layer), rect, [255, 0, 0, 255].repeat(100 * 120));
        }
        s.set_color(0.0, 0.0, 1.0);
        let at = |x: f32| crate::PenSample {
            x,
            y: 60.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: f64::from(x),
            roll: 0.0,
        };
        s.stroke_begin(crate::PointerKind::Stylus, vec![at(60.0)]);
        s.stroke_add((61..=160).map(|x| at(x as f32)).collect());
        s.stroke_end(vec![]);
        let px = s.shared.lock().layer_pixels(procraft_engine::document::LayerId(layer)).unwrap();
        let at_x = |x: usize| &px[(60 * 200 + x) * 4..(60 * 200 + x) * 4 + 4];
        // out in the clear, the bristles' red, not the colour's blue
        assert!(at_x(140)[3] > 64 && at_x(140)[0] > 200 && at_x(140)[2] < 64, "soaked up red: {:?}", at_x(140));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An airbrush's engine is handed the pen again while it holds still, and lays its dabs as
    /// the time passes.
    #[test]
    fn an_airbrush_lays_dabs_while_the_pen_holds_still() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let s = crate::CanvasSession::new(200, 120);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 120);
            st.view = Some(procraft_engine::view::View::fit((200, 120), (200, 120)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        let dir = std::env::temp_dir().join(format!("procraft-airbrush-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        let packed = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1;
        assert_eq!(plugins.install(packed.to_vec()), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id, true));
        s.use_plugins(Arc::clone(&plugins));
        // a dab every 50 ms
        let airbrush = r#"<param name="PaintOpSettings/isAirbrushing" type="string"><![CDATA[true]]></param><param name="PaintOpSettings/rate" type="string"><![CDATA[20]]></param>"#;
        let read = crate::read_brushes(preset_of(airbrush), Some("Air.kpp".to_owned())).unwrap();
        let set = s.add_brushes_read(read, 0).unwrap();
        assert!(s.select_brush(set, 0));
        s.wait_for_look();
        let at = |t: f64| crate::PenSample {
            x: 50.0,
            y: 60.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: t,
            roll: 0.0,
        };
        s.stroke_begin(crate::PointerKind::Stylus, vec![at(1000.0)]);
        let count = |s: &crate::CanvasSession| s.shared.lock().stroke.as_ref().map_or(0, |b| b.stamp_count());
        let first = count(&s);
        assert_eq!(s.touch_deadline(), Some(1025.0), "handed again every half interval");
        s.touch_tick(1500.0);
        assert!(count(&s) >= first + 9, "{first} then {}", count(&s));
        s.stroke_end(vec![]);
        assert_eq!(s.touch_deadline(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A look its engine gave is kept on the disk with the plugin: the next start reads it from
    /// there, and the engine only reads the brush to draw it. Kept again, the plugin's looks go.
    #[test]
    fn a_look_is_kept_on_the_disk_until_its_plugin_is_kept_again() {
        let packed = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1;
        let dir = std::env::temp_dir().join(format!("procraft-kept-looks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = || crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        let plugins = store();
        assert_eq!(plugins.install(packed.to_vec()), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id.clone(), true));
        let hooks = plugins.hooks();
        let mut brushes = Vec::new();
        join_made(&mut brushes, make_brushes(&hooks, "Squares.kpp", &square_preset()));
        let brush = &brushes[0];
        let key = brush.engine.as_ref().unwrap().engine.clone();
        let (look, data) = look_of(&hooks, brush, true).unwrap();
        let kept = hooks.look_on_disk(&id, &key, data).expect("kept on the disk");
        assert_eq!(look_kept_as(&kept, data).as_ref(), Some(&*look));
        drop((plugins, hooks));

        // the next start: what is kept there (a look of a 3 x 5 tip put in its place) is taken
        let next = store();
        assert!(next.set_on(id.clone(), true), "on again, as the app turns it on at a start");
        let hooks = next.hooks();
        let mut other = Record::default();
        other.field(field::TIP, &tip(3, 5));
        let other = read_look(&other.bytes(), data).unwrap();
        crate::plugin_file::write_look(&dir, &id, &key, data, &kept_look(&other));
        let (again, _) = look_of(&hooks, brush, true).unwrap();
        assert_eq!(*again, other);
        // the engine reads the brush, its look held, and draws it
        let starting = Starting {
            colour: [0.0; 3],
            secondary: [1.0; 3],
            diameter: 1.0,
            start_diameter: 1.0,
            opacity: 1.0,
            tool: 0,
            pointer: 1,
            seed: 7,
            turn: 0.0,
            under_at: None,
        };
        let (mut first, _) = begin_stroke(&hooks, brush, again, data, starting, None, (200, 120)).expect("a stroke");
        let samples: Vec<Sample> =
            (0..100).map(|i| Sample::at(Vec2::new(20.0 + i as f32, 60.0), 1.0, f64::from(i))).collect();
        let stamps = first.add(&samples);
        assert!(stamps.len() >= 2 && stamps.iter().all(|s| s.tip == 0), "squares, 40 apart");
        // the plugin kept again: its looks go
        assert_eq!(next.install(packed.to_vec()), None);
        assert_eq!(hooks.look_on_disk(&id, &key, data), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// How long the bundled Krita plugin takes on a real file, in the interpreter: making its
    /// brushes, reading each, and a stroke of each (`KRITA_FILE=x.bundle cargo test -p
    /// procraft-core krita_timings -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn krita_timings() {
        use std::time::Instant;
        let Ok(path) = std::env::var("KRITA_FILE") else { return };
        let dir = std::env::temp_dir().join(format!("procraft-krita-timings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        let packed = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1;
        assert_eq!(plugins.install(packed.to_vec()), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id.clone(), true));
        let hooks = plugins.hooks();
        let bytes = std::fs::read(&path).unwrap();
        let t = Instant::now();
        let made = make_brushes(&hooks, &path, &bytes);
        println!("{} brushes made of {} bytes in {:?}", made.len(), bytes.len(), t.elapsed());
        let mut brushes = Vec::new();
        join_made(&mut brushes, made);
        let (mut loading, mut drawing, mut dabs) = (std::time::Duration::ZERO, std::time::Duration::ZERO, 0);
        for brush in &brushes {
            let t = Instant::now();
            let Some((look, id)) = look_of(&hooks, brush, true) else { continue };
            loading += t.elapsed();
            let starting = Starting {
                colour: [0.0; 3],
                secondary: [1.0; 3],
                diameter: 1.0,
                start_diameter: 1.0,
                opacity: 1.0,
                tool: 0,
                pointer: 1,
                seed: 7,
                turn: 0.0,
                under_at: None,
            };
            let Some((mut first, _)) = begin_stroke(&hooks, brush, look, id, starting, None, (200, 120)) else {
                continue;
            };
            let t = Instant::now();
            // a second of a pen at 240 samples a second, 4 at a time as frames bring them
            for batch in 0..60 {
                let samples: Vec<Sample> = (0..4)
                    .map(|i| {
                        let k = (batch * 4 + i) as f32;
                        Sample::at(Vec2::new(100.0 + k * 3.0, 300.0 + (k * 0.05).sin() * 40.0), 0.7, f64::from(k) * 4.0)
                    })
                    .collect();
                dabs += first.add(&samples).len();
            }
            dabs += first.finish().len();
            drawing += t.elapsed();
        }
        println!("{} read in {loading:?}; their strokes' {dabs} dabs in {drawing:?}", brushes.len());
        // the next start: each look from the disk, the engine reading each brush only to draw it
        let next = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        assert!(next.set_on(id, true));
        let hooks = next.hooks();
        let t = Instant::now();
        let (mut begun, mut looking) = (0, std::time::Duration::ZERO);
        for brush in &brushes {
            let t = Instant::now();
            let Some((look, id)) = look_of(&hooks, brush, true) else { continue };
            looking += t.elapsed();
            let starting = Starting {
                colour: [0.0; 3],
                secondary: [1.0; 3],
                diameter: 1.0,
                start_diameter: 1.0,
                opacity: 1.0,
                tool: 0,
                pointer: 1,
                seed: 7,
                turn: 0.0,
                under_at: None,
            };
            begun += usize::from(begin_stroke(&hooks, brush, look, id, starting, None, (200, 120)).is_some());
        }
        println!("the next start: {begun} looks kept and strokes begun in {:?} (the looks {looking:?})", t.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_look_holds_its_tips_fitted_into_one_square_side() {
        let mut look = Record::default();
        look.field(field::TIP, &tip(100, 50)).field(field::TIP, &tip(30, 30)).field(field::BUILD, &[1]);
        look.field(field::BLEND, &3_u32.to_le_bytes()).field(field::ERASES, &[1]);
        let look = read_look(&look.bytes(), 7).unwrap();
        assert_eq!(look.tips.side, 128);
        assert_eq!(look.tips.layers.len(), 2);
        assert_eq!((look.build, look.blend, look.erases), (Build::Towards, Some(3), true));
        // the wide tip across all of the square, a quarter of it clear above and below
        let wide = &look.tips.layers[0];
        assert_eq!(wide[64 * 128 + 64], 255);
        assert_eq!(wide[10 * 128 + 64], 0);
        // a tip whose pixels aren't its size, or none, holds nothing up
        let mut bad = Record::default();
        bad.field(field::TIP, &tip(4, 4)[..20]);
        assert!(read_look(&bad.bytes(), 7).is_none());
        assert!(read_look(&[], 7).is_none());
    }

    #[test]
    fn a_dab_becomes_a_stamp_as_its_stroke_builds_up() {
        let mut dab = [0_u8; DAB_BYTES];
        let words: [f32; 7] = [10.0, 20.0, 8.0, 0.5, 0.5, 0.6, 0.25];
        for (i, v) in words.iter().enumerate() {
            dab[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        dab[28..32].copy_from_slice(&9_u32.to_le_bytes());
        dab[32..36].copy_from_slice(&(FLIP_ACROSS | OWN_COLOUR).to_le_bytes());
        for (i, v) in [0.0_f32, 0.0, 1.0, 0.5].iter().enumerate() {
            dab[36 + i * 4..40 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        let over = stamp_of(&dab, [1.0, 0.0, 0.0], 4, Build::Over, 0.0).unwrap();
        assert_eq!((over.center, over.radius, over.roundness), (Vec2::new(10.0, 20.0), 4.0, 0.5));
        assert!((over.alpha - 0.15).abs() < 1e-6 && over.transfer == 1.0);
        assert_eq!((over.tip, over.flip, over.grain_depth), (3, [true, false], 0.5));
        let towards = stamp_of(&dab, [1.0, 0.0, 0.0], 4, Build::Towards, 0.0).unwrap();
        assert_eq!((towards.alpha, towards.transfer), (0.25, 0.6));
        // red tinted to blue: the hue turned two thirds, kept full
        assert!((towards.tint.hue - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!((towards.tint.saturation, towards.tint.brightness), ([0.0, 1.0], [0.0, 1.0]));
        // a dab of no size, or with no tips to draw, is none
        dab[8..12].copy_from_slice(&0.0_f32.to_le_bytes());
        assert!(stamp_of(&dab, [1.0; 3], 4, Build::Over, 0.0).is_none());
    }

    #[test]
    fn made_brushes_join_those_read_by_their_names() {
        let mut brushes = vec![Brush { name: "Pencil".into(), ..Brush::round_pen() }];
        let made = |name: &str| MadeBrush { engine: "a.b.c".into(), name: name.into(), data: vec![1], thumbnail: None };
        join_made(&mut brushes, vec![made("Pencil"), made("Ink")]);
        assert_eq!(brushes.len(), 2);
        assert_eq!(brushes[0].engine.as_ref().map(|e| e.engine.as_str()), Some("a.b.c"));
        assert_eq!(brushes[1].name, "Ink");
        assert!(brushes[1].engine.is_some());
    }

    /// A Krita tip turned by its angle points where Krita draws it, by the plugin's engine and by the
    /// app's own reading of the preset alike: Krita turns a tip counter-clockwise on the canvas
    /// (whose rows run down) by its angle, so a bar from the tip's middle to its right edge points up
    /// and to the right at 30°, straight up at 90° and straight down at 270°, the notch at its end on
    /// its left as it goes (never mirrored to its right).
    #[test]
    fn a_krita_tip_turned_by_its_angle_points_where_krita_draws_it() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let s = crate::CanvasSession::new(200, 200);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 200);
            st.view = Some(procraft_engine::view::View::fit((200, 200), (200, 200)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        let dir = std::env::temp_dir().join(format!("procraft-krita-angle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        let packed = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1;
        assert_eq!(plugins.install(packed.to_vec()), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        s.use_plugins(Arc::clone(&plugins));
        // a tip 40 pixels a side, a picture (see-through): a bar 8 rows thick from its middle to its
        // right edge, nothing on its left, with a notch up from the bar's end
        let side = 40_usize;
        let pixels: Vec<u8> = (0..side * side)
            .flat_map(|i| {
                let (x, y) = ((i % side) as f32 - 19.5, (i / side) as f32 - 19.5);
                let bar = x > 2.0 && y.abs() < 4.0;
                let notch = (11.0..19.0).contains(&x) && (-15.0..-3.0).contains(&y);
                let a = if bar || notch { 255 } else { 0 };
                [0, 0, 0, a]
            })
            .collect();
        let png = crate::project::png_of(side as u32, side as u32, &pixels).unwrap();
        let tip = procraft_engine::base64::encode(&png);
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(kind);
            c.extend_from_slice(data);
            c.extend_from_slice(&[0; 4]);
            c
        };
        // a dab of the tip at `degrees` in the middle of a layer of its own
        let dab_at = |degrees: f32, engine: bool| {
            let angle = degrees.to_radians();
            let xml = format!(
                r#"<Preset name="Bar" paintopid="paintbrush"><resources><resource name="bar" filename="bar.png" type="brushes"><![CDATA[{tip}]]></resource></resources><param name="brush_definition"><![CDATA[<Brush type="png_brush" filename="bar.png" spacing="5" BrushVersion="2" brushApplication="0" scale="1" angle="{angle}" useAutoSpacing="0"/>]]></param><param name="PressureSize"><![CDATA[false]]></param><param name="PressureOpacity"><![CDATA[false]]></param></Preset>"#
            );
            let mut kpp = b"\x89PNG\r\n\x1a\n".to_vec();
            kpp.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0]));
            kpp.extend(chunk(b"tEXt", &[b"preset\0".as_slice(), xml.as_bytes()].concat()));
            kpp.extend(chunk(b"IEND", &[]));
            let read = crate::read_brushes(kpp, Some("Bar.kpp".to_owned())).unwrap();
            if engine {
                s.make_engine_brushes(Arc::clone(&read), None);
            }
            let set = s.add_brushes_read(read, 0).unwrap();
            assert_eq!(s.shared.lock().library[set as usize].brushes[0].engine.is_some(), engine);
            assert!(s.select_brush(set, 0));
            s.wait_for_look();
            s.wait_for_look();
            if !engine {
                // the app's own brush at its biggest, to be measured as finely
                s.set_brush_size(1.0);
            }
            s.set_color(0.0, 0.0, 0.0);
            let layer = s.add_layer();
            let at = crate::PenSample {
                x: 100.0,
                y: 100.0,
                pressure: 1.0,
                tilt: 0.0,
                orientation: 0.0,
                time_ms: 0.0,
                roll: 0.0,
            };
            s.stroke_begin(crate::PointerKind::Stylus, vec![at]);
            s.stroke_end(vec![]);
            s.shared.lock().layer_pixels(procraft_engine::document::LayerId(layer)).unwrap()
        };
        let alpha = |rgba: &[u8], x: f32, y: f32| rgba[(y.round() as usize * 200 + x.round() as usize) * 4 + 3];
        for engine in [true, false] {
            assert!(plugins.set_on(id.clone(), engine));
            for degrees in [0.0_f32, 30.0, 90.0, 270.0] {
                let rgba = dab_at(degrees, engine);
                // the dab's size against the tip's: its farthest corner (19, -15) from its middle
                let far = (0..200 * 200)
                    .filter(|i| rgba[i * 4 + 3] > 128)
                    .map(|i| ((i % 200) as f32 - 100.0).hypot((i / 200) as f32 - 100.0))
                    .fold(0.0_f32, f32::max);
                let k = far / 24.2;
                let (c, sn) = (degrees.to_radians().cos(), degrees.to_radians().sin());
                // a point of the tip (right, down) where it lands, turned counter-clockwise on the
                // canvas
                let lands = |(x, y): (f32, f32)| (100.0 + k * (x * c + y * sn), 100.0 + k * (y * c - x * sn));
                let way = if engine { "the engine" } else { "the app" };
                // 8 pixels out along the bar, and as far the other way
                let (toward, away) = (lands((8.0, 0.0)), lands((-8.0, 0.0)));
                let (on, off) = (alpha(&rgba, toward.0, toward.1), alpha(&rgba, away.0, away.1));
                assert!(on > 200 && off < 20, "{way}, {degrees}°: {on} toward {toward:?}, {off} away (k {k})");
                // the notch, on the bar's left (up, at 0°), not mirrored across it
                let (notch, mirrored) = (lands((15.0, -10.0)), lands((15.0, 10.0)));
                let (on, off) = (alpha(&rgba, notch.0, notch.1), alpha(&rgba, mirrored.0, mirrored.1));
                assert!(on > 200 && off < 20, "{way}, {degrees}°: the notch {on} at {notch:?}, {off} mirrored");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An engine's module: `import` and `load` the bodies of its import and load (a made brush called
    /// N is at 512, a look of one tip a pixel big at 600, what it is handed at 1024: its file's name,
    /// then its bytes).
    fn engine_module(import: &str, load: &str) -> Vec<u8> {
        wat::parse_str(format!(
            r#"(module
            (import "procraft" "inflate" (func $inflate (param i32 i32 i32) (result i32)))
            (memory (export "memory") 2)
            (global $out (mut i32) (i32.const 512))
            (data (i32.const 512) "\01\00\0e\00\00\00\01\00\01\00\00\00N\02\00\01\00\00\00d")
            (data (i32.const 600) "\01\00\1b\00\00\00\01\00\04\00\00\00\01\00\00\00\02\00\04\00\00\00\01\00\00\00\03\00\01\00\00\00\ff")
            (func (export "procraft_brush_in") (param i32) (result i32) (i32.const 1024))
            (func (export "procraft_brush_out") (result i32) (global.get $out))
            (func (export "procraft_brush_import") (param i32 i32) (result i32) (global.set $out (i32.const 512)) {import})
            (func (export "procraft_brush_load") (param i32 i32) (result i32) (global.set $out (i32.const 600)) {load})
            (func (export "procraft_brush_begin") (param i32) (result i32) (i32.const -1))
            (func (export "procraft_brush_add") (param i32 i32) (result i32) (i32.const 0))
            (func (export "procraft_brush_end") (param i32) (result i32) (i32.const 0)))"#
        ))
        .unwrap()
    }

    /// Makes the brush N.
    const MAKES: &str = "(i32.const 20)";
    /// Asks for 300 MB more memory (drawing may have 256 at most), then makes N.
    const GROWS: &str = "(drop (memory.grow (i32.const 4800))) (i32.const 20)";
    /// Never ends.
    const LOOPS: &str = "(loop $l (br $l)) (i32.const 0)";
    /// Has the app unpack the whole of its file, then never ends.
    const READS_THEN_LOOPS: &str = "(drop (call $inflate
            (i32.add (i32.const 1036) (i32.load (i32.const 1026)))
            (i32.load (i32.add (i32.const 1032) (i32.load (i32.const 1026))))
            (i32.const 0)))
        (loop $l (br $l)) (i32.const 0)";

    const PENS: &str = "org.example.pens";
    const PENS_KEY: &str = "org.example.pens.pens";

    /// Hooks with an engine of [`engine_module`] on, called Pens and reading `.pens` files, and how a
    /// copy of its code starts.
    fn pens_on(import: &str, load: &str) -> Arc<Hooks> {
        use crate::plugin_run::{Hook, Running};
        let bytes = engine_module(import, load);
        let hooks = Arc::new(Hooks::default());
        hooks.put_on(PENS.to_owned(), Running::start_on(&bytes, &[], &[Hook::Brushes], &[]).unwrap(), &[Hook::Brushes]);
        let engine = BrushEngine {
            key: PENS_KEY.to_owned(),
            label: "Pens".to_owned(),
            accepts: vec!["pens".to_owned()],
            studio: None,
        };
        hooks.brush_engines(PENS, vec![engine]);
        hooks.give_copier(
            PENS,
            Arc::new(move |memory| Running::start_told_within(&bytes, &[], &[Hook::Brushes], &[], None, memory)),
        );
        hooks
    }

    fn limits(memory: usize, time: std::time::Duration) -> ImportLimits {
        ImportLimits { memory, steps: crate::plugin_run::IMPORT_FUEL, time }
    }

    /// The app read two brushes of a `.pens` file holding `bytes`: N and Other.
    fn pens_read(bytes: &[u8]) -> crate::ReadBrushes {
        let named = |name: &str| Brush { name: name.to_owned(), ..Brush::round_pen() };
        crate::ReadBrushes::Set {
            name: "Pens".to_owned(),
            brushes: vec![named("N"), named("Other")],
            file: Some(("Pens.pens".to_owned(), BrushFileBytes::Memory(bytes.into()))),
        }
    }

    /// `pens_read(bytes)` through the engines on `hooks` within `limits`: the brushes, and what the
    /// reading says after (its title, if any, then a new line, then its text).
    fn pens_made(
        hooks: &Arc<Hooks>,
        bytes: &[u8],
        reading: &crate::BrushFileReading,
        limits: ImportLimits,
    ) -> (Vec<Brush>, String) {
        let engines = crate::brush_files::BrushEngines::of(Some(Arc::clone(hooks)));
        let crate::ReadBrushes::Set { brushes, file, .. } = engines.make_within(pens_read(bytes), reading, limits)
        else {
            panic!()
        };
        assert!(file.is_none());
        let titled = reading.notice_title().map(|t| format!("{t}\n")).unwrap_or_default();
        (brushes, titled + &reading.notice().unwrap_or_default())
    }

    /// An import goes on a copy of the plugin's code with an import's memory, many times what
    /// drawing has: what its load can't ask for, its import can; and the plugin's own code is never
    /// touched by it. What came in is told, the brushes the app's own reading draws counted.
    #[test]
    fn an_import_has_more_memory_than_drawing_on_a_copy_of_its_own() {
        let hooks = pens_on(GROWS, &GROWS.replace("(i32.const 20)", "(i32.const -1)"));
        let reading = crate::BrushFileReading::default();
        let (brushes, notice) = pens_made(&hooks, b"pens", &reading, limits(512 << 20, Duration::from_secs(60)));
        assert!(brushes[0].engine.is_some() && brushes[1].engine.is_none());
        assert_eq!(notice, "2 brushes imported, 1 drawn by the built-in reader.");
        let (_, running, index) = hooks.engine(PENS_KEY).unwrap();
        let mut running = at_work(&running);
        assert_eq!((running.spent(), running.memory_used()), (0, 2 << 16), "the import went on a copy");
        // a load (drawing) asking the same is refused: it has what it always had
        assert_eq!(running.brush_load(index, b"d"), Err(Stop::Memory));
    }

    /// An import too big for the memory an import has on this device, a file bigger than that, an
    /// import taking too long, and one the engine crashes on each come in as the app read them, the
    /// plugin still on, and say so.
    #[test]
    fn an_import_too_much_for_its_limits_comes_in_as_read_and_says_so() {
        let said = |import: &str, bytes: &[u8], limits: ImportLimits| {
            let hooks = pens_on(import, MAKES);
            let (brushes, notice) = pens_made(&hooks, bytes, &crate::BrushFileReading::default(), limits);
            assert!(brushes.iter().all(|b| b.engine.is_none()), "as the app read them");
            assert_eq!(brushes.len(), 2);
            assert!(hooks.engine(PENS_KEY).is_some(), "still on");
            notice
        };
        let minute = Duration::from_secs(60);
        let too_big = said(GROWS, b"pens", limits(64 << 20, minute));
        assert_eq!(
            too_big,
            "File too big for Pens\nThis file is too big for Pens on this device. Its brushes came in with the built-in reader instead, so they may look a little different. Pens is still on."
        );
        assert_eq!(said(MAKES, &[7; 4096], limits(1000, minute)), too_big, "a file over the memory isn't handed over");
        let t = std::time::Instant::now();
        let too_long = said(LOOPS, b"pens", limits(512 << 20, Duration::from_millis(200)));
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
        assert!(
            too_long.starts_with("Pens took too long\nPens took too long with this file. Its brushes came in"),
            "{too_long}"
        );
        let crashed = said("unreachable", b"pens", limits(512 << 20, minute));
        assert!(
            crashed.starts_with("Pens couldn't read this file\nPens couldn't read this file.")
                && crashed.ends_with("Pens is still on.")
        );
        // a pack: a zip
        let mut zip = procraft_engine::zip::ZipWriter::new();
        zip.add("a.kpp", b"a");
        assert!(
            said(GROWS, &zip.finish(), limits(64 << 20, minute))
                .starts_with("Pack too big for Pens\nThis pack is too big for Pens")
        );
    }

    /// While an import runs, its progress is how far into its file it has had the app unpack; Cancel
    /// stops it at once, whatever it is doing, and says so; the plugin stays on.
    #[test]
    fn an_import_shows_how_far_it_has_read_and_cancel_stops_it() {
        let hooks = pens_on(READS_THEN_LOOPS, MAKES);
        let reading = Arc::new(crate::BrushFileReading::default());
        let watcher = {
            let reading = Arc::clone(&reading);
            std::thread::spawn(move || {
                let until = std::time::Instant::now() + Duration::from_secs(30);
                let mut seen = Vec::new();
                while std::time::Instant::now() < until {
                    let shown = (reading.engine_size(), reading.engine_progress());
                    if seen.last() != Some(&shown) {
                        seen.push(shown);
                    }
                    if shown.1 >= 1.0 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                reading.cancel();
                (seen, std::time::Instant::now())
            })
        };
        let file = vec![3_u8; 3000];
        let (brushes, notice) = pens_made(&hooks, &file, &reading, limits(512 << 20, Duration::from_secs(120)));
        let ended = std::time::Instant::now();
        let (seen, cancelled) = watcher.join().unwrap();
        assert!(seen.iter().any(|&(size, progress)| size == 3000 && progress >= 1.0), "{seen:?}");
        assert!(ended.duration_since(cancelled) < Duration::from_secs(5), "stopped at once");
        assert_eq!(notice, crate::brush_files::IMPORT_CANCELLED);
        assert_eq!(brushes.iter().filter(|b| b.engine.is_some()).count(), 0);
        assert!(hooks.engine(PENS_KEY).is_some(), "still on");
        assert_eq!((reading.engine_size(), reading.engine_progress()), (0, 0.0), "none at it after");
    }

    /// A brush's look is made as an import's work is, on a copy of the plugin's code with an import's
    /// memory: a look drawing has no room for comes. One the engine can't make is asked for once,
    /// the plugin staying on, and so is one the app can't take.
    #[test]
    fn a_look_is_made_with_an_import_s_limits_and_one_it_cant_have_is_asked_once() {
        let brush = Brush {
            engine: Some(OutsideBrush { engine: PENS_KEY.to_owned(), data: Arc::from(&b"d"[..]) }),
            ..Brush::round_pen()
        };
        let id = data_id(b"d");
        let hooks = pens_on(MAKES, "(drop (memory.grow (i32.const 4800))) (i32.const 33)");
        let (look, _) = look_of(&hooks, &brush, true).expect("made on a copy");
        assert_eq!(look.tips.layers.len(), 1);
        let (_, running, _) = hooks.engine(PENS_KEY).unwrap();
        assert_eq!(at_work(&running).memory_used(), 2 << 16, "not on the plugin's own code");
        for load in ["unreachable", "(i32.const -1)", "(i32.const 6)"] {
            let hooks = pens_on(MAKES, load);
            assert!(look_of(&hooks, &brush, true).is_none(), "{load}");
            assert!(hooks.has_no_look(PENS_KEY, id), "{load}: not asked again");
            assert!(look_of(&hooks, &brush, true).is_none());
            assert!(hooks.engine(PENS_KEY).is_some(), "{load}: still on");
        }
    }

    /// A look that takes long is made on a thread of its own, once: asked without waiting (as a
    /// stroke asks, on the interface's thread) it is none at once, however often it is asked while
    /// it is made; a preview's ask waits for it; once made it is had at once.
    #[test]
    fn a_slow_look_never_holds_up_an_ask_that_doesnt_wait() {
        let slow = "(local.set 1 (i32.const 0))
            (loop $l (local.set 1 (i32.add (local.get 1) (i32.const 1)))
                (br_if $l (i32.lt_u (local.get 1) (i32.const 150000000))))
            (i32.const 33)";
        let brush = Brush {
            engine: Some(OutsideBrush { engine: PENS_KEY.to_owned(), data: Arc::from(&b"d"[..]) }),
            ..Brush::round_pen()
        };
        let id = data_id(b"d");
        let hooks = pens_on(MAKES, slow);
        let t = std::time::Instant::now();
        let (mut asks, mut longest) = (0, Duration::ZERO);
        while t.elapsed() < Duration::from_secs(120) {
            let ask = std::time::Instant::now();
            let had = look_of(&hooks, &brush, false).is_some();
            longest = longest.max(ask.elapsed());
            asks += 1;
            if had {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let made_in = t.elapsed();
        println!("the look made in {made_in:.2?}: {asks} asks meanwhile, the longest {longest:.2?}");
        assert!(made_in > Duration::from_millis(300), "slow enough to tell: {made_in:?}");
        assert!(longest < Duration::from_millis(50), "{longest:?}");
        assert!(hooks.look_kept(PENS_KEY, id).is_some());
        // a preview's ask waits for the one being made
        let hooks = pens_on(MAKES, slow);
        assert!(look_of(&hooks, &brush, false).is_none(), "being made");
        assert!(look_of(&hooks, &brush, true).is_some(), "waited for");
    }

    /// The first stroke of a brush whose look takes seconds to make (a pipe of 200 cells 300 pixels
    /// across) starts and ends at once on the interface's thread, drawn as the app read the brush
    /// while its look is made; once it is, strokes are the engine's.
    #[test]
    fn a_stroke_never_waits_on_a_look_being_made() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let dir = std::env::temp_dir().join(format!("procraft-slow-look-{}", std::process::id()));
        let (plugins, hooks, _, _, _) = krita_on(&dir);
        let s = crate::CanvasSession::new(200, 120);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 120);
            st.view = Some(procraft_engine::view::View::fit((200, 120), (200, 120)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        s.use_plugins(Arc::clone(&plugins));
        let mut pipe = b"slow\n200 ncells:200 dim:1 rank0:200 sel0:incremental\n".to_vec();
        for i in 0..200 {
            pipe.extend(colour_gbr(300, 300, i as u8));
        }
        let mut zip = procraft_engine::zip::ZipWriter::new();
        let tip = r#"<Brush type="gbr_brush" filename="slow.gih" spacing="0.5" scale="1" BrushVersion="2"/>"#;
        zip.add("paintoppresets/Slow.kpp", &preset_named("Slow", tip));
        zip.add("brushes/slow.gih", &pipe);
        let read = crate::read_brushes(zip.finish(), Some("Slow.bundle".to_owned())).unwrap();
        s.make_engine_brushes(Arc::clone(&read), None);
        let set = s.add_brushes_read(read, 0).unwrap();
        let brush = s.shared.lock().library[set as usize].brushes[0].clone();
        let key = brush.engine.as_ref().unwrap().engine.clone();
        let id = data_id(&brush.engine.as_ref().unwrap().data);
        // chosen: its look starts being made
        let chosen = std::time::Instant::now();
        assert!(s.select_brush(set, 0));
        let at = |x: f32| crate::PenSample {
            x,
            y: 60.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: f64::from(x),
            roll: 0.0,
        };
        let mut longest = chosen.elapsed();
        let timed = |f: &dyn Fn()| {
            let t = std::time::Instant::now();
            f();
            t.elapsed()
        };
        longest = longest.max(timed(&|| s.stroke_begin(crate::PointerKind::Stylus, vec![at(20.0)])));
        let made_yet = hooks.look_kept(&key, id).is_some();
        longest = longest.max(timed(&|| s.stroke_add((21..=180).map(|x| at(x as f32)).collect())));
        longest = longest.max(timed(&|| s.stroke_end(vec![])));
        s.wait_for_look();
        let made_in = chosen.elapsed();
        println!("the look made in {made_in:.2?}; the interface's longest wait meanwhile {longest:.2?}");
        assert!(!made_yet, "the stroke began before the look was made");
        assert!(made_in > Duration::from_millis(500), "slow enough to tell: {made_in:?}");
        assert!(longest < Duration::from_millis(250), "{longest:?}");
        assert!(hooks.look_kept(&key, id).is_some(), "made");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A GIMP brush `w` x `h` in colour, all `v`.
    fn colour_gbr(w: u32, h: u32, v: u8) -> Vec<u8> {
        let mut b = Vec::new();
        for x in [28 + 2, 2, w, h, 4] {
            b.extend_from_slice(&u32::to_be_bytes(x));
        }
        b.extend_from_slice(b"GIMP");
        b.extend_from_slice(&25_u32.to_be_bytes());
        b.extend_from_slice(b"t\0");
        b.resize(b.len() + (w * h * 4) as usize, v);
        b
    }

    /// A bundle far too big for what drawing may have comes in through the Krita plugin with an
    /// import's: 24 presets, each with a tip of its own of 12 MB, 300 MB unpacked. The plugin's own
    /// code couldn't take it; the import does, every preset made, and the plugin stays on.
    #[test]
    fn a_bundle_too_big_for_drawing_s_limits_comes_in_with_an_import_s() {
        let dir = std::env::temp_dir().join(format!("procraft-big-bundle-{}", std::process::id()));
        let (_plugins, hooks, plugin, index, key) = krita_on(&dir);
        let mut zip = procraft_engine::zip::ZipWriter::new();
        for i in 0..24 {
            let tip = format!(r#"<Brush type="gbr_brush" filename="tip{i}.gbr" spacing="0.5" scale="1"/>"#);
            let preset = preset_named(&format!("Big {i}"), &tip);
            zip.add(&format!("paintoppresets/Big {i}.kpp"), &preset);
            zip.add(&format!("brushes/tip{i}.gbr"), &colour_gbr(1536, 2048, i as u8));
        }
        let bytes: Arc<[u8]> = zip.finish().into();
        let file = BrushFileBytes::Memory(Arc::clone(&bytes));
        // drawing's limits: the plugin's own code runs out of memory on it (and would go off)
        let (record, span) = file_record("Big.bundle", &file, usize::MAX).unwrap();
        let mut own = hooks.copier(&plugin).unwrap()(crate::plugin_run::MOST_LAYER_MEMORY).unwrap();
        let following = Following::default();
        let answer = own.brush_import(
            index,
            record,
            span,
            (crate::plugin_run::IMPORT_FUEL, Duration::from_secs(120)),
            &following,
        );
        assert_eq!(answer.map(|a| a.len()), Err(Stop::Memory));
        // an import's
        let t = std::time::Instant::now();
        let made =
            make_brushes_following(&hooks, "Big.bundle", &file, &following, limits(1 << 30, Duration::from_secs(300)))
                .unwrap();
        println!(
            "24 presets, 300 MB unpacked, made in {:?}, the copy's memory at most {} MB",
            t.elapsed(),
            made.memory >> 20
        );
        assert_eq!((made.made.len(), made.trouble), (24, None));
        assert!(made.memory > crate::plugin_run::MOST_LAYER_MEMORY, "more than drawing may have");
        assert!(made.made.iter().all(|m| m.data.len() > 12 << 20), "each with its tip");
        assert!(hooks.engine(&key).is_some(), "still on");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Krita preset called `name` whose tip is `brush` (a `Brush` element): a PNG with only its
    /// settings in it.
    fn preset_named(name: &str, brush: &str) -> Vec<u8> {
        preset_png(&format!(
            r#"<Preset name="{name}" paintopid="paintbrush"><param name="brush_definition" type="string"><![CDATA[{brush}]]></param></Preset>"#
        ))
    }

    /// A Krita preset of settings `xml`, kept as Krita keeps one: a PNG with only them in it.
    fn preset_png(xml: &str) -> Vec<u8> {
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(kind);
            c.extend_from_slice(data);
            c.extend_from_slice(&[0; 4]);
            c
        };
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = Vec::new();
        for v in [1_u32, 1] {
            header.extend_from_slice(&v.to_be_bytes());
        }
        header.extend_from_slice(&[8, 0, 0, 0, 0]);
        png.extend(chunk(b"IHDR", &header));
        let mut text = b"preset\0".to_vec();
        text.extend_from_slice(xml.as_bytes());
        png.extend(chunk(b"tEXt", &text));
        png.extend(chunk(b"IEND", &[]));
        png
    }

    /// The Krita plugin (the one the app comes with, or the one at `KRITA_PLUGIN`) installed and on
    /// in a store of its own at `dir`: the plugins, their hooks, and the engine's plugin, place and
    /// key.
    fn krita_on(dir: &std::path::Path) -> (Arc<crate::Plugins>, Arc<Hooks>, String, usize, String) {
        let packed = match std::env::var("KRITA_PLUGIN") {
            Ok(path) => std::fs::read(path).unwrap(),
            Err(_) => crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "krita-brushes").unwrap().1.to_vec(),
        };
        let _ = std::fs::remove_dir_all(dir);
        let plugins = crate::Plugins::with_store(dir.to_string_lossy().into_owned());
        assert_eq!(plugins.install(packed), None);
        let id = plugins.plugins().into_iter().find(|p| p.name == "Krita brushes").unwrap().id;
        assert!(plugins.set_on(id, true));
        let hooks = plugins.hooks();
        let (plugin, _, index, key) = hooks.engines_reading(Some("bundle")).into_iter().next().unwrap();
        (plugins, hooks, plugin, index, key)
    }

    fn megabytes(bytes: usize) -> f64 {
        bytes as f64 / f64::from(1 << 20)
    }

    /// A real Krita bundle through the Krita plugin, with numbers: as drawing's limits would take it,
    /// then with an import's; each brush made then read for its look on the plugin's own code, as
    /// the library's previews read it (`KRITA_FILE=x.bundle [KRITA_PLUGIN=x.procraft-plugin] cargo
    /// test -p procraft-core krita_bundle_calls -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn krita_bundle_calls() {
        use std::time::Instant;
        let Ok(path) = std::env::var("KRITA_FILE") else { return };
        let dir = std::env::temp_dir().join(format!("procraft-krita-calls-{}", std::process::id()));
        let (_plugins, hooks, plugin, index, key) = krita_on(&dir);
        let copy = hooks.copier(&plugin).unwrap();
        let file = BrushFileBytes::Disk(path.clone().into());
        let following = Following::default();
        for (what, memory) in
            [("drawing's", crate::plugin_run::MOST_LAYER_MEMORY), ("an import's", ImportLimits::now().memory)]
        {
            let (record, span) = file_record(&path, &file, usize::MAX).unwrap();
            let mut copied = copy(memory).unwrap();
            let t = Instant::now();
            let steps = (crate::plugin_run::IMPORT_FUEL, crate::plugin_run::MOST_IMPORT_TIME);
            let answer = copied.brush_import(index, record, span, steps, &following);
            println!(
                "with {what} memory ({:.0} MB): {:?} in {:.1?}, {} M steps, memory at most {:.0} MB",
                megabytes(memory),
                answer.map(|a| read_made(&a, &key).len()),
                t.elapsed(),
                copied.spent() / 1_000_000,
                megabytes(copied.memory_used())
            );
        }
        let t = Instant::now();
        let made = make_brushes_following(&hooks, &path, &file, &following, ImportLimits::now()).unwrap();
        let data: usize = made.made.iter().map(|m| m.data.len()).sum();
        println!(
            "made {} in {:.1?} ({:?}), the copy's memory at most {:.0} MB, the brushes' data {:.0} MB",
            made.made.len(),
            t.elapsed(),
            made.trouble,
            megabytes(made.memory),
            megabytes(data)
        );
        let mut brushes = Vec::new();
        join_made(&mut brushes, made.made);
        let (_, running, _) = hooks.engine(&key).unwrap();
        let (mut drawn, mut most) = (0, 0);
        for brush in &brushes {
            // made as a look is, on a copy of its own: what it takes, and whether the app takes it
            let outside = brush.engine.as_ref().unwrap();
            let id = data_id(&outside.data);
            let limits = LOOK_LIMITS.memory(ImportLimits::now().memory);
            let mut copied = copy(limits.memory).unwrap();
            let t = Instant::now();
            let answer =
                copied.brush_load_within(index, &load_record(id, &outside.data, false), (limits.steps, limits.time));
            let took = t.elapsed();
            let tips = answer
                .as_ref()
                .ok()
                .and_then(|a| a.as_ref())
                .and_then(|a| read_look(a, id))
                .map(|l| l.tips.layers.len());
            drawn += usize::from(tips.is_some());
            most = most.max(copied.memory_used());
            println!(
                "  look of {}: {tips:?} tips in {took:.1?}, {} M steps, its copy's memory {:.0} MB{}",
                brush.name,
                copied.spent() / 1_000_000,
                megabytes(copied.memory_used()),
                answer.err().map(|e| format!(", {e:?}")).unwrap_or_default()
            );
        }
        println!("{drawn} of {} drawn by the plugin; a look's copy at most {:.0} MB", brushes.len(), megabytes(most));
        for brush in &brushes {
            look_of(&hooks, brush, true);
        }
        let r = at_work(&running);
        println!(
            "the plugin still on: {}, its own memory {:.0} MB",
            hooks.engine(&key).is_some(),
            megabytes(r.memory_used())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Big real Krita bundles (`KRITA_BIG_PACKS`, their paths with `;` between: they are never kept
    /// here, and without it this passes over), read as the app reads a file the user picks: every
    /// preset comes in through the plugin, the plugin staying on, and stays on as the looks of its
    /// brushes are made.
    #[test]
    fn big_krita_bundles_come_in_through_the_plugin() {
        use std::time::Instant;
        let Ok(paths) = std::env::var("KRITA_BIG_PACKS") else { return };
        let dir = std::env::temp_dir().join(format!("procraft-krita-big-{}", std::process::id()));
        let (_plugins, hooks, _, _, key) = krita_on(&dir);
        let engines = crate::brush_files::BrushEngines::of(Some(Arc::clone(&hooks)));
        for path in paths.split(';').filter(|p| !p.trim().is_empty()) {
            let path = std::path::Path::new(path.trim());
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let presets = procraft_engine::zip::ZipReader::open(path)
                .unwrap()
                .names()
                .filter(|n| n.starts_with("paintoppresets/") && n.ends_with(".kpp"))
                .count();
            let reading = crate::BrushFileReading::default();
            let t = Instant::now();
            let read = crate::read_brushes_from(path, Some(name.clone()), &reading).unwrap();
            let read_in = t.elapsed();
            let t = Instant::now();
            let crate::ReadBrushes::Set { brushes, .. } = engines.make_following(read, &reading) else { panic!() };
            let notice = reading.notice().unwrap_or_default();
            println!("{name}: read by the app in {read_in:.1?}, by the plugin in {:.1?}: {notice}", t.elapsed());
            assert_eq!(
                brushes.iter().filter(|b| b.engine.is_some()).count(),
                presets,
                "every preset through the plugin"
            );
            assert_eq!(notice, format!("{presets} brushes imported."));
            assert!(hooks.engine(&key).is_some(), "the plugin stays on");
            // each brush's look, as the library's previews ask for them
            let t = Instant::now();
            let looks = brushes.iter().filter(|b| b.engine.is_some() && look_of(&hooks, b, true).is_some()).count();
            println!("  {looks} of {presets} looks made in {:.1?} (the rest drawn as the app read them)", t.elapsed());
            assert!(hooks.engine(&key).is_some(), "the plugin stays on as its brushes are drawn");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
