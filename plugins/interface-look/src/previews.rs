//! Coloured brush previews: the brush library's sample strokes drawn in the colour being painted
//! with, or in a colour of its own set by its Hue, Saturation and Brightness, rather than white;
//! the brush in use, on the accent, too if its switch says. Off, they're as the app draws them.
//!
//! It answers the `brush_preview_colour` hook.

use crate::{
    CHOSEN, COLOURED_PREVIEWS as FEATURE, CONTROLS, OWN_COLOUR as OWN, PREVIEW_BRIGHTNESS as BRIGHTNESS,
    PREVIEW_HUE as HUE, PREVIEW_SATURATION as SATURATION, rgb,
};

/// The colour (0xRRGGBB) a brush's sample stroke is drawn in, the current colour `current`: its
/// own, or the current; -1 (as the app draws it) for the brush in use (`chosen` not 0) unless the
/// switch says, and for every brush with the feature's switch off.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_brush_preview_colour(current: i32, chosen: i32) -> i32 {
    if !CONTROLS.on(FEATURE) || (chosen != 0 && !CONTROLS.on(CHOSEN)) {
        return -1;
    }
    if CONTROLS.on(OWN) {
        rgb(CONTROLS.get(HUE), CONTROLS.get(SATURATION), CONTROLS.get(BRIGHTNESS))
    } else {
        current & 0xFF_FFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn a_hue_saturation_and_brightness_make_their_colour() {
        for (hue, s, v, want) in [
            (0.0, 1.0, 1.0, 0xFF0000),
            (60.0, 1.0, 1.0, 0xFFFF00),
            (120.0, 1.0, 1.0, 0x00FF00),
            (180.0, 1.0, 1.0, 0x00FFFF),
            (240.0, 1.0, 1.0, 0x0000FF),
            (300.0, 1.0, 1.0, 0xFF00FF),
            (360.0, 1.0, 1.0, 0xFF0000),
            (30.0, 1.0, 1.0, 0xFF8000),
            (0.0, 0.0, 1.0, 0xFFFFFF),
            (200.0, 0.0, 0.5, 0x808080),
            (123.0, 0.7, 0.0, 0x000000),
        ] {
            assert_eq!(rgb(hue, s, v), want, "{hue} {s} {v}");
        }
        // what isn't in range, or isn't a number, is taken to the nearest it can be
        assert_eq!(rgb(-30.0, 2.0, 7.0), 0xFF0000);
        assert_eq!(rgb(f32::NAN, f32::NAN, f32::NAN), 0x000000);
    }

    #[test]
    fn previews_go_in_the_current_colour_or_its_own_the_brush_in_use_as_its_switch_says() {
        let _turn = crate::fresh(FEATURE);
        let current = 0x12_3456;
        // as it starts: the current colour, the brush in use as the app draws it
        assert_eq!(procraft_brush_preview_colour(current, 0), current);
        assert_eq!(procraft_brush_preview_colour(current, 1), -1);
        // the current colour's bits alone, whatever comes above them
        assert_eq!(procraft_brush_preview_colour(-1, 0), 0xFF_FFFF);
        // a colour of its own, from its sliders
        procraft_control(OWN as i32, 1.0);
        procraft_control(HUE as i32, 120.0);
        procraft_control(SATURATION as i32, 1.0);
        procraft_control(BRIGHTNESS as i32, 0.5);
        assert_eq!(procraft_brush_preview_colour(current, 0), 0x008000);
        // the brush in use too
        procraft_control(CHOSEN as i32, 1.0);
        assert_eq!(procraft_brush_preview_colour(current, 1), 0x008000);
        procraft_control(OWN as i32, 0.0);
        assert_eq!(procraft_brush_preview_colour(current, 1), current);
        procraft_control(CHOSEN as i32, 0.0);
    }

    #[test]
    fn with_its_switch_off_previews_are_as_the_app_draws_them() {
        let _turn = crate::fresh(FEATURE);
        let current = 0x12_3456;
        procraft_control(CHOSEN as i32, 1.0);
        assert_eq!(
            (procraft_brush_preview_colour(current, 0), procraft_brush_preview_colour(current, 1)),
            (current, current)
        );
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!((procraft_brush_preview_colour(current, 0), procraft_brush_preview_colour(current, 1)), (-1, -1));
        procraft_control(OWN as i32, 1.0);
        assert_eq!(procraft_brush_preview_colour(current, 0), -1);
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(procraft_brush_preview_colour(current, 0), rgb(200.0, 0.6, 1.0));
    }
}
