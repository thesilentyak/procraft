//! Reading a sprite sheet picture into clean, lined-up animation frames: the background taken away,
//! each sprite found, and on pixel art each one made real pixels on one shared set of colours. The
//! steps are those of `docs/pixel-art/05-sprite-import-design.md` §F3; no GPU and no UI.

use std::collections::VecDeque;

/// How a sheet is read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    /// The artwork is pixel art: frames become real pixels on one shared set of colours.
    pub pixel_art: bool,
    /// Frames across and down, to cut the sheet into equal cells instead of by the gaps.
    pub grid: Option<(u32, u32)>,
    /// The background's colour, instead of the colours found round the picture's edge.
    pub background: Option<[u8; 3]>,
}

/// A frame's place in the sheet picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A sheet read: where each frame was found, and the frames themselves, every one the same size
/// and lined up, as straight-alpha RGBA.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// Each frame's place in the picture, in reading order.
    pub places: Vec<Place>,
    /// The background colour taken away (the commonest, when several were), none when the picture
    /// had its own transparency.
    pub background: Option<[u8; 3]>,
    /// Picture pixels to one art pixel (1 when the sheet is already real pixels, or not pixel art).
    pub pixel_size: f32,
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Vec<u8>>,
}

/// The most colours a sheet's frames are brought to on pixel art.
pub const MOST_COLOURS: usize = 32;
/// The most frames read from one sheet.
pub const MOST_FRAMES: usize = 64;
/// How near (summed over red, green and blue) a colour must be to the background's to count as it.
const BACKGROUND_WITHIN: i32 = 60;
/// How much of the picture's edge a colour must make up to be a background colour.
const BACKGROUND_SHARE: f32 = 0.1;
/// An enclosed background-coloured patch is taken away only from this many art pixels up.
const ENCLOSED_FROM: f32 = 24.0;
/// A piece this much smaller than the middle-sized piece joins the nearest frame.
const PIECE_SHARE: f32 = 0.15;
/// The least share of a blown-up sheet's pixels a colour must cover to stay one of its own.
const RARE_SHARE: f64 = 0.004;
/// How near (as `apart` measures, 0 to 255) colours must be to merge into one.
const COLOURS_WITHIN: u32 = 14;
/// The lowest edge rhythm (see `rhythm`) that counts as blown-up pixels.
const RHYTHM_FROM: f32 = 0.2;
/// The least share of the edges a blown-up picture's grid catches: real pixels have edges between
/// any two pixels, so no grid catches most of them.
const CAUGHT_FROM: f32 = 0.45;

/// Reads a straight-alpha RGBA picture as a sprite sheet. None when nothing solid is found.
pub fn read(rgba: &[u8], width: u32, height: u32, options: &Options) -> Option<Sheet> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() != w * h * 4 {
        return None;
    }
    let picture = Picture { rgba, w, h };
    let transparent = picture.transparent_share() > 0.05 && options.background.is_none();
    let backgrounds = if transparent {
        Vec::new()
    } else {
        options.background.map(|c| vec![c]).unwrap_or_else(|| picture.edge_colours())
    };
    let mut solid = if transparent {
        Mask::from_fn(w, h, |x, y| picture.alpha(x, y) >= 128)
    } else {
        picture.without_outer_background(&backgrounds)
    };
    drop_specks(&mut solid, 4);
    let find = |solid: &Mask| match options.grid {
        Some((across, down)) => cells(solid, across, down),
        None => pieces(solid),
    };
    let pixel_size = if options.pixel_art { picture.pixel_size(&solid, &find(&solid)) } else { 1.0 };
    if !transparent {
        picture.take_enclosed_background(&mut solid, &backgrounds, ENCLOSED_FROM * pixel_size * pixel_size);
    }
    if pixel_size >= 4.0 {
        // Lines a pixel or two thin can't be art drawn this big: a soft edge between two
        // background colours, say, that would tie a row of sprites together.
        solid = opened(&solid);
    }
    if pixel_size > 1.0 {
        drop_specks(&mut solid, (2.0 * pixel_size).powi(2) as usize);
    }
    let places = find(&solid);
    if places.is_empty() {
        return None;
    }
    let real = pixel_size >= 1.5;
    let mut frames: Vec<Art> = places
        .iter()
        .map(|place| if real { picture.art_pixels(&solid, *place, pixel_size) } else { picture.cut(&solid, *place) })
        .collect();
    if options.pixel_art {
        let colours = shared_colours(&frames);
        for frame in &mut frames {
            crate::pixel::snap_to(&mut frame.rgba, &colours);
        }
    }
    let keep_cells = options.grid.is_some() && !real;
    let (fw, fh, mut lined) = if keep_cells { in_cells(&frames) } else { line_up(&frames) };
    if options.pixel_art
        && let Some((first, rest)) = lined.split_first_mut()
    {
        for frame in rest {
            crate::pixel::steady(frame, first, fw as u32, fh as u32);
        }
    }
    lined.truncate(MOST_FRAMES);
    Some(Sheet {
        places: places.into_iter().take(MOST_FRAMES).collect(),
        background: backgrounds.first().copied(),
        pixel_size: if real { pixel_size } else { 1.0 },
        width: fw as u32,
        height: fh as u32,
        frames: lined,
    })
}

/// The file's name made an animation's: its stem, unless that's only digits (a phone's own name
/// for a download), then "imported".
pub fn name_from_file(file_name: &str) -> String {
    let stem = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let stem = stem.rsplit_once('.').map_or(stem, |(s, _)| s);
    let words: String = stem.chars().map(|c| if c == '_' || c == '-' { ' ' } else { c }).collect();
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().any(char::is_alphabetic) {
        words.chars().take(crate::animation::MOST_TAG_NAME).collect()
    } else {
        "imported".to_owned()
    }
}

struct Picture<'a> {
    rgba: &'a [u8],
    w: usize,
    h: usize,
}

impl Picture<'_> {
    fn px(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        self.rgba.get(i..i + 4).and_then(|p| p.try_into().ok()).unwrap_or([0; 4])
    }

    fn rgb(&self, x: usize, y: usize) -> [i32; 3] {
        let [r, g, b, _] = self.px(x, y);
        [i32::from(r), i32::from(g), i32::from(b)]
    }

    fn alpha(&self, x: usize, y: usize) -> u8 {
        self.px(x, y)[3]
    }

    fn transparent_share(&self) -> f32 {
        let clear = self.rgba.as_chunks::<4>().0.iter().filter(|p| p[3] < 128).count();
        clear as f32 / (self.w * self.h) as f32
    }

    fn edge(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let (w, h) = (self.w, self.h);
        (0..w)
            .flat_map(move |x| [(x, 0), (x, h - 1)])
            .chain((1..h.saturating_sub(1)).flat_map(move |y| [(0, y), (w - 1, y)]))
    }

    /// The colours making up at least a tenth of the picture's edge, commonest first.
    fn edge_colours(&self) -> Vec<[u8; 3]> {
        let mut groups: Vec<([i64; 3], usize)> = Vec::new();
        let mut total = 0;
        for (x, y) in self.edge() {
            let c = self.rgb(x, y);
            total += 1;
            let near = groups.iter_mut().find(|(sum, n)| {
                let n = *n as i64;
                (0..3)
                    .map(|i| (sum.get(i).copied().unwrap_or(0) / n - i64::from(c.get(i).copied().unwrap_or(0))).abs())
                    .sum::<i64>()
                    < i64::from(BACKGROUND_WITHIN) / 2
            });
            match near {
                Some((sum, n)) => {
                    for (s, v) in sum.iter_mut().zip(c) {
                        *s += i64::from(v);
                    }
                    *n += 1;
                }
                None => groups.push(([i64::from(c[0]), i64::from(c[1]), i64::from(c[2])], 1)),
            }
        }
        groups.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        groups
            .into_iter()
            .filter(|(_, n)| *n as f32 >= BACKGROUND_SHARE * total as f32)
            .map(|(sum, n)| sum.map(|s| (s / n as i64).clamp(0, 255) as u8))
            .collect()
    }

    fn is_background(&self, x: usize, y: usize, backgrounds: &[[u8; 3]]) -> bool {
        let c = self.rgb(x, y);
        self.alpha(x, y) < 128
            || backgrounds.iter().any(|b| {
                (c[0] - i32::from(b[0])).abs() + (c[1] - i32::from(b[1])).abs() + (c[2] - i32::from(b[2])).abs()
                    < BACKGROUND_WITHIN
            })
    }

    /// Solid wherever the background colours don't reach from the picture's edge.
    fn without_outer_background(&self, backgrounds: &[[u8; 3]]) -> Mask {
        let (w, h) = (self.w, self.h);
        let near = Mask::from_fn(w, h, |x, y| self.is_background(x, y, backgrounds));
        let mut outer = Mask::new(w, h);
        let mut queue = VecDeque::new();
        for (x, y) in self.edge() {
            if near.get(x, y) && !outer.get(x, y) {
                outer.set(x, y, true);
                queue.push_back((x, y));
            }
        }
        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in neighbours4(x, y, w, h) {
                if near.get(nx, ny) && !outer.get(nx, ny) {
                    outer.set(nx, ny, true);
                    queue.push_back((nx, ny));
                }
            }
        }
        Mask::from_fn(w, h, |x, y| !outer.get(x, y))
    }

    /// Takes away enclosed background-coloured patches of at least `from` picture pixels.
    fn take_enclosed_background(&self, solid: &mut Mask, backgrounds: &[[u8; 3]], from: f32) {
        let (w, h) = (self.w, self.h);
        let near = Mask::from_fn(w, h, |x, y| solid.get(x, y) && self.is_background(x, y, backgrounds));
        for patch in regions(&near, false) {
            if patch.len() as f32 >= from {
                for (x, y) in patch {
                    solid.set(x, y, false);
                }
            }
        }
    }

    /// Picture pixels to one art pixel: the middle of the sizes found in the largest frames (an AI
    /// picture's grid drifts across a whole sheet), or 1 when fewer than half have one.
    fn pixel_size(&self, solid: &Mask, places: &[Place]) -> f32 {
        let mut biggest: Vec<&Place> = places.iter().collect();
        biggest.sort_by_key(|p| std::cmp::Reverse(p.width * p.height));
        biggest.truncate(8);
        let tried = biggest.len();
        let mut sizes: Vec<f32> = biggest.into_iter().filter_map(|p| self.frame_pixel_size(solid, *p)).collect();
        if sizes.is_empty() || sizes.len() * 2 < tried {
            return 1.0;
        }
        sizes.sort_by(f32::total_cmp);
        sizes.get(sizes.len() / 2).copied().unwrap_or(1.0)
    }

    /// One frame's pixel size, from the rhythm of its colour edges; none when they have no rhythm
    /// (real pixels, or a painting).
    fn frame_pixel_size(&self, solid: &Mask, place: Place) -> Option<f32> {
        let (x0, y0) = (place.x as usize, place.y as usize);
        let (x1, y1) = (x0 + place.width as usize, y0 + place.height as usize);
        let inside = |x: usize, y: usize| x >= x0 && x < x1 && y >= y0 && y < y1 && solid.get(x, y);
        let (raw_across, raw_down) = self.edge_profiles(&inside, x0, y0, x1, y1);
        let (across, down) = (peaks(&raw_across), peaks(&raw_down));
        // At least six art pixels each way, so a few edges can't fit a grid by chance.
        let most = (place.width.min(place.height) as usize / 6).clamp(2, 64) as f32;
        // Small sizes are read on the edges as they are: gathering onto peaks would merge a real
        // pixel's two edges into one.
        let profiles = |p: f32| if p < 4.0 { (&raw_across, &raw_down) } else { (&across, &down) };
        let score = |p: f32| {
            let (a, d) = profiles(p);
            (rhythm(a, p).0 + rhythm(d, p).0) / 2.0
        };
        let mut best = (1.0_f32, f32::MIN);
        let mut p = 2.0;
        while p <= most {
            let s = score(p);
            if s > best.1 {
                best = (p, s);
            }
            p += 0.05;
        }
        let mut size = refine(best.0, 0.06, &score);
        // An exported sheet is blown up by a whole number: take it when it fits as well.
        let whole = size.round();
        if (whole - size).abs() <= 0.15 && score(whole) >= 0.98 * score(size) {
            size = whole;
        }
        let (a, d) = profiles(size);
        let caught = (fit(a, size).2 + fit(d, size).2) / 2.0;
        // Below 3 only an exact blow-up (every edge on the grid) is believed: real pixels catch a
        // small grid's lines often enough by chance.
        let least = if size < 3.0 { 0.9 } else { CAUGHT_FROM };
        (score(size) >= RHYTHM_FROM && caught >= least).then_some(size)
    }

    /// Colour-edge strength between neighbouring picture pixels in a box, summed down each column
    /// boundary and across each row boundary, counting only edges next to something solid.
    fn edge_profiles(
        &self,
        solid: &dyn Fn(usize, usize) -> bool,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        let mut across = vec![0.0_f32; x1.saturating_sub(x0)];
        let mut down = vec![0.0_f32; y1.saturating_sub(y0)];
        let diff = |a: [i32; 3], b: [i32; 3]| ((a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()) as f32;
        for y in y0..y1 {
            for x in x0..x1 {
                let here = self.rgb(x, y);
                let solid_here = solid(x, y);
                if x + 1 < x1
                    && (solid_here || solid(x + 1, y))
                    && let Some(v) = across.get_mut(x - x0)
                {
                    *v += diff(here, self.rgb(x + 1, y));
                }
                if y + 1 < y1
                    && (solid_here || solid(x, y + 1))
                    && let Some(v) = down.get_mut(y - y0)
                {
                    *v += diff(here, self.rgb(x, y + 1));
                }
            }
        }
        (across, down)
    }

    /// A frame's art pixels: its place cut into blocks of `size` picture pixels on the frame's own
    /// grid offset, each block the middle colour of its solid middle.
    fn art_pixels(&self, solid: &Mask, place: Place, size: f32) -> Art {
        let pad = size.ceil() as usize;
        let (px, py, pw, ph) = (place.x as usize, place.y as usize, place.width as usize, place.height as usize);
        let (x0, y0) = (px.saturating_sub(pad), py.saturating_sub(pad));
        let (x1, y1) = ((px + pw + pad).min(self.w), (py + ph + pad).min(self.h));
        let inside = |x: usize, y: usize| x >= px && x < px + pw && y >= py && y < py + ph && solid.get(x, y);
        let (across, down) = self.edge_profiles(&inside, x0, y0, x1, y1);
        let (across, down) = (peaks(&across), peaks(&down));
        let (ox, oy) = (x0 as f32 + rhythm(&across, size).1 + 1.0, y0 as f32 + rhythm(&down, size).1 + 1.0);
        let start_x = ox - ((ox - x0 as f32) / size).ceil() * size;
        let start_y = oy - ((oy - y0 as f32) / size).ceil() * size;
        let aw = (((x1 as f32 - start_x) / size).ceil() as usize).max(1);
        let ah = (((y1 as f32 - start_y) / size).ceil() as usize).max(1);
        let mut art = Art::new(aw, ah);
        for j in 0..ah {
            for i in 0..aw {
                let (bx, by) = (start_x + i as f32 * size, start_y + j as f32 * size);
                let (sx0, sx1) = ((bx + 0.3 * size).round(), (bx + 0.7 * size).round());
                let (sy0, sy1) = ((by + 0.3 * size).round(), (by + 0.7 * size).round());
                let mut colours: [Vec<u8>; 3] = Default::default();
                let mut samples = 0;
                let mut y = sy0;
                while y <= sy1 {
                    let mut x = sx0;
                    while x <= sx1 {
                        if x >= 0.0 && y >= 0.0 {
                            samples += 1;
                            let (xu, yu) = (x as usize, y as usize);
                            if xu < self.w && yu < self.h && inside(xu, yu) {
                                let [r, g, b, _] = self.px(xu, yu);
                                for (list, v) in colours.iter_mut().zip([r, g, b]) {
                                    list.push(v);
                                }
                            }
                        }
                        x += 1.0;
                    }
                    y += 1.0;
                }
                let found = colours[0].len();
                if samples > 0 && found * 2 > samples {
                    let mid = colours.map(|mut list| {
                        list.sort_unstable();
                        list.get(list.len() / 2).copied().unwrap_or(0)
                    });
                    art.set(i, j, [mid[0], mid[1], mid[2], 255]);
                }
            }
        }
        art.trimmed()
    }

    /// A frame's own picture pixels, the background taken away.
    fn cut(&self, solid: &Mask, place: Place) -> Art {
        let (px, py) = (place.x as usize, place.y as usize);
        let mut art = Art::new(place.width as usize, place.height as usize);
        for j in 0..art.h {
            for i in 0..art.w {
                let (x, y) = (px + i, py + j);
                if solid.get(x, y) {
                    let [r, g, b, a] = self.px(x, y);
                    art.set(i, j, [r, g, b, a.max(1)]);
                }
            }
        }
        art
    }
}

/// How well a grid of spacing `size` explains a profile's edges (gathered by `peaks`): the share
/// of all edge strength on the grid's lines, less the share of places those lines cover (what
/// any grid that dense would catch by chance), for the best offset; and that offset. The true
/// size scores highest: a whole multiple catches only some edges, a fraction covers more places.
fn rhythm(profile: &[f32], size: f32) -> (f32, f32) {
    let (score, offset, _) = fit(profile, size);
    (score, offset)
}

/// `rhythm`'s score and offset, and the share of edge strength the grid's lines catch.
fn fit(profile: &[f32], size: f32) -> (f32, f32, f32) {
    let n = profile.len();
    let total = profile.iter().sum::<f32>();
    if total <= 0.0 || size < 1.0 || (n as f32) < size * 2.0 {
        return (0.0, 0.0, 0.0);
    }
    let reach = 0_isize;
    let mut best = (f32::MIN, 0.0_f32, 0.0_f32);
    let mut offset = 0.0;
    while offset < size {
        let (mut caught, mut covered) = (0.0, 0);
        let mut at = offset;
        let mut last = None;
        while (at.round() as usize) < n {
            let line = at.round() as isize;
            for i in line - reach..=line + reach {
                if i >= 0 && last.is_none_or(|l| i > l) {
                    if let Some(v) = profile.get(i as usize) {
                        caught += v;
                        covered += 1;
                    }
                    last = Some(i);
                }
            }
            at += size;
        }
        let score = caught / total - covered as f32 / n as f32;
        if score > best.0 {
            best = (score, offset, caught / total);
        }
        offset += 0.25;
    }
    best
}

/// A profile's strength gathered onto its peaks: each place's moved to the strongest of it and its
/// two neighbours, so an edge a JPEG blurred over a few pixels counts at one.
fn peaks(profile: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0; profile.len()];
    for (i, v) in profile.iter().enumerate() {
        let at = |j: usize| profile.get(j).copied().unwrap_or(f32::MIN);
        let mut top = i;
        for j in [i.wrapping_sub(1), i + 1] {
            if j < profile.len() && at(j) > at(top) {
                top = j;
            }
        }
        if let Some(o) = out.get_mut(top) {
            *o += v;
        }
    }
    out
}

/// The best-scoring size within `span` of `around`, by small steps.
fn refine(around: f32, span: f32, score: &impl Fn(f32) -> f32) -> f32 {
    let mut best = (around, score(around));
    let mut p = around - span;
    while p <= around + span {
        let s = score(p);
        if s > best.1 {
            best = (p, s);
        }
        p += 0.005;
    }
    best.0
}

/// A yes or no for every picture pixel.
#[derive(Clone)]
struct Mask {
    w: usize,
    h: usize,
    data: Vec<bool>,
}

impl Mask {
    fn new(w: usize, h: usize) -> Mask {
        Mask { w, h, data: vec![false; w * h] }
    }

    fn from_fn(w: usize, h: usize, f: impl Fn(usize, usize) -> bool) -> Mask {
        Mask { w, h, data: (0..w * h).map(|i| f(i % w, i / w)).collect() }
    }

    fn get(&self, x: usize, y: usize) -> bool {
        x < self.w && y < self.h && self.data.get(y * self.w + x).copied().unwrap_or(false)
    }

    fn set(&mut self, x: usize, y: usize, v: bool) {
        if x < self.w
            && y < self.h
            && let Some(d) = self.data.get_mut(y * self.w + x)
        {
            *d = v;
        }
    }
}

fn neighbours4(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)]
        .into_iter()
        .filter(move |&(nx, ny)| nx < w && ny < h)
}

fn neighbours8(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    (-1_isize..=1)
        .flat_map(|dy| (-1_isize..=1).map(move |dx| (dx, dy)))
        .filter(|&d| d != (0, 0))
        .map(move |(dx, dy)| (x.wrapping_add_signed(dx), y.wrapping_add_signed(dy)))
        .filter(move |&(nx, ny)| nx < w && ny < h)
}

/// The mask's connected areas (by the eight neighbours, or the four), each its pixels.
fn regions(mask: &Mask, eight: bool) -> Vec<Vec<(usize, usize)>> {
    let (w, h) = (mask.w, mask.h);
    let mut seen = Mask::new(w, h);
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if !mask.get(x, y) || seen.get(x, y) {
                continue;
            }
            seen.set(x, y, true);
            let mut region = vec![(x, y)];
            let mut queue = VecDeque::from([(x, y)]);
            while let Some((cx, cy)) = queue.pop_front() {
                let next: Vec<_> =
                    if eight { neighbours8(cx, cy, w, h).collect() } else { neighbours4(cx, cy, w, h).collect() };
                for (nx, ny) in next {
                    if mask.get(nx, ny) && !seen.get(nx, ny) {
                        seen.set(nx, ny, true);
                        region.push((nx, ny));
                        queue.push_back((nx, ny));
                    }
                }
            }
            out.push(region);
        }
    }
    out
}

/// The mask without its thinnest parts: each solid pixel kept only if it lies in a fully solid 3 × 3
/// square.
fn opened(solid: &Mask) -> Mask {
    let (w, h) = (solid.w, solid.h);
    let full =
        |x: usize, y: usize| x > 0 && y > 0 && (y - 1..=y + 1).all(|ny| (x - 1..=x + 1).all(|nx| solid.get(nx, ny)));
    let core = Mask::from_fn(w, h, full);
    Mask::from_fn(w, h, |x, y| {
        solid.get(x, y)
            && (y.saturating_sub(1)..=y + 1).any(|ny| (x.saturating_sub(1)..=x + 1).any(|nx| core.get(nx, ny)))
    })
}

/// Takes away solid specks smaller than `below` pixels.
fn drop_specks(solid: &mut Mask, below: usize) {
    for region in regions(solid, true) {
        if region.len() < below {
            for (x, y) in region {
                solid.set(x, y, false);
            }
        }
    }
}

/// The frames found by the gaps: bands of rows split by empty rows, each split by empty columns,
/// small pieces joined to the nearest, in reading order.
fn pieces(solid: &Mask) -> Vec<Place> {
    let (w, h) = (solid.w, solid.h);
    let runs = |filled: &dyn Fn(usize) -> bool, n: usize| -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = None;
        for i in 0..=n {
            match (i < n && filled(i), start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    out.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        out
    };
    let mut found: Vec<(Place, usize)> = Vec::new();
    for (y0, y1) in runs(&|y| (0..w).any(|x| solid.get(x, y)), h) {
        for (x0, x1) in runs(&|x| (y0..y1).any(|y| solid.get(x, y)), w) {
            let place = tight(solid, x0, y0, x1, y1);
            let area = (place.y as usize..(place.y + place.height) as usize)
                .flat_map(|y| (place.x as usize..(place.x + place.width) as usize).map(move |x| (x, y)))
                .filter(|&(x, y)| solid.get(x, y))
                .count();
            found.push((place, area));
        }
    }
    let mut areas: Vec<usize> = found.iter().map(|(_, a)| *a).collect();
    areas.sort_unstable();
    let middle = areas.get(areas.len() / 2).copied().unwrap_or(0) as f32;
    let (mut big, small): (Vec<_>, Vec<_>) = found.into_iter().partition(|(_, a)| *a as f32 >= PIECE_SHARE * middle);
    for (piece, _) in small {
        let centre = |p: &Place| (p.x as f32 + p.width as f32 / 2.0, p.y as f32 + p.height as f32 / 2.0);
        let (cx, cy) = centre(&piece);
        let nearest = big.iter_mut().min_by(|(a, _), (b, _)| {
            let d = |p: &Place| {
                let (px, py) = centre(p);
                (px - cx).powi(2) + (py - cy).powi(2)
            };
            d(a).total_cmp(&d(b))
        });
        if let Some((place, _)) = nearest {
            *place = joined(*place, piece);
        }
    }
    let mut places: Vec<Place> = big.into_iter().map(|(p, _)| p).collect();
    // Reading order: by band (pieces overlapping in height share one), then left to right.
    places.sort_by_key(|p| (p.y, p.x));
    let mut ordered: Vec<Vec<Place>> = Vec::new();
    for p in places {
        match ordered.last_mut() {
            Some(row) if row.iter().any(|q| p.y < q.y + q.height && q.y < p.y + p.height) => row.push(p),
            _ => ordered.push(vec![p]),
        }
    }
    ordered
        .into_iter()
        .flat_map(|mut row| {
            row.sort_by_key(|p| p.x);
            row
        })
        .collect()
}

/// The frames found by equal cells: each cell's solid part, empty cells left out.
fn cells(solid: &Mask, across: u32, down: u32) -> Vec<Place> {
    let (across, down) = (across.max(1) as usize, down.max(1) as usize);
    let (cw, ch) = (solid.w / across, solid.h / down);
    if cw == 0 || ch == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for j in 0..down {
        for i in 0..across {
            let (x0, y0) = (i * cw, j * ch);
            let place = tight(solid, x0, y0, x0 + cw, y0 + ch);
            if place.width > 0 {
                // A cell keeps its whole size, so each frame keeps its place within it.
                out.push(Place { x: x0 as u32, y: y0 as u32, width: cw as u32, height: ch as u32 });
            }
        }
    }
    out
}

/// The smallest box round the solid pixels in a box, empty when there are none.
fn tight(solid: &Mask, x0: usize, y0: usize, x1: usize, y1: usize) -> Place {
    let (mut lx, mut ly, mut hx, mut hy) = (usize::MAX, usize::MAX, 0, 0);
    for y in y0..y1 {
        for x in x0..x1 {
            if solid.get(x, y) {
                lx = lx.min(x);
                ly = ly.min(y);
                hx = hx.max(x + 1);
                hy = hy.max(y + 1);
            }
        }
    }
    if lx == usize::MAX {
        return Place { x: x0 as u32, y: y0 as u32, width: 0, height: 0 };
    }
    Place { x: lx as u32, y: ly as u32, width: (hx - lx) as u32, height: (hy - ly) as u32 }
}

fn joined(a: Place, b: Place) -> Place {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = ((a.x + a.width).max(b.x + b.width), (a.y + a.height).max(b.y + b.height));
    Place { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
}

/// A frame being made: straight-alpha RGBA, `w × h`.
#[derive(Clone)]
struct Art {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

impl Art {
    fn new(w: usize, h: usize) -> Art {
        Art { w, h, rgba: vec![0; w * h * 4] }
    }

    fn get(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.w + x) * 4;
        if x >= self.w || y >= self.h {
            return [0; 4];
        }
        self.rgba.get(i..i + 4).and_then(|p| p.try_into().ok()).unwrap_or([0; 4])
    }

    fn set(&mut self, x: usize, y: usize, v: [u8; 4]) {
        let i = (y * self.w + x) * 4;
        if x < self.w
            && y < self.h
            && let Some(p) = self.rgba.get_mut(i..i + 4)
        {
            p.copy_from_slice(&v);
        }
    }

    fn solid(&self, x: usize, y: usize) -> bool {
        self.get(x, y)[3] > 0
    }

    /// The solid pixels' bounds: (left, top, right, bottom), right and bottom past the last.
    fn bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let mut b: Option<(usize, usize, usize, usize)> = None;
        for y in 0..self.h {
            for x in 0..self.w {
                if self.solid(x, y) {
                    b = Some(match b {
                        None => (x, y, x + 1, y + 1),
                        Some((l, t, r, bt)) => (l.min(x), t.min(y), r.max(x + 1), bt.max(y + 1)),
                    });
                }
            }
        }
        b
    }

    fn trimmed(&self) -> Art {
        let Some((l, t, r, b)) = self.bounds() else { return Art::new(1, 1) };
        let mut out = Art::new(r - l, b - t);
        for y in 0..out.h {
            for x in 0..out.w {
                out.set(x, y, self.get(l + x, t + y));
            }
        }
        out
    }

    /// Where it stands: the middle of its bottom few rows' solid pixels, and its bottom.
    fn feet(&self) -> (usize, usize) {
        let Some((_, _, _, bottom)) = self.bounds() else { return (0, 0) };
        let rows = (self.h / 10).clamp(1, 4);
        let (mut sum, mut n) = (0, 0);
        for y in bottom.saturating_sub(rows)..bottom {
            for x in 0..self.w {
                if self.solid(x, y) {
                    sum += x;
                    n += 1;
                }
            }
        }
        ((sum + n / 2).checked_div(n).unwrap_or(self.w / 2), bottom)
    }
}

/// One set of at most [`MOST_COLOURS`] colours for all the frames: the commonest colours first,
/// each nearer than [`COLOURS_WITHIN`] to one already taken merged into it.
fn shared_colours(frames: &[Art]) -> Vec<[u8; 3]> {
    let mut counts: std::collections::HashMap<[u8; 3], usize> = std::collections::HashMap::new();
    for frame in frames {
        for p in frame.rgba.as_chunks::<4>().0 {
            if p[3] >= 128 {
                *counts.entry([p[0], p[1], p[2]]).or_default() += 1;
            }
        }
    }
    if counts.len() <= MOST_COLOURS {
        return counts.into_keys().collect();
    }
    let mut by_count: Vec<([u8; 3], usize)> = counts.into_iter().collect();
    by_count.sort_by_key(|&(c, n)| (std::cmp::Reverse(n), c));
    let mut within = COLOURS_WITHIN;
    loop {
        let mut groups: Vec<([f64; 3], f64)> = Vec::new();
        for &(c, n) in &by_count {
            let as_px = |g: &([f64; 3], f64)| {
                let m = g.0.map(|s| (s / g.1).round() as u8);
                [m[0], m[1], m[2], 255]
            };
            let here = [c[0], c[1], c[2], 255];
            match groups.iter_mut().find(|g| colour_apart(as_px(g), here) < within) {
                Some(g) => {
                    for (s, v) in g.0.iter_mut().zip(c) {
                        *s += f64::from(v) * n as f64;
                    }
                    g.1 += n as f64;
                }
                None => groups.push((c.map(|v| f64::from(v) * n as f64), n as f64)),
            }
        }
        // A colour on very few pixels is the blur between two others, not one of the sprite's.
        let all: f64 = groups.iter().map(|g| g.1).sum();
        groups.retain(|g| g.1 >= RARE_SHARE * all);
        if groups.len() <= MOST_COLOURS || within > 120 {
            return groups.into_iter().take(MOST_COLOURS).map(|(s, n)| s.map(|v| (v / n).round() as u8)).collect();
        }
        within += 4;
    }
}

fn colour_apart(a: [u8; 4], b: [u8; 4]) -> u32 {
    let [dr, dg, db] = [(a[0], b[0]), (a[1], b[1]), (a[2], b[2])].map(|(x, y)| u32::from(x.abs_diff(y)));
    ((f64::from(2 * dr * dr + 4 * dg * dg + 3 * db * db) / 9.0).sqrt()) as u32
}

/// The frames moved so their feet stand on one spot, all the same size.
fn line_up(frames: &[Art]) -> (usize, usize, Vec<Vec<u8>>) {
    let feet: Vec<(usize, usize)> = frames.iter().map(Art::feet).collect();
    let bounds: Vec<(usize, usize, usize, usize)> = frames.iter().map(|f| f.bounds().unwrap_or((0, 0, 1, 1))).collect();
    let left = feet.iter().zip(&bounds).map(|((fx, _), b)| fx.saturating_sub(b.0)).max().unwrap_or(0);
    let right = feet.iter().zip(&bounds).map(|((fx, _), b)| b.2.saturating_sub(*fx)).max().unwrap_or(1);
    let up = feet.iter().zip(&bounds).map(|((_, fy), b)| fy.saturating_sub(b.1)).max().unwrap_or(1);
    let (w, h) = ((left + right).max(1), up.max(1));
    let out = frames
        .iter()
        .zip(&feet)
        .map(|(f, &(fx, fy))| {
            let mut canvas = Art::new(w, h);
            for y in 0..f.h {
                for x in 0..f.w {
                    let p = f.get(x, y);
                    if p[3] > 0 {
                        let (cx, cy) = ((x + left).checked_sub(fx), (y + h).checked_sub(fy));
                        if let (Some(cx), Some(cy)) = (cx, cy) {
                            canvas.set(cx, cy, p);
                        }
                    }
                }
            }
            canvas.rgba
        })
        .collect();
    (w, h, out)
}

/// The frames as their cells drew them, all the same size (the largest cell's).
fn in_cells(frames: &[Art]) -> (usize, usize, Vec<Vec<u8>>) {
    let w = frames.iter().map(|f| f.w).max().unwrap_or(1);
    let h = frames.iter().map(|f| f.h).max().unwrap_or(1);
    let out = frames
        .iter()
        .map(|f| {
            let mut canvas = Art::new(w, h);
            for y in 0..f.h {
                for x in 0..f.w {
                    canvas.set(x, y, f.get(x, y));
                }
            }
            canvas.rgba
        })
        .collect();
    (w, h, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INK: [u8; 3] = [10, 8, 20];
    const SHELL: [u8; 3] = [240, 210, 30];
    const LEAF: [u8; 3] = [60, 180, 60];

    /// A little sprite, 12 × 10 art pixels, its legs (the bottom rows) moved by `step`.
    fn sprite(step: usize) -> Vec<Option<[u8; 3]>> {
        let (w, h) = (12, 10);
        let mut s = vec![None; w * h];
        let mut put = |x: usize, y: usize, c| {
            if let Some(p) = s.get_mut(y * w + x) {
                *p = Some(c);
            }
        };
        for y in 1..7 {
            for x in 2..9 {
                let edge = y == 1 || y == 6 || x == 2 || x == 8;
                put(x, y, if edge { INK } else { SHELL });
            }
        }
        put(5, 0, LEAF);
        put(6, 0, LEAF);
        // An eye in the background's own grey: enclosed and small, so it stays.
        put(6, 3, [88, 88, 88]);
        for y in 7..10 {
            put(3 + step, y, INK);
            put(7 - step, y, INK);
        }
        s
    }

    /// A sheet of `frames` sprites in a grid, each art pixel `scale` picture pixels, on `bg`, with
    /// a little JPEG-like noise.
    fn sheet(across: usize, down: usize, scale: f32, bg: [u8; 4], noise: bool) -> (Vec<u8>, u32, u32) {
        let (cell_w, cell_h) = (18.0 * scale, 16.0 * scale);
        let (w, h) = ((across as f32 * cell_w) as usize, (down as f32 * cell_h) as usize);
        let mut rgba = vec![0; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let (ci, cj) = ((x as f32 / cell_w) as usize, (y as f32 / cell_h) as usize);
                let (ax, ay) = (
                    ((x as f32 - ci as f32 * cell_w) / scale) as isize - 3,
                    ((y as f32 - cj as f32 * cell_h) / scale) as isize - 3,
                );
                let art = sprite((ci + cj * across) % 3);
                let c = (ax >= 0 && ay >= 0 && ax < 12 && ay < 10)
                    .then(|| art.get(ay as usize * 12 + ax as usize).copied().flatten())
                    .flatten();
                let hash = (x as u64).wrapping_mul(2_654_435_761) ^ (y as u64).wrapping_mul(40_503).rotate_left(17);
                let wobble = if noise { (hash >> 9) as i32 % 9 - 4 } else { 0 };
                let px = match c {
                    Some(c) => c.map(|v| (i32::from(v) + wobble).clamp(0, 255) as u8),
                    None => [bg[0], bg[1], bg[2]].map(|v| (i32::from(v) + wobble).clamp(0, 255) as u8),
                };
                let alpha = if c.is_some() { 255 } else { bg[3] };
                if let Some(p) = rgba.get_mut((y * w + x) * 4..(y * w + x) * 4 + 4) {
                    p.copy_from_slice(&[px[0], px[1], px[2], alpha]);
                }
            }
        }
        (rgba, w as u32, h as u32)
    }

    fn colours(sheet: &Sheet) -> usize {
        let mut all: Vec<[u8; 3]> =
            sheet.frames.iter().flat_map(|f| crate::pixel::colours_of(f, 256).unwrap_or_default()).collect();
        all.sort_unstable();
        all.dedup();
        all.len()
    }

    #[test]
    fn a_blown_up_noisy_sheet_on_grey_comes_back_as_its_real_pixels() {
        let (rgba, w, h) = sheet(4, 2, 10.7, [88, 88, 88, 255], true);
        let options = Options { pixel_art: true, ..Options::default() };
        let read = read(&rgba, w, h, &options).expect("a sheet");
        assert_eq!(read.places.len(), 8);
        assert!((read.pixel_size - 10.7).abs() < 0.1, "pixel size {}", read.pixel_size);
        assert!(read.background.is_some_and(|c| c.iter().all(|v| v.abs_diff(88) < 6)), "{:?}", read.background);
        // The sprite's own size: 7 across, 10 down.
        assert!((7..=8).contains(&read.width) && read.height == 10, "{} × {}", read.width, read.height);
        assert_eq!(colours(&read), 4, "ink, shell, leaf and the grey eye");
        // Every frame stands on the bottom row.
        for f in &read.frames {
            let bottom = f
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter(|(_, p)| p[3] > 0)
                .map(|(i, _)| i / read.width as usize)
                .max();
            assert_eq!(bottom, Some(read.height as usize - 1));
        }
    }

    #[test]
    fn a_white_band_and_a_second_background_colour_go_too() {
        let (mut rgba, w, h) = sheet(3, 1, 8.0, [40, 40, 40, 255], false);
        for y in 0..12 {
            for x in 0..w as usize {
                let i = (y * w as usize + x) * 4;
                if let Some(p) = rgba.get_mut(i..i + 4)
                    && p[..3] == [40, 40, 40]
                {
                    p.copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        let read = read(&rgba, w, h, &Options { pixel_art: true, ..Options::default() }).expect("a sheet");
        assert_eq!(read.places.len(), 3);
        assert_eq!(read.height, 10);
    }

    #[test]
    fn a_sheet_of_real_pixels_on_transparency_keeps_its_pixels() {
        let (rgba, w, h) = sheet(3, 1, 1.0, [0, 0, 0, 0], false);
        let read = read(&rgba, w, h, &Options { pixel_art: true, ..Options::default() }).expect("a sheet");
        assert_eq!(read.background, None);
        assert_eq!(read.pixel_size, 1.0);
        assert_eq!(read.places.len(), 3);
        assert_eq!(read.height, 10);
        assert_eq!(colours(&read), 4);
    }

    #[test]
    fn an_exported_sheet_at_eight_is_read_at_eight() {
        let (rgba, w, h) = sheet(2, 2, 8.0, [0, 0, 0, 0], false);
        let read = read(&rgba, w, h, &Options { pixel_art: true, ..Options::default() }).expect("a sheet");
        assert!((read.pixel_size - 8.0).abs() < 0.05, "pixel size {}", read.pixel_size);
        assert_eq!(read.places.len(), 4);
        assert_eq!(read.height, 10);
    }

    #[test]
    fn touching_sprites_are_cut_by_the_grid_and_keep_their_places() {
        // Real pixels in 18 × 16 cells whose sprites touch across the cells' edges: the gaps find
        // one piece, the grid finds the four cells.
        let (mut rgba, w, h) = sheet(4, 1, 1.0, [0, 0, 0, 0], false);
        for x in 0..w as usize {
            let i = (12 * w as usize + x) * 4;
            if let Some(p) = rgba.get_mut(i..i + 4) {
                p.copy_from_slice(&[10, 8, 20, 255]);
            }
        }
        let by_gaps = read(&rgba, w, h, &Options { pixel_art: true, ..Options::default() }).expect("a sheet");
        assert_eq!(by_gaps.places.len(), 1);
        let by_grid =
            read(&rgba, w, h, &Options { pixel_art: true, grid: Some((4, 1)), ..Options::default() }).expect("a sheet");
        assert_eq!(by_grid.places.len(), 4);
        assert_eq!((by_grid.width, by_grid.height), (18, 16));
    }

    #[test]
    fn a_checkerboard_background_goes_and_specks_go_with_it() {
        let (mut rgba, w, h) = sheet(2, 1, 6.0, [0, 0, 0, 255], false);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let i = (y * w as usize + x) * 4;
                if let Some(p) = rgba.get_mut(i..i + 4)
                    && p[..3] == [0, 0, 0]
                {
                    let light = (x / 8 + y / 8) % 2 == 0;
                    p.copy_from_slice(if light { &[200, 200, 200, 255] } else { &[150, 150, 150, 255] });
                }
            }
        }
        // A speck far from both sprites.
        let i = (2 * w as usize + (w as usize / 2)) * 4;
        if let Some(p) = rgba.get_mut(i..i + 4) {
            p.copy_from_slice(&[255, 0, 0, 255]);
        }
        let read = read(&rgba, w, h, &Options { pixel_art: true, ..Options::default() }).expect("a sheet");
        assert_eq!(read.places.len(), 2);
        assert_eq!(read.height, 10);
    }

    #[test]
    fn a_detached_bit_joins_its_frame() {
        let (mut rgba, w, h) = sheet(2, 1, 4.0, [0, 0, 0, 0], false);
        // A sparkle a little left of the first sprite, apart from it by empty columns.
        for y in 4..14 {
            for x in 2..12 {
                let i = (y * w as usize + x) * 4;
                if let Some(p) = rgba.get_mut(i..i + 4) {
                    p.copy_from_slice(&[255, 255, 0, 255]);
                }
            }
        }
        let read = read(&rgba, w, h, &Options { pixel_art: true, ..Options::default() }).expect("a sheet");
        assert_eq!(read.places.len(), 2);
        assert_eq!(read.places.first().map(|p| p.x), Some(2));
    }

    #[test]
    fn a_large_enclosed_background_patch_goes_and_a_small_one_stays() {
        let (w, h) = (60_usize, 40_usize);
        let mut rgba = vec![0_u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let ring = (5..35).contains(&y) && (5..55).contains(&x);
                let hole = (10..30).contains(&y) && (10..30).contains(&x);
                let eye = (15..17).contains(&y) && (40..42).contains(&x);
                let c = if ring && !hole && !eye { [200, 20, 20] } else { [88, 88, 88] };
                if let Some(p) = rgba.get_mut((y * w + x) * 4..(y * w + x) * 4 + 4) {
                    p.copy_from_slice(&[c[0], c[1], c[2], 255]);
                }
            }
        }
        let read = read(&rgba, w as u32, h as u32, &Options::default()).expect("a sheet");
        let f = read.frames.first().expect("a frame");
        let at = |x: usize, y: usize| f.get((y * read.width as usize + x) * 4 + 3).copied();
        assert_eq!((read.width, read.height), (50, 30));
        assert_eq!(at(10, 10), Some(0), "the big hole is cleared");
        assert_eq!(at(35, 10), Some(255), "the small eye stays");
    }

    #[test]
    fn names_come_from_the_file_unless_it_is_only_digits() {
        assert_eq!(name_from_file("/sdcard/Download/turtle_walk.png"), "turtle walk");
        assert_eq!(name_from_file("16418.jpg"), "imported");
        assert_eq!(name_from_file("C:\\sprites\\hero-run.webp"), "hero run");
    }

    /// The user's own sheets, as raw RGBA files beside a `.size` file ("w h"), from
    /// `PROCRAFT_SHEETS`: not in git.
    #[test]
    #[ignore = "needs sheets outside the repository"]
    fn the_users_sheets_read() {
        let Ok(dir) = std::env::var("PROCRAFT_SHEETS") else { return };
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "rgba") {
                let size = std::fs::read_to_string(path.with_extension("size")).unwrap_or_default();
                let mut it = size.split_whitespace().filter_map(|v| v.parse::<u32>().ok());
                let (Some(w), Some(h)) = (it.next(), it.next()) else { continue };
                let rgba = std::fs::read(&path).unwrap_or_default();
                let started = std::time::Instant::now();
                let read = read(&rgba, w, h, &Options { pixel_art: true, ..Options::default() });
                let Some(read) = read else {
                    println!("{}: nothing", path.display());
                    continue;
                };
                println!(
                    "{}: {} frames, pixel size {:.2}, {} × {}, {} colours, background {:?}, {:?}",
                    path.display(),
                    read.places.len(),
                    read.pixel_size,
                    read.width,
                    read.height,
                    colours(&read),
                    read.background,
                    started.elapsed()
                );
                let mut out = Vec::new();
                for f in &read.frames {
                    out.extend_from_slice(f);
                }
                let _ = std::fs::write(path.with_extension("frames"), out);
                let _ = std::fs::write(
                    path.with_extension("framesize"),
                    format!("{} {} {}", read.width, read.height, read.frames.len()),
                );
            }
        }
    }
}
