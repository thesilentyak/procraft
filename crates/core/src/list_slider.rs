//! A slider in a list that scrolls (the brush studio's settings): whether a touch on it moves the
//! slider or scrolls the list, so a list scrolled with a finger landing on a slider doesn't change
//! it by accident.

/// Landing this close to the knob's middle (points), a finger moves the slider at once and the
/// list doesn't scroll; a pen, which lands where it is aimed, this close.
const KNOB_REACH_FINGER: f32 = 22.0;
const KNOB_REACH_PEN: f32 = 12.0;

/// Elsewhere, the first way the touch goes this far (points) decides it.
const DECIDED_AT: f32 = 10.0;

/// Whose a touch on a slider in a scrolling list is, so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ListSliderTouch {
    /// Not yet decided: nothing moves (the list may begin to scroll meanwhile, on its own).
    Undecided,
    /// The slider's: it follows the touch, and the list doesn't scroll.
    Slider,
    /// The list's: it scrolls, and the slider stays as it was when the touch landed.
    List,
}

/// Whose a touch is that landed `from_knob` points from the slider knob's middle (a pen's if
/// `pen`) and has since gone `along` points along the slider and `across` points across it (the
/// way the list scrolls): near the knob the slider's at once; elsewhere undecided until it has
/// gone [`DECIDED_AT`] one way or the other, the list's if across first, the slider's if along.
#[uniffi::export]
pub fn list_slider_touch(from_knob: f32, pen: bool, along: f32, across: f32) -> ListSliderTouch {
    let reach = if pen { KNOB_REACH_PEN } else { KNOB_REACH_FINGER };
    if from_knob.abs() <= reach {
        return ListSliderTouch::Slider;
    }
    let (along, across) = (along.abs(), across.abs());
    // (a move that is no number is no move)
    let (along, across) = (if along.is_finite() { along } else { 0.0 }, if across.is_finite() { across } else { 0.0 });
    if across > DECIDED_AT && across >= along {
        ListSliderTouch::List
    } else if along > DECIDED_AT {
        ListSliderTouch::Slider
    } else {
        ListSliderTouch::Undecided
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_the_knob_the_slider_takes_the_touch_at_once_elsewhere_the_first_ten_points_decide() {
        use ListSliderTouch::{List, Slider, Undecided};
        // a finger within 22 points of the knob, a pen within 12: the slider's however it moves
        assert_eq!(list_slider_touch(21.0, false, 0.0, 0.0), Slider);
        assert_eq!(list_slider_touch(-22.0, false, 0.0, 30.0), Slider);
        assert_eq!(list_slider_touch(13.0, true, 0.0, 0.0), Undecided);
        assert_eq!(list_slider_touch(11.0, true, 0.0, 30.0), Slider);
        // elsewhere: nothing until ten points one way
        assert_eq!(list_slider_touch(80.0, false, 6.0, 9.5), Undecided);
        assert_eq!(list_slider_touch(80.0, false, -10.5, 4.0), Slider);
        assert_eq!(list_slider_touch(80.0, false, 4.0, -10.5), List);
        // both past ten at once (a quick flick): across wins as far, the list scrolls
        assert_eq!(list_slider_touch(80.0, false, 14.0, 14.0), List);
        assert_eq!(list_slider_touch(80.0, false, 16.0, 14.0), Slider);
        // no numbers, no move
        assert_eq!(list_slider_touch(80.0, false, f32::NAN, f32::INFINITY), Undecided);
    }
}
