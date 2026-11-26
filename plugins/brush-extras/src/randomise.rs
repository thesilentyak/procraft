//! Brush randomiser: a Randomise button in the brush studio sets the brush's settings at random,
//! within bounds that keep it something that still paints. Off, the brush studio has no Randomise.
//!
//! The app asks about each setting by its number (the list is in the plugin guide) with a seed
//! for the press, and sets what comes back.

use crate::{CONTROLS, RANDOMISE as FEATURE};

/// Where each setting may go, as a share of its range: (least, most), by the setting's number. A
/// share below 0 leaves the setting as it is.
const BOUNDS: [(f32, f32); 36] = [
    (0.02, 0.45), // spacing: never so wide the stamps fall apart
    (0.0, 0.8),   // streamline
    (0.0, 1.0),   // streamline pressure
    (0.0, 0.5),   // pen taper start
    (0.0, 0.5),   // pen taper end
    (0.0, 1.0),   // pen taper size
    (0.0, 1.0),   // pen taper opacity
    (0.0, 0.5),   // touch taper start
    (0.0, 0.5),   // touch taper end
    (0.0, 1.0),   // touch taper size
    (0.0, 1.0),   // touch taper opacity
    (-1.0, -1.0), // input style: left as it is
    (0.0, 1.0),   // shape rotation
    (0.0, 0.6),   // scatter
    (0.0, 0.3),   // count
    (0.0, 1.0),   // randomised
    (0.3, 1.0),   // roundness
    (0.0, 1.0),   // shape angle
    (0.0, 1.0),   // grain behaviour
    (0.05, 0.6),  // grain scale
    (0.0, 1.0),   // grain depth
    (-1.0, -1.0), // rendering mode: left as it is
    (0.4, 1.0),   // flow
    (0.0, 0.5),   // size jitter
    (0.0, 0.5),   // opacity jitter
    (0.5, 1.0),   // pressure size
    (0.5, 1.0),   // pressure opacity
    (0.0, 1.0),   // tilt angle
    (0.0, 1.0),   // tilt opacity
    (0.0, 1.0),   // tilt size
    (0.3, 0.9),   // max size
    (0.0, 0.3),   // min size
    (0.7, 1.0),   // max opacity
    (0.0, 0.5),   // min opacity
    (0.0, 1.0),   // smudge pull
    (0.0, 1.0),   // wet pull
];

/// Where setting `setting` goes this press: below 0, left as it is, with its switch off.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_randomise(setting: i32, seed: i32) -> f32 {
    if !CONTROLS.on(FEATURE) {
        return -1.0;
    }
    let Some(&(least, most)) = usize::try_from(setting).ok().and_then(|i| BOUNDS.get(i)) else { return -1.0 };
    if least < 0.0 {
        return -1.0;
    }
    least + (most - least) * unit(seed as u32, setting as u32)
}

/// A number from 0 to 1 that `seed` and `setting` stir up (a splitmix-style mix).
fn unit(seed: u32, setting: u32) -> f32 {
    let mut x = seed ^ setting.wrapping_mul(0x9E37_79B9);
    x = (x ^ (x >> 16)).wrapping_mul(0x7FEB_352D);
    x = (x ^ (x >> 15)).wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn each_setting_goes_within_its_bounds_and_those_left_are_left() {
        let _turn = crate::fresh(FEATURE);
        for seed in [0, 1, 7, -1, i32::MAX, 123_456] {
            for (setting, &(least, most)) in BOUNDS.iter().enumerate() {
                let to = procraft_randomise(setting as i32, seed);
                if least < 0.0 {
                    assert_eq!(to, -1.0, "{setting}");
                } else {
                    assert!((least..=most).contains(&to), "{setting} {seed}: {to}");
                }
            }
        }
        // a setting it doesn't know is left
        assert_eq!((procraft_randomise(-1, 3), procraft_randomise(BOUNDS.len() as i32, 3)), (-1.0, -1.0));
        // the same seed and setting, the same place; another seed, most likely another
        assert_eq!(procraft_randomise(0, 42), procraft_randomise(0, 42));
        assert_ne!(procraft_randomise(0, 42), procraft_randomise(0, 43));
    }

    #[test]
    fn with_its_switch_off_every_setting_is_left() {
        let _turn = crate::fresh(FEATURE);
        assert!(procraft_randomise(0, 42) >= 0.0);
        procraft_control(FEATURE as i32, 0.0);
        for setting in 0..BOUNDS.len() as i32 {
            assert!(procraft_randomise(setting, 42) < 0.0, "{setting}");
        }
        procraft_control(FEATURE as i32, 1.0);
        let (least, most) = BOUNDS[0];
        assert_eq!(procraft_randomise(0, 42), least + (most - least) * unit(42, 0));
    }
}
