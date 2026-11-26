//! A GPU driver that crashes the app once drawing, told from how the last runs ended, and left for
//! the other graphics backend (Android: Vulkan and OpenGL ES).
//!
//! The record's try (see the parent module) catches a crash while a backend is at risk, until a
//! canvas first shows. A driver that crashes later (one whose Vulkan driver crashed in the queue's
//! submit half a minute into every start) crashed every start. So the platform hands over how the
//! last runs ended, with the system's trace of each crash in native code, before the device is
//! made: when the newest run, or two of the last three, crashed in the driver of the backend the
//! record has the app on, this start is on the other.
//!
//! Each driver's crashes are counted on the record's third line, for the system they happened
//! under. Once the other driver has crashed too, whichever crashed less is kept for good, never
//! back and forth. A system update (perhaps a new driver) gives both another chance.

use std::path::Path;

use procraft_gpu::{Gpu, wgpu};

use super::{RECORD, Stage, after_build, built_by, read, record, write_record, writing};

/// How many of the last runs are looked at.
const LOOKED_AT: usize = 3;

/// How one of the app's last runs ended, as the platform keeps it.
#[derive(uniffi::Record, Clone, Debug, Default)]
pub struct EndedRun {
    /// When it ended, milliseconds since 1970.
    pub ended: i64,
    /// Whether it crashed in native code.
    pub crashed_natively: bool,
    /// The system's trace of that crash (Android 12 on: its record in binary form), or at least
    /// its start, where the threads are; none if it kept none.
    pub trace: Option<Vec<u8>>,
}

/// Hands the core how the app's last runs ended (`runs`, newest first: Android keeps them from
/// Android 11 on) under the system `system` (Android's build fingerprint). Call it after
/// [`super::set_gpu_record_folder`] and before the GPU's device is made: a GPU driver that crashed
/// the last runs sends this start to the other graphics backend (see the module's note).
#[uniffi::export]
pub fn gpu_last_runs(runs: Vec<EndedRun>, system: String) {
    super::start_log();
    let Some((dir, build)) = record() else {
        return log::warn!("how the last runs ended came before the GPU record's folder: not looked at");
    };
    update_in(&dir, &build, &ended(&runs), system.trim(), Gpu::backends);
}

/// When each run ended and the driver it crashed in, if it did, newest first.
fn ended(runs: &[EndedRun]) -> Vec<(i64, Option<Driver>)> {
    let mut ended: Vec<(i64, Option<Driver>)> = runs
        .iter()
        .map(|run| {
            let files = run.trace.as_deref().filter(|_| run.crashed_natively).map(crate::app_log::crashed_frame_files);
            (run.ended, files.and_then(|files| driver_of(&files)))
        })
        .collect();
    ended.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    ended
}

/// A graphics API whose driver a crash was in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Driver {
    Vulkan,
    Gl,
}

impl Driver {
    /// The driver a backend draws with, if it is one of the two.
    fn of(backends: wgpu::Backends) -> Option<Driver> {
        if backends == wgpu::Backends::VULKAN {
            Some(Driver::Vulkan)
        } else if backends == wgpu::Backends::GL {
            Some(Driver::Gl)
        } else {
            None
        }
    }

    fn other(self) -> Driver {
        match self {
            Driver::Vulkan => Driver::Gl,
            Driver::Gl => Driver::Vulkan,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Driver::Vulkan => "Vulkan",
            Driver::Gl => "OpenGL ES",
        }
    }

    fn word(self) -> &'static str {
        match self {
            Driver::Vulkan => "vulkan",
            Driver::Gl => "gl",
        }
    }

    fn from_word(word: &str) -> Option<Driver> {
        [Driver::Vulkan, Driver::Gl].into_iter().find(|d| d.word() == word)
    }
}

/// The driver a crashed thread was in, by the files its frames were in (innermost first): the
/// outermost frame in a graphics library, the API the app (or the driver's own thread) called
/// into. So OpenGL ES drawn by ANGLE over the Vulkan driver counts as OpenGL ES's (its frames sit
/// above the Vulkan driver's), and a Mali's Vulkan driver, a file of its OpenGL ES library, counts
/// as Vulkan's (the Vulkan loader's frame sits above it). None: a crash elsewhere.
fn driver_of(files: &[String]) -> Option<Driver> {
    files.iter().filter_map(|file| library(file)).next_back()
}

/// The graphics API the library at `path` is part of, if it is one: Vulkan's loader
/// (libvulkan.so) or a vendor's Vulkan driver (vulkan.<chip>.so, in the system's own folders);
/// OpenGL ES's and EGL's libraries, the system's, the vendors' and ANGLE's.
fn library(path: &str) -> Option<Driver> {
    let path = path.to_ascii_lowercase();
    let name = path.rsplit('/').next().unwrap_or(&path);
    let system = ["/vendor/", "/system/", "/odm/", "/system_ext/", "/product/", "/apex/"]
        .iter()
        .any(|folder| path.starts_with(folder));
    if name.starts_with("libgles") || name.starts_with("libegl") || (system && path.contains("/egl/")) {
        Some(Driver::Gl)
    } else if name == "libvulkan.so" || (system && name.starts_with("vulkan.") && name.ends_with(".so")) {
        Some(Driver::Vulkan)
    } else {
        None
    }
}

/// What the drivers' crashes came to under one system: the record's third line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Crashes {
    vulkan: u32,
    gl: u32,
    /// The backend kept for good once both drivers crashed: whichever crashed less.
    settled: Option<Driver>,
    /// Whether the app was sent off Vulkan for its driver's crashes (to OpenGL ES, or settled
    /// there): OpenGL ES is then kept, whatever fails on it (see `vulkan_crashed`).
    left_vulkan: bool,
    /// When the newest run counted ended (milliseconds since 1970): none before it counts again.
    seen: i64,
    /// The system they crashed under (Android's build fingerprint).
    system: String,
}

impl Crashes {
    const PREFIX: &'static str = "driver crashes:";

    fn count(&self, driver: Driver) -> u32 {
        match driver {
            Driver::Vulkan => self.vulkan,
            Driver::Gl => self.gl,
        }
    }

    fn add(&mut self, driver: Driver) {
        match driver {
            Driver::Vulkan => self.vulkan += 1,
            Driver::Gl => self.gl += 1,
        }
    }

    fn line(&self) -> String {
        format!(
            "{} vulkan {}, gl {}, settled on {}, left vulkan {}, seen {}, system {}",
            Crashes::PREFIX,
            self.vulkan,
            self.gl,
            self.settled.map_or("none", Driver::word),
            if self.left_vulkan { "yes" } else { "no" },
            self.seen,
            self.system
        )
    }

    /// A line written by [`Crashes::line`]; none if it isn't one.
    fn parse(line: &str) -> Option<Crashes> {
        let (fields, system) = line.trim().strip_prefix(Crashes::PREFIX)?.split_once(", system ")?;
        let mut crashes = Crashes { system: system.trim().to_owned(), ..Crashes::default() };
        let mut left = None;
        for field in fields.split(',') {
            let (key, value) = field.trim().rsplit_once(' ')?;
            match key {
                "vulkan" => crashes.vulkan = value.parse().ok()?,
                "gl" => crashes.gl = value.parse().ok()?,
                "settled on" => crashes.settled = Driver::from_word(value),
                "left vulkan" => left = Some(value == "yes"),
                "seen" => crashes.seen = value.parse().ok()?,
                _ => {}
            }
        }
        // (a line from before it was kept: the app was sent off Vulkan by any of its crashes)
        crashes.left_vulkan = left.unwrap_or(crashes.vulkan > 0);
        Some(crashes)
    }
}

/// What the record comes to after the last runs: its start (backend and stage), its crashes, and
/// what is logged (decisions as warnings).
#[derive(Debug, PartialEq)]
struct Decided {
    start: Option<(usize, Stage)>,
    crashes: Crashes,
    said: Vec<(log::Level, String)>,
}

/// The record after `runs` (newest first: when each ended, and the driver it crashed in if it
/// did) under the system `system`, from its start `start` and the crashes it kept; `order` the
/// backends by index (asked only once a driver has crashed). None: nothing to change.
fn decide(
    start: Option<(usize, Stage)>,
    kept: Option<Crashes>,
    runs: &[(i64, Option<Driver>)],
    system: &str,
    order: impl FnOnce() -> Vec<wgpu::Backends>,
) -> Option<Decided> {
    let mut start = start;
    let mut said = Vec::new();
    let mut crashes = match kept {
        Some(kept) if kept.system == system => kept,
        Some(kept) => {
            // another system, and perhaps another driver: both backends get another chance
            if kept.vulkan + kept.gl > 0 {
                said.push((
                    log::Level::Warn,
                    "the system was updated since the GPU's drivers crashed the app: its first backend is tried again"
                        .to_owned(),
                ));
                start = Some((0, Stage::Made));
            }
            Crashes { seen: kept.seen, system: system.to_owned(), ..Crashes::default() }
        }
        None => Crashes { system: system.to_owned(), ..Crashes::default() },
    };
    let looked = runs.get(..runs.len().min(LOOKED_AT)).unwrap_or_default();
    // each crash counted once, in the first start that sees it
    let new: Vec<Driver> = looked.iter().filter(|(at, _)| *at > crashes.seen).filter_map(|(_, d)| *d).collect();
    if new.is_empty() {
        return (!said.is_empty()).then_some(Decided { start, crashes, said });
    }
    for driver in &new {
        crashes.add(*driver);
    }
    crashes.seen = looked.iter().map(|(at, _)| *at).fold(crashes.seen, i64::max);
    said.push((
        log::Level::Info,
        format!(
            "the last runs: {} crashed in the GPU's driver; under this system, {} crashes in Vulkan's, {} in OpenGL ES's",
            new.len(),
            crashes.vulkan,
            crashes.gl
        ),
    ));
    // the backend the record has the app on, and whether the newest run or two of the last three
    // crashed in its driver, one of them new
    let order = order();
    let on_index = start.map_or(0, |(backend, _)| backend);
    let on = order.get(on_index).copied().and_then(Driver::of);
    let other = on.and_then(|on| Some((on.other(), order.iter().position(|b| Driver::of(*b) == Some(on.other()))?)));
    let (Some(on), Some((other, other_index))) = (on, other) else {
        return Some(Decided { start, crashes, said });
    };
    let crashed_in = |run: &(i64, Option<Driver>)| run.1 == Some(on);
    let often = looked.first().is_some_and(crashed_in) || looked.iter().filter(|run| crashed_in(run)).count() >= 2;
    if !(often && new.contains(&on)) {
        return Some(Decided { start, crashes, said });
    }
    let to = match crashes.settled {
        Some(settled) => {
            said.push((
                log::Level::Warn,
                if settled == on {
                    format!("the GPU's {} driver crashed again: staying on it, as settled before", on.name())
                } else {
                    format!("the GPU's {} driver crashed: back to {}, as settled before", on.name(), settled.name())
                },
            ));
            settled
        }
        None if crashes.count(other) == 0 => {
            said.push((
                log::Level::Warn,
                format!("the GPU's {} driver crashed in the last runs: starting on {}", on.name(), other.name()),
            ));
            other
        }
        None => {
            // both have crashed: whichever crashed less, for good (even, the one not crashed last)
            let settled = if crashes.count(on) < crashes.count(other) { on } else { other };
            crashes.settled = Some(settled);
            said.push((
                log::Level::Warn,
                format!(
                    "the GPU's {} driver crashed in the last runs, as its {} driver did before ({} crashes in \
                     Vulkan's, {} in OpenGL ES's): {} from now on, until the system is updated",
                    on.name(),
                    other.name(),
                    crashes.vulkan,
                    crashes.gl,
                    settled.name()
                ),
            ));
            settled
        }
    };
    if to == Driver::Gl {
        crashes.left_vulkan = true;
    }
    start = if to == on {
        // a crash while it was being tried would send the next start on: it stays
        start.map(|(backend, stage)| (backend, if stage == Stage::Trying { Stage::Made } else { stage }))
    } else {
        Some((other_index, Stage::Made))
    };
    Some(Decided { start, crashes, said })
}

/// [`decide`] on the record in `dir` (for `build`, the app's and the system's), written back if it
/// changes.
fn update_in(
    dir: &Path,
    build: &str,
    runs: &[(i64, Option<Driver>)],
    system: &str,
    order: impl FnOnce() -> Vec<wgpu::Backends>,
) {
    let _writing = writing();
    let path = dir.join(RECORD);
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    let start = read(&saved);
    let kept = after_build(&saved).lines().find_map(Crashes::parse);
    let Some(decided) = decide(start, kept, runs, system, order) else { return };
    for (level, line) in &decided.said {
        log::log!(*level, "{line}");
    }
    // a start changed is this build's; one left as it was keeps the build it was written in
    let (first, build) = match decided.start {
        Some((backend, stage)) if decided.start != start => (format!("{backend} {}", stage.word()), build.trim()),
        _ => (saved.lines().next().unwrap_or_default().trim().to_owned(), built_by(&saved)),
    };
    write_record(&path, &first, build, &decided.crashes.line());
}

/// Whether the record `saved` says the app was sent off Vulkan for its driver's crashes, and not
/// settled back on it as the driver crashing less.
pub(super) fn vulkan_crashed(saved: &str) -> bool {
    after_build(saved)
        .lines()
        .find_map(Crashes::parse)
        .is_some_and(|c| c.left_vulkan && c.settled != Some(Driver::Vulkan))
}

#[cfg(test)]
mod tests {
    use super::super::{note_in, start_in};
    use super::*;
    use wgpu::Backends;

    const BUILD: &str = "1795462404146 BLU/B160V/B160V:14/UP1A.231005.007/1746439500:user/release-keys";
    const SYSTEM: &str = "BLU/B160V/B160V:14/UP1A.231005.007/1746439500:user/release-keys";
    const UPDATED: &str = "BLU/B160V/B160V:14/UP1A.231005.007/1760000000:user/release-keys";

    /// The tester's crash, as the log tells it: in the queue's submit, inside the GPU's Vulkan
    /// driver, under the Vulkan loader, called from the app.
    const VULKAN_CRASH: &str = "signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault address 0x0\n\
        thread 31694 \"procraft-render\", which crashed:\n\
        \x20   #00 pc 000000000006d2f8  /vendor/lib64/hw/vulkan.mt6761.so (BuildId: dd45d29103204ebf5fbc8710499c3325)\n\
        \x20   #01 pc 0000000000021380  /system/lib64/libvulkan.so (vulkan::driver::QueueSubmit(VkQueue_T*, unsigned \
        int, VkSubmitInfo const*, VkFence_T*)+64) (BuildId: 49c34caf38fab87d5b7a927cf5f13c52)\n\
        \x20   #02 pc 00000000011b320c  /data/app/~~a==/io.github.artcraftandroid.procraft-b==/base.apk\n\
        thread 31304 \"ndroid.procraft\":\n\
        \x20   #00 pc 00000000000b5fd8  /apex/com.android.runtime/lib64/bionic/libc.so (__epoll_pwait+8)\n";

    /// A crash in the OpenGL ES driver, as a system's own text tombstone tells it.
    const GL_CRASH: &str = "pid: 4000, tid: 4321, name: procraft-render  >>> io.github.artcraftandroid.procraft <<<\n\
        signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0x0\n\n\
        backtrace:\n\
        \x20     #00 pc 00000000001a2b3c  /vendor/lib64/egl/libGLESv2_mtk.so (BuildId: 0a)\n\
        \x20     #01 pc 0000000000012345  /system/lib64/libGLESv2.so (glDrawArrays+28) (BuildId: 0b)\n\
        \x20     #02 pc 0000000000fedcba  /data/app/~~a==/io.github.artcraftandroid.procraft-b==/base.apk\n";

    /// A crash in the app's own code, nowhere near a driver.
    const APP_CRASH: &str = "signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault address 0x10\n\
        thread 4321 \"procraft-render\", which crashed:\n\
        \x20   #00 pc 0000000000001234  /apex/com.android.runtime/lib64/bionic/libc.so (memcpy+12)\n\
        \x20   #01 pc 0000000000fedcba  /data/app/~~a==/io.github.artcraftandroid.procraft-b==/base.apk\n";

    fn files(trace: &str) -> Vec<String> {
        crate::app_log::crashed_frame_files(trace.as_bytes())
    }

    fn crash(at: i64, trace: &str) -> EndedRun {
        EndedRun { ended: at, crashed_natively: true, trace: Some(trace.as_bytes().to_vec()) }
    }

    /// A run that ended without a crash (the user closed it, the app was updated).
    fn closed(at: i64) -> EndedRun {
        EndedRun { ended: at, crashed_natively: false, trace: None }
    }

    /// A record folder of its own.
    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("procraft-driver-crashes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Hands the record in `dir` how the last runs ended, as `gpu_last_runs` does, on a device
    /// whose backends are `order`; the backend the next start begins from, and the record's crashes.
    fn start_after(dir: &Path, runs: &[EndedRun], system: &str, order: &[Backends]) -> (usize, Option<Crashes>) {
        update_in(dir, BUILD, &ended(runs), system, || order.to_vec());
        let saved = std::fs::read_to_string(dir.join(RECORD)).unwrap_or_default();
        (start_in(dir, order), after_build(&saved).lines().find_map(Crashes::parse))
    }

    const VULKAN_FIRST: [Backends; 2] = [Backends::VULKAN, Backends::GL];

    #[test]
    fn a_crash_is_told_by_the_outermost_graphics_library_its_thread_was_in() {
        assert_eq!(driver_of(&files(VULKAN_CRASH)), Some(Driver::Vulkan));
        assert_eq!(driver_of(&files(GL_CRASH)), Some(Driver::Gl));
        assert_eq!(driver_of(&files(APP_CRASH)), None);
        let of = |files: &[&str]| driver_of(&files.iter().map(|f| (*f).to_owned()).collect::<Vec<_>>());
        // ANGLE's OpenGL ES over the Vulkan driver: OpenGL ES's
        assert_eq!(
            of(&[
                "/vendor/lib64/hw/vulkan.samsung.so",
                "/system/lib64/libvulkan.so",
                "/system/lib64/libGLESv2_angle.so",
                "/data/app/~~a==/io.github.artcraftandroid.procraft-b==/base.apk"
            ]),
            Some(Driver::Gl)
        );
        // a Mali's Vulkan driver (a file of its OpenGL ES library) under the Vulkan loader: Vulkan's
        assert_eq!(
            of(&["/vendor/lib64/egl/libGLES_mali.so", "/system/lib64/libvulkan.so", "base.apk"]),
            Some(Driver::Vulkan)
        );
        assert_eq!(of(&["/vendor/lib64/egl/libGLES_mali.so", "base.apk"]), Some(Driver::Gl));
        // the driver's own thread
        assert_eq!(
            of(&["/vendor/lib64/hw/vulkan.adreno.so", "/apex/com.android.runtime/lib64/bionic/libc.so"]),
            Some(Driver::Vulkan)
        );
        // a library of the app's own named like a driver isn't one
        assert_eq!(of(&["/data/app/~~a==/x-b==/lib/arm64/vulkan.mine.so"]), None);
        assert_eq!(of(&[]), None);
    }

    #[test]
    fn the_crashes_line_reads_back_as_written() {
        let crashes = Crashes {
            vulkan: 5,
            gl: 1,
            settled: Some(Driver::Gl),
            left_vulkan: true,
            seen: 1_763_926_445_246,
            system: SYSTEM.to_owned(),
        };
        assert_eq!(
            crashes.line(),
            format!(
                "driver crashes: vulkan 5, gl 1, settled on gl, left vulkan yes, seen 1763926445246, system {SYSTEM}"
            )
        );
        assert_eq!(Crashes::parse(&crashes.line()), Some(crashes));
        assert_eq!(Crashes::parse("driver crashes: vulkan five, gl 1, settled on none, seen 1, system x"), None);
        assert_eq!(Crashes::parse("1 drew"), None);
    }

    /// The tester's device: five starts in a row crashed in its Vulkan driver half a minute in.
    /// The newest crash in it sends the next start to OpenGL ES; so do two of the last three, as
    /// the first start of an updated app sees them (the update ended the run before).
    #[test]
    fn a_crash_in_the_vulkan_driver_starts_the_next_run_on_opengl_es() {
        let dir = folder("vulkan");
        note_in(&dir, BUILD, 0, Stage::Made);
        assert_eq!(start_after(&dir, &[closed(100)], SYSTEM, &VULKAN_FIRST).0, 0);
        // a run without a driver crash writes nothing
        assert_eq!(std::fs::read_to_string(dir.join(RECORD)).unwrap(), format!("0 made\n{BUILD}"));
        let (start, crashes) = start_after(&dir, &[crash(200, VULKAN_CRASH), closed(100)], SYSTEM, &VULKAN_FIRST);
        assert_eq!(start, 1);
        assert_eq!(
            crashes,
            Some(Crashes { vulkan: 1, gl: 0, settled: None, left_vulkan: true, seen: 200, system: SYSTEM.to_owned() })
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(RECORD)).unwrap(),
            format!(
                "1 made\n{BUILD}\ndriver crashes: vulkan 1, gl 0, settled on none, left vulkan yes, seen 200, system {SYSTEM}"
            )
        );
        // the try on it noted as before, the crashes kept beside it; one that drew is kept for
        // the build with them there too
        note_in(&dir, BUILD, 1, Stage::Trying);
        note_in(&dir, BUILD, 1, Stage::Drew);
        note_in(&dir, BUILD, 1, Stage::Trying);
        assert_eq!(
            std::fs::read_to_string(dir.join(RECORD)).unwrap(),
            format!(
                "1 drew\n{BUILD}\ndriver crashes: vulkan 1, gl 0, settled on none, left vulkan yes, seen 200, system {SYSTEM}"
            )
        );
        assert_eq!(built_by(&std::fs::read_to_string(dir.join(RECORD)).unwrap()), BUILD);
        // the same runs seen again change nothing
        let before = std::fs::read_to_string(dir.join(RECORD)).unwrap();
        assert_eq!(start_after(&dir, &[crash(200, VULKAN_CRASH), closed(100)], SYSTEM, &VULKAN_FIRST).0, 1);
        assert_eq!(std::fs::read_to_string(dir.join(RECORD)).unwrap(), before);

        // two of the last three, the newest an update
        let dir = folder("vulkan-two");
        note_in(&dir, BUILD, 0, Stage::Made);
        let runs = [closed(500), crash(400, VULKAN_CRASH), crash(300, VULKAN_CRASH), crash(200, VULKAN_CRASH)];
        let (start, crashes) = start_after(&dir, &runs, SYSTEM, &VULKAN_FIRST);
        assert_eq!(start, 1);
        // the three looked at: two crashes counted
        assert_eq!(crashes.map(|c| (c.vulkan, c.gl, c.seen)), Some((2, 0, 500)));
        // no record at all yet: the first backend's, so the same
        let dir = folder("vulkan-unrecorded");
        assert_eq!(start_after(&dir, &[crash(200, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST).0, 1);
        for name in ["vulkan", "vulkan-two", "vulkan-unrecorded"] {
            std::fs::remove_dir_all(folder(name)).unwrap();
        }
    }

    /// A crash anywhere but the Vulkan driver, a crash in it no longer among the newest, or one
    /// while the app was on OpenGL ES, leaves the app on Vulkan.
    #[test]
    fn a_crash_elsewhere_stays_on_vulkan() {
        let dir = folder("elsewhere");
        note_in(&dir, BUILD, 0, Stage::Drew);
        let saved = std::fs::read_to_string(dir.join(RECORD)).unwrap();
        for runs in [
            vec![crash(300, APP_CRASH), crash(200, APP_CRASH), crash(100, APP_CRASH)],
            // crashed in Kotlin, or natively with no trace kept
            vec![EndedRun { ended: 300, crashed_natively: false, trace: Some(VULKAN_CRASH.into()) }],
            vec![EndedRun { ended: 300, crashed_natively: true, trace: None }],
            // one crash in the driver, not the newest of the three
            vec![closed(300), crash(200, VULKAN_CRASH), closed(100)],
            // two, but older than the three looked at
            vec![closed(600), closed(500), closed(400), crash(300, VULKAN_CRASH), crash(200, VULKAN_CRASH)],
        ] {
            assert_eq!(start_after(&dir, &runs, SYSTEM, &VULKAN_FIRST).0, 0, "{runs:?}");
            assert!(std::fs::read_to_string(dir.join(RECORD)).unwrap().starts_with(&saved));
        }
        // the one crash counted, nothing more
        let saved = std::fs::read_to_string(dir.join(RECORD)).unwrap();
        assert!(
            saved.ends_with(&format!("vulkan 1, gl 0, settled on none, left vulkan no, seen 300, system {SYSTEM}")),
            "{saved}"
        );
        // on OpenGL ES (no Vulkan device made), a Vulkan driver's crash is counted, not acted on
        let dir = folder("elsewhere-gl");
        note_in(&dir, BUILD, 1, Stage::Drew);
        assert_eq!(start_after(&dir, &[crash(200, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST).0, 1);
        // and a GPU with one backend has nowhere to go
        let dir = folder("elsewhere-one");
        assert_eq!(start_after(&dir, &[crash(200, VULKAN_CRASH)], SYSTEM, &[Backends::VULKAN]).0, 0);
        for name in ["elsewhere", "elsewhere-gl", "elsewhere-one"] {
            std::fs::remove_dir_all(folder(name)).unwrap();
        }
    }

    /// Vulkan's driver crashes once: OpenGL ES. Its driver crashes once too: back to Vulkan, which
    /// crashed no more than it did, for good. Vulkan crashing again, or OpenGL ES once more
    /// (tried after a failure), changes nothing: never back and forth.
    #[test]
    fn a_crash_in_the_opengl_es_driver_goes_back_once_and_then_stops() {
        let dir = folder("back");
        note_in(&dir, BUILD, 0, Stage::Drew);
        assert_eq!(start_after(&dir, &[crash(100, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST).0, 1);
        note_in(&dir, BUILD, 1, Stage::Drew);
        let (start, crashes) =
            start_after(&dir, &[crash(200, GL_CRASH), crash(100, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST);
        assert_eq!(start, 0);
        assert_eq!(crashes.as_ref().map(|c| (c.vulkan, c.gl, c.settled)), Some((1, 1, Some(Driver::Vulkan))));
        // Vulkan crashes again (twice): it stays
        note_in(&dir, BUILD, 0, Stage::Drew);
        let runs = [crash(400, VULKAN_CRASH), crash(300, VULKAN_CRASH), crash(200, GL_CRASH)];
        let (start, crashes) = start_after(&dir, &runs, SYSTEM, &VULKAN_FIRST);
        assert_eq!(start, 0);
        assert_eq!(crashes.as_ref().map(|c| (c.vulkan, c.gl, c.settled)), Some((3, 1, Some(Driver::Vulkan))));
        // the start left as it was, with its build, and the crashes once
        let saved = std::fs::read_to_string(dir.join(RECORD)).unwrap();
        assert!(saved.starts_with(&format!("0 drew\n{BUILD}\ndriver crashes: vulkan 3")), "{saved}");
        assert_eq!(saved.lines().count(), 3, "{saved}");
        // even crashing while it was being tried (which would send the next start on; noted
        // under another build, so that the one that drew isn't kept)
        note_in(&dir, "another build", 0, Stage::Trying);
        assert!(
            std::fs::read_to_string(dir.join(RECORD)).unwrap().starts_with("0 trying\nanother build\ndriver crashes:")
        );
        assert_eq!(
            start_after(&dir, &[crash(500, VULKAN_CRASH), crash(400, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST).0,
            0
        );
        // OpenGL ES, tried after Vulkan failed with an error, crashes: back to Vulkan
        note_in(&dir, BUILD, 1, Stage::Made);
        let (start, crashes) =
            start_after(&dir, &[crash(600, GL_CRASH), crash(500, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST);
        assert_eq!(start, 0);
        assert_eq!(crashes.map(|c| (c.vulkan, c.gl, c.settled)), Some((4, 2, Some(Driver::Vulkan))));

        // Vulkan crashed more (both of two): OpenGL ES's one crash keeps it, for good
        let dir = folder("back-less");
        note_in(&dir, BUILD, 0, Stage::Drew);
        assert_eq!(
            start_after(&dir, &[crash(200, VULKAN_CRASH), crash(100, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST).0,
            1
        );
        note_in(&dir, BUILD, 1, Stage::Drew);
        let runs = [crash(300, GL_CRASH), crash(200, VULKAN_CRASH), crash(100, VULKAN_CRASH)];
        let (start, crashes) = start_after(&dir, &runs, SYSTEM, &VULKAN_FIRST);
        assert_eq!(start, 1);
        assert_eq!(crashes.map(|c| (c.vulkan, c.gl, c.settled)), Some((2, 1, Some(Driver::Gl))));
        for name in ["back", "back-less"] {
            std::fs::remove_dir_all(folder(name)).unwrap();
        }
    }

    /// A GPU that starts on OpenGL ES (an old PowerVR's) whose OpenGL ES driver crashes: Vulkan.
    #[test]
    fn a_crash_in_the_opengl_es_driver_of_a_gpu_starting_on_it_starts_on_vulkan() {
        let dir = folder("gl-first");
        note_in(&dir, BUILD, 0, Stage::Drew);
        let gl_first = [Backends::GL, Backends::VULKAN];
        let (start, crashes) = start_after(&dir, &[crash(100, GL_CRASH)], SYSTEM, &gl_first);
        assert_eq!(start, 1);
        assert_eq!(crashes.map(|c| (c.vulkan, c.gl, c.settled)), Some((0, 1, None)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The tester's record after the first test build: Vulkan's driver had crashed four times,
    /// three pipelines failed on OpenGL ES and the app went back to Vulkan ("1 made", OpenGL ES
    /// first on the GE8300). The next start is on OpenGL ES; a crash counted but not acted on, a
    /// settling on Vulkan as the driver crashing less, or a try of OpenGL ES that never finished
    /// leaves the start where the record has it.
    #[test]
    fn a_start_left_on_vulkan_after_its_driver_crashed_is_on_opengl_es() {
        let dir = folder("left");
        let gl_first = [Backends::GL, Backends::VULKAN];
        let start_with = |first: &str, crashes: &str, order: &[Backends]| {
            std::fs::write(
                dir.join(RECORD),
                format!(
                    "{first}
{BUILD}
driver crashes: {crashes}, system {SYSTEM}"
                ),
            )
            .unwrap();
            (start_in(&dir, order), vulkan_crashed(&std::fs::read_to_string(dir.join(RECORD)).unwrap()))
        };
        // the line as the first test build wrote it, with no word of leaving Vulkan
        assert_eq!(start_with("1 made", "vulkan 4, gl 0, settled on none, seen 1795469998766", &gl_first), (0, true));
        assert_eq!(
            start_with("0 drew", "vulkan 1, gl 0, settled on none, left vulkan yes, seen 1", &VULKAN_FIRST),
            (1, true)
        );
        assert_eq!(
            start_with("0 drew", "vulkan 1, gl 0, settled on none, left vulkan no, seen 1", &VULKAN_FIRST),
            (0, false)
        );
        assert_eq!(
            start_with("0 made", "vulkan 3, gl 4, settled on vulkan, left vulkan yes, seen 1", &VULKAN_FIRST),
            (0, false)
        );
        assert_eq!(
            start_with("0 trying", "vulkan 4, gl 0, settled on none, left vulkan yes, seen 1", &gl_first),
            (1, true)
        );
        // no crashes kept at all
        std::fs::write(
            dir.join(RECORD),
            format!(
                "1 made
{BUILD}"
            ),
        )
        .unwrap();
        assert_eq!(start_in(&dir, &gl_first), 1);
        procraft_gpu::vulkan_crashed(false);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A system update (a new build fingerprint, perhaps a new driver) gives Vulkan another
    /// chance; the crashes before it count no more.
    #[test]
    fn a_new_build_fingerprint_gives_vulkan_another_chance() {
        let dir = folder("updated");
        note_in(&dir, BUILD, 0, Stage::Drew);
        assert_eq!(start_after(&dir, &[crash(100, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST).0, 1);
        note_in(&dir, BUILD, 1, Stage::Drew);
        // the same system: still OpenGL ES
        assert_eq!(start_after(&dir, &[closed(200), crash(100, VULKAN_CRASH)], SYSTEM, &VULKAN_FIRST).0, 1);
        // updated, with the crash before still among the last runs
        let (start, crashes) =
            start_after(&dir, &[closed(300), closed(200), crash(100, VULKAN_CRASH)], UPDATED, &VULKAN_FIRST);
        assert_eq!(start, 0);
        assert_eq!(
            crashes,
            Some(Crashes {
                vulkan: 0,
                gl: 0,
                settled: None,
                left_vulkan: false,
                seen: 100,
                system: UPDATED.to_owned()
            })
        );
        // Vulkan crashing under it again: OpenGL ES again
        note_in(&dir, BUILD, 0, Stage::Drew);
        let (start, crashes) = start_after(&dir, &[crash(400, VULKAN_CRASH), closed(300)], UPDATED, &VULKAN_FIRST);
        assert_eq!(start, 1);
        assert_eq!(crashes.map(|c| (c.vulkan, c.seen)), Some((1, 400)));
        // a settled choice is undone by an update too
        let dir = folder("updated-settled");
        let settled = Crashes {
            vulkan: 3,
            gl: 1,
            settled: Some(Driver::Gl),
            left_vulkan: true,
            seen: 100,
            system: SYSTEM.to_owned(),
        };
        std::fs::write(dir.join(RECORD), format!("1 drew\n{BUILD}\n{}", settled.line())).unwrap();
        assert_eq!(start_after(&dir, &[closed(200)], UPDATED, &VULKAN_FIRST).0, 0);
        for name in ["updated", "updated-settled"] {
            std::fs::remove_dir_all(folder(name)).unwrap();
        }
    }
}
