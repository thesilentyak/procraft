#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
//! Sharing a big brush set or library writes its file a brush at a time: the heap at its fullest
//! while it is written stays far below the file's size, however many brushes it holds. (A test of
//! its own, as it counts every allocation the program makes.)

use std::sync::Arc;
use std::time::{Duration, Instant};

use procraft_core::{BrushFileWriting, CanvasSession};
use procraft_engine::brush::Brush;
use procraft_engine::brushfile::{Pictures, write_set};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// One test at a time: the heap is counted for the whole program, so another test's allocations
/// (making its big set) would count against the one counting.
static COUNTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Held for the whole of a test (see [`COUNTING`]).
fn alone() -> std::sync::MutexGuard<'static, ()> {
    COUNTING.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Brushes in the set, and the side of each one's shape and grain: noise, which doesn't
/// compress, so the file is as big as its pictures (two of 1 MB a brush).
const BRUSHES: usize = 24;
const SIDE: u32 = 1024;

/// Grey noise `side` pixels square, from `seed`.
fn noise(side: u32, seed: u64) -> Vec<u8> {
    let mut x = 0x2545_f491_4f6c_dd1du64 ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (0..side * side)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

/// A grey PNG of noise `side` pixels square, from `seed`.
fn noise_png(side: u32, seed: u64) -> Vec<u8> {
    gray_png(side, &noise(side, seed))
}

/// A grey PNG of `pixels`, `side` square.
fn gray_png(side: u32, pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, side, side);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    writer.finish().unwrap();
    out
}

/// A small preview, so that none is drawn (the brushes bring their own).
fn preview_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 32, 10);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[255; 32 * 10 * 4]).unwrap();
    writer.finish().unwrap();
    out
}

/// A session holding a set of [`BRUSHES`] brushes, each with its own big shape and grain: the
/// session and the set's place.
fn session_with_big_set() -> (Arc<CanvasSession>, u32) {
    let s = CanvasSession::new(256, 256);
    let shapes: Vec<Vec<u8>> = (0..BRUSHES).map(|i| noise_png(SIDE, i as u64)).collect();
    let grains: Vec<Vec<u8>> = (0..BRUSHES).map(|i| noise_png(SIDE, 1000 + i as u64)).collect();
    let preview = preview_png();
    let brushes: Vec<Brush> =
        (0..BRUSHES).map(|i| Brush { name: format!("Brush {i}"), ..Brush::round_pen() }).collect();
    let list: Vec<(&Brush, &str, Pictures<'_>)> = brushes
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let pictures = Pictures {
                shape_png: &shapes[i],
                grain_png: Some(&grains[i]),
                metallic_png: None,
                roughness_png: None,
                thumbnail_png: &preview,
                author_png: None,
                signature_png: None,
                secondary: None,
            };
            (b, "", pictures)
        })
        .collect();
    let file = write_set("Big", &list, |i| format!("folder-{i}"));
    let set = s.import_brushes(file).expect("the big set, imported");
    (s, set)
}

/// A folder of the test's own.
fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("procraft-brush-export-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// What writing a file took: the heap at its fullest past what it held before (bytes), the
/// allocations made, and how long it took.
struct Took {
    held: usize,
    blocks: u64,
    time: Duration,
}

/// `write` run with the heap counted from its start (in a test holding [`alone`]).
fn counted(write: impl FnOnce()) -> Took {
    let profiler = dhat::Profiler::builder().testing().build();
    let started = Instant::now();
    write();
    let time = started.elapsed();
    let stats = dhat::HeapStats::get();
    drop(profiler);
    Took { held: stats.max_bytes, blocks: stats.total_blocks, time }
}

/// Set `set` of `s` shared: written into the file at `path`.
fn export_set(s: &CanvasSession, set: u32, path: &std::path::Path) {
    s.write_brush_set_file(set, path.to_string_lossy().into_owned(), None).expect("the set's file");
}

/// The quickest of `runs` runs of `f`.
fn quickest(runs: usize, mut f: impl FnMut()) -> Duration {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            f();
            started.elapsed()
        })
        .min()
        .unwrap_or_default()
}

#[test]
fn a_big_set_is_written_a_brush_at_a_time() {
    let _alone = alone();
    let (s, set) = session_with_big_set();
    let dir = folder("set");
    let path = dir.join("Big.brushset");
    let took = counted(|| export_set(&s, set, &path));
    let file_bytes = std::fs::metadata(&path).unwrap().len() as usize;
    let back = procraft_engine::brushfile::read_set(&std::fs::read(&path).unwrap()).expect("the set reads back");
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!(
        "set of {BRUSHES}: heap at its fullest {} KB, {} allocations, {} ms; file {} KB",
        took.held >> 10,
        took.blocks,
        took.time.as_millis(),
        file_bytes >> 10
    );
    assert_eq!(back.0, "Big");
    assert_eq!(back.1.len(), BRUSHES);
    assert!(file_bytes > BRUSHES * (SIDE * SIDE) as usize * 2, "the pictures are in it whole");
    // one brush's pictures (2 MB here) as they are made and written, a sixth of the file at the
    // most, where putting it together in memory held three times the file (163 MB for 48 MB)
    let brush_bytes = 2 * (SIDE * SIDE) as usize;
    assert!(took.held < 4 * brush_bytes, "held {} KB for brushes of {} KB", took.held >> 10, brush_bytes >> 10);
    assert!(took.held < file_bytes / 6, "held {} KB writing a file of {} KB", took.held >> 10, file_bytes >> 10);
}

/// Writing a set takes little more than making its pictures (a PNG of each brush's shape and
/// grain), as they go into the file as they are: deflated again on top, the file took about
/// twice as long. Judged by the quickest of three runs of each.
#[test]
fn writing_a_set_takes_little_more_than_making_its_pictures() {
    let _alone = alone();
    let (s, set) = session_with_big_set();
    let dir = folder("time");
    let path = dir.join("Big.brushset");
    let pixels: Vec<Vec<u8>> = (0..BRUSHES * 2).map(|i| noise(SIDE, i as u64)).collect();
    let pictures = quickest(3, || {
        for p in &pixels {
            std::hint::black_box(gray_png(SIDE, p));
        }
    });
    let written = quickest(3, || export_set(&s, set, &path));
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!(
        "set of {BRUSHES}: its pictures made in {} ms, the file written in {} ms ({:.2}x)",
        pictures.as_millis(),
        written.as_millis(),
        written.as_secs_f64() / pictures.as_secs_f64().max(1e-6)
    );
    assert!(written <= pictures * 3 / 2 + Duration::from_millis(250), "{written:?} against {pictures:?}");
}

/// The whole library shared (the big set in it, and the sets the app comes with) is written a
/// brush at a time too: the heap at its fullest stays far below the file's size.
#[test]
fn a_library_is_written_a_brush_at_a_time() {
    let _alone = alone();
    let (s, _) = session_with_big_set();
    let dir = folder("library");
    let path = dir.join("Brushes.brushlibrary");
    // the stock brushes' textures, read once for the app and kept for drawing, aren't the
    // export's: read before counting (a first export reads them), as they are in an app that has
    // drawn with its brushes
    assert!(s.brush_library_as_file().is_some());
    let writing = BrushFileWriting::new();
    let took = counted(|| {
        s.write_brush_library_file(path.to_string_lossy().into_owned(), Some(Arc::clone(&writing)))
            .expect("the library's file");
    });
    let file_bytes = std::fs::metadata(&path).unwrap().len() as usize;
    let names = procraft_engine::zip::names_in_file(&path).expect("a zip");
    let brushes = names.iter().filter(|n| n.ends_with(".brush")).count();
    let _ = std::fs::remove_dir_all(&dir);
    let library: usize = s.brush_library().iter().map(|set| set.brushes.len()).sum();
    eprintln!(
        "library of {brushes} brushes: heap at its fullest {} KB, {} allocations, {} ms; file {} KB",
        took.held >> 10,
        took.blocks,
        took.time.as_millis(),
        file_bytes >> 10
    );
    assert_eq!(brushes, library, "each brush in it");
    assert_eq!(writing.progress(), 1.0, "all the way once written");
    // a brush at a time (the big set's of 2 MB the biggest): an eighth of the file at the most,
    // where putting it together in memory held four times the file (354 MB for 88 MB)
    assert!(took.held < file_bytes / 8, "held {} KB writing a file of {} KB", took.held >> 10, file_bytes >> 10);
}

/// An export stopped (the card's Cancel) writes no more brushes and leaves no file behind.
#[test]
fn a_stopped_export_leaves_nothing() {
    let _alone = alone();
    let (s, set) = session_with_big_set();
    let dir = folder("stopped");
    let path = dir.join("Big.brushset");
    let writing = BrushFileWriting::new();
    let stopper = Arc::clone(&writing);
    // stopped once the first few brushes are written
    let watching = std::thread::spawn(move || {
        let started = Instant::now();
        while stopper.progress() < 0.1 && started.elapsed() < Duration::from_secs(30) {
            std::thread::sleep(Duration::from_millis(1));
        }
        stopper.cancel();
    });
    let result = s.write_brush_set_file(set, path.to_string_lossy().into_owned(), Some(Arc::clone(&writing)));
    watching.join().unwrap();
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_err(), "a stopped export fails");
    assert!(writing.progress() < 1.0, "it stopped part way: {}", writing.progress());
    assert!(left.is_empty(), "nothing left: {left:?}");
}
