//! The side bar's place down its edge of the window. Its centre is kept, from one start to the
//! next, as a share of the window's height (one share for either edge and either way up), and the
//! bar stays clear of the top bar and as far from the window's bottom. Its modify button dragged
//! out from the edge and then up or down moves it, the bar stretching a little toward the finger
//! with its edge side kept against the window's edge; let go, it settles back to its width.
//!
//! Everything here is in the side bar's own points: the window's height and the top bar's bottom
//! as the bar measures them (divided by how big a plugin shows it).

use std::sync::{Arc, Mutex, MutexGuard};

/// The side bar's height with its tracks as measured (points).
const BAR: f32 = 519.0;
/// Each track's length as measured, and the least it shrinks to in a window too short for it.
const TRACK: f32 = 170.0;
const SHORTEST_TRACK: f32 = 44.0;
/// Left under the bar in a window too short for its measured tracks (points).
const SHORT_FOOT: f32 = 17.0;
/// Where its centre sits until it is moved: a share of the window's height.
const FIRST_CENTRE: f32 = 0.5418;
/// The least gap between it and the top bar (points).
const UNDER_TOP_BAR: f32 = 6.0;
/// It moves only with more room than this to move in (points).
const LEAST_ROOM: f32 = 20.0;
/// How far the finger goes from where it pressed the modify button before it moves the bar
/// (points, all told) ...
const MOVE_AFTER: f32 = 15.0;
/// ... unless it rested this long first (ms): then it is the button held, not a move ...
const REST_MS: f64 = 150.0;
/// ... resting while it moves less than this (points, all told, for a touchscreen's jitter).
const REST_REACH: f32 = 1.0;
/// How far the bar stretches toward the finger at most (points): its far side follows the finger
/// out past where the move began, point for point, up to this, while its edge side stays against
/// the window's edge (measured: 5 points with the finger 20 out, the move begun at 15; 8 at most).
const MOST_STRETCH: f32 = 8.0;
/// How stiffly it gives past the ends of its room (points): the further past, the less it gives.
const GIVE: f32 = 10.0;

/// The share of the window's height the side bar's centre sits at until it is first moved.
#[uniffi::export]
pub fn side_bar_first_centre() -> f32 {
    FIRST_CENTRE
}

/// What the side bar's centre is kept under, with the interface's other settings.
#[uniffi::export]
pub fn side_bar_centre_key() -> String {
    "side_bar_centre".to_owned()
}

/// A kept centre as it is used: a share of the window's height, or where nothing readable was kept
/// (not a number, or past either end), where it sits at first.
#[uniffi::export]
pub fn side_bar_kept_centre(kept: f32) -> f32 {
    if kept.is_finite() && (0.0..=1.0).contains(&kept) { kept } else { FIRST_CENTRE }
}

/// Where the side bar goes in a window: its top, each track's length, and the highest and the
/// lowest its top may go (the same where it can't move).
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct SideBarFit {
    pub top: f32,
    pub track: f32,
    pub highest: f32,
    pub lowest: f32,
}

impl SideBarFit {
    /// It has room to be moved.
    pub fn movable(&self) -> bool {
        self.lowest - self.highest > LEAST_ROOM
    }
}

/// The side bar in a window `height` tall under a top bar reaching `top_bar` down, its centre kept
/// at `centre` (a share of the height). With room for its measured tracks it goes there, but no
/// higher than 6 points under the top bar nor lower than the top bar's height from the window's
/// bottom; in a shorter window (split screen) it sits just under the top bar, its tracks giving up
/// what it is short of, so that all of it can be reached, and it doesn't move.
#[uniffi::export]
pub fn side_bar_fit(height: f32, top_bar: f32, centre: f32) -> SideBarFit {
    let height = if height.is_finite() { height.max(0.0) } else { 0.0 };
    let top_bar = if top_bar.is_finite() { top_bar.max(0.0) } else { 0.0 };
    let highest = top_bar + UNDER_TOP_BAR;
    if highest + BAR + SHORT_FOOT > height {
        let rest = BAR - TRACK * 2.0;
        let track = ((height - top_bar - SHORT_FOOT - rest) / 2.0).clamp(SHORTEST_TRACK, TRACK);
        return SideBarFit { top: top_bar, track, highest: top_bar, lowest: top_bar };
    }
    let lowest = (height - BAR - top_bar).max(highest);
    let top = (height * side_bar_kept_centre(centre) - BAR / 2.0).clamp(highest, lowest);
    SideBarFit { top, track: TRACK, highest, lowest }
}

/// The share of a window `height` tall that the side bar's centre is at with its top at `top`.
fn centre_of(height: f32, top: f32) -> f32 {
    if height > 0.0 { side_bar_kept_centre((top + BAR / 2.0) / height) } else { FIRST_CENTRE }
}

/// What a touch on the modify button is doing with the side bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SideBarDragState {
    /// Not yet decided: the button is pressed.
    Waiting,
    /// It went out from the edge first: it moves the bar (the button's press is let go, untapped).
    Moving,
    /// It went another way first, or rested before going, or the bar has no room: it is the
    /// button's press alone.
    Pressing,
}

/// Where the side bar shows as the touch has it: its top, how far its far side is stretched toward
/// the finger (points; its edge side stays against the window's edge, and what is on it moves out
/// with the far side), and the share of the window's height its centre is kept at.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct SideBarShown {
    pub state: SideBarDragState,
    pub top: f32,
    pub stretch: f32,
    pub centre: f32,
}

struct Drag {
    state: SideBarDragState,
    right: bool,
    height: f32,
    fit: SideBarFit,
    centre: f32,
    /// Where the finger pressed, where it last was, and how far it has gone all told.
    down: (f32, f32),
    last: (f32, f32),
    travel: f32,
    /// Since when it has rested, and how far it has gone since.
    rest_since: f64,
    rested: f32,
    /// Where the finger has taken the bar's top, past the ends of its room too.
    reach: f32,
    top: f32,
    /// How far out from the edge the finger was as the move began, and the bar's stretch.
    began: f32,
    stretch: f32,
}

/// A touch on the side bar's modify button: made as it presses the button, told each move, and
/// let go. Dragged out from the bar's edge (right for a bar on the left, left for one on the
/// right) once it has gone 15 points, it moves the bar up or down with the finger, the bar
/// stretching toward the finger as it goes further out (up to 8 points), its edge side staying
/// against the window's edge; past the ends of its room it gives less and less, never under the top
/// bar nor past the window's bottom. Let go, the bar settles back to its width, inside its room,
/// and its centre is kept there.
#[derive(uniffi::Object)]
pub struct SideBarDrag {
    drag: Mutex<Drag>,
}

#[uniffi::export]
impl SideBarDrag {
    /// The modify button pressed at (`x`, `y`) at `ms`, the bar against the window's right edge or
    /// its left, in a window `height` tall under a top bar reaching `top_bar` down, its centre kept
    /// at `centre`.
    #[uniffi::constructor]
    pub fn new(x: f32, y: f32, ms: f64, right: bool, height: f32, top_bar: f32, centre: f32) -> Arc<SideBarDrag> {
        let fit = side_bar_fit(height, top_bar, centre);
        Arc::new(SideBarDrag {
            drag: Mutex::new(Drag {
                state: SideBarDragState::Waiting,
                right,
                height,
                fit,
                centre: side_bar_kept_centre(centre),
                down: (x, y),
                last: (x, y),
                travel: 0.0,
                rest_since: ms,
                rested: 0.0,
                reach: fit.top,
                top: fit.top,
                began: 0.0,
                stretch: 0.0,
            }),
        })
    }

    /// The finger moved to (`x`, `y`) at `ms`.
    pub fn moved(&self, x: f32, y: f32, ms: f64) -> SideBarShown {
        let mut d = self.lock();
        if !(x.is_finite() && y.is_finite()) {
            return d.shown();
        }
        let (dx, dy) = (x - d.last.0, y - d.last.1);
        d.last = (x, y);
        match d.state {
            SideBarDragState::Waiting => d.decide(dx.hypot(dy), ms),
            SideBarDragState::Moving => {
                d.reach += dy;
                d.follow();
            }
            SideBarDragState::Pressing => {}
        }
        d.shown()
    }

    /// The finger lifted (or the touch was taken): moved, the bar settles into its width, inside its
    /// room, and its centre is kept there; else it stays where it was.
    pub fn let_go(&self) -> SideBarShown {
        let mut d = self.lock();
        if d.state == SideBarDragState::Moving {
            d.top = d.top.clamp(d.fit.highest, d.fit.lowest);
            d.stretch = 0.0;
            d.centre = centre_of(d.height, d.top);
        }
        d.shown()
    }

    /// Where the bar shows, as things stand.
    pub fn now(&self) -> SideBarShown {
        self.lock().shown()
    }
}

impl SideBarDrag {
    fn lock(&self) -> MutexGuard<'_, Drag> {
        self.drag.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drag {
    fn shown(&self) -> SideBarShown {
        SideBarShown { state: self.state, top: self.top, stretch: self.stretch, centre: self.centre }
    }

    /// Still waiting, the finger went `step` further at `ms`: rested first, it is the button's
    /// press; gone far enough, it moves the bar if it went out from the edge and the bar has room.
    fn decide(&mut self, step: f32, ms: f64) {
        if ms - self.rest_since >= REST_MS {
            self.state = SideBarDragState::Pressing;
            return;
        }
        self.travel += step;
        self.rested += step;
        if self.rested > REST_REACH {
            self.rest_since = ms;
            self.rested = 0.0;
        }
        if self.travel < MOVE_AFTER {
            return;
        }
        let (across, along) = (self.away(), self.last.1 - self.down.1);
        // out from the edge more than up or down
        self.state = if across > along.abs() && self.fit.movable() {
            SideBarDragState::Moving
        } else {
            SideBarDragState::Pressing
        };
        if self.state == SideBarDragState::Moving {
            self.began = across;
            self.follow();
        }
    }

    /// How far the finger has gone out from the bar's edge since it pressed.
    fn away(&self) -> f32 {
        let dx = self.last.0 - self.down.0;
        if self.right { -dx } else { dx }
    }

    /// The bar follows the finger up or down, giving less past the ends of its room, and stretches
    /// toward it as far as it has gone further out since the move began, up to [`MOST_STRETCH`].
    fn follow(&mut self) {
        self.stretch = (self.away() - self.began).clamp(0.0, MOST_STRETCH);
        let SideBarFit { highest, lowest, .. } = self.fit;
        // the top bar's bottom, and where the bar's bottom meets the window's
        let (top_most, bottom_most) = (highest - UNDER_TOP_BAR, lowest + highest - UNDER_TOP_BAR);
        self.top = if self.reach < highest {
            (highest - give(highest - self.reach)).max(top_most)
        } else if self.reach > lowest {
            (lowest + give(self.reach - lowest)).min(bottom_most)
        } else {
            self.reach
        };
    }
}

/// How far the bar goes when the finger has taken it `past` points past an end of its room: a
/// little at first, then less and less (whole points).
fn give(past: f32) -> f32 {
    (GIVE * (past / GIVE + 1.0).sqrt()).round() - GIVE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tablet on its side and upright, as the side bar measures them, under a 44 point top bar.
    const SIDE_WAYS: f32 = 744.0;
    const UPRIGHT: f32 = 1133.0;
    const TOP_BAR: f32 = 44.0;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.05
    }

    /// A drag on the modify button of a bar on the left (or the right) of a window `height` tall,
    /// pressed at (25, 400) at 0 ms, its centre where it is at first.
    fn drag_in(height: f32, right: bool) -> Arc<SideBarDrag> {
        SideBarDrag::new(25.0, 400.0, 0.0, right, height, TOP_BAR, FIRST_CENTRE)
    }

    /// The finger moved from `from` to `to` in `steps` even steps, 10 ms apart after `start_ms`;
    /// where the bar then shows.
    fn go(drag: &SideBarDrag, from: (f32, f32), to: (f32, f32), steps: u32, start_ms: f64) -> SideBarShown {
        let mut shown = drag.now();
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let (x, y) = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            shown = drag.moved(x, y, start_ms + f64::from(k) * 10.0);
        }
        shown
    }

    #[test]
    fn at_first_it_sits_at_its_first_centre_either_way_up() {
        for height in [SIDE_WAYS, UPRIGHT, 1280.0, 800.0] {
            let fit = side_bar_fit(height, TOP_BAR, side_bar_first_centre());
            assert!(close(fit.top, height * 0.5418 - 259.5), "{height}: {fit:?}");
            assert_eq!(fit.track, TRACK);
        }
        // on its side, 143.6 down; upright, 354.4
        assert!(close(side_bar_fit(SIDE_WAYS, TOP_BAR, FIRST_CENTRE).top, 143.6));
        assert!(close(side_bar_fit(UPRIGHT, TOP_BAR, FIRST_CENTRE).top, 354.4));
    }

    #[test]
    fn it_stays_six_points_under_the_top_bar_and_as_far_from_the_bottom_as_the_top_bar_is_tall() {
        for height in (586..2400).step_by(3) {
            let height = height as f32;
            for top_bar in [TOP_BAR, 22.0, 66.0] {
                if top_bar + 6.0 + BAR + SHORT_FOOT > height {
                    continue;
                }
                let fit = side_bar_fit(height, top_bar, 0.0);
                assert_eq!(fit.top, top_bar + 6.0, "{height} {top_bar}");
                assert_eq!(fit.highest, top_bar + 6.0);
                let low = side_bar_fit(height, top_bar, 1.0);
                assert_eq!(low.top, (height - BAR - top_bar).max(top_bar + 6.0), "{height} {top_bar}");
                assert_eq!(low.lowest, low.top);
                for centre in [0.0, 0.2, 0.5, 0.8, 1.0] {
                    let fit = side_bar_fit(height, top_bar, centre);
                    assert!(fit.top >= fit.highest && fit.top <= fit.lowest, "{height} {top_bar} {centre}");
                    assert!(fit.top + BAR <= height, "{height} {top_bar} {centre}");
                    assert_eq!(fit.track, TRACK);
                }
            }
        }
    }

    #[test]
    fn a_window_too_short_for_it_keeps_it_under_the_top_bar_with_shorter_tracks_and_unmoved() {
        for height in 328..586 {
            let height = height as f32;
            for centre in [0.0, FIRST_CENTRE, 1.0] {
                let fit = side_bar_fit(height, TOP_BAR, centre);
                assert_eq!(fit.top, TOP_BAR, "{height}");
                assert!(fit.track <= TRACK && fit.track >= SHORTEST_TRACK);
                assert!(fit.top + BAR - (TRACK - fit.track) * 2.0 <= height, "{height}");
                assert!(!fit.movable());
            }
        }
        // the tracks give up nothing until it is short of room for them
        assert!((side_bar_fit(579.0, TOP_BAR, FIRST_CENTRE).track - TRACK).abs() < 0.6);
        assert_eq!(side_bar_fit(585.0, TOP_BAR, FIRST_CENTRE).track, TRACK);
    }

    #[test]
    fn it_moves_only_with_more_than_twenty_points_to_move_in() {
        // room = height - 519 - 2 * 44 - 6
        assert!(!side_bar_fit(633.0, TOP_BAR, FIRST_CENTRE).movable());
        assert!(side_bar_fit(634.0, TOP_BAR, FIRST_CENTRE).movable());
        assert!(side_bar_fit(SIDE_WAYS, TOP_BAR, FIRST_CENTRE).movable());
        // between too short for its tracks and room to move, it sits as high as it goes
        let fit = side_bar_fit(600.0, TOP_BAR, FIRST_CENTRE);
        assert_eq!((fit.top, fit.track, fit.movable()), (50.0, TRACK, false));
    }

    #[test]
    fn turned_or_resized_it_keeps_its_share_of_the_height_and_a_smaller_window_only_holds_it_in() {
        // moved down on its side, it keeps the same share upright, and back
        let drag = drag_in(SIDE_WAYS, false);
        go(&drag, (25.0, 400.0), (45.0, 400.0), 4, 0.0);
        go(&drag, (45.0, 400.0), (45.0, 430.0), 3, 40.0);
        let kept = drag.let_go();
        assert!(close(kept.top, 143.6 + 30.0), "{kept:?}");
        let upright = side_bar_fit(UPRIGHT, TOP_BAR, kept.centre);
        assert!(close((upright.top + BAR / 2.0) / UPRIGHT, kept.centre));
        assert!(close(side_bar_fit(SIDE_WAYS, TOP_BAR, kept.centre).top, kept.top));
        // kept low; a shorter window holds it in without changing what is kept
        let low = 0.75;
        let short = side_bar_fit(700.0, TOP_BAR, low);
        assert_eq!(short.top, 700.0 - BAR - TOP_BAR);
        let tall = side_bar_fit(1400.0, TOP_BAR, low);
        assert!(close(tall.top, 1400.0 * low - BAR / 2.0));
        // too short for it, then tall again: back where it was kept
        assert_eq!(side_bar_fit(500.0, TOP_BAR, low).top, TOP_BAR);
        assert!(close(side_bar_fit(1400.0, TOP_BAR, low).top, tall.top));
        // a bigger top bar (a plugin's) pushes it down from the top too
        assert_eq!(side_bar_fit(1400.0, 88.0, 0.0).top, 94.0);
    }

    #[test]
    fn what_is_kept_is_read_back_and_nonsense_is_its_first_centre() {
        assert_eq!(side_bar_centre_key(), "side_bar_centre");
        for kept in [0.0, 0.3, 0.5418, 1.0] {
            assert_eq!(side_bar_kept_centre(kept), kept);
        }
        for nonsense in [f32::NAN, f32::INFINITY, -0.1, 1.5] {
            assert_eq!(side_bar_kept_centre(nonsense), FIRST_CENTRE);
            assert_eq!(side_bar_fit(SIDE_WAYS, TOP_BAR, nonsense), side_bar_fit(SIDE_WAYS, TOP_BAR, FIRST_CENTRE));
        }
        // nothing to measure in: nothing crashes, nothing moves
        for height in [0.0, -5.0, f32::NAN, f32::INFINITY] {
            let fit = side_bar_fit(height, TOP_BAR, FIRST_CENTRE);
            assert!(fit.top.is_finite() && fit.track.is_finite() && !fit.movable(), "{height}");
        }
        assert_eq!(centre_of(0.0, 100.0), FIRST_CENTRE);
    }

    #[test]
    fn dragged_out_from_its_edge_and_down_it_follows_the_finger_and_is_kept_where_let_go() {
        let drag = drag_in(SIDE_WAYS, false);
        let start = drag.now();
        assert_eq!(start.state, SideBarDragState::Waiting);
        // under 15 points it waits, the bar where it was
        let s = go(&drag, (25.0, 400.0), (39.0, 400.0), 7, 0.0);
        assert_eq!((s.state, s.top, s.stretch), (SideBarDragState::Waiting, start.top, 0.0));
        // 15 out: it moves, not yet stretched (it doesn't jump out to the finger)
        let s = drag.moved(40.0, 400.0, 80.0);
        assert_eq!((s.state, s.top, s.stretch), (SideBarDragState::Moving, start.top, 0.0));
        // then down with the finger, point for point; stretched toward it no more than 8
        let s = go(&drag, (40.0, 400.0), (100.0, 425.0), 5, 80.0);
        assert!(close(s.top, start.top + 25.0) && s.stretch == MOST_STRETCH, "{s:?}");
        // let go: its own width again where it is, its centre kept there
        let kept = drag.let_go();
        assert_eq!((kept.state, kept.stretch), (SideBarDragState::Moving, 0.0));
        assert!(close(kept.top, start.top + 25.0));
        assert!(close(kept.centre, (kept.top + BAR / 2.0) / SIDE_WAYS));
        assert!(close(side_bar_fit(SIDE_WAYS, TOP_BAR, kept.centre).top, kept.top));
    }

    #[test]
    fn on_the_right_it_is_dragged_out_to_the_left() {
        let drag = SideBarDrag::new(1100.0, 400.0, 0.0, true, SIDE_WAYS, TOP_BAR, FIRST_CENTRE);
        let s = go(&drag, (1100.0, 400.0), (1080.0, 400.0), 4, 0.0);
        assert_eq!((s.state, s.stretch), (SideBarDragState::Moving, 5.0));
        let s = go(&drag, (1080.0, 400.0), (1080.0, 360.0), 4, 40.0);
        assert!(close(s.top, 143.6 - 40.0));
        assert!(close(drag.let_go().top, 103.6));
        // dragged right, toward its edge, it is the button's press
        let drag = SideBarDrag::new(1100.0, 400.0, 0.0, true, SIDE_WAYS, TOP_BAR, FIRST_CENTRE);
        assert_eq!(go(&drag, (1100.0, 400.0), (1120.0, 400.0), 4, 0.0).state, SideBarDragState::Pressing);
    }

    /// As measured: the finger 20 points out in four even steps (the move begun at
    /// 15), then down. The bar stretches 5 points toward the finger and stays so as it goes
    /// down, never past 8 however far out the finger goes; it isn't moved out from its edge.
    #[test]
    fn dragged_twenty_out_it_stretches_five_toward_the_finger_and_never_past_eight() {
        for right in [false, true] {
            let (x, outward) = if right { (1100.0, -1.0) } else { (25.0, 1.0) };
            let drag = SideBarDrag::new(x, 400.0, 0.0, right, SIDE_WAYS, TOP_BAR, FIRST_CENTRE);
            let mut stretched = Vec::new();
            for k in 1..=4 {
                let s = drag.moved(x + outward * 5.0 * k as f32, 400.0, 20.0 * f64::from(k));
                stretched.push(s.stretch);
            }
            assert_eq!(stretched, [0.0, 0.0, 0.0, 5.0], "right {right}");
            let s = go(&drag, (x + outward * 20.0, 400.0), (x + outward * 20.0, 430.0), 30, 80.0);
            assert_eq!((s.state, s.stretch), (SideBarDragState::Moving, 5.0), "right {right}");
            assert!(close(s.top, 143.6 + 30.0), "right {right}: {s:?}");
            // however far out, 8 at most; back toward the edge, less again
            for (out, most) in [(23.0, 8.0), (60.0, 8.0), (400.0, 8.0), (17.0, 2.0), (10.0, 0.0)] {
                let s = drag.moved(x + outward * out, 430.0, 500.0);
                eprintln!("right {right}: the finger {out} out, the bar stretched {}", s.stretch);
                assert!(s.stretch <= 8.0 && s.stretch == most, "right {right}, {out} out: {s:?}");
            }
            assert_eq!(drag.let_go().stretch, 0.0);
        }
    }

    #[test]
    fn going_up_or_down_or_to_the_edge_first_or_resting_first_is_the_buttons_press() {
        for (to, why) in [((25.0, 380.0), "up"), ((25.0, 420.0), "down"), ((5.0, 400.0), "to the edge")] {
            let drag = drag_in(SIDE_WAYS, false);
            let s = go(&drag, (25.0, 400.0), to, 4, 0.0);
            assert_eq!(s.state, SideBarDragState::Pressing, "{why}");
            // and stays so, however it goes on
            let s = go(&drag, to, (200.0, 600.0), 10, 40.0);
            assert_eq!((s.state, s.stretch), (SideBarDragState::Pressing, 0.0), "{why}");
            assert!(close(s.top, 143.6), "{why}");
            assert_eq!(drag.let_go().centre, FIRST_CENTRE);
        }
        // as far out as down: not out more than down
        let drag = drag_in(SIDE_WAYS, false);
        assert_eq!(go(&drag, (25.0, 400.0), (36.0, 411.0), 2, 0.0).state, SideBarDragState::Pressing);
        // a little to the side of out is still out
        let drag = drag_in(SIDE_WAYS, false);
        assert_eq!(go(&drag, (25.0, 400.0), (40.0, 408.0), 3, 0.0).state, SideBarDragState::Moving);
        // held still (a finger's jitter) for 0.15 s first: the button held
        let drag = drag_in(SIDE_WAYS, false);
        drag.moved(25.3, 400.2, 60.0);
        drag.moved(25.1, 400.5, 120.0);
        let s = drag.moved(45.0, 400.0, 150.0);
        assert_eq!(s.state, SideBarDragState::Pressing);
        // still for a little under that, then out: it moves
        let drag = drag_in(SIDE_WAYS, false);
        drag.moved(25.3, 400.2, 70.0);
        let s = go(&drag, (25.3, 400.2), (45.0, 400.0), 2, 130.0);
        assert_eq!(s.state, SideBarDragState::Moving);
    }

    #[test]
    fn with_no_room_to_move_a_drag_out_is_the_buttons_press() {
        for height in [500.0, 600.0, 633.0] {
            let drag = drag_in(height, false);
            let s = go(&drag, (25.0, 400.0), (60.0, 400.0), 4, 0.0);
            assert_eq!(s.state, SideBarDragState::Pressing, "{height}");
        }
    }

    #[test]
    fn past_the_ends_of_its_room_it_gives_less_and_less_and_never_past_the_bars() {
        let fit = side_bar_fit(SIDE_WAYS, TOP_BAR, FIRST_CENTRE);
        assert_eq!((fit.highest, fit.lowest), (50.0, SIDE_WAYS - BAR - TOP_BAR));
        // how far it goes past an end, by how far the finger takes it past
        for (past, gives) in [(0.0, 0.0), (2.0, 1.0), (10.0, 4.0), (30.0, 10.0), (100.0, 23.0), (200.0, 36.0)] {
            assert_eq!(give(past), gives, "{past}");
        }
        // taken up: under the top bar it gives the 6 points between them at most
        let drag = drag_in(SIDE_WAYS, false);
        go(&drag, (25.0, 400.0), (45.0, 400.0), 2, 0.0);
        let s = go(&drag, (45.0, 400.0), (45.0, 400.0 - (fit.top - 50.0) - 10.0), 5, 20.0);
        assert!(close(s.top, 46.0), "{s:?}");
        let s = go(&drag, (45.0, 290.0), (45.0, 0.0), 10, 80.0);
        assert!(close(s.top, TOP_BAR), "{s:?}");
        // let go: inside its room again
        assert!(close(drag.let_go().top, 50.0));
        // taken down: its bottom no lower than the window's
        let drag = drag_in(SIDE_WAYS, false);
        go(&drag, (25.0, 400.0), (45.0, 400.0), 2, 0.0);
        let to_lowest = fit.lowest - fit.top;
        let s = go(&drag, (45.0, 400.0), (45.0, 400.0 + to_lowest + 30.0), 5, 20.0);
        assert!(close(s.top, fit.lowest + 10.0), "{s:?}");
        let s = go(&drag, (45.0, 400.0 + to_lowest + 30.0), (45.0, 4000.0), 10, 80.0);
        assert!(close(s.top + BAR, SIDE_WAYS), "{s:?}");
        // coming back, it comes all the way the finger went past first
        let s = drag.moved(45.0, 3900.0, 200.0);
        assert!(close(s.top + BAR, SIDE_WAYS));
        let kept = drag.let_go();
        assert!(close(kept.top, fit.lowest) && close(kept.centre, (fit.lowest + BAR / 2.0) / SIDE_WAYS));
    }

    #[test]
    fn nonsense_from_the_pointer_moves_nothing() {
        let drag = drag_in(SIDE_WAYS, false);
        go(&drag, (25.0, 400.0), (45.0, 400.0), 2, 0.0);
        let before = drag.now();
        assert_eq!(drag.moved(f32::NAN, 500.0, 30.0), before);
        assert_eq!(drag.moved(45.0, f32::INFINITY, 40.0), before);
        let after = drag.moved(45.0, 410.0, 50.0);
        assert!(close(after.top, before.top + 10.0));
    }
}
