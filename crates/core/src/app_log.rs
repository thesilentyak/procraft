//! The app's log kept in a file as well, so that someone whose app crashed can send what led up to
//! it: on Android the log otherwise goes only to the system's, which nothing in the app can share.
//! It is `procraft.log` in the app's own folder, with the run before's kept beside it as
//! `procraft-previous.log`, as the desktop app keeps its own.
//!
//! The interface begins it with a header (the app, the device) and how the last runs ended (with
//! the system's trace of a crash: see [`trace_text`]), and adds its own warnings and errors; the
//! core's log goes in from `info` up. Each line goes into the file as it comes, with no buffer in
//! between, so a crash in native code leaves every line before it. A log that reaches [`MOST`]
//! keeps its start and its latest lines (see [`cut`]), and what could name the person or their
//! files is left out of every line (see [`redact`]).
//!
//! Help's Share log hands both logs on as one text file, with the GPU start record the device
//! module keeps (see [`write_log_to_share`]); so does the gallery's question about an artwork whose
//! opening closed the app. Each log begins with its run's process, so that the log of a run that
//! closed while it opened an artwork is kept apart as well (see [`keep_log_of_ended_runs`]): the
//! starts after it would otherwise push it out before it is shared.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

/// The log, and the run before's, in the app's own folder (the desktop app's own log has the same
/// names).
pub(crate) const LOG: &str = "procraft.log";
pub(crate) const PREVIOUS_LOG: &str = "procraft-previous.log";
/// The log of a run that closed while it opened an artwork, kept while the open marks name it.
pub(crate) const CRASHED_LOG: &str = "procraft-crashed.log";

/// How large a log grows, bytes.
const MOST: usize = 1 << 20;

/// What a log at [`MOST`] keeps of its start (the header, the GPU) and of its latest lines, bytes.
const HEAD: usize = 128 << 10;
const TAIL: usize = 512 << 10;

/// The most one write puts in, bytes.
const WRITE_MOST: usize = 64 << 10;

/// The most of a trace of how a run ended that goes in, bytes.
const TRACE_MOST: usize = 48 << 10;

/// What stands where a log was cut.
const LEFT_OUT: &str = "[... lines left out here, to keep the log under 1 MB ...]\n";

/// Where the device's shared storage is: a file's path in it names the person's own file.
const SHARED_STORAGE: [&str; 4] = ["/storage/", "/sdcard/", "/mnt/", "/data/media/"];

/// The log's file, once begun.
static KEPT: Mutex<Option<LogFile>> = Mutex::new(None);

/// When the log began: each line says how long after.
static BEGAN: OnceLock<Instant> = OnceLock::new();

std::thread_local! {
    /// Whether this thread is writing into the log's file.
    // (already const: clippy 1.99 asks for it again when built for Android)
    #[allow(clippy::missing_const_for_thread_local)]
    static WRITING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// How serious a line of the interface's log is.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn level(self) -> log::Level {
        match self {
            LogLevel::Info => log::Level::Info,
            LogLevel::Warn => log::Level::Warn,
            LogLevel::Error => log::Level::Error,
        }
    }
}

/// Begins the log's file in `dir` (the app's files folder), the last run's kept beside it, with
/// `header` (the app, the device) at its top. Once a process: a later call (the app's screen made
/// again) goes on in the same file.
#[uniffi::export]
pub fn start_log_file(dir: String, header: String) {
    crate::device::start_log();
    BEGAN.get_or_init(Instant::now);
    let begun = {
        let mut kept = kept();
        if kept.is_some() {
            return;
        }
        *kept = LogFile::begin(Path::new(&dir));
        kept.is_some()
    };
    if begun {
        write(&process_line(std::process::id()));
        write(&header);
    } else {
        log::warn!("the log's file couldn't be made");
    }
}

/// A line of the interface's log (`tag`: the part of the app it comes from), into the file only:
/// the interface gives it to the system's log itself.
#[uniffi::export]
pub fn log_line(level: LogLevel, tag: String, message: String) {
    write(&line(level.level(), &tag, &message));
}

/// A trace the system kept of how one of the app's last runs ended, as text for the log: a frozen
/// app's (already text) as it is; a crash in native code's (the system's record of it, in its
/// binary form) told as its signal, its cause, the threads' places in the code and the process's
/// last lines in the system's log. At most [`TRACE_MOST`] either way.
#[uniffi::export]
pub fn trace_text(trace: Vec<u8>) -> String {
    let text = match std::str::from_utf8(&trace) {
        Ok(text) if is_text(text) => text.to_owned(),
        _ => {
            tombstone::read(&trace).unwrap_or_else(|| format!("(a trace of {} bytes the app can't read)", trace.len()))
        }
    };
    trimmed(&text, TRACE_MOST)
}

/// Whether a trace is text (a frozen app's, an older system's crash), not a record in binary form.
fn is_text(text: &str) -> bool {
    text.chars().all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
}

/// The files the frames of the thread that crashed were in, innermost first, from the system's
/// trace of a crash in native code (its record in binary form, or as text: the first backtrace's),
/// as much of it as was read; none if it names none.
pub(crate) fn crashed_frame_files(trace: &[u8]) -> Vec<String> {
    let text = match std::str::from_utf8(trace) {
        Ok(text) => Some(text),
        // cut short inside a character
        Err(e) if e.error_len().is_none() => trace.get(..e.valid_up_to()).and_then(|t| std::str::from_utf8(t).ok()),
        Err(_) => None,
    };
    match text {
        Some(text) if is_text(text) => text
            .lines()
            .map(str::trim_start)
            .skip_while(|line| !line.starts_with("#00 pc "))
            .take_while(|line| line.starts_with('#') && line.split_whitespace().nth(1) == Some("pc"))
            .filter_map(|line| line.split_whitespace().nth(3).map(str::to_owned))
            .collect(),
        _ => tombstone::crashed_files(trace),
    }
}

/// Writes the log to share into the file `to`: what the GPU start record says and the GPU each run
/// drew with, then this run's log and the run before's, all from `dir` (the app's files folder);
/// whether it was written.
#[uniffi::export]
pub fn write_log_to_share(dir: String, to: String) -> bool {
    let text = shared_log(Path::new(&dir));
    let to = Path::new(&to);
    if let Some(folder) = to.parent() {
        let _ = fs::create_dir_all(folder);
    }
    crate::storage::write(to, text).map_err(|e| log::warn!("the log to share: {e}")).is_ok()
}

/// The line a log begins with: the run (process) it is of. (The desktop app's log has one where
/// each run's part begins: a start it makes itself goes on in the same log.)
pub fn process_line(run: u32) -> String {
    format!("process {run}\n")
}

/// The run a log is of, by its first line; none for a log from before logs said.
fn process_of(log: &[u8]) -> Option<u32> {
    let first = log.split(|&b| b == b'\n').next()?;
    process_in(std::str::from_utf8(first).ok()?)
}

/// The run (process) `line` names, if it is the line a run's part of a log begins with.
pub(crate) fn process_in(line: &str) -> Option<u32> {
    line.trim().strip_prefix("process ")?.parse().ok()
}

/// The runs a log is of, by the lines their parts begin with.
pub(crate) fn processes_of(log: &[u8]) -> impl Iterator<Item = u32> + '_ {
    log.split(|&b| b == b'\n').filter_map(|line| process_in(std::str::from_utf8(line).ok()?))
}

/// Keeps the run before's log apart as [`CRASHED_LOG`] when that run closed while it opened an
/// artwork (`ended`: the runs that did, as the open marks say), so that it is still there to share
/// after later starts; one kept for a run the marks no longer name (its artwork opened since) goes.
pub(crate) fn keep_log_of_ended_runs(ended: &[u32]) {
    // (the lock let go first: a line logged meanwhile takes it)
    let dir = kept().as_ref().and_then(|f| f.path.parent().map(Path::to_path_buf));
    if let Some(dir) = dir {
        keep_ended_in(&dir, ended);
    }
}

/// [`keep_log_of_ended_runs`] for the logs in `dir`, any of whose runs may be one that closed.
pub(crate) fn keep_ended_in(dir: &Path, ended: &[u32]) {
    let crashed = dir.join(CRASHED_LOG);
    if let Ok(before) = fs::read(dir.join(PREVIOUS_LOG))
        && processes_of(&before).any(|run| ended.contains(&run))
    {
        let written = crate::project::write_atomically(&crashed, |p| {
            crate::storage::write(p, &before).map_err(|e| crate::project::file_error("keeping the log", e))
        });
        if let Err(e) = written {
            log::warn!("{e}");
        }
        return;
    }
    let not_named = |log: &[u8]| {
        let mut runs = processes_of(log).peekable();
        runs.peek().is_some() && !runs.any(|run| ended.contains(&run))
    };
    if fs::read(&crashed).is_ok_and(|log| not_named(&log)) {
        let _ = fs::remove_file(&crashed);
    }
}

/// The log to share, from `dir` (see [`write_log_to_share`]), with what could name the person left
/// out once more: the logs were written so, but a log from another version may not have been.
fn shared_log(dir: &Path) -> String {
    use std::fmt::Write as _;
    let read = |name: &str| fs::read(dir.join(name)).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
    let (now, before) = (read(LOG), read(PREVIOUS_LOG));
    // a run that closed while it opened an artwork, unless that is the run before
    let run_before = before.as_deref().and_then(|b| process_of(b.as_bytes()));
    let crashed = read(CRASHED_LOG).filter(|c| process_of(c.as_bytes()) != run_before);
    let record = read(crate::device::RECORD).map(|r| r.trim().to_owned()).filter(|r| !r.is_empty());
    let mut out = String::from("ProCraft's log\n\n");
    // "<backend> trying|made|drew", on a second line the build it was kept for, and on a third
    // what the GPU's drivers' crashes came to
    let record = match record {
        Some(r) => {
            let mut lines = r.lines();
            let first = lines.next().unwrap_or_default().trim().to_owned();
            let build = lines.next().map(str::trim).filter(|b| !b.is_empty());
            let crashes = lines.next().map(str::trim).filter(|c| !c.is_empty());
            let shown = match first.split_whitespace().next().and_then(|b| b.parse::<usize>().ok()) {
                Some(backend) => format!("{first} (backend {backend} is {})", procraft_gpu::Gpu::backend_name(backend)),
                None => first,
            };
            let shown = match build {
                Some(build) => format!("{shown}, kept for the build {build}"),
                None => shown,
            };
            match crashes {
                Some(crashes) => format!("{shown}; {crashes}"),
                None => shown,
            }
        }
        None => "none".to_owned(),
    };
    let _ = writeln!(out, "GPU start record ({}): {record}", crate::device::RECORD);
    for (run, log) in [("this run", &now), ("the run before", &before)] {
        let _ = writeln!(out, "GPU, {run}: {}", log.as_deref().and_then(gpu_line).unwrap_or("not in its log"));
    }
    for (run, name, log) in [("This run", LOG, &now), ("The run before", PREVIOUS_LOG, &before)] {
        let _ = write!(out, "\n===== {run} ({name}) =====\n{}", log.as_deref().unwrap_or("(none)"));
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    if let Some(log) = crashed {
        let _ =
            write!(out, "\n===== An earlier run, which closed while it opened an artwork ({CRASHED_LOG}) =====\n{log}");
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    redact(&out, &app_folders(dir))
}

/// The GPU a log says the app drew with: what its last "GPU: " line says.
fn gpu_line(log: &str) -> Option<&str> {
    log.lines().rev().find_map(|line| line.find("] GPU: ").and_then(|at| line.get(at + 7..)))
}

/// The core's log into the file, from `info` up, and to the platform's own log (`system`: Android's)
/// where it has one the core writes to.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn install(system: Option<Box<dyn log::Log>>) {
    if log::set_logger(Box::leak(Box::new(Both(system)))).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// The core's log into the file, and to the platform's own log if it has one.
#[cfg(any(target_os = "android", target_os = "ios", test))]
struct Both(Option<Box<dyn log::Log>>);

#[cfg(any(target_os = "android", target_os = "ios", test))]
impl log::Log for Both {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if let Some(system) = &self.0 {
            system.log(record);
        }
        write(&line(record.level(), record.target(), &record.args().to_string()));
    }

    fn flush(&self) {}
}

/// A line of the log: how long after it began, how serious, where from, and what.
fn line(level: log::Level, target: &str, message: &str) -> String {
    let ms = BEGAN.get_or_init(Instant::now).elapsed().as_millis();
    format!("[{ms:>7} ms {level:<5} {target}] {message}\n")
}

/// Writes `text` into the log's file, if it has begun. Not from within a write on the same thread
/// (a panic in it, logged), which holds the file.
fn write(text: &str) {
    struct Writing;

    impl Drop for Writing {
        fn drop(&mut self) {
            WRITING.with(|w| w.set(false));
        }
    }

    if WRITING.with(|w| w.replace(true)) {
        return;
    }
    let _writing = Writing;
    if let Some(file) = kept().as_mut() {
        file.write(text);
    }
}

fn kept() -> MutexGuard<'static, Option<LogFile>> {
    KEPT.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The log's file, and how long it is.
struct LogFile {
    path: PathBuf,
    /// None while it is cut, or if it couldn't be opened again after.
    file: Option<File>,
    length: usize,
    /// The app's own folders, shortened in what is written.
    app: Vec<String>,
}

impl LogFile {
    /// A new log in `dir`, the last run's kept beside it; none if it can't be made.
    fn begin(dir: &Path) -> Option<LogFile> {
        let path = dir.join(LOG);
        let _ = fs::rename(&path, dir.join(PREVIOUS_LOG));
        let file = File::create(&path).ok()?;
        Some(LogFile { path, file: Some(file), length: 0, app: app_folders(dir) })
    }

    /// Writes `text` at the log's end, as whole lines: what could name the person left out, the
    /// log cut first if it would go past its most.
    fn write(&mut self, text: &str) {
        let mut text = trimmed(&redact(text, &self.app), WRITE_MOST);
        if !text.ends_with('\n') {
            text.push('\n');
        }
        if self.length + text.len() > MOST {
            self.cut();
        }
        let Some(file) = self.file.as_mut() else { return };
        if file.write_all(text.as_bytes()).is_ok() {
            self.length += text.len();
        }
    }

    /// The log, at its most, with its middle left out (see [`cut`]), in place of the whole.
    fn cut(&mut self) {
        // let go of it first: the cut log is put in its place
        self.file = None;
        let whole = fs::read(&self.path).unwrap_or_default();
        let kept = cut(&whole, HEAD, TAIL);
        let written = crate::project::write_atomically(&self.path, |tmp| {
            fs::write(tmp, &kept).map_err(|e| crate::project::file_error("the log", e))
        });
        // not written (the device full): begun again, rather than let grow past its most
        self.length = match written {
            Ok(()) => kept.len(),
            Err(_) => {
                let _ = File::create(&self.path);
                0
            }
        };
        self.file = OpenOptions::new().append(true).open(&self.path).ok();
    }
}

/// `log` with its middle left out: `head` bytes of its start and `tail` of its end, each to whole
/// lines, with a line saying so between them. As it is if it is no longer.
fn cut(log: &[u8], head: usize, tail: usize) -> Vec<u8> {
    if log.len() <= head + tail {
        return log.to_vec();
    }
    let start = log.get(..head).unwrap_or_default();
    let start = match start.iter().rposition(|&b| b == b'\n') {
        Some(i) => start.get(..=i).unwrap_or(start),
        None => start,
    };
    let end = log.get(log.len() - tail..).unwrap_or_default();
    let end = match end.iter().position(|&b| b == b'\n') {
        Some(i) => end.get(i + 1..).unwrap_or(end),
        None => end,
    };
    [start, LEFT_OUT.as_bytes(), end].concat()
}

/// `text`, or its start if it is longer than `most` bytes (to a line's end where there is one), with
/// a line saying how much was left out.
fn trimmed(text: &str, most: usize) -> String {
    if text.len() <= most {
        return text.to_owned();
    }
    let end = (0..=most).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
    let start = text.get(..end).unwrap_or_default();
    let start = match start.rfind('\n') {
        Some(i) if i > 0 => start.get(..=i).unwrap_or(start),
        _ => start,
    };
    let gap = if start.ends_with('\n') { "" } else { "\n" };
    format!("{start}{gap}[... {} KB more left out ...]\n", (text.len() - start.len()).div_ceil(1024))
}

/// The app's own folders, from `dir`, its files' folder: the app's data folder, and its other name
/// on Android (`/data/data/<package>`).
fn app_folders(dir: &Path) -> Vec<String> {
    let Some(data) = dir.parent().filter(|d| d.components().count() > 2) else { return Vec::new() };
    let mut folders = vec![data.to_string_lossy().into_owned()];
    if let Some(package) = data.file_name() {
        let other = format!("/data/data/{}", package.to_string_lossy());
        if !folders.contains(&other) {
            folders.push(other);
        }
    }
    folders
}

/// `text` with what could name the person or their files left out:
/// - the name in a home folder's path (`/home/<name>`, `/Users/<name>`, `C:\Users\<name>`), as
///   `<home>`, the rest of the path kept;
/// - the path of a file in the device's shared storage (`/storage/...`, `/sdcard/...`), and the
///   address of a file another app handed over (`content://...`, `file://...`), with only the
///   file's kind (its extension) kept, and the app that handed it over;
/// - the app's own folders (`app`), shortened to `<app>`, the rest of the path kept: nothing in
///   them is named by the person (an artwork's folder has a name of its own).
pub(crate) fn redact(text: &str, app: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let taken = if c == '/' || c.is_ascii_alphabetic() { left_out(rest, app, &mut out) } else { None };
        let n = taken.unwrap_or_else(|| {
            out.push(c);
            c.len_utf8()
        });
        rest = rest.get(n..).unwrap_or_default();
    }
    out
}

/// What is said in place of what `rest` begins with, if that is left out (see [`redact`]), put in
/// `out`; the bytes of `rest` it stands for.
fn left_out(rest: &str, app: &[String], out: &mut String) -> Option<usize> {
    if let Some(folder) = app.iter().find(|a| !a.is_empty() && rest.starts_with(a.as_str())) {
        out.push_str("<app>");
        return Some(folder.len());
    }
    if let Some(n) = home(rest) {
        out.push_str("<home>");
        return Some(n);
    }
    if starts_with_any_case(rest, "content://") {
        let n = address_length(rest);
        let path = rest.get("content://".len()..n)?;
        // only the app that handed it over: an address with no file in it says nothing
        let (from, file) = path.split_once('/')?;
        out.push_str(&format!("content://{from}/...{}", extension(file)));
        return Some(n);
    }
    if starts_with_any_case(rest, "file://") {
        let n = address_length(rest);
        out.push_str(&format!("file://...{}", extension(rest.get(..n)?)));
        return Some(n);
    }
    if SHARED_STORAGE.iter().any(|root| rest.starts_with(root)) {
        let n = path_length(rest);
        out.push_str(&format!("<storage>/...{}", extension(rest.get(..n)?)));
        return Some(n);
    }
    None
}

/// How long the home folder's path at `rest`'s start is, with the name in it (`/home/<name>`,
/// `/Users/<name>`, `C:\Users\<name>`); none if it doesn't start with one.
fn home(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let drive = bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && matches!(bytes.get(2), Some(b'\\' | b'/'))
        && rest.get(3..8).is_some_and(|w| w.eq_ignore_ascii_case("users"))
        && matches!(bytes.get(8), Some(b'\\' | b'/'));
    let prefix =
        if drive { 9 } else { ["/home/", "/Users/"].iter().find(|p| rest.starts_with(*p)).map(|p| p.len())? };
    // to the next folder, or what no account's name holds: a space, an apostrophe or a bracket
    // doesn't end it, as a name may have them
    let name = rest.get(prefix..)?;
    let n = name.find(|c: char| "/\\\n\r\t\"<>|:,;=+*?".contains(c)).unwrap_or(name.len());
    (n > 0).then_some(prefix + n)
}

/// How far the address at `rest`'s start goes: to a space, a double quote or what an address has
/// only encoded (`<>[]{}|` and the backquote). An apostrophe or a round bracket doesn't end it, as
/// Android leaves them as they are in a file's name; a closing bracket at its end is the text's,
/// though, when the address opens none.
fn address_length(rest: &str) -> usize {
    let n = rest.find(|c: char| c.is_whitespace() || "\"`<>[]{}|".contains(c)).unwrap_or(rest.len());
    let address = rest.get(..n).unwrap_or_default();
    if address.ends_with(')') && !address.contains('(') { n - 1 } else { n }
}

/// How far the path at `rest`'s start goes: to the end of its line, a double quote or what no
/// file's name holds on the device's shared storage (`<>|`), or a colon, comma or semicolon before a
/// space. A space, an apostrophe or a bracket doesn't end it: a file's name may have them.
fn path_length(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let ends = |i: usize, b: u8| match b {
        b'\n' | b'\r' | b'\t' | b'"' | b'<' | b'>' | b'|' => true,
        b':' | b',' | b';' => bytes.get(i + 1).is_none_or(u8::is_ascii_whitespace),
        _ => false,
    };
    bytes.iter().enumerate().position(|(i, &b)| ends(i, b)).unwrap_or(bytes.len())
}

/// The extension of the file `path` names (with its dot), if it has a plain one; else nothing.
fn extension(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) if (1..=10).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()) => {
            format!(".{ext}")
        }
        _ => String::new(),
    }
}

fn starts_with_any_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

/// The system's record of a crash in native code (its "tombstone", in protocol buffers' binary
/// form), read as far as the log needs it.
mod tombstone {
    use std::fmt::Write as _;

    /// How many frames of a thread that didn't crash are told.
    const OTHER_FRAMES: usize = 4;

    /// How many of the process's last lines in the system's log are told.
    const LOG_LINES: usize = 30;

    /// A field's value: a number, or bytes (a string, or a message of its own).
    enum Value<'a> {
        Number(u64),
        Bytes(&'a [u8]),
    }

    /// A message's fields in turn (number and value), up to one that can't be read: a trace cut
    /// short still gives what came before.
    struct Fields<'a>(&'a [u8]);

    impl<'a> Iterator for Fields<'a> {
        type Item = (u64, Value<'a>);

        fn next(&mut self) -> Option<Self::Item> {
            let field = self.field();
            if field.is_none() {
                self.0 = &[];
            }
            field
        }
    }

    impl<'a> Fields<'a> {
        fn field(&mut self) -> Option<(u64, Value<'a>)> {
            let key = self.varint()?;
            let value = match key & 7 {
                0 => Value::Number(self.varint()?),
                1 => Value::Number(u64::from_le_bytes(self.take(8)?.try_into().ok()?)),
                2 => {
                    let length = usize::try_from(self.varint()?).ok()?;
                    Value::Bytes(self.take(length)?)
                }
                5 => Value::Number(u64::from(u32::from_le_bytes(self.take(4)?.try_into().ok()?))),
                _ => return None,
            };
            Some((key >> 3, value))
        }

        fn varint(&mut self) -> Option<u64> {
            let mut value = 0;
            for shift in (0..64).step_by(7) {
                let (&byte, rest) = self.0.split_first()?;
                self.0 = rest;
                value |= u64::from(byte & 0x7f) << shift;
                if byte & 0x80 == 0 {
                    return Some(value);
                }
            }
            None
        }

        fn take(&mut self, length: usize) -> Option<&'a [u8]> {
            let (taken, rest) = self.0.split_at_checked(length)?;
            self.0 = rest;
            Some(taken)
        }
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// A signed field's number (sent as 64 bits, two's complement).
    fn signed(n: u64) -> i64 {
        i64::from_ne_bytes(n.to_ne_bytes())
    }

    #[derive(Default)]
    struct Signal {
        number: i64,
        name: String,
        code: i64,
        code_name: String,
        fault: Option<u64>,
    }

    #[derive(Default)]
    struct Frame {
        rel_pc: u64,
        function: String,
        offset: u64,
        file: String,
        build_id: String,
    }

    #[derive(Default)]
    struct Thread {
        id: u64,
        name: String,
        registers: Vec<(String, u64)>,
        frames: Vec<Frame>,
    }

    fn signal(bytes: &[u8]) -> Signal {
        let mut signal = Signal::default();
        let (mut has_fault, mut fault) = (false, 0);
        for (n, value) in Fields(bytes) {
            match (n, value) {
                (1, Value::Number(v)) => signal.number = signed(v),
                (2, Value::Bytes(b)) => signal.name = text(b),
                (3, Value::Number(v)) => signal.code = signed(v),
                (4, Value::Bytes(b)) => signal.code_name = text(b),
                (8, Value::Number(v)) => has_fault = v != 0,
                (9, Value::Number(v)) => fault = v,
                _ => {}
            }
        }
        signal.fault = has_fault.then_some(fault);
        signal
    }

    fn frame(bytes: &[u8]) -> Frame {
        let mut frame = Frame::default();
        for (n, value) in Fields(bytes) {
            match (n, value) {
                (1, Value::Number(v)) => frame.rel_pc = v,
                (4, Value::Bytes(b)) => frame.function = text(b),
                (5, Value::Number(v)) => frame.offset = v,
                (6, Value::Bytes(b)) => frame.file = text(b),
                (8, Value::Bytes(b)) => frame.build_id = text(b),
                _ => {}
            }
        }
        frame
    }

    fn thread(bytes: &[u8]) -> Thread {
        let mut thread = Thread::default();
        for (n, value) in Fields(bytes) {
            match (n, value) {
                (1, Value::Number(v)) => thread.id = v,
                (2, Value::Bytes(b)) => thread.name = text(b),
                (3, Value::Bytes(b)) => {
                    let (mut name, mut at) = (String::new(), 0);
                    for (n, value) in Fields(b) {
                        match (n, value) {
                            (1, Value::Bytes(b)) => name = text(b),
                            (2, Value::Number(v)) => at = v,
                            _ => {}
                        }
                    }
                    thread.registers.push((name, at));
                }
                (4, Value::Bytes(b)) => thread.frames.push(frame(b)),
                _ => {}
            }
        }
        thread
    }

    /// A buffer of the system's log: its lines, each as "time level tag: message".
    fn log_lines(bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for (n, value) in Fields(bytes) {
            let (2, Value::Bytes(b)) = (n, value) else { continue };
            let (mut time, mut level, mut tag, mut message) = (String::new(), 0, String::new(), String::new());
            for (n, value) in Fields(b) {
                match (n, value) {
                    (1, Value::Bytes(b)) => time = text(b),
                    (4, Value::Number(v)) => level = v,
                    (5, Value::Bytes(b)) => tag = text(b),
                    (6, Value::Bytes(b)) => message = text(b),
                    _ => {}
                }
            }
            let level = match level {
                2 => 'V',
                3 => 'D',
                4 => 'I',
                5 => 'W',
                6 => 'E',
                7 => 'F',
                _ => '?',
            };
            lines.push(format!("{time} {level} {tag}: {}", message.trim_end()));
        }
        lines
    }

    /// A thread from the record's map of them: each under its number, as a pair of fields.
    fn thread_entry(bytes: &[u8]) -> Option<Thread> {
        Fields(bytes).find_map(|(n, v)| match (n, v) {
            (2, Value::Bytes(b)) => Some(thread(b)),
            _ => None,
        })
    }

    /// The files the crashed thread's frames were in, innermost first (see
    /// [`super::crashed_frame_files`]).
    pub(super) fn crashed_files(bytes: &[u8]) -> Vec<String> {
        let mut crashed = None;
        let mut threads = Vec::new();
        for (n, value) in Fields(bytes) {
            match (n, value) {
                (6, Value::Number(v)) => crashed = Some(v),
                (16, Value::Bytes(b)) => threads.extend(thread_entry(b)),
                _ => {}
            }
        }
        let thread = threads.into_iter().find(|t| Some(t.id) == crashed);
        thread.map(|t| t.frames.into_iter().map(|f| f.file).collect()).unwrap_or_default()
    }

    /// The record `bytes` as text: none if it isn't one (no signal and no threads in it).
    pub(super) fn read(bytes: &[u8]) -> Option<String> {
        let (mut crashed, mut uptime, mut said) = (None, None, None);
        let (mut causes, mut threads, mut logs) = (Vec::new(), Vec::new(), Vec::new());
        let mut abort = String::new();
        for (n, value) in Fields(bytes) {
            match (n, value) {
                (6, Value::Number(v)) => crashed = Some(v),
                (10, Value::Bytes(b)) => said = Some(signal(b)),
                (14, Value::Bytes(b)) => abort = text(b),
                (15, Value::Bytes(b)) => causes.extend(Fields(b).find_map(|(n, v)| match (n, v) {
                    (1, Value::Bytes(b)) => Some(text(b)),
                    _ => None,
                })),
                (16, Value::Bytes(b)) => threads.extend(thread_entry(b)),
                (18, Value::Bytes(b)) => logs.extend(log_lines(b)),
                (20, Value::Number(v)) => uptime = Some(v),
                _ => {}
            }
        }
        if said.is_none() && threads.is_empty() {
            return None;
        }
        let mut out = String::new();
        if let Some(s) = said {
            let _ = write!(out, "signal {} ({}), code {} ({})", s.number, s.name, s.code, s.code_name);
            if let Some(fault) = s.fault {
                let _ = write!(out, ", fault address {fault:#x}");
            }
            out.push('\n');
        }
        if let Some(seconds) = uptime {
            let _ = writeln!(out, "the process had run {seconds} s");
        }
        if !abort.is_empty() {
            let _ = writeln!(out, "abort message: {abort}");
        }
        for cause in causes {
            let _ = writeln!(out, "cause: {cause}");
        }
        // the thread that crashed first and whole, the others after it with their first frames
        threads.sort_by_key(|t| (Some(t.id) != crashed, t.id));
        for t in &threads {
            let it = Some(t.id) == crashed;
            let _ = writeln!(out, "thread {} \"{}\"{}:", t.id, t.name, if it { ", which crashed" } else { "" });
            if it {
                for four in t.registers.chunks(4) {
                    let line: Vec<String> = four.iter().map(|(name, at)| format!("{name:<4}{at:016x}")).collect();
                    let _ = writeln!(out, "    {}", line.join("  "));
                }
            }
            let shown = if it { t.frames.len() } else { t.frames.len().min(OTHER_FRAMES) };
            for (i, f) in t.frames.iter().take(shown).enumerate() {
                let _ = write!(out, "    #{i:02} pc {:016x}  {}", f.rel_pc, f.file);
                if !f.function.is_empty() {
                    let _ = write!(out, " ({}+{})", f.function, f.offset);
                }
                if !f.build_id.is_empty() {
                    let _ = write!(out, " (BuildId: {})", f.build_id);
                }
                out.push('\n');
            }
            if shown < t.frames.len() {
                let _ = writeln!(out, "    ... {} more", t.frames.len() - shown);
            }
        }
        if !logs.is_empty() {
            let _ = writeln!(out, "the process's last lines in the system's log:");
            for line in logs.iter().skip(logs.len().saturating_sub(LOG_LINES)) {
                let _ = writeln!(out, "    {line}");
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("procraft-log-{name}-{}", std::process::id())).join("files");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_new_log_keeps_the_last_runs_beside_it() {
        let dir = folder("roll");
        let mut first = LogFile::begin(&dir).unwrap();
        first.write("the first run\n");
        drop(first);
        let mut second = LogFile::begin(&dir).unwrap();
        second.write("the second run\n");
        drop(second);
        assert_eq!(fs::read_to_string(dir.join(PREVIOUS_LOG)).unwrap(), "the first run\n");
        assert_eq!(fs::read_to_string(dir.join(LOG)).unwrap(), "the second run\n");
        // the one before that goes
        LogFile::begin(&dir).unwrap();
        assert_eq!(fs::read_to_string(dir.join(PREVIOUS_LOG)).unwrap(), "the second run\n");
        assert_eq!(fs::read_to_string(dir.join(LOG)).unwrap(), "");
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    /// The log of a run that closed while it opened an artwork outlasts the starts after it, until
    /// that artwork opens: shared with this run's and the run before's, as itself, once.
    #[test]
    fn the_log_of_a_run_that_closed_opening_an_artwork_outlasts_the_starts_after_it() {
        let dir = folder("crashed");
        let run = |pid: u32, said: &str| {
            let mut log = LogFile::begin(&dir).unwrap();
            log.write(&process_line(pid));
            log.write(said);
        };
        // run 7 closes while it opens an artwork (its mark stays); run 8 starts
        run(7, "opening the artwork that closes the app\n");
        run(8, "the gallery, once\n");
        keep_ended_in(&dir, &[7]);
        let kept = fs::read(dir.join(CRASHED_LOG)).unwrap();
        assert_eq!(process_of(&kept), Some(7));
        assert_eq!(kept, fs::read(dir.join(PREVIOUS_LOG)).unwrap());
        // the run before is it: shared once, not twice
        let shared = shared_log(&dir);
        assert_eq!(shared.matches("opening the artwork that closes the app").count(), 1, "{shared}");
        // run 9 starts: run 7's log is no longer the run before's, and is still shared
        run(9, "the gallery again\n");
        keep_ended_in(&dir, &[7]);
        let shared = shared_log(&dir);
        assert_eq!(process_of(&fs::read(dir.join(PREVIOUS_LOG)).unwrap()), Some(8));
        assert!(shared.contains("===== An earlier run, which closed while it opened an artwork"), "{shared}");
        assert_eq!(shared.matches("opening the artwork that closes the app").count(), 1, "{shared}");
        assert!(shared.contains("the gallery, once") && shared.contains("the gallery again"));
        // its artwork opened since (no mark names run 7): it goes
        run(10, "opened it anyway\n");
        keep_ended_in(&dir, &[]);
        assert!(!dir.join(CRASHED_LOG).exists());
        assert!(!shared_log(&dir).contains("opening the artwork that closes the app"));
        // a log from before logs said their run is never taken for one that closed
        fs::write(dir.join(PREVIOUS_LOG), "no process line\n").unwrap();
        keep_ended_in(&dir, &[7]);
        assert!(!dir.join(CRASHED_LOG).exists());
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    /// The desktop's log goes on in the same file after a start the app made itself, each run's
    /// part beginning with its process: kept apart as well when a later run in it closed.
    #[test]
    fn a_log_whose_later_run_closed_opening_an_artwork_is_kept_too() {
        let dir = folder("crashed-again");
        fs::write(dir.join(PREVIOUS_LOG), "process 7\nthe first start\nprocess 8\nopening the artwork\n").unwrap();
        keep_ended_in(&dir, &[8]);
        assert_eq!(fs::read(dir.join(CRASHED_LOG)).unwrap(), fs::read(dir.join(PREVIOUS_LOG)).unwrap());
        assert_eq!(processes_of(&fs::read(dir.join(CRASHED_LOG)).unwrap()).collect::<Vec<_>>(), [7, 8]);
        // a start later, no longer the run before's but still named: kept
        fs::write(dir.join(PREVIOUS_LOG), "process 9\n").unwrap();
        keep_ended_in(&dir, &[8]);
        assert!(dir.join(CRASHED_LOG).exists());
        // its artwork opened since: it goes
        keep_ended_in(&dir, &[]);
        assert!(!dir.join(CRASHED_LOG).exists());
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_log_at_its_most_keeps_its_start_and_its_latest_lines() {
        let dir = folder("cap");
        let mut log = LogFile::begin(&dir).unwrap();
        log.write("the header\n");
        for i in 0..40_000 {
            log.write(&format!("line {i} of the run, long enough to fill a megabyte soon\n"));
        }
        drop(log);
        let text = fs::read_to_string(dir.join(LOG)).unwrap();
        assert!(text.len() <= MOST, "{} bytes", text.len());
        assert!(text.starts_with("the header\nline 0 of the run"), "{}", &text[..100]);
        assert!(text.ends_with("line 39999 of the run, long enough to fill a megabyte soon\n"));
        assert!(text.contains(LEFT_OUT));
        // whole lines either side of the cut, and every line kept is whole
        assert!(text.lines().all(|l| l == "the header" || l == LEFT_OUT.trim_end() || l.ends_with("megabyte soon")));
        // the lines kept follow on from one another after the cut, up to the last
        let after: Vec<usize> = text
            .rsplit(LEFT_OUT)
            .next()
            .unwrap()
            .lines()
            .map(|l| l.split(' ').nth(1).unwrap().parse().unwrap())
            .collect();
        assert!(after.windows(2).all(|w| w[1] == w[0] + 1));
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_cut_keeps_whole_lines_from_either_end() {
        let log = b"aaaa\nbbbb\ncccc\ndddd\neeee\n";
        assert_eq!(cut(log, 100, 100), log.to_vec());
        let kept = String::from_utf8(cut(log, 7, 7)).unwrap();
        assert_eq!(kept, format!("aaaa\n{LEFT_OUT}eeee\n"));
        // a line longer than what is kept: cut where it falls
        let kept = String::from_utf8(cut(b"aaaaaaaaaa\nbbbbbbbbbb", 4, 4)).unwrap();
        assert_eq!(kept, format!("aaaa{LEFT_OUT}bbbb"));
    }

    #[test]
    fn a_write_too_long_is_trimmed_to_whole_lines() {
        assert_eq!(trimmed("short\n", 10), "short\n");
        assert_eq!(trimmed("one\ntwo\nthree\n", 9), "one\ntwo\n[... 1 KB more left out ...]\n");
        // no line's end in reach, and a letter of two bytes not split
        assert_eq!(trimmed("ééééé", 5), "éé\n[... 1 KB more left out ...]\n");
    }

    #[test]
    fn what_could_name_the_person_or_their_files_is_left_out() {
        let app = app_folders(Path::new("/data/user/0/io.github.artcraftandroid.procraft/files"));
        let r = |text: &str| redact(text, &app);
        // the app's own folders, shortened
        assert_eq!(
            r("keeping /data/user/0/io.github.artcraftandroid.procraft/files/artworks/a1b2/doc.json: no room"),
            "keeping <app>/files/artworks/a1b2/doc.json: no room"
        );
        assert_eq!(r("/data/data/io.github.artcraftandroid.procraft/cache/x"), "<app>/cache/x");
        // home folders: the name goes, the rest of the path stays
        assert_eq!(
            r("panicked at /home/jo/procraft/crates/core/src/lib.rs:10:5"),
            "panicked at <home>/procraft/crates/core/src/lib.rs:10:5"
        );
        assert_eq!(r("/Users/Jo Smith/x.rs"), "<home>/x.rs");
        assert_eq!(r("/home/o'brien (work)/x.rs"), "<home>/x.rs");
        assert_eq!(r(r"at C:\Users\jo\src\main.rs and c:/users/jo/x"), r"at <home>\src\main.rs and <home>/x");
        // a file of the person's: only its kind stays, whatever its name holds
        assert_eq!(
            r("java.io.FileNotFoundException: /storage/emulated/0/Download/Jo's Drawing (2).psd: open failed: EACCES"),
            "java.io.FileNotFoundException: <storage>/....psd: open failed: EACCES"
        );
        assert_eq!(r("reading /sdcard/Pictures/holiday [1] {b}.jpg, 2 MB"), "reading <storage>/....jpg, 2 MB");
        assert_eq!(
            r("opening content://com.android.providers.downloads.documents/document/msf%3A1000.procraft failed"),
            "opening content://com.android.providers.downloads.documents/....procraft failed"
        );
        // an apostrophe and round brackets stay as they are in an address's file name
        assert_eq!(
            r("content://com.android.externalstorage.documents/document/primary%3AJo's%20sketch%20(2).psd"),
            "content://com.android.externalstorage.documents/....psd"
        );
        assert_eq!(r("(file:///storage/emulated/0/Secret%20name.brushset)"), "(file://....brushset)");
        assert_eq!(r("(content://media/external/images/media/1234)"), "(content://media/...)");
        // nothing else changes: words, numbers, the system's own paths, an address with no file
        let plain = "GPU: Mali-G715 (Vulkan, driver 1.3), /vendor/lib64/egl/libGLES_mali.so at 0x7a, content://media é";
        assert_eq!(r(plain), plain);
        // and what has been through it once stays as it is
        let once = r(
            "at /storage/emulated/0/a.psd\nand content://x/y/z.png and /home/jo/q\n/data/data/io.github.artcraftandroid.procraft/f",
        );
        assert_eq!(once, "at <storage>/....psd\nand content://x/....png and <home>/q\n<app>/f");
        assert_eq!(r(&once), once);
        // a space in a file's path doesn't end it, so the rest of its line goes too, unless quoted
        assert_eq!(r("reading /storage/emulated/0/My Art/a b.png took 3 ms"), "reading <storage>/...");
        assert_eq!(
            r("reading \"/storage/emulated/0/My Art/a b.png\" took 3 ms"),
            "reading \"<storage>/....png\" took 3 ms"
        );
    }

    #[test]
    fn written_lines_leave_out_what_could_name_the_person() {
        let dir = folder("redact");
        let mut log = LogFile::begin(&dir).unwrap();
        let given = format!(
            "[ 1 ms WARN  ProCraft] importing a PSD\njava.io.IOException: {}: gone\n/storage/emulated/0/Download/me.psd\n",
            dir.join("x").display()
        );
        log.write(&given);
        drop(log);
        let text = fs::read_to_string(dir.join(LOG)).unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            text,
            format!(
                "[ 1 ms WARN  ProCraft] importing a PSD\njava.io.IOException: <app>{sep}files{sep}x: gone\n<storage>/....psd\n"
            )
        );
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_core_s_log_goes_on_to_the_platform_s_from_info_up() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Counted(Arc<AtomicUsize>);
        impl log::Log for Counted {
            fn enabled(&self, _: &log::Metadata) -> bool {
                true
            }
            fn log(&self, _: &log::Record) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
            fn flush(&self) {}
        }
        let heard = Arc::new(AtomicUsize::new(0));
        let both = Both(Some(Box::new(Counted(Arc::clone(&heard)))));
        for level in [log::Level::Error, log::Level::Warn, log::Level::Info, log::Level::Debug, log::Level::Trace] {
            log::Log::log(
                &both,
                &log::Record::builder().level(level).target("procraft").args(format_args!("a line")).build(),
            );
        }
        // error, warn and info reach it; debug and trace don't
        assert_eq!(heard.load(Ordering::SeqCst), 3);
        assert!(!log::Log::enabled(&both, &log::Metadata::builder().level(log::Level::Debug).build()));
        // without a platform's log (the iPad's), lines go to the file alone, and nothing fails
        log::Log::log(
            &Both(None),
            &log::Record::builder().level(log::Level::Warn).args(format_args!("a line")).build(),
        );
    }

    #[test]
    fn a_line_says_how_long_after_the_start_how_serious_and_where_from() {
        let l = line(log::Level::Warn, "ProCraft", "importing a PSD");
        assert!(l.starts_with('['), "{l}");
        assert!(l.ends_with(" ms WARN  ProCraft] importing a PSD\n"), "{l}");
    }

    /// A field of a record in protocol buffers' binary form: its number, then a number or bytes.
    fn varint(mut n: u64, out: &mut Vec<u8>) {
        while n >= 0x80 {
            out.push((n as u8) | 0x80);
            n >>= 7;
        }
        out.push(n as u8);
    }

    fn number(field: u64, n: u64) -> Vec<u8> {
        let mut out = Vec::new();
        varint(field << 3, &mut out);
        varint(n, &mut out);
        out
    }

    fn bytes(field: u64, b: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        varint((field << 3) | 2, &mut out);
        varint(b.len() as u64, &mut out);
        out.extend_from_slice(b);
        out
    }

    fn frame(pc: u64, function: &str) -> Vec<u8> {
        let mut f = number(1, pc);
        f.extend(number(2, pc + 0x7000_0000));
        if !function.is_empty() {
            f.extend(bytes(4, function.as_bytes()));
            f.extend(number(5, 40));
        }
        f.extend(bytes(6, b"/data/app/~~a==/io.github.artcraftandroid.procraft-b==/lib/arm64/libprocraft_core.so"));
        f.extend(bytes(8, b"0123abcd"));
        f
    }

    fn thread(id: u64, name: &str, frames: usize) -> Vec<u8> {
        let mut t = number(1, id);
        t.extend(bytes(2, name.as_bytes()));
        for (register, at) in
            [("x0", 0u64), ("x1", 0x10), ("pc", 0x7000_1234), ("sp", 0x7fff_0000), ("lr", 0x7000_1000)]
        {
            let mut r = bytes(1, register.as_bytes());
            r.extend(number(2, at));
            t.extend(bytes(3, &r));
        }
        for i in 0..frames {
            t.extend(bytes(
                4,
                &frame(0x1234 + i as u64 * 0x10, if i == 0 { "wgpu_hal::vulkan::Queue::submit" } else { "" }),
            ));
        }
        // under its number
        let mut entry = number(1, id);
        entry.extend(bytes(2, &t));
        bytes(16, &entry)
    }

    /// A record of a crash in native code, as the system keeps it.
    fn tombstone() -> Vec<u8> {
        let mut signal = number(1, 11);
        signal.extend(bytes(2, b"SIGSEGV"));
        signal.extend(number(3, 1));
        signal.extend(bytes(4, b"SEGV_MAPERR"));
        signal.extend(number(8, 1));
        signal.extend(number(9, 0x10));
        let mut log = bytes(1, b"main");
        for (level, tag, message) in [(4, "procraft", "GPU: Mali-G715"), (5, "vulkan", "the device was lost\n")] {
            let mut line = bytes(1, b"11-13 14:02:11.123");
            line.extend(number(4, level));
            line.extend(bytes(5, tag.as_bytes()));
            line.extend(bytes(6, message.as_bytes()));
            log.extend(bytes(2, &line));
        }
        let mut t = number(1, 1);
        t.extend(bytes(2, b"samsung/gts9/gts9:14/UP1A/X:user/release-keys"));
        t.extend(number(5, 4000));
        t.extend(number(6, 4321));
        t.extend(bytes(10, &signal));
        t.extend(bytes(15, &bytes(1, b"null pointer dereference")));
        t.extend(thread(4000, "main", 6));
        t.extend(thread(4321, "procraft render", 3));
        t.extend(bytes(17, &bytes(1, b"a memory map, not told")));
        t.extend(bytes(18, &log));
        t.extend(number(20, 95));
        t
    }

    #[test]
    fn a_native_crash_is_told_as_its_signal_cause_threads_and_last_log_lines() {
        let text = trace_text(tombstone());
        let crashed = text.find("thread 4321 \"procraft render\", which crashed:\n").expect(&text);
        let other = text.find("thread 4000 \"main\":\n").expect(&text);
        assert!(crashed < other, "{text}");
        for said in [
            "signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault address 0x10\n",
            "the process had run 95 s\n",
            "cause: null pointer dereference\n",
            "    x0  0000000000000000  x1  0000000000000010  pc  0000000070001234  sp  000000007fff0000\n    lr  0000000070001000\n",
            "    #00 pc 0000000000001234  /data/app/~~a==/io.github.artcraftandroid.procraft-b==/lib/arm64/libprocraft_core.so \
             (wgpu_hal::vulkan::Queue::submit+40) (BuildId: 0123abcd)\n",
            "    #02 pc 0000000000001254  /data/app/",
            // the others' first frames only
            "    #03 pc 0000000000001264",
            "    ... 2 more\n",
            "    11-13 14:02:11.123 W vulkan: the device was lost\n",
        ] {
            assert!(text.contains(said), "{said:?} not in\n{text}");
        }
        assert!(!text.contains("memory map"), "{text}");
        assert!(!text.contains("#04"), "{text}");
    }

    /// The files the crashed thread's frames were in, innermost first, from the record in binary
    /// form (the crashed thread's, wherever it is among the others), from one cut short after its
    /// threads, and from text (the first backtrace's, as the log tells it).
    #[test]
    fn a_native_crash_s_frames_are_told_by_the_files_they_were_in() {
        let files = [
            "/vendor/lib64/hw/vulkan.mt6761.so",
            "/system/lib64/libvulkan.so",
            "/data/app/~~a==/io.github.artcraftandroid.procraft-b==/base.apk",
        ];
        let mut t = number(1, 31694);
        t.extend(bytes(2, b"procraft-render"));
        for (i, file) in files.iter().enumerate() {
            let mut f = number(1, 0x6d2f8 + i as u64);
            f.extend(bytes(6, file.as_bytes()));
            t.extend(bytes(4, &f));
        }
        let mut entry = number(1, 31694);
        entry.extend(bytes(2, &t));
        let mut record = number(6, 31694);
        record.extend(thread(31304, "ndroid.procraft", 4));
        record.extend(bytes(16, &entry));
        record.extend(thread(31305, "Signal Catcher", 2));
        record.extend(bytes(17, &[0x7f; 64]));
        assert_eq!(crashed_frame_files(&record), files);
        assert_eq!(crashed_frame_files(&record[..record.len() - 20]), files);
        let text = trace_text(record);
        assert_eq!(crashed_frame_files(text.as_bytes()), files);
        // text cut short inside a character
        let mut cut = format!("{text}\u{e9}").into_bytes();
        cut.pop();
        assert_eq!(crashed_frame_files(&cut), files);
        // a system's own text tombstone: its backtrace
        let tombstone = "pid: 1, tid: 2, name: procraft-render  >>> io.github.artcraftandroid.procraft <<<\n\
                         backtrace:\n      #00 pc 0000000000001234  /vendor/lib64/egl/libGLESv2_mtk.so (BuildId: 0a)\n      \
                         #01 pc 0000000000005678  /system/lib64/libGLESv2.so (glDrawArrays+28)\n\nmemory near x0:\n";
        assert_eq!(
            crashed_frame_files(tombstone.as_bytes()),
            ["/vendor/lib64/egl/libGLESv2_mtk.so", "/system/lib64/libGLESv2.so"]
        );
        // no thread that crashed, or neither form
        assert!(crashed_frame_files(&thread(4000, "main", 3)).is_empty());
        assert!(crashed_frame_files(&[8, 1, 0xff]).is_empty());
    }

    #[test]
    fn a_trace_cut_short_still_tells_what_came_before() {
        let whole = tombstone();
        let text = trace_text(whole[..whole.len() - 20].to_vec());
        assert!(text.starts_with("signal 11 (SIGSEGV)"), "{text}");
        assert!(text.contains("which crashed"), "{text}");
    }

    #[test]
    fn a_trace_in_text_stays_as_it_is_up_to_its_most() {
        let anr = "----- pid 4000 at 2026-11-13 14:02:11 -----\n\"main\" prio=5 tid=1 Native\n  at a.b(c.kt:1)\n";
        assert_eq!(trace_text(anr.as_bytes().to_vec()), anr);
        let long: String = (0..10_000).map(|i| format!("  at frame{i}(f.kt:{i})\n")).collect();
        let text = trace_text(long.into_bytes());
        assert!(text.len() <= TRACE_MOST + 100, "{}", text.len());
        assert!(text.starts_with("  at frame0(f.kt:0)\n") && text.ends_with(" KB more left out ...]\n"));
        // neither text nor a record
        assert_eq!(trace_text(vec![8, 1, 0xff]), "(a trace of 3 bytes the app can't read)");
    }

    #[test]
    fn the_log_to_share_holds_both_runs_and_the_gpu_first() {
        let dir = folder("share");
        let mut log = LogFile::begin(&dir).unwrap();
        log.write(
            "the run before\n[     20 ms INFO  procraft_gpu] GPU: Adreno 750 (Vulkan, driver 1.3), chosen in 3 ms\n",
        );
        drop(log);
        let mut log = LogFile::begin(&dir).unwrap();
        log.write("this run\n[     21 ms INFO  procraft_gpu] GPU: Adreno 750 (Gl, driver 3.2), chosen in 4 ms\n");
        drop(log);
        fs::write(dir.join(crate::device::RECORD), "1 drew").unwrap();
        // a line from a log written before lines were redacted
        fs::OpenOptions::new()
            .append(true)
            .open(dir.join(LOG))
            .unwrap()
            .write_all(b"opening /storage/emulated/0/Download/mine.psd\n")
            .unwrap();
        let to = dir.parent().unwrap().join("shared").join("procraft-log.txt");
        assert!(write_log_to_share(dir.display().to_string(), to.display().to_string()));
        let text = fs::read_to_string(&to).unwrap();
        let backend = procraft_gpu::Gpu::backend_name(1);
        assert_eq!(
            text,
            format!(
                "ProCraft's log\n\n\
                 GPU start record (gpu-start.txt): 1 drew (backend 1 is {backend})\n\
                 GPU, this run: Adreno 750 (Gl, driver 3.2), chosen in 4 ms\n\
                 GPU, the run before: Adreno 750 (Vulkan, driver 1.3), chosen in 3 ms\n\
                 \n===== This run (procraft.log) =====\n\
                 this run\n[     21 ms INFO  procraft_gpu] GPU: Adreno 750 (Gl, driver 3.2), chosen in 4 ms\n\
                 opening <storage>/....psd\n\
                 \n===== The run before (procraft-previous.log) =====\n\
                 the run before\n[     20 ms INFO  procraft_gpu] GPU: Adreno 750 (Vulkan, driver 1.3), chosen in 3 ms\n"
            )
        );
        // nothing yet: said so
        let empty = folder("share-empty");
        let text = shared_log(&empty);
        assert!(text.contains("GPU start record (gpu-start.txt): none\nGPU, this run: not in its log\n"), "{text}");
        assert!(text.ends_with("===== The run before (procraft-previous.log) =====\n(none)\n"), "{text}");
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
        fs::remove_dir_all(empty.parent().unwrap()).unwrap();
    }
}
