//! Reading a big brush set from its file holds a few of its brushes' files at a time, never the
//! whole file: the heap at its fullest while it is read stays far below the file's size. (A test
//! of its own, as it counts every allocation the program makes.)

use procraft_core::{BrushFileReading, ReadBrushes, read_brushes_from};
use procraft_engine::brush::Brush;
use procraft_engine::brushfile::{Pictures, write_set};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// One heap count at a time (the tests here run side by side).
static COUNTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Brushes in the set, and the bytes of each one's shape: bytes no picture reader takes, so each
/// is read, found to be no picture and let go (the brush drawn with a stand-in), nothing of it
/// kept.
const BRUSHES: usize = 48;
const SHAPE_BYTES: usize = 2 << 20;

#[test]
fn a_big_set_is_read_a_few_brushes_at_a_time() {
    let dir = std::env::temp_dir().join(format!("procraft-brush-file-memory-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Big.brushset");
    {
        // noise, which doesn't compress: the file is as big as its shapes
        let mut x = 0x2545_f491_4f6c_dd1du64;
        let shape: Vec<u8> = (0..SHAPE_BYTES)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect();
        let brushes: Vec<Brush> =
            (0..BRUSHES).map(|i| Brush { name: format!("Brush {i}"), ..Brush::round_pen() }).collect();
        let pictures = || Pictures {
            shape_png: &shape,
            grain_png: None,
            metallic_png: None,
            roughness_png: None,
            thumbnail_png: &[],
            author_png: None,
            signature_png: None,
            secondary: None,
        };
        let list: Vec<(&Brush, &str, Pictures<'_>)> = brushes.iter().map(|b| (b, "", pictures())).collect();
        std::fs::write(&path, write_set("Big", &list, |i| format!("folder-{i}"))).unwrap();
    }
    let file_bytes = std::fs::metadata(&path).unwrap().len() as usize;
    assert!(file_bytes > BRUSHES * SHAPE_BYTES);

    let counting = COUNTING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let profiler = dhat::Profiler::builder().testing().build();
    let reading = BrushFileReading::default();
    let read = read_brushes_from(&path, Some("Big.brushset".into()), &reading);
    let held = dhat::HeapStats::get().max_bytes;
    drop(profiler);
    drop(counting);
    let _ = std::fs::remove_dir_all(&dir);

    let Some(ReadBrushes::Set { name, brushes, .. }) = read else { panic!("no set read") };
    assert_eq!(name, "Big");
    let names: Vec<String> = brushes.iter().map(|b| b.name.clone()).collect();
    assert_eq!(names, (0..BRUSHES).map(|i| format!("Brush {i}")).collect::<Vec<_>>());
    assert_eq!(reading.progress(), 1.0);
    // a few brushes' files (one to a thread, 8 threads at most) and what was made of them: a
    // quarter of the file at the most, where reading it whole holds all of it and more
    eprintln!("heap at its fullest {held} bytes, file {file_bytes} bytes");
    assert!(held < file_bytes / 4, "held {held} bytes reading a file of {file_bytes}");
}

#[test]
fn a_reading_stopped_reads_as_nothing() {
    let dir = std::env::temp_dir().join(format!("procraft-brush-file-stopped-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Two.brushset");
    let brushes = [Brush::round_pen(), Brush::round_pen()];
    let pictures = || Pictures {
        shape_png: &[],
        grain_png: None,
        metallic_png: None,
        roughness_png: None,
        thumbnail_png: &[],
        author_png: None,
        signature_png: None,
        secondary: None,
    };
    let list: Vec<(&Brush, &str, Pictures<'_>)> = brushes.iter().map(|b| (b, "", pictures())).collect();
    std::fs::write(&path, write_set("Two", &list, |i| format!("f{i}"))).unwrap();
    let reading = BrushFileReading::default();
    assert!(read_brushes_from(&path, Some("Two.brushset".into()), &reading).is_some());
    reading.cancel();
    assert!(read_brushes_from(&path, Some("Two.brushset".into()), &reading).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A real big set, when one is given (`BIG_BRUSH_SET=path`, such as a synthetic one of a gigabyte
/// and more): it reads, and how much the heap held at its fullest is told.
#[test]
fn a_given_big_set_reads() {
    let Ok(path) = std::env::var("BIG_BRUSH_SET") else { return };
    let path = std::path::PathBuf::from(path);
    let file_bytes = std::fs::metadata(&path).unwrap().len();
    let counting = COUNTING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let profiler = dhat::Profiler::builder().testing().build();
    let started = std::time::Instant::now();
    let reading = BrushFileReading::default();
    let read = read_brushes_from(&path, path.file_name().map(|n| n.to_string_lossy().into_owned()), &reading);
    let stats = dhat::HeapStats::get();
    drop(profiler);
    drop(counting);
    let count = match &read {
        Some(ReadBrushes::Set { brushes, .. }) => brushes.len(),
        Some(ReadBrushes::Library { sets, .. }) => sets.iter().map(|s| s.2.len()).sum(),
        Some(ReadBrushes::One(_)) => 1,
        _ => 0,
    };
    eprintln!(
        "{count} brushes from {file_bytes} bytes in {:.1} s; heap at its fullest {} bytes, at the end {}",
        started.elapsed().as_secs_f32(),
        stats.max_bytes,
        stats.curr_bytes
    );
    assert!(count > 0);
    // what is made of the brushes is kept (their pictures at up to 1024 a side), the files not
    assert!((stats.max_bytes as u64) < file_bytes / 2);
}
