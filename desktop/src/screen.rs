//! The window's surface and the device the canvas and the interface share: each frame the
//! canvas is drawn into the window's image, and the interface over it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use procraft_core::{CanvasSession, Plugins, configure, screen_profile};
use procraft_gpu::{Gpu, wgpu};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

pub struct Screen {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: Option<wgpu::SurfaceConfiguration>,
    pub gpu: Gpu,
    pub egui: egui_winit::State,
    /// Made for the surface's format.
    renderer: egui_wgpu::Renderer,
    format: wgpu::TextureFormat,
    /// The brush studio's drawing pad: its own picture, shown by the interface.
    pad: Option<Pad>,
    /// The canvases' pipelines being made on a thread of their own, the window showing meanwhile
    /// (seconds; many more with Direct3D 12's shader compiler): whether they were made without a
    /// GPU error.
    canvases: Option<std::thread::JoinHandle<bool>>,
    /// Since when they have been being made.
    canvases_since: Instant,
    /// On OpenGL they are made only after the window's first frame, and nothing is drawn while
    /// they are (see [`Screen::gl_busy`]): what to wake once they are, till then.
    canvases_later: Option<winit::event_loop::EventLoopProxy<()>>,
    /// How many backends have been tried, its own the last of them (with those the app tried
    /// before it started again), and whether that is all there are.
    pub tried: usize,
    pub last: bool,
    /// The system's on-screen keyboard, for a field focused by a finger or a pen.
    pub touch_keyboard: crate::touch_keyboard::TouchKeyboard,
    /// Whether the interface was told it is on a touch screen, for a tablet's pen that moves the
    /// mouse pointer.
    pen_touch_screen: crate::input::PenTouchScreen,
    /// The interface's frames missed in a row, and what the app is to do after the last one (see
    /// `interface_pass`).
    misses: crate::interface_pass::Misses,
    after_frame: crate::interface_pass::AfterFrame,
    /// The frames the window's surface gave no picture for, in a row (see `window_watch`).
    surface_misses: crate::window_watch::SurfaceMisses,
    /// The surface found at another size than the window's, told in the log at most so often.
    size_told: crate::window_watch::LogBudget,
}

/// A canvas drawn into a picture of its own (the surface's format), its size, and the
/// interface's name for it.
struct Pad {
    view: wgpu::TextureView,
    size: (u32, u32),
    id: egui::TextureId,
}

/// The stack the canvases' pipelines are made on (reserved; only what is used is taken).
const PIPELINES_STACK: usize = 64 << 20;

/// Makes the canvases' pipelines on `gpu` (and the brush images they hold) on a thread of their
/// own, waking `wake` once they are; a canvas made meanwhile waits for them. The thread says
/// whether they were made without a GPU error (a driver that panics on them fails too).
fn make_canvases(gpu: &Gpu, wake: winit::event_loop::EventLoopProxy<()>) -> Option<std::thread::JoinHandle<bool>> {
    let gpu = gpu.clone();
    // on a big stack: a driver's shader compiler recurses, and a stack overflow ends the app with
    // next to no word
    let thread = std::thread::Builder::new().name("procraft-pipelines".to_owned()).stack_size(PIPELINES_STACK);
    let made = thread.spawn(move || {
        log::info!("making the canvases' pipelines");
        let (started, errors) = (Instant::now(), gpu.errors());
        let fine = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gpu.prepare_canvases())) {
            Ok(()) => gpu.errors() == errors && !gpu.is_lost(),
            Err(panic) => {
                log::error!("the canvases' pipelines panicked: {}", procraft_gpu::panic_message(panic.as_ref()));
                false
            }
        };
        log::info!(
            "the canvases' pipelines made in {} ms: {} GPU errors, lost {}",
            started.elapsed().as_millis(),
            gpu.errors().saturating_sub(errors),
            gpu.is_lost()
        );
        let _ = wake.send_event(());
        fine
    });
    // without a thread, the first canvas makes them
    made.inspect_err(|e| log::warn!("the canvases' pipelines' thread: {e}")).ok()
}

/// Whether the window's surface may be wide colour: no, as egui draws the interface in sRGB onto
/// it as it is (a Mac's screen, which takes Display P3, would show it over-saturated); the canvas
/// is shown in sRGB there, as on screens without wide colour.
const INTERFACE_WIDE: bool = false;

/// How soon a window left without its surface asks for it again.
const SURFACE_RETRY: Duration = Duration::from_millis(100);

/// How soon a frame whose interface couldn't go up to the GPU is drawn again, once the app has let
/// go of what it keeps.
const MISSED_RETRY: Duration = Duration::from_millis(50);

/// A window of no size is minimised: nothing shows until it has one.
fn is_minimised(size: winit::dpi::PhysicalSize<u32>) -> bool {
    size.width == 0 || size.height == 0
}

/// Whether the window's surface, set up at `set` (none: not set up), is to be set up again for the
/// window at `window`: when it isn't, or is at another size, and the window isn't minimised. A size
/// the window was given without its resize coming through (as it goes into or out of a Mac's full
/// screen) is caught by the next frame.
fn needs_set_up(set: Option<(u32, u32)>, window: winit::dpi::PhysicalSize<u32>) -> bool {
    !is_minimised(window) && set != Some((window.width, window.height))
}

impl Screen {
    /// The window's surface, and a device that can show it, and which backend made them: the
    /// first of the backends in `order` (see `Gpu::instance_after`), from backend `from` on (round
    /// to those before it), that makes both and can show on the window without an error; one whose
    /// driver panics gives way to the next too. `trying` is told each backend before it is tried
    /// (a driver that crashes takes the app down with it). Else what each backend said. The
    /// canvases' pipelines are then made on a thread of their own (see [`Screen::canvases_made`]),
    /// `wake` woken once they are. `tried` backends were tried before the app started again: only
    /// those left are.
    pub fn new(
        window: Arc<Window>,
        display: &OwnedDisplayHandle,
        order: &[usize],
        from: usize,
        tried: usize,
        mut trying: impl FnMut(usize),
        wake: winit::event_loop::EventLoopProxy<()>,
    ) -> Result<(Screen, usize), String> {
        let count = order.len().max(1);
        let left = count.saturating_sub(tried).max(1);
        let at = order.iter().position(|&b| b == from).unwrap_or(0);
        let mut failed = Vec::new();
        for tries in 0..left {
            let backend = order.get((at + tries) % count).copied().unwrap_or(from);
            let last = tries + 1 >= left;
            trying(backend);
            let started = Instant::now();
            let made =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Screen::on(&window, display, backend, last)))
                    .unwrap_or_else(|panic| Err(format!("panicked: {}", procraft_gpu::panic_message(panic.as_ref()))));
            let name = Gpu::backend_name(backend);
            match made {
                Ok(mut screen) => {
                    log::info!("graphics backend {backend} ({name}) ready in {} ms", started.elapsed().as_millis());
                    if screen.gpu.is_gl() {
                        screen.canvases_later = Some(wake);
                    } else {
                        screen.canvases = make_canvases(&screen.gpu, wake);
                    }
                    screen.canvases_since = Instant::now();
                    screen.tried = count - left + tries + 1;
                    return Ok((screen, backend));
                }
                Err(e) => {
                    log::warn!("graphics backend {backend} ({name}): {e}");
                    failed.push(format!("{name}: {e}"));
                }
            }
        }
        Err(failed.join("\n"))
    }

    /// [`Screen::new`] on backend `backend`; one that isn't the `last` to try fails on any GPU
    /// error, a lost device or a surface it can't show.
    fn on(window: &Arc<Window>, display: &OwnedDisplayHandle, backend: usize, last: bool) -> Result<Screen, String> {
        // each stage timed in the log: on a slow driver, where a start stopped shows
        let ms = |since: Instant| since.elapsed().as_millis();
        let window = Arc::clone(window);
        let started = Instant::now();
        let instance =
            Gpu::instance_for_display(backend, Box::new(display.clone())).ok_or("no such graphics backend")?;
        let surface = instance.create_surface(Arc::clone(&window)).map_err(|e| format!("the window's surface: {e}"))?;
        log::info!("the backend's instance and the window's surface made in {} ms", ms(started));
        let gpu = Gpu::for_surface(instance, &surface).map_err(|e| e.to_string())?;
        procraft_core::keep_device(&gpu);
        let size = window.inner_size();
        let configured = Instant::now();
        let config = configure(&gpu, &surface, (size.width, size.height), INTERFACE_WIDE);
        log::info!("the window's surface set up in {} ms", ms(configured));
        if !last && config.is_none() {
            return Err("the window's surface couldn't be set up".to_owned());
        }
        let format = config.as_ref().map_or(wgpu::TextureFormat::Bgra8Unorm, |c| c.format);
        let interface = Instant::now();
        let renderer = egui_wgpu::Renderer::new(&gpu.device, format, egui_wgpu::RendererOptions::default());
        log::info!("the interface's pipelines made in {} ms", ms(interface));
        if !last && (gpu.errors() > 0 || gpu.is_lost()) {
            return Err(format!("the interface's pipelines: {} GPU errors, lost {}", gpu.errors(), gpu.is_lost()));
        }
        let ctx = egui::Context::default();
        // Ctrl (⌘) with + - 0 zoom the canvas (the interface's shortcuts), not the whole interface
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let egui = egui_winit::State::new(
            ctx,
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            usize::try_from(gpu.max_texture_size()).ok(),
        );
        Ok(Screen {
            window,
            surface,
            config,
            gpu,
            egui,
            renderer,
            format,
            pad: None,
            canvases: None,
            canvases_since: Instant::now(),
            canvases_later: None,
            tried: 0,
            last,
            touch_keyboard: crate::touch_keyboard::TouchKeyboard::default(),
            pen_touch_screen: crate::input::PenTouchScreen::default(),
            misses: crate::interface_pass::Misses::default(),
            after_frame: crate::interface_pass::AfterFrame::Fine,
            surface_misses: crate::window_watch::SurfaceMisses::default(),
            size_told: crate::window_watch::LogBudget::new(10, Duration::from_secs(60)),
        })
    }

    /// What the app is to do after the last frame drawn: whether its interface missed (told once).
    pub fn take_after_frame(&mut self) -> crate::interface_pass::AfterFrame {
        std::mem::replace(&mut self.after_frame, crate::interface_pass::AfterFrame::Fine)
    }

    /// What the window's events gave the interface since its last frame; once a tablet's pen moves
    /// the mouse pointer, it is told it is on a touch screen (see [`crate::input::PenTouchScreen`]).
    pub fn take_input(&mut self) -> egui::RawInput {
        let mut raw = self.egui.take_egui_input(&self.window);
        self.pen_touch_screen.tell(&mut raw, crate::pen::mouse_pen().is_some());
        raw
    }

    /// Whether the canvases' pipelines were made without a GPU error, once they are made (told
    /// once).
    pub fn canvases_made(&mut self) -> Option<bool> {
        if !self.canvases.as_ref().is_some_and(std::thread::JoinHandle::is_finished) {
            return None;
        }
        Some(self.canvases.take()?.join().unwrap_or(false))
    }

    /// How long the canvases' pipelines have been being made, while they are (a canvas opened
    /// meanwhile waits for them).
    pub fn canvases_waited(&self) -> Option<Duration> {
        self.canvases.as_ref().map(|_| self.canvases_since.elapsed())
    }

    /// Whether the canvases' pipelines are to be made once the window's first frame shows (on
    /// OpenGL, see [`Screen::gl_busy`]): nothing is drawn while they are.
    pub fn canvases_later(&self) -> bool {
        self.canvases_later.is_some()
    }

    /// Whether nothing may be drawn now: on OpenGL while the canvases' pipelines are made, or the
    /// artwork open is exported or saved on a thread of its own. Its one context is held by one
    /// thread at a time, and one that waits for it more than a second panics (wgpu's lock), while
    /// a pipeline can take longer than that to compile, and an export's work on a big canvas (its
    /// picture made and read back) longer than that to send and wait for.
    pub fn gl_busy(&self) -> bool {
        self.gpu.is_gl()
            && (self.canvases.as_ref().is_some_and(|made| !made.is_finished())
                || crate::files::exporting_since().is_some())
    }

    /// Whether nothing can show: its next size brings it back.
    pub fn minimised(&self) -> bool {
        is_minimised(self.window.inner_size())
    }

    /// The window is `width` × `height` pixels now.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || self.gl_busy() {
            // minimised: nothing is shown until it comes back (on OpenGL while the pipelines are
            // made, the next frame after them sets it up)
            self.config = None;
            return;
        }
        self.config = configure(&self.gpu, &self.surface, (width, height), INTERFACE_WIDE);
        if let Some(format) = self.config.as_ref().map(|c| c.format).filter(|f| *f != self.format) {
            self.format = format;
            self.renderer = egui_wgpu::Renderer::new(&self.gpu.device, format, egui_wgpu::RendererOptions::default());
        }
    }

    /// The interface as `plugins` say for this window; `session` is told, so what it hit-tests goes
    /// with what is drawn. Everything in points follows, the canvas stays in pixels.
    /// A press under way holds it as it is: a slider growing with its own drag would run from the
    /// pointer dragging it.
    pub fn size_interface(&self, plugins: &Plugins, session: Option<&CanvasSession>) {
        let ctx = self.egui.egui_ctx();
        let (size, scale) = (self.window.inner_size(), self.window.scale_factor() as f32);
        let shown = plugins.interface_scale(size.width as f32 / scale, size.height as f32 / scale);
        if ctx.zoom_factor() != shown {
            ctx.request_repaint();
            if ctx.input(|i| i.pointer.any_down()) {
                return;
            }
            // at once, not from the next frame on: this one's input is about to be read at it
            ctx.options_mut(|o| o.zoom_factor = shown);
        }
        if let Some(session) = session {
            session.set_interface_size(shown);
        }
        // and in the accent they give, if any
        crate::look::set_accent(plugins.accent_colour());
    }

    /// Whether `p` (window pixels) is on the interface rather than the canvas, which shows in
    /// `canvas` (points).
    pub fn over_interface(&self, p: (f32, f32), canvas: egui::Rect) -> bool {
        on_interface(self.egui.egui_ctx(), p, canvas)
    }

    /// Draws `pad` (the brush studio's drawing pad) into its own picture, `size` pixels (made again
    /// at another size); returns the interface's name for the picture, to show it with.
    pub fn draw_pad(&mut self, pad: &CanvasSession, size: (u32, u32), started: Instant) -> egui::TextureId {
        let size = (size.0.max(1), size.1.max(1));
        let config = self.config.as_ref();
        let screen = config.map_or(procraft_engine::profile::Profile::Srgb, screen_profile);
        if self.pad.as_ref().is_none_or(|p| p.size != size) {
            let device = &self.gpu.device;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("drawing pad"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            // the interface shows the picture's values as they are, as the canvas wrote them
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let shown = &view;
            let id = match &self.pad {
                Some(p) => {
                    self.renderer.update_egui_texture_from_wgpu_texture(device, shown, wgpu::FilterMode::Linear, p.id);
                    p.id
                }
                None => self.renderer.register_native_texture(device, shown, wgpu::FilterMode::Linear),
            };
            pad.show_in(size.0, size.1);
            self.pad = Some(Pad { view, size, id });
        }
        let Some(p) = &self.pad else { return egui::TextureId::default() };
        pad.draw(&p.view, self.format, size, screen, started);
        p.id
    }

    /// The pad's picture goes (the studio closed).
    pub fn drop_pad(&mut self) {
        if let Some(p) = self.pad.take() {
            self.renderer.free_texture(&p.id);
        }
    }

    /// Draws the canvas (if an artwork is open) and the interface over it, and shows them. Returns
    /// how soon another frame is wanted, if one is (the interface can ask for one later, as a hold
    /// waits).
    pub fn draw(
        &mut self,
        session: Option<&CanvasSession>,
        started: Instant,
        output: egui::FullOutput,
    ) -> Option<Duration> {
        let egui::FullOutput { platform_output, mut textures_delta, shapes, pixels_per_point, viewport_output } =
            output;
        // a pen read by the app itself (on Wayland) has its cursor set apart from the mouse's
        crate::pen::set_cursor(platform_output.cursor_icon);
        self.egui.handle_platform_output(&self.window, platform_output);
        let typing = self.egui.egui_ctx().text_edit_focused();
        self.touch_keyboard.frame(&self.window, typing);
        let interface_after = viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.repaint_delay)
            .filter(|d| *d < Duration::from_secs(60 * 60));
        // textures change even when nothing is shown, so none is missed (the device keeps one
        // let go of until what drew with it is done)
        for (id, deltas) in std::mem::take(&mut textures_delta.set) {
            for delta in &deltas {
                crate::interface_pass::upload(&mut self.renderer, &self.gpu, id, delta);
            }
        }
        for id in std::mem::take(&mut textures_delta.free) {
            self.renderer.free_texture(&id);
        }
        // a surface the GPU refused (it couldn't come idle, another thread submitting work) is asked
        // for again here: only a new size would, and the window would stay as it was until then;
        // and one at another size than the window's is set up again at the window's
        let window = self.window.inner_size();
        let set = self.config.as_ref().map(|c| (c.width, c.height));
        if needs_set_up(set, window) {
            if let Some((width, height)) = set
                && let Some(end) = self.size_told.take(Instant::now())
            {
                log::info!(
                    "the window's surface was {width}×{height}, the window {}×{}: set up again{end}",
                    window.width,
                    window.height
                );
            }
            self.resize(window.width, window.height);
        }
        let Some(config) = self.config.as_ref() else {
            return (!is_minimised(window)).then_some(SURFACE_RETRY);
        };
        let size = (config.width, config.height);
        let (format, screen) = (config.format, screen_profile(config));
        let frame = match crate::window_watch::picture(self.surface.get_current_texture()) {
            Ok(frame) => frame,
            // drawn again whatever the surface said, soon, then less often while it says it: on a
            // Mac an occluded window is one the system counts as hidden, and it can be late to count
            // it as shown again (as it opens, or once its full screen space is ready); nothing else
            // may ask for a frame then, and the window would stay black
            Err(miss) => {
                let (retry, line) = self.surface_misses.missed(miss, Instant::now());
                if let Some(line) = line {
                    log::info!("{line}");
                }
                // made again at the window's size as it now is: the window changing is what put
                // the surface out of date, and made at its old size it would stay so
                if retry.set_up_again {
                    self.resize(window.width, window.height);
                }
                return Some(retry.after);
            }
        };
        if let Some(line) = self.surface_misses.shown(Instant::now()) {
            log::info!("{line}");
        }
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let canvas_after = session.and_then(|s| s.draw(&view, format, size, screen, started));

        let jobs = self.egui.egui_ctx().tessellate(shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [size.0, size.1], pixels_per_point };
        let mut encoder =
            self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("interface") });
        // over the canvas just drawn (the gallery covers the whole window)
        let load = if session.is_some() { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) };
        let recorded =
            crate::interface_pass::record(&mut self.renderer, &self.gpu, &mut encoder, &view, load, &jobs, &screen);
        let Some(uploads) = recorded else {
            // the interface couldn't go up to the GPU: the frame goes unshown (the window keeps the
            // last), and the app lets go of what it keeps and draws again (see `interface_pass`)
            self.after_frame = self.misses.frame(false);
            return Some(MISSED_RETRY);
        };
        self.after_frame = self.misses.frame(true);
        self.gpu.queue.submit(uploads.into_iter().chain([encoder.finish()]));
        self.window.pre_present_notify();
        self.gpu.queue.present(frame);
        // the backend shows: a crash from now on isn't its start's
        crate::startup::gpu_shown();
        // on OpenGL the canvases' pipelines once there is a window to see
        if let Some(wake) = self.canvases_later.take() {
            self.canvases = make_canvases(&self.gpu, wake);
            self.canvases_since = Instant::now();
        }
        // nor, once a canvas opening has drawn (or closed first), the canvas's
        if crate::startup::canvas_awaited() {
            crate::startup::canvas_shown(session.map(|s| s.frames_drawn() > 0));
            // the pipelines its first frames made kept for the next run, with the canvases'
            let gpu = self.gpu.clone();
            let thread = std::thread::Builder::new().name("procraft-keep-pipelines".to_owned());
            if let Err(e) = thread.spawn(move || gpu.keep_pipelines()) {
                log::warn!("keeping the pipelines: {e}");
            }
        }
        match (canvas_after, interface_after) {
            (Some(canvas), Some(interface)) => Some(canvas.min(interface)),
            (canvas, interface) => canvas.or(interface),
        }
    }
}

/// Whether `p` (window pixels) is on `ctx`'s interface rather than the canvas, which shows in
/// `canvas` (points): off the canvas, or on anything shown over it (an area, or what was
/// [`claim`]ed).
pub fn on_interface(ctx: &egui::Context, p: (f32, f32), canvas: egui::Rect) -> bool {
    let at = egui::pos2(p.0, p.1) / ctx.pixels_per_point();
    !canvas.contains(at) || ctx.layer_id_at(at).is_some_and(|l| l.order != egui::Order::Background) || claimed(ctx, at)
}

/// What the interface took of the window as the frames were drawn (the window's points): the
/// frame's, and the one's before.
#[derive(Clone, Default)]
struct Claims {
    frame: u64,
    now: Vec<egui::Rect>,
    before: Vec<egui::Rect>,
}

fn claims_id() -> egui::Id {
    egui::Id::unique("interface claims")
}

/// `rect` (`ui`'s points) is the interface's this frame, though its area doesn't take it (an area
/// is only as big as what it lays out, and a bar, a window's strip, a grabber or a handle is drawn
/// and touched without laying anything out): a press there never reaches the canvas, for all its
/// drag and its release.
pub fn claim(ui: &egui::Ui, rect: egui::Rect) {
    let to_window = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
    claim_window(ui.ctx(), to_window.mul_rect(rect));
}

/// [`claim`] for `rect` in the window's points.
pub fn claim_window(ctx: &egui::Context, rect: egui::Rect) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| {
        let c = d.get_temp_mut_or_default::<Claims>(claims_id());
        if c.frame != frame {
            // (from the frame just before, or none: older ones show no more)
            let last = std::mem::take(&mut c.now);
            c.before = if c.frame + 1 == frame { last } else { Vec::new() };
            c.frame = frame;
        }
        c.now.push(rect);
    });
}

/// Whether `at` (points) was claimed by the frame drawn last (between frames), or by the one drawing
/// now or the one before (as it draws).
fn claimed(ctx: &egui::Context, at: egui::Pos2) -> bool {
    let frame = ctx.cumulative_frame_nr();
    ctx.data(|d| {
        d.get_temp::<Claims>(claims_id()).is_some_and(|c| {
            let now = c.frame + 1 >= frame && c.now.iter().any(|r| r.contains(at));
            now || (c.frame == frame && c.before.iter().any(|r| r.contains(at)))
        })
    })
}

#[cfg(test)]
mod tests {
    use winit::dpi::PhysicalSize;

    use super::{claim, is_minimised, needs_set_up, on_interface};

    #[test]
    fn only_a_window_of_no_size_counts_as_minimised() {
        assert!(is_minimised(PhysicalSize::new(0, 0)));
        assert!(is_minimised(PhysicalSize::new(0, 600)));
        assert!(!is_minimised(PhysicalSize::new(1, 1)));
    }

    /// The surface is set up again when the window's size isn't the one it was set up at (a
    /// window gone full screen on a Mac without its resize coming through stayed at its old size),
    /// or it isn't set up; never for a minimised window, nor at the size it has.
    #[test]
    fn a_surface_at_another_size_than_the_window_is_set_up_again() {
        let window = PhysicalSize::new(2560, 1440);
        assert!(!needs_set_up(Some((2560, 1440)), window));
        assert!(needs_set_up(Some((1440, 900)), window));
        assert!(needs_set_up(None, window));
        assert!(!needs_set_up(Some((1440, 900)), PhysicalSize::new(0, 0)));
        assert!(!needs_set_up(None, PhysicalSize::new(1440, 0)));
    }

    /// A handle drawn and touched in an area that lays nothing out (Edit layout's Done, a floating
    /// window's grabber, a folded window's tab): the area alone is no bigger than a point, so a
    /// press on the handle went to the canvas and painted; claimed, it is the interface's while it
    /// shows and for the frame after, and the canvas's again once it is gone.
    #[test]
    fn a_handle_drawn_without_laying_anything_out_keeps_its_presses_off_the_canvas() {
        let ctx = egui::Context::default();
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0));
        let done = egui::Rect::from_min_size(egui::pos2(500.0, 700.0), egui::vec2(90.0, 44.0));
        let frame = |claimed: bool| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::Area::new(egui::Id::unique("handles"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        let _ = ui.interact(done, egui::Id::unique("done"), egui::Sense::click());
                        if claimed {
                            claim(ui, done);
                        }
                    });
            });
            output.textures_delta.clear();
        };
        let on_done = (545.0, 722.0);
        // drawn but not claimed: the press goes through to the canvas
        frame(false);
        assert!(!on_interface(&ctx, on_done, canvas));
        // claimed: the interface's, between frames and while the next draws
        frame(true);
        assert!(on_interface(&ctx, on_done, canvas));
        assert!(!on_interface(&ctx, (100.0, 100.0), canvas), "the rest of the canvas stays the canvas's");
        frame(true);
        assert!(on_interface(&ctx, on_done, canvas));
        // gone (a frame without it): the canvas's again
        frame(false);
        assert!(!on_interface(&ctx, on_done, canvas));
    }
}
