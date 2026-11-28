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

/// The most colours a pixel art palette keeps.
pub const MOST_COLOURS: usize = 256;

/// The palette a new pixel art canvas starts with: sixteen colours (0xRRGGBB), dark to light and
/// round the hues, the set many small games are drawn in.
pub const STARTING_PALETTE: [u32; 16] = [
    0x000000, 0x1D2B53, 0x7E2553, 0x008751, 0xAB5236, 0x5F574F, 0xC2C3C7, 0xFFF1E8, 0xFF004D, 0xFFA300, 0xFFEC27,
    0x00E436, 0x29ADFF, 0x83769C, 0xFF77A8, 0xFFCCAA,
];

/// The colours (0xRRGGBB, in order, each once, at most [`MOST_COLOURS`]) written in `text` as hex
/// codes: `#1a2b3c`, `1a2b3c` or `#abc`, separated by anything else, as Lospec and art programs give
/// them (paint.net's `FF1a2b3c` too).
pub fn parse_palette(text: &str) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        let hex = match word.len() {
            3 => word.chars().flat_map(|c| [c, c]).collect::<String>(),
            6 => word.to_owned(),
            8 if word.get(..2).is_some_and(|a| a.eq_ignore_ascii_case("ff")) => {
                word.get(2..).unwrap_or_default().to_owned()
            }
            _ => continue,
        };
        if let Ok(colour) = u32::from_str_radix(&hex, 16)
            && hex.len() == 6
            && !out.contains(&colour)
            && out.len() < MOST_COLOURS
        {
            out.push(colour);
        }
    }
    out
}

/// `colours` each once, in order, at most [`MOST_COLOURS`], every one within 0xRRGGBB.
pub fn tidy_palette(colours: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for &c in colours {
        let c = c & 0xFF_FFFF;
        if !out.contains(&c) && out.len() < MOST_COLOURS {
            out.push(c);
        }
    }
    out
}

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

/// The colours of a straight-alpha RGBA picture's solid pixels (alpha at least half), in the
/// order first met; none if there are more than `most` or none at all.
pub fn colours_of(rgba: &[u8], most: usize) -> Option<Vec<[u8; 3]>> {
    let mut seen = std::collections::HashSet::new();
    let mut colours = Vec::new();
    for px in rgba.as_chunks::<4>().0 {
        let [r, g, b, a] = *px;
        if a >= 128 && seen.insert([r, g, b]) {
            colours.push([r, g, b]);
            if colours.len() > most {
                return None;
            }
        }
    }
    (!colours.is_empty()).then_some(colours)
}

/// Puts a straight-alpha RGBA picture on `colours`: each pixel under half alpha becomes clear,
/// every other the nearest of the colours (by red, green and blue, weighted as the eye weighs
/// them), solid. Nothing changes with no colours.
pub fn snap_to(rgba: &mut [u8], colours: &[[u8; 3]]) {
    if colours.is_empty() {
        return;
    }
    let mut known: std::collections::HashMap<[u8; 3], [u8; 3]> = std::collections::HashMap::new();
    for px in rgba.as_chunks_mut::<4>().0 {
        if px[3] < 128 {
            *px = [0; 4];
            continue;
        }
        let here = [px[0], px[1], px[2]];
        let near = *known.entry(here).or_insert_with(|| nearest(here, colours));
        *px = [near[0], near[1], near[2], 255];
    }
}

/// How far apart two straight-alpha pixels look, 0 to 255: clear against clear is 0, clear
/// against solid 255, otherwise their weighted colour difference.
fn apart(a: [u8; 4], b: [u8; 4]) -> u32 {
    match (a[3] >= 128, b[3] >= 128) {
        (false, false) => 0,
        (true, true) => {
            let [dr, dg, db] =
                [(a[0], b[0]), (a[1], b[1]), (a[2], b[2])].map(|(x, y)| (i32::from(x) - i32::from(y)).unsigned_abs());
            let sum = 2 * dr * dr + 4 * dg * dg + 3 * db * db;
            ((f64::from(sum) / 9.0).sqrt()) as u32
        }
        _ => 255,
    }
}

/// How little a pixel and all eight around it may change for a frame to keep the reference's.
pub const STEADY_WITHIN: u32 = 24;

/// Steadies an animation frame against the picture it was made from (both straight-alpha RGBA,
/// `width × height`): wherever a pixel and every pixel around it barely changed, the reference's
/// pixels are put back, so the parts that don't move stay exactly as drawn and only the moving
/// parts come from the frame. Returns how many pixels were put back.
pub fn steady(frame: &mut [u8], reference: &[u8], width: u32, height: u32) -> usize {
    steady_between(frame, reference, reference, width, height)
}

/// Steadies an in-between animation frame against its `first` and `last` key frames (all
/// straight-alpha RGBA, `width × height`): wherever a pixel and all eight around it barely changed
/// between `first` and `last` and barely changed in `frame`, `first`'s pixel is put back, so parts
/// that stayed still stay crisp while moving parts transition freely. Returns how many pixels were
/// put back.
pub fn steady_between(frame: &mut [u8], first: &[u8], last: &[u8], width: u32, height: u32) -> usize {
    let (w, h) = (width as usize, height as usize);
    if frame.len() != w * h * 4 || first.len() != frame.len() || last.len() != frame.len() {
        return 0;
    }
    let pixel = |rgba: &[u8], i: usize| -> [u8; 4] {
        rgba.get(i * 4..i * 4 + 4).and_then(|p| p.try_into().ok()).unwrap_or([0; 4])
    };
    let changed: Vec<u32> = (0..w * h)
        .map(|i| {
            let f = pixel(first, i);
            apart(pixel(frame, i), f).max(apart(f, pixel(last, i)))
        })
        .collect();
    let mut put_back = 0;
    for y in 0..h {
        for x in 0..w {
            let around = (y.saturating_sub(1)..(y + 2).min(h))
                .flat_map(|ny| (x.saturating_sub(1)..(x + 2).min(w)).map(move |nx| ny * w + nx))
                .map(|i| changed.get(i).copied().unwrap_or(255))
                .max()
                .unwrap_or(255);
            if around < STEADY_WITHIN {
                let i = y * w + x;
                let r = pixel(first, i);
                let back = if r[3] >= 128 { [r[0], r[1], r[2], 255] } else { [0; 4] };
                if let Some(px) = frame.get_mut(i * 4..i * 4 + 4) {
                    px.copy_from_slice(&back);
                    put_back += 1;
                }
            }
        }
    }
    put_back
}

/// The colour of `colours` nearest `c`.
fn nearest(c: [u8; 3], colours: &[[u8; 3]]) -> [u8; 3] {
    let distance = |o: &[u8; 3]| {
        let d = |a: u8, b: u8| i32::from(a) - i32::from(b);
        let (dr, dg, db) = (d(c[0], o[0]), d(c[1], o[1]), d(c[2], o[2]));
        2 * dr * dr + 4 * dg * dg + 3 * db * db
    };
    colours.iter().min_by_key(|o| distance(o)).copied().unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_read_hex_codes_as_lospec_gives_them() {
        assert_eq!(parse_palette("#1a1c2c\n#5d275d, b13e53 ef7d57"), vec![0x1A1C2C, 0x5D275D, 0xB13E53, 0xEF7D57]);
        assert_eq!(parse_palette("#abc #ABC"), vec![0xAABBCC], "short codes, each once");
        assert_eq!(parse_palette("FF1a2b3c ;paint.net"), vec![0x1A2B3C]);
        assert_eq!(parse_palette("not a colour 12345 #12"), Vec::<u32>::new());
        assert_eq!(tidy_palette(&[0xFF_000000 | 0x123456, 0x123456, 0xABCDEF]), vec![0x123456, 0xABCDEF]);
    }

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
    fn a_fill_that_isnt_contiguous_takes_every_pixel_of_the_colour() {
        let (red, blue) = ([255u8, 0, 0, 255], [0u8, 0, 255, 255]);
        let pixels = [red, blue, red].concat();
        let flood = crate::fill::Flood::new(3, 1, &pixels, (0, 0)).unwrap();
        // joined: the blue between stops it
        assert_eq!(flood.mask(0.0).data, vec![255, 0, 0]);
        assert_eq!(flood.everywhere(0.0).data, vec![255, 0, 255]);
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

    #[test]
    fn a_picture_s_colours_are_its_solid_pixels_and_too_many_are_none() {
        let rgba = [10, 20, 30, 255, 10, 20, 30, 255, 1, 2, 3, 0, 200, 0, 0, 128];
        assert_eq!(colours_of(&rgba, 8), Some(vec![[10, 20, 30], [200, 0, 0]]));
        assert_eq!(colours_of(&rgba, 1), None);
        assert_eq!(colours_of(&[0, 0, 0, 0], 8), None);
    }

    #[test]
    fn snapping_puts_each_pixel_on_the_nearest_colour_solid_or_clear() {
        let colours = [[0, 0, 0], [250, 120, 40]];
        let mut rgba = [240, 130, 50, 200, 20, 10, 5, 255, 240, 130, 50, 100];
        snap_to(&mut rgba, &colours);
        assert_eq!(rgba, [250, 120, 40, 255, 0, 0, 0, 255, 0, 0, 0, 0]);
        let mut same = [9, 9, 9, 255];
        snap_to(&mut same, &[]);
        assert_eq!(same, [9, 9, 9, 255]);
    }

    #[test]
    fn a_frame_keeps_the_reference_where_it_barely_changed_and_its_own_where_it_moved() {
        // a 5 × 1 row: the reference grey, the frame a shade off on the left, red on the right
        let grey = [100u8, 100, 100, 255];
        let reference: Vec<u8> = grey.repeat(5);
        let mut frame: Vec<u8> =
            [[104, 98, 101, 255], [104, 98, 101, 255], [104, 98, 101, 255], [220, 0, 0, 255], [220, 0, 0, 255]]
                .concat();
        assert_eq!(steady(&mut frame, &reference, 5, 1), 2);
        assert_eq!(&frame[..8], &[grey, grey].concat()[..]);
        // the third pixel is next to the red: it keeps its own
        assert_eq!(&frame[8..12], &[104, 98, 101, 255]);
        assert_eq!(&frame[12..], &[[220, 0, 0, 255], [220, 0, 0, 255]].concat()[..]);
        // clear stays clear; a size mismatch changes nothing
        let mut clear = [0u8; 4];
        assert_eq!(steady(&mut clear, &[5, 5, 5, 0], 1, 1), 1);
        assert_eq!(clear, [0; 4]);
        assert_eq!(steady(&mut clear, &[0; 8], 2, 1), 0);
    }

    #[test]
    fn steady_between_keeps_shared_still_pixels_and_leaves_moving_ones() {
        // 5 × 1: left two pixels are grey in both first and last; right two move to blue in last
        let grey = [100u8, 100, 100, 255];
        let blue = [0u8, 80, 220, 255];
        let first: Vec<u8> = grey.repeat(5);
        let last: Vec<u8> = [grey, grey, grey, blue, blue].concat();
        let off = [104u8, 98, 101, 255];
        let mut frame: Vec<u8> = off.repeat(5);
        // only pixels 0 and 1 have a still 3×3 neighbourhood across first and last
        assert_eq!(steady_between(&mut frame, &first, &last, 5, 1), 2);
        assert_eq!(&frame[..8], &[grey, grey].concat()[..]);
        assert_eq!(&frame[8..], &[off, off, off].concat()[..]);
    }
}
