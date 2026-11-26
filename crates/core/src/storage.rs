//! Room on the device for what the app keeps. A write of the user's work that finds the storage
//! full (no space left, a quota met, the disk full) fails soft: the artwork stays open and paints
//! from memory, its last save stays whole (a save is written beside its files and put in place only
//! once all of it is written), the time-lapse pauses, and the apps say so once, as [`NO_ROOM`]
//! puts it, whichever write meets it first, until a save of the artwork works again.
//!
//! The core's files are written through [`write`] and [`create`], which note a full storage as
//! they meet it. In tests they also meet the full disks a test makes ([`FullDisk`]): so many bytes
//! taken, the rest refused as a real disk refuses it, the file left partly written.

use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// What the apps say, once, when the storage is full.
pub const NO_ROOM: &str = "Not enough storage: free some space to keep saving";

/// The system's numbers for a full storage: no space left, and a quota met.
#[cfg(windows)]
const FULL_CODES: [i32; 2] = [112, 39]; // ERROR_DISK_FULL, ERROR_HANDLE_DISK_FULL
#[cfg(target_vendor = "apple")]
const FULL_CODES: [i32; 2] = [28, 69]; // ENOSPC, EDQUOT
#[cfg(not(any(windows, target_vendor = "apple")))]
const FULL_CODES: [i32; 2] = [28, 122]; // ENOSPC, EDQUOT

/// Where the storage stands: found full since a save last worked there, and whether that was told.
struct Room {
    full: bool,
    told: bool,
}

static ROOM: Mutex<Room> = Mutex::new(Room { full: false, told: false });

/// How many writes have found the storage full since the app started.
static FULL_WRITES: AtomicU64 = AtomicU64::new(0);

fn room() -> MutexGuard<'static, Room> {
    ROOM.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `e` is the storage being full.
pub(crate) fn is_full(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded)
        || e.raw_os_error().is_some_and(|code| FULL_CODES.contains(&code))
}

/// Whether `reason`, an error made into text (a failed save's), says the storage was full.
pub(crate) fn says_full(reason: &str) -> bool {
    let reason = reason.to_lowercase();
    FULL_CODES.iter().any(|code| reason.contains(&format!("(os error {code})")))
        || ["no space left", "no storage space", "not enough space", "quota exceeded", "enospc"]
            .iter()
            .any(|words| reason.contains(words))
}

/// `e`, noted if it is the storage being full.
pub(crate) fn noted(e: io::Error) -> io::Error {
    if is_full(&e) {
        FULL_WRITES.fetch_add(1, Ordering::Relaxed);
        found_full();
    }
    e
}

/// A write found the storage full.
pub(crate) fn found_full() {
    room().full = true;
}

/// Whether a write has found the storage full since a save last worked after one did.
pub(crate) fn full() -> bool {
    room().full
}

/// A save worked after one found the storage full: there is room again, and the next time it fills
/// is told again.
pub(crate) fn room_again() {
    *room() = Room { full: false, told: false };
}

/// Whether to tell now that the storage is full: the first time since it last had room (noted as
/// told).
pub(crate) fn tell_full() -> bool {
    let mut room = room();
    room.full = true;
    !std::mem::replace(&mut room.told, true)
}

/// How many of the core's writes have found the storage full since the app started: one that
/// changed over an import that failed was the storage full, for the apps to say so.
#[uniffi::export]
pub fn storage_full_writes() -> u64 {
    full_writes()
}

pub(crate) fn full_writes() -> u64 {
    FULL_WRITES.load(Ordering::Relaxed)
}

/// What to tell of a save that failed for `reason` (its error), if anything: the storage being full
/// is told once ([`NO_ROOM`]), by whichever failure meets it first (a save, the time-lapse), until a
/// save works again; any other failure as [`crate::save_failure_said`] puts it.
#[uniffi::export]
pub fn save_failure_to_tell(reason: String) -> Option<String> {
    if says_full(&reason) { tell_full().then(|| NO_ROOM.to_owned()) } else { Some(crate::save_failure_said(reason)) }
}

/// What is asked when an artwork being left couldn't be saved for `reason` (the save's error),
/// whether it had been saved before (`ever_saved`): a title, and what it says, which tells what
/// leaving without saving loses. The storage full says so first, plainly.
pub(crate) fn leave_unsaved_said(reason: &str, ever_saved: bool) -> (String, String) {
    let loses = if ever_saved {
        "Leaving without saving loses the changes made since it was last saved."
    } else {
        "Leaving without saving loses this artwork."
    };
    if says_full(reason) {
        ("Not enough storage".to_owned(), format!("Free some space to keep saving. {loses}"))
    } else {
        ("This artwork couldn't be saved".to_owned(), loses.to_owned())
    }
}

/// Whether `reason` (a failed write's or import's error) was the storage being full, for the apps
/// to say so rather than that the file was wrong.
#[uniffi::export]
pub fn storage_was_full(reason: String) -> bool {
    says_full(&reason)
}

/// Writes `bytes` to the file at `path` (made, or cut to nothing first), as `std::fs::write` does;
/// a full storage noted.
pub(crate) fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    create(path.as_ref())?.write_all(bytes.as_ref())
}

/// The file at `path` made to be written (cut to nothing if it is there), as `File::create`
/// makes it; a full storage noted as its writes meet it.
pub(crate) fn create(path: &Path) -> io::Result<File> {
    let file = std::fs::File::create(path).map_err(noted)?;
    Ok(File {
        file,
        #[cfg(test)]
        disk: simulated::disk_for(path),
    })
}

/// A file being written (see [`create`]).
pub(crate) struct File {
    file: std::fs::File,
    /// The full disk a test made it on, if it did.
    #[cfg(test)]
    disk: Option<std::sync::Arc<simulated::Disk>>,
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        #[cfg(test)]
        let buf = match &self.disk {
            Some(disk) => match disk.take(buf.len()) {
                0 if !buf.is_empty() => return Err(noted(disk.refuse())),
                n => buf.get(..n).unwrap_or(buf),
            },
            None => buf,
        };
        self.file.write(buf).map_err(noted)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush().map_err(noted)
    }
}

impl io::Seek for File {
    fn seek(&mut self, to: io::SeekFrom) -> io::Result<u64> {
        self.file.seek(to)
    }
}

/// The full disks tests make: a folder whose writes, through [`write`] and [`create`], take so
/// many bytes in all and then fail as a full storage's do.
#[cfg(test)]
pub(crate) mod simulated {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};

    pub(crate) struct Disk {
        root: PathBuf,
        /// Bytes it still takes.
        room: Mutex<u64>,
        /// Bytes written to it, and writes it refused.
        pub(crate) taken: AtomicU64,
        pub(crate) refused: AtomicU64,
    }

    static DISKS: Mutex<Vec<Arc<Disk>>> = Mutex::new(Vec::new());

    /// The full disk `path` is on, if a test made one there.
    pub(crate) fn disk_for(path: &Path) -> Option<Arc<Disk>> {
        DISKS.lock().unwrap_or_else(PoisonError::into_inner).iter().find(|d| path.starts_with(&d.root)).cloned()
    }

    impl Disk {
        /// Of a write of `want` bytes, how many it takes.
        pub(crate) fn take(&self, want: usize) -> usize {
            let mut room = self.room.lock().unwrap_or_else(PoisonError::into_inner);
            let n = (*room).min(want as u64);
            *room -= n;
            self.taken.fetch_add(n, Ordering::Relaxed);
            n as usize
        }

        /// A write refused: the error a full storage gives here.
        pub(crate) fn refuse(&self) -> std::io::Error {
            self.refused.fetch_add(1, Ordering::Relaxed);
            std::io::Error::from_raw_os_error(super::FULL_CODES[0])
        }
    }

    /// A full disk at a folder (and in it), while this is held.
    pub(crate) struct FullDisk(Arc<Disk>);

    impl FullDisk {
        /// The folder `root` is a disk with `room` bytes left.
        pub(crate) fn at(root: &Path, room: u64) -> FullDisk {
            let disk = Arc::new(Disk {
                root: root.to_path_buf(),
                room: Mutex::new(room),
                taken: AtomicU64::new(0),
                refused: AtomicU64::new(0),
            });
            DISKS.lock().unwrap_or_else(PoisonError::into_inner).push(Arc::clone(&disk));
            FullDisk(disk)
        }

        /// `bytes` more made room for (files deleted elsewhere).
        pub(crate) fn make_room(&self, bytes: u64) {
            *self.0.room.lock().unwrap_or_else(PoisonError::into_inner) += bytes;
        }

        /// Bytes written to it so far, and writes it refused.
        pub(crate) fn taken(&self) -> u64 {
            self.0.taken.load(Ordering::Relaxed)
        }

        pub(crate) fn refused(&self) -> u64 {
            self.0.refused.load(Ordering::Relaxed)
        }
    }

    impl Drop for FullDisk {
        fn drop(&mut self) {
            DISKS.lock().unwrap_or_else(PoisonError::into_inner).retain(|d| !Arc::ptr_eq(d, &self.0));
        }
    }

    /// Held by each test that fills a disk: the storage's state is the app's, one at a time.
    pub(crate) static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// The storage's state as the app starts (for a test that fills a disk, under [`ONE_AT_A_TIME`]).
    pub(crate) fn fresh() {
        super::room_again();
    }
}

#[cfg(test)]
pub(crate) use simulated::FullDisk;

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty folder of the test's own.
    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("procraft-storage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A full disk takes what fits and then refuses the rest as a real one does: the file written
    /// as far as there was room, the error the system's for no space, the storage noted full.
    #[test]
    fn a_full_disk_takes_what_fits_then_refuses_as_the_system_does() {
        let _one = simulated::ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        simulated::fresh();
        let dir = folder("takes");
        let disk = FullDisk::at(&dir, 1000);
        let before = full_writes();
        write(dir.join("a"), [1u8; 600]).unwrap();
        let e = write(dir.join("b"), [2u8; 600]).unwrap_err();
        assert!(is_full(&e), "{e}");
        assert!(says_full(&e.to_string()), "{e}");
        assert_eq!(std::fs::read(dir.join("b")).unwrap().len(), 400, "written as far as there was room");
        assert_eq!((disk.taken(), disk.refused()), (1000, 1));
        assert_eq!(full_writes() - before, 1);
        assert!(full());
        // elsewhere, writes go on
        let other = folder("takes-other");
        write(other.join("c"), [3u8; 5000]).unwrap();
        // room made: written whole again
        disk.make_room(600);
        write(dir.join("b"), [2u8; 600]).unwrap();
        assert_eq!(std::fs::read(dir.join("b")).unwrap(), [2u8; 600]);
        drop(disk);
        write(dir.join("d"), [4u8; 100_000]).unwrap();
        simulated::fresh();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }

    /// The storage full is told once, by whichever failure meets it first, until a save works
    /// again; other failures are told as they were.
    #[test]
    fn the_storage_full_is_told_once_until_a_save_works_again() {
        let _one = simulated::ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        simulated::fresh();
        let reasons = [
            "writing a layer: No space left on device (os error 28)",
            "saving: There is not enough space on the disk. (os error 112)",
            "saving: no storage space",
            "writing the artwork: Disk quota exceeded (os error 122)",
        ];
        let told: Vec<Option<String>> = reasons.iter().map(|r| save_failure_to_tell((*r).to_owned())).collect();
        assert_eq!(told.iter().flatten().count(), 1, "{told:?}");
        assert_eq!(told[0].as_deref(), Some(NO_ROOM));
        // another failure still says what it is
        assert_eq!(
            save_failure_to_tell("making the folder: Access is denied. (os error 5)".into()).as_deref(),
            Some("This artwork couldn't be saved.")
        );
        // a save worked: the next fill told again
        room_again();
        assert_eq!(save_failure_to_tell(reasons[0].to_owned()).as_deref(), Some(NO_ROOM));
        assert_eq!(save_failure_to_tell(reasons[0].to_owned()), None);
        simulated::fresh();
        // what isn't the storage full isn't taken for it
        for other in [
            "Access is denied. (os error 5)",
            "Not a directory (os error 20)",
            "The system cannot find the path specified. (os error 3)",
        ] {
            assert!(!says_full(other), "{other}");
        }
    }

    /// A real full device (Linux's /dev/full refuses every write with no space left) is told as
    /// full, as the apps meet it.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_systems_own_full_device_reads_as_full() {
        let _one = simulated::ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        let e = write("/dev/full", [0u8; 4096]).unwrap_err();
        assert!(is_full(&e), "{e}");
        assert!(says_full(&format!("saving: {e}")), "{e}");
        simulated::fresh();
    }
}
