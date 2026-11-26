//! Work done side by side on a few threads (encoding a file's layers, packing tiles), its results
//! in order. A thread that can't be made (out of memory, or of address space in a 32-bit build)
//! never fails the work: what it would have done is done by the others, or by the thread asking.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// How many threads work is spread over: one to a core, 8 at most (2 in a 32-bit build, whose
/// memory and address space each thread's stack takes from are small), and no more than
/// [`hold_threads_to`] says (nor, for work begun on a thread, [`within_threads`]).
pub fn cores() -> usize {
    let most = if cfg!(target_pointer_width = "32") { 2 } else { 8 };
    let held = HELD_TO.load(Ordering::Relaxed).max(1).min(HELD_HERE.with(std::cell::Cell::get));
    std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, most).min(held)
}

std::thread_local! {
    /// The most threads work begun on this thread is spread over (see [`within_threads`]).
    // (already const: clippy 1.99 asks for it again when built for Android)
    #[allow(clippy::missing_const_for_thread_local)]
    static HELD_HERE: std::cell::Cell<usize> = const { std::cell::Cell::new(usize::MAX) };
}

/// `f`, the work it begins on this thread done side by side on `threads` threads at the most:
/// work in the background, leaving the other cores to what is drawn meanwhile. What it works out
/// is the same however many threads share it.
pub fn within_threads<R>(threads: usize, f: impl FnOnce() -> R) -> R {
    let before = HELD_HERE.with(|held| held.replace(threads.max(1)));
    let out = f();
    HELD_HERE.with(|held| held.set(before));
    out
}

/// The most threads work is spread over, whatever the cores (see [`hold_threads_to`]).
static HELD_TO: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Holds work done side by side to at most `threads` threads (on a device short of memory); 0 lets
/// it have one to a core again.
pub fn hold_threads_to(threads: usize) {
    HELD_TO.store(if threads == 0 { usize::MAX } else { threads }, Ordering::Relaxed);
}

/// How much memory work done side by side may hold at once (as many work together as fit): less
/// on a tablet than on a computer, and less again in a 32-bit build.
const BUDGET: usize = if cfg!(target_pointer_width = "32") {
    64 << 20
} else if cfg!(target_os = "android") {
    256 << 20
} else {
    1 << 30
};

/// Whether threads are taken as never made (see [`threads_fail`]).
static NO_THREADS: AtomicBool = AtomicBool::new(false);

/// Makes every thread asked for here fail to be made, as on a device out of memory or of address
/// space, or lets them be made again: for tests, which check that work comes out the same done
/// without them (anything else running meanwhile only takes longer).
#[doc(hidden)]
pub fn threads_fail(on: bool) {
    NO_THREADS.store(on, Ordering::Relaxed);
}

/// A thread of `scope`'s running `f`, if one can be made.
fn try_spawn<'scope, F: FnOnce() + Send + 'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    f: F,
) -> Option<std::thread::ScopedJoinHandle<'scope, ()>> {
    if NO_THREADS.load(Ordering::Relaxed) {
        return None;
    }
    std::thread::Builder::new().spawn_scoped(scope, f).ok()
}

/// Runs `f` on a thread of `scope`'s, or in this one where none can be made.
pub fn beside<'scope, F: FnOnce() + Send + 'scope>(scope: &'scope std::thread::Scope<'scope, '_>, f: F) {
    let job = Arc::new(Mutex::new(Some(f)));
    let theirs = Arc::clone(&job);
    let take = |job: &Mutex<Option<F>>| job.lock().unwrap_or_else(PoisonError::into_inner).take();
    let spawned = try_spawn(scope, move || {
        if let Some(f) = take(&theirs) {
            f();
        }
    });
    if spawned.is_none()
        && let Some(f) = take(&job)
    {
        f();
    }
}

/// `f` of each of `items`, worked out side by side on a few threads (each taking the next item
/// there is), in their order; none for one whose thread stopped.
pub fn side_by_side<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<Option<R>> {
    side_by_side_within(items, 0, f)
}

/// [`side_by_side`] for work holding about `each` bytes an item while it is done: no more of them
/// at once than fit in the budget (one, at least).
pub fn side_by_side_within<T: Sync, R: Send>(items: &[T], each: usize, f: impl Fn(&T) -> R + Sync) -> Vec<Option<R>> {
    let fit = BUDGET.checked_div(each).unwrap_or(usize::MAX).max(1);
    let workers = cores().min(items.len()).min(fit);
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::new());
    // a worker takes the next item there is until none are left, then puts what it did in `done`
    let work = || {
        let mut mine = Vec::new();
        loop {
            let i = next.fetch_add(1, Ordering::Relaxed);
            let Some(item) = items.get(i) else { break };
            mine.push((i, f(item)));
        }
        done.lock().unwrap_or_else(PoisonError::into_inner).extend(mine);
    };
    std::thread::scope(|scope| {
        let work = &work;
        let workers: Vec<_> = (0..workers).filter_map(|_| try_spawn(scope, work)).collect();
        // where none could be made, this thread does it all
        if workers.is_empty() {
            work();
        }
        for w in workers {
            // (one that stopped leaves its items undone)
            let _ = w.join();
        }
    });
    let mut out: Vec<Option<R>> = std::iter::repeat_with(|| None).take(items.len()).collect();
    for (i, r) in done.into_inner().unwrap_or_else(PoisonError::into_inner) {
        if let Some(slot) = out.get_mut(i) {
            *slot = Some(r);
        }
    }
    out
}

/// `f` of each `part`-long piece of `items` (the last perhaps shorter), side by side on a few
/// threads (each taking the next piece there is); no threads for items of one piece or less.
pub fn each_part<T: Send>(items: &mut [T], part: usize, f: impl Fn(&mut [T]) + Sync) {
    let part = part.max(1);
    if items.len() <= part {
        f(items);
        return;
    }
    let workers = cores().min(items.len().div_ceil(part));
    let parts = Mutex::new(items.chunks_mut(part));
    let work = || {
        while let Some(piece) = parts.lock().ok().and_then(|mut p| p.next()) {
            f(piece);
        }
    };
    std::thread::scope(|scope| {
        let work = &work;
        let spawned = (0..workers).filter_map(|_| try_spawn(scope, work)).count();
        if spawned == 0 {
            work();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Work begun within a few threads is spread over no more, on that thread only, and after it as
    /// before.
    #[test]
    fn work_held_to_a_few_threads_is_spread_over_no_more() {
        let all = cores();
        assert_eq!(within_threads(1, cores), 1);
        assert_eq!(within_threads(2, cores), all.min(2));
        assert_eq!(within_threads(0, cores), 1);
        // another thread is not held
        assert_eq!(within_threads(1, || std::thread::spawn(cores).join().unwrap()), all);
        assert_eq!(cores(), all);
    }

    #[test]
    fn results_come_back_in_order() {
        let items: Vec<u32> = (0..1000).collect();
        let out = side_by_side(&items, |v| v * 2);
        assert!(out.iter().enumerate().all(|(i, r)| *r == Some(i as u32 * 2)));
        assert!(side_by_side(&[] as &[u32], |v| *v).is_empty());
        // work too big for two at once still comes back whole, in order
        let out = side_by_side_within(&items, BUDGET / 2 + 1, |v| v + 1);
        assert!(out.iter().enumerate().all(|(i, r)| *r == Some(i as u32 + 1)));
    }

    #[test]
    fn each_part_is_done_once() {
        for (len, part) in [(1000, 7), (1000, 1000), (5, 0), (0, 3)] {
            let mut items: Vec<u32> = (0..len).collect();
            each_part(&mut items, part, |piece| piece.iter_mut().for_each(|v| *v = *v * 3 + 1));
            assert!(items.iter().enumerate().all(|(i, v)| *v == i as u32 * 3 + 1), "{len} {part}");
        }
    }

    /// Work done where no thread can be made (a device out of memory or address space) comes out
    /// whole and the same, done by the thread asking; and held to fewer threads, likewise.
    #[test]
    fn work_goes_on_where_no_thread_can_be_made() {
        let items: Vec<u32> = (0..500).collect();
        let whole = side_by_side(&items, |v| v * 5);
        let mut parts: Vec<u32> = (0..500).collect();
        each_part(&mut parts, 9, |piece| piece.iter_mut().for_each(|v| *v += 2));
        let mut beside_done = vec![0u32; 4];
        threads_fail(true);
        assert_eq!(side_by_side(&items, |v| v * 5), whole);
        let mut alone: Vec<u32> = (0..500).collect();
        each_part(&mut alone, 9, |piece| piece.iter_mut().for_each(|v| *v += 2));
        std::thread::scope(|s| {
            for (k, slot) in beside_done.iter_mut().enumerate() {
                beside(s, move || *slot = k as u32 + 1);
            }
        });
        threads_fail(false);
        assert_eq!(alone, parts);
        assert_eq!(beside_done, [1, 2, 3, 4]);
        hold_threads_to(1);
        assert_eq!(cores(), 1);
        assert_eq!(side_by_side(&items, |v| v * 5), whole);
        hold_threads_to(0);
        assert!(cores() >= 1);
    }
}
