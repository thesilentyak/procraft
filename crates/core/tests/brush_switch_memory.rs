#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
//! Switching brushes on a tablet of little memory, as a 4 GB Snapdragon 680 did when it closed as a
//! different brush was taken up and drawn with (#358): every stock brush taken up in turn, a stroke
//! drawn with each on a 2000 × 1200 canvas and its sample stroke drawn as the library's card does,
//! on a GPU held to wgpu's downlevel limits; then every third brush again. Printed: the heap at its
//! highest over what it held before, the GPU's memory at its highest over what it held before, and
//! what each round left held. Bounds: no panic and no GPU error; the heap at most [`HEAP_MOST`] MB
//! over what it held before, the GPU at most [`GPU_MOST`] MB (measured 154 and 516 on an NVIDIA
//! laptop GPU); the second round leaving at most [`LEFT_MOST`] MB more on the heap and the GPU
//! than the first (measured 1 and 0: nothing kept for each brush taken up). (A test of its own, as
//! it counts every allocation the program makes.)

use std::time::Instant;

use procraft_core::{CanvasSession, PenSample, PointerKind, TouchMove};
use procraft_engine::profile::Profile;
use procraft_gpu::{Gpu, wgpu};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The canvas and window: a 2000 × 1200 tablet's.
const SIDE: (u32, u32) = (2000, 1200);

/// The bounds (MB), from what was measured (see the module's note).
const HEAP_MOST: u64 = 256;
const GPU_MOST: u64 = 768;
const LEFT_MOST: u64 = 24;

/// The `k`th sample of a stroke across the canvas's middle, its pressure swelling and easing.
fn point(k: usize) -> PenSample {
    let t = k as f32 / 120.0;
    PenSample {
        x: 300.0 + t * 1400.0,
        y: 600.0 + (t * 9.0).sin() * 250.0,
        pressure: 0.3 + 0.6 * (t * 14.0).sin().abs(),
        tilt: 0.4,
        orientation: 0.8,
        time_ms: k as f64 * 8.0,
        roll: 0.0,
    }
}

const MB: u64 = 1 << 20;

#[test]
fn taking_up_every_stock_brush_and_drawing_with_it_keeps_memory_bounded() {
    let gpu = match Gpu::headless_limited(wgpu::Limits::downlevel_defaults()) {
        Ok(g) => g,
        Err(e) => return eprintln!("skipped: {e}"),
    };
    let reserved = |gpu: &Gpu| gpu.device.generate_allocator_report().map_or(0, |r| r.total_reserved_bytes);
    let s = CanvasSession::new(SIDE.0, SIDE.1);
    s.use_gpu(gpu.clone());
    s.show_in(SIDE.0, SIDE.1);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window"),
        size: wgpu::Extent3d { width: SIDE.0, height: SIDE.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let began = Instant::now();
    let frame = |s: &CanvasSession| {
        let _ = s.draw(&view, format, SIDE, Profile::Srgb, began);
    };
    frame(&s);
    let brushes: Vec<(String, u32, u32)> = s
        .brush_library()
        .iter()
        .enumerate()
        .flat_map(|(i, set)| {
            set.brushes.iter().enumerate().map(move |(j, b)| (b.name.clone(), i as u32, j as u32)).collect::<Vec<_>>()
        })
        .collect();
    let profiler = dhat::Profiler::builder().testing().build();
    let (heap_before, gpu_before) = (dhat::HeapStats::get().curr_bytes as u64, reserved(&gpu));
    let mut gpu_highest = gpu_before;
    let mut left = Vec::new();
    let mut panicked = Vec::new();
    for round in 0..2 {
        let started = Instant::now();
        for (name, set, index) in brushes.iter().step_by(if round == 0 { 1 } else { 3 }) {
            let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                assert!(s.select_brush(*set, *index));
                s.set_brush_size(0.4);
                s.set_color(0.8, 0.2, 0.1);
                frame(&s);
                s.touch_down(1, PointerKind::Stylus, vec![point(0)]);
                for e in 1..=60 {
                    s.touch_moves(vec![TouchMove { id: 1, samples: vec![point(2 * e - 1), point(2 * e)] }]);
                    if e % 2 == 0 {
                        frame(&s);
                    }
                }
                s.touch_up(1, vec![point(121)]);
                frame(&s);
                let _ = s.brush_preview(*set, *index, 1060, 324);
                s.undo();
                frame(&s);
            }));
            if drawn.is_err() {
                panicked.push(format!("{name} (round {round})"));
            }
            gpu_highest = gpu_highest.max(reserved(&gpu));
        }
        let now = (dhat::HeapStats::get().curr_bytes as u64, reserved(&gpu));
        println!(
            "round {round}: {} brushes in {:.1} s; left {} MB on the heap, {} MB on the GPU",
            brushes.len().div_ceil(if round == 0 { 1 } else { 3 }),
            started.elapsed().as_secs_f32(),
            now.0.saturating_sub(heap_before) / MB,
            now.1.saturating_sub(gpu_before) / MB
        );
        left.push(now);
    }
    let heap_highest = dhat::HeapStats::get().max_bytes as u64;
    drop(profiler);
    let (heap_peak, gpu_peak) =
        (heap_highest.saturating_sub(heap_before) / MB, gpu_highest.saturating_sub(gpu_before) / MB);
    println!("highest: the heap {heap_peak} MB over what it held before, the GPU {gpu_peak} MB");
    assert!(panicked.is_empty(), "panicked: {panicked:?}");
    assert_eq!(gpu.errors(), 0, "GPU validation errors");
    assert!(heap_peak <= HEAP_MOST, "the heap went {heap_peak} MB over");
    assert!(gpu_peak <= GPU_MOST, "the GPU went {gpu_peak} MB over");
    let grew = |a: u64, b: u64| b.saturating_sub(a) / MB;
    let (heap_grew, gpu_grew) = (grew(left[0].0, left[1].0), grew(left[0].1, left[1].1));
    assert!(
        heap_grew <= LEFT_MOST && gpu_grew <= LEFT_MOST,
        "the second round left {heap_grew} MB more on the heap, {gpu_grew} MB on the GPU"
    );
}
