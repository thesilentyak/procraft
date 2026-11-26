//! Slider look: the side bar's size and opacity sliders as see-through, frosted glass, the canvas
//! behind them blurred and showing through as much as its Opacity says. Off, they're as the app
//! has them.
//!
//! It answers the `slider_look` hook.

use crate::{CONTROLS, FROSTED, SLIDER_LOOK as FEATURE, SLIDER_OPACITY as OPACITY};

/// What the slider look's hook asks: the tracks' opacity, and whether what is behind is frosted.
const ASKED_OPACITY: i32 = 0;
const ASKED_FROSTED: i32 = 1;

/// The tracks' look, `what` of it (0 to 1); below 0 for what it doesn't know, or with its switch
/// off, left as it is.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_slider_look(what: i32) -> f32 {
    if !CONTROLS.on(FEATURE) {
        return -1.0;
    }
    match what {
        ASKED_OPACITY => {
            let v = CONTROLS.get(OPACITY);
            if v.is_nan() { -1.0 } else { v.clamp(0.0, 1.0) }
        }
        ASKED_FROSTED => {
            if CONTROLS.on(FROSTED) {
                1.0
            } else {
                0.0
            }
        }
        _ => -1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn the_tracks_show_through_as_its_controls_say_and_what_it_doesn_t_know_is_left() {
        let _turn = crate::fresh(FEATURE);
        // as it starts: a third opaque, frosted
        assert_eq!(procraft_slider_look(ASKED_OPACITY), 0.35);
        assert_eq!(procraft_slider_look(ASKED_FROSTED), 1.0);
        for what in [2, -1, 99] {
            assert!(procraft_slider_look(what) < 0.0, "{what}");
        }
        procraft_control(OPACITY as i32, 2.0);
        assert_eq!(procraft_slider_look(ASKED_OPACITY), 1.0, "kept to its range");
        procraft_control(OPACITY as i32, 0.1);
        procraft_control(FROSTED as i32, 0.0);
        assert_eq!((procraft_slider_look(ASKED_OPACITY), procraft_slider_look(ASKED_FROSTED)), (0.1, 0.0));
    }

    #[test]
    fn with_its_switch_off_the_sliders_are_the_app_s_own() {
        let _turn = crate::fresh(FEATURE);
        assert_eq!((procraft_slider_look(ASKED_OPACITY), procraft_slider_look(ASKED_FROSTED)), (0.35, 1.0));
        procraft_control(FEATURE as i32, 0.0);
        for what in [ASKED_OPACITY, ASKED_FROSTED] {
            assert!(procraft_slider_look(what) < 0.0, "{what}");
        }
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!((procraft_slider_look(ASKED_OPACITY), procraft_slider_look(ASKED_FROSTED)), (0.35, 1.0));
    }
}
