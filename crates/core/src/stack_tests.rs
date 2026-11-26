//! What the apps run on threads of their own, run on a small stack: a session made, an artwork
//! opened, the brushes read ahead and files brought in. An iPad's dispatch queues give their
//! threads 512 KB and an Android thread has 1 MB; the session's state built by value once took
//! 800 KB of one frame, and every artwork opened there ended the app.
//!
//! An overflow can't be caught: it ends the test run, saying "thread '<name>' has overflowed its
//! stack", the thread's name telling which test and which stack.

use std::path::{Path, PathBuf};

use procraft_engine::document::BlendMode;
use procraft_engine::formats::{PsdKind, PsdLayer, psd};

use crate::{CanvasSession, PsdOutcome, State, ThumbnailImage, import_psd, prepare_brushes};

/// A thread's stack on an iPad's dispatch queue.
const QUEUE_STACK: usize = 512 * 1024;
/// Half that: the margin these keep.
const HALF_QUEUE_STACK: usize = 256 * 1024;
/// The most the session's state may take, moved as a whole: it is built once, and the frame that
/// builds it holds a copy or two.
const STATE_MOST: usize = 16 * 1024;
/// What [`stack_probe`] is told to do, by the measuring test: "<work> <stack bytes> <path>".
const PROBE: &str = "PROCRAFT_STACK_PROBE";

/// Runs `work` on a thread of `stack` bytes named after `what`, and gives what it made.
fn on_stack<T: Send + 'static>(what: &str, stack: usize, work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .name(format!("{what} on a {} KB stack", stack / 1024))
        .stack_size(stack)
        .spawn(work)
        .unwrap()
        .join()
        .unwrap()
}

/// Runs `work` on a thread of each small stack, the smallest first.
fn on_small_stacks<T: Send + 'static>(what: &str, work: impl Fn() -> T + Send + Sync + Clone + 'static) -> Vec<T> {
    [HALF_QUEUE_STACK, QUEUE_STACK].into_iter().map(|stack| on_stack(what, stack, work.clone())).collect()
}

/// An empty folder of the test's own.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("procraft-stack-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A small picture: `w` × `h` of a gradient, opaque.
fn picture(w: u32, h: u32) -> ThumbnailImage {
    let rgba = (0..w * h).flat_map(|i| [(i % 251) as u8, (i % 13 * 19) as u8, 90, 255]).collect();
    ThumbnailImage { width: w, height: h, rgba }
}

#[test]
fn the_sessions_state_is_small_enough_to_move() {
    let state = std::mem::size_of::<State>();
    let shared = std::mem::size_of::<crate::Shared>();
    println!("the session's state: {state} bytes ({:.1} KB); all it shares: {shared} bytes", state as f64 / 1024.0);
    assert!(state <= STATE_MOST, "the state takes {state} bytes, more than {STATE_MOST}");
    assert!(shared <= STATE_MOST + 4096, "the shared part takes {shared} bytes");
}

#[test]
fn a_session_is_made_on_a_small_stack() {
    let made = on_small_stacks("a new session", || {
        let s = CanvasSession::new(1, 1);
        let one = s.name();
        drop(s);
        let s = CanvasSession::new(64, 48);
        (one, s.info().width, s.info().height)
    });
    for (name, w, h) in made {
        assert_eq!((name.as_str(), w, h), (crate::DEFAULT_NAME, 64, 48));
    }
}

#[test]
fn an_artwork_opens_on_a_small_stack() {
    let dir = folder("open");
    let saved = dir.join("artwork");
    let s = CanvasSession::from_image(picture(40, 30), "Kept".to_owned());
    s.add_layer();
    s.save(saved.to_string_lossy().into_owned()).unwrap();
    let path = saved.to_string_lossy().into_owned();
    let opened = on_small_stacks("an artwork opened", move || {
        let s = CanvasSession::open(path.clone()).unwrap();
        (s.name(), s.info().width, s.info().height, s.layers().len())
    });
    for (name, w, h, layers) in opened {
        assert_eq!((name.as_str(), w, h, layers), ("Kept", 40, 30, 2));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn brushes_are_read_ahead_on_a_small_stack() {
    let dir = folder("brushes");
    let path = dir.to_string_lossy().into_owned();
    let first = CanvasSession::new(32, 32);
    let set = first.new_brush_set();
    assert!(first.rename_brush_set(set, "Read ahead".into()));
    assert!(first.keep_brushes(path.clone()));
    for stack in [HALF_QUEUE_STACK, QUEUE_STACK] {
        // another folder's library held meanwhile: this one's file is read again, by a session
        // made for it, as at the app's start
        assert!(CanvasSession::new(32, 32).keep_brushes(dir.join("other").to_string_lossy().into_owned()));
        let ahead = path.clone();
        on_stack("the brushes read ahead", stack, move || prepare_brushes(ahead));
    }
    let next = CanvasSession::new(32, 32);
    assert!(next.open_brushes(path));
    assert_eq!(next.saved_brush_library(), first.saved_brush_library());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn files_are_brought_in_on_a_small_stack() {
    let dir = folder("import");
    let (w, h) = (24, 16);
    let base = picture(w, h).rgba;
    let layers = [PsdLayer {
        name: "Base",
        kind: PsdKind::Pixels { rgba: &base, mask: None },
        opacity: 1.0,
        visible: true,
        blend: BlendMode::Normal,
        clipped: false,
    }];
    let file = dir.join("in.psd");
    std::fs::write(&file, psd(w, h, 72.0, &[], &layers, &base, None)).unwrap();
    for (i, stack) in [HALF_QUEUE_STACK, QUEUE_STACK].into_iter().enumerate() {
        let (from, to) = (file.to_string_lossy().into_owned(), dir.join(format!("psd-{i}")));
        let into = to.to_string_lossy().into_owned();
        let outcome = on_stack("a PSD brought in", stack, move || import_psd(from, into, "Brought".to_owned()));
        assert_eq!(outcome, PsdOutcome::Imported);
        let opened = to.to_string_lossy().into_owned();
        let (name, layers) = on_stack("a PSD brought in, opened", stack, move || {
            let s = CanvasSession::open(opened).unwrap();
            (s.name(), s.layers().len())
        });
        assert_eq!((name.as_str(), layers), ("Brought", 1));
    }
    // a picture, as Photos hands one over
    let made = on_small_stacks("a picture brought in", || {
        let s = CanvasSession::from_image(picture(30, 20), "Photo".to_owned());
        (s.name(), s.info().width, s.info().height)
    });
    for (name, w, h) in made {
        assert_eq!((name.as_str(), w, h), ("Photo", 30, 20));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Does one piece of work on a thread of a given stack, as [`PROBE`] says, in a process of its
/// own: [`the_least_stack_each_needs`] runs it, an overflow ending only that process. Nothing
/// without it.
#[test]
#[ignore = "run by the_least_stack_each_needs, in a process of its own"]
fn stack_probe() {
    let Ok(asked) = std::env::var(PROBE) else { return };
    let mut parts = asked.splitn(3, ' ');
    let (Some(work), Some(stack), Some(path)) = (parts.next(), parts.next(), parts.next()) else { return };
    let (work, stack, path) = (work.to_owned(), stack.parse::<usize>().unwrap(), path.to_owned());
    on_stack(&work.clone(), stack, move || match work.as_str() {
        "new" => drop(CanvasSession::new(1, 1)),
        "open" => drop(CanvasSession::open(path).unwrap()),
        "brushes" => prepare_brushes(path),
        "psd" => {
            let into = std::env::temp_dir().join(format!("procraft-stack-probe-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&into);
            let outcome = import_psd(path, into.to_string_lossy().into_owned(), "Probe".to_owned());
            let _ = std::fs::remove_dir_all(&into);
            assert_eq!(outcome, PsdOutcome::Imported);
        }
        _ => panic!("no such work: {work}"),
    });
}

/// Whether `work` on `path` goes through on a thread of `kb` KB, in a process of its own.
fn goes_on(work: &str, path: &Path, kb: usize) -> bool {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "stack_tests::stack_probe", "--test-threads=1", "-q"])
        .env(PROBE, format!("{work} {} {}", kb * 1024, path.display()))
        .output()
        .is_ok_and(|out| out.status.success())
}

/// The least stack `work` on `path` goes through on, in KB to within 16 (none past 1 MB).
fn least_stack(work: &str, path: &Path) -> Option<usize> {
    let (mut fails, mut works) = (16, 1024);
    if !goes_on(work, path, works) {
        return None;
    }
    while works - fails > 16 {
        let mid = (fails + works) / 2;
        if goes_on(work, path, mid) {
            works = mid;
        } else {
            fails = mid;
        }
    }
    Some(works)
}

/// The least stack each piece of work the apps run on threads of their own goes through on: a
/// session made, an artwork opened, the brushes read ahead and a PSD brought in. Measured on the
/// build the tests run (the frames of an optimized build are smaller; Windows gives a thread's
/// stack in steps of 64 KB), each needs half a dispatch queue's thread at most.
#[test]
fn the_least_stack_each_needs() {
    let dir = folder("least");
    let saved = dir.join("artwork");
    let s = CanvasSession::from_image(picture(40, 30), "Kept".to_owned());
    s.save(saved.to_string_lossy().into_owned()).unwrap();
    let brushes = dir.join("brushes");
    assert!(CanvasSession::new(32, 32).keep_brushes(brushes.to_string_lossy().into_owned()));
    let base = picture(24, 16).rgba;
    let layers = [PsdLayer {
        name: "Base",
        kind: PsdKind::Pixels { rgba: &base, mask: None },
        opacity: 1.0,
        visible: true,
        blend: BlendMode::Normal,
        clipped: false,
    }];
    let file = dir.join("in.psd");
    std::fs::write(&file, psd(24, 16, 72.0, &[], &layers, &base, None)).unwrap();
    let mut over = Vec::new();
    for (work, path) in [("new", dir.clone()), ("open", saved), ("brushes", brushes), ("psd", file)] {
        let least = least_stack(work, &path);
        match least {
            Some(kb) => println!("{work}: goes through on a {kb} KB stack"),
            None => println!("{work}: needs more than 1 MB of stack"),
        }
        if least.is_none_or(|kb| kb * 1024 > HALF_QUEUE_STACK) {
            over.push(work);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(over.is_empty(), "need more than {} KB: {over:?}", HALF_QUEUE_STACK / 1024);
}
