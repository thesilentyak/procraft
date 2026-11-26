//! A brush held in the library, as the original does it (measured from screen recordings at about
//! 31 frames a second): held still [`BRUSH_MENU_HOLD`], its menu shows while the finger is still
//! down (let go before, it was a tap, which chooses the brush). The finger moving on within
//! [`BRUSH_DRAG_WINDOW`] of the menu showing lifts the brush to follow it, the menu going; moving on
//! later, the hold is over and the list under the menu has the touch (it scrolls), the menu staying.
//! Let go without moving on, the menu stays and nothing is chosen. The apps follow the touch; what
//! a move does is [`brush_hold_move`]'s to say, and how long the hold still has to go when an app
//! first sees the touch, [`brush_menu_wait`]'s.

/// How long a brush is held still before its menu shows, seconds from the touch coming down: the
/// menu started showing 0.32 to 0.35 s into each hold measured.
pub const BRUSH_MENU_HOLD: f64 = 0.33;

/// How long after its menu shows a held brush still lifts to follow the finger moving on, seconds:
/// presses of 0.4 and 0.55 s then moved on lifted the brush, their moves taking hold 0.51 and
/// 0.67 s after the touch came down (0.18 and 0.34 s after the menu showed); presses of 0.7 s and
/// 1 s scrolled the list under the menu instead. So the window ends 0.65 to 0.75 s after the touch
/// came down: here in the middle of that.
pub const BRUSH_DRAG_WINDOW: f64 = 0.37;

/// How far back the touch's own clock may put its coming down when an app first sees it, seconds,
/// for the hold to count from then: further back (or ahead), that clock isn't one to go by (a
/// touch screen just made gives its first touch a time most of a second early), and the hold
/// counts from when the app saw it. A touch reaches an app some milliseconds after it came down,
/// a busy frame or two more; well inside the hold, so the press is never taken as held at once.
pub const BRUSH_TOUCH_CLOCK_TRUST: f64 = 0.25;

/// What the finger holding a brush does as it moves (see [`brush_hold_move`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum BrushHoldMove {
    /// Not yet past the slop: the hold goes on as it is.
    Stays,
    /// The brush lifts and follows the finger; its menu goes.
    Drags,
    /// The hold is over without lifting the brush: the list under the menu has the touch (it
    /// scrolls up and down), and the menu stays. Nothing else has it: moved sideways, the brush's
    /// row doesn't slide open to its buttons (see [`brush_row_slides`]).
    Leaves,
}

/// [`BRUSH_MENU_HOLD`], for the apps.
#[uniffi::export]
pub fn brush_menu_hold_seconds() -> f64 {
    BRUSH_MENU_HOLD
}

/// How much longer a brush held still waits before its menu shows, seconds, when the app first
/// sees the touch `since_down` seconds after it came down by the touch's own clock: the menu shows
/// [`BRUSH_MENU_HOLD`] after the finger landed, not after the app saw it. A time that isn't a number,
/// one ahead of the app's clock or further back than [`BRUSH_TOUCH_CLOCK_TRUST`] is a clock not to go
/// by: the hold then counts from now.
#[uniffi::export]
pub fn brush_menu_wait(since_down: f64) -> f64 {
    if !(0.0..=BRUSH_TOUCH_CLOCK_TRUST).contains(&since_down) {
        return BRUSH_MENU_HOLD;
    }
    (BRUSH_MENU_HOLD - since_down).max(0.0)
}

/// Whether a press on a brush's row may slide the row sideways to show its buttons: not once the
/// brush's menu has shown under it (`menu_shown`). From then on the press is the hold's until let go:
/// moved on in time it lifts the brush, later the list under the menu has it, which only scrolls up
/// and down, the menu staying (as the original: a sideways move late in a hold opens no buttons).
#[uniffi::export]
pub fn brush_row_slides(menu_shown: bool) -> bool {
    !menu_shown
}

/// What the finger holding a brush does, moved `moved` from where it came down (the app's points
/// or pixels, as `slop` is), `since_menu` seconds after the brush's menu showed: within the slop,
/// nothing; past it within [`BRUSH_DRAG_WINDOW`] the brush lifts to be dragged, if it can be
/// (`movable`: the Recent set's brushes stay where they are); past it later, or for a brush that
/// stays, the hold is over and the list has the touch. A distance or a time that isn't a number is
/// taken as no move and as late.
#[uniffi::export]
pub fn brush_hold_move(moved: f32, slop: f32, since_menu: f64, movable: bool) -> BrushHoldMove {
    if moved.is_nan() || moved <= slop.max(0.0) {
        return BrushHoldMove::Stays;
    }
    // (a clock a moment behind the touch's own counts as the menu just showing)
    if movable && since_menu.is_finite() && since_menu.max(0.0) <= BRUSH_DRAG_WINDOW {
        BrushHoldMove::Drags
    } else {
        BrushHoldMove::Leaves
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The presses measured on the original, and when their moves took hold (from the touch coming
    /// down: where the menu began to fade as the brush lifted), and what each move then did: the
    /// drag window has to keep each where it was seen.
    #[test]
    fn the_measured_presses_move_on_as_the_original_did() {
        // still holds: the menu showed 0.32 to 0.35 s in, and a hold of 0.3 s let go was a tap
        let hold = brush_menu_hold_seconds();
        assert!((0.32..=0.35).contains(&hold) && hold > 0.3, "{hold}");
        // a 0.4 s press (its move taking hold at 0.508 s) and a 0.55 s one (0.668 s) lifted the
        // brush; a 0.7 s press, its move no sooner than 0.75 s, and a 1 s one scrolled the list
        for (press, moved_at, lifts) in [(0.4, 0.508, true), (0.55, 0.668, true), (0.7, 0.75, false), (1.0, 1.0, false)]
        {
            let moved = brush_hold_move(30.0, 8.0, moved_at - BRUSH_MENU_HOLD, true);
            let want = if lifts { BrushHoldMove::Drags } else { BrushHoldMove::Leaves };
            assert_eq!(moved, want, "a press of {press} s, moved on at {moved_at} s");
        }
    }

    #[test]
    fn a_finger_never_quite_still_keeps_the_hold() {
        for moved in [0.0, 1.0, 3.5, 7.9, 8.0] {
            assert_eq!(brush_hold_move(moved, 8.0, 0.0, true), BrushHoldMove::Stays);
            assert_eq!(brush_hold_move(moved, 8.0, 5.0, true), BrushHoldMove::Stays);
        }
        // a move that isn't a number is no move
        assert_eq!(brush_hold_move(f32::NAN, 8.0, 0.1, true), BrushHoldMove::Stays);
    }

    #[test]
    fn moved_on_in_time_the_brush_lifts_and_later_the_list_has_it() {
        assert_eq!(brush_hold_move(8.5, 8.0, 0.0, true), BrushHoldMove::Drags);
        assert_eq!(brush_hold_move(40.0, 8.0, BRUSH_DRAG_WINDOW, true), BrushHoldMove::Drags);
        assert_eq!(brush_hold_move(40.0, 8.0, BRUSH_DRAG_WINDOW + 0.01, true), BrushHoldMove::Leaves);
        assert_eq!(brush_hold_move(40.0, 8.0, 3.0, true), BrushHoldMove::Leaves);
        // a clock a moment behind: the menu just showed
        assert_eq!(brush_hold_move(40.0, 8.0, -0.004, true), BrushHoldMove::Drags);
        // a time that isn't a number is late
        assert_eq!(brush_hold_move(40.0, 8.0, f64::NAN, true), BrushHoldMove::Leaves);
        assert_eq!(brush_hold_move(40.0, 8.0, f64::INFINITY, true), BrushHoldMove::Leaves);
    }

    /// The menu shows the hold after the finger landed: an app seeing the touch late waits only the
    /// rest of it, and one that saw it after the whole hold shows it at once.
    #[test]
    fn the_menu_counts_from_when_the_finger_landed() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // seen at once: the whole 0.33 s; seen 40 ms or 90 ms late (the 0.37 to 0.42 s the menu
        // showed at, timed from when the app saw the touch): 0.29 and 0.24 s more
        assert!(close(brush_menu_wait(0.0), 0.33), "{}", brush_menu_wait(0.0));
        assert!(close(brush_menu_wait(0.040), 0.29), "{}", brush_menu_wait(0.040));
        assert!(close(brush_menu_wait(0.090), 0.24), "{}", brush_menu_wait(0.090));
        // so the menu shows at 0.33 s from the touch however late (up to a quarter second) the app
        // saw it, and always some time after the app saw it (never held at once)
        for late in [0.0, 0.004, 0.016, 0.040, 0.090, 0.2, 0.25] {
            assert!(close(late + brush_menu_wait(late), BRUSH_MENU_HOLD), "seen {late} s late");
            assert!(brush_menu_wait(late) >= 0.08, "seen {late} s late");
        }
        // a clock not to go by, the hold counted from now: a virtual touch screen's first touch
        // (0.9 s early, which made a press moved at once lift and drag its brush), one ahead of
        // the app's clock, one far off, or not a number
        for odd in [0.26, 0.5, 0.9, -0.004, -3.0, 4_000_000.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(brush_menu_wait(odd), BRUSH_MENU_HOLD, "{odd}");
        }
    }

    /// Moved sideways late in a hold (after the drag window), the list has the press, and only to
    /// scroll: the brush's row doesn't slide open to its buttons, as it does for a press whose menu
    /// hasn't shown.
    #[test]
    fn a_late_sideways_move_opens_no_buttons() {
        // 30 sideways, 2 down, with an 8 slop: 0.5 s and 1 s after the menu showed (past the 0.37 s
        // window), the hold leaves to the list, and the row stays shut
        for since_menu in [0.5, 1.0] {
            assert_eq!(brush_hold_move(30.0, 8.0, since_menu, true), BrushHoldMove::Leaves);
            assert!(!brush_row_slides(true));
        }
        // in time, the same move lifts the brush instead (no buttons either)
        assert_eq!(brush_hold_move(30.0, 8.0, 0.2, true), BrushHoldMove::Drags);
        // a Recent brush held, moved sideways at once: the list's, no buttons
        assert_eq!(brush_hold_move(30.0, 8.0, 0.0, false), BrushHoldMove::Leaves);
        // a press whose menu hasn't shown (moved before 0.33 s) slides its row
        assert!(brush_row_slides(false));
    }

    #[test]
    fn the_recent_sets_brushes_never_lift() {
        assert_eq!(brush_hold_move(40.0, 8.0, 0.05, false), BrushHoldMove::Leaves);
        assert_eq!(brush_hold_move(4.0, 8.0, 0.05, false), BrushHoldMove::Stays);
    }
}
