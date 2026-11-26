//! Frames timed to the screen's refreshes, where the system tells them (Android 13 and later): one
//! frame for each time the compositor takes a new one (its latch), begun just in time to be handed
//! over before that latch.
//!
//! Drawn as fast as input came, frames queued up for the compositor, which takes one a refresh: on
//! a 120 Hz tablet a canvas frame reached the screen 32 ms after it was handed over, three refreshes
//! of it spent behind the frames before it. Drawn (by the GPU too, which the compositor waits for)
//! just before a latch with none waiting, a frame is taken at once and shows a refresh later.
// (the desktop draws from its own loop: only the tablet apps' render thread and the tests use it)
#![cfg_attr(not(any(target_os = "android", target_os = "ios", test)), allow(dead_code))]

use std::time::{Duration, Instant};

/// Handed over this long before the latch it is meant for at least, besides what drawing it takes:
/// for the compositor waking a little early, and this thread a little late.
const MARGIN: Duration = Duration::from_micros(1500);

/// What drawing a frame is taken to take before any has been timed.
const FIRST_WORK: Duration = Duration::from_millis(4);

/// What a frame took counts toward the next one's timing by this share: a frame that took longer
/// counts at once, one that took less brings it down slowly (a sixteenth a frame), so that one quick
/// frame after slow ones doesn't make the next late.
const SETTLE: u32 = 16;

/// A timeline older than this many refreshes is no longer followed (the screen's rate may have
/// changed, as it does when nothing moves): the frame is drawn at once.
const STALE_REFRESHES: u32 = 3;

/// A frame drawn without a fresh timeline (the first after a pause), the next waits this long at
/// most for the refreshes to be told again, looking every [`AWAIT_STEP`]: drawn untimed too, two
/// frames could be handed over for one latch, and one would wait behind the other for the rest of
/// the stroke (frames timed one a latch never catch up).
const AWAIT_MOST: Duration = Duration::from_millis(50);
const AWAIT_STEP: Duration = Duration::from_millis(2);

/// Frames go back to being drawn in a refresh only once they fit in it with this share of it to
/// spare too: each going back and forth leaves a refresh without a new frame.
const FIT_AGAIN_SPARE: u32 = 8;

/// Frames that miss their latch this often (of the last [`MISSES_IN`]) leave the screen without
/// a new one that often: the frames go back to being drawn as they come, queued for the compositor
/// (later to show, but a frame ready at every refresh), until they rest (see [`Pacer::rested`]).
const MOST_MISSES: u32 = 2;
const MISSES_IN: u32 = 60;

/// Refreshes outside these aren't believed.
const SHORTEST_REFRESH: Duration = Duration::from_micros(2500);
const LONGEST_REFRESH: Duration = Duration::from_millis(100);

/// The screen's refreshes as last told: when the compositor takes a new frame (one of its latches,
/// the rest a whole number of refreshes from it), the time between them, and when this was told.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Timeline {
    pub(crate) latch: Instant,
    pub(crate) period: Duration,
    pub(crate) told: Instant,
}

impl Timeline {
    /// Whether it still holds at `now`: told within the last few refreshes, at a believable rate.
    pub(crate) fn fresh(&self, now: Instant) -> bool {
        (SHORTEST_REFRESH..=LONGEST_REFRESH).contains(&self.period)
            && now.saturating_duration_since(self.told) <= self.period * STALE_REFRESHES
    }

    /// The first latch at `at` or after it.
    pub(crate) fn latch_from(&self, at: Instant) -> Instant {
        let period = self.period.as_nanos().max(1);
        if at >= self.latch {
            let after = (at - self.latch).as_nanos().div_ceil(period) * period;
            self.latch.checked_add(nanos(after)).unwrap_or(at)
        } else {
            let before = (self.latch - at).as_nanos() / period * period;
            self.latch.checked_sub(nanos(before)).unwrap_or(at)
        }
    }
}

fn nanos(n: u128) -> Duration {
    Duration::from_nanos(u64::try_from(n).unwrap_or(u64::MAX))
}

/// The next frame planned: when to begin it, the latch it is to be handed over by, and the latch
/// that is to take it: the same, or (its GPU's work not done by then, as a slow frame's isn't)
/// the one after.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Plan {
    pub(crate) begin: Instant,
    pub(crate) latch: Instant,
    pub(crate) taken: Instant,
}

/// Times frames to a [`Timeline`], one for each latch. The compositor takes a frame once the GPU
/// has drawn it: a frame whose drawing (to the GPU's end of it) fits in a refresh is begun that and
/// a margin before a latch, and taken at it; one that doesn't is handed over before a latch, the
/// GPU still drawing it, and taken at the latch after (the next begun meanwhile).
#[derive(Debug)]
pub(crate) struct Pacer {
    /// What a frame takes from its beginning to being handed over, and to the GPU's end of it: the
    /// longest lately, coming down slowly.
    work: Duration,
    drawn: Duration,
    /// The last frame handed over: the latch it is taken at, as far as is known.
    last: Option<Last>,
    /// The last frame handed over while the GPU's end of it isn't known yet.
    pending: Option<Pending>,
    /// The timeline was waited for after a frame drawn without it and didn't come: frames aren't
    /// held up for it again until it does.
    gave_up: bool,
    /// Whether frames are drawn in a refresh (see [`FIT_AGAIN_SPARE`]).
    fitting: bool,
    /// Which of the last frames missed the latch planned to take them (a bit each, the newest
    /// lowest), and whether too many did (see [`MOST_MISSES`]).
    misses: u64,
    strained: bool,
}

/// The last frame handed over, as the pacer knows it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Last {
    /// Timed: the latch it is taken at.
    Latch(Instant),
    /// Drawn without a fresh timeline: when it was handed over (the first latch after it takes it).
    Sent(Instant),
}

/// A frame handed over, the GPU maybe still drawing it.
#[derive(Clone, Copy, Debug)]
struct Pending {
    timeline: Timeline,
    plan: Plan,
    began: Instant,
    sent: Instant,
}

impl Default for Pacer {
    fn default() -> Pacer {
        Pacer {
            work: FIRST_WORK,
            drawn: FIRST_WORK,
            last: None,
            pending: None,
            gave_up: false,
            fitting: true,
            misses: 0,
            strained: false,
        }
    }
}

/// `sample` counted into `estimate`: at once if longer, a [`SETTLE`]th of the way if shorter.
fn settle(estimate: Duration, sample: Duration) -> Duration {
    if sample >= estimate { sample } else { estimate - (estimate - sample) / SETTLE }
}

impl Pacer {
    /// Whether a frame, the GPU's drawing of it too, fits in a refresh with the margin.
    pub(crate) fn fits(&self, timeline: &Timeline) -> bool {
        self.fitting && self.drawn + MARGIN <= timeline.period
    }

    /// What the GPU's end of a frame took, at least, counted in; drawn in a refresh again only with
    /// some of it to spare.
    fn drawn_took(&mut self, took: Duration, period: Duration) {
        self.drawn = settle(self.drawn, took);
        let spare = if self.fitting { Duration::ZERO } else { period / FIT_AGAIN_SPARE };
        self.fitting = self.drawn + MARGIN + spare <= period;
    }

    /// How long before the latch it is handed over by a frame is begun: what drawing one takes
    /// (to the GPU's end of it, where that fits in a refresh) and the margin, a refresh and the
    /// margin at most.
    pub(crate) fn lead(&self, timeline: &Timeline) -> Duration {
        let work = if self.fits(timeline) { self.drawn } else { self.work };
        work.min(timeline.period) + MARGIN
    }

    /// The next frame, wanted at `now`: taken at a later latch than the last frame (one a latch),
    /// handed over by the first latch it can be drawn by; begun no sooner than the latch before
    /// the one to take it (handed over sooner, it would only wait behind the last frame, its input
    /// older).
    pub(crate) fn plan(&self, timeline: &Timeline, now: Instant) -> Plan {
        let lead = self.lead(timeline);
        // taken this long after the latch it is handed over by
        let after = if self.fits(timeline) { Duration::ZERO } else { timeline.period };
        let mut earliest = now + lead;
        let last = self.last_taken(timeline);
        if let Some(last) = last {
            earliest = earliest.max((last + timeline.period / 2).checked_sub(after).unwrap_or(earliest));
        }
        let latch = timeline.latch_from(earliest);
        let mut begin = latch.checked_sub(lead).unwrap_or(now).max(now);
        if let Some(last) = last.and_then(|l| l.checked_sub(after)) {
            begin = begin.max(last);
        }
        Plan { begin, latch, taken: latch + after }
    }

    /// The latch the last frame is taken at, as far as is known (the one planned, while the GPU's
    /// end of it isn't known).
    fn last_taken(&self, timeline: &Timeline) -> Option<Instant> {
        match self.last {
            Some(Last::Latch(latch)) => Some(latch),
            Some(Last::Sent(sent)) => Some(timeline.latch_from(sent)),
            None => None,
        }
    }

    /// The frame planned as `plan`, begun at `began`, was handed over at `sent`, the GPU maybe
    /// still drawing it: taken, as far as is known, where planned. See [`Pacer::drawn`].
    pub(crate) fn handed_over(&mut self, timeline: &Timeline, plan: Plan, began: Instant, sent: Instant) {
        let took = sent.saturating_duration_since(began).min(timeline.period * 2);
        self.work = settle(self.work, took);
        self.last = Some(Last::Latch(plan.taken));
        self.pending = Some(Pending { timeline: *timeline, plan, began, sent });
        self.gave_up = false;
    }

    /// The frame planned as `plan` was handed over at `sent`, and drawn by then (the GPU's work
    /// too): [`Pacer::handed_over`] and [`Pacer::drawn`] at once.
    #[cfg(test)]
    pub(crate) fn sent(&mut self, timeline: &Timeline, plan: Plan, began: Instant, sent: Instant) {
        self.handed_over(timeline, plan, began, sent);
        self.drawn(sent);
    }

    /// Until when the GPU's end of the last frame handed over is to be looked for: the latch that
    /// is to take it. None if it is known.
    pub(crate) fn drawn_unknown_until(&self) -> Option<Instant> {
        self.pending.map(|p| p.plan.taken)
    }

    /// The GPU had drawn the last frame handed over by `at`: what it took is timed, and the latch
    /// that takes it known (the next it can be, if it was late).
    pub(crate) fn drawn(&mut self, at: Instant) {
        let Some(p) = self.pending.take() else { return };
        let took = at.saturating_duration_since(p.began).min(p.timeline.period * 2);
        self.drawn_took(took, p.timeline.period);
        let ready = at.max(p.sent);
        let taken = if ready <= p.plan.taken { p.plan.taken } else { p.timeline.latch_from(ready) };
        self.last = Some(Last::Latch(taken));
        self.missed(taken > p.plan.taken);
    }

    /// Whether the last frame resolved missed its latch, counted in.
    fn missed(&mut self, missed: bool) {
        self.misses = ((self.misses << 1) | u64::from(missed)) & ((1 << MISSES_IN) - 1);
        if self.misses.count_ones() >= MOST_MISSES {
            self.strained = true;
        }
    }

    /// Whether frames miss their latches too often to be timed to them, until they rest.
    pub(crate) fn strained(&self) -> bool {
        self.strained
    }

    /// The frames have rested (none drawn a while): timed to the latches again.
    pub(crate) fn rested(&mut self) {
        self.strained = false;
        self.misses = 0;
    }

    /// The GPU hadn't drawn the last frame handed over by `at`, when it was last looked at: at
    /// the latch that was to take it (taken at a later one), or as the next frame begins (taken
    /// where planned, as far as is known). What it took is at least that.
    pub(crate) fn not_drawn_by(&mut self, at: Instant) {
        let Some(p) = self.pending.take() else { return };
        let took = at.saturating_duration_since(p.began).min(p.timeline.period * 2);
        self.drawn_took(took, p.timeline.period);
        if at >= p.plan.taken {
            self.last = Some(Last::Latch(p.plan.taken + p.timeline.period));
        }
        self.missed(at >= p.plan.taken);
    }

    /// A frame drawn without a fresh timeline was handed over at `sent`: the next one planned is
    /// meant for a later latch than the one that takes it.
    pub(crate) fn sent_untimed(&mut self, sent: Instant) {
        self.pending = None;
        self.last = Some(Last::Sent(sent));
    }

    /// With no fresh timeline at `now`: until when to wait for one before the next frame, if the
    /// last was drawn without one lately (see [`AWAIT_MOST`]), and it hasn't been waited for in vain
    /// since it was last fresh.
    pub(crate) fn awaiting(&mut self, now: Instant) -> Option<Instant> {
        match self.last {
            Some(Last::Sent(sent)) if !self.gave_up => {
                let until = sent + AWAIT_MOST;
                self.gave_up = now >= until;
                (now < until).then(|| (now + AWAIT_STEP).min(until))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);
    const US: Duration = Duration::from_micros(1);

    fn ms(v: f64) -> Duration {
        Duration::from_secs_f64(v / 1000.0)
    }

    /// A 120 Hz screen whose compositor latches 10.33 ms before each refresh shows (as measured on
    /// the tablet), told at `t0`.
    fn screen(t0: Instant) -> Timeline {
        Timeline { latch: t0 + ms(5.0), period: ms(1000.0 / 120.0), told: t0 }
    }

    #[test]
    fn latches_come_a_whole_number_of_refreshes_from_the_one_told() {
        let t0 = Instant::now() + Duration::from_secs(1);
        let tl = screen(t0);
        assert_eq!(tl.latch_from(t0), t0 + ms(5.0));
        assert_eq!(tl.latch_from(t0 + ms(5.0)), t0 + ms(5.0));
        assert_eq!(tl.latch_from(t0 + ms(5.001)), t0 + ms(5.0) + tl.period);
        // before the one told too
        assert_eq!(tl.latch_from(t0 - ms(10.0)), t0 + ms(5.0) - tl.period);
        assert_eq!(tl.latch_from(t0 - ms(12.0)), t0 + ms(5.0) - tl.period * 2);
        assert_eq!(tl.latch_from(t0 + ms(5.0) - tl.period), t0 + ms(5.0) - tl.period);
        // a timeline goes stale after three refreshes, and a rate past belief never holds
        assert!(tl.fresh(t0 + ms(24.0)));
        assert!(!tl.fresh(t0 + ms(26.0)));
        assert!(!Timeline { period: Duration::ZERO, ..tl }.fresh(t0));
        assert!(!Timeline { period: Duration::from_secs(1), ..tl }.fresh(t0));
    }

    #[test]
    fn a_frame_is_begun_its_work_and_a_margin_before_its_latch_and_one_a_latch() {
        let t0 = Instant::now() + Duration::from_secs(1);
        let tl = screen(t0);
        let mut pacer = Pacer::default();
        // wanted at once: for the first latch 4 + 1.5 ms ahead
        let plan = pacer.plan(&tl, t0);
        assert_eq!(plan.latch, t0 + ms(5.0) + tl.period);
        assert_eq!(plan.begin, plan.latch - US * 5500);
        // drawn in 2 ms: the lead comes down slowly, never under that
        pacer.sent(&tl, plan, plan.begin, plan.begin + 2 * MS);
        assert!(pacer.lead(&tl) < US * 5500 && pacer.lead(&tl) > US * 3500);
        // wanted again at once: not for the same latch, the next
        let next = pacer.plan(&tl, plan.begin + 2 * MS);
        assert_eq!(next.latch, plan.latch + tl.period);
        for _ in 0..200 {
            let p = pacer.plan(&tl, t0);
            pacer.sent(&tl, p, p.begin, p.begin + 2 * MS);
        }
        assert!((pacer.lead(&tl) - US * 3500) < US * 100, "{:?}", pacer.lead(&tl));
        // one long frame: at once
        let p = pacer.plan(&tl, t0 + Duration::from_secs(1));
        pacer.sent(&tl, p, p.begin, p.begin + 7 * MS);
        assert_eq!(pacer.lead(&tl), US * 8500);
        // longer than a refresh: begun a refresh and the margin ahead at most
        let p = pacer.plan(&tl, t0 + Duration::from_secs(2));
        pacer.sent(&tl, p, p.begin, p.begin + 30 * MS);
        assert_eq!(pacer.lead(&tl), tl.period + US * 1500);
        // and so not drawn in a refresh: the next handed over by the latch after the last one's,
        // its GPU's work left to go on, and taken at the latch after that
        assert!(!pacer.fits(&tl));
        let last = tl.latch_from(p.begin + 30 * MS);
        let next = pacer.plan(&tl, last - 4 * MS);
        assert_eq!((next.latch, next.taken, next.begin), (last + tl.period, last + tl.period * 2, last - US * 1500));
    }

    #[test]
    fn a_frame_handed_over_late_takes_the_next_latch_and_the_one_after_waits_for_the_one_after_that() {
        let t0 = Instant::now() + Duration::from_secs(1);
        let tl = screen(t0);
        let mut pacer = Pacer::default();
        let plan = pacer.plan(&tl, t0);
        // late by a millisecond
        pacer.sent(&tl, plan, plan.begin, plan.latch + MS);
        assert_eq!(pacer.last, Some(Last::Latch(plan.latch + tl.period)));
        let next = pacer.plan(&tl, plan.latch + MS);
        assert_eq!(next.latch, plan.latch + tl.period * 2);
    }

    /// A frame drawn before the refreshes are told again (the first after a pause) takes the first
    /// latch after it is handed over: the next is meant for the one after, and waits a little for
    /// the timeline rather than be drawn untimed too.
    #[test]
    fn after_a_frame_drawn_untimed_the_next_waits_for_the_timeline_and_for_a_later_latch() {
        let t0 = Instant::now() + Duration::from_secs(1);
        let tl = screen(t0);
        let mut pacer = Pacer::default();
        assert_eq!(pacer.awaiting(t0), None);
        // handed over 7 ms before a latch
        let sent = t0 - 2 * MS;
        pacer.sent_untimed(sent);
        assert_eq!(pacer.awaiting(sent), Some(sent + 2 * MS));
        assert_eq!(pacer.awaiting(sent + 49 * MS), Some(sent + 50 * MS));
        assert_eq!(pacer.awaiting(sent + 50 * MS), None);
        // told: not for that latch, though there would be time to draw for it
        let plan = pacer.plan(&tl, sent + US * 500);
        assert!(t0 + ms(5.0) - (sent + US * 500) > pacer.lead(&tl));
        assert_eq!(plan.latch, t0 + ms(5.0) + tl.period);
        // waited for in vain, not again for frames drawn without it after
        let mut vain = Pacer::default();
        vain.sent_untimed(sent);
        assert_eq!(vain.awaiting(sent + 50 * MS), None);
        vain.sent_untimed(sent + 60 * MS);
        assert_eq!(vain.awaiting(sent + 60 * MS), None);
        // timed, no more waiting; and after the next drawn without it, waited for again
        pacer.sent(&tl, plan, plan.begin, plan.begin + 2 * MS);
        assert_eq!(pacer.awaiting(plan.begin + 2 * MS), None);
        pacer.sent_untimed(plan.begin + 100 * MS);
        assert!(pacer.awaiting(plan.begin + 101 * MS).is_some());
    }

    /// The compositor as measured on the tablet (120 Hz): it latches 10.33 ms before each refresh
    /// shows, taking the oldest frame waiting (one a latch), and holds at most three waiting; a
    /// frame handed over meanwhile waits in the app's buffer queue (a newer one in place of an older)
    /// until a latch frees a place, 2 ms after it. Frames are drawn in `work`, begun when `next`
    /// says (given when the last was begun and handed over); what each waited from reaching the
    /// compositor to showing.
    fn shown_after(mut next: impl FnMut(Instant, Option<(Instant, Instant)>) -> Instant, work: Duration) -> Vec<f64> {
        let t0 = Instant::now() + Duration::from_secs(1);
        let period = ms(1000.0 / 120.0);
        let latch = |k: u32| t0 + period * k;
        let mut waiting: std::collections::VecDeque<Instant> = Default::default();
        let mut held = false;
        let mut shown = Vec::new();
        let (mut now, mut last, mut k) = (t0, None, 1);
        while k < 1200 {
            let begin = next(now, last);
            let sent = begin + work;
            // the latches before it is handed over
            while latch(k) < sent {
                if let Some(f) = waiting.pop_front() {
                    shown.push((latch(k) + ms(10.33) - f).as_secs_f64() * 1000.0);
                    if std::mem::take(&mut held) {
                        waiting.push_back(latch(k) + ms(2.0));
                    }
                }
                k += 1;
            }
            if waiting.len() < 3 {
                waiting.push_back(sent);
            } else {
                held = true;
            }
            last = Some((begin, sent));
            now = sent;
        }
        shown
    }

    fn median(mut v: Vec<f64>) -> f64 {
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
    }

    /// Drawn as fast as frames may be (a tenth under the fastest refresh, 165 Hz on the tablet, as
    /// before), frames wait three refreshes for the compositor and show 33 ms after reaching it,
    /// as measured on the tablet (32); timed to the latches, they show 12 ms after, never later
    /// than 17 (two refreshes), and one shows each refresh still.
    #[test]
    fn frames_timed_to_the_latches_show_a_refresh_after_being_handed_over_not_four() {
        let work = ms(2.0);
        let spacing = ms(900.0 / 165.0);
        let before =
            shown_after(|now, last| last.map_or(now, |(b, _): (Instant, Instant)| (b + spacing).max(now)), work);
        let t0 = Instant::now() + Duration::from_secs(1);
        let tl = Timeline { latch: t0, period: ms(1000.0 / 120.0), told: t0 };
        let mut pacer = Pacer::default();
        let mut planned: Option<Plan> = None;
        let after = shown_after(
            |now, last| {
                if let (Some(p), Some((b, s))) = (planned, last) {
                    pacer.sent(&tl, p, b, s);
                }
                let p = pacer.plan(&tl, now);
                planned = Some(p);
                p.begin
            },
            work,
        );
        let (b, a) = (median(before.clone()), median(after.clone()));
        let worst = after.iter().copied().fold(0.0, f64::max);
        eprintln!(
            "reaching the compositor to shown, median: drawn as fast as allowed {b:.1} ms, timed to the latches {a:.1} ms (worst {worst:.1}); {} and {} frames shown",
            before.len(),
            after.len()
        );
        assert!(b > 30.0, "the queue as measured: {b}");
        assert!(a <= 17.0, "a refresh after being handed over: {a}");
        assert!(worst <= 17.0, "never two refreshes: {worst}");
        assert!(after.len() + 10 >= before.len(), "{} against {}", after.len(), before.len());
    }

    /// Frames whose GPU work goes on after they are handed over (`cpu` then `gpu`), the compositor
    /// taking each, one a latch, once it is drawn (120 Hz, latching 10.33 ms before the refresh
    /// shows), the pacer told as the frame loop tells it: what each frame waited from its beginning
    /// to showing, and how many showed.
    fn timed_with_gpu(cpu: f64, gpu: f64) -> (Vec<f64>, usize) {
        let t0 = Instant::now() + Duration::from_secs(1);
        let tl = Timeline { latch: t0, period: ms(1000.0 / 120.0), told: t0 };
        let mut pacer = Pacer::default();
        let mut taken_last: Option<Instant> = None;
        let mut shown = Vec::new();
        let mut now = t0;
        for _ in 0..600 {
            let plan = pacer.plan(&tl, now);
            let sent = plan.begin + ms(cpu);
            let drawn = sent + ms(gpu);
            pacer.handed_over(&tl, plan, plan.begin, sent);
            // looked at until the next begins (or the latch to take it): known if drawn by then
            let next_begins = pacer.plan(&tl, sent).begin;
            let looked_until = next_begins.min(plan.taken);
            if drawn <= looked_until {
                pacer.drawn(drawn);
            } else {
                pacer.not_drawn_by(looked_until.max(sent));
            }
            // the compositor: the first latch it is drawn by, after the last frame's
            let mut taken = tl.latch_from(drawn);
            if let Some(last) = taken_last
                && taken <= last
            {
                taken = last + tl.period;
            }
            taken_last = Some(taken);
            shown.push((taken + ms(10.33) - plan.begin).as_secs_f64() * 1000.0);
            now = sent;
        }
        let span = taken_last.map_or(0.0, |l| (l - t0).as_secs_f64());
        let refreshes = (span * 120.0).round() as usize;
        (shown, refreshes)
    }

    /// A pencil's frame (2 ms, then 2.7 on the GPU, as measured on the tablet) fits in a refresh:
    /// begun just in time, it shows 15 ms after it was begun. A wet brush's (3, then 5) doesn't:
    /// handed over a refresh ahead, the GPU drawing it while the next is begun, still one shows
    /// each refresh (waited for, they showed every other refresh), a refresh later.
    #[test]
    fn frames_the_gpu_draws_in_a_refresh_show_a_refresh_sooner_and_slower_ones_still_every_refresh() {
        let (light, light_refreshes) = timed_with_gpu(2.0, 2.7);
        let (heavy, heavy_refreshes) = timed_with_gpu(3.0, 5.0);
        let (l, h) = (median(light.clone()), median(heavy.clone()));
        eprintln!(
            "begun to shown, median: a light frame {l:.1} ms ({} frames over {light_refreshes} refreshes), a heavy one {h:.1} ms ({} over {heavy_refreshes})",
            light.len(),
            heavy.len()
        );
        assert!(l <= 17.5, "a light frame: {l}");
        assert!(h <= 26.0, "a heavy frame: {h}");
        // one shown each refresh, both ways
        assert!(light_refreshes <= light.len() + 10, "{light_refreshes}");
        assert!(heavy_refreshes <= heavy.len() + 10, "{heavy_refreshes}");
    }

    /// Frames whose drawing hovers about a refresh don't go back and forth between being drawn in
    /// one and being handed over a refresh ahead: back only with an eighth of it to spare.
    #[test]
    fn frames_about_a_refresh_long_keep_to_one_way_of_being_drawn() {
        let t0 = Instant::now() + Duration::from_secs(1);
        let tl = Timeline { latch: t0, period: ms(1000.0 / 120.0), told: t0 };
        let mut pacer = Pacer::default();
        let mut now = t0;
        let mut changes = 0;
        let mut was = pacer.fits(&tl);
        for i in 0..400 {
            // 6 ms all told, which fits with the margin, but one in twenty 7, which doesn't
            let total = if i % 20 == 0 { 7.0 } else { 6.0 };
            let plan = pacer.plan(&tl, now);
            let sent = plan.begin + ms(2.0);
            pacer.handed_over(&tl, plan, plan.begin, sent);
            pacer.drawn(plan.begin + ms(total));
            if pacer.fits(&tl) != was {
                changes += 1;
                was = !was;
            }
            now = sent;
        }
        eprintln!("frames of 6 ms, one in twenty 7: the way they are drawn changed {changes} times");
        assert!(changes <= 1, "{changes}");
        // well under it again: drawn in a refresh
        for _ in 0..100 {
            let plan = pacer.plan(&tl, now);
            pacer.sent(&tl, plan, plan.begin, plan.begin + ms(3.0));
            now = plan.begin + ms(3.0);
        }
        assert!(pacer.fits(&tl));
    }

    /// Frames missing their latches now and then (a wet brush's) go untimed after two misses in
    /// sixty frames, and are timed again after a rest; one miss now and then doesn't.
    #[test]
    fn frames_missing_too_many_latches_go_untimed_until_they_rest() {
        let t0 = Instant::now() + Duration::from_secs(1);
        let tl = Timeline { latch: t0, period: ms(1000.0 / 120.0), told: t0 };
        let run = |pacer: &mut Pacer, frames: usize, miss_every: usize| {
            let mut now = t0;
            for i in 0..frames {
                let plan = pacer.plan(&tl, now);
                let sent = plan.begin + ms(2.0);
                pacer.handed_over(&tl, plan, plan.begin, sent);
                // a miss: drawn a millisecond after its latch
                let drawn = if (i + 1) % miss_every == 0 { plan.taken + MS } else { sent + ms(1.0) };
                pacer.drawn(drawn);
                now = drawn.max(sent);
            }
        };
        let mut pacer = Pacer::default();
        run(&mut pacer, 600, 70);
        assert!(!pacer.strained(), "one in seventy");
        run(&mut pacer, 600, 15);
        assert!(pacer.strained(), "one in fifteen");
        pacer.rested();
        assert!(!pacer.strained());
    }

    /// Where the compositor takes the newest frame at each refresh and drops older ones waiting
    /// (Mailbox, as the desktop's windows are on Windows), frames drawn a tenth under the refresh
    /// (the desktop's pace) never wait behind each other: at most a refresh to be taken and one to
    /// show, at 60, 120 and 144 Hz.
    #[test]
    fn where_the_newest_frame_is_shown_frames_a_tenth_under_the_refresh_never_queue() {
        for hz in [60.0, 120.0, 144.0] {
            let (period, spacing) = (ms(1000.0 / hz), ms(900.0 / hz));
            let t0 = Instant::now() + Duration::from_secs(1);
            let (mut newest, mut shown, mut sent, mut k) = (None::<Instant>, Vec::new(), t0, 1);
            for _ in 0..2000 {
                sent += spacing;
                while t0 + period * k < sent {
                    if let Some(f) = newest.take() {
                        shown.push((t0 + period * (k + 1) - f).as_secs_f64() * 1000.0);
                    }
                    k += 1;
                }
                newest = Some(sent);
            }
            let worst = shown.iter().copied().fold(0.0, f64::max);
            eprintln!(
                "{hz} Hz, the newest shown: handed over to shown {:.1} ms median, {worst:.1} at worst",
                median(shown)
            );
            assert!(worst <= 2000.0 / hz + 0.01, "{hz} Hz: {worst}");
        }
    }
}
