//! Brush extras: five ways round the brush and the stroke, each a feature with a switch of its own
//! and its controls under it. Brush randomiser: Randomise in the brush studio ([`randomise`]).
//! Brush sounds: a sound as you draw, by the brush ([`sounds`]). Finger pressure: a finger draws
//! with a pressure of its own ([`pressure`]). Touch position offset: strokes land off the pen's
//! tip ([`offset`]). QuickShape curves: a stroke that bends more than once snaps to a curve
//! ([`curves`]). It's a plugin made of those features, not part of the app:
//! `tools/pack_plugin.py` makes a `.procraft-plugin` file of it.
//!
//! The app runs each feature that is on in a copy of this code of its own, and tells that copy the
//! other features' switches as off; so each hook answers for its own feature alone, and as the app
//! has it while that feature's switch is off. The interface is written down at the top of
//! `crates/core/src/plugin_run.rs`.

mod curves;
mod offset;
mod pressure;
mod randomise;
mod sounds;

/// The controls by their place in the manifest, as they start: each feature's switch off, and no
/// recording picked.
const STARTS: [f32; 22] = [
    0.0, // Brush randomiser
    0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // Brush sounds
    0.0, 0.7, 20.0, 40.0, 0.1, 1.0, // Finger pressure
    0.0, 0.0, 0.0, // Touch position offset
    0.0, // QuickShape curves
];
static CONTROLS: kit::Controls<22> = kit::Controls::new(STARTS);

// a recording let go plays its sound's own again
kit::controls!(CONTROLS, sounds::control_told);

/// Brush randomiser's switch: it has no controls of its own.
pub const RANDOMISE: usize = 0;
/// Brush sounds' switch, then its own: the volume (0 to 1), the sound for every brush (0 each
/// brush's kind's, 1 to 6 one of the six), whether erasing makes a sound (0 or 1), the sound erasing
/// makes (1 to 6), then for each of the six sounds whether a recording is picked (0 or 1).
pub const BRUSH_SOUNDS: usize = 1;
pub const VOLUME: usize = 2;
pub const SOUND: usize = 3;
pub const ERASER: usize = 4;
pub const ERASER_SOUND: usize = 5;
pub const ERASER_RECORDING: usize = 6;
pub const PENCIL_RECORDING: usize = 7;
pub const PEN_RECORDING: usize = 8;
pub const PAINTBRUSH_RECORDING: usize = 9;
pub const AIRBRUSH_RECORDING: usize = 10;
pub const SPRAY_RECORDING: usize = 11;
/// Finger pressure's switch, then its own: how much speed takes the pressure down (0..1), the
/// start and end tapers (window points), and the least and most pressure.
pub const FINGER_PRESSURE: usize = 12;
pub const SPEED: usize = 13;
pub const START: usize = 14;
pub const END: usize = 15;
pub const LEAST: usize = 16;
pub const MOST: usize = 17;
/// Touch position offset's switch, then its own: across and down, in window pixels.
pub const TOUCH_OFFSET: usize = 18;
pub const X: usize = 19;
pub const Y: usize = 20;
/// QuickShape curves' switch: it has no controls of its own.
pub const QUICKSHAPE_CURVES: usize = 21;

// a recording picked for one of Brush sounds' six sounds: its file controls are the only ones
kit::files!(sounds::file_told);

/// Each feature's switch, in order: what comes after one, up to the next, is its own.
#[cfg(test)]
const FEATURES: [usize; 5] = [RANDOMISE, BRUSH_SOUNDS, FINGER_PRESSURE, TOUCH_OFFSET, QUICKSHAPE_CURVES];

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
