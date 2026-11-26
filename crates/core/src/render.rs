//! The render thread: owns the window's surface and draws a frame whenever something changed.

use std::sync::Arc;
use std::time::Duration;

use procraft_engine::profile::Profile;
use procraft_gpu::{Gpu, wgpu};

use crate::Shared;
use crate::frame::{configure, configure_with, screen_profile};
use crate::frame_loop::Acquired;

/// The window drawn into: Android's native window, or the iPad app's `CAMetalLayer`.
#[cfg(target_os = "android")]
type Window = ndk::native_window::NativeWindow;
#[cfg(target_os = "ios")]
type Window = crate::layer::Layer;

pub(crate) fn run(shared: Arc<Shared>, window: Window) {
    let own = shared.lock().gpu.as_ref().map(|g| g.instance.clone());
    // the app's device (waited for while it is being made), else one made for this window and
    // kept for every artwork after
    let mut kept = crate::device::kept();
    // one lost holds and draws nothing: this artwork makes a new one, and opens as last saved
    if kept.as_ref().is_some_and(Gpu::is_lost) {
        log::warn!("the GPU device was lost: making a new one");
        *kept = None;
    }
    // where to make a device from, if there is none to draw with
    let from = if own.is_none() && kept.is_none() { crate::device::first_backend() } else { 0 };
    // until the canvas first shows, a crash counts against the device's backend
    let mut showing = crate::device::Showing::begin();
    let mut instance = own
        .clone()
        .or_else(|| kept.as_ref().map(|g| g.instance.clone()))
        .or_else(|| Gpu::instance_after(from))
        .unwrap_or_else(Gpu::instance);
    // while a device is still to be made, a backend that can't make the window's surface gives way
    // to the next (one whose driver can draw offscreen but not on a window would otherwise leave
    // the canvas blank)
    let count = Gpu::backend_count().max(1);
    let choosing = own.is_none() && kept.is_none();
    let mut tries = 0;
    let mut surface = loop {
        match crate::window::create_surface(&instance, &window) {
            Ok(s) => break s,
            Err(e) => {
                let next =
                    (choosing && tries + 1 < count).then(|| Gpu::instance_after((from + tries + 1) % count)).flatten();
                let Some(next) = next else { return fail(&shared, e) };
                log::warn!("the window's surface: {e}; trying the next backend");
                tries += 1;
                instance = next;
            }
        }
    };
    let device = if own.is_some() {
        None
    } else if let Some(gpu) = kept.as_ref() {
        Some(gpu.clone())
    } else {
        // the first instance, from the backend the app starts on, that makes a device for the
        // window (see `Gpu::instance_after`, `device::first_backend`)
        loop {
            let backend = (from + tries) % count;
            crate::device::trying(backend);
            match Gpu::for_surface(instance.clone(), &surface) {
                Ok(gpu) => {
                    crate::device::made_on(backend);
                    showing = crate::device::Showing::begin();
                    *kept = Some(gpu.clone());
                    break Some(gpu);
                }
                Err(e) => {
                    // the next backend that makes the window's surface, if any is left
                    let mut why = e.to_string();
                    let made = loop {
                        tries += 1;
                        let next = if tries < count { Gpu::instance_after((from + tries) % count) } else { None };
                        let Some(next) = next else { break None };
                        log::warn!("{why}; trying the next backend");
                        match crate::window::create_surface(&next, &window) {
                            Ok(s) => break Some((next, s)),
                            Err(e) => why = format!("the window's surface: {e}"),
                        }
                    };
                    let Some((next, s)) = made else { return fail(&shared, why) };
                    surface = s;
                    instance = next;
                }
            }
        }
    };
    drop(kept);
    if let Some(gpu) = device {
        // it holds the session's layers from now on
        shared.lock().gpu.get_or_insert(gpu);
    }
    shared.make_canvas();
    let queue = {
        let mut st = shared.lock();
        st.reconfigure = true;
        st.needs_frame = true;
        st.gpu.as_ref().map(|g| (g.queue.clone(), g.clone()))
    };
    let Some((queue, gpu)) = queue else { return };

    let mut screen = WindowScreen {
        surface,
        queue,
        gpu,
        caps: None,
        config: None,
        #[cfg(target_os = "android")]
        refreshes: crate::window::Refreshes::start(),
    };
    let mut pipelines_kept = false;
    crate::frame_loop::draw_loop(&shared, &mut screen, || {
        showing.shown();
        if !std::mem::replace(&mut pipelines_kept, true) {
            keep_pipelines(&shared);
        }
    });
    drop(screen);
    drop(window);
}

/// The window's surface, as the render loop draws into it: its capabilities asked once (they last
/// as long as it does), how it was last configured, and (on Android 13 and later) the screen's
/// refreshes as the system tells them.
struct WindowScreen {
    surface: wgpu::Surface<'static>,
    queue: wgpu::Queue,
    gpu: Gpu,
    caps: Option<wgpu::SurfaceCapabilities>,
    config: Option<wgpu::SurfaceConfiguration>,
    #[cfg(target_os = "android")]
    refreshes: Option<crate::window::Refreshes>,
}

impl crate::frame_loop::Screen for WindowScreen {
    type Image = wgpu::SurfaceTexture;

    fn configure(&mut self, gpu: &Gpu, size: (u32, u32), wide: bool) -> Option<(wgpu::TextureFormat, Profile)> {
        let caps = self.caps.get_or_insert_with(|| self.surface.get_capabilities(&gpu.adapter));
        self.config = configure_with(gpu, &self.surface, caps, size, wide);
        self.config.as_ref().map(|c| (c.format, screen_profile(c)))
    }

    fn acquire(&mut self) -> Acquired<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => Acquired::Ready(f),
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => Acquired::Suboptimal(f),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => Acquired::Outdated,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => Acquired::Unavailable,
        }
    }

    fn view(&self, image: &wgpu::SurfaceTexture) -> wgpu::TextureView {
        image.texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn present(&mut self, image: wgpu::SurfaceTexture) {
        self.queue.present(image);
    }

    #[cfg(target_os = "android")]
    fn refreshes(&mut self, drawing: bool) -> Option<crate::pacing::Timeline> {
        self.refreshes.as_ref().and_then(|r| r.timeline(drawing))
    }

    fn gpu_idle(&mut self) -> bool {
        // looked at, never waited on: a wait with a timeout lost the device on a tablet's driver
        // (one without, as the canvas's own, never did)
        let now = wgpu::PollType::Wait { submission_index: None, timeout: Some(Duration::ZERO) };
        self.gpu.is_lost() || self.gpu.device.poll(now).is_ok()
    }
}

/// Keeps the device's pipelines for the next run (see [`Gpu::keep_pipelines`]): the screen's,
/// which a canvas's first frame makes, with the canvases'. On a thread of its own (it writes a
/// file).
fn keep_pipelines(shared: &Shared) {
    let Some(gpu) = shared.lock().gpu.clone() else { return };
    let thread = std::thread::Builder::new().name("procraft-keep-pipelines".to_owned());
    if let Err(e) = thread.spawn(move || gpu.keep_pipelines()) {
        log::warn!("keeping the pipelines: {e}");
    }
}

/// How long the second display waits at most for a new frame of the main window before it looks
/// again (its size or its stop come sooner, as each wakes it).
const PROJECTION_WAIT: Duration = Duration::from_secs(1);

/// Project canvas's thread: the whole canvas fitted and centred on black on a second display,
/// drawn again after each frame of the main window.
pub(crate) fn project(shared: Arc<Shared>, window: Window) {
    // the device comes with the main window
    let instance = loop {
        {
            let st = shared.lock();
            if st.projection_stop {
                return;
            }
            if let Some(gpu) = st.gpu.as_ref() {
                break gpu.instance.clone();
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let surface = match crate::window::create_surface(&instance, &window) {
        Ok(s) => s,
        Err(e) => return fail(&shared, e),
    };
    let Some(queue) = shared.lock().gpu.as_ref().map(|g| g.queue.clone()) else { return };
    let mut config: Option<wgpu::SurfaceConfiguration> = None;
    let mut size = (0, 0);
    let mut wide = false;
    let mut drawn: Option<u64> = None;
    loop {
        let (wanted, frames, wants_wide) = {
            let st = shared.lock();
            if st.projection_stop {
                break;
            }
            (st.projection_size, st.frames, st.wide_colour())
        };
        if wanted != size || config.is_none() || wants_wide != wide {
            size = wanted;
            let st = shared.lock();
            let Some(gpu) = st.gpu.as_ref() else { break };
            config = configure(gpu, &surface, size, wants_wide);
            wide = wants_wide;
            drawn = None;
        }
        let Some(format) = config.as_ref().map(|c| c.format) else {
            std::thread::sleep(Duration::from_millis(16));
            continue;
        };
        if drawn == Some(frames) {
            // until the main window draws a frame (it wakes this), or the display's size or stop
            // come
            let mut st = shared.lock();
            if st.frames == frames && st.projection_size == size && !st.projection_stop {
                let _ = shared.wake.wait_for(&mut st, PROJECTION_WAIT);
            }
            continue;
        }
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                config = None;
                std::thread::sleep(Duration::from_millis(4));
                continue;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => {
                std::thread::sleep(Duration::from_millis(4));
                continue;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        {
            let mut st = shared.lock();
            if st.projection_size != size {
                continue;
            }
            let screen = config.as_ref().map_or(Profile::Srgb, screen_profile);
            st.draw_fitted(&view, format, size, screen);
        }
        drawn = Some(frames);
        queue.present(frame);
    }
    // the picture goes with the canvas: the display is left black
    if config.is_some()
        && let wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) =
            surface.get_current_texture()
    {
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        if let Some(gpu) = shared.lock().gpu.as_ref() {
            let mut encoder =
                gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("projection clear") });
            drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("projection clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
            gpu.queue.submit([encoder.finish()]);
        }
        queue.present(frame);
    }
    drop(surface);
    drop(window);
}

fn fail(shared: &Shared, reason: String) {
    log::error!("render thread: {reason}");
    shared.lock().error = Some(reason);
}
