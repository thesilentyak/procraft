//! Fills: the region a colour dropped at a point fills, found on a picture of what is drawn, kept
//! out of gaps in the lines up to a width, and its outlines.
//!
//! What is drawn more than the threshold bounds a fill. With gaps closed, the fill first spreads
//! only where it is further than half the gap from anything drawn, so it can't run out through a gap
//! that narrow; then it grows back to the lines (going halfway into a gap, which closes it to the
//! eye), never through them. Last it reaches a pixel under the lines' edges, so drawn behind them
//! it leaves no light seam.
//!
//! It runs in the app's interpreter on pictures up to 2048 pixels a side, where every step taken
//! for each pixel counts, and a loop is charged for all of its body at each turn: so it works on
//! each row's runs of pixels, reading a row eight pixels at a time, closes gaps on pictures of a bit
//! a pixel, 64 at a time, and finds its outlines' corners along each row without hashing them.

/// A picture of what is drawn: how much of each pixel (0..255), rows top to bottom.
pub(crate) struct Drawn<'a> {
    pub width: usize,
    pub height: usize,
    pub coverage: &'a [u8],
}

/// The most half a gap reaches, pixels.
const MOST_REACH: f32 = 40.0;

/// What a pixel of the working picture is, a bit each. Reached from the seed side by side across
/// what isn't drawn:
const PLAIN: u8 = 1;
/// Drawn, beside or corner to corner with a plain pixel:
const EDGE: u8 = 2;
/// With gaps closed, where the fill first spreads and what grows back from there:
const GROWN: u8 = 4;
/// Drawn, under the fill's edge:
const UNDER: u8 = 8;

/// Each byte's lowest bit, and each byte's highest.
const ONES: u64 = 0x0101_0101_0101_0101;
const HIGHS: u64 = 0x8080_8080_8080_8080;

/// The highest bit of each byte of `v` that is more than `n`.
fn more_than(v: u64, n: u8) -> u64 {
    let sum = (v & !HIGHS).wrapping_add(ONES * u64::from(127 - (n & 127)));
    if n < 128 { (sum | v) & HIGHS } else { sum & v & HIGHS }
}

/// The highest bit of each byte of `v` that isn't 0.
fn not_zero(v: u64) -> u64 {
    ((v & !HIGHS).wrapping_add(!HIGHS) | v) & HIGHS
}

/// The runs (each its first x and the one past its last) of the pixels of a row from x `left`,
/// a byte each in `bytes`, that `eight` takes (told eight at once, it answers with the highest bit of
/// each byte it takes) and `one` takes (told one).
///
/// The interpreter charges a loop's every step for all the code in its body, taken or not, so the
/// loop over eight pixels at a time only reads them, and where a run starts or ends goes to
/// [`Opening`], out of it.
fn runs_of(bytes: &[u8], left: usize, eight: impl Fn(u64) -> u64, one: impl Fn(u8) -> bool) -> Vec<(u32, u32)> {
    let mut runs = Opening { out: Vec::new(), from: None };
    let mut x = left as u32;
    // all of the last eight taken (each byte's highest bit), or none
    let mut going = 0;
    let (chunks, rest) = bytes.as_chunks::<8>();
    for chunk in chunks {
        let taken = eight(u64::from_le_bytes(*chunk));
        if taken != going {
            going = runs.eight(taken, x);
        }
        x += 8;
    }
    for &b in rest {
        runs.one(one(b), x);
        x += 1;
    }
    runs.end(x)
}

/// The runs of a row as they are found.
struct Opening {
    out: Vec<(u32, u32)>,
    /// Where the run going on started.
    from: Option<u32>,
}

impl Opening {
    /// Eight pixels from `x`, those taken with the highest bit of their bytes in `taken`: all of
    /// them taken if the last is (each byte's highest bit), or none.
    #[inline(never)]
    fn eight(&mut self, taken: u64, x: u32) -> u64 {
        for i in 0..8 {
            self.one((taken >> (8 * i + 7)) & 1 != 0, x + i);
        }
        if self.from.is_some() { HIGHS } else { 0 }
    }

    /// A word of 64 pixels from `x`, a bit each: all of them taken if the last is (every bit), or
    /// none.
    #[inline(never)]
    fn word(&mut self, v: u64, x: u32) -> u64 {
        let going = u64::from(self.from.is_some());
        // where a run starts or ends
        let mut changes = v ^ ((v << 1) | going);
        while changes != 0 {
            let i = changes.trailing_zeros();
            self.one((v >> i) & 1 != 0, x + i);
            changes &= changes - 1;
        }
        if self.from.is_some() { u64::MAX } else { 0 }
    }

    /// Pixel `x`, taken or not.
    #[inline(never)]
    fn one(&mut self, taken: bool, x: u32) {
        match (taken, self.from) {
            (true, None) => self.from = Some(x),
            (false, Some(a)) => {
                self.out.push((a, x));
                self.from = None;
            }
            _ => {}
        }
    }

    /// The runs, the row ending at `x`.
    fn end(mut self, x: u32) -> Vec<(u32, u32)> {
        if let Some(a) = self.from {
            self.out.push((a, x));
        }
        self.out
    }
}

/// Runs of pixels, row by row: each its row, first x and the one past its last, in order of row
/// and then x; and where each row's runs start among them (one more, past the last row's).
struct Rows {
    runs: Vec<(u32, u32, u32)>,
    starts: Vec<usize>,
}

impl Rows {
    /// The runs `row` gives for each of `h` rows.
    fn new(h: usize, mut row: impl FnMut(usize) -> Vec<(u32, u32)>) -> Rows {
        let mut rows = Rows { runs: Vec::new(), starts: Vec::with_capacity(h + 1) };
        for y in 0..h {
            rows.starts.push(rows.runs.len());
            rows.runs.extend(row(y).into_iter().map(|(a, b)| (y as u32, a, b)));
        }
        rows.starts.push(rows.runs.len());
        rows
    }

    /// The runs of row `y`, and where the first is among them all.
    fn of(&self, y: usize) -> (&[(u32, u32, u32)], usize) {
        let (Some(&from), Some(&to)) = (self.starts.get(y), self.starts.get(y + 1)) else { return (&[], 0) };
        (self.runs.get(from..to).unwrap_or(&[]), from)
    }

    /// The run holding pixel (`x`, `y`), by its place.
    fn at(&self, x: usize, y: usize) -> Option<usize> {
        let (row, from) = self.of(y);
        let i = row.partition_point(|r| r.2 as usize <= x);
        row.get(i).filter(|r| r.1 as usize <= x).map(|_| from + i)
    }

    /// The runs reached from run `start`, each reaching those it overlaps in the rows above and
    /// below, by their places in order.
    fn reached(&self, start: usize) -> Vec<usize> {
        let mut seen = vec![false; self.runs.len()];
        let mut out = Vec::new();
        let mut stack = vec![start];
        if let Some(s) = seen.get_mut(start) {
            *s = true;
        }
        while let Some(i) = stack.pop() {
            out.push(i);
            let Some(&(y, a, b)) = self.runs.get(i) else { continue };
            for ny in [y.checked_sub(1), Some(y + 1)].into_iter().flatten() {
                let (row, from) = self.of(ny as usize);
                let first = row.partition_point(|r| r.2 <= a);
                for (k, r) in row.iter().enumerate().skip(first) {
                    if r.1 >= b {
                        break;
                    }
                    if let Some(s) = seen.get_mut(from + k).filter(|s| !**s) {
                        *s = true;
                        stack.push(from + k);
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }
}

/// The region a fill covers.
pub(crate) struct Region {
    width: usize,
    marks: Vec<u8>,
    /// The marks that put a pixel in it.
    bits: u8,
    /// The box it lies in: left, top, right, bottom, inclusive.
    bounds: [usize; 4],
}

/// Pixel `j` noted as drawn beside the plain region, once.
fn note(marks: &mut [u8], edges: &mut Vec<usize>, j: usize) {
    if let Some(m) = marks.get_mut(j).filter(|m| **m & EDGE == 0) {
        *m |= EDGE;
        edges.push(j);
    }
}

impl Drawn<'_> {
    /// The region a fill from `seed` covers, where what is drawn more than `limit` bounds it and
    /// gaps up to `gap` pixels wide are closed; none if the seed is on something drawn (or off the
    /// picture).
    pub(crate) fn fill(&self, seed: (usize, usize), limit: u8, gap: f32) -> Option<Region> {
        let (w, h) = (self.width, self.height);
        let n = w.checked_mul(h)?;
        let coverage = self.coverage.get(..n)?;
        if seed.0 >= w || seed.1 >= h || coverage[seed.1 * w + seed.0] > limit {
            return None;
        }
        // what isn't drawn, row by row, and of it what is reached from the seed
        let open = Rows::new(h, |y| {
            let row = coverage.get(y * w..(y + 1) * w).unwrap_or(&[]);
            runs_of(row, 0, |v| !more_than(v, limit) & HIGHS, |c| c <= limit)
        });
        let plain = open.reached(open.at(seed.0, seed.1)?);
        let mut marks = vec![0_u8; n];
        let mut bounds = [seed.0, seed.1, seed.0, seed.1];
        for &i in &plain {
            let (y, a, b) = open.runs[i];
            let (y, a, b) = (y as usize, a as usize, b as usize);
            if let Some(run) = marks.get_mut(y * w + a..y * w + b) {
                run.fill(PLAIN);
            }
            bounds = [bounds[0].min(a), bounds[1].min(y), bounds[2].max(b - 1), bounds[3].max(y)];
        }
        // what is drawn beside it: the pixels at each end of its runs, and those of the rows above
        // and below them (corners too) that aren't in those rows' runs
        let mut edges = Vec::new();
        for &i in &plain {
            let (y, a, b) = open.runs[i];
            let (y, a, b) = (y as usize, a as usize, b as usize);
            if a > 0 {
                note(&mut marks, &mut edges, y * w + a - 1);
            }
            if b < w {
                note(&mut marks, &mut edges, y * w + b);
            }
            let (lo, hi) = (a.saturating_sub(1), (b + 1).min(w));
            for ny in [y.checked_sub(1), Some(y + 1).filter(|ny| *ny < h)].into_iter().flatten() {
                let (row, _) = open.of(ny);
                let mut x = lo;
                for r in row.iter().skip(row.partition_point(|r| r.2 as usize <= lo)) {
                    let (c, d) = (r.1 as usize, r.2 as usize);
                    if c >= hi {
                        break;
                    }
                    for g in x..c {
                        note(&mut marks, &mut edges, ny * w + g);
                    }
                    x = x.max(d);
                }
                for g in x..hi {
                    note(&mut marks, &mut edges, ny * w + g);
                }
            }
        }
        let reach = (gap / 2.0).clamp(0.0, MOST_REACH);
        let grown = closed(w, h, seed, &mut marks, &open, &plain, bounds, reach);
        // and a pixel under the lines' edges: each one beside the plain region, or with gaps
        // closed, beside (or corner to corner with) what grew
        let [_, top, _, bottom] = bounds;
        let beside = grown.as_ref().map(|g| g.beside(top.saturating_sub(1)..(bottom + 2).min(h)));
        for &q in &edges {
            if beside.as_ref().is_none_or(|b| b.has(q % w, q / w)) {
                marks[q] |= UNDER;
            }
        }
        let bits = if grown.is_some() { GROWN } else { PLAIN };
        let [left, top, right, bottom] = bounds;
        let bounds = [left.saturating_sub(1), top.saturating_sub(1), (right + 1).min(w - 1), (bottom + 1).min(h - 1)];
        Some(Region { width: w, marks, bits: bits | UNDER, bounds })
    }
}

/// The plain region (marked [`PLAIN`], its runs `plain` among `open`'s, in the box `bounds`) with
/// gaps closed up to `reach` pixels from each side: the pixels further than that from what is drawn,
/// side by side from `seed`, its core; and what grows back from there a pixel more, never through what
/// is drawn, marked [`GROWN`], and answered; none where no gap closes (the plain region is the fill).
#[allow(clippy::too_many_arguments)]
fn closed(
    w: usize,
    h: usize,
    seed: (usize, usize),
    marks: &mut [u8],
    open: &Rows,
    plain: &[usize],
    bounds: [usize; 4],
    reach: f32,
) -> Option<Bits> {
    let steps = reach as usize;
    if steps == 0 {
        // nothing is near enough to close
        return None;
    }
    let [_, top, _, bottom] = bounds;
    // what is drawn near enough the plain region's rows to reach it
    let around = top.saturating_sub(steps)..(bottom + steps + 1).min(h);
    let mut drawn = Bits::new(w, h);
    for y in around.clone() {
        let mut x = 0;
        for &(_, a, b) in open.of(y).0 {
            drawn.set(y, x, a as usize);
            x = b as usize;
        }
        drawn.set(y, x, w);
    }
    let near = drawn.grown(steps, around, None);
    if near.has(seed.0, seed.1) {
        // the seed itself near a line: no gap closes
        return None;
    }
    // the core: the plain region less what is near, side by side from the seed
    let rows = top..bottom + 1;
    let mut region = Bits::new(w, h);
    for &i in plain {
        let (y, a, b) = open.runs[i];
        region.set(y as usize, a as usize, b as usize);
    }
    let far = region.less(&near, rows.clone());
    let core = Rows::new(h, |y| if rows.contains(&y) { far.runs(y, w) } else { Vec::new() });
    let first = core.at(seed.0, seed.1)?;
    let mut kept = Bits::new(w, h);
    for i in core.reached(first) {
        let (y, a, b) = core.runs[i];
        kept.set(y as usize, a as usize, b as usize);
    }
    // grown back a pixel further than the gap's half, kept to the plain region
    let grown = kept.grown(steps + 1, rows.clone(), Some(&region));
    for y in rows {
        for (a, b) in grown.runs(y, w) {
            // (plain pixels, marked nothing else yet)
            if let Some(run) = marks.get_mut(y * w + a as usize..y * w + b as usize) {
                run.fill(PLAIN | GROWN);
            }
        }
    }
    Some(grown)
}

/// A picture's pixels, a bit each: rows of words of 64 pixels, the lowest bit leftmost, with a word
/// of none at each end of a row and a row of none above and below it all, so no pixel's neighbours
/// are off it.
struct Bits {
    stride: usize,
    words: Vec<u64>,
}

/// Of the steps growing a set of pixels in turn, the share that reach corner to corner as well as
/// beside: √2 - 1, so that as many steps reach as far along a diagonal as across.
const CORNER_SHARE: f32 = std::f32::consts::SQRT_2 - 1.0;

impl Bits {
    fn new(w: usize, h: usize) -> Bits {
        let stride = w.div_ceil(64) + 2;
        Bits { stride, words: vec![0; stride * (h + 2)] }
    }

    /// Pixels `a..b` of row `y` set.
    fn set(&mut self, y: usize, a: usize, b: usize) {
        let row = (y + 1) * self.stride + 1;
        let mut x = a;
        while x < b {
            let (word, bit) = (x / 64, x % 64);
            let n = (64 - bit).min(b - x);
            let ones = if n == 64 { u64::MAX } else { ((1_u64 << n) - 1) << bit };
            if let Some(v) = self.words.get_mut(row + word) {
                *v |= ones;
            }
            x += n;
        }
    }

    /// Whether pixel (`x`, `y`) is set.
    fn has(&self, x: usize, y: usize) -> bool {
        self.words.get((y + 1) * self.stride + 1 + x / 64).is_some_and(|v| (v >> (x % 64)) & 1 != 0)
    }

    /// Row `y`'s runs of pixels set, the picture `w` wide.
    fn runs(&self, y: usize, w: usize) -> Vec<(u32, u32)> {
        let row = (y + 1) * self.stride + 1;
        let words = self.words.get(row..row + self.stride - 2).unwrap_or(&[]);
        let mut runs = Opening { out: Vec::new(), from: None };
        let mut going = 0;
        for (j, &v) in words.iter().enumerate() {
            if v != going {
                going = runs.word(v, j as u32 * 64);
            }
        }
        runs.end(w as u32)
    }

    /// These pixels less those of `other`, over rows `rows`.
    fn less(&self, other: &Bits, rows: std::ops::Range<usize>) -> Bits {
        let mut out = Bits { stride: self.stride, words: vec![0; self.words.len()] };
        let span = (rows.start + 1) * self.stride..(rows.end + 1) * self.stride;
        if let (Some(to), Some(a), Some(b)) =
            (out.words.get_mut(span.clone()), self.words.get(span.clone()), other.words.get(span))
        {
            for ((t, a), b) in to.iter_mut().zip(a).zip(b) {
                *t = a & !b;
            }
        }
        out
    }

    /// The pixels beside these or corner to corner with them, and these, over rows `rows`.
    fn beside(&self, rows: std::ops::Range<usize>) -> Bits {
        let mut out = Bits { stride: self.stride, words: vec![0; self.words.len()] };
        grow::<true, false>(self, &mut out, rows, &[]);
        out
    }

    /// Grown by `steps` over rows `rows`: each step a pixel further, beside it or (for a share of
    /// the steps) corner to corner too, kept to `within` where given.
    fn grown(self, steps: usize, rows: std::ops::Range<usize>, within: Option<&Bits>) -> Bits {
        let mut now = self;
        let mut next = Bits { stride: now.stride, words: vec![0; now.words.len()] };
        for i in 0..steps {
            let corners = ((i + 1) as f32 * CORNER_SHARE) as usize > (i as f32 * CORNER_SHARE) as usize;
            match (corners, within) {
                (true, Some(m)) => grow::<true, true>(&now, &mut next, rows.clone(), &m.words),
                (true, None) => grow::<true, false>(&now, &mut next, rows.clone(), &[]),
                (false, Some(m)) => grow::<false, true>(&now, &mut next, rows.clone(), &m.words),
                (false, None) => grow::<false, false>(&now, &mut next, rows.clone(), &[]),
            }
            std::mem::swap(&mut now, &mut next);
        }
        now
    }
}

/// One step of growing `from` into `to` over rows `rows`: each pixel set where it or a pixel beside
/// it is (or corner to corner, with `CORNERS`), kept to `within` with `WITHIN`. Straight through, a
/// word at a time, as the interpreter charges a loop's every step for all of its body.
fn grow<const CORNERS: bool, const WITHIN: bool>(
    from: &Bits,
    to: &mut Bits,
    rows: std::ops::Range<usize>,
    within: &[u64],
) {
    let s = from.stride;
    for y in rows {
        let row = (y + 1) * s;
        let (Some(up), Some(here), Some(down)) =
            (from.words.get(row - s..row), from.words.get(row..row + s), from.words.get(row + s..row + 2 * s))
        else {
            continue;
        };
        let mask = if WITHIN { within.get(row..row + s).unwrap_or(here) } else { here };
        let (Some(out), Some(mask)) = (to.words.get_mut(row + 1..row + s), mask.get(1..)) else { continue };
        let (Some(&[mut ul, mut u]), Some(&[mut cl, mut c]), Some(&[mut dl, mut d])) =
            (up.first_chunk::<2>(), here.first_chunk::<2>(), down.first_chunk::<2>())
        else {
            continue;
        };
        // each word beside the one before it and the one after, in the rows above, here and below,
        // the next of each read as it goes
        let next = up.iter().skip(2).zip(here.iter().skip(2)).zip(down.iter().skip(2));
        for (((&ur, &cr), &dr), (t, &m)) in next.zip(out.iter_mut().zip(mask)) {
            let mut v = c | (c << 1) | (cl >> 63) | (c >> 1) | (cr << 63) | u | d;
            if CORNERS {
                v |= (u << 1) | (ul >> 63) | (u >> 1) | (ur << 63) | (d << 1) | (dl >> 63) | (d >> 1) | (dr << 63);
            }
            if WITHIN {
                v &= m;
            }
            *t = v;
            (ul, u, cl, c, dl, d) = (u, ur, c, cr, d, dr);
        }
    }
}

impl Region {
    /// Whether it covers pixel (`x`, `y`).
    #[cfg(test)]
    pub(crate) fn has(&self, x: usize, y: usize) -> bool {
        x < self.width && self.marks.get(y * self.width + x).is_some_and(|m| m & self.bits != 0)
    }

    /// How many pixels it covers.
    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.marks.iter().filter(|m| **m & self.bits != 0).count()
    }

    /// Its outlines: see [`outlines_of`].
    pub(crate) fn outlines(&self, tolerance: f32) -> Vec<Vec<(f32, f32)>> {
        let [left, top, right, bottom] = self.bounds;
        let (bits, many) = (self.bits, ONES * u64::from(self.bits));
        let rows: Vec<Vec<(u32, u32)>> = (top..=bottom)
            .map(|y| {
                let row = self.marks.get(y * self.width + left..=y * self.width + right).unwrap_or(&[]);
                runs_of(row, left, |v| not_zero(v & many), |m| m & bits != 0)
            })
            .collect();
        outlines_of(top, &rows, tolerance)
    }
}

/// The way from `a` to `b`, each part -1, 0 or 1.
fn way(a: (u32, u32), b: (u32, u32)) -> (i64, i64) {
    ((i64::from(b.0) - i64::from(a.0)).signum(), (i64::from(b.1) - i64::from(a.1)).signum())
}

/// The parts of the run `a..b` that `other`'s runs (sorted, apart) don't cover, each to `out`.
fn uncovered((a, b): (u32, u32), other: &[(u32, u32)], mut out: impl FnMut(u32, u32)) {
    let mut x = a;
    for &(c, d) in other.iter().skip(other.partition_point(|o| o.1 <= a)) {
        if c >= b {
            break;
        }
        if c > x {
            out(x, c);
        }
        x = x.max(d);
    }
    if x < b {
        out(x, b);
    }
}

/// The edges between a region and what isn't it, each from a corner to a corner with the region on
/// its right, kept by the line of corners they start on (line `k` is the top of row `k`), in order
/// along it: each the x it starts at and the corner it goes to (x, line).
struct Edges {
    edges: Vec<(u32, (u32, u32))>,
    starts: Vec<usize>,
}

impl Edges {
    /// The edges of the region whose rows hold `rows` (each row's runs).
    fn of(rows: &[Vec<(u32, u32)>]) -> Edges {
        let lines = rows.len() + 1;
        let mut out = Edges { edges: Vec::new(), starts: Vec::with_capacity(lines + 1) };
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        let none = Vec::new();
        for k in 0..lines {
            out.starts.push(out.edges.len());
            let line = k as u32;
            let (above, below) =
                (k.checked_sub(1).and_then(|k| rows.get(k)).unwrap_or(&none), rows.get(k).unwrap_or(&none));
            // those of the row below the line: along its top where the row above leaves it, and
            // down its right ends
            mine.clear();
            for &(a, b) in below {
                uncovered((a, b), above, |s, e| mine.push((s, (e, line))));
                mine.push((b, (b, line + 1)));
            }
            // those of the row above: up its left ends, and along its bottom where the row below
            // leaves it, leftwards
            theirs.clear();
            for &(a, b) in above {
                theirs.push((a, (a, line - 1)));
                uncovered((a, b), below, |s, e| theirs.push((e, (s, line))));
            }
            // the two in order along the line
            let (mut i, mut j) = (0, 0);
            while let (Some(&m), Some(&t)) = (mine.get(i), theirs.get(j)) {
                if m.0 <= t.0 {
                    out.edges.push(m);
                    i += 1;
                } else {
                    out.edges.push(t);
                    j += 1;
                }
            }
            out.edges.extend(mine.get(i..).unwrap_or(&[]));
            out.edges.extend(theirs.get(j..).unwrap_or(&[]));
        }
        out.starts.push(out.edges.len());
        out
    }

    /// The edge not yet `used` out of corner `at`: where two leave it, the one turning right from
    /// the way it `came`, which keeps two loops touching there apart.
    fn next(&self, used: &[bool], at: (u32, u32), came: (i64, i64)) -> Option<usize> {
        let line = at.1 as usize;
        let (&from, &to) = (self.starts.get(line)?, self.starts.get(line + 1)?);
        let here = self.edges.get(from..to)?;
        let first = from + here.partition_point(|e| e.0 < at.0);
        let right = (-came.1, came.0);
        let mut found = None;
        for e in first..to {
            let Some(&(x, end)) = self.edges.get(e) else { break };
            if x != at.0 {
                break;
            }
            if used.get(e) == Some(&false) {
                if way(at, end) == right {
                    return Some(e);
                }
                found = found.or(Some(e));
            }
        }
        found
    }
}

/// The outlines of the region whose rows from `top` down hold `rows` (each row's runs): closed
/// loops along its pixels' edges, the region on their right, which filled even-odd give it back
/// exactly; each smoothed by as much as `tolerance` pixels.
fn outlines_of(top: usize, rows: &[Vec<(u32, u32)>], tolerance: f32) -> Vec<Vec<(f32, f32)>> {
    let edges = Edges::of(rows);
    let mut used = vec![false; edges.edges.len()];
    let mut loops = Vec::new();
    for line in 0..edges.starts.len().saturating_sub(1) {
        let (Some(&from), Some(&to)) = (edges.starts.get(line), edges.starts.get(line + 1)) else { break };
        for e in from..to {
            let (Some(&(x, end)), Some(false)) = (edges.edges.get(e), used.get(e).copied()) else { continue };
            used[e] = true;
            let start = (x, line as u32);
            let (mut at, mut came) = (end, way(start, end));
            let mut points = vec![start];
            while at != start {
                points.push(at);
                let Some(next) = edges.next(&used, at, came) else { break };
                used[next] = true;
                let Some(&(_, end)) = edges.edges.get(next) else { break };
                came = way(at, end);
                at = end;
            }
            let points: Vec<(i64, i64)> =
                points.iter().map(|&(x, line)| (i64::from(x), top as i64 + i64::from(line))).collect();
            let corners = corners_of(&points);
            if corners.len() >= 3 {
                loops.push(simplified(&corners, tolerance));
            }
        }
    }
    loops
}

/// The outlines of the region `inside` covers (a `w` × `h` picture): see [`outlines_of`].
#[cfg(test)]
pub(crate) fn outlines(w: usize, h: usize, inside: &[bool], tolerance: f32) -> Vec<Vec<(f32, f32)>> {
    let bytes: Vec<u8> = inside.iter().map(|b| u8::from(*b)).collect();
    let rows: Vec<Vec<(u32, u32)>> =
        (0..h).map(|y| runs_of(&bytes[y * w..(y + 1) * w], 0, not_zero, |b| b != 0)).collect();
    outlines_of(0, &rows, tolerance)
}

/// The points of a closed loop where it turns (those along a straight run left out).
fn corners_of(points: &[(i64, i64)]) -> Vec<(f32, f32)> {
    let n = points.len();
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            (b.0 - a.0) * (c.1 - b.1) != (b.1 - a.1) * (c.0 - b.0)
        })
        .map(|i| (points[i].0 as f32, points[i].1 as f32))
        .collect()
}

/// A closed loop with the points it can do without (within `tolerance` of the line past them) left
/// out, a staircase becoming the slope it climbs.
fn simplified(points: &[(f32, f32)], tolerance: f32) -> Vec<(f32, f32)> {
    if points.len() < 8 || tolerance <= 0.0 {
        return points.to_vec();
    }
    // cut at the point furthest from the first, each half simplified
    let first = points[0];
    let far = (1..points.len())
        .max_by(|&a, &b| dist2(points[a], first).total_cmp(&dist2(points[b], first)))
        .unwrap_or(points.len() / 2);
    let mut out = Vec::new();
    keep(&points[..=far], tolerance, &mut out);
    out.pop();
    let mut back: Vec<(f32, f32)> = points[far..].to_vec();
    back.push(first);
    keep(&back, tolerance, &mut out);
    out.pop();
    if out.len() < 3 { points.to_vec() } else { out }
}

fn dist2(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

/// `run`'s first point and those of it standing out from its ends' line by more than `tolerance`
/// (in turn, each half alike), and its last, onto `out`.
fn keep(run: &[(f32, f32)], tolerance: f32, out: &mut Vec<(f32, f32)>) {
    let (Some(&a), Some(&b)) = (run.first(), run.last()) else { return };
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = (dx * dx + dy * dy).sqrt();
    let off = |p: (f32, f32)| {
        if length > 0.0 { ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / length } else { dist2(p, a).sqrt() }
    };
    let worst = (1..run.len().saturating_sub(1)).max_by(|&i, &j| off(run[i]).total_cmp(&off(run[j])));
    match worst {
        Some(i) if off(run[i]) > tolerance => {
            keep(&run[..=i], tolerance, out);
            out.pop();
            keep(&run[i..], tolerance, out);
        }
        _ => {
            out.push(a);
            out.push(b);
        }
    }
}

/// Whether `p` is inside the region `outlines` make, even-odd.
pub(crate) fn inside(outlines: &[Vec<(f32, f32)>], p: (f32, f32)) -> bool {
    let mut odd = false;
    for outline in outlines {
        let n = outline.len();
        for i in 0..n {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                odd = !odd;
            }
        }
    }
    odd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `side` square picture with a square ring drawn `thick` wide from `from` to `to`, a gap
    /// `gap` wide in its top.
    fn ring(side: usize, from: usize, to: usize, thick: usize, gap: usize) -> Vec<u8> {
        let mut out = vec![0u8; side * side];
        let mid = (from + to) / 2;
        for y in from..to {
            for x in from..to {
                let edge = x < from + thick || x >= to - thick || y < from + thick || y >= to - thick;
                let in_gap = y < from + thick && x >= mid - gap / 2 && x < mid - gap / 2 + gap;
                if edge && !in_gap {
                    out[y * side + x] = 255;
                }
            }
        }
        out
    }

    /// Whether a region's outlines give it back exactly, pixel by pixel.
    fn outlined_exactly(region: &Region, side: usize) -> bool {
        let loops = region.outlines(0.0);
        (0..side).all(|y| (0..side).all(|x| inside(&loops, (x as f32 + 0.5, y as f32 + 0.5)) == region.has(x, y)))
    }

    #[test]
    fn a_fill_inside_a_ring_stays_in_it_and_closes_a_narrow_gap_but_not_a_wide_one() {
        let side = 64;
        let closed = ring(side, 10, 50, 3, 0);
        let drawn = Drawn { width: side, height: side, coverage: &closed };
        let inner = drawn.fill((30, 30), 0, 0.0).unwrap();
        // the inside, and a pixel under the ring's inner edge
        assert!(inner.has(30, 30) && inner.has(30, 13) && inner.has(30, 12));
        assert!(!inner.has(30, 11) && !inner.has(5, 5));
        assert_eq!(inner.count(), 36 * 36);
        assert!(outlined_exactly(&inner, side));
        // on the ring: none (a tap there is for the stroke)
        assert!(drawn.fill((10, 30), 0, 0.0).is_none());
        // a gap of 4: through it without closing, kept in with gaps up to 6 closed
        let open = ring(side, 10, 50, 3, 4);
        let drawn = Drawn { width: side, height: side, coverage: &open };
        assert!(drawn.fill((30, 30), 0, 0.0).unwrap().has(2, 2), "runs out");
        let kept = drawn.fill((30, 30), 0, 6.0).unwrap();
        assert!(!kept.has(2, 2) && kept.has(30, 30));
        assert!(kept.has(20, 13), "grown back to the line");
        assert!(kept.has(30, 11) && !kept.has(30, 8), "halfway into the gap");
        assert!(outlined_exactly(&kept, side));
        // a gap wider than that: out
        let wide = ring(side, 10, 50, 3, 12);
        let drawn = Drawn { width: side, height: side, coverage: &wide };
        assert!(drawn.fill((30, 30), 0, 6.0).unwrap().has(2, 2));
        // a threshold lets faint drawing through
        let mut faint = closed.clone();
        faint.iter_mut().for_each(|c| *c /= 4);
        let drawn = Drawn { width: side, height: side, coverage: &faint };
        assert!(drawn.fill((30, 30), 100, 0.0).unwrap().has(2, 2));
        // dropped near a line, closer than half the gap: nothing closes, the fill is as without
        let near = Drawn { width: side, height: side, coverage: &open }.fill((30, 14), 0, 6.0).unwrap();
        assert!(near.has(2, 2), "runs out");
    }

    #[test]
    fn a_fill_round_strokes_holds_a_hole_for_each_and_doesn_t_leak_through_a_crack_between_corners() {
        // a 1 pixel line drawn corner to corner, splitting the picture: a fill on one side stays
        // there, gaps closed or not
        let side = 40;
        let mut line = vec![0u8; side * side];
        for i in 0..side {
            line[i * side + i] = 255;
        }
        let drawn = Drawn { width: side, height: side, coverage: &line };
        for gap in [0.0, 6.0] {
            let below = drawn.fill((5, 30), 0, gap).unwrap();
            assert!(below.has(5, 30) && !below.has(30, 5), "{gap}");
            assert!(below.has(20, 20), "under the line's edge");
        }
        // a fill round 30 small strokes: an outline round the picture and one round each
        let side = 200;
        let mut dots = vec![0u8; side * side];
        for k in 0..30 {
            let (x, y) = (10 + (k % 6) * 30, 10 + (k / 6) * 36);
            for yy in y..y + 3 {
                dots[yy * side + x..yy * side + x + 8].fill(255);
            }
        }
        let drawn = Drawn { width: side, height: side, coverage: &dots };
        let round = drawn.fill((0, 0), 0, 6.0).unwrap();
        assert_eq!(round.outlines(0.0).len(), 31);
        assert!(outlined_exactly(&round, side));
    }

    #[test]
    fn a_region_s_outlines_fill_back_to_it_holes_and_all() {
        let side = 32;
        // a ring: an outline round it and one round its hole
        let r = ring(side, 4, 28, 5, 0);
        let mask: Vec<bool> = r.iter().map(|c| *c > 0).collect();
        let loops = outlines(side, side, &mask, 0.0);
        assert_eq!(loops.len(), 2);
        for y in 0..side {
            for x in 0..side {
                let p = (x as f32 + 0.5, y as f32 + 0.5);
                assert_eq!(inside(&loops, p), mask[y * side + x], "at {x}, {y}");
            }
        }
        // a staircase smoothed into its slope: far fewer points, still round the same pixels
        let mut disc = vec![false; side * side];
        for y in 0..side {
            for x in 0..side {
                disc[y * side + x] = (x as f32 - 15.5).hypot(y as f32 - 15.5) < 12.0;
            }
        }
        let exact = outlines(side, side, &disc, 0.0);
        let smooth = outlines(side, side, &disc, 0.75);
        assert!(smooth[0].len() * 2 < exact[0].len(), "{} and {}", smooth[0].len(), exact[0].len());
        assert!(inside(&smooth, (15.5, 15.5)) && !inside(&smooth, (1.0, 1.0)));
        // two squares touching at a corner: two outlines
        let mut two = vec![false; 16];
        two[0] = true;
        two[5] = true;
        assert_eq!(outlines(4, 4, &two, 0.0).len(), 2);
        // and the other way round
        let mut other = vec![false; 16];
        other[1] = true;
        other[4] = true;
        assert_eq!(outlines(4, 4, &other, 0.0).len(), 2);
    }
}
