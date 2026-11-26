//! The sample stroke a brush is shown with in the library and in the files it is shared in: a
//! gentle wave across a canvas of [`CANVAS`] pixels, drawn fast, its pressure swelling to full in
//! the middle, at a size the brush's preview size sets. Its path, pressure and the way its pen is
//! turned are the ones brush files' own previews are drawn with, sample for sample; how far that
//! pen leans goes untold (the stroke is drawn as by a pen that doesn't say, see
//! `StrokeBuilder::with_input`).

use std::f32::consts::FRAC_PI_2;

use crate::brush::Brush;
use crate::geom::Vec2;
use crate::input::Sample;

/// The canvas the sample stroke is drawn on (pixels); a preview of another size fits it in.
pub const CANVAS: (f32, f32) = (1060.0, 324.0);

/// How many pen samples the stroke has.
const SAMPLES: usize = 36;

/// The time between samples (milliseconds): a pen reporting 240 times a second.
const STEP_MS: f64 = 1000.0 / 240.0;

/// The wave's middle line: where it starts (canvas pixels), how long it runs and how far it
/// slopes down from level (degrees).
const LINE_START: Vec2 = Vec2::new(122.06892, 133.71185);
const LINE_LENGTH: f32 = 810.0;
const LINE_SLOPE: f32 = 6.000014;

/// The wave across that line: how far it swings either way (pixels), the phase it starts at
/// and how far it turns along the line (radians).
const SWING: f32 = 60.00541;
const SWING_PHASE: f32 = std::f32::consts::FRAC_PI_3;
const SWING_TURN: f32 = 3.7591707;

/// The way the pen leans on every sample after the first (radians; the first tells none).
const LEAN_HEADING: f32 = 0.053382;

/// The pressure the pen's swell is turned into: the brush's preview minimum plus this many times
/// the swell (times its preview pressure scale), a little past double it.
const PRESSURE_GAIN: f32 = 2.04;

/// Where along the wave `t` (0..1) is, in canvas pixels.
fn along(t: f32) -> Vec2 {
    let (sin, cos) = LINE_SLOPE.to_radians().sin_cos();
    let forward = LINE_LENGTH * t;
    let across = SWING * (SWING_PHASE + SWING_TURN * t).sin();
    LINE_START + Vec2::new(cos * forward - sin * across, sin * forward + cos * across)
}

/// The pen's pressure at each sample: light at the ends, swelling to full in the middle.
const SWELL: [f32; SAMPLES] = [
    0.1059249, 0.1359825, 0.172023, 0.2144414, 0.2634203, 0.3188661, 0.3803522, 0.4470768, 0.5178415, 0.591058,
    0.664786, 0.7368043, 0.8047129, 0.8660606, 0.9184895, 0.9598836, 0.9885111, 1.0031434, 1.0031434, 0.9885111,
    0.9598836, 0.9184895, 0.8660606, 0.8047129, 0.7368043, 0.664786, 0.591058, 0.5178415, 0.4470768, 0.3803522,
    0.3188661, 0.2634203, 0.2144414, 0.172023, 0.1359825, 0.1059249,
];

/// The pen's swell at `t` (0..1 along): its pressure at the samples, and between two of them the
/// way from one to the other.
fn swell(t: f32) -> f32 {
    let at = t.clamp(0.0, 1.0) * (SAMPLES - 1) as f32;
    let i = (at as usize).min(SAMPLES - 2);
    let (a, b) = (SWELL.get(i).copied().unwrap_or(0.0), SWELL.get(i + 1).copied().unwrap_or(0.0));
    a + (b - a) * (at - i as f32)
}

/// The pressure the sample stroke presses `brush` with at `t` (0..1 along): the brush's preview
/// minimum plus its swell, the control's full travel a little past doubling it.
pub fn pressure_at(brush: &Brush, t: f32) -> f32 {
    let least = brush.setting("preview_pressure_min");
    let gain = PRESSURE_GAIN * brush.setting("preview_pressure_scale");
    (least + gain * swell(t)).clamp(0.0, 1.0)
}

/// The sample stroke for `brush`, in canvas pixels.
pub fn samples(brush: &Brush) -> Vec<Sample> {
    // the pen held a quarter turn round from upright, then turned by the preview's own offset (a
    // quarter turn at 1), and by the way it leans once it does
    let turn = (brush.setting("preview_tilt_offset") - 1.0) * FRAC_PI_2;
    (0..SAMPLES)
        .map(|i| {
            let t = i as f32 / (SAMPLES - 1) as f32;
            let azimuth = turn + if i == 0 { 0.0 } else { LEAN_HEADING };
            Sample { azimuth, ..Sample::at(along(t), pressure_at(brush, t), i as f64 * STEP_MS) }
        })
        .collect()
}

/// The brush's size along the sample stroke, from its least (0) to its most (1): the preview size,
/// less for brushes so big they would not fit, more for small ones (to be seen) and for those
/// that thin out with speed (the stroke is fast).
pub fn size_share(brush: &Brush) -> f32 {
    let units = size_units(brush);
    let small = if units < 4.0 { (0.1 - units.min(0.08)) * 9.5 } else { 0.0 };
    share(brush, units, small)
}

/// The size the studio's drawing pad draws at, from the brush's least (0) to its most (1): its
/// preview size, as its sliders start, so that a bigger most size draws bigger there too.
pub fn pad_size_share(brush: &Brush) -> f32 {
    let share = brush.setting("preview_size");
    if share.is_finite() { share.clamp(0.0, 1.0) } else { 0.0 }
}

/// The brush's most size in the units brush files keep it in.
fn size_units(brush: &Brush) -> f32 {
    brush.biggest() / brush.size_unit()
}

fn share(brush: &Brush, units: f32, small: f32) -> f32 {
    let speed = 0.3 * brush.setting("speed_size").abs();
    let share = speed + small + brush.setting("preview_size") / (units / 2.0).max(1.0);
    if share.is_finite() { share.clamp(0.0, 1.0) } else { 0.0 }
}

/// The diameter the sample stroke is drawn at (canvas pixels).
pub fn diameter(brush: &Brush) -> f32 {
    at_share(brush, size_share(brush))
}

/// The diameter a dual brush's `second` is drawn at along `brush`'s sample stroke: as far along
/// its own range, from its least size to its most, as the first's preview size takes the first
/// along its (the stroke's samples carry that share, and each part draws it from its own range).
pub fn second_diameter(brush: &Brush, second: &Brush) -> f32 {
    at_share(second, size_share(brush))
}

/// `brush`'s size `share` of the way from its least to its most (canvas pixels).
fn at_share(brush: &Brush, share: f32) -> f32 {
    let most = brush.biggest();
    let least = brush.min_size.min(most);
    (least + (most - least) * share).max(0.5)
}

/// The canvas (premultiplied RGBA8, [`CANVAS`] in size) fitted into a `width` × `height` preview:
/// as big as fits, in the middle, each pixel the average of the canvas pixels it covers.
pub fn fit(canvas: &[u8], width: u32, height: u32) -> Vec<u8> {
    fit_image(canvas, (CANVAS.0 as u32, CANVAS.1 as u32), width, height)
}

/// An image (premultiplied RGBA8, `size` big) fitted into a `width` × `height` preview, as
/// [`fit`] fits the canvas.
pub fn fit_image(canvas: &[u8], size: (u32, u32), width: u32, height: u32) -> Vec<u8> {
    let (sw, sh) = (size.0 as usize, size.1 as usize);
    let (w, h) = (width as usize, height as usize);
    let k = (width as f32 / size.0.max(1) as f32).min(height as f32 / size.1.max(1) as f32);
    // for each preview row or column, the canvas ones under it and how much of it each covers
    let spans = |n: usize, source: usize, fitted: f32| -> Vec<Vec<(usize, f32)>> {
        let start = (n as f32 - fitted) / 2.0;
        (0..n)
            .map(|o| {
                let (a, b) = ((o as f32 - start) / k, (o as f32 + 1.0 - start) / k);
                let (first, last) = (a.max(0.0).floor() as usize, (b.min(source as f32).ceil() as usize).min(source));
                (first..last)
                    .map(|i| (i, ((i as f32 + 1.0).min(b) - (i as f32).max(a)).max(0.0) * k))
                    .filter(|&(_, weight)| weight > 0.0)
                    .collect()
            })
            .collect()
    };
    let (columns, rows) = (spans(w, sw, sw as f32 * k), spans(h, sh, sh as f32 * k));
    let mut out = vec![0u8; w * h * 4];
    for (y, row) in rows.iter().enumerate() {
        for (x, column) in columns.iter().enumerate() {
            let mut sum = [0.0f32; 4];
            for &(sy, wy) in row {
                for &(sx, wx) in column {
                    let at = (sy * sw + sx) * 4;
                    if let Some(p) = canvas.get(at..at + 4) {
                        for (s, &v) in sum.iter_mut().zip(p) {
                            *s += f32::from(v) * wx * wy;
                        }
                    }
                }
            }
            let at = (y * w + x) * 4;
            if let Some(dst) = out.get_mut(at..at + 4) {
                for (d, s) in dst.iter_mut().zip(sum) {
                    *d = s.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wave_crosses_the_canvas() {
        let s = samples(&Brush::round_pen());
        assert_eq!(s.len(), SAMPLES);
        let (first, last) = (s.first().unwrap(), s.last().unwrap());
        assert!((first.pos.x - 116.6).abs() < 0.5 && (last.pos.x - 933.9).abs() < 0.5);
        assert!(s.iter().all(|p| p.pos.y > 0.4 * CANVAS.1 && p.pos.y < 0.7 * CANVAS.1));
        // light at the ends, swelling to full in the middle
        assert!(first.pressure < 0.15 && last.pressure < 0.15);
        assert!(s.get(SAMPLES / 4).unwrap().pressure < 0.62);
        assert_eq!(s.get(SAMPLES / 2).unwrap().pressure, 1.0);
        assert!((last.time_ms - 35.0 * STEP_MS).abs() < 1e-9);
    }

    #[test]
    fn the_wave_goes_through_the_sample_stroke_s_own_points_and_presses_as_it_does() {
        // samples 0, 9, 17, 26 and 35 of the stroke brush files' previews are drawn with (canvas
        // pixels, and the pen's own pressure before a brush's preview controls scale it)
        let points = [
            (0, 116.637, 185.3934, 0.1059249),
            (9, 323.5469, 209.3986, 0.591058),
            (17, 511.6782, 190.6685, 1.0031434),
            (26, 724.5187, 158.2488, 0.591058),
            (35, 933.8762, 158.9668, 0.1059249),
        ];
        for (i, x, y, press) in points {
            let t = i as f32 / (SAMPLES - 1) as f32;
            let at = along(t);
            assert!(at.distance(Vec2::new(x, y)) < 1e-3, "sample {i} at {at:?}");
            assert!((swell(t) - press).abs() < 1e-6, "sample {i} pressed {}", swell(t));
        }
        // between samples, the way from one to the next
        let half = 8.5 / (SAMPLES - 1) as f32;
        assert!((swell(half) - (0.5178415 + 0.591058) / 2.0).abs() < 1e-5, "{}", swell(half));
        // every step between samples as long as the stroke's own: none bunched or skipped
        let s = samples(&Brush::round_pen());
        let steps: Vec<f32> = s.windows(2).map(|w| w[0].pos.distance(w[1].pos)).collect();
        assert!(steps.iter().all(|&d| (22.0..25.0).contains(&d)), "{steps:?}");
    }

    #[test]
    fn the_preview_s_pressure_is_its_minimum_plus_the_swell_scaled_by_its_control() {
        let mut b = Brush::round_pen();
        b.set_setting("preview_pressure_scale", 0.25);
        b.set_setting("preview_pressure_min", 0.2);
        // half travel leaves the swell a little stronger than it is; a quarter about halves it
        assert!((pressure_at(&b, 0.0) - (0.2 + 0.51 * 0.1059249)).abs() < 1e-5);
        assert!((pressure_at(&b, 0.5) - (0.2 + 0.51 * 1.0031434)).abs() < 1e-5);
        b.set_setting("preview_pressure_scale", 0.5);
        assert!((pressure_at(&b, 0.0) - (0.2 + 1.02 * 0.1059249)).abs() < 1e-5);
        b.set_setting("preview_pressure_scale", 1.0);
        assert_eq!(pressure_at(&b, 0.5), 1.0);
        let s = samples(&b);
        assert!(s.iter().enumerate().all(|(i, p)| p.pressure == pressure_at(&b, i as f32 / 35.0)));
    }

    #[test]
    fn the_preview_s_pen_is_held_a_quarter_turn_round_then_turned_by_its_offset() {
        let mut b = Brush::round_pen();
        // the first sample tells no lean of its own; the rest lean the same small way
        let s = samples(&b);
        assert!((s[0].azimuth + FRAC_PI_2).abs() < 1e-6);
        assert!(s[1..].iter().all(|p| (p.azimuth + FRAC_PI_2 - LEAN_HEADING).abs() < 1e-6));
        b.set_setting("preview_tilt_offset", 1.0);
        let s = samples(&b);
        assert!(s[0].azimuth.abs() < 1e-6);
        assert!(s[1..].iter().all(|p| (p.azimuth - 0.053382).abs() < 1e-6));
        // how far it leans goes untold, whether the brush previews tilt or not
        for tilts in [0.0, 1.0] {
            b.set_setting("preview_tilt", tilts);
            assert!(samples(&b).iter().all(|p| p.tilt == 0.0));
        }
    }

    #[test]
    fn a_brush_turned_as_the_pen_is_held_keeps_the_sample_pen_s_turn_along_its_preview() {
        use crate::input::Pointer;
        use crate::stroke::StrokeBuilder;
        // the stamps' turns along the sample stroke, drawn as the library draws it
        let spread = |azimuth: bool| {
            let mut b = Brush::round_pen();
            b.set_setting("input_style", if azimuth { 1.0 } else { 0.0 });
            b.set_setting("shape_rotation", 1.0);
            let mut stroke = StrokeBuilder::new(b.clone(), 20.0, Pointer::Stylus).steady().on_pad(false);
            let mut stamps = stroke.add(&samples(&b));
            stamps.extend(stroke.finish());
            let angles: Vec<f32> = stamps.iter().map(|s| s.angle).collect();
            assert!(angles.len() > 30, "{}", angles.len());
            angles.iter().fold(f32::MIN, |m, &a| m.max(a)) - angles.iter().fold(f32::MAX, |m, &a| m.min(a))
        };
        // turned as the pen is held: no more than the pen's own small turn after its first sample
        let held = spread(true);
        assert!(held <= LEAN_HEADING + 1e-3, "{held}");
        // following the stroke instead, they swing with the wave
        let following = spread(false);
        assert!(following > 0.5, "{following}");
    }

    #[test]
    fn small_brushes_show_bigger_and_big_ones_smaller() {
        let mut b = Brush::round_pen();
        b.set_setting("preview_size", 0.3);
        let unit = b.size_unit();
        // under 0.08 of a unit, a small brush is shown up to nearly its full size
        b.max_size = 0.04 * unit;
        assert!((size_share(&b) - (0.3 + 0.06 * 9.5)).abs() < 1e-4);
        b.max_size = 1.0 * unit;
        assert!((size_share(&b) - 0.49).abs() < 1e-4);
        // from 2 units the preview size is shared out over the size
        b.max_size = 8.0 * unit;
        assert!((size_share(&b) - 0.075).abs() < 1e-4);
    }

    #[test]
    fn the_drawing_pad_draws_at_its_preview_size_of_the_brush_s_range_however_big_its_most() {
        let mut b = Brush::round_pen();
        b.set_setting("preview_size", 0.3);
        let unit = b.size_unit();
        // the same share of a bigger most size: a bigger stroke on the pad
        for most in [0.5, 4.0, 10.0] {
            b.max_size = most * unit;
            assert!((pad_size_share(&b) - 0.3).abs() < 1e-6, "{most}");
        }
    }

    #[test]
    fn the_canvas_fits_in_the_middle() {
        let (sw, sh) = (CANVAS.0 as usize, CANVAS.1 as usize);
        // opaque white on the left half
        let canvas: Vec<u8> = (0..sw * sh).flat_map(|i| if i % sw < sw / 2 { [255u8; 4] } else { [0; 4] }).collect();
        // half the size: the halves stay halves
        let half = fit(&canvas, 530, 162);
        assert_eq!(half.len(), 530 * 162 * 4);
        assert_eq!(half.get((80 * 530 + 264) * 4..(80 * 530 + 265) * 4), Some(&[255u8; 4][..]));
        assert_eq!(half.get((80 * 530 + 265) * 4..(80 * 530 + 266) * 4), Some(&[0u8; 4][..]));
        // a taller preview: the canvas across its width, empty above and below
        let tall = fit(&canvas, 106, 100);
        let a = |x: usize, y: usize| tall.get((y * 106 + x) * 4 + 3).copied();
        assert_eq!(a(10, 2), Some(0));
        assert_eq!(a(10, 50), Some(255));
        assert_eq!(a(10, 97), Some(0));
    }
}
