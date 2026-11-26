//! A PSD of many layers, as painting apps write them (every layer the canvas's size), comes in
//! holding a few layers at a time whatever their number: 113 layers at 6103 x 1500, as a report's
//! file, on a tablet that works two of them side by side; and where the canvas holds fewer, it is
//! turned away at once, before any layer is read. (A test of its own, as it counts every
//! allocation the program makes.)

// (the helpers too are the test's)
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::path::PathBuf;

use procraft_core::{CanvasSession, PsdOutcome, import_psd, set_layer_budget};
use procraft_engine::document::BlendMode;
use procraft_engine::formats::{PsdKind, PsdLayer, PsdPixels, write_psd};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const W: u32 = 6103;
const H: u32 = 1500;
const LAYERS: usize = 113;

/// One layer's pixels, bytes.
const LAYER_BYTES: u64 = W as u64 * H as u64 * 4;

const MB: f64 = (1u64 << 20) as f64;

/// The most the import may hold at once, in layers' worth: two layers worked on side by side, each
/// with what it is read from and its file, and the flattened picture read for the thumbnail. (All
/// of them at once, as before, came to the 113 layers twice over: 226.)
const IMPORTING: u64 = 6;

/// Layer `i`'s pixels, straight: dots at the canvas's corners (so its rectangle is the whole canvas,
/// as such apps write every layer) and a band of soft colour somewhere across it.
fn layer_pixels(i: usize) -> Vec<u8> {
    let (w, h) = (W as usize, H as usize);
    let mut px = vec![0u8; w * h * 4];
    for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
        px[(y * w + x) * 4..][..4].copy_from_slice(&[1, 2, 3, 255]);
    }
    let y0 = (i * 13) % (h - 40);
    for y in y0..y0 + 40 {
        for x in 0..w {
            let a = ((x + i) % 256) as u8;
            px[(y * w + x) * 4..][..4].copy_from_slice(&[(i * 2) as u8, 100, a, a]);
        }
    }
    px
}

/// The PSD written into `path` a layer at a time.
fn write_file(path: &PathBuf) {
    let names: Vec<String> = (0..LAYERS).map(|i| format!("Layer {}", i + 1)).collect();
    let layers: Vec<PsdLayer<'_>> = names
        .iter()
        .map(|name| PsdLayer {
            name,
            kind: PsdKind::Read { masked: false },
            opacity: 1.0,
            visible: true,
            blend: BlendMode::Normal,
            clipped: false,
        })
        .collect();
    let composite = vec![255u8; LAYER_BYTES as usize];
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut read =
        |i: usize| -> std::io::Result<PsdPixels> { Ok(PsdPixels { rgba: layer_pixels(LAYERS - 1 - i), mask: None }) };
    write_psd(&mut out, W, H, 72.0, &[], &layers, &mut read, &composite, None).unwrap();
}

/// Runs `step` and returns the most the heap held at once while it ran (beyond what it held
/// before).
fn peak<T>(step: impl FnOnce() -> T) -> (T, u64) {
    let profiler = dhat::Profiler::builder().testing().build();
    let out = step();
    let most = dhat::HeapStats::get().max_bytes as u64;
    drop(profiler);
    (out, most)
}

#[test]
fn a_psd_of_113_layers_comes_in_a_few_layers_at_a_time_and_one_too_big_is_turned_away_at_once() {
    let root = std::env::temp_dir().join(format!("procraft-psd-memory-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("big.psd");
    write_file(&path);
    let size = std::fs::metadata(&path).unwrap().len();
    // a tablet: two layers worked on side by side (what its memory for such work holds of these)
    procraft_engine::par::hold_threads_to(2);

    // the report's device: about 2.8 GB, so layers take 1.4 GB, which holds 34 at this size
    set_layer_budget(1_400_000_000);
    let dir = root.join("refused");
    std::fs::create_dir_all(&dir).unwrap();
    let path_s = path.to_string_lossy().into_owned();
    let (outcome, held) = peak(|| import_psd(path_s.clone(), dir.to_string_lossy().into_owned(), "Big".to_owned()));
    eprintln!("refused: {outcome:?}, holding at most {:.1} MB", held as f64 / MB);
    assert_eq!(outcome, PsdOutcome::TooManyLayers { most: 34 });
    // turned away from its records alone: no layer read
    assert!(held < LAYER_BYTES, "{held} bytes held to turn it away");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);

    // a device that holds them all
    set_layer_budget(16 << 30);
    let dir = root.join("artwork");
    std::fs::create_dir_all(&dir).unwrap();
    let started = std::time::Instant::now();
    let (outcome, held) = peak(|| import_psd(path_s.clone(), dir.to_string_lossy().into_owned(), "Big".to_owned()));
    eprintln!(
        "{LAYERS} layers of {W} x {H} ({:.0} MB a layer) from a file of {:.0} MB: {outcome:?} in {:.1} s, holding at most {:.0} MB ({:.1} layers' worth; all at once would be {:.0} MB)",
        LAYER_BYTES as f64 / MB,
        size as f64 / MB,
        started.elapsed().as_secs_f64(),
        held as f64 / MB,
        held as f64 / LAYER_BYTES as f64,
        (LAYER_BYTES * LAYERS as u64 * 2) as f64 / MB,
    );
    assert_eq!(outcome, PsdOutcome::Imported);
    assert!(held <= IMPORTING * LAYER_BYTES, "held {held} bytes, more than {IMPORTING} layers' worth");
    // and it opens with every layer
    let s = CanvasSession::open(dir.to_string_lossy().into_owned()).unwrap();
    assert_eq!(s.layers().len(), LAYERS);
    drop(s);
    procraft_engine::par::hold_threads_to(0);
    let _ = std::fs::remove_dir_all(&root);
}
