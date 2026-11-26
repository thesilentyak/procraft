//! Touch position offset: moves where a stroke lands by two sliders, for a pen whose ink doesn't
//! sit under its tip. Off, a stroke lands where it is drawn.
//!
//! The app tells it the sliders and asks where each sample lands (the plugin kit passes them).

use crate::{CONTROLS, TOUCH_OFFSET as FEATURE, X, Y};

// where a sample lands: moved by the sliders while its switch is on
kit::position!(|x, y| if CONTROLS.on(FEATURE) { (x + CONTROLS.get(X), y + CONTROLS.get(Y)) } else { (x, y) });

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    /// Where a sample at (`x`, `y`) lands, as the app reads it.
    fn landed(x: f32, y: f32) -> (f32, f32) {
        let packed = procraft_position(x, y) as u64;
        (f32::from_bits(packed as u32), f32::from_bits((packed >> 32) as u32))
    }

    #[test]
    fn a_sample_lands_moved_by_the_sliders_and_where_it_is_with_its_switch_off() {
        let _turn = crate::fresh(FEATURE);
        // as it starts: where it is
        assert_eq!(landed(100.0, 50.0), (100.0, 50.0));
        procraft_control(X as i32, -12.0);
        procraft_control(Y as i32, 30.0);
        assert_eq!(landed(100.0, 50.0), (88.0, 80.0));
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!(landed(100.0, 50.0), (100.0, 50.0));
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(landed(100.0, 50.0), (88.0, 80.0));
    }
}
