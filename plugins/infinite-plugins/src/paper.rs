//! Paper: a paper for each artwork, set from its row in the Layers panel or the plugin's card. Every
//! brush takes its grain as it paints, a grain fixed to the canvas on top of the brush's own: where
//! the paper is low its hollows take a share of the paint (Grain says how much, Tooth how sharply).
//! The paper shows under the art, in a colour of its own (one of the plugin's, or any from its Hue,
//! Saturation and Brightness) or the artwork's background colour, its hollows a little darker. Each
//! artwork keeps its own paper. The papers are drawn here, a tile that repeats seamlessly, or made
//! of a picture someone picks for Your own paper: no picture of anyone's comes with it. Off, no
//! brush takes a paper and none shows.
//!
//! The app asks the paper's hook for the tile and how it is taken; for Your own paper the plugin
//! names its picture control, and the app makes the tile of the picture picked. Its row in the
//! Layers panel opens a panel of its settings, which it sets as its card would.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::{
    BRIGHTNESS, COLOUR, CONTROLS, GRAIN, HUE, KIND, OWN_COLOUR, PAPER as FEATURE, PICTURE, PICTURE2, PICTURE3,
    PICTURE4, SATURATION, SHOWN, SIZE, TEXTURE, TOOTH,
};

/// The file controls the texture offers, in its order.
const OWN: [usize; 4] = [PICTURE, PICTURE2, PICTURE3, PICTURE4];

/// The tile's side: what the app asks for.
const SIDE: usize = 256;

/// The Paper control's step that is the picture picked for Your own paper, and the one that is the
/// texture picked.
const OWN_STEP: u32 = 6;
const TEXTURES_STEP: u32 = 7;

/// The papers drawn here, by the Paper control's steps (0 is none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    ColdPress,
    HotPress,
    Rough,
    Canvas,
    Laid,
}

impl Kind {
    fn of(step: f32) -> Option<Kind> {
        match (step + 0.5) as i32 {
            1 => Some(Kind::ColdPress),
            2 => Some(Kind::HotPress),
            3 => Some(Kind::Rough),
            4 => Some(Kind::Canvas),
            5 => Some(Kind::Laid),
            _ => None,
        }
    }
}

/// The colours under the art, by the Paper colour control's steps from White (0xRRGGBB); the
/// first step is the artwork's own background colour.
const COLOURS: [i32; 6] = [0xFFFFFF, 0xF4ECD8, 0xF8F4E6, 0xC9C9C6, 0xC2A27A, 0x1C1C1C];

/// `hue` (degrees), `saturation` and `brightness` (0 to 1) as 0xRRGGBB.
fn rgb(hue: f32, saturation: f32, brightness: f32) -> i32 {
    let clamp = |v: f32, most: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, most) };
    let (s, v) = (clamp(saturation, 1.0), clamp(brightness, 1.0));
    // the hue in sixths of the wheel, 0 to 6 (360° the same as 0°)
    let sixths = clamp(hue, 360.0) / 60.0;
    let sector = (sixths as i32).min(5);
    let within = sixths - sector as f32;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * within), v * (1.0 - s * (1.0 - within)));
    let (r, g, b) = match sector {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let byte = |c: f32| (c * 255.0 + 0.5) as i32;
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// A number from a lattice point and a seed, 0 to 1.
fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8DA6_B343) ^ y.wrapping_mul(0xD816_3841) ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// The most lattice points across a tile: its finest cells are 2 pixels.
const MOST_CELLS: usize = SIDE / 2;

/// Smooth noise on a lattice of cells `cell` pixels a side, repeating every [`SIDE`] pixels (`cell`
/// divides it, 2 at the least), `strength` times as strong, added to `sum` (a tile, rows from the
/// top). Each row is worked out down the lattice's columns first, then across: two hashes a
/// column a row, and a few steps a pixel.
fn add_noise(sum: &mut [f32; SIDE * SIDE], cell: usize, seed: u32, strength: f32) {
    let cell = cell.clamp(2, SIDE);
    let n = SIDE / cell;
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    // how far across its cell each column is, eased
    let mut across = [0.0_f32; SIDE];
    for (x, f) in across.iter_mut().enumerate() {
        *f = ease((x % cell) as f32 / cell as f32);
    }
    let mut column = [0.0_f32; MOST_CELLS + 1];
    for (y, row) in sum.as_chunks_mut::<SIDE>().0.iter_mut().enumerate() {
        let (cy, fy) = (y / cell, ease((y % cell) as f32 / cell as f32));
        let (j0, j1) = ((cy % n) as u32, ((cy + 1) % n) as u32);
        for (i, v) in column.iter_mut().take(n + 1).enumerate() {
            let i = (i % n) as u32;
            let (top, bottom) = (hash(i, j0, seed), hash(i, j1, seed));
            *v = top + (bottom - top) * fy;
        }
        for (x, (pixel, fx)) in row.iter_mut().zip(across).enumerate() {
            let cx = x / cell;
            let (left, right) = (column.get(cx).copied().unwrap_or(0.0), column.get(cx + 1).copied().unwrap_or(0.0));
            *pixel += (left + (right - left) * fx) * strength;
        }
    }
}

/// Octaves of noise from cells `cells` pixels a side, each half as strong as the last, as a tile:
/// 0 to 1.
fn fractal(out: &mut [f32; SIDE * SIDE], cells: &[usize], seed: u32) {
    out.fill(0.0);
    let (mut weight, mut strength) = (0.0, 1.0);
    for (i, &cell) in cells.iter().enumerate() {
        add_noise(out, cell, seed.wrapping_add(i as u32 * 101), strength);
        weight += strength;
        strength *= 0.5;
    }
    for v in out.iter_mut() {
        *v /= weight;
    }
}

/// `v` pushed away from 0.5 by `k` (1 keeps it), within 0 to 1.
fn contrast(v: f32, k: f32) -> f32 {
    ((v - 0.5) * k + 0.5).clamp(0.0, 1.0)
}

/// A triangle wave over `period` pixels at `at`: 0 at its ends, 1 in its middle.
fn ridge(at: usize, period: usize) -> f32 {
    let t = (at % period) as f32 / period as f32;
    let d = 2.0 * t - 1.0;
    1.0 - if d < 0.0 { -d } else { d }
}

/// How high paper `kind` stands at each pixel of its tile, 0 (a hollow) to 1 (a peak), into `out`.
fn heights(kind: Kind, out: &mut [f32; SIDE * SIDE]) {
    let each = |out: &mut [f32; SIDE * SIDE], f: &dyn Fn(usize, usize, f32) -> f32| {
        for (i, v) in out.iter_mut().enumerate() {
            *v = f(i % SIDE, i / SIDE, *v);
        }
    };
    match kind {
        // medium tooth, blotchy: the hollows a strong brush still fills
        Kind::ColdPress => {
            fractal(out, &[32, 16, 8, 4], 11);
            each(out, &|_, _, v| contrast(v, 1.8));
        }
        // smooth, the tooth fine and shallow
        Kind::HotPress => {
            fractal(out, &[8, 4, 2], 23);
            each(out, &|_, _, v| 0.75 + 0.25 * contrast(v, 1.4));
        }
        // coarse, deep tooth
        Kind::Rough => {
            fractal(out, &[64, 32, 16, 8, 4], 37);
            each(out, &|_, _, v| contrast(v, 2.4));
        }
        // threads over and under each other, eight pixels apart, a little uneven
        Kind::Canvas => {
            fractal(out, &[16, 4], 41);
            each(out, &|x, y, v| {
                let over = (x / 8 + y / 8) % 2 == 0;
                let thread = if over { ridge(y, 8) } else { ridge(x, 8) };
                (0.15 + 0.75 * thread + 0.1 * v).clamp(0.0, 1.0)
            });
        }
        // fine laid lines across, chain lines down every 64 pixels, a light tooth over both
        Kind::Laid => {
            fractal(out, &[16, 8], 53);
            each(out, &|x, y, v| {
                let chain = if x % 64 < 3 { 0.35 } else { 0.0 };
                (0.35 + 0.45 * ridge(y, 4) - chain + 0.2 * v).clamp(0.0, 1.0)
            });
        }
    }
}

/// The papers by the Paper control's steps, and the colours by the Paper colour control's, as the
/// panel names them.
const PAPERS: [&str; 8] = ["None", "Cold press", "Hot press", "Rough", "Canvas", "Laid", "Your own", "Textures"];
const COLOUR_NAMES: [&str; 7] = ["Background", "White", "Cream", "Ivory", "Grey", "Kraft", "Black"];

/// The app's functions that set one of the plugin's own controls (interface 12) and show its panel.
#[cfg(target_arch = "wasm32")]
mod app {
    #[link(wasm_import_module = "procraft")]
    unsafe extern "C" {
        pub fn set_control(at: i32, value: f32) -> i32;
        pub fn show_panel(at: i32, len: i32) -> i32;
    }
}

/// What stands in the panel's rows for the row of the texture control, until it is shown: the kit
/// writes no such row (interface 15), so it goes in in its place.
const TEXTURE_ROW: &str = "@texture@";

/// `panel`'s text, the texture control's row where it stands in.
fn panel_text(panel: &kit::app::Panel) -> String {
    let stand_in = format!(r#"{{"type":"text","text":"{TEXTURE_ROW}"}}"#);
    panel.json().replace(&stand_in, r#"{"type":"control","key":"texture"}"#)
}

/// Shows `panel` (or shows it again, changed), the texture control's row where it stands in.
fn show(panel: &kit::app::Panel) {
    let text = panel_text(panel);
    // SAFETY: its own memory, read by the app as it is
    #[cfg(target_arch = "wasm32")]
    unsafe {
        app::show_panel(text.as_ptr() as i32, text.len() as i32);
    }
    let _ = text;
}

/// Control `at` set to `value`, as on the card (the app keeps it, with the artwork open if it's
/// kept there), and told here at once.
fn set(at: usize, value: f32) {
    // SAFETY: plain numbers; the app checks them
    #[cfg(target_arch = "wasm32")]
    unsafe {
        app::set_control(at as i32, value);
    }
    CONTROLS.told(at as i32, value);
}

/// The panel the Layers panel's Paper row opens, as the controls are now, and what each of its rows
/// sets: a control, and what the row's value is multiplied by to be its (none for words).
fn panel() -> (kit::app::Panel, Vec<Option<(usize, f32)>>) {
    let step = |at: usize| (CONTROLS.get(at) + 0.5) as usize;
    let shown = |at: usize| (CONTROLS.get(at) * 100.0).round();
    let mut sets = vec![Some((KIND, 1.0)), Some((COLOUR, 1.0)), Some((OWN_COLOUR, 1.0))];
    let mut panel = kit::app::Panel::new("Paper").choice("Paper", &PAPERS, step(KIND).min(PAPERS.len() - 1));
    // the texture picked, under the Paper while it is Textures
    if step(KIND) as u32 == TEXTURES_STEP {
        panel = panel.text(TEXTURE_ROW);
        sets.insert(1, None);
    }
    panel = panel
        .choice("Colour", &COLOUR_NAMES, step(COLOUR).min(COLOUR_NAMES.len() - 1))
        .switch("A colour of your own", CONTROLS.on(OWN_COLOUR));
    if CONTROLS.on(OWN_COLOUR) {
        panel = panel
            .slider("Hue", 0.0, 360.0, 1.0, CONTROLS.get(HUE).round(), "°")
            .slider("Saturation", 0.0, 100.0, 1.0, shown(SATURATION), "%")
            .slider("Brightness", 0.0, 100.0, 1.0, shown(BRIGHTNESS), "%");
        sets.extend([Some((HUE, 1.0)), Some((SATURATION, 0.01)), Some((BRIGHTNESS, 0.01))]);
    }
    panel = panel
        .slider("Grain", 0.0, 100.0, 5.0, shown(GRAIN), "%")
        .slider("Tooth", 0.0, 100.0, 5.0, shown(TOOTH), "%")
        .slider("Grain size", 50.0, 400.0, 25.0, shown(SIZE), "%")
        .slider("Texture under the art", 0.0, 100.0, 5.0, shown(SHOWN), "%");
    sets.extend([Some((GRAIN, 0.01)), Some((TOOTH, 0.01)), Some((SIZE, 0.01)), Some((SHOWN, 0.01))]);
    if step(KIND) as u32 == OWN_STEP && !CONTROLS.on(PICTURE) {
        panel = panel.text("Pick the picture for Your own paper on the Infinite Plugins card, under Paper.");
        sets.push(None);
    }
    (panel, sets)
}

/// Its row in the Layers panel tapped: its panel shown (none with its switch off).
pub fn act() {
    if CONTROLS.on(FEATURE) {
        show(&panel().0);
    }
}

/// A touch on its panel: row `row` told `value`, setting the control the row is.
pub fn touched(row: usize, value: f32) {
    if !CONTROLS.on(FEATURE) {
        return;
    }
    if let Some(Some((at, times))) = panel().1.get(row) {
        set(*at, value * times);
    }
    // shown again as it is now: a colour of your own's sliders come and go with its switch, the
    // texture with its step
    show(&panel().0);
}

/// Memory of the module's own, written by one call at a time (a plugin's module runs on one thread).
struct Owned<T>(UnsafeCell<T>);
// SAFETY: a plugin's module runs on one thread at a time
unsafe impl<T> Sync for Owned<T> {}

/// The tile as last drawn, its heights as they were worked out, and which paper that was (its
/// step, 0 for none yet).
static TILE: Owned<[u8; SIDE * SIDE]> = Owned(UnsafeCell::new([0; SIDE * SIDE]));
static HEIGHTS: Owned<[f32; SIDE * SIDE]> = Owned(UnsafeCell::new([0.0; SIDE * SIDE]));
static DRAWN: AtomicU32 = AtomicU32::new(0);

/// The tile of paper `kind`, drawn again only when the paper changes.
fn tile(kind: Kind) -> &'static [u8; SIDE * SIDE] {
    let step = kind as u32 + 1;
    // SAFETY: only this function writes them, never while another call reads them (one thread)
    let (pixels, worked) = unsafe { (&mut *TILE.0.get(), &mut *HEIGHTS.0.get()) };
    if DRAWN.load(Ordering::Relaxed) != step {
        heights(kind, worked);
        for (pixel, h) in pixels.iter_mut().zip(worked.iter()) {
            *pixel = (h * 255.0 + 0.5) as u8;
        }
        DRAWN.store(step, Ordering::Relaxed);
    }
    pixels
}

/// Where the paper's tile is, `side` square (the app asks for [`SIDE`]); 0 for no paper.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_paper(side: i32) -> i32 {
    match Kind::of(CONTROLS.get(KIND)) {
        Some(kind) if side == SIDE as i32 && CONTROLS.on(FEATURE) => tile(kind).as_ptr() as i32,
        _ => 0,
    }
}

/// The place of the control whose picture is the paper (the app makes the tile of it): Your own
/// paper's while the Paper control is on its step; else none (-1), the tile drawn here.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_paper_picture() -> i32 {
    if !CONTROLS.on(FEATURE) {
        return -1;
    }
    match (CONTROLS.get(KIND) + 0.5) as u32 {
        OWN_STEP => PICTURE as i32,
        // a picture of your own picked from the textures
        TEXTURES_STEP => {
            let k = -1 - CONTROLS.get(TEXTURE).round() as i32;
            usize::try_from(k).ok().and_then(|k| OWN.get(k)).map_or(-1, |&at| at as i32)
        }
        _ => -1,
    }
}

/// The app's grain that is the paper, by its number in the Source Library, while the Paper control
/// is Textures and the texture picked is one; else none (0).
#[unsafe(no_mangle)]
pub extern "C" fn procraft_paper_grain() -> i32 {
    let texture = CONTROLS.get(TEXTURE).round() as i32;
    let textures = (CONTROLS.get(KIND) + 0.5) as u32 == TEXTURES_STEP && CONTROLS.on(FEATURE);
    if textures && texture > 0 { texture } else { 0 }
}

/// How the paper is taken: how strongly its hollows take paint away (0), how many canvas pixels a
/// pixel of its tile covers (1), how deep it shows under the art (2), and how sharply only its
/// peaks take paint (3, its tooth).
#[unsafe(no_mangle)]
pub extern "C" fn procraft_paper_look(what: i32) -> f32 {
    match what {
        0 => CONTROLS.get(GRAIN).clamp(0.0, 1.0),
        1 => CONTROLS.get(SIZE).clamp(0.25, 16.0),
        2 => CONTROLS.get(SHOWN).clamp(0.0, 1.0),
        3 => CONTROLS.get(TOOTH).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// The colour under the art: the plugin's own from its Hue, Saturation and Brightness while A colour
/// of your own is on, else one of its colours, or -1 for the artwork's own.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_paper_colour() -> i32 {
    if CONTROLS.on(OWN_COLOUR) {
        return rgb(CONTROLS.get(HUE), CONTROLS.get(SATURATION), CONTROLS.get(BRIGHTNESS));
    }
    let step = (CONTROLS.get(COLOUR) + 0.5) as usize;
    step.checked_sub(1).and_then(|at| COLOURS.get(at)).copied().unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    const KINDS: [Kind; 5] = [Kind::ColdPress, Kind::HotPress, Kind::Rough, Kind::Canvas, Kind::Laid];

    /// Its turn at the controls, which every test shares: as they start, its switch on.
    fn alone() -> std::sync::MutexGuard<'static, ()> {
        crate::fresh(FEATURE)
    }

    #[test]
    fn on_textures_the_paper_is_the_grain_or_picture_its_texture_names_and_its_panel_shows_it() {
        let _alone = alone();
        procraft_control(KIND as i32, TEXTURES_STEP as f32);
        // none picked: no paper
        procraft_control(TEXTURE as i32, 0.0);
        assert_eq!((procraft_paper_grain(), procraft_paper_picture(), procraft_paper(SIDE as i32)), (0, -1, 0));
        // one of the app's grains, by its number in the Source Library
        procraft_control(TEXTURE as i32, 19.0);
        assert_eq!((procraft_paper_grain(), procraft_paper_picture()), (19, -1));
        // the third picture of your own; past the four, none
        procraft_control(TEXTURE as i32, -3.0);
        assert_eq!((procraft_paper_grain(), procraft_paper_picture()), (0, PICTURE3 as i32));
        procraft_control(TEXTURE as i32, -5.0);
        assert_eq!((procraft_paper_grain(), procraft_paper_picture()), (0, -1));
        // its panel: the texture's row under the Paper's, set as the card sets it (no row of the
        // panel's own)
        let (shown, sets) = panel();
        let text = panel_text(&shown);
        assert!(text.contains(r#"{"type":"control","key":"texture"}"#) && !text.contains(TEXTURE_ROW), "{text}");
        assert_eq!((sets[0], sets[1]), (Some((KIND, 1.0)), None));
        // another paper: neither, and no texture row
        procraft_control(KIND as i32, 1.0);
        procraft_control(TEXTURE as i32, 19.0);
        assert_eq!((procraft_paper_grain(), procraft_paper_picture()), (0, -1));
        assert!(!panel_text(&panel().0).contains("control"));
        // as it starts again
        procraft_control(KIND as i32, 0.0);
        procraft_control(TEXTURE as i32, 0.0);
    }

    /// Paper `kind`'s heights, worked out as its tile is.
    fn worked(kind: Kind) -> Box<[f32; SIDE * SIDE]> {
        let mut out: Box<[f32; SIDE * SIDE]> = vec![0.0; SIDE * SIDE].into_boxed_slice().try_into().unwrap();
        heights(kind, &mut out);
        out
    }

    #[test]
    fn each_paper_repeats_without_a_seam_and_has_a_tooth() {
        for kind in KINDS {
            let all = worked(kind);
            let at = |x: usize, y: usize| all[(y % SIDE) * SIDE + x % SIDE];
            // across the tile's edge it goes on as it does inside: no jump bigger than inside
            let step = |a: f32, b: f32| (a - b).abs();
            let inside: f32 = (1..SIDE).map(|x| step(at(x, 7), at(x - 1, 7))).fold(0.0, f32::max);
            assert!(step(at(0, 7), at(SIDE - 1, 7)) <= inside + 1e-6, "{kind:?} across");
            let inside: f32 = (1..SIDE).map(|y| step(at(7, y), at(7, y - 1))).fold(0.0, f32::max);
            assert!(step(at(7, 0), at(7, SIDE - 1)) <= inside + 1e-6, "{kind:?} down");
            // hollows and peaks, all within 0 to 1
            let all: Vec<f32> = (0..SIDE * SIDE).map(|i| at(i % SIDE, i / SIDE)).collect();
            let (low, high) = all.iter().fold((1.0_f32, 0.0_f32), |(l, h), &v| (l.min(v), h.max(v)));
            assert!(low >= 0.0 && high <= 1.0 && high - low > 0.2, "{kind:?}: {low} to {high}");
        }
        // each paper its own
        let first: Vec<u8> = KINDS.iter().map(|&k| (worked(k)[50 * SIDE + 30] * 255.0) as u8).collect();
        assert!(first.windows(2).any(|w| w[0] != w[1]));
    }

    #[test]
    fn the_app_is_given_the_paper_chosen_and_how_it_is_taken() {
        let _alone = alone();
        procraft_control(KIND as i32, 0.0);
        assert_eq!((procraft_paper(SIDE as i32), procraft_paper_picture()), (0, -1));
        procraft_control(KIND as i32, 3.0);
        assert_ne!(procraft_paper(SIDE as i32), 0);
        assert_eq!(procraft_paper_picture(), -1, "a paper drawn here");
        // a side it doesn't draw
        assert_eq!(procraft_paper(128), 0);
        assert_eq!(tile(Kind::Rough)[5], (worked(Kind::Rough)[5] * 255.0 + 0.5) as u8);
        // Your own paper: the picture picked for its control, no tile of its own
        procraft_control(KIND as i32, OWN_STEP as f32);
        assert_eq!((procraft_paper(SIDE as i32), procraft_paper_picture()), (0, PICTURE as i32));
        // how it's taken, as the controls say, within the app's ranges
        procraft_control(GRAIN as i32, 0.4);
        procraft_control(SIZE as i32, 2.0);
        procraft_control(TOOTH as i32, 0.75);
        assert_eq!([0, 1, 2, 3].map(|what| procraft_paper_look(what)), [0.4, 2.0, 0.3, 0.75]);
        procraft_control(TOOTH as i32, 3.0);
        assert_eq!((procraft_paper_look(3), procraft_paper_look(9)), (1.0, 0.0));
    }

    #[test]
    fn the_colour_under_the_art_is_one_of_its_own_or_any_from_its_sliders() {
        let _alone = alone();
        procraft_control(OWN_COLOUR as i32, 0.0);
        // the artwork's background colour, then each of its own
        procraft_control(COLOUR as i32, 0.0);
        assert_eq!(procraft_paper_colour(), -1);
        procraft_control(COLOUR as i32, 1.0);
        assert_eq!(procraft_paper_colour(), 0xFFFFFF);
        procraft_control(COLOUR as i32, 6.0);
        assert_eq!(procraft_paper_colour(), 0x1C1C1C);
        // a colour of your own, as its sliders start: a warm white, whatever the step
        procraft_control(OWN_COLOUR as i32, 1.0);
        assert_eq!(procraft_paper_colour(), 0xF5EDDC);
        procraft_control(HUE as i32, 210.0);
        procraft_control(SATURATION as i32, 0.5);
        procraft_control(BRIGHTNESS as i32, 0.4);
        assert_eq!(procraft_paper_colour(), 0x334D66);
        procraft_control(OWN_COLOUR as i32, 0.0);
        assert_eq!(procraft_paper_colour(), 0x1C1C1C);
    }

    #[test]
    fn its_panel_shows_its_controls_and_a_touch_sets_them() {
        let _alone = alone();
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        for (at, value) in [(KIND, 0.0), (OWN_COLOUR, 0.0), (GRAIN, 0.6), (SIZE, 1.0), (PICTURE, 0.0)] {
            procraft_control(at as i32, value);
        }
        // the paper, its colour, a colour of your own's switch, and its four sliders in percent
        let (shown, sets) = panel();
        let json = shown.json();
        assert!(json.contains(r#""title":"Paper""#) && json.contains(r#""label":"Grain","least":0"#), "{json}");
        assert!(json.contains(r#""value":60"#) && json.contains(r#""value":100"#), "{json}");
        assert_eq!(sets.len(), 7);
        // the switch turned on: the colour's three sliders show after it
        touched(2, 1.0);
        assert!(CONTROLS.on(OWN_COLOUR));
        let sets = panel().1;
        assert_eq!((sets.len(), sets[3], sets[6]), (10, Some((HUE, 1.0)), Some((GRAIN, 0.01))));
        // a slider's percent: its control's share
        touched(6, 40.0);
        touched(8, 200.0);
        touched(5, 50.0);
        assert!(near(CONTROLS.get(GRAIN), 0.4) && near(CONTROLS.get(SIZE), 2.0), "{}", CONTROLS.get(GRAIN));
        assert!(near(CONTROLS.get(BRIGHTNESS), 0.5));
        // a choice's place: the step; Your own paper, with no picture picked, says where to pick one
        touched(1, 4.0);
        assert_eq!(procraft_paper_colour(), rgb(CONTROLS.get(HUE), CONTROLS.get(SATURATION), 0.5));
        touched(0, 6.0);
        assert_eq!(procraft_paper_picture(), PICTURE as i32);
        assert_eq!((panel().1.len(), panel().1.last().copied()), (11, Some(None)));
        procraft_control(PICTURE as i32, 1.0);
        assert_eq!(panel().1.len(), 10);
        // its switch off again: the colour as chosen
        touched(2, 0.0);
        assert_eq!(procraft_paper_colour(), COLOURS[3]);
        for (at, value) in [(KIND, 0.0), (COLOUR, 0.0), (GRAIN, 0.6), (SIZE, 1.0), (BRIGHTNESS, 0.96), (PICTURE, 0.0)] {
            procraft_control(at as i32, value);
        }
    }

    #[test]
    fn with_its_switch_off_there_is_no_paper_and_its_panel_sets_nothing() {
        let _alone = alone();
        procraft_control(KIND as i32, 3.0);
        assert_ne!(procraft_paper(SIDE as i32), 0);
        procraft_control(KIND as i32, TEXTURES_STEP as f32);
        procraft_control(TEXTURE as i32, 19.0);
        assert_eq!(procraft_paper_grain(), 19);
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!((procraft_paper(SIDE as i32), procraft_paper_picture(), procraft_paper_grain()), (0, -1, 0));
        touched(0, 1.0);
        assert_eq!(CONTROLS.get(KIND), TEXTURES_STEP as f32);
        // on again: as it was
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(procraft_paper_grain(), 19);
        procraft_control(KIND as i32, 0.0);
        procraft_control(TEXTURE as i32, 0.0);
    }
}
