#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
//! The brush library opened for the first time on a tablet of little memory, as an old Android 12
//! one closed while it loaded (discord): the stock library, a 2000 × 1200 canvas open on a GPU held
//! to wgpu's downlevel limits, the sample stroke of each brush of the first set drawn as the
//! library's rows ask for them (530 × 162), then every other set's as each is looked through.
//! Printed: the heap and the GPU (its allocator's blocks) at their highest over what they held
//! with the canvas open, after the first set and after every set, and what is still held after.
//! Measured on an NVIDIA laptop GPU: the first set's 10 strokes 7 MB of heap and 132 MB of the
//! GPU; all 598, 83 MB and 132 MB (196 MB while the strokes' canvas kept the most its work had
//! grown to), 80 MB of heap kept after (the strokes kept packed, some 20 MB, and the brushes'
//! pictures drawn for them). Bounds: those, half again.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use procraft_core::CanvasSession;
use procraft_gpu::{Gpu, wgpu};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const SIDE: (u32, u32) = (2000, 1200);
const MB: u64 = 1 << 20;

/// The bounds (MB), from what was measured (see the module's note): the heap and the GPU at the
/// first set, at every set, and the heap kept after.
const FIRST_MOST: (u64, u64) = (16, 198);
const ALL_MOST: (u64, u64) = (125, 198);
const KEPT_MOST: u64 = 120;

static GPU_PEAK: AtomicU64 = AtomicU64::new(0);

fn reserved(gpu: &Gpu) -> u64 {
    gpu.device.generate_allocator_report().map_or(0, |r| r.total_reserved_bytes)
}

#[test]
fn the_brush_library_opened_the_first_time_holds_little() {
    let gpu = match Gpu::headless_limited(wgpu::Limits::downlevel_defaults()) {
        Ok(g) => g,
        Err(e) => return eprintln!("skipped: {e}"),
    };
    let s = CanvasSession::new(SIDE.0, SIDE.1);
    s.use_gpu(gpu.clone());
    s.show_in(SIDE.0, SIDE.1);
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let sets: Vec<Vec<(u32, u32)>> =
        s.brush_library().iter().map(|set| set.brushes.iter().map(|b| (b.set, b.index)).collect()).collect();
    let brushes: usize = sets.iter().map(Vec::len).sum();
    // the GPU sampled while the strokes are drawn
    let gpu_before = reserved(&gpu);
    GPU_PEAK.store(gpu_before, Ordering::Relaxed);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sampler = {
        let (gpu, stop) = (gpu.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                GPU_PEAK.fetch_max(reserved(&gpu), Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    };
    let profiler = dhat::Profiler::builder().testing().build();
    let heap_before = dhat::HeapStats::get().curr_bytes as u64;
    let over = |v: u64, before: u64| v.saturating_sub(before) / MB;
    let started = Instant::now();
    let mut drawn = 0;
    let mut first = None;
    for (k, set) in sets.iter().enumerate() {
        for &(set, index) in set {
            if s.brush_preview(set, index, 530, 162).is_some() {
                drawn += 1;
            }
        }
        if k == 0 {
            let at = (
                over(dhat::HeapStats::get().max_bytes as u64, heap_before),
                over(GPU_PEAK.load(Ordering::Relaxed), gpu_before),
            );
            println!(
                "the first set ({} brushes) in {:.1} s: the heap {} MB at its highest, the GPU {} MB",
                set.len(),
                started.elapsed().as_secs_f32(),
                at.0,
                at.1
            );
            first = Some(at);
        }
    }
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    stop.store(true, Ordering::Relaxed);
    sampler.join().unwrap();
    let stats = dhat::HeapStats::get();
    drop(profiler);
    let (heap_all, gpu_all) =
        (over(stats.max_bytes as u64, heap_before), over(GPU_PEAK.load(Ordering::Relaxed), gpu_before));
    let kept = over(stats.curr_bytes as u64, heap_before);
    println!(
        "every set ({brushes} brushes, {drawn} drawn) in {:.1} s: the heap {heap_all} MB at its highest, the GPU \
         {gpu_all} MB; {kept} MB kept after",
        started.elapsed().as_secs_f32()
    );
    assert_eq!(gpu.errors(), 0, "GPU validation errors");
    assert!(drawn * 10 >= brushes * 9, "{drawn} of {brushes} drawn");
    let (heap_first, gpu_first) = first.unwrap();
    assert!(heap_first <= FIRST_MOST.0 && gpu_first <= FIRST_MOST.1, "the first set: {heap_first} MB, {gpu_first} MB");
    assert!(heap_all <= ALL_MOST.0 && gpu_all <= ALL_MOST.1, "every set: {heap_all} MB, {gpu_all} MB");
    assert!(kept <= KEPT_MOST, "{kept} MB kept");
}
