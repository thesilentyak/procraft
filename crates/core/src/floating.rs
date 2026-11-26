//! Panels torn off into windows of their own, while a plugin on the `floating_panels` hook lets
//! them: the Layers panel and the brush library, each dragged out by its grabber, moved by it,
//! sized from its corner, closed by its Close, and kept where it was left from one start to the
//! next. The panel inside is the same as the one that hangs from its button; what is here is
//! where its window goes, how big it may be, and how that is kept.

/// A panel a plugin may let float in a window of its own, by its number on the
/// `floating_panels` hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum FloatingPanel {
    /// The Layers panel.
    Layers = 0,
    /// The brush library.
    Brushes = 1,
}

impl FloatingPanel {
    /// What its window is kept under, with the interface's other settings.
    fn key(self) -> &'static str {
        match self {
            FloatingPanel::Layers => "floating_layers",
            FloatingPanel::Brushes => "floating_brushes",
        }
    }
}

/// A floating panel's window: whether it shows, its top-left in the window it floats in (points as
/// the window measures them), and its size in the panel's own points (it shows at the floating
/// windows' size, a plugin's, times this).
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct FloatingWindow {
    pub open: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// How big a panel's window may be, in its own points: as wide as it hangs from its button at
/// first, and never narrower or wider than these, nor shorter.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct FloatingSizes {
    pub width: f32,
    pub least_width: f32,
    pub most_width: f32,
    pub height: f32,
    pub least_height: f32,
}

/// Where a window may go: the part of the window it floats in that it stays inside (points as the
/// window measures them), clear of the side bar.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct FloatingRoom {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// How big `panel`'s window may be. The Layers panel is 384 wide where it hangs, the library 440;
/// either may grow by half again, or shrink to where its rows still read.
#[uniffi::export]
pub fn floating_sizes(panel: FloatingPanel) -> FloatingSizes {
    match panel {
        FloatingPanel::Layers => {
            FloatingSizes { width: 384.0, least_width: 320.0, most_width: 576.0, height: 520.0, least_height: 240.0 }
        }
        FloatingPanel::Brushes => {
            FloatingSizes { width: 440.0, least_width: 380.0, most_width: 660.0, height: 600.0, least_height: 280.0 }
        }
    }
}

/// What `panel`'s window is kept under, with the interface's other settings.
#[uniffi::export]
pub fn floating_key(panel: FloatingPanel) -> String {
    panel.key().to_owned()
}

/// `window` as kept: whether it shows, then where and how big, a comma between each.
#[uniffi::export]
pub fn floating_kept(window: FloatingWindow) -> String {
    let FloatingWindow { open, x, y, width, height } = window;
    format!("{},{x},{y},{width},{height}", u8::from(open))
}

/// A window kept as [`floating_kept`] writes it, if it reads as one: five numbers, the first 0 or 1
/// and the rest finite, the size more than nothing. Anything else is no window (a panel then floats
/// afresh, as it first does).
#[uniffi::export]
pub fn floating_read(kept: String) -> Option<FloatingWindow> {
    let mut numbers = kept.trim().split(',').map(|n| n.trim().parse::<f32>().ok().filter(|n| n.is_finite()));
    let mut next = || numbers.next().flatten();
    let (open, x, y, width, height) = (next()?, next()?, next()?, next()?, next()?);
    if numbers.next().is_some() || !(open == 0.0 || open == 1.0) || width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(FloatingWindow { open: open == 1.0, x, y, width, height })
}

/// `panel`'s window as it first floats, its grabber's middle (its top edge's middle) at (`x`, `y`),
/// at its size as it hangs and as tall as it first is.
#[uniffi::export]
pub fn floating_at(panel: FloatingPanel, x: f32, y: f32, scale: f32) -> FloatingWindow {
    let sizes = floating_sizes(panel);
    let k = shown_scale(scale);
    FloatingWindow { open: true, x: x - sizes.width * k / 2.0, y, width: sizes.width, height: sizes.height }
}

/// `window` as it may be, for `panel` shown `scale` times as big as its own points, in `room`: its
/// size within the panel's (and no taller than the room), and all of it inside the room (or as
/// much as fits, from its top-left). What isn't a number goes back to the panel's own.
#[uniffi::export]
pub fn floating_fitted(panel: FloatingPanel, window: FloatingWindow, room: FloatingRoom, scale: f32) -> FloatingWindow {
    let sizes = floating_sizes(panel);
    let k = shown_scale(scale);
    let or = |v: f32, otherwise: f32| if v.is_finite() { v } else { otherwise };
    let (left, top) = (or(room.left, 0.0), or(room.top, 0.0));
    let (right, bottom) = (or(room.right, left).max(left), or(room.bottom, top).max(top));
    let most_height = ((bottom - top) / k).max(sizes.least_height);
    let width = or(window.width, sizes.width).clamp(sizes.least_width, sizes.most_width);
    let height = or(window.height, sizes.height).clamp(sizes.least_height, most_height);
    let x = or(window.x, left).min(right - width * k).max(left);
    let y = or(window.y, top).min(bottom - height * k).max(top);
    FloatingWindow { open: window.open, x, y, width, height }
}

/// `window` moved by (`dx`, `dy`) points of the window it floats in, then fitted.
#[uniffi::export]
pub fn floating_moved(
    panel: FloatingPanel,
    window: FloatingWindow,
    dx: f32,
    dy: f32,
    room: FloatingRoom,
    scale: f32,
) -> FloatingWindow {
    let moved = FloatingWindow { x: window.x + dx, y: window.y + dy, ..window };
    floating_fitted(panel, moved, room, scale)
}

/// `window` sized from its lower right corner, the corner dragged by (`dx`, `dy`) points of the
/// window it floats in, then fitted: its top-left stays where it is.
#[uniffi::export]
pub fn floating_resized(
    panel: FloatingPanel,
    window: FloatingWindow,
    dx: f32,
    dy: f32,
    room: FloatingRoom,
    scale: f32,
) -> FloatingWindow {
    let k = shown_scale(scale);
    let sized = FloatingWindow { width: window.width + dx / k, height: window.height + dy / k, ..window };
    // the corner past the room's edge goes no further, rather than pushing the window back
    let sizes = floating_sizes(panel);
    let width = sized.width.min(((room.right - window.x) / k).max(sizes.least_width));
    let height = sized.height.min(((room.bottom - window.y) / k).max(sizes.least_height));
    floating_fitted(panel, FloatingWindow { width, height, ..sized }, room, scale)
}

/// A scale a window may show at: the floating windows' size, a plugin's (as the part scale hook
/// keeps it), or 1.
fn shown_scale(scale: f32) -> f32 {
    if scale.is_finite() { scale.clamp(0.25, 4.0) } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: FloatingRoom = FloatingRoom { left: 0.0, top: 0.0, right: 1600.0, bottom: 1000.0 };

    fn window(x: f32, y: f32, width: f32, height: f32) -> FloatingWindow {
        FloatingWindow { open: true, x, y, width, height }
    }

    #[test]
    fn a_window_is_kept_as_five_numbers_and_read_back_leniently() {
        let w = FloatingWindow { open: false, x: 120.5, y: -3.0, width: 400.0, height: 512.25 };
        assert_eq!(floating_kept(w), "0,120.5,-3,400,512.25");
        assert_eq!(floating_read(floating_kept(w)), Some(w));
        let open = window(10.0, 20.0, 384.0, 520.0);
        assert_eq!(floating_read(floating_kept(open)), Some(open));
        assert_eq!(floating_read(" 1, 10 ,20,384,520 ".to_owned()), Some(open));
        // what isn't one is none
        for bad in [
            "",
            "1,2,3,4",
            "1,2,3,4,5,6",
            "2,0,0,100,100",
            "1,0,0,0,100",
            "1,0,0,100,-5",
            "1,nan,0,100,100",
            "1,x,0,100,100",
            "1,0,0,inf,100",
        ] {
            assert_eq!(floating_read(bad.to_owned()), None, "{bad:?}");
        }
        // each panel under a key of its own
        assert_ne!(floating_key(FloatingPanel::Layers), floating_key(FloatingPanel::Brushes));
    }

    #[test]
    fn a_window_stays_inside_its_room_at_a_size_its_panel_allows() {
        for panel in [FloatingPanel::Layers, FloatingPanel::Brushes] {
            let sizes = floating_sizes(panel);
            assert!(
                sizes.least_width <= sizes.width
                    && sizes.width <= sizes.most_width
                    && sizes.least_height <= sizes.height
            );
            // as it is, where it fits
            let w = window(100.0, 80.0, sizes.width, sizes.height);
            assert_eq!(floating_fitted(panel, w, ROOM, 1.0), w);
            // too small or too big: as small or as big as it may be
            let tiny = floating_fitted(panel, window(0.0, 0.0, 10.0, 10.0), ROOM, 1.0);
            assert_eq!((tiny.width, tiny.height), (sizes.least_width, sizes.least_height));
            let huge = floating_fitted(panel, window(0.0, 0.0, 5000.0, 5000.0), ROOM, 1.0);
            assert_eq!((huge.width, huge.height), (sizes.most_width, 1000.0), "no taller than the room");
            // off an edge: back inside, at its shown size
            let off = floating_fitted(panel, window(1500.0, 900.0, sizes.width, 400.0), ROOM, 1.25);
            assert_eq!((off.x, off.y), (1600.0 - sizes.width * 1.25, 1000.0 - 400.0 * 1.25));
            let before = floating_fitted(panel, window(-50.0, -50.0, sizes.width, 400.0), ROOM, 1.0);
            assert_eq!((before.x, before.y), (0.0, 0.0));
            // a room smaller than the window: from its top-left
            let small = FloatingRoom { left: 60.0, top: 0.0, right: 300.0, bottom: 200.0 };
            let squeezed = floating_fitted(panel, window(500.0, 500.0, sizes.width, sizes.height), small, 1.0);
            assert_eq!((squeezed.x, squeezed.y, squeezed.height), (60.0, 0.0, sizes.least_height));
            // what isn't a number is the panel's own, and it stays shown or not as it was
            let nan = FloatingWindow { open: false, x: f32::NAN, y: f32::INFINITY, width: f32::NAN, height: f32::NAN };
            let fitted = floating_fitted(panel, nan, ROOM, f32::NAN);
            assert_eq!(
                fitted,
                FloatingWindow { open: false, x: 0.0, y: 0.0, width: sizes.width, height: sizes.height }
            );
        }
    }

    #[test]
    fn a_window_floats_from_its_grabber_and_is_moved_and_sized_inside_its_room() {
        let panel = FloatingPanel::Layers;
        // its top edge's middle under the finger
        let first = floating_at(panel, 800.0, 100.0, 1.0);
        assert_eq!(first, window(800.0 - 192.0, 100.0, 384.0, 520.0));
        assert_eq!(floating_at(panel, 800.0, 100.0, 0.5).x, 800.0 - 96.0);
        // moved, but not off the room
        let moved = floating_moved(panel, first, 50.0, -20.0, ROOM, 1.0);
        assert_eq!((moved.x, moved.y), (first.x + 50.0, 80.0));
        let pushed = floating_moved(panel, first, 5000.0, 5000.0, ROOM, 1.0);
        assert_eq!((pushed.x, pushed.y), (1600.0 - 384.0, 1000.0 - 520.0));
        // sized from its corner, its top-left staying, the drag taken at the size it shows
        let sized = floating_resized(panel, first, 50.0, 25.0, ROOM, 0.5);
        assert_eq!(sized, window(first.x, first.y, 484.0, 570.0));
        let least = floating_resized(panel, first, -1000.0, -1000.0, ROOM, 1.0);
        assert_eq!((least.x, least.y, least.width, least.height), (first.x, first.y, 320.0, 240.0));
        // dragged past the room's edge, it stops there rather than moving
        let near = window(1300.0, 600.0, 320.0, 300.0);
        let stopped = floating_resized(panel, near, 400.0, 400.0, ROOM, 1.0);
        assert_eq!(stopped, window(1280.0, 600.0, 320.0, 400.0));
    }
}
