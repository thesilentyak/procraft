//! A paper made of a picture someone picked (interface 12): a plugin on the `paper` hook names one of
//! its `file` controls of pictures (`procraft_paper_picture`), and the paper's tile is made here of
//! the picture picked for it, the same whichever plugin asks: all of it stretched to a square (a
//! picture that isn't one squeezed along its longer side), grey by how bright it is, scaled to the
//! tile and cross-faded into itself at its edges so that it repeats without a seam, its tones
//! stretched from a hollow to a peak.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// The tile's side, as the paper hook's.
const SIDE: usize = crate::plugin_run::PAPER_SIDE;
/// How far the tile is made to run on past its side, to be cross-faded into its start.
const FADE: usize = 16;
/// The longest side a picture is first made, each pixel the mean of those it covers: four times
/// what the tile needs, however big the picture.
const MOST_SIDE: u32 = 4 * (SIDE + FADE) as u32;

/// A paper's tile made of the picture in a file a plugin's control keeps.
pub(crate) struct PaperPicture {
    /// The file it was made from, its length and when it was last written: made again when they
    /// change.
    from: (PathBuf, u64, Option<SystemTime>),
    /// [`SIDE`] square, rows from the top, each byte how high the paper stands there.
    pub(crate) tile: Vec<u8>,
}

impl std::fmt::Debug for PaperPicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaperPicture").field("from", &self.from.0).finish()
    }
}

/// The tile of the picture in the file at `path` as it is now: `kept` if that was made of the file
/// as it is, made again otherwise; none if there is no file or it holds no picture.
pub(crate) fn tile_now(path: Option<PathBuf>, kept: Option<Arc<PaperPicture>>) -> Option<Arc<PaperPicture>> {
    let path = path?;
    let now = crate::surround::file_now(&path)?;
    if let Some(kept) = kept.filter(|k| k.from == now) {
        return Some(kept);
    }
    let made = read(&path).map(Arc::new);
    if made.is_none() {
        log::warn!("a paper's picture couldn't be read");
    }
    made
}

/// The tile of the picture (PNG or JPEG) in the file at `path`.
fn read(path: &Path) -> Option<PaperPicture> {
    let from = crate::surround::file_now(path)?;
    let bytes = std::fs::read(path).ok()?;
    let (width, height, rgba) = crate::plugin_commands::decoded_image(&bytes)?;
    let (width, height, rgba) = crate::surround::within(width, height, rgba, MOST_SIDE)?;
    let tile = picture_tile(width as usize, height as usize, &rgba)?;
    Some(PaperPicture { from, tile })
}

/// How bright an RGBA8 pixel (not premultiplied) is over white, 0 to 255 (Rec. 709's weights).
fn brightness(px: &[u8]) -> f32 {
    let [r, g, b, a] = [0, 1, 2, 3].map(|i| f32::from(px.get(i).copied().unwrap_or(255)));
    let lit = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let a = a / 255.0;
    lit * a + 255.0 * (1.0 - a)
}

/// `src` resampled to `n` values: each the mean of what it covers when there are fewer, read
/// between the nearest two when there are more.
fn resample(src: &[f32], n: usize) -> Vec<f32> {
    let m = src.len();
    if m == 0 || n == 0 {
        return vec![0.0; n];
    }
    let at = |i: usize| src.get(i.min(m - 1)).copied().unwrap_or(0.0);
    if n > m {
        return (0..n)
            .map(|i| {
                let x = ((i as f32 + 0.5) * m as f32 / n as f32 - 0.5).max(0.0);
                let (i0, f) = (x as usize, x.fract());
                at(i0) * (1.0 - f) + at(i0 + 1) * f
            })
            .collect();
    }
    let share = m as f32 / n as f32;
    (0..n)
        .map(|i| {
            let (from, to) = (i as f32 * share, (i + 1) as f32 * share);
            let mut sum = 0.0;
            let mut j = from as usize;
            while (j as f32) < to && j < m {
                let covered = (to.min(j as f32 + 1.0) - from.max(j as f32)).max(0.0);
                sum += at(j) * covered;
                j += 1;
            }
            sum / share
        })
        .collect()
}

/// `line` ([`SIDE`] + [`FADE`] values) as [`SIDE`] that repeat without a seam: its first [`FADE`]
/// cross-faded from what runs on past its end into what is there.
fn fold(line: &[f32]) -> Vec<f32> {
    (0..SIDE)
        .map(|x| {
            let here = line.get(x).copied().unwrap_or(0.0);
            if x >= FADE {
                return here;
            }
            let t = (x as f32 + 0.5) / FADE as f32;
            here * t + line.get(x + SIDE).copied().unwrap_or(here) * (1.0 - t)
        })
        .collect()
}

/// The tile made of a picture `width` × `height` (RGBA8 rows from the top, not premultiplied): all of
/// it stretched to a square, grey by how bright it is, scaled to the tile and cross-faded into itself at its
/// edges so that it repeats without a seam, its tones stretched so that its darkest hundredth is a
/// hollow (0) and its lightest a peak (255). A picture of one tone is flat, all peak. None for no
/// picture.
pub(crate) fn picture_tile(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() < width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let long = SIDE + FADE;
    // across each whole row, then down each whole column: stretched to the square each way
    let mut rows = Vec::with_capacity(height * SIDE);
    let mut line = vec![0.0; width];
    for y in 0..height {
        for (x, v) in line.iter_mut().enumerate() {
            let at = (y * width + x) * 4;
            *v = brightness(rgba.get(at..at + 4).unwrap_or_default());
        }
        rows.extend(fold(&resample(&line, long)));
    }
    let mut grey = vec![0.0_f32; SIDE * SIDE];
    let mut column = vec![0.0; height];
    for x in 0..SIDE {
        for (y, v) in column.iter_mut().enumerate() {
            *v = rows.get(y * SIDE + x).copied().unwrap_or(0.0);
        }
        for (y, v) in fold(&resample(&column, long)).into_iter().enumerate() {
            if let Some(g) = grey.get_mut(y * SIDE + x) {
                *g = v;
            }
        }
    }
    // its tones stretched between its darkest and lightest hundredths
    let mut counts = [0_usize; 256];
    for v in &grey {
        if let Some(c) = counts.get_mut(v.round().clamp(0.0, 255.0) as usize) {
            *c += 1;
        }
    }
    let hundredth = grey.len() / 100;
    let mut seen = 0;
    let low = counts.iter().position(|c| {
        seen += c;
        seen > hundredth
    });
    seen = 0;
    let high = counts.iter().rposition(|c| {
        seen += c;
        seen > hundredth
    });
    let (low, high) = (low.unwrap_or(0) as f32, high.unwrap_or(255) as f32);
    if high - low < 1.0 {
        return Some(vec![255; SIDE * SIDE]);
    }
    Some(grey.iter().map(|v| ((v - low) / (high - low) * 255.0).round().clamp(0.0, 255.0) as u8).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture `width` × `height` whose pixel at (x, y) is grey `of(x, y)`, opaque.
    fn picture(width: usize, height: usize, of: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        (0..width * height)
            .flat_map(|i| {
                let g = of(i % width, i / width);
                [g, g, g, 255]
            })
            .collect()
    }

    #[test]
    fn a_picture_makes_a_tile_of_all_of_it_stretched_to_a_square_its_tones_from_hollow_to_peak() {
        // 400 × 100, dark on its first quarter alone: stretched, the tile's first quarter is dark
        // (a middle square, x 150 to 250, would have been all light)
        let wide = picture_tile(400, 100, &picture(400, 100, |x, _| if x < 100 { 90 } else { 200 })).unwrap();
        let at = |x: usize, y: usize| wide[y * SIDE + x];
        assert_eq!((at(SIDE / 8, SIDE / 2), at(SIDE * 5 / 8, SIDE / 2)), (0, 255));
        // 300 × 200, dark on the left half, light on the right: the edge at the tile's middle
        let pixels = picture(300, 200, |x, _| if x < 150 { 90 } else { 200 });
        let tile = picture_tile(300, 200, &pixels).unwrap();
        assert_eq!(tile.len(), SIDE * SIDE);
        let at = |x: usize, y: usize| tile[y * SIDE + x];
        // away from the edge and the cross-fade: a hollow and a peak
        assert_eq!((at(40, 100), at(110, 30), at(200, 100), at(250, 200)), (0, 0, 255, 255));
        // a picture of one tone: flat, all peak (the paper takes nothing away)
        let flat = picture_tile(64, 64, &picture(64, 64, |_, _| 120)).unwrap();
        assert!(flat.iter().all(|&v| v == 255));
        // see-through is taken as over white: a see-through black is a peak
        let mut clear = picture(32, 32, |x, _| if x < 16 { 0 } else { 255 });
        for px in clear.as_chunks_mut::<4>().0 {
            px[3] = 0;
        }
        assert!(picture_tile(32, 32, &clear).unwrap().iter().all(|&v| v == 255));
        // no picture, or too few bytes for one
        assert_eq!(picture_tile(0, 10, &[]), None);
        assert_eq!(picture_tile(10, 10, &[0; 399]), None);
    }

    #[test]
    fn a_picture_s_tile_repeats_without_a_seam() {
        // a ramp across and down: across the tile's edge it goes on no more sharply than inside
        let ramp = picture(512, 400, |x, y| ((x * 255 / 511 + y * 255 / 399) / 2) as u8);
        let tile = picture_tile(512, 400, &ramp).unwrap();
        let at = |x: usize, y: usize| i32::from(tile[(y % SIDE) * SIDE + x % SIDE]);
        let across_inside = (1..SIDE).map(|x| (at(x, 100) - at(x - 1, 100)).abs()).max().unwrap();
        let down_inside = (1..SIDE).map(|y| (at(100, y) - at(100, y - 1)).abs()).max().unwrap();
        let across = (at(0, 100) - at(SIDE - 1, 100)).abs();
        let down = (at(100, 0) - at(100, SIDE - 1)).abs();
        println!("a ramp's tile: {across} across its edge ({across_inside} inside), {down} down ({down_inside})");
        assert!(across <= across_inside && down <= down_inside, "across {across}, down {down}");
        // it still runs from hollow to peak
        let (low, high) = (*tile.iter().min().unwrap(), *tile.iter().max().unwrap());
        assert!(low < 20 && high > 235, "{low} to {high}");
    }

    #[test]
    fn a_big_picture_s_tile_is_made_from_it_made_small_first_quickly() {
        let big = picture(4096, 3072, |x, y| (((x / 4) ^ (y / 4)) & 255) as u8);
        let start = std::time::Instant::now();
        let (w, h, small) = crate::surround::within(4096, 3072, big, MOST_SIDE).unwrap();
        let tile = picture_tile(w as usize, h as usize, &small).unwrap();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        println!("a 4096 × 3072 picture's tile: {ms:.0} ms (this build), made {w} × {h} first");
        assert_eq!((w, h, tile.len()), (MOST_SIDE, 816, SIDE * SIDE));
        // a picture is picked now and then, off the drawing's way: under a second even unoptimised
        assert!(ms < 1000.0 || cfg!(debug_assertions) && ms < 5000.0, "{ms} ms");
    }

    #[test]
    fn a_file_s_tile_is_made_again_only_when_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("procraft-paper-picture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("paper.png");
        // a picture `side` square, dark on its left half and light on its right (or the other way)
        let png = |side: u32, light_left: bool| {
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, side, side);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let grey = |x: u32| if (x < side / 2) == light_left { 255 } else { 0 };
            let pixels: Vec<u8> =
                (0..side * side).flat_map(|i| [grey(i % side), grey(i % side), grey(i % side), 255]).collect();
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
            bytes
        };
        std::fs::write(&path, png(64, false)).unwrap();
        let first = tile_now(Some(path.clone()), None).unwrap();
        assert_eq!((first.tile[100 * SIDE + 60], first.tile[100 * SIDE + 200]), (0, 255));
        // the same file: the tile kept
        let again = tile_now(Some(path.clone()), Some(Arc::clone(&first))).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        // another picture in it (of another length): made again
        std::fs::write(&path, png(48, true)).unwrap();
        let other = tile_now(Some(path.clone()), Some(Arc::clone(&first))).unwrap();
        assert!(!Arc::ptr_eq(&first, &other));
        assert_eq!((other.tile[100 * SIDE + 60], other.tile[100 * SIDE + 200]), (255, 0));
        // no file, or no picture in it
        assert!(tile_now(None, Some(other)).is_none());
        std::fs::write(&path, b"not a picture").unwrap();
        assert!(tile_now(Some(path), None).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
