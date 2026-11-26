//! Project canvas, as the Android app's (canvas/Projection.kt): with it on and a second display
//! attached, that display shows the whole canvas fitted and centred on black, drawn again after
//! each frame of the main window, and black while the gallery shows.

use std::sync::Arc;

use procraft_core::{CanvasSession, configure, screen_profile};
use procraft_gpu::{Gpu, wgpu};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowAttributes};

pub struct Projection {
    pub window: Arc<Window>,
    /// The display it fills.
    pub monitor: MonitorHandle,
    surface: wgpu::Surface<'static>,
    config: Option<wgpu::SurfaceConfiguration>,
    /// What it last showed: the canvas's frame, or none for black; none yet, drawn again.
    drawn: Option<Option<u64>>,
}

impl Projection {
    /// A borderless window filling `monitor`, its surface on `gpu`'s device.
    pub fn open(event_loop: &ActiveEventLoop, monitor: MonitorHandle, gpu: &Gpu) -> Option<Projection> {
        let attributes = Window::default_attributes()
            .with_title(crate::APP_NAME)
            .with_decorations(false)
            // the keyboard stays with the main window
            .with_active(false);
        let window = match event_loop.create_window(filling(attributes, &monitor)) {
            Ok(window) => Arc::new(window),
            Err(e) => {
                log::warn!("Project canvas: its window couldn't be made: {e}");
                return None;
            }
        };
        let surface = match gpu.instance.create_surface(Arc::clone(&window)) {
            Ok(surface) => surface,
            Err(e) => {
                log::warn!("Project canvas: its window's surface couldn't be made: {e}");
                return None;
            }
        };
        let size = window.inner_size();
        // only the canvas is shown there: in wide colour where the display takes it
        let config = configure(gpu, &surface, (size.width, size.height), true);
        let at = monitor.position();
        log::info!(
            "Project canvas: opened on {} at {},{} ({}×{}), its window {}×{}",
            monitor.name().unwrap_or_else(|| "a display".to_owned()),
            at.x,
            at.y,
            monitor.size().width,
            monitor.size().height,
            size.width,
            size.height
        );
        Some(Projection { window, monitor, surface, config, drawn: None })
    }

    /// The window is `width` × `height` pixels now.
    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        self.config = if width > 0 && height > 0 { configure(gpu, &self.surface, (width, height), true) } else { None };
        self.drawn = None;
    }

    /// Shows the canvas of `session` (black with none) if that changed since it was last shown.
    pub fn draw(&mut self, gpu: &Gpu, session: Option<&CanvasSession>) {
        let Some(config) = self.config.as_ref() else { return };
        let showing = session.map(CanvasSession::frames_drawn);
        if self.drawn == Some(showing) {
            return;
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            // set up again at the window's size as it now is (at its old size it would stay out of
            // date)
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                let size = self.window.inner_size();
                self.resize(gpu, size.width, size.height);
                return;
            }
            // (occluded: drawn again once it shows, or with the main window's next frame)
            _ => return,
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let size = (config.width, config.height);
        match session {
            Some(session) => session.draw_projection(&view, config.format, size, screen_profile(config)),
            None => {
                let mut encoder =
                    gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("projection") });
                drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("projection"),
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
        }
        self.window.pre_present_notify();
        gpu.queue.present(frame);
        self.drawn = Some(showing);
    }
}

/// `attributes` for a window over the whole of `monitor`. On a Mac a window without a frame put
/// over the display, not the system's full screen: that makes a space of its own for the window,
/// slides the display into it and hands it the keyboard, and with each display not given spaces of
/// its own it blanks the other displays, the main window's with them. Project canvas is kept on
/// between runs, so that came back at each start, with the window that turns it off out of sight.
#[cfg(target_os = "macos")]
fn filling(attributes: WindowAttributes, monitor: &MonitorHandle) -> WindowAttributes {
    // in the display's points, as the system places windows (a display's pixels are its own scale's)
    let scale = monitor.scale_factor();
    attributes
        .with_position(monitor.position().to_logical::<f64>(scale))
        .with_inner_size(monitor.size().to_logical::<f64>(scale))
        .with_resizable(false)
}

/// `attributes` for a window over the whole of `monitor`: full screen there, without a frame.
#[cfg(not(target_os = "macos"))]
fn filling(attributes: WindowAttributes, monitor: &MonitorHandle) -> WindowAttributes {
    attributes.with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(monitor.clone()))))
}

/// The display a projection goes on: the first of `displays` that isn't `main`, the one the main
/// window is on; none if there is no other.
pub fn other_display<T: PartialEq>(displays: impl IntoIterator<Item = T>, main: &T) -> Option<T> {
    displays.into_iter().find(|d| d != main)
}

/// Whether `key`, `state`, on the projection's window turns Project canvas off: Escape pressed. The
/// window has no frame to close it by, and covers its display.
pub fn closes(key: &winit::keyboard::Key, state: winit::event::ElementState) -> bool {
    state.is_pressed() && *key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
}

#[cfg(test)]
mod tests {
    use winit::event::ElementState;
    use winit::keyboard::{Key, NamedKey};

    use super::{closes, other_display};

    /// The projection goes on a display the main window isn't on, never on the main window's own
    /// (it would cover the window that turns it off), and nowhere with one display.
    #[test]
    fn the_projection_goes_on_another_display_than_the_main_windows_only() {
        assert_eq!(other_display([1, 2], &1), Some(2));
        assert_eq!(other_display([1, 2], &2), Some(1));
        assert_eq!(other_display([1, 2, 3], &2), Some(1));
        assert_eq!(other_display([1], &1), None);
        assert_eq!(other_display(Vec::<u32>::new(), &1), None);
    }

    /// Escape pressed on the projection's window turns it off; released, or another key, doesn't.
    #[test]
    fn escape_pressed_on_the_projection_turns_it_off() {
        let escape = Key::Named(NamedKey::Escape);
        assert!(closes(&escape, ElementState::Pressed));
        assert!(!closes(&escape, ElementState::Released));
        assert!(!closes(&Key::Named(NamedKey::Enter), ElementState::Pressed));
        assert!(!closes(&Key::Character("q".into()), ElementState::Pressed));
    }
}
