//! Folders someone picks outside the app, for the plugins that keep artworks there (interface 22):
//! a `folder` control on a plugin's card, and three hooks that name one. What goes there, when,
//! under what name, which copy is newer and what happens when both changed is all decided here;
//! the apps give only their way into a folder ([`FolderAccess`]): its files listed, a file written
//! in whole, a file read out whole.
//!
//! - **Artworks kept there** (`artwork_saved`): after its saves, an artwork is written there as one
//!   package, the file Share makes, once its saves stop for a while (and at most so long after the
//!   first): from the artwork while it is open and nothing is under way on it, or from its folder
//!   once it is left. It is named after the artwork. A file there that changed since it was
//!   written (another device wrote it), or that was never the app's, is never written over: the
//!   artwork goes under a name of its own beside it.
//! - **Videos put there** (`video_exported`): a time-lapse video exported is copied there too.
//! - **Packages brought in** (`gallery_opened`): as the gallery opens, a package there that changed
//!   since the app last wrote or read it comes into the gallery: over the app's copy if that didn't
//!   change meanwhile (keeping its place), else beside it as a copy; one the app never had, or
//!   deleted before it changed there, comes in as a new artwork.
//!
//! Nothing there is ever taken away: the access has no way to. What was written and read is noted
//! with the plugin ([`Records`]), read leniently and written whole. A folder that can't be reached
//! (gone, or the app's permission lost) is told once, until it can be again, and the app goes on as
//! usual.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use crate::CanvasSession;
use crate::gallery::ArtworkStore;
use crate::hooks::Hooks;
use crate::plugin_run::Hook;
use crate::plugins::Plugins;

/// The extension of the packages written there: the app's own, as Share names them.
pub(crate) const PACKAGE_EXTENSION: &str = "procraft";
/// The extensions of the packages brought in from there: the app's own, and the layered package
/// the app also reads.
const BROUGHT_EXTENSIONS: [&str; 2] = [PACKAGE_EXTENSION, procraft_engine::native::EXTENSION];
/// What the apps say, once, when a plugin's folder can't be reached, until it can be again.
pub const FOLDER_UNREACHABLE: &str =
    "A plugin's folder can't be reached. Pick the folder again on the plugin's card to keep using it.";
/// What a copy brought in beside an artwork changed on both sides is named after it with.
const COPY_NAMED: &str = "from folder";
/// What a time-lapse video put there is named after its artwork with.
const VIDEO_NAMED: &str = "time-lapse";
/// How many times a package is tried again when the artwork was busy each time it was looked at.
const MOST_TRIES: u32 = 30;

/// A file in a folder, as the app's access lists it: its name, size in bytes, and when it was last
/// changed (milliseconds since 1970).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FolderEntry {
    pub name: String,
    pub size: u64,
    pub modified_ms: i64,
}

/// Why the app's access to a folder couldn't do something.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum FolderError {
    /// The folder can't be reached: it is gone, or the app's permission to it was lost.
    #[error("the folder can't be reached: {reason}")]
    Unreachable { reason: String },
    /// Something else went wrong (a write cut short, no room): the folder is as it was.
    #[error("{reason}")]
    Failed { reason: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for FolderError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> FolderError {
        FolderError::Failed { reason: e.reason }
    }
}

/// The app's way into the folders picked for plugins' `folder` controls, each at a `location` the
/// app's folder picker gave (a document tree's address, a path, a bookmark): only what the system's
/// folders need, and nothing that takes a file away.
#[uniffi::export(with_foreign)]
pub trait FolderAccess: Send + Sync {
    /// The files directly in the folder, its folders and hidden files left out.
    fn list(&self, location: String) -> Result<Vec<FolderEntry>, FolderError>;
    /// The local file `from` written into the folder as `name`, whole: written beside it first and
    /// put in its place only once all of it is there, so a write cut short leaves what was there
    /// before. The file as it is now.
    fn write_whole(&self, location: String, name: String, from: String) -> Result<FolderEntry, FolderError>;
    /// The folder's file `name` copied whole to the local file `to`.
    fn read_whole(&self, location: String, name: String, to: String) -> Result<(), FolderError>;
}

/// What bringing a folder's packages into the gallery came to (see [`Plugins::gallery_opened`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct FolderBroughtIn {
    /// Artworks new to the gallery.
    pub added: u32,
    /// The gallery's own copies brought up to date, each keeping its place.
    pub updated: u32,
    /// Copies brought in beside an artwork changed both here and there.
    pub copies: u32,
    /// What to tell, once: the folder can't be reached.
    pub notice: Option<String>,
}

impl FolderBroughtIn {
    fn changed(&self) -> bool {
        self.added + self.updated + self.copies > 0
    }
}

/// A folder a plugin names: whose (its `folder` control kept under `key`), where it is, and where
/// what was written there and read from it is noted.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Target {
    pub(crate) key: String,
    pub(crate) location: String,
    pub(crate) records: Option<PathBuf>,
}

/// A file's size and when it was last changed (a folder's file to the millisecond, as the systems'
/// folders give it; an artwork's own to the nanosecond, so two saves a moment apart differ): as it
/// was last seen, a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: i64,
}

impl Stamp {
    fn of(entry: &FolderEntry) -> Stamp {
        Stamp { size: entry.size, modified: entry.modified_ms }
    }

    /// The artwork in `dir`'s, by its own file: what a save changes. None while it has none.
    fn of_artwork(dir: &Path) -> Option<Stamp> {
        let meta = std::fs::metadata(dir.join("artwork.json")).ok()?;
        let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(Stamp { size: meta.len(), modified: i64::try_from(modified.as_nanos()).unwrap_or(i64::MAX) })
    }
}

/// What the app last knew of one of a folder's files.
#[derive(Clone, Debug, Default, PartialEq)]
struct Record {
    /// The file as the app last wrote or read it.
    stamp: Option<Stamp>,
    /// The artwork it is a copy of, by its folder's name, and that artwork's stamp when the two
    /// were the same: none for a file that is no artwork's now (it comes in again if it changes).
    artwork: Option<String>,
    artwork_stamp: Option<Stamp>,
    /// A time-lapse video the app put there.
    video: bool,
}

/// What the app wrote to a folder and read from it, by file name, kept with the plugin beside its
/// folder control's choice. Another folder chosen starts afresh.
#[derive(Clone, Debug, Default, PartialEq)]
struct Records {
    location: String,
    files: BTreeMap<String, Record>,
}

impl Records {
    /// Those kept for `target`, read leniently: a field that doesn't read is left out, an entry
    /// with nothing that reads goes, and a file that doesn't read at all is none (every file in the
    /// folder is then the folder's own, so nothing there is written over).
    fn read(target: &Target) -> Records {
        let fresh = Records { location: target.location.clone(), files: BTreeMap::new() };
        let Some(path) = &target.records else { return fresh };
        let Some(json) = std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        else {
            return fresh;
        };
        if json.get("location").and_then(serde_json::Value::as_str) != Some(target.location.as_str()) {
            return fresh;
        }
        let mut files = BTreeMap::new();
        let entries = json.get("files").and_then(serde_json::Value::as_object);
        for (name, entry) in entries.into_iter().flatten() {
            let number = |k: &str| entry.get(k).and_then(serde_json::Value::as_i64);
            let stamp = |size: &str, modified: &str| {
                let size = number(size).and_then(|s| u64::try_from(s).ok())?;
                Some(Stamp { size, modified: number(modified)? })
            };
            let record = Record {
                stamp: stamp("size", "modified_ms"),
                artwork: entry.get("artwork").and_then(serde_json::Value::as_str).map(str::to_owned),
                artwork_stamp: stamp("artwork_size", "artwork_modified_ns"),
                video: entry.get("video").and_then(serde_json::Value::as_bool).unwrap_or(false),
            };
            if record.stamp.is_some() || record.artwork.is_some() {
                files.insert(name.clone(), record);
            }
        }
        Records { location: target.location.clone(), files }
    }

    /// Kept for `target`, whole (written beside, then put in place). False if it couldn't be.
    fn write(&self, target: &Target) -> bool {
        let Some(path) = &target.records else { return false };
        let files: serde_json::Map<String, serde_json::Value> = self
            .files
            .iter()
            .map(|(name, r)| {
                let mut entry = serde_json::Map::new();
                if let Some(s) = r.stamp {
                    entry.insert("size".into(), s.size.into());
                    entry.insert("modified_ms".into(), s.modified.into());
                }
                if let Some(a) = &r.artwork {
                    entry.insert("artwork".into(), a.clone().into());
                }
                if let Some(s) = r.artwork_stamp {
                    entry.insert("artwork_size".into(), s.size.into());
                    entry.insert("artwork_modified_ns".into(), s.modified.into());
                }
                if r.video {
                    entry.insert("video".into(), true.into());
                }
                (name.clone(), serde_json::Value::Object(entry))
            })
            .collect();
        let text = serde_json::json!({ "location": self.location, "files": files }).to_string();
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let written = crate::project::write_atomically(path, |p| {
            crate::storage::write(p, &text).map_err(|e| crate::project::file_error("noting the folder", e))
        });
        if let Err(e) = &written {
            log::warn!("{e}");
        }
        written.is_ok()
    }

    /// Whether a file named `name` is taken, in `listing` or here (names alike but for case are
    /// one, as most folders take them).
    fn taken(&self, listing: &[FolderEntry], name: &str) -> bool {
        let same = |other: &str| other.eq_ignore_ascii_case(name);
        listing.iter().any(|e| same(&e.name)) || self.files.keys().any(|k| same(k))
    }

    /// Whether `entry` is as the app last wrote or read it.
    fn unchanged(&self, entry: &FolderEntry) -> bool {
        self.files.get(&entry.name).and_then(|r| r.stamp) == Some(Stamp::of(entry))
    }

    /// The file named `base` with `ext`, or with a number after it from 2 (`sep` between them), that
    /// no file there or here has.
    fn fresh_name(&self, listing: &[FolderEntry], base: &str, sep: &str, ext: &str) -> String {
        let first = format!("{base}.{ext}");
        if !self.taken(listing, &first) {
            return first;
        }
        (2u32..)
            .map(|n| format!("{base}{sep}{n}.{ext}"))
            .find(|name| !self.taken(listing, name))
            .unwrap_or_else(|| format!("{base}.{ext}"))
    }
}

/// Whether `file` is named `base` with `ext`, or with a number after it (`sep` between).
fn named_after(file: &str, base: &str, sep: &str, ext: &str) -> bool {
    let Some(stem) = file.strip_suffix(&format!(".{ext}")) else { return false };
    stem == base
        || stem
            .strip_prefix(base)
            .and_then(|rest| rest.strip_prefix(sep))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The artwork in `dir`'s name, as its file says.
fn artwork_name(dir: &Path) -> String {
    let json: Option<serde_json::Value> =
        std::fs::read(dir.join("artwork.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
    json.as_ref()
        .and_then(|j| j.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Untitled Artwork")
        .to_owned()
}

fn folder_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The seed a package of the artwork in `dir` makes its layers' ids from: the same each time, so
/// an artwork that didn't change is written the same.
fn seed_of(dir: &Path) -> u64 {
    folder_name(dir).bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// The time-lapse of the artwork in `dir` as a package keeps it (its videos as made so far), if
/// it has one.
pub(crate) fn time_lapse_of(dir: &Path) -> Option<crate::project::PackageTimeLapse> {
    dir.join("timelapse")
        .is_dir()
        .then(|| crate::timelapse::TimeLapse::new(dir.to_string_lossy().into_owned()).for_package())
}

/// What makes the packages written to a folder: the app's own, or a test's.
pub(crate) trait MakesPackages: Send + Sync {
    /// The artwork in `dir` (open as `session`, if it still is) as a package at `to`: false if
    /// something under way on it keeps it from being made now (it is tried again before long).
    fn make(&self, session: Option<Arc<CanvasSession>>, dir: &Path, to: &Path) -> Result<bool, String>;
}

/// The packages Share makes: from the artwork while it is open and nothing is under way on it,
/// else from its folder, opened for this alone.
pub(crate) struct SharePackages;

impl MakesPackages for SharePackages {
    fn make(&self, session: Option<Arc<CanvasSession>>, dir: &Path, to: &Path) -> Result<bool, String> {
        let (seed, time_lapse, path) = (seed_of(dir), time_lapse_of(dir), to.to_string_lossy().into_owned());
        if let Some(s) = session.filter(|s| s.shows_canvas()) {
            return s.export_package_when(path, seed, time_lapse, true).map_err(|e| e.to_string());
        }
        let s = CanvasSession::open(dir.to_string_lossy().into_owned()).map_err(|e| e.to_string())?;
        if !s.prepare_offscreen() {
            return Err("there is no GPU to make the package with".to_owned());
        }
        let made = s.export_package_when(path, seed, time_lapse, false).map_err(|e| e.to_string());
        s.release_canvas();
        made
    }
}

/// How long the work waits: after an artwork's last save, after it is left, at most after its
/// first save waiting, and before an artwork that was busy is looked at again.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Timing {
    pub(crate) settle: Duration,
    pub(crate) left: Duration,
    pub(crate) longest: Duration,
    pub(crate) again: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            settle: Duration::from_secs(10),
            left: Duration::from_secs(5),
            longest: Duration::from_secs(120),
            again: Duration::from_secs(2),
        }
    }
}

/// Work waiting for a folder.
enum Work {
    /// The artwork in `dir`, open as `session` while it is, saved since it was last written there:
    /// waiting since `first`, tried `tries` times.
    Artwork { dir: PathBuf, session: Weak<CanvasSession>, first: Instant, tries: u32 },
    /// A time-lapse video at `path`, of the artwork named `name`, to put in `target`.
    Video { path: PathBuf, name: String, target: Target },
}

struct Job {
    work: Work,
    due: Instant,
}

#[derive(Default)]
struct Queue {
    jobs: Vec<Job>,
    running: bool,
    worker: bool,
}

/// What the app has told of a folder that can't be reached.
#[derive(Default)]
struct Told {
    /// Told since a folder was last reached.
    told: bool,
    /// Not taken by the app yet.
    untold: Option<String>,
}

/// What a piece of work came to.
#[derive(Debug, PartialEq)]
enum Outcome {
    Done,
    /// Not now: something under way on the artwork.
    NotNow,
    Failed,
}

/// The folders plugins name, and the work for them (see the module's words). One for the app's
/// plugins.
pub(crate) struct Folders {
    plugins: Weak<Plugins>,
    hooks: Arc<Hooks>,
    access: Mutex<Option<Arc<dyn FolderAccess>>>,
    /// The app's own folder where packages are made before they go into a folder, and those read
    /// out are kept until they are in.
    work: Mutex<Option<PathBuf>>,
    maker: Mutex<Arc<dyn MakesPackages>>,
    timing: Mutex<Timing>,
    queue: Mutex<Queue>,
    ready: Condvar,
    /// Held through each piece of work on a folder, one at a time.
    working: Mutex<()>,
    told: Mutex<Told>,
}

impl std::fmt::Debug for Folders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Folders").finish_non_exhaustive()
    }
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Folders {
    pub(crate) fn new(plugins: Weak<Plugins>, hooks: Arc<Hooks>) -> Arc<Folders> {
        Arc::new(Folders {
            plugins,
            hooks,
            access: Mutex::new(None),
            work: Mutex::new(None),
            maker: Mutex::new(Arc::new(SharePackages)),
            timing: Mutex::new(Timing::default()),
            queue: Mutex::new(Queue::default()),
            ready: Condvar::new(),
            working: Mutex::new(()),
            told: Mutex::new(Told::default()),
        })
    }

    pub(crate) fn use_access(&self, access: Arc<dyn FolderAccess>, work: PathBuf) {
        *locked(&self.access) = Some(access);
        *locked(&self.work) = Some(work);
    }

    #[cfg(test)]
    pub(crate) fn use_maker(&self, maker: Arc<dyn MakesPackages>, timing: Timing) {
        *locked(&self.maker) = maker;
        *locked(&self.timing) = timing;
    }

    /// The folder the first plugin by id on `hook` names, if it names one picked.
    fn target(&self, hook: Hook) -> Option<Target> {
        let (id, at) = self.hooks.folder(hook)?;
        self.plugins.upgrade()?.folder_target(&id, at)
    }

    /// What to tell once, not yet taken: a folder that can't be reached.
    pub(crate) fn take_notice(&self) -> Option<String> {
        locked(&self.told).untold.take()
    }

    /// A folder was reached: the next time one can't be, it is told again.
    fn reached(&self) {
        let mut told = locked(&self.told);
        told.told = false;
        told.untold = None;
    }

    /// A folder couldn't be reached, for `why`: told once, until one is reached again.
    fn unreachable(&self, why: &str) {
        log::warn!("a plugin's folder: {why}");
        let mut told = locked(&self.told);
        if !std::mem::replace(&mut told.told, true) {
            told.untold = Some(FOLDER_UNREACHABLE.to_owned());
        }
    }

    /// Another folder was picked, or one cleared: its own is told afresh.
    pub(crate) fn chosen_again(&self) {
        self.reached();
    }

    /// `result` of the access, any failure noted: an unreachable folder told once, any other logged.
    fn noted<T>(&self, result: Result<T, FolderError>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(FolderError::Unreachable { reason }) => {
                self.unreachable(&reason);
                None
            }
            Err(FolderError::Failed { reason }) => {
                log::warn!("a plugin's folder: {reason}");
                None
            }
        }
    }

    /// The access and the work folder, if the app gave them.
    fn ways(&self) -> Option<(Arc<dyn FolderAccess>, PathBuf)> {
        let access = locked(&self.access).clone()?;
        let work = locked(&self.work).clone()?;
        let _ = std::fs::create_dir_all(&work);
        Some((access, work))
    }

    /// The folder `target` as listed, the folder reached or told.
    fn list(&self, access: &dyn FolderAccess, target: &Target) -> Option<Vec<FolderEntry>> {
        let listed = self.noted(access.list(target.location.clone()))?;
        self.reached();
        Some(listed)
    }

    /// The artwork in `dir`, open as `session`, was saved: written to the folder a plugin keeps
    /// artworks in once its saves stop for a while. With none on the hook, nothing: not even a
    /// plugin asked (which folder is asked when the work is done, on a thread of its own).
    pub(crate) fn artwork_saved(self: &Arc<Self>, session: Weak<CanvasSession>, dir: &Path) {
        if !self.hooks.listens(Hook::ArtworkSaved) {
            return;
        }
        let timing = *locked(&self.timing);
        let now = Instant::now();
        let mut queue = locked(&self.queue);
        let waiting = queue.jobs.iter_mut().find(|j| matches!(&j.work, Work::Artwork { dir: d, .. } if d == dir));
        match waiting {
            Some(job) => {
                if let Work::Artwork { session: s, first, .. } = &mut job.work {
                    *s = session;
                    job.due = (now + timing.settle).min(*first + timing.longest);
                }
            }
            None => queue.jobs.push(Job {
                work: Work::Artwork { dir: dir.to_path_buf(), session, first: now, tries: 0 },
                due: now + timing.settle,
            }),
        }
        self.wake(queue);
    }

    /// The artwork in `dir` was left, saved: what waits for it goes before long.
    pub(crate) fn artwork_left(self: &Arc<Self>, dir: &Path) {
        let left = locked(&self.timing).left;
        let mut queue = locked(&self.queue);
        let mut any = false;
        for job in &mut queue.jobs {
            if matches!(&job.work, Work::Artwork { dir: d, .. } if d == dir) {
                job.due = job.due.min(Instant::now() + left);
                any = true;
            }
        }
        if any {
            self.wake(queue);
        }
    }

    /// The time-lapse video at `path`, of the artwork named `name`, was exported: copied to the
    /// folder a plugin puts them in, if one does. False if none does.
    pub(crate) fn video_exported(self: &Arc<Self>, path: &Path, name: &str) -> bool {
        if !self.hooks.listens(Hook::VideoExported) {
            return false;
        }
        let Some(target) = self.target(Hook::VideoExported) else { return false };
        let mut queue = locked(&self.queue);
        let work = Work::Video { path: path.to_path_buf(), name: name.to_owned(), target };
        queue.jobs.push(Job { work, due: Instant::now() });
        self.wake(queue);
        true
    }

    /// The work changed: the thread that does it told, started if it isn't running.
    fn wake(self: &Arc<Self>, mut queue: MutexGuard<'_, Queue>) {
        if !queue.worker {
            let me = Arc::downgrade(self);
            // (a package made goes as deep into the core as an artwork opened)
            let started =
                std::thread::Builder::new().name("folders".into()).stack_size(8 << 20).spawn(move || work_on(&me));
            match started {
                Ok(_) => queue.worker = true,
                Err(e) => log::error!("a plugin's folder: {e}"),
            }
        }
        drop(queue);
        self.ready.notify_all();
    }

    /// Waits until no work waits or runs, at most `most`; true if none does.
    #[cfg(test)]
    pub(crate) fn wait_done(&self, most: Duration) -> bool {
        let until = Instant::now() + most;
        loop {
            {
                let queue = locked(&self.queue);
                if queue.jobs.is_empty() && !queue.running {
                    return true;
                }
            }
            if Instant::now() >= until {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Does `job`; one to wait for again, if it has to.
    fn run(&self, job: Job) -> Option<Job> {
        match job.work {
            Work::Artwork { dir, session, first, tries } => {
                let target = self.target(Hook::ArtworkSaved)?;
                let outcome = self.keep_artwork(&target, &dir, session.upgrade());
                (outcome == Outcome::NotNow && tries < MOST_TRIES).then(|| Job {
                    work: Work::Artwork { dir, session, first, tries: tries + 1 },
                    due: Instant::now() + locked(&self.timing).again,
                })
            }
            Work::Video { path, name, target } => {
                self.keep_video(&target, &path, &name);
                None
            }
        }
    }

    /// The artwork in `dir` (open as `session`, if it still is) written to `target` as a package.
    fn keep_artwork(&self, target: &Target, dir: &Path, session: Option<Arc<CanvasSession>>) -> Outcome {
        let Some((access, work)) = self.ways() else { return Outcome::Failed };
        // what it is as saved, before the package is made: one saved again meanwhile counts as
        // changed here, so it is never taken for the package's
        let Some(stamp) = Stamp::of_artwork(dir) else { return Outcome::Done };
        let _working = locked(&self.working);
        let Some(listing) = self.list(&*access, target) else { return Outcome::Failed };
        let mut records = Records::read(target);
        let id = folder_name(dir);
        let base = crate::project::export_file_name(artwork_name(dir), true);
        let file = Self::file_for(&mut records, &listing, &id, &base);
        let staged = work.join(format!("{id}.{PACKAGE_EXTENSION}"));
        let maker = locked(&self.maker).clone();
        // (made from the artwork open, one that changed as it was read is tried again)
        let live = session.as_ref().is_some_and(|s| s.shows_canvas());
        match maker.make(session, dir, &staged) {
            Ok(true) => {}
            Ok(false) => {
                let _ = std::fs::remove_file(&staged);
                return Outcome::NotNow;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&staged);
                log::warn!("a package for a plugin's folder: {e}");
                return if live { Outcome::NotNow } else { Outcome::Failed };
            }
        }
        let written = access.write_whole(target.location.clone(), file.clone(), staged.to_string_lossy().into_owned());
        let _ = std::fs::remove_file(&staged);
        let Some(entry) = self.noted(written) else { return Outcome::Failed };
        log::info!("{} kept in the folder of {}", entry.name, target.key);
        let record =
            Record { stamp: Some(Stamp::of(&entry)), artwork: Some(id), artwork_stamp: Some(stamp), video: false };
        records.files.insert(entry.name, record);
        records.write(target);
        Outcome::Done
    }

    /// The name the artwork `id` (named `base` for a file) goes under in a folder listed as
    /// `listing`: its own while it is still named after it and unchanged there since it was
    /// written, else one no file there or noted has (its own then let go, and what is there now no
    /// artwork's: never written over, and brought in if it changes).
    fn file_for(records: &mut Records, listing: &[FolderEntry], id: &str, base: &str) -> String {
        let mine =
            records.files.iter().find(|(_, r)| !r.video && r.artwork.as_deref() == Some(id)).map(|(n, _)| n.clone());
        if let Some(mine) = mine {
            let there = listing.iter().find(|e| e.name == mine);
            let unchanged = there.is_none_or(|e| records.unchanged(e));
            if unchanged && named_after(&mine, base, "_", PACKAGE_EXTENSION) {
                return mine;
            }
            if let Some(r) = records.files.get_mut(&mine) {
                r.artwork = None;
                r.artwork_stamp = None;
            }
        }
        records.fresh_name(listing, base, "_", PACKAGE_EXTENSION)
    }

    /// The video at `path`, of the artwork named `name`, put in `target`: over the one put there
    /// before under that name if it is as the app put it, else under a name of its own.
    fn keep_video(&self, target: &Target, path: &Path, name: &str) {
        let Some((access, _)) = self.ways() else { return };
        let _working = locked(&self.working);
        let Some(listing) = self.list(&*access, target) else { return };
        let mut records = Records::read(target);
        let ext = path.extension().map_or_else(|| "mp4".to_owned(), |e| e.to_string_lossy().to_lowercase());
        let base = format!("{} {VIDEO_NAMED}", crate::project::export_file_name(name.to_owned(), false));
        let first = format!("{base}.{ext}");
        let ours = records.files.get(&first).is_some_and(|r| r.video)
            && listing.iter().find(|e| e.name == first).is_none_or(|e| records.unchanged(e));
        let file = if ours || !records.taken(&listing, &first) {
            first
        } else {
            records.fresh_name(&listing, &base, " ", &ext)
        };
        let written = access.write_whole(target.location.clone(), file, path.to_string_lossy().into_owned());
        if let Some(entry) = self.noted(written) {
            let record = Record { stamp: Some(Stamp::of(&entry)), artwork: None, artwork_stamp: None, video: true };
            records.files.insert(entry.name, record);
            records.write(target);
        }
    }

    /// The gallery opened, its artworks in `store`: the packages in the folder a plugin brings in
    /// from that changed since the app last wrote or read them, or that it never had, come in (see
    /// the module's words). A long call.
    pub(crate) fn gallery_opened(&self, store: &ArtworkStore) -> FolderBroughtIn {
        let mut brought = FolderBroughtIn::default();
        if self.hooks.listens(Hook::GalleryOpened)
            && let Some(target) = self.target(Hook::GalleryOpened)
            && let Some((access, work)) = self.ways()
        {
            brought = self.bring_in(&*access, &work, &target, store);
        }
        brought.notice = self.take_notice();
        brought
    }

    fn bring_in(
        &self,
        access: &dyn FolderAccess,
        work: &Path,
        target: &Target,
        store: &ArtworkStore,
    ) -> FolderBroughtIn {
        let mut brought = FolderBroughtIn::default();
        let _working = locked(&self.working);
        let Some(listing) = self.list(access, target) else { return brought };
        let mut records = Records::read(target);
        let root = PathBuf::from(store.root());
        let packages = listing.iter().filter(|e| {
            let ext = Path::new(&e.name).extension().map(|x| x.to_string_lossy().to_lowercase());
            !e.name.starts_with('.') && ext.is_some_and(|x| BROUGHT_EXTENSIONS.contains(&x.as_str()))
        });
        for entry in packages {
            if records.unchanged(entry) {
                continue;
            }
            let record = records.files.get(&entry.name).cloned().unwrap_or_default();
            let local = record.artwork.as_deref().and_then(|id| find_artwork(&root, id));
            // the gallery's copy, if it is as it was when the two were the same, and isn't open
            let unchanged = local.as_deref().is_some_and(|d| {
                record.artwork_stamp.is_some() && Stamp::of_artwork(d) == record.artwork_stamp && !is_open(d)
            });
            let ext = Path::new(&entry.name).extension().map_or_else(String::new, |x| x.to_string_lossy().into_owned());
            let staged = work.join(format!("incoming.{ext}"));
            let read =
                access.read_whole(target.location.clone(), entry.name.clone(), staged.to_string_lossy().into_owned());
            match read {
                Ok(()) => {}
                Err(FolderError::Unreachable { reason }) => {
                    self.unreachable(&reason);
                    let _ = std::fs::remove_file(&staged);
                    break;
                }
                Err(FolderError::Failed { reason }) => {
                    log::warn!("reading {} from a plugin's folder: {reason}", entry.name);
                    let _ = std::fs::remove_file(&staged);
                    continue;
                }
            }
            let into = local.as_deref().and_then(Path::parent).map_or_else(|| root.clone(), Path::to_path_buf);
            let made = match (&local, unchanged) {
                // newer there, the same here: it comes in over the gallery's copy, in its place
                (Some(old), true) => bring_package(store, &staged, &into, None).map(|new| {
                    store.put_in_place_of(old, &new);
                    let _ = std::fs::remove_dir_all(old);
                    brought.updated += 1;
                    Record {
                        stamp: Some(Stamp::of(entry)),
                        artwork: Some(folder_name(&new)),
                        artwork_stamp: Stamp::of_artwork(&new),
                        video: false,
                    }
                }),
                // changed on both sides: kept both, what came as a copy beside the gallery's
                (Some(_), false) => bring_package(store, &staged, &into, Some(COPY_NAMED)).map(|_| {
                    brought.copies += 1;
                    Record { stamp: Some(Stamp::of(entry)), ..record.clone() }
                }),
                // one the gallery doesn't have
                (None, _) => bring_package(store, &staged, &root, None).map(|new| {
                    brought.added += 1;
                    Record {
                        stamp: Some(Stamp::of(entry)),
                        artwork: Some(folder_name(&new)),
                        artwork_stamp: Stamp::of_artwork(&new),
                        video: false,
                    }
                }),
            };
            let _ = std::fs::remove_file(&staged);
            match made {
                Ok(record) => {
                    records.files.insert(entry.name.clone(), record);
                }
                Err(e) => log::warn!("bringing {} in from a plugin's folder: {e}", entry.name),
            }
        }
        if brought.changed() || records != Records::read(target) {
            records.write(target);
        }
        brought
    }
}

/// The thread doing the folders' work, while there is any: each piece once it is due, the earliest
/// first; it ends a second after the last.
fn work_on(me: &Weak<Folders>) {
    loop {
        let Some(folders) = me.upgrade() else { return };
        let job = {
            let mut queue = locked(&folders.queue);
            loop {
                let now = Instant::now();
                let due = queue.jobs.iter().enumerate().min_by_key(|(_, j)| j.due).map(|(i, j)| (i, j.due));
                match due {
                    Some((i, at)) if at <= now => {
                        queue.running = true;
                        break Some(queue.jobs.swap_remove(i));
                    }
                    Some((_, at)) => {
                        let wait = at.saturating_duration_since(now).min(Duration::from_secs(1));
                        queue = folders.ready.wait_timeout(queue, wait).unwrap_or_else(PoisonError::into_inner).0;
                    }
                    None => {
                        let (q, timeout) = folders
                            .ready
                            .wait_timeout(queue, Duration::from_secs(1))
                            .unwrap_or_else(PoisonError::into_inner);
                        queue = q;
                        if timeout.timed_out() && queue.jobs.is_empty() {
                            queue.worker = false;
                            break None;
                        }
                    }
                }
            }
        };
        let Some(job) = job else { return };
        let again = folders.run(job);
        let mut queue = locked(&folders.queue);
        queue.running = false;
        if let Some(again) = again {
            // saved again meanwhile: that one waits instead, as soon as this would have
            let newer = queue.jobs.iter_mut().find(|j| match (&j.work, &again.work) {
                (Work::Artwork { dir: a, .. }, Work::Artwork { dir: b, .. }) => a == b,
                _ => false,
            });
            match newer {
                Some(newer) => newer.due = newer.due.min(again.due),
                None => queue.jobs.push(again),
            }
        }
        drop(queue);
        folders.ready.notify_all();
    }
}

/// The artwork whose folder is named `id` in the gallery at `root`, or in one of its stacks.
fn find_artwork(root: &Path, id: &str) -> Option<PathBuf> {
    if id.is_empty() || id.contains(['/', '\\']) || id == "." || id == ".." {
        return None;
    }
    let top = root.join(id);
    if top.join("artwork.json").is_file() {
        return Some(top);
    }
    std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path().join(id))
        .find(|d| d.join("artwork.json").is_file())
}

/// The artworks open now, by their folders: an open one is never brought in over.
static OPEN: Mutex<Vec<(PathBuf, Weak<CanvasSession>)>> = Mutex::new(Vec::new());

/// The artwork in `dir` is open as `session` (kept saved there).
pub(crate) fn opened(dir: &Path, session: Weak<CanvasSession>) {
    let mut open = locked(&OPEN);
    open.retain(|(d, s)| s.strong_count() > 0 && d != dir);
    open.push((dir.to_path_buf(), session));
}

/// Whether the artwork in `dir` is open: kept saved there by a session that is still on.
fn is_open(dir: &Path) -> bool {
    locked(&OPEN).iter().any(|(d, s)| d == dir && s.upgrade().is_some_and(|s| !s.closed()))
}

/// The package at `source` brought into the gallery `store` as a new artwork in `into` (the
/// gallery's folder or a stack's), named as it says (with `copy` after its name, if given): its
/// folder, or why not (none then left half made).
fn bring_package(store: &ArtworkStore, source: &Path, into: &Path, copy: Option<&str>) -> Result<PathBuf, String> {
    let folder = (into != Path::new(&store.root())).then(|| into.to_string_lossy().into_owned());
    let dir = PathBuf::from(store.new_folder(folder));
    let importing = crate::gallery::Importing::begin(&dir);
    match import_package(source, &dir, copy) {
        Ok(()) => {
            importing.done();
            Ok(dir)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            Err(e)
        }
    }
}

/// The layered package at `source` made into the artwork folder `dir`, as the gallery's Import makes
/// it: its layers and settings, its time-lapse's videos as they are (when their frames can be
/// counted), and its preview as the thumbnail until it is opened.
fn import_package(source: &Path, dir: &Path, copy: Option<&str>) -> Result<(), String> {
    let path = source.to_string_lossy().into_owned();
    let s = CanvasSession::open_package(path.clone()).map_err(|e| e.to_string())?;
    if let Some(copy) = copy {
        let name = s.name();
        s.set_name(format!("{name} ({copy})"));
    }
    let at = dir.to_string_lossy().into_owned();
    s.save(at.clone()).map_err(|e| e.to_string())?;
    let segments = dir.join("incoming-videos");
    if let Ok(Some(t)) = crate::project::package_time_lapse(path.clone(), segments.to_string_lossy().into_owned())
        && !crate::timelapse::TimeLapse::new(at.clone()).take_clips(t)
    {
        log::warn!("a package's time-lapse couldn't be kept as it came: left out");
    }
    let _ = std::fs::remove_dir_all(&segments);
    crate::project::keep_package_thumbnail(path, at);
    Ok(())
}

/// A folder's files as plain files on the device: the desktop's way into a folder, and the tests'.
pub struct PlainFolderAccess;

impl PlainFolderAccess {
    fn entry(path: &Path) -> Option<FolderEntry> {
        let meta = std::fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(FolderEntry {
            name: path.file_name()?.to_string_lossy().into_owned(),
            size: meta.len(),
            modified_ms: i64::try_from(modified.as_millis()).unwrap_or(i64::MAX),
        })
    }

    /// Whether `name` is one file's name in a folder: nothing that reaches out of it.
    fn plain(name: &str) -> bool {
        !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
    }

    /// `from` copied to `to` whole: beside it first, then moved into its place.
    fn copy_whole(from: &Path, to: &Path) -> Result<(), FolderError> {
        let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let beside = to.with_file_name(format!(".{name}.{}.partial", std::process::id()));
        let copied = std::fs::copy(from, &beside).and_then(|_| std::fs::rename(&beside, to));
        copied.map_err(|e| {
            let _ = std::fs::remove_file(&beside);
            FolderError::Failed { reason: e.to_string() }
        })
    }

    fn folder(location: &str) -> Result<PathBuf, FolderError> {
        let folder = PathBuf::from(location);
        if folder.is_dir() {
            Ok(folder)
        } else {
            Err(FolderError::Unreachable { reason: format!("{location} isn't there") })
        }
    }
}

impl FolderAccess for PlainFolderAccess {
    fn list(&self, location: String) -> Result<Vec<FolderEntry>, FolderError> {
        let folder = Self::folder(&location)?;
        let entries = std::fs::read_dir(&folder).map_err(|e| FolderError::Unreachable { reason: e.to_string() })?;
        Ok(entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
            .filter_map(|p| Self::entry(&p))
            .collect())
    }

    fn write_whole(&self, location: String, name: String, from: String) -> Result<FolderEntry, FolderError> {
        let folder = Self::folder(&location)?;
        if !Self::plain(&name) {
            return Err(FolderError::Failed { reason: format!("{name} isn't a file's name") });
        }
        let to = folder.join(&name);
        Self::copy_whole(Path::new(&from), &to)?;
        Self::entry(&to).ok_or_else(|| FolderError::Failed { reason: format!("{name} wasn't written") })
    }

    fn read_whole(&self, location: String, name: String, to: String) -> Result<(), FolderError> {
        let folder = Self::folder(&location)?;
        if !Self::plain(&name) {
            return Err(FolderError::Failed { reason: format!("{name} isn't a file's name") });
        }
        Self::copy_whole(&folder.join(name), Path::new(&to))
    }
}

#[cfg(test)]
mod tests;
