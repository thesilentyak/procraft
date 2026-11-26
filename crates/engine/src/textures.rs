//! Brush shape and grain images, generated here (ProCraft's own, made procedurally at start-up):
//! 8-bit greyscale, white = paint. Grains tile seamlessly. Those with detail finer than their
//! texels show can be drawn bigger too, the same pictures sharper, for a stamp or a tile bigger than
//! [`TEXTURE_SIZE`] (see [`shape_at`] and [`grain_at`]).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Edge of every shape and grain image as brushes hold it, pixels.
pub const TEXTURE_SIZE: usize = 256;

/// The most pixels a side our shapes and grains are drawn at, sharper, for a big stamp or tile.
pub const SHARPEST: usize = 2048;

/// A square greyscale image, `TEXTURE_SIZE` on a side, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrayImage {
    pub name: &'static str,
    pub pixels: Vec<u8>,
}

/// Shapes, by index (brushes refer to them by index).
pub mod shape {
    pub const SOFT_ROUND: u32 = 0;
    pub const GRAINY_DISC: u32 = 1;
    pub const CHUNK: u32 = 2;
    pub const BRISTLES: u32 = 3;
    pub const SPLATTER: u32 = 4;
    pub const FLAT: u32 = 5;
    pub const PASTEL: u32 = 6;
    pub const BLOT: u32 = 7;
    pub const LEAF: u32 = 8;
    pub const GRASS: u32 = 9;
    pub const SPARKLE: u32 = 10;
    pub const CLOUD: u32 = 11;
    pub const SQUARE: u32 = 12;
    pub const DRY: u32 = 13;
    pub const RING: u32 = 14;
    pub const SPRAY: u32 = 15;
    pub const HATCH: u32 = 16;
    pub const WISP: u32 = 17;
    pub const STAR: u32 = 18;
    pub const FUR: u32 = 19;
    pub const DROPLET: u32 = 20;
    pub const FLAME: u32 = 21;
    pub const BUBBLE: u32 = 22;
    pub const STONE: u32 = 23;
    pub const STREAK: u32 = 24;
    pub const FLOWER: u32 = 25;
    pub const TWIG: u32 = 26;
    pub const CRACK: u32 = 27;
    pub const SPONGE: u32 = 28;
    pub const TRIANGLE: u32 = 29;
    pub const DIAMOND: u32 = 30;
    pub const NEEDLES: u32 = 31;
    pub const FEATHER: u32 = 32;
    pub const CONFETTI: u32 = 33;
    pub const HAIR: u32 = 34;
    pub const SMUDGE: u32 = 35;
    pub const MOSS: u32 = 36;
    pub const HEART: u32 = 37;
    pub const SNOWFLAKE: u32 = 38;
    pub const SCRIBBLE: u32 = 39;
    pub const DRIP: u32 = 40;
    pub const BOLT: u32 = 41;
    pub const BLOCK: u32 = 42;
    pub const STOCK_GRAINY_DISC: u32 = 43;
    pub const STOCK_CHUNK: u32 = 44;
    pub const STOCK_PASTEL: u32 = 45;
    pub const STOCK_DAB: u32 = 46;
    pub const STOCK_FLAT_STROKE: u32 = 47;
    pub const STOCK_DOTS: u32 = 48;
    pub const STOCK_SQUARE_DOTS: u32 = 49;
    pub const STOCK_BLEED: u32 = 50;
    pub const STOCK_MESS: u32 = 51;
    pub const SLIM_OVAL: u32 = 52;
    pub const ULTRA_SOFT_ROUND: u32 = 53;
    pub const STOCK_BLOT: u32 = 54;
    pub const STOCK_HOOK: u32 = 55;
    pub const STOCK_GRAINY_SQUARE: u32 = 56;
    pub const STOCK_LINE: u32 = 57;
    pub const DAISY: u32 = 58;
    pub const FLICKS: u32 = 59;
    pub const SPINDLE: u32 = 60;
    pub const STUBBLE: u32 = 61;
}

/// Grains, by index.
pub mod grain {
    pub const PAPER: u32 = 0;
    pub const COARSE: u32 = 1;
    pub const WEAVE: u32 = 2;
    pub const FIBRE: u32 = 3;
    pub const LINEN: u32 = 4;
    pub const WASH: u32 = 5;
    pub const CONCRETE: u32 = 6;
    pub const HALFTONE: u32 = 7;
    pub const HATCHING: u32 = 8;
    pub const NOISE: u32 = 9;
    pub const CRUMPLED: u32 = 10;
    pub const STIPPLE: u32 = 11;
    pub const WOOD: u32 = 12;
    pub const BRICK: u32 = 13;
    pub const SCALES: u32 = 14;
    pub const CELLS: u32 = 15;
    pub const MARBLE: u32 = 16;
    pub const STRIPES: u32 = 17;
    pub const GRUNGE: u32 = 18;
    pub const SAND: u32 = 19;
    pub const HEXAGONS: u32 = 20;
    pub const CAUSTICS: u32 = 21;
    pub const LEATHER: u32 = 22;
    pub const BARK: u32 = 23;
    pub const SCRATCHES: u32 = 24;
    pub const CHECKER: u32 = 25;
    pub const SPECKLE: u32 = 26;
    pub const TWILL: u32 = 27;
    pub const RUST: u32 = 28;
    pub const RECYCLED: u32 = 29;
    pub const SHEET: u32 = 30;
    pub const WORN: u32 = 31;
    pub const CHARCOAL: u32 = 32;
    pub const FINE_NOISE: u32 = 33;
    pub const SPECKLED: u32 = 34;
    pub const MOTTLED: u32 = 35;
    pub const ROUGH_CANVAS: u32 = 36;
    pub const SPRAYED: u32 = 37;
    pub const DRY_PATCHES: u32 = 38;
    pub const GRAPH_PAPER: u32 = 39;
    pub const DOT_GRID: u32 = 40;
    pub const DOTTED_STAR: u32 = 41;
    pub const DIAGONAL_STRIPES: u32 = 42;
    pub const FINE_GRID: u32 = 43;
    pub const SOFT_DOTS: u32 = 44;
    pub const LABYRINTH: u32 = 45;
    pub const CUBES: u32 = 46;
    pub const SPOTS: u32 = 47;
    pub const DISC: u32 = 48;
    pub const SCROLLS: u32 = 49;
}

/// How many pixels wide the stock image our grain `index` is drawn to the proportions of is (a
/// pattern, not a texture like it), so that it stands in for it at the stock image's own tile
/// rather than by feature size; none for any other grain. Only a grain fixed to the canvas goes by
/// the width: one read stamp by stamp is read as the image is, at the moving grain's tile.
pub fn drawn_to_stock_width(index: u32) -> Option<f32> {
    match index {
        grain::GRAPH_PAPER | grain::DOT_GRID | grain::DOTTED_STAR | grain::LABYRINTH => Some(2048.0),
        grain::DIAGONAL_STRIPES | grain::FINE_GRID | grain::CUBES | grain::DISC => Some(1024.0),
        grain::SOFT_DOTS => Some(1792.0),
        grain::SPOTS => Some(2048.0),
        grain::SCROLLS => Some(1024.0),
        _ => None,
    }
}

/// Whether our grain `index` is drawn to a stock image's proportions (see
/// [`drawn_to_stock_width`]).
pub fn drawn_to_stock_size(index: u32) -> bool {
    drawn_to_stock_width(index).is_some()
}

/// How many shapes and grains there are: their indices run up to one under. Known without
/// making them (a library of brushes standing in with them is made long before it draws).
pub const SHAPE_COUNT: u32 = shape::STUBBLE + 1;
pub const GRAIN_COUNT: u32 = grain::SCROLLS + 1;

/// The Coarse grain's tooth: below this the fibres darken it, by this much a unit.
const COARSE_LIGHT: f32 = 0.6;
const COARSE_DEPTH: f32 = 1.25;

static SHAPES: std::sync::OnceLock<Vec<GrayImage>> = std::sync::OnceLock::new();
static GRAINS: std::sync::OnceLock<Vec<GrayImage>> = std::sync::OnceLock::new();

/// The shapes, made once.
pub fn shapes() -> Vec<GrayImage> {
    SHAPES.get_or_init(make_shapes).clone()
}

/// The grains, made once.
pub fn grains() -> Vec<GrayImage> {
    GRAINS.get_or_init(make_grains).clone()
}

/// Shape `index`, if there is one (without copying the others).
pub fn shape_image(index: u32) -> Option<GrayImage> {
    SHAPES.get_or_init(make_shapes).get(index as usize).cloned()
}

/// Grain `index`, if there is one (without copying the others).
pub fn grain_image(index: u32) -> Option<GrayImage> {
    GRAINS.get_or_init(make_grains).get(index as usize).cloned()
}

/// Shape `index` drawn `side` pixels across (a power of two from [`TEXTURE_SIZE`] up to
/// [`SHARPEST`]): at [`TEXTURE_SIZE`] the one brushes hold; bigger, the same picture sharper, for a
/// stamp bigger than that (worth it only for those [`shape_sharpens`] says). None for any other
/// side. Slow at the biggest sides: drawn for a brush that wants it, off the thread that draws.
pub fn shape_at(index: u32, side: usize) -> Option<Vec<u8>> {
    if side == TEXTURE_SIZE {
        return shape_image(index).map(|s| s.pixels);
    }
    sharper_side(side).then(|| draw_shape(index, side)).flatten()
}

/// Grain `index` drawn `side` pixels across, as [`shape_at`] draws a shape: the same tile, sharper,
/// for a tile bigger than [`TEXTURE_SIZE`] on the canvas (see [`grain_sharpens`]).
pub fn grain_at(index: u32, side: usize) -> Option<Vec<u8>> {
    if side == TEXTURE_SIZE {
        return grain_image(index).map(|g| g.pixels);
    }
    sharper_side(side).then(|| draw_grain(index, side)).flatten()
}

/// Whether `side` is one our shapes and grains are drawn sharper at.
fn sharper_side(side: usize) -> bool {
    side.is_power_of_two() && side > TEXTURE_SIZE && side <= SHARPEST
}

/// Whether our shape `index` shows more drawn bigger than [`TEXTURE_SIZE`]: crisp edges, fine
/// lines or marks, noise down to about a texel. The rest are soft all through (a soft round, a
/// cloud, a smudge, a chunk's broad lumps): drawn four times as big, they come within a fifth of a
/// level of themselves magnified (most within a twentieth; a block, a star, a hook a level or more,
/// up to a fifth of full at their edges).
pub fn shape_sharpens(index: u32) -> bool {
    let soft = [
        shape::SOFT_ROUND,
        shape::CHUNK,
        shape::BRISTLES,
        shape::SPLATTER,
        shape::FLAT,
        shape::BLOT,
        shape::SPARKLE,
        shape::CLOUD,
        shape::WISP,
        shape::SMUDGE,
        shape::STOCK_GRAINY_DISC,
        shape::STOCK_CHUNK,
        shape::STOCK_DAB,
        shape::ULTRA_SOFT_ROUND,
        shape::STOCK_BLOT,
    ];
    index < SHAPE_COUNT && !soft.contains(&index)
}

/// Whether our grain `index` shows more drawn bigger than [`TEXTURE_SIZE`] (see
/// [`shape_sharpens`]). Those made of noise a texel across (each texel its own speck, as a fine
/// paper's grain is a pixel or two, and as a stock grain's is where they stand in for one) have
/// nothing finer to show: drawn bigger they would be other noise, as fine in texels, so finer than
/// the grain they stand in for.
pub fn grain_sharpens(index: u32) -> bool {
    let noise = [grain::NOISE, grain::SHEET, grain::FINE_NOISE, grain::SPECKLED, grain::MOTTLED, grain::SPRAYED];
    index < GRAIN_COUNT && !noise.contains(&index)
}

/// How many times finer than [`TEXTURE_SIZE`] an image `side` pixels across is. Drawn at it, an
/// edge whose ramp is a few texels at [`TEXTURE_SIZE`] (a crisp edge, smoothed so that it doesn't
/// step) is made this many times steeper, so that it is as many texels wide at every side: the
/// edges our pictures soften on purpose (a soft round's falloff, a rim's glow) stay as they are,
/// and so do those under a texel there, which drawn bigger are already resolved.
fn zoom_of(side: usize) -> f32 {
    side as f32 / TEXTURE_SIZE as f32
}

/// How many octaves more noise whose finest octave is about a texel at [`TEXTURE_SIZE`] goes on
/// for at `side`: one more each time the side doubles, so that its finest stays about a texel.
fn octaves_more(side: usize) -> u32 {
    (side / TEXTURE_SIZE).max(1).ilog2()
}

/// The shares of a grain at which [`stock_grain_tones`] gives its values: none, 5%, 10%, a
/// quarter, half, three quarters, 90%, 95% and all of it.
pub const TONE_SHARES: [f32; 9] = [0.0, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 1.0];

/// The shares of a shape at which [`stock_shape_tones`] gives its values: every sixteenth.
pub const SHAPE_TONE_SHARES: [f32; 17] = [
    0.0, 0.0625, 0.125, 0.1875, 0.25, 0.3125, 0.375, 0.4375, 0.5, 0.5625, 0.625, 0.6875, 0.75, 0.8125, 0.875, 0.9375,
    1.0,
];

/// The tones of the stock grain a brush file names `name` (its path or file name) instead of
/// carrying it, as measured: its values at [`TONE_SHARES`], if it is one we know.
pub fn stock_grain_tones(name: &str) -> Option<[u8; 9]> {
    stock_grain_tones_of(stock_key(name))
}

/// [`stock_grain_tones`] of the stock grain whose measurements are kept under `key` (see
/// [`stock_key`]).
pub fn stock_grain_tones_of(key: u64) -> Option<[u8; 9]> {
    stock_tones(crate::stock_tones::STOCK_GRAIN_TONES, key)
}

/// The sides at which [`stock_grain_features`] gives a stock grain's feature size: the image as a
/// tile that size on the canvas shows it (no finer than its own).
pub const FEATURE_SIDES: [f32; 4] = [1024.0, 512.0, 256.0, 128.0];

/// How big the features of the stock grain a brush file names `name` are, as a share of its side
/// (how far it shifts before it matches itself only half), at each of [`FEATURE_SIDES`], as
/// measured, if it is one we know.
pub fn stock_grain_features(name: &str) -> Option<[f32; 4]> {
    stock_grain_features_of(stock_key(name))
}

/// [`stock_grain_features`] of the stock grain whose measurements are kept under `key`.
pub fn stock_grain_features_of(key: u64) -> Option<[f32; 4]> {
    let table = crate::stock_tones::STOCK_GRAIN_FEATURES;
    let at = table.binary_search_by_key(&key, |&(k, _)| k).ok()?;
    table.get(at).map(|&(_, v)| v.map(|v| f32::from(v) / 65536.0))
}

/// How coarse the texture of the stock shape a brush file names `name` is, as measured: the feature
/// size of what is left of it with its broad outline taken away, as a share of its side; `Some(None)`
/// for one with no texture of its own (solid or smooth), none if it is one we don't know.
pub fn stock_shape_feature(name: &str) -> Option<Option<f32>> {
    stock_shape_feature_of(stock_key(name))
}

/// [`stock_shape_feature`] of the stock shape whose measurements are kept under `key`.
pub fn stock_shape_feature_of(key: u64) -> Option<Option<f32>> {
    let table = crate::stock_tones::STOCK_SHAPE_FEATURES;
    let at = table.binary_search_by_key(&key, |&(k, _)| k).ok()?;
    table.get(at).map(|&(_, [v])| (v < u16::MAX).then(|| f32::from(v) / 65536.0))
}

/// The grainy square (shape [`shape::STOCK_GRAINY_SQUARE`]) with a texture as coarse as `feature`
/// (a share of its side, as [`stock_shape_feature`] measures it), or smooth for none, its texture
/// as strong as `cover` (the share of the real tip that is ink) wants: a tip of few marks takes it
/// at full contrast, so that once its tones are put on it its marks fall scattered where the
/// texture says; a fuller tip at under half, so that its soft edge stays below its middle and its
/// marks don't open into holes. What a stock tip stands in as. Made once for each size and
/// strength of mark.
pub fn grainy_square_of(feature: Option<f32>, cover: f32) -> Vec<u8> {
    grainy_square_at(feature, cover, TEXTURE_SIZE)
}

/// [`grainy_square_of`] drawn `side` pixels across (as [`shape_at`] draws a shape), its marks as
/// big a share of it: made again each time at more than [`TEXTURE_SIZE`].
pub fn grainy_square_at(feature: Option<f32>, cover: f32, side: usize) -> Vec<u8> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock, PoisonError};
    // each made, by the half width of its marks and their strength
    type Made = Mutex<HashMap<(u32, u32), Vec<u8>>>;
    static MADE: OnceLock<Made> = OnceLock::new();
    // the mark's half width in texels, to a quarter (0: smooth)
    let half = feature.map_or(0, |f| ((f * TEXTURE_SIZE as f32 * 4.0).round() as u32).clamp(1, 4 * 64));
    // how strong the texture is: full for a twentieth of ink or less, 0.45 from a fifth up, in
    // twentieths
    let strength = 0.45 + 0.55 * ((0.2 - cover) / 0.15).clamp(0.0, 1.0);
    let key = (half, (strength * 20.0).round() as u32);
    let strength = key.1 as f32 / 20.0;
    // coarser than the square's own marks: those, which have finer detail in them too (one
    // softening at that size alone would leave smooth blobs where a coarse tip has streaks)
    if half > (COARSEST_OWN_MARK * TEXTURE_SIZE as f32 * 4.0) as u32 {
        return shape_at(shape::STOCK_GRAINY_SQUARE, side).unwrap_or_default();
    }
    let made = MADE.get_or_init(|| Mutex::new(HashMap::new()));
    let kept = side == TEXTURE_SIZE;
    if kept && let Some(pixels) = made.lock().unwrap_or_else(PoisonError::into_inner).get(&key) {
        return pixels.clone();
    }
    let n = side;
    let texture: Vec<f32> = if half == 0 {
        // smooth: brightest in the middle, a little less out to the edge (a flat middle would leave
        // its tones to fall anywhere in it)
        (0..n * n)
            .map(|k| {
                let (x, y) = (((k % n) as f32 + 0.5) / n as f32, ((k / n) as f32 + 0.5) / n as f32);
                1.0 - radius(x, y).min(1.0)
            })
            .collect()
    } else {
        marks_at(half, n)
    };
    let zoom = zoom_of(n);
    let pixels = field(n, |x, y| {
        let (u, v) = ((x - 0.5).abs() * 2.0, (y - 0.5).abs() * 2.0);
        let r = (u.powi(4) + v.powi(4)).powf(0.25);
        let (i, j) = (((x * n as f32) as usize).min(n - 1), ((y * n as f32) as usize).min(n - 1));
        let t = texture.get(j * n + i).copied().unwrap_or(1.0);
        edge((0.96 - r) * 24.0, zoom) * (1.0 - strength + strength * t)
    });
    if kept {
        made.lock().unwrap_or_else(PoisonError::into_inner).insert(key, pixels.clone());
    }
    pixels
}

/// `pixels`, one of our shapes standing in for a stock tip (square, of any side), given a texture
/// as fine as `feature` (as [`stock_shape_feature`] measures it, a share of the side) as strongly as
/// `cover` wants (see [`grainy_square_of`]): a smooth shape standing in for a textured tip takes its
/// marks. None where there is nothing to add (no texture, or one coarser than
/// [`COARSEST_OWN_MARK`]).
pub fn textured_shape(pixels: &[u8], feature: Option<f32>, cover: f32) -> Option<Vec<u8>> {
    let f = feature?;
    let n = pixels.len().isqrt();
    let half = ((f * TEXTURE_SIZE as f32 * 4.0).round() as u32).clamp(1, 4 * 64);
    if half > (COARSEST_OWN_MARK * TEXTURE_SIZE as f32 * 4.0) as u32 || n < TEXTURE_SIZE || pixels.len() != n * n {
        return None;
    }
    let strength = 0.45 + 0.55 * ((0.2 - cover) / 0.15).clamp(0.0, 1.0);
    let texture = marks_at(half, n);
    Some(
        pixels
            .iter()
            .zip(&texture)
            .map(|(&v, &t)| (f32::from(v) * (1.0 - strength + strength * t)).round().clamp(0.0, 255.0) as u8)
            .collect(),
    )
}

/// White noise softened until it matches itself only half at `half` quarter texels (a Gaussian's
/// softening of sigma s does at about 1.665 s), wrapping round, its values spread evenly over 0..1:
/// marks of that size. Made once for each.
fn marks(half: u32) -> Vec<f32> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock, PoisonError};
    static MADE: OnceLock<Mutex<HashMap<u32, Vec<f32>>>> = OnceLock::new();
    let made = MADE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(texture) = made.lock().unwrap_or_else(PoisonError::into_inner).get(&half) {
        return texture.clone();
    }
    let even = marks_drawn(half, TEXTURE_SIZE);
    made.lock().unwrap_or_else(PoisonError::into_inner).insert(half, even.clone());
    even
}

/// [`marks`] for an image `side` pixels across, as big a share of it (`half` is still in quarter
/// texels at [`TEXTURE_SIZE`]). Marks a texel and a half across or more there are the same marks at
/// every side, read between their texels (softened noise is smooth at that size); finer ones are
/// drawn as fine at the least side that shows them, and read between its texels from there: other
/// marks, as fine.
fn marks_at(half: u32, side: usize) -> Vec<f32> {
    if side <= TEXTURE_SIZE {
        return marks(half);
    }
    let sigma = (half as f32 / 4.0) / 1.665;
    let wanted = (1.5 / sigma.max(1e-3) * TEXTURE_SIZE as f32).ceil() as usize;
    let at = wanted.next_power_of_two().clamp(TEXTURE_SIZE, side);
    let drawn = if at == TEXTURE_SIZE { marks(half) } else { marks_drawn(half * (at / TEXTURE_SIZE) as u32, at) };
    upsampled(&drawn, at, side)
}

/// White noise `n` texels square softened until it matches itself only half at `half` quarter
/// texels, its values spread evenly over 0..1 (see [`marks`]).
fn marks_drawn(half: u32, n: usize) -> Vec<f32> {
    let sigma = (half as f32 / 4.0) / 1.665;
    let raw: Vec<f32> = (0..n * n).map(|k| hash((k % n) as i32, (k / n) as i32, 6101)).collect();
    let soft = if sigma < 0.35 { raw } else { gaussian_wrapped(&raw, n, sigma) };
    // in order of value, equal ones in order of place (as a stable sort by value)
    let mut order: Vec<(u32, u32)> = soft.iter().map(|&v| total_order(v)).zip(0..).collect();
    order.sort_unstable();
    let mut even = vec![0.0; n * n];
    for (rank, &(_, k)) in order.iter().enumerate() {
        if let Some(v) = even.get_mut(k as usize) {
            *v = rank as f32 / (n * n - 1) as f32;
        }
    }
    even
}

/// `values` (a square `from` texels across, tiling) read between its texels (Catmull-Rom across
/// and down, wrapping round) `to` texels across: as it is when `to` is `from`.
fn upsampled(values: &[f32], from: usize, to: usize) -> Vec<f32> {
    if to == from || from == 0 || values.len() != from * from {
        return values.to_vec();
    }
    // where each texel of the bigger one falls in the smaller, and the weights of the four texels
    // about it
    let taps: Vec<([usize; 4], [f32; 4])> = (0..to)
        .map(|i| {
            let at = (i as f32 + 0.5) * from as f32 / to as f32 - 0.5;
            let base = at.floor();
            let t = at - base;
            let w = [
                ((-t + 2.0) * t - 1.0) * t / 2.0,
                ((3.0 * t - 5.0) * t * t + 2.0) / 2.0,
                ((-3.0 * t + 4.0) * t + 1.0) * t / 2.0,
                (t - 1.0) * t * t / 2.0,
            ];
            let k = |d: i32| (base as i32 + d).rem_euclid(from as i32) as usize;
            ([k(-1), k(0), k(1), k(2)], w)
        })
        .collect();
    let read = |x: usize, y: usize| values.get(y * from + x).copied().unwrap_or(0.0);
    // across first (each of the smaller's rows), then down
    let mut rows = vec![0.0f32; from * to];
    for y in 0..from {
        for (i, (xs, w)) in taps.iter().enumerate() {
            if let Some(v) = rows.get_mut(y * to + i) {
                *v = xs.iter().zip(w).map(|(&x, w)| w * read(x, y)).sum();
            }
        }
    }
    let row = |x: usize, y: usize| rows.get(y * to + x).copied().unwrap_or(0.0);
    let mut out = vec![0.0f32; to * to];
    for (j, (ys, w)) in taps.iter().enumerate() {
        for i in 0..to {
            if let Some(v) = out.get_mut(j * to + i) {
                *v = ys.iter().zip(w).map(|(&y, w)| w * row(i, y)).sum();
            }
        }
    }
    out
}

/// The coarsest marks [`grainy_square_of`] makes, as a share of its side: past them the grainy
/// square's own (about as coarse, with finer detail in them) stand in.
const COARSEST_OWN_MARK: f32 = 0.04;

/// `values` (an `n` square) softened by a Gaussian of `sigma` texels across and down, wrapping round.
fn gaussian_wrapped(values: &[f32], n: usize, sigma: f32) -> Vec<f32> {
    let reach = (sigma * 3.0).ceil() as i32;
    let weights: Vec<f32> = (-reach..=reach).map(|d| (-(d * d) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let total: f32 = weights.iter().sum();
    // each pixel worked out as it was a pixel at a time, bands of rows side by side
    let pass = |src: &[f32], across: bool| -> Vec<f32> {
        let mut out = vec![0.0; n * n];
        let rows = n.div_ceil(crate::par::cores()).max(1);
        std::thread::scope(|s| {
            for (band, part) in out.chunks_mut(rows * n).enumerate() {
                let weights = &weights;
                crate::par::beside(s, move || {
                    for (at, px) in part.iter_mut().enumerate() {
                        let k = band * rows * n + at;
                        let (i, j) = ((k % n) as i32, (k / n) as i32);
                        let mut v = 0.0;
                        for (w, d) in weights.iter().zip(-reach..=reach) {
                            let (x, y) = if across {
                                ((i + d).rem_euclid(n as i32), j)
                            } else {
                                (i, (j + d).rem_euclid(n as i32))
                            };
                            v += w * src.get(y as usize * n + x as usize).copied().unwrap_or(0.0);
                        }
                        *px = v / total;
                    }
                });
            }
        });
        out
    };
    pass(&pass(values, true), false)
}

/// The feature size `features` (see [`stock_grain_features`]) shows at a tile `tile` canvas pixels
/// on a side: between the levels it falls between, by their sides' logarithms.
pub fn feature_at_tile(features: &[f32; 4], tile: f32) -> f32 {
    let level = (FEATURE_SIDES[0].log2() - tile.max(1.0).log2()).clamp(0.0, (FEATURE_SIDES.len() - 1) as f32);
    let low = level.floor() as usize;
    let f = level - low as f32;
    let at = |i: usize| features.get(i.min(features.len() - 1)).copied().unwrap_or(0.0);
    at(low) * (1.0 - f) + at(low + 1) * f
}

/// How big the features of our grain `index` are, as [`stock_grain_feature`] measures a stock
/// one, worked out the first time it is asked for: none for a grain with no pattern.
pub fn grain_feature(index: u32) -> Option<f32> {
    static FEATURES: std::sync::OnceLock<Vec<std::sync::OnceLock<Option<f32>>>> = std::sync::OnceLock::new();
    let all = FEATURES.get_or_init(|| (0..grains().len()).map(|_| std::sync::OnceLock::new()).collect());
    let slot = all.get(index as usize)?;
    *slot.get_or_init(|| {
        let grain = GRAINS.get_or_init(make_grains).get(index as usize)?;
        feature_share(&grain.pixels, TEXTURE_SIZE)
    })
}

/// How far a square tile `n` on a side shifts (wrapping round) before it matches itself only half
/// (its autocorrelation falls to 0.5), across and down averaged, as a share of its side; none for
/// a flat one.
fn feature_share(pixels: &[u8], n: usize) -> Option<f32> {
    if pixels.len() != n * n || n == 0 {
        return None;
    }
    let mean = pixels.iter().map(|&v| f64::from(v)).sum::<f64>() / pixels.len() as f64;
    let g: Vec<f64> = pixels.iter().map(|&v| f64::from(v) - mean).collect();
    let power: f64 = g.iter().map(|v| v * v).sum();
    if power < 1e-6 * pixels.len() as f64 {
        return None;
    }
    let shifted = |k: usize, across: bool| -> f64 {
        let mut sum = 0.0;
        for y in 0..n {
            for x in 0..n {
                let (sx, sy) = if across { ((x + k) % n, y) } else { (x, (y + k) % n) };
                let at = |i: usize| g.get(i).copied().unwrap_or(0.0);
                sum += at(y * n + x) * at(sy * n + sx);
            }
        }
        sum / power
    };
    let half = |across: bool| -> f64 {
        let mut before = 1.0;
        for k in 1..n / 2 {
            let now = shifted(k, across);
            if now <= 0.5 {
                return ((k - 1) as f64 + (before - 0.5) / (before - now).max(1e-9)) / n as f64;
            }
            before = now;
        }
        0.5
    };
    Some(((half(true) + half(false)) / 2.0) as f32)
}

/// The tones of the stock shape a brush file names `name`, as measured: its values at
/// [`SHAPE_TONE_SHARES`] (of it inverted, for a brush that inverts it), if it is one we know.
pub fn stock_shape_tones(name: &str, inverted: bool) -> Option<[u8; 17]> {
    stock_shape_tones_of(stock_key(name), inverted)
}

/// [`stock_shape_tones`] of the stock shape whose measurements are kept under `key`.
pub fn stock_shape_tones_of(key: u64, inverted: bool) -> Option<[u8; 17]> {
    let tones = stock_tones(crate::stock_tones::STOCK_SHAPE_TONES, key)?;
    // inverted, the darkest share is what was the lightest (the shares run evenly both ways)
    Some(if inverted {
        std::array::from_fn(|i| 255 - tones.get(tones.len() - 1 - i).copied().unwrap_or(0))
    } else {
        tones
    })
}

/// How many pixels across the stock shape whose measurements are kept under `key` is (its longer
/// side), if it is one we know: our stand-in for it is drawn sharper for a big stamp only as far as
/// that, as blurred as the image itself past it.
pub fn stock_shape_side(key: u64) -> Option<usize> {
    stock_tones(crate::stock_tones::STOCK_SHAPE_SIDES, key).map(|[side]| usize::from(side))
}

/// How many pixels across the stock grain whose measurements are kept under `key` is, if it is
/// one we know (see [`stock_shape_side`]).
pub fn stock_grain_side(key: u64) -> Option<usize> {
    stock_tones(crate::stock_tones::STOCK_GRAIN_OCTAVES, key).map(|[width, ..]| usize::from(width))
}

/// How much ink each of 64 bands of the stock shape `name`'s rows holds, top to bottom, as shares
/// of its fullest (0..255), as measured (of it inverted, for a brush that inverts it), if it is
/// one we know.
pub fn stock_shape_rows(name: &str, inverted: bool) -> Option<[u8; 64]> {
    stock_shape_rows_of(stock_key(name), inverted)
}

/// [`stock_shape_rows`] of the stock shape whose measurements are kept under `key`.
pub fn stock_shape_rows_of(key: u64, inverted: bool) -> Option<[u8; 64]> {
    stock_tones(crate::stock_tones::STOCK_SHAPE_ROWS, key).map(|means| band_shares(&means, inverted))
}

/// The same of its columns, left to right.
pub fn stock_shape_cols(name: &str, inverted: bool) -> Option<[u8; 64]> {
    stock_shape_cols_of(stock_key(name), inverted)
}

/// [`stock_shape_cols`] of the stock shape whose measurements are kept under `key`.
pub fn stock_shape_cols_of(key: u64, inverted: bool) -> Option<[u8; 64]> {
    stock_tones(crate::stock_tones::STOCK_SHAPE_COLS, key).map(|means| band_shares(&means, inverted))
}

/// Bands' mean values as the ink each holds, as shares of the fullest (0..255): the means
/// themselves, or what they leave of white for an inverted image.
fn band_shares(means: &[u8; 64], inverted: bool) -> [u8; 64] {
    let ink = means.map(|m| if inverted { 255 - m } else { m });
    let top = u32::from(ink.iter().copied().max().unwrap_or(0).max(1));
    ink.map(|v| ((u32::from(v) * 255 + top / 2) / top).min(255) as u8)
}

/// A halftone tip: round dots on a lattice, the lattice's two sets of rows as the cycles of each
/// across and down the tile (`lattice`), each dot as big as the ink there says: the ink a stock tip
/// holds row by row and column by column (`rows`, `cols`, as [`stock_shape_rows`] gives them),
/// all of it `cover` of the tile on average. A stand-in for a stock tip printed in dots, whose
/// stroke shows their rows where the stamps overlap.
pub fn halftone_shape(lattice: [(f32, f32); 2], rows: &[u8; 64], cols: &[u8; 64], cover: f32) -> Vec<u8> {
    halftone_shape_at(lattice, rows, cols, cover, TEXTURE_SIZE)
}

/// [`halftone_shape`] drawn `side` pixels across, its dots' edges as crisp in texels.
pub fn halftone_shape_at(
    lattice: [(f32, f32); 2],
    rows: &[u8; 64],
    cols: &[u8; 64],
    cover: f32,
    side: usize,
) -> Vec<u8> {
    let [(ax, ay), (bx, by)] = lattice;
    let det = ax * by - ay * bx;
    if det.abs() < 1e-6 {
        return vec![0; side * side];
    }
    // the area of the tile each dot has (a share of the tile)
    let cell = 1.0 / det.abs();
    let band = |bands: &[u8; 64], t: f32| {
        let place = (t * bands.len() as f32 - 0.5).clamp(0.0, (bands.len() - 1) as f32);
        let (low, f) = (place.floor() as usize, place.fract());
        let at = |i: usize| f32::from(bands.get(i.min(bands.len() - 1)).copied().unwrap_or(0)) / 255.0;
        at(low) * (1.0 - f) + at(low + 1) * f
    };
    // how much of its cell each dot fills, from the ink across and down, scaled until the whole
    // averages `cover` (held to the cell: past it the dots run together)
    let n = TEXTURE_SIZE;
    let ink: Vec<f32> = (0..n * n)
        .map(|k| {
            let (x, y) = (((k % n) as f32 + 0.5) / n as f32, ((k / n) as f32 + 0.5) / n as f32);
            (band(rows, y) * band(cols, x)).sqrt()
        })
        .collect();
    let target = cover.clamp(0.0, 1.0) * ink.len() as f32;
    let filled = |k: f32| ink.iter().map(|&v| (v * k).min(1.0)).sum::<f32>();
    let (mut low, mut high) = (0.0f32, 64.0f32);
    for _ in 0..40 {
        let mid = (low + high) / 2.0;
        if filled(mid) < target { low = mid } else { high = mid }
    }
    let scale = (low + high) / 2.0;
    field(side, |x, y| {
        // the nearest dot: the point's place in each set of rows, to the nearest whole row, back
        // across the tile
        let (p, q) = (ax * x + ay * y, bx * x + by * y);
        let (dp, dq) = (p - p.round(), q - q.round());
        let (dx, dy) = ((by * dp - ay * dq) / det, (ax * dq - bx * dp) / det);
        let distance = dx.hypot(dy);
        let (i, j) = (((x * n as f32) as usize).min(n - 1), ((y * n as f32) as usize).min(n - 1));
        let share = (ink.get(j * n + i).copied().unwrap_or(0.0) * scale).min(1.0);
        let radius = (share * cell / std::f32::consts::PI).sqrt();
        ((radius - distance) * side as f32 + 0.5).clamp(0.0, 1.0)
    })
}

/// How much of its square the hard round tip's picture fills across: as much as the stock hard
/// tip's ink does (its rows and columns measured, 64 bands of each: 0.796 across the middle, 0.797
/// by the ink they hold). A stamp's shape fills its quad, which reaches a little past the stamp,
/// so a stroke with it is as wide as the stock tip's; the round tip drawn by its hardness fills its
/// stamp alone, and stands in at a share a little bigger ([`crate::brushfile::ROUND_TIP_SHARE`]).
pub const HARD_ROUND_SHARE: f32 = 0.797;

/// The hard round tip as a picture `side` pixels square: a disc [`HARD_ROUND_SHARE`] of its square
/// across, its edge a texel wide at any side. Read through its smaller levels as a stamp reads a
/// shape, its edge is about a canvas pixel and a half wide at every size the picture covers, as a
/// stock hard tip's is, where the round tip drawn by its hardness is crisper small and softer big.
pub fn hard_round_at(side: usize) -> Vec<u8> {
    let radius = HARD_ROUND_SHARE / 2.0;
    field(side, |x, y| ((radius - (x - 0.5).hypot(y - 0.5)) * side as f32 + 0.5).clamp(0.0, 1.0))
}

/// How far out the small point's dot paints half (a share of its square's side): where the stock
/// small point's ink, as its rows measure it (64 bands, averaged down to 256 square), puts it.
pub const POINT_RADIUS: f32 = 0.135;

/// How soft the small point's edge is: the spread of the blur across it, a share of its square's
/// side. It falls from nine tenths to a tenth over 2.56 times this (5.4% of a stamp's square), as
/// far as the stock small point's edge was measured to at every size from 20 to 1000 pixels.
pub const POINT_BLUR: f32 = 0.0213;

/// The small point as a picture `side` pixels square: a dot [`POINT_RADIUS`] of the side out, its
/// edge blurred over [`POINT_BLUR`] of it (a Gaussian's), as wide, as soft and holding as much ink
/// as the stock small point measures. Soft all through, so no bigger picture shows more.
pub fn point_at(side: usize) -> Vec<u8> {
    let spread = POINT_BLUR * std::f32::consts::SQRT_2;
    field(side, |x, y| 0.5 * (1.0 - crate::krita::erf(((x - 0.5).hypot(y - 0.5) - POINT_RADIUS) / spread)))
}

/// `pixels`, a shape, stretched across and down so that the bands of its rows and columns holding
/// ink span as many as `rows` and `cols` (a stock tip's, as [`stock_shape_rows`] gives them) say
/// theirs do, and lie where theirs lie: a stand-in as wide and tall as the tip it stands for. Only
/// ever grown: a soft tip's faint rim, under a twentieth of its fullest band, still paints, so a tip
/// that measures smaller than ours is no sure sign that ours reaches too far. None where it already
/// is (within a twentieth), or where either holds no ink. (Square, of any side a multiple of 64.)
pub fn fitted_to_box(pixels: &[u8], rows: &[u8; 64], cols: &[u8; 64]) -> Option<Vec<u8>> {
    let n = pixels.len().isqrt();
    if pixels.len() != n * n || !n.is_multiple_of(rows.len()) || n == 0 {
        return None;
    }
    let per = n / rows.len();
    let mut ours_rows = [0.0f32; 64];
    let mut ours_cols = [0.0f32; 64];
    for (i, &v) in pixels.iter().enumerate() {
        if let Some(r) = ours_rows.get_mut(i / n / per) {
            *r += f32::from(v);
        }
        if let Some(c) = ours_cols.get_mut(i % n / per) {
            *c += f32::from(v);
        }
    }
    // the first and last band holding a twentieth of the fullest band's ink, as texels (end past)
    let span = |bands: &[f32]| {
        let top = bands.iter().copied().fold(0.0, f32::max);
        if top <= 0.0 {
            return None;
        }
        let first = bands.iter().position(|&b| b > top * 0.05)?;
        let last = bands.iter().rposition(|&b| b > top * 0.05)?;
        Some(((first * per) as f32, ((last + 1) * per) as f32))
    };
    let theirs = |bands: &[u8; 64]| span(&bands.map(f32::from));
    let ((oy0, oy1), (ox0, ox1)) = (span(&ours_rows)?, span(&ours_cols)?);
    let ((ty0, ty1), (tx0, tx1)) = (theirs(rows)?, theirs(cols)?);
    // how far ours stretches each way (grown, to twice at most), about the middles of the two boxes
    let (sx, sy) = (((ox1 - ox0) / (tx1 - tx0)).clamp(0.5, 1.0), ((oy1 - oy0) / (ty1 - ty0)).clamp(0.5, 1.0));
    let (ocx, ocy, tcx, tcy) = ((ox0 + ox1) / 2.0, (oy0 + oy1) / 2.0, (tx0 + tx1) / 2.0, (ty0 + ty1) / 2.0);
    let off = |a: f32, b: f32| (a - b).abs() / n as f32;
    if sx > 0.95 && sy > 0.95 && off(ocx, tcx) < 0.05 && off(ocy, tcy) < 0.05 {
        return None;
    }
    let at = |x: i32, y: i32| {
        if x < 0 || y < 0 || x >= n as i32 || y >= n as i32 {
            0.0
        } else {
            f32::from(pixels.get(y as usize * n + x as usize).copied().unwrap_or(0))
        }
    };
    Some(
        (0..n * n)
            .map(|k| {
                // where in ours this texel of theirs falls, read between texels
                let (x, y) = ((k % n) as f32 + 0.5, (k / n) as f32 + 0.5);
                let (u, v) = (ocx + (x - tcx) * sx - 0.5, ocy + (y - tcy) * sy - 0.5);
                let (i, j) = (u.floor() as i32, v.floor() as i32);
                let (fx, fy) = (u - u.floor(), v - v.floor());
                let top = at(i, j) * (1.0 - fx) + at(i + 1, j) * fx;
                let bottom = at(i, j + 1) * (1.0 - fx) + at(i + 1, j + 1) * fx;
                (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8
            })
            .collect(),
    )
}

/// `pixels`, a shape, its rows weighted as [`striped`] weights them, then its columns as `cols`
/// says the same way: a stand-in takes a stock shape's outline across as well as down.
pub fn shaped(pixels: &[u8], rows: &[u8; 64], cols: &[u8; 64]) -> Vec<u8> {
    let n = pixels.len().isqrt();
    let down = striped(pixels, rows);
    let turned =
        |p: &[u8]| -> Vec<u8> { (0..n * n).map(|k| p.get((k % n) * n + k / n).copied().unwrap_or(0)).collect() };
    turned(&striped(&turned(&down), cols))
}

/// `pixels`, a shape, its rows weighted so that how much ink each band of them holds goes as
/// `rows` says (a stock shape's: 64 bands, top to bottom), between band middles smoothly: a
/// stand-in takes the stock shape's streaks and gaps across a stroke. (Square, of any side a
/// multiple of 64.)
pub fn striped(pixels: &[u8], rows: &[u8; 64]) -> Vec<u8> {
    let n = pixels.len().isqrt();
    let per = (n / rows.len()).max(1);
    let mut ours = [0.0f32; 64];
    for (i, &v) in pixels.iter().enumerate() {
        if let Some(band) = ours.get_mut(i / n / per) {
            *band += f32::from(v);
        }
    }
    let top = ours.iter().copied().fold(0.0, f32::max).max(1.0);
    // each band's weight: what it should hold over what it holds (none where it holds next to
    // nothing, which no weight can fill)
    let weights: Vec<f32> = ours
        .iter()
        .zip(rows)
        .map(|(&have, &want)| {
            let have = have / top;
            if have > 0.02 { (f32::from(want) / 255.0 / have).min(8.0) } else { 0.0 }
        })
        .collect();
    let at = |y: usize| {
        let place = ((y as f32 + 0.5) / per as f32 - 0.5).clamp(0.0, (rows.len() - 1) as f32);
        let (low, f) = (place.floor() as usize, place.fract());
        let w = |b: usize| weights.get(b.min(rows.len() - 1)).copied().unwrap_or(0.0);
        w(low) * (1.0 - f) + w(low + 1) * f
    };
    let most = (0..n).map(at).fold(0.0, f32::max).max(1e-6);
    pixels.iter().enumerate().map(|(i, &v)| (f32::from(v) * at(i / n) / most).round().clamp(0.0, 255.0) as u8).collect()
}

fn stock_tones<T: Copy, const N: usize>(table: &[(u64, [T; N])], key: u64) -> Option<[T; N]> {
    table.binary_search_by_key(&key, |&(k, _)| k).ok().and_then(|i| table.get(i)).map(|&(_, t)| t)
}

/// How many octaves of noise [`octave_grain`] lays together.
pub const OCTAVES: usize = 8;

/// How much of a stock grain's side our stand-in drawn by its octaves spans (see [`octave_grain`]):
/// a quarter, its texels the image's pixels with the image at 1024 a side.
pub const OCTAVE_SHARE: f32 = 0.25;

/// How wide the stock grain a brush file names `name` is (pixels), and how strong each octave of
/// [`octave_grain`]'s noise is (finest first) for it to vary as much at each size as that image
/// does, as measured, if it is one we know.
pub fn stock_grain_octaves(name: &str) -> Option<(f32, [f32; OCTAVES])> {
    stock_grain_octaves_of(stock_key(name))
}

/// [`stock_grain_octaves`] of the stock grain whose measurements are kept under `key`.
pub fn stock_grain_octaves_of(key: u64) -> Option<(f32, [f32; OCTAVES])> {
    let [width, octaves @ ..] = stock_tones(crate::stock_tones::STOCK_GRAIN_OCTAVES, key)?;
    Some((f32::from(width), octaves.map(|a| f32::from(a) / 10000.0)))
}

/// Whether our grain `index` stands in for a stock grain by the stock image's own octaves of noise
/// (see [`octave_grain`]) where they were measured: papers and washes, whose fibres, veins and
/// blotches, each at its own size, our patterns of them lack.
pub fn drawn_by_octaves(index: u32) -> bool {
    matches!(index, grain::SHEET | grain::WASH | grain::WORN)
}

/// Noise in [`OCTAVES`] octaves as `amplitudes` says (finest first: cells of 1, 2, 4 ... texels),
/// each a random value at its cells' corners eased between them, wrapping round so it tiles; its
/// values then moved onto `tones` (a stock grain's, at [`TONE_SHARES`]) keeping their order. A
/// stand-in that varies as much at each size as the stock image it was measured from.
pub fn octave_grain(amplitudes: &[f32; OCTAVES], tones: &[u8; 9]) -> Vec<u8> {
    let n = TEXTURE_SIZE;
    let raw: Vec<f32> = (0..n * n)
        .map(|k| {
            let (i, j) = ((k % n) as f32 + 0.5, (k / n) as f32 + 0.5);
            amplitudes
                .iter()
                .enumerate()
                .filter(|&(_, &amplitude)| amplitude > 0.0)
                .map(|(octave, &amplitude)| {
                    // (each cell's corner on a texel's middle: the finest octave is the texels'
                    // own values)
                    let cell = (1u32 << octave) as f32;
                    let period = (n >> octave).max(1) as i32;
                    let seed = 9001 + octave as u32 * 7919;
                    amplitude * (value_noise(i / cell - 0.5, j / cell - 0.5, period, seed) - 0.5)
                })
                .sum()
        })
        .collect();
    let spread: Vec<(f32, f32)> = tones.iter().zip(TONE_SHARES).map(|(&t, s)| (f32::from(t) / 255.0, s)).collect();
    ranked(&raw, &spread)
}

/// The key the stock tables keep an image's measurements under: FNV-1a of its file's name in lower
/// case. A stand-in drawn for a brush that names no image (the stock library's own) goes by it.
pub fn stock_key(name: &str) -> u64 {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).to_lowercase();
    base.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// `pixels`, a shape, with its values moved onto `tones` (its values at `shares`) keeping their
/// order, equal values ordered by how deep in the shape they lie (blurs of it, near to far, so
/// even a wide solid middle grades towards its centre): moved onto a stock shape's tones it grows
/// or shrinks at its rim, rather than taking holes in a solid middle or specks round it.
pub fn retoned_shape(pixels: &[u8], tones: &[u8], shares: &[f32]) -> Vec<u8> {
    let n = pixels.len().isqrt();
    let img: Vec<f32> = pixels.iter().map(|&v| f32::from(v) / 255.0).collect();
    // as deep at every side (bigger than [`TEXTURE_SIZE`], blurred as far by a window carried along
    // each row, whose cost doesn't grow with it), summed as each is made (a big picture's added in
    // place, so that drawing one holds no more pictures' worth than it must)
    let mut deep = vec![0.0f32; pixels.len()];
    for r in [6, 24, 64] {
        if n == TEXTURE_SIZE {
            deep.iter_mut().zip(&blurred(&img, r)).for_each(|(sum, d)| *sum += d);
        } else {
            wrapped_blur_adding(&img, n, (r * n / TEXTURE_SIZE).min(n.saturating_sub(1) / 2), &mut deep);
        }
    }
    drop(img);
    // what each is ranked by, in place of how deep it lies
    let mut raw = deep;
    raw.iter_mut().zip(pixels).for_each(|(sum, &v)| *sum = f32::from(v) + (*sum / 3.0).clamp(0.0, 1.0) * 0.999);
    let spread: Vec<(f32, f32)> = tones.iter().zip(shares).map(|(&t, &s)| (f32::from(t) / 255.0, s)).collect();
    ranked(&raw, &spread)
}

/// `pixels`, a grain or shape, with its values moved onto `tones` (its values at `shares`)
/// keeping their order: a stand-in pattern with a stock image's lights and darks. Equal values
/// are ordered at random, so a flat part takes a spread of values, not a ramp across the image.
pub fn retoned(pixels: &[u8], tones: &[u8], shares: &[f32]) -> Vec<u8> {
    let raw: Vec<f32> = pixels
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let h = (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40;
            f32::from(v) + (h as f32 / (1u64 << 24) as f32) * 0.999
        })
        .collect();
    let spread: Vec<(f32, f32)> = tones.iter().zip(shares).map(|(&t, &s)| (f32::from(t) / 255.0, s)).collect();
    ranked(&raw, &spread)
}

/// Makes the shapes and grains now, so the first canvas doesn't wait for them (slow: off the main
/// thread).
pub fn prepare() {
    SHAPES.get_or_init(make_shapes);
    GRAINS.get_or_init(make_grains);
}

/// A daisy: eight long, narrow petals round a small dot, a ring of black between.
fn daisy(side: usize) -> Vec<u8> {
    let zoom = zoom_of(side);
    field(side, |x, y| {
        let (u, v) = centred(x, y);
        let (r, a) = ((u * u + v * v).sqrt(), v.atan2(u));
        // the nearest petal's way, and how far along and across it the point lies
        let gap = TAU / 8.0;
        let off = a - (a / gap).round() * gap;
        let (along, across) = (r * off.cos(), r * off.sin());
        let petal = 1.0 - ((along - 0.56) / 0.4).powi(2) - (across / 0.085).powi(2);
        let dot = 0.05 - r;
        (petal * 12.0).max(dot * 120.0 * zoom).clamp(0.0, 1.0)
    })
}

/// Our shapes' names, in their order.
const SHAPE_NAMES: [&str; SHAPE_COUNT as usize] = [
    "Soft round",
    "Grainy disc",
    "Chunk",
    "Bristles",
    "Splatter",
    "Flat",
    "Pastel",
    "Blot",
    "Leaf",
    "Grass",
    "Sparkle",
    "Cloud",
    "Square",
    "Dry",
    "Ring",
    "Spray",
    "Hatch",
    "Wisp",
    "Star",
    "Fur",
    "Droplet",
    "Flame",
    "Bubble",
    "Stone",
    "Streak",
    "Flower",
    "Twig",
    "Crack",
    "Sponge",
    "Triangle",
    "Diamond",
    "Needles",
    "Feather",
    "Confetti",
    "Hair",
    "Smudge",
    "Moss",
    "Heart",
    "Snowflake",
    "Scribble",
    "Drip",
    "Bolt",
    "Block",
    "Grainy dot",
    "Small chunk",
    "Small pastel",
    "Solid dab",
    "Flat stroke",
    "Dots",
    "Square dots",
    "Bleed",
    "Mess",
    "Slim oval",
    "Ultra soft round",
    "Stock blot",
    "Hook",
    "Grainy square",
    "Line",
    "Daisy",
    "Flicks",
    "Spindle",
    "Stubble",
];

fn make_shapes() -> Vec<GrayImage> {
    let mut shapes: Vec<GrayImage> = Vec::with_capacity(SHAPE_NAMES.len());
    for (index, name) in (0..).zip(SHAPE_NAMES) {
        // one drawn from another takes that one as it was drawn already
        let pixels = match shape_from(index) {
            Some((from, make)) => make(shapes.get(from as usize).map_or(&[][..], |s| &s.pixels[..])),
            None => draw_shape(index, TEXTURE_SIZE).unwrap_or_default(),
        };
        shapes.push(GrayImage { name, pixels });
    }
    shapes
}

/// How one of our pictures is drawn from another's pixels (at the side they are).
type DrawnFrom = fn(&[u8]) -> Vec<u8>;

/// The shape a shape is drawn from, and how, for those drawn from another of ours: stand-ins for
/// stock shapes drawn smaller in their square, as the stock shapes that come nearest them spread
/// their mass over about half of the square's width, not three quarters; and a solid dab, the small
/// chunk filled in (stock stamped dabs are solid through most of their area: inked mean about 0.85,
/// mostly over 0.9, against the chunk's 0.68).
fn shape_from(index: u32) -> Option<(u32, DrawnFrom)> {
    Some(match index {
        shape::STOCK_GRAINY_DISC => (shape::GRAINY_DISC, |p| shrunk(p, SHRINK_DISC)),
        shape::STOCK_CHUNK => (shape::CHUNK, |p| shrunk(p, SHRINK_CHUNK)),
        shape::STOCK_PASTEL => (shape::PASTEL, |p| shrunk(p, SHRINK_PASTEL)),
        shape::STOCK_DAB => {
            (shape::STOCK_CHUNK, |p| p.iter().map(|&v| (f32::from(v) * DAB_GAIN).min(255.0) as u8).collect())
        }
        _ => return None,
    })
}

/// Our shape `index` drawn `side` pixels across; none past the last. Bigger than
/// [`TEXTURE_SIZE`], it is the same picture, sharper (see [`zoom_of`] and [`octaves_more`]).
fn draw_shape(index: u32, side: usize) -> Option<Vec<u8>> {
    if let Some((from, make)) = shape_from(index) {
        return Some(make(&draw_shape(from, side)?));
    }
    let zoom = zoom_of(side);
    let more = octaves_more(side);
    Some(match index {
        // full well out towards its edge: stamped along a stroke it builds up to a soft line most of
        // its width across, with a short fade at each side
        shape::SOFT_ROUND => field(side, |x, y| smooth(1.0 - radius(x, y)).powf(0.62)),
        shape::GRAINY_DISC => field(side, |x, y| {
            let edge = 1.0 - radius(x, y);
            let n = fbm_finer(x * 12.0, y * 12.0, 12, 3, more, 11);
            (edge * 6.0).clamp(0.0, 1.0) * (0.55 + 0.45 * n).clamp(0.0, 1.0) * if n > 0.25 { 1.0 } else { n * 4.0 }
        }),
        shape::CHUNK => field(side, |x, y| {
            let n = fbm(x * 4.0, y * 4.0, 4, 4, 23);
            let r = radius(x, y) + (n - 0.5) * 0.55;
            let edge = smooth(((1.0 - r) * 5.0).clamp(0.0, 1.0));
            edge * (0.6 + 0.4 * fbm(x * 16.0, y * 16.0, 16, 2, 7))
        }),
        shape::BRISTLES => field(side, |x, y| {
            let across = (x - 0.5).abs() * 2.0;
            let along = (y - 0.5).abs() * 2.0;
            let hairs = 0.5 + 0.5 * (x * 60.0 + fbm(x * 8.0, y, 8, 2, 5) * 6.0).sin();
            let body = smooth(((1.0 - across) * 4.0).clamp(0.0, 1.0)) * smooth(((1.0 - along) * 2.5).clamp(0.0, 1.0));
            body * (0.35 + 0.65 * hairs)
        }),
        shape::SPLATTER => {
            let dots = scatter_points(41, 26);
            field(side, |x, y| {
                dots.iter().fold(0.0f32, |acc, &(cx, cy, r)| {
                    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / r;
                    acc.max(smooth(((1.0 - d) * 3.0).clamp(0.0, 1.0)))
                })
            })
        }
        shape::FLAT => field(side, |x, y| {
            let across = (x - 0.5).abs() * 2.0;
            let along = (y - 0.5).abs() * 2.0;
            smooth(((1.0 - across) * 3.0).clamp(0.0, 1.0)) * smooth(((1.0 - along) * 6.0).clamp(0.0, 1.0))
        }),
        shape::PASTEL => field(side, |x, y| {
            // a worn block: squarish, ragged at its edge, patchy inside
            let n = fbm_finer(x * 6.0, y * 6.0, 6, 4, more, 31);
            let block = 1.0 - ((x - 0.5).abs() * 2.0).max((y - 0.5).abs() * 2.4);
            let edge = smooth(((block - 0.12 + (n - 0.5) * 0.45) * 5.0).clamp(0.0, 1.0));
            edge * contrast(fbm_finer(x * 24.0, y * 24.0, 24, 3, more, 37), 1.8).max(0.25)
        }),
        // its drops pushed out towards the square's edge
        shape::BLOT => blot(side, |cx, cy| ((cx - 0.5) * 2.2 + 0.5, (cy - 0.5) * 2.2 + 0.5)),
        shape::LEAF => field(side, |x, y| {
            // pointing up, widest below the middle, a pale midrib and a short stem
            let (u, v) = ((x - 0.5) * 2.0, (y - 0.5) * 2.0);
            let t = ((v + 0.85) / 1.5).clamp(0.0, 1.0);
            let width = 0.52 * (t * std::f32::consts::PI).sin().powf(0.8) * (1.0 - 0.25 * t);
            let blade = edge((width - u.abs()) * 30.0, zoom) * f32::from(u8::from(v < 0.65));
            let rib = 1.0 - 0.5 * edge((0.025 - u.abs()) * 60.0, zoom) * f32::from(u8::from(v > -0.6));
            let stem = edge((0.03 - u.abs()) * 60.0, zoom) * f32::from(u8::from(v > 0.6 && v < 0.9));
            (blade * rib).max(stem)
        }),
        shape::GRASS => field(side, |x, y| {
            // blades from the bottom, each leaning and narrowing to its tip
            (0..11).fold(0.0f32, |acc, k| {
                let base = 0.12 + 0.76 * hash(k, 1, 61);
                let lean = (hash(k, 2, 61) - 0.5) * 0.5;
                let top = 0.08 + 0.35 * hash(k, 3, 61);
                let along = ((0.92 - y) / (0.92 - top)).clamp(0.0, 1.0);
                if y < top || y > 0.92 {
                    return acc;
                }
                let cx = base + lean * along * along;
                let half = 0.03 * (1.0 - along).powf(0.7);
                acc.max(edge((half - (x - cx).abs()) * 160.0, zoom))
            })
        }),
        shape::SPARKLE => field(side, |x, y| {
            // four long points and four short ones, each narrowing to its tip, over a small glow
            let (u, v) = ((x - 0.5).abs() * 2.0, (y - 0.5).abs() * 2.0);
            let point = |along: f32, off: f32| {
                let width = 0.09 * (1.0 - along).max(0.0).powf(1.4) + 0.004;
                smooth(1.0 - off / width) * f32::from(u8::from(along < 1.0))
            };
            let long = point(u, v).max(point(v, u));
            let (a, b) = ((u + v) * std::f32::consts::FRAC_1_SQRT_2, (u - v).abs() * std::f32::consts::FRAC_1_SQRT_2);
            let short = point(a * 1.8, b * 1.4) * 0.8;
            let glow = (-(u * u + v * v) * 40.0).exp();
            long.max(short).max(glow).min(1.0) * smooth(((1.0 - radius(x, y)) * 8.0).clamp(0.0, 1.0))
        }),
        shape::CLOUD => {
            let puffs = scatter_points(71, 9);
            field(side, move |x, y| {
                // puffs drawn in towards the middle, overlapping into one cloud
                let lumps = puffs.iter().fold(0.0f32, |acc, &(cx, cy, r)| {
                    let (px, py) = (0.5 + (cx - 0.5) * 0.75, 0.5 + (cy - 0.5) * 0.6);
                    let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt() / (r * 1.5 + 0.2);
                    acc.max(smooth((1.0 - d).clamp(0.0, 1.0) * 1.6))
                });
                let n = fbm_finer(x * 8.0, y * 8.0, 8, 4, more, 73);
                (lumps * (0.6 + 0.55 * n)).min(1.0) * smooth(((1.0 - radius(x, y)) * 4.0).clamp(0.0, 1.0))
            })
        }
        shape::SQUARE => field(side, |x, y| f32::from(u8::from((x - 0.5).abs() < 0.4 && (y - 0.5).abs() < 0.4))),
        shape::DRY => field(side, |x, y| {
            // a flat stroke's worth of hairs, broken where the paint ran out
            let across = (x - 0.5).abs() * 2.0;
            let along = (y - 0.5).abs() * 2.0;
            let hair = value_noise(x * 90.0, y * 2.0, 90, 83);
            let broken = edge((hair - 0.35) * 5.0, zoom);
            let body = smooth(((1.0 - across) * 6.0).clamp(0.0, 1.0)) * smooth(((1.0 - along) * 3.0).clamp(0.0, 1.0));
            body * broken
        }),
        shape::RING => field(side, |x, y| {
            let r = radius(x, y);
            let band = edge((0.06 - (r - 0.82).abs()) * 40.0, zoom);
            band.max(0.25 * smooth(((0.8 - r) * 10.0).clamp(0.0, 1.0)))
        }),
        shape::SPRAY => {
            // fine droplets, thick in the middle and thinning out
            let drops: Vec<(f32, f32, f32)> = (0..420)
                .map(|i| {
                    let a = hash(i, 1, 97) * std::f32::consts::TAU;
                    let d = (hash(i, 2, 97) * hash(i, 4, 97)).sqrt() * 0.46;
                    (0.5 + a.cos() * d, 0.5 + a.sin() * d, 0.004 + 0.006 * hash(i, 3, 97))
                })
                .collect();
            field(side, move |x, y| {
                drops.iter().fold(0.0f32, |acc, &(cx, cy, r)| {
                    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / r;
                    acc.max(((1.0 - d) * zoom + 0.5).clamp(0.0, 1.0))
                })
            })
        }
        shape::HATCH => field(side, |x, y| {
            let lines = 0.5 + 0.5 * ((x + y) * std::f32::consts::TAU * 9.0).cos();
            let ink = edge((lines - 0.55) * 6.0, zoom);
            ink * smooth(((1.0 - radius(x, y)) * 5.0).clamp(0.0, 1.0))
        }),
        shape::WISP => field(side, |x, y| {
            // bands bent by noise: smoke curling
            let warp = fbm(x * 3.0, y * 3.0, 3, 3, 109);
            let bands = 0.5 + 0.5 * ((y + warp * 0.6) * std::f32::consts::TAU * 3.0).sin();
            let soft = smooth(((1.0 - radius(x, y)) * 2.0).clamp(0.0, 1.0));
            (bands.powi(3) * soft * (0.6 + 0.6 * fbm(x * 6.0, y * 6.0, 6, 3, 113))).min(1.0)
        }),
        shape::STAR => field(side, |x, y| {
            // five points, one straight up, the edges between them curving in
            let (u, v) = centred(x, y);
            let s = ((v.atan2(u) + FRAC_PI_2) / TAU * 5.0).rem_euclid(1.0);
            let edge = 0.4 + 0.52 * ((s - 0.5).abs() * 2.0).powf(1.6);
            self::edge((edge - (u * u + v * v).sqrt()) * 40.0, zoom)
        }),
        shape::FUR => field(side, |x, y| {
            // fine hairs from a band low down, bending as they rise and narrowing to their tips
            (0..48).fold(0.0f32, |acc, k| {
                let (foot, top) = (0.75 + 0.12 * hash(k, 2, 131), 0.1 + 0.3 * hash(k, 3, 131));
                if y < top || y > foot {
                    return acc;
                }
                let along = (foot - y) / (foot - top);
                let cx = 0.15 + 0.7 * hash(k, 1, 131) + (hash(k, 4, 131) - 0.5) * 0.3 * along * along;
                let half = 0.008 * (1.0 - along).powf(0.6) + 0.001;
                acc.max(smooth((half - (x - cx).abs()) * 400.0) * (0.5 + 0.5 * hash(k, 5, 131)))
            })
        }),
        shape::DROPLET => field(side, |x, y| {
            // round below and drawn up to a point; a strong rim, a paler middle, a bright spot
            let (u, v) = centred(x, y);
            let (cy, r) = (0.3, 0.58);
            let width = if v >= cy {
                (r * r - (v - cy).powi(2)).max(0.0).sqrt()
            } else {
                r * ((v + 0.92) / (cy + 0.92)).clamp(0.0, 1.0).powf(1.5)
            };
            let inside = width - u.abs();
            let rim = 0.55 + 0.45 * smooth((0.12 - inside) * 12.0);
            let spot = edge((0.1 - ((u + 0.2).powi(2) + (v - 0.18).powi(2)).sqrt()) * 30.0, zoom);
            edge(inside * 30.0, zoom) * rim * (1.0 - 0.8 * spot)
        }),
        shape::FLAME => field(side, |x, y| {
            // a tongue of fire: round and full at its foot, flickering up to a point
            let (u, v) = centred(x, y);
            let t = ((0.85 - v) / 1.8).clamp(0.0, 1.0);
            let width = if t < 0.2 {
                0.46 * (1.0 - ((0.2 - t) / 0.2).powi(2)).max(0.0).sqrt()
            } else {
                0.46 * ((1.0 - t) / 0.8).powf(1.5)
            };
            // the upper part sways and frays into tongues
            let sway = (fbm(x * 2.0, y * 4.0, 2, 3, 137) - 0.5) * 0.9 * t.powf(1.2);
            let fray = (fbm(x * 6.0, y * 3.0, 3, 3, 139) - 0.5) * 0.35 * t;
            let body = edge((width + fray - (u - sway).abs()) * 18.0, zoom);
            let flicker = 0.75 + 0.35 * fbm(x * 8.0, y * 8.0, 8, 3, 141);
            (body * (0.6 + 0.4 * (1.0 - t)) * flicker).min(1.0)
        }),
        shape::BUBBLE => field(side, |x, y| {
            // a thin skin, a faint inside and a curved highlight to the upper left
            let r = radius(x, y);
            let skin = edge((0.05 - (r - 0.86).abs()) * 50.0, zoom);
            let inside = 0.12 * smooth((0.86 - r) * 10.0);
            let a = (y - 0.5).atan2(x - 0.5);
            let arc = smooth(1.0 - ((a + 2.36) / 0.6).abs()) * edge((0.05 - (r - 0.66).abs()) * 40.0, zoom);
            skin.max(inside).max(arc * 0.9)
        }),
        shape::STONE => {
            // an angular pebble: straight edges between seven corners, lit from the upper left
            let corners: Vec<(f32, f32)> = (0..7)
                .map(|k| {
                    let a = (k as f32 + 0.4 * hash(k, 1, 139)) / 7.0 * TAU;
                    let r = 0.62 + 0.3 * hash(k, 2, 139);
                    (a.cos() * r, a.sin() * r)
                })
                .collect();
            field(side, move |x, y| {
                let (u, v) = centred(x, y);
                let (edge, inside) = polygon(&corners, u, v);
                if !inside {
                    return 0.0;
                }
                let lit = 0.85 - 0.15 * (u + v);
                self::edge(edge * 30.0, zoom) * (lit * (0.75 + 0.3 * fbm(x * 10.0, y * 10.0, 10, 3, 141))).min(1.0)
            })
        }
        shape::STREAK => field(side, |x, y| {
            // a long thin line, thickest a little below the middle, fading out at both ends
            let (u, v) = centred(x, y);
            let half = 0.035 * (1.0 - ((v - 0.05) / 0.85).powi(2)).max(0.0);
            edge((half - u.abs()) * 100.0, zoom)
        }),
        shape::FLOWER => field(side, |x, y| {
            // five rounded petals round a paler middle
            let (u, v) = centred(x, y);
            let r = (u * u + v * v).sqrt();
            // Round the angle to f32 after atan2 so the exported tip matches across platforms.
            let a = f64::from(v).atan2(f64::from(u)) as f32 + FRAC_PI_2;
            let petal = (a * 2.5).cos().abs();
            let body = edge((0.3 + 0.6 * petal.sqrt() - r) * 25.0, zoom);
            let middle = 1.0 - 0.45 * smooth((0.2 - r) * 20.0);
            body * (0.8 + 0.2 * petal) * middle
        }),
        shape::TWIG => {
            // a stick with three side shoots, each thinning to its end
            let parts = [
                ((0.02, 0.92), (-0.04, -0.88), 0.07, 0.025),
                ((0.0, 0.35), (-0.55, -0.25), 0.04, 0.012),
                ((-0.02, -0.15), (0.5, -0.6), 0.035, 0.01),
                ((-0.03, -0.5), (-0.3, -0.85), 0.025, 0.008),
            ];
            field(side, move |x, y| {
                let (u, v) = centred(x, y);
                parts.iter().fold(0.0f32, |acc, &(a, b, w0, w1)| {
                    let (d, t) = segment(u, v, a, b);
                    acc.max(edge((w0 + (w1 - w0) * t - d) * 60.0, zoom))
                }) * (0.8 + 0.25 * fbm_finer(x * 4.0, y * 32.0, 4, 2, more, 143)).min(1.0)
            })
        }
        shape::CRACK => {
            let lines = cracks(149);
            field(side, move |x, y| {
                let (u, v) = centred(x, y);
                lines.iter().fold(0.0f32, |acc, &(a, b, w)| acc.max(edge((w - segment(u, v, a, b).0) * 120.0, zoom)))
            })
        }
        shape::SPONGE => field(side, |x, y| {
            // a round dab full of holes of every size
            let n = fbm(x * 4.0, y * 4.0, 4, 2, 151);
            let body = smooth((0.9 - radius(x, y) - (n - 0.5) * 0.3) * 8.0);
            let (near, _) = cells(x, y, 10, 157);
            let (fine, _) = cells(x, y, 22, 163);
            body * edge((near - 0.28) * 6.0, zoom) * edge((fine - 0.22) * 6.0, zoom)
        }),
        shape::TRIANGLE => field(side, |x, y| {
            // pointing up, its middle at the centre
            let (u, v) = centred(x, y);
            let d = (v - 0.45).max(0.866 * u.abs() - 0.5 * v - 0.45);
            edge(-d * 60.0, zoom)
        }),
        shape::DIAMOND => field(side, |x, y| {
            let (u, v) = centred(x, y);
            edge((1.0 - u.abs() / 0.6 - v.abs() / 0.95) * 20.0, zoom)
        }),
        shape::NEEDLES => {
            // a sprig: a stem with pairs of needles slanting up, shorter towards its tip
            let mut parts = vec![((0.0, 0.92), (0.0, -0.9), 0.025)];
            for k in 0..13 {
                let at = 0.8 - k as f32 * 0.13;
                let len = 0.25 + 0.25 * (at + 0.9) / 1.7;
                for way in [-1.0f32, 1.0] {
                    parts.push(((0.0, at), (way * len * 0.77, at - len * 0.64), 0.012));
                }
            }
            field(side, move |x, y| {
                let (u, v) = centred(x, y);
                parts.iter().fold(0.0f32, |acc, &(a, b, w)| {
                    let (d, t) = segment(u, v, a, b);
                    acc.max(smooth((w * (1.0 - 0.6 * t) - d) * 150.0))
                })
            })
        }
        shape::FEATHER => field(side, |x, y| {
            // a vane round a quill, its barbs slanting out and up, split here and there
            let (u, v) = centred(x, y);
            let t = (v + 0.95) / 1.85;
            let vane = 0.42 * (PI * (t / 0.82).clamp(0.0, 1.0)).sin().powf(0.7);
            let lean = u + 0.08 * (t * PI).sin();
            let barbs = 0.5 + 0.5 * ((v + lean.abs() * 0.8) * TAU * 14.0).cos();
            let split = smooth((value_noise((v + lean.abs() * 0.8) * 9.0, u.signum(), 9, 167) - 0.2) * 8.0);
            let body = edge((vane - lean.abs()) * 30.0, zoom) * (0.55 + 0.45 * barbs) * split;
            let quill = edge((0.018 * (1.0 - 0.3 * t) - lean.abs()) * 120.0, zoom) * f32::from(u8::from(t < 1.0));
            body.max(quill)
        }),
        shape::CONFETTI => {
            // little paper strips turned every way
            let strips: Vec<(f32, f32, f32, f32, f32, f32)> = (0..22)
                .map(|i| {
                    let a = hash(i, 1, 173) * TAU;
                    let d = hash(i, 2, 173).sqrt() * 0.36;
                    let turn = hash(i, 3, 173) * PI;
                    let (w, h) = (0.012 + 0.012 * hash(i, 4, 173), 0.03 + 0.025 * hash(i, 5, 173));
                    (0.5 + a.cos() * d, 0.5 + a.sin() * d, turn, w, h, 0.6 + 0.4 * hash(i, 6, 173))
                })
                .collect();
            field(side, move |x, y| {
                strips.iter().fold(0.0f32, |acc, &(cx, cy, turn, w, h, ink)| {
                    let (dx, dy) = (x - cx, y - cy);
                    let (s, c) = turn.sin_cos();
                    let (along, across) = (dx * c + dy * s, dy * c - dx * s);
                    acc.max(smooth((w - across.abs()).min(h - along.abs()) * 600.0) * ink)
                })
            })
        }
        shape::HAIR => field(side, |x, y| {
            // a lock of long strands, each waving a little
            let ends = smooth((y - 0.06) * 12.0) * smooth((0.94 - y) * 12.0);
            (0..9).fold(0.0f32, |acc, k| {
                let cx = 0.22 + 0.56 * (k as f32 + hash(k, 1, 179) * 0.6) / 9.0;
                let wave = 0.05 * (y * TAU * 1.2 + hash(k, 2, 179) * TAU).sin();
                acc.max(smooth((0.006 - (x - cx - wave).abs()) * 500.0) * (0.6 + 0.4 * hash(k, 3, 179)))
            }) * ends
        }),
        shape::SMUDGE => field(side, |x, y| {
            // a soft oval dragged along: streaks run its length
            let (u, v) = centred(x, y);
            let soft = smooth(1.0 - (u * u / 0.5 + v * v / 0.95).sqrt()).powf(0.8);
            soft * (0.6 + 0.4 * value_noise(x * 40.0, y * 3.0, 40, 181))
        }),
        shape::MOSS => field(side, |x, y| {
            // clumped fuzz, thick in the middle and sparse at the edge
            let falloff = 1.0 - radius(x, y);
            let n = fbm_finer(x * 16.0, y * 16.0, 16, 3, more, 191);
            smooth((n - 0.62 + falloff * 0.35) * 8.0) * smooth(falloff * 4.0)
        }),
        shape::HEART => {
            let outline = heart();
            field(side, move |x, y| {
                let (u, v) = centred(x, y);
                let (edge, inside) = polygon(&outline, u, v);
                if inside { self::edge(edge * 40.0, zoom) } else { 0.0 }
            })
        }
        shape::SNOWFLAKE => field(side, |x, y| {
            // six arms, each with two pairs of side branches, round a small hexagon
            let (u, v) = centred(x, y);
            let r = (u * u + v * v).sqrt();
            let sector = TAU / 6.0;
            let a = v.atan2(u).rem_euclid(sector);
            let a = a.min(sector - a);
            let (px, py) = (r * a.cos(), r * a.sin());
            let arm = edge((0.03 - segment(px, py, (0.0, 0.0), (0.88, 0.0)).0) * 80.0, zoom);
            let branch = |at: f32, len: f32| {
                let tip = (at + len * 0.5, len * 0.866);
                edge((0.022 - segment(px, py, (at, 0.0), tip).0) * 80.0, zoom)
            };
            let middle = edge((0.16 - r) * 40.0, zoom);
            arm.max(branch(0.38, 0.28)).max(branch(0.6, 0.2)).max(middle)
        }),
        shape::SCRIBBLE => {
            // a looping line wandering round the middle
            let points: Vec<(f32, f32)> = (0..=96)
                .map(|i| {
                    let t = i as f32 / 96.0 * TAU * 1.5;
                    (
                        0.5 * (2.1 * t).sin() + 0.25 * (5.3 * t + 1.0).sin(),
                        0.5 * (1.7 * t).cos() + 0.25 * (4.1 * t + 2.0).sin(),
                    )
                })
                .collect();
            field(side, move |x, y| {
                let (u, v) = centred(x, y);
                points
                    .windows(2)
                    .filter_map(|w| Some(segment(u, v, *w.first()?, *w.get(1)?).0))
                    .fold(0.0f32, |acc, d| acc.max(edge((0.02 - d) * 80.0, zoom)))
            })
        }
        shape::DRIP => field(side, |x, y| {
            // a blob with paint running down from it to a bead
            let (u, v) = centred(x, y);
            let blob = edge((0.35 - (u * u + (v + 0.55).powi(2)).sqrt()) * 30.0, zoom);
            let run = edge((0.06 - (u - 0.03 * (v * 3.0).sin()).abs()) * 60.0, zoom)
                * f32::from(u8::from(v > -0.6 && v < 0.66));
            let bead = edge((0.1 - (u * u + (v - 0.7).powi(2)).sqrt()) * 40.0, zoom);
            blob.max(run).max(bead)
        }),
        shape::BOLT => {
            // a zigzag from the top edge's middle to the bottom's, with two short forks, in a
            // faint glow: laid end to end along a stroke, one bolt runs into the next
            let mut points = vec![(0.0f32, -1.0f32)];
            for k in 1..8 {
                points.push(((hash(k, 1, 197) - 0.5) * 0.5, -1.0 + k as f32 * 0.25));
            }
            points.push((0.0, 1.0));
            let mut parts: Vec<Line> =
                points.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?, 0.035))).collect();
            for (k, &from) in points.iter().enumerate().skip(2).step_by(3).take(2) {
                let way = if from.0 > 0.0 { 1.0 } else { -1.0 };
                let to = (from.0 + way * (0.25 + 0.15 * hash(k as i32, 2, 197)), from.1 + 0.2);
                parts.push((from, to, 0.018));
            }
            field(side, move |x, y| {
                let (u, v) = centred(x, y);
                let d = parts.iter().fold(f32::MAX, |acc, &(a, b, w)| acc.min(segment(u, v, a, b).0 - w));
                edge(-d * 120.0, zoom).max(0.3 * smooth(1.0 - d.max(0.0) * 8.0)) * smooth((1.0 - u.abs()) * 6.0)
            })
        }
        shape::BLOCK => field(side, |x, y| {
            // a solid upright block, half as wide as it is tall, with a few specks out of it
            let (dx, dy) = ((x - 0.5).abs(), (y - 0.47).abs());
            let edge = self::edge((0.245 - dx) * 60.0, zoom) * self::edge((0.4 - dy) * 60.0, zoom);
            let n = TEXTURE_SIZE as f32;
            let speck = hash((x * n / 3.0) as i32, (y * n / 3.0) as i32, 1601);
            edge * if speck > 0.86 { 0.25 } else { 1.0 }
        }),
        shape::STOCK_FLAT_STROKE => flat_stroke(side),
        shape::STOCK_DOTS => {
            let round = stock_dots(false);
            field(side, move |x, y| dots(&round, x, y))
        }
        shape::STOCK_SQUARE_DOTS => {
            let square = stock_dots(true);
            field(side, move |x, y| dots(&square, x, y))
        }
        shape::STOCK_BLEED => field(side, |x, y| {
            // a drop of watery paint dried: a ragged edge where the pigment gathered, paler and
            // grainy inside, palest in the middle
            let n = fbm(x * 5.0, y * 5.0, 5, 4, 2153);
            let fringe = fbm_finer(x * 32.0, y * 32.0, 32, 2, more, 2161);
            let r = radius(x, y) + (n - 0.5) * 0.3 + (fringe - 0.5) * 0.06;
            let inside = 0.86 - r;
            let rim = smooth(1.0 - inside / 0.13);
            let middle = 0.55 + 0.3 * smooth(1.0 - r / 0.55);
            let grain = 0.7 + 0.3 * value_noise(x * 96.0, y * 96.0, 96, 2167);
            edge(inside * 60.0, zoom) * rim.max(middle * grain)
        }),
        shape::STOCK_MESS => field(side, |x, y| {
            // a thick dab smeared out: solid nearly to its rough edge, which frays into bits
            let n = fbm(x * 4.0, y * 4.0, 4, 4, 2171);
            let fray = fbm_finer(x * 24.0, y * 24.0, 24, 3, more, 2179);
            let r = radius(x, y) + (n - 0.5) * 0.35 + (fray - 0.5) * 0.25;
            edge((0.84 - r) * 14.0, zoom) * (0.88 + 0.12 * fbm_finer(x * 48.0, y * 48.0, 48, 2, more, 2183))
        }),
        // a hard, flat oval lying across its square (the stock slim oval's proportions: as wide as
        // most of the square, a quarter as tall), for flat brushes that turn with the pen
        shape::SLIM_OVAL => field(side, |x, y| {
            let r = ((x - 0.5) / SLIM_OVAL_HALF.0).hypot((y - 0.5) / SLIM_OVAL_HALF.1);
            // about a texel of anti-aliasing at the edge
            smooth((1.0 - r) * SLIM_OVAL_HALF.1 * side as f32 + 0.5)
        }),
        // a faint round falling away from its middle (the stock ultra soft tip's falloff: dim, and
        // gone just inside its square's edge), so stamped along a stroke it builds up rounded
        shape::ULTRA_SOFT_ROUND => field(side, |x, y| ULTRA_SOFT_PEAK * smooth(1.0 - radius(x, y))),
        // a stock ink blot: the blot with its drops just off its body's edge, as stock blots' specks
        // lie (within about 0.9 to 1.3 of the body's radius from the middle); pushed far out, they
        // would each draw a line of their own along a stroke whose stamps don't turn
        shape::STOCK_BLOT => blot(side, |cx, cy| {
            let (dx, dy) = (cx - 0.5, cy - 0.5);
            let d = dx.hypot(dy).max(1e-6);
            let out = BLOT_BODY * (0.9 + 0.4 * d / SCATTER_REACH) / d;
            (0.5 + dx * out, 0.5 + dy * out)
        }),
        // a calligraphic hook: one curved stroke from the lower left, a thin tail at first, swelling
        // as it rises, curling over at the top right and thinning out again, so a stroke that
        // sweeps it paints a broad band (a stock tip of one such curve's stand-in)
        shape::STOCK_HOOK => hook(side),
        // grain filling nearly all of the square, its corners rounded off: what a stock tip we know
        // nothing of but how its ink lies across and down stands in as (see [`shaped`]), so a bar, a
        // block or a slab can take its outline where a disc couldn't
        shape::STOCK_GRAINY_SQUARE => field(side, |x, y| {
            let (u, v) = ((x - 0.5).abs() * 2.0, (y - 0.5).abs() * 2.0);
            let r = (u.powi(4) + v.powi(4)).powf(0.25);
            let n = fbm_finer(x * 12.0, y * 12.0, 12, 3, more, 13);
            edge((0.96 - r) * 24.0, zoom) * (0.55 + 0.45 * n).clamp(0.0, 1.0)
        }),
        // a thin straight line corner to corner, its ends tapered: a spike's stand-in
        shape::STOCK_LINE => field(side, |x, y| {
            // along the diagonal from the lower left, and how far off it
            let (u, v) = (x - 0.5, 0.5 - y);
            let along = (u + v) / std::f32::consts::SQRT_2;
            let off = (u - v).abs() / std::f32::consts::SQRT_2;
            let reach = 0.66;
            let thick = 0.012 * (1.0 - (along / reach).powi(2)).max(0.0).sqrt();
            smooth(((thick - off) * side as f32 + 0.5).clamp(0.0, 1.0))
        }),
        shape::DAISY => daisy(side),
        shape::FLICKS => flicks(side),
        shape::SPINDLE => spindle(side),
        shape::STUBBLE => stubble(side),
        _ => return None,
    })
}

/// Whether our shape `index` is drawn to a stock picture's own proportions (its marks as wide,
/// as many and as fine as the picture's), so that standing in for it, it takes the picture's tones
/// but none of the marks or streaks put on a shape standing in for a picture we know less of.
pub fn drawn_to_stock(index: u32) -> bool {
    matches!(index, shape::FLICKS | shape::SPINDLE | shape::STUBBLE)
}

/// Splashes of water flicked on: six big ones (a sixteenth of the tile across down to a 26th), a
/// few drops a fifth their size near them and over the rest, many specks, all crisp, their rims
/// crowned with short spikes and fine streaks flung out of the biggest; about a twentieth of the
/// tile, inside its middle nine tenths across and three quarters down.
fn flicks(side: usize) -> Vec<u8> {
    let random = |k: u32, salt: u32| hash(k as i32, salt as i32, 7717);
    // somewhere in the part of the tile the splashes keep to
    let anywhere = |k: u32| (0.07 + 0.88 * random(k, 1), 0.16 + 0.71 * random(k, 2));
    // (middle, radius) of each: the big ones spread out, the drops round them and anywhere
    let big = [0.063f32, 0.061, 0.052, 0.039, 0.039, 0.039];
    let mut blots: Vec<((f32, f32), f32)> = big.iter().enumerate().map(|(k, &r)| (anywhere(k as u32), r)).collect();
    let drops = [0.021f32, 0.021, 0.0125, 0.0125, 0.0104, 0.0098];
    let specks = (0..16).map(|k| 0.003 + 0.0065 * random(k, 3));
    let dots = (0..73).map(|k| 0.0015 + 0.0015 * random(k, 4));
    for (k, r) in drops.into_iter().chain(specks).chain(dots).enumerate() {
        let k = k as u32 + 100;
        let at = if random(k, 5) < 0.5 {
            // thrown off one of the big ones, two to four of its radii out
            let ((x, y), from) =
                blots.get((random(k, 6) * big.len() as f32) as usize % big.len()).copied().unwrap_or_default();
            let (way, out) = (random(k, 7) * TAU, from * (2.0 + 2.0 * random(k, 8)));
            (x + way.cos() * out, y + way.sin() * out)
        } else {
            anywhere(k)
        };
        blots.push((at, r));
    }
    // fine streaks flung out of the big ones: (from, to)
    let streaks: Vec<((f32, f32), (f32, f32))> = (0..14u32)
        .filter_map(|k| {
            let &((x, y), r) = blots.get(k as usize % big.len())?;
            let way = random(k, 9) * TAU;
            let (from, to) = (r * (1.6 + random(k, 10)), r * (2.2 + 1.6 * random(k, 11)));
            Some(((x + way.cos() * from, y + way.sin() * from), (x + way.cos() * to, y + way.sin() * to)))
        })
        .collect();
    field(side, |x, y| {
        let mut ink = 0.0f32;
        for (k, &((bx, by), r)) in blots.iter().enumerate() {
            let (dx, dy) = (x - bx, y - by);
            let d = (dx * dx + dy * dy).sqrt();
            if d > r * 1.4 + 2.0 / TEXTURE_SIZE as f32 {
                continue;
            }
            // the rim: a few short spikes round it, a wobble between
            let a = dy.atan2(dx);
            let seed = k as u32 * 31;
            let mut rim = 1.0 + 0.05 * (a * 5.0 + random(seed, 12) * TAU).sin();
            for s in 0..9u32 {
                let off = (a - random(seed + s, 13) * TAU + PI).rem_euclid(TAU) - PI;
                rim += 0.3 * random(seed + s, 14) * (1.0 - (off / 0.12).abs()).max(0.0);
            }
            ink = ink.max(((r * rim - d) * side as f32 * 1.5 + 0.5).clamp(0.0, 1.0));
        }
        for &(from, to) in &streaks {
            let (d, _) = segment(x, y, from, to);
            ink = ink.max(mark(d, 0.6 / TEXTURE_SIZE as f32, side) * 0.8);
        }
        ink
    })
}

/// A spindle: one fine line across the tile's diagonal, up from its lower left corner, nine
/// tenths of the diagonal long, its width (at most a 140th of the tile) going to nothing straight
/// from its middle to its ends.
fn spindle(side: usize) -> Vec<u8> {
    let (a, b) = ((0.065, 0.937), (0.95, 0.044));
    let widest = 1.0 / 140.0;
    let texel = 1.0 / side as f32;
    field(side, |x, y| {
        let (d, t) = segment(x, y, a, b);
        let width = widest * (1.0 - (2.0 * t - 1.0).abs()).powf(0.9);
        // a line under a texel wide as faint as it is thin
        mark(d, width.max(texel) * 0.5, side) * (width / texel).min(1.0)
    })
}

/// Stubble: 110 short hairs, each a shallow arc lying across (ends up, a 30th of the tile long,
/// brightest at its middle and fading to its ends, a 400th thick), scattered over the tile's
/// middle three quarters, thicker towards its middle.
fn stubble(side: usize) -> Vec<u8> {
    let random = |k: u32, salt: u32| hash(k as i32, salt as i32, 9091);
    let hairs: Vec<(f32, f32, f32)> = (0..110u32)
        .map(|k| {
            let (way, out) = (random(k, 1) * TAU, random(k, 2).powf(0.7));
            (0.505 + 0.37 * out * way.cos(), 0.51 + 0.39 * out * way.sin(), (random(k, 3) - 0.5) * 0.25)
        })
        .collect();
    let (half, sag, thick) = (0.016f32, 0.004f32, 1.25f32 / 1024.0);
    let least = 0.5 / side as f32;
    field(side, |x, y| {
        let mut ink = 0.0f32;
        for &(hx, hy, turn) in &hairs {
            let (dx, dy) = (x - hx, y - hy);
            if dx.abs() > half * 1.2 || dy.abs() > half * 0.6 {
                continue;
            }
            let (c, s) = (turn.cos(), turn.sin());
            let (u, v) = (dx * c + dy * s, -dx * s + dy * c);
            let along = u / half;
            if along.abs() > 1.0 {
                continue;
            }
            // the arc's ends lie `sag` above its middle
            let d = (v + sag * (1.0 - along * along)).abs();
            ink = ink.max(mark(d, thick.max(least), side) * (thick / least).min(1.0) * (1.0 - along * along).sqrt());
        }
        ink
    })
}

/// The hook's pixels (see the shape of that name): the distance to a curve sampled along its length,
/// against how thick the curve is there.
fn hook(side: usize) -> Vec<u8> {
    // a cubic from the lower left up to the top right, then curling back over
    let curve = |t: f32| {
        let p = [(0.08f32, 0.92f32), (0.45, 0.80), (0.86, 0.50), (0.82, 0.16)];
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        let x = w.iter().zip(&p).map(|(w, p)| w * p.0).sum::<f32>();
        let y = w.iter().zip(&p).map(|(w, p)| w * p.1).sum::<f32>();
        (x, y)
    };
    let curl = |t: f32| {
        // a short arc over to the left at the top
        let a = std::f32::consts::PI * (0.15 + 0.85 * t);
        (0.72 + 0.10 * a.cos(), 0.16 - 0.08 * a.sin())
    };
    const STEPS: usize = 160;
    let points: Vec<(f32, f32, f32)> = (0..=STEPS)
        .map(|i| {
            let s = i as f32 / STEPS as f32;
            // the first four fifths along the cubic, the rest round the curl
            let ((x, y), along) = if s < 0.8 { (curve(s / 0.8), s) } else { (curl((s - 0.8) / 0.2), s) };
            // a thin tail at the start, widest about seven tenths along, tapering into the curl
            let thick = 0.005 + 0.042 * (std::f32::consts::PI * along * along).sin().max(0.0);
            (x, y, thick)
        })
        .collect();
    field(side, move |x, y| {
        let inside = points.iter().fold(f32::MIN, |best, &(px, py, thick)| best.max(thick - (x - px).hypot(y - py)));
        // about a texel of anti-aliasing at the edge
        smooth((inside * side as f32 + 0.5).clamp(0.0, 1.0))
    })
}

/// An ink blot spread over two thirds of the square, as a stock ink blot's mass, and drops about
/// it: `place` moves each drop's middle from where it was scattered (within [`SCATTER_REACH`] of
/// the square's middle).
fn blot(side: usize, place: fn(f32, f32) -> (f32, f32)) -> Vec<u8> {
    let drops = scatter_points(53, 9);
    field(side, move |x, y| {
        let (x, y) = (0.5 + (x - 0.5) / 1.22, 0.5 + (y - 0.5) / 1.22);
        let n = fbm(x * 3.0, y * 3.0, 3, 3, 41);
        let body = smooth(((2.0 * BLOT_BODY - radius(x, y) - (n - 0.5) * 0.5) * 9.0).clamp(0.0, 1.0));
        let spots = drops.iter().fold(0.0f32, |acc, &(cx, cy, r)| {
            let (dx, dy) = place(cx, cy);
            let d = ((x - dx).powi(2) + (y - dy).powi(2)).sqrt() / (r * 0.6);
            acc.max(smooth(((1.0 - d) * 4.0).clamp(0.0, 1.0)))
        });
        body.max(spots) * (1.0 - 0.05 * radius(x, y))
    })
}

/// The blot's body's radius (before its noisy edge), a share of the square it is drawn in.
const BLOT_BODY: f32 = 0.31;

/// How far from the square's middle [`scatter_points`] scatters, a share of the square.
const SCATTER_REACH: f32 = 0.38;

/// A stand-in for stock shapes our others don't come near: a flat brush's stroke, laid along x
/// (the way a stroke that turns its shape with it runs). Stamped close along a stroke, each part of
/// it draws a line, so the stroke comes out streaked along its length as a bristle brush's does.
fn flat_stroke(side: usize) -> Vec<u8> {
    let zoom = zoom_of(side);
    field(side, |x, y| {
        // the bristles' streaks run its length, with fine gaps between them and broader bands thin
        // in paint, its top and bottom ragged and its far end breaking up where the paint runs out
        let (u, v) = centred(x, y);
        let top = -0.7 + 0.08 * (value_noise_xy(x * 9.0, 0.0, (9, 1), 2111) - 0.5);
        let bottom = 0.74 + 0.08 * (value_noise_xy(x * 9.0, 0.0, (9, 1), 2113) - 0.5);
        let start = -0.88 + 0.1 * value_noise_xy(0.0, y * 40.0, (1, 40), 2117);
        let end = 0.92 - 0.25 * value_noise_xy(0.0, y * 20.0, (1, 20), 2119);
        let body = edge((v - top) * 40.0, zoom)
            * edge((bottom - v) * 40.0, zoom)
            * edge((u - start) * 30.0, zoom)
            * smooth((end - u) * 12.0);
        // the streaks hold all the way along, so a stroke keeps them however it builds up: fine
        // gaps between hairs, streaks thick and thin in paint, broad bands (the streaks wide enough
        // to show on a small stamp, the gaps for a big one)
        let gaps = edge((value_noise_xy(0.0, y * 96.0, (1, 96), 2129) - 0.15) * 5.0, zoom);
        let streaks = 0.55 + 0.45 * smooth((value_noise_xy(0.0, y * 28.0, (1, 28), 2131) - 0.25) * 2.5);
        let bands = 0.7 + 0.3 * value_noise_xy(0.0, y * 7.0, (1, 7), 2133);
        let dry = smooth((value_noise_xy(x * 8.0, y * 60.0, (8, 60), 2137) - (u - 0.15) * 0.7) * 4.0);
        body * gaps * streaks * bands * dry
    })
}

/// Stand-ins for stock shapes our others don't come near, two scatters of dots (stamped close along
/// a stroke, each draws a line, so the stroke comes out streaked): dots of every size, some crisp
/// and some blurred, through a disc; or (`square`) soft dots through a square, crowding along its
/// edges. Each (centre x, centre y, radius, blur 0..1), as [`dots`] draws them.
fn stock_dots(square: bool) -> Vec<(f32, f32, f32, f32)> {
    if !square {
        return (0..380)
            .map(|i| {
                let a = hash(i, 1, 2141) * TAU;
                let d = hash(i, 2, 2141).sqrt() * 0.43;
                let r = 0.005 + 0.026 * hash(i, 3, 2141).powi(2);
                (0.5 + a.cos() * d, 0.5 + a.sin() * d, r, 0.8 * hash(i, 4, 2141).powi(2))
            })
            .collect();
    }
    (0..230)
        .map(|i| {
            let (mut cx, mut cy) = (0.07 + 0.86 * hash(i, 1, 2143), 0.07 + 0.86 * hash(i, 2, 2143));
            if i % 3 == 0 {
                // pushed out to the nearest edge
                let (dx, dy) = (cx - 0.5, cy - 0.5);
                if dx.abs() > dy.abs() {
                    cx = 0.5 + dx.signum() * (0.38 + 0.05 * hash(i, 5, 2143));
                } else {
                    cy = 0.5 + dy.signum() * (0.38 + 0.05 * hash(i, 5, 2143));
                }
            }
            (cx, cy, 0.008 + 0.022 * hash(i, 3, 2143).powf(1.5), 0.4 + 0.6 * hash(i, 4, 2143))
        })
        .collect()
}

/// The brightest of `dots` (centre x, centre y, radius, blur 0..1) at (`x`, `y`): a crisp dot full
/// out to its edge, a blurred one spread wider and paler.
fn dots(dots: &[(f32, f32, f32, f32)], x: f32, y: f32) -> f32 {
    dots.iter().fold(0.0f32, |acc, &(cx, cy, r, blur)| {
        let reach = r * (1.0 + 1.5 * blur);
        let (dx, dy) = (x - cx, y - cy);
        if dx.abs() > reach || dy.abs() > reach {
            return acc;
        }
        let d = (dx * dx + dy * dy).sqrt() / reach;
        acc.max(smooth((1.0 - d) * (6.0 - 4.5 * blur)) * (1.0 - 0.35 * blur))
    })
}

/// The slim oval's half width and half height, shares of its square.
const SLIM_OVAL_HALF: (f32, f32) = (0.432, 0.102);

/// The ultra soft round's middle, as measured.
const ULTRA_SOFT_PEAK: f32 = 0.43;

/// How much the solid dab raises the small chunk's values.
const DAB_GAIN: f32 = 1.4;

/// How much smaller the stock stand-ins draw our grainy disc, chunk and pastel shapes: as measured,
/// stock brushes on them paint as much of a measured drawing pad as ours then do (the disc's
/// and the chunk's median areas within 3%; the pastel by the stock shapes' spread alone).
const SHRINK_DISC: f32 = 1.7;
const SHRINK_CHUNK: f32 = 1.47;
const SHRINK_PASTEL: f32 = 1.35;

/// `pixels` (a square image) drawn `by` times smaller about its middle, empty around.
fn shrunk(pixels: &[u8], by: f32) -> Vec<u8> {
    let n = pixels.len().isqrt();
    let img: Vec<f32> = pixels.iter().map(|&v| f32::from(v)).collect();
    let mut out = vec![0u8; n * n];
    for (k, px) in out.iter_mut().enumerate() {
        let (x, y) = ((k % n) as f32 + 0.5, (k / n) as f32 + 0.5);
        let half = n as f32 / 2.0;
        let (u, v) = (half + (x - half) * by, half + (y - half) * by);
        if u < 0.5 || v < 0.5 || u > n as f32 - 0.5 || v > n as f32 - 0.5 {
            continue;
        }
        *px = sample_wrapped(&img, n, u - 0.5, v - 0.5).round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Our grains' names, in their order.
const GRAIN_NAMES: [&str; GRAIN_COUNT as usize] = [
    "Paper",
    "Coarse",
    "Weave",
    "Fibre",
    "Linen",
    "Wash",
    "Concrete",
    "Halftone",
    "Hatching",
    "Noise",
    "Crumpled",
    "Stipple",
    "Wood",
    "Brick",
    "Scales",
    "Cells",
    "Marble",
    "Stripes",
    "Grunge",
    "Sand",
    "Hexagons",
    "Caustics",
    "Leather",
    "Bark",
    "Scratches",
    "Checker",
    "Speckle",
    "Twill",
    "Rust",
    "Recycled",
    "Paper sheet",
    "Worn paper",
    "Charcoal",
    "Fine noise",
    "Speckled",
    "Mottled",
    "Rough canvas",
    "Sprayed",
    "Dry patches",
    "Graph paper",
    "Dot grid",
    "Dotted star",
    "Diagonal stripes",
    "Fine grid",
    "Soft dots",
    "Labyrinth",
    "Cubes",
    "Spots",
    "Disc",
    "Scrolls",
];

fn make_grains() -> Vec<GrayImage> {
    let mut grains: Vec<GrayImage> = Vec::with_capacity(GRAIN_NAMES.len());
    for (index, name) in (0..).zip(GRAIN_NAMES) {
        // one drawn from another takes that one as it was drawn already
        let pixels = match grain_from(index) {
            Some((from, make)) => make(grains.get(from as usize).map_or(&[][..], |g| &g.pixels[..])),
            None => draw_grain(index, TEXTURE_SIZE).unwrap_or_default(),
        };
        grains.push(GrayImage { name, pixels });
    }
    grains
}

/// The grain a grain is drawn from, and how, for those drawn from another of ours: stand-ins for
/// stock grains, our own patterns with their values spread as measured from brushes drawn with the
/// stock ones.
fn grain_from(index: u32) -> Option<(u32, DrawnFrom)> {
    Some(match index {
        grain::CHARCOAL => (grain::COARSE, |p| respread(p, &CHARCOAL_SPREAD)),
        grain::FINE_NOISE => (grain::NOISE, |p| respread(p, &FINE_NOISE_SPREAD)),
        _ => return None,
    })
}

/// `pixels`, a grain, its values spread onto `spread` keeping their order (see [`ranked`]).
fn respread(pixels: &[u8], spread: &[(f32, f32)]) -> Vec<u8> {
    ranked(&pixels.iter().map(|&v| f32::from(v)).collect::<Vec<f32>>(), spread)
}

/// Our grain `index` drawn `side` pixels across, tiling; none past the last. Bigger than
/// [`TEXTURE_SIZE`], it is the same tile, sharper (see [`zoom_of`] and [`octaves_more`]).
fn draw_grain(index: u32, side: usize) -> Option<Vec<u8>> {
    if let Some((from, make)) = grain_from(index) {
        return Some(make(&draw_grain(from, side)?));
    }
    let zoom = zoom_of(side);
    let more = octaves_more(side);
    Some(match index {
        grain::PAPER => field(side, |x, y| contrast(fbm_finer(x * 32.0, y * 32.0, 32, 4, more, 101), 1.6)),
        grain::COARSE => field(side, |x, y| {
            // fine fibres crossing both ways, light with darker threads between (as a charcoal
            // paper's tooth lets paint through)
            let along = fbm_xy_finer(x * 128.0, y * 48.0, (128, 48), 3, more, 202);
            let down = fbm_xy_finer(x * 48.0, y * 128.0, (48, 128), 3, more, 203);
            let n = (along + down) / 2.0;
            let dark = ((COARSE_LIGHT - n) * COARSE_DEPTH).max(0.0);
            0.97 - dark
        }),
        grain::WEAVE => field(side, |x, y| {
            // horizontal and vertical threads, crossing over and under in a checker
            let across = 0.5 + 0.5 * (y * TAU * 16.0).sin();
            let down = 0.5 + 0.5 * (x * TAU * 16.0).sin();
            let over = 0.5 + 0.5 * (x * TAU * 8.0).sin() * (y * TAU * 8.0).sin();
            let n = fbm_finer(x * 64.0, y * 64.0, 64, 2, more, 303);
            (across + (down - across) * over) * 0.8 + 0.2 * n
        }),
        grain::FIBRE => field(side, |x, y| {
            contrast(
                0.6 * fbm_finer(x * 64.0, y * 4.0, 64, 3, more, 404)
                    + 0.4 * fbm_finer(x * 4.0, y * 64.0, 4, 3, more, 505),
                1.8,
            )
        }),
        grain::LINEN => field(side, |x, y| {
            // uneven threads: their thickness wanders
            let across = value_noise(x * 4.0, y * 4.0, 4, 601);
            let down = value_noise(x * 4.0, y * 4.0, 4, 607);
            let threads = 0.5
                + 0.25 * ((y * 24.0 + across * 1.5) * std::f32::consts::TAU).sin()
                + 0.25 * ((x * 24.0 + down * 1.5) * std::f32::consts::TAU).sin();
            contrast(threads * 0.8 + 0.2 * fbm_finer(x * 32.0, y * 32.0, 32, 2, more, 613), 1.6)
        }),
        grain::WASH => field(side, |x, y| {
            // broad pools of pigment, granulated
            let pools = fbm(x * 4.0, y * 4.0, 4, 3, 701);
            let grains = fbm_finer(x * 64.0, y * 64.0, 64, 2, more, 709);
            contrast(0.7 * pools + 0.3 * grains, 1.9)
        }),
        grain::CONCRETE => field(side, |x, y| {
            let surface = fbm_finer(x * 16.0, y * 16.0, 16, 4, more, 801);
            let pits = fbm_finer(x * 32.0, y * 32.0, 32, 2, more, 809);
            let pitted = if pits < 0.3 { pits / 0.3 * 0.4 } else { 1.0 };
            contrast(surface, 1.4) * pitted
        }),
        grain::HALFTONE => field(side, |x, y| {
            // a dot grid, 16 across
            let (fx, fy) = ((x * 16.0).fract() - 0.5, (y * 16.0).fract() - 0.5);
            smooth(((0.36 - (fx * fx + fy * fy).sqrt()) * 30.0).clamp(0.0, 1.0))
        }),
        grain::HATCHING => field(side, |x, y| {
            // two sets of diagonal lines, slightly wavering
            let w = (fbm(x * 8.0, y * 8.0, 8, 2, 901) - 0.5) * 0.04;
            let one = 0.5 + 0.5 * ((x + y + w) * std::f32::consts::TAU * 16.0).cos();
            let two = 0.5 + 0.5 * ((x - y + w) * std::f32::consts::TAU * 16.0).cos();
            edge((one.max(two) - 0.7) * 6.0, zoom)
        }),
        grain::NOISE => field(side, |x, y| {
            // near white, darkened here and there by fine flecks about two texels across: each
            // texel the average of sparse specks over it and its neighbours to the right and
            // below (wrapping, so it tiles), as a speckled image shrunk to half
            let n = TEXTURE_SIZE as i32;
            let (i, j) = ((x * n as f32) as i32, (y * n as f32) as i32);
            let specks: f32 = (0..4)
                .map(|k| {
                    let u = hash((i + k % 2).rem_euclid(n), (j + k / 2).rem_euclid(n), 1471);
                    ((u - 0.5) * 2.0).max(0.0).powi(2)
                })
                .sum();
            1.0 - specks / 4.0
        }),
        grain::CRUMPLED => field(side, |x, y| {
            // ridges where folds were
            let ridged = 1.0 - (fbm_finer(x * 6.0, y * 6.0, 6, 4, more, 1103) * 2.0 - 1.0).abs();
            contrast(ridged.powi(2), 1.5)
        }),
        grain::STIPPLE => field(side, |x, y| {
            // a dot in each of 32 cells across, placed and sized at random
            let (cx, cy) = ((x * 32.0).floor() as i32, (y * 32.0).floor() as i32);
            let (ox, oy) = (0.25 + 0.5 * hash(cx, cy, 1201), 0.25 + 0.5 * hash(cx, cy, 1213));
            let r = 0.2 + 0.2 * hash(cx, cy, 1223);
            let (fx, fy) = ((x * 32.0).fract() - ox, (y * 32.0).fract() - oy);
            smooth(((r - (fx * fx + fy * fy).sqrt()) * 20.0).clamp(0.0, 1.0))
        }),
        grain::WOOD => field(side, |x, y| {
            // growth rings running across, bent round knots, fine fibres along them
            let bend = fbm(x * 2.0, y * 2.0, 2, 3, 1301);
            let rings = 0.5 + 0.5 * ((y * 10.0 + bend * 2.0) * TAU).sin();
            let fibres = fbm_xy_finer(x * 4.0, y * 64.0, (4, 64), 2, more, 1307);
            contrast(0.6 * rings.powf(0.7) + 0.4 * fibres, 1.3)
        }),
        grain::BRICK => field(side, |x, y| {
            // eight courses of four bricks, every other one shifted by half; worn faces, deep joints
            let row = (y * 8.0).floor() as i32;
            let shift = 0.5 * row.rem_euclid(2) as f32;
            let (fx, fy) = ((x * 4.0 + shift).fract(), (y * 8.0).fract());
            let joint = (fx.min(1.0 - fx) / 4.0).min(fy.min(1.0 - fy) / 8.0);
            let brick = ((x * 4.0 + shift).floor() as i32).rem_euclid(4);
            let face = 0.62 + 0.18 * hash(brick, row, 1311) + 0.25 * fbm_finer(x * 16.0, y * 16.0, 16, 3, more, 1313);
            smooth((joint - 0.006) * 300.0) * face.min(1.0)
        }),
        grain::SCALES => field(side, |x, y| {
            // rows of round scales, each row lying over the one above, darker at their edges
            let (px, py) = (x * 8.0, y * 16.0);
            let (ci, cj) = (px.floor() as i32, py.floor() as i32);
            let mut best: Option<(i32, f32)> = None;
            for j in cj - 2..=cj + 2 {
                for i in ci - 1..=ci + 1 {
                    let cx = i as f32 + 0.5 * j.rem_euclid(2) as f32;
                    let d = ((px - cx).powi(2) + ((py - j as f32) * 0.5).powi(2)).sqrt() / 0.62;
                    if d < 1.0 && best.is_none_or(|(bj, _)| j > bj) {
                        best = Some((j, d));
                    }
                }
            }
            best.map_or(0.0, |(_, d)| 0.3 + 0.7 * smooth((1.0 - d) * 3.0))
        }),
        grain::CELLS => field(side, |x, y| {
            let (f1, f2) = cells(x, y, 12, 1321);
            edge((f2 - f1 - 0.06) * 8.0, zoom) * (0.8 + 0.2 * f1)
        }),
        grain::MARBLE => field(side, |x, y| {
            // thin dark veins wandering through a pale stone
            let warp = fbm_finer(x * 4.0, y * 4.0, 4, 5, more, 1331);
            let veins = ((x + y + warp * 1.6) * TAU * 2.0).sin().abs();
            veins.powf(0.25) * (0.85 + 0.15 * fbm_finer(x * 32.0, y * 32.0, 32, 2, more, 1337))
        }),
        grain::STRIPES => field(side, |x, y| {
            let wobble = (fbm(x * 4.0, y * 4.0, 4, 2, 1341) - 0.5) * 0.4;
            edge((0.5 + 0.5 * ((x * 12.0 + wobble) * TAU).cos() - 0.5) * 6.0, zoom)
        }),
        grain::GRUNGE => field(side, |x, y| {
            // blotches, flecks knocked out of them
            let blotch = contrast(fbm_finer(x * 6.0, y * 6.0, 6, 5, more, 1351), 2.4);
            let flecks = edge((value_noise(x * 96.0, y * 96.0, 96, 1357) - 0.8) * 10.0, zoom);
            (blotch - flecks * 0.8).max(0.0) * (0.8 + 0.2 * fbm_finer(x * 32.0, y * 32.0, 32, 2, more, 1359))
        }),
        grain::SAND => field(side, |x, y| {
            let fine = value_noise(x * 128.0, y * 128.0, 128, 1361);
            contrast(0.7 * fine + 0.3 * value_noise(x * 64.0, y * 64.0, 64, 1367), 2.6)
        }),
        grain::HEXAGONS => field(side, |x, y| {
            // a honeycomb's walls: 14 cells across, 16 rows (as near to regular as tiling allows)
            let (cols, rows) = (14.0f32, 16.0f32);
            let (ci, cj) = ((x * cols).floor() as i32, (y * rows).floor() as i32);
            let (mut f1, mut f2) = (f32::MAX, f32::MAX);
            for j in cj - 1..=cj + 1 {
                for i in ci - 1..=ci + 1 {
                    let cx = (i as f32 + 0.5 + 0.5 * j.rem_euclid(2) as f32) / cols;
                    let d = ((x - cx).powi(2) + (y - (j as f32 + 0.5) / rows).powi(2)).sqrt();
                    (f1, f2) = if d < f1 { (d, f1) } else { (f1, f2.min(d)) };
                }
            }
            edge((f2 - f1 - 0.006) * 200.0, zoom)
        }),
        grain::CAUSTICS => field(side, |x, y| {
            // light netted on the bottom of a pool: bright where the cells meet
            let wx = (fbm(x * 3.0, y * 3.0, 3, 3, 1381) - 0.5) * 0.15;
            let wy = (fbm(x * 3.0, y * 3.0, 3, 3, 1387) - 0.5) * 0.15;
            let (f1, f2) = cells(x + wx, y + wy, 8, 1391);
            1.0 - smooth((f2 - f1) * 5.0)
        }),
        grain::LEATHER => field(side, |x, y| {
            let (f1, f2) = cells(x, y, 24, 1401);
            let crease = edge((f2 - f1) * 6.0, zoom) * (0.8 + 0.2 * f1);
            let pores = edge((value_noise(x * 128.0, y * 128.0, 128, 1409) - 0.75) * 8.0, zoom);
            contrast(crease * (0.8 + 0.2 * fbm(x * 8.0, y * 8.0, 8, 2, 1405)) - 0.3 * pores, 1.3)
        }),
        grain::BARK => field(side, |x, y| {
            // deep furrows running up and down
            let ridges = 1.0 - (fbm_xy_finer(x * 16.0, y * 2.0, (16, 2), 4, more, 1411) * 2.0 - 1.0).abs();
            contrast(ridges.powf(1.5) * (0.8 + 0.2 * fbm_finer(x * 32.0, y * 32.0, 32, 2, more, 1413)), 1.6)
        }),
        grain::SCRATCHES => {
            // fine lines every way across a worn surface
            let lines: Vec<(f32, f32, f32, f32, f32)> = (0..60)
                .map(|k| {
                    let a = hash(k, 3, 1421) * PI;
                    let len = 0.05 + 0.2 * hash(k, 4, 1421).powi(2);
                    (hash(k, 1, 1421), hash(k, 2, 1421), a.cos() * len, a.sin() * len, 0.6 + 0.4 * hash(k, 5, 1421))
                })
                .collect();
            field(side, move |x, y| {
                let marks = lines.iter().fold(0.0f32, |acc, &(cx, cy, dx, dy, depth)| {
                    // the nearest copy of the line, the image wrapping round
                    let (u, v) = (x - cx - (x - cx).round(), y - cy - (y - cy).round());
                    acc.max(smooth((0.002 - segment(u, v, (-dx, -dy), (dx, dy)).0) * 400.0) * depth)
                });
                (1.0 - 0.9 * marks) * (0.9 + 0.1 * fbm(x * 16.0, y * 16.0, 16, 2, 1427))
            })
        }
        grain::CHECKER => field(side, |x, y| {
            // shifted by half a square, so its edges fall inside one, not between two
            let (i, j) = ((x * 16.0 + 0.5).floor() as i32, (y * 16.0 + 0.5).floor() as i32);
            f32::from(u8::from((i + j).rem_euclid(2) == 0))
        }),
        grain::SPECKLE => field(side, |x, y| {
            // a sparse spray of tiny dots
            let (ci, cj) = ((x * 32.0).floor() as i32, (y * 32.0).floor() as i32);
            if hash(ci, cj, 1431) > 0.5 {
                return 0.0;
            }
            let (ox, oy) = (0.3 + 0.4 * hash(ci, cj, 1433), 0.3 + 0.4 * hash(ci, cj, 1437));
            let (fx, fy) = ((x * 32.0).fract() - ox, (y * 32.0).fract() - oy);
            smooth((0.1 + 0.2 * hash(ci, cj, 1439) - (fx * fx + fy * fy).sqrt()) * 10.0)
        }),
        grain::TWILL => field(side, |x, y| {
            // diagonal ribs, each thread showing in short steps
            let rib = 0.5 + 0.5 * ((x + y) * 32.0 * TAU).cos();
            let steps = 0.5 + 0.5 * (x * 64.0 * TAU).cos();
            contrast(0.6 * rib + 0.25 * steps + 0.15 * fbm_finer(x * 32.0, y * 32.0, 32, 2, more, 1441), 1.5)
        }),
        grain::RUST => field(side, |x, y| {
            // patches eaten away, pitted inside
            let n = fbm_finer(x * 8.0, y * 8.0, 8, 5, more, 1451);
            let patches = smooth((n - 0.45) * 6.0);
            let pits = edge((value_noise(x * 64.0, y * 64.0, 64, 1459) - 0.7) * 6.0, zoom);
            contrast(0.35 + 0.65 * patches * (1.0 - 0.6 * pits), 1.3)
        }),
        grain::RECYCLED => recycled(side),
        grain::SHEET => sheet(side),
        grain::WORN => worn(side),
        grain::SPECKLED => ranked(&soft_noise(side, 1601, 0.4), &SPECKLED_SPREAD),
        grain::MOTTLED => ranked(&soft_noise(side, 4001, 0.4), &MOTTLED_SPREAD),
        grain::ROUGH_CANVAS => rough_canvas(side),
        grain::SPRAYED => ranked(&soft_noise(side, 4201, 0.4), &SPRAYED_SPREAD),
        grain::DRY_PATCHES => dry_patches(side),
        grain::GRAPH_PAPER => graph_paper(side),
        grain::DOT_GRID => dot_grid(side),
        grain::DOTTED_STAR => dotted_star(side),
        grain::DIAGONAL_STRIPES => diagonal_stripes(side),
        grain::FINE_GRID => fine_grid(side),
        grain::SOFT_DOTS => soft_dots(side),
        grain::LABYRINTH => labyrinth(side),
        grain::CUBES => cubes(side),
        grain::SPOTS => spots(side),
        grain::DISC => disc(side),
        grain::SCROLLS => scrolls(side),
        _ => return None,
    })
}

/// Scrolls: four white curls on black, one in each quarter of the tile, each turned a quarter from
/// the one before: a band winding in about one and a quarter turns, thinning as it goes, ending in
/// a dot, with two leaves on its outer curve; about a third of the tile white.
fn scrolls(side: usize) -> Vec<u8> {
    let zoom = zoom_of(side);
    // a curl's band as points along it (curl-sized units, its middle at 0), each with its half
    // width; and its leaves, as (middle, way, half length, half width)
    let turns = 1.25;
    let steps = 120;
    let band: Vec<(f32, f32, f32)> = (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let angle = t * turns * TAU;
            let r = 0.92 - 0.72 * t;
            (r * angle.cos(), r * angle.sin(), 0.17 - 0.09 * t)
        })
        .collect();
    let leaves = [(-0.66, -0.86, 0.5, 0.34, 0.16), (1.04, -0.42, -1.2, 0.32, 0.15)];
    let curl = |u: f32, v: f32| -> f32 {
        let mut ink = 0.0f32;
        for pair in band.windows(2) {
            if let [(ax, ay, aw), (bx, by, bw)] = *pair {
                let (dx, dy) = (bx - ax, by - ay);
                let t = (((u - ax) * dx + (v - ay) * dy) / (dx * dx + dy * dy).max(1e-9)).clamp(0.0, 1.0);
                let d = (u - ax - t * dx).hypot(v - ay - t * dy);
                let w = aw + (bw - aw) * t;
                ink = ink.max(((w - d) * 60.0 * zoom + 0.5).clamp(0.0, 1.0));
            }
        }
        // the dot it ends in
        if let Some(&(ex, ey, _)) = band.last() {
            ink = ink.max(((0.13 - (u - ex).hypot(v - ey)) * 60.0 * zoom + 0.5).clamp(0.0, 1.0));
        }
        for &(lx, ly, way, long, wide) in &leaves {
            let (c, s) = (f32::cos(way), f32::sin(way));
            let (a, b) = ((u - lx) * c + (v - ly) * s, -(u - lx) * s + (v - ly) * c);
            // a leaf: pointed at both ends
            let e = (a / long).powi(2) + (b / (wide * (1.0 - (a / long).abs()).max(0.0).sqrt())).powi(2);
            ink = ink.max(((1.0 - e) * 8.0).clamp(0.0, 1.0));
        }
        ink
    };
    field(side, |x, y| {
        let mut ink = 0.0f32;
        for (k, (cx, cy)) in [(0.25, 0.25), (0.75, 0.25), (0.75, 0.75), (0.25, 0.75)].into_iter().enumerate() {
            let turn = k as f32 * FRAC_PI_2;
            let (c, s) = (turn.cos(), turn.sin());
            // the curl a quarter of the tile wide either way of its middle, read across the tile's
            // wrap
            for (ox, oy) in [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let (px, py) = ((x + ox - cx) / 0.215, (y + oy - cy) / 0.215);
                if px.abs() > 1.4 || py.abs() > 1.4 {
                    continue;
                }
                ink = ink.max(curl(px * c + py * s, -px * s + py * c));
            }
        }
        ink
    })
}

/// Stacked cubes seen from above at an angle: two to the tile across and two rows down (each row
/// half a cube along), each cube's top black, its left face grey and its right face white, a third
/// of the tile each. A cube's top is a rhombus as wide as the cube and a third of a row tall (so
/// the cubes are a little taller than true isometric ones), its faces hanging a row's two thirds
/// below it.
fn cubes(side: usize) -> Vec<u8> {
    const TOP: f32 = 0.0;
    const LEFT: f32 = 0.47;
    const RIGHT: f32 = 0.97;
    field(side, |x, y| {
        // in cubes across (a cube 1 wide) and rows down (a top's half height a third of a row),
        // a quarter cube along so that no edge lies on the tile's
        let (u, v) = (x * 2.0 + 0.25, y * 2.0);
        let row = v.floor();
        // the cube whose top this row's line runs through, and the point from that top's middle
        let shift = if (row as i32).rem_euclid(2) == 1 { 0.5 } else { 0.0 };
        let near = |r: f32, s: f32| {
            let cx = (u - s).round() + s;
            (u - cx, v - r)
        };
        let (dx, dy) = near(row, shift);
        let (nx, ny) = near(row + 1.0, 0.5 - shift);
        let h = 1.0 / 3.0;
        // on a top: this row's or the next's
        if dx.abs() * 2.0 + dy.abs() / h <= 1.0 || nx.abs() * 2.0 + ny.abs() / h <= 1.0 {
            return TOP;
        }
        // below this row's top: the face on the side of its middle the point is
        if dx < 0.0 { LEFT } else { RIGHT }
    })
}

/// Spots: rounded white patches packed close, about forty to the tile each way, parted by black
/// gaps of uneven width, a little over half of it white (a hide's spots).
fn spots(side: usize) -> Vec<u8> {
    let n = side;
    let raw: Vec<f32> = (0..n * n)
        .map(|k| {
            let (x, y) = (((k % n) as f32 + 0.5) / n as f32, ((k / n) as f32 + 0.5) / n as f32);
            // the gaps wander: where each texel is read from is moved a little by soft noise
            let wx = x + 0.004 * (fbm(x * 16.0, y * 16.0, 16, 2, 4703) - 0.5);
            let wy = y + 0.004 * (fbm(x * 16.0, y * 16.0, 16, 2, 4709) - 0.5);
            let (f1, f2) = cells(wx.rem_euclid(1.0), wy.rem_euclid(1.0), 40, 4701);
            // how far inside its patch, against the patch next to it, rounded off at its corners
            (f2 - f1) - 0.25 * f1
        })
        .collect();
    let mut sorted = raw.clone();
    sorted.sort_by(f32::total_cmp);
    let cut = sorted.get(((n * n) as f32 * 0.48) as usize).copied().unwrap_or(0.0);
    raw.iter().map(|&v| if v > cut { 255 } else { 0 }).collect()
}

/// One hard white disc in the middle of the tile on black, four fifths of it across.
fn disc(side: usize) -> Vec<u8> {
    field(side, |x, y| mark((x - 0.5).hypot(y - 0.5), 0.398, side))
}

/// Diagonal stripes, white on black, rising to the right: five of differing widths in each quarter
/// of the tile across the diagonal (each at a share of that quarter, from its start, and its width
/// there), about a seventh of it white.
fn diagonal_stripes(side: usize) -> Vec<u8> {
    const STRIPES: [(f32, f32); 5] = [(28.0, 5.0), (48.0, 10.0), (108.0, 10.0), (134.0, 11.0), (210.0, 3.0)];
    // how many of those 256ths a texel spans across the diagonal and down
    let texel = 4.0 / zoom_of(side);
    field(side, |x, y| {
        // where along a quarter's repeat this texel's diagonal falls, in the stock picture's 256ths
        let along = ((x + y) * 1024.0).rem_euclid(256.0);
        let ink = STRIPES.iter().map(|&(at, width)| {
            let middle = at + width / 2.0;
            let d = (along - middle).abs().min(256.0 - (along - middle).abs());
            // smoothed over a texel
            ((width / 2.0 - d) / texel + 0.5).clamp(0.0, 1.0)
        });
        ink.fold(0.0, f32::max)
    })
}

/// A fine grid of white lines on black, twelve to the tile each way, each a 512th of it wide (so
/// half a texel: grey where it falls), half a square in from the tile's edges.
fn fine_grid(side: usize) -> Vec<u8> {
    field(side, |x, y| {
        let line = |v: f32| {
            let d = wrapped(v * 12.0, 0.5 + 12.0 / 1024.0) / 12.0;
            mark(d, 1.0 / 1024.0, side)
        };
        line(x).max(line(y))
    })
}

/// Dark dots on light, on a staggered lattice: fourteen to the tile across, nine rows of each kind
/// down, every other row shifted half a dot. Each dot lightens slowly from its middle (93) to its
/// edge a 50th of the tile out (137), steps up to 171 across it, and fades into the light between
/// the dots (196) by about a 25th.
fn soft_dots(side: usize) -> Vec<u8> {
    // (distance from the middle in 1792ths of the tile, tone), between which it goes straight
    const PROFILE: [(f32, f32); 11] = [
        (0.0, 93.0),
        (12.0, 100.0),
        (24.0, 116.0),
        (30.0, 126.0),
        (36.0, 137.0),
        (42.0, 171.0),
        (48.0, 180.0),
        (54.0, 186.0),
        (60.0, 190.0),
        (72.0, 196.0),
        (1792.0, 196.0),
    ];
    let (across, down) = (14.0f32, 9.0f32);
    field(side, |x, y| {
        let near = |cx: f32, cy: f32| {
            let (dx, dy) = (wrapped(x * across, cx) / across, wrapped(y * down, cy) / down);
            // its distance in the tile's widths (rows are farther apart than dots are across)
            (dx * dx + dy * dy).sqrt()
        };
        let d = near(0.0, 0.0).min(near(0.5, 0.5)) * 1792.0;
        let tone = PROFILE
            .windows(2)
            .find_map(|pair| match *pair {
                [(d0, t0), (d1, t1)] if d <= d1 => Some(t0 + (t1 - t0) * ((d - d0) / (d1 - d0)).clamp(0.0, 1.0)),
                _ => None,
            })
            .unwrap_or(196.0);
        tone / 255.0
    })
}

/// A labyrinth: winding white bands about as wide as the black between them (white a little under
/// half), some 27 to the tile: band-passed noise cut at the share that leaves that much white.
/// Bigger, the same bands (smooth noise, read between its texels) cut at the same value.
fn labyrinth(side: usize) -> Vec<u8> {
    let n = TEXTURE_SIZE;
    let raw: Vec<f32> = (0..n * n).map(|k| hash((k % n) as i32, (k / n) as i32, 7307)).collect();
    // a band of sizes about the bands' own: the noise softened a little, less it softened twice
    let (fine, broad) = (gaussian_wrapped(&raw, n, 1.6), gaussian_wrapped(&raw, n, 3.2));
    let band: Vec<f32> = fine.iter().zip(&broad).map(|(a, b)| a - b).collect();
    let mut sorted = band.clone();
    sorted.sort_by(f32::total_cmp);
    let cut = sorted.get(((n * n) as f32 * 0.55) as usize).copied().unwrap_or(0.0);
    upsampled(&band, n, side).iter().map(|&v| if v > cut { 255 } else { 0 }).collect()
}

/// How much of a texel at distance `d` (a share of the tile) from a mark's middle line or centre
/// the mark covers, the mark reaching `half` either way: its edge smoothed over one texel of an
/// image `side` texels across.
fn mark(d: f32, half: f32, side: usize) -> f32 {
    ((half - d) * side as f32 + 0.5).clamp(0.0, 1.0)
}

/// How far `v` is from `at`, the tile wrapping round (both shares of it).
fn wrapped(v: f32, at: f32) -> f32 {
    let d = (v - at).rem_euclid(1.0);
    d.min(1.0 - d)
}

/// Graph paper: dark lines across and down on white, six squares to the tile, the line through
/// the middle about four times as thick and the one along the edge a little thicker.
fn graph_paper(side: usize) -> Vec<u8> {
    // each line's place and half its width, as shares of the tile
    const LINES: [(f32, f32); 6] = [
        (0.0, 0.0039),
        (1.0 / 6.0, 0.0029),
        (2.0 / 6.0, 0.0029),
        (0.5, 0.0114),
        (4.0 / 6.0, 0.0029),
        (5.0 / 6.0, 0.0029),
    ];
    field(side, |x, y| {
        let ink =
            LINES.iter().map(|&(at, half)| mark(wrapped(x, at), half, side).max(mark(wrapped(y, at), half, side)));
        1.0 - ink.fold(0.0, f32::max)
    })
}

/// A dot grid: round dark dots on white, one in the middle of the tile and one at its corners
/// (so a grid turned a quarter of a right angle), each about a fifteenth of the tile across.
fn dot_grid(side: usize) -> Vec<u8> {
    const HALF: f32 = 0.0322;
    field(side, |x, y| {
        let corner = wrapped(x, 0.0).hypot(wrapped(y, 0.0));
        let middle = wrapped(x, 0.5).hypot(wrapped(y, 0.5));
        1.0 - mark(corner.min(middle), HALF, side)
    })
}

/// Dotted lines in a star: round dark dots on white along the middle row and column (eight each
/// side of the middle, which has none) and along both diagonals (eleven in from each corner).
fn dotted_star(side: usize) -> Vec<u8> {
    const HALF: f32 = 0.015;
    // along the middle row and column, from the edges in, and along the diagonals from the
    // corners in (shares of the tile)
    let straight: Vec<f32> = (0..8).map(|k| (63.9 + 120.0 * k as f32) / 2048.0).collect();
    let slanted: Vec<f32> = (0..11).map(|k| (58.0 + 84.35 * k as f32) / 2048.0).collect();
    let mut dots: Vec<(f32, f32)> = Vec::new();
    for &t in &straight {
        for along in [t, 1.0 - t] {
            dots.extend([(along, 0.5), (0.5, along)]);
        }
    }
    for &t in &slanted {
        for along in [t, 1.0 - t] {
            dots.extend([(along, along), (along, 1.0 - along)]);
        }
    }
    field(side, |x, y| {
        let near = dots.iter().map(|&(dx, dy)| wrapped(x, dx).hypot(wrapped(y, dy))).fold(f32::MAX, f32::min);
        1.0 - mark(near, HALF, side)
    })
}

/// The sprayed grain's values: light, half over 0.78, hardly any under 0.45 (as measured from a
/// stock spray grain).
const SPRAYED_SPREAD: [(f32, f32); 9] = [
    (0.3, 0.0),
    (0.45, 0.01),
    (0.53, 0.05),
    (0.58, 0.1),
    (0.67, 0.25),
    (0.78, 0.5),
    (0.86, 0.75),
    (0.95, 0.95),
    (1.0, 1.0),
];

/// The dry patches' values: three quarters of it about black, nearly all the rest light (as
/// measured from a stock grain of ink dried in patches).
const DRY_PATCHES_SPREAD: [(f32, f32); 7] =
    [(0.0, 0.0), (0.0, 0.5), (0.06, 0.75), (0.5, 0.8), (0.91, 0.9), (0.96, 0.95), (1.0, 1.0)];

/// Ink dried in patches: rounded islands of every size with holes in them, its values ranked onto
/// [`DRY_PATCHES_SPREAD`].
fn dry_patches(side: usize) -> Vec<u8> {
    let (n, more) = (side, octaves_more(side));
    let raw: Vec<f32> = (0..n * n)
        .map(|k| {
            let (x, y) = (((k % n) as f32 + 0.5) / n as f32, ((k / n) as f32 + 0.5) / n as f32);
            let (near, _) = cells(x, y, 24, 4211);
            fbm_finer(x * 32.0, y * 32.0, 32, 3, more, 4217) - 0.35 * near
                + 0.1 * value_noise(x * 128.0, y * 128.0, 128, 4219)
        })
        .collect();
    ranked(&raw, &DRY_PATCHES_SPREAD)
}

/// The rough canvas's values: a quarter dark, under 0.14, the rest spread up to the weave's light
/// tops at about 0.8 (as measured from a stock canvas grain).
const ROUGH_CANVAS_SPREAD: [(f32, f32); 10] = [
    (0.0, 0.0),
    (0.03, 0.01),
    (0.07, 0.05),
    (0.09, 0.1),
    (0.14, 0.25),
    (0.32, 0.5),
    (0.75, 0.75),
    (0.81, 0.9),
    (0.84, 0.99),
    (1.0, 1.0),
];

/// A painted canvas: the weave's crossings raised in rows, some threads thicker than others,
/// under a coat dragged on across it that lies dark in patches and catches the light in others,
/// its values ranked onto [`ROUGH_CANVAS_SPREAD`].
fn rough_canvas(side: usize) -> Vec<u8> {
    let (n, more) = (side, octaves_more(side));
    let raw: Vec<f32> = (0..n * n)
        .map(|k| {
            let (x, y) = (((k % n) as f32 + 0.5) / n as f32, ((k / n) as f32 + 0.5) / n as f32);
            let crossings = ((0.5 - 0.5 * (x * TAU * 64.0).cos()) * (0.5 - 0.5 * (y * TAU * 64.0).cos())).sqrt();
            let threads = 0.5 * value_noise_xy(x * 2.0, y * 64.0, (2, 64), 4101)
                + 0.5 * value_noise_xy(x * 64.0, y * 2.0, (64, 2), 4103);
            let coat = fbm_finer(x * 12.0, y * 12.0, 12, 4, more, 4107);
            let dragged = value_noise_xy(x * 4.0, y * 48.0, (4, 48), 4109);
            crossings * (0.4 + 0.6 * threads) + 1.2 * coat + 0.3 * dragged
        })
        .collect();
    ranked(&raw, &ROUGH_CANVAS_SPREAD)
}

/// The charcoal grain's values: most of it from 0.19 to 0.41, a fifth the bright tops of the tooth
/// (as measured from brushes drawn with stock charcoal grains, their stamps building up past it).
const CHARCOAL_SPREAD: [(f32, f32); 9] = [
    (0.0, 0.0),
    (0.188, 0.04),
    (0.225, 0.1),
    (0.262, 0.2),
    (0.3, 0.5),
    (0.338, 0.6),
    (0.413, 0.8),
    (1.0, 0.9),
    (1.0, 1.0),
];

/// The fine noise's values: narrow and dark, from about 0.14 to 0.39 (as measured from a brush
/// drawn with a stock noise grain).
const FINE_NOISE_SPREAD: [(f32, f32); 6] =
    [(0.075, 0.0), (0.141, 0.05), (0.191, 0.25), (0.235, 0.5), (0.279, 0.75), (0.39, 1.0)];

/// The speckled grain's values: a twentieth black, half under 0.09, thinning out up to 0.83. Read
/// between its texels (softened noise, a 230 px tile), it comes out as measured pixel by pixel
/// from a stock speckled grain against how far its stamps build up there (half under 0.095, up
/// to about 0.68), with grain 1.5 px across.
const SPECKLED_SPREAD: [(f32, f32); 15] = [
    (0.0, 0.0),
    (0.003, 0.05),
    (0.012, 0.1),
    (0.03, 0.2),
    (0.048, 0.3),
    (0.068, 0.4),
    (0.09, 0.5),
    (0.117, 0.6),
    (0.147, 0.7),
    (0.195, 0.8),
    (0.292, 0.9),
    (0.395, 0.95),
    (0.506, 0.98),
    (0.655, 0.99),
    (0.825, 1.0),
];

/// The mottled grain's values: mostly light, half over 0.6, from about 0.25 to 0.9 (as a stock
/// grain is applied, typically: the middle of those whose names say nothing of what they are).
const MOTTLED_SPREAD: [(f32, f32); 8] =
    [(0.0, 0.0), (0.25, 0.05), (0.4, 0.2), (0.55, 0.4), (0.65, 0.6), (0.75, 0.8), (0.88, 0.95), (1.0, 1.0)];

/// The paper sheet's values: narrow, nearly all from 0.11 to 0.5, half under 0.29 (a stock paper
/// grain's own values, half as light again, as the stand-ins measured against a drawing pad
/// all come out; narrow, a thin line drawn on it stays smooth).
const SHEET_SPREAD: [(f32, f32); 11] = [
    (0.0, 0.0),
    (0.03, 0.01),
    (0.106, 0.05),
    (0.153, 0.1),
    (0.217, 0.25),
    (0.294, 0.5),
    (0.377, 0.75),
    (0.459, 0.9),
    (0.506, 0.95),
    (0.606, 0.99),
    (0.9, 1.0),
];

/// A paper sheet: fine tooth, each texel its own (as the stock paper's, whose grain is a pixel or
/// two across), its values ranked onto [`SHEET_SPREAD`].
fn sheet(side: usize) -> Vec<u8> {
    ranked(&soft_noise(side, 3001, 0.0), &SHEET_SPREAD)
}

/// The worn paper's values, as [`RECYCLED_SPREAD`]: light, most of it full and hardly any under
/// 0.33 (as measured from a brush pressed into a stock worn paper).
const WORN_SPREAD: [(f32, f32); 8] =
    [(0.15, 0.0), (0.33, 0.02), (0.48, 0.12), (0.96, 0.3), (1.0, 0.41), (1.0, 0.85), (1.0, 0.99), (1.0, 1.0)];

/// Worn paper: mottled and scuffed, its values ranked onto [`WORN_SPREAD`].
fn worn(side: usize) -> Vec<u8> {
    let (n, more) = (side, octaves_more(side));
    let raw: Vec<f32> = (0..n * n)
        .map(|k| {
            let (u, v) = (((k % n) as f32 + 0.5) / n as f32, ((k / n) as f32 + 0.5) / n as f32);
            let mottle = fbm_finer(u * 16.0, v * 16.0, 16, 4, more, 3101);
            let scuffs = value_noise(u * 128.0, v * 32.0, 128, 3109);
            0.6 * mottle + 0.25 * scuffs + 0.15 * value_noise(u * 256.0, v * 256.0, 256, 3119)
        })
        .collect();
    ranked(&raw, &WORN_SPREAD)
}

/// `raw` values ranked onto `spread` ((value, share at or below it), rising): the darkest share
/// of them takes the darkest values, and so on.
/// `v`'s bits as a number that orders as [`f32::total_cmp`] orders the values.
fn total_order(v: f32) -> u32 {
    let bits = v.to_bits();
    if bits >> 31 == 1 { !bits } else { bits | 1 << 31 }
}

fn ranked(raw: &[f32], spread: &[(f32, f32)]) -> Vec<u8> {
    // in order of value, equal ones in order of place (as a stable sort by value)
    // (places as u32: half the memory of an image's worth, and none has 2^32 pixels)
    let mut order: Vec<(u32, u32)> = raw.iter().map(|&v| total_order(v)).zip(0..).collect();
    order.sort_unstable();
    let mut pixels = vec![0u8; raw.len()];
    let count = order.len().max(1) as f32;
    let last = spread.last().map_or(1.0, |s| s.0);
    // the stretch of `spread` the share falls in: the first whose end it doesn't pass (the shares
    // only grow, so it only moves on)
    let mut at = 0;
    for (rank, &(_, k)) in order.iter().enumerate() {
        let share = (rank as f32 + 0.5) / count;
        while spread.get(at + 1).is_some_and(|b| share > b.1) {
            at += 1;
        }
        let value = match (spread.get(at), spread.get(at + 1)) {
            (Some(a), Some(b)) => a.0 + (b.0 - a.0) * (share - a.1) / (b.1 - a.1).max(1e-6),
            _ => last,
        };
        if let Some(p) = pixels.get_mut(k as usize) {
            *p = (value * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    pixels
}

/// The recycled paper's values, as a share of the image at or below each (value, share): about
/// an eighth dark fibre, little in between, the sheet itself light, most of it 0.8 and over.
const RECYCLED_SPREAD: [(f32, f32); 11] = [
    (0.0, 0.0),
    (0.075, 0.02),
    (0.225, 0.06),
    (0.33, 0.13),
    (0.48, 0.16),
    (0.81, 0.33),
    (0.96, 0.41),
    (1.0, 0.52),
    (1.0, 0.82),
    (1.0, 0.99),
    (1.0, 1.0),
];

/// Recycled paper: a light, mottled sheet flecked with small dark rough-edged bits, its values
/// ranked onto [`RECYCLED_SPREAD`].
fn recycled(side: usize) -> Vec<u8> {
    const CELLS: i32 = 32;
    let (n, zoom, more) = (side, zoom_of(side), octaves_more(side));
    // the distance from texel (x, y) to the nearest fleck, in texels at [`TEXTURE_SIZE`]: a short
    // segment, at most one to a cell, wrapping so the image tiles
    let cell = TEXTURE_SIZE as f32 / CELLS as f32;
    let fibre = |x: f32, y: f32| -> f32 {
        let (ci, cj) = ((x / cell).floor() as i32, (y / cell).floor() as i32);
        let mut nearest = f32::MAX;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                let (wi, wj) = (i.rem_euclid(CELLS), j.rem_euclid(CELLS));
                if hash(wi, wj, 2011) > 0.62 {
                    continue;
                }
                let centre = ((i as f32 + hash(wi, wj, 2017)) * cell, (j as f32 + hash(wi, wj, 2027)) * cell);
                let turn = hash(wi, wj, 2029) * std::f32::consts::PI;
                let half = 0.3 + 1.7 * hash(wi, wj, 2039);
                let (dx, dy) = (x - centre.0, y - centre.1);
                let (c, s) = (turn.cos(), turn.sin());
                let along = (dx * c + dy * s).clamp(-half, half);
                let (ex, ey) = (dx - along * c, dy - along * s);
                nearest = nearest.min((ex * ex + ey * ey).sqrt());
            }
        }
        nearest
    };
    let raw: Vec<f32> = (0..n * n)
        .map(|k| {
            let (u, v) = (((k % n) as f32 + 0.5) / n as f32, ((k / n) as f32 + 0.5) / n as f32);
            let (x, y) = (u * TEXTURE_SIZE as f32, v * TEXTURE_SIZE as f32);
            let sheet = 0.6 * fbm_finer(u * 16.0, v * 16.0, 16, 4, more, 2003)
                + 0.4 * value_noise(u * 128.0, v * 128.0, 128, 2053);
            let rough = 0.8 + 0.9 * value_noise(u * 256.0, v * 256.0, 256, 2063);
            sheet - 1.5 * edge(1.0 - fibre(x, y) / rough, zoom)
        })
        .collect();
    ranked(&raw, &RECYCLED_SPREAD)
}

/// An image `n` pixels square from `f(x, y)` with x, y in 0..1 at pixel centres and values clamped
/// to 0..1. Bands of rows are made side by side, one to a core.
fn field(n: usize, f: impl Fn(f32, f32) -> f32 + Sync) -> Vec<u8> {
    let mut out = vec![0u8; n * n];
    let rows = n.div_ceil(crate::par::cores());
    std::thread::scope(|s| {
        for (band, pixels) in out.chunks_mut(rows * n).enumerate() {
            let f = &f;
            crate::par::beside(s, move || {
                for (k, px) in pixels.iter_mut().enumerate() {
                    let (i, j) = (k % n, band * rows + k / n);
                    let v = f((i as f32 + 0.5) / n as f32, (j as f32 + 0.5) / n as f32);
                    *px = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            });
        }
    });
    out
}

/// White noise `n` texels square softened over each texel's neighbours by `side` (`side`, 1,
/// `side` each way, wrapping round, so it tiles).
fn soft_noise(n: usize, seed: u32, side: f32) -> Vec<f32> {
    let n = n as i32;
    let at = |i: i32, j: i32| hash(i.rem_euclid(n), j.rem_euclid(n), seed);
    let weight = |d: i32| if d == 0 { 1.0 } else { side };
    (0..n * n)
        .map(|k| {
            let (i, j) = (k % n, k / n);
            let mut v = 0.0;
            for dj in -1..=1 {
                for di in -1..=1 {
                    v += weight(di) * weight(dj) * at(i + di, j + dj);
                }
            }
            v
        })
        .collect()
}

/// Distance from the centre, 1 at the inscribed circle.
fn radius(x: f32, y: f32) -> f32 {
    ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 2.0
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// [`smooth`] of `t`, an edge's ramp at [`TEXTURE_SIZE`], made `zoom` times narrower about its
/// middle (see [`zoom_of`]): where it was, as many texels wide at every side.
fn edge(t: f32, zoom: f32) -> f32 {
    if zoom == 1.0 { smooth(t) } else { smooth((t - 0.5) * zoom + 0.5) }
}

fn contrast(v: f32, k: f32) -> f32 {
    ((v - 0.5) * k + 0.5).clamp(0.0, 1.0)
}

/// A hash of a lattice point to 0..1.
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h =
        (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 0x00ff_ffff as f32
}

/// Value noise on a lattice that repeats every `period` cells, so the image tiles.
fn value_noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    value_noise_xy(x, y, (period, period), seed)
}

/// Value noise on a lattice that repeats every `period.0` cells across and `period.1` down.
fn value_noise_xy(x: f32, y: f32, period: (i32, i32), seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (smooth(x - xi as f32), smooth(y - yi as f32));
    let (px, py) = (period.0.max(1), period.1.max(1));
    let at = |i: i32, j: i32| hash(i.rem_euclid(px), j.rem_euclid(py), seed);
    let top = at(xi, yi) + (at(xi + 1, yi) - at(xi, yi)) * fx;
    let bottom = at(xi, yi + 1) + (at(xi + 1, yi + 1) - at(xi, yi + 1)) * fx;
    top + (bottom - top) * fy
}

/// Fractal value noise: `octaves` layers, each at twice the frequency (and period) of the last.
fn fbm(x: f32, y: f32, period: i32, octaves: u32, seed: u32) -> f32 {
    fbm_xy(x, y, (period, period), octaves, seed)
}

/// [`fbm`] with `more` octaves of finer detail on it (see [`octaves_more`]), each as much weaker
/// again and about none on average, so that it is the same noise, as bright and as strong, finer.
fn fbm_finer(x: f32, y: f32, period: i32, octaves: u32, more: u32, seed: u32) -> f32 {
    fbm_xy_finer(x, y, (period, period), octaves, more, seed)
}

/// [`fbm`] repeating every `period.0` cells across and `period.1` down.
fn fbm_xy(x: f32, y: f32, period: (i32, i32), octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut f) = (0.0, 0.5, 0.0, 1.0f32);
    for o in 0..octaves {
        let p = (period.0 * f as i32, period.1 * f as i32);
        sum += amp * value_noise_xy(x * f, y * f, p, seed.wrapping_add(o * 7919));
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

/// [`fbm_xy`] with `more` octaves of finer detail on it (see [`fbm_finer`]).
fn fbm_xy_finer(x: f32, y: f32, period: (i32, i32), octaves: u32, more: u32, seed: u32) -> f32 {
    let base = fbm_xy(x, y, period, octaves, seed);
    if more == 0 {
        return base;
    }
    // the base's octaves' weights, by which the finer ones are taken as they would have been
    let norm: f32 = (0..octaves).map(|o| 0.5f32.powi(o as i32 + 1)).sum();
    let mut detail = 0.0;
    for o in octaves..octaves + more {
        let f = (1u32 << o) as f32;
        let p = (period.0 * f as i32, period.1 * f as i32);
        let noise = value_noise_xy(x * f, y * f, p, seed.wrapping_add(o * 7919));
        detail += 0.5f32.powi(o as i32 + 1) * (noise - 0.5);
    }
    base + detail / norm.max(1e-6)
}

/// A heart's outline (in −1..1, point down), from the usual sum of cosines.
fn heart() -> Vec<(f32, f32)> {
    (0..96)
        .map(|i| {
            let t = i as f32 / 96.0 * TAU;
            let x = 16.0 * t.sin().powi(3);
            let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
            (x / 19.0, -(y + 2.5) / 19.0)
        })
        .collect()
}

/// A line drawn in a shape: its two ends and its half width.
type Line = ((f32, f32), (f32, f32), f32);

/// `x`, `y` (0..1) as −1..1 from the centre.
fn centred(x: f32, y: f32) -> (f32, f32) {
    ((x - 0.5) * 2.0, (y - 0.5) * 2.0)
}

/// The distance from (`x`, `y`) to the segment `a`–`b`, and how far along it (0..1) the nearest
/// point is.
fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = (dx * dx + dy * dy).max(1e-12);
    let t = (((x - a.0) * dx + (y - a.1) * dy) / len2).clamp(0.0, 1.0);
    (((x - a.0 - dx * t).powi(2) + (y - a.1 - dy * t).powi(2)).sqrt(), t)
}

/// The distance from (`x`, `y`) to the edge of the polygon through `corners`, and whether the
/// point is inside it.
fn polygon(corners: &[(f32, f32)], x: f32, y: f32) -> (f32, bool) {
    let mut edge = f32::MAX;
    let mut inside = false;
    for (i, &a) in corners.iter().enumerate() {
        let b = corners.get((i + 1) % corners.len()).copied().unwrap_or(a);
        edge = edge.min(segment(x, y, a, b).0);
        if (a.1 > y) != (b.1 > y) && x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    (edge, inside)
}

/// The distances to the nearest and second nearest of points scattered one to a cell, `n` cells
/// across, the cells wrapping round so the image tiles (in cell widths).
fn cells(x: f32, y: f32, n: i32, seed: u32) -> (f32, f32) {
    let (px, py) = (x * n as f32, y * n as f32);
    let (ci, cj) = (px.floor() as i32, py.floor() as i32);
    let (mut f1, mut f2) = (f32::MAX, f32::MAX);
    for j in cj - 1..=cj + 1 {
        for i in ci - 1..=ci + 1 {
            let (wi, wj) = (i.rem_euclid(n), j.rem_euclid(n));
            let (ox, oy) = (hash(wi, wj, seed), hash(wi, wj, seed.wrapping_add(1)));
            let d = ((i as f32 + ox - px).powi(2) + (j as f32 + oy - py).powi(2)).sqrt();
            (f1, f2) = if d < f1 { (d, f1) } else { (f1, f2.min(d)) };
        }
    }
    (f1, f2)
}

/// Cracks spreading from the middle (in −1..1): three jagged lines, each with a few side cracks,
/// as segments (start, end, half width).
fn cracks(seed: u32) -> Vec<Line> {
    /// `steps` steps of a jagged line from `from` (its heading wandering), stopping short of the
    /// edge; where it ended up and its heading there.
    fn walk(
        lines: &mut Vec<Line>,
        from: (f32, f32),
        heading: f32,
        steps: i32,
        width: f32,
        k: i32,
        seed: u32,
    ) -> ((f32, f32), f32) {
        let (mut at, mut a) = (from, heading);
        for s in 0..steps {
            a += hash(k, s, seed) - 0.5;
            let next = (at.0 + a.cos() * 0.15, at.1 + a.sin() * 0.15);
            if (next.0 * next.0 + next.1 * next.1).sqrt() > 0.92 {
                break;
            }
            lines.push((at, next, width * (1.0 - s as f32 / steps as f32 * 0.5)));
            at = next;
        }
        (at, a)
    }
    let mut lines = Vec::new();
    let mut branches = Vec::new();
    for k in 0..3 {
        let (mut at, mut heading) = ((0.0, 0.0), (k as f32 + hash(k, 99, seed) * 0.4) / 3.0 * TAU);
        for part in 0..3 {
            (at, heading) = walk(&mut lines, at, heading, 2, 0.03 - part as f32 * 0.007, k * 10 + part, seed);
            branches.push((at, heading + (hash(k, part + 50, seed) - 0.5) * 2.4));
        }
    }
    for (i, (from, heading)) in branches.into_iter().enumerate() {
        walk(&mut lines, from, heading, 3, 0.012, 100 + i as i32, seed);
    }
    lines
}

/// `count` dots (centre x, centre y, radius) inside the image, deterministic for `seed`.
fn scatter_points(seed: u32, count: i32) -> Vec<(f32, f32, f32)> {
    (0..count)
        .map(|i| {
            let a = hash(i, 1, seed) * std::f32::consts::TAU;
            let d = hash(i, 2, seed).sqrt() * SCATTER_REACH;
            (0.5 + a.cos() * d, 0.5 + a.sin() * d, 0.02 + 0.07 * hash(i, 3, seed).powi(2))
        })
        .collect()
}

/// How a grain is made to repeat without seams (the grain editor's Auto repeat).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Repeat {
    /// The image this many times across the tile, 1 to 4.
    pub scale: f32,
    /// Turned by this much, radians.
    pub rotation: f32,
    /// How far in from the tile's edges the image is faded into its shifted copy, 0..1 (1: to
    /// the middle).
    pub overlap: f32,
    /// How sharp that fade is, 0 (soft) to 1 (hard).
    pub hardness: f32,
    /// Fade into the shifted copy turned round, not just shifted.
    pub mirror: bool,
    /// Fade the fine detail over a narrower band than the broad tones.
    pub pyramid: bool,
}

/// `pixels` (a square greyscale image; anything else comes back unchanged) made to tile without
/// seams as `repeat` says, alike at any size: faded at its edges into a copy shifted by half a tile,
/// which runs on across them, then scaled and turned (wrapping round) and faded again.
pub fn repeatable(pixels: &[u8], repeat: Repeat) -> Vec<u8> {
    let n = pixels.len().isqrt();
    if n == 0 || n * n != pixels.len() {
        return pixels.to_vec();
    }
    let img: Vec<f32> = pixels.iter().map(|&v| f32::from(v)).collect();
    let seamless = blend_edges(&img, n, repeat);
    let scale = if repeat.scale.is_finite() { repeat.scale.clamp(1.0, 4.0) } else { 1.0 };
    let rotation = if repeat.rotation.is_finite() { repeat.rotation } else { 0.0 };
    let out = if (scale - 1.0).abs() < 1e-4 && rotation.abs() < 1e-4 {
        seamless
    } else {
        // the seamless image read through the scale and turn, wrapping round, its edges faded again
        let (sin, cos) = rotation.sin_cos();
        let half = n as f32 / 2.0;
        let mut moved = vec![0.0; n * n];
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 + 0.5 - half, y as f32 + 0.5 - half);
                let u = (dx * cos + dy * sin) * scale + half;
                let v = (-dx * sin + dy * cos) * scale + half;
                if let Some(px) = moved.get_mut(y * n + x) {
                    *px = sample_wrapped(&seamless, n, u - 0.5, v - 0.5);
                }
            }
        }
        blend_edges(&moved, n, repeat)
    };
    out.iter().map(|&v| v.round().clamp(0.0, 255.0) as u8).collect()
}

/// `img` faded at its edges into its copy shifted by half a tile (turned round too, with
/// mirror), over a band `overlap` of the way to the middle.
fn blend_edges(img: &[f32], n: usize, repeat: Repeat) -> Vec<f32> {
    let half = n / 2;
    let at = |x: usize, y: usize| img.get(y * n + x).copied().unwrap_or(0.0);
    let other: Vec<f32> = (0..n * n)
        .map(|i| {
            let (x, y) = ((i % n + half) % n, (i / n + half) % n);
            if repeat.mirror { at(n - 1 - x, n - 1 - y) } else { at(x, y) }
        })
        .collect();
    let overlap = if repeat.overlap.is_finite() { repeat.overlap.clamp(0.0, 1.0) } else { 1.0 };
    let width = overlap * half as f32;
    if width < 1.0 {
        return img.to_vec();
    }
    let hardness = if repeat.hardness.is_finite() { repeat.hardness.clamp(0.0, 1.0) } else { 0.0 };
    // how much of the image itself shows at (x, y): none at the edges, all a band in
    let mask = |x: usize, y: usize, band: f32| {
        let d = x.min(n - 1 - x).min(y).min(n - 1 - y) as f32 + 0.5;
        let t = (d / band.max(1.0)).clamp(0.0, 1.0);
        smooth((t - 0.5) * (1.0 + hardness * 8.0) + 0.5)
    };
    let mix = |a: &[f32], b: &[f32], band: f32| -> Vec<f32> {
        (0..n * n)
            .map(|i| {
                let m = mask(i % n, i / n, band);
                let (p, q) = (a.get(i).copied().unwrap_or(0.0), b.get(i).copied().unwrap_or(0.0));
                p * m + q * (1.0 - m)
            })
            .collect()
    };
    if !repeat.pyramid {
        return mix(img, &other, width);
    }
    // broad tones over the whole band, fine detail over a third of it
    let (low_a, low_b) = (wrapped_blur(img, n, n / 32), wrapped_blur(&other, n, n / 32));
    let high = |v: &[f32], low: &[f32]| v.iter().zip(low).map(|(a, b)| a - b).collect::<Vec<f32>>();
    let tones = mix(&low_a, &low_b, width);
    let detail = mix(&high(img, &low_a), &high(&other, &low_b), width / 3.0);
    tones.iter().zip(&detail).map(|(a, b)| a + b).collect()
}

/// A box blur of radius `r` (under `TEXTURE_SIZE`) over the square image, wrapping round its
/// edges: each pixel the sum of the `2r + 1` about it across, in order, over their count, then the
/// same down. The rows (then the columns, turned into rows) are worked side by side.
fn blurred(img: &[f32], r: usize) -> Vec<f32> {
    let n = TEXTURE_SIZE;
    let across = blurred_rows(img, n, r);
    transposed(&blurred_rows(&transposed(&across, n), n, r), n)
}

/// Each row of `img` (`n` square) box-blurred across by radius `r` (at most `n`), wrapping round:
/// each pixel the sum of the `2r + 1` about it, left to right, over their count. Bands of rows are
/// worked side by side.
fn blurred_rows(img: &[f32], n: usize, r: usize) -> Vec<f32> {
    let span = (2 * r + 1) as f32;
    let mut out = vec![0.0; n * n];
    let rows = n.div_ceil(crate::par::cores()).max(1);
    std::thread::scope(|s| {
        for (band, part) in out.chunks_mut(rows * n).enumerate() {
            crate::par::beside(s, move || {
                // a row with `r` of it wrapped round on each side
                let mut wrapped = vec![0.0f32; n + 2 * r];
                for (j, line) in part.chunks_mut(n).enumerate() {
                    let y = band * rows + j;
                    let row = img.get(y * n..(y + 1) * n).unwrap_or_default();
                    for (i, w) in wrapped.iter_mut().enumerate() {
                        *w = row.get((i + n - r) % n.max(1)).copied().unwrap_or(0.0);
                    }
                    for (a, px) in line.iter_mut().enumerate() {
                        let sum: f32 = wrapped.get(a..=a + 2 * r).unwrap_or_default().iter().sum();
                        *px = sum / span;
                    }
                }
            });
        }
    });
    out
}

/// `img` (`n` square) turned about its diagonal: its rows as columns.
fn transposed(img: &[f32], n: usize) -> Vec<f32> {
    (0..n * n).map(|k| img.get((k % n) * n + k / n).copied().unwrap_or(0.0)).collect()
}

/// A box blur of radius `r` (less than `n`) over the `n` square image, wrapping round its edges.
/// The window's sum is carried along each row, so a bigger image's wider radius costs no more.
/// Not [`blurred`], whose exact sums the stock shapes' stand-ins are ranked by.
fn wrapped_blur(img: &[f32], n: usize, r: usize) -> Vec<f32> {
    let mut out = vec![0.0; n * n];
    wrapped_blur_into(img, n, r, &mut out, |px, v| *px = v);
    out
}

/// [`wrapped_blur`] added into `sums` (as big as `img`), with no picture's worth of its own for the
/// blur.
fn wrapped_blur_adding(img: &[f32], n: usize, r: usize, sums: &mut [f32]) {
    wrapped_blur_into(img, n, r, sums, |px, v| *px += v);
}

/// [`wrapped_blur`] of `img`, each pixel of it handed to `put` with its place in `out`.
fn wrapped_blur_into(img: &[f32], n: usize, r: usize, out: &mut [f32], put: impl Fn(&mut f32, f32)) {
    let span = (2 * r + 1) as f64;
    let pass = |src: &[f32], across: bool, out: &mut [f32], put: &dyn Fn(&mut f32, f32)| {
        let at = |a: usize, b: usize| {
            let (x, y) = if across { (a % n, b) } else { (b, a % n) };
            f64::from(src.get(y * n + x).copied().unwrap_or(0.0))
        };
        for b in 0..n {
            let mut sum: f64 = (0..=2 * r).map(|k| at(n - r + k, b)).sum();
            for a in 0..n {
                let (x, y) = if across { (a, b) } else { (b, a) };
                if let Some(px) = out.get_mut(y * n + x) {
                    put(px, (sum / span) as f32);
                }
                sum += at(a + n + r + 1, b) - at(a + n - r, b);
            }
        }
    };
    let mut across = vec![0.0; n * n];
    pass(img, true, &mut across, &|px, v| *px = v);
    pass(&across, false, out, &put);
}

/// The image at (`u`, `v`) (pixel corners at whole numbers), bilinear, wrapping round.
fn sample_wrapped(img: &[f32], n: usize, u: f32, v: f32) -> f32 {
    let (fx, fy) = (u.floor(), v.floor());
    let (tx, ty) = (u - fx, v - fy);
    let wrap = |c: f32| (c as i64).rem_euclid(n as i64) as usize;
    let at = |x: f32, y: f32| img.get(wrap(y) * n + wrap(x)).copied().unwrap_or(0.0);
    let top = at(fx, fy) * (1.0 - tx) + at(fx + 1.0, fy) * tx;
    let bottom = at(fx, fy + 1.0) * (1.0 - tx) + at(fx + 1.0, fy + 1.0) * tx;
    top * (1.0 - ty) + bottom * ty
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The box blur as first written, a pixel at a time: what [`blurred`] must match bit for bit.
    fn blurred_slowly(img: &[f32], r: usize) -> Vec<f32> {
        let n = TEXTURE_SIZE;
        let span = (2 * r + 1) as f32;
        let pass = |src: &[f32], across: bool| -> Vec<f32> {
            let at = |a: usize, b: usize| {
                let (x, y) = if across { (a % n, b) } else { (b, a % n) };
                src[y * n + x]
            };
            let mut out = vec![0.0; n * n];
            for b in 0..n {
                for a in 0..n {
                    let sum: f32 = (0..=2 * r).map(|k| at(a + n - r + k, b)).sum();
                    let (x, y) = if across { (a, b) } else { (b, a) };
                    out[y * n + x] = sum / span;
                }
            }
            out
        };
        pass(&pass(img, true), false)
    }

    /// The ranking as first written (a stable sort, each share's stretch looked for from the
    /// start): what [`ranked`] must match.
    fn ranked_slowly(raw: &[f32], spread: &[(f32, f32)]) -> Vec<u8> {
        let mut order: Vec<usize> = (0..raw.len()).collect();
        order.sort_by(|&a, &b| raw[a].total_cmp(&raw[b]));
        let mut pixels = vec![0u8; raw.len()];
        let count = order.len().max(1) as f32;
        let last = spread.last().map_or(1.0, |s| s.0);
        for (rank, &k) in order.iter().enumerate() {
            let share = (rank as f32 + 0.5) / count;
            let at = spread.windows(2).find(|w| matches!(w, [_, b] if share <= b.1));
            let value = match at {
                Some([a, b]) => a.0 + (b.0 - a.0) * (share - a.1) / (b.1 - a.1).max(1e-6),
                _ => last,
            };
            pixels[k] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        pixels
    }

    /// The Gaussian softening as first written, a pixel at a time on one thread.
    fn gaussian_slowly(values: &[f32], n: usize, sigma: f32) -> Vec<f32> {
        let reach = (sigma * 3.0).ceil() as i32;
        let weights: Vec<f32> = (-reach..=reach).map(|d| (-(d * d) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let total: f32 = weights.iter().sum();
        let pass = |src: &[f32], across: bool| -> Vec<f32> {
            (0..n * n)
                .map(|k| {
                    let (i, j) = ((k % n) as i32, (k / n) as i32);
                    let mut v = 0.0;
                    for (w, d) in weights.iter().zip(-reach..=reach) {
                        let (x, y) =
                            if across { ((i + d).rem_euclid(n as i32), j) } else { (i, (j + d).rem_euclid(n as i32)) };
                        v += w * src[y as usize * n + x as usize];
                    }
                    v / total
                })
                .collect()
        };
        pass(&pass(values, true), false)
    }

    #[test]
    fn there_are_as_many_shapes_and_grains_as_counted() {
        assert_eq!(shapes().len(), SHAPE_COUNT as usize);
        assert_eq!(grains().len(), GRAIN_COUNT as usize);
    }

    #[test]
    fn the_blur_and_ranking_give_what_they_gave_a_pixel_at_a_time() {
        let n = TEXTURE_SIZE;
        let noise: Vec<f32> = (0..n * n).map(|k| hash((k % n) as i32, (k / n) as i32, 91)).collect();
        for sigma in [0.5f32, 2.0, 6.0] {
            let (fast, slow) = (gaussian_wrapped(&noise, n, sigma), gaussian_slowly(&noise, n, sigma));
            assert!(fast.iter().zip(&slow).all(|(a, b)| a.to_bits() == b.to_bits()), "sigma {sigma}");
        }
        for v in [-1.5f32, -0.0, 0.0, 1e-30, 0.25, 7.0] {
            for w in [-1.5f32, -0.0, 0.0, 1e-30, 0.25, 7.0] {
                assert_eq!(total_order(v).cmp(&total_order(w)), v.total_cmp(&w), "{v} {w}");
            }
        }
        let n = TEXTURE_SIZE;
        // noise with runs of equal values, as shapes have
        let img: Vec<f32> =
            (0..n * n).map(|k| (hash((k % n) as i32 / 3, (k / n) as i32, 77) * 8.0).floor() / 8.0).collect();
        for r in [0, 1, 6, 24, 64] {
            let (fast, slow) = (blurred(&img, r), blurred_slowly(&img, r));
            assert!(fast.iter().zip(&slow).all(|(a, b)| a.to_bits() == b.to_bits()), "radius {r}");
        }
        let spreads: [&[(f32, f32)]; 4] = [&SHEET_SPREAD, &RECYCLED_SPREAD, &[(0.2, 0.0)], &[]];
        for spread in spreads {
            assert_eq!(ranked(&img, spread), ranked_slowly(&img, spread));
        }
        let shape = shapes().into_iter().nth(shape::LEAF as usize).unwrap().pixels;
        let tones: Vec<u8> = (0..17).map(|k| (k * 15) as u8).collect();
        let raw: Vec<f32> = shape.iter().map(|&v| f32::from(v) + 0.5).collect();
        let spread: Vec<(f32, f32)> =
            tones.iter().zip(SHAPE_TONE_SHARES).map(|(&t, s)| (f32::from(t) / 255.0, s)).collect();
        assert_eq!(ranked(&raw, &spread), ranked_slowly(&raw, &spread));
    }

    #[test]
    fn a_retoned_grain_takes_the_tones_it_is_given_in_its_own_order() {
        let tones = [10, 40, 60, 90, 140, 190, 220, 235, 250];
        for g in grains() {
            let out = retoned(&g.pixels, &tones, &TONE_SHARES);
            let mut sorted = out.clone();
            sorted.sort_unstable();
            for (share, want) in TONE_SHARES.iter().zip(tones) {
                let got = sorted[((sorted.len() - 1) as f32 * share) as usize];
                assert!(got.abs_diff(want) <= 3, "{}: at {share} {got} for {want}", g.name);
            }
            // a lighter pixel of the pattern stays at least as light
            let (a, b) = (g.pixels.iter().position(|&v| v == 0), g.pixels.iter().position(|&v| v == 255));
            if let (Some(a), Some(b)) = (a, b) {
                assert!(out[a] <= out[b], "{}", g.name);
            }
        }
        assert_eq!(stock_grain_tones("no such grain.png"), None);
    }

    #[test]
    fn grains_features_are_measured_as_the_stock_ones_are() {
        // fine noise has features of about a pixel, bricks of a quarter of the tile
        let fine = grain_feature(grain::FINE_NOISE).unwrap();
        let brick = grain_feature(grain::BRICK).unwrap();
        assert!(fine < 0.01 && brick > 0.1, "{fine} {brick}");
        assert_eq!(grain_feature(10_000), None);
        // a flat tile has none; a checker of 8 pixel squares matches itself half 2 pixels over
        assert_eq!(feature_share(&[7; 64], 8), None);
        let checker: Vec<u8> = (0..64 * 64).map(|i| if (i % 64 / 8 + i / 64 / 8) % 2 == 0 { 0 } else { 255 }).collect();
        let share = feature_share(&checker, 64).unwrap();
        assert!((share - 2.0 / 64.0).abs() < 0.003, "{share}");
        // the stock table: each name once, in order, every feature some size
        let table = crate::stock_tones::STOCK_GRAIN_FEATURES;
        assert!(table.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(table.iter().all(|&(_, v)| v.iter().all(|&v| v > 0)));
        assert_eq!(stock_grain_features("no such grain.png"), None);
        // between levels, by the tile's size
        let levels = [0.001, 0.002, 0.004, 0.008];
        assert_eq!(feature_at_tile(&levels, 4096.0), 0.001);
        assert_eq!(feature_at_tile(&levels, 256.0), 0.004);
        assert!((feature_at_tile(&levels, 362.0) - 0.003).abs() < 2e-4);
        assert_eq!(feature_at_tile(&levels, 16.0), 0.008);
    }

    #[test]
    fn a_retoned_shape_takes_a_round_tips_falloff_from_its_tones() {
        // the soft round moved onto a stock tip's tones keeps its middle the lightest and takes
        // the tones it is given, an empty half to a dim middle
        let tones: [u8; 17] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 20, 30, 45, 60, 75, 90, 110];
        let soft = shapes().into_iter().next().unwrap();
        let out = retoned(&soft.pixels, &tones, &SHAPE_TONE_SHARES);
        let middle = (TEXTURE_SIZE / 2) * TEXTURE_SIZE + TEXTURE_SIZE / 2;
        assert!(out[middle] >= 105 && out[0] == 0, "{} {}", out[middle], out[0]);
        let mut sorted = out.clone();
        sorted.sort_unstable();
        for (share, want) in SHAPE_TONE_SHARES.iter().zip(tones) {
            let got = sorted[((sorted.len() - 1) as f32 * share) as usize];
            assert!(got.abs_diff(want) <= 3, "at {share} {got} for {want}");
        }
        assert_eq!(stock_shape_tones("no such shape.png", false), None);
    }

    #[test]
    fn a_striped_shape_takes_its_rows_share_of_the_ink() {
        // the soft round given bands that hold nothing in the middle eighth and full elsewhere
        // where it has ink: its middle rows emptied, the rows above and below kept
        let soft = shapes().into_iter().next().unwrap();
        let mut rows = [255u8; 64];
        rows[28..36].fill(0);
        let out = striped(&soft.pixels, &rows);
        let n = TEXTURE_SIZE;
        let row = |y: usize| out[y * n..(y + 1) * n].iter().map(|&v| u32::from(v)).sum::<u32>();
        assert_eq!(row(n / 2), 0);
        assert!(row(n / 2 - 40) > 1000 && row(n / 2 + 40) > 1000, "{} {}", row(n / 2 - 40), row(n / 2 + 40));
        assert_eq!(stock_shape_rows("no such shape.png", false), None);
    }

    #[test]
    fn the_grainy_square_takes_the_coarseness_it_is_asked_for() {
        // fine marks repeat over a short distance, coarse ones over a long one; none is smooth
        let n = TEXTURE_SIZE;
        let fine = grainy_square_of(Some(1.5 / n as f32), 0.02);
        let coarse = grainy_square_of(Some(12.0 / n as f32), 0.02);
        let smooth = grainy_square_of(None, 0.02);
        // the share of neighbours across the middle that differ by more than a little
        let busy = |p: &[u8]| {
            let row = n / 2;
            (n / 4..3 * n / 4).filter(|&x| p[row * n + x].abs_diff(p[row * n + x + 1]) > 6).count() as f32
                / (n / 2) as f32
        };
        assert!(busy(&fine) > 3.0 * busy(&coarse), "{} {}", busy(&fine), busy(&coarse));
        assert_eq!(busy(&smooth), 0.0);
        // a fuller tip's texture is weaker: its middle stays above its soft edge
        let full = grainy_square_of(Some(1.5 / n as f32), 0.5);
        let least_middle = |p: &[u8]| (n / 4..3 * n / 4).map(|x| p[n / 2 * n + x]).min().unwrap_or(0);
        assert!(
            least_middle(&full) > 100 && least_middle(&fine) < 40,
            "{} {}",
            least_middle(&full),
            least_middle(&fine)
        );
        // the same one twice is the same
        assert_eq!(grainy_square_of(Some(1.5 / n as f32), 0.02), fine);
        // another shape takes fine marks; none for no texture or a coarse one
        let soft = shapes().into_iter().next().unwrap().pixels;
        let marked = textured_shape(&soft, Some(1.5 / n as f32), 0.02).unwrap();
        assert!(busy(&marked) > busy(&soft) + 0.2, "{} {}", busy(&marked), busy(&soft));
        assert_eq!(textured_shape(&soft, None, 0.02), None);
        assert_eq!(textured_shape(&soft, Some(0.2), 0.02), None);
        assert_eq!(stock_shape_feature("no such shape.png"), None);
    }

    #[test]
    fn the_patterned_grains_draw_their_marks_where_they_should() {
        let all = grains();
        let n = TEXTURE_SIZE;
        let at = |index: u32, x: f32, y: f32| {
            let px = &all[index as usize].pixels;
            px[((y * n as f32) as usize).min(n - 1) * n + ((x * n as f32) as usize).min(n - 1)]
        };
        // graph paper: dark on its lines (the middle one thickest), white inside a square
        assert!(at(grain::GRAPH_PAPER, 1.0 / 6.0, 0.08) < 60 && at(grain::GRAPH_PAPER, 0.08, 0.08) > 250);
        assert!(
            at(grain::GRAPH_PAPER, 0.5 + 0.009, 0.08) < 60 && at(grain::GRAPH_PAPER, 1.0 / 6.0 + 0.009, 0.08) > 200
        );
        // the dot grid: dark in the middle and at the corners, white between
        assert!(at(grain::DOT_GRID, 0.5, 0.5) < 30 && at(grain::DOT_GRID, 0.0, 0.0) < 30);
        assert!(at(grain::DOT_GRID, 0.25, 0.25) > 250);
        // the star: dots along the middle row and the diagonal, none in the very middle
        assert!(at(grain::DOTTED_STAR, 63.9 / 2048.0, 0.5) < 60 && at(grain::DOTTED_STAR, 0.5, 0.5) > 250);
        assert!(at(grain::DOTTED_STAR, 58.0 / 2048.0, 58.0 / 2048.0) < 60 && at(grain::DOTTED_STAR, 0.25, 0.1) > 250);
        assert!(drawn_to_stock_size(grain::DOT_GRID) && !drawn_to_stock_size(grain::PAPER));
        // diagonal stripes: about a seventh white, the same along the diagonal; the fine grid
        // white on its lines; soft dots dark in their middles and light between; the labyrinth
        // a little under half white
        let share =
            |index: u32| all[index as usize].pixels.iter().filter(|&&v| v > 128).count() as f32 / (n * n) as f32;
        assert!((share(grain::DIAGONAL_STRIPES) - 0.15).abs() < 0.03, "{}", share(grain::DIAGONAL_STRIPES));
        assert_eq!(at(grain::DIAGONAL_STRIPES, 0.2, 0.3), at(grain::DIAGONAL_STRIPES, 0.3, 0.2));
        let on_line = [0.0405, 0.0425, 0.0445].map(|x| at(grain::FINE_GRID, x, 0.3));
        assert!(on_line.into_iter().max().unwrap_or(0) > 60 && at(grain::FINE_GRID, 0.0, 0.0) < 20, "{on_line:?}");
        assert!(at(grain::SOFT_DOTS, 0.0, 0.0) < 110 && at(grain::SOFT_DOTS, 0.5 / 14.0, 0.0) > 180);
        // its dots' hard edge: dark a 56th of the tile out, light a 40th out
        assert!(at(grain::SOFT_DOTS, 1.0 / 56.0, 0.0) < 140 && at(grain::SOFT_DOTS, 1.0 / 40.0, 0.0) > 170);
        assert!((share(grain::LABYRINTH) - 0.45).abs() < 0.02, "{}", share(grain::LABYRINTH));
        assert_eq!(drawn_to_stock_width(grain::SOFT_DOTS), Some(1792.0));
    }

    #[test]
    fn a_paper_stands_in_varying_at_each_size_as_much_as_it_does() {
        let (width, octaves) = stock_grain_octaves("Wax-Crayon-Paper.jpg").expect("measured");
        assert_eq!(width, 2048.0);
        let tones = stock_grain_tones("Wax-Crayon-Paper.jpg").expect("measured");
        let pixels = octave_grain(&octaves, &tones);
        let n = TEXTURE_SIZE;
        let a: Vec<f32> = pixels.iter().map(|&v| f32::from(v) / 255.0).collect();
        // how much it varies between softenings of `from` and `to` texels
        let band = |from: f32, to: f32| {
            let (fine, broad) = (gaussian_wrapped(&a, n, from), gaussian_wrapped(&a, n, to));
            let d: Vec<f32> = fine.iter().zip(&broad).map(|(f, b)| f - b).collect();
            let mean = d.iter().sum::<f32>() / d.len() as f32;
            (d.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / d.len() as f32).sqrt()
        };
        // its veins and blotches, 4 to 64 texels: our paper sheet standing in for it had next to
        // none (0.007 and 0.002), the paper as much as its fibres
        assert!(band(4.0, 16.0) > 0.025, "{}", band(4.0, 16.0));
        assert!(band(16.0, 64.0) > 0.015, "{}", band(16.0, 64.0));
        // with the paper's own tones
        let mean = a.iter().sum::<f32>() / a.len() as f32;
        assert!((mean - 0.73).abs() < 0.02, "{mean}");
        // only papers and washes stand in so
        assert!(drawn_by_octaves(grain::SHEET) && drawn_by_octaves(grain::WASH));
        assert!(!drawn_by_octaves(grain::CHARCOAL));
    }

    #[test]
    fn a_stand_in_is_stretched_to_the_box_its_stock_tip_inks() {
        // ours a square across the middle fifth to four fifths...
        let n = TEXTURE_SIZE;
        let inside = |v: usize, from: usize, to: usize| v >= from * n / 20 && v < to * n / 20;
        let square: Vec<u8> =
            (0..n * n).map(|k| if inside(k % n, 6, 14) && inside(k / n, 6, 14) { 255 } else { 0 }).collect();
        // ...the stock tip's ink wide (from 10 to 54 of 64 bands across) and as tall but lower (26
        // to 52 down)
        let bands =
            |from: usize, to: usize| std::array::from_fn::<u8, 64, _>(|b| if b >= from && b < to { 255 } else { 0 });
        let (rows, cols) = (bands(26, 52), bands(10, 54));
        let fitted = fitted_to_box(&square, &rows, &cols).expect("stretched");
        let span = |along: bool| {
            let inked = |i: usize| (0..n).any(|j| fitted[if along { j * n + i } else { i * n + j }] > 127);
            let first = (0..n).find(|&i| inked(i)).unwrap();
            let last = (0..n).rev().find(|&i| inked(i)).unwrap();
            (first, last)
        };
        let ((x0, x1), (y0, y1)) = (span(true), span(false));
        assert!(x0.abs_diff(40) <= 2 && x1.abs_diff(215) <= 2, "{x0}..{x1}");
        assert!(y0.abs_diff(104) <= 2 && y1.abs_diff(207) <= 2, "{y0}..{y1}");
        // one as big as its tip already, where it lies, is left alone, and one bigger than its tip
        // isn't shrunk
        assert!(fitted_to_box(&square, &bands(19, 45), &bands(19, 45)).is_none());
        assert!(fitted_to_box(&square, &bands(24, 40), &bands(24, 40)).is_none());
    }

    #[test]
    fn a_stock_shape_s_bands_and_tones_turn_round_for_a_brush_that_inverts_it() {
        // a shape dark on white: inverted, its dark band is where its ink is
        let mut means = [250u8; 64];
        means[30] = 10;
        let plain = band_shares(&means, false);
        let inverted = band_shares(&means, true);
        assert!(plain[30] < 20 && plain[0] == 255, "{} {}", plain[30], plain[0]);
        assert!(inverted[30] == 255 && inverted[0] < 10, "{} {}", inverted[30], inverted[0]);
        // the columns too: a shape whose ink lies only in its middle columns keeps only those
        let soft = shapes().into_iter().next().unwrap();
        let mut cols = [0u8; 64];
        cols[28..36].fill(255);
        let rows = [255u8; 64];
        let out = shaped(&soft.pixels, &rows, &cols);
        let n = TEXTURE_SIZE;
        let column = |x: usize| (0..n).map(|y| u32::from(out[y * n + x])).sum::<u32>();
        assert!(column(n / 2) > 1000 && column(n / 2 - 60) == 0, "{} {}", column(n / 2), column(n / 2 - 60));
    }

    #[test]
    fn a_retoned_solid_shape_shrinks_at_its_rim_without_holes_or_specks() {
        // a solid disc over most of the square, moved onto tones that leave four fifths empty
        let n = TEXTURE_SIZE;
        let disc: Vec<u8> = (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as f32 / n as f32, (i / n) as f32 / n as f32);
                if radius(x, y) < 0.9 { 255 } else { 0 }
            })
            .collect();
        let mut tones = [0u8; 17];
        tones[14..].copy_from_slice(&[255, 255, 255]);
        let out = retoned_shape(&disc, &tones, &SHAPE_TONE_SHARES);
        let at = |x: f32, y: f32| out[(y * n as f32) as usize * n + (x * n as f32) as usize];
        // whole in the middle, empty well inside the old rim and outside it
        for r in [0.0f32, 0.1, 0.2] {
            assert_eq!(at(0.5 + r / 2.0, 0.5), 255, "{r}");
        }
        for r in [0.7f32, 0.85, 0.95] {
            assert_eq!(at(0.5 + r / 2.0, 0.5), 0, "{r}");
        }
    }

    #[test]
    fn images_have_the_right_size_and_content() {
        for img in shapes().into_iter().chain(grains()) {
            assert_eq!(img.pixels.len(), TEXTURE_SIZE * TEXTURE_SIZE, "{}", img.name);
            let max = *img.pixels.iter().max().unwrap();
            let min = *img.pixels.iter().min().unwrap();
            // (a stand-in spread as measured may stay dark, as the fine noise does)
            assert!(max >= min + 48, "{} has contrast", img.name);
        }
    }

    #[test]
    fn shapes_are_empty_at_the_corners() {
        for img in shapes() {
            assert!(img.pixels[0] < 8, "{} corner {}", img.name, img.pixels[0]);
        }
    }

    #[test]
    fn grains_tile() {
        // the left column continues from the right one as any column does from its neighbour
        for img in grains() {
            let n = TEXTURE_SIZE;
            let step = |a: usize, b: usize| -> u32 {
                (0..n).map(|j| u32::from(img.pixels[j * n + a].abs_diff(img.pixels[j * n + b]))).sum::<u32>() / n as u32
            };
            let inside = (1..n).map(|i| step(i - 1, i)).sum::<u32>() / (n as u32 - 1);
            let seam = step(n - 1, 0);
            assert!(seam <= inside * 3 / 2 + 6, "{} seam {seam}, inside {inside}", img.name);
        }
    }

    /// Writes every shape and grain as a PGM into the directory in `PROCRAFT_DUMP`, for looking at
    /// them: `cargo test -p procraft-engine dump_textures -- --ignored`.
    #[test]
    #[ignore]
    fn dump_textures() {
        let Ok(dir) = std::env::var("PROCRAFT_DUMP") else { return };
        for (kind, images) in [("shape", shapes()), ("grain", grains())] {
            for (i, img) in images.iter().enumerate() {
                let mut pgm = format!(
                    "P5 {TEXTURE_SIZE} {TEXTURE_SIZE} 255
"
                )
                .into_bytes();
                pgm.extend(&img.pixels);
                std::fs::write(format!("{dir}/{kind}-{i:02}-{}.pgm", img.name.replace(' ', "_")), pgm).unwrap();
            }
        }
    }

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(shapes(), shapes());
        assert_eq!(grains(), grains());
    }

    #[test]
    fn a_repeated_grain_has_no_seams() {
        let n = TEXTURE_SIZE;
        // a ramp: a hard edge where it wraps round
        let ramp: Vec<u8> = (0..n * n).map(|i| (i % n * 255 / (n - 1)) as u8).collect();
        let jump =
            |img: &[u8]| (0..n).map(|y| (i32::from(img[y * n]) - i32::from(img[y * n + n - 1])).abs()).max().unwrap();
        assert!(jump(&ramp) > 200);
        let base = Repeat { scale: 1.0, rotation: 0.0, overlap: 1.0, hardness: 0.0, mirror: false, pyramid: false };
        for r in [
            base,
            Repeat { mirror: true, ..base },
            Repeat { pyramid: true, ..base },
            Repeat { scale: 2.0, rotation: 0.6, ..base },
        ] {
            let out = repeatable(&ramp, r);
            assert!(jump(&out) <= 6, "{r:?}: {}", jump(&out));
        }
        // no overlap: left as it was
        assert_eq!(repeatable(&ramp, Repeat { overlap: 0.0, ..base }), ramp);
        assert_eq!(repeatable(&[1, 2, 3], base), vec![1, 2, 3]);
    }

    #[test]
    fn a_bigger_image_repeats_as_the_smaller_one_does() {
        let (small, big) = (TEXTURE_SIZE, TEXTURE_SIZE * 4);
        let base = Repeat { scale: 1.0, rotation: 0.0, overlap: 1.0, hardness: 0.0, mirror: false, pyramid: false };
        let settings = [
            base,
            Repeat { overlap: 0.3, hardness: 0.5, ..base },
            Repeat { pyramid: true, ..base },
            Repeat { mirror: true, ..base },
            Repeat { scale: 2.0, rotation: 0.6, ..base },
        ];
        // a ramp: a hard edge where it wraps round, gone at any size
        let ramp: Vec<u8> = (0..big * big).map(|i| (i % big * 255 / (big - 1)) as u8).collect();
        // broad tones and fine detail, the same at any size. Odd across and even down, so a copy
        // shifted half a tile both ways is its opposite: all the fading shows, and where the
        // pyramid parts the two
        let waves = |n: usize| -> Vec<u8> {
            let turn = std::f32::consts::TAU / n as f32;
            let at = |i: usize| {
                let (x, y) = (((i % n) as f32 + 0.5) * turn, ((i / n) as f32 + 0.5) * turn);
                0.6 * (x * 3.0).sin() * (y * 2.0).cos() + 0.4 * (x * 17.0).sin() * (y * 16.0).sin()
            };
            (0..n * n).map(|i| (128.0 + 100.0 * at(i)).round() as u8).collect()
        };
        for r in settings {
            let out = repeatable(&ramp, r);
            assert_eq!(out.len(), big * big, "{r:?}");
            let jump = (0..big).map(|y| (i32::from(out[y * big]) - i32::from(out[y * big + big - 1])).abs()).max();
            assert!(jump.unwrap() <= 6, "{r:?}: {jump:?}");
            // the same picture: each small pixel against the big ones it covers
            let (a, b) = (repeatable(&waves(small), r), repeatable(&waves(big), r));
            let worst = (0..small * small)
                .map(|i| {
                    let (x, y) = (i % small * 4, i / small * 4);
                    let sum: i32 = (0..16).map(|k| i32::from(b[(y + k / 4) * big + x + k % 4])).sum();
                    (sum / 16 - i32::from(a[i])).abs()
                })
                .max();
            assert!(worst.unwrap() <= 6, "{r:?}: {worst:?}");
        }
    }

    #[test]
    fn the_blur_is_a_box_that_wraps_round() {
        let (n, r) = (24, 3);
        let img: Vec<f32> = (0..n * n).map(|i| ((i * 37) % 101) as f32).collect();
        let at = |x: usize, y: usize| img[(y % n) * n + x % n];
        let out = wrapped_blur(&img, n, r);
        for y in 0..n {
            for x in 0..n {
                let window = (0..=2 * r).flat_map(|j| (0..=2 * r).map(move |i| (i, j)));
                let sum: f32 = window.map(|(i, j)| at(x + n + i - r, y + n + j - r)).sum();
                let want = sum / ((2 * r + 1) * (2 * r + 1)) as f32;
                assert!((out[y * n + x] - want).abs() < 1e-2, "({x}, {y}): {} against {want}", out[y * n + x]);
            }
        }
    }

    /// Each shape and grain drawn alone at the size brushes hold is the one in the set (one drawn
    /// from another drawn from it again, the same), byte for byte.
    #[test]
    fn each_drawn_alone_is_the_one_in_the_set() {
        for (i, s) in (0..).zip(shapes()) {
            assert_eq!(draw_shape(i, TEXTURE_SIZE).as_deref(), Some(&s.pixels[..]), "{}", s.name);
            assert_eq!(shape_at(i, TEXTURE_SIZE).as_deref(), Some(&s.pixels[..]), "{}", s.name);
        }
        for (i, g) in (0..).zip(grains()) {
            assert_eq!(draw_grain(i, TEXTURE_SIZE).as_deref(), Some(&g.pixels[..]), "{}", g.name);
        }
        assert!(draw_shape(SHAPE_COUNT, TEXTURE_SIZE).is_none() && draw_grain(GRAIN_COUNT, TEXTURE_SIZE).is_none());
        // only powers of two from the size brushes hold to the sharpest
        assert!(shape_at(shape::BLOCK, 300).is_none() && shape_at(shape::BLOCK, 2 * SHARPEST).is_none());
        assert!(grain_at(grain::GRUNGE, 128).is_none());
    }

    /// `pixels` (`side` square) averaged down to [`TEXTURE_SIZE`].
    fn averaged_down(pixels: &[u8], side: usize) -> Vec<f32> {
        let k = side / TEXTURE_SIZE;
        let n = TEXTURE_SIZE;
        (0..n * n)
            .map(|i| {
                let (x, y) = (i % n * k, i / n * k);
                let sum: u32 = (0..k * k).map(|j| u32::from(pixels[(y + j / k) * side + x + j % k])).sum();
                sum as f32 / (k * k) as f32
            })
            .collect()
    }

    /// `pixels` ([`TEXTURE_SIZE`] square) read between its texels at `side` a side, as the GPU
    /// magnifies it (bilinear; wrapping round for a grain, held at the edge for a shape).
    fn magnified(pixels: &[u8], side: usize, wrap: bool) -> Vec<f32> {
        let n = TEXTURE_SIZE as i64;
        let at = |x: i64, y: i64| {
            let (x, y) = if wrap { (x.rem_euclid(n), y.rem_euclid(n)) } else { (x.clamp(0, n - 1), y.clamp(0, n - 1)) };
            f32::from(pixels[(y * n + x) as usize])
        };
        (0..side * side)
            .map(|i| {
                let place = |c: usize| (c as f32 + 0.5) * TEXTURE_SIZE as f32 / side as f32 - 0.5;
                let (u, v) = (place(i % side), place(i / side));
                let (x, y) = (u.floor() as i64, v.floor() as i64);
                let (fx, fy) = (u - u.floor(), v - v.floor());
                let top = at(x, y) * (1.0 - fx) + at(x + 1, y) * fx;
                let bottom = at(x, y + 1) * (1.0 - fx) + at(x + 1, y + 1) * fx;
                top * (1.0 - fy) + bottom * fy
            })
            .collect()
    }

    /// How steep an image `side` square is from each texel to the next, across and down: the mean of
    /// each step squared (an edge half as wide counts twice).
    fn steepness(values: &[f32], side: usize) -> f64 {
        let mut sum = 0.0f64;
        for y in 0..side - 1 {
            for x in 0..side - 1 {
                let v = values[y * side + x];
                sum += f64::from((values[y * side + x + 1] - v).powi(2) + (values[(y + 1) * side + x] - v).powi(2));
            }
        }
        sum / ((side - 1) * (side - 1)) as f64
    }

    /// `values` ([`TEXTURE_SIZE`] square) each the mean of the 3 x 3 about it, wrapping round: an
    /// edge's place, not each texel's share of it.
    fn softened(values: &[f32]) -> Vec<f32> {
        let n = TEXTURE_SIZE as i64;
        let at = |x: i64, y: i64| values[(y.rem_euclid(n) * n + x.rem_euclid(n)) as usize];
        (0..n * n).map(|i| (0..9).map(|k| at(i % n + k % 3 - 1, i / n + k / 3 - 1)).sum::<f32>() / 9.0).collect()
    }

    /// How alike two images of the same size are: their correlation (1 the same, up to brightness
    /// and contrast).
    fn alike(a: &[f32], b: &[f32]) -> f32 {
        let mean = |v: &[f32]| v.iter().map(|&x| f64::from(x)).sum::<f64>() / v.len() as f64;
        let (ma, mb) = (mean(a), mean(b));
        let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
        for (&x, &y) in a.iter().zip(b) {
            let (x, y) = (f64::from(x) - ma, f64::from(y) - mb);
            (ab, aa, bb) = (ab + x * y, aa + x * x, bb + y * y);
        }
        (ab / (aa * bb).sqrt().max(1e-9)) as f32
    }

    /// Drawn twice as big, each shape and grain that shows more so is the same picture (averaged
    /// back down, alike with the one brushes hold, each edge's place as it was, and as bright) and
    /// sharper than that one magnified (steeper from texel to texel). One soft all through drawn
    /// bigger is next to the same as it magnified, which is why it isn't; a grain of noise a texel
    /// across would be other noise, as fine in texels, so isn't either.
    #[test]
    fn drawn_bigger_each_is_the_same_picture_sharper() {
        let side = 2 * TEXTURE_SIZE;
        let noise = [grain::NOISE, grain::SHEET, grain::FINE_NOISE, grain::SPECKLED, grain::MOTTLED, grain::SPRAYED];
        let mut least_alike = (f32::MAX, "");
        let mut brightest = (0.0f32, "");
        let mut least_sharper = (f64::MAX, "");
        let mut soft_most = (0.0f32, "");
        for (kind, count) in [("shape", SHAPE_COUNT), ("grain", GRAIN_COUNT)] {
            for i in 0..count {
                let (small, big, name, sharpens) = if kind == "shape" {
                    let s = shape_image(i).unwrap();
                    (s.pixels, draw_shape(i, side).unwrap(), s.name, shape_sharpens(i))
                } else {
                    let g = grain_image(i).unwrap();
                    (g.pixels, draw_grain(i, side).unwrap(), g.name, grain_sharpens(i))
                };
                let small_f: Vec<f32> = small.iter().map(|&v| f32::from(v)).collect();
                let (down, small_soft) = (softened(&averaged_down(&big, side)), softened(&small_f));
                let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
                let (same, brighter) = (alike(&down, &small_soft), (mean(&down) - mean(&small_soft)).abs());
                let magnified = magnified(&small, side, kind == "grain");
                let big_f: Vec<f32> = big.iter().map(|&v| f32::from(v)).collect();
                let sharper = steepness(&big_f, side) / steepness(&magnified, side).max(1e-6);
                let apart = big_f.iter().zip(&magnified).map(|(a, b)| (a - b).abs()).sum::<f32>() / big_f.len() as f32;
                if sharpens {
                    if same < least_alike.0 {
                        least_alike = (same, name);
                    }
                    if brighter > brightest.0 {
                        brightest = (brighter, name);
                    }
                    if sharper < least_sharper.0 {
                        least_sharper = (sharper, name);
                    }
                } else if !(kind == "grain" && noise.contains(&i)) && apart > soft_most.0 {
                    soft_most = (apart, name);
                }
            }
        }
        eprintln!("drawn at {side}: averaged down, the least alike {least_alike:?}, the most brightened {brightest:?}");
        eprintln!("the least sharper {least_sharper:?} times as steep; the soft at most {soft_most:?} levels off");
        assert!(least_alike.0 > 0.95, "{least_alike:?}");
        assert!(brightest.0 < 4.0, "{brightest:?}");
        assert!(least_sharper.0 > 1.0, "{least_sharper:?}");
        assert!(soft_most.0 < 1.0, "{soft_most:?}");
    }

    /// The block's edges are as many texels wide at every side, where they were: drawn four times
    /// as big, its edge spans a quarter as much of it.
    #[test]
    fn the_block_s_edge_is_as_many_texels_wide_at_every_side() {
        // where the rows through its middle first pass a tenth, a half and nine tenths of full, as a
        // share of the image (the median over the rows: a speck at the edge holds one back)
        let crossings = |side: usize| {
            let pixels = shape_at(shape::BLOCK, side).unwrap();
            [0.1f32, 0.5, 0.9].map(|level| {
                let mut at: Vec<f32> = (side * 3 / 8..side * 5 / 8)
                    .map(|y| {
                        let row = &pixels[y * side..(y + 1) * side];
                        let x = row.iter().position(|&v| f32::from(v) / 255.0 >= level).unwrap();
                        let (before, now) = (f32::from(row[x - 1]) / 255.0, f32::from(row[x]) / 255.0);
                        (x as f32 - 0.5 + (level - before) / (now - before).max(1e-6)) / side as f32
                    })
                    .collect();
                at.sort_by(f32::total_cmp);
                at[at.len() / 2]
            })
        };
        let (small, big) = (crossings(TEXTURE_SIZE), crossings(4 * TEXTURE_SIZE));
        let width = |c: [f32; 3]| (c[2] - c[0]) * TEXTURE_SIZE as f32;
        eprintln!("the block's edge: {:.2} texels of 256 as held, {:.2} drawn at 1024", width(small), width(big));
        assert!((width(big) * 4.0 - width(small)).abs() < 0.5, "{} against {}", width(big), width(small));
        assert!((big[1] - small[1]).abs() < 0.5 / TEXTURE_SIZE as f32, "{big:?} against {small:?}");
    }

    /// Read between its texels at its own side an image is as it was; bigger, a smooth one keeps
    /// its values and a constant one stays so.
    #[test]
    fn read_between_texels_an_image_keeps_its_values() {
        let n = 16;
        let wave: Vec<f32> = (0..n * n).map(|i| ((i % n) as f32 * TAU / n as f32).sin()).collect();
        assert_eq!(upsampled(&wave, n, n), wave);
        let big = upsampled(&wave, n, 4 * n);
        for (i, v) in big.iter().enumerate() {
            let x = ((i % (4 * n)) as f32 + 0.5) / 4.0 - 0.5;
            assert!((v - (x * TAU / n as f32).sin()).abs() < 0.02, "{i}: {v}");
        }
        assert!(upsampled(&vec![0.25; n * n], n, 2 * n).iter().all(|&v| (v - 0.25).abs() < 1e-6));
    }

    /// Fine marks (under a texel and a half at the size brushes hold) are drawn as fine bigger: as
    /// big a share of the image, from texel to texel no blurrier; coarse ones are the same marks.
    #[test]
    fn marks_are_as_big_a_share_drawn_bigger() {
        let share = |values: &[f32], side: usize| {
            let pixels: Vec<u8> = values.iter().map(|&v| (v * 255.0).round() as u8).collect();
            feature_share(&pixels, side).unwrap()
        };
        for half in [2u32, 6, 24] {
            let (small, big) = (marks(half), marks_at(half, 1024));
            let (a, b) = (share(&small, TEXTURE_SIZE), share(&big, 1024));
            eprintln!("marks of {half} quarter texels: {a:.4} of the side at 256, {b:.4} at 1024");
            assert!(b <= a * 1.25 && b >= a * 0.6, "{half}: {b} against {a}");
        }
        // coarse marks are those of the size brushes hold, read between texels
        assert_eq!(marks_at(24, 1024), upsampled(&marks(24), TEXTURE_SIZE, 1024));
    }

    /// [`retoned_shape`] as first written (each blur and what it ranks by a picture's worth of its
    /// own): what it must match bit for bit.
    fn retoned_shape_as_first(pixels: &[u8], tones: &[u8], shares: &[f32]) -> Vec<u8> {
        let n = pixels.len().isqrt();
        let img: Vec<f32> = pixels.iter().map(|&v| f32::from(v) / 255.0).collect();
        let mut deep = vec![0.0f32; pixels.len()];
        for r in [6, 24, 64] {
            let depth = match n {
                TEXTURE_SIZE => blurred(&img, r),
                _ => wrapped_blur(&img, n, (r * n / TEXTURE_SIZE).min(n.saturating_sub(1) / 2)),
            };
            deep.iter_mut().zip(&depth).for_each(|(sum, d)| *sum += d);
        }
        let raw: Vec<f32> =
            pixels.iter().zip(&deep).map(|(&v, &sum)| f32::from(v) + (sum / 3.0).clamp(0.0, 1.0) * 0.999).collect();
        let spread: Vec<(f32, f32)> = tones.iter().zip(shares).map(|(&t, &s)| (f32::from(t) / 255.0, s)).collect();
        ranked(&raw, &spread)
    }

    /// A shape retoned with its blurs added in place is the one retoned as first written, pixel for
    /// pixel, at the size brushes hold and big.
    #[test]
    fn a_shape_retoned_in_less_memory_is_the_same() {
        let tones = [0, 0, 0, 0, 0, 0, 3, 17, 73, 188, 253, 255, 255, 255, 255, 255, 255];
        for (index, side) in [(shape::LEAF, TEXTURE_SIZE), (shape::BLOCK, 512), (shape::STAR, 1024)] {
            let pixels = shape_at(index, side).unwrap();
            let now = retoned_shape(&pixels, &tones, &SHAPE_TONE_SHARES);
            assert!(now == retoned_shape_as_first(&pixels, &tones, &SHAPE_TONE_SHARES), "{index} at {side}");
        }
    }

    /// Each of 64 bands of `pixels`' rows' mean value (a `side` square), as the stock tables measure
    /// a stock shape's at 256 square.
    fn row_bands(pixels: &[u8], side: usize) -> Vec<i32> {
        let per = side / 64;
        pixels
            .chunks(per * side)
            .map(|band| (band.iter().map(|&v| f64::from(v)).sum::<f64>() / band.len() as f64).round() as i32)
            .collect()
    }

    /// The hard round tip's picture and the small point's hold the ink their stock tips do, band by
    /// band down their rows and in all: as big, and their edges as soft, as those are. At 256, as
    /// the stock tips were measured, and the hard round's big picture averaged down to that.
    #[test]
    fn the_hard_round_and_the_point_hold_their_stock_tips_ink() {
        // (the stock tips' keys: the hard round and the small point)
        let pictures = [
            (0xe114_279e_198b_3aec_u64, hard_round_at(TEXTURE_SIZE), 6),
            (0x79d5_ed1a_9f14_cee3, point_at(TEXTURE_SIZE), 2),
        ];
        for (key, pixels, most_apart) in pictures {
            let stock: Vec<i32> =
                stock_tones(crate::stock_tones::STOCK_SHAPE_ROWS, key).unwrap().map(i32::from).to_vec();
            let ours = row_bands(&pixels, TEXTURE_SIZE);
            let apart = ours.iter().zip(&stock).map(|(a, b)| (a - b).abs()).max().unwrap();
            let ink = |bands: &[i32]| f64::from(bands.iter().sum::<i32>()) / 64.0 / 255.0;
            eprintln!(
                "{key:016x}: bands at most {apart} levels apart; ink {:.4}, the stock tip's {:.4}",
                ink(&ours),
                ink(&stock)
            );
            assert!(apart <= most_apart, "{key:016x}: {ours:?} against {stock:?}");
            assert!((ink(&ours) / ink(&stock) - 1.0).abs() < 0.02, "{key:016x}");
            // no ink within 4 texels of its edges
            let inside = |i: usize| (4..TEXTURE_SIZE - 4).contains(&i);
            assert!(
                pixels
                    .iter()
                    .enumerate()
                    .all(|(k, &v)| v == 0 || (inside(k % TEXTURE_SIZE) && inside(k / TEXTURE_SIZE)))
            );
        }
        // the hard round's edge a texel wide at every side: four times the texels round it at 1024
        // as at 256, not sixteen
        let (small, big) = (hard_round_at(TEXTURE_SIZE), hard_round_at(1024));
        let edge = |pixels: &[u8]| pixels.iter().filter(|&&v| v > 25 && v < 230).count();
        let (texels_small, texels_big) = (edge(&small), edge(&big));
        eprintln!("hard round edge texels (a tenth to nine tenths): {texels_small} at 256, {texels_big} at 1024");
        assert!(texels_big <= texels_small * 5 && texels_big >= texels_small * 3, "{texels_small} {texels_big}");
    }
}
