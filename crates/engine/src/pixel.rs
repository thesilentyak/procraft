//! Pixel art: what a pixel art canvas ([`crate::document::Document::pixel_art`]) does with whole
//! pixels. Its tools set pixels on the CPU rather than stamping them, so everything here works on
//! RGBA8 rows.

/// The longest side a pixel art canvas may have: its tools read and write the whole layer on the
/// CPU, at most 4 MB.
pub const MOST_EDGE: u32 = 1024;

/// The sizes the New canvas list offers for pixel art (square).
pub const SIZES: [u32; 5] = [16, 32, 64, 128, 256];

/// The scales PNG export offers: each pixel a square of that many.
pub const EXPORT_SCALES: [u32; 5] = [1, 2, 4, 8, 16];

/// `rgba` (`width` × `height`, four bytes a pixel, rows from the top) with each pixel a `scale` ×
/// `scale` square of its colour. None when the result would be past `u32` or `usize` sizes, or
/// `rgba` isn't that size.
pub fn scaled_up(rgba: &[u8], width: u32, height: u32, scale: u32) -> Option<Vec<u8>> {
    let (w, h, n) = (width as usize, height as usize, scale.max(1) as usize);
    if rgba.len() != w.checked_mul(h)?.checked_mul(4)? {
        return None;
    }
    width.checked_mul(scale.max(1))?;
    height.checked_mul(scale.max(1))?;
    let row = w.checked_mul(n)?.checked_mul(4)?;
    let mut out = Vec::with_capacity(row.checked_mul(h)?.checked_mul(n)?);
    for line in rgba.chunks_exact(w * 4) {
        let start = out.len();
        for px in line.chunks_exact(4) {
            for _ in 0..n {
                out.extend_from_slice(px);
            }
        }
        for _ in 1..n {
            out.extend_from_within(start..start + row);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_up_makes_each_pixel_a_square() {
        let red = [255, 0, 0, 255];
        let clear = [0, 0, 0, 0];
        let picture = [red, clear].concat();
        let out = scaled_up(&picture, 2, 1, 3).unwrap();
        assert_eq!(out.len(), 6 * 3 * 4);
        for y in 0..3 {
            for x in 0..6 {
                let px = &out[(y * 6 + x) * 4..(y * 6 + x) * 4 + 4];
                assert_eq!(px, if x < 3 { &red } else { &clear }, "at {x}, {y}");
            }
        }
        assert_eq!(scaled_up(&picture, 2, 1, 1).unwrap(), picture);
        assert_eq!(scaled_up(&picture, 3, 1, 2), None, "the wrong size");
    }
}
