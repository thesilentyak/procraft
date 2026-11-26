//! A press on a colour that can be dragged out, the colour button or a palette's swatch, told
//! apart as the apps' colour button and swatches see their pointer: a tap, a hold, or a drag that
//! pulls the colour out (ColorDrop). The apps feed it the pointer and the time and do what it says.
//!
//! The colour button:
//! - **Held** still for [`COLOUR_HOLD_MS`] it goes back to the previous colour. Still means having
//!   moved under [`STILL`] points since the wait began: each time the press has moved that far the
//!   wait starts again. Once it has moved [`HOLD_REACH`] points all told, it is not a hold.
//! - **Dragged** [`DRAG_AFTER`] points all told, the colour comes out. A press that has been held
//!   never drags out. One that has moved [`DRAG_WAITS_FROM`] points all told and then keeps still
//!   for the hold's time never does either.
//! - Otherwise, let go, it is a **tap**.
//!
//! A swatch is a hold after its own time unless it moved its slop away from where it went down
//! (it drags then), and a tap when let go before either.
//!
//! A pen, a finger or a mouse press alike. Positions are points, times milliseconds on one clock.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::history::COLOUR_HOLD_MS;

/// Moving less than this (points, all told) since a wait began, a press keeps still: each time it
/// has moved this far the wait starts again.
pub const STILL: f32 = 3.0;
/// The colour button's press is no longer a hold once it has moved this far (points, all told).
pub const HOLD_REACH: f32 = 15.0;
/// Moved this far (points, all told), the colour button's colour comes out.
pub const DRAG_AFTER: f32 = 44.0;
/// From this far on (points, all told), the colour button's press kept still for the hold's time
/// no longer drags out.
pub const DRAG_WAITS_FROM: f32 = 15.0;

/// What a press turned out to be, said once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PressTurn {
    /// Nothing yet, or nothing more.
    None,
    Tap,
    Hold,
    /// The colour began being dragged out.
    Drag,
}

/// A wait for the press to keep still: when it is due, and how far it has moved since it began.
#[derive(Clone, Copy, Debug)]
struct Wait {
    due: f64,
    moved: f32,
}

impl Wait {
    fn from(ms: f64) -> Wait {
        Wait { due: ms + f64::from(COLOUR_HOLD_MS), moved: 0.0 }
    }

    /// The press moved `step` at `ms`: moved far enough, the wait starts again.
    fn moved(&mut self, step: f32, ms: f64) {
        self.moved += step;
        if self.moved > STILL {
            *self = Wait::from(ms);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Rules {
    Button {
        /// The hold's wait, until the press is held or can no longer be.
        hold: Option<Wait>,
        /// The drag's wait to give up, once the press has moved far enough to have one.
        stall: Option<Wait>,
        stalled: bool,
    },
    Swatch {
        hold_due: Option<f64>,
        slop: f32,
    },
}

#[derive(Debug)]
struct Press {
    rules: Rules,
    from: (f32, f32),
    last: (f32, f32),
    /// How far it has moved (points, all told).
    path: f32,
    /// It turned out to be something, or was let go.
    told: bool,
}

/// One press, made as it goes down and told each move, the time passing, and the lift.
#[derive(uniffi::Object)]
pub struct ColourPress {
    press: Mutex<Press>,
}

#[uniffi::export]
impl ColourPress {
    /// The colour button pressed at (`x`, `y`) at `ms`.
    #[uniffi::constructor]
    pub fn button(x: f32, y: f32, ms: f64) -> Arc<ColourPress> {
        let rules = Rules::Button { hold: Some(Wait::from(ms)), stall: None, stalled: false };
        ColourPress::with(rules, x, y)
    }

    /// A swatch pressed at (`x`, `y`) at `ms`: held still `hold_ms` (if given) it is a hold, moved
    /// further than `slop` (points) from here it drags.
    #[uniffi::constructor]
    pub fn swatch(x: f32, y: f32, ms: f64, hold_ms: Option<u32>, slop: f32) -> Arc<ColourPress> {
        let rules = Rules::Swatch { hold_due: hold_ms.map(|h| ms + f64::from(h)), slop };
        ColourPress::with(rules, x, y)
    }

    /// The pointer moved to (`x`, `y`) at `ms` (a hold that came due before then comes first).
    pub fn moved(&self, x: f32, y: f32, ms: f64) -> PressTurn {
        let mut p = self.lock();
        let due = p.tick(ms);
        if due != PressTurn::None || p.told {
            return due;
        }
        let step = (x - p.last.0).hypot(y - p.last.1);
        p.last = (x, y);
        p.path += step;
        let (path, away) = (p.path, (x - p.from.0).hypot(y - p.from.1));
        let turn = match &mut p.rules {
            Rules::Button { hold, stall, stalled } => {
                if let Some(wait) = hold {
                    if path > HOLD_REACH {
                        *hold = None;
                    } else {
                        wait.moved(step, ms);
                    }
                }
                // far enough out, keeping still gives up the drag (its wait counts what it moves
                // from here on, and begins once it has moved enough for one)
                if path >= DRAG_WAITS_FROM && !*stalled {
                    stall.get_or_insert(Wait { due: f64::INFINITY, moved: 0.0 }).moved(step, ms);
                }
                if path >= DRAG_AFTER && !*stalled { PressTurn::Drag } else { PressTurn::None }
            }
            Rules::Swatch { slop, .. } => {
                if away > *slop {
                    PressTurn::Drag
                } else {
                    PressTurn::None
                }
            }
        };
        p.tell(turn)
    }

    /// The time is `ms` and the pointer hasn't moved: a hold, if one is due.
    pub fn tick(&self, ms: f64) -> PressTurn {
        self.lock().tick(ms)
    }

    /// Let go at `ms`: a tap, unless it was something else (or a hold came due before then).
    pub fn lifted(&self, ms: f64) -> PressTurn {
        let mut p = self.lock();
        let due = p.tick(ms);
        if due != PressTurn::None || p.told {
            p.told = true;
            return due;
        }
        p.tell(PressTurn::Tap)
    }

    /// When (ms) the press, kept still, turns into something or gives up dragging; none when
    /// nothing is waited for.
    pub fn due(&self) -> Option<f64> {
        let p = self.lock();
        if p.told {
            return None;
        }
        match p.rules {
            Rules::Button { hold: Some(wait), .. } => Some(wait.due),
            Rules::Button { stall: Some(wait), .. } => wait.due.is_finite().then_some(wait.due),
            Rules::Button { .. } => None,
            Rules::Swatch { hold_due, .. } => hold_due,
        }
    }
}

impl ColourPress {
    fn with(rules: Rules, x: f32, y: f32) -> Arc<ColourPress> {
        Arc::new(ColourPress { press: Mutex::new(Press { rules, from: (x, y), last: (x, y), path: 0.0, told: false }) })
    }

    fn lock(&self) -> MutexGuard<'_, Press> {
        self.press.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Press {
    /// What the time reaching `ms` makes of it: a hold, or a drag given up.
    fn tick(&mut self, ms: f64) -> PressTurn {
        if self.told {
            return PressTurn::None;
        }
        let turn = match &mut self.rules {
            Rules::Button { hold, stall, stalled } => {
                if hold.is_some_and(|w| ms >= w.due) {
                    *hold = None;
                    PressTurn::Hold
                } else {
                    if stall.is_some_and(|w| ms >= w.due) {
                        *stall = None;
                        *stalled = true;
                    }
                    PressTurn::None
                }
            }
            Rules::Swatch { hold_due, .. } => {
                if hold_due.is_some_and(|due| ms >= due) {
                    PressTurn::Hold
                } else {
                    PressTurn::None
                }
            }
        };
        self.tell(turn)
    }

    /// Says `turn` once: after it, nothing more.
    fn tell(&mut self, turn: PressTurn) -> PressTurn {
        if turn != PressTurn::None {
            self.told = true;
        }
        turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use PressTurn::{Drag, Hold, None as Nothing, Tap};

    /// The colour button pressed at (100, 20) at 0 ms.
    fn button() -> Arc<ColourPress> {
        ColourPress::button(100.0, 20.0, 0.0)
    }

    /// `press` moved straight down `by` points a step from `from_y`, every 16 ms from `ms`,
    /// `steps` times: what each step made of it.
    fn down(press: &ColourPress, from_y: f32, by: f32, ms: f64, steps: usize) -> Vec<PressTurn> {
        (1..=steps).map(|k| press.moved(100.0, from_y + by * k as f32, ms + 16.0 * k as f64)).collect()
    }

    #[test]
    fn a_tap_opens_the_colours() {
        let press = button();
        assert_eq!(press.due(), Some(300.0));
        assert_eq!(press.moved(101.0, 21.0, 60.0), Nothing);
        assert_eq!(press.lifted(120.0), Tap);
        // said once
        assert_eq!(press.lifted(130.0), Nothing);
        assert_eq!(press.due(), None);
    }

    #[test]
    fn held_still_it_goes_back_to_the_previous_colour_and_then_nothing_more() {
        let press = button();
        assert_eq!(press.tick(299.0), Nothing);
        assert_eq!(press.tick(300.0), Hold);
        assert_eq!(press.due(), None);
        // dragged off afterwards: no drop, and let go, no tap
        assert_eq!(down(&press, 20.0, 10.0, 300.0, 10), vec![Nothing; 10]);
        assert_eq!(press.lifted(600.0), Nothing);
    }

    #[test]
    fn a_hold_comes_first_when_the_next_move_comes_after_it_was_due() {
        // the host never woke for it: the hold still comes before the move that follows
        let press = button();
        assert_eq!(press.moved(160.0, 20.0, 350.0), Hold);
        assert_eq!(press.moved(200.0, 20.0, 366.0), Nothing);
        // nor a tap when let go after it was due
        let press = button();
        assert_eq!(press.lifted(320.0), Hold);
    }

    #[test]
    fn a_finger_s_jitter_is_still_and_each_three_points_of_moving_starts_the_wait_again() {
        let press = button();
        // a point this way and that, three all told: still, due at 300
        for (k, x) in [101.0, 100.0, 101.0].into_iter().enumerate() {
            assert_eq!(press.moved(x, 20.0, 50.0 + 20.0 * k as f64), Nothing);
        }
        assert_eq!(press.due(), Some(300.0));
        // a little more: past three, the wait starts again from there
        assert_eq!(press.moved(101.5, 20.0, 120.0), Nothing);
        assert_eq!(press.due(), Some(420.0));
        assert_eq!(press.tick(300.0), Nothing);
        assert_eq!(press.tick(420.0), Hold);
    }

    #[test]
    fn a_slow_press_that_keeps_moving_never_holds() {
        // two points every 16 ms: never three points still for the hold's time
        let press = button();
        let turns = down(&press, 20.0, 2.0, 0.0, 30);
        // off the hold past 15 points, dragged out at 44 (the 22nd step)
        assert_eq!(turns.iter().position(|t| *t == Drag), Some(21));
        assert!(turns.iter().all(|t| *t != Hold));
    }

    #[test]
    fn a_quick_drag_pulls_the_colour_out_once_it_has_moved_44_points() {
        let press = button();
        assert_eq!(down(&press, 20.0, 10.0, 0.0, 4), vec![Nothing; 4]);
        assert_eq!(press.moved(100.0, 64.0, 80.0), Drag);
        // said once, and no tap when let go
        assert_eq!(press.moved(100.0, 90.0, 96.0), Nothing);
        assert_eq!(press.lifted(200.0), Nothing);
    }

    #[test]
    fn it_goes_by_how_far_the_press_moved_all_told_not_how_far_it_is_from_where_it_went_down() {
        // scrubbed to and fro over five points: out at 44 points all told
        let press = button();
        let mut turns = Vec::new();
        for k in 1..=10 {
            let y = if k % 2 == 1 { 25.0 } else { 20.0 };
            turns.push(press.moved(100.0, y, 16.0 * f64::from(k)));
        }
        assert_eq!(turns.iter().position(|t| *t == Drag), Some(8));
        // past 15 points it is no longer a hold, though it came back where it went down
        let press = button();
        assert_eq!(down(&press, 20.0, 4.0, 0.0, 4), vec![Nothing; 4]);
        assert_eq!(press.moved(100.0, 20.0, 80.0), Nothing);
        assert_eq!(press.tick(1000.0), Nothing);
        assert_eq!(press.lifted(1100.0), Tap);
    }

    #[test]
    fn a_press_that_pauses_past_the_hold_time_and_then_drags_only_goes_back_to_the_previous_colour() {
        // still for the hold's time: the previous colour, and the drag that follows drops nothing
        let press = button();
        assert_eq!(press.moved(100.5, 20.5, 40.0), Nothing);
        assert_eq!(press.tick(300.0), Hold);
        assert_eq!(down(&press, 20.5, 8.0, 500.0, 20), vec![Nothing; 20]);
        assert_eq!(press.lifted(900.0), Nothing);
        // moved within the hold's reach and then paused: a hold too
        let press = button();
        assert_eq!(down(&press, 20.0, 5.0, 0.0, 2), vec![Nothing; 2]);
        assert_eq!(press.due(), Some(332.0));
        assert_eq!(press.tick(332.0), Hold);
        assert_eq!(down(&press, 30.0, 10.0, 400.0, 10), vec![Nothing; 10]);
    }

    #[test]
    fn a_drag_that_starts_within_the_hold_s_reach_and_goes_on_drops() {
        // ten points (within the hold's reach), then on without stopping
        let press = button();
        assert_eq!(down(&press, 20.0, 5.0, 0.0, 2), vec![Nothing; 2]);
        let turns = down(&press, 30.0, 6.0, 32.0, 6);
        assert_eq!(turns, vec![Nothing, Nothing, Nothing, Nothing, Nothing, Drag]);
    }

    #[test]
    fn moved_out_past_15_points_and_kept_still_for_the_hold_s_time_it_never_drags_out() {
        let press = button();
        // 20 points out at 80 ms: no longer a hold, its wait to give up dragging begun
        assert_eq!(down(&press, 20.0, 4.0, 0.0, 5), vec![Nothing; 5]);
        assert_eq!(press.due(), Some(380.0));
        assert_eq!(press.tick(379.0), Nothing);
        // still that long: given up, nothing to wait for, and moved on it never drags out
        assert_eq!(press.tick(380.0), Nothing);
        assert_eq!(press.due(), None);
        assert_eq!(down(&press, 40.0, 10.0, 400.0, 10), vec![Nothing; 10]);
        // let go, it is a tap (on the colour button's own terms)
        assert_eq!(press.lifted(700.0), Tap);
        // kept moving instead, it drags out at 44 points
        let press = button();
        assert_eq!(down(&press, 20.0, 4.0, 0.0, 5), vec![Nothing; 5]);
        assert_eq!(press.tick(300.0), Nothing);
        let turns = down(&press, 40.0, 4.0, 300.0, 6);
        assert_eq!(turns, vec![Nothing, Nothing, Nothing, Nothing, Nothing, Drag]);
    }

    #[test]
    fn a_swatch_holds_after_its_time_and_drags_past_its_slop_from_where_it_went_down() {
        let swatch = ColourPress::swatch(50.0, 50.0, 0.0, Some(400), 8.0);
        assert_eq!(swatch.due(), Some(400.0));
        // eight points to and fro: within its slop
        assert_eq!(swatch.moved(58.0, 50.0, 16.0), Nothing);
        assert_eq!(swatch.moved(50.0, 50.0, 32.0), Nothing);
        assert_eq!(swatch.moved(58.0, 50.0, 48.0), Nothing);
        assert_eq!(swatch.tick(399.0), Nothing);
        assert_eq!(swatch.tick(400.0), Hold);
        // past it: a drag
        let swatch = ColourPress::swatch(50.0, 50.0, 0.0, Some(400), 8.0);
        assert_eq!(swatch.moved(50.0, 58.5, 16.0), Drag);
        // with no hold, it waits for nothing; let go, a tap
        let swatch = ColourPress::swatch(50.0, 50.0, 0.0, None, 8.0);
        assert_eq!(swatch.due(), None);
        assert_eq!(swatch.tick(5000.0), Nothing);
        assert_eq!(swatch.lifted(5000.0), Tap);
    }
}
