//! The side bar's size and opacity sliders: what a tap on one does, how far a drag moves its
//! thumb, and the thumb's shadow. Values run from 0 at the bottom to 1 at the top.

/// Within this far of a slider's middle line (points) a drag moves the thumb with the finger.
const FULL_REACH: f32 = 52.0;

/// The least share of the finger's move a drag passes on, however far away the finger is.
const FINEST: f32 = 0.01;

/// The share of the finger's move along a slider that moves its thumb, with the finger `across`
/// points from the slider's middle line: all of it close by, then less with the square of the
/// distance (a quarter at twice [`FULL_REACH`]), never under [`FINEST`]. Dragging out sideways
/// first gives finer control.
#[uniffi::export]
pub fn side_slider_drag_share(across: f32) -> f32 {
    let d = across.abs();
    if !d.is_finite() {
        return FINEST;
    }
    if d <= FULL_REACH {
        return 1.0;
    }
    (FULL_REACH / d).powi(2).max(FINEST)
}

/// What a tap on a side bar slider does.
///
/// A slider's panel (its value, the "+" that saves it as a notch, the brush's stamp) comes up as
/// the slider is dragged or held, and stays up once the finger lifts, for its "+" to be tapped. It
/// goes with a touch anywhere off the side bar and the panel, or as these say.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum SideSliderTap {
    /// On the thumb with no panel up: the slider's panel comes up.
    Panel,
    /// On the thumb with a panel up: the panel goes.
    ClosePanel,
    /// Elsewhere on the track: the thumb goes to this value, and a panel up goes.
    Jump { value: f32 },
}

/// A tap at `at` (the value under the finger, the thumb's middle there) on a slider at `value`,
/// whose thumb reaches `thumb` (in value units) either side of its middle, with a slider's panel
/// up or not (`panel_up`): on the thumb it brings up the panel, or takes it away; elsewhere the
/// thumb goes to the tap, or onto a saved notch the tap is as close to.
#[uniffi::export]
pub fn side_slider_tap(at: f32, value: f32, thumb: f32, notches: Vec<f32>, panel_up: bool) -> SideSliderTap {
    if (at - value).abs() <= thumb {
        return if panel_up { SideSliderTap::ClosePanel } else { SideSliderTap::Panel };
    }
    let near = notches
        .into_iter()
        .filter(|n| (n - at).abs() <= thumb)
        .min_by(|a, b| (a - at).abs().total_cmp(&(b - at).abs()));
    SideSliderTap::Jump { value: near.unwrap_or(at).clamp(0.0, 1.0) }
}

/// One step of a side slider thumb's shadow: the thumb's pill grown by `grow` points (shrunk where
/// negative, its ends as much rounder or tighter), moved `down` points, filled black at `alpha`
/// over the steps drawn before it.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ShadowStep {
    pub grow: f32,
    pub down: f32,
    pub alpha: f32,
}

/// How far below its thumb the shadow falls (points).
const SHADOW_DROP: f32 = 1.0;
/// How soft its edge is: it fades as a Gaussian this wide (points).
const SHADOW_SOFTNESS: f32 = 1.0;
/// How dark it is at its heart.
const SHADOW_DARKEST: f32 = 0.32;
/// The steps reach this far out past the dropped thumb's edge (points), where it has faded away ...
const SHADOW_REACH: f32 = 3.0;
/// ... and are this far apart: a pixel on a screen of 2 pixels a point.
const SHADOW_STEP: f32 = 0.5;
/// ... down to a point inside the edge, the lowest the thumb's own edge leaves showing.
const SHADOW_STEPS: usize = 9;

/// The standard normal distribution's share below `x`.
fn normal_below(x: f32) -> f32 {
    // the error function, to within about 1e-7 (Abramowitz and Stegun's 7.1.26)
    let z = x / std::f32::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * z.abs());
    let poly = t * (0.254_829_6 + t * (-0.284_496_7 + t * (1.421_413_8 + t * (-1.453_152 + t * 1.061_405_4))));
    let erf = 1.0 - poly * (-z * z).exp();
    0.5 * (1.0 + if z < 0.0 { -erf } else { erf })
}

/// How dark a side slider thumb's shadow is `outside` points out past the edge of the thumb
/// dropped [`SHADOW_DROP`] (inside where negative).
fn shadow_darkness(outside: f32) -> f32 {
    SHADOW_DARKEST * normal_below(-outside / SHADOW_SOFTNESS)
}

/// The steps that draw a side slider thumb's shadow, the widest first: the thumb dropped a point,
/// black at 32% fading over about a point either side of its edge, as the thumb's shadow measures
/// on either look. Drawn under the thumb, only the rim of it below and round the thumb shows.
#[uniffi::export]
pub fn side_thumb_shadow() -> Vec<ShadowStep> {
    let mut steps = Vec::with_capacity(SHADOW_STEPS);
    // how dark the steps so far show, over each other, inside the last one
    let mut shown = 0.0;
    for i in 0..SHADOW_STEPS {
        let grow = SHADOW_REACH - i as f32 * SHADOW_STEP;
        // inside this step's edge and outside the next's, the darkness halfway between them
        let want = shadow_darkness(grow - SHADOW_STEP / 2.0);
        let alpha = (1.0 - (1.0 - want) / (1.0 - shown)).clamp(0.0, 1.0);
        steps.push(ShadowStep { grow, down: SHADOW_DROP, alpha });
        shown = want;
    }
    steps
}

/// The percent a side slider's panel shows for its value (0..1): the size slider's share of the
/// brush's size range, which goes as the slider to the power [`procraft_engine::brush::SIZE_CURVE`]
/// (its middle 22%), the opacity slider's value as it is. (It showed the slider's place, so a size
/// read "35%" painted a tenth of the range, and the low sizes read alike.)
#[uniffi::export]
pub fn side_slider_shown_percent(size: bool, value: f32) -> i32 {
    let v = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
    let shown = if size { v.powf(procraft_engine::brush::SIZE_CURVE) } else { v };
    (shown * 100.0).round() as i32
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_size_panel_shows_the_share_of_the_size_range() {
        // as measured: "Size 3%" with the thumb at 0.21 of the bar, "15%" to "17%" at 0.43 to
        // 0.45; the opacity as it is
        assert_eq!(side_slider_shown_percent(true, 0.21), 3);
        assert_eq!(side_slider_shown_percent(true, 0.43), 16);
        assert_eq!(side_slider_shown_percent(true, 0.45), 17);
        assert_eq!(side_slider_shown_percent(true, 1.0), 100);
        assert_eq!(side_slider_shown_percent(true, 0.0), 0);
        assert_eq!(side_slider_shown_percent(false, 0.21), 21);
        assert_eq!(side_slider_shown_percent(true, f32::NAN), 0);
    }

    use super::*;

    /// How dark `steps` show `below` points under the thumb's lower edge, at its middle.
    fn darkness_below(steps: &[ShadowStep], below: f32) -> f32 {
        let clear: f32 = steps.iter().filter(|s| below < s.down + s.grow).map(|s| 1.0 - s.alpha).product();
        1.0 - clear
    }

    #[test]
    fn a_thumb_s_shadow_falls_below_it_as_measured() {
        let steps = side_thumb_shadow();
        // widest first, every one a point down and see-through
        assert_eq!(steps.len(), SHADOW_STEPS);
        assert!(steps.windows(2).all(|w| w[0].grow > w[1].grow));
        assert!(steps.iter().all(|s| s.down == 1.0 && s.alpha > 0.0 && s.alpha < 0.32));
        // measured under the dark thumb (at 2 pixels a point): how much darker the track shows
        for (below, measured) in [(0.25, 0.243), (0.75, 0.184), (1.25, 0.126), (1.75, 0.078), (2.25, 0.039)] {
            let dark = darkness_below(&steps, below);
            assert!((dark - measured).abs() < 0.015, "{below}: {dark} against {measured}");
        }
        // gone a few points down, never darker than its heart
        assert_eq!(darkness_below(&steps, 4.5), 0.0);
        assert!(darkness_below(&steps, -5.0) <= 0.32);
    }

    #[test]
    fn a_drag_off_to_the_side_moves_the_thumb_less_as_measured() {
        // measured: the share of a 60 point move along, by the finger's distance from the slider
        for (across, measured) in
            [(30.0, 1.0), (45.0, 1.0), (60.0, 0.76), (90.0, 0.36), (120.0, 0.19), (150.0, 0.117), (180.0, 0.083)]
        {
            let share = side_slider_drag_share(across);
            assert!((share - measured).abs() < 0.03, "{across}: {share} against {measured}");
        }
        // either side, and never stuck
        assert_eq!(side_slider_drag_share(-30.0), 1.0);
        assert_eq!(side_slider_drag_share(5000.0), FINEST);
        assert_eq!(side_slider_drag_share(f32::NAN), FINEST);
    }

    #[test]
    fn a_tap_on_the_thumb_opens_the_panel_and_elsewhere_the_thumb_goes_there() {
        assert_eq!(side_slider_tap(0.52, 0.5, 0.06, vec![], false), SideSliderTap::Panel);
        assert_eq!(side_slider_tap(0.2, 0.5, 0.06, vec![], false), SideSliderTap::Jump { value: 0.2 });
        // onto a notch the tap is close to, the nearest of two
        assert_eq!(side_slider_tap(0.21, 0.5, 0.06, vec![0.25, 0.18], false), SideSliderTap::Jump { value: 0.18 });
        assert_eq!(side_slider_tap(0.9, 0.5, 0.06, vec![0.25], false), SideSliderTap::Jump { value: 0.9 });
        // past the ends it holds to them
        assert_eq!(side_slider_tap(1.02, 0.5, 0.01, vec![], false), SideSliderTap::Jump { value: 1.0 });
    }

    #[test]
    fn a_tap_on_the_thumb_with_the_panel_up_takes_it_away_and_on_the_track_still_jumps() {
        assert_eq!(side_slider_tap(0.48, 0.5, 0.06, vec![], true), SideSliderTap::ClosePanel);
        // on a saved size's mark: the thumb goes onto it (and the panel goes)
        assert_eq!(side_slider_tap(0.27, 0.5, 0.06, vec![0.25], true), SideSliderTap::Jump { value: 0.25 });
    }
}
