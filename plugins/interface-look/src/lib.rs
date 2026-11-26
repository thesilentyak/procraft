//! Interface look: the interface as one likes it, four features each with a switch of its own and
//! its controls under it. Interface: its size (part by part if one likes) and accent colour, and
//! undo and redo or every action without a notice ([`interface`]). Slider look: the side bar's
//! sliders as frosted glass ([`sliders`]). Canvas surround: the area round the canvas in a colour of
//! one's own, or a picture ([`surround`]). Coloured brush previews: the brush library's strokes in
//! the colour one paints with ([`previews`]). Floating panels and panels left open went to Edit
//! layout, with what was set for them. It's a plugin made of those features, not part of the app:
//! `tools/pack_plugin.py` makes a `.procraft-plugin` file of it.
//!
//! The app runs each feature that is on in a copy of this code of its own, and tells that copy the
//! other features' switches as off; so each hook answers for its own feature alone, and as the app
//! has it while that feature's switch is off. The interface is written down at the top of
//! `crates/core/src/plugin_run.rs`.

mod interface;
mod previews;
mod sliders;
mod surround;

/// The controls by their place in the manifest, as they start: each feature's switch off, and
/// Canvas surround's picture none.
const STARTS: [f32; 30] = [
    0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 212.0, 1.0, 0.0, 0.0, // Interface
    0.0, 0.35, 1.0, // Slider look
    0.0, 30.0, 0.1, 0.3, 0.0, 0.0, // Canvas surround
    0.0, 0.0, 200.0, 0.6, 1.0, 0.0, // Coloured brush previews
];
static CONTROLS: kit::Controls<30> = kit::Controls::new(STARTS);
kit::controls!(CONTROLS);

/// Interface's switch, then its own: the size's switch, the size, whether the parts are sized
/// separately, the parts' sizes (in the order the app numbers the parts: floating panels, popovers
/// and menus, the side bar, the top bar, the panels), keeping clear, the accent's switch, hue and
/// saturation, and the switches for the notices.
pub const INTERFACE: usize = 0;
pub const SIZE_ON: usize = 1;
pub const SIZE: usize = 2;
pub const PARTS: usize = 3;
pub const FLOATING: usize = 4;
pub const MENUS: usize = 5;
pub const SIDE_BAR: usize = 6;
pub const TOP_BAR: usize = 7;
pub const PANELS: usize = 8;
pub const CLEAR: usize = 9;
pub const COLOUR: usize = 10;
pub const HUE: usize = 11;
pub const SATURATION: usize = 12;
pub const HIDE_UNDO: usize = 13;
pub const HIDE_NOTICES: usize = 14;
/// Slider look's switch, then its own: how opaque the tracks are (0 to 1), and whether what is
/// behind them is frosted.
pub const SLIDER_LOOK: usize = 15;
pub const SLIDER_OPACITY: usize = 16;
pub const FROSTED: usize = 17;
/// Canvas surround's switch, then its own: the colour's hue (degrees), saturation and brightness
/// (0 to 1), whether a picture shows, and whether one is picked.
pub const CANVAS_SURROUND: usize = 18;
pub const SURROUND_HUE: usize = 19;
pub const SURROUND_SATURATION: usize = 20;
pub const SURROUND_BRIGHTNESS: usize = 21;
pub const OWN_PICTURE: usize = 22;
pub const PICTURE: usize = 23;
/// Coloured brush previews' switch, then its own: whether it draws in a colour of its own, that
/// colour's hue (degrees), saturation and brightness (0 to 1), and whether the brush in use is
/// drawn so too.
pub const COLOURED_PREVIEWS: usize = 24;
pub const OWN_COLOUR: usize = 25;
pub const PREVIEW_HUE: usize = 26;
pub const PREVIEW_SATURATION: usize = 27;
pub const PREVIEW_BRIGHTNESS: usize = 28;
pub const CHOSEN: usize = 29;

/// The colour (0xRRGGBB) the interface draws `role` in: the accent as Interface says, the surround
/// as Canvas surround says (each leaves every other role); -1 for any other role, or one whose
/// feature is off, left as the app has it.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_interface_colour(role: i32) -> i32 {
    match interface::colour(role) {
        -1 => surround::colour(role),
        accent => accent,
    }
}

/// `hue` (degrees), `saturation` and `brightness` (0 to 1) as 0xRRGGBB: Canvas surround's and
/// Coloured brush previews' own colours.
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

/// Each feature's switch, in order: what comes after one, up to the next, is its own.
#[cfg(test)]
const FEATURES: [usize; 4] = [INTERFACE, SLIDER_LOOK, CANVAS_SURROUND, COLOURED_PREVIEWS];

/// The tests one at a time, as they share the controls.
#[cfg(test)]
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A test's turn on `feature`: its controls as they start, then its switch on. The other features'
/// controls are left as they are.
#[cfg(test)]
fn fresh(feature: usize) -> std::sync::MutexGuard<'static, ()> {
    let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let end = FEATURES.iter().copied().find(|&next| next > feature).unwrap_or(STARTS.len());
    for (at, start) in STARTS.iter().enumerate().take(end).skip(feature) {
        procraft_control(at as i32, *start);
    }
    procraft_control(feature as i32, 1.0);
    turn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accent_and_the_surround_each_go_by_their_own_feature() {
        // both features' switches, as the app tells each copy, put back before the next test
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let both = || (procraft_interface_colour(0), procraft_interface_colour(1));
        let (interface_was, surround_was) = (CONTROLS.get(INTERFACE), CONTROLS.get(CANVAS_SURROUND));
        procraft_control(COLOUR as i32, 1.0);
        procraft_control(HUE as i32, 212.0);
        procraft_control(SATURATION as i32, 1.0);
        for (at, start) in STARTS.iter().enumerate().take(PICTURE).skip(SURROUND_HUE) {
            procraft_control(at as i32, *start);
        }
        // Interface on alone: the accent, and the surround left; Canvas surround alone, the other
        // way round
        procraft_control(INTERFACE as i32, 1.0);
        procraft_control(CANVAS_SURROUND as i32, 0.0);
        assert_eq!(both(), (0x0076FD, -1));
        procraft_control(INTERFACE as i32, 0.0);
        procraft_control(CANVAS_SURROUND as i32, 1.0);
        assert_eq!(both(), (-1, 0x4D4945));
        // a role neither knows
        assert_eq!((procraft_interface_colour(2), procraft_interface_colour(-1)), (-1, -1));
        procraft_control(INTERFACE as i32, interface_was);
        procraft_control(CANVAS_SURROUND as i32, surround_was);
    }
}
