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

/// The most pixels across the pixel pencil draws.
pub const MOST_PENCIL: u32 = 8;

/// The pencil's width in pixels for the size slider at `size` (0 to 1): 1 to [`MOST_PENCIL`].
pub fn pencil_width(size: f32) -> u32 {
    let size = if size.is_finite() { size.clamp(0.0, 1.0) } else { 0.0 };
    1 + (size * (MOST_PENCIL - 1) as f32).round() as u32
}

/// The pixels a pencil `width` pixels across covers, as offsets from the pixel it's on: a square up
/// to 2 across, a disc of whole pixels from 3. An even width reaches one further right and down.
pub fn footprint(width: u32) -> Vec<(i32, i32)> {
    let n = width.clamp(1, MOST_PENCIL) as i32;
    let lo = -(n - 1) / 2;
    let r = n as f32 / 2.0;
    let mut out = Vec::new();
    for dy in lo..lo + n {
        for dx in lo..lo + n {
            // the pixel's middle from the footprint's
            let (cx, cy) = ((dx - lo) as f32 + 0.5 - r, (dy - lo) as f32 + 0.5 - r);
            if n <= 2 || cx * cx + cy * cy <= (r - 0.25) * (r - 0.25) {
                out.push((dx, dy));
            }
        }
    }
    out
}

/// The pixels on the line from `a` to `b`, both ends included, one per step along its longer way
/// (Bresenham's).
pub fn line(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x, mut y) = a;
    let (dx, dy) = ((b.0 - x).abs(), -(b.1 - y).abs());
    let (sx, sy) = (if x < b.0 { 1 } else { -1 }, if y < b.1 { 1 } else { -1 });
    let mut err = dx + dy;
    let mut out = Vec::with_capacity((dx.max(-dy) + 1) as usize);
    loop {
        out.push((x, y));
        if (x, y) == b {
            return out;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// A one-pixel line's path as it's drawn, kept *pixel-perfect*: where it turns an L-shaped corner,
/// the corner's middle pixel is taken out, so the line steps diagonally as a pixel artist draws it.
#[derive(Debug, Default)]
pub struct Perfect {
    path: Vec<(i32, i32)>,
}

impl Perfect {
    /// `p` added to the path (nothing if it's where the path is): whether it goes on, and the pixel
    /// to take out again, if adding it made a corner.
    pub fn add(&mut self, p: (i32, i32)) -> (bool, Option<(i32, i32)>) {
        if self.path.last() == Some(&p) {
            return (false, None);
        }
        self.path.push(p);
        let Some(&[a, b, c]) = self.path.last_chunk::<3>() else { return (true, None) };
        let side = |u: (i32, i32), v: (i32, i32)| (u.0 - v.0).abs() + (u.1 - v.1).abs() == 1;
        let diagonal = (a.0 - c.0).abs() == 1 && (a.1 - c.1).abs() == 1;
        let n = self.path.len();
        // the corner's middle goes, unless the path was on it before (it crosses itself there)
        let earlier = self.path.get(..n - 3).unwrap_or_default();
        if side(a, b) && side(b, c) && diagonal && !earlier.contains(&b) {
            self.path.remove(n - 2);
            return (true, Some(b));
        }
        (true, None)
    }
}

/// `mask` (0 to 255 a pixel) made all or nothing: a pixel at least half selected wholly in, the
/// rest out, as a pixel art canvas's selections are.
pub fn harden(mask: &mut [u8]) {
    for v in mask {
        *v = if *v >= 128 { 255 } else { 0 };
    }
}

/// How the pixel pencil changes a pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ink {
    /// The colour (RGB 0 to 1) at this opacity over what was there.
    Paint([f32; 3], f32),
    /// This much of what was there taken away.
    Erase(f32),
}

/// A pixel pencil stroke over a layer's pixels (premultiplied RGBA8, `width` × `height`): each pixel
/// it reaches is set once, from what it was as the stroke began, so going over it again doesn't
/// build up. It keeps to `mask` (a pixel in when at least half selected) and, with `alpha_lock`, to
/// the alpha that was there.
pub struct Stroke {
    width: u32,
    height: u32,
    before: Vec<u8>,
    now: Vec<u8>,
    done: Vec<bool>,
    ink: Ink,
    alpha_lock: bool,
    mask: Option<Vec<u8>>,
    changed: Option<[i32; 4]>,
}

impl Stroke {
    /// None when `pixels` or `mask` isn't `width` × `height`.
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        ink: Ink,
        alpha_lock: bool,
        mask: Option<Vec<u8>>,
    ) -> Option<Stroke> {
        let count = (width as usize).checked_mul(height as usize)?;
        if pixels.len() != count.checked_mul(4)? || mask.as_ref().is_some_and(|m| m.len() != count) {
            return None;
        }
        Some(Stroke {
            width,
            height,
            now: pixels.clone(),
            before: pixels,
            done: vec![false; count],
            ink,
            alpha_lock,
            mask,
            changed: None,
        })
    }

    fn index(&self, (x, y): (i32, i32)) -> Option<usize> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        (x < self.width && y < self.height).then(|| y as usize * self.width as usize + x as usize)
    }

    fn grow(&mut self, (x, y): (i32, i32)) {
        let [x0, y0, x1, y1] = self.changed.unwrap_or([x, y, x + 1, y + 1]);
        self.changed = Some([x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)]);
    }

    /// Sets the pixel at `p` (on the canvas, in the mask), once a stroke.
    pub fn set(&mut self, p: (i32, i32)) {
        let Some(i) = self.index(p) else { return };
        if self.done.get(i).copied().unwrap_or(true)
            || self.mask.as_ref().is_some_and(|m| m.get(i).is_none_or(|&v| v < 128))
        {
            return;
        }
        let Some(was) = self.before.get(i * 4..i * 4 + 4).and_then(|b| <[u8; 4]>::try_from(b).ok()) else { return };
        let out = inked(was, self.ink, self.alpha_lock);
        if let (Some(px), Some(d)) = (self.now.get_mut(i * 4..i * 4 + 4), self.done.get_mut(i)) {
            px.copy_from_slice(&out);
            *d = true;
        }
        self.grow(p);
    }

    /// Puts the pixel at `p` back as it was before the stroke (a pixel-perfect corner taken out).
    pub fn unset(&mut self, p: (i32, i32)) {
        let Some(i) = self.index(p) else { return };
        if !self.done.get(i).copied().unwrap_or(false) {
            return;
        }
        let was: Option<[u8; 4]> = self.before.get(i * 4..i * 4 + 4).and_then(|b| b.try_into().ok());
        if let (Some(was), Some(px), Some(d)) = (was, self.now.get_mut(i * 4..i * 4 + 4), self.done.get_mut(i)) {
            px.copy_from_slice(&was);
            *d = false;
        }
        self.grow(p);
    }

    /// Puts every pixel the stroke set back as it was (a shape drawn again in place of the line).
    pub fn reset(&mut self) {
        let w = self.width.max(1) as usize;
        let set: Vec<usize> = self.done.iter().enumerate().filter(|(_, d)| **d).map(|(i, _)| i).collect();
        for i in set {
            self.unset(((i % w) as i32, (i / w) as i32));
        }
    }

    /// The rectangle (x0, y0, x1, y1, ends past) changed since this was last asked, and its pixels
    /// as they are now, row by row; none if nothing changed.
    pub fn take_changed(&mut self) -> Option<([i32; 4], Vec<u8>)> {
        let rect = self.changed.take()?;
        Some((rect, self.part(rect, &self.now)))
    }

    /// The pixels inside `rect` as they were before the stroke, row by row: to put back.
    pub fn before_part(&self, rect: [i32; 4]) -> Vec<u8> {
        self.part(rect, &self.before)
    }

    /// Whether any pixel is set.
    pub fn any(&self) -> bool {
        self.done.contains(&true)
    }

    fn part(&self, [x0, y0, x1, y1]: [i32; 4], from: &[u8]) -> Vec<u8> {
        let w = self.width as usize;
        let mut out = Vec::new();
        for y in y0.max(0)..y1.min(self.height as i32) {
            let row = y as usize * w;
            let (a, b) = (x0.max(0) as usize, (x1.max(0) as usize).min(w));
            out.extend_from_slice(from.get((row + a) * 4..(row + b) * 4).unwrap_or_default());
        }
        out
    }
}

/// `was` (premultiplied RGBA8) as `ink` leaves it.
fn inked(was: [u8; 4], ink: Ink, alpha_lock: bool) -> [u8; 4] {
    let [r, g, b, a] = was.map(|v| f32::from(v) / 255.0);
    let out = match ink {
        Ink::Paint(colour, opacity) => {
            let o = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 };
            let c = colour.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 });
            if alpha_lock {
                // the colour mixed into what's there, its coverage kept
                let [sr, sg, sb] = [r, g, b].map(|v| if a > 0.0 { v / a } else { 0.0 });
                let mix = |was: f32, to: f32| (was + (to - was) * o) * a;
                [mix(sr, c[0]), mix(sg, c[1]), mix(sb, c[2]), a]
            } else {
                [c[0] * o + r * (1.0 - o), c[1] * o + g * (1.0 - o), c[2] * o + b * (1.0 - o), o + a * (1.0 - o)]
            }
        }
        // an alpha-locked layer keeps its alpha: erasing leaves it as it is
        Ink::Erase(_) if alpha_lock => [r, g, b, a],
        Ink::Erase(amount) => {
            let keep = 1.0 - if amount.is_finite() { amount.clamp(0.0, 1.0) } else { 1.0 };
            [r * keep, g * keep, b * keep, a * keep]
        }
    };
    out.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pencil_is_one_to_eight_pixels() {
        assert_eq!(pencil_width(0.0), 1);
        assert_eq!(pencil_width(1.0), 8);
        assert_eq!(pencil_width(f32::NAN), 1);
        assert_eq!(footprint(1), vec![(0, 0)]);
        assert_eq!(footprint(2), vec![(0, 0), (1, 0), (0, 1), (1, 1)]);
        // three across: a plus, its corners outside the disc
        assert_eq!(footprint(3).len(), 5);
        assert!(footprint(3).contains(&(-1, 0)) && !footprint(3).contains(&(-1, -1)));
        // a disc, as wide as it says
        let five = footprint(5);
        assert_eq!(five.iter().map(|p| p.0).min(), Some(-2));
        assert_eq!(five.iter().map(|p| p.0).max(), Some(2));
    }

    #[test]
    fn lines_have_a_pixel_a_step() {
        assert_eq!(line((0, 0), (3, 0)), vec![(0, 0), (1, 0), (2, 0), (3, 0)]);
        assert_eq!(line((0, 0), (2, 2)), vec![(0, 0), (1, 1), (2, 2)]);
        assert_eq!(line((3, 1), (0, 0)).len(), 4);
        assert_eq!(line((5, 5), (5, 5)), vec![(5, 5)]);
    }

    #[test]
    fn pixel_perfect_takes_out_l_corners() {
        let mut p = Perfect::default();
        assert_eq!(p.add((0, 0)), (true, None));
        assert_eq!(p.add((1, 0)), (true, None));
        // right then down: the corner pixel (1, 0) goes, the line steps diagonally
        assert_eq!(p.add((1, 1)), (true, Some((1, 0))));
        assert_eq!(p.add((1, 1)), (false, None), "the same pixel again");
        // straight on: nothing taken out
        let mut q = Perfect::default();
        for x in 0..4 {
            assert_eq!(q.add((x, 0)).1, None);
        }
    }

    #[test]
    fn a_stroke_sets_each_pixel_once() {
        let clear = vec![0u8; 4 * 4 * 4];
        let mut s = Stroke::new(4, 4, clear, Ink::Paint([1.0, 0.0, 0.0], 0.5), false, None).unwrap();
        s.set((1, 1));
        s.set((1, 1));
        let (rect, part) = s.take_changed().unwrap();
        assert_eq!(rect, [1, 1, 2, 2]);
        // half red over clear, once: not built up by going over it again
        assert_eq!(part, vec![128, 0, 0, 128]);
        assert!(s.take_changed().is_none());
        // off the canvas: nothing
        s.set((-1, 0));
        s.set((4, 0));
        assert!(s.take_changed().is_none());
        // taken back out, as it was
        s.unset((1, 1));
        assert_eq!(s.take_changed().unwrap().1, vec![0, 0, 0, 0]);
        assert!(!s.any());
    }

    #[test]
    fn masks_harden_at_half_and_strokes_reset() {
        let mut m = vec![0, 127, 128, 255];
        harden(&mut m);
        assert_eq!(m, vec![0, 0, 255, 255]);
        let mut s = Stroke::new(3, 1, vec![0; 12], Ink::Paint([1.0; 3], 1.0), false, None).unwrap();
        s.set((0, 0));
        s.set((2, 0));
        let _ = s.take_changed();
        s.reset();
        let (rect, part) = s.take_changed().unwrap();
        assert_eq!(rect, [0, 0, 3, 1]);
        assert_eq!(part, vec![0; 12]);
        assert!(!s.any());
    }

    #[test]
    fn a_stroke_keeps_to_the_selection_and_the_alpha_lock() {
        let grey = [100u8, 100, 100, 255].repeat(4);
        let mask = vec![255, 0, 255, 100];
        let mut s = Stroke::new(2, 2, grey.clone(), Ink::Paint([1.0, 1.0, 1.0], 1.0), false, Some(mask)).unwrap();
        for p in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            s.set(p);
        }
        // only the selected pixels, (0, 0) and (0, 1), changed; (1, 0) isn't selected and (1, 1)
        // less than half
        let (rect, part) = s.take_changed().unwrap();
        assert_eq!(rect, [0, 0, 1, 2]);
        assert_eq!(part, [255u8, 255, 255, 255].repeat(2));
        // alpha locked over half-clear: the colour changes, the coverage doesn't
        let half = vec![50u8, 50, 50, 128];
        let mut locked = Stroke::new(1, 1, half.clone(), Ink::Paint([1.0, 0.0, 0.0], 1.0), true, None).unwrap();
        locked.set((0, 0));
        assert_eq!(locked.take_changed().unwrap().1, vec![128, 0, 0, 128]);
        let mut erased = Stroke::new(1, 1, half.clone(), Ink::Erase(1.0), false, None).unwrap();
        erased.set((0, 0));
        assert_eq!(erased.take_changed().unwrap().1, vec![0, 0, 0, 0]);
        let mut kept = Stroke::new(1, 1, half.clone(), Ink::Erase(1.0), true, None).unwrap();
        kept.set((0, 0));
        assert_eq!(kept.take_changed().unwrap().1, half, "alpha locked: nothing erased");
    }

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
