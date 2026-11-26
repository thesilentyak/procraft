#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
//! What a big eraser costs a phone's canvas, as one lagged when it went on to the brush or the
//! canvas was moved (discord): a 1080 × 2400 canvas of 31 painted layers (a phone's screen, on a
//! GPU held to wgpu's downlevel limits but for its textures' size), erased across by the eraser's
//! own brush at 10% and at 100% (600 pixels across) with a finger, each frame waited for on the
//! GPU, the Reference window showing the canvas reading it after each stroke (as hers did), then
//! the brush taken up again and the canvas moved by two fingers. Printed: each part's frames, in
//! all and the slowest, and the longest a touch waited for the canvas while the window read it.
//! Bounds: the big eraser's frames at most [`BIG_MOST`] times the small one's in all (measured 1.3
//! to 1.9 on an NVIDIA laptop GPU: its stamps cover 5600 times the pixels, the composite's caches
//! keep the layers below and above out of it), none over [`FRAME_MOST`], nor a touch's wait; the
//! brush taken up, and each frame moving the canvas, within [`FRAME_MOST`] (measured under 7 ms).

use std::time::{Duration, Instant};

use procraft_core::{CanvasSession, PenSample, PointerKind, ToolKind, TouchMove};
use procraft_engine::profile::Profile;
use procraft_gpu::{Gpu, wgpu};

const SIDE: (u32, u32) = (1080, 2400);
const BIG_MOST: f64 = 4.0;
const FRAME_MOST: Duration = Duration::from_millis(60);

fn at(x: f32, y: f32, k: usize) -> PenSample {
    PenSample { x, y, pressure: 1.0, tilt: 0.0, orientation: 0.0, time_ms: k as f64 * 8.0, roll: 0.0 }
}

/// The `k`th sample of a stroke across the canvas, winding up and down.
fn point(k: usize) -> PenSample {
    let t = k as f32 / 120.0;
    at(200.0 + t * 700.0, 1200.0 + (t * 9.0).sin() * 500.0, k)
}

/// Frames' times: in all, and the slowest.
fn sum(times: &[Duration]) -> (Duration, Duration) {
    (times.iter().sum(), times.iter().max().copied().unwrap_or_default())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

#[test]
fn a_big_eraser_costs_the_frames_little_more_than_a_small_one() {
    // a phone's GPU: downlevel, but taking a screen-sized canvas
    let limits = wgpu::Limits { max_texture_dimension_2d: 8192, ..wgpu::Limits::downlevel_defaults() };
    let gpu = match Gpu::headless_limited(limits) {
        Ok(g) => g,
        Err(e) => return eprintln!("skipped: {e}"),
    };
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
    // a frame, and the GPU's work for it
    let frame = |s: &CanvasSession| {
        let t = Instant::now();
        let _ = s.draw(&view, format, SIDE, Profile::Srgb, began);
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        t.elapsed()
    };
    frame(&s);
    // something to erase, on a layer between 30 others painted
    s.set_tool(ToolKind::Paint);
    s.set_brush_size(1.0);
    for k in 0..3 {
        let shifted = |e: usize| {
            let mut p = point(e);
            p.y += k as f32 * 100.0;
            p
        };
        s.stroke_begin(PointerKind::Finger, vec![shifted(0)]);
        for e in 1..=60 {
            s.stroke_add(vec![shifted(2 * e)]);
            if e % 4 == 0 {
                frame(&s);
            }
        }
        s.stroke_end(vec![shifted(121)]);
        frame(&s);
    }
    let painted = s.layers().iter().find(|l| l.current).map(|l| l.id).unwrap();
    for k in 0..30 {
        let id = s.add_layer();
        s.set_color(0.1 * (k % 10) as f32, 0.5, 0.3);
        assert!(s.fill_layer(id));
        assert!(s.set_layer_opacity(id, 0.1, true));
    }
    assert!(s.select_layer(painted));
    frame(&s);
    assert!(s.info().can_undo && s.info().error.is_none(), "{:?}", s.info().error);
    s.set_tool(ToolKind::Erase);
    let mut erasing = Vec::new();
    for size in [0.1f32, 1.0] {
        s.set_brush_size(size);
        frame(&s);
        let diameter = s.info().brush_diameter;
        let mut times = Vec::new();
        s.stroke_begin(PointerKind::Finger, vec![point(0)]);
        for e in 1..=60 {
            s.stroke_add(vec![point(2 * e - 1), point(2 * e)]);
            // the cursor's size and outline, as the app reads them for each move
            let t = Instant::now();
            let _ = (s.cursor_radius(), s.brush_outline());
            times.push(t.elapsed() + frame(&s));
        }
        s.stroke_end(vec![point(121)]);
        // the Reference window showing the canvas reads it after the stroke, on a thread of its own
        // as the app does, at the side a phone's window zoomed in asks (the frames and the touches
        // waited behind that read in 0.0.7, for most of a second after a heavy stroke)
        let reader = {
            let s = std::sync::Arc::clone(&s);
            std::thread::spawn(move || {
                let t = Instant::now();
                let read = s.reference_canvas(1920);
                (t.elapsed(), read.map(|r| (r.width, r.height)))
            })
        };
        let mut waits = Vec::new();
        loop {
            let t = Instant::now();
            let _ = s.info();
            waits.push(t.elapsed());
            times.push(frame(&s));
            if reader.is_finished() {
                break;
            }
        }
        let (read_took, read) = reader.join().unwrap();
        let (all, slowest) = sum(&times);
        let waited = sum(&waits).1;
        println!(
            "erasing {diameter:.0} across with {}: {} frames, {:.1} ms in all, the slowest {:.1} ms; the Reference window's read {read:?} in {:.1} ms, a touch's wait for the canvas meanwhile {:.1} ms at most",
            s.current_brush().name,
            times.len(),
            ms(all),
            ms(slowest),
            ms(read_took),
            ms(waited)
        );
        assert!(slowest <= FRAME_MOST && waited <= FRAME_MOST, "{slowest:?} {waited:?}");
        assert_eq!(read, Some((864, 1920)));
        erasing.push(all);
        // the brush taken up, then the eraser again
        let t = Instant::now();
        s.set_tool(ToolKind::Paint);
        frame(&s);
        let switched = t.elapsed();
        s.set_tool(ToolKind::Erase);
        frame(&s);
        // the canvas moved by two fingers, the first a moment before the second
        let mut moving = Vec::new();
        s.touch_down(1, PointerKind::Finger, vec![at(400.0, 1000.0, 1000)]);
        moving.push(frame(&s));
        s.touch_down(2, PointerKind::Finger, vec![at(700.0, 1400.0, 1002)]);
        moving.push(frame(&s));
        for k in 3..40 {
            let d = k as f32 * 8.0;
            s.touch_moves(vec![
                TouchMove { id: 1, samples: vec![at(400.0 + d, 1000.0 + d, 1000 + k)] },
                TouchMove { id: 2, samples: vec![at(700.0 + d, 1400.0 + d, 1000 + k)] },
            ]);
            moving.push(frame(&s));
        }
        s.touch_up(2, vec![at(1000.0, 1700.0, 1041)]);
        s.touch_up(1, vec![at(700.0, 1300.0, 1041)]);
        moving.push(frame(&s));
        let (all, slowest) = sum(&moving);
        println!(
            "  the brush taken up in {:.1} ms; the canvas moved in {} frames, {:.1} ms in all, the slowest {:.1} ms",
            ms(switched),
            moving.len(),
            ms(all),
            ms(slowest)
        );
        assert!(switched <= FRAME_MOST && slowest <= FRAME_MOST, "{switched:?} {slowest:?}");
    }
    let ratio = erasing[1].as_secs_f64() / erasing[0].as_secs_f64().max(1e-6);
    println!("the big eraser's frames: {ratio:.2} times the small one's");
    assert!(ratio <= BIG_MOST, "{ratio}");
    assert_eq!(gpu.errors(), 0, "GPU validation errors");
}
