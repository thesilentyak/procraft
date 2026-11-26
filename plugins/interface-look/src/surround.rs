//! Canvas surround: the area round the canvas in a colour of its own, set by its Hue, Saturation
//! and Brightness (black at no brightness), or a picture someone picks, in place of the interface's
//! grey. Off, the surround is the app's.
//!
//! It answers the `interface_colour` hook for the surround (role 1), leaving every other role (the
//! accent) as it is, and the `surround_picture` hook with its Picture control while its switch is
//! on: the app reads the picture someone picked and shows it covering the window, the colour where
//! it is see-through.

use crate::{
    CANVAS_SURROUND as FEATURE, CONTROLS, OWN_PICTURE as OWN, PICTURE, SURROUND_BRIGHTNESS as BRIGHTNESS,
    SURROUND_HUE as HUE, SURROUND_SATURATION as SATURATION, rgb,
};

/// The colour hook's role that is the area round the canvas.
const SURROUND: i32 = 1;

/// The colour (0xRRGGBB) the interface draws `role` in: the surround in the sliders' colour; -1 for
/// any other role, or with its switch off, left as it is.
pub fn colour(role: i32) -> i32 {
    if role != SURROUND || !CONTROLS.on(FEATURE) {
        return -1;
    }
    rgb(CONTROLS.get(HUE), CONTROLS.get(SATURATION), CONTROLS.get(BRIGHTNESS))
}

/// The place of the control whose picture shows round the canvas: Picture's while the switch is on
/// and one is picked, else none (-1).
#[unsafe(no_mangle)]
pub extern "C" fn procraft_surround_picture() -> i32 {
    if CONTROLS.on(FEATURE) && CONTROLS.on(OWN) && CONTROLS.on(PICTURE) { PICTURE as i32 } else { -1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn the_surround_goes_in_the_sliders_colour_or_the_picture_and_every_other_role_as_it_is() {
        let _turn = crate::fresh(FEATURE);
        // as it starts: a dark warm grey, no picture
        assert_eq!(colour(SURROUND), rgb(30.0, 0.1, 0.3));
        assert_eq!(rgb(30.0, 0.1, 0.3), 0x4D4945);
        assert_eq!(procraft_surround_picture(), -1);
        for role in [0, 2, -1] {
            assert_eq!(colour(role), -1, "{role}");
        }
        procraft_control(HUE as i32, 240.0);
        procraft_control(SATURATION as i32, 1.0);
        procraft_control(BRIGHTNESS as i32, 1.0);
        assert_eq!(colour(SURROUND), 0x0000FF);
        procraft_control(BRIGHTNESS as i32, 0.0);
        assert_eq!(colour(SURROUND), 0x000000, "black");
        assert_eq!(colour(0), -1, "the accent stays the app's");
        // a picture: only with its switch on and one picked
        procraft_control(OWN as i32, 1.0);
        assert_eq!(procraft_surround_picture(), -1, "none picked");
        procraft_control(PICTURE as i32, 1.0);
        assert_eq!(procraft_surround_picture(), PICTURE as i32);
        procraft_control(OWN as i32, 0.0);
        assert_eq!(procraft_surround_picture(), -1);
    }

    #[test]
    fn a_hue_saturation_and_brightness_make_their_colour() {
        for (hue, s, v, want) in [
            (0.0, 1.0, 1.0, 0xFF0000),
            (120.0, 1.0, 1.0, 0x00FF00),
            (360.0, 1.0, 1.0, 0xFF0000),
            (0.0, 0.0, 1.0, 0xFFFFFF),
            (200.0, 0.0, 0.5, 0x808080),
        ] {
            assert_eq!(rgb(hue, s, v), want, "{hue} {s} {v}");
        }
        assert_eq!(rgb(f32::NAN, f32::NAN, f32::NAN), 0x000000);
    }

    #[test]
    fn with_its_switch_off_the_surround_is_the_app_s_own() {
        let _turn = crate::fresh(FEATURE);
        procraft_control(OWN as i32, 1.0);
        procraft_control(PICTURE as i32, 1.0);
        assert_eq!((colour(SURROUND), procraft_surround_picture()), (0x4D4945, PICTURE as i32));
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!((colour(SURROUND), procraft_surround_picture()), (-1, -1));
        assert_eq!(crate::procraft_interface_colour(SURROUND), -1);
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!((colour(SURROUND), procraft_surround_picture()), (0x4D4945, PICTURE as i32));
    }
}
