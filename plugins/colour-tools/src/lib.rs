//! Colour tools: three ways round colour, each a feature with a switch of its own and its controls
//! under it. Colour triangle: the colour panel's Classic tab picks saturation and brightness in a
//! triangle, with the hue on a ring round it if you like ([`triangle`]). Colour labels: a layer's
//! colour label, from its options, shown on its row in the Layers panel ([`labels`]). ColorDrop
//! tools: a fill by every layer seen, kept out of gaps in the lines ([`colordrop`]). It's a plugin made of those features, not part of the app:
//! `tools/pack_plugin.py` makes a `.procraft-plugin` file of it.
//!
//! The app runs each feature that is on in a copy of this code of its own, and tells that copy the
//! other features' switches as off; so each hook answers for its own feature alone, and as the app
//! has it while that feature's switch is off. The interface is written down at the top of
//! `crates/core/src/plugin_run.rs`.

mod colordrop;
mod labels;
mod triangle;

/// The controls by their place in the manifest, as they start: each feature's switch off.
const STARTS: [f32; 9] = [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
static CONTROLS: kit::Controls<9> = kit::Controls::new(STARTS);
kit::controls!(CONTROLS);

/// Colour triangle's switch, then its own: the triangle in Classic, the hue ring round it, and the
/// floating colour window as the picker alone.
pub const COLOUR_TRIANGLE: usize = 0;
pub const TRIANGLE: usize = 1;
pub const RING: usize = 2;
pub const PICKER_ONLY: usize = 3;
/// Colour labels' switch: it has no controls of its own, only its action.
pub const COLOUR_LABELS: usize = 4;
/// ColorDrop tools' switch, then its own: a fill by all layers, gap closing and its gap (whole
/// pixels, from Off).
pub const COLORDROP: usize = 5;
pub const ALL_LAYERS: usize = 6;
pub const GAP_CLOSING: usize = 7;
pub const GAP: usize = 8;

/// The actions by their place in the manifest: Colour label, in a layer's options.
const LABEL: usize = 0;

kit::actions!(|index| {
    if index == LABEL {
        labels::act();
    }
});

// Colour labels' panel is the only one shown
kit::panel!(labels::touched);

/// Each feature's switch, in order: what comes after one, up to the next, is its own.
#[cfg(test)]
const FEATURES: [usize; 3] = [COLOUR_TRIANGLE, COLOUR_LABELS, COLORDROP];

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
