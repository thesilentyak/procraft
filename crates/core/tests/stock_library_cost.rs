//! The brushes the app comes with are made quickly and hold little: their shapes' and grains'
//! stand-ins are drawn only when a brush wants them, and their big pictures only when a brush
//! that reads them is chosen, on a thread of their own, so making the library draws none. (A test of its own, as it counts every allocation the program
//! makes.)

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn the_stock_library_is_made_quickly_and_draws_no_pictures() {
    let profiler = dhat::Profiler::builder().testing().build();
    let started = std::time::Instant::now();
    // the brushes the app comes with, made ahead as the gallery does (no library kept here)
    let dir = std::env::temp_dir().join(format!("procraft-stock-cost-{}", std::process::id()));
    procraft_core::prepare_brushes(dir.to_string_lossy().into_owned());
    let took = started.elapsed();
    let stats = dhat::HeapStats::get();
    drop(profiler);
    eprintln!(
        "the stock library made in {} ms: {} KB held after, {} KB at the most while made",
        took.as_millis(),
        stats.curr_bytes >> 10,
        stats.max_bytes >> 10
    );
    // the brushes' settings and the stand-ins' measurements (some 24 MB when this was written,
    // nearly all of it kept: nothing drawn and let go), and no picture drawn (each shape or grain
    // 64 KB at the size brushes hold, 4 MB drawn at the most; one way of loading full-size pictures
    // with the library took 206 MB more and 20 s); counted, every allocation is slower here than
    // in the app (a tenth of a second there)
    assert!(stats.curr_bytes < 32 << 20, "{} bytes held", stats.curr_bytes);
    assert!(stats.max_bytes < stats.curr_bytes + (4 << 20), "{} bytes at the most", stats.max_bytes);
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");
}
