//! Cropping and resizing the canvas. The canvas is made again at its new size with every layer
//! resampled; the canvas from before is kept, history and all, and undoing past the new canvas's
//! first step puts it back.

use procraft_engine::crop::{crop, resample};
use procraft_engine::view::View;
use procraft_gpu::{Canvas, Gpu};

use crate::layers::CANVAS_LAYERS;
use crate::{CanvasSession, State};

/// How long the canvas takes to shrink into place as crop begins.
const CROP_SHRINK_SECONDS: f32 = 0.3;

/// What the apps tell when the GPU's device is lost (see [`CanvasSession::take_failure`]).
const LOST_SAID: &str =
    "The graphics stopped working. Go back to the gallery and open the artwork again: it comes back as last saved.";

/// What the apps tell when a smudge stroke couldn't be begun, or couldn't go on.
const SMUDGE_SAID: &str = "There isn't enough graphics memory to smudge on this canvas.";

/// What the apps tell when a crop couldn't be made.
fn crop_failure_said(e: &procraft_gpu::GpuError) -> String {
    match e {
        procraft_gpu::GpuError::Lost => LOST_SAID,
        _ => "The canvas couldn't be cropped, most likely for lack of graphics memory. Nothing was changed.",
    }
    .to_owned()
}

/// How a crop's new canvas takes the layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Across {
    /// Copied (moved by whole pixels) or resampled on the GPU, as many as its textures' budget
    /// holds; the rest worked out on the CPU and kept compressed.
    Gpu,
    /// Every one read back, worked out on the CPU and kept compressed until needed: no GPU work
    /// of the crop's own, and no layer's texture made at once.
    Compressed,
}

/// The canvas cropped as `c` says from `old` (`sw` × `sh`), its layers `ids` taken `across` so;
/// `doc` is the document cropped, `budget` its textures' budget.
#[allow(clippy::too_many_arguments)]
fn cropped(
    gpu: &Gpu,
    old: &Canvas,
    doc: &procraft_engine::document::Document,
    ids: &[procraft_engine::document::LayerId],
    c: &procraft_engine::crop::Crop,
    (sw, sh): (u32, u32),
    budget: u64,
    across: Across,
) -> Result<Box<Canvas>, procraft_gpu::GpuError> {
    let mut canvas = Box::new(Canvas::unfilled(gpu, doc)?);
    canvas.set_resident_budget(budget);
    for &id in ids {
        let m = c.to_old;
        let room = across == Across::Gpu && canvas.has_room();
        let on_gpu = room
            && match c.whole_pixels() {
                Some(offset) => canvas.copy_layer_from(gpu, old, id, offset),
                None => canvas.resample_layer_from(gpu, old, id, [m.a, m.b, m.c, m.d, m.e, m.f]),
            };
        if !on_gpu {
            let pixels = resample(&old.read_layer(gpu, id)?, sw, sh, c);
            // past what fits, kept compressed until needed
            if room {
                canvas.upload_layer(gpu, id, &pixels)?;
            } else {
                canvas.park_pixels(id, &pixels)?;
            }
        }
    }
    canvas.fill_rest(gpu, doc);
    #[cfg(test)]
    tests::fail_here(gpu, across);
    Ok(canvas)
}

impl State {
    /// Whether redoing would put an undone crop back: only while nothing new was done since.
    pub(crate) fn crop_redo_ready(&self) -> bool {
        match (&self.crop_redo, &self.canvas) {
            (Some((_, _, steps)), Some(c)) => !c.can_redo() && c.steps_made() == *steps,
            _ => false,
        }
    }
}

#[uniffi::export]
impl CanvasSession {
    /// Crops and resizes the canvas, as one undo step: keeps the rectangle `x, y, width, height`
    /// (canvas pixels; it may reach past the edges) turned by `angle` radians about its middle.
    /// With `resample`, what is kept is scaled to `new_width` × `new_height` pixels; otherwise the
    /// canvas is the rectangle's size. False if the canvas isn't shown yet, or if the GPU couldn't
    /// make the new canvas (out of memory, its device lost): the canvas is then left as it was,
    /// and [`CanvasSession::take_failure`] says why.
    #[allow(clippy::too_many_arguments)]
    pub fn crop_canvas(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        angle: f32,
        resample_to: bool,
        new_width: u32,
        new_height: u32,
    ) -> bool {
        self.text_end(true);
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        if st.canvas.is_none() || st.gpu.is_none() {
            return false;
        }
        let (sw, sh) = (st.doc.width(), st.doc.height());
        let c = crop([x, y, width, height], angle, resample_to.then_some((new_width, new_height)));
        let mut doc = st.doc.cropped(&c);
        // plugins' layers' items moved with their pixels (those whose plugin can't move them become
        // ordinary layers); undoing the crop brings back the document as it was
        if let Some(to_new) = c.to_old.inverse() {
            st.move_all_items(to_new, &mut doc);
        } else {
            let plugin_layers: Vec<_> = doc.layers().iter().filter(|l| l.plugin.is_some()).map(|l| l.id).collect();
            for id in plugin_layers {
                doc.rasterize(id);
            }
        }
        let (Some(old), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) else { return false };
        let ids: Vec<_> = st.doc.pixel_layer_ids().collect();
        // the canvas as it is, drawn up to date first and outside what is checked below: a fault
        // in drawing it (one a weak GPU's limits make, logged and lived with as ever) isn't the
        // crop's to fail on
        old.render(gpu, &mut st.doc, None);
        #[cfg(test)]
        tests::fault_drawing(gpu);
        // the old canvas stays for undo, so this is all more: refused while there is still room,
        // not left to the system to find out. The layers that fit go across on the GPU; where
        // that fails, or the memory free can't hold them twice, each layer is worked out on the
        // CPU and kept compressed until needed
        let budget = old.resident_budget();
        let per = u64::from(c.width) * u64::from(c.height) * 4;
        let resident = ids.len().min(procraft_gpu::most_resident(budget, c.width, c.height)) as u64;
        let short = |layers: u64| crate::device::short_of_memory((layers + CANVAS_LAYERS) * per);
        let ways: Vec<Across> = [(Across::Gpu, !short(resident)), (Across::Compressed, !short(0))]
            .into_iter()
            .filter(|w| w.1)
            .map(|w| w.0)
            .collect();
        if ways.is_empty() {
            log::error!("cropping: {} layers of {per} bytes won't fit in the memory free", resident);
            st.crop_edit.failure = Some(crop_failure_said(&procraft_gpu::GpuError::OutOfMemory(String::new())));
            return false;
        }
        // the new canvas made beside the old, which stays as it is unless every layer went across
        // as it should: a texture the GPU had no memory for would otherwise come over empty
        let mut made = Err(procraft_gpu::GpuError::Failed("not tried".to_owned()));
        for way in ways {
            made = gpu.checked(|| cropped(gpu, old, &doc, &ids, &c, (sw, sh), budget, way)).and_then(|made| made);
            match &made {
                Ok(_) | Err(procraft_gpu::GpuError::Lost) => break,
                Err(e) => {
                    log::warn!("cropping {way:?}: {e}");
                    // memory may be short: the buffers kept for reads go
                    gpu.let_go_of_kept_buffers();
                }
            }
        }
        let canvas = match made {
            Ok(canvas) => canvas,
            Err(e) => {
                log::error!("cropping: {e}");
                st.crop_edit.failure = Some(crop_failure_said(&e));
                gpu.let_go_of_kept_buffers();
                return false;
            }
        };
        let old_canvas = st.canvas.replace(canvas);
        let old_doc = std::mem::replace(&mut st.doc, doc);
        if let Some(mut old_canvas) = old_canvas {
            // the crop is a new step: what was undone before it can't be redone
            old_canvas.forget_redo();
            // (kept only for the next stroke, as big as a layer four times over)
            old_canvas.let_go_of_carried();
            st.crop_undo = Some((old_canvas, old_doc));
        }
        st.crop_redo = None;
        st.selection = None;
        st.view = Some(st.fitted_view());
        st.reconfigure = true;
        st.revision += 1;
        self.shared.frame(st);
        true
    }
}

#[uniffi::export]
impl CanvasSession {
    /// What the GPU couldn't do for the canvas, worded for the app to tell, once each: a crop or a
    /// smudge there was no memory for (the canvas is as it was), or its device lost (what it held
    /// is gone, and the last save is what there is). None if all is well.
    pub fn take_failure(&self) -> Option<String> {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        // layers whose files couldn't be read as the artwork opened (shown empty, their files kept)
        if !st.unreadable_untold.is_empty() {
            let names: Vec<String> = std::mem::take(&mut st.unreadable_untold)
                .into_iter()
                .filter_map(|id| st.doc.layer(id).map(|l| l.name.clone()))
                .collect();
            if !names.is_empty() {
                return Some(crate::project::unreadable_said(&names));
            }
        }
        if st.gpu.as_ref().is_some_and(Gpu::is_lost) && !std::mem::replace(&mut st.crop_edit.lost_told, true) {
            st.crop_edit.failure = None;
            return Some(LOST_SAID.to_owned());
        }
        let refused = st.canvas.as_deref_mut().is_some_and(Canvas::take_smudge_refused);
        let failed = st.crop_edit.failure.take().or_else(|| refused.then(|| SMUDGE_SAID.to_owned()));
        let failed = failed.or_else(|| st.filters.take_said());
        if failed.is_some() {
            return failed;
        }
        // the plugins' list can wait on a plugin at work: asked with the canvas let go, so this
        // poll (every few seconds) never holds up a touch meanwhile
        let hooks = st.hooks.clone();
        drop(guard);
        let said = hooks.and_then(|hooks| hooks.take_said());
        // a plugin's folder that can't be reached, once (interface 22)
        said.or_else(|| {
            let folders = self.shared.folders.lock().unwrap_or_else(|e| e.into_inner()).clone();
            folders.and_then(|f| f.take_notice())
        })
    }

    /// The canvas's colour profile, by name.
    pub fn profile(&self) -> String {
        self.shared.lock().doc.profile_name()
    }

    /// Puts a canvas just made in the colour profile named `name` (nothing on it is moved). False
    /// if no profile has that name.
    pub fn set_new_profile(&self, name: String) -> bool {
        let Some((profile, icc)) = crate::colour::resolve(&name) else { return false };
        let mut st = self.shared.lock();
        st.doc.profile = profile;
        st.doc.icc = icc;
        self.shared.frame(&mut st);
        true
    }

    /// Puts the canvas in the colour profile named `name`, as one undo step. As measured, nothing
    /// is turned: its colours (the layers', the background's, the text's) keep their numbers and
    /// take the new profile's look. False if no profile has that name or the canvas is in it.
    pub fn change_profile(&self, name: String) -> bool {
        let Some((to, icc)) = crate::colour::resolve(&name) else { return false };
        self.text_end(true);
        self.change(&format!("Set color profile \"{name}\""), |d| {
            (d.profile_name() != name).then(|| d.assign_profile(to, icc))
        })
        .is_some()
    }

    /// The canvas's dots per inch (1 to 9999), as an undo step.
    pub fn set_dpi(&self, dpi: f32) -> bool {
        if !dpi.is_finite() {
            return false;
        }
        let dpi = dpi.clamp(1.0, 9999.0);
        self.change("DPI", |d| ((d.dpi - dpi).abs() > f32::EPSILON).then(|| d.dpi = dpi)).is_some()
    }

    /// Fits the whole canvas in the window, then shrinks it by `scale` (0.05 to 1) about the
    /// window's middle, as crop does so its frame's handles can be reached. The canvas shrinks
    /// into place over [`CROP_SHRINK_SECONDS`], slowing to a stop.
    pub fn fit_canvas_scaled(&self, scale: f32) {
        use procraft_engine::geom::{Affine, Vec2};
        let scale = if scale.is_finite() { scale.clamp(0.05, 1.0) } else { 1.0 };
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let (ww, wh) = st.window_size;
        let fit = View::fit((st.doc.width(), st.doc.height()), (ww, wh)).to_screen();
        let (cx, cy) = (ww as f32 / 2.0, wh as f32 / 2.0);
        let shrunk = fit
            .then(Affine::translate(Vec2::new(-cx, -cy)))
            .then(Affine::scale(scale))
            .then(Affine::translate(Vec2::new(cx, cy)));
        // shrinking into place, slowing to a stop
        let to = View::from_affine(shrunk);
        st.view_motion = st.view.map(|from| crate::ViewMotion {
            from,
            to,
            start: std::time::Instant::now(),
            curve: crate::ViewCurve::EaseOut(CROP_SHRINK_SECONDS),
        });
        if st.view_motion.is_none() {
            st.view = Some(to);
        }
        if let Some(c) = st.canvas.as_mut() {
            c.invalidate();
        }
        self.shared.frame(st);
    }
}

impl CanvasSession {
    /// Undo past the first step of a cropped canvas puts the canvas from before back; redo, while
    /// nothing new was done, crops again. False when there is no such step (the canvas's own
    /// history comes first).
    pub(crate) fn swap_crop(&self, undo: bool) -> bool {
        {
            // nothing to swap (or an adjustment open, whose undo is its own): leave it all be
            let st = self.shared.lock();
            if st.adjusting || (st.crop_undo.is_none() && st.crop_redo.is_none()) {
                return false;
            }
        }
        self.text_end(true);
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        // queued work becomes steps first, so the counts are current
        st.settle();
        st.flush();
        if undo {
            if st.canvas.as_deref().is_none_or(Canvas::can_undo) {
                return false;
            }
            let Some((canvas, doc)) = st.crop_undo.take() else { return false };
            let cropped = st.canvas.replace(canvas);
            let cropped_doc = std::mem::replace(&mut st.doc, doc);
            let steps = st.canvas.as_deref().map_or(0, Canvas::steps_made);
            st.crop_redo = cropped.map(|c| (c, cropped_doc, steps));
        } else {
            if !st.crop_redo_ready() {
                return false;
            }
            let Some((canvas, doc, _)) = st.crop_redo.take() else { return false };
            let before = st.canvas.replace(canvas);
            let before_doc = std::mem::replace(&mut st.doc, doc);
            st.crop_undo = before.map(|c| (c, before_doc));
        }
        st.selection = None;
        st.view = Some(st.fitted_view());
        st.reconfigure = true;
        // the files saved are the other canvas's, at its size: every layer is saved again
        let ids: Vec<_> = st.doc.pixel_layer_ids().collect();
        if let Some(c) = st.canvas.as_mut() {
            c.mark_changed(ids);
            c.invalidate();
        }
        st.revision += 1;
        self.shared.frame(st);
        true
    }
}

#[cfg(test)]
mod tests {
    use procraft_engine::view::View;
    use procraft_gpu::{Gpu, wgpu};

    use crate::{CanvasSession, PenSample, PointerKind};

    use super::Across;

    std::thread_local! {
        /// The ways this thread's next crop fails (see [`fail_here`]).
        static FAILING: std::cell::RefCell<Vec<Across>> = const { std::cell::RefCell::new(Vec::new()) };
        /// Whether drawing the canvas as the next crop begins makes a fault (see [`fault_drawing`]).
        static FAULT_DRAWING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// Makes this thread's next crop fail on the GPU every way it is tried, as one whose new
    /// canvas's textures there was no memory for does.
    fn fail_next_crop() {
        FAILING.with(|f| *f.borrow_mut() = vec![Across::Gpu, Across::Compressed]);
    }

    /// A texture the GPU won't make: a validation error, as a texture there is no memory for.
    fn invalid_texture(gpu: &Gpu) {
        let most = gpu.device.limits().max_texture_dimension_2d;
        drop(gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("too large"),
            size: wgpu::Extent3d { width: most + 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
    }

    /// Where a crop's work taken `across` so fails, if that way was set to fail.
    pub(super) fn fail_here(gpu: &Gpu, across: Across) {
        let fails = FAILING.with(|f| {
            let mut f = f.borrow_mut();
            let at = f.iter().position(|w| *w == across);
            at.map(|i| f.remove(i)).is_some()
        });
        if fails {
            invalid_texture(gpu);
        }
    }

    /// Where drawing the canvas as a crop begins makes a fault, if the next was set to.
    pub(super) fn fault_drawing(gpu: &Gpu) {
        if FAULT_DRAWING.with(|f| f.replace(false)) {
            invalid_texture(gpu);
        }
    }

    fn pen(x: f32, y: f32, t: f64) -> PenSample {
        PenSample { x, y, pressure: 1.0, tilt: 0.0, orientation: 0.0, time_ms: t, roll: 0.0 }
    }

    /// A canvas `side` square on a window its size, with a stroke on each of three layers.
    fn painted(side: u32) -> Option<std::sync::Arc<CanvasSession>> {
        Some(painted_on(Gpu::headless().ok()?, side))
    }

    /// [`painted`] on `gpu`.
    fn painted_on(gpu: Gpu, side: u32) -> std::sync::Arc<CanvasSession> {
        let s = CanvasSession::new(side, side);
        {
            let mut st = s.shared.lock();
            st.window_size = (side, side);
            st.view = Some(View::fit((side, side), (side, side)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        s.set_brush_size(0.3);
        for k in 0..3 {
            if k > 0 {
                s.add_layer();
            }
            s.set_color(0.3 * k as f32, 0.2, 0.9 - 0.3 * k as f32);
            let y = side as f32 * (k + 1) as f32 / 4.0;
            s.stroke_begin(PointerKind::Finger, vec![pen(10.0, y, 0.0)]);
            s.stroke_add((1..12).map(|i| pen(10.0 + i as f32 * side as f32 / 13.0, y, i as f64 * 8.0)).collect());
            s.stroke_end(vec![]);
        }
        s
    }

    /// Each layer's pixels, and the canvas's size.
    fn contents(s: &CanvasSession) -> (Vec<Vec<u8>>, (u32, u32)) {
        let layers = s.layers().into_iter().map(|l| s.copy_layer_contents(l.id).map(|c| c.rgba).unwrap_or_default());
        (layers.collect(), (s.info().width, s.info().height))
    }

    #[test]
    fn a_crop_the_gpu_cant_make_leaves_the_canvas_as_it_was_and_says_so() {
        let Some(s) = painted(120) else { return };
        let dpi = s.info().dpi;
        let before = contents(&s);
        assert!(before.0.iter().all(|px| px.as_chunks::<4>().0.iter().any(|p| p[3] != 0)), "every layer painted");
        let dir = std::env::temp_dir().join(format!("procraft-crop-failed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.to_string_lossy().into_owned();
        s.save(path.clone()).unwrap();
        // by whole pixels and resampled, through Done with a DPI typed in, and straight
        for resampled in [false, true] {
            fail_next_crop();
            s.crop_begin();
            s.crop_set_dpi(600.0);
            s.crop_set_resample(resampled);
            s.crop_set_size(Some(80), Some(70));
            assert!(!s.crop_end(true));
            assert!(s.take_failure().is_some_and(|why| why.contains("memory")));
            assert_eq!(s.take_failure(), None, "told once");
            assert_eq!(contents(&s), before);
            assert_eq!(s.info().dpi, dpi, "nor the DPI");
        }
        fail_next_crop();
        assert!(!s.crop_canvas(10.0, 10.0, 60.5, 50.0, 0.3, false, 0, 0));
        assert!(s.take_failure().is_some());
        assert_eq!(contents(&s), before);
        // no step was made
        assert_ne!(s.undo().as_deref(), Some("Crop canvas"));
        s.redo();
        // saved and opened again as it was
        s.save(path.clone()).unwrap();
        let opened = CanvasSession::open(path).unwrap();
        let Some(gpu) = s.shared.lock().gpu.clone() else { return };
        opened.use_gpu(gpu);
        {
            let mut st = opened.shared.lock();
            st.window_size = (120, 120);
            st.view = Some(View::fit((120, 120), (120, 120)));
            st.ensure_canvas();
        }
        assert_eq!(contents(&opened), before);
        // and a crop the GPU can make, made
        assert!(s.crop_canvas(10.0, 10.0, 60.0, 50.0, 0.0, false, 0, 0));
        assert_eq!(contents(&s).1, (60, 50));
        assert_eq!(s.take_failure(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_artwork_opened_on_a_lost_graphics_device_stays_as_it_was_saved() {
        let Some(s) = painted(80) else { return };
        let before = contents(&s);
        let dir = std::env::temp_dir().join(format!("procraft-open-lost-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.to_string_lossy().into_owned();
        s.save(path.clone()).unwrap();
        let Ok(lost) = Gpu::headless() else { return };
        lost.device.destroy();
        let _ = lost.device.poll(wgpu::PollType::wait_indefinitely());
        // shown on it: no canvas of empty layers, and it says so
        let opened = CanvasSession::open(path.clone()).unwrap();
        opened.use_gpu(lost);
        {
            let mut st = opened.shared.lock();
            st.ensure_canvas();
            assert!(st.canvas.is_none());
            assert!(st.error.is_some());
        }
        // whatever a save does with it, what was saved stays
        let _ = opened.save(path.clone());
        let again = CanvasSession::open(path).unwrap();
        let Some(gpu) = s.shared.lock().gpu.clone() else { return };
        again.use_gpu(gpu);
        assert_eq!(contents(&again), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_elsewhere_doesnt_stop_the_canvas_being_made() {
        let Some(s) = painted(120) else { return };
        let dir = std::env::temp_dir().join(format!("procraft-open-after-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.to_string_lossy().into_owned();
        s.save(path.clone()).unwrap();
        let opened = CanvasSession::open(path).unwrap();
        // a window's surface that couldn't be made, as the app came back
        opened.shared.lock().error = Some("no surface".to_owned());
        let Some(gpu) = s.shared.lock().gpu.clone() else { return };
        opened.use_gpu(gpu);
        let mut st = opened.shared.lock();
        st.ensure_canvas();
        assert!(st.canvas.is_some());
        drop(st);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crop_past_the_memory_free_is_refused_and_says_so() {
        let Some(s) = painted(120) else { return };
        let before = contents(&s);
        // nothing like enough left beside what the system keeps
        crate::device::MEMORY_FREE.with(|f| f.set(Some(100 << 20)));
        assert!(!s.crop_canvas(10.0, 10.0, 60.0, 50.0, 0.0, true, 4000, 4000));
        assert!(s.take_failure().is_some_and(|why| why.contains("memory")));
        assert_eq!(contents(&s), before);
        // a small canvas needs next to nothing, and is the way out of being short
        assert!(s.crop_canvas(10.0, 10.0, 60.0, 50.0, 0.0, false, 0, 0));
        crate::device::MEMORY_FREE.with(|f| f.set(None));
        assert_eq!(contents(&s).1, (60, 50));
        assert_eq!(s.take_failure(), None);
    }

    #[test]
    fn a_crop_past_the_budget_parks_what_doesnt_fit_and_loses_nothing() {
        let (Some(fits), Some(parks)) = (painted(120), painted(120)) else { return };
        let per = 80 * 70 * 4;
        // room for one layer of the new size: the other two go compressed. Moved by whole pixels,
        // every pixel comes across as it was; resampled, those parked are worked out on the CPU and
        // the rest on the GPU, whose float sums may round a level apart
        for (x, angle, most) in [(10.0, 0.0, 0u8), (10.5, 0.2, 1)] {
            if let Some(c) = parks.shared.lock().canvas.as_mut() {
                c.set_resident_budget(per);
            }
            assert!(fits.crop_canvas(x, 10.0, 80.0, 70.0, angle, false, 0, 0));
            assert!(parks.crop_canvas(x, 10.0, 80.0, 70.0, angle, false, 0, 0));
            assert_eq!(parks.shared.lock().canvas.as_deref().map(procraft_gpu::Canvas::parked_layers), Some(2));
            let ((parked, size), (resident, resident_size)) = (contents(&parks), contents(&fits));
            assert_eq!((parked.len(), size), (resident.len(), resident_size));
            for (a, b) in parked.iter().zip(&resident) {
                assert_eq!(a.len(), b.len());
                let off = a.iter().zip(b).map(|(p, q)| p.abs_diff(*q)).max().unwrap_or(0);
                assert!(off <= most, "{off} levels apart at {x}, {angle}");
            }
            assert!(parks.undo().is_some());
            assert!(fits.undo().is_some());
        }
    }

    /// Each layer of `a` and `b` alike to within `most` levels, and the canvases the same size.
    fn alike(a: &CanvasSession, b: &CanvasSession, most: u8) {
        let ((a, a_size), (b, b_size)) = (contents(a), contents(b));
        assert_eq!((a.len(), a_size), (b.len(), b_size));
        for (p, q) in a.iter().zip(&b) {
            assert_eq!(p.len(), q.len());
            let off = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            assert!(off <= most, "{off} levels apart");
        }
    }

    /// The crop's own work on the GPU failing, each layer is worked out on the CPU and kept
    /// compressed instead: the crop is made, nothing lost, nothing said.
    #[test]
    fn a_crop_the_gpu_cant_copy_goes_across_compressed() {
        let (Some(gpu_way), Some(cpu_way)) = (painted(120), painted(120)) else { return };
        for (x, angle, most) in [(10.0, 0.0, 0u8), (10.5, 0.2, 1)] {
            FAILING.with(|f| *f.borrow_mut() = vec![Across::Gpu]);
            assert!(cpu_way.crop_canvas(x, 10.0, 80.0, 70.0, angle, false, 0, 0));
            assert!(FAILING.with(|f| f.borrow().is_empty()), "the GPU's way was tried first");
            assert!(gpu_way.crop_canvas(x, 10.0, 80.0, 70.0, angle, false, 0, 0));
            assert_eq!(cpu_way.take_failure(), None);
            assert_eq!(cpu_way.shared.lock().canvas.as_deref().map(procraft_gpu::Canvas::parked_layers), Some(3));
            alike(&cpu_way, &gpu_way, most);
            assert!(cpu_way.undo().is_some() && gpu_way.undo().is_some());
            assert_eq!(contents(&cpu_way), contents(&gpu_way));
        }
    }

    /// Memory free for the new canvas but not for its layers on the GPU too: they go across
    /// compressed.
    #[test]
    fn a_crop_with_room_only_for_the_canvas_keeps_its_layers_compressed() {
        let (Some(short), Some(roomy)) = (painted(120), painted(120)) else { return };
        // 2000 square: the layers on the GPU and the canvas's own (3 + 7) 160 MB, beside what the
        // system keeps (512): 672 won't fit in 650, the canvas's own alone (624) will
        crate::device::MEMORY_FREE.with(|f| f.set(Some(650 << 20)));
        assert!(short.crop_canvas(10.0, 10.0, 60.0, 50.0, 0.0, true, 2000, 2000));
        crate::device::MEMORY_FREE.with(|f| f.set(None));
        assert!(roomy.crop_canvas(10.0, 10.0, 60.0, 50.0, 0.0, true, 2000, 2000));
        assert_eq!(short.take_failure(), None);
        assert_eq!(short.shared.lock().canvas.as_deref().map(procraft_gpu::Canvas::parked_layers), Some(3));
        alike(&short, &roomy, 1);
    }

    /// A fault drawing the canvas as it is (one a weak GPU makes, and the app lives with) doesn't
    /// stop a crop: only the crop's own work is checked.
    /// Crop & Resize takes the drawing guide with the picture: the perspective guide's vanishing
    /// points go where their picture pixels went (within a thousandth of a pixel; they were
    /// dropped, so the guide drew nothing), and the grids' and symmetry's centre, turn and size
    /// with them. Undone, the guide is as it was; redone, moved again.
    #[test]
    fn a_crop_takes_the_drawing_guide_with_the_picture() {
        use procraft_engine::geom::Vec2;
        use procraft_engine::guide::GuideKind;
        let Some(s) = painted(120) else { return };
        {
            let mut st = s.shared.lock();
            let g = &mut st.doc.guide;
            g.on = true;
            g.kind = GuideKind::Perspective;
            g.centre = [60.0, 50.0];
            g.angle = 0.2;
            g.vanishing = vec![[-40.0, 30.0], [150.0, 25.0], [70.0, 300.0]];
        }
        let before = s.shared.lock().doc.guide.clone();
        // the middle 80 x 60, turned 0.25 and resampled to twice its size
        let (rect, turn, size) = ([20.0, 30.0, 80.0, 60.0], 0.25, (160, 120));
        assert!(s.crop_canvas(rect[0], rect[1], rect[2], rect[3], turn, true, size.0, size.1));
        assert_eq!(contents(&s).1, size);
        let c = procraft_engine::crop::crop(rect, turn, Some(size));
        let after = s.shared.lock().doc.guide.clone();
        let back = |[x, y]: [f32; 2]| {
            let p = c.to_old.apply(Vec2::new(x, y));
            [p.x, p.y]
        };
        let far = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        assert_eq!(after.vanishing.len(), 3, "the vanishing points kept");
        let worst = after
            .vanishing
            .iter()
            .zip(&before.vanishing)
            .map(|(new, old)| far(back(*new), *old))
            .fold(far(back(after.centre), before.centre), f32::max);
        assert!(worst < 1e-3, "{worst} px off");
        assert!((after.angle - (before.angle - turn)).abs() < 1e-5, "turned {}", after.angle);
        for (new, old) in after.looks.iter().zip(&before.looks) {
            assert!((new.size - old.size * 2.0).abs() < 1e-3, "grid {} from {}", new.size, old.size);
        }
        // shown: three points and their rays over the new canvas
        let shown = s.guide();
        assert!(shown.on);
        assert_eq!(shown.vanishing.len(), 6);
        assert!(shown.lines.len() / 4 > 24, "{} lines", shown.lines.len() / 4);
        // undone, as it was; redone, moved again
        s.undo();
        assert_eq!(contents(&s).1, (120, 120));
        assert_eq!(s.shared.lock().doc.guide, before);
        s.redo();
        assert_eq!(s.shared.lock().doc.guide, after);
    }

    #[test]
    fn a_fault_drawing_the_canvas_doesnt_stop_a_crop() {
        let Some(s) = painted(120) else { return };
        FAULT_DRAWING.with(|f| f.set(true));
        assert!(s.crop_canvas(10.0, 10.0, 80.0, 70.0, 0.0, false, 0, 0));
        assert!(!FAULT_DRAWING.with(std::cell::Cell::get));
        assert_eq!(s.take_failure(), None);
        assert_eq!(contents(&s).1, (80, 70));
    }

    /// On an old GPU (no storage buffers or compute, no half floats to draw into, its textures at
    /// most 4096: see `Gpu::headless_old`), crops by whole pixels, turned and resampled come out as
    /// the CPU works them out, each an undo step, nothing said.
    #[test]
    fn a_crop_on_an_old_gpu_comes_out_as_the_cpu_works_it_out() {
        let Ok(old) = Gpu::headless_old() else { return };
        let s = painted_on(old, 120);
        let before = contents(&s);
        let crops = [
            (10.0, 0.0, None, 0u8),
            (10.5, 0.2, None, 1),
            (0.0, 0.0, Some((200, 150)), 1),
            (-20.0, 0.0, Some((40, 30)), 1),
        ];
        for (x, angle, size, most) in crops {
            let c = procraft_engine::crop::crop([x, 10.0, 80.0, 70.0], angle, size);
            let (w, h) = size.unwrap_or((0, 0));
            assert!(s.crop_canvas(x, 10.0, 80.0, 70.0, angle, size.is_some(), w, h));
            assert_eq!(s.take_failure(), None);
            // made on the GPU, every layer there: not the way through the CPU kept for when that fails
            assert_eq!(s.shared.lock().canvas.as_deref().map(procraft_gpu::Canvas::parked_layers), Some(0));
            let (after, made) = contents(&s);
            assert_eq!(made, (c.width, c.height));
            for (was, is) in before.0.iter().zip(&after) {
                let expected = procraft_engine::crop::resample(was, 120, 120, &c);
                assert_eq!(expected.len(), is.len());
                let off = expected.iter().zip(is).map(|(p, q)| p.abs_diff(*q)).max().unwrap_or(0);
                assert!(off <= most, "{off} levels off at {x}, {angle}, {size:?}");
            }
            let gpu = s.shared.lock().gpu.clone().unwrap();
            assert_eq!(gpu.errors(), 0, "GPU validation errors");
            assert!(s.undo().is_some());
            assert_eq!(contents(&s), before);
        }
    }

    #[test]
    fn what_the_system_has_free_is_read_from_its_memory_report() {
        let report = "MemTotal:       11543576 kB\nMemFree:         1248464 kB\nMemAvailable:    6577012 kB\n";
        assert_eq!(crate::device::available_in(report), Some(6_577_012 * 1024));
        assert_eq!(crate::device::available_in("MemTotal: 1 kB\n"), None);
    }

    #[test]
    fn a_crop_on_a_lost_graphics_device_changes_nothing() {
        let Some(s) = painted(80) else { return };
        let gpu = s.shared.lock().gpu.clone().unwrap();
        gpu.device.destroy();
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        assert!(gpu.is_lost());
        // resampled (whose bindings wgpu panicked making on a lost device) and by whole pixels
        assert!(!s.crop_canvas(5.0, 5.0, 40.5, 30.0, 0.2, false, 0, 0));
        assert!(!s.crop_canvas(5.0, 5.0, 40.0, 30.0, 0.0, false, 0, 0));
        assert!(s.take_failure().is_some_and(|why| why.contains("open the artwork again")));
        assert_eq!((s.info().width, s.info().height), (80, 80));
    }
}
