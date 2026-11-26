//! How the app's last runs ended, as the desktop app's own log tells it: a crash in native code
//! (the crash handler's record of it, made with [`CrashLine`]), the app stopping itself on an
//! error it caught (a panic on the window's thread), the app ending as it should, or no word at
//! all (killed, a log-out or a shut-down, the power gone, a crash nothing could log). Android asks
//! the system instead (see `app_log.rs`).
//!
//! Each run's part of a log begins with its process's line (see `app_log::process_line`): a start
//! the app makes itself on another graphics backend goes on in the same log. What follows is told
//! by the lines' targets and messages, as the desktop's logger writes them:
//! `[<ms> ms <level> <target>] <message>`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::app_log::{CRASHED_LOG, LOG, PREVIOUS_LOG, process_in, processes_of};
use crate::open_marks::RunEnd;

/// The target of the crash's record in the log.
pub const CRASH_TARGET: &str = "crash";
/// What the crash's record begins with (its first line's message, the crash said after it).
pub const RUN_CRASHED: &str = "the app crashed: ";
/// What the log says as the app stops itself on an error it caught (what stopped it after it).
pub const RUN_STOPPED: &str = "stopped: ";
/// What the log says as the app starts itself again (on another graphics backend).
pub const RUN_STARTING_AGAIN: &str = "starting again";
/// What the log says as the app ends as it should.
pub const RUN_ENDING: &str = "the app is ending as it should";

/// What a line of the crash's record has before its message: the error level and the target.
const CRASH_MARK: &str = " ERROR crash] ";

/// How many lines of a crash's record are told at most, after its first.
const RECORD_LINES: usize = 80;

/// The most of a log read to tell how its runs ended, bytes: its end, where they end.
const READ_MOST: u64 = 8 << 20;

/// The most a line of a crash's record holds, bytes, with its end.
const CRASH_LINE_MOST: usize = 384;

/// A line of a crash's record, made with no allocation in a buffer of its own: a signal handler
/// may not allocate (the crash may have happened in the allocator). What doesn't fit is cut.
pub struct CrashLine {
    buffer: [u8; CRASH_LINE_MOST],
    len: usize,
}

impl Default for CrashLine {
    fn default() -> Self {
        Self::new()
    }
}

impl CrashLine {
    pub const fn new() -> CrashLine {
        CrashLine { buffer: [0; CRASH_LINE_MOST], len: 0 }
    }

    /// Begins the line again as the log's lines begin (`ms` after the app started, at the error
    /// level, with the crash's target), for a line written straight into the log's file.
    pub fn begin(&mut self, ms: u64) -> &mut Self {
        self.len = 0;
        self.text("[").padded(ms, 7).text(" ms ERROR ").text(CRASH_TARGET).text("] ")
    }

    /// Begins the line again with nothing in it (for a line handed to the logger, which begins it).
    pub fn clear(&mut self) -> &mut Self {
        self.len = 0;
        self
    }

    pub fn text(&mut self, text: &str) -> &mut Self {
        self.bytes(text.as_bytes())
    }

    /// Bytes as they are (a file's name), as far as there is room: a byte is kept for its end.
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for &b in bytes {
            if self.len + 1 >= CRASH_LINE_MOST {
                break;
            }
            if let Some(slot) = self.buffer.get_mut(self.len) {
                *slot = b;
                self.len += 1;
            }
        }
        self
    }

    /// A number in decimal.
    pub fn dec(&mut self, n: u64) -> &mut Self {
        self.padded(n, 0)
    }

    /// A number in decimal, spaces before it up to `width` characters.
    pub fn padded(&mut self, n: u64, width: usize) -> &mut Self {
        let mut digits = [0u8; 20];
        let count = digits_of(n, 10, &mut digits);
        for _ in count..width {
            self.bytes(b" ");
        }
        self.bytes(digits.get(digits.len() - count..).unwrap_or_default())
    }

    /// A signed number in decimal.
    pub fn signed(&mut self, n: i64) -> &mut Self {
        if n < 0 {
            self.bytes(b"-");
        }
        self.dec(n.unsigned_abs())
    }

    /// A number in hexadecimal after "0x", zeros before it up to `width` digits.
    pub fn hex(&mut self, n: u64, width: usize) -> &mut Self {
        let mut digits = [0u8; 16];
        let count = digits_of(n, 16, &mut digits);
        self.text("0x");
        for _ in count..width {
            self.bytes(b"0");
        }
        self.bytes(digits.get(digits.len() - count..).unwrap_or_default())
    }

    /// A frame of the crashed thread: its place in the backtrace (the innermost 0) and its address.
    pub fn frame(&mut self, index: usize, address: u64) -> &mut Self {
        self.text("  #");
        if index < 10 {
            self.text("0");
        }
        self.dec(index as u64).text(" ").hex(address, 16)
    }

    /// The file (a library, the app) whose code a frame's address is in, by its name alone (its
    /// folder left out), and how far into it the address is.
    pub fn module(&mut self, file: &[u8], offset: u64) -> &mut Self {
        let name = file.rsplit(|&b| b == b'/' || b == b'\\').next().unwrap_or(file);
        self.text(" ").bytes(name).text("+").hex(offset, 0)
    }

    /// The line so far.
    pub fn as_bytes(&self) -> &[u8] {
        self.buffer.get(..self.len).unwrap_or_default()
    }

    /// The line so far as text: what was cut inside a character left out.
    pub fn as_str(&self) -> &str {
        let bytes = self.as_bytes();
        match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => bytes.get(..e.valid_up_to()).and_then(|b| std::str::from_utf8(b).ok()).unwrap_or_default(),
        }
    }

    /// The line with its end put after it, as the log's file takes it.
    pub fn ended(&mut self) -> &[u8] {
        if let Some(slot) = self.buffer.get_mut(self.len) {
            *slot = b'\n';
            self.len += 1;
        }
        self.as_bytes()
    }
}

/// `n`'s digits in `base` (10 or 16, lowercase) at the end of `out`; how many.
fn digits_of(mut n: u64, base: u64, out: &mut [u8]) -> usize {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut count = 0;
    loop {
        let at = out.len().saturating_sub(count + 1);
        if let (Some(slot), Some(&digit)) = (out.get_mut(at), DIGITS.get((n % base) as usize)) {
            *slot = digit;
        }
        count += 1;
        n /= base;
        if n == 0 || count == out.len() {
            return count;
        }
    }
}

/// How a run ended, as its part of the log tells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ended {
    /// It crashed in native code: what the crash's record said first (the signal or exception,
    /// where, on which thread), and its other lines (the frames).
    Crashed { said: String, record: Vec<String> },
    /// It stopped itself on an error it caught (a panic on the window's thread, its window that
    /// failed): what it said.
    Stopped(String),
    /// It ended as it should.
    AsItShould,
    /// It started itself again (on another graphics backend): its part ends there.
    StartedAgain,
    /// Its log says nothing of how it ended.
    NoWord,
}

impl Ended {
    /// How much it tells: what tells more wins over what tells less, wherever each is in the run's
    /// part (a crash as the app ends is a crash).
    fn weight(&self) -> u8 {
        match self {
            Ended::Crashed { .. } => 4,
            Ended::Stopped(_) => 3,
            Ended::AsItShould => 2,
            Ended::StartedAgain => 1,
            Ended::NoWord => 0,
        }
    }

    /// Whether the run crashed: in native code, or stopping itself on an error it caught.
    pub fn crashed(&self) -> bool {
        matches!(self, Ended::Crashed { .. } | Ended::Stopped(_))
    }

    /// How it ended, in words for the log; a crash's record after it, a line each, indented.
    pub fn words(&self) -> String {
        match self {
            Ended::Crashed { said, record } => {
                let mut words = format!("it crashed: {said}");
                for line in record {
                    words.push_str("\n    ");
                    words.push_str(line);
                }
                words
            }
            Ended::Stopped(said) => format!("it stopped: {said}"),
            Ended::AsItShould => "it ended as it should".to_owned(),
            Ended::StartedAgain => "it started itself again".to_owned(),
            Ended::NoWord => "its log doesn't say how it ended: it was closed from outside (killed, a log-out or a \
                              shut-down, the power gone), or it crashed where nothing could log it"
                .to_owned(),
        }
    }
}

/// A run's part of a log: its process (none for a log from before logs said), and how it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedRun {
    pub process: Option<u32>,
    pub ended: Ended,
}

/// The runs a log is of, oldest first, each with how it ended.
pub fn runs_in_log(log: &str) -> Vec<LoggedRun> {
    let mut runs = Vec::new();
    let mut run = LoggedRun { process: None, ended: Ended::NoWord };
    // whether the run has a part yet (a log from before logs said their process may have one)
    let mut begun = false;
    // whether the lines of a crash's record go on to the run's
    let mut recording = false;
    for line in log.lines() {
        if let Some(process) = process_in(line) {
            if begun {
                runs.push(run);
            }
            run = LoggedRun { process: Some(process), ended: Ended::NoWord };
            (begun, recording) = (true, false);
            continue;
        }
        begun |= !line.trim().is_empty();
        if let Some(message) = crash_message(line) {
            if let Some(said) = message.strip_prefix(RUN_CRASHED) {
                // a second record (another thread's) adds nothing to the first
                recording = !matches!(run.ended, Ended::Crashed { .. });
                if recording {
                    run.ended = Ended::Crashed { said: said.trim_end().to_owned(), record: Vec::new() };
                }
            } else if let Ended::Crashed { record, .. } = &mut run.ended
                && recording
                && record.len() < RECORD_LINES
            {
                record.push(message.trim_end().to_owned());
            }
            continue;
        }
        let Some(message) = message_of(line) else { continue };
        let ended = if let Some(said) = message.strip_prefix(RUN_STOPPED) {
            Ended::Stopped(said.trim().to_owned())
        } else if message.trim_end() == RUN_ENDING {
            Ended::AsItShould
        } else if message.trim_end() == RUN_STARTING_AGAIN {
            Ended::StartedAgain
        } else {
            continue;
        };
        if ended.weight() > run.ended.weight() {
            run.ended = ended;
        }
    }
    if begun {
        runs.push(run);
    }
    runs
}

/// The message of a line of a crash's record, wherever in the line it begins (a line the crash
/// cut short may be before it).
fn crash_message(line: &str) -> Option<&str> {
    line.find(CRASH_MARK).and_then(|at| line.get(at + CRASH_MARK.len()..))
}

/// The message of a line of the log as its logger writes it; none for a line that goes on from
/// the one before (a message of more than one line).
fn message_of(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let end = rest.find("] ")?;
    rest.get(..end)?.contains(" ms ").then(|| rest.get(end + 2..)).flatten()
}

/// What this run's log says of how the last runs ended, from the log of the run before
/// (`before`, none if there is none), newest first: a line for each run, a crash's record after
/// its line.
pub fn last_runs_said(before: Option<&str>) -> Vec<String> {
    let runs = before.map(runs_in_log).unwrap_or_default();
    if runs.is_empty() {
        return vec!["no earlier run is known".to_owned()];
    }
    let mut said = vec![if runs.len() == 1 {
        "how the run before ended:".to_owned()
    } else {
        format!("how the last {} runs ended, the newest first:", runs.len())
    }];
    said.extend(runs.iter().rev().map(|run| {
        let who = run
            .process
            .map_or_else(|| "a run whose log doesn't say its process".to_owned(), |p| format!("process {p}"));
        format!("{who}: {}", run.ended.words())
    }));
    said
}

/// [`last_runs_said`] from the log of the run before in the app's folder `dir`.
pub fn last_runs_in_folder(dir: &Path) -> Vec<String> {
    last_runs_said(read_log(&dir.join(PREVIOUS_LOG)).as_deref())
}

/// How run (process) `run` ended, by its part of a log in the app's folder `dir` (this run's, the
/// run before's, or the one kept for a run that closed while it opened an artwork): crashed, or
/// not known (its part says nothing of a crash, or no log has it).
pub fn run_end_in_folder(dir: &Path, run: u32) -> RunEnd {
    let found = [LOG, PREVIOUS_LOG, CRASHED_LOG]
        .into_iter()
        .filter_map(|name| read_log(&dir.join(name)))
        .find_map(|log| runs_in_log(&log).into_iter().find(|r| r.process == Some(run)));
    match found {
        Some(r) if r.ended.crashed() => RunEnd::Crashed,
        _ => RunEnd::Unknown,
    }
}

/// The log in the app's folder `dir` that has run `run`'s part, if one has: the one to share
/// about it.
pub fn log_of_run(dir: &Path, run: u32) -> Option<PathBuf> {
    [PREVIOUS_LOG, CRASHED_LOG, LOG]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| fs::read(path).is_ok_and(|log| processes_of(&log).any(|r| r == run)))
}

/// The log at `path` as text, at most its last [`READ_MOST`] bytes (from a whole line); none if it
/// can't be read.
fn read_log(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path).ok()?;
    let from = file.metadata().ok()?.len().saturating_sub(READ_MOST);
    if from > 0 {
        file.seek(SeekFrom::Start(from)).ok()?;
    }
    let mut bytes = Vec::new();
    file.take(READ_MOST).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let text = if from > 0 { text.split_once('\n').map_or("", |(_, rest)| rest) } else { &*text };
    Some(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line of the log as the desktop's logger writes it.
    fn logged(level: &str, target: &str, message: &str) -> String {
        format!("[{:>7} ms {level:<5} {target}] {message}\n", 12)
    }

    /// A crash's record as the crash handler writes it: its first line, a backtrace of two frames.
    fn record(said: &str) -> String {
        let mut out = String::new();
        let mut line = CrashLine::new();
        out.push_str(std::str::from_utf8(line.begin(12).text(RUN_CRASHED).text(said).ended()).unwrap());
        out.push_str(
            std::str::from_utf8(line.begin(12).text("backtrace, the innermost frame first:").ended()).unwrap(),
        );
        for (i, pc) in [0x5555_0000_1234u64, 0x7f00_0000_5678].into_iter().enumerate() {
            out.push_str(
                std::str::from_utf8(line.begin(12).frame(i, pc).module(b"/usr/lib/libx.so", 0x1234).ended()).unwrap(),
            );
        }
        out
    }

    #[test]
    fn a_crash_line_is_made_as_the_log_s_lines_are_and_cut_where_it_doesn_t_fit() {
        let mut line = CrashLine::new();
        line.begin(42).text("signal ").signed(11).text(", code ").signed(-6).text(" at ").hex(0x10, 0);
        assert_eq!(line.as_str(), format!("[{:>7} ms ERROR crash] signal 11, code -6 at 0x10", 42));
        assert!(line.as_str().ends_with("0x10") && line.as_str().contains(CRASH_MARK));
        line.clear().frame(3, 0x7fff_1234).module(b"/usr/lib/x86_64-linux-gnu/libvulkan_intel.so", 0xabc);
        assert_eq!(line.as_str(), "  #03 0x000000007fff1234 libvulkan_intel.so+0xabc");
        line.clear().frame(12, u64::MAX).module(br"C:\Windows\System32\igvk64.dll", 0);
        assert_eq!(line.as_str(), "  #12 0xffffffffffffffff igvk64.dll+0x0");
        line.clear().dec(0).text(" ").dec(u64::MAX).text(" ").padded(7, 3).text(" ").signed(i64::MIN);
        assert_eq!(line.as_str(), format!("0 {}   7 {}", u64::MAX, i64::MIN));
        // too long: cut, with room for its end
        line.clear();
        for _ in 0..100 {
            line.text("é ok");
        }
        assert!(line.as_bytes().len() < CRASH_LINE_MOST);
        assert!(line.as_str().len() <= line.as_bytes().len());
        let ended = line.ended().to_vec();
        assert_eq!(ended.len(), CRASH_LINE_MOST);
        assert_eq!(ended.last(), Some(&b'\n'));
    }

    #[test]
    fn each_run_in_a_log_is_told_by_how_its_part_ends() {
        let log = [
            "process 41\n".to_owned(),
            logged("INFO", "procraft::startup", "ProCraft 0.0.7 on linux x86_64"),
            record("signal 11 (SIGSEGV, a bad memory access) at address 0x10, code 1, on thread \"main\""),
            "process 42\n".to_owned(),
            logged("ERROR", "procraft::startup", "panicked: panicked at src/a.rs:1:2:\nno"),
            logged("ERROR", "procraft::startup", &format!("{RUN_STOPPED}no")),
            "process 43\n".to_owned(),
            logged("INFO", "procraft::startup", RUN_STARTING_AGAIN),
            "process 44\n".to_owned(),
            logged("INFO", "procraft::startup", RUN_ENDING),
            "process 45\n".to_owned(),
            logged("INFO", "procraft::gallery", "the gallery"),
        ]
        .concat();
        let runs = runs_in_log(&log);
        let ended: Vec<(Option<u32>, &Ended)> = runs.iter().map(|r| (r.process, &r.ended)).collect();
        let crashed = Ended::Crashed {
            said: "signal 11 (SIGSEGV, a bad memory access) at address 0x10, code 1, on thread \"main\"".to_owned(),
            record: vec![
                "backtrace, the innermost frame first:".to_owned(),
                "  #00 0x0000555500001234 libx.so+0x1234".to_owned(),
                "  #01 0x00007f0000005678 libx.so+0x1234".to_owned(),
            ],
        };
        assert_eq!(
            ended,
            [
                (Some(41), &crashed),
                (Some(42), &Ended::Stopped("no".to_owned())),
                (Some(43), &Ended::StartedAgain),
                (Some(44), &Ended::AsItShould),
                (Some(45), &Ended::NoWord),
            ]
        );
        assert!(runs[0].ended.crashed() && runs[1].ended.crashed());
        assert!(runs[2..].iter().all(|r| !r.ended.crashed()));
    }

    #[test]
    fn a_crash_tells_more_than_what_came_around_it() {
        // crashed as it ended, and said so on Windows before starting again on the next backend
        let log = [
            "process 7\n".to_owned(),
            logged("INFO", "procraft::startup", RUN_ENDING),
            record("exception 0xC0000005 (an access violation) in igvk64.dll at 0x7ff812345678"),
            logged("ERROR", "procraft::startup", &format!("{RUN_STOPPED}it crashed in igvk64.dll")),
            logged("INFO", "procraft::startup", RUN_STARTING_AGAIN),
            // a second thread's record adds nothing
            record("signal 6 (SIGABRT, an abort), code -6"),
        ]
        .concat();
        let runs = runs_in_log(&log);
        assert_eq!(runs.len(), 1);
        let Ended::Crashed { said, record: lines } = &runs[0].ended else { panic!("{runs:?}") };
        assert!(said.starts_with("exception 0xC0000005"), "{said}");
        assert_eq!(lines.len(), 3);
        // a line the crash cut short: the record begins inside it
        let cut = format!("process 8\n[     5 ms INFO  x] half a li{}", record("signal 7 (SIGBUS, a bus error)"));
        assert!(matches!(&runs_in_log(&cut)[0].ended, Ended::Crashed { said, .. } if said.starts_with("signal 7")));
        // a log from before logs said their process: one run, its process not known
        let old = logged("ERROR", "procraft::startup", "stopped: it crashed in igvk64.dll (exception 0xC0000005)");
        assert_eq!(
            runs_in_log(&old),
            [LoggedRun {
                process: None,
                ended: Ended::Stopped("it crashed in igvk64.dll (exception 0xC0000005)".into())
            }]
        );
        assert!(runs_in_log("").is_empty());
        assert!(runs_in_log("\n\n").is_empty());
    }

    /// What this run's log says of the run before: its crash and frames, newest first; and those
    /// lines, in this run's log, aren't taken for this run's own crash by the start after it.
    #[test]
    fn the_last_runs_are_told_newest_first_and_not_taken_for_this_run_s_end() {
        let before = [
            "process 41\n".to_owned(),
            record("signal 11 (SIGSEGV, a bad memory access) at address 0x10, code 1"),
            "process 42\n".to_owned(),
            logged("INFO", "procraft::startup", RUN_ENDING),
        ]
        .concat();
        let said = last_runs_said(Some(&before));
        assert_eq!(
            said,
            [
                "how the last 2 runs ended, the newest first:".to_owned(),
                "process 42: it ended as it should".to_owned(),
                "process 41: it crashed: signal 11 (SIGSEGV, a bad memory access) at address 0x10, code 1\n    \
                 backtrace, the innermost frame first:\n      #00 0x0000555500001234 libx.so+0x1234\n      \
                 #01 0x00007f0000005678 libx.so+0x1234"
                    .to_owned(),
            ]
        );
        // this run's log, with them in it, then ended with no word: not a crash
        let now: String = std::iter::once("process 43\n".to_owned())
            .chain(said.iter().map(|s| logged("INFO", "last runs", s)))
            .collect();
        assert_eq!(runs_in_log(&now), [LoggedRun { process: Some(43), ended: Ended::NoWord }]);
        assert_eq!(last_runs_said(None), ["no earlier run is known"]);
        let one = last_runs_said(Some("process 9\n"));
        assert_eq!(one[0], "how the run before ended:");
        assert!(one[1].starts_with("process 9: its log doesn't say how it ended"), "{one:?}");
    }

    #[test]
    fn a_run_s_end_and_its_log_are_found_in_the_app_s_folder() {
        let dir = std::env::temp_dir().join(format!("procraft-run-ends-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(PREVIOUS_LOG), format!("process 41\n{}process 42\n", record("signal 11 (SIGSEGV)")))
            .unwrap();
        fs::write(dir.join(CRASHED_LOG), format!("process 40\n{}", logged("ERROR", "x", "stopped: a panic"))).unwrap();
        fs::write(dir.join(LOG), format!("process 43\n{}process 44\n", record("signal 4 (SIGILL)"))).unwrap();
        assert_eq!(run_end_in_folder(&dir, 41), RunEnd::Crashed);
        assert_eq!(run_end_in_folder(&dir, 40), RunEnd::Crashed);
        assert_eq!(run_end_in_folder(&dir, 43), RunEnd::Crashed, "this log: a run before the app started itself again");
        assert_eq!(run_end_in_folder(&dir, 42), RunEnd::Unknown);
        assert_eq!(run_end_in_folder(&dir, 99), RunEnd::Unknown);
        assert_eq!(log_of_run(&dir, 42), Some(dir.join(PREVIOUS_LOG)));
        assert_eq!(log_of_run(&dir, 40), Some(dir.join(CRASHED_LOG)));
        assert_eq!(log_of_run(&dir, 44), Some(dir.join(LOG)));
        assert_eq!(log_of_run(&dir, 99), None);
        let said = last_runs_in_folder(&dir);
        assert!(said[2].starts_with("process 41: it crashed: signal 11 (SIGSEGV)\n"), "{said:?}");
        // a log too long to read whole: its end, from a whole line
        let filler = "a line of the run, long enough\n";
        let long = format!(
            "process 50\n{}{}",
            filler.repeat(READ_MOST as usize / filler.len() + 1),
            record("signal 8 (SIGFPE)")
        );
        fs::write(dir.join(PREVIOUS_LOG), long).unwrap();
        let said = last_runs_in_folder(&dir);
        assert!(said[1].starts_with("a run whose log doesn't say its process: it crashed: signal 8"), "{said:?}");
        let _ = fs::remove_dir_all(&dir);
    }
}
