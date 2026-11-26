#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]
//! The brush library looked through and closed, as a phone of 4 GB froze and closed opening its
//! Gouache and Oils sets (discord): the stock library, a 2000 × 1200 canvas open on a GPU held to
//! wgpu's downlevel limits, the sample stroke of each brush of those two sets drawn as the
//! library's rows ask for them (530 × 162). Printed: what the GPU holds (its allocator's blocks)
//! over the canvas's own with the strokes drawn and once the library has closed, and the most the
//! canvas the strokes are drawn on kept of its work from one stroke to the next.
//!
//! Measured on an NVIDIA laptop GPU. Before: 68 MB held over the canvas's with the strokes drawn,
//! all of it still held with the library closed (until the system said it was short); the strokes'
//! canvas kept 52 MB of its work's buffers from one stroke to the next (16 MB of wet passes'
//! uniforms, 24 MB of a dual brush's carried floats, 8 MB of the rule's output). After: 4 MB once
//! the library closed, and 4 MB once 2.0 s had gone by with no stroke drawn, the library open; 11
//! MB of work's buffers kept at most, 2 MB of them lists of tiles. Bounds: what the library closed
//! may leave (readback buffers the device keeps, the allocator's rounding); what the strokes'
//! canvas may keep of its work, that measured and half again; and its lists alone, eight of 1 MB
//! (each past that is let go after its stroke).

use std::time::{Duration, Instant};

use procraft_core::CanvasSession;
use procraft_gpu::{Gpu, wgpu};

const SIDE: (u32, u32) = (2000, 1200);
const MB: u64 = 1 << 20;

/// The most the library closed may leave held over the canvas's own, MB.
const CLOSED_MOST: u64 = 16;

/// The most the strokes' canvas may keep of its work's buffers between strokes, and of that in
/// lists of tiles, MB.
const WORK_KEPT_MOST: u64 = 16;
const LISTS_KEPT_MOST: u64 = 8;

/// The GPU's allocator's blocks, once what was let go is gone.
fn reserved(gpu: &Gpu) -> u64 {
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    gpu.device.generate_allocator_report().map_or(0, |r| r.total_reserved_bytes)
}

/// A session on a canvas the library's size, on a GPU of wgpu's downlevel limits (none without
/// one).
fn session() -> Option<(std::sync::Arc<CanvasSession>, Gpu)> {
    let gpu = match Gpu::headless_limited(wgpu::Limits::downlevel_defaults()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipped: {e}");
            return None;
        }
    };
    let s = CanvasSession::new(SIDE.0, SIDE.1);
    s.use_gpu(gpu.clone());
    s.show_in(SIDE.0, SIDE.1);
    Some((s, gpu))
}

#[test]
fn the_library_closed_gives_back_what_its_strokes_took() {
    let Some((s, gpu)) = session() else { return };
    let canvas_only = reserved(&gpu);
    let library = s.brush_library();
    let (mut drawn, mut brushes, mut work_most, mut lists_most) = (0, 0, 0, 0);
    let started = Instant::now();
    for name in ["Gouache", "Oils"] {
        let set = library.iter().find(|set| set.name == name).unwrap();
        for b in &set.brushes {
            brushes += 1;
            if s.brush_preview(b.set, b.index, 530, 162).is_some() {
                drawn += 1;
            }
            // what the canvas kept for the next stroke
            if let Some([_, work, lists]) = s.preview_canvas_bytes() {
                work_most = work_most.max(work);
                lists_most = lists_most.max(lists);
            }
        }
    }
    let kept = s.preview_canvas_bytes();
    let open = reserved(&gpu);
    s.let_go_of_previews();
    let closed = reserved(&gpu);
    let over = |v: u64| v.saturating_sub(canvas_only) / MB;
    println!(
        "{drawn} of {brushes} strokes in {:.1} s: the GPU {} MB over the canvas's with them drawn, {} MB once \
         the library closed; the strokes' canvas kept {} MB of its work's buffers at most, {} MB in lists \
         of tiles (its textures {} MB)",
        started.elapsed().as_secs_f32(),
        over(open),
        over(closed),
        work_most / MB,
        lists_most / MB,
        kept.map_or(0, |[textures, _, _]| textures / MB),
    );
    assert_eq!(gpu.errors(), 0, "GPU validation errors");
    assert!(drawn * 10 >= brushes * 9, "{drawn} of {brushes} drawn");
    assert!(s.preview_canvas_bytes().is_none(), "the strokes' canvas let go of");
    assert!(over(closed) <= CLOSED_MOST, "{} MB held with the library closed", over(closed));
    assert!(work_most <= WORK_KEPT_MOST * MB, "{} MB of work kept", work_most / MB);
    assert!(lists_most <= LISTS_KEPT_MOST * MB, "{} MB of lists kept", lists_most / MB);
    // another set's stroke drawn as the library opens again (those drawn come from what is kept,
    // drawing nothing), then the artwork left
    let b = &library.iter().find(|set| set.name == "Inks").unwrap().brushes[0];
    assert!(s.brush_preview(b.set, b.index, 530, 162).is_some());
    assert!(s.preview_canvas_bytes().is_some(), "a canvas made again for the next");
    s.release_canvas();
    assert!(s.preview_canvas_bytes().is_none(), "the strokes' canvas let go of with the canvas");
}

#[test]
fn the_strokes_canvas_goes_a_while_after_the_last() {
    let Some((s, gpu)) = session() else { return };
    let canvas_only = reserved(&gpu);
    let library = s.brush_library();
    let set = library.iter().find(|set| set.name == "Gouache").unwrap();
    for b in set.brushes.iter().take(3) {
        assert!(s.brush_preview(b.set, b.index, 530, 162).is_some());
    }
    let last = Instant::now();
    assert!(s.preview_canvas_bytes().is_some(), "kept for the next stroke");
    // gone once no stroke has been drawn for a while, by itself, the library still open
    while s.preview_canvas_bytes().is_some() {
        assert!(last.elapsed() < Duration::from_secs(30), "still kept after {:?}", last.elapsed());
        std::thread::sleep(Duration::from_millis(50));
    }
    let gone = last.elapsed();
    let left = reserved(&gpu).saturating_sub(canvas_only) / MB;
    println!(
        "the strokes' canvas let go of {:.1} s after the last stroke, {left} MB left over the canvas's",
        gone.as_secs_f32()
    );
    assert!(gone >= Duration::from_millis(1500), "let go of after {gone:?}: too soon for the next row's");
    assert!(left <= CLOSED_MOST, "{left} MB left");
}
