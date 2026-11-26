//! The reference window's picture (the canvas, or a picture imported): fitted whole into the
//! window (all of it: the window's bars lie over the picture), then moved there by a finger and
//! zoomed by two, a view of its own apart from the canvas's; and which of its pixels is under a
//! point in the window, for its eyedropper.

/// How a picture sits in the reference window: its zoom past fitting whole (1 fits it), and how
/// far its middle is from the window's (window pixels).
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ReferenceView {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl Default for ReferenceView {
    fn default() -> ReferenceView {
        ReferenceView { zoom: 1.0, x: 0.0, y: 0.0 }
    }
}

/// The window's picture area and the picture's size, pixels.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ReferenceSizes {
    pub window_width: f32,
    pub window_height: f32,
    pub picture_width: f32,
    pub picture_height: f32,
}

/// The most a reference picture is zoomed past fitting whole.
const MOST_ZOOM: f32 = 20.0;

impl ReferenceSizes {
    /// The picture's pixel size in the window at `zoom`: fitted whole, then zoomed.
    fn scale(&self, zoom: f32) -> f32 {
        let fit = (self.window_width / self.picture_width).min(self.window_height / self.picture_height);
        if fit.is_finite() && fit > 0.0 { fit * zoom } else { 0.0 }
    }
}

/// The view after two fingers moved from (`from_x`, `from_y`) to (`to_x`, `to_y`) (their middle,
/// window pixels) and spread `spread` times as far apart: what was under them stays under them,
/// zoomed between fitting whole and [`MOST_ZOOM`], the window's middle kept on the picture.
#[uniffi::export]
pub fn reference_view_pinched(
    view: ReferenceView,
    sizes: ReferenceSizes,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    spread: f32,
) -> ReferenceView {
    let spread = if spread.is_finite() && spread > 0.0 { spread } else { 1.0 };
    let zoom = (view.zoom * spread).clamp(1.0, MOST_ZOOM);
    let (before, after) = (sizes.scale(view.zoom), sizes.scale(zoom));
    if before <= 0.0 {
        return ReferenceView::default();
    }
    let (cx, cy) = (sizes.window_width / 2.0, sizes.window_height / 2.0);
    // the picture's middle, in the window, moved so the point under the fingers follows them
    let k = after / before;
    let mx = to_x - (from_x - (cx + view.x)) * k;
    let my = to_y - (from_y - (cy + view.y)) * k;
    let (half_w, half_h) = (sizes.picture_width * after / 2.0, sizes.picture_height * after / 2.0);
    ReferenceView { zoom, x: (mx - cx).clamp(-half_w, half_w), y: (my - cy).clamp(-half_h, half_h) }
}

/// The side the canvas is read at for a window showing it before the window's size is known.
pub const REFERENCE_CANVAS_FIRST: u32 = 700;

/// The most pixels the canvas is read at on its longer side for a window (as a picture imported
/// into one is kept at most): a read of 2048 square is 16 MB.
const CANVAS_MOST: u32 = 2048;

/// The steps the side the canvas is read at goes up and down in: a pinch reads it again now and
/// then, not at every move of the fingers.
const CANVAS_STEP: u32 = 128;

/// The longer side to read a `canvas_width` x `canvas_height` canvas at for a Reference window
/// whose picture area is `window_width` x `window_height` pixels of the screen, zoomed `zoom` past
/// fitting whole: a pixel of the picture for each pixel of the screen it covers, so it shows as
/// sharp as the screen can show it, up to the canvas's own pixels (read whole, it shows crisp
/// past them as the canvas does), and no more than [`CANVAS_MOST`]; in steps of [`CANVAS_STEP`].
/// [`REFERENCE_CANVAS_FIRST`] (or the canvas's side, if less) while the window's size isn't known.
#[uniffi::export]
pub fn reference_canvas_side(
    window_width: f32,
    window_height: f32,
    zoom: f32,
    canvas_width: u32,
    canvas_height: u32,
) -> u32 {
    let long = canvas_width.max(canvas_height).max(1);
    let most = long.min(CANVAS_MOST);
    let sizes = ReferenceSizes {
        window_width,
        window_height,
        picture_width: canvas_width.max(1) as f32,
        picture_height: canvas_height.max(1) as f32,
    };
    let zoom = if zoom.is_finite() { zoom.clamp(1.0, MOST_ZOOM) } else { 1.0 };
    // screen pixels a canvas pixel covers
    let scale = sizes.scale(zoom);
    if !(scale.is_finite() && scale > 0.0) {
        return REFERENCE_CANVAS_FIRST.min(most);
    }
    let wanted = (long as f32 * scale.min(1.0)).ceil();
    let steps = (wanted / CANVAS_STEP as f32).ceil().max(1.0);
    let side = if steps * (CANVAS_STEP as f32) >= most as f32 { most } else { steps as u32 * CANVAS_STEP };
    side.clamp(1, most)
}

/// Where the picture is drawn in the window: left, top, width and height (window pixels).
#[uniffi::export]
pub fn reference_picture_rect(view: ReferenceView, sizes: ReferenceSizes) -> Vec<f32> {
    let s = sizes.scale(view.zoom);
    let (w, h) = (sizes.picture_width * s, sizes.picture_height * s);
    let (mx, my) = (sizes.window_width / 2.0 + view.x, sizes.window_height / 2.0 + view.y);
    vec![mx - w / 2.0, my - h / 2.0, w, h]
}

/// The picture's point (its pixels, fractional) under window point (`x`, `y`), and how many of
/// its pixels a window pixel spans; none off the picture.
#[uniffi::export]
pub fn reference_picture_point(view: ReferenceView, sizes: ReferenceSizes, x: f32, y: f32) -> Option<Vec<f32>> {
    let s = sizes.scale(view.zoom);
    if s <= 0.0 {
        return None;
    }
    let (mx, my) = (sizes.window_width / 2.0 + view.x, sizes.window_height / 2.0 + view.y);
    let px = (x - mx) / s + sizes.picture_width / 2.0;
    let py = (y - my) / s + sizes.picture_height / 2.0;
    let on = (0.0..sizes.picture_width).contains(&px) && (0.0..sizes.picture_height).contains(&py);
    on.then(|| vec![px, py, 1.0 / s])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1000 x 500 picture in a 400 x 400 window: fitted at 0.4, 400 x 200, 100 down.
    const SIZES: ReferenceSizes =
        ReferenceSizes { window_width: 400.0, window_height: 400.0, picture_width: 1000.0, picture_height: 500.0 };

    #[test]
    fn a_picture_fits_whole_in_the_middle() {
        let rect = reference_picture_rect(ReferenceView::default(), SIZES);
        assert_eq!(rect, vec![0.0, 100.0, 400.0, 200.0]);
        assert_eq!(
            reference_picture_point(ReferenceView::default(), SIZES, 200.0, 200.0),
            Some(vec![500.0, 250.0, 2.5])
        );
        // above the picture, in the window's empty band
        assert_eq!(reference_picture_point(ReferenceView::default(), SIZES, 200.0, 50.0), None);
    }

    #[test]
    fn a_pinch_keeps_what_is_under_the_fingers_under_them() {
        let start = ReferenceView::default();
        let under = reference_picture_point(start, SIZES, 100.0, 150.0).unwrap();
        // spread twice as far, and moved 30 right
        let view = reference_view_pinched(start, SIZES, 100.0, 150.0, 130.0, 150.0, 2.0);
        assert_eq!(view.zoom, 2.0);
        let now = reference_picture_point(view, SIZES, 130.0, 150.0).unwrap();
        assert!((now[0] - under[0]).abs() < 1e-3 && (now[1] - under[1]).abs() < 1e-3, "{now:?} {under:?}");
        assert!((now[2] - 1.25).abs() < 1e-6);
    }

    #[test]
    fn it_zooms_no_less_than_fitting_and_no_more_than_the_most() {
        let out = reference_view_pinched(ReferenceView::default(), SIZES, 200.0, 200.0, 200.0, 200.0, 0.3);
        assert_eq!(out.zoom, 1.0);
        let mut view = ReferenceView::default();
        for _ in 0..10 {
            view = reference_view_pinched(view, SIZES, 200.0, 200.0, 200.0, 200.0, 3.0);
        }
        assert_eq!(view.zoom, MOST_ZOOM);
    }

    #[test]
    fn the_window_s_middle_stays_on_the_picture() {
        // dragged far off to the left and down
        let view = reference_view_pinched(ReferenceView::default(), SIZES, 200.0, 200.0, -5000.0, 9000.0, 1.0);
        assert_eq!((view.x, view.y), (-200.0, 100.0));
        // its right edge, and its top, at the window's middle
        assert_eq!(reference_picture_rect(view, SIZES), vec![-200.0, 200.0, 400.0, 200.0]);
    }

    /// The screen pixels each pixel of a `canvas` read at `side` spreads over in a `window` zoomed
    /// `zoom`: 1 or less shows as sharp as the screen can; more is blown up, soft.
    fn spread(window: (f32, f32), zoom: f32, canvas: (u32, u32), side: u32) -> f32 {
        let sizes = ReferenceSizes {
            window_width: window.0,
            window_height: window.1,
            picture_width: canvas.0 as f32,
            picture_height: canvas.1 as f32,
        };
        sizes.scale(zoom) * canvas.0.max(canvas.1) as f32 / side.min(canvas.0.max(canvas.1)) as f32
    }

    #[test]
    fn the_canvas_is_read_as_sharp_as_the_window_shows_it() {
        // a phone's window, 910 x 720 pixels, over a 1080 x 2520 canvas (as reported), and a
        // tablet's, 1300 x 1100, over a 4000 x 3000 one and a 1500 x 1000 one
        let cases =
            [((910.0, 720.0), (1080, 2520)), ((1300.0, 1100.0), (4000, 3000)), ((1300.0, 1100.0), (1500, 1000))];
        for (window, canvas) in cases {
            for zoom in [1.0, 1.5, 2.5, 4.0, 8.0, 20.0] {
                let side = reference_canvas_side(window.0, window.1, zoom, canvas.0, canvas.1);
                let (before, now) = (spread(window, zoom, canvas, 700), spread(window, zoom, canvas, side));
                println!(
                    "{window:?} over {canvas:?} zoomed {zoom}: read at {side}, a pixel over {now:.2} (700: {before:.2})"
                );
                let long = canvas.0.max(canvas.1);
                // never past the canvas's own pixels or 2048 a side, and within a step of what it shows
                assert!(side <= long.min(CANVAS_MOST), "{side}");
                if side < long.min(CANVAS_MOST) {
                    assert!(now <= 1.0, "{now}");
                    assert!(spread(window, zoom, canvas, side - CANVAS_STEP) > 1.0, "a step less would do: {side}");
                }
            }
        }
        // the reported window zoomed 2.5 times: a pixel over 2.57 of the screen's before, 1 now
        let side = reference_canvas_side(910.0, 720.0, 2.5, 1080, 2520);
        assert_eq!(side, 1920);
        assert!(spread((910.0, 720.0), 2.5, (1080, 2520), 700) > 2.5);
        assert!(spread((910.0, 720.0), 2.5, (1080, 2520), side) <= 1.0);
    }

    #[test]
    fn a_canvas_shown_past_its_own_pixels_is_read_whole_and_never_past_2048() {
        // shown three times its size: read whole (and shown crisp past its pixels)
        assert_eq!(reference_canvas_side(900.0, 700.0, 3.0, 1000, 800), 1000);
        assert_eq!(reference_canvas_side(900.0, 700.0, 20.0, 4096, 4096), 2048);
        // a window not laid out yet: as before
        assert_eq!(reference_canvas_side(0.0, 0.0, 1.0, 2048, 2048), REFERENCE_CANVAS_FIRST);
        assert_eq!(reference_canvas_side(f32::NAN, f32::NAN, 1.0, 2048, 2048), REFERENCE_CANVAS_FIRST);
        // (one side known: by that side)
        assert_eq!(reference_canvas_side(f32::NAN, 500.0, 1.0, 2048, 2048), 512);
        assert_eq!(reference_canvas_side(0.0, 0.0, 1.0, 400, 300), 400);
        assert_eq!(reference_canvas_side(500.0, 500.0, f32::NAN, 0, 0), 1);
    }

    #[test]
    fn a_pinch_reads_the_canvas_again_only_now_and_then() {
        // zoomed from fitting whole to the most, a hundredth at a time: 300 moves of the fingers
        let mut sides = Vec::new();
        let mut zoom = 1.0f32;
        while zoom <= MOST_ZOOM {
            let side = reference_canvas_side(910.0, 720.0, zoom, 1080, 2520);
            if sides.last() != Some(&side) {
                sides.push(side);
            }
            zoom *= 1.01;
        }
        println!("{} reads: {sides:?}", sides.len());
        assert!(sides.len() <= 12, "{sides:?}");
    }
}
