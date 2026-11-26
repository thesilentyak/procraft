//! Infinite Plugins: five features painters asked for, each with a switch of its own and its
//! controls under it. Lasso fill: strokes that fill what they go round ([`lasso`]). Paper: a paper
//! for each artwork, its grain in every brush and shown under the art ([`paper`]). Trace view: Solo
//! shows the layer you're on alone, Trace dims the rest ([`trace`]). Filter layers: layers of a
//! filter from the Layers panel's +, their filters in `filters/` (the app runs them; no code of
//! this module's). Colour extras: the colour panel's sliders step at their ends, and its swatch
//! shows the colour before ([`extras`]). It's a plugin made of those features, not part of the app:
//! `tools/pack_plugin.py` makes a `.procraft-plugin` file of it.
//!
//! The app runs each feature that is on in a copy of this code of its own, and tells that copy the
//! other features' switches as off; so each hook answers for its own feature alone, and as the app
//! has it while that feature's switch is off. The interface is written down at the top of
//! `crates/core/src/plugin_run.rs`.

use core::sync::atomic::{AtomicU32, Ordering};

mod extras;
mod lasso;
mod paper;
mod trace;

/// The controls by their place in the manifest, as they start: each feature's switch off.
const STARTS: [f32; 27] = [
    0.0, 0.0, 1.0, 0.0, // Lasso fill
    0.0, 0.0, 0.0, 0.6, 1.0, 0.3, 0.0, 0.0, 40.0, 0.1, 0.96, 0.0, 0.0, 0.0, 0.0, 0.0, // Paper
    0.0, 0.0, 0.7, // Trace view
    0.0, // Filter layers
    0.0, 0.0, 0.0, // Colour extras
];
static CONTROLS: kit::Controls<27> = kit::Controls::new(STARTS);
kit::controls!(CONTROLS);

/// Lasso fill's switch, then its own: every stroke fills, the fill shows as it grows, and the
/// eraser fills too.
pub const LASSO_FILL: usize = 0;
pub const EVERY: usize = 1;
pub const GROWING: usize = 2;
pub const ERASER: usize = 3;
/// Paper's switch, then its own: which paper, its colour, how it takes paint and shows, a colour of
/// your own, and the pictures of your own with the texture picked among them and the app's grains.
pub const PAPER: usize = 4;
pub const KIND: usize = 5;
pub const COLOUR: usize = 6;
pub const GRAIN: usize = 7;
pub const SIZE: usize = 8;
pub const SHOWN: usize = 9;
pub const TOOTH: usize = 10;
pub const OWN_COLOUR: usize = 11;
pub const HUE: usize = 12;
pub const SATURATION: usize = 13;
pub const BRIGHTNESS: usize = 14;
pub const PICTURE: usize = 15;
pub const TEXTURE: usize = 16;
pub const PICTURE2: usize = 17;
pub const PICTURE3: usize = 18;
pub const PICTURE4: usize = 19;
/// Trace view's switch, then its own: the Layer view (Normal, Solo or Trace), and how far Trace
/// dims the others.
pub const TRACE_VIEW: usize = 20;
pub const VIEW: usize = 21;
pub const DIM: usize = 22;
/// Filter layers' switch: its filters are listed in the manifest, and it has no controls.
pub const FILTER_LAYERS: usize = 23;
/// Colour extras' switch, then its own: the sliders' steps, and the colour before beside now.
pub const COLOUR_EXTRAS: usize = 24;
pub const STEPS: usize = 25;
pub const COMPARE: usize = 26;

/// The actions by their place in the manifest: Paper's row in the Layers panel, then Trace view's
/// row there and its two keys.
const PAPER_ROW: usize = 0;
pub const LAYER_VIEW: usize = 1;
pub const SOLO_KEY: usize = 2;
pub const TRACE_KEY: usize = 3;

/// Whose panel is showing (its feature's switch, +1; 0 none): the one whose action showed it last
/// in this copy of the code, as its touches come to this copy.
static SHOWING: AtomicU32 = AtomicU32::new(0);

kit::actions!(|index| match index {
    PAPER_ROW => {
        SHOWING.store(PAPER as u32 + 1, Ordering::Relaxed);
        paper::act();
    }
    LAYER_VIEW | SOLO_KEY | TRACE_KEY => {
        if index == LAYER_VIEW {
            SHOWING.store(TRACE_VIEW as u32 + 1, Ordering::Relaxed);
        }
        trace::act(index);
    }
    _ => {}
});

kit::panel!(|row, value| {
    match (SHOWING.load(Ordering::Relaxed) as usize).checked_sub(1) {
        Some(PAPER) => paper::touched(row, value),
        Some(TRACE_VIEW) => trace::touched(row, value),
        _ => {}
    }
});

/// Each feature's switch, in order: what comes after one, up to the next, is its own.
#[cfg(test)]
const FEATURES: [usize; 5] = [LASSO_FILL, PAPER, TRACE_VIEW, FILTER_LAYERS, COLOUR_EXTRAS];

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
    fn its_controls_start_as_the_manifest_starts_them_each_feature_off() {
        let text = include_str!("../craft-extension.json");
        let manifest: Vec<&str> = text.lines().filter(|l| l.trim_start().starts_with(r#"{"key""#)).collect();
        // the controls' lines come first, then the brush rows' and the actions'
        let controls = &manifest[..STARTS.len()];
        let feature = |line: &str| line.contains(r#""type": "feature""#);
        for (at, line) in controls.iter().enumerate() {
            if feature(line) {
                assert!(FEATURES.contains(&at) && STARTS[at] == 0.0, "{line}");
            }
            let starts = line.split(r#""starts": "#).nth(1).map(|rest| rest.split([',', '}']).next().unwrap_or(""));
            let wanted = match starts {
                Some("true") => 1.0,
                Some("false") | None => 0.0,
                Some(number) => number.parse::<f32>().unwrap(),
            };
            assert_eq!(STARTS[at], wanted, "{line}");
        }
        assert!(!feature(manifest[STARTS.len()]), "as many controls as it starts");
    }

    #[test]
    fn a_panel_s_touches_go_to_the_feature_that_showed_it_and_none_with_none_shown() {
        let _turn = fresh(PAPER);
        procraft_control(TRACE_VIEW as i32, 1.0);
        procraft_control(DIM as i32, 0.7);
        // none shown: a touch sets nothing
        SHOWING.store(0, Ordering::Relaxed);
        procraft_panel_event(0, 3.0);
        assert_eq!((CONTROLS.get(KIND), CONTROLS.get(VIEW)), (0.0, 0.0));
        // Paper's row: its first row is the paper
        procraft_action(PAPER_ROW as i32);
        procraft_panel_event(0, 3.0);
        assert_eq!((CONTROLS.get(KIND), CONTROLS.get(VIEW)), (3.0, 0.0));
        // Layer view's: its first row is the view
        procraft_action(LAYER_VIEW as i32);
        procraft_panel_event(0, 2.0);
        assert_eq!((CONTROLS.get(KIND), CONTROLS.get(VIEW)), (3.0, 2.0));
        // a key shows no panel: the touches still go to the view's
        procraft_action(SOLO_KEY as i32);
        procraft_panel_event(1, 40.0);
        assert!((CONTROLS.get(DIM) - 0.4).abs() < 1e-6 && CONTROLS.get(KIND) == 3.0);
        for (at, value) in [(KIND, 0.0), (VIEW, 0.0), (DIM, 0.7), (TRACE_VIEW, 0.0)] {
            procraft_control(at as i32, value);
        }
    }
}
