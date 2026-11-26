//! Value check: Values only, a switch in the Actions panel's Canvas tab (and Alt+G), shows the
//! canvas in greys, each colour as the grey of its luminance, to check its values as a painter
//! squints. Contrast spreads the greys from a middle grey, to tell close values apart. It's the
//! screen alone: what is saved, exported and picked keeps its colours. Off, the canvas shows as it
//! is.
//!
//! The app asks the view look's hook for the matrix the canvas's colours go through on the screen,
//! entry by entry.

use crate::{CONTRAST, CONTROLS, VALUE_CHECK as FEATURE, VALUES};

/// How much of a colour's linear red, green and blue its luminance is (Rec. 709): together all of
/// it, so white stays white.
const WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// The grey contrast spreads from, in linear light: a middle grey (18%).
const MIDDLE: f32 = 0.18;

/// Rows red, green and blue: three weights for a colour's linear red, green and blue, then what is
/// added.
type Look = [[f32; 4]; 3];

/// The look that changes nothing.
const AS_IT_IS: Look = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

/// The look: with `values` each colour as the grey of its luminance, spread from [`MIDDLE`]
/// `contrast` times as far; without, the colours as they are.
fn look(values: bool, contrast: f32) -> Look {
    if !values {
        return AS_IT_IS;
    }
    let k = if contrast.is_finite() { contrast.clamp(1.0, 2.0) } else { 1.0 };
    let [r, g, b] = WEIGHTS.map(|w| w * k);
    [[r, g, b, MIDDLE * (1.0 - k)]; 3]
}

/// Entry `entry` of the look (row by row, four to a row); 0 past them.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_view_look(entry: i32) -> f32 {
    let look = look(CONTROLS.on(FEATURE) && CONTROLS.on(VALUES), CONTROLS.get(CONTRAST));
    let at = usize::try_from(entry).ok();
    at.and_then(|at| look.get(at / 4).and_then(|row| row.get(at % 4))).copied().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    /// A linear colour through `look`.
    fn through(look: &Look, rgb: [f32; 3]) -> [f32; 3] {
        look.map(|row| row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2] + row[3])
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5)
    }

    #[test]
    fn each_colour_shows_as_the_grey_of_its_luminance() {
        let grey = look(true, 1.0);
        // white and black stay, each primary is the grey of its share, and a colour the grey of
        // its weighed sum
        assert!(near(through(&grey, [1.0; 3]), [1.0; 3]));
        assert!(near(through(&grey, [0.0; 3]), [0.0; 3]));
        for (primary, weight) in [([1.0, 0.0, 0.0], 0.2126), ([0.0, 1.0, 0.0], 0.7152), ([0.0, 0.0, 1.0], 0.0722)] {
            assert!(near(through(&grey, primary), [weight; 3]), "{primary:?}");
        }
        let y = 0.2126 * 0.5 + 0.7152 * 0.25 + 0.0722 * 0.75;
        assert!(near(through(&grey, [0.5, 0.25, 0.75]), [y; 3]));
        // off, colours are as they are
        assert_eq!(look(false, 2.0), AS_IT_IS);
    }

    #[test]
    fn contrast_spreads_the_greys_from_a_middle_grey() {
        let strong = look(true, 2.0);
        // the middle grey stays; a lighter grey goes lighter, a darker darker
        assert!(near(through(&strong, [MIDDLE; 3]), [MIDDLE; 3]));
        assert!(near(through(&strong, [0.3; 3]), [0.42; 3]));
        assert!(near(through(&strong, [0.1; 3]), [0.02; 3]));
        // past its range it goes no further, and a contrast that isn't a number is none
        assert_eq!(look(true, 9.0), strong);
        assert_eq!(look(true, f32::NAN), look(true, 1.0));
    }

    #[test]
    fn the_app_is_given_the_look_entry_by_entry() {
        let _turn = crate::fresh(FEATURE);
        procraft_control(VALUES as i32, 1.0);
        procraft_control(CONTRAST as i32, 1.0);
        let entries: Vec<f32> = (0..12).map(|e| procraft_view_look(e)).collect();
        assert_eq!(entries[..4], [0.2126, 0.7152, 0.0722, 0.0]);
        assert_eq!(entries[4..8], entries[..4]);
        assert_eq!((procraft_view_look(-1), procraft_view_look(12)), (0.0, 0.0));
        procraft_control(VALUES as i32, 0.0);
        let entries: Vec<f32> = (0..12).map(|e| procraft_view_look(e)).collect();
        assert_eq!(entries, AS_IT_IS.concat());
    }

    #[test]
    fn with_its_switch_off_the_canvas_shows_as_it_is() {
        let _turn = crate::fresh(FEATURE);
        let entries = || (0..12).map(|e| procraft_view_look(e)).collect::<Vec<f32>>();
        procraft_control(VALUES as i32, 1.0);
        procraft_control(CONTRAST as i32, 2.0);
        assert_eq!(entries(), look(true, 2.0).concat());
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!(entries(), AS_IT_IS.concat());
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(entries(), look(true, 2.0).concat());
    }
}
