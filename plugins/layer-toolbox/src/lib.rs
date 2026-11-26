//! Layer tools: three ways round the layers, each a feature with a switch of its own and its
//! controls under it. Layer tools: picked layers take one opacity together, Paste goes into a mask,
//! and the eyedropper picks from the current layer alone ([`tools`]). Layer fills: a grid layer in
//! the brush's colour, from its action ([`fills`]), and paper behind a layer's paint, a filter
//! (`filters/paper.wgsl`). Effects: filters under Adjustments, each under a switch of its own
//! (`filters/extract_lines.wgsl`, `filters/outline.wgsl` and `filters/posterise.wgsl`); it has no
//! code. It's a plugin made of those features, not part of the app: `tools/pack_plugin.py` makes a
//! `.procraft-plugin` file of it.
//!
//! The app runs each feature that is on in a copy of this code of its own, and tells that copy the
//! other features' switches as off; so each hook answers for its own feature alone, and as the app
//! has it while that feature's switch is off. The interface is written down at the top of
//! `crates/core/src/plugin_run.rs`.

mod fills;
mod tools;

/// The controls by their place in the manifest, as they start: each feature's switch off. The
/// filters' sliders are here too, though only the filters read them.
const STARTS: [f32; 21] = [
    0.0, 0.0, 0.0, 0.0, // Layer tools
    0.0, 0.0, 100.0, 2.0, 0.0, 0.5, 3.0, 0.3, // Layer fills
    0.0, 0.0, 0.15, 0.1, 0.0, 0.0, 3.0, 0.0, 4.0, // Effects
];
static CONTROLS: kit::Controls<21> = kit::Controls::new(STARTS);
kit::controls!(CONTROLS);

/// Layer tools' switch, then its own: the picked layers' opacity, Paste into masks and the
/// eyedropper on the layer (the last two kept as the settings the app used to have).
pub const LAYER_TOOLS: usize = 0;
pub const OPACITY: usize = 1;
pub const PASTE: usize = 2;
pub const EYEDROPPER: usize = 3;
/// Layer fills' switch, then its own: the grid's switch, how far apart its lines are and how wide
/// (pixels), then the paper's switch and its sliders (its filter's).
pub const LAYER_FILLS: usize = 4;
pub const GRID: usize = 5;
pub const SPACING: usize = 6;
pub const LINE: usize = 7;
pub const PAPER: usize = 8;
pub const GRAIN: usize = 9;
pub const SCALE: usize = 10;
pub const WARMTH: usize = 11;
/// Effects' switch, then its own: each filter's switch and the sliders it reads.
pub const EFFECTS: usize = 12;
pub const EXTRACT_LINES: usize = 13;
pub const CUT: usize = 14;
pub const SOFT: usize = 15;
pub const KEEP_COLOUR: usize = 16;
pub const OUTLINE: usize = 17;
pub const REACH: usize = 18;
pub const POSTERISE: usize = 19;
pub const STEPS: usize = 20;

/// The actions by their place in the manifest: Add a grid layer.
const ADD_GRID: usize = 0;

kit::actions!(|index| {
    if index == ADD_GRID {
        fills::add_grid();
    }
});

/// Each feature's switch, in order: what comes after one, up to the next, is its own.
#[cfg(test)]
const FEATURES: [usize; 3] = [LAYER_TOOLS, LAYER_FILLS, EFFECTS];

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
