//! What a session does by itself: counted, so that a test (and the log, as the app goes away and
//! comes back) can tell that it does nothing while nothing changes; and the changes told to the
//! autosave, which waits for one rather than looking every few seconds.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::Duration;

/// A session's own work since it began, as [`crate::CanvasSession::activity`] tells it.
#[derive(uniffi::Record, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    /// Frames drawn into the window.
    pub frames: u64,
    /// Times the render thread woke to see whether to draw.
    pub wakeups: u64,
    /// Window images it couldn't have, or a surface that couldn't be configured, tried again.
    pub retries: u64,
    /// Times the autosave looked for changes to save.
    pub autosave_looks: u64,
}

impl Activity {
    /// What was done between `before` and this.
    pub fn since(&self, before: &Activity) -> Activity {
        Activity {
            frames: self.frames.saturating_sub(before.frames),
            wakeups: self.wakeups.saturating_sub(before.wakeups),
            retries: self.retries.saturating_sub(before.retries),
            autosave_looks: self.autosave_looks.saturating_sub(before.autosave_looks),
        }
    }
}

/// The counts behind [`Activity`], each moved by the thread doing the work.
#[derive(Default)]
pub(crate) struct Counts {
    pub(crate) frames: AtomicU64,
    pub(crate) wakeups: AtomicU64,
    pub(crate) retries: AtomicU64,
    pub(crate) autosave_looks: AtomicU64,
}

impl Counts {
    pub(crate) fn add(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn now(&self) -> Activity {
        Activity {
            frames: self.frames.load(Ordering::Relaxed),
            wakeups: self.wakeups.load(Ordering::Relaxed),
            retries: self.retries.load(Ordering::Relaxed),
            autosave_looks: self.autosave_looks.load(Ordering::Relaxed),
        }
    }
}

/// Sessions alive now (see [`live_sessions`]).
static LIVE: AtomicU64 = AtomicU64::new(0);

/// How many sessions are alive: made and not yet let go, counting what their own threads still
/// hold of them (the autosave, a fill, the render thread). With no artwork open and none being
/// made, exported or previewed, none: the apps check it once a canvas is left, as a session kept
/// alive keeps all its state (its brushes, its document) for as long as the app runs.
#[uniffi::export]
pub fn live_sessions() -> u64 {
    LIVE.load(Ordering::Relaxed)
}

/// One session alive while this is: kept in the session's shared state, so it counts until the
/// last of whatever holds that state lets go.
pub(crate) struct Alive(());

impl Alive {
    pub(crate) fn new() -> Alive {
        LIVE.fetch_add(1, Ordering::Relaxed);
        Alive(())
    }
}

impl Drop for Alive {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Changes to the session, told to whoever waits for one (the autosave) without the canvas's lock:
/// a count that moves with each, and whether the session has gone.
#[derive(Default)]
pub(crate) struct Nudge {
    state: Mutex<(u64, bool)>,
    changed: Condvar,
}

impl Nudge {
    /// Something changed.
    pub(crate) fn nudge(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.0 = state.0.wrapping_add(1);
        self.changed.notify_all();
    }

    /// The session has gone: waits end, and none waits again.
    pub(crate) fn close(&self) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).1 = true;
        self.changed.notify_all();
    }

    /// The count as it is.
    pub(crate) fn seen(&self) -> u64 {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).0
    }

    /// Waits until the count has moved from `seen` (at most `timeout`, if given): the count then,
    /// or none once the session has gone. Past the timeout with nothing changed, `seen` again.
    pub(crate) fn wait_past(&self, seen: u64, timeout: Option<Duration>) -> Option<u64> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let unchanged = |s: &mut (u64, bool)| s.0 == seen && !s.1;
        let state = match timeout {
            Some(t) => self.changed.wait_timeout_while(state, t, unchanged).unwrap_or_else(PoisonError::into_inner).0,
            None => self.changed.wait_while(state, unchanged).unwrap_or_else(PoisonError::into_inner),
        };
        (!state.1).then_some(state.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_ends_with_a_change_or_the_session_going_and_never_by_itself() {
        let nudge = std::sync::Arc::new(Nudge::default());
        let seen = nudge.seen();
        // nothing changes: a wait with a limit ends at it, unchanged
        let started = std::time::Instant::now();
        assert_eq!(nudge.wait_past(seen, Some(Duration::from_millis(30))), Some(seen));
        assert!(started.elapsed() >= Duration::from_millis(30));
        // a change from another thread ends a wait without a limit
        let other = std::sync::Arc::clone(&nudge);
        let changer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            other.nudge();
        });
        assert_eq!(nudge.wait_past(seen, None), Some(seen + 1));
        changer.join().unwrap();
        // the session going ends it too, and says so
        let other = std::sync::Arc::clone(&nudge);
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            other.close();
        });
        assert_eq!(nudge.wait_past(seen + 1, None), None);
        closer.join().unwrap();
        assert_eq!(nudge.wait_past(0, None), None);
    }
}
