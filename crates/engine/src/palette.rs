//! Palettes: thirty swatches each, kept in a JSON file; written to and read from swatches files
//! (`.swatches`, a zip holding `Swatches.json`), and read from other programs' colour swatch
//! (`.aco`) and swatch exchange (`.ase`) files; and the main colours of a picture, for a palette
//! made from it. Colours are 0xRRGGBB, sRGB.

use serde_json::{Value, json};

use crate::zip::{ZipReader, ZipWriter};

/// The swatches a palette holds: ten across, three down.
pub const SLOTS: usize = 30;
/// A swatches file's extension, and the entry it keeps the palette in.
pub const SWATCHES_EXTENSION: &str = "swatches";
/// The entry a swatches file keeps its palette in.
pub const SWATCHES_ENTRY: &str = "Swatches.json";

/// A named set of [`SLOTS`] swatches, empty slots none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<Option<u32>>,
}

impl Palette {
    /// A palette named `name` holding `colors` (the first [`SLOTS`], the rest of the slots empty).
    pub fn new(name: impl Into<String>, colors: impl IntoIterator<Item = Option<u32>>) -> Palette {
        let mut colors: Vec<Option<u32>> = colors.into_iter().take(SLOTS).map(|c| c.map(|v| v & 0xFF_FFFF)).collect();
        colors.resize(SLOTS, None);
        Palette { name: name.into(), colors }
    }

    /// An empty palette.
    pub fn empty(name: impl Into<String>) -> Palette {
        Palette::new(name, std::iter::empty())
    }
}

/// The palettes a new install starts with.
pub fn starters() -> Vec<Palette> {
    let basics = [
        0xFFFFFF, 0xE3E3E3, 0xC6C6C6, 0xA9A9A9, 0x8C8C8C, 0x707070, 0x545454, 0x393939, 0x1F1F1F, 0x000000, 0xD7263D,
        0xF46036, 0xF5B700, 0x7CB518, 0x1B998B, 0x2E86AB, 0x2E4A9E, 0x6A3FA0, 0xC2377D, 0x7A4B2A, 0xF2A7B1, 0xF9B99A,
        0xFBE38E, 0xC5E39A, 0x9AD9CF, 0xA4CCE3, 0xA3B1DD, 0xC5ADE0, 0xE7A9C8, 0xC9A88C,
    ];
    let earth = [
        0xF6E7CF, 0xE9CFA5, 0xD8B07A, 0xC28F55, 0xA8703B, 0x8A552B, 0x6C3F20, 0x4F2D17, 0x34200F, 0x1E1309, 0x9FB28A,
        0x7E9A6A, 0x60804F, 0x46663A, 0x324E2A, 0xB9C4C9, 0x8FA3AD, 0x66808E, 0x46606F, 0x2D4250, 0xE8C1A0, 0xD9A37F,
        0xC4845F, 0xA86846, 0x8A5135, 0x6B3E2A,
    ];
    vec![Palette::new("Basics", basics.map(Some)), Palette::new("Earth", earth.map(Some))]
}

/// The palettes kept in `text` (the store's JSON) and which one is the default; none if it can't
/// be read or holds none.
pub fn read_store(text: &str) -> Option<(Vec<Palette>, usize)> {
    let root: Value = serde_json::from_str(text).ok()?;
    let list = root.get("palettes")?.as_array()?;
    let palettes: Vec<Palette> = list
        .iter()
        .filter_map(|p| {
            // one damaged in part keeps what can be read: a name it can't be "Untitled Palette",
            // colours it can't be empty slots (dropped, it would be gone at the next write)
            p.as_object()?;
            let name = p.get("name").and_then(Value::as_str).unwrap_or("Untitled Palette").to_owned();
            let colors = p.get("colors").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
            // a colour kept with its alpha, as a signed 32-bit number, keeps its red, green and blue
            Some(Palette::new(name, colors.iter().map(|c| c.as_i64().map(|v| (v as u32) & 0xFF_FFFF))))
        })
        .collect();
    if palettes.is_empty() {
        return None;
    }
    let default = root.get("default").and_then(Value::as_u64).map_or(0, |d| d as usize).min(palettes.len() - 1);
    Some((palettes, default))
}

/// The store's JSON for `palettes`, `default` the default one.
pub fn write_store(palettes: &[Palette], default: usize) -> String {
    let list: Vec<Value> = palettes.iter().map(|p| json!({ "name": p.name, "colors": p.colors })).collect();
    json!({ "default": default, "palettes": list }).to_string()
}

/// Palette `p` as a swatches file: all thirty slots, empty ones null, each colour as hue,
/// saturation and brightness in sRGB, the profile it is in kept once beside them (named by its
/// SHA-256 in base64).
pub fn write_swatches(p: &Palette, srgb_icc: &[u8]) -> Vec<u8> {
    let hash = crate::base64::encode(&sha256(srgb_icc));
    let swatches: Vec<Value> = p
        .colors
        .iter()
        .map(|c| match c {
            None => Value::Null,
            Some(rgb) => {
                let [h, s, b] = rgb_to_hsb(*rgb);
                json!({
                    "alpha": 1, "origin": 1, "colorSpace": 0, "colorModel": 0, "brightness": b,
                    "components": [h, s, b], "version": "5.0", "colorProfile": hash,
                    "saturation": s, "hue": h,
                })
            }
        })
        .collect();
    let profile = json!({
        "colorSpace": 0, "hash": hash, "iccData": crate::base64::encode(srgb_icc), "iccName": "sRGB IEC61966-2.1",
    });
    let text = json!({ "name": p.name, "swatches": swatches, "colorProfiles": [profile] }).to_string();
    let mut zip = ZipWriter::new();
    zip.add(SWATCHES_ENTRY, text.as_bytes());
    zip.finish()
}

/// The palette in a swatches file, or none if `bytes` isn't one.
pub fn read_swatches(bytes: &[u8]) -> Option<Palette> {
    let zip = ZipReader::new(bytes)?;
    let entry = zip.names().find(|n| n.rsplit('/').next() == Some(SWATCHES_ENTRY))?.to_owned();
    let text = String::from_utf8(zip.read(&entry)?).ok()?;
    let clean = without_trailing_commas(&text);
    let root: Value = serde_json::from_str(clean.trim()).ok()?;
    let root = match root {
        Value::Array(a) => a.into_iter().next()?,
        other => other,
    };
    let list = root.get("swatches")?.as_array()?;
    let colors = list.iter().map(|s| {
        let f = |k: &str| s.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        s.is_object().then(|| hsb_to_rgb(f("hue"), f("saturation"), f("brightness")))
    });
    let name =
        root.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Untitled Palette");
    Some(Palette::new(name, colors))
}

/// `text` without commas just before a closing bracket (some writers leave them).
fn without_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, c) in chars.iter().enumerate() {
        if *c == ',' {
            let next = chars.iter().skip(i + 1).find(|n| !n.is_whitespace());
            if matches!(next, Some('}' | ']')) {
                continue;
            }
        }
        out.push(*c);
    }
    out
}

/// Whether `head` (a file's first bytes) starts a swatch exchange file.
pub fn is_exchange(head: &[u8]) -> bool {
    head.starts_with(b"ASEF")
}

/// The palette in `bytes`, a swatch exchange file or (when `file_name` ends in `.aco`) a colour
/// swatch file, named by its first group or else the file; none if it is neither.
pub fn read_other(bytes: &[u8], file_name: Option<&str>) -> Option<Palette> {
    let base = file_name
        .map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n))
        .map(|n| n.rsplit_once('.').map_or(n, |(stem, _)| stem).trim().to_owned())
        .unwrap_or_default();
    if is_exchange(bytes) {
        exchange(bytes, &base)
    } else if file_name.is_some_and(|n| n.to_lowercase().ends_with(".aco")) {
        colour_swatch(bytes, &base)
    } else {
        None
    }
}

/// Big-endian reading, none past the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let b: [u8; N] = self.bytes.get(self.at..self.at + N)?.try_into().ok()?;
        self.at += N;
        Some(b)
    }
    fn u16(&mut self) -> Option<u16> {
        self.take::<2>().map(u16::from_be_bytes)
    }
    fn u32(&mut self) -> Option<u32> {
        self.take::<4>().map(u32::from_be_bytes)
    }
    fn f32(&mut self) -> Option<f32> {
        self.take::<4>().map(f32::from_be_bytes)
    }
    /// A name: its length in UTF-16 units (with the closing zero), then the units.
    fn utf16(&mut self) -> Option<String> {
        let units = self.u16()?;
        let chars: Option<Vec<u16>> = (0..units).map(|_| self.u16()).collect();
        Some(String::from_utf16_lossy(&chars?).trim_end_matches('\0').to_owned())
    }
}

/// A colour swatch file: a list of colours, then (in version 2) the same again with names.
fn colour_swatch(bytes: &[u8], name: &str) -> Option<Palette> {
    let mut r = Reader { bytes, at: 0 };
    let version = r.u16()?;
    if !(1..=2).contains(&version) {
        return None;
    }
    let count = r.u16()?;
    let mut colours = Vec::new();
    for _ in 0..count {
        let space = r.u16()?;
        let v = [r.u16()?, r.u16()?, r.u16()?, r.u16()?];
        // version 2 names each colour: its length in UTF-16 units, then the units
        if version == 2 {
            let units = r.u32()? as usize;
            r.at += 2 * units;
        }
        if let Some(c) = aco_colour(space, v) {
            colours.push(Some(c));
        }
    }
    if colours.is_empty() {
        return None;
    }
    Some(Palette::new(if name.is_empty() { "Imported Palette" } else { name }, colours))
}

/// A colour swatch file's colour in colour space `space` from its four numbers.
fn aco_colour(space: u16, [a, b, c, d]: [u16; 4]) -> Option<u32> {
    let n = |v: u16| f64::from(v) / 65535.0;
    match space {
        0 => Some(rgb(n(a), n(b), n(c))),
        1 => Some(hsb(n(a), n(b), n(c))),
        // 0 is full ink
        2 => Some(cmyk(1.0 - n(a), 1.0 - n(b), 1.0 - n(c), 1.0 - n(d))),
        7 => Some(lab(f64::from(a) / 100.0, f64::from(b as i16) / 100.0, f64::from(c as i16) / 100.0)),
        // 0 is white
        8 => {
            let g = 1.0 - f64::from(a) / 10000.0;
            Some(rgb(g, g, g))
        }
        9 => Some(cmyk(f64::from(a) / 10000.0, f64::from(b) / 10000.0, f64::from(c) / 10000.0, f64::from(d) / 10000.0)),
        _ => None,
    }
}

/// A swatch exchange file: blocks of groups and colours, each colour named and in a model.
fn exchange(bytes: &[u8], fallback: &str) -> Option<Palette> {
    let mut r = Reader { bytes, at: 8 };
    let blocks = r.u32()?;
    let mut colours = Vec::new();
    let mut group: Option<String> = None;
    for _ in 0..blocks {
        let kind = r.u16()?;
        let length = r.u32()? as usize;
        let end = r.at + length;
        match kind {
            // a group starts: the palette takes the first one's name
            0xC001 if length >= 2 => {
                let name = r.utf16()?;
                if group.is_none() && !name.trim().is_empty() {
                    group = Some(name);
                }
            }
            0x0001 => {
                r.utf16()?;
                let model = r.take::<4>()?;
                let c = match &model {
                    b"RGB " => Some(rgb(f64::from(r.f32()?), f64::from(r.f32()?), f64::from(r.f32()?))),
                    b"CMYK" => {
                        Some(cmyk(f64::from(r.f32()?), f64::from(r.f32()?), f64::from(r.f32()?), f64::from(r.f32()?)))
                    }
                    b"LAB " => Some(lab(f64::from(r.f32()?) * 100.0, f64::from(r.f32()?), f64::from(r.f32()?))),
                    b"Gray" => {
                        let g = f64::from(r.f32()?);
                        Some(rgb(g, g, g))
                    }
                    _ => None,
                };
                if let Some(c) = c {
                    colours.push(Some(c));
                }
            }
            _ => {}
        }
        r.at = end;
    }
    if colours.is_empty() {
        return None;
    }
    let name = group
        .map(|g| g.trim().to_owned())
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| if fallback.is_empty() { "Imported Palette".to_owned() } else { fallback.to_owned() });
    Some(Palette::new(name, colours))
}

fn rgb(r: f64, g: f64, b: f64) -> u32 {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

fn hsb(h: f64, s: f64, v: f64) -> u32 {
    let k = |n: f64| (n + h * 6.0) % 6.0;
    let f = |n: f64| v - v * s * k(n).min(4.0 - k(n)).clamp(0.0, 1.0);
    rgb(f(5.0), f(3.0), f(1.0))
}

/// Ink amounts (0..1) to RGB, simply: no press profile.
fn cmyk(c: f64, m: f64, y: f64, k: f64) -> u32 {
    rgb((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k))
}

/// CIE Lab (D50, as print programs keep it) to sRGB.
fn lab(l: f64, a: f64, bb: f64) -> u32 {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - bb / 200.0;
    let e = 6.0 / 29.0;
    let inv = |t: f64| if t > e { t * t * t } else { 3.0 * e * e * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.96422 * inv(fx), inv(fy), 0.82521 * inv(fz));
    // D50 XYZ to linear sRGB (Bradford-adapted to D65)
    let r = 3.1338561 * x - 1.6168667 * y - 0.4906146 * z;
    let g = -0.9787684 * x + 1.9161415 * y + 0.0334540 * z;
    let b = 0.0719453 * x - 0.2289914 * y + 1.4052427 * z;
    let encode = |c: f64| if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.max(0.0).powf(1.0 / 2.4) - 0.055 };
    rgb(encode(r), encode(g), encode(b))
}

/// 0xRRGGBB as hue, saturation and brightness (0..1).
fn rgb_to_hsb(v: u32) -> [f64; 3] {
    let [_, r, g, b] = v.to_be_bytes().map(|c| f64::from(c) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, if max > 0.0 { d / max } else { 0.0 }, max]
}

/// Hue, saturation and brightness (0..1) as 0xRRGGBB.
fn hsb_to_rgb(h: f32, s: f32, v: f32) -> u32 {
    hsb(f64::from(h), f64::from(s), f64::from(v))
}

/// How many colours a palette made from a picture is picked from: one more than it holds.
pub const PICTURE_COLOURS: usize = 31;
/// A picture of no more different colours than this gives its own colours; one of more, colours
/// standing for them.
const OWN_COLOURS_MOST: usize = 256;
/// The rounds the colours standing for a picture of many are settled in.
const SETTLING_ROUNDS: usize = 20;

/// The colours of a palette made from a picture (`pixels` 0xRRGGBB), the first `count` of them by
/// hue (greys first). A picture of up to 256 different colours gives its own, exactly (the
/// [`PICTURE_COLOURS`] most used, where it has more); one of more gives that many colours standing
/// for them: its colours grouped by a median cut, then each taken to the group whose mean is
/// nearest, over [`SETTLING_ROUNDS`] rounds, each group's mean kept.
pub fn main_colours(pixels: &[u32], count: usize) -> Vec<u32> {
    let mut colours = picture_colours(pixels);
    colours.sort_unstable();
    colours.dedup();
    // by hue, greys (hue 0) first, ties as they were (by their numbers)
    colours.sort_by(|a, b| rgb_to_hsb(*a)[0].total_cmp(&rgb_to_hsb(*b)[0]));
    colours.truncate(count);
    colours
}

/// A picture's colours as [`main_colours`] picks them, in no order.
fn picture_colours(pixels: &[u32]) -> Vec<u32> {
    // each colour's count, until there are too many to give them all; and the picture's colours
    // in 32768 boxes (five bits a channel), each's sums and count
    let mut own: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut many = false;
    let mut boxes = vec![[0u64; 4]; 1 << 15];
    for &p in pixels {
        let c = p & 0xFF_FFFF;
        if !many {
            *own.entry(c).or_insert(0) += 1;
            many = own.len() > OWN_COLOURS_MOST;
        }
        let [_, r, g, b] = c.to_be_bytes();
        let at = usize::from(r >> 3) << 10 | usize::from(g >> 3) << 5 | usize::from(b >> 3);
        if let Some(s) = boxes.get_mut(at) {
            *s = [s[0] + u64::from(r), s[1] + u64::from(g), s[2] + u64::from(b), s[3] + 1];
        }
    }
    if !many {
        // the most used first, ties by their numbers
        let mut counted: Vec<(u32, u64)> = own.into_iter().collect();
        counted.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        return counted.into_iter().take(PICTURE_COLOURS).map(|(c, _)| c).collect();
    }
    let points: Vec<[u64; 4]> = boxes.into_iter().filter(|s| s[3] > 0).collect();
    settled_colours(&points, PICTURE_COLOURS)
}

/// Channel `k` of `v` (0 past its three).
fn channel(v: [f64; 3], k: usize) -> f64 {
    v.get(k).copied().unwrap_or(0.0)
}

/// Up to `count` colours standing for `points` (each a box of colours: its red, green and blue
/// sums and its count): a median cut, then each point taken to the nearest colour and the colours
/// made their points' means, round after round.
fn settled_colours(points: &[[u64; 4]], count: usize) -> Vec<u32> {
    let mean = |s: [u64; 4]| -> [f64; 3] {
        let n = s[3].max(1) as f64;
        [s[0] as f64 / n, s[1] as f64 / n, s[2] as f64 / n]
    };
    let sum =
        |group: &[&[u64; 4]]| group.iter().fold([0u64; 4], |a, s| [a[0] + s[0], a[1] + s[1], a[2] + s[2], a[3] + s[3]]);
    // the median cut: the group of the widest spread in a channel split where half its count is
    let mut groups: Vec<Vec<&[u64; 4]>> = vec![points.iter().collect()];
    while groups.len() < count {
        let spread = |g: &[&[u64; 4]], k: usize| {
            let (lo, hi) = g
                .iter()
                .map(|s| channel(mean(**s), k))
                .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            hi - lo
        };
        let widest = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.len() > 1)
            .map(|(i, g)| {
                let (k, s) = (0..3).map(|k| (k, spread(g, k))).fold((0, -1.0), |a, b| if b.1 > a.1 { b } else { a });
                (i, k, s)
            })
            .max_by(|a, b| a.2.total_cmp(&b.2));
        let Some((i, k, _)) = widest else { break };
        let mut g = groups.swap_remove(i);
        g.sort_by(|a, b| channel(mean(**a), k).total_cmp(&channel(mean(**b), k)));
        let half = g.iter().map(|s| s[3]).sum::<u64>() / 2;
        let mut seen = 0;
        let cut = g
            .iter()
            .position(|s| {
                seen += s[3];
                seen >= half
            })
            .map_or(1, |p| p + 1)
            .clamp(1, g.len() - 1);
        let upper = g.split_off(cut);
        groups.push(g);
        groups.push(upper);
    }
    let mut centres: Vec<[f64; 3]> = groups.iter().map(|g| mean(sum(g))).collect();
    let mut sums: Vec<[u64; 4]> = groups.iter().map(|g| sum(g)).collect();
    // the rounds: each point to its nearest colour, each colour its points' mean (one with none
    // stays as it was)
    for _ in 0..SETTLING_ROUNDS {
        sums = vec![[0u64; 4]; centres.len()];
        for p in points {
            let m = mean(*p);
            let far = |c: &[f64; 3]| c.iter().zip(m).map(|(a, b)| (a - b).powi(2)).sum::<f64>();
            let nearest = centres.iter().enumerate().min_by(|a, b| far(a.1).total_cmp(&far(b.1))).map_or(0, |(i, _)| i);
            if let Some(s) = sums.get_mut(nearest) {
                *s = [s[0] + p[0], s[1] + p[1], s[2] + p[2], s[3] + p[3]];
            }
        }
        let before = centres.clone();
        for (c, s) in centres.iter_mut().zip(&sums) {
            if s[3] > 0 {
                *c = mean(*s);
            }
        }
        if centres == before {
            break;
        }
    }
    // each group's mean, in whole numbers (rounded down); groups left with nothing dropped
    sums.iter()
        .filter(|s| s[3] > 0)
        .map(|s| {
            let [r, g, b] = [s[0] / s[3], s[1] / s[3], s[2] / s[3]].map(|v| v.min(255) as u32);
            r << 16 | g << 8 | b
        })
        .collect()
}

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            if let Some(slot) = w.get_mut(i) {
                *slot = u32::from_be_bytes(*word);
            }
        }
        for i in 16..64 {
            let at = |j: usize| w.get(j).copied().unwrap_or(0);
            let (w15, w2, w16, w7) = (at(i - 15), at(i - 2), at(i - 16), at(i - 7));
            let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
            let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
            if let Some(slot) = w.get_mut(i) {
                *slot = w16.wrapping_add(s0).wrapping_add(w7).wrapping_add(s1);
            }
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (k, wi) in K.iter().zip(w.iter()) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(*k).wrapping_add(*wi);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (chunk, v) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = v.to_be_bytes();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_be_bytes());
    }

    fn name(out: &mut Vec<u8>, s: &str) {
        be16(out, s.len() as u16 + 1);
        for u in s.encode_utf16() {
            be16(out, u);
        }
        be16(out, 0);
    }

    #[test]
    fn colour_swatch_files_read_every_colour_space() {
        let mut aco = Vec::new();
        be16(&mut aco, 1);
        be16(&mut aco, 5);
        // red in RGB; green as hue 120°; full cyan ink (0 is full); mid grey (0 is white); white in Lab
        for c in [
            [0, 65535, 0, 0, 0],
            [1, 65535 / 3, 65535, 65535, 0],
            [2, 0, 65535, 65535, 65535],
            [8, 5000, 0, 0, 0],
            [7, 10000, 0, 0, 0],
        ] {
            for v in c {
                be16(&mut aco, v);
            }
        }
        let p = read_other(&aco, Some("Shop Colours.aco")).unwrap();
        assert_eq!(p.name, "Shop Colours");
        assert_eq!(p.colors.len(), SLOTS);
        assert_eq!(
            &p.colors[..6],
            &[Some(0xFF0000), Some(0x00FF00), Some(0x00FFFF), Some(0x808080), Some(0xFFFFFF), None]
        );
        crate::damaged::survives("colour swatch file", &aco, |b| {
            let _ = read_other(b, Some("x.aco"));
        });
    }

    #[test]
    fn swatch_exchange_files_take_their_groups_name() {
        let mut ase = b"ASEF".to_vec();
        be16(&mut ase, 1);
        be16(&mut ase, 0);
        ase.extend_from_slice(&4u32.to_be_bytes());
        let block = |ase: &mut Vec<u8>, kind: u16, body: Vec<u8>| {
            be16(ase, kind);
            ase.extend_from_slice(&(body.len() as u32).to_be_bytes());
            ase.extend_from_slice(&body);
        };
        let mut group = Vec::new();
        name(&mut group, "Sunset");
        block(&mut ase, 0xC001, group);
        let mut orange = Vec::new();
        name(&mut orange, "Orange");
        orange.extend_from_slice(b"RGB ");
        for v in [1.0f32, 0.5, 0.0] {
            orange.extend_from_slice(&v.to_be_bytes());
        }
        be16(&mut orange, 2);
        block(&mut ase, 0x0001, orange);
        let mut ink = Vec::new();
        name(&mut ink, "Ink");
        ink.extend_from_slice(b"CMYK");
        for v in [0.0f32, 0.0, 0.0, 1.0] {
            ink.extend_from_slice(&v.to_be_bytes());
        }
        be16(&mut ink, 2);
        block(&mut ase, 0x0001, ink);
        block(&mut ase, 0xC002, Vec::new());
        assert!(is_exchange(&ase));
        let p = read_other(&ase, Some("whatever.ase")).unwrap();
        assert_eq!(p.name, "Sunset");
        assert_eq!(&p.colors[..3], &[Some(0xFF8000), Some(0x000000), None]);
        crate::damaged::survives("swatch exchange file", &ase, |b| {
            let _ = read_other(b, None);
        });
    }

    #[test]
    fn other_files_are_not_palettes() {
        assert!(read_other(&[1, 2, 3], Some("notes.txt")).is_none());
        assert!(read_other(&[0, 1], Some("broken.aco")).is_none());
    }

    #[test]
    fn swatches_files_and_the_store_keep_a_palette_as_it_is() {
        let p = Palette::new("Mine", [Some(0x3366CC), None, Some(0xFFCC00)]);
        let file = write_swatches(&p, b"icc");
        assert_eq!(read_swatches(&file), Some(p));
        crate::damaged::survives("swatches file", &file, |b| {
            let _ = read_swatches(b);
        });
        let (palettes, default) = read_store(&write_store(&starters(), 1)).unwrap();
        assert_eq!((palettes, default), (starters(), 1));
        assert!(read_store("not json").is_none());
        // as the Android app kept them before: a colour with its alpha is a negative number
        let kept = r#"{"default":0,"palettes":[{"name":"Old","colors":[-15584170,null,1193046]}]}"#;
        let (palettes, _) = read_store(kept).unwrap();
        assert_eq!(palettes.first().map(|p| p.colors[..3].to_vec()), Some(vec![Some(0x123456), None, Some(0x123456)]));
        // damaged in part: a palette without a name, one with its colours unreadable, kept
        let damaged = r#"{"default":0,"palettes":[{"colors":[1193046]},{"name":"Bare","colors":"x"},7]}"#;
        let (palettes, _) = read_store(damaged).unwrap();
        let shown: Vec<(&str, Option<u32>)> = palettes.iter().map(|p| (p.name.as_str(), p.colors[0])).collect();
        assert_eq!(shown, vec![("Untitled Palette", Some(0x123456)), ("Bare", None)]);
    }

    /// A picture of flat colours (as a Discord report's: seventeen blobs on white, no smoothing)
    /// gives exactly its colours, each once, by hue; a median cut gave means of neighbouring
    /// colours instead (four greens for two).
    #[test]
    fn a_picture_of_few_colours_gives_exactly_its_own_by_hue() {
        const FLAT: [u32; 18] = [
            0xffffff, 0x662482, 0x5439a2, 0xe60083, 0x75808c, 0x005cc3, 0x000000, 0xa05634, 0x00775e, 0x63bad5,
            0x00a939, 0xf6d900, 0x9f76b0, 0xff6e94, 0xc81c3c, 0x9ad300, 0x1d81d0, 0xf9723e,
        ];
        // white the most (the paper), the rest as much as the blobs cover, one a single pixel
        let mut pixels: Vec<u32> = vec![0xffffff; 184_487];
        for (i, &c) in FLAT.iter().enumerate().skip(1) {
            pixels.extend(std::iter::repeat_n(c, if i == 9 { 1 } else { 3000 + 200 * i }));
        }
        let colours = main_colours(&pixels, SLOTS);
        let mut got = colours.clone();
        got.sort_unstable();
        let mut want = FLAT.to_vec();
        want.sort_unstable();
        assert_eq!(got, want, "exactly the picture's colours, none made up, none twice");
        let hues: Vec<f64> = colours.iter().map(|c| rgb_to_hsb(*c)[0]).collect();
        assert!(hues.windows(2).all(|w| w[0] <= w[1]), "by hue: {colours:06x?}");
        // greys (no hue) first, by their numbers
        assert_eq!(colours[..2], [0x000000, 0xffffff]);
        // the order the pixels come in changes nothing
        pixels.reverse();
        assert_eq!(main_colours(&pixels, SLOTS), colours);
        assert!(main_colours(&[], SLOTS).is_empty());
    }

    /// Between 32 and 256 colours: the 31 most used, exactly, the first 30 by hue kept; past 256:
    /// 31 colours standing for them, each near a part of the picture.
    #[test]
    fn a_picture_of_many_colours_gives_colours_standing_for_them() {
        // forty colours, the first thirty-one used more
        let forty: Vec<u32> = (0..40u32).map(|i| (i * 6) << 16 | (255 - i * 5) << 8 | (i * 3)).collect();
        let mut pixels = Vec::new();
        for (i, &c) in forty.iter().enumerate() {
            pixels.extend(std::iter::repeat_n(c, if i < 31 { 50 } else { 10 }));
        }
        let colours = main_colours(&pixels, SLOTS);
        assert_eq!(colours.len(), SLOTS);
        assert!(colours.iter().all(|c| forty[..31].contains(c)), "{colours:06x?}");
        // five colours with a little noise each: thousands of colours, five places
        let places = [0xd03020u32, 0x2050c0, 0x30a040, 0xf0e0c0, 0x202020];
        let mut noisy = Vec::new();
        let mut seed = 12345u32;
        for &c in &places {
            for _ in 0..4000 {
                let [_, r, g, b] = c.to_be_bytes();
                let mut jitter = |v: u8| {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                    (i32::from(v) + (seed >> 16) as i32 % 13 - 6).clamp(0, 255) as u32
                };
                noisy.push(jitter(r) << 16 | jitter(g) << 8 | jitter(b));
            }
        }
        let colours = main_colours(&noisy, SLOTS);
        assert!(colours.len() > 5 && colours.len() <= SLOTS, "{}", colours.len());
        let apart = |a: u32, b: u32| {
            let (a, b) = (a.to_be_bytes(), b.to_be_bytes());
            (1..4).map(|k| (f64::from(a[k]) - f64::from(b[k])).powi(2)).sum::<f64>().sqrt()
        };
        // each colour is one of the places' (within the noise), and each place has one
        let worst =
            colours.iter().map(|&c| places.iter().map(|&p| apart(c, p)).fold(f64::MAX, f64::min)).fold(0.0, f64::max);
        eprintln!("{} colours for five noisy places, the farthest {worst:.1} from its place", colours.len());
        assert!(worst < 10.0, "{worst}");
        // (thirty-one colours for five places: each place's noise split among several of them)
        let each: Vec<usize> =
            places.iter().map(|&p| colours.iter().filter(|&&c| apart(c, p) < 10.0).count()).collect();
        eprintln!("colours at each place: {each:?}");
        assert!(each.iter().all(|&n| n >= 2), "{each:?}");
        // a smooth ramp, 4096 colours of a dark red to a light blue: thirty along it, end to end
        let ramp: Vec<u32> =
            (0..4096u32).map(|i| (200 - i * 160 / 4095) << 16 | (i % 64) << 8 | (40 + i * 200 / 4095)).collect();
        let colours = main_colours(&ramp, SLOTS);
        assert_eq!(colours.len(), SLOTS);
        let blues: Vec<u32> = colours.iter().map(|c| c & 0xFF).collect();
        let (lo, hi) = (blues.iter().min().copied().unwrap(), blues.iter().max().copied().unwrap());
        eprintln!("the ramp's blues {lo} to {hi} of 40 to 240");
        assert!(lo < 60 && hi > 220, "{lo} {hi}");
    }

    #[test]
    fn sha256_is_sha256() {
        let hex = |b: [u8; 32]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
