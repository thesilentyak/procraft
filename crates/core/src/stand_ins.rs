//! Our stand-ins for the stock shapes and grains brush files name (and the stock brushes draw
//! with): how each is drawn, from our shapes and grains and the stock image's measurements. Kept
//! apart from the rest of reading brush files because the big pictures made from them are kept on
//! disk under a key made from this file (see `big_pictures.rs`): a change here draws them again.

use procraft_engine::brush::Source;
use procraft_engine::brushfile::{Pick, StandIn};
use procraft_engine::textures::{self, TEXTURE_SIZE, shape};

/// The shape a stand-in for a stock one draws (see [`shape_for`]), its image drawn the first time
/// it is wanted.
pub(crate) fn drawn_shape(s: &StandIn) -> (Option<u32>, Option<Source>, Option<f32>) {
    let (key, inverted) = (s.key, s.inverted);
    let i = match s.pick {
        // a hard tip: a disc of ours as its picture, its edge read through its levels as a stock
        // tip's is (a big picture too, as big as the stock tip, for a big stamp)
        Pick::Round(hardness) if hardness >= HARD_ROUND && crisp(key, inverted) => {
            let most = textures::stock_shape_side(key).map_or(HARD_ROUND_SIDE, |side| sharpest(Some(side)));
            let disc = drawn(SHAPE, s, most, |side| Some(textures::hard_round_at(side)));
            return (None, Some(disc), Some(hardness));
        }
        Pick::Round(hardness) => return (None, None, Some(hardness)),
        // the small point: our soft dot, as big and as soft as the stock one measures (soft all
        // through, so no big picture)
        Pick::Point => {
            let dot = drawn(SHAPE, s, TEXTURE_SIZE, |side| Some(emptied_border(textures::point_at(side), side)));
            return (None, Some(dot), None);
        }
        // a plain image: the stamp's whole square (nothing, inverted)
        Pick::Blank => {
            let fill = if inverted { 0 } else { 255 };
            return (
                None,
                Some(drawn(SHAPE, s, TEXTURE_SIZE, move |_| Some(vec![fill; TEXTURE_SIZE * TEXTURE_SIZE]))),
                None,
            );
        }
        // a tip printed in dots: ours dotted on the same lattice, each dot as big as the stock
        // tip's ink there
        Pick::Halftone(lattice) => {
            let measured = (
                textures::stock_shape_rows_of(key, inverted),
                textures::stock_shape_cols_of(key, inverted),
                textures::stock_shape_tones_of(key, inverted),
            );
            let (Some(rows), Some(cols), Some(tones)) = measured else { return (Some(shape::STOCK_DOTS), None, None) };
            let most = sharpest(textures::stock_shape_side(key));
            let dots = drawn(SHAPE, s, most, move |side| {
                let cover = tones.iter().map(|&t| f32::from(t)).sum::<f32>() / (tones.len().max(1) as f32 * 255.0);
                Some(emptied_border(textures::halftone_shape_at(lattice, &rows, &cols, cover, side), side))
            });
            return (Some(shape::STOCK_DOTS), Some(dots), None);
        }
        Pick::Ours(None) => return (None, None, None),
        Pick::Ours(Some(i)) => i,
    };
    // our shape with the stock one's own lights and darks and its rows' share of the ink, where
    // they were measured: all of the stock one as its brush shows it (inverted, for one that
    // inverts it: most stock tips are drawn dark on white), our shape as it is
    let Some(tones) = textures::stock_shape_tones_of(key, inverted) else { return (Some(i), None, None) };
    if i >= textures::SHAPE_COUNT {
        return (Some(i), None, None);
    }
    // a big picture too, the size of the real tip's own, where ours shows more so, or takes marks
    // finer than its texels show
    let marked = textures::stock_shape_feature_of(key).flatten().is_some();
    let most =
        if textures::shape_sharpens(i) || marked { sharpest(textures::stock_shape_side(key)) } else { TEXTURE_SIZE };
    let retoned = drawn(SHAPE, s, most, move |side| {
        let mut pixels = textures::shape_at(i, side)?;
        // as wide and tall as the real tip, where it lies in its square (the grainy square,
        // standing in for a tip we know nothing else of, takes its outline across and down below)
        if i != shape::STOCK_GRAINY_SQUARE
            && let (Some(rows), Some(cols)) =
                (textures::stock_shape_rows_of(key, inverted), textures::stock_shape_cols_of(key, inverted))
            && let Some(fitted) = textures::fitted_to_box(&pixels, &rows, &cols)
        {
            pixels = fitted;
        }
        // its marks as fine as the real tip's: standing in for a tip we know nothing else of, the
        // grainy square (smooth for a solid one); any other shape of ours takes them over itself,
        // but one drawn to the picture, which has its own
        let drawn = textures::drawn_to_stock(i);
        if let Some(feature) = textures::stock_shape_feature_of(key).filter(|_| !drawn) {
            let cover = tones.iter().map(|&t| f32::from(t)).sum::<f32>() / (tones.len().max(1) as f32 * 255.0);
            if i == shape::STOCK_GRAINY_SQUARE {
                pixels = textures::grainy_square_at(feature, cover, side);
            } else if let Some(marked) = textures::textured_shape(&pixels, feature, cover) {
                pixels = marked;
            }
        }
        // its streaks and gaps across the stroke first (and, standing in for a tip we know
        // nothing else of, its outline across too)
        let rows = textures::stock_shape_rows_of(key, inverted).filter(|_| !drawn);
        let cols = textures::stock_shape_cols_of(key, inverted).filter(|_| i == shape::STOCK_GRAINY_SQUARE);
        let striped = match (rows, cols) {
            (Some(rows), Some(cols)) => Some(textures::shaped(&pixels, &rows, &cols)),
            (Some(rows), None) => Some(textures::striped(&pixels, &rows)),
            _ => None,
        };
        let pixels = striped.as_deref().unwrap_or(&pixels);
        Some(emptied_border(textures::retoned_shape(pixels, &tones, &textures::SHAPE_TONE_SHARES), side))
    });
    (Some(i), Some(retoned), None)
}

/// The least hardness a round tip standing in for a stock one is drawn as a picture at (see
/// [`drawn_shape`]): the hard tip's (a medium one keeps its falloff by its hardness).
const HARD_ROUND: f32 = 0.9;

/// The side of the big picture of a hard tip whose stock image's side isn't known: the stock hard
/// tip's.
const HARD_ROUND_SIDE: usize = 1024;

/// Whether the stock tip under `key` is crisp, as the stock hard tip is: its measured tones go
/// from none to full with one between, or weren't measured (a tip whose name says it is hard, and
/// nothing else known of it). A softer one, a photo of a round with a blurred rim, keeps the round
/// tip drawn by its hardness.
fn crisp(key: u64, inverted: bool) -> bool {
    textures::stock_shape_tones_of(key, inverted)
        .is_none_or(|tones| tones.iter().filter(|&&v| v > 0 && v < 255).count() <= 1)
}

/// How far in from its edges (a share of its side) a stand-in shape holds no faint ink: a stock
/// tip's outer rows and columns are empty, and a faint one there (a level or two, where its tones
/// put the faintest of it) adds up under the many stamps a wet stroke lays into a line beside it.
const EMPTY_MARGIN: usize = 32;

/// The most a texel near the edges of a stand-in shape may hold and still be taken for faint
/// (out of 255).
const FAINT: u8 = 3;

/// `pixels` (a shape `side` square) with its edges empty, as a stock tip's are: its outer texels
/// (as many as one at [`TEXTURE_SIZE`]) taken to none, and anything faint (up to [`FAINT`]) within
/// a [`EMPTY_MARGIN`]th of its side of them, so that no level of it holds any there either.
fn emptied_border(mut pixels: Vec<u8>, side: usize) -> Vec<u8> {
    let outer = (side / TEXTURE_SIZE).max(1);
    let margin = (side / EMPTY_MARGIN).max(outer);
    for (k, v) in pixels.iter_mut().enumerate() {
        let (x, y) = (k % side.max(1), k / side.max(1));
        let edge = x.min(y).min(side.saturating_sub(1 + x)).min(side.saturating_sub(1 + y));
        if edge < outer || (edge < margin && *v <= FAINT) {
            *v = 0;
        }
    }
    pixels
}

/// What [`drawn`] draws: a shape or a grain.
const SHAPE: u8 = 0;
const GRAIN: u8 = 1;

/// A stand-in drawn, as what it is drawn from tells it apart from any other.
type DrawnKey = (u8, u64, bool, [u32; 4]);

/// The stand-ins made already, while a brush holds them: brushes standing in for the same stock
/// image share its pixels, drawn once, rather than each drawing and holding its own.
static DRAWN: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<DrawnKey, procraft_engine::brush::WeakPixels>>,
> = std::sync::LazyLock::new(Default::default);

/// Stand-in `s` (a shape or a grain, `kind`), its image what `draw` draws at a side (`TEXTURE_SIZE`
/// square the first time it is wanted, and `most` a side for its big picture), or
/// another brush's for the same; known by what it is drawn from, and as standing in for the stock
/// image `s` is for.
fn drawn(
    kind: u8,
    s: &StandIn,
    most: usize,
    draw: impl Fn(usize) -> Option<Vec<u8>> + Send + Sync + 'static,
) -> Source {
    let pick = match s.pick {
        Pick::Round(h) => [1, h.to_bits(), 0, 0],
        Pick::Blank => [2, 0, 0, 0],
        Pick::Ours(i) => [3, i.unwrap_or(u32::MAX), 0, 0],
        Pick::Halftone([(a, b), (c, d)]) => [a.to_bits(), b.to_bits(), c.to_bits(), d.to_bits()],
        Pick::Point => [4, 0, 0, 0],
    };
    let key: DrawnKey = (kind, s.key, s.inverted, pick);
    // its id: FNV-1a of what it is drawn from
    let words = [u64::from(kind), s.key, u64::from(s.inverted)].into_iter().chain(pick.map(u64::from));
    let id = words
        .flat_map(u64::to_le_bytes)
        .fold(0x6c62_272e_07bb_0142u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    let mut map = DRAWN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let pixels = match map.get(&key).and_then(procraft_engine::brush::WeakPixels::upgrade) {
        Some(pixels) => pixels,
        None => {
            if map.len() >= 1024 {
                map.retain(|_, pixels| pixels.upgrade().is_some());
            }
            let pixels = procraft_engine::brush::Pixels::later_at(most, draw);
            map.insert(key, pixels.downgrade());
            pixels
        }
    };
    Source::drawn_later(id, pixels).standing_in_for(s.key)
}

/// The side of the big picture a stand-in for a stock image `side` pixels across (if it is known)
/// has, for a big stamp or tile (see `big_pictures.rs`): the power of two nearest the image's own
/// side, as near in the ratio either way (1448 is 1024, 1449 is 2048), up to [`textures::SHARPEST`];
/// [`TEXTURE_SIZE`] (none bigger than brushes hold) for one nearer that; the most, for one not
/// known.
fn sharpest(side: Option<usize>) -> usize {
    match side {
        Some(side) if side >= TEXTURE_SIZE => {
            let below = 1usize << side.ilog2();
            // from below times root two the next is nearer
            let nearer = (side as u64).pow(2) >= 2 * (below as u64).pow(2);
            (if nearer { below * 2 } else { below }).min(textures::SHARPEST)
        }
        Some(_) => TEXTURE_SIZE,
        None => textures::SHARPEST,
    }
}

/// The grain a stand-in for a stock one draws (see [`grain_for`]), its image drawn the first time
/// it is wanted: our pattern with the stock grain's own lights and darks, where they were measured;
/// one drawn to the stock image's proportions as it is drawn (measured smaller, the stock image's
/// fine lines and dots go grey, which would take the pattern's contrast).
pub(crate) fn drawn_grain(s: &StandIn) -> (Option<u32>, Option<Source>) {
    let Pick::Ours(Some(i)) = s.pick else { return (None, None) };
    let (key, inverted) = (s.key, s.inverted);
    let invert = move |mut pixels: Vec<u8>| {
        if inverted {
            pixels.iter_mut().for_each(|v| *v = 255 - *v);
        }
        pixels
    };
    let tones = textures::stock_grain_tones_of(key);
    let source = if textures::drawn_by_octaves(i)
        && let (Some((_, octaves)), Some(tones)) = (textures::stock_grain_octaves_of(key), tones)
    {
        // a paper or a wash: noise as strong at each size as the stock image is, with its tones
        // (tiled as a share of that image is; its finest octave is its texels, nothing finer known)
        Some(drawn(GRAIN, s, TEXTURE_SIZE, move |_| Some(invert(textures::octave_grain(&octaves, &tones)))))
    } else if i < textures::GRAIN_COUNT && (textures::drawn_to_stock_size(i) || tones.is_some()) {
        // a big picture too, the size of the stock image's own, where ours shows more so
        let most = if textures::grain_sharpens(i) { sharpest(textures::stock_grain_side(key)) } else { TEXTURE_SIZE };
        Some(drawn(GRAIN, s, most, move |side| {
            let pattern = textures::grain_at(i, side)?;
            let pixels = match tones.filter(|_| !textures::drawn_to_stock_size(i)) {
                Some(tones) => textures::retoned(&pattern, &tones, &textures::TONE_SHARES),
                None => pattern,
            };
            Some(invert(pixels))
        }))
    } else {
        None
    };
    (Some(i), source)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Of `pixels` (a shape `side` square): how far in from its edges the first texel holding any
    /// ink is, how many texels within `margin` of them are faint (1 to [`FAINT`]), and whether any
    /// there holds more.
    fn border(pixels: &[u8], side: usize, margin: usize) -> (usize, usize, bool) {
        let (mut nearest, mut faint, mut inked) = (side, 0, false);
        for (k, &v) in pixels.iter().enumerate() {
            let (x, y) = (k % side, k / side);
            let edge = x.min(y).min(side - 1 - x).min(side - 1 - y);
            if v > 0 {
                nearest = nearest.min(edge);
            }
            if edge < margin {
                faint += usize::from(v > 0 && v <= FAINT);
                inked |= v > FAINT;
            }
        }
        (nearest, faint, inked)
    }

    /// The stock shapes' stand-ins, drawn (all but the blank square, the stamp's whole square).
    fn stock_shapes() -> Vec<(u64, Source)> {
        let mut seen = std::collections::HashSet::new();
        crate::stock_brushes::SETS
            .iter()
            .flat_map(|set| set.brushes)
            .flat_map(|b| [Some(b), b.second].into_iter().flatten())
            .filter_map(|b| b.shape)
            .filter(|s| s.pick != Pick::Blank && seen.insert((s.key, s.inverted, format!("{:?}", s.pick))))
            .filter_map(|s| Some((s.key, drawn_shape(&s).1?)))
            .collect()
    }

    /// `pixels` (a shape `side` square) has empty edges: none of its outer texels (as many as one
    /// at [`TEXTURE_SIZE`]) holds any ink, nothing faint lies near them, and, unless it has ink
    /// near them itself, its smaller levels down to 32 texels have none at their edges.
    fn assert_empty_edges(pixels: &[u8], side: usize, what: &str) {
        let (nearest, faint, inked) = border(pixels, side, side / EMPTY_MARGIN);
        assert!(nearest >= (side / TEXTURE_SIZE).max(1) && faint == 0, "{what}: ink {nearest} in, {faint} faint");
        if inked {
            return;
        }
        let levels = procraft_gpu::grey_levels(pixels, side as u32).unwrap();
        let (mut at, mut n) = (0, side);
        while n >= 32 {
            let (nearest, ..) = border(&levels[at..at + n * n], n, 1);
            assert!(nearest >= 1, "{what}: ink at the edge of its level {n} across");
            at += n * n;
            n /= 2;
        }
    }

    /// Every stock shape's stand-in has empty edges, as the stock tips have (so that a wet stroke's
    /// many stamps lay no line beside it): at the size brushes hold, and big for a few of them
    /// (all: `every_stock_shape_s_big_picture_has_empty_edges`).
    #[test]
    fn every_stock_shape_has_empty_edges() {
        let shapes = stock_shapes();
        assert!(shapes.len() > 200, "{} shapes", shapes.len());
        let mut checked_big = 0;
        for (n, (key, source)) in shapes.iter().enumerate() {
            assert_empty_edges(&source.pixels.shared(), TEXTURE_SIZE, &format!("{key:016x}"));
            let side = source.pixels.most_side();
            if n % 40 == 0 && side > TEXTURE_SIZE {
                assert_empty_edges(&source.pixels.sharper(side).unwrap(), side, &format!("{key:016x} big"));
                checked_big += 1;
            }
        }
        assert!(checked_big >= 3, "{checked_big}");
    }

    /// [`every_stock_shape_has_empty_edges`] for every stock shape's big picture (minutes: run with
    /// `--ignored`).
    #[test]
    #[ignore]
    fn every_stock_shape_s_big_picture_has_empty_edges() {
        for (key, source) in stock_shapes() {
            let side = source.pixels.most_side();
            if side > TEXTURE_SIZE {
                assert_empty_edges(&source.pixels.sharper(side).unwrap(), side, &format!("{key:016x} big"));
            }
        }
    }

    /// A stand-in's big picture is the power of two nearest the stock image's side: as near to it
    /// as can be, a little bigger or a little smaller.
    #[test]
    fn a_stand_in_s_big_picture_is_the_power_of_two_nearest_the_stock_image() {
        let at = |side| sharpest(Some(side));
        assert_eq!([at(2048), at(1925), at(1449), at(1448), at(1024)], [2048, 2048, 2048, 1024, 1024]);
        assert_eq!([at(800), at(648), at(400), at(300), at(64)], [1024, 512, 512, 256, 256]);
        // no bigger than ours are drawn, and the most for one not known
        assert_eq!(at(4096), textures::SHARPEST);
        assert_eq!(sharpest(None), textures::SHARPEST);
    }
}
