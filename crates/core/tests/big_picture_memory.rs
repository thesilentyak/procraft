#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The big pictures of our shapes and grains are drawn one at a time, on a thread of their own,
//! each within a bound of memory, and what drawing one holds is let go of before the next: brushes
//! chosen one after another (their pictures drawn as a first start draws a chosen brush's, before
//! the rest), the most the program holds over what it held before is printed for each, against
//! [`MOST_PER_PIXEL`] bytes a pixel of the biggest picture's side, beside the pictures held for
//! the brushes chosen. Drawing a 2048 picture held 77 MB at most while each of its blurs and what
//! it is ranked by took a picture's worth of its own; 61 MB with them added in place (106 and 90 MB
//! in all, beside those held). (A test of its own, as it counts every allocation the program makes.)

use std::time::{Duration, Instant};

use procraft_core::CanvasSession;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The most bytes drawing a big picture may hold at once, for each of its pixels.
const MOST_PER_PIXEL: u64 = 16;

/// The most bytes the pictures made for chosen brushes are held in, beside their drawing (as
/// `big_pictures::HELD_BYTES`, with a picture's levels over).
const HELD_MOST: u64 = 32 << 20;

/// How many brushes are chosen, one after another.
const BRUSHES: usize = 6;

/// What the program holds now, and the most it has held, bytes.
fn held() -> (u64, u64) {
    let stats = dhat::HeapStats::get();
    (stats.curr_bytes as u64, stats.max_bytes as u64)
}

/// Waits for the big pictures a brush just chosen asks for to be made.
fn settle() {
    let asked = Instant::now();
    while procraft_core::big_pictures_left() == 0 && asked.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(procraft_core::wait_for_big_pictures(Duration::from_secs(900)), "the big pictures weren't made");
}

#[test]
fn a_big_picture_is_drawn_within_a_bound_and_let_go_of_before_the_next() {
    let _profiler = dhat::Profiler::builder().testing().build();
    let s = CanvasSession::new(64, 64);
    // the brushes that read big pictures at their most size, those with the biggest first
    let mut big: Vec<(u32, u32, String, Vec<u32>)> = s
        .brush_library()
        .iter()
        .flat_map(|set| set.brushes.iter().map(|b| (b.set, b.index, b.name.clone())))
        .map(|(set, index, name)| (set, index, name, s.big_picture_sides(set, index)))
        .filter(|b| !b.3.is_empty())
        .collect();
    assert!(big.len() > 50, "{} brushes read big pictures", big.len());
    big.sort_by_key(|b| std::cmp::Reverse(b.3.iter().map(|&side| u64::from(side).pow(2)).sum::<u64>()));
    let chosen: Vec<_> = big.into_iter().step_by(7).take(BRUSHES).collect();
    let side = chosen.iter().flat_map(|b| b.3.iter().copied()).max().unwrap();
    let bound = u64::from(side).pow(2) * MOST_PER_PIXEL;
    let (start, _) = held();
    let mut peak_over = 0u64;
    for (set, index, name, sides) in &chosen {
        let (before, before_most) = held();
        assert!(s.select_brush(*set, *index));
        settle();
        let (after, after_most) = held();
        // the most held while its pictures were drawn, over what was held as it was chosen (shown
        // only where that passes the most so far: the count keeps the program's most)
        let over = after_most.saturating_sub(before);
        let new_most = after_most > before_most;
        peak_over = peak_over.max(after_most.saturating_sub(start));
        println!(
            "{name:<22} pictures {sides:?}: {} held at most while drawn, over {} before{}; {} held after",
            mb(over),
            mb(before.saturating_sub(start)),
            if new_most { "" } else { " (no new most)" },
            mb(after.saturating_sub(start))
        );
    }
    let (end, _) = held();
    println!(
        "{} brushes' pictures, the biggest {side} a side: at most {} held over the start (bound {} for the \
         drawing, {} for those held), {} held at the end",
        chosen.len(),
        mb(peak_over),
        mb(bound),
        mb(HELD_MOST),
        mb(end.saturating_sub(start))
    );
    // one picture drawn at a time, within its bound, its working buffers let go of before the next:
    // the most held never passes one drawing beside the pictures kept, however many are drawn
    assert!(peak_over <= bound + HELD_MOST, "{} held at most", mb(peak_over));
    assert!(end.saturating_sub(start) <= HELD_MOST + (8 << 20), "{} held at the end", mb(end.saturating_sub(start)));
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / f64::from(1 << 20))
}
