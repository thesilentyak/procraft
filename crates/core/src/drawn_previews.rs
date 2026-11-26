//! Sample strokes drawn already, kept a while by what they were drawn from: the library asks for
//! a brush's again whenever it changes (every row's, after any brush is moved, renamed or edited),
//! and sharing a set or library wants every brush's at the file's size. A brush as it was drawn
//! finds its stroke here, whatever its place, and a brush changed in any way that shows draws
//! anew, as it isn't the brush its kept stroke was drawn from.

use std::collections::VecDeque;
use std::hash::Hasher;
use std::sync::Arc;

use procraft_engine::brush::Brush;

/// How many bytes of kept strokes are held at most (each a PNG of the sample stroke's canvas,
/// some tens of KB; the library as it ships takes about 20 MB): past it the least recently used
/// go.
pub(crate) const KEPT_BYTES: usize = 32 << 20;

/// The strokes kept, the most recently used last.
#[derive(Default)]
pub(crate) struct DrawnPreviews {
    kept: VecDeque<(u64, Arc<[u8]>)>,
    bytes: usize,
    /// How many asks found their stroke kept, and how many didn't (for the tests and the log).
    pub(crate) found: u64,
    pub(crate) missed: u64,
}

impl DrawnPreviews {
    /// The kept stroke drawn from a brush looking as `key` says (see [`look_key`]), as
    /// [`packed`] packs it; none if none is kept.
    pub(crate) fn find(&mut self, key: u64) -> Option<Arc<[u8]>> {
        let at = self.kept.iter().position(|(k, _)| *k == key)?;
        self.found += 1;
        let entry = self.kept.remove(at)?;
        let png = Arc::clone(&entry.1);
        self.kept.push_back(entry);
        Some(png)
    }

    /// Keeps `png`, the stroke drawn from a brush looking as `key` says, the least recently used
    /// let go past [`KEPT_BYTES`].
    pub(crate) fn keep(&mut self, key: u64, png: Arc<[u8]>) {
        if png.len() > KEPT_BYTES {
            return;
        }
        if let Some(at) = self.kept.iter().position(|(k, _)| *k == key)
            && let Some((_, old)) = self.kept.remove(at)
        {
            self.bytes -= old.len();
        }
        self.bytes += png.len();
        self.kept.push_back((key, png));
        while self.bytes > KEPT_BYTES {
            let Some((_, old)) = self.kept.pop_front() else { break };
            self.bytes -= old.len();
        }
    }

    /// How many strokes are kept and their bytes.
    #[cfg(test)]
    pub(crate) fn held(&self) -> (usize, usize) {
        (self.kept.len(), self.bytes)
    }
}

/// The sample stroke's canvas (premultiplied RGBA8, `width` × `height`) packed to be kept: a PNG
/// of its coverage alone where it is white wherever it shows (the colour of each pixel its
/// coverage within [`WHITE_WITHIN`], as nearly every brush's is), else of its pixels as they are.
/// None if it can't be. What it gives back unpacked is what is shown, kept or just drawn.
pub(crate) fn packed(width: u32, height: u32, rgba: &[u8]) -> Option<Arc<[u8]>> {
    let (pixels, _) = rgba.as_chunks::<4>();
    let near = |c: u8, a: u8| c.abs_diff(a) <= WHITE_WITHIN;
    let png = if pixels.iter().all(|&[r, g, b, a]| near(r, a) && near(g, a) && near(b, a)) {
        let coverage: Vec<u8> = pixels.iter().map(|p| p[3]).collect();
        crate::brush_files::sized_gray_png(width, height, &coverage)
    } else {
        crate::project::png_of(width, height, rgba).ok()?
    };
    Some(png.into())
}

/// The canvas a kept stroke was packed from (see [`packed`]), if it is `width` × `height`.
pub(crate) fn unpacked(width: u32, height: u32, png: &[u8]) -> Option<Vec<u8>> {
    let (w, h, channels, pixels) = crate::brush_files::decode_picture(png)?;
    if (w, h) != (width as usize, height as usize) {
        return None;
    }
    match channels {
        1 => Some(pixels.iter().flat_map(|&v| [v; 4]).collect()),
        4 => Some(pixels),
        _ => None,
    }
}

/// How far a white stroke's colour may stray from its coverage, in levels, and be kept as its
/// coverage alone (the GPU rounds the two apart by a level here and there).
const WHITE_WITHIN: u8 = 2;

/// What a brush's sample stroke is drawn from, as a number: everything about it that can show in
/// the stroke, not what can't (its name, its place, its notches and sliders, its author, the
/// preview it came with, the point it resets to).
pub(crate) fn look_key(brush: &Brush) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut into = HashWriter(&mut hasher);
    // (formatted straight into the hash: nothing is held)
    let _ = std::fmt::Write::write_fmt(&mut into, format_args!("{:?}", looks(brush)));
    hasher.finish()
}

/// `brush` without what doesn't show in its stroke (see [`look_key`]), its second the same.
fn looks(brush: &Brush) -> Brush {
    Brush {
        name: String::new(),
        notches: Default::default(),
        sliders: [None; 3],
        thumbnail: None,
        author: None,
        reset_point: None,
        secondary: brush.secondary.as_deref().map(|second| Box::new(looks(second))),
        ..brush.clone()
    }
}

/// Text written into a hash.
struct HashWriter<'h>(&'h mut std::collections::hash_map::DefaultHasher);

impl std::fmt::Write for HashWriter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn past_the_most_bytes_the_least_recently_used_strokes_go() {
        let mut kept = DrawnPreviews::default();
        let third: Arc<[u8]> = vec![0u8; KEPT_BYTES / 3].into();
        for key in 1..=3 {
            kept.keep(key, Arc::clone(&third));
        }
        assert_eq!(kept.held(), (3, 3 * (KEPT_BYTES / 3)));
        // 1 used again, so 2 is the least recently used when a fourth comes
        assert!(kept.find(1).is_some());
        kept.keep(4, Arc::clone(&third));
        assert!(kept.find(2).is_none());
        assert!([1, 3, 4].iter().all(|&k| kept.find(k).is_some()));
        assert!(kept.held().1 <= KEPT_BYTES);
        // kept again under the same key: replaced, not held twice
        kept.keep(4, vec![1u8; 10].into());
        assert_eq!(kept.held(), (3, 2 * (KEPT_BYTES / 3) + 10));
        assert_eq!(kept.find(4).as_deref(), Some(&[1u8; 10][..]));
        // one bigger than all may hold is never kept
        kept.keep(5, vec![0u8; KEPT_BYTES + 1].into());
        assert!(kept.find(5).is_none());
        assert_eq!(kept.found, 5);
    }

    #[test]
    fn a_white_stroke_is_kept_as_its_coverage_and_any_other_as_it_is() {
        let white: Vec<u8> = (0..64u32).flat_map(|i| [(i * 4) as u8; 4]).collect();
        let packed_white = packed(8, 8, &white).unwrap();
        assert_eq!(unpacked(8, 8, &packed_white).as_deref(), Some(&white[..]));
        // a level or two off its coverage: white still, given back as its coverage
        let mut near = white.clone();
        near[4 * 9] = near[4 * 9 + 3] + 2;
        assert_eq!(unpacked(8, 8, &packed(8, 8, &near).unwrap()).as_deref(), Some(&white[..]));
        let mut tinted = white.clone();
        tinted[4 * 9] = 200;
        let packed_tinted = packed(8, 8, &tinted).unwrap();
        assert_eq!(unpacked(8, 8, &packed_tinted).as_deref(), Some(&tinted[..]));
        // a quarter of the bytes or less, for a white one
        assert!(packed_white.len() < packed_tinted.len());
        assert!(unpacked(4, 16, &packed_white).is_none());
    }

    #[test]
    fn a_brush_s_look_leaves_out_what_its_stroke_never_shows() {
        let pen = Brush::round_pen();
        let key = look_key(&pen);
        assert_eq!(key, look_key(&pen.clone()));
        let renamed = Brush { name: "Another".to_owned(), sliders: [Some([0.2, 0.3]); 3], ..pen.clone() };
        assert_eq!(look_key(&renamed), key);
        let mut spaced = pen.clone();
        spaced.set_setting("spacing", 0.5);
        assert_ne!(look_key(&spaced), key);
        // a second counts, its name doesn't
        let dual = Brush { secondary: Some(Box::new(pen.clone())), ..pen.clone() };
        let named =
            Brush { secondary: Some(Box::new(Brush { name: "Second".to_owned(), ..pen.clone() })), ..pen.clone() };
        assert_ne!(look_key(&dual), key);
        assert_eq!(look_key(&named), look_key(&dual));
        // a picture drawn the first time it is read looks the same before as after
        let source = procraft_engine::brush::Source::drawn_later(
            7,
            procraft_engine::brush::Pixels::later(|| Some(vec![9; procraft_engine::textures::TEXTURE_SIZE.pow(2)])),
        );
        let mut shaped = pen.clone();
        shaped.shape.source = Some(source.clone());
        let before = look_key(&shaped);
        let _ = source.pixels.shared();
        assert_eq!(look_key(&shaped), before);
    }
}
