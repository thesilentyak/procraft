#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
//! A pen's events, handed in as the app's thread hands them (a touch down, moves, a touch up),
//! allocate as little at the end of a long stroke as at its start: a call that made a buffer as
//! big as the stroke at every event costs a fresh page of memory (a page fault) for every few KB
//! of it, and a long stroke's events then queue behind each other until the app stops answering.
//! What each event allocated is printed for each brush, its first and last hundred events apart,
//! and what the lift allocated: a dual brush's end works out and lays its whole stroke again, a
//! copy of its stamps or two, but no more (it made five, 39 MB, for Dunlin), and a pen that shows
//! its tip copies nothing of the stroke as it ends (it copied its builder whole). Bounds: an
//! event late in the stroke at most twice an early one and 128 KB, none past 24 MB (the stroke's
//! own lists doubling as they grow), the lift within each brush's [`lift_most`]. (A test of its
//! own, as it counts every allocation the program makes.)

use std::time::{Duration, Instant};

use procraft_core::{CanvasSession, PenSample, PointerKind, TouchMove};
use procraft_engine::profile::Profile;
use procraft_gpu::{Gpu, wgpu};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The canvas's side and the window's.
const SIDE: u32 = 2048;
const WINDOW: (u32, u32) = (1600, 1200);

/// The events of a stroke (two samples each, a 240 Hz pen's, so about 6.7 s), and the events a
/// frame (one every two: 120 frames a second).
const EVENTS: usize = 1600;
const EVERY: usize = 2;

/// The brushes stroked: dual, wet and dual, wet, a tapered pen, a pen shown with its tip, a big
/// soft one, and a smudge.
const BRUSHES: [&str; 7] =
    ["Dunlin", "Lestrade", "Gouache", "Merlin", "Studio Pen", "Soft Airbrush@1", "smudge:Oil Paint"];

/// The most a stroke's lift may allocate (MB): a dual brush's laid whole again, any other's next to
/// nothing.
fn lift_most(entry: &str) -> u64 {
    if entry == "Dunlin" { 32 } else { 2 }
}

/// The `k`th sample of the stroke, in window pixels: a wave across the window, its pressure
/// swelling and easing.
fn point(k: usize) -> PenSample {
    let t = k as f32 / (2 * EVENTS) as f32;
    PenSample {
        x: 100.0 + t * 1400.0,
        y: 600.0 + (t * 25.0).sin() * 400.0,
        pressure: 0.3 + 0.6 * (t * 40.0).sin().abs(),
        tilt: 0.4,
        orientation: 0.8,
        time_ms: k as f64 * 4.17,
        roll: 0.0,
    }
}

/// Bytes the program has allocated in all, so far.
fn allocated() -> u64 {
    dhat::HeapStats::get().total_bytes
}

/// Waits for the big pictures a brush just chosen asks for to be made: on a thread of their own, a
/// first start's work and not its pen's, but counted here with every other allocation the program
/// makes (a 2048 picture's drawing, 33 MB, in the middle of an event).
fn settle_big_pictures() {
    let asked = Instant::now();
    while procraft_core::big_pictures_left() == 0 && asked.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(procraft_core::wait_for_big_pictures(Duration::from_secs(600)), "the big pictures weren't made");
}

/// The median of `v`.
fn median(v: &[u64]) -> u64 {
    let mut v = v.to_vec();
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}

#[test]
fn a_long_pen_stroke_s_last_events_allocate_no_more_than_its_first() {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => return eprintln!("skipped: {e}"),
    };
    let s = CanvasSession::new(SIDE, SIDE);
    s.use_gpu(gpu.clone());
    s.show_in(WINDOW.0, WINDOW.1);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window"),
        size: wgpu::Extent3d { width: WINDOW.0, height: WINDOW.1, depth_or_array_layers: 1 },
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
        let _ = s.draw(&view, format, WINDOW, Profile::Srgb, began);
    };
    let library = s.brush_library();
    let pick = |wanted: &str| {
        library
            .iter()
            .enumerate()
            .find_map(|(i, set)| set.brushes.iter().position(|b| b.name == wanted).map(|j| (i as u32, j as u32)))
    };
    // with `PEN_EVENT_PROFILE=file.json`, where each allocation was made written there as a heap
    // profile (to read in a heap profile viewer), and `PEN_EVENT_BRUSHES` the brushes stroked
    let profile = std::env::var("PEN_EVENT_PROFILE").ok();
    let _profiler = match &profile {
        Some(file) => dhat::Profiler::builder().file_name(file).build(),
        None => dhat::Profiler::builder().testing().build(),
    };
    let listed = std::env::var("PEN_EVENT_BRUSHES").ok();
    let brushes: Vec<&str> = listed.as_deref().map_or_else(|| BRUSHES.to_vec(), |l| l.split(',').collect());
    let mut failures = Vec::new();
    for entry in brushes {
        let (smudge, rest) = match entry.strip_prefix("smudge:") {
            Some(rest) => (true, rest),
            None => (false, entry),
        };
        let (name, size) = rest.split_once('@').map_or((rest, 0.4), |(n, s)| (n, s.parse().unwrap()));
        let Some((set, index)) = pick(name) else {
            failures.push(format!("no brush {name}"));
            continue;
        };
        s.set_tool(if smudge { procraft_core::ToolKind::Smudge } else { procraft_core::ToolKind::Paint });
        assert!(s.select_brush(set, index));
        s.set_brush_size(size);
        s.set_color(0.8, 0.2, 0.1);
        settle_big_pictures();
        frame(&s);
        let mut each = Vec::with_capacity(EVENTS);
        let before = allocated();
        s.touch_down(1, PointerKind::Stylus, vec![point(0)]);
        let down = allocated() - before;
        for e in 1..=EVENTS {
            let before = allocated();
            s.touch_moves(vec![TouchMove { id: 1, samples: vec![point(2 * e - 1), point(2 * e)] }]);
            each.push(allocated() - before);
            if e % EVERY == 0 {
                frame(&s);
            }
        }
        let before = allocated();
        s.touch_up(1, vec![point(2 * EVENTS + 1)]);
        let up = allocated() - before;
        frame(&s);
        let (first, last) = (&each[..100], &each[EVENTS - 100..]);
        let (m0, m1) = (median(first), median(last));
        let most = each.iter().copied().max().unwrap_or(0);
        println!(
            "{entry:<18} an event allocates {:6.1} KB at the start, {:6.1} KB at the end (most {:7.1} KB); down {:.1} KB, up {:.1} KB",
            m0 as f64 / 1024.0,
            m1 as f64 / 1024.0,
            most as f64 / 1024.0,
            down as f64 / 1024.0,
            up as f64 / 1024.0
        );
        // as much at the end as at the start, give or take, and never a buffer bigger than the
        // stroke's own lists doubling
        if m1 > 2 * m0 + (128 << 10) {
            failures.push(format!("{entry}: {m0} B an event at the start, {m1} B at the end"));
        }
        if most > 24 << 20 {
            failures.push(format!("{entry}: an event allocated {most} B"));
        }
        if up > lift_most(entry) << 20 {
            failures.push(format!("{entry}: the lift allocated {up} B"));
        }
        s.undo();
        frame(&s);
    }
    assert!(profile.is_some() || failures.is_empty(), "{failures:#?}");
}
