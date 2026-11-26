#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! What a colour dropped on a big canvas holds of the heap at its fullest: into a ring too big for
//! the part of the canvas a small fill reads first, and into the whole of an empty layer, each
//! then its threshold slid and let go. Printed in MiB and in layers' worth (a layer's pixels,
//! 64 MiB on the 4096 square canvas): the layer as it was, each pixel's distance and reach, the
//! fill's masks and the pixels poured on their way to the GPU come to about 3.5. Bounds: at most
//! 4 layers' worth at the fullest, and nothing held once it is let go. `COLORDROP_SIDE` sets the
//! canvas's side. (A test of its own, as it counts every allocation the program makes.)

use std::time::Instant;

use procraft_core::{CanvasSession, PenSample, PointerKind};
use procraft_engine::profile::Profile;
use procraft_gpu::{Gpu, wgpu};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const MIB: f64 = (1u64 << 20) as f64;
const WINDOW: (u32, u32) = (1600, 1200);

/// The most a drop may hold at its fullest, in layers' worth.
const MOST_LAYERS: f64 = 4.0;

#[test]
fn a_colour_dropped_on_a_big_canvas_holds_a_few_layers_worth_at_most() {
    let side: u32 = std::env::var("COLORDROP_SIDE").ok().and_then(|v| v.parse().ok()).unwrap_or(4096);
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => return eprintln!("skipped: {e}"),
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window"),
        size: wgpu::Extent3d { width: WINDOW.0, height: WINDOW.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let began = Instant::now();
    let s = CanvasSession::new(side, side);
    s.use_gpu(gpu.clone());
    s.show_in(WINDOW.0, WINDOW.1);
    let frame = |s: &CanvasSession| {
        let _ = s.draw(&view, wgpu::TextureFormat::Rgba8Unorm, WINDOW, Profile::Srgb, began);
    };
    // a ring in the middle of the window, about a third of the canvas across
    s.set_brush_size(0.15);
    s.set_color(0.0, 0.0, 0.0);
    let ring: Vec<PenSample> = (0..=72)
        .map(|i| i as f32 / 72.0 * std::f32::consts::TAU)
        .map(|a| (800.0 + 200.0 * a.cos(), 600.0 + 200.0 * a.sin()))
        .enumerate()
        .map(|(i, (x, y))| PenSample {
            x,
            y,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: i as f64 * 8.0,
            roll: 0.0,
        })
        .collect();
    s.stroke_begin(PointerKind::Stylus, ring[..1].to_vec());
    s.stroke_add(ring[1..].to_vec());
    s.stroke_end(vec![]);
    frame(&s);
    frame(&s);
    let layer = f64::from(side) * f64::from(side) * 4.0 / MIB;
    let mut failures = Vec::new();
    // (the canvas's top left corner is in the window's, 200 in from its left)
    for (what, at) in [("inside the ring", (800.0, 600.0)), ("the whole layer", (205.0, 5.0))] {
        // the heap counted from here
        let profiler = dhat::Profiler::builder().testing().build();
        let t = Instant::now();
        let poured = s.pour_drop(at.0, at.1, 0.9, 0.2, 0.1, 0.3);
        let drop_ms = t.elapsed().as_secs_f64() * 1000.0;
        for k in 0..5 {
            s.pour_threshold(0.3 + k as f32 * 0.05);
        }
        s.pour_end(true, None);
        frame(&s);
        let end = dhat::HeapStats::get();
        drop(profiler);
        let (peak, left) = (end.max_bytes as f64 / MIB, end.curr_bytes as f64 / MIB);
        println!(
            "{side} square, {what}: dropped in {drop_ms:.0} ms, the heap at its fullest {peak:.1} MiB ({:.2} layers), {left:.1} MiB left held",
            peak / layer
        );
        assert!(poured, "{what}");
        if peak > MOST_LAYERS * layer {
            failures.push(format!("{what}: {peak:.1} MiB at the fullest"));
        }
        if left > 1.0 {
            failures.push(format!("{what}: {left:.1} MiB still held"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
