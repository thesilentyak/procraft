//! Reference windows: more than one Reference window open at once. While it's on, + on a window's
//! top bar opens another a little below it, showing the canvas or a picture imported into it; each
//! is moved, sized, zoomed and closed on its own and kept with the artwork, as the first is. Off,
//! there is the one, as the app has it.
//!
//! The app asks the reference windows' hook how many may be open, and does the rest.

use crate::{CONTROLS, REFERENCE_WINDOWS as FEATURE, WINDOWS as MOST};

/// How many Reference windows may be open at once: as the control says, 2 to 8; the app's own one
/// with its switch off.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_reference_windows() -> i32 {
    if !CONTROLS.on(FEATURE) {
        return 1;
    }
    let most = CONTROLS.get(MOST);
    if most.is_finite() { ((most + 0.5) as i32).clamp(2, 8) } else { 2 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn as_many_windows_as_the_control_says_within_two_to_eight() {
        let _turn = crate::fresh(FEATURE);
        assert_eq!(procraft_reference_windows(), 3);
        for (set, said) in [(5.0, 5), (2.0, 2), (8.0, 8), (20.0, 8), (0.0, 2), (f32::NAN, 2)] {
            procraft_control(MOST as i32, set);
            assert_eq!(procraft_reference_windows(), said, "{set}");
        }
    }

    #[test]
    fn with_its_switch_off_there_is_the_one_window() {
        let _turn = crate::fresh(FEATURE);
        procraft_control(MOST as i32, 6.0);
        assert_eq!(procraft_reference_windows(), 6);
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!(procraft_reference_windows(), 1);
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(procraft_reference_windows(), 6);
    }
}
