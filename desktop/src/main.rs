//! ProCraft on the desktop: the engine draws the canvas into the window, and the interface is
//! drawn over it with egui, on the same device.

// no console window behind the app on Windows (a debug build keeps it, for the log)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// The app's name, in its window, dialogs and settings: ProCraft Diffusion, the fork of ProCraft
/// with image generation. Its data lives apart from ProCraft's, in [`APP_DIR`] (Linux) or a folder
/// named after it.
pub const APP_NAME: &str = "ProCraft Diffusion";
/// The data folder's name on Linux.
pub const APP_DIR: &str = "procraft-diffusion";

mod actions;
mod adjust_panels;
mod adjustments;
mod alert;
mod animated_export;
mod animation;
mod canvas_info;
mod chrome;
mod collapse;
mod colour_drop;
mod colours;
mod compact_colours;
mod companion;
mod copy_paste;
mod crop;
mod curve;
mod custom_canvas;
mod edit_layout;
mod export_card;
mod files;
mod filter_list;
#[cfg(target_os = "macos")]
mod finder;
mod floating_panels;
mod gallery;
#[cfg(feature = "generation")]
mod generation;
#[cfg(feature = "generation")]
mod generation_studio;
#[cfg(feature = "generation")]
mod generation_styles;
#[cfg(feature = "generation")]
mod generation_tags;
mod gesture_controls;
mod gradient_map;
mod guide_editor;
mod held_layers;
mod icons;
mod input;
mod interface_pass;
mod layer_choice;
mod layers;
mod library;
mod licences;
mod lighting;
mod liquify;
mod look;
mod menu;
mod model_picture;
mod overlays;
mod panel;
mod pdf;
mod pen;
mod prefs;
mod projection;
mod questions;
mod quick_menu;
mod recolor;
mod reference;
mod screen;
mod selection;
mod settings;
mod shortcuts;
mod sound;
mod source_editor;
mod startup;
mod stroke_sound;
mod studio;
mod system_clipboard;
mod text;
mod textures;
mod timelapse;
mod tool_header;
mod touch_keyboard;
mod transform;
mod transform_numeric;
mod ui;
mod video;
mod window_watch;
mod zoom;

use std::sync::Arc;
use std::time::{Duration, Instant};

use procraft_core::CanvasSession;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::screen::Screen;
use crate::ui::{Action, Interface};

/// A new artwork's size when the screen's isn't known.
const FALLBACK_CANVAS: (u32, u32) = (1920, 1080);

/// A picture being made a new artwork: since when, the gallery folder it goes in, and its reading.
type ReadingPhoto = (Instant, Option<String>, std::thread::JoinHandle<Result<Arc<CanvasSession>, String>>);

/// An artwork in the gallery being opened: since when, its folder, the moving format it is
/// opened to be shared as (the gallery's Share), and its reading.
type ReadingArtwork =
    (Instant, String, Option<animated_export::Animated>, std::thread::JoinHandle<Result<Arc<CanvasSession>, String>>);

/// Artworks being brought in into a gallery folder: one read on a thread of its own (an artwork of
/// many layers takes seconds), since when, and those still to come.
struct Importing {
    since: Instant,
    stage: ImportStage,
    folder: Option<String>,
    rest: std::collections::VecDeque<Vec<std::path::PathBuf>>,
}

/// An artwork coming in: read, then (a package's) its time-lapse taken into its new folder, both
/// off the window's thread (a long time-lapse is hundreds of videos, every frame decoded).
enum ImportStage {
    Reading(std::thread::JoinHandle<Option<(Arc<CanvasSession>, Option<std::path::PathBuf>)>>),
    /// Read, waiting for the artwork open to be left (whether to leave it unsaved being asked).
    Read(Arc<CanvasSession>, Option<std::path::PathBuf>),
    TakingTimeLapse(std::thread::JoinHandle<()>, Arc<CanvasSession>, String),
}

impl ImportStage {
    fn is_finished(&self) -> bool {
        match self {
            ImportStage::Reading(t) => t.is_finished(),
            ImportStage::Read(..) => true,
            ImportStage::TakingTimeLapse(t, ..) => t.is_finished(),
        }
    }
}

impl Importing {
    /// The first of `artworks` (each its files) read on its own thread, into `folder`; none left,
    /// none.
    fn next(
        mut artworks: std::collections::VecDeque<Vec<std::path::PathBuf>>,
        folder: Option<String>,
    ) -> Option<Importing> {
        let paths = artworks.pop_front()?;
        let reading = std::thread::spawn(move || files::open_artwork_files(paths));
        Some(Importing { since: Instant::now(), stage: ImportStage::Reading(reading), folder, rest: artworks })
    }
}

/// On a laptop of two GPUs, the drivers give OpenGL the discrete one when the app says so with these
/// (NVIDIA's Optimus, AMD's PowerXpress), as Direct3D 12 and Vulkan are given the faster by wgpu
/// itself: OpenGL drew on the integrated one, its frames taking 100 to 480 ms. The linker exports
/// them from the .exe (build.rs).
#[cfg(windows)]
#[allow(unsafe_code, non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static NvOptimusEnablement: u32 = 1;
#[cfg(windows)]
#[allow(unsafe_code, non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

/// What the layers' textures may take on the GPU, bytes: past it, the layers not worked on are kept
/// compressed in memory until needed. A graphics card's (see [`layers_on_gpu`]).
const LAYERS_ON_GPU: u64 = 2 << 30;
/// The least they may take on a GPU that shares the computer's memory.
const LAYERS_ON_SHARED_GPU_LEAST: u64 = 512 << 20;

/// What the layers' textures may take on the GPU (see [`LAYERS_ON_GPU`]), on a GPU that has its own
/// memory or `shares` the computer's `memory` (bytes, none if the system doesn't say). A graphics
/// card holds 2 GiB of them. An integrated GPU has at most half the computer's memory (as Windows
/// gives it), which the system and every other app draw on too, and running out of it loses the
/// device (wgpu loses it on a copy it can't make room for): its layers take an eighth of the
/// computer's memory, 512 MiB to 2 GiB. A laptop of 8 GB gives them 1 GiB, which the canvas takes
/// for a device of little memory: its history's older steps kept compressed off the GPU, and fewer
/// of wet paint's marks to go back to.
fn layers_on_gpu(shares: bool, memory: Option<u64>) -> u64 {
    match (shares, memory) {
        (false, _) => LAYERS_ON_GPU,
        (true, Some(memory)) => (memory / 8).clamp(LAYERS_ON_SHARED_GPU_LEAST, LAYERS_ON_GPU),
        (true, None) => LAYERS_ON_GPU / 2,
    }
}

/// The computer's memory, bytes; none if the system doesn't say.
#[cfg(windows)]
#[allow(unsafe_code)]
fn memory_total() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: `status` is a MEMORYSTATUSEX with its length set, as the call asks
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some(status.ullTotalPhys).filter(|&b| b > 0)
}

/// The computer's memory, bytes; none if the system doesn't say.
#[cfg(target_os = "macos")]
fn memory_total() -> Option<u64> {
    Some(objc2_foundation::NSProcessInfo::processInfo().physicalMemory()).filter(|&b| b > 0)
}

/// The computer's memory, bytes; none if the system doesn't say.
#[cfg(not(any(windows, target_os = "macos")))]
fn memory_total() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = meminfo.lines().find_map(|l| l.strip_prefix("MemTotal:"))?;
    let kb: u64 = line.trim().strip_suffix("kB")?.trim().parse().ok()?;
    Some(kb * 1024).filter(|&b| b > 0)
}

/// How long an artwork opens before it says so, as the tablet's.
const SLOW_OPENING: Duration = Duration::from_millis(500);

/// How often the window's title is looked at again while the artwork open has changes no save has
/// yet (see [`App::follow_title`]).
const TITLE_CHECK: Duration = Duration::from_secs(1);

/// A window event (a frame among them) that takes longer than this is logged: the window waits on
/// it.
const SLOW_EVENT: Duration = Duration::from_millis(100);

/// Logs, once dropped, a window event that took longer than [`SLOW_EVENT`] (a dialog it waited on
/// counts too): what holds the window up shows in the log.
struct Timed(&'static str, Instant);

impl Drop for Timed {
    fn drop(&mut self) {
        let took = self.1.elapsed();
        if took > SLOW_EVENT {
            log::warn!("{} took {} ms", self.0, took.as_millis());
        }
    }
}
/// The veil over the window while it does.
const OPENING_VEIL: u32 = 0x66000000;

/// Over the window while a slow artwork opens: veiled, saying so (and taking the presses).
fn opening_veil(ctx: &egui::Context) {
    veil(ctx, "Opening…");
}

/// Over the window while something slow is under way: veiled, saying `what` (and taking the
/// presses).
fn veil(ctx: &egui::Context, what: &str) {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("opening")).fixed_pos(screen.min).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.allocate_rect(screen, egui::Sense::click_and_drag());
        ui.painter().rect_filled(screen, 0, look::argb(OPENING_VEIL));
        let ink = look::argb(look::pc::TEXT);
        look::text(ui.painter(), screen.center(), egui::Align2::CENTER_CENTER, what, 17.0, look::Weight::Regular, ink);
    });
}

/// How long the canvases' pipelines are made before the window says so (on a slow GPU's shader
/// compiler they take a minute or more, and a canvas opened meanwhile waits for them).
const CANVASES_SLOW: Duration = Duration::from_secs(2);
/// What the window says meanwhile.
const GETTING_READY: &str = "Getting the canvas ready…";

/// What a canvas's veil says while it opens: while the pipelines it waits for are still being
/// made (`waiting`), that.
fn opening_words(waiting: bool) -> &'static str {
    if waiting { GETTING_READY } else { "Opening…" }
}

/// At the gallery's foot while the canvases' pipelines are still being made: a note saying so,
/// over the gallery, which takes its presses as ever.
fn getting_ready_note(ctx: &egui::Context) {
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("getting ready")));
    let ink = look::argb(look::pc::TEXT);
    let wide = painter.layout_no_wrap(GETTING_READY.to_owned(), look::Weight::Regular.font(15.0), ink).size().x;
    let note = egui::Rect::from_center_size(
        egui::pos2(screen.center().x, screen.bottom() - 24.0 - 18.0),
        egui::vec2(wide + 36.0, 36.0),
    );
    painter.rect_filled(note, 18.0, look::argb(look::pc::BAR));
    look::text(&painter, note.center(), egui::Align2::CENTER_CENTER, GETTING_READY, 15.0, look::Weight::Regular, ink);
}

fn main() {
    // the log, to the app folder's file too, and a crash's message into it
    startup::init();
    // artworks being opened, noted until they show: one the app ended while opening is asked about
    // when next picked, not opened at once into the same end
    procraft_core::set_open_marks_file(files::app_dir().join("opening.txt").to_string_lossy().into_owned());
    // and the log of a run that closed while it opened one kept apart, to share when it is asked about
    procraft_core::keep_desktop_logs_of_ended_runs(&files::app_dir());
    // the layers may take half the computer's memory, as on the tablet (so how many a canvas holds
    // grows with it), their textures up to 2 GiB of it; the composite is made around the layers
    // not being worked on, so a frame composites only those
    if let Some(total) = memory_total() {
        procraft_core::set_layer_budget(total / 2);
    }
    procraft_core::set_resident_budget(LAYERS_ON_GPU);
    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            startup::stopped(&format!("it couldn't make its window's event loop: {e}"));
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    // the devices' own raw input, which nothing here reads: under X11 a tablet's pen sends it for
    // each of its moves, besides the window's own events
    #[cfg(target_os = "linux")]
    event_loop.listen_device_events(winit::event_loop::DeviceEvents::Never);
    // a Mac hands the files opened from the Finder to the app as it runs, not in its arguments
    #[cfg(target_os = "macos")]
    finder::watch();
    // the colour profiles imported before
    procraft_core::set_profile_folder(files::app_dir().join("profiles").to_string_lossy().into_owned());
    let mut app = App {
        wake: event_loop.create_proxy(),
        started: Instant::now(),
        last_frame: Instant::now(),
        input_frame: false,
        gap: Duration::from_millis(15),
        again: None,
        finishing_since: None,
        stroke_sound: stroke_sound::StrokeSound::default(),
        opening: None,
        importing: None,
        reading_artwork: None,
        share_once_made: None,
        reading_photo: None,
        screen: None,
        session: None,
        artwork: None,
        gallery: gallery::Gallery::new(),
        ui: Interface::default(),
        input: input::Input::default(),
        title: TitleWatch::default(),
        zoom: None,
        projection: None,
        displays_seen: None,
        // files the app was opened with ("Open with", or dropped on its icon) come in as a drop on
        // the gallery does
        dropped: std::env::args_os().skip(1).map(std::path::PathBuf::from).filter(|p| p.is_file()).collect(),
        tap_held: None,
        interface_touches: input::InterfaceTouches::default(),
        modifiers: winit::keyboard::ModifiersState::empty(),
        maximised_before_full_screen: false,
        window_watch: window_watch::WindowWatch::default(),
        closed: false,
        leave_question: questions::LeaveQuestion::default(),
        left_unsaved: None,
        close_asked: false,
        open_anyway: questions::OpenAnyway::default(),
    };
    // what a plugin brings in from its folder, as the gallery first shows
    app.gallery.bring_in(&app.ui.plugins_shared());
    // a panic on the window's thread ends the app: said, rather than the window just vanishing
    // (its message and place are in the log already)
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| event_loop.run_app(&mut app))) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => startup::stopped(&format!("its window's event loop stopped: {e}")),
        Err(panic) => {
            if let Some(screen) = &app.screen {
                screen.window.set_visible(false);
            }
            startup::stopped(procraft_gpu::panic_message(panic.as_ref()));
            // what the app held is let go by the system: a driver that panicked may not take it back
            std::process::exit(101);
        }
    }
}

/// Whether `window` is an X11 one.
fn is_x11(window: &Window) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    matches!(window.window_handle().map(|h| h.as_raw()), Ok(RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)))
}

struct App {
    /// Wakes the loop from other threads (the pen's, on Wayland).
    wake: winit::event_loop::EventLoopProxy<()>,
    started: Instant,
    /// When the last frame was drawn, and when another is wanted (the canvas or the interface still
    /// moving, or a hold to be timed).
    last_frame: Instant,
    again: Option<Instant>,
    /// Input came that wants a frame sooner than the screen shows another after the last (see
    /// [`frame_gap`]): drawn once it does. And that gap, as the last frame found it.
    input_frame: bool,
    gap: Duration,
    /// Since when a plugin has been finishing a fill.
    finishing_since: Option<Instant>,
    /// The strokes' sound, played while a plugin makes it.
    stroke_sound: stroke_sound::StrokeSound,
    /// An artwork's canvas being made on a thread of its own as it opens, and since when.
    opening: Option<(Instant, std::thread::JoinHandle<()>)>,
    /// Artworks being brought in, read on a thread of their own one after another.
    importing: Option<Importing>,
    /// An artwork in the gallery being opened, read on a thread of its own (an artwork of many
    /// layers holds hundreds of MB): since when, its folder, and the reading.
    reading_artwork: Option<ReadingArtwork>,
    /// The gallery's animated Share of the artwork just opened, started once its canvas is made
    /// (the preview's frames and the file are read from it).
    share_once_made: Option<animated_export::Animated>,
    /// A picture made a new artwork (the gallery's Photo), read on a thread of its own: since
    /// when, the gallery folder it goes in, and the reading.
    reading_photo: Option<ReadingPhoto>,
    screen: Option<Screen>,
    /// The artwork open and its folder in the gallery; none while the gallery shows.
    session: Option<Arc<CanvasSession>>,
    artwork: Option<String>,
    gallery: gallery::Gallery,
    ui: Interface,
    input: input::Input,
    /// The window's title.
    title: TitleWatch,
    /// The gallery zooming into the artwork opening, or out of the one closing.
    zoom: Option<zoom::Zoom>,
    /// Project canvas's window on a second display, and when the displays were last looked at.
    projection: Option<projection::Projection>,
    displays_seen: Option<Instant>,
    /// Files dropped on the window (or the app opened with), taken in together at the next frame
    /// (a model with its material file and images comes in one drop).
    dropped: Vec<std::path::PathBuf>,
    /// Tap Assistance: a lone finger's touch, where it landed and when, while it is held there.
    tap_held: Option<(u64, winit::dpi::PhysicalPosition<f64>, Instant)>,
    /// Which touch the interface follows: a finger on it beside a hand resting on the canvas.
    interface_touches: input::InterfaceTouches,
    /// The keyboard's modifiers held now, for Alt+Enter.
    modifiers: winit::keyboard::ModifiersState,
    /// The window was maximised as it went full screen, and is maximised again as it comes back.
    maximised_before_full_screen: bool,
    /// Whether the window is full screen and counted as hidden, as last seen (for the log).
    window_watch: window_watch::WindowWatch,
    /// The window was closed (the artwork open kept): the app's end needs nothing more.
    closed: bool,
    /// Whether to leave the artwork open without saving it, asked once it couldn't be saved; what
    /// the person chose to leave it unsaved for (done next, the artwork left as it is: that leave
    /// alone saves nothing); and to close the window (at the loop's next turn).
    leave_question: questions::LeaveQuestion,
    left_unsaved: Option<questions::Then>,
    close_asked: bool,
    /// Whether to open an artwork whose opening ended the app last time, asked over the gallery:
    /// its folder, and to share it as what (see [`App::read_artwork`]).
    open_anyway: questions::OpenAnyway<(String, Option<animated_export::Animated>)>,
}

/// The window's title as the artwork open is: its name, marked while it has changes no save has
/// yet, and looked at again a second later until one has them (autosaves come in on their own),
/// with no frame drawn for it: on OpenGL the swap of a frame drawn after the window had been still
/// a moment held the window's thread 90 to 450 ms (the driver's), each second until the autosave.
#[derive(Default)]
struct TitleWatch {
    /// The title as last set.
    shown: String,
    /// When it is looked at again, if it is.
    check_at: Option<Instant>,
}

impl TitleWatch {
    /// The artwork called `name`, `saved` or not, at `now`: the title to set, if it changed; looked
    /// at again [`TITLE_CHECK`] later while it isn't saved.
    fn follow(&mut self, name: &str, saved: bool, now: Instant) -> Option<String> {
        let unsaved = if saved { "" } else { " •" };
        let title = format!("{name}{unsaved} — {APP_NAME}");
        self.check_at = (!saved).then(|| now + TITLE_CHECK);
        (title != self.shown).then(|| {
            self.shown.clone_from(&title);
            title
        })
    }

    /// Whether it is time to look at it again.
    fn due(&self, now: Instant) -> bool {
        self.check_at.is_some_and(|at| now >= at)
    }

    /// The gallery's title, set if it isn't already; nothing to look at again.
    fn gallery(&mut self) -> Option<&'static str> {
        self.check_at = None;
        (self.shown != GALLERY_TITLE).then(|| {
            self.shown = GALLERY_TITLE.to_owned();
            GALLERY_TITLE
        })
    }
}

/// The window's title over the gallery.
const GALLERY_TITLE: &str = APP_NAME;

/// The app's icon, for the window's title bar and the task bar.
fn window_icon() -> Option<winit::window::Icon> {
    let picture = image::load_from_memory(include_bytes!("../../assets/app-icon/procraft-256.png")).ok()?;
    let rgba = picture.into_rgba8();
    let (width, height) = rgba.dimensions();
    winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

/// Whether `key` pressed with `modifiers` held is the window's full screen key: F11, or off a Mac
/// Alt+Enter as well (Ctrl with Alt is AltGr, typing).
fn full_screen_key(key: &winit::keyboard::Key, modifiers: winit::keyboard::ModifiersState) -> bool {
    use winit::keyboard::{Key, NamedKey};
    match key {
        Key::Named(NamedKey::F11) => true,
        Key::Named(NamedKey::Enter) => {
            !cfg!(target_os = "macos") && modifiers.alt_key() && !modifiers.control_key() && !modifiers.super_key()
        }
        _ => false,
    }
}

/// The window fills its screen without its frame, or goes back to a window. On Windows a maximised
/// window is put back to its own size first and maximised again as it comes back: left maximised,
/// Windows keeps it to the screen less the task bar, and the task bar's strip shows black.
fn toggle_full_screen(window: &Window, maximised_before: &mut bool) {
    let leaving = window.fullscreen().is_some();
    log::info!("full screen asked {} by its key", if leaving { "off" } else { "on" });
    if leaving {
        pen::set_full_screen(false);
        window.set_fullscreen(None);
        if std::mem::take(maximised_before) {
            window.set_maximized(true);
        }
        return;
    }
    *maximised_before = cfg!(windows) && window.is_maximized();
    if *maximised_before {
        window.set_maximized(false);
    }
    pen::set_full_screen(true);
    window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
}

/// Tap Assistance holds a finger's touch where it landed this long.
const TAP_HOLD: std::time::Duration = std::time::Duration::from_millis(300);
/// How far it may drift meanwhile and still be held (points): farther, it is a scroll or a drag.
const TAP_DRIFT: f64 = 24.0;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.screen.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(APP_NAME)
            .with_window_icon(window_icon())
            .with_inner_size(LogicalSize::new(1440.0, 900.0))
            .with_min_inner_size(LogicalSize::new(640.0, 480.0))
            // opened for a test (PROCRAFT_BACKGROUND set), it leaves the keyboard where it was
            .with_active(std::env::var_os("PROCRAFT_BACKGROUND").is_none());
        // on Linux the desktop's menus match the window to the app's entry (procraft.desktop)
        #[cfg(target_os = "linux")]
        let attributes = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            use winit::platform::x11::WindowAttributesExtX11;
            let attributes = WindowAttributesExtWayland::with_name(attributes, "procraft", "procraft");
            WindowAttributesExtX11::with_name(attributes, "procraft", "ProCraft")
        };
        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                startup::stopped(&format!("it couldn't open its window: {e}"));
                event_loop.exit();
                return;
            }
        };
        // the file dialogs open over it
        files::dialogs_over(&window);
        // a pen's lean, which winit's touches leave out (on Wayland the whole pen)
        pen::watch(&window, self.wake.clone());
        // an X11 window manager may hold each touch back and hand it on whole
        input::set_whole_touches(is_x11(&window));
        // the brushes read while the gallery shows, so the first artwork opens without reading them
        let read = std::thread::Builder::new().name("procraft-read-brushes".to_owned());
        if let Err(e) = read.spawn(|| procraft_core::prepare_brushes(files::brushes_dir())) {
            log::warn!("reading the brushes ahead: {e}");
        }
        // in this machine's order, from the backend the last run used, or the next if that one
        // was at risk when it stopped
        let order = startup::backend_order();
        let from = startup::gpu_start(order);
        // the canvases' pipelines kept between runs, where the GPU can
        procraft_gpu::keep_pipelines_in(files::app_dir());
        // what the start waits for logged while it does
        startup::watch_start(self.wake.clone());
        let tried = startup::gpu_tried();
        let display = event_loop.owned_display_handle();
        let screen = match Screen::new(window, &display, order, from, tried, startup::gpu_trying, self.wake.clone()) {
            Ok((s, backend)) => {
                startup::gpu_made(backend);
                // on OpenGL, whether the drivers gave it the GPU Direct3D 12 would draw with
                startup::gpu_drawing(&s.gpu.adapter.get_info());
                // the layers' room on the GPU, by whether its memory is the computer's (before any
                // canvas is made)
                let layers = layers_on_gpu(s.gpu.shares_memory(), memory_total());
                log::info!("the layers' textures may take {} MB on the GPU", layers >> 20);
                procraft_core::set_resident_budget(layers);
                s
            }
            Err(e) => {
                log::error!("no GPU for the window:\n{e}");
                files::tell("ProCraft can't draw", &startup::no_gpu_text(&e));
                event_loop.exit();
                return;
            }
        };
        // Screen Size's artwork is the screen's size
        self.gallery.screen_px = screen
            .window
            .current_monitor()
            .map(|m| m.size())
            .filter(|s| s.width > 0 && s.height > 0)
            .map_or(FALLBACK_CANVAS, |s| (s.width, s.height));
        // the app opens on the gallery
        screen.window.request_redraw();
        self.screen = Some(screen);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // X11 gives each axis of a pen's move (its pressure, its tilts) as an event of its own as
        // well, which nothing here reads: several for every move a tablet's pen makes
        if matches!(event, WindowEvent::AxisMotion { .. }) {
            return;
        }
        let _timed =
            Timed(if event == WindowEvent::RedrawRequested { "a frame" } else { "a window event" }, Instant::now());
        if self.projection.as_ref().is_some_and(|p| p.window.id() == id) {
            self.projection_event(event);
            return;
        }
        let Some(event) = self.tap_assist(event) else { return };
        let Some(screen) = self.screen.as_mut() else { return };
        let session = self.session.clone();
        // a finger or a pen pressing brings the on-screen keyboard up for a field it focuses (a
        // tablet's pen that moves the mouse as well as the system's pen)
        match &event {
            WindowEvent::Touch(t) if t.phase == winit::event::TouchPhase::Started => {
                screen.touch_keyboard.touched(Instant::now());
            }
            WindowEvent::MouseInput { state: winit::event::ElementState::Pressed, .. } => {
                screen.touch_keyboard.clicked(crate::pen::mouse_pen().is_some(), Instant::now());
            }
            _ => {}
        }
        // on the canvas Ctrl+X, C and V are the app's shortcuts (a layer cut, copied or pasted,
        // from a clipboard of its own, the system's perhaps empty); a text field keeps them for
        // the system's clipboard
        let canvas_keys = session.is_some() && !screen.egui.egui_ctx().egui_wants_keyboard_input();
        screen.egui.set_clipboard_shortcuts(!canvas_keys);
        // a finger landing on the interface while a hand rests on the canvas takes the pointer over
        let canvas = self.ui.canvas_rect;
        let heard = self.interface_touches.route(event.clone(), |p| screen.over_interface(p, canvas));
        let mut response = egui_winit::EventResponse { consumed: false, repaint: false };
        for each in &heard {
            let one = screen.egui.on_window_event(&screen.window, each);
            response.consumed |= one.consumed;
            response.repaint |= one.repaint;
        }
        // egui counts every redraw as wanting another; the frames it does want come from its
        // repaint delay, paced in `about_to_wait`
        if response.repaint && event != WindowEvent::RedrawRequested {
            redraw_paced(&screen.window, self.last_frame, self.gap, &mut self.input_frame);
        }
        // F11, or Alt+Enter off a Mac: the window fills its screen without its frame (a pen
        // display's whole screen for the canvas, as the tablet's), or goes back to a window
        if let WindowEvent::ModifiersChanged(m) = &event {
            self.modifiers = m.state();
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state.is_pressed()
            && !key.repeat
            && full_screen_key(&key.logical_key, self.modifiers)
        {
            toggle_full_screen(&screen.window, &mut self.maximised_before_full_screen);
        }
        match event {
            WindowEvent::CloseRequested => {
                // the artwork open is kept in the gallery, and files still being written finish
                let waits = self.hide_while_opening();
                if self.leave_artwork(Some(questions::Then::Close)) {
                    self.close_window(event_loop);
                } else if waits && let Some(screen) = &self.screen {
                    screen.window.set_visible(true);
                }
            }
            WindowEvent::Resized(size) => {
                screen.resize(size.width, size.height);
                if let Some(session) = &session {
                    // the moves waiting were made in the window as it was
                    self.input.flush(session);
                    session.resize_window(size.width, size.height);
                    // minimised, time on the artwork stops counting; back, it counts again
                    session.set_tracking(!screen.minimised());
                }
                screen.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(session) = &session {
                    self.input.flush(session);
                    session.set_display_scale(scale_factor as f32);
                    let interface = scale_factor as f32 * screen.egui.egui_ctx().zoom_factor();
                    self.ui.place_canvas(session, interface);
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            WindowEvent::DroppedFile(path) => {
                self.dropped.push(path);
                screen.window.request_redraw();
            }
            // shown again: drawn at once, for the frames missed while it was hidden (a Mac's
            // surface gives none while the system counts the window as hidden)
            WindowEvent::Occluded(occluded) => {
                let (draw, line) = self.window_watch.occluded(occluded, Instant::now());
                if let Some(line) = line {
                    log::info!("{line}");
                }
                if draw {
                    screen.window.request_redraw();
                }
            }
            event => {
                let Some(session) = session else { return };
                // with a popover open, a press on the canvas only closes it (a panel alone lets it
                // through, to wait there to be a tap or a drag); while the artwork is being
                // exported, or a plugin finishes a fill, none reaches it (nothing either would end is
                // started)
                let through = self.ui.popover_open && self.ui.panel_lets_through;
                session.set_touch_panel_open(through);
                let canvas = self.ui.canvas_rect;
                // asked only of a press (a stroke's moves would wait on the canvas each)
                // (nor while Edit Layout moves the bars: the canvas takes no press then)
                let blocked = || {
                    (self.ui.popover_open && !through)
                        || self.ui.editing_layout()
                        || files::exporting_since().is_some()
                        || session.pour_finishing()
                };
                let over = |p: (f32, f32)| blocked() || screen.over_interface(p, canvas);
                if self.input.on_event(&session, &event, response.consumed, over) {
                    redraw_paced(&screen.window, self.last_frame, self.gap, &mut self.input_frame);
                }
            }
        }
    }

    /// A pen read by the app itself moved: its touches and hovering, as winit's would come.
    fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
        // (the start's watch asks too)
        startup::answered();
        let Some(window) = self.screen.as_ref().map(|s| (s.window.id(), s.window.scale_factor())) else { return };
        for event in pen::take_events(window.1) {
            self.window_event(event_loop, window.0, event);
        }
    }

    /// The app ending without its window closed first (a Mac's Quit, from the app menu or
    /// Cmd+Q, ends it with no close asked): the artwork open kept as closing the window keeps it.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // the backend did its run: the next starts on it again
        startup::gpu_ended();
        // before winit closes the connection the pen is read on
        pen::stop();
        if std::mem::replace(&mut self.closed, true) {
            // (the brushes kept since, if any, written whole)
            files::finish_writing();
            return;
        }
        self.hide_while_opening();
        // (nothing can be asked as it ends)
        if !self.leave_artwork(None) {
            log::error!("the artwork open couldn't be kept as the app ended");
        }
        files::finish_writing();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // files opened from the Finder meanwhile come in as files dropped on the window do
        #[cfg(target_os = "macos")]
        {
            let opened = finder::take();
            if !opened.is_empty() {
                self.dropped.extend(opened);
                if let Some(screen) = &self.screen {
                    screen.window.request_redraw();
                }
            }
        }
        if self.screen.as_ref().is_some_and(|s| s.gpu.is_lost()) {
            self.gpu_lost(event_loop);
            return;
        }
        // the window gone full screen or back, by its key or the system's own (a Mac's green
        // button): logged
        if let Some(screen) = &self.screen {
            let size = screen.window.inner_size();
            if let Some(line) = self.window_watch.full_screen(screen.window.fullscreen().is_some(), size.into()) {
                log::info!("{line}");
            }
        }
        // the window's close went on with once the artwork was left without saving
        if std::mem::take(&mut self.close_asked) && self.leave_artwork(Some(questions::Then::Close)) {
            self.close_window(event_loop);
            return;
        }
        // the interface's last frame couldn't go up to the GPU: what is kept is let go of and it is
        // drawn again, and frame after frame missed, the app starts again as on a device lost
        match self.screen.as_mut().map(Screen::take_after_frame) {
            Some(interface_pass::AfterFrame::LetGoAndRetry) => self.let_go_of_kept(),
            Some(interface_pass::AfterFrame::StartAgain) => {
                log::error!(
                    "the interface's frames couldn't go up to the GPU {} times in a row",
                    interface_pass::MISSES_MOST
                );
                self.gpu_lost(event_loop);
                return;
            }
            Some(interface_pass::AfterFrame::Fine) | None => {}
        }
        if let Some(fine) = self.screen.as_mut().and_then(Screen::canvases_made) {
            self.canvases_made(fine, event_loop);
            // the gallery's note that they are being made goes
            if let Some(screen) = &self.screen {
                screen.window.request_redraw();
            }
        }
        // what the files written meanwhile had to say; looked at again soon while any is writing
        files::tell_what_was_told();
        if files::still_writing() {
            let soon = Instant::now() + Duration::from_millis(250);
            self.again = Some(self.again.map_or(soon, |due| due.min(soon)));
        }
        self.sync_projection(event_loop);
        // the title looked at again, with no frame drawn for it
        if self.title.due(Instant::now()) {
            self.follow_title();
        }
        let title_at = self.title.check_at;
        let Some(screen) = self.screen.as_ref() else { return };
        let Some(session) = self.session.as_ref() else {
            // the gallery: only the interface's own wants, and input's once a refresh
            let input = self.input_frame.then(|| self.last_frame + self.gap);
            match [self.again, input].into_iter().flatten().min() {
                Some(due) if Instant::now() >= due => screen.window.request_redraw(),
                Some(due) => {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(due));
                    return;
                }
                None => {}
            }
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        let mut deadline = self.input.next_deadline(session);
        if let Some(at) = title_at {
            deadline = Some(deadline.map_or(at, |d| d.min(at)));
        }
        // the touches' waits (a hold, a repeat) run out
        if self.input.tick(session) || !self.input.commands.is_empty() {
            screen.window.request_redraw();
        } else if let Some(due) = {
            // what moves by itself (the selection's bands, a fade), and input that came meanwhile,
            // draws once a refresh, not as fast as frames can be made
            let wanted = ((session.wants_frame() || self.input_frame) && !screen.minimised()).then(Instant::now);
            match (self.again, wanted) {
                (Some(again), Some(wanted)) => Some(again.min(wanted)),
                (again, wanted) => again.or(wanted),
            }
        } {
            let due = due.max(self.last_frame + self.gap);
            if Instant::now() >= due {
                screen.window.request_redraw();
            } else {
                deadline = Some(deadline.map_or(due, |d| d.min(due)));
            }
        }
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

impl App {
    /// Shows `session` in the window, in place of the artwork open.
    fn show(&mut self, session: Arc<CanvasSession>) {
        let Some(screen) = self.screen.as_mut() else { return };
        let size = screen.window.inner_size();
        let scale = screen.window.scale_factor() as f32;
        session.set_display_scale(scale);
        // the system's ask for less motion: a dropped colour's fill shows at once
        session.set_reduce_motion(colour_drop::reduce_motion());
        // until it shows, a driver crashing counts against the backend (unless a canvas has drawn
        // on it before)
        startup::canvas_opening();
        // its layers go up on a thread of their own (an artwork of many takes seconds), the window
        // drawing meanwhile
        let (gpu, opened) = (screen.gpu.clone(), session.clone());
        self.opening = Some((Instant::now(), std::thread::spawn(move || opened.use_gpu(gpu))));
        // the brushes as the last artwork left them
        if let Some(last) = &self.session {
            files::keep_brushes(last);
        }
        session.open_brushes(files::brushes_dir());
        self.ui.opened(&session, scale * screen.egui.egui_ctx().zoom_factor());
        session.show_in(size.width, size.height);
        self.input = input::Input::default();
        // time on it counts while it is shown (not while the window is minimised)
        session.set_tracking(!screen.minimised());
        self.session = Some(session);
        screen.window.request_redraw();
    }

    /// The brush studio's drawing pad: its canvas made when the studio opens (the size the pad
    /// shows at, on this device), drawn into its picture each frame, and let go when it closes.
    /// True while the pad wants another frame.
    fn studio_pad(&mut self, session: &Arc<CanvasSession>) -> bool {
        let Some(screen) = self.screen.as_mut() else { return false };
        let studio = &mut self.ui.studio;
        if !studio.open {
            // closing the studio lets its texture go first: the picture goes whether or not
            studio.pad_texture = None;
            screen.drop_pad();
            return false;
        }
        let ppp = screen.egui.egui_ctx().pixels_per_point();
        let Some(rect) = studio.pad_rect else { return true };
        let size = ((rect.width() * ppp).round() as u32, (rect.height() * ppp).round() as u32);
        if size.0 < 2 || size.1 < 2 {
            return true;
        }
        if studio.pad.is_none() {
            // a new pad is shown afresh: the picture an earlier one was shown in (the studio closed
            // without it being let go) would be drawn into without the new pad ever being told its
            // size and view, and the pad would take no strokes
            screen.drop_pad();
            let pad = CanvasSession::new(size.0, size.1);
            pad.use_gpu(screen.gpu.clone());
            pad.set_display_scale(ppp);
            studio.start_pad(pad, session);
        }
        let Some(pad) = studio.pad.clone() else { return false };
        studio.pad_texture = Some(screen.draw_pad(&pad, size, self.started));
        pad.wants_frame()
    }

    /// Runs the interface, draws the canvas and the interface over it, and does what was asked.
    fn frame(&mut self) {
        if self.screen.is_none() {
            return;
        }
        // on OpenGL nothing is drawn while the canvases' pipelines are made (see
        // `Screen::gl_busy`), and nothing that would draw is taken in: looked at again soon (the
        // window answers meanwhile)
        if self.screen.as_ref().is_some_and(Screen::gl_busy) {
            self.again = Some(Instant::now() + Duration::from_millis(100));
            return;
        }
        self.take_dropped();
        self.imported();
        // (a leave without saving chosen is done by now, or not at all)
        self.left_unsaved = None;
        self.artwork_read();
        self.photo_read();
        let Some(screen) = self.screen.as_mut() else { return };
        let Some(session) = self.session.clone() else {
            self.gallery_frame();
            return;
        };
        self.last_frame = Instant::now();
        self.input_frame = false;
        self.gap = frame_gap(&screen.window);
        // the canvas made: drawn from now on; a slow one says it is opening
        if self.opening.as_ref().is_some_and(|(_, made)| made.is_finished())
            && let Some((_, made)) = self.opening.take()
        {
            let _ = made.join();
        }
        // the gallery's animated Share, now there is a canvas to read the frames from
        if self.opening.is_none()
            && let Some(format) = self.share_once_made.take()
        {
            self.ui.share_animated_from_gallery(format);
        }
        let slow = self.opening.as_ref().is_some_and(|(since, _)| since.elapsed() >= SLOW_OPENING)
            || self.importing.as_ref().is_some_and(|i| i.since.elapsed() >= SLOW_OPENING);
        // the gallery stays as it is until there is a canvas to zoom into
        if self.opening.is_some()
            && let Some(zoom) = self.zoom.as_mut().filter(|z| z.opening)
        {
            zoom.hold();
        }
        // a plugin finishing a fill: after a moment it says so
        let finishing = session.pour_finishing();
        self.finishing_since = finishing.then(|| self.finishing_since.unwrap_or_else(Instant::now));
        let finishing_slowly = self.finishing_since.is_some_and(|since| since.elapsed() >= SLOW_OPENING);
        // an export shows its own card; the veil is for a slow save
        let exporting = !self.ui.export_card.busy()
            && files::exporting_since().is_some_and(|since| since.elapsed() >= SLOW_OPENING);
        // a canvas opening while the pipelines are still being made waits for them: it says so
        let opening = opening_words(self.opening.is_some() && screen.canvases_waited().is_some());
        screen.size_interface(self.ui.plugins(), Some(session.as_ref()));
        // the strokes heard while a plugin makes their sound
        self.stroke_sound.follow(Some(&session), self.ui.plugins().makes_stroke_sound());
        let raw = screen.take_input();
        let mut leave_answer = None;
        let output = screen.egui.egui_ctx().clone().run_ui(raw, |ui| {
            self.ui.show(ui, &session, &mut self.input);
            // opening: the gallery over it, growing from the artwork's picture
            if let Some(zoom) = self.zoom.as_ref().filter(|z| z.opening) {
                zoom.over_canvas(ui.ctx(), &mut self.gallery, &mut self.ui.look);
            }
            if slow {
                veil(ui.ctx(), opening);
            } else if exporting {
                veil(ui.ctx(), "Exporting…");
            } else if finishing_slowly {
                veil(ui.ctx(), "Finishing the fill…");
            }
            // whether to leave the artwork without saving it, over everything
            leave_answer = leave_answer.or(self.leave_question.show(ui.ctx(), &mut self.ui.look));
        });
        let pad_soon = self.studio_pad(&session);
        let Some(screen) = self.screen.as_mut() else { return };
        self.again = screen.draw(Some(&session), self.started, output).map(|after| Instant::now() + after);
        if pad_soon {
            self.again = Some(Instant::now());
        }
        // looked at again soon while the canvas is being made, an artwork read or saved as, or a
        // fill finished
        if self.ui.saved_as() || self.opening.is_some() || self.importing.is_some() || finishing {
            let soon = Instant::now() + Duration::from_millis(50);
            self.again = Some(self.again.map_or(soon, |due| due.min(soon)));
        }
        self.follow_title();
        self.zoom_on();
        // Project canvas shows the canvas as just drawn
        if let (Some(p), Some(screen)) = (self.projection.as_mut(), &self.screen) {
            p.draw(&screen.gpu, Some(&session));
        }
        if std::mem::take(&mut self.ui.keep_brushes) {
            // off the window's thread: a library with many brushes brought in is tens of megabytes
            // when something in it changed (a brush chosen only writes the choices); waited for as
            // the app ends
            files::keep_brushes_meanwhile(&session);
        }
        for action in std::mem::take(&mut self.ui.actions) {
            self.act(action, &session);
        }
        if let Some((then, leave)) = leave_answer {
            self.leave_answered(then, leave, &session);
        }
    }

    /// The leave question answered, asked to do `then`: leaving the artwork without saving it
    /// (`leave`), its autosave stops and `then` goes on, the artwork left as it is; staying, the
    /// artwork stays open (an artwork read to come in goes, with any after it).
    fn leave_answered(&mut self, then: questions::Then, leave: bool, session: &Arc<CanvasSession>) {
        if !leave {
            if then == questions::Then::Import {
                self.importing = None;
            }
            return;
        }
        session.stop_autosave();
        self.left_unsaved = Some(then);
        match then {
            questions::Then::Gallery => self.act(Action::Gallery, session),
            questions::Then::New => self.act(Action::New, session),
            // at the loop's next turn, which has the loop to end; an artwork coming in comes in at
            // the next frame
            questions::Then::Close | questions::Then::Import => {
                self.close_asked = then == questions::Then::Close;
                if let Some(screen) = &self.screen {
                    screen.window.request_redraw();
                }
            }
        }
    }

    /// The window's title as the artwork open is now (see [`TitleWatch`]).
    fn follow_title(&mut self) {
        let (Some(screen), Some(session)) = (&self.screen, &self.session) else {
            self.title.check_at = None;
            return;
        };
        let edits = session.edits();
        let saved = edits == self.ui.saved_edits || session.edits_saved() == Some(edits);
        if let Some(title) = self.title.follow(&session.name(), saved, Instant::now()) {
            screen.window.set_title(&title);
        }
    }

    /// The window goes and the app ends, the files still being written finished first.
    fn close_window(&mut self, event_loop: &ActiveEventLoop) {
        files::finish_writing();
        files::tell_what_was_told();
        self.closed = true;
        event_loop.exit();
    }

    /// The gallery over the whole window, and what a tap in it asked for.
    fn gallery_frame(&mut self) {
        let Some(screen) = self.screen.as_mut() else { return };
        self.last_frame = Instant::now();
        self.input_frame = false;
        self.gap = frame_gap(&screen.window);
        screen.size_interface(self.ui.plugins(), None);
        self.stroke_sound.follow(None, false);
        let raw = screen.take_input();
        // what a plugin brought in from its folder, once it is in
        if let Some(said) = self.gallery.brought() {
            files::say(&said);
        }
        let mut asked = None;
        let mut opened_anyway = None;
        let (gallery, interface, open_anyway) = (&mut self.gallery, &mut self.ui, &mut self.open_anyway);
        let closing = self.zoom.as_ref().filter(|z| !z.opening);
        let reading = match (&self.importing, &self.reading_artwork, &self.reading_photo) {
            (None, None, None) => None,
            (importing, opening, photo) => Some(
                importing.as_ref().is_some_and(|i| i.since.elapsed() >= SLOW_OPENING)
                    || opening.as_ref().is_some_and(|(since, ..)| since.elapsed() >= SLOW_OPENING)
                    || photo.as_ref().is_some_and(|(since, ..)| since.elapsed() >= SLOW_OPENING),
            ),
        };
        // the canvases' pipelines still being made after a moment: said at the gallery's foot
        let waited = screen.canvases_waited();
        // (on OpenGL from the first frame, which stays while they are made)
        let getting_ready = waited.is_some_and(|w| w >= CANVASES_SLOW) || screen.canvases_later();
        let output = screen.egui.egui_ctx().clone().run_ui(raw, |ui| {
            interface.style_once(ui.ctx());
            // closing: the gallery settling back into place, the canvas fading over it
            // otherwise in its place (a closing zoom cut short leaves nothing behind)
            match closing {
                Some(zoom) => zoom.over_gallery(ui.ctx()),
                None => zoom::Zoom::end(ui.ctx()),
            }
            // the New canvas list offers the kinds of artwork the plugins on make
            gallery.kinds = interface.plugins().kinds();
            asked = gallery.show(ui.ctx(), &mut interface.look, &interface.colours);
            // layers held out of an artwork: held a moment over another, it opens; let go here,
            // they go nowhere
            if let Some(held) = interface.held.as_mut() {
                let at = ui.ctx().input(|i| i.pointer.latest_pos());
                let under = at.and_then(|at| gallery.artwork_at(at)).map(held_layers::Spring::Artwork);
                match held.frame(ui.ctx(), under, false) {
                    held_layers::Outcome::Open(held_layers::Spring::Artwork(dir)) => {
                        asked = Some(gallery::Asked::Open(dir));
                    }
                    held_layers::Outcome::Holding | held_layers::Outcome::Open(_) => {}
                    held_layers::Outcome::Dropped(_) | held_layers::Outcome::Gone => interface.held = None,
                }
            }
            // a pen's buttons do nothing here, and nothing in the artwork opened next either
            crate::pen::take_button_presses();
            // a share from the gallery shows its card there
            interface.export_card.show(ui.ctx(), &mut interface.look);
            interface.plugin_alert(ui.ctx());
            // an artwork whose opening ended the app last time opened only when asked to
            let log = open_anyway.log().map(std::path::Path::to_path_buf).unwrap_or_else(startup::last_run_log);
            let share_log = || files::reveal(&log);
            opened_anyway = opened_anyway.take().or(open_anyway.show(ui.ctx(), &mut interface.look, share_log));
            if reading == Some(true) {
                opening_veil(ui.ctx());
            } else if getting_ready {
                getting_ready_note(ui.ctx());
            }
        });
        self.again = screen.draw(None, self.started, output).map(|after| Instant::now() + after);
        // looked at again soon while an artwork is read, and once the pipelines have taken long
        // enough to say so (once made, the window is asked to draw again)
        let soon = match waited {
            _ if reading.is_some() => Some(Instant::now() + Duration::from_millis(50)),
            _ if self.gallery.bringing_in() => Some(Instant::now() + Duration::from_millis(200)),
            Some(waited) if !getting_ready => Some(Instant::now() + CANVASES_SLOW.saturating_sub(waited)),
            _ => None,
        };
        if let Some(soon) = soon {
            self.again = Some(self.again.map_or(soon, |due| due.min(soon)));
        }
        if let Some(title) = self.title.gallery() {
            screen.window.set_title(title);
        }
        self.zoom_on();
        // Project canvas is black while the gallery shows
        if let (Some(p), Some(screen)) = (self.projection.as_mut(), &self.screen) {
            p.draw(&screen.gpu, None);
        }
        // while an artwork, a photo or an import is being read, the first tap stands: another
        // artwork, a new one or a photo isn't opened over it
        let busy = self.reading_artwork.is_some() || self.reading_photo.is_some() || self.importing.is_some();
        if let Some(Some((dir, shared_as))) = opened_anyway
            && !busy
        {
            self.read_artwork_anyway(dir, shared_as);
        }
        match asked {
            // read on a thread of its own; one at a time
            Some(gallery::Asked::Open(dir)) => {
                if !busy {
                    self.read_artwork(dir, None);
                }
            }
            Some(gallery::Asked::New(preset, folder)) => {
                if !busy {
                    let session = files::new_from_preset(&preset);
                    let dir = self.gallery.store.new_folder(folder);
                    // its time-lapse made as the preset says
                    files::keep_preset_time_lapse(&preset, &dir);
                    self.open_artwork(session, dir);
                    self.ui.started_new();
                }
            }
            Some(gallery::Asked::ScreenSize(folder)) => {
                if !busy {
                    let (w, h) = self.gallery.screen_px;
                    let session = CanvasSession::new(w, h);
                    session.set_new_profile(procraft_core::P3.into());
                    let dir = self.gallery.store.new_folder(folder);
                    self.open_artwork(session, dir);
                    self.ui.started_new();
                }
            }
            Some(gallery::Asked::PixelArt(w, h, folder)) => {
                if !busy {
                    let session = CanvasSession::new(w, h);
                    session.set_new_profile(procraft_core::P3.into());
                    session.make_pixel_art();
                    let dir = self.gallery.store.new_folder(folder);
                    self.open_artwork(session, dir);
                    self.ui.started_new();
                }
            }
            // a plugin's kind of artwork, the screen's size, its plugin keeping it
            Some(gallery::Asked::PluginKind(kind, folder)) => {
                let (w, h) = self.gallery.screen_px;
                let canvas = self.ui.plugins_shared().open_canvas(kind.clone(), w, h, None);
                match canvas {
                    Some(canvas) if !busy => {
                        let session = CanvasSession::new(w, h);
                        session.set_new_profile(procraft_core::P3.into());
                        if session.attach_plugin_canvas(kind, canvas) {
                            let dir = self.gallery.store.new_folder(folder);
                            self.open_artwork(session, dir);
                            self.ui.started_new();
                        }
                    }
                    Some(_) => {}
                    None => files::say("The plugin couldn't make this artwork."),
                }
            }
            Some(gallery::Asked::Share(list, format)) => {
                files::share_artworks(&list, format, 100, &mut self.ui.export_card);
            }
            // several as one PDF, a page each
            Some(gallery::Asked::SharePdf(list, quality, group)) => {
                files::share_pdf(&list, quality, &group, &mut self.ui.export_card);
            }
            Some(gallery::Asked::DragOut(artwork)) => self.drag_out(&artwork),
            // opened (read as Open reads it), then shared
            Some(gallery::Asked::ShareAnimated(artwork, format)) => {
                if !busy {
                    self.read_artwork(artwork.dir, Some(format));
                }
            }
            // read on a thread of its own (a large photo takes a while); one at a time
            Some(gallery::Asked::Photo(folder)) => {
                if !busy && let Some(path) = files::pick_picture() {
                    let reading = std::thread::spawn(move || files::picture_artwork(&path));
                    self.reading_photo = Some((Instant::now(), folder, reading));
                }
            }
            Some(gallery::Asked::Import(folder)) => {
                if let Some(paths) = files::pick_import() {
                    self.import(paths, folder);
                }
            }
            None => {}
        }
    }

    /// Shows `session`, kept in the gallery's folder `dir`.
    /// Project canvas: its window opened on a second display while it is on and there is one
    /// (the displays looked at once a second), closed otherwise.
    fn sync_projection(&mut self, event_loop: &ActiveEventLoop) {
        let Some(screen) = &self.screen else { return };
        if !self.ui.project_canvas() {
            if self.projection.take().is_some() {
                log::info!("Project canvas: turned off");
            }
            return;
        }
        if self.displays_seen.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
            return;
        }
        self.displays_seen = Some(Instant::now());
        // the main window's display unknown (as some systems have it while the window is
        // minimised): nothing changes, as a projection opened now could cover the main window's own
        // display, with nothing left on screen to turn it off with
        let Some(main) = screen.window.current_monitor() else { return };
        let second = projection::other_display(event_loop.available_monitors(), &main);
        // gone, or the main window moved onto its display: opened again where it now goes
        if self.projection.as_ref().is_some_and(|p| Some(&p.monitor) != second.as_ref()) {
            log::info!("Project canvas: its display is gone, or the main window is on it: closed");
            self.projection = None;
        }
        if self.projection.is_none()
            && let Some(monitor) = second
        {
            self.projection = projection::Projection::open(event_loop, monitor, &screen.gpu);
            if let Some(p) = &self.projection {
                p.window.request_redraw();
            }
        }
    }

    /// An event of Project canvas's window.
    fn projection_event(&mut self, event: WindowEvent) {
        let (Some(screen), Some(projection)) = (&self.screen, self.projection.as_mut()) else { return };
        match event {
            WindowEvent::Resized(size) => {
                projection.resize(&screen.gpu, size.width, size.height);
                projection.window.request_redraw();
            }
            WindowEvent::RedrawRequested => projection.draw(&screen.gpu, self.session.as_deref()),
            // shown again: drawn, for a frame missed while it was hidden
            WindowEvent::Occluded(false) => projection.window.request_redraw(),
            // closed there, or Escape pressed on it: Project canvas goes off
            WindowEvent::CloseRequested => {
                log::info!("Project canvas: its window closed");
                self.projection = None;
                self.ui.stop_projecting();
            }
            WindowEvent::KeyboardInput { event: key, .. } if projection::closes(&key.logical_key, key.state) => {
                log::info!("Project canvas: Escape pressed on its window");
                self.projection = None;
                self.ui.stop_projecting();
            }
            _ => {}
        }
    }

    /// While the gallery zooms, another frame soon; once done, everything back in its place.
    fn zoom_on(&mut self) {
        let Some(zoom) = &self.zoom else { return };
        if zoom.done() {
            self.zoom = None;
            if let Some(screen) = &self.screen {
                zoom::Zoom::end(screen.egui.egui_ctx());
                screen.window.request_redraw();
            }
        } else {
            self.again = Some(Instant::now());
        }
    }

    fn open_artwork(&mut self, session: Arc<CanvasSession>, dir: String) {
        // a plugin's own artwork goes to its plugin (on); else it shows what it last drew
        if session.plugin_kind().is_some() && !session.open_plugin_artwork(self.ui.plugins_shared()) {
            log::warn!("an artwork of a plugin's that isn't on opened as what it last drew");
        }
        self.ui.artwork_dir = Some(dir.clone());
        // a new artwork is in the gallery from the start (an opened one is there already)
        files::keep_new_artwork(&session, &dir);
        // kept saved as it changes, as the tablet's
        Arc::clone(&session).start_autosave(dir.clone());
        self.artwork = Some(dir);
        self.ui.path = None;
        self.ui.saved_edits = session.edits();
        self.show(session);
    }

    /// Tap Assistance: for its first moments a lone finger's touch stays where it landed (its
    /// moves held back, its lift put back there), so a tap that drifts still lands where it began.
    /// More fingers, a pen, a touch held longer, or one gone farther than a drift (a list scrolled)
    /// go on as they are. None: the event is held back.
    fn tap_assist(&mut self, event: WindowEvent) -> Option<WindowEvent> {
        let WindowEvent::Touch(touch) = &event else { return Some(event) };
        // (a finger may give a pressure too: the system says which it is)
        let pen = crate::pen::is_pen(touch.id).unwrap_or(touch.force.is_some());
        if !self.ui.tap_assistance() || pen {
            return Some(event);
        }
        let held = self.tap_held.filter(|(id, _, _)| *id == touch.id);
        let drift = TAP_DRIFT * self.screen.as_ref().map_or(1.0, |s| s.window.scale_factor());
        match touch.phase {
            // a second finger lets the first go on as it is
            winit::event::TouchPhase::Started if self.tap_held.is_some() => self.tap_held = None,
            winit::event::TouchPhase::Started => self.tap_held = Some((touch.id, touch.location, Instant::now())),
            winit::event::TouchPhase::Moved => match held {
                Some((_, at, since))
                    if since.elapsed() <= TAP_HOLD
                        && (touch.location.x - at.x).hypot(touch.location.y - at.y) <= drift =>
                {
                    return None;
                }
                Some(_) => self.tap_held = None,
                None => {}
            },
            winit::event::TouchPhase::Ended => {
                if let Some((_, at, _)) = held {
                    self.tap_held = None;
                    return Some(WindowEvent::Touch(winit::event::Touch { location: at, ..*touch }));
                }
            }
            winit::event::TouchPhase::Cancelled => self.tap_held = None,
        }
        Some(event)
    }

    /// Files dropped on the window: on the canvas its pictures are inserted; on the gallery they
    /// come in as Import brings them (an artwork, a picture, or a model with its files).
    fn take_dropped(&mut self) {
        let paths = std::mem::take(&mut self.dropped);
        // a plugin file is asked about wherever it lands
        let (plugins, paths): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| files::is_plugin_file(p));
        if let Some(path) = plugins.first() {
            self.ui.offer_plugin_file(path);
        }
        if paths.is_empty() {
            return;
        }
        if let Some(session) = self.session.clone() {
            self.ui.dropped(&session, &paths);
            return;
        }
        self.import(paths, self.gallery.folder());
    }

    /// Files brought into the gallery (picked with Import, or dropped on it) into `folder`:
    /// palettes join the colour panel's, brushes wait for a canvas to open, and the rest come in as
    /// an artwork (a package, a PSD, a PDF, a picture, or a model with its files).
    fn import(&mut self, paths: Vec<std::path::PathBuf>, folder: Option<String>) {
        let (palettes, paths): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| files::is_palette_file(p));
        if !palettes.is_empty() {
            let store = &self.ui.colours.palettes;
            let added = palettes.iter().filter(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
                std::fs::read(p).is_ok_and(|bytes| store.import_file(bytes, name))
            });
            files::say(if added.count() > 0 { "Palette added." } else { "This palette can't be read." });
        }
        let (brushes, paths): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| files::is_brush_file(p));
        if !brushes.is_empty() {
            files::keep_pending_brushes(&brushes);
        }
        // a model takes the files picked with it; else each is an artwork of its own, the one
        // before kept (closed) as the next comes in
        let artworks: std::collections::VecDeque<Vec<std::path::PathBuf>> =
            if paths.iter().any(|p| files::is_model_file(p)) {
                [paths].into()
            } else {
                paths.into_iter().map(|p| vec![p]).collect()
            };
        // after any already coming in
        match &mut self.importing {
            Some(importing) => importing.rest.extend(artworks),
            None => self.importing = Importing::next(artworks, folder),
        }
    }

    /// Starts reading the artwork in gallery folder `dir` on a thread of its own, to open it (to
    /// share it as `shared_as`, if given).
    fn read_artwork(&mut self, dir: String, shared_as: Option<animated_export::Animated>) {
        // one whose opening ended the app last time is opened again only when asked to (the
        // gallery stays as it is meanwhile, the artwork in it), so that it can't take the app down
        // each time it is picked
        if procraft_core::opening_ended_app(dir.clone()) {
            log::warn!("the app ended while opening {dir} last time: asking before opening it again");
            // what is said follows how that run ended, as its log tells (a crash, or not known);
            // Share Log shows that run's log
            let run = procraft_core::opening_ended_run(dir.clone());
            let (said, log) = (startup::opening_ended_said(run), startup::log_of_run(run));
            self.open_anyway.ask((dir, shared_as), said, log);
            return;
        }
        self.read_artwork_anyway(dir, shared_as);
    }

    /// Starts reading the artwork in gallery folder `dir` as [`Self::read_artwork`] does, however
    /// its opening went last time.
    fn read_artwork_anyway(&mut self, dir: String, shared_as: Option<animated_export::Animated>) {
        let folder = dir.clone();
        let reading = std::thread::spawn(move || CanvasSession::open(folder).map_err(|e| e.to_string()));
        self.reading_artwork = Some((Instant::now(), dir, shared_as, reading));
    }

    /// The artwork opened from the gallery, once read: shown, the gallery zooming into its
    /// picture (unless another was opened meanwhile); or, opened to be shared, shared.
    fn artwork_read(&mut self) {
        let Some((_, dir, shared_as, reading)) = self.reading_artwork.take_if(|(.., r)| r.is_finished()) else {
            return;
        };
        let read = reading.join().unwrap_or_else(|_| Err("it stopped".to_owned()));
        if self.session.is_some() {
            return;
        }
        match read {
            Ok(session) if shared_as.is_some() => {
                self.open_artwork(session, dir);
                self.share_once_made = shared_as;
            }
            Ok(session) => {
                let cell = self.gallery.picture_of(&dir);
                let turns = session.shown_turns();
                self.open_artwork(Arc::clone(&session), dir);
                // the gallery zooms into its picture
                if let (Some(cell), Some(screen)) = (cell, &self.screen) {
                    let size = screen.window.inner_size();
                    let scale = screen.window.scale_factor() as f32 * screen.egui.egui_ctx().zoom_factor();
                    self.zoom = Some(zoom::Zoom::opening(&session, cell, turns, (size.width, size.height), scale));
                }
            }
            Err(e) => files::tell("Can't open the artwork", &e),
        }
    }

    /// The gallery's Photo, once read: a new artwork of the picture, opened (unless another was
    /// opened meanwhile).
    fn photo_read(&mut self) {
        let Some((_, folder, reading)) = self.reading_photo.take_if(|(.., r)| r.is_finished()) else { return };
        let read = reading.join().unwrap_or_else(|_| Err("it stopped".to_owned()));
        if self.session.is_some() {
            return;
        }
        match read {
            Ok(session) => {
                let dir = self.gallery.store.new_folder(folder);
                self.open_artwork(session, dir);
                self.ui.started_new();
            }
            Err(e) => files::tell("Can't open the picture", &e),
        }
    }

    /// An artwork read meanwhile, opened (the one open before kept), and the next one read.
    fn imported(&mut self) {
        // (waits while the artwork open is asked whether to be left without saving)
        if self.leave_question.is_open() {
            return;
        }
        let Some(importing) = self.importing.take_if(|i| i.stage.is_finished()) else { return };
        let Importing { since, stage, folder, rest } = importing;
        // read (now, or before the artwork open was asked whether to be left unsaved), or with its
        // time-lapse taken in
        let read = match stage {
            ImportStage::Reading(reading) => Ok(reading.join().ok().flatten()),
            ImportStage::Read(session, package) => Ok(Some((session, package))),
            ImportStage::TakingTimeLapse(taking, session, dir) => Err((taking, session, dir)),
        };
        let (session, dir) = match read {
            Ok(read) => {
                let Some((session, package)) = read else {
                    self.importing = Importing::next(rest, folder);
                    return;
                };
                if self.session.is_some() && !self.leave_artwork(Some(questions::Then::Import)) {
                    // asked whether to leave the artwork open unsaved: this one waits for the answer
                    if self.leave_question.is_open() {
                        let stage = ImportStage::Read(session, package);
                        self.importing = Some(Importing { since, stage, folder, rest });
                    }
                    return;
                }
                let dir = self.gallery.store.new_folder(folder.clone());
                // a package's time-lapse taken in on its own thread first, the window veiled
                if let Some(package) = package {
                    let into = dir.clone();
                    let taking = std::thread::spawn(move || timelapse::take_package(&package, &into));
                    let stage = ImportStage::TakingTimeLapse(taking, session, dir);
                    self.importing = Some(Importing { since, stage, folder, rest });
                    return;
                }
                (session, dir)
            }
            Err((taking, session, dir)) => {
                let _ = taking.join();
                (session, dir)
            }
        };
        // a model is a new artwork painting on it
        let model = session.model_info().is_some();
        self.open_artwork(session, dir);
        if model {
            self.ui.started_new();
        }
        self.importing = Importing::next(rest, folder);
    }

    /// An artwork dragged out of the window: written as Drag and Drop export's file format says,
    /// then dragged on as that file, for wherever it is let go (the system's drag carries it).
    #[cfg(any(windows, target_os = "macos"))]
    fn drag_out(&mut self, artwork: &procraft_core::GalleryArtwork) {
        let format = self.ui.drag_format();
        let Some(screen) = self.screen.as_mut() else { return };
        let path = match files::drag_file(artwork, format) {
            Ok(path) => path,
            Err(e) => {
                files::tell("Can't drag it out", &e);
                return;
            }
        };
        let image = drag::Image::Raw(files::drag_picture(&artwork.dir, 256).unwrap_or_default());
        let item = drag::DragItem::Files(vec![path]);
        if let Err(e) = drag::start_drag(&*screen.window, item, image, |_, _| {}, drag::Options::default()) {
            log::warn!("dragging an artwork out: {e}");
        }
        // the button is let go inside the system's drag (on Windows once it is over; a Mac's
        // goes on by itself), which egui never hears
        let pos = screen.egui.egui_ctx().input(|i| i.pointer.latest_pos()).unwrap_or(egui::Pos2::ZERO);
        screen.egui.egui_input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        screen.window.request_redraw();
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn drag_out(&mut self, _artwork: &procraft_core::GalleryArtwork) {}

    /// The interface's frame couldn't go up to the GPU (short of memory): what is kept on it only to
    /// be quicker is let go of (the brush library's sample strokes, the canvas's spares and the
    /// buffers kept, the preview's canvas), the GPU given the time to free it, and the window drawn
    /// again.
    fn let_go_of_kept(&mut self) {
        log::warn!("letting go of what is kept on the GPU, to draw the interface again");
        if let Some(session) = &self.session {
            session.trim_memory();
        }
        self.ui.let_go_of_kept();
        if let Some(screen) = &self.screen {
            screen.gpu.let_go_of_kept_buffers();
            screen.gpu.flush();
            screen.window.request_redraw();
        }
    }

    /// The graphics device was lost (its driver reset or stopped), and all it held: nothing is
    /// saved from it (the gallery keeps the artwork as it was last saved, seconds before); the
    /// windows go, and the app says so and starts again.
    fn gpu_lost(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(session) = self.session.take() {
            session.stop_autosave();
        }
        // nothing they show can be drawn (and with the screen gone this happens once)
        self.projection = None;
        self.screen = None;
        // as a backend starts, or a canvas first draws on it, that counts against the backend
        if let Some(tried) = startup::device_lost() {
            log::warn!("the graphics device stopped as this backend started: starting again on the next");
            if !startup::start_on_next(tried) {
                startup::stopped("the graphics device stopped, and it couldn't start itself again");
            }
        } else {
            files::tell("The graphics device stopped", "ProCraft starts again, with the artwork as it was last saved.");
            startup::start_again(None);
        }
        event_loop.exit();
    }

    /// The canvases' pipelines are made, `fine` without a GPU error. A backend that fails on them
    /// gives way to the next: the app starts again on it while nothing is open or being read
    /// (else the next start takes it), unless it is the last there is (then kept as it is).
    fn canvases_made(&mut self, fine: bool, event_loop: &ActiveEventLoop) {
        let Some((tried, last)) = self.screen.as_ref().map(|s| (s.tried, s.last)) else { return };
        startup::gpu_prepared(fine || last);
        if fine || last {
            return;
        }
        let busy = self.session.is_some()
            || self.reading_artwork.is_some()
            || self.reading_photo.is_some()
            || self.importing.is_some()
            || files::still_writing();
        if busy || !startup::kept() {
            log::warn!("the canvases' pipelines failed on this graphics backend: the next start takes the next");
            return;
        }
        log::warn!("the canvases' pipelines failed on this graphics backend: starting again on the next");
        self.screen = None;
        self.closed = true;
        if !startup::start_again(Some(tried)) {
            startup::stopped("its pipelines failed on this graphics backend, and it couldn't start itself again");
        }
        event_loop.exit();
    }

    /// The stack the open artwork is in (its folder in the gallery's), none at the top.
    fn artwork_folder(&self) -> Option<String> {
        let folder = self.artwork.as_deref().and_then(|d| std::path::Path::new(d).parent());
        folder.map(|f| f.to_string_lossy().into_owned()).filter(|f| *f != self.gallery.store.root())
    }

    /// The window goes from the screen while the artwork open is still being made (its canvas
    /// waits for the canvases' pipelines: a minute or more on a slow GPU's shader compiler), as
    /// keeping it waits for that: no window stands still meanwhile. True if it went.
    fn hide_while_opening(&self) -> bool {
        let waits = self.opening.as_ref().is_some_and(|(_, made)| !made.is_finished());
        if waits && let Some(screen) = &self.screen {
            log::info!(
                "closing while a canvas is still being made: the window goes, the artwork is kept once it is made"
            );
            screen.window.set_visible(false);
        }
        waits
    }

    /// The artwork open is put away and kept, to do `then`; false while it stays open. One that
    /// can't be saved (the storage full, say) stays, kept saved again, and the person is asked
    /// whether to leave it without saving (the core's rule, as the tablet's): chosen, `then` goes on
    /// ([`Self::leave_answered`]). With no `then` (the app ending) nothing is asked. Nothing is
    /// left while that is asked.
    fn leave_artwork(&mut self, then: Option<questions::Then>) -> bool {
        if self.leave_question.is_open() {
            return false;
        }
        if let Some(session) = self.session.clone() {
            // its time counted up to now, saved with it
            session.set_tracking(false);
            self.ui.closing(&session);
        }
        let left = self.keep_artwork_to_leave(then);
        // still open: its time counts again
        if !left && let Some(session) = &self.session {
            session.set_tracking(true);
        }
        left
    }

    /// Saves the artwork open to leave it (and the brushes as it left them): true once it is kept,
    /// or once the person chose to leave it without saving when it couldn't be; false to stay
    /// (asked whether to leave it unsaved to do `then`, if given).
    fn keep_artwork_to_leave(&mut self, then: Option<questions::Then>) -> bool {
        let (Some(session), Some(dir)) = (self.session.clone(), self.artwork.clone()) else { return true };
        // left without saving, as the person chose, for what they chose it for
        if then.is_some() && self.left_unsaved.take() == then {
            return true;
        }
        // its canvas made first (it is being made on a thread of its own), so the save has its picture
        if let Some((_, made)) = self.opening.take() {
            let _ = made.join();
        }
        files::keep_brushes(&session);
        let edits = session.edits();
        match Arc::clone(&session).save_to_leave(dir) {
            procraft_core::Leaving::Left => {
                self.ui.saved_edits = edits;
                true
            }
            procraft_core::Leaving::Stays { title, message } => {
                match then {
                    Some(then) => self.leave_question.ask(&title, &message, then),
                    None => log::warn!("{title}: {message}"),
                }
                false
            }
        }
    }

    /// Saves the artwork open into its folder in the gallery (and the brushes as it left them);
    /// false if it couldn't be.
    fn keep_artwork(&mut self) -> bool {
        let (Some(session), Some(dir)) = (self.session.clone(), self.artwork.clone()) else { return true };
        // its canvas made first (it is being made on a thread of its own), so the save has its picture
        if let Some((_, made)) = self.opening.take() {
            let _ = made.join();
        }
        files::keep_brushes(&session);
        let edits = session.edits();
        match session.save(dir) {
            Ok(()) => {
                self.ui.saved_edits = edits;
                true
            }
            Err(e) => {
                // (the storage full said plainly: it stays open until there is room)
                let why = e.to_string();
                let said =
                    if procraft_core::storage_was_full(why.clone()) { procraft_core::NO_ROOM.to_owned() } else { why };
                files::tell("Can't keep the artwork", &said);
                false
            }
        }
    }

    fn act(&mut self, action: Action, session: &Arc<CanvasSession>) {
        let opened = match action {
            Action::Gallery => {
                if self.leave_artwork(Some(questions::Then::Gallery)) {
                    // the gallery zooms out of the artwork's picture, as it will show (turned as its
                    // canvas was left)
                    let (info, turns) = (session.info(), session.shown_turns());
                    if let (Some(dir), Some(screen)) = (&self.artwork, &self.screen)
                        && let Some(cell) = self.gallery.picture_as(dir, (info.width, info.height), turns)
                    {
                        self.zoom = Some(zoom::Zoom::closing(screen.egui.egui_ctx(), session, cell, turns));
                    }
                    self.session = None;
                    self.artwork = None;
                    self.gallery.refresh();
                    self.gallery.bring_in(&self.ui.plugins_shared());
                    if let Some(screen) = &self.screen {
                        screen.window.request_redraw();
                    }
                }
                None
            }
            // a new artwork the size of this one beside it in the gallery
            Action::New => {
                if !self.leave_artwork(Some(questions::Then::New)) {
                    return;
                }
                let info = session.info();
                let dir = self.gallery.store.new_folder(self.artwork_folder());
                self.open_artwork(CanvasSession::new(info.width, info.height), dir);
                self.ui.started_new();
                None
            }
            // one picked, beside it: read on a thread of its own as the gallery's Import does (a
            // big file takes seconds), this one open meanwhile and kept as that one opens
            Action::Open => {
                if let Some(paths) = files::pick_artwork_files(&[]) {
                    let folder = self.artwork_folder();
                    match &mut self.importing {
                        Some(importing) => importing.rest.push_back(paths),
                        None => self.importing = Importing::next([paths].into(), folder),
                    }
                }
                None
            }
            Action::Save => {
                self.keep_artwork();
                None
            }
            Action::SaveAs => {
                files::save(session, &mut self.ui, true);
                None
            }
            Action::Export(format, pdf_quality) => {
                files::export(session, format, self.ui.artwork_dir.as_deref(), pdf_quality, &mut self.ui.export_card);
                None
            }
            Action::PluginExport(i) => {
                if let Some(export) = session.plugin_exports().into_iter().nth(usize::from(i)) {
                    files::export_plugin(session, &export.key, &mut self.ui.export_card);
                }
                None
            }
        };
        if let Some(session) = opened {
            self.show(session);
        }
    }
}

/// The least time between two frames: a tenth under the time between the screen's refreshes (a
/// 60 Hz screen's when it can't be told), so a frame is ready for each as they drift, as the
/// tablet's are.
fn frame_gap(window: &Window) -> Duration {
    let millihertz = window.current_monitor().and_then(|m| m.refresh_rate_millihertz()).filter(|&m| m > 0);
    Duration::from_secs_f64(0.9 * 1000.0 / f64::from(millihertz.unwrap_or(60_000)))
}

/// Asks for a frame as input wants one: at once when the last was `gap` ago or more, else once it
/// was (`later`, seen to in `about_to_wait`). A mouse or pen reports hundreds of moves a second:
/// drawn as each came, where the window shows the newest frame without waiting for the screen,
/// frames were drawn that never showed.
fn redraw_paced(window: &Window, last_frame: Instant, gap: Duration, later: &mut bool) {
    if Instant::now() >= last_frame + gap {
        window.request_redraw();
    } else {
        *later = true;
    }
}

#[cfg(test)]
mod tests {
    /// The title marks an artwork with changes no save has yet, and is looked at again a second
    /// later until one has them; then not again. Looking at it again only sets the title if it
    /// changed, and asks for nothing to be drawn (no frame comes of it: on OpenGL a frame's swap
    /// after the window had been still held the window's thread 90 to 450 ms each second).
    #[test]
    fn the_title_is_looked_at_again_each_second_until_saved_and_sets_only_changes() {
        let mut title = super::TitleWatch::default();
        let now = std::time::Instant::now();
        assert_eq!(title.follow("Untitled", false, now).as_deref(), Some("Untitled \u{2022} \u{2014} ProCraft"));
        assert!(!title.due(now));
        assert!(title.due(now + super::TITLE_CHECK));
        // still unsaved a second later: nothing to set, looked at again
        let later = now + super::TITLE_CHECK;
        assert_eq!(title.follow("Untitled", false, later), None);
        assert_eq!(title.check_at, Some(later + super::TITLE_CHECK));
        // saved: the mark goes, and it isn't looked at again
        assert_eq!(title.follow("Untitled", true, later).as_deref(), Some("Untitled \u{2014} ProCraft"));
        assert_eq!(title.check_at, None);
        assert!(!title.due(later + super::TITLE_CHECK * 10));
        // the gallery: its title once, nothing to look at again
        assert_eq!(title.gallery(), Some(crate::APP_NAME));
        assert_eq!(title.gallery(), None);
    }

    #[test]
    fn the_computers_memory_is_read() {
        let total = super::memory_total().expect("the system says how much memory there is");
        assert!(total >= 1 << 30, "{total} bytes");
    }

    /// The .exe asks the drivers for the discrete GPU for OpenGL: the two values are exported, set
    /// (the test's .exe is linked as the app's is).
    #[cfg(windows)]
    #[test]
    #[allow(unsafe_code)]
    fn the_exe_asks_the_drivers_for_the_discrete_gpu() {
        use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
        // SAFETY: the module of this process, which stays loaded
        let me = unsafe { GetModuleHandleW(None) }.unwrap();
        for name in
            [windows::core::s!("NvOptimusEnablement"), windows::core::s!("AmdPowerXpressRequestHighPerformance")]
        {
            // SAFETY: a name ending in a nul, looked up in this module's exports
            let at = unsafe { GetProcAddress(me, name) };
            let at = at.map(|f| f as usize).unwrap_or_else(|| panic!("{name:?} isn't exported"));
            // SAFETY: the export is one of the u32 statics above, there for the process's life
            assert_eq!(unsafe { std::ptr::read(at as *const u32) }, 1);
        }
    }

    #[test]
    fn a_gpu_sharing_the_computers_memory_gives_the_layers_an_eighth_of_it() {
        use super::layers_on_gpu;
        const GB: u64 = 1 << 30;
        // a graphics card: 2 GiB, whatever the computer has
        assert_eq!(layers_on_gpu(false, Some(8 * GB)), 2 * GB);
        assert_eq!(layers_on_gpu(false, None), 2 * GB);
        // an integrated GPU: an eighth of the computer's memory, 512 MiB to 2 GiB (a laptop of 8 GB,
        // as #341's, 1 GiB: a device of little memory to the canvas, at most 1280 MiB)
        assert_eq!(layers_on_gpu(true, Some(8 * GB)), GB);
        assert!(layers_on_gpu(true, Some(8 * GB)) <= 1280 << 20);
        assert_eq!(layers_on_gpu(true, Some(4 * GB)), 512 << 20);
        assert_eq!(layers_on_gpu(true, Some(2 * GB)), 512 << 20);
        assert_eq!(layers_on_gpu(true, Some(16 * GB)), 2 * GB);
        assert_eq!(layers_on_gpu(true, Some(64 * GB)), 2 * GB);
        assert_eq!(layers_on_gpu(true, None), GB);
    }
}
