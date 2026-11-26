#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
//! An artwork opened from the gallery and left, twenty times over, as the tablet apps do it (made
//! on a thread of its own, its brushes and plugins taken up, the gallery's zoom read, shown, kept
//! saved, painted on and recorded, saved to leave, the zoom's picture read, its canvas let go),
//! leaves no session alive and the heap where it was; so does a new canvas made and left. A session
//! something still holds after its canvas is let go keeps the rest of it (its brushes, its
//! document) for as long as the app runs: what one holds is printed too, to compare with a device's
//! memory (the iPad grew 7 MB each time it opened and closed an artwork). Printed: the heap after
//! the warm-up rounds and after the last, and what a left session still holds. Bounds: no session
//! alive after any round (within a moment, for the autosave's thread to see it go); the heap at
//! most [`GROWTH_MOST`] more a round after the warm-up. (A test of its own, as it counts every
//! allocation and every session the program makes.)

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use procraft_core::{
    ArtworkStore, CanvasSession, ColourHistory, Leaving, PenSample, Plugins, PointerKind, TimeLapse, live_sessions,
};
use procraft_engine::profile::Profile;
use procraft_gpu::{Gpu, wgpu};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The rounds, and those first that fill what is kept across artworks (the brushes' pictures, the
/// pipelines) before the heap is compared.
const ROUNDS: usize = 20;
const WARM_UP: usize = 4;
/// The most the heap may grow a round after the warm-up, bytes: a left session kept alive holds
/// far more (see the printed line).
const GROWTH_MOST: u64 = 48 << 10;
/// The canvas and the window it is shown in.
const SIDE: (u32, u32) = (800, 600);
const WINDOW: (u32, u32) = (1180, 820);
/// How long a session let go may take to go: the autosave's thread may be holding it a moment.
const GOING: Duration = Duration::from_secs(5);

/// The tests one at a time: each counts every session and allocation the program makes.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

const MB: f64 = (1u64 << 20) as f64;

/// What the app keeps across artworks: the GPU, its plugins, its brushes' and colour history's
/// files, its gallery, and the window a canvas is drawn into.
struct App {
    gpu: Gpu,
    window: wgpu::Texture,
    plugins: Arc<Plugins>,
    brushes: String,
    history: Arc<ColourHistory>,
    gallery: Arc<ArtworkStore>,
    root: PathBuf,
}

impl App {
    /// None (the test skipped) without a GPU.
    fn new(name: &str) -> Option<App> {
        let gpu = match Gpu::headless() {
            Ok(gpu) => gpu,
            Err(e) => {
                eprintln!("skipped: {e}");
                return None;
            }
        };
        procraft_core::keep_device(&gpu);
        let root = std::env::temp_dir().join(format!("procraft-left-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("brushes")).unwrap();
        let window = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("window"),
            size: wgpu::Extent3d { width: WINDOW.0, height: WINDOW.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Some(App {
            gpu,
            window,
            plugins: Plugins::with_bundled(root.join("plugins").to_string_lossy().into_owned()),
            brushes: root.join("brushes").to_string_lossy().into_owned(),
            history: ColourHistory::open(root.join("history.json").to_string_lossy().into_owned()),
            gallery: ArtworkStore::open(root.join("gallery").to_string_lossy().into_owned()),
            root,
        })
    }

    /// Draws `s` as the screen would, a frame at a time, until it wants no more.
    fn show(&self, s: &CanvasSession) {
        let view = self.window.create_view(&wgpu::TextureViewDescriptor::default());
        s.show_in(WINDOW.0, WINDOW.1);
        let began = Instant::now();
        for _ in 0..2000 {
            let again = s.draw(&view, wgpu::TextureFormat::Rgba8Unorm, WINDOW, Profile::Srgb, began);
            let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
            if again.is_none_or(|d| d > Duration::from_millis(100)) && !s.wants_frame() {
                return;
            }
        }
    }

    /// Takes `s` up as the canvas's screen does, shows it, paints a stroke on it and leaves it:
    /// saved, the gallery's zoom out read, its canvas let go and the session dropped. With `held`,
    /// the session is kept there after (as something in the app holding it would).
    fn visit(&self, s: Arc<CanvasSession>, dir: &str, held: Option<&mut Vec<Arc<CanvasSession>>>) {
        // as the canvas's model takes it up
        s.open_brushes(self.brushes.clone());
        s.use_plugins(Arc::clone(&self.plugins));
        s.use_colour_history(Arc::clone(&self.history));
        let _ = (s.brush_library(), s.current_brush(), s.info(), s.edits());
        // the gallery zooming into where it will be
        let _ = s.opening_view_matrix(WINDOW.0, WINDOW.1);
        s.use_gpu(self.gpu.clone());
        self.show(&s);
        Arc::clone(&s).start_autosave(dir.to_owned());
        // a stroke with the Pencil, recorded for the time-lapse
        let at = |k: usize| PenSample {
            x: 300.0 + k as f32 * 12.0,
            y: 300.0 + (k as f32 * 0.3).sin() * 80.0,
            pressure: 0.6,
            tilt: 0.3,
            orientation: 1.0,
            time_ms: 1000.0 + k as f64 * 8.0,
            roll: 0.0,
        };
        let _ = s.touch_down(1, PointerKind::Stylus, vec![at(0)]);
        for k in 1..40 {
            let _ = s.touch_move(1, vec![at(k)]);
        }
        let _ = s.touch_up(1, vec![at(40)]);
        self.show(&s);
        let lapse = TimeLapse::new(dir.to_owned());
        let _ = s.record_time_lapse(Arc::clone(&lapse));
        self.show(&s);
        // left: its time and brushes kept, saved whole, the zoom out's picture read, then its
        // canvas let go
        s.add_tracked_time(4500);
        let _ = s.keep_brush_choices(self.brushes.clone());
        assert_eq!(Arc::clone(&s).save_to_leave(dir.to_owned()), Leaving::Left);
        let _ = (s.info(), s.view_matrix(), s.shown_turns(), s.canvas_thumbnail(768));
        s.detach_window();
        s.stop_autosave();
        drop(lapse);
        s.release_canvas();
        if let Some(held) = held {
            held.push(s);
        }
    }
}

/// The sessions alive once those let go have gone (or [`GOING`] has passed).
fn live_once_gone() -> u64 {
    let since = Instant::now();
    loop {
        let live = live_sessions();
        if live == 0 || since.elapsed() > GOING {
            return live;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The heap now, bytes.
fn heap() -> u64 {
    dhat::HeapStats::get().curr_bytes as u64
}

/// Runs `round` [`ROUNDS`] times, each to be left with no session alive, and checks the heap
/// stays where it was after the warm-up; prints what one round's left session still held while it
/// was alive.
fn rounds(what: &str, mut round: impl FnMut(usize, Option<&mut Vec<Arc<CanvasSession>>>)) {
    let mut warm = 0;
    for k in 0..ROUNDS {
        if k == WARM_UP {
            warm = heap();
            // what a left session holds while something holds it: its canvas let go, the rest kept
            let mut held = Vec::new();
            round(k, Some(&mut held));
            let holding = heap().saturating_sub(warm);
            drop(held);
            assert_eq!(live_once_gone(), 0, "{what}: the held session didn't go once let go");
            eprintln!("{what}: a left session still held {:.1} MB while it was kept", holding as f64 / MB);
            warm = heap();
            continue;
        }
        round(k, None);
        assert_eq!(live_once_gone(), 0, "{what}: round {k} left a session alive");
    }
    let last = heap();
    let rounds = (ROUNDS - WARM_UP - 1) as u64;
    let growth = last.saturating_sub(warm) / rounds;
    eprintln!(
        "{what}: heap {:.1} MB after the warm-up, {:.1} MB after {ROUNDS} rounds: {:.1} KB a round (at most {} KB)",
        warm as f64 / MB,
        last as f64 / MB,
        growth as f64 / 1024.0,
        GROWTH_MOST >> 10
    );
    assert!(growth <= GROWTH_MOST, "{what}: the heap grew {growth} bytes a round");
}

/// A painted artwork of two layers in a new folder of the gallery.
fn painted_artwork(app: &App) -> String {
    let dir = app.gallery.new_folder(None);
    let s = CanvasSession::new(SIDE.0, SIDE.1);
    s.use_gpu(app.gpu.clone());
    app.show(&s);
    let _ = s.touch_down(1, PointerKind::Stylus, vec![PenSample { x: 100.0, y: 100.0, pressure: 1.0, ..sample() }]);
    let _ = s.touch_up(1, vec![PenSample { x: 600.0, y: 400.0, pressure: 1.0, ..sample() }]);
    let _ = s.add_layer();
    app.show(&s);
    s.save(dir.clone()).unwrap();
    drop(s);
    dir
}

fn sample() -> PenSample {
    PenSample { x: 0.0, y: 0.0, pressure: 0.5, tilt: 0.0, orientation: 0.0, time_ms: 0.0, roll: 0.0 }
}

#[test]
fn an_artwork_opened_and_left_again_and_again_leaves_nothing_behind() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let _profiler = dhat::Profiler::builder().testing().build();
    let Some(app) = App::new("open") else { return };
    let dir = painted_artwork(&app);
    assert_eq!(live_once_gone(), 0);
    rounds("opened", |_, held| {
        // opened off the main thread, as the gallery opens it
        let path = dir.clone();
        let s = std::thread::spawn(move || CanvasSession::open(path).unwrap()).join().unwrap();
        app.visit(s, &dir, held);
    });
    let _ = std::fs::remove_dir_all(&app.root);
}

#[test]
fn a_new_canvas_made_and_left_again_and_again_leaves_nothing_behind() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let _profiler = dhat::Profiler::builder().testing().build();
    let Some(app) = App::new("new") else { return };
    let mut made: Vec<String> = Vec::new();
    rounds("new", |_, held| {
        let s = CanvasSession::new(SIDE.0, SIDE.1);
        s.made_with_screen_turned(1);
        let _ = s.set_new_profile("Display P3".to_owned());
        // in the gallery from the start
        let dir = app.gallery.new_folder(None);
        s.save(dir.clone()).unwrap();
        app.visit(s, &dir, held);
        made.push(dir);
    });
    assert_eq!(made.len(), ROUNDS);
    let _ = std::fs::remove_dir_all(&app.root);
}
