//! The colour panel's hue controls, which are not linear in hue: a position along one (0..1)
//! maps to a hue by straight lines between a few knots, the same for the classic slider, the
//! value rows' hue and the disc's ring. Yellow, cyan and magenta sit a little off a third of the
//! way round, so the warm hues get more room than the greens.

/// A hue control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HueControl {
    /// The classic slider (and the value rows' hue): drawn through 25 points.
    Slider,
    /// The disc's ring, anticlockwise from the right: drawn through 49 points, every 7.5 degrees.
    Ring,
}

/// Where along a hue control (0..1) each knot is, and its hue (0..1): straight lines between
/// them, so a turn of the ring always turns the hue (no stretch of it holds one hue).
const KNOTS: [(f32, f32); 5] = [(0.0, 0.0), (0.33, 0.166), (0.58, 0.5), (0.91, 0.833), (1.0, 1.0)];

/// The classic slider's hues (degrees) at its 25 evenly spaced points, for drawing it.
pub const SLIDER_HUES: [f32; 25] = [
    0.0, 7.55, 15.09, 22.64, 30.18, 37.73, 45.27, 52.82, 61.36, 81.4, 101.44, 121.48, 141.52, 161.56, 181.21, 196.35,
    211.48, 226.62, 241.76, 256.89, 272.03, 287.17, 304.33, 332.17, 360.0,
];

/// The disc's ring's hues (degrees) at its 49 evenly spaced points, anticlockwise from the right,
/// for drawing it.
pub const RING_HUES: [f32; 49] = [
    0.0, 3.77, 7.55, 11.32, 15.09, 18.86, 22.64, 26.41, 30.18, 33.95, 37.73, 41.5, 45.27, 49.05, 52.82, 56.59, 61.36,
    71.38, 81.4, 91.42, 101.44, 111.46, 121.48, 131.5, 141.52, 151.54, 161.56, 171.58, 181.21, 188.78, 196.35, 203.92,
    211.48, 219.05, 226.62, 234.19, 241.76, 249.32, 256.89, 264.46, 272.03, 279.6, 287.17, 294.73, 304.33, 318.25,
    332.17, 346.08, 360.0,
];

fn stops(control: HueControl) -> &'static [f32] {
    match control {
        HueControl::Slider => &SLIDER_HUES,
        HueControl::Ring => &RING_HUES,
    }
}

/// The hues (degrees) at `control`'s evenly spaced points, for drawing it.
#[uniffi::export]
pub fn hue_stops(control: HueControl) -> Vec<f32> {
    stops(control).to_vec()
}

/// Along the knots from `from` (each knot's first or second) to `to`: the straight line `x` is on.
fn along(x: f32, from: fn(&(f32, f32)) -> f32, to: fn(&(f32, f32)) -> f32) -> f32 {
    let x = if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) };
    for w in KNOTS.windows(2) {
        if let [a, b] = w
            && x <= from(b)
        {
            let span = from(b) - from(a);
            let t = if span > 0.0 { (x - from(a)) / span } else { 0.0 };
            return to(a) + (to(b) - to(a)) * t;
        }
    }
    1.0
}

/// The hue (0..1) at `position` (0..1) along `control`.
#[uniffi::export]
pub fn hue_at(control: HueControl, position: f32) -> f32 {
    let _ = control;
    along(position, |k| k.0, |k| k.1)
}

/// Where `hue` (0..1) sits (0..1) along `control`.
#[uniffi::export]
pub fn hue_position(control: HueControl, hue: f32) -> f32 {
    let _ = control;
    along(hue, |k| k.1, |k| k.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hue_goes_to_its_place_and_back() {
        for control in [HueControl::Slider, HueControl::Ring] {
            for k in 0..=360 {
                let hue = k as f32 / 360.0;
                let back = hue_at(control, hue_position(control, hue));
                assert!((back - hue).abs() < 1e-5, "{control:?} {hue} came back {back}");
            }
        }
        // the knots: yellow a third of the way along, cyan a little past the middle, magenta
        assert_eq!(hue_at(HueControl::Slider, 0.0), 0.0);
        assert!((hue_at(HueControl::Ring, 0.33) - 0.166).abs() < 1e-6);
        assert!((hue_position(HueControl::Ring, 0.5) - 0.58).abs() < 1e-6);
        assert!((hue_position(HueControl::Slider, 0.833) - 0.91).abs() < 1e-6);
        assert_eq!(hue_at(HueControl::Ring, 1.0), 1.0);
    }

    #[test]
    fn turning_the_ring_always_turns_the_hue_with_no_stretch_held_at_red_or_green() {
        // every 0.1 degree of the ring: the hue moves on each step, never less than 0.45 times or
        // more than twice as fast as the ring
        let steps = 3600;
        let mut least = f32::MAX;
        let mut most = 0.0f32;
        let mut last = hue_at(HueControl::Ring, 0.0);
        for i in 1..=steps {
            let hue = hue_at(HueControl::Ring, i as f32 / steps as f32);
            let step = (hue - last) * steps as f32;
            least = least.min(step);
            most = most.max(step);
            last = hue;
        }
        assert!(least > 0.45 && most < 2.0, "hue per ring turn {least}..{most}");
        // a drag of 22.5 degrees from red, and across green, moves the hue at least 11 degrees
        for (from, to) in [(0.0, 22.5), (135.0, 157.5), (337.5, 360.0)] {
            let moved = (hue_at(HueControl::Ring, to / 360.0) - hue_at(HueControl::Ring, from / 360.0)) * 360.0;
            assert!(moved > 11.0, "{from}..{to} moved the hue {moved} degrees");
        }
    }

    #[test]
    fn the_drawn_stops_are_the_knots_lines() {
        for control in [HueControl::Slider, HueControl::Ring] {
            let drawn = stops(control);
            let n = (drawn.len() - 1) as f32;
            for (i, d) in drawn.iter().enumerate() {
                let hue = hue_at(control, i as f32 / n) * 360.0;
                assert!((hue - d).abs() < 0.01, "{control:?} stop {i}: drawn {d}, maps to {hue}");
            }
        }
    }
}
