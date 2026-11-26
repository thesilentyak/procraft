//! ProCraft's GPU renderer (wgpu): layer pixels, stroke rendering, compositing, and drawing the
//! canvas to a window surface. Layers are premultiplied RGBA8 textures the size of the canvas.
//!
//! Every blend here uses fixed-function blending or reads from a different texture than it writes:
//! nothing relies on reading the render target in the shader (Android GPUs have no portable way).

mod canvas;
#[cfg(windows)]
mod compiler;
// (naga's GLSL writer comes with the OpenGL backend, which Apple's systems go without)
#[cfg(all(test, not(target_vendor = "apple")))]
mod gles_shaders;
mod kept;
mod lighting;
mod placed;
mod scene;
mod sphere;
mod stale;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

pub use canvas::filter_layers::{FilterReach, FilterTable, LayerFilterProgram, MOST_LAYER_FILTER_VALUES};
pub use canvas::plugin_filter::{
    FilterCode, FilterProgram, FilterRun, read_filter, read_layer_filter, read_layer_table_filter,
};
pub use canvas::plugin_picture::{PictureCode, PictureProgram, PictureStop, read_picture, read_picture_given};
pub use canvas::{
    Backdrop, Canvas, DisplayLut, FillPreview, Filter, FineThumbnail, LayerStore, LayerThumbnailStep, LiquifyMode,
    LiquifyStamp, MOST_TIPS, PendingAlphas, PendingThumbnail, Readback, ReadyToPark, RemakeStats, Resample, STORE_LIT,
    SecondStroke, SelectionLook, SharedStroke, SmallScene, Stripes, StrokeStyle, StrokeTool, SurroundGrid, Target,
    Thumbnail, TipKind, VEIL_GRID, VeilCell, VeilCells, grey_levels, most_resident, read_texture, start_reading,
    textures_bytes,
};
pub use canvas::{CanvasPaper, CarryFault, GREY_LOOK, ViewLook};
pub use kept::keep_pipelines_in;
pub use lighting::Lighting;
pub use scene::{
    GrainLook, Light, MOST_LIGHTS, MaterialLook, MeshView, Orbit, ShownSelection, SurfaceMap, material_source,
};
pub use sphere::{SphereLook, SpherePreview, preview_environment};
pub use stale::{STALE, StaleTextures, starts_stale};
pub use wgpu;

#[cfg(windows)]
use compiler::with_compiler;

/// The canvas's textures are made so the GPU keeps them uncompressed: on Mali GPUs (Vulkan), whose
/// block compression (AFBC) can lose blocks of a picture redrawn a part at a time (a stroke's
/// live picture showed holes in a grid on a Mali-G68).
static UNCOMPRESSED: AtomicBool = AtomicBool::new(false);

/// Keeps the canvas's textures made from now on uncompressed (`on`), as on a GPU that needs it:
/// for tests on other GPUs.
#[doc(hidden)]
pub fn keep_textures_uncompressed(on: bool) {
    UNCOMPRESSED.store(on, Ordering::Relaxed);
}

/// Whether the canvas's textures are to be kept uncompressed (see [`UNCOMPRESSED`]).
pub(crate) fn textures_uncompressed() -> bool {
    UNCOMPRESSED.load(Ordering::Relaxed)
}

/// What a panic said (its message, when it gave one as text).
pub fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic.downcast_ref::<&str>().copied().or_else(|| panic.downcast_ref::<String>().map(String::as_str)).unwrap_or("")
}

/// Whether a wait on the GPU that failed has been told in the log: only the first is.
static WAIT_FAILED_TOLD: AtomicBool = AtomicBool::new(false);

/// Polls `device` as `how` says (waiting for what was sent to it, or not), failing soft: wgpu's own
/// poll panics where the device is lost or out of memory (errors it can't return), which here is
/// [`GpuError::Lost`], and any other failure [`GpuError::Failed`], the first of them logged. A wait
/// that runs out of time fails too, as nothing wrong: not logged. A wait without end goes in slices
/// (see [`WAIT_SLICE`]).
pub fn poll(device: &wgpu::Device, how: wgpu::PollType) -> Result<wgpu::PollStatus, GpuError> {
    let submission_index = match how {
        wgpu::PollType::Wait { submission_index, timeout: None } => submission_index,
        how => return polled(|| device.poll(how)),
    };
    loop {
        let slice = wgpu::PollType::Wait { submission_index: submission_index.clone(), timeout: Some(WAIT_SLICE) };
        let polled_now = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| device.poll(slice)));
        if !matches!(polled_now, Ok(Err(wgpu::PollError::Timeout))) {
            return told(polled_now);
        }
        // the context let go a moment, for a thread waiting on it to take it
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// The longest a wait on the GPU holds it at once: on OpenGL a wait holds the one context the whole
/// time, and wgpu panics in any other thread kept from it for a second (the window's frame, as an
/// export waited for a big canvas to be read back). Between slices the context is let go.
const WAIT_SLICE: std::time::Duration = std::time::Duration::from_millis(250);

/// [`poll`] with the poll given, as wgpu's might go.
fn polled(poll: impl FnOnce() -> Result<wgpu::PollStatus, wgpu::PollError>) -> Result<wgpu::PollStatus, GpuError> {
    told(std::panic::catch_unwind(std::panic::AssertUnwindSafe(poll)))
}

/// What a poll came to, as [`poll`] gives it, the first failure logged.
fn told(polled: std::thread::Result<Result<wgpu::PollStatus, wgpu::PollError>>) -> Result<wgpu::PollStatus, GpuError> {
    let (error, why) = match polled {
        Ok(Ok(status)) => return Ok(status),
        Ok(Err(wgpu::PollError::Timeout)) => return Err(GpuError::Failed("the GPU took too long".to_owned())),
        Ok(Err(e)) => (GpuError::Failed(format!("waiting on the GPU: {e}")), e.to_string()),
        Err(panic) => (GpuError::Lost, panic_message(panic.as_ref()).to_owned()),
    };
    if !WAIT_FAILED_TOLD.swap(true, Ordering::Relaxed) {
        log::error!("waiting on the GPU failed ({why}); later failures aren't logged");
    }
    Err(error)
}

/// The graphics APIs in `backends` by name ("Vulkan", "Direct3D 12", "OpenGL", ...).
fn backends_name(backends: wgpu::Backends) -> String {
    let named = [
        (wgpu::Backends::VULKAN, "Vulkan"),
        (wgpu::Backends::DX12, "Direct3D 12"),
        (wgpu::Backends::METAL, "Metal"),
        (wgpu::Backends::GL, "OpenGL"),
    ];
    let names: Vec<&str> = named.iter().filter(|(b, _)| backends.contains(*b)).map(|(_, name)| *name).collect();
    if names.is_empty() { format!("{backends:?}") } else { names.join(" or ") }
}

/// The layers an array texture of `n` is made with on OpenGL (ES): at least two (one makes a plain
/// 2D texture), and never six or a multiple of it, which wgpu's OpenGL takes for a cube map (a
/// square texture's): read as an array, that gave nothing (every brush shape and grain blank once
/// there were 58 shapes and 8 spare layers).
fn gl_array_layers(n: u32) -> u32 {
    let n = n.max(2);
    if n.is_multiple_of(6) { n + 1 } else { n }
}

/// Whether a GPU named `name` is a weak one, as budget tablets and phones have (PowerVR's, the
/// smaller Malis, Adrenos below the 630s and 710), or draws on the CPU: what costs it most and
/// shows least is made cheaper (the glass's blur is worked out on a smaller picture).
fn weak_gpu(name: &str, device_type: wgpu::DeviceType) -> bool {
    if device_type == wgpu::DeviceType::Cpu {
        return true;
    }
    let name = name.to_ascii_lowercase();
    // the number after `prefix`, if the name has one there ("adreno (tm) 610" → 610)
    let number_after = |prefix: &str| -> Option<u32> {
        let rest = name.get(name.find(prefix)? + prefix.len()..)?;
        let digits: String = rest.trim_start().chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    if name.contains("powervr") || name.contains("llvmpipe") || name.contains("swiftshader") {
        return true;
    }
    if name.contains("mali-t") {
        return true;
    }
    if let Some(n) = number_after("mali-g") {
        return matches!(n, 31 | 51 | 52 | 53 | 57 | 71 | 72 | 310);
    }
    if let Some(n) = number_after("adreno (tm)").or_else(|| number_after("adreno")) {
        return n < 630 || (700..710).contains(&n);
    }
    false
}

/// Every GPU taken for a weak one (see [`weak_gpu`]): for measuring what that saves on others.
static WEAK: AtomicBool = AtomicBool::new(false);

/// Takes every GPU for a weak one from now on (`on`), as [`Gpu::is_weak`] tells: for measuring.
#[doc(hidden)]
pub fn treat_as_weak(on: bool) {
    WEAK.store(on, Ordering::Relaxed);
}

/// What a device made as an old GPU's is held to (see [`Gpu::headless_old`]): what a 2014 Mali
/// offers on OpenGL ES 3.1 at the least. The downlevel limits, with no storage buffers or images
/// in any stage (wgpu takes the vertex and fragment stages' counts, often none there, for every
/// stage), the GLES 3.1 compute minimums and 4096-pixel textures.
fn old_gpu_limits() -> wgpu::Limits {
    wgpu::Limits {
        max_texture_dimension_1d: 4096,
        max_texture_dimension_2d: 4096,
        max_storage_buffers_per_shader_stage: 0,
        max_storage_textures_per_shader_stage: 0,
        max_dynamic_storage_buffers_per_pipeline_layout: 0,
        max_storage_buffer_binding_size: 0,
        max_compute_invocations_per_workgroup: 128,
        max_compute_workgroup_size_x: 128,
        max_compute_workgroup_size_y: 128,
        max_compute_workgroup_size_z: 64,
        max_compute_workgroup_storage_size: 16 << 10,
        ..wgpu::Limits::downlevel_defaults()
    }
}

/// Whether a GPU named `name` on `backend` needs its canvas textures uncompressed.
fn keeps_textures_uncompressed(name: &str, backend: wgpu::Backend) -> bool {
    backend == wgpu::Backend::Vulkan && name.to_ascii_lowercase().contains("mali")
}

/// Whether a GPU named `name` starts on OpenGL ES on Android: an old PowerVR Rogue (GE8100,
/// GE8300, GE8320 and the like, in budget MediaTek and Unisoc phones and tablets), whose Vulkan
/// driver crashed the app in the queue's submit some seconds after a canvas opened.
fn crashes_on_vulkan(name: &str) -> bool {
    name.to_ascii_lowercase().contains("powervr rogue")
}

/// The backends on Android, best first, for the GPUs Vulkan sees (`vulkan_gpus`): Vulkan, then
/// OpenGL ES (for a device whose Vulkan is missing or fails); the other way round for a GPU whose
/// Vulkan driver is known to crash (see [`crashes_on_vulkan`]).
fn android_backends(vulkan_gpus: &[String]) -> Vec<wgpu::Backends> {
    if vulkan_gpus.iter().any(|name| crashes_on_vulkan(name)) {
        vec![wgpu::Backends::GL, wgpu::Backends::VULKAN]
    } else {
        vec![wgpu::Backends::VULKAN, wgpu::Backends::GL]
    }
}

/// Whether a device on `backend` whose canvases' pipelines failed in part is kept, not given up
/// for the next backend: OpenGL ES, not lost, where it is kept whatever fails (`kept`, see
/// [`opengl_kept`]). What needs the pipelines that failed doesn't draw; the rest does.
fn takes_opengl_with_errors(backend: wgpu::Backend, lost: bool, kept: bool) -> bool {
    let taken = backend == wgpu::Backend::Gl && !lost && kept;
    if taken {
        log::warn!(
            "OpenGL ES kept all the same, as Vulkan's driver crashes on this GPU: only what needs the pipelines \
             that failed (above) won't draw"
        );
    }
    taken
}

/// Whether the app's record says Vulkan's driver crashed the app on this device and system (see
/// [`vulkan_crashed`]).
static VULKAN_CRASHED: AtomicBool = AtomicBool::new(false);

/// Tells the GPU whether Vulkan's driver crashed the app on this device and system, as the app's
/// record of its starts says: OpenGL ES is then kept even where some of the canvases' pipelines
/// fail on it (see [`Gpu::for_canvases`]).
pub fn vulkan_crashed(crashed: bool) {
    VULKAN_CRASHED.store(crashed, Ordering::Relaxed);
}

/// Whether OpenGL ES is kept whatever pipelines fail on it, never giving way to Vulkan: Vulkan's
/// driver crashed the app here (see [`vulkan_crashed`]), or is known to crash on this GPU (see
/// [`crashes_on_vulkan`]). A crash takes the app down where a pipeline that fails costs only what
/// needs it.
fn opengl_kept() -> bool {
    VULKAN_CRASHED.load(Ordering::Relaxed)
        || (cfg!(target_os = "android") && vulkan_gpus().iter().any(|name| crashes_on_vulkan(name)))
}

/// The GPUs Vulkan sees, by name, asked once (it lists them, making no device); none where it
/// can't.
fn vulkan_gpus() -> &'static [String] {
    static SEEN: OnceLock<Vec<String>> = OnceLock::new();
    SEEN.get_or_init(|| {
        let seen: Vec<String> = std::panic::catch_unwind(|| Gpu::adapters(wgpu::Backends::VULKAN))
            .unwrap_or_default()
            .into_iter()
            .map(|a| a.name)
            .collect();
        if let Some(name) = seen.iter().find(|name| crashes_on_vulkan(name)) {
            log::info!("{name}: its Vulkan driver is known to crash, so OpenGL ES first");
        }
        seen
    })
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no suitable GPU: {0}")]
    NoAdapter(String),
    #[error("GPU device: {0}")]
    Device(String),
    #[error("canvas of {0}×{1} pixels is larger than this GPU allows ({2})")]
    TooLarge(u32, u32, u32),
    #[error("surface: {0}")]
    Surface(String),
    #[error("reading pixels back: {0}")]
    Readback(String),
    #[error("the GPU is out of memory: {0}")]
    OutOfMemory(String),
    #[error("the GPU device was lost")]
    Lost,
    #[error("GPU work failed: {0}")]
    Failed(String),
}

/// The device everything renders with. A clone is the same device, sharing what is made for it.
#[derive(Clone)]
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    errors: Arc<AtomicU64>,
    /// Whether the device has been lost (its driver reset or stopped).
    lost: Arc<AtomicBool>,
    /// The canvases' pipelines, once the first canvas has made them.
    canvas_pipelines: Arc<OnceLock<Arc<canvas::Pipelines>>>,
    /// The pipeline cache kept between runs (see [`kept`]), once the canvases' pipelines are made
    /// with it.
    kept: Arc<OnceLock<Option<kept::Kept>>>,
    /// Buffers pixels came back from the GPU through, kept for the next readback (see
    /// [`Gpu::readback_buffer`]).
    readbacks: Arc<std::sync::Mutex<Vec<wgpu::Buffer>>>,
    /// Made as an old GPU's (see [`Gpu::headless_old`]).
    old: bool,
    /// Whether its copies are made as Direct3D 12 without unrestricted copy pitches makes them
    /// (see [`Gpu::emulate_placed_copies`]).
    placed: Arc<placed::Placed>,
    /// Whether its new textures start stale, so each is cleared as it is made (see `stale.rs`).
    stale: Arc<stale::Stale>,
    /// Whether a dual brush's parts laid by their rule are carried from lot to lot here: checked
    /// before the first stroke that would carry one (see `canvas/carry_check.rs`).
    carry: Arc<canvas::CarryCheck>,
}

/// Elsewhere there's no Direct3D 12 to choose a shader compiler for.
#[cfg(not(windows))]
fn with_compiler(desc: wgpu::InstanceDescriptor) -> wgpu::InstanceDescriptor {
    desc
}

impl Gpu {
    /// A wgpu instance for this platform: Vulkan on Android, the native API elsewhere.
    pub fn instance() -> wgpu::Instance {
        Gpu::instance_after(0).unwrap_or_else(|| {
            wgpu::Instance::new(with_compiler(wgpu::InstanceDescriptor::new_without_display_handle()))
        })
    }

    /// The backends a device is made from, best first: those `WGPU_BACKEND` names if it is set
    /// (to try another when a driver fails); on Android Vulkan, then OpenGL ES (for a device
    /// whose Vulkan is missing or fails), the other way round for a GPU whose Vulkan driver is
    /// known to crash (see [`android_backends`]); on Windows Vulkan and Direct3D 12, Direct3D 12 first on
    /// ARM (the Snapdragon X's Vulkan driver fails as a canvas opens), then OpenGL (a GPU or
    /// driver too old for either); on Linux Vulkan, then OpenGL (the same); elsewhere the native
    /// API. A backend's index here is the one the other calls take.
    pub fn backends() -> Vec<wgpu::Backends> {
        if let Some(chosen) = wgpu::Backends::from_env().filter(|b| !b.is_empty()) {
            return vec![chosen];
        }
        if cfg!(target_os = "android") {
            android_backends(vulkan_gpus())
        } else if cfg!(all(windows, target_arch = "aarch64")) {
            vec![wgpu::Backends::DX12, wgpu::Backends::VULKAN, wgpu::Backends::GL]
        } else if cfg!(windows) {
            vec![wgpu::Backends::VULKAN, wgpu::Backends::DX12, wgpu::Backends::GL]
        } else if cfg!(target_os = "linux") {
            vec![wgpu::Backends::VULKAN, wgpu::Backends::GL]
        } else {
            vec![wgpu::Backends::PRIMARY]
        }
    }

    /// How many backends there are to make a device from (see [`Gpu::instance_after`]).
    pub fn backend_count() -> usize {
        Gpu::backends().len()
    }

    /// Whether `WGPU_BACKEND` names the backends: then they are the only ones tried.
    pub fn backends_named() -> bool {
        wgpu::Backends::from_env().is_some_and(|b| !b.is_empty())
    }

    /// The GPUs `backends` sees, as each says what it is, from an instance of their own: no device
    /// is made from any (Direct3D 12 makes one of its own for each, to ask what it can do; Vulkan
    /// only asks), e.g. to choose which backend to try first.
    pub fn adapters(backends: wgpu::Backends) -> Vec<wgpu::AdapterInfo> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = backends;
        let instance = wgpu::Instance::new(with_compiler(desc));
        pollster::block_on(instance.enumerate_adapters(backends)).iter().map(wgpu::Adapter::get_info).collect()
    }

    /// Backend `index`'s name, for the log and messages (see [`Gpu::instance_after`]).
    pub fn backend_name(index: usize) -> String {
        Gpu::backends().get(index).map_or_else(|| format!("backend {index}"), |b| backends_name(*b))
    }

    /// The instance to try after `tries` failed to make a device (see [`Gpu::backends`]); None
    /// past the last.
    pub fn instance_after(tries: usize) -> Option<wgpu::Instance> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = *Gpu::backends().get(tries)?;
        Some(wgpu::Instance::new(with_compiler(desc)))
    }

    /// [`Gpu::instance_after`] for windows on `display`: OpenGL under Wayland and X11 can show
    /// nothing on a window without it, its surface failing as the window's is set up.
    pub fn instance_for_display(
        tries: usize,
        display: Box<dyn wgpu::wgt::WgpuHasDisplayHandle>,
    ) -> Option<wgpu::Instance> {
        let mut desc = wgpu::InstanceDescriptor::new_with_display_handle(display);
        desc.backends = *Gpu::backends().get(tries)?;
        Some(wgpu::Instance::new(with_compiler(desc)))
    }

    /// Sends the writes queued so far to the device and waits for them, so the copies they were
    /// staged in are let go.
    pub fn flush(&self) {
        self.queue.submit(std::iter::empty());
        let _ = self.wait();
    }

    /// Waits for everything sent to the device; an error (logged once) if the device is lost or
    /// the wait fails (see [`poll`]), never a panic.
    pub fn wait(&self) -> Result<(), GpuError> {
        if self.is_lost() {
            return Err(GpuError::Lost);
        }
        poll(&self.device, wgpu::PollType::wait_indefinitely()).map(|_| ())
    }

    /// A device for offscreen work (tests, exports), from the first instance that makes one. With
    /// `PROCRAFT_HEADLESS_OLD` set, an old GPU's (see [`Gpu::headless_old`]): to run every test there.
    pub fn headless() -> Result<Gpu, GpuError> {
        if std::env::var_os("PROCRAFT_HEADLESS_OLD").is_some() {
            return Gpu::headless_old();
        }
        if std::env::var_os("PROCRAFT_HEADLESS_DOWNLEVEL").is_some() {
            return Gpu::headless_limited(wgpu::Limits::downlevel_defaults());
        }
        Gpu::first_that(0, |_| {}, |_| true).map(|(gpu, _)| gpu)
    }

    /// [`Gpu::headless`] as an old GPU's: held to what a 2014 Mali offers on OpenGL ES 3.1
    /// ([`old_gpu_limits`]), and taken to draw into no half floats, as on one whose OpenGL ES
    /// can't. For testing what the canvas does without.
    #[doc(hidden)]
    pub fn headless_old() -> Result<Gpu, GpuError> {
        let instance = Gpu::instance_after(0).ok_or_else(|| GpuError::NoAdapter("no backend".to_owned()))?;
        Gpu::create(instance, None, true, false)
    }

    /// [`Gpu::headless`] held to `limits` (where the GPU's own are lower, to those), as a weak
    /// phone's GPU that reports less: for testing what the canvas does within them. With
    /// `PROCRAFT_HEADLESS_DOWNLEVEL` set, [`Gpu::headless`] is held to wgpu's downlevel limits.
    #[doc(hidden)]
    pub fn headless_limited(limits: wgpu::Limits) -> Result<Gpu, GpuError> {
        let instance = Gpu::instance_after(0).ok_or_else(|| GpuError::NoAdapter("no backend".to_owned()))?;
        Gpu::create_within(instance, None, Some(limits), false, false)
    }

    /// [`Gpu::headless_old`] on the system's software GPU (Direct3D 12's WARP, Vulkan's lavapipe),
    /// where it has one: a GPU as slow as an old phone's, for testing what waits on the GPU.
    #[doc(hidden)]
    pub fn headless_software_old() -> Result<Gpu, GpuError> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::PRIMARY;
        let gpu = Gpu::create(wgpu::Instance::new(with_compiler(desc)), None, true, true)?;
        if gpu.adapter.get_info().device_type != wgpu::DeviceType::Cpu {
            return Err(GpuError::NoAdapter("no software GPU".to_owned()));
        }
        Ok(gpu)
    }

    /// The app's device, the canvases' pipelines made: from the first instance that makes one
    /// and builds them without an error or losing it (a driver that fails on them gives way to
    /// the next backend; the last is kept as it is). OpenGL ES with pipelines failing on it is
    /// kept all the same where Vulkan's driver crashes (see [`opengl_kept`]).
    pub fn for_canvases() -> Result<Gpu, GpuError> {
        Gpu::for_canvases_from(0, |_| {}).map(|(gpu, _)| gpu)
    }

    /// [`Gpu::for_canvases`] from backend `from` on (round to those before it), `trying` told
    /// each backend's index before it is tried (a driver that takes the app down leaves no error
    /// to see); the device, and its backend's index.
    pub fn for_canvases_from(from: usize, trying: impl FnMut(usize)) -> Result<(Gpu, usize), GpuError> {
        Gpu::first_that(from, trying, |gpu| {
            gpu.prepare_canvases();
            let fine = gpu.errors() == 0 && !gpu.is_lost();
            if !fine {
                log::warn!("the canvases' pipelines: {} GPU errors, lost {}", gpu.errors(), gpu.is_lost());
            }
            fine || takes_opengl_with_errors(gpu.adapter.get_info().backend, gpu.is_lost(), opengl_kept())
        })
    }

    /// A device from the first instance, from backend `from` on, that makes one `fine` takes, or
    /// the last's; a backend whose driver panics making it or in `fine` fails as if it had said
    /// so.
    fn first_that(
        from: usize,
        mut trying: impl FnMut(usize),
        fine: impl Fn(&Gpu) -> bool,
    ) -> Result<(Gpu, usize), GpuError> {
        let count = Gpu::backends().len().max(1);
        let mut tries = 0;
        loop {
            let backend = (from + tries) % count;
            let Some(instance) = Gpu::instance_after(backend) else {
                return Err(GpuError::NoAdapter("no backend".to_owned()));
            };
            let last = tries + 1 >= count;
            trying(backend);
            let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Gpu::create(instance, None, false, false).map(|gpu| {
                    let taken = last || fine(&gpu);
                    (gpu, taken)
                })
            }))
            .unwrap_or_else(|panic| Err(GpuError::Device(format!("panicked: {}", panic_message(panic.as_ref())))));
            match made {
                Ok((gpu, true)) => return Ok((gpu, backend)),
                Ok(_) => log::warn!("trying the next backend"),
                Err(e) if !last => log::warn!("{e}; trying the next backend"),
                Err(e) => return Err(e),
            }
            tries += 1;
        }
    }

    /// A device that can present to `surface` (made from `instance`).
    pub fn for_surface(instance: wgpu::Instance, surface: &wgpu::Surface<'_>) -> Result<Gpu, GpuError> {
        Gpu::create(instance, Some(surface), false, false)
    }

    /// A device from `instance` (for `surface`), held to an old GPU's limits if `old`, on the
    /// software GPU if `software`.
    fn create(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        old: bool,
        software: bool,
    ) -> Result<Gpu, GpuError> {
        Gpu::create_within(instance, surface, old.then(old_gpu_limits), old, software)
    }

    /// [`Gpu::create`], held to `within` where given (a test's limits; where the GPU's own are
    /// lower, to those).
    fn create_within(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        within: Option<wgpu::Limits>,
        old: bool,
        software: bool,
    ) -> Result<Gpu, GpuError> {
        // a driver's own description can run over several lines: one line in the log
        fn one_line(text: &str) -> String {
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        // each stage timed in the log: on a slow driver, where a start stopped shows
        let started = std::time::Instant::now();
        // every GPU the backend sees, for the log (a laptop's two, a driver's own name and version)
        for adapter in pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all())) {
            let info = adapter.get_info();
            log::info!(
                "GPU seen: {} ({:?}, {:?}, driver {} {})",
                info.name,
                info.backend,
                info.device_type,
                info.driver,
                one_line(&info.driver_info)
            );
        }
        log::info!("the GPUs listed in {} ms", started.elapsed().as_millis());
        let chosen = std::time::Instant::now();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            // the faster GPU of a laptop's two, unless `WGPU_POWER_PREF` says (`low`: to try the
            // other when one fails or lags)
            power_preference: wgpu::PowerPreference::from_env().unwrap_or(wgpu::PowerPreference::HighPerformance),
            force_fallback_adapter: software,
            compatible_surface: surface,
            apply_limit_buckets: false,
        }))
        .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let info = adapter.get_info();
        log::info!(
            "GPU: {} ({:?}, driver {}), chosen in {} ms",
            info.name,
            info.backend,
            one_line(&info.driver_info),
            chosen.elapsed().as_millis()
        );
        let made = std::time::Instant::now();
        if keeps_textures_uncompressed(&info.name, info.backend) {
            UNCOMPRESSED.store(true, Ordering::Relaxed);
            log::info!("the canvas's textures kept uncompressed on this GPU");
        }
        if weak_gpu(&info.name, info.device_type) {
            log::info!("a weak GPU: the frosted glass is blurred from a smaller picture");
        }
        let stale = starts_stale(&info);
        if stale {
            log::info!("new textures start stale on this GPU: each is cleared as it is made");
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("procraft"),
            // where the GPU has it: the pipelines' compiled form to keep between runs
            required_features: adapter.features() & wgpu::Features::PIPELINE_CACHE,
            // the adapter's own limits, so large canvases fit where the GPU allows them
            required_limits:
                within.clone().map_or_else(|| adapter.limits(), |l| l.or_worse_values_from(&adapter.limits())),
            // memory taken in small blocks (a few MB up to 64): a block of each kind, once made,
            // is kept for good, and the larger ones held what was never used (a first read back
            // of a few bytes kept 64 MB the CPU can see)
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .map_err(|e| GpuError::Device(e.to_string()))?;
        log::info!("the GPU's device made in {} ms", made.elapsed().as_millis());
        // what it holds to, for a report from a GPU that fails within them
        let l = device.limits();
        log::info!(
            "the GPU's limits: textures {} px, a storage binding {} MB, a buffer {} MB, {} storage buffers a \
             stage, workgroups of {} ({} KB shared)",
            l.max_texture_dimension_2d,
            l.max_storage_buffer_binding_size >> 20,
            l.max_buffer_size >> 20,
            l.max_storage_buffers_per_shader_stage,
            l.max_compute_invocations_per_workgroup,
            l.max_compute_workgroup_storage_size >> 10
        );
        // new canvases and crops kept to what its textures can be (not by one made as an old GPU's
        // to test with: that would hold every other test's canvases to its size)
        if within.is_none() {
            procraft_engine::document::set_gpu_texture_most(device.limits().max_texture_dimension_2d);
        }
        // a validation error must never take the app down: log it instead
        let errors = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&errors);
        device.on_uncaptured_error(Arc::new(move |e| {
            counter.fetch_add(1, Ordering::Relaxed);
            log::error!("GPU error: {e}");
        }));
        // a device lost keeps nothing it held: what its textures had is gone
        let lost = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&lost);
        device.set_device_lost_callback(move |reason, message| {
            flag.store(true, Ordering::Relaxed);
            log::error!("GPU lost ({reason:?}): {message}");
        });
        let gpu = Gpu {
            instance,
            adapter,
            device,
            queue,
            errors,
            lost,
            canvas_pipelines: Arc::default(),
            kept: Arc::default(),
            readbacks: Arc::default(),
            old,
            placed: Arc::default(),
            stale: Arc::new(stale::Stale::new(stale)),
            carry: Arc::default(),
        };
        // what an old GPU can't do, and what the canvas does without it (in the shared log)
        for lack in gpu.lacks() {
            log::warn!("this GPU {lack}");
        }
        Ok(gpu)
    }

    /// A buffer of `size` bytes or more to read pixels back through: one kept from an earlier
    /// readback, or a new one. Making one the size of a layer each time took twice the read
    /// itself on a tablet.
    pub(crate) fn readback_buffer(&self, size: u64) -> wgpu::Buffer {
        let mut kept = self.readbacks.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let best =
            kept.iter().enumerate().filter(|(_, b)| b.size() >= size).min_by_key(|(_, b)| b.size()).map(|(i, _)| i);
        if let Some(i) = best {
            return kept.swap_remove(i);
        }
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Makes sure `count` readback buffers of `size` bytes or more are kept (see
    /// [`Gpu::readback_buffer`]), making those missing now: a reader about to start that many
    /// copies in a hold of whatever guards what it reads makes them before it, so the memory is
    /// got (and cleared by the system, a large buffer's slowest part) with nothing waiting on it.
    pub fn keep_readbacks(&self, size: u64, count: usize) {
        let have = {
            let kept = self.readbacks.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            kept.iter().filter(|b| b.size() >= size).count()
        };
        for _ in have..count {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.readbacks.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(buffer);
        }
    }

    /// Lets go of the readback buffers kept for next time (see [`Gpu::readback_buffer`]).
    pub fn let_go_of_kept_buffers(&self) {
        self.readbacks.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
    }

    /// Whether the device has been lost: nothing it held can be read back.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// Runs `work`, waits for what it sent the GPU, and says whether any of it went wrong: a
    /// texture or buffer there was no memory for (made all the same, but holding nothing), work
    /// that used one (left undone: a copy into it copies nothing), the device lost meanwhile, or
    /// the work panicking (as wgpu does on a lost device). What `work` made must then not take
    /// the place of what was there. Only what this thread does is looked at.
    pub fn checked<T>(&self, work: impl FnOnce() -> T) -> Result<T, GpuError> {
        if self.is_lost() {
            return Err(GpuError::Lost);
        }
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let invalid = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        // done on the GPU, so a device lost doing it shows
        self.flush();
        let invalid = pollster::block_on(invalid.pop());
        let memory = pollster::block_on(memory.pop());
        if self.is_lost() {
            return Err(GpuError::Lost);
        }
        if let Some(e) = memory {
            return Err(GpuError::OutOfMemory(e.to_string()));
        }
        if let Some(e) = invalid {
            return Err(GpuError::Failed(e.to_string()));
        }
        done.map_err(|panic| GpuError::Failed(format!("panicked: {}", panic_message(panic.as_ref()))))
    }

    /// Makes the pipelines canvases draw with, and the brush images they hold, now, so the first
    /// canvas on this device is made at once (slow: off the main thread).
    pub fn prepare_canvases(&self) {
        let started = std::time::Instant::now();
        let _ = self.canvas_pipelines();
        self.keep_pipelines();
        let pipelines = started.elapsed().as_millis();
        procraft_engine::textures::prepare();
        log::info!(
            "canvases prepared: their pipelines in {pipelines} ms, the brushes' shapes and grains in {} ms",
            started.elapsed().as_millis().saturating_sub(pipelines)
        );
    }

    /// The pipelines canvases draw with, made by the first canvas on this device: from their
    /// compiled form kept by an earlier run, where there is one (see [`keep_pipelines_in`]).
    pub(crate) fn canvas_pipelines(&self) -> Arc<canvas::Pipelines> {
        Arc::clone(self.canvas_pipelines.get_or_init(|| {
            let kept = self
                .kept
                .get_or_init(|| kept::folder().and_then(|dir| kept::Kept::open(&self.device, &self.adapter, &dir)));
            let abilities = canvas::Abilities::of(self);
            let pipelines = canvas::Pipelines::new(&self.device, abilities, kept.as_ref().map(|k| k.cache.clone()));
            if let Some(kept) = kept {
                kept.made();
            }
            Arc::new(pipelines)
        }))
    }

    /// Keeps the pipelines made so far for the next run, where they are kept (see
    /// [`keep_pipelines_in`]): those a window's first frame made as well. Writes a file: off the
    /// main thread.
    pub fn keep_pipelines(&self) {
        if let Some(Some(kept)) = self.kept.get() {
            kept.save();
        }
    }

    /// GPU validation errors so far (each one is logged); tests expect none.
    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn max_texture_size(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    /// The fewest layers a texture viewed as an array has: two on OpenGL (ES), which makes a
    /// texture of one a plain 2D one that an array view reads nothing from; else one.
    pub fn array_layers_least(&self) -> u32 {
        if self.is_gl() { 2 } else { 1 }
    }

    /// The layers a texture viewed as an array of `n` is made with: on OpenGL (ES) never a
    /// multiple of six (see [`gl_array_layers`]); else `n`.
    pub fn array_layers(&self, n: u32) -> u32 {
        if self.is_gl() { gl_array_layers(n) } else { n.max(1) }
    }

    /// Whether the GPU is a weak one (see [`weak_gpu`]).
    pub fn is_weak(&self) -> bool {
        let info = self.adapter.get_info();
        WEAK.load(Ordering::Relaxed) || weak_gpu(&info.name, info.device_type)
    }

    /// Whether the GPU's memory is the computer's own: an integrated GPU's (a laptop's Intel or
    /// AMD graphics, a Mac's) or the system's software one. What it holds, the system and every
    /// other app go without, and its share of it is smaller than a graphics card's own memory.
    pub fn shares_memory(&self) -> bool {
        matches!(self.adapter.get_info().device_type, wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::Cpu)
    }

    /// Whether the device is OpenGL (ES)'s.
    pub fn is_gl(&self) -> bool {
        self.adapter.get_info().backend == wgpu::Backend::Gl
    }

    /// Whether the GPU draws into half floats, blended and filtered: strokes build up in them, and
    /// liquify's field and 3D painting's lighting are drawn in them. An old GPU's OpenGL ES
    /// without `EXT_color_buffer_half_float` (or `_float`) can only read them.
    pub fn draws_half_floats(&self) -> bool {
        if self.old {
            return false;
        }
        let features = self.adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
        let usages = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
        let flags = wgpu::TextureFormatFeatureFlags::FILTERABLE | wgpu::TextureFormatFeatureFlags::BLENDABLE;
        features.allowed_usages.contains(usages) && features.flags.contains(flags)
    }

    /// Whether the GPU runs the compute passes that work in storage buffers (smudging, wet paint,
    /// paint laid in place, stamps laid by their stroke's rule, 3D painting's footprint): four
    /// storage buffers a stage and groups of 256. An old GPU's OpenGL ES 3.1 often has none
    /// outside its compute shaders, which wgpu takes for every stage.
    pub fn computes(&self) -> bool {
        let limits = self.device.limits();
        self.adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
            && limits.max_storage_buffers_per_shader_stage >= 4
            && limits.max_compute_invocations_per_workgroup >= 256
            && limits.max_compute_workgroup_size_x >= 64
            && limits.max_compute_workgroup_size_y >= 16
    }

    /// Whether the GPU draws into `format` (without blending).
    pub(crate) fn draws(&self, format: wgpu::TextureFormat) -> bool {
        let usages = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
        !self.old && self.adapter.get_texture_format_features(format).allowed_usages.contains(usages)
    }

    /// What the GPU lacks that others have, each with what the canvas does without it, worded
    /// for the log (after "this GPU"); none on most.
    pub fn lacks(&self) -> Vec<&'static str> {
        let mut lacks = Vec::new();
        if !self.draws_half_floats() {
            lacks.push(
                "can't draw into half floats: strokes build up in half floats kept as integers beside them, and \
                 Liquify's field is kept in integers, 3D painting's distances and light in 8 bits a channel",
            );
        }
        if !self.computes() {
            lacks.push(
                "has no compute passes with storage buffers: strokes, smudges and wet paint are laid in fragment \
                 passes instead",
            );
        }
        lacks
    }
}

#[cfg(test)]
mod tests {
    /// A GPU or driver too old for the rest still draws: without OpenGL behind them the app can't
    /// open at all. (First, on Android, for a GPU whose Vulkan driver is known to crash.)
    #[test]
    #[cfg(any(target_os = "linux", windows, target_os = "android"))]
    fn opengl_is_the_last_backend_tried() {
        if std::env::var_os("WGPU_BACKEND").is_none() {
            let backends = super::Gpu::backends();
            let first = cfg!(target_os = "android") && super::vulkan_gpus().iter().any(|n| super::crashes_on_vulkan(n));
            assert_eq!(if first { backends.first() } else { backends.last() }, Some(&wgpu::Backends::GL));
        }
    }

    /// Old PowerVR Rogue GPUs start on OpenGL ES: their Vulkan driver crashed the app in the
    /// queue's submit ("PowerVR Rogue GE8300", Vulkan driver 1.15, five starts in a row). Others,
    /// newer PowerVRs among them, start on Vulkan.
    #[test]
    fn old_powervr_rogue_gpus_start_on_opengl_es() {
        use super::android_backends;
        use wgpu::Backends;
        let names = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        for rogue in [
            "PowerVR Rogue GE8300",
            "PowerVR Rogue GE8320",
            "PowerVR Rogue GE8322",
            "PowerVR Rogue GE8100",
            "PowerVR Rogue GM9446",
            "PowerVR Rogue G6200",
        ] {
            assert_eq!(android_backends(&names(&[rogue])), [Backends::GL, Backends::VULKAN], "{rogue}");
        }
        for other in [
            "PowerVR B-Series BXM-8-256",
            "Adreno (TM) 750",
            "Adreno (TM) 610",
            "Mali-G57 MC2",
            "Mali-G52 MC2",
            "Samsung Xclipse 940",
        ] {
            assert_eq!(android_backends(&names(&[other])), [Backends::VULKAN, Backends::GL], "{other}");
        }
        // Vulkan seeing no GPU at all: Vulkan first still (it fails soft, and OpenGL ES follows)
        assert_eq!(android_backends(&[]), [Backends::VULKAN, Backends::GL]);
    }

    /// Where Vulkan's driver crashes, OpenGL ES with some of the canvases' pipelines failing is
    /// kept, never given up for Vulkan (the tester's GE8300 went back to Vulkan over three smudge
    /// pipelines and crashed); anywhere else, or lost, or on another backend, it gives way.
    #[test]
    fn opengl_es_with_failing_pipelines_is_kept_where_vulkan_crashes() {
        use super::takes_opengl_with_errors;
        use wgpu::Backend::{Dx12, Gl, Vulkan};
        assert!(takes_opengl_with_errors(Gl, false, true));
        assert!(!takes_opengl_with_errors(Gl, false, false));
        assert!(!takes_opengl_with_errors(Gl, true, true));
        assert!(!takes_opengl_with_errors(Vulkan, false, true));
        assert!(!takes_opengl_with_errors(Dx12, false, true));
        // the app's record saying so is enough (no GPU on the known list on the PC)
        super::vulkan_crashed(true);
        assert!(super::opengl_kept());
        super::vulkan_crashed(false);
        assert_eq!(
            super::opengl_kept(),
            cfg!(target_os = "android") && super::vulkan_gpus().iter().any(|n| super::crashes_on_vulkan(n))
        );
    }

    /// Every wait on the GPU outside the tests goes through [`super::poll`], which fails soft where
    /// wgpu's own poll panics (a device lost under it, as a phone short of memory can lose it):
    /// the canvas's looks at how fast the GPU got through paint laid in place, or laid a stroke
    /// again, and a sample stroke's paced waits, polled it themselves (#358).
    #[test]
    fn every_wait_on_the_gpu_outside_the_tests_fails_soft() {
        let sources = [
            ("canvas.rs", include_str!("canvas.rs")),
            ("canvas/fragments.rs", include_str!("canvas/fragments.rs")),
            ("canvas/plugin_filter.rs", include_str!("canvas/plugin_filter.rs")),
            ("canvas/plugin_picture.rs", include_str!("canvas/plugin_picture.rs")),
            ("canvas/rewind.rs", include_str!("canvas/rewind.rs")),
            ("canvas/smudge.rs", include_str!("canvas/smudge.rs")),
            ("canvas/store.rs", include_str!("canvas/store.rs")),
            ("kept.rs", include_str!("kept.rs")),
            ("lighting.rs", include_str!("lighting.rs")),
            ("scene.rs", include_str!("scene.rs")),
            ("sphere.rs", include_str!("sphere.rs")),
        ];
        for (name, text) in sources {
            // (the code before the file's tests)
            let code = text.split("\nmod tests {").next().unwrap_or(text);
            let raw = code.lines().filter(|l| l.contains("device.poll(") && !l.trim_start().starts_with("//")).count();
            assert_eq!(raw, 0, "{name} polls the device itself");
        }
    }

    /// A poll panicking, as wgpu's does on a device lost or out of memory, is the device lost; one
    /// that fails or runs out of time is an error; neither goes on as a panic.
    #[test]
    fn a_poll_that_panics_or_fails_is_an_error() {
        use super::{GpuError, polled};
        assert!(matches!(polled(|| Ok(wgpu::PollStatus::QueueEmpty)), Ok(wgpu::PollStatus::QueueEmpty)));
        assert!(matches!(polled(|| panic!("Error in Device::poll: Parent device is lost")), Err(GpuError::Lost)));
        assert!(matches!(polled(|| Err(wgpu::PollError::Timeout)), Err(GpuError::Failed(_))));
        assert!(matches!(polled(|| Err(wgpu::PollError::WrongSubmissionIndex(2, 1))), Err(GpuError::Failed(_))));
        // a device destroyed: waited on, it is lost, then refused at once
        let Ok(gpu) = super::Gpu::headless() else { return eprintln!("skipped: no GPU") };
        gpu.device.destroy();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gpu.wait()));
        assert!(gpu.is_lost());
        assert!(matches!(gpu.wait(), Err(GpuError::Lost)));
        gpu.flush();
    }

    #[test]
    fn array_textures_on_opengl_are_never_taken_for_cube_maps() {
        use super::gl_array_layers;
        assert_eq!([1, 2, 5, 6, 7, 12, 63, 66, 67].map(gl_array_layers), [2, 2, 5, 7, 7, 13, 63, 67, 67]);
    }

    #[test]
    fn backends_are_named_for_the_log() {
        use wgpu::Backends;
        assert_eq!(super::backends_name(Backends::DX12), "Direct3D 12");
        assert_eq!(super::backends_name(Backends::VULKAN | Backends::GL), "Vulkan or OpenGL");
        assert_eq!(super::Gpu::backend_name(99), "backend 99");
    }

    #[test]
    fn budget_gpus_are_told_from_their_names() {
        use super::weak_gpu;
        use wgpu::DeviceType::{Cpu, DiscreteGpu, IntegratedGpu};
        for weak in [
            "PowerVR Rogue GE8320",
            "PowerVR B-Series BXM-8-256",
            "Mali-G52 MC2",
            "Mali-G57 MC2",
            "Mali-G53 MC2",
            "Mali-G310 MC1",
            "Mali-T830 MP2",
            "Adreno (TM) 610",
            "Adreno (TM) 619",
            "Adreno (TM) 506",
            "Adreno (TM) 702",
        ] {
            assert!(weak_gpu(weak, IntegratedGpu), "{weak}");
        }
        for strong in [
            "Mali-G68 MC4",
            "Mali-G610 MC6",
            "Mali-G78 MP14",
            "Adreno (TM) 642L",
            "Adreno (TM) 710",
            "Adreno (TM) 750",
            "Adreno (TM) 830",
            "Samsung Xclipse 940",
            "Intel(R) Graphics",
            "NVIDIA GeForce RTX 5060 Laptop GPU",
            "Apple M2",
        ] {
            assert!(!weak_gpu(strong, IntegratedGpu), "{strong}");
        }
        assert!(!weak_gpu("NVIDIA GeForce RTX 5060 Laptop GPU", DiscreteGpu));
        assert!(weak_gpu("Microsoft Basic Render Driver", Cpu));
    }

    #[test]
    fn mali_gpus_on_vulkan_keep_their_canvas_textures_uncompressed() {
        use wgpu::Backend;
        assert!(super::keeps_textures_uncompressed("Mali-G68 MC4", Backend::Vulkan));
        assert!(!super::keeps_textures_uncompressed("Mali-G68 MC4", Backend::Gl));
        assert!(!super::keeps_textures_uncompressed("Adreno (TM) 750", Backend::Vulkan));
    }

    use super::*;

    /// A wait goes in slices (see [`WAIT_SLICE`]), each letting OpenGL's one context go, and one
    /// longer than a slice still waits for everything sent: a picture copied back and forth over
    /// and over is done, and the device has nothing left to do. Another thread's call into the
    /// GPU meanwhile is timed (on OpenGL one kept from the context a second panics in wgpu).
    /// `LONG_WAIT_COPIES` (600) says how much work the GPU is given.
    #[test]
    fn a_wait_longer_than_its_slices_still_waits_for_everything_sent() {
        use std::time::{Duration, Instant};
        let Ok(gpu) = Gpu::headless() else { return };
        let copies: usize = std::env::var("LONG_WAIT_COPIES").ok().and_then(|v| v.parse().ok()).unwrap_or(600);
        let size = wgpu::Extent3d { width: 4096, height: 4096, depth_or_array_layers: 1 };
        let texture = |label| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let (a, b) = (texture("a"), texture("b"));
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for _ in 0..copies {
            encoder.copy_texture_to_texture(a.as_image_copy(), b.as_image_copy(), size);
            encoder.copy_texture_to_texture(b.as_image_copy(), a.as_image_copy(), size);
        }
        let sent = Instant::now();
        gpu.queue.submit([encoder.finish()]);
        let submitted = sent.elapsed();
        let waiting = {
            let gpu = gpu.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                let done = gpu.wait();
                (started.elapsed(), done.is_ok())
            })
        };
        let mut longest = Duration::ZERO;
        while !waiting.is_finished() {
            let started = Instant::now();
            let _ = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("meanwhile"),
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            longest = longest.max(started.elapsed());
            std::thread::sleep(Duration::from_millis(5));
        }
        let (waited, done) = waiting.join().unwrap();
        println!(
            "{}: sent in {} ms, waited {} ms; a call meanwhile took {} ms at most",
            gpu.adapter.get_info().name,
            submitted.as_millis(),
            waited.as_millis(),
            longest.as_millis()
        );
        assert!(done);
        // nothing left: the wait didn't end with a slice
        let left = gpu.device.poll(wgpu::PollType::Poll).unwrap();
        assert!(left.is_queue_empty(), "{left:?} after the wait");
        assert_eq!(gpu.errors(), 0);
    }

    #[test]
    fn work_that_fails_on_the_gpu_is_told() {
        let gpu = match Gpu::headless() {
            Ok(g) => g,
            Err(e) => return eprintln!("skipped: {e}"),
        };
        assert_eq!(gpu.checked(|| 7).ok(), Some(7));
        // a texture it won't make, and a panic on the way
        let most = gpu.device.limits().max_texture_dimension_2d;
        let made = gpu.checked(|| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("too large"),
                size: wgpu::Extent3d { width: most + 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });
        assert!(matches!(made, Err(GpuError::Failed(_))));
        assert!(matches!(gpu.checked::<()>(|| panic!("a wrapper panicking")), Err(GpuError::Failed(_))));
        // nothing goes past it to the errors counted
        assert_eq!(gpu.errors(), 0);
        // and a device lost
        gpu.device.destroy();
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        assert!(matches!(gpu.checked(|| 7), Err(GpuError::Lost)));
    }

    /// A device made as an old GPU's is held to what one offers, says what it lacks, and its
    /// canvases' pipelines are made without asking for any of it.
    #[test]
    fn an_old_gpu_is_held_to_its_limits_and_says_what_it_lacks() {
        let gpu = match Gpu::headless_old() {
            Ok(g) => g,
            Err(e) => return eprintln!("skipped: {e}"),
        };
        assert_eq!(gpu.max_texture_size(), 4096);
        assert_eq!(gpu.device.limits().max_storage_buffers_per_shader_stage, 0);
        assert!(!gpu.draws_half_floats() && !gpu.computes());
        assert_eq!(gpu.lacks().len(), 2);
        gpu.prepare_canvases();
        assert_eq!(gpu.errors(), 0);
    }

    #[test]
    fn canvases_on_one_device_share_their_pipelines() {
        let gpu = match Gpu::headless() {
            Ok(g) => g,
            Err(e) => return eprintln!("skipped: {e}"),
        };
        let first = gpu.canvas_pipelines();
        // the same once made, and for a clone of the device too
        assert!(Arc::ptr_eq(&first, &gpu.canvas_pipelines()));
        assert!(Arc::ptr_eq(&first, &gpu.clone().canvas_pipelines()));
        // another device makes its own
        if let Ok(other) = Gpu::headless() {
            assert!(!Arc::ptr_eq(&first, &other.canvas_pipelines()));
        }
    }

    #[test]
    fn a_backend_whose_driver_panics_gives_way_to_the_next() {
        let count = Gpu::backend_count();
        let mut tried = Vec::new();
        // every backend but the last panics as its device is checked; the last is kept as it is
        let made = Gpu::first_that(1, |b| tried.push(b), |_| panic!("a driver's wrapper panicking"));
        // tried from the one asked for, round to those before it
        let order: Vec<usize> = (0..count).map(|t| (1 + t) % count).collect();
        assert!(order.starts_with(&tried), "{tried:?} of {order:?}");
        match made {
            Ok((_, backend)) => assert_eq!(Some(&backend), tried.last()),
            Err(e) => eprintln!("skipped: {e}"),
        }
    }
}
