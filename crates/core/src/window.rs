//! The window handed over from Kotlin, the wgpu surface on it, and the screen's refreshes as the
//! system tells them: the crate's only unsafe code.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use jni_sys::{JNIEnv, jclass, jlong, jobject};
use ndk::native_window::NativeWindow;
use procraft_gpu::wgpu;
use raw_window_handle::{AndroidDisplayHandle, AndroidNdkWindowHandle, RawDisplayHandle, RawWindowHandle};

use crate::pacing::Timeline;

/// Windows acquired from Kotlin and not yet attached, by id.
static WINDOWS: Mutex<Vec<(u64, NativeWindow)>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Takes the window registered as `id`.
pub(crate) fn take(id: u64) -> Option<NativeWindow> {
    let mut windows = WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
    let at = windows.iter().position(|(i, _)| *i == id)?;
    Some(windows.swap_remove(at).1)
}

/// `NativeWindows.acquire(surface: Surface): Long`: registers the surface's native window and
/// returns its id (0 if there is none).
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_artcraftandroid_procraft_canvas_NativeWindows_acquire(
    env: *mut JNIEnv,
    _class: jclass,
    surface: jobject,
) -> jlong {
    if env.is_null() || surface.is_null() {
        return 0;
    }
    // SAFETY: the JVM calls this with its own valid `env`, and the Kotlin declaration only takes an
    // `android.view.Surface`.
    let Some(window) = (unsafe { NativeWindow::from_surface(env, surface) }) else { return 0 };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).push((id, window));
    jlong::try_from(id).unwrap_or(0)
}

/// `NativeWindows.release(id: Long)`: lets go of a window that was never attached.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_artcraftandroid_procraft_canvas_NativeWindows_release(
    _env: *mut JNIEnv,
    _class: jclass,
    id: jlong,
) {
    if let Ok(id) = u64::try_from(id) {
        drop(take(id));
    }
}

/// A wgpu surface on `window`. The caller keeps `window` alive until the surface is dropped.
pub(crate) fn create_surface(
    instance: &wgpu::Instance,
    window: &NativeWindow,
) -> Result<wgpu::Surface<'static>, String> {
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(RawDisplayHandle::Android(AndroidDisplayHandle::new())),
        raw_window_handle: RawWindowHandle::AndroidNdk(AndroidNdkWindowHandle::new(window.ptr().cast::<c_void>())),
    };
    // SAFETY: the handle is a live ANativeWindow, and the render thread, which owns both, drops
    // the surface before the window.
    unsafe { instance.create_surface_unsafe(target) }.map_err(|e| e.to_string())
}

/// How long after a frame was last wanted the screen's refreshes are still asked for: a pause
/// between strokes keeps them told, an idle canvas lets them go (each wakes a thread). Asked for
/// again, the first frame is drawn at once and the next waits up to a refresh for them (see
/// `pacing`).
const KEEP_ASKING: Duration = Duration::from_secs(2);

/// A choreographer's callback for a refresh: its frame data, and the data it was asked with.
type OnRefresh = unsafe extern "C" fn(*const c_void, *mut c_void);

/// Android's choreographer, as far as it tells the compositor's deadlines (Android 13 and later):
/// looked up as the app runs, which it does from Android 8.
#[derive(Clone, Copy)]
struct Choreographer {
    instance: unsafe extern "C" fn() -> *mut c_void,
    post: unsafe extern "C" fn(*mut c_void, Option<OnRefresh>, *mut c_void),
    timelines: unsafe extern "C" fn(*const c_void) -> usize,
    preferred: unsafe extern "C" fn(*const c_void) -> usize,
    deadline: unsafe extern "C" fn(*const c_void, usize) -> i64,
    shows: unsafe extern "C" fn(*const c_void, usize) -> i64,
}

static CHOREOGRAPHER: OnceLock<Option<Choreographer>> = OnceLock::new();

fn choreographer() -> Option<Choreographer> {
    *CHOREOGRAPHER.get_or_init(|| {
        // SAFETY: the system's library, loaded with the app (and kept loaded by the handle), and
        // NUL-terminated names; each symbol, where the system has it, is the function its NDK
        // header declares, of the type it is taken as.
        unsafe {
            let library = libc::dlopen(c"libandroid.so".as_ptr(), libc::RTLD_NOW);
            if library.is_null() {
                return None;
            }
            let symbol = |name: &std::ffi::CStr| {
                let at = libc::dlsym(library, name.as_ptr());
                (!at.is_null()).then_some(at)
            };
            Some(Choreographer {
                instance: std::mem::transmute::<*mut c_void, unsafe extern "C" fn() -> *mut c_void>(symbol(
                    c"AChoreographer_getInstance",
                )?),
                post: std::mem::transmute::<
                    *mut c_void,
                    unsafe extern "C" fn(*mut c_void, Option<OnRefresh>, *mut c_void),
                >(symbol(c"AChoreographer_postVsyncCallback")?),
                timelines: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const c_void) -> usize>(symbol(
                    c"AChoreographerFrameCallbackData_getFrameTimelinesLength",
                )?),
                preferred: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const c_void) -> usize>(symbol(
                    c"AChoreographerFrameCallbackData_getPreferredFrameTimelineIndex",
                )?),
                deadline: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const c_void, usize) -> i64>(
                    symbol(c"AChoreographerFrameCallbackData_getFrameTimelineDeadlineNanos")?,
                ),
                shows: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const c_void, usize) -> i64>(symbol(
                    c"AChoreographerFrameCallbackData_getFrameTimelineExpectedPresentationTimeNanos",
                )?),
            })
        }
    })
}

/// The screen's refreshes as Android's choreographer tells them (Android 13 and later; see
/// [`crate::pacing`]): asked for on a thread of its own, with a looper, while frames are drawn.
pub(crate) struct Refreshes {
    told: Arc<Told>,
    looper: Option<ndk::looper::ForeignLooper>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// What the refreshes' thread shares: the timeline last told, until when it is asked for, whether
/// a refresh is asked for and not yet told, and its stop.
struct Told {
    choreographer: Choreographer,
    timeline: parking_lot::Mutex<Option<Timeline>>,
    wanted_until: parking_lot::Mutex<Option<Instant>>,
    asked: AtomicBool,
    stop: AtomicBool,
}

impl Told {
    fn wanted(&self) -> bool {
        self.wanted_until.lock().is_some_and(|until| Instant::now() < until)
    }
}

impl Refreshes {
    /// Starts asking for them, where the system tells them; none where it doesn't.
    pub(crate) fn start() -> Option<Refreshes> {
        let choreographer = choreographer()?;
        let told = Arc::new(Told {
            choreographer,
            timeline: parking_lot::Mutex::new(None),
            // (told from the first, so the first stroke's frames are timed)
            wanted_until: parking_lot::Mutex::new(Some(Instant::now() + KEEP_ASKING)),
            asked: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        });
        let (ready, looper) = std::sync::mpsc::channel();
        let theirs = Arc::clone(&told);
        let thread =
            std::thread::Builder::new().name("procraft-refreshes".to_owned()).spawn(move || listen(&theirs, &ready));
        let thread = match thread {
            Ok(t) => t,
            Err(e) => {
                log::warn!("the screen's refreshes: {e}");
                return None;
            }
        };
        let mut refreshes = Refreshes { told, looper: None, thread: Some(thread) };
        // (dropped without one, its thread is stopped and waited for)
        refreshes.looper = Some(looper.recv_timeout(Duration::from_secs(2)).ok()?);
        Some(refreshes)
    }

    /// The refreshes as last told. `drawing`: a frame is wanted, so they are asked for (again).
    pub(crate) fn timeline(&self, drawing: bool) -> Option<Timeline> {
        if drawing {
            *self.told.wanted_until.lock() = Some(Instant::now() + KEEP_ASKING);
            if !self.told.asked.load(Ordering::Acquire)
                && let Some(looper) = &self.looper
            {
                looper.wake();
            }
        }
        *self.told.timeline.lock()
    }
}

impl Drop for Refreshes {
    fn drop(&mut self) {
        self.told.stop.store(true, Ordering::Release);
        if let Some(looper) = &self.looper {
            looper.wake();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The refreshes' thread: a looper, and the choreographer asked for the next refresh while frames
/// are wanted, until stopped.
fn listen(told: &Arc<Told>, ready: &std::sync::mpsc::Sender<ndk::looper::ForeignLooper>) {
    let looper = ndk::looper::ThreadLooper::prepare();
    if ready.send(looper.as_foreign().clone()).is_err() {
        return;
    }
    // SAFETY: on a thread with a looper, as the choreographer needs
    let instance = unsafe { (told.choreographer.instance)() };
    if instance.is_null() {
        log::warn!("the screen's refreshes: no choreographer");
        return;
    }
    // (the callback runs on this thread, in its polls below, while `told` is held here)
    let data = Arc::as_ptr(told).cast_mut().cast::<c_void>();
    while !told.stop.load(Ordering::Acquire) {
        if told.wanted() && !told.asked.swap(true, Ordering::AcqRel) {
            // SAFETY: this thread's choreographer, and a callback that only reads `data` while
            // this thread holds it
            unsafe { (told.choreographer.post)(instance, Some(on_refresh), data) };
        }
        // until a refresh is told, or something wakes it
        if looper.poll_once().is_err() {
            log::warn!("the screen's refreshes: the looper failed");
            return;
        }
    }
}

/// A refresh told: the compositor's deadline for the frame shown at the refresh the choreographer
/// prefers, and the time between refreshes.
unsafe extern "C" fn on_refresh(frame: *const c_void, data: *mut c_void) {
    if frame.is_null() || data.is_null() {
        return;
    }
    // SAFETY: `data` is the `Told` its thread holds as it polls, which is when this is called
    let told = unsafe { &*data.cast::<Told>() };
    told.asked.store(false, Ordering::Release);
    let c = told.choreographer;
    let (now, at) = (Instant::now(), monotonic_nanos());
    // SAFETY: the system's frame data, as it hands it to this call
    let (count, preferred) = unsafe { ((c.timelines)(frame), (c.preferred)(frame)) };
    if count < 2 || preferred >= count {
        return;
    }
    // SAFETY: as above, at indices it has
    let (deadline, first, second) =
        unsafe { ((c.deadline)(frame, preferred), (c.shows)(frame, 0), (c.shows)(frame, 1)) };
    let (Some(at), Ok(period)) = (at, u64::try_from(second.saturating_sub(first))) else { return };
    let latch = if deadline >= at {
        now.checked_add(Duration::from_nanos(u64::try_from(deadline - at).unwrap_or(0)))
    } else {
        now.checked_sub(Duration::from_nanos(u64::try_from(at - deadline).unwrap_or(0)))
    };
    if let Some(latch) = latch {
        *told.timeline.lock() = Some(Timeline { latch, period: Duration::from_nanos(period), told: now });
    }
}

/// The monotonic clock the choreographer's times are on (nanoseconds).
// (its seconds and nanoseconds are 32-bit on 32-bit Android, 64-bit here)
#[allow(clippy::useless_conversion)]
fn monotonic_nanos() -> Option<i64> {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a timespec of its own to fill
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) } != 0 {
        return None;
    }
    i64::from(now.tv_sec).checked_mul(1_000_000_000)?.checked_add(i64::from(now.tv_nsec))
}
