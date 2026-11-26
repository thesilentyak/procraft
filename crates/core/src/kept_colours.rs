//! The colour painted with and the one masks paint with, as last left, the same in every app: the
//! app holds them, tells each change, and they are written to the file it names as they change,
//! so the next start finds them however this run ends (closed, put away, or killed outright,
//! which runs nothing of the app's on the way out). Colours are hue, saturation and brightness
//! (0..1), so a grey keeps the hue it was chosen on.
//!
//! The file is one line, `h s b mh ms mb`: the brush colour and the masks'. A change is written
//! whole (beside the file, then renamed over it) on a thread of its own, at once, and while the
//! colour keeps changing (a finger dragged over the colour disc) at most once every
//! [`COLOUR_WRITE_GAP_MS`], the latest each time: the file is never more than that behind.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// The shortest time (milliseconds) between two writes of the colours: the longest the file is
/// behind a colour still changing.
pub const COLOUR_WRITE_GAP_MS: u64 = 200;

/// The brush colour a first start has: a saturated blue.
pub const FIRST_BRUSH_COLOUR: [f32; 3] = [0.59, 1.0, 1.0];

/// The colour masks paint with at first: black.
pub const FIRST_MASK_COLOUR: [f32; 3] = [0.0, 0.0, 0.0];

#[derive(uniffi::Object)]
pub struct KeptColours {
    shared: Arc<Shared>,
}

/// What the app's calls and the thread writing the file share.
struct Shared {
    state: Mutex<State>,
    /// Told as each write ends.
    written: Condvar,
    /// Where the colours are kept, if anywhere.
    file: Option<PathBuf>,
    /// Writing them there failed, and was logged: not again until a write works.
    failing: AtomicBool,
    /// How many times they were written, for the tests.
    writes: AtomicU32,
}

struct State {
    brush: [f32; 3],
    mask: [f32; 3],
    /// The file held them as the app started.
    found: bool,
    /// They changed since they were last written.
    unwritten: bool,
    /// A write is under way.
    writing: bool,
    /// The thread that writes them is running (writing, or waiting out the gap after a write).
    writer: bool,
}

#[uniffi::export]
impl KeptColours {
    /// Colours kept only while the app runs, starting on the first ones.
    #[uniffi::constructor]
    pub fn new() -> Arc<KeptColours> {
        Arc::new(KeptColours::with(None, None, None))
    }

    /// The colours kept at `path`, as the app last left them; each read leniently (a colour that
    /// isn't there whole, or no file, is the first one). Each change is written there.
    #[uniffi::constructor]
    pub fn open(path: String) -> Arc<KeptColours> {
        let file = PathBuf::from(path);
        let (brush, mask) = std::fs::read(&file).map(|b| read_kept(&String::from_utf8_lossy(&b))).unwrap_or_default();
        Arc::new(KeptColours::with(Some(file), brush, mask))
    }

    /// The brush colour: hue, saturation, brightness.
    pub fn brush(&self) -> Vec<f32> {
        self.state().brush.to_vec()
    }

    /// The colour masks paint with: hue, saturation, brightness.
    pub fn mask(&self) -> Vec<f32> {
        self.state().mask.to_vec()
    }

    /// Whether the file held a colour as it was opened (else the app may carry over the colours it
    /// kept elsewhere before).
    pub fn was_kept(&self) -> bool {
        self.state().found
    }

    /// The brush colour and the masks' now (hue, saturation, brightness; each clamped to 0..1). A
    /// colour that isn't three numbers is left as it was. Cheap enough for every change, or every
    /// frame: unchanged, nothing happens; changed, it is written soon, on another thread.
    pub fn keep(&self, brush: Vec<f32>, mask: Vec<f32>) {
        let mut state = self.state();
        let brush = colour_of(&brush).unwrap_or(state.brush);
        let mask = colour_of(&mask).unwrap_or(state.mask);
        if brush == state.brush && mask == state.mask {
            return;
        }
        state.brush = brush;
        state.mask = mask;
        if self.shared.file.is_none() {
            return;
        }
        state.unwritten = true;
        if state.writer {
            return;
        }
        state.writer = true;
        drop(state);
        let shared = self.shared.clone();
        let started =
            std::thread::Builder::new().name("procraft-colour".to_owned()).spawn(move || shared.write_while_changing());
        if let Err(e) = started {
            // no thread to be had: written here, now
            log::warn!("writing the colour on a thread of its own: {e}");
            self.shared.state().writer = false;
            self.shared.write_now();
        }
    }
}

impl KeptColours {
    fn with(file: Option<PathBuf>, brush: Option<[f32; 3]>, mask: Option<[f32; 3]>) -> KeptColours {
        let state = State {
            brush: brush.unwrap_or(FIRST_BRUSH_COLOUR),
            mask: mask.unwrap_or(FIRST_MASK_COLOUR),
            found: brush.is_some() || mask.is_some(),
            unwritten: false,
            writing: false,
            writer: false,
        };
        let shared = Shared {
            state: Mutex::new(state),
            written: Condvar::new(),
            file,
            failing: AtomicBool::new(false),
            writes: AtomicU32::new(0),
        };
        KeptColours { shared: Arc::new(shared) }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.shared.state()
    }

    /// Waits up to `limit` for every change to be written; whether it was.
    #[cfg(test)]
    fn wait_written(&self, limit: Duration) -> bool {
        let state = self.state();
        let waited = self.shared.written.wait_timeout_while(state, limit, |s| s.unwritten || s.writing);
        let (state, _) = waited.unwrap_or_else(std::sync::PoisonError::into_inner);
        !state.unwritten && !state.writing
    }
}

impl Drop for KeptColours {
    /// The app done with them (the desktop's window closed): a change still waiting out the gap is
    /// written now. (Killed, this doesn't run; a change waits no longer than the gap.)
    fn drop(&mut self) {
        self.shared.write_now();
    }
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The writing thread: the colours written, then, after the gap, again if they changed
    /// meanwhile, until a gap passes with no change (seen under the lock a change takes, so a
    /// change as it ends starts another thread).
    fn write_while_changing(&self) {
        loop {
            self.write_now();
            std::thread::sleep(Duration::from_millis(COLOUR_WRITE_GAP_MS));
            let mut state = self.state();
            if !state.unwritten {
                state.writer = false;
                return;
            }
        }
    }

    /// The colours written if they changed since last written (one write at a time: one under way
    /// is waited for); whether they were.
    fn write_now(&self) -> bool {
        let mut state = self.state();
        while state.writing {
            state = self.written.wait(state).unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if !state.unwritten {
            return false;
        }
        state.unwritten = false;
        state.writing = true;
        let text = write_kept(state.brush, state.mask);
        drop(state);
        self.write(&text);
        self.state().writing = false;
        self.written.notify_all();
        true
    }

    /// `text` written whole to the file.
    fn write(&self, text: &str) {
        let Some(file) = &self.file else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let written = crate::project::write_atomically(file, |tmp| {
            crate::storage::write(tmp, text).map_err(|e| crate::project::file_error("saving", e))
        });
        self.writes.fetch_add(1, Ordering::Relaxed);
        match written {
            Ok(()) => self.failing.store(false, Ordering::Relaxed),
            Err(e) => {
                if !self.failing.swap(true, Ordering::Relaxed) {
                    log::error!("keeping the colour: {e}");
                }
            }
        }
    }
}

/// A colour from three numbers, each clamped to 0..1; none from anything else.
fn colour_of(values: &[f32]) -> Option<[f32; 3]> {
    match values {
        [h, s, b] if [h, s, b].iter().all(|c| c.is_finite()) => Some([*h, *s, *b].map(|c| c.clamp(0.0, 1.0))),
        _ => None,
    }
}

/// The colours as the file keeps them: one line, `h s b mh ms mb`.
fn write_kept([h, s, b]: [f32; 3], [mh, ms, mb]: [f32; 3]) -> String {
    format!("{h} {s} {b} {mh} {ms} {mb}\n")
}

/// The brush colour and the masks' a file keeps, leniently: each that is there whole, as three
/// numbers in its place (clamped to 0..1); a file cut short keeps the colours before the cut.
fn read_kept(text: &str) -> (Option<[f32; 3]>, Option<[f32; 3]>) {
    let values: Vec<Option<f32>> =
        text.split_whitespace().take(6).map(|t| t.parse::<f32>().ok().filter(|c| c.is_finite())).collect();
    let colour = |at: usize| match values.get(at..at + 3)? {
        [Some(h), Some(s), Some(b)] => colour_of(&[*h, *s, *b]),
        _ => None,
    };
    (colour(0), colour(3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("procraft-kept-colours-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn path(dir: &std::path::Path) -> String {
        dir.join("colour").to_string_lossy().into_owned()
    }

    /// The app killed (force-stopped, or out of memory) right after a colour was chosen: nothing of
    /// it runs on the way out, no pause, no close, not even the colours' own drop. Started again
    /// on the same folder, the colour is the one chosen (it was kept only as the app went into the
    /// background, so a kill in front lost it). The history, written as it changes, too.
    #[test]
    fn a_colour_chosen_is_there_when_the_app_starts_again_after_being_killed() {
        let dir = dir("killed");
        let kept = KeptColours::open(path(&dir));
        assert!(!kept.was_kept(), "a first start");
        assert_eq!(kept.brush(), FIRST_BRUSH_COLOUR.to_vec());
        assert_eq!(kept.mask(), FIRST_MASK_COLOUR.to_vec());
        let history = crate::ColourHistory::open(dir.join("colour-history.json").to_string_lossy().into_owned());
        let session = crate::CanvasSession::new(64, 64);
        session.use_colour_history(history.clone());

        // an orange chosen, a stroke painted in it, a grey for masks
        let (orange, grey) = (vec![0.07, 0.9, 0.95], vec![0.3, 0.0, 0.5]);
        let chosen = Instant::now();
        kept.keep(orange.clone(), grey.clone());
        session.set_color(0.95, 0.4, 0.1);
        let at = |ms| {
            vec![crate::PenSample {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
                tilt: 0.0,
                orientation: 0.0,
                time_ms: ms,
                roll: 0.0,
            }]
        };
        session.stroke_end(at(0.0));
        // written at once on its own thread (a lone change waits for no gap): on disk well before
        // anyone could kill the app
        assert!(kept.wait_written(Duration::from_secs(10)), "written");
        let took = chosen.elapsed();
        println!("a colour chosen was on disk {:.2} ms later", took.as_secs_f64() * 1e3);
        assert!(took < Duration::from_secs(2), "written {took:?} after it was chosen");

        // killed: nothing runs on the way out
        std::mem::forget(kept);
        std::mem::forget(session);
        std::mem::forget(history);

        let again = KeptColours::open(path(&dir));
        assert!(again.was_kept());
        assert_eq!(again.brush(), orange, "the brush colour chosen");
        assert_eq!(again.mask(), grey, "the masks' colour");
        let history = crate::ColourHistory::open(dir.join("colour-history.json").to_string_lossy().into_owned());
        assert_eq!(history.colours().first(), Some(&0xF2661A), "the colour painted with heads the history");
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Killed while the colours were being written: what's left is the last whole file and a
    /// half-written one beside it, which is never read. A file cut short anyhow (or damaged) keeps
    /// each colour there whole and starts the rest on the first; the next change writes it whole.
    #[test]
    fn a_file_cut_short_is_read_leniently_and_keeps_a_usable_colour() {
        let dir = dir("cut");
        let file = dir.join("colour");
        let kept = KeptColours::open(path(&dir));
        kept.keep(vec![0.25, 0.5, 0.75], vec![0.1, 0.2, 0.3]);
        assert!(kept.wait_written(Duration::from_secs(10)));
        std::mem::forget(kept);
        // a write cut off half-way, beside it
        std::fs::write(dir.join("colour.7.tmp"), b"0.9 0.9 0.").unwrap();
        let again = KeptColours::open(path(&dir));
        assert_eq!((again.brush(), again.mask()), (vec![0.25, 0.5, 0.75], vec![0.1, 0.2, 0.3]), "the last whole file");
        drop(again);

        let read = |text: &[u8]| {
            std::fs::write(&file, text).unwrap();
            let k = KeptColours::open(path(&dir));
            (k.brush(), k.mask(), k.was_kept())
        };
        let (first, black) = (FIRST_BRUSH_COLOUR.to_vec(), FIRST_MASK_COLOUR.to_vec());
        // cut in the masks' colour: the brush colour kept
        assert_eq!(read(b"0.25 0.5 0.75 0.1 0."), (vec![0.25, 0.5, 0.75], black.clone(), true));
        // cut in the brush colour: the first colours, as a first start
        assert_eq!(read(b"0.25 0.5"), (first.clone(), black.clone(), false));
        assert_eq!(read(b""), (first.clone(), black.clone(), false));
        // the desktop's older file (no line end; three numbers before masks had their own)
        assert_eq!(read(b"0.59 0.2 0.8"), (vec![0.59, 0.2, 0.8], black.clone(), true));
        // damaged: a value that isn't a number costs only its colour; out of range is clamped
        assert_eq!(read(b"0.1 x 0.3 0.4 0.5 0.6"), (first.clone(), vec![0.4, 0.5, 0.6], true));
        assert_eq!(read(b"NaN 0.5 0.5 inf 0 0"), (first.clone(), black.clone(), false));
        assert_eq!(read(b"1.5 -2 0.5 0 0 9\n"), (vec![1.0, 0.0, 0.5], vec![0.0, 0.0, 1.0], true));
        assert_eq!(read(b"\x00\x00\xFF\xFE garbage"), (first.clone(), black.clone(), false));
        // the next change written whole over it
        let k = KeptColours::open(path(&dir));
        k.keep(vec![0.5, 0.5, 0.5], k.mask());
        assert!(k.wait_written(Duration::from_secs(10)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "0.5 0.5 0.5 0 0 0\n");
        drop(k);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Keeping the colour costs a lock and a compare, so it can be told on every change (or every
    /// frame); a colour dragged across the disc is written at most once every COLOUR_WRITE_GAP_MS, the
    /// last one always; a write itself is a small file renamed into place.
    #[test]
    fn keeping_the_colour_on_every_change_is_cheap_and_writes_at_most_once_a_gap() {
        let dir = dir("cheap");
        let kept = KeptColours::open(path(&dir));
        let shared = kept.shared.clone();

        // told every frame with no change: nothing written
        for _ in 0..1000 {
            kept.keep(FIRST_BRUSH_COLOUR.to_vec(), FIRST_MASK_COLOUR.to_vec());
        }
        assert_eq!(shared.writes.load(Ordering::Relaxed), 0, "unchanged: nothing written");
        // and something that isn't a colour changes nothing
        kept.keep(vec![0.5], vec![f32::NAN, 0.0, 0.0]);
        assert_eq!(kept.brush(), FIRST_BRUSH_COLOUR.to_vec());
        assert_eq!(shared.writes.load(Ordering::Relaxed), 0);

        // a drag: a change every half millisecond for a second (many more than a screen shows)
        let start = Instant::now();
        let mut changes = 0u32;
        let mut busy = Duration::ZERO;
        let mut last = Vec::new();
        while start.elapsed() < Duration::from_millis(1000) {
            last = vec![(changes % 1000) as f32 / 1000.0, 0.8, 0.9];
            let t = Instant::now();
            kept.keep(last.clone(), FIRST_MASK_COLOUR.to_vec());
            busy += t.elapsed();
            changes += 1;
            std::thread::sleep(Duration::from_micros(500));
        }
        let lasted = start.elapsed();
        assert!(kept.wait_written(Duration::from_secs(10)));
        let writes = shared.writes.load(Ordering::Relaxed);
        let per_keep = busy.as_secs_f64() / f64::from(changes) * 1e6;
        let most = (lasted.as_millis() as u64 / COLOUR_WRITE_GAP_MS + 2) as u32;
        println!(
            "{changes} changes over {:.0} ms: {writes} writes (at most {most}); keep() {per_keep:.2} µs each",
            lasted.as_secs_f64() * 1e3
        );
        assert!(writes >= 2, "written as it changes, not only at the end: {writes}");
        assert!(writes <= most, "{writes} writes for {:?}: more than one a gap", lasted);
        assert!(per_keep < 100.0, "keep() takes {per_keep:.2} µs: no disk on the caller's thread");
        let again = KeptColours::open(path(&dir));
        assert_eq!(again.brush(), last, "the last colour of the drag written");
        drop(again);

        // a write: the small file renamed into place
        let n = 50;
        let t = Instant::now();
        for i in 0..n {
            shared.write(&write_kept([i as f32 / 100.0, 0.5, 0.5], FIRST_MASK_COLOUR));
        }
        let per_write = t.elapsed().as_secs_f64() / f64::from(n) * 1e3;
        println!("a write takes {per_write:.3} ms");
        assert!(per_write < 50.0, "a write takes {per_write:.3} ms");
        assert_eq!(KeptColours::open(path(&dir)).brush(), vec![0.49, 0.5, 0.5]);
        drop(kept);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The desktop's window closed less than a gap after the last change: dropping the colours
    /// writes it now. Kept only in memory, nothing is written anywhere.
    #[test]
    fn closing_writes_a_change_still_waiting_out_the_gap() {
        let dir = dir("closing");
        let kept = KeptColours::open(path(&dir));
        kept.keep(vec![0.1, 0.1, 0.1], vec![0.0, 0.0, 0.0]);
        kept.keep(vec![0.2, 0.2, 0.2], vec![0.0, 0.0, 0.0]);
        drop(kept);
        assert_eq!(KeptColours::open(path(&dir)).brush(), vec![0.2, 0.2, 0.2]);
        let _ = std::fs::remove_dir_all(&dir);

        let memory = KeptColours::new();
        memory.keep(vec![0.3, 0.3, 0.3], vec![0.0, 0.0, 0.0]);
        assert_eq!(memory.brush(), vec![0.3, 0.3, 0.3]);
        assert!(memory.wait_written(Duration::ZERO));
        assert_eq!(memory.shared.writes.load(Ordering::Relaxed), 0);
    }
}
