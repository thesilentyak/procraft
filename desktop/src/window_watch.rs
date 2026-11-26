//! What the window's surface and the window went through, for the frames and the log. A frame the
//! surface gives no picture for is drawn again: soon, then less and less often while none comes.
//! The log says when the surface gave none and when it showed again, when the system counted the
//! window as hidden or shown, and when it went full screen or came back, without a line for each
//! frame of a long wait.
//!
//! On a Mac the surface gives no picture while the system counts the window as hidden: wgpu asks
//! the window before each frame. The system can count a window as hidden for a moment after it
//! shows (as it opens, or once a full screen space is ready). A frame missed then was drawn again
//! only when something else asked for one, and the window could stay black.

use std::time::{Duration, Instant};

use procraft_gpu::wgpu;

/// Why the window's surface gave no picture for a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Miss {
    /// The system counts the window as hidden: covered, minimised, in another space or going into
    /// one (a Mac's).
    Occluded,
    /// No picture came in time.
    Timeout,
    /// The window changed since the surface was set up.
    Outdated,
    /// The surface is gone.
    Lost,
    /// The ask itself was wrong (wgpu says why in the log).
    Validation,
}

impl Miss {
    const ALL: [Miss; 5] = [Miss::Occluded, Miss::Timeout, Miss::Outdated, Miss::Lost, Miss::Validation];

    fn word(self) -> &'static str {
        match self {
            Miss::Occluded => "occluded",
            Miss::Timeout => "timed out",
            Miss::Outdated => "outdated",
            Miss::Lost => "lost",
            Miss::Validation => "invalid",
        }
    }

    fn index(self) -> usize {
        Miss::ALL.iter().position(|&m| m == self).unwrap_or(0)
    }
}

/// The picture the surface gave for a frame, or why it gave none.
pub fn picture(status: wgpu::CurrentSurfaceTexture) -> Result<wgpu::SurfaceTexture, Miss> {
    match status {
        wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => Ok(f),
        wgpu::CurrentSurfaceTexture::Timeout => Err(Miss::Timeout),
        wgpu::CurrentSurfaceTexture::Occluded => Err(Miss::Occluded),
        wgpu::CurrentSurfaceTexture::Outdated => Err(Miss::Outdated),
        wgpu::CurrentSurfaceTexture::Lost => Err(Miss::Lost),
        wgpu::CurrentSurfaceTexture::Validation => Err(Miss::Validation),
    }
}

/// What a frame the surface gave no picture for does: whether it sets the surface up again first,
/// and how soon it is drawn again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retry {
    pub set_up_again: bool,
    pub after: Duration,
}

/// The first wait after a miss that is waited out, and the longest: the wait doubles from one to
/// the other while the misses go on.
pub const RETRY_FIRST: Duration = Duration::from_millis(100);
pub const RETRY_MOST: Duration = Duration::from_secs(1);

/// The same for a surface that timed out or was asked wrongly. A timeout has waited a second for
/// its picture already (wgpu's wait), with the window's thread held, and a wrong ask says so in
/// the log each time: tried again from a second, up to every 8 s.
pub const SLOW_RETRY_FIRST: Duration = Duration::from_secs(1);
pub const SLOW_RETRY_MOST: Duration = Duration::from_secs(8);

/// How many outdated or lost surfaces in a row are set up again and drawn at once: a window being
/// resized gives one now and then. More in a row wait as the other misses do: a surface that stays
/// so would otherwise be drawn as fast as the computer can.
pub const AT_ONCE_MOST: u32 = 3;

/// The wait after the `n`th miss in a row that waits (1 the first): from `first`, doubling, up to
/// `most`.
fn backoff(n: u32, first: Duration, most: Duration) -> Duration {
    let doublings = n.saturating_sub(1).min(16);
    first.saturating_mul(1 << doublings).min(most)
}

/// What a frame does after `miss`, the `in_a_row`th miss in a row (1 the first). Every miss is drawn
/// again: none waits for something else to ask for a frame.
pub fn after_miss(miss: Miss, in_a_row: u32) -> Retry {
    let (quick, slow) = ((RETRY_FIRST, RETRY_MOST), (SLOW_RETRY_FIRST, SLOW_RETRY_MOST));
    let (set_up_again, waits, n) = match miss {
        Miss::Outdated | Miss::Lost if in_a_row <= AT_ONCE_MOST => {
            return Retry { set_up_again: true, after: Duration::ZERO };
        }
        Miss::Outdated | Miss::Lost => (true, quick, in_a_row - AT_ONCE_MOST),
        Miss::Occluded => (false, quick, in_a_row),
        Miss::Timeout => (false, slow, in_a_row),
        Miss::Validation => (true, slow, in_a_row),
    };
    Retry { set_up_again, after: backoff(n, waits.0, waits.1) }
}

/// At most `most` lines each `span` for something that can happen over and over. The lines left out
/// are counted and said with the next line logged.
pub struct LogBudget {
    most: u32,
    span: Duration,
    started: Option<Instant>,
    used: u32,
    left_out: u32,
}

impl LogBudget {
    pub const fn new(most: u32, span: Duration) -> LogBudget {
        LogBudget { most, span, started: None, used: 0, left_out: 0 }
    }

    /// Whether a line may be logged at `now`, and if so what goes at its end: how many were left out
    /// before it, and whether it is the last for a while.
    pub fn take(&mut self, now: Instant) -> Option<String> {
        if self.started.is_none_or(|at| now.saturating_duration_since(at) >= self.span) {
            self.started = Some(now);
            self.used = 0;
        }
        if self.used >= self.most {
            self.left_out = self.left_out.saturating_add(1);
            return None;
        }
        self.used += 1;
        let mut end = String::new();
        if self.left_out > 0 {
            end.push_str(&format!(" ({} more not logged before)", std::mem::take(&mut self.left_out)));
        }
        if self.used == self.most {
            end.push_str(&format!(" (no more like it for {} s)", self.span.as_secs()));
        }
        Some(end)
    }
}

/// How many runs of misses each [`MISS_LOG_SPAN`] the log tells of.
pub const MISS_LOG_MOST: u32 = 10;
pub const MISS_LOG_SPAN: Duration = Duration::from_secs(60);
/// How often a run of misses that goes on is told again.
pub const STILL_TOLD_EVERY: Duration = Duration::from_secs(30);

/// The window's surface's misses in a row: how soon each frame is drawn again, and what the log
/// says of them.
pub struct SurfaceMisses {
    in_a_row: u32,
    /// Since when, and how many of each kind (in the order of [`Miss::ALL`]).
    since: Option<Instant>,
    counts: [u32; 5],
    /// Whether this run's start was told (its end is then too), and when it was last told.
    told_at: Option<Instant>,
    budget: LogBudget,
}

impl Default for SurfaceMisses {
    fn default() -> SurfaceMisses {
        SurfaceMisses {
            in_a_row: 0,
            since: None,
            counts: [0; 5],
            told_at: None,
            budget: LogBudget::new(MISS_LOG_MOST, MISS_LOG_SPAN),
        }
    }
}

impl SurfaceMisses {
    /// The surface gave no picture at `now`, for `miss`: what the frame does, and a line for the log.
    pub fn missed(&mut self, miss: Miss, now: Instant) -> (Retry, Option<String>) {
        self.in_a_row = self.in_a_row.saturating_add(1);
        let since = *self.since.get_or_insert(now);
        if let Some(count) = self.counts.get_mut(miss.index()) {
            *count = count.saturating_add(1);
        }
        let retry = after_miss(miss, self.in_a_row);
        let line = if self.in_a_row == 1 {
            self.budget.take(now).map(|end| {
                self.told_at = Some(now);
                let set_up = if retry.set_up_again { "set up again and " } else { "" };
                format!(
                    "the window's surface came back {}: {set_up}drawn again in {} ms{end}",
                    miss.word(),
                    ms(retry.after)
                )
            })
        } else if self.told_at.is_some_and(|at| now.saturating_duration_since(at) >= STILL_TOLD_EVERY) {
            self.told_at = Some(now);
            Some(format!(
                "the window's surface has given no picture for {} ms ({}), drawn again each {} ms",
                ms(now.saturating_duration_since(since)),
                self.counts_said(),
                ms(retry.after)
            ))
        } else {
            None
        };
        (retry, line)
    }

    /// The surface gave a picture at `now`: a line for the log if it had given none before (told).
    pub fn shown(&mut self, now: Instant) -> Option<String> {
        if self.in_a_row == 0 {
            return None;
        }
        let line = self.told_at.is_some().then(|| {
            let lasted = self.since.map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
            format!(
                "the window's surface shows again after {} frames missed over {} ms ({})",
                self.in_a_row,
                ms(lasted),
                self.counts_said()
            )
        });
        let budget = std::mem::replace(&mut self.budget, LogBudget::new(0, Duration::ZERO));
        *self = SurfaceMisses { budget, ..SurfaceMisses::default() };
        line
    }

    /// The misses in a row so far.
    #[cfg(test)]
    fn in_a_row(&self) -> u32 {
        self.in_a_row
    }

    fn counts_said(&self) -> String {
        let said: Vec<String> = Miss::ALL
            .iter()
            .zip(self.counts)
            .filter(|(_, count)| *count > 0)
            .map(|(miss, count)| format!("{count} {}", miss.word()))
            .collect();
        said.join(", ")
    }
}

/// How many of the window's hidings and showings each [`OCCLUSION_LOG_SPAN`] the log tells of.
pub const OCCLUSION_LOG_MOST: u32 = 20;
pub const OCCLUSION_LOG_SPAN: Duration = Duration::from_secs(60);

/// What the window last was, for the log: full screen or not, counted as hidden by the system or
/// not.
pub struct WindowWatch {
    full_screen: bool,
    occluded: bool,
    budget: LogBudget,
}

impl Default for WindowWatch {
    fn default() -> WindowWatch {
        WindowWatch {
            full_screen: false,
            occluded: false,
            budget: LogBudget::new(OCCLUSION_LOG_MOST, OCCLUSION_LOG_SPAN),
        }
    }
}

impl WindowWatch {
    /// The window is `full_screen` or not, `size` pixels: a line for the log if that changed. (The
    /// window starts as a window.)
    pub fn full_screen(&mut self, full_screen: bool, size: (u32, u32)) -> Option<String> {
        if full_screen == self.full_screen {
            return None;
        }
        self.full_screen = full_screen;
        let (width, height) = size;
        Some(if full_screen {
            format!("the window went full screen ({width}×{height})")
        } else {
            format!("the window left full screen ({width}×{height})")
        })
    }

    /// The system counts the window as `occluded` (hidden) or as shown again, at `now`: whether a
    /// frame is to be drawn now (it shows again: the frames missed while it was hidden are made up),
    /// and a line for the log if that changed.
    pub fn occluded(&mut self, occluded: bool, now: Instant) -> (bool, Option<String>) {
        let changed = occluded != self.occluded;
        self.occluded = occluded;
        let line = if changed { self.budget.take(now) } else { None };
        let line = line.map(|end| {
            let what = if occluded { "the system counts the window as hidden" } else { "the window shows again" };
            format!("{what}{end}")
        });
        (!occluded, line)
    }
}

fn ms(d: Duration) -> u128 {
    d.as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every way the surface can give no picture is drawn again: a frame that returned nothing to
    /// wait for (as an occluded, timed-out or invalid surface did) left the window as it was until
    /// something else asked for one, black on a Mac whose window had just shown.
    #[test]
    fn every_surface_miss_is_drawn_again_and_none_waits_for_something_else() {
        for miss in Miss::ALL {
            let most = if matches!(miss, Miss::Timeout | Miss::Validation) { SLOW_RETRY_MOST } else { RETRY_MOST };
            for n in [1, 2, 3, 4, 5, 10, 100, 1000, u32::MAX] {
                let retry = after_miss(miss, n);
                assert!(retry.after <= most, "{miss:?} #{n}: {:?}", retry.after);
            }
        }
        // an occluded or timed-out surface isn't set up again (nothing changed); the others are
        assert!(!after_miss(Miss::Occluded, 1).set_up_again);
        assert!(!after_miss(Miss::Timeout, 1).set_up_again);
        assert!(after_miss(Miss::Outdated, 1).set_up_again);
        assert!(after_miss(Miss::Lost, 1).set_up_again);
        assert!(after_miss(Miss::Validation, 1).set_up_again);
    }

    /// The waits: an outdated or lost surface is drawn again at once (as before) the first three
    /// times in a row, then waits from 100 ms; an occluded one waits 100 ms, doubling up to 1 s; a
    /// timed-out or wrongly asked one 1 s, doubling up to 8 s.
    #[test]
    fn the_waits_double_from_100_ms_to_1_s_and_after_a_timeout_from_1_s_to_8_s() {
        let waits = |miss| (1..=7).map(|n| after_miss(miss, n).after.as_millis()).collect::<Vec<_>>();
        assert_eq!(waits(Miss::Occluded), [100, 200, 400, 800, 1000, 1000, 1000]);
        assert_eq!(waits(Miss::Outdated), [0, 0, 0, 100, 200, 400, 800]);
        assert_eq!(waits(Miss::Lost), [0, 0, 0, 100, 200, 400, 800]);
        assert_eq!(waits(Miss::Timeout), [1000, 2000, 4000, 8000, 8000, 8000, 8000]);
        assert_eq!(waits(Miss::Validation), [1000, 2000, 4000, 8000, 8000, 8000, 8000]);
        assert_eq!(after_miss(Miss::Occluded, u32::MAX).after, RETRY_MOST);
        assert_eq!(after_miss(Miss::Timeout, u32::MAX).after, SLOW_RETRY_MOST);
        // a surface that times out for a minute (each holding the window's thread up to 1 s) is
        // asked at most 11 times: at most 11 s of the minute held
        let (mut at, mut asked) = (Duration::ZERO, 0);
        while at < Duration::from_secs(60) {
            asked += 1;
            at += Duration::from_secs(1) + after_miss(Miss::Timeout, asked).after;
        }
        assert!(asked <= 11, "{asked}");
    }

    /// The statuses wgpu gives map to their misses.
    #[test]
    fn each_status_without_a_picture_is_its_miss() {
        use wgpu::CurrentSurfaceTexture as S;
        assert_eq!(picture(S::Occluded).err(), Some(Miss::Occluded));
        assert_eq!(picture(S::Timeout).err(), Some(Miss::Timeout));
        assert_eq!(picture(S::Outdated).err(), Some(Miss::Outdated));
        assert_eq!(picture(S::Lost).err(), Some(Miss::Lost));
        assert_eq!(picture(S::Validation).err(), Some(Miss::Validation));
    }

    /// A window the system keeps counting as hidden for a while after it shows, with no word that
    /// it shows (the Mac's lag): frames are tried again by the waits alone, and the window is drawn
    /// at most 1 s after the surface would give a picture. A long hiding costs little: over a
    /// minute hidden, at most 66 frames are tried (about one a second), not one each refresh.
    #[test]
    fn a_window_counted_as_hidden_is_drawn_within_1_s_of_showing_and_a_long_hiding_tries_few_frames() {
        // the frames tried while the surface gives none until `shows`, from 0; the first drawn
        let run = |shows: Duration| {
            let start = Instant::now();
            let mut misses = SurfaceMisses::default();
            let (mut at, mut tried) = (Duration::ZERO, 0u32);
            loop {
                tried += 1;
                if at >= shows {
                    assert!(misses.shown(start + at).is_some(), "the end of the run is told");
                    return (at, tried);
                }
                let (retry, _) = misses.missed(Miss::Occluded, start + at);
                at += retry.after;
            }
        };
        for shows_ms in [1, 50, 99, 100, 250, 700, 1500, 3000, 10_000, 60_000] {
            let shows = Duration::from_millis(shows_ms);
            let (drawn, tried) = run(shows);
            let late = drawn - shows;
            assert!(late <= RETRY_MOST, "shown at {shows_ms} ms, drawn {} ms later", late.as_millis());
            assert!(tried <= 66, "{tried} frames tried over {shows_ms} ms");
        }
        // shown within the first 100 ms: drawn by 100 ms
        assert_eq!(run(Duration::from_millis(80)).0, Duration::from_millis(100));
        let (_, tried) = run(Duration::from_secs(60));
        println!("frames tried over a minute hidden: {tried}");
    }

    /// The log tells a run of misses as it starts, every 30 s while it goes on, and as it ends;
    /// not each frame.
    #[test]
    fn a_long_run_of_misses_is_told_at_its_start_every_30_s_and_at_its_end() {
        let start = Instant::now();
        let mut misses = SurfaceMisses::default();
        let mut lines = Vec::new();
        let mut at = Duration::ZERO;
        while at < Duration::from_secs(95) {
            let (retry, line) = misses.missed(Miss::Occluded, start + at);
            lines.extend(line);
            at += retry.after;
        }
        lines.extend(misses.shown(start + at));
        assert_eq!(lines.len(), 5, "{lines:#?}");
        assert!(lines[0].starts_with("the window's surface came back occluded: drawn again in 100 ms"), "{}", lines[0]);
        assert!(lines[1].starts_with("the window's surface has given no picture for 30"), "{}", lines[1]);
        assert!(lines[4].starts_with("the window's surface shows again after "), "{}", lines[4]);
        assert!(lines[4].contains(" occluded)"), "{}", lines[4]);
        // a picture with no misses before says nothing, and the next miss starts a new run
        assert_eq!(misses.shown(start + at), None);
        assert_eq!(misses.in_a_row(), 0);
        let (retry, line) = misses.missed(Miss::Outdated, start + at);
        assert_eq!(retry, Retry { set_up_again: true, after: Duration::ZERO });
        assert!(line.is_some_and(|l| l.contains("outdated: set up again and drawn again in 0 ms")));
    }

    /// Runs of misses that keep coming (a surface outdated at each step of a resize) are told at
    /// most 10 a minute, with how many were left out, and their ends only when their starts were.
    #[test]
    fn runs_of_misses_are_told_at_most_10_a_minute() {
        let start = Instant::now();
        let mut misses = SurfaceMisses::default();
        let mut lines = Vec::new();
        for i in 0..100u64 {
            let at = start + Duration::from_millis(i * 100);
            lines.extend(misses.missed(Miss::Outdated, at).1);
            lines.extend(misses.shown(at + Duration::from_millis(5)));
        }
        // 10 starts and their 10 ends in the first 10 s (all in one minute)
        assert_eq!(lines.len(), 20, "{lines:#?}");
        assert!(lines[18].ends_with("(no more like it for 60 s)"), "{}", lines[18]);
        // a minute on, told again, with the 90 left out
        let later = start + Duration::from_secs(61);
        let line = misses.missed(Miss::Outdated, later).1.unwrap_or_default();
        assert!(line.ends_with("(90 more not logged before)"), "{line}");
    }

    /// The system's word that the window shows again draws a frame at once (made up for those
    /// missed while it was hidden); a hiding draws none. Each change is logged once, at most 20 a
    /// minute.
    #[test]
    fn the_window_showing_again_draws_at_once_and_hidings_are_logged_at_most_20_a_minute() {
        let start = Instant::now();
        let mut watch = WindowWatch::default();
        let (draw, line) = watch.occluded(true, start);
        assert!(!draw);
        assert_eq!(line.as_deref(), Some("the system counts the window as hidden"));
        let (draw, line) = watch.occluded(false, start);
        assert!(draw);
        assert_eq!(line.as_deref(), Some("the window shows again"));
        // told again that it shows: drawn again, nothing logged
        assert_eq!(watch.occluded(false, start), (true, None));
        // flapping: 20 lines in the minute, then none until the next
        let mut logged = 2;
        for i in 0..100u64 {
            let at = start + Duration::from_millis(i * 10);
            logged += usize::from(watch.occluded(i % 2 == 0, at).1.is_some());
        }
        assert_eq!(logged, 20);
        assert!(watch.occluded(true, start + Duration::from_secs(61)).1.is_some());
    }

    /// Full screen entered and left are logged once each, with the window's size.
    #[test]
    fn full_screen_entered_and_left_are_logged_once_each() {
        let mut watch = WindowWatch::default();
        assert_eq!(watch.full_screen(false, (1440, 900)), None);
        assert_eq!(watch.full_screen(true, (2560, 1440)).as_deref(), Some("the window went full screen (2560×1440)"));
        assert_eq!(watch.full_screen(true, (2560, 1440)), None);
        assert_eq!(watch.full_screen(false, (1440, 900)).as_deref(), Some("the window left full screen (1440×900)"));
    }
}
