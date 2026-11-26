#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! What the Reference windows cost a canvas: a 4000 × 4000 photo imported into one is kept as a
//! picture of 2048 square (kept whole, its PNG alone held 4000 square's worth), and strokes drawn
//! while one window shows the photo and another the canvas (read again after each stroke, as the
//! app reads it) hold no more than the same strokes drawn with no window, but for the one picture
//! of the canvas the windows share. Bounds: the photo held in at most 16 MiB, and 200 strokes
//! growing the heap at most 2 MiB more with the windows than without. (A test of its own, as it
//! counts every allocation the program makes.)

use std::time::Instant;

use procraft_core::{CanvasSession, PenSample, PointerKind, ReferenceShows, ReferenceView, ReferenceWindowState};
use procraft_engine::profile::Profile;
use procraft_gpu::{Gpu, wgpu};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

const MIB: f64 = (1u64 << 20) as f64;

/// The canvas's side, and the window's.
const SIDE: u32 = 2048;
const WINDOW: (u32, u32) = (1600, 1200);

/// The strokes drawn, after a few first.
const STROKES: usize = 200;
const FIRST: usize = 10;

/// A photo `side` square as a PNG: soft fields of colour with grain, so that it packs about as
/// well as a photo does.
fn photo(side: u32) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let pixels = image::RgbImage::from_fn(side, side, |px, py| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let grain = (x & 0x0F) as f32 - 8.0;
        let (u, v) = (px as f32 / side as f32, py as f32 / side as f32);
        let field = |a: f32| (a * 200.0 + 30.0 + grain).clamp(0.0, 255.0) as u8;
        image::Rgb([field(u), field(v), field((u * 3.0 + v * 5.0).sin() * 0.5 + 0.5)])
    });
    let mut png = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut png, image::ImageFormat::Png).unwrap();
    png.into_inner()
}

/// Bytes the heap holds now.
fn held() -> u64 {
    dhat::HeapStats::get().curr_bytes as u64
}

/// The window's state: shown, showing `shows`.
fn shown(shows: ReferenceShows) -> ReferenceWindowState {
    ReferenceWindowState {
        shown: true,
        shows,
        sized: false,
        width: 0.0,
        height: 0.0,
        placed: false,
        x: 0.0,
        y: 0.0,
        canvas_view: ReferenceView { zoom: 1.0, x: 0.0, y: 0.0 },
        image_view: ReferenceView { zoom: 1.0, x: 0.0, y: 0.0 },
    }
}

/// The `k`th stroke's samples, window pixels: a short line somewhere across the window.
fn stroke(k: usize) -> Vec<PenSample> {
    let (y, x0) = (100.0 + (k * 37 % 1000) as f32, 100.0 + (k * 53 % 400) as f32);
    (0..24)
        .map(|i| PenSample {
            x: x0 + i as f32 * 40.0,
            y: y + (i as f32 * 0.7).sin() * 30.0,
            pressure: 0.7,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: k as f64 * 1000.0 + i as f64 * 8.0,
            roll: 0.0,
        })
        .collect()
}

#[test]
fn reference_windows_hold_a_photo_made_smaller_and_nothing_more_as_strokes_are_drawn() {
    let _profiler = dhat::Profiler::builder().testing().build();

    // a 4000 square photo given to a window
    let s = CanvasSession::new(64, 64);
    let before = held();
    let png = photo(4000);
    let given = png.len() as f64 / MIB;
    s.set_reference_picture(png);
    let kept = held().saturating_sub(before) as f64 / MIB;
    let picture = image::load_from_memory(&s.reference_picture().unwrap()).unwrap();
    println!(
        "a 4000 x 4000 photo ({given:.1} MiB as a PNG) is held as {} x {} in {kept:.1} MiB (whole, {:.1} MiB of pixels)",
        picture.width(),
        picture.height(),
        4000.0 * 4000.0 * 4.0 / MIB,
    );
    assert_eq!((picture.width(), picture.height()), (2048, 2048));
    assert!(kept <= 16.0, "{kept} MiB held");
    drop((s, picture));

    // strokes drawn with the windows showing and without
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => return eprintln!("skipped the strokes: {e}"),
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
    let grown = |windows: bool| {
        let s = CanvasSession::new(SIDE, SIDE);
        s.use_gpu(gpu.clone());
        s.show_in(WINDOW.0, WINDOW.1);
        if windows {
            s.set_reference_picture(photo(4000));
            s.set_reference_window(shown(ReferenceShows::Image));
            assert_eq!(s.add_reference_window(shown(ReferenceShows::Canvas)), Some(1));
        }
        s.set_brush_size(0.2);
        let mut at = 0;
        for k in 0..FIRST + STROKES {
            if k == FIRST {
                at = held();
            }
            let samples = stroke(k);
            s.stroke_begin(PointerKind::Stylus, samples[..1].to_vec());
            s.stroke_add(samples[1..].to_vec());
            s.stroke_end(vec![]);
            let _ = s.draw(&view, wgpu::TextureFormat::Rgba8Unorm, WINDOW, Profile::Srgb, began);
            if windows {
                // the window showing the canvas reads it again, as the app does after a change
                assert!(s.reference_canvas(700).is_some());
            }
        }
        held() as i64 - at as i64
    };
    let without = grown(false);
    let with = grown(true);
    println!(
        "{STROKES} strokes grew the heap {:.2} MiB with no window, {:.2} MiB with a photo and the canvas in two",
        without as f64 / MIB,
        with as f64 / MIB
    );
    assert!(with - without <= 2 << 20, "{with} B with the windows, {without} B without");
}
