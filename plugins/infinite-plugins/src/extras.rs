//! Colour extras: two ways round the colour panel. Its sliders step one unit at a tap on either end
//! (a degree of hue, a percent of saturation or brightness, a level of red, green or blue), and its
//! swatch shows the colour as the panel opened beside the colour now, a tap on the first going back
//! to it. Each has a switch of its own, under the feature's.
//!
//! It answers the `colour_panel` hook.

use crate::{COLOUR_EXTRAS as FEATURE, COMPARE, CONTROLS, STEPS};

/// What the colour panel's hook asks: whether its sliders step at their ends, and whether its
/// swatch shows the colour before beside the colour now.
const ASKED_STEPS: i32 = 0;
const ASKED_COMPARE: i32 = 1;

/// Whether the colour panel does `part` of what it may (not 0 for yes).
#[unsafe(no_mangle)]
pub extern "C" fn procraft_colour_panel(part: i32) -> i32 {
    let on = match part {
        ASKED_STEPS => CONTROLS.on(STEPS),
        ASKED_COMPARE => CONTROLS.on(COMPARE),
        _ => false,
    };
    i32::from(CONTROLS.on(FEATURE) && on)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn each_part_is_as_its_switch_says_and_what_it_doesn_t_know_is_no() {
        let _turn = crate::fresh(FEATURE);
        // as it starts: neither
        assert_eq!((procraft_colour_panel(ASKED_STEPS), procraft_colour_panel(ASKED_COMPARE)), (0, 0));
        procraft_control(STEPS as i32, 1.0);
        assert_eq!((procraft_colour_panel(ASKED_STEPS), procraft_colour_panel(ASKED_COMPARE)), (1, 0));
        procraft_control(COMPARE as i32, 1.0);
        procraft_control(STEPS as i32, 0.0);
        assert_eq!((procraft_colour_panel(ASKED_STEPS), procraft_colour_panel(ASKED_COMPARE)), (0, 1));
        for part in [2, -1, 99] {
            assert_eq!(procraft_colour_panel(part), 0, "{part}");
        }
    }

    #[test]
    fn with_its_switch_off_the_panel_is_the_app_s_own() {
        let _turn = crate::fresh(FEATURE);
        procraft_control(STEPS as i32, 1.0);
        procraft_control(COMPARE as i32, 1.0);
        assert_eq!((procraft_colour_panel(ASKED_STEPS), procraft_colour_panel(ASKED_COMPARE)), (1, 1));
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!((procraft_colour_panel(ASKED_STEPS), procraft_colour_panel(ASKED_COMPARE)), (0, 0));
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!((procraft_colour_panel(ASKED_STEPS), procraft_colour_panel(ASKED_COMPARE)), (1, 1));
    }
}
