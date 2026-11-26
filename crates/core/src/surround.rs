//! The area round the canvas as plugins have it (interface 11): a colour of their own (the
//! `interface_colour` hook's role [`crate::hooks::SURROUND_ROLE`]), or a picture someone picked for
//! one of their `file` controls (the `surround_picture` hook), shown covering the window, fixed to it,
//! in place of the surround's grid.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use procraft_engine::geom::Vec2;

/// The longest side a picture round the canvas keeps: a bigger one is made this big.
pub(crate) const MOST_SIDE: u32 = 4096;

/// The id the next picture read is known by.
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The picture round the canvas the plugins give now (`path`, the file a plugin's control keeps),
/// `kept` if it is still the one read from that file as it is; read again, or none, otherwise.
pub(crate) fn picture_now(
    path: Option<PathBuf>,
    kept: Option<std::sync::Arc<SurroundPicture>>,
) -> Option<std::sync::Arc<SurroundPicture>> {
    let path = path?;
    let now = file_now(&path)?;
    if let Some(kept) = kept.filter(|k| k.from == now) {
        return Some(kept);
    }
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let read = read(&path, id).map(std::sync::Arc::new);
    if read.is_none() {
        log::warn!("a picture round the canvas couldn't be read");
    }
    read
}

/// `canvas` shows `picture` round itself where a target's grid says, unless it does already.
pub(crate) fn put_on(canvas: &mut procraft_gpu::Canvas, gpu: &procraft_gpu::Gpu, picture: Option<&SurroundPicture>) {
    if canvas.surround_picture_id() != picture.map(|p| p.id) {
        let id = picture.map_or(0, |p| p.id);
        canvas.set_surround_picture(gpu, id, picture.map(|p| (p.width, p.height, p.rgba.as_slice())));
    }
}

/// A picture round the canvas, read from the file a plugin's control keeps: straight sRGB RGBA8,
/// rows from the top, known by `id` (another for each picture read).
pub(crate) struct SurroundPicture {
    pub(crate) id: u64,
    /// The file it was read from, its length and when it was last written: read again when they
    /// change.
    pub(crate) from: (PathBuf, u64, Option<SystemTime>),
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

impl std::fmt::Debug for SurroundPicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurroundPicture").field("id", &self.id).field("size", &(self.width, self.height)).finish()
    }
}

/// What `path` is now, to tell whether a picture read from it is still it.
pub(crate) fn file_now(path: &Path) -> Option<(PathBuf, u64, Option<SystemTime>)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((path.to_path_buf(), meta.len(), meta.modified().ok()))
}

/// The picture in the file at `path` (PNG or JPEG), its longest side [`MOST_SIDE`] at most; none
/// if it can't be read.
pub(crate) fn read(path: &Path, id: u64) -> Option<SurroundPicture> {
    let from = file_now(path)?;
    let bytes = std::fs::read(path).ok()?;
    let (width, height, rgba) = crate::plugin_commands::decoded_image(&bytes)?;
    let (width, height, rgba) = within(width, height, rgba, MOST_SIDE)?;
    Some(SurroundPicture { id, from, width, height, rgba })
}

/// `rgba` (`width` × `height`) made no longer than `most` either way, each pixel the mean of those
/// it covers; as it is if it is already.
pub(crate) fn within(width: u32, height: u32, rgba: Vec<u8>, most: u32) -> Option<(u32, u32, Vec<u8>)> {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return None;
    }
    if width <= most && height <= most {
        return Some((width, height, rgba));
    }
    let k = f64::from(width.max(height)) / f64::from(most.max(1));
    let (w, h) = (((f64::from(width) / k).round() as u32).max(1), ((f64::from(height) / k).round() as u32).max(1));
    let mut out = vec![0_u8; w as usize * h as usize * 4];
    for y in 0..h {
        let (y0, y1) = (y * height / h, ((y + 1) * height / h).max(y * height / h + 1));
        for x in 0..w {
            let (x0, x1) = (x * width / w, ((x + 1) * width / w).max(x * width / w + 1));
            let mut sum = [0_u64; 4];
            for sy in y0..y1.min(height) {
                for sx in x0..x1.min(width) {
                    let at = (sy as usize * width as usize + sx as usize) * 4;
                    for (s, &v) in sum.iter_mut().zip(rgba.get(at..at + 4)?) {
                        *s += u64::from(v);
                    }
                }
            }
            let n = u64::from((y1.min(height) - y0) * (x1.min(width) - x0)).max(1);
            let at = (y as usize * w as usize + x as usize) * 4;
            for (o, s) in out.get_mut(at..at + 4)?.iter_mut().zip(sum) {
                *o = ((s + n / 2) / n) as u8;
            }
        }
    }
    Some((w, h, out))
}

/// Where a picture `size` lies covering a window `window` big (pixels), its middle on the window's
/// and as little of it cut off as can be, shifted by `by`: left, top, right and bottom.
pub(crate) fn cover(window: (u32, u32), size: (u32, u32), by: Vec2) -> [f32; 4] {
    let (ww, wh) = (window.0.max(1) as f32, window.1.max(1) as f32);
    let (pw, ph) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let k = (ww / pw).max(wh / ph);
    let (w, h) = (pw * k, ph * k);
    let (x0, y0) = ((ww - w) / 2.0 + by.x, (wh - h) / 2.0 + by.y);
    [x0, y0, x0 + w, y0 + h]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_covers_the_window_its_middle_on_the_window_s() {
        // wider than the window: as tall, its sides cut off alike
        assert_eq!(cover((1000, 500), (400, 100), Vec2::ZERO), [-500.0, 0.0, 1500.0, 500.0]);
        // taller: as wide
        assert_eq!(cover((1000, 500), (100, 100), Vec2::ZERO), [0.0, -250.0, 1000.0, 750.0]);
        // shifted, as the eyedropper's small target is
        assert_eq!(cover((1000, 500), (2, 1), Vec2::new(-10.0, -20.0)), [-10.0, -20.0, 990.0, 480.0]);
    }

    #[test]
    fn a_big_picture_is_made_small_each_pixel_the_mean_of_those_it_covers() {
        // 4 × 2: two white columns, two black, then halved
        let mut rgba = Vec::new();
        for _ in 0..2 {
            for v in [255, 255, 0, 0] {
                rgba.extend([v, v, v, 255]);
            }
        }
        let (w, h, small) = within(4, 2, rgba.clone(), 2).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(small, [255, 255, 255, 255, 0, 0, 0, 255]);
        // one that fits stays as it is; one that isn't a picture is none
        assert_eq!(within(4, 2, rgba.clone(), 4).unwrap().2, rgba);
        assert!(within(4, 3, rgba, 4).is_none());
        // a long one keeps its shape
        let long = vec![128_u8; 8000 * 10 * 4];
        let (w, h, _) = within(8000, 10, long, MOST_SIDE).unwrap();
        assert_eq!((w, h), (MOST_SIDE, 5));
    }
}
