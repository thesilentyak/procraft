//! The blend picker's modes: a list turning under a band fixed in its middle, the row in the band
//! the layer's mode. How the list moves is worked out here for every interface: how far it goes
//! once let go (slowing as a scrolled list does, straight onto a row), how far a pull past an end
//! stretches it, and how a tapped mode scrolls to the band.
//!
//! Distances are points, the list at 0 with its first mode in the band and one row `row` further
//! for each mode after; speeds are points a second, positive down the list.

/// A list let go keeps this share of its speed every millisecond.
const KEPT_PER_MS: f64 = 0.998;
/// Pulled past an end, the list follows this share of the pull at first, less the farther it goes.
const STRETCH_SHARE: f32 = 0.55;
/// A list let go glides onto its row only if that slows it no less than this share of the usual
/// and no more than [`QUICKEST_GLIDE`] times; else it springs there.
const SLOWEST_GLIDE: f32 = 0.8;
const QUICKEST_GLIDE: f32 = 2.5;
/// How quickly a list too slow to glide springs onto its row (critically damped, a second).
const LAND_RATE: f32 = 12.0;
/// How quickly a list springs back from past an end (critically damped, a second).
const BOUNCE_RATE: f32 = 14.0;
/// Closer than this to where it stops (points) and slower than [`STILL`] (points a second), the
/// list is there.
const NEAR: f32 = 0.25;
const STILL: f32 = 5.0;
/// A tapped mode scrolls to the band over this long (seconds); meanwhile the list takes no
/// touches and the modes it passes don't show on the layer.
pub const BLEND_TAP_SECONDS: f32 = 0.3;

/// How quickly a list let go slows: its speed falls by this much a second (exponentially).
fn slowing() -> f32 {
    (-1000.0 * KEPT_PER_MS.ln()) as f32
}

/// How a list let go moves on, worked out once by [`blend_coast`] and followed by
/// [`blend_coast_at`].
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum BlendCoast {
    /// From `from` going `speed`, slowing at `rate` a second straight onto the row at `to`.
    Glide { from: f32, speed: f32, to: f32, rate: f32 },
    /// From `from` going `speed`, springing onto `to` (critically damped at `rate`).
    Spring { from: f32, speed: f32, to: f32, rate: f32 },
    /// From `from` going `speed`, slowing as usual into the end at `edge`, reached after `seconds`
    /// going `edge_speed`; past it, it springs back to it.
    Bounce { from: f32, speed: f32, edge: f32, seconds: f32, edge_speed: f32 },
}

/// Where a moving list is: its `position` and `speed`, and whether it is `done` (at rest where it
/// stops, `position` exactly that).
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct BlendStep {
    pub position: f32,
    pub speed: f32,
    pub done: bool,
}

/// The farthest the list scrolls: the last of `rows` rows, `row` apart, in the band.
fn last(row: f32, rows: u32) -> f32 {
    row * rows.saturating_sub(1) as f32
}

/// `v`, or 0 if it isn't a number.
fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

/// A row's height as given, or a point if it isn't one.
fn row_or_one(row: f32) -> f32 {
    if row.is_finite() && row > 0.0 { row } else { 1.0 }
}

/// How the list of `rows` rows, `row` apart, moves on, let go at `from` going `speed`: slowing as
/// a scrolled list does, where it would come to rest moved to the nearest row, so it slows
/// straight onto that row in one motion (a release too slow to glide there springs there); let go
/// past an end, it springs back to it; flicked into an end, it overshoots and springs back.
#[uniffi::export]
pub fn blend_coast(from: f32, speed: f32, row: f32, rows: u32) -> BlendCoast {
    let (from, speed, row) = (finite(from), finite(speed), row_or_one(row));
    let last = last(row, rows);
    if from < 0.0 || from > last {
        return BlendCoast::Spring { from, speed, to: from.clamp(0.0, last), rate: BOUNCE_RATE };
    }
    let k = slowing();
    let stop = from + speed / k;
    if stop < 0.0 || stop > last {
        let edge = stop.clamp(0.0, last);
        // the share of its speed left as it reaches the end (above 0: it would go past it)
        let left = (1.0 - k * (edge - from) / speed).clamp(f32::MIN_POSITIVE, 1.0);
        return BlendCoast::Bounce { from, speed, edge, seconds: -left.ln() / k, edge_speed: speed * left };
    }
    let to = ((stop / row).round() * row).clamp(0.0, last);
    let rate = speed / (to - from);
    if (to - from) * speed > 0.0 && (k * SLOWEST_GLIDE..=k * QUICKEST_GLIDE).contains(&rate) {
        BlendCoast::Glide { from, speed, to, rate }
    } else {
        BlendCoast::Spring { from, speed, to, rate: LAND_RATE }
    }
}

/// Where the list moving as `coast` says is `seconds` after it was let go.
#[uniffi::export]
pub fn blend_coast_at(coast: BlendCoast, seconds: f32) -> BlendStep {
    let t = finite(seconds).max(0.0);
    match coast {
        BlendCoast::Glide { from, speed, to, rate } => {
            let fade = (-rate * t).exp();
            let left = (to - from) * fade;
            if left.abs() < NEAR {
                BlendStep { position: to, speed: 0.0, done: true }
            } else {
                BlendStep { position: to - left, speed: speed * fade, done: false }
            }
        }
        BlendCoast::Spring { from, speed, to, rate } => spring(to, from - to, speed, rate, t),
        BlendCoast::Bounce { from, speed, edge, seconds: reach, edge_speed } => {
            if t < reach {
                let k = slowing();
                let fade = (-k * t).exp();
                BlendStep { position: from + speed / k * (1.0 - fade), speed: speed * fade, done: false }
            } else {
                spring(edge, 0.0, edge_speed, BOUNCE_RATE, t - reach)
            }
        }
    }
}

/// A critically damped spring onto `to`, `t` seconds after it was `off` from it going `speed`.
fn spring(to: f32, off: f32, speed: f32, rate: f32, t: f32) -> BlendStep {
    let b = speed + rate * off;
    let fade = (-rate * t).exp();
    let x = (off + b * t) * fade;
    let v = (b - rate * (off + b * t)) * fade;
    if x.abs() < NEAR && v.abs() < STILL {
        BlendStep { position: to, speed: 0.0, done: true }
    } else {
        BlendStep { position: to + x, speed: v, done: false }
    }
}

/// Pulled `past` points beyond an end, how far beyond it the list goes: about half the pull at
/// first, less and less the farther it goes, with no hard stop (`height` the list's height).
#[uniffi::export]
pub fn blend_stretch(past: f32, height: f32) -> f32 {
    let d = finite(height).max(1.0);
    let x = finite(past);
    (1.0 - 1.0 / (x.abs() * STRETCH_SHARE / d + 1.0)) * d * x.signum()
}

/// How far beyond an end a pull is that takes the list `shown` points beyond it: what
/// [`blend_stretch`] undoes.
#[uniffi::export]
pub fn blend_unstretch(shown: f32, height: f32) -> f32 {
    let d = finite(height).max(1.0);
    let f = finite(shown).abs().min(d * 0.999);
    d / STRETCH_SHARE * (f / (d - f)) * shown.signum()
}

/// Where the list of `rows` rows, `row` apart, `height` tall, shows when a finger has taken it to
/// `pull`: there between the ends, stretched past them.
#[uniffi::export]
pub fn blend_pulled(pull: f32, row: f32, rows: u32, height: f32) -> f32 {
    let pull = finite(pull);
    let edge = pull.clamp(0.0, last(row_or_one(row), rows));
    edge + blend_stretch(pull - edge, height)
}

/// Where a finger has to have taken the list for it to show at `position`: what [`blend_pulled`]
/// undoes.
#[uniffi::export]
pub fn blend_pull_for(position: f32, row: f32, rows: u32, height: f32) -> f32 {
    let position = finite(position);
    let edge = position.clamp(0.0, last(row_or_one(row), rows));
    edge + blend_unstretch(position - edge, height)
}

/// The row in the band with the list at `position`: the nearest, changing half way between two;
/// the first or the last while it is stretched past an end.
#[uniffi::export]
pub fn blend_centred(position: f32, row: f32, rows: u32) -> u32 {
    let i = (finite(position) / row_or_one(row) + 0.5).floor();
    i.clamp(0.0, rows.saturating_sub(1) as f32) as u32
}

/// A tap on row `tapped` with the list at `position`: where the list goes, the row's mode becoming
/// the layer's at once; none for the row in the band (a tap there changes nothing) or no row.
#[uniffi::export]
pub fn blend_tap(tapped: u32, position: f32, row: f32, rows: u32) -> Option<f32> {
    (tapped < rows && tapped != blend_centred(position, row, rows)).then(|| tapped as f32 * row_or_one(row))
}

/// Where a list sent from `from` to `to` by a tap is `seconds` later: easing in and out over
/// [`BLEND_TAP_SECONDS`] however far it goes.
#[uniffi::export]
pub fn blend_tap_at(from: f32, to: f32, seconds: f32) -> BlendStep {
    let u = finite(seconds) / BLEND_TAP_SECONDS;
    if u >= 1.0 {
        return BlendStep { position: to, speed: 0.0, done: true };
    }
    let u = u.max(0.0);
    BlendStep {
        position: from + (to - from) * u * u * (3.0 - 2.0 * u),
        speed: (to - from) * 6.0 * u * (1.0 - u) / BLEND_TAP_SECONDS,
        done: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The desktop app's rows and list.
    const ROW: f32 = 38.8;
    const HEIGHT: f32 = 349.0;
    const ROWS: u32 = 27;

    /// A coast followed at 120 frames a second: each frame's step, the last done.
    fn follow(coast: BlendCoast) -> Vec<(f32, BlendStep)> {
        let mut steps = Vec::new();
        for frame in 0..2400 {
            let t = frame as f32 / 120.0;
            let step = blend_coast_at(coast, t);
            steps.push((t, step));
            if step.done {
                return steps;
            }
        }
        panic!("{coast:?} still moving after 20 s");
    }

    fn on_a_row(position: f32) -> bool {
        (position - (position / ROW).round() * ROW).abs() < 1e-3
    }

    #[test]
    fn a_flick_at_1000_points_a_second_goes_about_500_points_over_about_2_seconds() {
        let coast = blend_coast(0.0, 1000.0, ROW, ROWS);
        assert!(matches!(coast, BlendCoast::Glide { .. }), "{coast:?}");
        let steps = follow(coast);
        let (_, end) = steps[steps.len() - 1];
        // 13 rows: 504.4, the nearest row to 499.5
        assert!((end.position - 500.0).abs() <= 50.0, "went {}", end.position);
        assert!(on_a_row(end.position));
        // all but still (under 10 points a second) after about 2 s
        let (slow, _) = steps.iter().find(|(_, s)| s.speed.abs() < 10.0).copied().unwrap();
        assert!((1.8..=2.8).contains(&slow), "under 10 points a second after {slow} s");
        eprintln!("1000 pt/s: {} pt, under 10 pt/s after {slow:.2} s", end.position);
    }

    #[test]
    fn let_go_anywhere_it_comes_to_rest_exactly_on_a_row() {
        for from in [0.0, 3.3, 77.6, 120.0, 400.7, 1000.0] {
            for speed in [-3000.0, -1000.0, -250.0, -40.0, -5.0, 0.0, 5.0, 40.0, 250.0, 1000.0, 3000.0] {
                let coast = blend_coast(from, speed, ROW, ROWS);
                let steps = follow(coast);
                let (_, end) = steps[steps.len() - 1];
                assert!(on_a_row(end.position), "from {from} at {speed}: {coast:?} ends at {}", end.position);
                assert!((0.0..=last(ROW, ROWS)).contains(&end.position));
            }
        }
    }

    /// Flicked, it slows straight onto its row: its speed only ever falls and never turns, no
    /// second move to centre it.
    #[test]
    fn a_flick_slows_straight_onto_its_row_in_one_motion() {
        for from in [0.0, 13.0, 200.0, 333.3] {
            for speed in [200.0, 600.0, 1000.0, 1500.0, -200.0, -600.0] {
                if !(0.0..=last(ROW, ROWS)).contains(&(from + speed / slowing())) {
                    continue;
                }
                let coast = blend_coast(from, speed, ROW, ROWS);
                assert!(matches!(coast, BlendCoast::Glide { .. }), "from {from} at {speed}: {coast:?}");
                let steps = follow(coast);
                for pair in steps.windows(2) {
                    let ((_, a), (_, b)) = (pair[0], pair[1]);
                    assert!(b.speed.abs() <= a.speed.abs() + 1e-3, "from {from} at {speed}: sped up");
                    assert!((b.position - a.position) * speed >= 0.0, "from {from} at {speed}: turned");
                }
            }
        }
    }

    #[test]
    fn let_go_still_between_rows_it_settles_on_the_nearest() {
        for (from, to) in [(2.3 * ROW, 2.0 * ROW), (2.6 * ROW, 3.0 * ROW), (0.4 * ROW, 0.0)] {
            let steps = follow(blend_coast(from, 0.0, ROW, ROWS));
            assert_eq!(steps[steps.len() - 1].1.position, to);
            // from rest it doesn't overshoot
            assert!(steps.iter().all(|(_, s)| (s.position - to) * (from - to) >= 0.0));
        }
    }

    #[test]
    fn pulled_past_an_end_it_stretches_about_half_as_far_with_no_hard_stop() {
        let at = |pull: f32| blend_stretch(pull, HEIGHT);
        // 100 points of pull about 48 points, 200 about 84
        assert!((at(100.0) - 48.0).abs() < 2.0, "{}", at(100.0));
        assert!((at(200.0) - 84.0).abs() < 2.0, "{}", at(200.0));
        assert!((at(-100.0) + 48.0).abs() < 2.0);
        // ever farther, ever less for each point more
        let mut last_gain = f32::MAX;
        for pull in (1..40).map(|i| i as f32 * 25.0) {
            let gain = at(pull + 25.0) - at(pull);
            assert!(gain > 0.0 && gain < last_gain, "at {pull}");
            last_gain = gain;
        }
        for pull in [0.0, 10.0, 100.0, 300.0, -50.0] {
            assert!((blend_unstretch(at(pull), HEIGHT) - pull).abs() < 0.01 * pull.abs().max(1.0));
        }
        eprintln!("stretch: 100 pt -> {:.1} pt, 200 pt -> {:.1} pt", at(100.0), at(200.0));
    }

    #[test]
    fn a_pull_shows_between_the_ends_as_it_is_and_past_them_stretched() {
        let most = last(ROW, ROWS);
        assert_eq!(blend_pulled(200.0, ROW, ROWS, HEIGHT), 200.0);
        assert!((blend_pulled(-100.0, ROW, ROWS, HEIGHT) + blend_stretch(100.0, HEIGHT)).abs() < 1e-3);
        assert!((blend_pulled(most + 200.0, ROW, ROWS, HEIGHT) - most - blend_stretch(200.0, HEIGHT)).abs() < 1e-3);
        for pull in [-150.0, -3.0, 0.0, 500.0, most, most + 40.0] {
            let shown = blend_pulled(pull, ROW, ROWS, HEIGHT);
            assert!((blend_pull_for(shown, ROW, ROWS, HEIGHT) - pull).abs() < 0.05, "{pull}");
        }
    }

    #[test]
    fn let_go_past_an_end_it_springs_back_onto_it() {
        let most = last(ROW, ROWS);
        for (from, edge) in [(-48.0, 0.0), (most + 30.0, most)] {
            let coast = blend_coast(from, 0.0, ROW, ROWS);
            let steps = follow(coast);
            assert_eq!(steps[steps.len() - 1].1.position, edge);
            assert!(steps.iter().all(|(_, s)| (s.position - edge) * (from - edge) >= 0.0), "overshot");
        }
    }

    #[test]
    fn flicked_into_an_end_it_overshoots_and_springs_back() {
        let most = last(ROW, ROWS);
        for (from, speed, edge) in [(2.0 * ROW, -2000.0, 0.0), (most - 2.0 * ROW, 2000.0, most)] {
            let coast = blend_coast(from, speed, ROW, ROWS);
            assert!(matches!(coast, BlendCoast::Bounce { .. }), "{coast:?}");
            let steps = follow(coast);
            let beyond = steps.iter().map(|(_, s)| (s.position - edge) * speed.signum()).fold(0.0, f32::max);
            assert!(beyond > 5.0, "overshot {beyond}");
            assert_eq!(steps[steps.len() - 1].1.position, edge);
        }
    }

    #[test]
    fn the_row_in_the_band_changes_half_way_between_rows() {
        assert_eq!(blend_centred(2.49 * ROW, ROW, ROWS), 2);
        assert_eq!(blend_centred(2.5 * ROW, ROW, ROWS), 3);
        assert_eq!(blend_centred(-60.0, ROW, ROWS), 0);
        assert_eq!(blend_centred(30.0 * ROW, ROW, ROWS), ROWS - 1);
        assert_eq!(blend_centred(f32::NAN, ROW, ROWS), 0);
    }

    #[test]
    fn a_tap_scrolls_its_row_to_the_band_in_the_same_time_however_far() {
        // the row in the band: nothing
        assert_eq!(blend_tap(2, 2.0 * ROW, ROW, ROWS), None);
        assert_eq!(blend_tap(ROWS, 2.0 * ROW, ROW, ROWS), None);
        for rows_away in [1, 2, 4] {
            let to = blend_tap(2 + rows_away, 2.0 * ROW, ROW, ROWS).unwrap();
            assert_eq!(to, (2 + rows_away) as f32 * ROW);
            let step = blend_tap_at(2.0 * ROW, to, BLEND_TAP_SECONDS - 0.01);
            assert!(!step.done && step.position < to);
            let step = blend_tap_at(2.0 * ROW, to, BLEND_TAP_SECONDS);
            assert!(step.done && step.position == to);
        }
        assert_eq!(blend_tap_at(0.0, 100.0, 0.15).position, 50.0);
    }
}
