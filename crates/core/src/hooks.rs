//! The points where a plugin that's on steps in. Each does nothing with none on, and holds no
//! feature of its own: what a plugin does is in its file. They stay for good, so there are as few as
//! plugins need, and each is as narrow as it can be.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::plugin_file;
use crate::plugin_run::{Hook, Running, Stop};
use crate::plugins::Plugins;
use crate::{CanvasSession, KeyCombo};

/// What the slider look's hook is asked of the side bar's slider tracks: their opacity, and how
/// frosted what is behind them is (interface 11).
pub(crate) const SLIDER_OPACITY: i32 = 0;
pub(crate) const SLIDER_FROSTED: i32 = 1;
/// What the colour panel's hook asks (interface 12): whether its sliders step one unit at a tap on
/// either end, and whether its swatch shows the colour as the panel opened beside the colour now.
pub(crate) const COLOUR_STEPS: i32 = 0;
pub(crate) const COLOUR_BEFORE: i32 = 1;

/// What the panel layout's hook asks (interface 20): how near the floating panels snap, whether they
/// fold to a tab, whether those docked make room in the drawing area, and whether the interface's
/// bars may be moved.
pub(crate) const LAYOUT_SNAP: i32 = 0;
pub(crate) const LAYOUT_FOLD: i32 = 1;
pub(crate) const LAYOUT_ROOM: i32 = 2;
pub(crate) const LAYOUT_EDIT: i32 = 3;
/// The panel layout hook's part 4: whether the Tool Belt shows (see [`crate::PanelLayout::belt_shown`]).
pub(crate) const LAYOUT_BELT: i32 = 4;
/// The panel layout hook's part 5: Collapse to edge (see [`crate::BarLayout::collapsed`] and
/// [`crate::PanelLayout::piece_collapsed`]).
pub(crate) const LAYOUT_COLLAPSE: i32 = 5;

/// The role of the colour hook's that is the area round the canvas (interface 11).
pub(crate) const SURROUND_ROLE: i32 = 1;

/// The parts of the view the gesture lock's hook is asked about: its turn, and the fit a quick
/// pinch in makes (interface 10).
pub(crate) const GESTURE_TURN: i32 = 0;
pub(crate) const GESTURE_FIT: i32 = 1;

/// A plugin at work: a lock of its own, so a long call holds up nothing but it.
pub(crate) type Shared = Arc<Mutex<Running>>;

pub(crate) fn at_work(running: &Shared) -> MutexGuard<'_, Running> {
    running.lock().unwrap_or_else(|e| e.into_inner())
}

struct OnHook {
    id: String,
    running: Shared,
    /// Of a plugin of features (interface 16), a copy of its code for each feature that is on: the
    /// feature this one is for (its switch's place among the controls), and every feature's switch,
    /// which this copy is told as off but for its own, so it answers as the plugin it was.
    feature: Option<usize>,
    features: Arc<[usize]>,
    /// The id the marks it keeps with layers go by: its own, or that of a plugin it took the place
    /// of whose marks it keeps (interface 16).
    marks: String,
    /// The hooks it is on, as bits (see [`Hook::bit`]).
    hooks: u64,
    /// The keys of its rows in the brush studio, in their order.
    brush_keys: Vec<String>,
    /// On the picker hook: its picture.
    picture: Option<Picture>,
    /// On an event's hook: a copy of its code of its own for them, so that what an event calls in
    /// the app never waits on this one (its other hooks' copy).
    events: Option<Shared>,
    /// The kinds of layer it adds (keys `<id>.<key>`), in its manifest's order.
    layer_kinds: Vec<crate::PluginKind>,
    /// Its brush engines, in its manifest's order, and the looks of the brushes they drew lately,
    /// by engine and data (the latest last).
    brush_engines: Vec<crate::plugin_brushes::BrushEngine>,
    looks: Vec<(String, u64, Arc<crate::plugin_brushes::BrushLook>)>,
    /// Starts a copy of its code of its own with that much memory at most: what its brush engines
    /// make brushes of files on (see `plugin_brushes.rs`).
    copier: Option<Copier>,
    /// The brushes (by engine and data) whose looks it couldn't make, or made such as the app can't
    /// take: drawn as the app read them, not asked again until it comes on again.
    no_looks: Vec<(String, u64)>,
    /// The looks (by engine and data) being made on threads of their own.
    making: Vec<(String, u64, Arc<crate::plugin_brushes::Making>)>,
}

/// Which copy of a plugin's code goes on the hooks: the plugin's id, the feature it is for (of a
/// plugin of features, interface 16, its switch's place among the controls) with every feature's
/// switch, and the id its layers' marks go by.
pub(crate) struct OnCopy {
    pub(crate) id: String,
    pub(crate) feature: Option<usize>,
    pub(crate) features: Arc<[usize]>,
    pub(crate) marks: String,
}

/// The most brushes whose looks couldn't be made are remembered, the latest kept.
const MOST_NO_LOOKS: usize = 1024;

/// Starts a copy of a plugin's code of its own, as it is on now, with that much memory at most.
pub(crate) type Copier = Arc<dyn Fn(usize) -> Result<Running, Stop> + Send + Sync>;

/// A picker plugin's picture: its code, the keys it reads its plugin's controls by (written to
/// interface 6 or later; none before), and their values now.
#[derive(Clone)]
pub(crate) struct Picture {
    pub(crate) text: Arc<str>,
    pub(crate) keys: Option<Arc<[String]>>,
    pub(crate) values: Vec<f32>,
}

#[derive(Default)]
struct Working {
    /// In id order, so plugins go in the same order however they were turned on.
    on: Vec<OnHook>,
    /// What stopped since the list last asked, and why.
    stopped: Vec<(String, Stop)>,
    /// What the app hasn't told yet: whoever is drawing sees the stroke change, not the list.
    untold: Option<String>,
}

/// The plugins that are on, shared by a list and the sessions going through it. Not a static: they go
/// with the list, so one comes off at once, no restart.
#[derive(Default)]
pub(crate) struct Hooks {
    /// The hooks with a plugin on them, as bits: all a stroke with none pays for is reading this.
    hooked: AtomicU64,
    working: Mutex<Working>,
    /// Where a stop is written down, to stay off through a restart.
    store: Option<PathBuf>,
    /// The plugins' own artworks open, each by its plugin: told its controls as they change.
    canvases: Mutex<Vec<(String, std::sync::Weak<crate::PluginCanvas>)>>,
    /// Counts the plugins coming on and off and their controls changing.
    generation: std::sync::atomic::AtomicU64,
}

impl std::fmt::Debug for Hooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hooks").field("hooked", &self.hooked).finish_non_exhaustive()
    }
}

impl Hooks {
    pub(crate) fn kept_in(store: Option<PathBuf>) -> Hooks {
        Hooks { store, ..Hooks::default() }
    }

    fn working(&self) -> MutexGuard<'_, Working> {
        self.working.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Writes down that `id` stopped, for the list and for the next start.
    fn note_stop(&self, working: &mut Working, id: String, stop: Stop) {
        log::warn!("a plugin stopped: {id}: {stop:?}");
        if let Some(store) = &self.store {
            plugin_file::note_stopped(store, &id, stop.text());
        }
        working.stopped.push((id, stop));
        working.untold = Some(format!("A plugin stopped. {}", stop.text()));
        self.changed();
    }

    /// The plugin whose copy of its code is at `at` comes off, every copy, having stopped: where
    /// what came after `at` is now.
    fn stop_at(&self, working: &mut Working, at: usize, stop: Stop) -> usize {
        let Some(id) = working.on.get(at).map(|p| p.id.clone()) else { return at };
        let before = working.on.iter().take(at).filter(|p| p.id == id).count();
        working.on.retain(|p| p.id != id);
        self.note_stop(working, id, stop);
        at - before
    }

    fn has(&self, hook: Hook) -> bool {
        self.hooked.load(Ordering::Relaxed) & hook.bit() != 0
    }

    /// A number that changes whenever a plugin comes on or off or its controls change: what a
    /// plugin answered before it changed may not be what it answers now.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn changed(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// The bits go by what is on now.
    fn rehook(&self, working: &Working) {
        let hooked = working.on.iter().fold(0, |bits, plugin| bits | plugin.hooks);
        self.hooked.store(hooked, Ordering::Relaxed);
    }

    /// `start` through each plugin on `hook`, in turn. One that stops is off the hooks there and then,
    /// and what it was handed goes on as it came to it.
    fn through<T: Copy>(&self, hook: Hook, start: T, mut step: impl FnMut(&mut Running, T) -> Result<T, Stop>) -> T {
        self.through_by_id(hook, start, |_, running, at| step(running, at))
    }

    /// The same, each step told whose code it is (by its id).
    fn through_by_id<T: Copy>(
        &self,
        hook: Hook,
        start: T,
        mut step: impl FnMut(&str, &mut Running, T) -> Result<T, Stop>,
    ) -> T {
        let mut working = self.working();
        let mut at = start;
        let mut i = 0;
        while let Some(plugin) = working.on.get_mut(i) {
            if plugin.hooks & hook.bit() == 0 {
                i += 1;
                continue;
            }
            let answer = step(&plugin.id, &mut at_work(&plugin.running), at);
            match answer {
                Ok(to) => {
                    at = to;
                    i += 1;
                }
                Err(stop) => i = self.stop_at(&mut working, i, stop),
            }
        }
        self.rehook(&working);
        at
    }

    /// The position hook: where a sample in window pixels lands, before the stroke sees it.
    #[inline]
    pub(crate) fn position(&self, x: f32, y: f32) -> (f32, f32) {
        // what isn't a position goes on as it came, and no plugin answers for it
        if !self.has(Hook::Position) || !x.is_finite() || !y.is_finite() {
            return (x, y);
        }
        self.through(Hook::Position, (x, y), |running, at| running.position(at.0, at.1))
    }

    /// The pick hook: whether the eyedropper reads the current layer, not the canvas as it is
    /// composited. It only asks: reading either is the app's, and it is a read like any other.
    pub(crate) fn pick_layer(&self) -> bool {
        self.has(Hook::PickLayer)
            && self.through(Hook::PickLayer, false, |running, said| Ok(said | running.pick_layer()?))
    }

    /// The fill-by-all hook: whether ColorDrop measures what it fills on the canvas as composited
    /// (every layer seen) rather than on the current layer or the reference layer. It only asks.
    pub(crate) fn fill_all_layers(&self) -> bool {
        self.has(Hook::FillAllLayers)
            && self.through(Hook::FillAllLayers, false, |running, said| Ok(said | running.fill_all_layers()?))
    }

    /// The colour hook: the colour (0xRRGGBB) the interface draws `role` in, if a plugin says (the
    /// first that does).
    pub(crate) fn interface_colour(&self, role: i32) -> Option<u32> {
        if !self.has(Hook::InterfaceColour) {
            return None;
        }
        self.through(Hook::InterfaceColour, None, |running, said| match said {
            Some(colour) => Ok(Some(colour)),
            None => running.interface_colour(role),
        })
    }

    /// The sound hook: `frames` samples at `rate` of the stroke as `motion` has it, as the first
    /// plugin on the hook makes them, told `brush` when that changed: for the sound's own thread
    /// (the first at a rate readies the plugin, a long call). None with none on it, or if it stops.
    pub(crate) fn stroke_sound(
        &self,
        frames: usize,
        rate: u32,
        motion: &crate::stroke_sound::SoundMotion,
        brush: &crate::stroke_sound::SoundBrush,
    ) -> Option<Vec<f32>> {
        let (id, running) = self.long_calls(Hook::StrokeSound).into_iter().next()?;
        let made = at_work(&running).stroke_sound(frames, rate, motion, brush);
        match made {
            Ok(samples) => samples,
            Err(stop) => {
                self.stopped(&id, stop);
                None
            }
        }
    }

    /// The keys of the rows in the brush studio of the plugin making the sound, to tell it the
    /// current brush's.
    pub(crate) fn sound_brush_keys(&self) -> Vec<String> {
        let working = self.working();
        let first = working.on.iter().find(|p| p.hooks & Hook::StrokeSound.bit() != 0);
        first.map(|p| p.brush_keys.clone()).unwrap_or_default()
    }

    /// The in-between hook: the frame between `this` and `next` (each `width` × `height`,
    /// premultiplied RGBA8), as the first plugin on the hook draws it: a long call, not for the main
    /// thread. None with none on it, if it draws none, or if it stops.
    pub(crate) fn inbetween(&self, this: &[u8], next: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
        let (id, running) = self.long_calls(Hook::Inbetween).into_iter().next()?;
        match at_work(&running).inbetween(this, next, width, height) {
            Ok(made) => made,
            Err(stop) => {
                self.stopped(&id, stop);
                None
            }
        }
    }

    /// Whether a plugin is on `hook`, an event's (see [`Hooks::tell`]).
    pub(crate) fn listens(&self, hook: Hook) -> bool {
        self.has(hook)
    }

    /// Tells the plugins on `hook` (`stroke_kept` or `layer_added`) that it happened, on the artwork
    /// `session`: each acts as an action does, briefly. One that goes wrong comes off and says why.
    pub(crate) fn tell(&self, hook: Hook, session: Arc<CanvasSession>) {
        let kind = match hook {
            Hook::StrokeKept => 1,
            Hook::LayerAdded => 2,
            _ => return,
        };
        if !self.has(hook) {
            return;
        }
        let told: Vec<(String, Shared)> = {
            let working = self.working();
            let on = working.on.iter().filter(|plugin| plugin.hooks & hook.bit() != 0);
            on.map(|p| (p.id.clone(), Arc::clone(p.events.as_ref().unwrap_or(&p.running)))).collect()
        };
        for (id, running) in told {
            let answer = at_work(&running).event(kind, Arc::clone(&session));
            if let Err(stop) = answer {
                self.stopped(&id, stop);
            }
        }
    }

    /// Whether a plugin draws in-betweens: the frame options then offer In-between.
    pub(crate) fn tweens(&self) -> bool {
        self.has(Hook::Inbetween)
    }

    /// Whether a plugin makes the stroke's sound: the apps play it while one does.
    pub(crate) fn makes_stroke_sound(&self) -> bool {
        self.has(Hook::StrokeSound)
    }

    /// The curve hook: the curve the first plugin on it offers for a snapping stroke of `points`
    /// (window pixels), QuickShape having found `best` first (-1: a curve asked for): its nodes.
    /// None with none on it, if it offers none, or if it stops.
    pub(crate) fn quickshape_curve(&self, points: &[(f32, f32)], best: i32) -> Option<Vec<(f32, f32)>> {
        let (id, running) = self.long_calls(Hook::QuickshapeCurve).into_iter().next()?;
        let offered = at_work(&running).quickshape_curve(points, best);
        match offered {
            Ok(nodes) => nodes,
            Err(stop) => {
                self.stopped(&id, stop);
                None
            }
        }
    }

    /// The triangle hook: whether the colour panel's Classic shows a triangle in place of its
    /// square.
    pub(crate) fn colour_triangle(&self) -> bool {
        self.has(Hook::ColourTriangle)
            && self.through(Hook::ColourTriangle, false, |running, said| Ok(said | running.colour_triangle()?))
    }

    /// Whether a plugin sets brushes' settings at random (the brush studio shows Randomise).
    pub(crate) fn randomises_brushes(&self) -> bool {
        self.has(Hook::RandomiseBrush)
    }

    /// The randomise hook: where brush setting `setting` goes, as a share of its range, if a plugin
    /// says (the first that does).
    pub(crate) fn randomise(&self, setting: i32, seed: i32) -> Option<f32> {
        if !self.has(Hook::RandomiseBrush) {
            return None;
        }
        self.through(Hook::RandomiseBrush, None, |running, said| match said {
            Some(share) => Ok(Some(share)),
            None => running.randomise(setting, seed),
        })
    }

    /// The opacity hook: whether a layer's opacity changed while it is one of several picked sets
    /// every picked layer's.
    pub(crate) fn opacity_together(&self) -> bool {
        self.has(Hook::OpacityTogether)
            && self.through(Hook::OpacityTogether, false, |running, said| Ok(said | running.opacity_together()?))
    }

    /// The pin hook: whether a touch on the canvas with a panel open paints at once and leaves the
    /// panel up, rather than closing it (a tap) or fading it (a drag).
    pub(crate) fn pin_panels(&self) -> bool {
        self.has(Hook::PinPanels)
            && self.through(Hook::PinPanels, false, |running, said| Ok(said | running.pin_panels()?))
    }

    /// The clear hook: whether the interface keeps clear of the status bar and the screen's cutouts
    /// (a camera's), rather than taking the whole window.
    pub(crate) fn keep_clear(&self) -> bool {
        self.has(Hook::KeepClear)
            && self.through(Hook::KeepClear, false, |running, said| Ok(said | running.keep_clear()?))
    }

    /// The undo notice hook: whether undo and redo go by without the notice naming what they took
    /// back or brought back.
    pub(crate) fn hide_undo_notice(&self) -> bool {
        self.has(Hook::HideUndoNotice)
            && self.through(Hook::HideUndoNotice, false, |running, said| Ok(said | running.hide_undo_notice()?))
    }

    /// The radial hook: how many parts radial symmetry makes, if a plugin says (the first that
    /// does); the guide's usual eight otherwise.
    pub(crate) fn radial_segments(&self) -> Option<u32> {
        if !self.has(Hook::RadialSegments) {
            return None;
        }
        self.through(Hook::RadialSegments, None, |running, said| match said {
            Some(n) => Ok(Some(n)),
            None => running.radial_segments(),
        })
    }

    /// The scale hook: how big the interface shows in a window `width` × `height` points at 1. The
    /// number is the plugin's, and all the app adds is what keeps every page in reach: see
    /// [`crate::interface_size`].
    pub(crate) fn interface_scale(&self, width: f32, height: f32) -> f32 {
        if !self.has(Hook::InterfaceScale) {
            return 1.0;
        }
        let said = self.through(Hook::InterfaceScale, 1.0, |running, _| running.interface_scale(width, height));
        said.min(crate::interface_size::most_for(width, height))
    }

    /// The paste hook: whether a picture pasted with a mask current goes into the mask.
    pub(crate) fn paste_mask(&self) -> bool {
        self.has(Hook::PasteMask)
            && self.through(Hook::PasteMask, false, |running, said| Ok(said | running.paste_mask()?))
    }

    /// What the first plugin on `hook` answers to `ask`; none with none on it, or if it stops (it
    /// comes off).
    fn first_on<T>(&self, hook: Hook, ask: impl FnOnce(&mut Running) -> Result<T, Stop>) -> Option<T> {
        if !self.has(hook) {
            return None;
        }
        let mut working = self.working();
        let at = working.on.iter().position(|plugin| plugin.hooks & hook.bit() != 0)?;
        let answer = working.on.get(at).map(|plugin| ask(&mut at_work(&plugin.running)))?;
        match answer {
            Ok(said) => Some(said),
            Err(stop) => {
                self.stop_at(&mut working, at, stop);
                self.rehook(&working);
                None
            }
        }
    }

    /// The key hook: what a hardware keyboard's `press` is taken as, as the first plugin on it says
    /// (only one rebinds keys): itself, another press, or none at all.
    pub(crate) fn key_shortcut(&self, press: KeyCombo) -> Option<KeyCombo> {
        match self.first_on(Hook::KeyShortcut, |running| running.key_shortcut(&press)) {
            Some(Some(taken)) => taken.into_iter().next(),
            Some(None) | None => Some(press),
        }
    }

    /// The key hook: the presses that now do what `press` does by default, as the first plugin on
    /// it says; none: still `press`.
    pub(crate) fn key_shortcut_keys(&self, press: &KeyCombo) -> Option<Vec<KeyCombo>> {
        self.first_on(Hook::KeyShortcut, |running| running.key_shortcut_keys(press)).flatten()
    }

    /// Whether a plugin rebinds keys.
    pub(crate) fn rebinds_keys(&self) -> bool {
        self.has(Hook::KeyShortcut)
    }

    /// What `id` keeps handed to each copy of its code that is on, and its artworks open: another
    /// copy (its panel's) kept it. One that stops comes off.
    pub(crate) fn give_kept(&self, id: &str, data: &[u8]) {
        {
            let mut working = self.working();
            let mut stopped = None;
            for (at, plugin) in working.on.iter().enumerate().filter(|(_, p)| p.id == id) {
                if let Some(events) = plugin.events.as_ref() {
                    let _ = at_work(events).give_kept(data);
                }
                if let Err(stop) = at_work(&plugin.running).give_kept(data) {
                    stopped = Some((at, stop));
                    break;
                }
            }
            if let Some((at, stop)) = stopped {
                self.stop_at(&mut working, at, stop);
                self.rehook(&working);
            }
        }
        for canvas in self.canvases_of(id) {
            canvas.give_kept(data);
        }
    }

    /// `id`'s artworks open.
    fn canvases_of(&self, id: &str) -> Vec<Arc<crate::PluginCanvas>> {
        let mut canvases = self.canvases.lock().unwrap_or_else(|e| e.into_inner());
        canvases.retain(|(_, canvas)| canvas.strong_count() > 0);
        canvases.iter().filter(|(of, _)| of == id).filter_map(|(_, canvas)| canvas.upgrade()).collect()
    }

    /// The plugins on `hook`, to be called with the list let go: a long call holds up only them.
    fn long_calls(&self, hook: Hook) -> Vec<(String, Shared)> {
        let working = self.working();
        let on = working.on.iter().filter(|plugin| plugin.hooks & hook.bit() != 0);
        on.map(|plugin| (plugin.id.clone(), Arc::clone(&plugin.running))).collect()
    }

    /// `id` comes off the hooks, having stopped in a long call (or an action).
    pub(crate) fn stopped(&self, id: &str, stop: Stop) {
        let mut working = self.working();
        working.on.retain(|p| p.id != id);
        self.note_stop(&mut working, id.to_owned(), stop);
        self.rehook(&working);
    }

    /// `rgba`, rows of `width` pixels, as the plugin on the paste hook makes it; false if it can't,
    /// and what has gone through is no use. The first that takes it has it all.
    pub(crate) fn paste_rows(&self, rgba: &mut [u8], width: usize) -> bool {
        let Some((id, running)) = self.long_calls(Hook::PasteMask).into_iter().next() else { return false };
        let done = at_work(&running).paste_rows(rgba, width);
        done.inspect_err(|stop| self.stopped(&id, *stop)).is_ok()
    }

    /// The fill hook: whether a fill being let go has anything to go through.
    pub(crate) fn fill_mask(&self) -> bool {
        self.has(Hook::FillMask) && self.through(Hook::FillMask, false, |running, said| Ok(said | running.fill_mask()?))
    }

    /// `rows`, a fill of `width` × `height` from `seed`, a bit a pixel in words of 64, through each
    /// plugin on the fill hook, each taking out what it says: a long call, not for the main thread.
    /// Whether anything came out. One that stops takes nothing out.
    pub(crate) fn fill_rows(&self, rows: &mut Vec<u64>, width: usize, height: usize, seed: (usize, usize)) -> bool {
        let mut changed = false;
        for (id, running) in self.long_calls(Hook::FillMask) {
            let mut left = rows.clone();
            match at_work(&running).fill(&mut left, width, height, seed) {
                Ok(true) => {
                    *rows = left;
                    changed = true;
                }
                Ok(false) => {}
                Err(stop) => self.stopped(&id, stop),
            }
        }
        changed
    }

    /// `words` for the app to tell, once.
    pub(crate) fn say(&self, words: &str) {
        self.working().untold = Some(words.to_owned());
    }

    pub(crate) fn is_on(&self, id: &str) -> bool {
        self.working().on.iter().any(|p| p.id == id)
    }

    #[cfg(test)]
    pub(crate) fn put_on(&self, id: String, running: Running, hooks: &[Hook]) {
        let marks = id.clone();
        self.put_on_copy(OnCopy { id, feature: None, features: Arc::from([]), marks }, running, hooks);
    }

    /// The copy of a plugin's code `copy` says (one feature's, interface 16) on `hooks`, in place of
    /// the one there was. A plugin's copies go in their features' order.
    pub(crate) fn put_on_copy(&self, copy: OnCopy, running: Running, hooks: &[Hook]) {
        let OnCopy { id, feature, features, marks } = copy;
        let mut working = self.working();
        working.on.retain(|p| p.id != id || (feature.is_some() && p.feature != feature));
        let at = working.on.partition_point(|p| p.id < id || (p.id == id && p.feature < feature));
        let hooks = hooks.iter().fold(0, |bits, hook| bits | hook.bit());
        let running = Arc::new(Mutex::new(running));
        working.on.insert(
            at,
            OnHook {
                id,
                running,
                feature,
                features,
                marks,
                hooks,
                brush_keys: Vec::new(),
                picture: None,
                events: None,
                layer_kinds: Vec::new(),
                brush_engines: Vec::new(),
                looks: Vec::new(),
                copier: None,
                no_looks: Vec::new(),
                making: Vec::new(),
            },
        );
        self.rehook(&working);
        self.changed();
    }

    /// `id`'s copy of its code for its events (see [`OnHook::events`]): for `feature`'s, of a
    /// plugin of features.
    pub(crate) fn give_events(&self, id: &str, feature: Option<usize>, running: Running) {
        if let Some(plugin) = self.working().on.iter_mut().find(|p| p.id == id && p.feature == feature) {
            plugin.events = Some(Arc::new(Mutex::new(running)));
        }
    }

    /// `id`'s picture's code, for the picker hook, read by `keys` (if it reads its controls) at
    /// its controls' `values`: of the copy of its code on the hook.
    pub(crate) fn give_picture(&self, id: &str, text: String, keys: Option<Vec<String>>, values: Vec<f32>) {
        let mut working = self.working();
        let on_it = |p: &&mut OnHook| p.id == id && (p.feature.is_none() || p.hooks & Hook::Picker.bit() != 0);
        if let Some(plugin) = working.on.iter_mut().find(on_it) {
            plugin.picture = Some(Picture { text: text.into(), keys: keys.map(Into::into), values });
        }
    }

    /// The picker hook's plugin (the first on it that draws Classic now): its id and its picture.
    pub(crate) fn picker(&self) -> Option<(String, Picture)> {
        if !self.has(Hook::Picker) {
            return None;
        }
        let working = self.working();
        let mut on = working.on.iter().filter(|p| p.hooks & Hook::Picker.bit() != 0);
        // (one that stops saying is taken off the next time the hook goes through it)
        let on = on.find(|p| at_work(&p.running).picker_on().unwrap_or(false))?;
        Some((on.id.clone(), on.picture.clone()?))
    }

    /// What the picker hook's plugin (the first on it that draws Classic now) answers to `ask`.
    fn picker_asks<T: Copy>(&self, mut ask: impl FnMut(&mut Running) -> Result<Option<T>, Stop>) -> Option<T> {
        if !self.has(Hook::Picker) {
            return None;
        }
        let said = self.through(Hook::Picker, None, |running, said: Option<Option<T>>| {
            if said.is_some() || !running.picker_on()? {
                return Ok(said);
            }
            ask(running).map(Some)
        });
        said.flatten()
    }

    /// The picker hook: the saturation and brightness a point of the picker picks (0..1, y down).
    pub(crate) fn picker_pick(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        self.picker_asks(|running| running.picker_pick(x, y))
    }

    /// The picker hook: where saturation `s` and brightness `v` sit on the picker (0..1, y down).
    pub(crate) fn picker_point(&self, s: f32, v: f32) -> Option<(f32, f32)> {
        self.picker_asks(|running| running.picker_point(s, v))
    }

    /// The picker hook: the hue (0..1) a touch starting at a point of the picker picks; none where
    /// it picks saturation and brightness, or if the picker leaves the hue to the app's slider.
    pub(crate) fn picker_hue(&self, x: f32, y: f32) -> Option<f32> {
        self.picker_asks(|running| running.picker_hue(x, y))
    }

    /// The picker hook: where hue `h`'s knob sits on the picker (0..1, y down); none while it leaves
    /// the hue to the app's slider.
    pub(crate) fn picker_hue_point(&self, h: f32) -> Option<(f32, f32)> {
        self.picker_asks(|running| running.picker_hue_point(h))
    }

    /// The part scale hook: how big part `part` of the interface shows next to the rest of it,
    /// each plugin on it scaling it in turn, .5 to 2 in all.
    pub(crate) fn interface_part_scale(&self, part: i32) -> f32 {
        if !self.has(Hook::InterfacePartScale) {
            return 1.0;
        }
        let said =
            self.through(Hook::InterfacePartScale, 1.0, |running, at| Ok(at * running.interface_part_scale(part)?));
        said.clamp(0.5, 2.0)
    }

    /// The floating picker hook: whether the floating colour window shows Classic's picker alone.
    pub(crate) fn floating_picker_only(&self) -> bool {
        self.has(Hook::FloatingPickerOnly)
            && self.through(Hook::FloatingPickerOnly, false, |running, said| Ok(said | running.floating_picker_only()?))
    }

    /// The notices hook: whether the notice of something done, of kind `kind` (by
    /// [`crate::plugins::NoticeKind`]), goes by unseen, as any plugin on it says.
    pub(crate) fn hide_notice(&self, kind: i32) -> bool {
        self.has(Hook::HideNotices)
            && self.through(Hook::HideNotices, false, |running, said| Ok(said || running.hide_notice(kind)?))
    }

    /// The floating panels hook: whether panel `panel` (by [`crate::floating::FloatingPanel`]) can be
    /// torn off into a window of its own, as any plugin on it says.
    pub(crate) fn floating_panel(&self, panel: i32) -> bool {
        self.has(Hook::FloatingPanels)
            && self.through(Hook::FloatingPanels, false, |running, said| Ok(said || running.floating_panel(panel)?))
    }

    /// The gesture lock's hook: whether a two-finger gesture leaves part `part` of the view (by
    /// [`GESTURE_TURN`] and [`GESTURE_FIT`]) as it is, as any plugin on it says.
    pub(crate) fn gesture_lock(&self, part: i32) -> bool {
        self.has(Hook::GestureLock)
            && self.through(Hook::GestureLock, false, |running, said| Ok(said || running.gesture_lock(part)?))
    }

    /// The view look's hook: the matrix the canvas's colours go through on the screen, each
    /// plugin's in turn (in order of id); none while together they change nothing.
    pub(crate) fn view_look(&self) -> Option<crate::ViewLook> {
        if !self.has(Hook::ViewLook) {
            return None;
        }
        let look = self.through(Hook::ViewLook, crate::view_look::IDENTITY, |running, look| {
            Ok(running.view_look()?.map_or(look, |next| crate::view_look::then(&look, &next)))
        });
        (look != crate::view_look::IDENTITY).then_some(look)
    }

    /// The paper's hook: the paper the first plugin on it (by id) that gives one now gives, and
    /// whose it is (a picture of its controls' is that plugin's).
    pub(crate) fn paper(&self) -> Option<(String, crate::plugin_run::Paper)> {
        if !self.has(Hook::Paper) {
            return None;
        }
        let mut given = None;
        self.through_by_id(Hook::Paper, (), |id, running, ()| {
            if given.is_none() {
                given = running.paper()?.map(|paper| (id.to_owned(), paper));
            }
            Ok(())
        });
        given
    }

    /// The reference windows' hook: how many Reference windows may be open at once, the most any
    /// plugin on it says (1 with none).
    pub(crate) fn reference_windows(&self) -> u32 {
        if !self.has(Hook::ReferenceWindows) {
            return 1;
        }
        self.through(Hook::ReferenceWindows, 1, |running, most| Ok(most.max(running.reference_windows()?)))
    }

    /// The pressure hook: the pressure a stroke's sample draws with, as the plugins on it say in
    /// turn (in order of id), each told what the last one said; none while none says, or for a
    /// sample that isn't a number.
    #[inline]
    pub(crate) fn pressure(&self, asked: crate::plugin_run::PressureAsked) -> Option<f32> {
        let numbers = [asked.x, asked.y, asked.ms, asked.pressure].iter().all(|v| v.is_finite());
        if !self.has(Hook::Pressure) || !numbers {
            return None;
        }
        self.through(Hook::Pressure, None, |running, said: Option<f32>| {
            let told = crate::plugin_run::PressureAsked { pressure: said.unwrap_or(asked.pressure), ..asked };
            Ok(running.pressure(told)?.or(said))
        })
    }

    /// The pressure hook's tapers for a stroke by `pointer` (numbered as for [`Hooks::pressure`]):
    /// how far (window points) its pressure grows from nothing at its start and falls to nothing at
    /// its end, each the longest any plugin on it says.
    pub(crate) fn pressure_tapers(&self, pointer: i32) -> [f32; 2] {
        if !self.has(Hook::Pressure) {
            return [0.0; 2];
        }
        self.through(Hook::Pressure, [0.0; 2], |running, [start, end]| {
            Ok([start.max(running.pressure_taper(pointer, 0)?), end.max(running.pressure_taper(pointer, 1)?)])
        })
    }

    /// The brush preview's hook: the colour (0xRRGGBB) the brush library draws a brush's sample
    /// stroke in, the current colour `current` (0xRRGGBB) and the brush the one in use if `chosen`,
    /// as the first plugin by id that answers says; none for the app's own.
    pub(crate) fn brush_preview_colour(&self, current: u32, chosen: bool) -> Option<u32> {
        if !self.has(Hook::BrushPreviewColour) {
            return None;
        }
        self.through(Hook::BrushPreviewColour, None, |running, said| match said {
            Some(colour) => Ok(Some(colour)),
            None => running.brush_preview_colour(current, chosen),
        })
    }

    /// The slider look's hook: what the first plugin by id on it that answers says of `what` of the
    /// side bar's slider tracks' look (0 their opacity, 1 how frosted what is behind them is), 0 to
    /// 1; none for the app's own.
    pub(crate) fn slider_look(&self, what: i32) -> Option<f32> {
        if !self.has(Hook::SliderLook) {
            return None;
        }
        self.through(Hook::SliderLook, None, |running, said| match said {
            Some(v) => Ok(Some(v)),
            None => running.slider_look(what),
        })
    }

    /// The panel layout's hook (interface 20): what the first plugin by id on it that answers says of
    /// `part` ([`LAYOUT_SNAP`] and on); none for the app's own.
    pub(crate) fn panel_layout(&self, part: i32) -> Option<f32> {
        if !self.has(Hook::PanelLayout) {
            return None;
        }
        self.through(Hook::PanelLayout, None, |running, said| match said {
            Some(v) => Ok(Some(v)),
            None => running.panel_layout(part),
        })
    }

    /// The dimming hook (interface 13): how much the layers but the current one are dimmed on the
    /// screen alone, the most any plugin on it says (0 for none).
    pub(crate) fn view_dim_others(&self) -> f32 {
        if !self.has(Hook::ViewDimOthers) {
            return 0.0;
        }
        self.through(Hook::ViewDimOthers, 0.0_f32, |running, most| Ok(most.max(running.view_dim_others()?)))
    }

    /// The colour panel's hook (interface 12): whether a plugin on it says the panel does `part` of
    /// what it may ([`COLOUR_STEPS`], [`COLOUR_BEFORE`]).
    pub(crate) fn colour_panel(&self, part: i32) -> bool {
        self.has(Hook::ColourPanel)
            && self.through(Hook::ColourPanel, false, |running, said| Ok(said || running.colour_panel(part)?))
    }

    /// The surround picture's hook: the plugin (by id) and the place in its controls of the file
    /// control whose picture shows round the canvas, as the first plugin by id on it that answers
    /// says; none for none.
    pub(crate) fn surround_picture(&self) -> Option<(String, u32)> {
        if !self.has(Hook::SurroundPicture) {
            return None;
        }
        let mut whose = None;
        self.through_by_id(Hook::SurroundPicture, false, |id, running, found| {
            if found {
                return Ok(true);
            }
            let said = running.surround_picture()?;
            if let Some(at) = said {
                whose = Some((id.to_owned(), at));
            }
            Ok(said.is_some())
        });
        whose
    }

    /// The folder hooks (interface 22: [`Hook::ArtworkSaved`], [`Hook::VideoExported`],
    /// [`Hook::GalleryOpened`]): whose `folder` control, by its place in that plugin's controls, the
    /// first plugin by id on `hook` that names one names; none for none.
    pub(crate) fn folder(&self, hook: Hook) -> Option<(String, u32)> {
        if !self.has(hook) {
            return None;
        }
        let mut whose = None;
        self.through_by_id(hook, false, |id, running, found| {
            if found {
                return Ok(true);
            }
            let said = running.folder(hook)?;
            if let Some(at) = said {
                whose = Some((id.to_owned(), at));
            }
            Ok(said.is_some())
        });
        whose
    }

    /// The layer label's hook: the colour (0xRRGGBB) the row of a layer keeping `marks` (by
    /// plugin id) shows, as the first plugin by id on it that keeps a mark there says; none for none.
    /// Each is told only its own mark.
    pub(crate) fn layer_label(&self, marks: &std::collections::BTreeMap<String, i64>) -> Option<u32> {
        if marks.is_empty() || !self.has(Hook::LayerLabel) {
            return None;
        }
        // each told the mark kept under the id its marks go by
        let by: Vec<(String, String)> = self.working().on.iter().map(|p| (p.id.clone(), p.marks.clone())).collect();
        let marks_of = |id: &str| by.iter().find(|(of, _)| of == id).and_then(|(_, by)| marks.get(by));
        self.through_by_id(Hook::LayerLabel, None, |id, running, said| match (said, marks_of(id)) {
            (Some(colour), _) => Ok(Some(colour)),
            (None, Some(&mark)) if mark != 0 => running.layer_label(mark),
            _ => Ok(None),
        })
    }

    /// The stroke-fill hook: whether a stroke beginning with `tool` (0 paint, 1 erase) by `pointer`
    /// (numbered as for [`Hooks::pressure`]) fills the region its path encloses, as the first
    /// plugin on it by id that says so says (1 shown filling as it is drawn, 2 filled as it ends),
    /// each told the brush first, with the values of its own rows (`brush_of` its keys); 0 while
    /// none says.
    pub(crate) fn stroke_fill(
        &self,
        tool: i32,
        pointer: i32,
        brush_of: impl Fn(&[String]) -> crate::stroke_sound::SoundBrush,
    ) -> u8 {
        if !self.has(Hook::StrokeFill) {
            return 0;
        }
        let mut working = self.working();
        let mut i = 0;
        let mut said = 0;
        while let Some(plugin) = working.on.get_mut(i) {
            if plugin.hooks & Hook::StrokeFill.bit() == 0 {
                i += 1;
                continue;
            }
            let brush = brush_of(&plugin.brush_keys);
            let answer = at_work(&plugin.running).stroke_fill(&brush, tool, pointer);
            match answer {
                Ok(fill) => {
                    said = fill;
                    if said != 0 {
                        break;
                    }
                    i += 1;
                }
                Err(stop) => i = self.stop_at(&mut working, i, stop),
            }
        }
        self.rehook(&working);
        said
    }

    /// The kinds of layer `id` adds, in its manifest's order.
    pub(crate) fn layer_kinds(&self, id: &str, kinds: Vec<crate::PluginKind>) {
        if let Some(plugin) = self.working().on.iter_mut().find(|p| p.id == id) {
            plugin.layer_kinds = kinds;
        }
    }

    /// What a plugin on calls layers of `kind` (`<id>.<key>`), if one adds them.
    pub(crate) fn layer_kind_label(&self, kind: &str) -> Option<String> {
        if !self.has(Hook::Layers) {
            return None;
        }
        let working = self.working();
        working.on.iter().find_map(|p| p.layer_kinds.iter().find(|k| k.key == kind).map(|k| k.label.clone()))
    }

    /// What layers of `kind` can be exported as by the plugin on that adds them.
    pub(crate) fn layer_kind_exports(&self, kind: &str) -> Vec<crate::PluginExport> {
        if !self.has(Hook::Layers) {
            return Vec::new();
        }
        let working = self.working();
        working
            .on
            .iter()
            .find_map(|p| p.layer_kinds.iter().find(|k| k.key == kind).map(|k| k.exports.clone()))
            .unwrap_or_default()
    }

    /// What layers of `kind` are known by on their thumbnails, if a plugin on adds them and draws it.
    pub(crate) fn layer_kind_badge(&self, kind: &str) -> Option<crate::PluginBadge> {
        if !self.has(Hook::Layers) {
            return None;
        }
        let working = self.working();
        working.on.iter().find_map(|p| p.layer_kinds.iter().find(|k| k.key == kind).and_then(|k| k.badge.clone()))
    }

    /// `work` done by the plugin on that adds layers of `kind`, handed its code and the kind's place
    /// among its kinds: none if no plugin on adds them, or it stopped (it goes off, and says why).
    pub(crate) fn on_layer<T>(
        &self,
        kind: &str,
        work: impl FnOnce(&mut Running, usize) -> Result<T, Stop>,
    ) -> Option<T> {
        if !self.has(Hook::Layers) {
            return None;
        }
        let (id, running, index) = {
            let working = self.working();
            working.on.iter().find_map(|p| {
                let index = p.layer_kinds.iter().position(|k| k.key == kind)?;
                Some((p.id.clone(), Arc::clone(&p.running), index))
            })?
        };
        let done = work(&mut at_work(&running), index);
        done.inspect_err(|stop| self.stopped(&id, *stop)).ok()
    }

    /// The brush engines `id` has, in its manifest's order.
    pub(crate) fn brush_engines(&self, id: &str, engines: Vec<crate::plugin_brushes::BrushEngine>) {
        if let Some(plugin) = self.working().on.iter_mut().find(|p| p.id == id) {
            plugin.brush_engines = engines;
        }
    }

    /// How a copy of `id`'s code of its own starts, for its brush engines' imports.
    pub(crate) fn give_copier(&self, id: &str, copier: Copier) {
        if let Some(plugin) = self.working().on.iter_mut().find(|p| p.id == id) {
            plugin.copier = Some(copier);
        }
    }

    /// How a copy of `id`'s code of its own starts, if it is on and has one.
    pub(crate) fn copier(&self, id: &str) -> Option<Copier> {
        self.working().on.iter().find(|p| p.id == id)?.copier.clone()
    }

    /// The brush engines on that read files whose names end `ending` (all of them for none): each
    /// plugin's id, code, the engine's place among its engines and its key.
    pub(crate) fn engines_reading(&self, ending: Option<&str>) -> Vec<(String, Shared, usize, String)> {
        if !self.has(Hook::Brushes) {
            return Vec::new();
        }
        let working = self.working();
        let mut found = Vec::new();
        for p in &working.on {
            for (index, engine) in p.brush_engines.iter().enumerate() {
                if ending.is_none_or(|e| engine.accepts.iter().any(|a| a == e)) {
                    found.push((p.id.clone(), Arc::clone(&p.running), index, engine.key.clone()));
                }
            }
        }
        found
    }

    /// What brush engine `key` (`<id>.<key>`) is called, if its plugin is on.
    pub(crate) fn engine_label(&self, key: &str) -> Option<String> {
        let working = self.working();
        working.on.iter().find_map(|p| p.brush_engines.iter().find(|e| e.key == key).map(|e| e.label.clone()))
    }

    /// The brush engine `key` (`<id>.<key>`), if its plugin is on: its plugin's id, code and the
    /// engine's place among its engines.
    pub(crate) fn engine(&self, key: &str) -> Option<(String, Shared, usize)> {
        if !self.has(Hook::Brushes) {
            return None;
        }
        let working = self.working();
        working.on.iter().find_map(|p| {
            let index = p.brush_engines.iter().position(|e| e.key == key)?;
            Some((p.id.clone(), Arc::clone(&p.running), index))
        })
    }

    /// The action that is engine `key`'s brushes' studio, if its plugin is on and has one.
    pub(crate) fn engine_studio(&self, key: &str) -> Option<String> {
        if !self.has(Hook::Brushes) {
            return None;
        }
        let working = self.working();
        working.on.iter().find_map(|p| p.brush_engines.iter().find(|e| e.key == key)?.studio.clone())
    }

    /// The look engine `key` gave the brush whose data is `id`, if it is kept.
    pub(crate) fn look_kept(&self, key: &str, id: u64) -> Option<Arc<crate::plugin_brushes::BrushLook>> {
        let mut working = self.working();
        let plugin = working.on.iter_mut().find(|p| p.brush_engines.iter().any(|e| e.key == key))?;
        let at = plugin.looks.iter().position(|(k, i, _)| k == key && *i == id)?;
        // the latest last
        let kept = plugin.looks.remove(at);
        let look = Arc::clone(&kept.2);
        plugin.looks.push(kept);
        Some(look)
    }

    /// The look engine `key` gave the brush whose data is `id`, kept while its plugin stays on (the
    /// last few).
    /// Engine `key` couldn't give the brush whose data is `id` a look the app takes: it isn't asked
    /// again while its plugin stays on.
    pub(crate) fn no_look(&self, key: &str, id: u64) {
        let mut working = self.working();
        if let Some(plugin) = working.on.iter_mut().find(|p| p.brush_engines.iter().any(|e| e.key == key)) {
            plugin.no_looks.push((key.to_owned(), id));
            let over = plugin.no_looks.len().saturating_sub(MOST_NO_LOOKS);
            plugin.no_looks.drain(..over);
        }
    }

    /// The look engine `key` is making of the brush whose data is `id`, to wait on, and whether this
    /// ask is the one that starts it (none if the engine's plugin is off).
    pub(crate) fn making_look(&self, key: &str, id: u64) -> Option<(Arc<crate::plugin_brushes::Making>, bool)> {
        let mut working = self.working();
        let plugin = working.on.iter_mut().find(|p| p.brush_engines.iter().any(|e| e.key == key))?;
        if let Some((_, _, making)) = plugin.making.iter().find(|(k, i, _)| k == key && *i == id) {
            return Some((Arc::clone(making), false));
        }
        let making = Arc::new(crate::plugin_brushes::Making::default());
        plugin.making.push((key.to_owned(), id, Arc::clone(&making)));
        Some((making, true))
    }

    /// Engine `key`'s look of the brush whose data is `id` is no longer being made (kept, or not had).
    pub(crate) fn made_look(&self, key: &str, id: u64) {
        let mut working = self.working();
        for plugin in &mut working.on {
            plugin.making.retain(|(k, i, _)| !(k == key && *i == id));
        }
    }

    /// Whether engine `key` couldn't give the brush whose data is `id` a look.
    pub(crate) fn has_no_look(&self, key: &str, id: u64) -> bool {
        let working = self.working();
        working.on.iter().any(|p| p.no_looks.iter().any(|(k, i)| k == key && *i == id))
    }

    pub(crate) fn keep_look(&self, key: &str, id: u64, look: Arc<crate::plugin_brushes::BrushLook>) {
        let mut working = self.working();
        if let Some(plugin) = working.on.iter_mut().find(|p| p.brush_engines.iter().any(|e| e.key == key)) {
            plugin.looks.retain(|(k, i, _)| !(k == key && *i == id));
            plugin.looks.push((key.to_owned(), id, look));
            let over = plugin.looks.len().saturating_sub(crate::plugin_brushes::LOOKS_KEPT);
            plugin.looks.drain(..over);
        }
    }

    /// The look engine `key` of plugin `id` gave the brush whose data is `data`, as it was kept on
    /// the disk.
    pub(crate) fn look_on_disk(&self, id: &str, key: &str, data: u64) -> Option<Vec<u8>> {
        plugin_file::read_look(self.store.as_deref()?, id, key, data)
    }

    /// Keeps `look` on the disk with plugin `id`, as engine `key`'s for the brush whose data is
    /// `data`.
    pub(crate) fn keep_look_on_disk(&self, id: &str, key: &str, data: u64, look: &[u8]) {
        if let Some(store) = &self.store {
            plugin_file::write_look(store, id, key, data, look);
        }
    }

    /// The keys of `id`'s rows in the brush studio, in their order: each copy of its code's.
    pub(crate) fn brush_keys(&self, id: &str, keys: Vec<String>) {
        for plugin in self.working().on.iter_mut().filter(|p| p.id == id) {
            plugin.brush_keys.clone_from(&keys);
        }
    }

    /// The file picked for `id`'s control `index` (none: let go) handed to it: a long call. Of a
    /// plugin of features (interface 16), only to the copy of `feature`'s, the one the control comes
    /// under: what one feature's files take is never another's memory. One that stops is off its
    /// hooks.
    pub(crate) fn give_file(&self, id: &str, feature: Option<usize>, index: usize, bytes: Option<&[u8]>) {
        let found: Vec<Shared> = {
            let working = self.working();
            let theirs = working.on.iter().filter(|p| p.id == id && (p.feature.is_none() || p.feature == feature));
            theirs.map(|p| Arc::clone(&p.running)).collect()
        };
        for running in found {
            if let Err(stop) = at_work(&running).give_file(index, bytes) {
                self.stopped(id, stop);
                return;
            }
        }
    }

    /// Takes `id` off: from the next sample on, the app is as if it had never been.
    pub(crate) fn take_off(&self, id: &str) {
        let mut working = self.working();
        working.on.retain(|p| p.id != id);
        self.rehook(&working);
        self.changed();
    }

    /// Takes the copy of `id`'s code for `feature` off (interface 16): the feature is off, and on
    /// none of its hooks from the next sample on.
    pub(crate) fn take_off_feature(&self, id: &str, feature: usize) {
        let mut working = self.working();
        working.on.retain(|p| p.id != id || p.feature != Some(feature));
        self.rehook(&working);
        self.changed();
    }

    /// Whether the copy of `id`'s code for `feature` is on.
    pub(crate) fn feature_on(&self, id: &str, feature: usize) -> bool {
        self.working().on.iter().any(|p| p.id == id && p.feature == Some(feature))
    }

    /// Tells `id`, if it is on, control `index` has `value`: each copy of its code, a copy of a
    /// plugin of features told another feature's switch as off.
    pub(crate) fn control(&self, id: &str, index: usize, value: f32) {
        self.changed();
        {
            let mut working = self.working();
            let mut stopped = None;
            for (at, plugin) in working.on.iter_mut().enumerate().filter(|(_, p)| p.id == id) {
                let others = plugin.features.contains(&index) && plugin.feature != Some(index);
                let told = if others { 0.0 } else { value };
                // what its picture is drawn with
                if let Some(kept) = plugin.picture.as_mut().and_then(|p| p.values.get_mut(index)) {
                    *kept = told;
                }
                // its events' copy too
                if let Some(events) = plugin.events.as_ref() {
                    let _ = at_work(events).control(index, told);
                }
                if let Err(stop) = at_work(&plugin.running).control(index, told) {
                    stopped = Some((at, stop));
                    break;
                }
            }
            if let Some((at, stop)) = stopped {
                self.stop_at(&mut working, at, stop);
                self.rehook(&working);
            }
        }
        // its artworks open too, once the list is let go (a canvas telling of a stop takes it)
        for canvas in self.canvases_of(id) {
            canvas.control(index, value);
        }
    }

    /// `canvas`, an artwork of `id`'s open, told `id`'s controls as they change from now on.
    pub(crate) fn keep_canvas(&self, id: &str, canvas: &Arc<crate::PluginCanvas>) {
        let mut canvases = self.canvases.lock().unwrap_or_else(|e| e.into_inner());
        canvases.retain(|(_, canvas)| canvas.strong_count() > 0);
        canvases.push((id.to_owned(), Arc::downgrade(canvas)));
    }

    /// Writes down that `id` couldn't start, as if it had stopped.
    pub(crate) fn failed(&self, id: &str, stop: Stop) {
        self.note_stop(&mut self.working(), id.to_owned(), stop);
    }

    /// What there is for the app to tell, once.
    pub(crate) fn take_said(&self) -> Option<String> {
        self.working().untold.take()
    }

    /// What stopped since this was last asked.
    pub(crate) fn take_stopped(&self) -> Vec<(String, Stop)> {
        std::mem::take(&mut self.working().stopped)
    }
}

impl CanvasSession {
    /// The values of plugins' controls this artwork keeps, by their keys.
    pub(crate) fn plugin_values(&self) -> std::collections::BTreeMap<String, f32> {
        self.shared.lock().plugin_values.clone()
    }

    /// `value` is what the artwork keeps for the control kept under `key` from now on, saved with
    /// it (a change being an edit, the autosave has it).
    pub(crate) fn keep_plugin_value(&self, key: String, value: f32) {
        let mut st = self.shared.lock();
        if value.is_finite() && st.plugin_values.get(&key) != Some(&value) {
            st.plugin_values.insert(key, value);
            st.revision += 1;
        }
    }
}

#[uniffi::export]
impl CanvasSession {
    /// Strokes go through the plugins on in `plugins`, as they come and go, from here on.
    pub fn use_plugins(&self, plugins: Arc<Plugins>) {
        // the controls kept with this artwork, as it keeps them, the first time it comes
        if let Some(me) = self.me.upgrade() {
            plugins.take_up_artwork(&me);
        }
        let hooks = plugins.hooks();
        // where the brush cursor shows goes through them too, as a stroke's samples do
        *self.shared.cursor.hooks.lock() = Some(Arc::clone(&hooks));
        // the filter layers they offer, their code read now rather than as it is drawn
        let layer_filters = plugins.layer_filter_defs();
        // how the plugins have the window show the canvas, and the paper they give, asked now
        // rather than as it is drawn
        let look = hooks.view_look();
        let dim = hooks.view_dim_others();
        let sets = plugins.brush_sets();
        // the colour round the canvas and the picture there, read again only as it changes
        let surround =
            hooks.interface_colour(SURROUND_ROLE).map(|rgb| [16, 8, 0].map(|s| ((rgb >> s) & 0xFF) as f32 / 255.0));
        let kept = self.shared.lock().surround_picture.clone();
        let picture = crate::surround::picture_now(plugins.surround_picture_file(), kept);
        // a paper's picture made its tile, again only as the file changes
        let kept_paper = self.shared.lock().paper_picture.clone();
        let mut paper_picture = None;
        let paper = hooks.paper().and_then(|(id, p)| {
            let tile = match p.picture {
                Some(at) => {
                    let made = crate::paper_picture::tile_now(plugins.picture_file(&id, at), kept_paper)?;
                    let tile = made.tile.clone();
                    paper_picture = Some(made);
                    tile
                }
                None => p.tile,
            };
            Some(procraft_gpu::CanvasPaper {
                tile,
                strength: p.strength,
                scale: p.scale,
                depth: p.depth,
                tooth: p.tooth,
                colour: p.colour.map(|rgb| [16, 8, 0].map(|shift| ((rgb >> shift) & 0xFF) as f32 / 255.0)),
            })
        });
        // each save told to the folders the plugins name (interface 22)
        *self.shared.folders.lock().unwrap_or_else(|e| e.into_inner()) = Some(plugins.folders());
        let mut st = self.shared.lock();
        st.paper_picture = paper_picture;
        st.offer_layer_filters(layer_filters);
        st.hooks = Some(hooks);
        st.plugin_sets = Some(sets);
        let mut changed = false;
        if st.plugin_look != look {
            st.plugin_look = look;
            changed = true;
        }
        if st.plugin_dim != dim {
            st.plugin_dim = dim;
            changed = true;
        }
        if st.plugin_surround != surround {
            st.plugin_surround = surround;
            changed = true;
        }
        let id = |p: &Option<Arc<crate::surround::SurroundPicture>>| p.as_ref().map(|p| p.id);
        if id(&st.surround_picture) != id(&picture) {
            st.surround_picture = picture;
            changed = true;
        }
        if st.plugin_paper != paper {
            st.plugin_paper = paper;
            st.apply_paper();
            // the background changes with it: the Reference windows read it again
            st.reference_canvas = None;
            changed = true;
        }
        if changed {
            self.shared.frame(&mut st);
        }
        drop(st);
        // the brushes they come with, into the library once it is open
        self.give_plugin_sets();
    }

    /// The picker plugin's picture for `hue` (0..1), `side` pixels square: premultiplied RGBA rows,
    /// drawn on the GPU (it is compiled once). None with no picker plugin on or no GPU yet, or if it
    /// couldn't be drawn: one that runs away or takes too long goes off, and says why.
    pub fn picker_picture(&self, hue: f32, side: u32) -> Option<Vec<u8>> {
        let (hooks, gpu, cached) = {
            let st = self.shared.lock();
            (st.hooks.clone()?, st.gpu.clone()?, st.picker_program.clone())
        };
        let (id, picture) = hooks.picker()?;
        let crate::hooks::Picture { text, keys, values } = picture;
        let program = match cached.filter(|(was, code, read_by, _)| *was == id && *code == text && *read_by == keys) {
            Some((_, _, _, program)) => program,
            None => {
                let code = match &keys {
                    Some(keys) => procraft_gpu::read_picture_given(&text, keys),
                    None => procraft_gpu::read_picture(&text),
                };
                let code = code.map_err(|why| log::warn!("a picture was refused: {why}"));
                let made = code.ok().and_then(|code| gpu.make_picture(code).ok());
                let Some(program) = made else {
                    hooks.stopped(&id, Stop::Damaged);
                    return None;
                };
                self.shared.lock().picker_program = Some((id.clone(), Arc::clone(&text), keys, Arc::clone(&program)));
                program
            }
        };
        match gpu.draw_picture(&program, hue, side, &values) {
            Ok(rgba) => Some(rgba),
            // asked bigger than it can be drawn, or the device gone: not the plugin's doing
            Err(procraft_gpu::PictureStop::TooBig | procraft_gpu::PictureStop::Lost) => None,
            Err(procraft_gpu::PictureStop::Spent | procraft_gpu::PictureStop::Cut) => {
                hooks.stopped(&id, Stop::TooSlow);
                None
            }
        }
    }

    /// The notice an undo (`undo`) or a redo of the step `name` shows: "Undo Brush stroke", or
    /// "Redo" for a step with no name; none while a plugin hides it (the undo notice's hook, or the
    /// notices' for what undo and redo did).
    pub fn step_notice(&self, undo: bool, name: Option<String>) -> Option<String> {
        let hooks = self.shared.lock().hooks.clone();
        let step = crate::plugins::NoticeKind::Step as i32;
        if hooks.is_some_and(|hooks| hooks.hide_undo_notice() || hooks.hide_notice(step)) {
            return None;
        }
        let word = if undo { "Undo" } else { "Redo" };
        Some(name.map_or_else(|| word.to_owned(), |name| format!("{word} {name}")))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Instant;

    use procraft_engine::view::View;
    use procraft_gpu::Gpu;
    use serde_json::{Value, json};

    use super::*;
    use crate::plugin_file::tests::{coded, pack};
    use crate::plugin_run::tests::{ACTS, FILL_AT, OFFSET, TAKES_MOST, fill_module, position_module};
    use crate::plugin_run::{MODULE, Running};
    use crate::{PenSample, PointerKind};

    const ID: &str = "org.example.offset";
    /// Turns a row to its brightness, as the app used to.
    const PASTES: &str = r#"(module
        (memory (export "memory") 2)
        (global $on (mut i32) (i32.const 0))
        (func (export "procraft_control") (param i32 f32)
            (global.set $on (f32.ne (local.get 1) (f32.const 0))))
        (func (export "procraft_paste_mask") (result i32) (global.get $on))
        (func (export "procraft_row_buffer") (param i32) (result i32) (i32.const 65536))
        (func (export "procraft_paste_row") (param $at i32) (param $width i32)
            (local $end i32) (local $p i32) (local $grey i32)
            (local.set $end (i32.add (local.get $at) (i32.shl (local.get $width) (i32.const 2))))
            (block $done (loop $next
                (br_if $done (i32.ge_u (local.get $at) (local.get $end)))
                (local.set $p (i32.load (local.get $at)))
                (local.set $grey (i32.shr_u (i32.add (i32.add (i32.add
                    (i32.mul (i32.and (local.get $p) (i32.const 255)) (i32.const 54))
                    (i32.mul (i32.and (i32.shr_u (local.get $p) (i32.const 8)) (i32.const 255)) (i32.const 183)))
                    (i32.mul (i32.and (i32.shr_u (local.get $p) (i32.const 16)) (i32.const 255)) (i32.const 19)))
                    (i32.const 128)) (i32.const 8)))
                (i32.store (local.get $at)
                    (i32.or (i32.const 0xff000000) (i32.mul (local.get $grey) (i32.const 0x010101))))
                (local.set $at (i32.add (local.get $at) (i32.const 4)))
                (br $next)))))"#;
    /// Answers the scale hook by its one slider.
    const SCALES: &str = r#"(module
        (global $size (mut f32) (f32.const 1))
        (func (export "procraft_control") (param i32 f32) (global.set $size (local.get 1)))
        (func (export "procraft_interface_scale") (param f32 f32) (result f32) (global.get $size)))"#;
    /// Answers the clear hook by its one switch.
    const CLEARS: &str = r#"(module
        (global $clear (mut i32) (i32.const 0))
        (func (export "procraft_control") (param i32 f32)
            (global.set $clear (f32.ne (local.get 1) (f32.const 0))))
        (func (export "procraft_keep_clear") (result i32) (global.get $clear)))"#;
    /// Answers the pressure hook: a finger's sample (pointer 0) pressed at its x over 100, anything
    /// else left as it came; a finger's stroke tapered over 10 points at its start and 20 at its end.
    const PRESSES: &str = r#"(module
        (func (export "procraft_pressure") (param i32 i32 f32 f32 f32 f32) (result f32)
            (if (result f32) (i32.eqz (local.get 0))
                (then (f32.div (local.get 2) (f32.const 100)))
                (else (f32.const -1))))
        (func (export "procraft_pressure_taper") (param i32 i32) (result f32)
            (if (result f32) (i32.eqz (local.get 0))
                (then (f32.convert_i32_s (i32.mul (i32.add (local.get 1) (i32.const 1)) (i32.const 10))))
                (else (f32.const 0)))))"#;

    /// Answers the pressure hook with half the pressure it is told, for every pointer, and a taper
    /// of 5 points at the start and 30 at the end.
    const PRESSES_HALF: &str = r#"(module
        (func (export "procraft_pressure") (param i32 i32 f32 f32 f32 f32) (result f32)
            (f32.mul (local.get 5) (f32.const 0.5)))
        (func (export "procraft_pressure_taper") (param i32 i32) (result f32)
            (select (f32.const 30) (f32.const 5) (local.get 1))))"#;

    /// A plugin on the pressure hook that answers `pressure` and `taper` as given (WAT bodies).
    fn pressure_module(pressure: &str, taper: &str) -> Vec<u8> {
        let text = format!(
            r#"(module
                (func (export "procraft_pressure") (param i32 i32 f32 f32 f32 f32) (result f32) {pressure})
                (func (export "procraft_pressure_taper") (param i32 i32) (result f32) {taper}))"#
        );
        wat::parse_str(text).unwrap()
    }

    /// Answers the pick hook by its one switch.
    const PICKS: &str = r#"(module
        (global $layer (mut i32) (i32.const 0))
        (func (export "procraft_control") (param i32 f32)
            (global.set $layer (f32.ne (local.get 1) (f32.const 0))))
        (func (export "procraft_pick_layer") (result i32) (global.get $layer)))"#;

    fn store(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("procraft-hooks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.to_string_lossy().into_owned()
    }

    fn file(manifest: &Value, module: &[u8]) -> Vec<u8> {
        pack(manifest, &[(MODULE, module)])
    }

    fn offset() -> Vec<u8> {
        wat::parse_str(OFFSET).unwrap()
    }

    fn offset_manifest() -> Value {
        let mut manifest = coded(ID, &offset());
        manifest["controls"] = json!([
            { "key": "x", "type": "slider", "label": "X", "least": -100, "most": 100, "step": 1, "starts": 0 },
            { "key": "y", "type": "slider", "label": "Y", "least": -100, "most": 100, "step": 1, "starts": 0 },
        ]);
        manifest
    }

    /// A list holding the offset plugin, in `dir`.
    fn with_offset(dir: &str) -> Arc<Plugins> {
        let plugins = Plugins::with_store(dir.to_owned());
        assert_eq!(plugins.install(file(&offset_manifest(), &offset())), None);
        plugins
    }

    /// A plugin of no controls whose `procraft_position` does `body`, imported and on.
    fn turned_on(dir: &str, before: &str, body: &str) -> Arc<Plugins> {
        let module = position_module(before, body);
        let plugins = Plugins::with_store(dir.to_owned());
        assert_eq!(plugins.install(file(&coded(ID, &module), &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        plugins
    }

    fn card(plugins: &Plugins) -> crate::plugins::PluginInfo {
        plugins.plugins().remove(0)
    }

    /// A list as the next start reads it back.
    fn restarted(dir: &str, plugins: &Plugins) -> Arc<Plugins> {
        let next = Plugins::with_store(dir.to_owned());
        for line in plugins.kept_lines() {
            let (key, value) = line.split_once(' ').unwrap();
            next.read_kept(key.to_owned(), value.to_owned());
        }
        next
    }

    #[test]
    fn the_plugins_that_come_with_the_app_and_only_answer_answer_from_their_own_code() {
        const ANSWERING: [&str; 5] =
            ["colour-tools", "edit-layout", "interface-look", "layer-toolbox", "radial-segments"];
        let dir = store("answering");
        let plugins = Plugins::with_store(dir.clone());
        let hooks = plugins.hooks();
        let id = |name: &str| format!("io.github.artcraftandroid.{name}");
        for (name, bytes) in crate::bundled::BUNDLED.iter().filter(|(name, _)| ANSWERING.contains(name)) {
            // each holds code of its own
            let package = crate::plugin_file::read_package(bytes).unwrap();
            assert!(package.file(MODULE).is_some(), "{name}");
            assert_eq!(plugins.install(bytes.to_vec()), None, "{name}");
        }
        let said = |hooks: &Hooks| {
            let yes = [
                hooks.pick_layer(),
                hooks.fill_all_layers(),
                hooks.hide_undo_notice(),
                hooks.opacity_together(),
                hooks.pin_panels(),
            ];
            (yes, hooks.radial_segments())
        };
        assert_eq!(said(&hooks), ([false; 5], None), "imported, they say nothing");
        for name in ANSWERING {
            assert!(plugins.set_on(id(name), true), "{name}");
        }
        // each waits for its feature and its switch, and radial symmetry makes eight until its
        // slider moves
        assert_eq!(said(&hooks), ([false; 5], Some(8)));
        for (name, feature) in
            [("colour-tools", "colordrop"), ("interface-look", "interface"), ("layer-toolbox", "layer_tools")]
        {
            assert!(plugins.set_switch(format!("{}.{feature}", id(name)), true), "{feature}");
        }
        assert_eq!(said(&hooks), ([false; 5], Some(8)));
        // (the eyedropper's kept under the setting the app had before it was a plugin)
        assert!(plugins.set_switch("eyedropper_layer".to_owned(), true));
        assert_eq!(said(&hooks), ([true, false, false, false, false], Some(8)));
        for (name, switch) in [
            ("colour-tools", "all_layers"),
            ("interface-look", "hide_undo"),
            ("layer-toolbox", "opacity"),
            ("edit-layout", "pin_panels"),
        ] {
            assert!(plugins.set_switch(format!("{}.{switch}", id(name)), true), "{switch}");
        }
        assert!(plugins.set_slider(format!("{}.parts", id("radial-segments")), 12.0).is_some());
        assert_eq!(said(&hooks), ([true; 5], Some(12)));
        for name in ANSWERING {
            assert!(plugins.set_on(id(name), false), "{name}");
        }
        assert_eq!(said(&hooks), ([false; 5], None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_nothing_applied_a_sample_goes_through_as_it_came() {
        let hooks = Hooks::default();
        assert_eq!(hooks.position(1.5, -2.5), (1.5, -2.5));
        // whatever it is, even what no plugin could hand on
        let (x, y) = hooks.position(f32::NAN, f32::INFINITY);
        assert!(x.is_nan() && y == f32::INFINITY);
    }

    #[test]
    fn a_sample_that_isn_t_a_position_doesn_t_stop_a_plugin() {
        let hooks = Hooks::default();
        hooks.put_on("offset".to_owned(), Running::start(&offset(), &[]).unwrap(), &[Hook::Position]);
        assert!(hooks.position(f32::NAN, 2.0).0.is_nan());
        assert_eq!(hooks.position(1.0, f32::INFINITY), (1.0, f32::INFINITY));
        assert!(hooks.is_on("offset"));
        assert_eq!(hooks.take_stopped(), []);
    }

    /// A plugin that says the pick reads the layer as its one switch says, in a store at `dir`.
    fn picking(dir: &str) -> Arc<Plugins> {
        let module = wat::parse_str(PICKS).unwrap();
        let plugins = Plugins::with_store(dir.to_owned());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["pick_layer"]);
        manifest["controls"] = json!([
            { "key": "layer", "type": "switch", "label": "Layer", "keeps": "eyedropper_layer" },
        ]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        plugins
    }

    #[test]
    fn the_pick_reads_the_layer_only_while_a_plugin_says_so() {
        let dir = store("pick");
        let plugins = picking(&dir);
        let hooks = plugins.hooks();
        assert!(!hooks.pick_layer(), "imported, it says nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert!(!hooks.pick_layer(), "on, it goes by its switch");
        assert!(plugins.set_switch("eyedropper_layer".to_owned(), true));
        assert!(hooks.pick_layer());
        assert!(plugins.set_on(ID.to_owned(), false));
        assert!(!hooks.pick_layer());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A plugin that says ColorDrop fills by every layer, in a store at `dir`.
    /// A plugin on whose action is `module`'s, reaching what `permissions` give.
    fn acting(dir: &str, module: &[u8], permissions: Value) -> Arc<Plugins> {
        let plugins = Plugins::with_store(dir.to_owned());
        let mut manifest = coded(ID, module);
        manifest["compat"]["procraft"]["interface"] = json!(2);
        manifest["hooks"] = json!([]);
        manifest["permissions"] = permissions;
        manifest["actions"] = json!([{ "key": "paint", "label": "Paint a square" }]);
        assert_eq!(plugins.install(file(&manifest, module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        plugins
    }

    #[test]
    fn an_action_paints_a_layer_of_its_own_and_one_undo_takes_it_back() {
        let Some(s) = session() else { return };
        let dir = store("action-session");
        let plugins = acting(&dir, &wat::parse_str(ACTS).unwrap(), json!(["artwork.write"]));
        let before = s.layers().len();
        assert!(plugins.run_action(format!("{ID}.paint"), Arc::clone(&s)));
        let layers = s.layers();
        assert_eq!(layers.len(), before + 1);
        let grid = layers.iter().find(|l| l.name == "Grid").expect("named");
        assert!(grid.current, "the layer added is the current one");
        let rgba = picture(&s);
        let at = |x: usize, y: usize| rgba[(y * 200 + x) * 4..(y * 200 + x) * 4 + 4].to_vec();
        assert_eq!(at(25, 35), [255, 0, 0, 255], "painted inside the square");
        assert_eq!(at(19, 35)[3], 0, "and nowhere else");
        assert_eq!(at(30, 35)[3], 0);
        // one undo takes the layer, its name and its paint back together
        s.undo();
        assert_eq!(s.layers().len(), before);
        assert!(s.layers().iter().all(|l| l.name != "Grid"));
        // a key it doesn't have does nothing, and nor does it off
        assert!(!plugins.run_action(format!("{ID}.other"), Arc::clone(&s)));
        assert!(plugins.set_on(ID.to_owned(), false));
        assert!(!plugins.run_action(format!("{ID}.paint"), Arc::clone(&s)));
        assert_eq!(s.layers().len(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Its action marks the current layer one more than it was; it labels a row marked 1 red, and
    /// marked 3 as no colour at all (past 0xFFFFFF).
    const MARKS: &str = r#"(module
        (import "procraft" "current_layer" (func $current (result i64)))
        (import "procraft" "layer_mark" (func $mark (param i64) (result i64)))
        (import "procraft" "set_layer_mark" (func $set (param i64 i64) (result i32)))
        (func (export "procraft_action") (param i32) (result i32)
            (drop (call $set (call $current) (i64.add (call $mark (call $current)) (i64.const 1))))
            (i32.const 0))
        (func (export "procraft_layer_label") (param i64) (result i32)
            (if (result i32) (i64.eq (local.get 0) (i64.const 1))
                (then (i32.const 0xFF0000))
                (else (if (result i32) (i64.eq (local.get 0) (i64.const 3))
                    (then (i32.const 0x1000000))
                    (else (i32.const -1)))))))"#;

    /// A plugin `id` whose code is [`MARKS`], on.
    fn layer_marking(dir: &str, id: &str) -> Arc<Plugins> {
        let module = wat::parse_str(MARKS).unwrap();
        let plugins = Plugins::with_store(dir.to_owned());
        let mut manifest = coded(id, &module);
        manifest["compat"]["procraft"]["interface"] = json!(10);
        manifest["hooks"] = json!(["layer_label"]);
        manifest["permissions"] = json!(["artwork.read", "artwork.write"]);
        manifest["actions"] = json!([{ "key": "mark", "label": "Mark it", "place": "layer" }]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(id.to_owned(), true));
        plugins
    }

    #[test]
    fn a_plugin_marks_a_layer_its_row_shows_the_label_and_undo_takes_it_back() {
        let Some(s) = session() else { return };
        let dir = store("marks");
        let plugins = layer_marking(&dir, ID);
        s.use_plugins(Arc::clone(&plugins));
        let other = s.add_layer();
        let current = || s.layers().into_iter().find(|l| l.current).unwrap();
        let (layer, mark) = (current().id, || s.layer_mark(ID, current().id));
        assert_eq!((current().label, mark()), (None, 0));
        // marked 1: its row red, the others' none
        assert!(plugins.run_action(format!("{ID}.mark"), Arc::clone(&s)));
        assert_eq!((current().label, mark()), (Some(0xFF0000), 1));
        assert!(s.layers().iter().filter(|l| l.id != layer).all(|l| l.label.is_none()));
        assert_eq!(layer, other, "the current layer");
        // marked 2, which it labels with nothing
        assert!(plugins.run_action(format!("{ID}.mark"), Arc::clone(&s)));
        assert_eq!((current().label, mark()), (None, 2));
        // each mark one step undo takes back
        s.undo();
        assert_eq!((current().label, mark()), (Some(0xFF0000), 1));
        s.undo();
        assert_eq!((current().label, mark()), (None, 0));
        s.redo();
        // only its own: another plugin's mark there is none
        assert_eq!(s.layer_mark("org.example.other", layer), 0);
        // off, no label, the mark kept as it was; on again, the label back
        assert!(plugins.set_on(ID.to_owned(), false));
        assert_eq!((current().label, mark()), (None, 1));
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(current().label, Some(0xFF0000));
        // a label that is no colour stops it
        assert!(plugins.run_action(format!("{ID}.mark"), Arc::clone(&s)));
        assert!(plugins.run_action(format!("{ID}.mark"), Arc::clone(&s)));
        assert_eq!((current().label, mark()), (None, 3));
        assert!(!card(&plugins).on);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Its action takes the second colour of the history and the first brush of the Recent set,
    /// and shows a ring of two colours and the first three brushes of the first set (at 16 in its
    /// memory); a tap on its ring's colours paints in that colour,
    /// one on its brushes chooses the brush there (its place in the first set).
    const PALETTE: &str = r#"(module
        (import "procraft" "recent_colour" (func $colour (param i32) (result i32)))
        (import "procraft" "recent_brush" (func $brush (param i32) (result i64)))
        (import "procraft" "choose_brush" (func $choose (param i64) (result i32)))
        (import "procraft" "set_brush_colour" (func $paint (param i32)))
        (import "procraft" "show_panel" (func $show (param i32 i32) (result i32)))
        (import "procraft" "close_panel" (func $close))
        (memory (export "memory") 1)
        (data (i32.const 16) "{\"title\":\"P\",\"ring\":true,\"rows\":[{\"type\":\"colours\",\"label\":\"C\",\"colours\":[255,65280]},{\"type\":\"brushes\",\"label\":\"B\",\"brushes\":[4294967296,4294967297,4294967298]}]}")
        (func (export "procraft_action") (param i32) (result i32)
            (call $paint (call $colour (i32.const 1)))
            (drop (call $choose (call $brush (i32.const 0))))
            (drop (call $show (i32.const 16) (i32.const 163)))
            (i32.const 0))
        (func (export "procraft_panel_event") (param i32 f32) (result i32)
            (if (i32.eqz (local.get 0))
                (then (call $paint (select (i32.const 255) (i32.const 65280) (f32.lt (local.get 1) (f32.const 0.5)))))
                (else (drop (call $choose (i64.or (i64.const 4294967296) (i64.trunc_f32_u (local.get 1)))))))
            (call $close)
            (i32.const 0)))"#;

    /// Its action sets its slider (control 0) to 0.73 and shows a panel of one switch; a touch on
    /// that sets its switch (control 1) as touched, and tries a place it has no control at (5) and a
    /// file control (2).
    const SETS: &str = r#"(module
        (import "procraft" "set_control" (func $set (param i32 f32) (result i32)))
        (import "procraft" "show_panel" (func $show (param i32 i32) (result i32)))
        (memory (export "memory") 1)
        (global $slider (mut f32) (f32.const 0))
        (global $said (mut i32) (i32.const 0))
        (data (i32.const 16) "{\"title\":\"S\",\"rows\":[{\"type\":\"switch\",\"label\":\"On\",\"on\":false}]}")
        (func (export "procraft_control") (param i32 f32)
            (if (i32.eqz (local.get 0)) (then (global.set $slider (local.get 1)))))
        (func (export "procraft_view_look") (param i32) (result f32) (global.get $slider))
        (func (export "procraft_action") (param i32) (result i32)
            (global.set $said (call $set (i32.const 0) (f32.const 0.73)))
            (drop (call $show (i32.const 16) (i32.const 64)))
            (i32.const 0))
        (func (export "procraft_panel_event") (param i32 f32) (result i32)
            (drop (call $set (i32.const 1) (local.get 1)))
            (drop (call $set (i32.const 5) (f32.const 1)))
            (drop (call $set (i32.const 2) (f32.const 1)))
            ;; told its slider as set: shown again, the switch's row on while it is
            (drop (call $show (i32.const 16) (i32.const 64)))
            (i32.const 0)))"#;

    #[test]
    fn an_action_in_the_layers_panel_sets_the_plugin_s_own_controls_as_its_card_would() {
        let s = CanvasSession::new(64, 64);
        let dir = store("sets-controls");
        let module = wat::parse_str(SETS).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["compat"]["procraft"]["interface"] = json!(12);
        manifest["hooks"] = json!(["view_look"]);
        manifest["permissions"] = json!(["panel"]);
        manifest["controls"] = json!([
            { "key": "size", "type": "slider", "label": "Size", "least": 0, "most": 1, "step": 0.1, "starts": 0 },
            { "key": "lit", "type": "switch", "label": "Lit", "starts": false },
            { "key": "picture", "type": "file", "label": "Picture", "accepts": "picture" }
        ]);
        manifest["actions"] = json!([{ "key": "open", "label": "Settings", "place": "layers" }]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        let key = format!("{ID}.open");
        let listed: Vec<String> = plugins.actions_in(crate::PluginPlace::Layers).into_iter().map(|a| a.key).collect();
        assert_eq!(listed, vec![key.clone()]);
        assert!(plugins.actions_in(crate::PluginPlace::Layer).is_empty());
        // as its card shows them
        let slider = || match card(&plugins).controls.first() {
            Some(crate::plugins::PluginControl::Slider { control }) => control.value,
            other => panic!("{other:?}"),
        };
        let switch = || matches!(card(&plugins).controls.get(1), Some(crate::plugins::PluginControl::Switch { control }) if control.on);
        // its action sets its slider, on its steps, as the card shows it and its hooks are told
        assert!(plugins.run_action(key.clone(), Arc::clone(&s)));
        assert!((slider() - 0.7).abs() < 1e-6, "{}", slider());
        let look = plugins.hooks().view_look().unwrap();
        assert!((look[0][0] - 0.7).abs() < 1e-6, "{look:?}");
        // its panel's touch sets its switch; a place with no switch or slider is left alone
        assert!(!switch());
        assert!(plugins.panel_touched(0, 1.0, Arc::clone(&s)));
        assert!(switch());
        // what was set, for the app to keep as the card keeps it: each once, as it is now
        let set = plugins.take_controls_set();
        let keys: Vec<&str> = set.iter().map(|c| c.key()).collect();
        assert_eq!(keys, [format!("{ID}.size"), format!("{ID}.lit")]);
        assert!((set[0].number() - 0.7).abs() < 1e-6 && set[1].number() == 1.0, "{set:?}");
        assert!(plugins.take_controls_set().is_empty());
        assert!(plugins.panel_touched(0, 0.0, Arc::clone(&s)));
        assert!(!switch());
        assert!(plugins.panel().is_some());
        // a module asking for set_control without the panel permission is refused
        let mut bare = manifest.clone();
        bare["id"] = json!("org.example.bare");
        bare["permissions"] = json!([]);
        assert!(plugins.install(file(&bare, &module)).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A paper plugin with a texture control (interface 15) whose value is the app's grain it gives
    /// as its paper, and an action showing a panel of that control.
    const TEXTURED: &str = r#"(module
        (import "procraft" "show_panel" (func $show (param i32 i32) (result i32)))
        (memory (export "memory") 1)
        (global $texture (mut f32) (f32.const 0))
        (data (i32.const 16) "{\"title\":\"P\",\"rows\":[{\"type\":\"control\",\"key\":\"texture\"}]}")
        (func (export "procraft_control") (param i32 f32)
            (if (i32.eqz (local.get 0)) (then (global.set $texture (local.get 1)))))
        (func (export "procraft_action") (param i32) (result i32)
            (drop (call $show (i32.const 16) (i32.const 57)))
            (i32.const 0))
        (func (export "procraft_panel_event") (param i32 f32) (result i32) (i32.const 0))
        (func (export "procraft_paper") (param i32) (result i32) (i32.const 0))
        (func (export "procraft_paper_look") (param i32) (result f32)
            (select (f32.const 1) (f32.const 0.5) (i32.eq (local.get 0) (i32.const 1))))
        (func (export "procraft_paper_colour") (result i32) (i32.const -1))
        (func (export "procraft_paper_grain") (result i32) (i32.trunc_f32_s (global.get $texture))))"#;

    #[test]
    fn a_texture_control_names_the_papers_grain_and_shows_in_its_panel_as_on_its_card() {
        use crate::plugins::PluginControl;
        let s = CanvasSession::new(64, 64);
        let dir = store("textures");
        let module = wat::parse_str(TEXTURED).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["compat"]["procraft"]["interface"] = json!(15);
        manifest["hooks"] = json!(["paper"]);
        manifest["permissions"] = json!(["panel"]);
        manifest["controls"] = json!([
            { "key": "texture", "type": "texture", "label": "Texture", "own": ["first", "second"] },
            { "key": "first", "type": "file", "label": "A picture", "accepts": "picture" },
            { "key": "second", "type": "file", "label": "Another", "accepts": "picture" }
        ]);
        manifest["actions"] = json!([{ "key": "open", "label": "Paper", "place": "layers" }]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        let key = format!("{ID}.texture");
        let texture = || match card(&plugins).controls.first() {
            Some(PluginControl::Texture { control }) => control.value,
            other => panic!("{other:?}"),
        };
        // as it starts, none: no paper
        assert_eq!(texture(), 0);
        assert!(plugins.hooks().paper().is_none());
        // the app's grains by their number in the Source Library, the plugin's pictures below 0;
        // past either end, none
        let most = procraft_engine::textures::GRAIN_COUNT as i32;
        assert_eq!(plugins.set_texture(key.clone(), most), Some(most));
        assert_eq!(plugins.set_texture(key.clone(), -2), Some(-2));
        assert_eq!(plugins.set_texture(key.clone(), -3), Some(0));
        assert_eq!(plugins.set_texture(key.clone(), most + 1), Some(0));
        assert_eq!(plugins.set_texture(format!("{ID}.first"), 1), None, "a file control is no texture");
        // grain 19 of the Source Library (the app's grain 18): the paper's tile, as it is
        assert_eq!(plugins.set_texture(key.clone(), 19), Some(19));
        assert_eq!(texture(), 19);
        let (_, paper) = plugins.hooks().paper().unwrap();
        assert_eq!(paper.tile, procraft_engine::textures::grain_at(18, crate::plugin_run::PAPER_SIDE).unwrap());
        // its panel shows the control by its key, as its card does; a touch there is the card's
        s.use_plugins(Arc::clone(&plugins));
        assert!(plugins.run_action(format!("{ID}.open"), Arc::clone(&s)));
        assert_eq!(plugins.panel().unwrap().rows, [crate::PanelRow::Control { key: key.clone() }]);
        assert!(plugins.panel_touched(0, 3.0, Arc::clone(&s)));
        assert_eq!(texture(), 19);
        // a picture picked for one it offers, shrunk to a thumbnail; none picked, none
        let first = format!("{ID}.first");
        assert_eq!(plugins.picture_thumbnail(first.clone(), 64), None);
        let picture: Vec<u8> = (0..300 * 200).flat_map(|i| [(i % 300) as u8, 90, 160, 255]).collect();
        let png = crate::project::png_of(300, 200, &picture).unwrap();
        assert!(plugins.set_picture_file(first.clone(), "weave.png".to_owned(), png));
        let thumbnail = plugins.picture_thumbnail(first, 64).unwrap();
        assert!(thumbnail.width == 64 && (42..=43).contains(&thumbnail.height), "{thumbnail:?}");
        assert_eq!(thumbnail.pixels.len(), (thumbnail.width * thumbnail.height * 4) as usize);
        assert_eq!(plugins.picture_thumbnail(format!("{ID}.second"), 64), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_reads_the_recent_colours_and_brushes_and_its_ring_opens_where_the_pen_is() {
        let s = CanvasSession::new(64, 64);
        let history = crate::ColourHistory::new();
        s.use_colour_history(Arc::clone(&history));
        for rgb in [0x112233, 0x445566] {
            history.painted(rgb);
        }
        s.shared.lock().note_recent((0, 2));
        let dir = store("palette");
        let module = wat::parse_str(PALETTE).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["compat"]["procraft"]["interface"] = json!(11);
        manifest["hooks"] = json!([]);
        manifest["permissions"] = json!(["brush", "panel"]);
        manifest["actions"] = json!([{ "key": "open", "label": "Palette", "place": "pen" }]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        let key = format!("{ID}.open");
        let listed: Vec<String> = plugins.actions_in(crate::PluginPlace::Pen).into_iter().map(|a| a.key).collect();
        assert_eq!(listed, vec![key.clone()]);
        let rgb = |s: &CanvasSession| crate::history::hex(s.shared.lock().color);
        // run where the pen is: the older colour of the two, the Recent set's brush, and its ring there
        assert!(plugins.run_action_at(key.clone(), Arc::clone(&s), 30.0, 40.0));
        assert_eq!(rgb(&s), 0x112233);
        let brush = s.current_brush();
        assert_eq!((brush.set, brush.index), (0, 2));
        let panel = plugins.panel().unwrap();
        assert!(panel.ring);
        assert_eq!(panel.at, Some(crate::PanelPoint { x: 30.0, y: 40.0 }));
        let crate::PanelRow::Brushes { brushes, chosen: None, .. } = &panel.rows[1] else { panic!("{:?}", panel.rows) };
        let library = s.brush_library();
        let names: Vec<&str> = library[0].brushes.iter().take(3).map(|b| b.name.as_str()).collect();
        assert_eq!(brushes.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), names);
        assert_eq!(brushes.iter().map(|b| (b.set, b.index)).collect::<Vec<_>>(), [(0, 0), (0, 1), (0, 2)]);
        // a brush tapped: chosen, and the ring closed
        assert!(!plugins.panel_touched(1, 1.0, Arc::clone(&s)));
        let brush = s.current_brush();
        assert_eq!((brush.set, brush.index), (0, 1));
        assert!(plugins.panel().is_none());
        // from its card: no point, the window's middle (the Recent set's brush taken again)
        assert!(plugins.run_action(key.clone(), Arc::clone(&s)));
        assert_eq!(plugins.panel().unwrap().at, None);
        assert_eq!((s.current_brush().set, s.current_brush().index), (0, 2));
        assert!(!plugins.panel_touched(0, 1.0, Arc::clone(&s)));
        assert_eq!(rgb(&s), 0x00FF00);
        assert_eq!(history.colours(), [0x445566, 0x112233], "the history as painted");
        // with nothing in them, a colour past the history's end is -1 (no colour: 0xFFFFFF) and a
        // brush past the Recent set's 0 (none: the brush stays)
        history.clear();
        s.clear_recent_brushes();
        assert!(plugins.run_action(key.clone(), Arc::clone(&s)));
        assert_eq!(rgb(&s), 0xFFFFFF);
        assert_eq!((s.current_brush().set, s.current_brush().index), (0, 2));
        assert!(card(&plugins).on);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_surround_picture_is_the_first_plugin_s_that_answers_and_a_nonsense_place_stops_it() {
        let picturing = |body: &str| {
            let text = format!(r#"(module (func (export "procraft_surround_picture") (result i32) {body}))"#);
            Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap()
        };
        let hooks = Hooks::default();
        assert_eq!(hooks.surround_picture(), None);
        hooks.put_on("b".to_owned(), picturing("(i32.const 2)"), &[Hook::SurroundPicture]);
        assert_eq!(hooks.surround_picture(), Some(("b".to_owned(), 2)));
        // one before it by id with none leaves it to the next
        hooks.put_on("a".to_owned(), picturing("(i32.const -1)"), &[Hook::SurroundPicture]);
        assert_eq!(hooks.surround_picture(), Some(("b".to_owned(), 2)));
        hooks.put_on("a".to_owned(), picturing("(i32.const 0)"), &[Hook::SurroundPicture]);
        assert_eq!(hooks.surround_picture(), Some(("a".to_owned(), 0)));
        assert!(hooks.take_stopped().is_empty());
        for odd in ["(i32.const -2)", "(i32.const 256)", "(unreachable)"] {
            let hooks = Hooks::default();
            hooks.put_on("odd".to_owned(), picturing(odd), &[Hook::SurroundPicture]);
            assert_eq!(hooks.surround_picture(), None, "{odd}");
            assert_eq!(hooks.take_stopped().len(), 1, "{odd}");
        }
    }

    /// A plugin on the slider look's hook answering `body` (WAT, given what is asked).
    fn slider_looking(body: &str) -> Running {
        let text = format!(r#"(module (func (export "procraft_slider_look") (param i32) (result f32) {body}))"#);
        Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap()
    }

    /// A plugin on the dimming hook answering `body` (WAT).
    fn dimming(body: &str) -> Running {
        let text = format!(r#"(module (func (export "procraft_view_dim_others") (result f32) {body}))"#);
        Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap()
    }

    #[test]
    fn the_layers_are_dimmed_as_much_as_the_most_any_plugin_on_the_dimming_hook_says() {
        let hooks = Hooks::default();
        assert_eq!(hooks.view_dim_others(), 0.0, "none on");
        hooks.put_on("a".to_owned(), dimming("(f32.const 0.3)"), &[Hook::ViewDimOthers]);
        assert_eq!(hooks.view_dim_others(), 0.3);
        hooks.put_on("b".to_owned(), dimming("(f32.const 0.7)"), &[Hook::ViewDimOthers]);
        assert_eq!(hooks.view_dim_others(), 0.7);
        hooks.take_off("b");
        assert_eq!(hooks.view_dim_others(), 0.3);
        // asked as the plugins change: quick
        let t = Instant::now();
        for _ in 0..10_000 {
            std::hint::black_box(hooks.view_dim_others());
        }
        let each = t.elapsed() / 10_000;
        println!("the dimming hook asked in {each:?}");
        assert!(each < std::time::Duration::from_micros(200), "{each:?}");
        // past 0 to 1, not a number, or a trap: it stops, and goes off its hook
        for odd in ["(f32.const 1.5)", "(f32.const -0.1)", "(f32.div (f32.const 0) (f32.const 0))", "unreachable"] {
            let hooks = Hooks::default();
            hooks.put_on("odd".to_owned(), dimming(odd), &[Hook::ViewDimOthers]);
            assert_eq!(hooks.view_dim_others(), 0.0, "{odd}");
            assert!(!hooks.listens(Hook::ViewDimOthers), "{odd}");
            assert_eq!(hooks.take_stopped().len(), 1, "{odd}");
        }
    }

    #[test]
    fn the_colour_panel_does_what_any_plugin_on_its_hook_says_and_nonsense_stops_one() {
        let hooks = Hooks::default();
        let asks = |hooks: &Hooks| (hooks.colour_panel(COLOUR_STEPS), hooks.colour_panel(COLOUR_BEFORE));
        assert_eq!(asks(&hooks), (false, false), "none on");
        // one that says its sliders step (part 0) alone
        let steps = "(i32.eqz (local.get 0))";
        let text = format!(r#"(module (func (export "procraft_colour_panel") (param i32) (result i32) {steps}))"#);
        hooks.put_on(
            "a".to_owned(),
            Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap(),
            &[Hook::ColourPanel],
        );
        assert_eq!(asks(&hooks), (true, false));
        // another that says its swatch shows the colour before (part 1): both
        let before = "(i32.eq (local.get 0) (i32.const 1))";
        let text = format!(r#"(module (func (export "procraft_colour_panel") (param i32) (result i32) {before}))"#);
        hooks.put_on(
            "b".to_owned(),
            Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap(),
            &[Hook::ColourPanel],
        );
        assert_eq!(asks(&hooks), (true, true));
        // asked as the panel shows: quick
        let t = Instant::now();
        for i in 0..10_000 {
            std::hint::black_box(hooks.colour_panel(i % 2));
        }
        let each = t.elapsed() / 10_000;
        println!("the colour panel's hook asked in {each:?}");
        assert!(each < std::time::Duration::from_micros(200), "{each:?}");
        // one that traps stops, and goes off its hook
        let text = r#"(module (func (export "procraft_colour_panel") (param i32) (result i32) unreachable))"#;
        let hooks = Hooks::default();
        hooks.put_on(
            "odd".to_owned(),
            Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap(),
            &[Hook::ColourPanel],
        );
        assert!(!hooks.colour_panel(COLOUR_STEPS));
        assert!(!hooks.listens(Hook::ColourPanel));
        assert_eq!(hooks.take_stopped().len(), 1);
    }

    #[test]
    fn the_slider_tracks_look_as_the_first_plugin_that_answers_says_and_quickly() {
        let hooks = Hooks::default();
        assert_eq!((hooks.slider_look(SLIDER_OPACITY), hooks.slider_look(SLIDER_FROSTED)), (None, None));
        // an opacity of a quarter, and frosted
        let quarter = "(if (result f32) (local.get 0) (then (f32.const 1)) (else (f32.const 0.25)))";
        hooks.put_on("b".to_owned(), slider_looking(quarter), &[Hook::SliderLook]);
        assert_eq!((hooks.slider_look(SLIDER_OPACITY), hooks.slider_look(SLIDER_FROSTED)), (Some(0.25), Some(1.0)));
        // one before it by id that leaves it (below 0) doesn't count; one that answers does
        hooks.put_on("a".to_owned(), slider_looking("(f32.const -1)"), &[Hook::SliderLook]);
        assert_eq!(hooks.slider_look(SLIDER_OPACITY), Some(0.25));
        hooks.put_on("a".to_owned(), slider_looking("(f32.const 0.5)"), &[Hook::SliderLook]);
        assert_eq!(hooks.slider_look(SLIDER_OPACITY), Some(0.5));
        assert!(hooks.take_stopped().is_empty());
        // asked as the plugins change and as the desktop draws its side bar: quick
        let t = Instant::now();
        for _ in 0..10_000 {
            assert_eq!(hooks.slider_look(std::hint::black_box(SLIDER_OPACITY)), Some(0.5));
        }
        let each = t.elapsed() / 10_000;
        println!("the slider look asked in {each:?}");
        assert!(each < std::time::Duration::from_micros(100), "{each:?}");
        // past 1, or no number, stops it
        for odd in ["(f32.const 1.5)", "(f32.const nan)", "(unreachable)"] {
            let hooks = Hooks::default();
            hooks.put_on("odd".to_owned(), slider_looking(odd), &[Hook::SliderLook]);
            assert_eq!(hooks.slider_look(SLIDER_OPACITY), None, "{odd}");
            assert_eq!(hooks.take_stopped().len(), 1, "{odd}");
        }
    }

    fn laying_out(body: &str) -> Running {
        let text = format!(r#"(module (func (export "procraft_panel_layout") (param i32) (result f32) {body}))"#);
        Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap()
    }

    #[test]
    fn the_panels_are_laid_out_as_the_first_plugin_that_answers_each_part_says() {
        let hooks = Hooks::default();
        let parts = |hooks: &Hooks| [LAYOUT_SNAP, LAYOUT_FOLD, LAYOUT_ROOM, LAYOUT_EDIT].map(|p| hooks.panel_layout(p));
        assert_eq!(parts(&hooks), [None; 4], "none on: as the app has it");
        // snapping within 16, folding, no room made, nothing said of the bars
        let says = "(if (result f32) (i32.eqz (local.get 0)) (then (f32.const 16)) (else \
            (if (result f32) (i32.eq (local.get 0) (i32.const 1)) (then (f32.const 1)) (else \
            (if (result f32) (i32.eq (local.get 0) (i32.const 2)) (then (f32.const 0)) (else (f32.const -1)))))))";
        hooks.put_on("b".to_owned(), laying_out(says), &[Hook::PanelLayout]);
        assert_eq!(parts(&hooks), [Some(16.0), Some(1.0), Some(0.0), None]);
        // one before it by id that leaves a part leaves it to the next; one that answers says
        hooks.put_on("a".to_owned(), laying_out("(f32.const -1)"), &[Hook::PanelLayout]);
        assert_eq!(parts(&hooks), [Some(16.0), Some(1.0), Some(0.0), None]);
        hooks.put_on("a".to_owned(), laying_out("(f32.const 1)"), &[Hook::PanelLayout]);
        assert_eq!(parts(&hooks), [Some(1.0); 4]);
        assert!(hooks.take_stopped().is_empty());
        // asked as the plugins change and as the desktop draws: quick
        let t = Instant::now();
        for _ in 0..10_000 {
            assert_eq!(hooks.panel_layout(std::hint::black_box(LAYOUT_SNAP)), Some(1.0));
        }
        let each = t.elapsed() / 10_000;
        println!("the panel layout asked in {each:?}");
        assert!(each < std::time::Duration::from_micros(100), "{each:?}");
        // a snap past 64 points, a yes past 1, or no number, stops it
        for (part, odd) in [
            (LAYOUT_SNAP, "(f32.const 64.5)"),
            (LAYOUT_FOLD, "(f32.const 1.5)"),
            (LAYOUT_ROOM, "(f32.const nan)"),
            (LAYOUT_SNAP, "(unreachable)"),
        ] {
            let hooks = Hooks::default();
            hooks.put_on("odd".to_owned(), laying_out(odd), &[Hook::PanelLayout]);
            assert_eq!(hooks.panel_layout(part), None, "{odd}");
            assert_eq!(hooks.take_stopped().len(), 1, "{odd}");
        }
        // 64 itself is a snap, not a yes
        let hooks = Hooks::default();
        hooks.put_on("far".to_owned(), laying_out("(f32.const 64)"), &[Hook::PanelLayout]);
        assert_eq!(hooks.panel_layout(LAYOUT_SNAP), Some(64.0));
        assert_eq!(hooks.panel_layout(LAYOUT_FOLD), None);
        assert_eq!(hooks.take_stopped().len(), 1);
    }

    #[test]
    fn each_plugin_is_told_its_own_mark_the_first_by_id_with_a_label_showing() {
        let hooks = Hooks::default();
        let labels = |hooks: &Hooks, marks: &[(&str, i64)]| {
            hooks.layer_label(&marks.iter().map(|(id, mark)| ((*id).to_owned(), *mark)).collect())
        };
        // none on: no label, whatever is marked
        assert_eq!(labels(&hooks, &[("a", 1)]), None);
        // [`MARKS`]'s label alone: a hook's copy of its code reaches none of the app's functions
        let labels_only = r#"(module
            (func (export "procraft_layer_label") (param i64) (result i32)
                (if (result i32) (i64.eq (local.get 0) (i64.const 1))
                    (then (i32.const 0xFF0000))
                    (else (i32.const -1)))))"#;
        for id in ["a", "b"] {
            let running = Running::start(&wat::parse_str(labels_only).unwrap(), &[]).unwrap();
            hooks.put_on(id.to_owned(), running, &[Hook::LayerLabel]);
        }
        // a's mark alone, b's alone, b's past a's that shows nothing, a mark of a plugin not on
        assert_eq!(labels(&hooks, &[("a", 1)]), Some(0xFF0000));
        assert_eq!(labels(&hooks, &[("b", 1)]), Some(0xFF0000));
        assert_eq!(labels(&hooks, &[("a", 2), ("b", 1)]), Some(0xFF0000));
        assert_eq!(labels(&hooks, &[("c", 1)]), None);
        assert_eq!(labels(&hooks, &[]), None);
        // a mark of 0 is none, and its plugin isn't asked (one asked with it would answer garbage)
        assert_eq!(labels(&hooks, &[("a", 0)]), None);
        assert!(hooks.take_stopped().is_empty());
        // asked for each marked layer as the Layers panel is drawn: quick
        let marks: std::collections::BTreeMap<String, i64> = [("b".to_owned(), 1)].into();
        let t = Instant::now();
        for _ in 0..10_000 {
            assert_eq!(hooks.layer_label(std::hint::black_box(&marks)), Some(0xFF0000));
        }
        let each = t.elapsed() / 10_000;
        println!("a layer's label asked in {each:?}");
        assert!(each < std::time::Duration::from_micros(100), "{each:?}");
    }

    /// What the Layers panel's list costs with every layer labelled, and with none on the hook:
    /// printed, and failing past 2 ms for 200 layers (the desktop lists them each frame it shows).
    #[test]
    fn the_layers_list_with_every_layer_labelled_stays_quick() {
        let s = CanvasSession::new(64, 64);
        let dir = store("marks-cost");
        let plugins = layer_marking(&dir, ID);
        {
            let mut st = s.shared.lock();
            for _ in 1..200 {
                st.doc.add_layer();
            }
            let ids: Vec<_> = st.doc.layers().iter().map(|l| l.id).collect();
            for id in ids {
                assert!(st.doc.set_mark(id, ID, 1));
            }
        }
        let listed = |s: &CanvasSession| {
            (0..5)
                .map(|_| {
                    let t = Instant::now();
                    for _ in 0..10 {
                        std::hint::black_box(s.layers());
                    }
                    t.elapsed() / 10
                })
                .min()
                .unwrap_or_default()
        };
        let bare = listed(&s);
        s.use_plugins(Arc::clone(&plugins));
        assert!(s.layers().iter().all(|l| l.label == Some(0xFF0000)));
        let labelled = listed(&s);
        println!("200 layers listed in {bare:?} with no labels, {labelled:?} each labelled");
        assert!(labelled < std::time::Duration::from_millis(2), "{labelled:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Told an event, adds a layer named "After".
    const TOLD: &str = r#"(module
        (import "procraft" "add_layer" (func $add (result i64)))
        (import "procraft" "set_layer_name" (func $name (param i64 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "After")
        (func (export "procraft_event") (param i32) (result i32)
            (drop (call $name (call $add) (i32.const 0) (i32.const 5)))
            (i32.const 0)))"#;

    #[test]
    fn a_plugin_is_told_a_stroke_was_kept_and_a_layer_added_and_acts_as_one_step() {
        let Some(s) = session() else { return };
        let dir = store("events");
        let module = wat::parse_str(TOLD).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["compat"]["procraft"]["interface"] = json!(2);
        manifest["hooks"] = json!(["stroke_kept", "layer_added"]);
        manifest["permissions"] = json!(["artwork.write"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        let afters = |s: &CanvasSession| s.layers().iter().filter(|l| l.name == "After").count();
        let before = s.layers().len();
        // a stroke kept: told once
        draw(&s);
        assert_eq!(afters(&s), 1);
        assert_eq!(s.layers().len(), before + 1);
        // a layer added from the panel: told, and the layer the plugin adds itself tells nothing
        s.add_layer();
        assert_eq!(afters(&s), 2);
        assert_eq!(s.layers().len(), before + 3);
        // undo takes the plugin's step back first, then the layer added
        s.undo();
        assert_eq!((afters(&s), s.layers().len()), (1, before + 2));
        s.undo();
        assert_eq!((afters(&s), s.layers().len()), (1, before + 1));
        // off: told nothing
        assert!(plugins.set_on(ID.to_owned(), false));
        s.add_layer();
        assert_eq!(afters(&s), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// On `stroke_kept` and `opacity_together`: told a stroke, it sets the current layer's opacity
    /// to a half, which asks its own `opacity_together`.
    const HALVES: &str = r#"(module
        (import "procraft" "current_layer" (func $current (result i64)))
        (import "procraft" "set_layer_opacity" (func $opacity (param i64 f32) (result i32)))
        (func (export "procraft_opacity_together") (result i32) (i32.const 1))
        (func (export "procraft_event") (param i32) (result i32)
            (drop (call $opacity (call $current) (f32.const 0.5)))
            (i32.const 0)))"#;

    #[test]
    fn an_event_that_calls_into_its_own_other_hook_doesn_t_wait_on_itself() {
        let Some(s) = session() else { return };
        let dir = store("events-own-hook");
        let module = wat::parse_str(HALVES).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["compat"]["procraft"]["interface"] = json!(2);
        manifest["hooks"] = json!(["stroke_kept", "opacity_together"]);
        manifest["permissions"] = json!(["artwork.read", "artwork.write"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        // (on one copy of its code this waited on itself for good)
        draw(&s);
        let current = s.layers().into_iter().find(|l| l.current).unwrap();
        assert!((current.opacity - 0.5).abs() < 1e-3, "{}", current.opacity);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_layer_the_app_adds_for_its_own_work_tells_no_plugin() {
        let Some(s) = session() else { return };
        let dir = store("events-quiet");
        let module = wat::parse_str(TOLD).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["compat"]["procraft"]["interface"] = json!(2);
        manifest["hooks"] = json!(["layer_added"]);
        manifest["permissions"] = json!(["artwork.write"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        draw(&s);
        s.select_shape(false, 0.0, 0.0, 100.0, 100.0, crate::SelectionJoin::Add);
        let before = s.layers().len();
        assert!(s.copy_selection_to_layer().is_some());
        assert_eq!(s.layers().len(), before + 1, "a selection copied to a layer: no plugin told");
        assert!(s.layers().iter().all(|l| l.name != "After"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_grid_plugin_adds_a_grid_layer_in_the_brush_colour() {
        let Some(s) = session() else { return };
        let dir = store("grid-layer");
        let plugins = Plugins::with_store(dir.clone());
        let (_, file) = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "layer-toolbox").unwrap();
        assert_eq!(plugins.install(file.to_vec()), None);
        let id = "io.github.artcraftandroid.layer-toolbox";
        assert!(plugins.set_on(id.to_owned(), true));
        assert!(plugins.set_switch(format!("{id}.layer_fills"), true));
        // there while its switch is on
        let card = || plugins.plugins().into_iter().find(|p| p.id == id).unwrap();
        assert!(card().actions.is_empty());
        assert!(!plugins.run_action(format!("{id}.add"), Arc::clone(&s)));
        assert!(plugins.set_switch(format!("{id}.grid"), true));
        // lines 50 apart, 3 wide, in blue
        let card = card();
        assert_eq!(card.actions.len(), 1);
        // on its card only
        assert!(plugins.actions_in(crate::plugins::PluginPlace::Adjustments).is_empty());
        assert_eq!(plugins.actions_in(crate::plugins::PluginPlace::Card), card.actions);
        assert!(plugins.set_slider(format!("{id}.spacing"), 50.0).is_some());
        assert!(plugins.set_slider(format!("{id}.line"), 3.0).is_some());
        s.set_color(0.0, 0.0, 1.0);
        assert!(plugins.run_action(card.actions[0].key.clone(), Arc::clone(&s)));
        assert!(s.layers().iter().any(|l| l.name == "Grid" && l.current));
        let rgba = picture(&s);
        let at = |x: usize, y: usize| rgba[(y * 200 + x) * 4..(y * 200 + x) * 4 + 4].to_vec();
        for (x, y) in [(0, 10), (2, 10), (50, 77), (152, 199), (10, 0), (77, 102), (199, 150)] {
            assert_eq!(at(x, y), [0, 0, 255, 255], "a line through ({x}, {y})");
        }
        for (x, y) in [(3, 10), (49, 77), (53, 77), (25, 25), (175, 175)] {
            assert_eq!(at(x, y)[3], 0, "between lines at ({x}, {y})");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_action_s_panel_stays_open_its_touches_told_to_the_same_code() {
        let Some(s) = session() else { return };
        let dir = store("action-panel");
        let json =
            r#"{"title":"Squares","rows":[{"type":"button","label":"Add a layer"},{"type":"button","label":"Done"}]}"#;
        let text = format!(
            r#"(module
                (import "procraft" "show_panel" (func $show (param i32 i32) (result i32)))
                (import "procraft" "close_panel" (func $close))
                (import "procraft" "add_layer" (func $add (result i64)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{}")
                (func (export "procraft_action") (param i32) (result i32)
                    (drop (call $show (i32.const 0) (i32.const {}))) (i32.const 0))
                (func (export "procraft_panel_event") (param $row i32) (param f32) (result i32)
                    (if (i32.eqz (local.get $row)) (then (drop (call $add))) (else (call $close)))
                    (i32.const 0)))"#,
            json.replace('"', "\\\""),
            json.len()
        );
        let plugins = acting(&dir, &wat::parse_str(text).unwrap(), json!(["artwork.write", "panel"]));
        assert_eq!(plugins.panel(), None);
        assert!(plugins.run_action(format!("{ID}.paint"), Arc::clone(&s)));
        let panel = plugins.panel().expect("shown");
        assert_eq!(panel.title, "Squares");
        assert_eq!(panel.rows.len(), 2);
        // its first row adds a layer, one step to undo; the panel stays
        let before = s.layers().len();
        assert!(plugins.panel_touched(0, 1.0, Arc::clone(&s)));
        assert_eq!(s.layers().len(), before + 1);
        s.undo();
        assert_eq!(s.layers().len(), before);
        assert!(plugins.panel().is_some());
        // its second closes it
        assert!(!plugins.panel_touched(1, 1.0, Arc::clone(&s)));
        assert_eq!(plugins.panel(), None);
        assert!(!plugins.panel_touched(0, 1.0, Arc::clone(&s)));
        assert_eq!(s.layers().len(), before);
        // turned off, its panel goes with it
        assert!(plugins.run_action(format!("{ID}.paint"), Arc::clone(&s)));
        assert!(plugins.panel().is_some());
        assert!(plugins.set_on(ID.to_owned(), false));
        assert_eq!(plugins.panel(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn combo(text: &str) -> KeyCombo {
        KeyCombo::read(text).unwrap()
    }

    /// The keys of the row that does `does` in `list`.
    fn keys_of(list: &[crate::ShortcutGroup], does: &str) -> Option<String> {
        list.iter().flat_map(|g| &g.rows).find(|r| r.does == does).map(|r| r.keys.clone())
    }

    /// `press` written for the platform the tests run on.
    fn written(press: &str) -> String {
        combo(press).label(crate::keys::MAC)
    }

    #[test]
    fn the_keyboard_plugin_binds_a_key_anew_from_its_panel_and_keeps_it() {
        use crate::{KeyShortcut, PanelRow};
        let Some(s) = session() else { return };
        let dir = store("keyboard");
        let id = "io.github.artcraftandroid.keyboard-shortcuts";
        let plugins = Plugins::with_store(dir.clone());
        let (_, file) = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "keyboard-shortcuts").unwrap();
        assert_eq!(plugins.install(file.to_vec()), None);
        // imported, off: every key as it came
        assert_eq!(plugins.key_press(combo("command+y")), Some(combo("command+y")));
        assert!(plugins.set_on(id.to_owned(), true));
        assert_eq!(plugins.key_press(combo("command+z")), Some(combo("command+z")));
        assert_eq!(plugins.key_shortcut_list(true, false), crate::key_shortcut_list(true, false));
        // its action shows the list, a group a page
        let card = plugins.plugins().into_iter().find(|p| p.id == id).unwrap();
        assert!(plugins.run_action(card.actions[0].key.clone(), Arc::clone(&s)));
        let panel = plugins.panel().expect("shown");
        assert_eq!(panel.title, "Keyboard shortcuts");
        assert!(!panel.key_press);
        assert!(matches!(&panel.rows[0], PanelRow::Choice { options, chosen: 0, .. } if options[0] == "Edit"));
        let undo =
            panel.rows.iter().position(|r| matches!(r, PanelRow::Keys { label, .. } if label == "Undo")).unwrap();
        assert_eq!(panel.rows[undo], PanelRow::Keys { label: "Undo".into(), keys: written("command+z") });
        // a key press goes to no panel that doesn't ask for one
        assert!(!plugins.panel_key(combo("command+y"), Arc::clone(&s)));
        // Undo tapped asks for its new keys, and takes the next press
        assert!(plugins.panel_touched(undo as u32, 1.0, Arc::clone(&s)));
        let asking = plugins.panel().unwrap();
        assert_eq!(asking.title, "Undo");
        assert!(asking.key_press);
        assert!(plugins.panel_key(combo("command+y"), Arc::clone(&s)));
        let panel = plugins.panel().unwrap();
        assert!(!panel.key_press);
        assert_eq!(panel.rows[undo], PanelRow::Keys { label: "Undo".into(), keys: written("command+y") });
        // on the canvas Ctrl+Y undoes, and Ctrl+Z does nothing; Redo is as it came
        assert_eq!(plugins.key_press(combo("command+y")), Some(combo("command+z")));
        assert_eq!(plugins.key_press(combo("command+z")), None);
        assert_eq!(plugins.key_shortcut("Y".into(), true, false, false, false), Some(KeyShortcut::Undo));
        assert_eq!(plugins.key_shortcut("z".into(), true, false, false, false), None);
        assert_eq!(plugins.key_shortcut("z".into(), true, true, false, false), Some(KeyShortcut::Redo));
        // and so the list says
        let list = plugins.key_shortcut_list(true, false);
        assert_eq!(keys_of(&list, "Undo").as_deref(), Some("Ctrl+Y"));
        assert_eq!(keys_of(&list, "Redo").as_deref(), Some("Ctrl+Shift+Z"));
        // kept: the next start has it
        let next = restarted(&dir, &plugins);
        assert_eq!(next.key_press(combo("command+y")), Some(combo("command+z")));
        // off, every key is as it came; on again, as it was bound
        assert!(plugins.set_on(id.to_owned(), false));
        assert_eq!(plugins.key_press(combo("command+z")), Some(combo("command+z")));
        assert_eq!(plugins.key_shortcut_list(true, false), crate::key_shortcut_list(true, false));
        assert!(plugins.set_on(id.to_owned(), true));
        assert_eq!(plugins.key_press(combo("command+z")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// On the key hook: Q is taken as Ctrl+Z, N as nothing, X as something that is no press, and
    /// anything else as it is.
    const REBINDS: &str = r#"(module
        (memory (export "memory") 1)
        (data (i32.const 16) "command+z")
        (data (i32.const 32) "ctrl+y")
        (func (export "procraft_key_buffer") (param i32) (result i32) (i32.const 256))
        (func (export "procraft_key_shortcut") (param $len i32) (result i64)
            (if (i32.eq (local.get $len) (i32.const 1)) (then
                (if (i32.eq (i32.load8_u (i32.const 256)) (i32.const 113))
                    (then (return (i64.or (i64.shl (i64.const 9) (i64.const 32)) (i64.const 16)))))
                (if (i32.eq (i32.load8_u (i32.const 256)) (i32.const 110))
                    (then (return (i64.const -1))))
                (if (i32.eq (i32.load8_u (i32.const 256)) (i32.const 120))
                    (then (return (i64.or (i64.shl (i64.const 6) (i64.const 32)) (i64.const 32)))))))
            (i64.const 0))
        (func (export "procraft_key_shortcut_keys") (param i32) (result i64) (i64.const 0)))"#;

    #[test]
    fn a_press_goes_as_the_key_hook_says_and_one_that_is_no_press_stops_it() {
        let dir = store("rebinds");
        let module = wat::parse_str(REBINDS).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["key_shortcut"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert_eq!(plugins.key_press(combo("q")), Some(combo("q")), "off");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(plugins.key_press(combo("q")), Some(combo("command+z")));
        assert_eq!(plugins.key_press(combo("n")), None);
        assert_eq!(plugins.key_press(combo("command+q")), Some(combo("command+q")));
        // a key no press could have isn't asked about
        assert_eq!(plugins.key_press(KeyCombo::of("a b", false, false, false)).map(|c| c.key), Some("a b".into()));
        // what it took "x" as is no press: off it goes, and the press goes as it came
        assert_eq!(plugins.key_press(combo("x")), Some(combo("x")));
        assert_eq!(card(&plugins).problem.as_deref(), Some("Switched off: it gave an answer that makes no sense."));
        assert_eq!(plugins.key_press(combo("q")), Some(combo("q")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_panel_asking_for_a_key_shows_only_with_somewhere_for_it_to_go() {
        let Some(s) = session() else { return };
        let dir = store("key-panel");
        let json = r#"{"title":"Keys","key_press":true,"rows":[{"type":"button","label":"Go"}]}"#;
        let text = |extra: &str| {
            format!(
                r#"(module
                    (import "procraft" "show_panel" (func $show (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "{}")
                    (func (export "procraft_action") (param i32) (result i32)
                        (drop (call $show (i32.const 0) (i32.const {}))) (i32.const 0))
                    (func (export "procraft_panel_event") (param i32 f32) (result i32) (i32.const 0))
                    {extra})"#,
                json.replace('"', "\\\""),
                json.len()
            )
        };
        let plugins = acting(&dir, &wat::parse_str(text("")).unwrap(), json!(["panel"]));
        assert!(plugins.run_action(format!("{ID}.paint"), Arc::clone(&s)));
        assert_eq!(plugins.panel(), None, "no procraft_panel_key");
        let _ = std::fs::remove_dir_all(&dir);
        let takes = r#"(func (export "procraft_key_buffer") (param i32) (result i32) (i32.const 1024))
            (func (export "procraft_panel_key") (param i32) (result i32) (i32.const 0))"#;
        let plugins = acting(&dir, &wat::parse_str(text(takes)).unwrap(), json!(["panel"]));
        assert!(plugins.run_action(format!("{ID}.paint"), Arc::clone(&s)));
        assert!(plugins.panel().is_some_and(|p| p.key_press));
        // it takes the next press, and shows no more asking for one
        assert!(plugins.panel_key(combo("k"), Arc::clone(&s)));
        assert!(plugins.panel().is_some_and(|p| !p.key_press));
        assert!(!plugins.panel_key(combo("k"), Arc::clone(&s)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_picker_plugin_draws_classic_and_says_where_touches_land() {
        let Some(s) = session() else { return };
        let dir = store("picker");
        let plugins = Plugins::with_store(dir.clone());
        let file = crate::plugin_file::tests::picker_file(ID, Some(crate::plugin_run::tests::PICTURE));
        assert_eq!(plugins.install(file), None);
        assert!(!plugins.picker_on(), "off until turned on");
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        assert!(plugins.picker_on());
        assert_eq!(plugins.picker_pick(0.25, 0.75), Some(vec![0.25, 0.25]));
        assert_eq!(plugins.picker_point(0.5, 0.0), Some(vec![0.5, 1.0]));
        // its picture, for the hue asked: red across, green down, the hue in blue
        let rgba = s.picker_picture(0.5, 8).expect("drawn");
        assert_eq!(rgba.len(), 8 * 8 * 4);
        let last = &rgba[rgba.len() - 4..];
        assert!(last[0] > 230 && last[1] > 230 && (last[2] as i32 - 128).abs() <= 1 && last[3] == 255, "{last:?}");
        // off, there is none
        assert!(plugins.set_on(ID.to_owned(), false));
        assert!(!plugins.picker_on());
        assert_eq!(s.picker_picture(0.5, 8), None);
        assert_eq!(plugins.picker_pick(0.25, 0.75), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_picture_written_to_interface_6_is_drawn_again_as_its_controls_say() {
        let Some(s) = session() else { return };
        let dir = store("picker-given");
        let plugins = Plugins::with_store(dir.clone());
        // a picker with a switch, its picture red while the switch is on
        let picks = crate::plugin_run::tests::PICKS;
        let text = format!(
            r#"(module (func (export "procraft_control") (param i32 f32)) {})"#,
            &picks["(module".len()..picks.len() - 1]
        );
        let module = wat::parse_str(text).unwrap();
        let picture = "fn picture(at: vec2<f32>) -> vec4<f32> { return vec4<f32>(v.red, 0.0, 0.0, 1.0); }";
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["picker"]);
        manifest["compat"]["procraft"]["interface"] = json!(6);
        manifest["controls"] = json!([{ "key": "red", "type": "switch", "label": "Red" }]);
        let listed = json!({ "path": "picture.wgsl", "sha256": crate::plugin_file::sha256(picture.as_bytes()), "size": picture.len() });
        manifest["files"].as_array_mut().unwrap().push(listed);
        let file = pack(&manifest, &[(MODULE, &module), ("picture.wgsl", picture.as_bytes())]);
        assert_eq!(plugins.install(file), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        let red = |s: &CanvasSession| s.picker_picture(0.0, 4).expect("drawn")[0];
        assert_eq!(red(&s), 0);
        let before = plugins.generation();
        assert!(plugins.set_switch(format!("{ID}.red"), true));
        assert_ne!(plugins.generation(), before, "what drew it changed");
        assert_eq!(red(&s), 255);
        // the same text written to interface 2 reads no controls: it can't be imported
        manifest["compat"]["procraft"]["interface"] = json!(2);
        let older = pack(&manifest, &[(MODULE, &module), ("picture.wgsl", picture.as_bytes())]);
        let older_dir = store("picker-given-2");
        assert!(Plugins::with_store(older_dir.clone()).install(older).is_some());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&older_dir);
    }

    #[test]
    fn a_picker_picture_that_never_ends_switches_its_plugin_off() {
        let Some(s) = session() else { return };
        let dir = store("picker-loops");
        let plugins = Plugins::with_store(dir.clone());
        let loops =
            "fn picture(at: vec2<f32>) -> vec4<f32> { var x = at.x; loop { x = x + 1.0; } return vec4<f32>(x); }";
        assert_eq!(plugins.install(crate::plugin_file::tests::picker_file(ID, Some(loops))), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(s.picker_picture(0.0, 64), None);
        let card = plugins.plugins().into_iter().find(|p| p.id == ID).unwrap();
        assert!(!card.on);
        assert_eq!(card.problem.as_deref(), Some("Switched off: it took too long."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_session_showing_a_plugin_s_artwork_sends_it_strokes_and_undoes_by_its_steps() {
        let Some(s) = session() else { return };
        let dir = store("plugin-artwork");
        let plugins = Plugins::with_store(dir.clone());
        let file = crate::plugin_file::tests::dots_file(ID, json!([{ "key": "dots", "label": "Dots" }]));
        assert_eq!(plugins.install(file), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        let kind = format!("{ID}.dots");
        let canvas = plugins.open_canvas(kind.clone(), 200, 200, None).unwrap();
        // only a canvas its size
        let small = plugins.open_canvas(kind.clone(), 100, 100, None).unwrap();
        assert!(!s.attach_plugin_canvas(kind.clone(), small));
        assert!(s.attach_plugin_canvas(kind.clone(), canvas));
        assert_eq!(s.plugin_kind(), Some(kind));
        let red_at = |s: &CanvasSession, x: usize, y: usize| {
            picture(s)[(y * 200 + x) * 4..(y * 200 + x) * 4 + 4] == [255, 0, 0, 255]
        };
        // a stroke goes to it, not to the brush: the dot follows the pen and the brush draws nothing
        s.stroke_begin(PointerKind::Stylus, vec![sample(40.0, 40.0, 0.0)]);
        s.stroke_add(vec![sample(60.0, 40.0, 8.0)]);
        s.stroke_end(vec![sample(100.0, 120.0, 16.0)]);
        assert!(red_at(&s, 100, 120), "the dot where the stroke ended");
        assert!(!red_at(&s, 40, 40) && !red_at(&s, 60, 40), "nothing else drawn");
        assert!(s.info().can_undo);
        let first = s.plugin_data().unwrap();
        // another, then undone back to the first, and redone
        s.stroke_begin(PointerKind::Stylus, vec![sample(30.0, 160.0, 0.0)]);
        s.stroke_end(vec![]);
        assert!(red_at(&s, 30, 160) && !red_at(&s, 100, 120));
        s.undo();
        assert!(red_at(&s, 100, 120) && !red_at(&s, 30, 160));
        assert_eq!(s.plugin_data().unwrap(), first);
        assert!(s.info().can_redo);
        s.redo();
        assert!(red_at(&s, 30, 160));
        // saved, its kind named and its data beside; opened again, its plugin takes it back
        let folder = store("plugin-artwork-saved");
        s.save(folder.clone()).unwrap();
        let json = std::fs::read_to_string(std::path::Path::new(&folder).join("artwork.json")).unwrap();
        assert!(json.contains(&format!("\"plugin\":\"{ID}.dots\"")), "{json}");
        let data = std::fs::read(std::path::Path::new(&folder).join(crate::project::PLUGIN_DATA)).unwrap();
        assert_eq!(Some(data.clone()), s.plugin_data());
        let opened = CanvasSession::open(folder.clone()).unwrap();
        assert_eq!(opened.plugin_kind(), Some(format!("{ID}.dots")));
        assert!(opened.open_plugin_artwork(Arc::clone(&plugins)));
        assert_eq!(opened.plugin_data(), Some(data));
        // its plugin off, it opens as what it last drew, the plugin's kind and data kept through a save
        let again = CanvasSession::open(folder.clone()).unwrap();
        assert!(plugins.set_on(ID.to_owned(), false));
        assert!(!again.open_plugin_artwork(Arc::clone(&plugins)));
        again.save(folder.clone()).unwrap();
        let json = std::fs::read_to_string(std::path::Path::new(&folder).join("artwork.json")).unwrap();
        assert!(json.contains(&format!("{ID}.dots")));
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Plugins with the artworks of marks (`MARKS`) on, going through a store `name` of their own,
    /// and the kind those artworks are.
    fn marking(name: &str) -> (String, Arc<Plugins>, String) {
        let dir = store(name);
        let plugins = Plugins::with_store(dir.clone());
        let file = crate::plugin_file::tests::marks_file(ID, json!([{ "key": "marks", "label": "Marks" }]));
        assert_eq!(plugins.install(file), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        (dir, plugins, format!("{ID}.marks"))
    }

    #[test]
    fn a_plugin_s_artwork_draws_strokes_in_the_brush_colour_over_what_it_drew_and_undoes_them() {
        let Some(s) = session() else { return };
        let (dir, plugins, kind) = marking("plugin-artwork-marks");
        let kind = plugins.kinds().into_iter().find(|k| k.key == kind).expect("listed").key;
        let canvas = plugins.open_canvas(kind.clone(), 200, 200, None).unwrap();
        assert!(s.attach_plugin_canvas(kind, canvas));
        s.set_color(0.0, 0.0, 1.0);
        s.set_brush_size(0.5);
        s.stroke_begin(PointerKind::Stylus, vec![sample(40.0, 100.0, 0.0)]);
        s.stroke_add((1..30).map(|i| sample(40.0 + i as f32 * 4.0, 100.0, f64::from(i) * 8.0)).collect());
        s.stroke_end(vec![sample(160.0, 100.0, 248.0)]);
        let rgba = picture(&s);
        let at = |x: usize, y: usize| rgba[(y * 200 + x) * 4..(y * 200 + x) * 4 + 4].to_vec();
        assert_eq!(at(100, 100), [0, 0, 255, 255], "a blue stroke through the middle");
        assert_eq!(at(100, 40)[3], 0, "and nothing far from it");
        // a second stroke is drawn over the first's picture: both show, as drawn whole
        s.set_color(1.0, 0.0, 0.0);
        s.stroke_begin(PointerKind::Stylus, vec![sample(100.0, 40.0, 0.0)]);
        s.stroke_add((1..30).map(|i| sample(100.0, 40.0 + i as f32 * 4.0, f64::from(i) * 8.0)).collect());
        s.stroke_end(vec![sample(100.0, 160.0, 248.0)]);
        let both = picture(&s);
        let at2 = |x: usize, y: usize| both[(y * 200 + x) * 4..(y * 200 + x) * 4 + 4].to_vec();
        assert_eq!(at2(60, 100), [0, 0, 255, 255], "the first still there");
        assert_eq!(at2(100, 60), [255, 0, 0, 255], "the second over it");
        s.undo();
        assert_eq!(picture(&s)[(60 * 200 + 100) * 4 + 3], 0, "the second undone");
        // shared as SVG, its shapes kept in its colour
        assert!(s.exports_svg());
        let svg = std::path::Path::new(&dir).join("artwork.svg");
        s.export_svg(svg.to_string_lossy().into_owned()).unwrap();
        let text = std::fs::read_to_string(&svg).unwrap();
        assert!(text.starts_with("<svg ") && text.contains("fill=\"#0000ff\""), "{text}");
        s.undo();
        assert_eq!(picture(&s)[(100 * 200 + 100) * 4 + 3], 0, "undone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_s_artwork_keeps_its_layer_its_steps_and_its_data_honestly() {
        let Some(s) = session() else { return };
        let (dir, plugins, kind) = marking("plugin-artwork-honest");
        let canvas = plugins.open_canvas(kind.clone(), 200, 200, None).unwrap();
        assert!(s.attach_plugin_canvas(kind.clone(), canvas));
        let art_layer = s.layers().iter().find(|l| l.current).unwrap().id;
        // a stroke called off leaves no step to undo
        s.stroke_begin(PointerKind::Stylus, vec![sample(40.0, 100.0, 0.0)]);
        s.stroke_add(vec![sample(60.0, 100.0, 8.0)]);
        s.stroke_cancel();
        assert!(!s.info().can_undo, "nothing to undo after a stroke called off");
        // with no step of its own, undo takes back the canvas's (a layer added)
        let before = s.layers().len();
        s.add_layer();
        assert_eq!(s.layers().len(), before + 1);
        s.undo();
        assert_eq!(s.layers().len(), before, "the layer added undone");
        // a layer added keeps the plugin's picture where it was, through a save
        let added = s.add_layer();
        s.set_color(0.0, 0.0, 1.0);
        s.set_brush_size(0.5);
        s.stroke_begin(PointerKind::Stylus, vec![sample(40.0, 100.0, 0.0)]);
        s.stroke_add((1..30).map(|i| sample(40.0 + i as f32 * 4.0, 100.0, f64::from(i) * 8.0)).collect());
        s.stroke_end(vec![sample(160.0, 100.0, 248.0)]);
        let alpha_on = |s: &CanvasSession, layer: u64| {
            let mut st = s.shared.lock();
            st.layer_pixels(procraft_engine::document::LayerId(layer)).unwrap()[(100 * 200 + 100) * 4 + 3]
        };
        assert_eq!(alpha_on(&s, art_layer), 255, "drawn on the plugin's layer");
        assert_eq!(alpha_on(&s, added), 0, "not on the layer added");
        let folder = store("plugin-artwork-honest-saved");
        s.save(folder.clone()).unwrap();
        let opened = CanvasSession::open(folder.clone()).unwrap();
        assert!(opened.open_plugin_artwork(Arc::clone(&plugins)));
        assert_eq!(opened.shared.lock().plugin_artwork.as_ref().map(|a| a.layer.0), Some(art_layer));
        // its data gone, it opens as what it last drew, not as a new, empty artwork
        std::fs::remove_file(std::path::Path::new(&folder).join(crate::project::PLUGIN_DATA)).unwrap();
        let bare = CanvasSession::open(folder.clone()).unwrap();
        assert!(!bare.open_plugin_artwork(Arc::clone(&plugins)));
        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_s_artwork_is_told_the_eraser_and_takes_away_what_it_passes_over() {
        let Some(s) = session() else { return };
        let (dir, plugins, kind) = marking("plugin-artwork-eraser");
        assert!(s.attach_plugin_canvas(kind.clone(), plugins.open_canvas(kind, 200, 200, None).unwrap()));
        s.set_color(0.0, 0.0, 1.0);
        s.set_brush_size(0.5);
        s.stroke_begin(PointerKind::Stylus, vec![sample(40.0, 100.0, 0.0)]);
        s.stroke_add((1..30).map(|i| sample(40.0 + i as f32 * 4.0, 100.0, f64::from(i) * 8.0)).collect());
        s.stroke_end(vec![sample(160.0, 100.0, 248.0)]);
        let alpha = |s: &CanvasSession, x: usize| picture(s)[(100 * 200 + x) * 4 + 3];
        assert_eq!((alpha(&s, 60), alpha(&s, 100), alpha(&s, 140)), (255, 255, 255));
        // the eraser down across the middle: the middle goes, the ends stay; one undo brings it back
        s.set_tool(crate::ToolKind::Erase);
        s.stroke_begin(PointerKind::Stylus, vec![sample(100.0, 80.0, 0.0)]);
        s.stroke_add((1..10).map(|i| sample(100.0, 80.0 + i as f32 * 4.0, f64::from(i) * 8.0)).collect());
        s.stroke_end(vec![sample(100.0, 120.0, 88.0)]);
        assert_eq!((alpha(&s, 60), alpha(&s, 100), alpha(&s, 140)), (255, 0, 255));
        s.undo();
        assert_eq!(alpha(&s, 100), 255);
        // painting again, it adds
        s.set_tool(crate::ToolKind::Paint);
        s.stroke_begin(PointerKind::Stylus, vec![sample(100.0, 20.0, 0.0)]);
        s.stroke_end(vec![sample(100.0, 20.0, 8.0)]);
        assert_eq!(picture(&s)[(20 * 200 + 100) * 4 + 3], 255);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_action_that_crashes_goes_off_and_leaves_one_step_to_undo() {
        let Some(s) = session() else { return };
        let dir = store("action-crash");
        let crashes = r#"(module
            (import "procraft" "add_layer" (func $add (result i64)))
            (func (export "procraft_action") (param i32) (result i32)
                (drop (call $add)) (unreachable)))"#;
        let plugins = acting(&dir, &wat::parse_str(crashes).unwrap(), json!(["artwork.write"]));
        let before = s.layers().len();
        assert!(!plugins.run_action(format!("{ID}.paint"), Arc::clone(&s)));
        let plugin = plugins.plugins().into_iter().find(|p| p.id == ID).unwrap();
        assert!(!plugin.on);
        assert_eq!(plugin.problem.as_deref(), Some("Switched off: it crashed."));
        // what it did before is still one step
        assert_eq!(s.layers().len(), before + 1);
        s.undo();
        assert_eq!(s.layers().len(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn filling_by_all(dir: &str) -> Arc<Plugins> {
        let module =
            wat::parse_str(r#"(module (func (export "procraft_fill_all_layers") (result i32) (i32.const 1)))"#)
                .unwrap();
        let plugins = Plugins::with_store(dir.to_owned());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["fill_all_layers"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        plugins
    }

    #[test]
    fn a_fill_goes_by_every_layer_only_while_a_plugin_says_so() {
        let dir = store("fill-all");
        let plugins = filling_by_all(&dir);
        let hooks = plugins.hooks();
        assert!(!hooks.fill_all_layers(), "imported, it says nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert!(hooks.fill_all_layers());
        assert!(plugins.set_on(ID.to_owned(), false));
        assert!(!hooks.fill_all_layers());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn colordrop_on_an_empty_layer_stops_at_another_layers_lines_with_the_plugin_on() {
        let Some(s) = session() else { return };
        let dir = store("fill-all-session");
        // a line across the canvas on the first layer, then an empty layer over it
        s.set_brush_size(0.5);
        s.stroke_begin(PointerKind::Finger, vec![sample(100.0, -10.0, 0.0)]);
        s.stroke_add((1..=44).map(|i| sample(100.0, -10.0 + i as f32 * 5.0, f64::from(i) * 8.0)).collect());
        s.stroke_end(vec![]);
        let line = s.layers()[0].id;
        let empty = s.add_layer();
        assert_ne!(empty, line);
        let red_at = |s: &CanvasSession, x: usize| {
            s.await_pour();
            let rgba = picture(s);
            let at = (100 * 200 + x) * 4;
            rgba[at] > 200 && rgba[at + 3] > 200
        };
        // as the app is: the empty layer fills whole
        assert!(s.pour_begin(40.0, 100.0, 1.0, 0.0, 0.0, 0.3));
        s.pour_end(true, None);
        assert!(red_at(&s, 40) && red_at(&s, 160), "fills the empty layer whole");
        s.undo();
        // with the plugin on: as far as the line on the layer under it
        let plugins = filling_by_all(&dir);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        assert!(s.pour_begin(40.0, 100.0, 1.0, 0.0, 0.0, 0.3));
        s.pour_end(true, None);
        assert!(red_at(&s, 40), "filled where it was dropped");
        assert!(!red_at(&s, 160), "not past the other layer's line");
        assert_eq!(s.layers().iter().find(|l| l.current).map(|l| l.id), Some(empty), "on the current layer");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn colordrop_by_all_layers_stops_at_a_light_line_as_at_a_dark_one() {
        let Some(s) = session() else { return };
        let dir = store("fill-all-light");
        // a light blue ring on the first layer, over the white background
        s.set_color(0.55, 0.8, 0.95);
        s.set_brush_size(0.2);
        let ring: Vec<PenSample> = (0..=120)
            .map(|i| {
                let a = i as f32 / 120.0 * std::f32::consts::TAU;
                sample(100.0 + 50.0 * a.cos(), 100.0 + 50.0 * a.sin(), f64::from(i) * 8.0)
            })
            .collect();
        s.stroke_begin(PointerKind::Finger, ring[..1].to_vec());
        s.stroke_add(ring[1..].to_vec());
        s.stroke_end(vec![]);
        // seen over white, every pixel of it is nearer to white than a drop's first threshold
        // reaches: a fill going by the canvas as it shows would pass it
        let line = picture(&s);
        let from_white =
            line.as_chunks::<4>().0.iter().map(|p| (0..3).map(|c| p[3].saturating_sub(p[c])).max().unwrap_or(0));
        assert!(from_white.max().unwrap() < 128);
        s.add_layer();
        let plugins = filling_by_all(&dir);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        // dropped inside it on the empty layer at the threshold a drop starts with: it fills up to
        // the ring and no further, as it does at a black ring
        assert!(s.pour_begin(100.0, 100.0, 0.0, 0.0, 1.0, 0.5));
        s.pour_end(true, None);
        s.await_pour();
        let rgba = picture(&s);
        let blue_at = |x: usize, y: usize| rgba[(y * 200 + x) * 4 + 2] > 200 && rgba[(y * 200 + x) * 4 + 3] > 200;
        assert!(blue_at(100, 100) && blue_at(80, 120), "filled where it was dropped");
        for (x, y) in [(100, 20), (20, 100), (180, 100), (100, 180), (5, 5), (195, 195)] {
            assert!(!blue_at(x, y), "not past the ring at ({x}, {y})");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_interface_takes_a_plugin_s_colour_while_it_is_on() {
        let dir = store("interface-colour");
        // red for the accent, nothing said for any other role
        let module = wat::parse_str(
            r#"(module (func (export "procraft_interface_colour") (param $role i32) (result i32)
                (if (result i32) (i32.eqz (local.get $role)) (then (i32.const 0xff0000)) (else (i32.const -1)))))"#,
        )
        .unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["interface_colour"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert_eq!(plugins.accent_colour(), None, "imported, it says nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(plugins.accent_colour(), Some(0xFF0000));
        assert_eq!(plugins.hooks().interface_colour(1), None);
        assert!(plugins.set_on(ID.to_owned(), false));
        assert_eq!(plugins.accent_colour(), None);
        // a colour past 24 bits stops it
        let wild = wat::parse_str(
            r#"(module (func (export "procraft_interface_colour") (param i32) (result i32) (i32.const 0x1000000)))"#,
        )
        .unwrap();
        let dir2 = store("interface-colour-wild");
        let plugins = Plugins::with_store(dir2.clone());
        let mut manifest = coded(ID, &wild);
        manifest["hooks"] = json!(["interface_colour"]);
        assert_eq!(plugins.install(file(&manifest, &wild)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(plugins.accent_colour(), None);
        assert!(card(&plugins).problem.is_some());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    /// Brush extras, which the app comes with, in a store of its own, `feature` on, the plugin not
    /// on yet.
    fn brush_extras(dir: &str, feature: &str) -> Arc<Plugins> {
        let plugins = Plugins::with_store(dir.to_owned());
        let (_, bytes) = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "brush-extras").unwrap();
        assert_eq!(plugins.install(bytes.to_vec()), None);
        assert!(plugins.set_switch(format!("{SOUNDS}.{feature}"), true));
        plugins
    }

    /// Brush extras with Brush sounds on, as [`brush_extras`] has it.
    fn brush_sounds(dir: &str) -> Arc<Plugins> {
        brush_extras(dir, "brush_sounds")
    }

    const SOUNDS: &str = "io.github.artcraftandroid.brush-extras";

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
    }

    #[test]
    fn a_stroke_sounds_as_fast_as_it_moves_while_the_sound_plugin_is_on() {
        let Some(s) = session() else { return };
        let dir = store("sound");
        let plugins = brush_sounds(&dir);
        s.use_plugins(Arc::clone(&plugins));
        // a fast stroke under way
        let fast: Vec<_> = (0..20).map(|i| sample(10.0 + i as f32 * 10.0, 50.0, f64::from(i) * 4.0)).collect();
        s.stroke_begin(PointerKind::Finger, fast[..1].to_vec());
        s.stroke_add(fast[1..].to_vec());
        assert!(!plugins.makes_stroke_sound());
        assert_eq!(rms(&s.stroke_sound(2_400, 48_000)), 0.0, "off: silent");
        assert!(plugins.set_on(SOUNDS.to_owned(), true));
        assert!(plugins.makes_stroke_sound());
        // (the first readies the plugin's sounds)
        s.stroke_add(vec![sample(220.0, 50.0, 84.0)]);
        let _ = s.stroke_sound(2_400, 48_000);
        s.stroke_add(vec![sample(240.0, 50.0, 88.0)]);
        let heard = rms(&s.stroke_sound(2_400, 48_000));
        let problem = plugins.plugins().into_iter().find(|p| p.id == SOUNDS).and_then(|p| p.problem);
        assert!(heard > 0.005, "a fast stroke is heard: {heard}, {problem:?}");
        // its volume down: silent
        assert!(plugins.set_slider(format!("{SOUNDS}.volume"), 0.0).is_some());
        s.stroke_add(vec![sample(260.0, 50.0, 92.0)]);
        let _ = s.stroke_sound(2_400, 48_000);
        s.stroke_add(vec![sample(280.0, 50.0, 96.0)]);
        assert!(rms(&s.stroke_sound(2_400, 48_000)) < 1e-4, "at no volume");
        // it ends: quiet again
        assert!(plugins.set_slider(format!("{SOUNDS}.volume"), 1.0).is_some());
        s.stroke_end(vec![]);
        let _ = s.stroke_sound(9_600, 48_000);
        assert!(rms(&s.stroke_sound(2_400, 48_000)) < 0.002);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_recording_picked_is_kept_with_its_plugin_and_played_by_it() {
        let Some(s) = session() else { return };
        let dir = store("recording");
        let plugins = brush_sounds(&dir);
        assert!(plugins.set_on(SOUNDS.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        // every brush the airbrush, which hisses while pressed; its recording a 440 Hz tone
        assert!(plugins.set_slider(format!("{SOUNDS}.sound"), 5.0).is_some());
        let tone: Vec<f32> =
            (0..22_050).map(|i| 0.7 * (std::f32::consts::TAU * 440.0 * i as f32 / 44_100.0).sin()).collect();
        let key = format!("{SOUNDS}.airbrush_recording");
        assert!(!plugins.set_sound_file(key.clone(), "x".to_owned(), b"not a sound".to_vec()));
        assert!(plugins.set_sound_file(
            key.clone(),
            "Tone.wav".to_owned(),
            crate::sound_file::write_wav(&tone, 44_100)
        ));
        s.stroke_begin(PointerKind::Finger, vec![sample(100.0, 100.0, 0.0)]);
        let _ = s.stroke_sound(9_600, 48_000);
        s.stroke_add(vec![sample(101.0, 100.0, 4.0)]);
        let heard = s.stroke_sound(48_000, 48_000);
        let crossings = heard.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        assert!((800..960).contains(&crossings), "the tone: {crossings}");
        s.stroke_end(vec![]);
        // kept with the plugin, read back as it was (its feature on as kept), not among the settings
        let again = restarted(&dir, &plugins);
        let chosen = again.plugins().into_iter().flat_map(|p| p.controls).find_map(|c| match c {
            crate::plugins::PluginControl::File { control } if control.key == key => control.chosen,
            _ => None,
        });
        assert_eq!(chosen.as_deref(), Some("Tone.wav"));
        assert!(again.kept_lines().iter().all(|line| !line.starts_with(&key)));
        // let go: the file gone
        let (file, _) = crate::plugin_file::chosen_paths(std::path::Path::new(&dir), SOUNDS, &key);
        assert!(file.is_file());
        assert!(plugins.clear_file(key));
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sound_plugin_s_row_in_the_brush_studio_is_kept_with_the_brush() {
        let dir = store("rows");
        let plugins = brush_sounds(&dir);
        let rows = plugins.plugins().into_iter().find(|p| p.id == SOUNDS).unwrap().brush_controls;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].key.as_str(), rows[0].choices.len()),
            ("plugin:io.github.artcraftandroid.brush-sounds:sound", 8)
        );
        let mut brush = procraft_engine::brush::Brush::round_pen();
        brush.set_setting(&rows[0].key, 5.0);
        assert_eq!(brush.setting(&rows[0].key), 5.0);
        brush.set_setting(&rows[0].key, 300.0);
        assert_eq!(brush.setting(&rows[0].key), 255.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quickshape_curves_stay_on_for_dense_and_wavy_strokes() {
        let dir = store("curves-dense-samples");
        let plugins = brush_extras(&dir, "quickshape_curves");
        const ID: &str = SOUNDS;
        assert!(plugins.set_on(ID.to_owned(), true));
        let hooks = plugins.hooks();
        // a curve that ran out of its budget would switch the plugin off, and every held stroke
        // after it would be a polyline: thousands of pen samples, and many bends over a few hundred
        // (the fit's most work), stay within it
        let dense = |count: usize| -> Vec<(f32, f32)> {
            (0..count)
                .map(|i| {
                    let x = i as f32 * 600.0 / (count - 1) as f32;
                    let tau = std::f32::consts::TAU;
                    (x, 120.0 + 55.0 * (x * tau / 300.0).sin() + 3.0 * (x * tau / 23.0).sin())
                })
                .collect()
        };
        let wavy = |count: usize, waves: f32| -> Vec<(f32, f32)> {
            (0..count)
                .map(|i| {
                    let t = i as f32 / (count - 1) as f32;
                    let tau = std::f32::consts::TAU;
                    (t * 1600.0, 400.0 + 300.0 * (t * waves * tau).sin() + 6.0 * (i as f32 * 1.7).sin())
                })
                .collect()
        };
        let strokes =
            [dense(4_001), dense(12_001), wavy(400, 4.0), wavy(257, 12.0), wavy(1000, 12.0), wavy(5000, 25.0)];
        for points in &strokes {
            let nodes = hooks.quickshape_curve(points, 1).expect("a curve");
            assert!(nodes.len() >= 10 && nodes.len() % 3 == 1, "{} samples: {} nodes", points.len(), nodes.len());
            assert!(hooks.is_on(ID), "off after {} samples", points.len());
        }
        assert!(plugins.plugins().iter().find(|p| p.id == ID).is_some_and(|p| p.on && p.problem.is_none()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_stroke_bending_twice_snaps_to_a_curve_while_a_plugin_says_so() {
        use crate::{QuickShapeAnchor, QuickShapeKind};
        let Some(s) = session() else { return };
        let dir = store("curves");
        let plugins = brush_extras(&dir, "quickshape_curves");
        const ID: &str = SOUNDS;
        s.use_plugins(Arc::clone(&plugins));
        // a zigzag: across, back and across again, as a hand draws one
        let zigzag = |s: &CanvasSession, t0: f64| {
            let points = [(20.0, 20.0), (180.0, 30.0), (30.0, 100.0), (180.0, 170.0)];
            let mut samples = Vec::new();
            for (k, pair) in points.windows(2).enumerate() {
                let (a, b) = (pair[0], pair[1]);
                for i in 0..20 {
                    let f = i as f32 / 20.0;
                    let at = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
                    samples.push(sample(at.0, at.1, t0 + ((k * 20 + i) * 8) as f64));
                }
            }
            s.stroke_begin(PointerKind::Finger, samples[..1].to_vec());
            s.stroke_add(samples[1..].to_vec());
            s.quickshape_hold()
        };
        // off: a polyline, as the app is
        let shape = zigzag(&s, 0.0).expect("it snaps");
        assert_eq!(shape.kind, QuickShapeKind::Polyline);
        s.quickshape_done();
        s.stroke_end(vec![]);
        // on: a curve, the polyline to be shown instead
        assert!(plugins.set_on(ID.to_owned(), true));
        let shape = zigzag(&s, 1000.0).expect("it snaps");
        assert_eq!(shape.kind, QuickShapeKind::Curve, "{shape:?}");
        assert!(shape.choices.contains(&QuickShapeKind::Polyline));
        assert!(shape.anchors.len() >= 3);
        assert_eq!(shape.anchors[1], QuickShapeAnchor::Broken, "a deliberate sharp turn needs independent handles");
        // a tap cycles a detected broken anchor through corner and smooth
        let shape = s.quickshape_cycle_anchor(3).expect("an anchor");
        assert_eq!(shape.anchors[1], QuickShapeAnchor::Corner);
        let shape = s.quickshape_cycle_anchor(3).expect("an anchor");
        assert_eq!(shape.anchors[1], QuickShapeAnchor::Smooth);
        // a control is no anchor
        assert!(s.quickshape_cycle_anchor(1).is_none());
        // shown as the polyline, then as a curve again: the plugin fits it afresh
        assert_eq!(s.quickshape_choose(QuickShapeKind::Polyline).map(|i| i.kind), Some(QuickShapeKind::Polyline));
        assert_eq!(s.quickshape_choose(QuickShapeKind::Curve).map(|i| i.kind), Some(QuickShapeKind::Curve));
        s.quickshape_done();
        s.stroke_end(vec![]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_loop_bending_in_and_out_snaps_to_a_closed_curve_while_a_plugin_says_so() {
        use crate::QuickShapeKind;
        let Some(s) = session() else { return };
        let dir = store("closed-curves");
        let plugins = brush_extras(&dir, "quickshape_curves");
        s.use_plugins(Arc::clone(&plugins));
        // a cloud: once round, bulging out and dipping in five times, ending where it began
        let cloud = |s: &CanvasSession, t0: f64| {
            let samples: Vec<PenSample> = (0..=100)
                .map(|i| {
                    let t = i as f32 / 100.0 * std::f32::consts::TAU;
                    let r = 60.0 * (1.0 + 0.3 * (5.0 * t).cos());
                    sample(100.0 + r * t.cos(), 100.0 + r * t.sin(), t0 + f64::from(i) * 8.0)
                })
                .collect();
            s.stroke_begin(PointerKind::Finger, samples[..1].to_vec());
            s.stroke_add(samples[1..].to_vec());
            s.quickshape_hold()
        };
        // off: QuickShape's ellipse
        assert_eq!(cloud(&s, 0.0).expect("it snaps").kind, QuickShapeKind::Ellipse);
        s.quickshape_done();
        s.stroke_end(vec![]);
        assert!(plugins.set_on(SOUNDS.to_owned(), true));
        let shape = cloud(&s, 2000.0).expect("it snaps");
        assert_eq!(shape.kind, QuickShapeKind::Curve, "{shape:?}");
        assert!(shape.choices.contains(&QuickShapeKind::Ellipse));
        // its last anchor is its first: dragged, both go
        let at = |h: &[f32], i: usize| (h[2 * i], h[2 * i + 1]);
        let last = shape.handles.len() / 2 - 1;
        assert_eq!(at(&shape.handles, 0), at(&shape.handles, last));
        let moved = s.quickshape_move_handle(0, 20.0, 30.0).unwrap();
        assert_eq!(at(&moved.handles, last), at(&moved.handles, 0));
        // tapped there it steps on as one anchor, at either end
        let tapped = s.quickshape_cycle_anchor(last as u32).unwrap();
        assert_eq!(tapped.anchors.first(), tapped.anchors.last());
        assert_eq!(tapped.anchors[0], crate::QuickShapeAnchor::Broken);
        s.quickshape_done();
        s.stroke_end(vec![]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn randomise_sets_the_studio_brush_as_a_plugin_says() {
        let Some(s) = session() else { return };
        let dir = store("randomise");
        // every setting to the middle of its range, but the spacing (0) left alone
        let module = wat::parse_str(
            r#"(module (func (export "procraft_randomise") (param i32 i32) (result f32)
                (if (result f32) (i32.eqz (local.get 0)) (then (f32.const -1)) (else (f32.const 0.5)))))"#,
        )
        .unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["randomise_brush"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        s.use_plugins(Arc::clone(&plugins));
        let _ = s.studio_begin();
        let keys = |keys: &[&str]| keys.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>();
        let before = s.studio_settings(keys(&["spacing", "streamline", "shape_rotation", "rendering_mode"]));
        assert!(!plugins.randomises_brushes());
        assert!(!s.studio_randomise(), "off, nothing changes");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert!(plugins.randomises_brushes());
        assert!(s.studio_randomise());
        let after = s.studio_settings(keys(&["spacing", "streamline", "shape_rotation", "rendering_mode"]));
        assert_eq!(after[0], before[0], "the spacing left as it was");
        assert!((after[1] - 0.5).abs() < 1e-4, "{after:?}");
        assert!(after[2].abs() < 1e-4, "the middle of -1 to 1: {after:?}");
        assert_eq!(after[3].round(), 3.0, "a choice made whole: {after:?}");
        s.studio_end(false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picked_layers_take_one_opacity_together_while_a_plugin_says_so() {
        let Some(s) = session() else { return };
        let dir = store("opacity-together");
        let module =
            wat::parse_str(r#"(module (func (export "procraft_opacity_together") (result i32) (i32.const 1)))"#)
                .unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["opacity_together"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        s.use_plugins(Arc::clone(&plugins));
        let first = s.layers()[0].id;
        let second = s.add_layer();
        let third = s.add_layer();
        assert!(s.pick_layer(first));
        let opacity = |s: &CanvasSession, id: u64| s.layers().iter().find(|l| l.id == id).map(|l| l.opacity);
        // off: only the one
        assert!(s.set_layer_opacity(third, 0.5, true));
        assert_eq!((opacity(&s, first), opacity(&s, third)), (Some(1.0), Some(0.5)));
        // on: every picked layer, as one step; one not picked keeps its own
        assert!(plugins.set_on(ID.to_owned(), true));
        assert!(s.set_layer_opacity(first, 0.25, false));
        assert!(s.set_layer_opacity(first, 0.3, true));
        assert_eq!((opacity(&s, first), opacity(&s, third)), (Some(0.3), Some(0.3)));
        assert_eq!(opacity(&s, second), Some(1.0));
        s.undo();
        assert_eq!((opacity(&s, first), opacity(&s, third)), (Some(1.0), Some(0.5)));
        // a layer not among the picked goes alone
        assert!(s.set_layer_opacity(second, 0.7, true));
        assert_eq!((opacity(&s, first), opacity(&s, second), opacity(&s, third)), (Some(1.0), Some(0.7), Some(0.5)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_touch_with_a_panel_open_paints_at_once_and_leaves_it_while_a_plugin_pins_it() {
        use crate::touch::{TouchCommand, TouchMove};
        let Some(s) = session() else { return };
        let dir = store("pin-panels");
        let module =
            wat::parse_str(r#"(module (func (export "procraft_pin_panels") (result i32) (i32.const 1)))"#).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["pin_panels"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        s.use_plugins(Arc::clone(&plugins));
        s.set_touch_panel_open(true);
        let closes = |out: &[TouchCommand]| {
            out.iter().any(|c| matches!(c, TouchCommand::PanelTapped | TouchCommand::PanelFades))
        };
        // off: a tap only closes the panel
        let mut out = s.touch_down(1, PointerKind::Finger, vec![sample(100.0, 100.0, 0.0)]);
        out.extend(s.touch_up(1, vec![sample(100.0, 100.0, 40.0)]));
        assert!(out.contains(&TouchCommand::PanelTapped), "{out:?}");
        // on: a tap and a drag paint, and the panel stays
        assert!(plugins.set_on(ID.to_owned(), true));
        let mut out = s.touch_down(2, PointerKind::Finger, vec![sample(100.0, 100.0, 100.0)]);
        out.extend(s.touch_up(2, vec![sample(100.0, 100.0, 140.0)]));
        assert!(!closes(&out), "{out:?}");
        let mut out = s.touch_down(3, PointerKind::Finger, vec![sample(100.0, 100.0, 200.0)]);
        out.extend(s.touch_moves(vec![TouchMove { id: 3, samples: vec![sample(140.0, 100.0, 220.0)] }]));
        out.extend(s.touch_moves(vec![TouchMove { id: 3, samples: vec![sample(180.0, 100.0, 240.0)] }]));
        out.extend(s.touch_up(3, vec![sample(200.0, 100.0, 260.0)]));
        assert!(!closes(&out), "{out:?}");
        assert!(out.contains(&TouchCommand::StrokeEnded), "{out:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_and_redo_show_no_notice_while_a_plugin_hides_it() {
        let Some(s) = session() else { return };
        let dir = store("undo-notice");
        assert_eq!(s.step_notice(true, Some("Brush stroke".into())).as_deref(), Some("Undo Brush stroke"));
        assert_eq!(s.step_notice(false, None).as_deref(), Some("Redo"));
        let module =
            wat::parse_str(r#"(module (func (export "procraft_hide_undo_notice") (result i32) (i32.const 1)))"#)
                .unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["hide_undo_notice"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        s.use_plugins(Arc::clone(&plugins));
        assert!(s.step_notice(true, Some("Brush stroke".into())).is_some(), "imported, it says nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(s.step_notice(true, Some("Brush stroke".into())), None);
        assert_eq!(s.step_notice(false, None), None);
        assert!(plugins.set_on(ID.to_owned(), false));
        assert_eq!(s.step_notice(false, Some("Fill".into())).as_deref(), Some("Redo Fill"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn radial_symmetry_goes_by_a_plugin_s_count_while_it_is_on() {
        let dir = store("radial");
        let module =
            wat::parse_str(r#"(module (func (export "procraft_radial_segments") (result i32) (i32.const 6)))"#)
                .unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["radial_segments"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        let hooks = plugins.hooks();
        assert_eq!(hooks.radial_segments(), None, "imported, it says nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(hooks.radial_segments(), Some(6));
        assert!(plugins.set_on(ID.to_owned(), false));
        assert_eq!(hooks.radial_segments(), None);
        // a count past what is allowed stops it
        let wild =
            wat::parse_str(r#"(module (func (export "procraft_radial_segments") (result i32) (i32.const 1000)))"#)
                .unwrap();
        let dir2 = store("radial-wild");
        let plugins = Plugins::with_store(dir2.clone());
        let mut manifest = coded(ID, &wild);
        manifest["hooks"] = json!(["radial_segments"]);
        assert_eq!(plugins.install(file(&manifest, &wild)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(plugins.hooks().radial_segments(), None);
        assert!(card(&plugins).problem.is_some());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn a_pick_plugin_that_crashes_leaves_the_pick_on_the_canvas() {
        let dir = store("pick-crash");
        let module =
            wat::parse_str(r#"(module (func (export "procraft_pick_layer") (result i32) (unreachable)))"#).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["pick_layer"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        let hooks = plugins.hooks();
        assert!(!hooks.pick_layer());
        assert_eq!(card(&plugins).problem.as_deref(), Some("Switched off: it crashed."));
        assert!(!hooks.has(Hook::PickLayer));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_eyedropper_reads_the_current_layer_while_a_plugin_says_so() {
        let Some(s) = session() else { return };
        // paint on the first layer, then an empty one over it
        draw(&s);
        s.add_layer();
        let shown = s.pick(100.0, 50.0, 21).unwrap().rgb;
        assert!(shown.is_some());
        let dir = store("pick-session");
        let plugins = picking(&dir);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(s.pick(100.0, 50.0, 21).unwrap().rgb, shown, "with none on, the canvas");
        plugins.set_on(ID.to_owned(), true);
        plugins.set_switch("eyedropper_layer".to_owned(), true);
        assert_eq!(s.pick(100.0, 50.0, 21).unwrap().rgb, None, "the current layer is clear there");
        plugins.set_on(ID.to_owned(), false);
        assert_eq!(s.pick(100.0, 50.0, 21).unwrap().rgb, shown);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A plugin that scales the interface by its one slider, in a store at `dir`.
    fn scaling(dir: &str) -> Arc<Plugins> {
        let module = wat::parse_str(SCALES).unwrap();
        let plugins = Plugins::with_store(dir.to_owned());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["interface_scale"]);
        manifest["controls"] = json!([{
            "key": "size", "type": "slider", "label": "Size", "least": 0.5, "most": 2, "step": 0.05, "starts": 1,
            "keeps": "interface_size",
        }]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        plugins
    }

    #[test]
    fn the_interface_shows_as_a_plugin_says_and_as_measured_without_one() {
        let dir = store("scale");
        let plugins = scaling(&dir);
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 1.0, "imported, it says nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(plugins.set_slider("interface_size".to_owned(), 1.2), Some(1.2));
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 1.2);
        assert_eq!(plugins.set_slider("interface_size".to_owned(), 0.75), Some(0.75));
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 0.75);
        assert!(plugins.set_on(ID.to_owned(), false));
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn whatever_a_plugin_says_every_page_stays_in_reach() {
        let dir = store("scale-room");
        let plugins = scaling(&dir);
        assert!(plugins.set_on(ID.to_owned(), true));
        plugins.set_slider("interface_size".to_owned(), 2.0);
        assert_eq!(plugins.interface_scale(2000.0, 1500.0), 2.0);
        // the panels need 640 x 480 points: past what leaves them that, it goes no further
        assert_eq!(plugins.interface_scale(960.0, 1500.0), 1.5);
        assert_eq!(plugins.interface_scale(2000.0, 600.0), 1.25);
        // a window with less shows as measured
        assert_eq!(plugins.interface_scale(640.0, 480.0), 1.0);
        assert_eq!(plugins.interface_scale(500.0, 400.0), 1.0);
        // smaller is the plugin's to say, whatever the window
        plugins.set_slider("interface_size".to_owned(), 0.75);
        assert_eq!(plugins.interface_scale(500.0, 400.0), 0.75);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_kept_interface_size_comes_back_through_a_restart() {
        let dir = store("scale-restart");
        let plugins = scaling(&dir);
        plugins.set_on(ID.to_owned(), true);
        plugins.set_slider("interface_size".to_owned(), 1.25);
        let next = restarted(&dir, &plugins);
        assert_eq!(next.interface_scale(1600.0, 1000.0), 1.25);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_scale_plugin_that_goes_wrong_leaves_the_interface_as_measured() {
        let dir = store("scale-wrong");
        let module = wat::parse_str(
            r#"(module (func (export "procraft_interface_scale") (param f32 f32) (result f32) (f32.const 40)))"#,
        )
        .unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["interface_scale"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 1.0);
        assert_eq!(card(&plugins).problem.as_deref(), Some("Switched off: it gave an answer that makes no sense."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_interface_keeps_clear_of_the_system_while_a_plugin_s_switch_says() {
        let dir = store("clear");
        let module = wat::parse_str(CLEARS).unwrap();
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["keep_clear"]);
        manifest["controls"] = json!([{ "key": "clear", "type": "switch", "label": "Keep clear" }]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(!plugins.keep_clear(), "imported, it says nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert!(!plugins.keep_clear(), "its switch starts off");
        let key = format!("{ID}.clear");
        assert!(plugins.set_switch(key.clone(), true));
        assert!(plugins.keep_clear());
        // kept through a restart
        let next = restarted(&dir, &plugins);
        assert!(next.keep_clear());
        assert!(plugins.set_switch(key, false));
        assert!(!plugins.keep_clear());
        assert!(plugins.set_on(ID.to_owned(), false));
        assert!(!plugins.keep_clear());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A list holding the paste plugin, its switch kept as the setting it took over, in `dir`.
    fn pasting(dir: &str) -> Arc<Plugins> {
        let module = wat::parse_str(PASTES).unwrap();
        let plugins = Plugins::with_store(dir.to_owned());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["paste_mask"]);
        manifest["controls"] = json!([
            { "key": "into", "type": "switch", "label": "Into", "keeps": "paste_into_mask" },
        ]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        plugins
    }

    /// `s` with the paste plugin on, its switch too, and where its files are.
    pub(crate) fn paste_into_masks(s: &CanvasSession, name: &str) -> (Arc<Plugins>, String) {
        let dir = store(name);
        let plugins = pasting(&dir);
        assert!(plugins.set_on(ID.to_owned(), true));
        assert!(plugins.set_switch("paste_into_mask".to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        (plugins, dir)
    }

    #[test]
    fn a_picture_goes_into_the_mask_only_while_a_plugin_says_so() {
        let dir = store("paste");
        let plugins = pasting(&dir);
        let hooks = plugins.hooks();
        assert!(!hooks.paste_mask(), "imported, it says nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert!(!hooks.paste_mask(), "on, it goes by its switch");
        assert!(plugins.set_switch("paste_into_mask".to_owned(), true));
        assert!(hooks.paste_mask());
        assert!(plugins.set_on(ID.to_owned(), false));
        assert!(!hooks.paste_mask());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rows_come_back_as_the_brightness_the_app_used_to_make() {
        let dir = store("paste-rows");
        let plugins = pasting(&dir);
        plugins.set_on(ID.to_owned(), true);
        let hooks = plugins.hooks();
        let colours: [[u8; 4]; 6] = [
            [255, 255, 255, 255],
            [0, 0, 0, 0],
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [10, 200, 90, 200],
        ];
        let mut rgba: Vec<u8> = colours.concat();
        assert!(hooks.paste_rows(&mut rgba, 3));
        for (px, [r, g, b, _]) in rgba.chunks(4).zip(colours.map(|c| c.map(u32::from))) {
            let grey = ((54 * r + 183 * g + 19 * b + 128) >> 8).min(255) as u8;
            assert_eq!(px, [grey, grey, grey, 255]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_paste_plugin_that_stops_leaves_the_picture_to_paste_as_any_is() {
        let Some(s) = session() else { return };
        let layer = s.layers().iter().find(|l| l.current).unwrap().id;
        let mask = s.add_mask(layer).unwrap();
        s.select_layer(mask);
        let dir = store("paste-stops");
        let module = crate::plugin_run::tests::paste_module("(i32.const 65536)", "(unreachable)");
        let plugins = Plugins::with_store(dir.clone());
        let mut manifest = coded(ID, &module);
        manifest["hooks"] = json!(["paste_mask"]);
        assert_eq!(plugins.install(file(&manifest, &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        let layers = s.layers().len();
        let picture = crate::layers::ThumbnailImage { width: 200, height: 200, rgba: vec![255; 200 * 200 * 4] };
        let pasted = s.paste(picture).unwrap();
        assert_ne!(pasted, mask, "a new layer, as anything pasted");
        assert_eq!(s.layers().len(), layers + 1);
        assert_eq!(s.take_failure().as_deref(), Some("A plugin stopped. Switched off: it crashed."));
        assert_eq!(card(&plugins).problem.as_deref(), Some("Switched off: it crashed."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Takes out everything past the first two words of each row, as a plugin of real use might.
    const RIGHT_OF_128: &str = "(local $i i32) (local.set $i (i32.const 16))
        (i64.store (i32.const 65536) (i64.const 0))
        (i64.store (i32.const 65544) (i64.const 0))
        (block $done (loop $next
            (br_if $done (i32.ge_u (local.get $i) (i32.const 32)))
            (i64.store (i32.add (i32.const 65536) (local.get $i)) (i64.const -1))
            (local.set $i (i32.add (local.get $i) (i32.const 8)))
            (br $next)))";

    /// A session with `module` on the fill hook, and the list holding it.
    fn filling(s: &CanvasSession, dir: &str, module: &[u8]) -> Arc<Plugins> {
        let plugins = Plugins::with_store(dir.to_owned());
        let mut manifest = coded(ID, module);
        manifest["hooks"] = json!(["fill_mask"]);
        assert_eq!(plugins.install(file(&manifest, module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        plugins
    }

    /// A layer's pixel, once the fill being finished is done.
    fn red_at(s: &CanvasSession, x: usize, y: usize) -> bool {
        s.await_pour();
        let rgba = picture(s);
        let at = (y * 200 + x) * 4;
        rgba[at] > 200 && rgba[at + 3] > 200
    }

    fn pour_at(s: &CanvasSession, x: f32, y: f32) {
        assert!(s.pour_begin(x, y, 1.0, 0.0, 0.0, 0.3));
        s.pour_end(true, None);
    }

    #[test]
    fn a_fill_is_finished_by_the_plugin_on_its_own_thread_and_is_one_step() {
        let Some(s) = session() else { return };
        let dir = store("fill-real");
        let plugins = filling(&s, &dir, &fill_module(FILL_AT, "(i32.const 1)", RIGHT_OF_128));
        pour_at(&s, 30.0, 30.0);
        assert!(red_at(&s, 30, 30) && red_at(&s, 120, 190));
        assert!(!red_at(&s, 130, 30) && !red_at(&s, 199, 199), "past where the plugin keeps it");
        assert!(!s.pour_finishing());
        // one undo takes the whole fill back
        s.undo();
        assert!(!red_at(&s, 30, 30));
        // off, the same fill runs on through
        plugins.set_on(ID.to_owned(), false);
        pour_at(&s, 30.0, 30.0);
        assert!(red_at(&s, 30, 30) && red_at(&s, 199, 199));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fill_keeps_to_a_selection_with_a_plugin_as_it_does_without() {
        let Some(s) = session() else { return };
        let dir = store("fill-selection");
        filling(&s, &dir, &fill_module(FILL_AT, "(i32.const 1)", RIGHT_OF_128));
        // what is selected: a band across, on both sides of where the plugin cuts
        let mut mask = vec![0u8; 200 * 200];
        for y in 60..120 {
            mask[y * 200..(y + 1) * 200].fill(255);
        }
        s.shared.lock().selection = Some(procraft_engine::selection::Mask { width: 200, height: 200, data: mask });
        pour_at(&s, 30.0, 80.0);
        assert!(red_at(&s, 30, 80), "in the selection, this side of the cut");
        assert!(!red_at(&s, 30, 30), "out of the selection");
        assert!(!red_at(&s, 150, 80), "in the selection, past the cut");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fill_box_goes_through_alone_with_a_pixel_round_it() {
        // a square of paper in a filled layer: the fill's box is a corner of the canvas. The plugin
        // says how much it was handed in the fill's own terms
        let Some(s) = session() else { return };
        let dir = store("fill-box");
        s.set_color(0.0, 0.0, 1.0);
        let layer = s.layers().iter().find(|l| l.current).unwrap().id;
        assert!(s.fill_layer(layer));
        {
            let mut st = s.shared.lock();
            let id = st.doc.current();
            let mut pixels = st.layer_pixels(id).unwrap();
            for y in 10..50 {
                for x in 20..70 {
                    pixels[(y * 200 + x) * 4..(y * 200 + x + 1) * 4].fill(0);
                }
            }
            st.canvas.as_mut().unwrap().write_layer(id, pixels);
        }
        // the row it is told of: 52 wide, 42 tall (50 x 40 and a pixel each way), the seed in terms
        // of that: anything else and it says nothing comes out
        let module = fill_module(
            "(if (i32.or (i32.ne (local.get 0) (i32.const 52)) (i32.or (i32.ne (local.get 1) (i32.const 42))
                (i32.or (i32.ne (local.get 2) (i32.const 11)) (i32.ne (local.get 3) (i32.const 11)))))
                (then (unreachable)))
             (i32.const 65536)",
            "(i32.const 1)",
            "(i64.store (i32.const 65536) (i64.const 0x00ffff0000000000))",
        );
        let plugins = filling(&s, &dir, &module);
        pour_at(&s, 30.0, 20.0);
        assert!(red_at(&s, 30, 20));
        assert!(!red_at(&s, 19 + 45, 20), "taken out, at its place in the box");
        assert_eq!(card(&plugins).problem, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a plugin that goes wrong on a fill must come to: the fill as it was, one step, and why.
    fn fill_goes_wrong(name: &str, module: &[u8], reason: &str) {
        let Some(s) = session() else { return };
        let dir = store(name);
        let plugins = filling(&s, &dir, module);
        pour_at(&s, 30.0, 30.0);
        assert!(red_at(&s, 30, 30) && red_at(&s, 199, 199), "the fill stays unclosed");
        assert_eq!(s.take_failure().as_deref(), Some(format!("A plugin stopped. {reason}").as_str()));
        let stopped = card(&plugins);
        assert!(!stopped.on);
        assert_eq!(stopped.problem.as_deref(), Some(reason));
        s.undo();
        assert!(!red_at(&s, 30, 30), "and it is one step");
        // the next fill takes no time over it
        pour_at(&s, 30.0, 30.0);
        assert!(!s.pour_finishing());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fill_plugin_that_never_ends_leaves_the_fill_unclosed() {
        let module = fill_module(FILL_AT, "(loop (br 0)) (i32.const 1)", "");
        fill_goes_wrong("fill-loop", &module, "Switched off: it took too long.");
    }

    #[test]
    fn a_fill_plugin_that_crashes_leaves_the_fill_unclosed() {
        let module = fill_module(FILL_AT, "(unreachable)", "");
        fill_goes_wrong("fill-trap", &module, "Switched off: it crashed.");
    }

    #[test]
    fn a_fill_plugin_that_takes_the_seed_leaves_the_fill_unclosed() {
        let module = fill_module(FILL_AT, "(i32.const 1)", "(i64.store (i32.const 65536) (i64.const -1))");
        fill_goes_wrong("fill-seed", &module, "Switched off: it gave an answer that makes no sense.");
    }

    #[test]
    fn a_fill_plugin_that_asks_too_much_leaves_the_fill_unclosed() {
        let module = fill_module(FILL_AT, "(drop (memory.grow (i32.const 2000))) (i32.const 1)", "");
        fill_goes_wrong("fill-memory", &module, "Switched off: it asked for too much memory.");
    }

    #[test]
    fn a_fill_too_big_for_a_plugin_stays_unclosed_and_says_so() {
        let hooks = Hooks::default();
        let module = fill_module(FILL_AT, "(i32.const 1)", TAKES_MOST);
        hooks.put_on("fill".to_owned(), Running::start(&module, &[]).unwrap(), &[Hook::FillMask]);
        let mask = crate::pour::tests::mask_of_box(9000, 9000);
        assert!(crate::pour::closed_by(&hooks, mask, (5, 5)).is_none());
        let said = hooks.take_said().unwrap();
        assert!(said.contains("too big"), "{said}");
        assert!(hooks.is_on("fill"), "it isn't the plugin's doing");
    }

    #[test]
    fn a_long_call_holds_up_no_other_hook() {
        // a plugin on the fill hook at work, and another on the position hook goes on answering
        let hooks = Arc::new(Hooks::default());
        let body = "(local $i i32) (local.set $i (i32.const 30000000))
            (loop (local.set $i (i32.sub (local.get $i) (i32.const 1))) (br_if 0 (local.get $i)))
            (i32.const 0)";
        let slow = fill_module(FILL_AT, body, "");
        hooks.put_on("fill".to_owned(), Running::start(&slow, &[]).unwrap(), &[Hook::FillMask]);
        let mut moves = Running::start(&offset(), &[]).unwrap();
        moves.control(0, 1.0).unwrap();
        hooks.put_on("offset".to_owned(), moves, &[Hook::Position]);
        let working = Arc::clone(&hooks);
        let long = std::thread::spawn(move || {
            let mut rows = vec![!0_u64; 2048 * 32];
            working.fill_rows(&mut rows, 2048, 2048, (0, 0))
        });
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(hooks.position(1.0, 1.0), (2.0, 1.0));
        assert!(!long.is_finished(), "answered while the other was at work");
        assert!(!long.join().unwrap());
    }

    /// What whole-canvas fills cost going through a real plugin file, up to the most that can: with
    /// `PROCRAFT_PLUGIN_FILE` naming the ColorDrop tools plugin, `--ignored --nocapture`, on a device.
    #[test]
    #[ignore]
    fn a_built_gap_plugin_costs() {
        let bytes = std::fs::read(std::env::var("PROCRAFT_PLUGIN_FILE").unwrap()).unwrap();
        let dir = store("built-gap-cost");
        let plugins = Plugins::with_store(dir.clone());
        assert_eq!(plugins.install(bytes), None);
        let found = card(&plugins);
        assert!(plugins.set_on(found.id.clone(), true));
        // (the switch it comes under, if it has one)
        plugins.set_switch(format!("{}.gap_closing", found.id), true);
        plugins.set_slider("fill_gap".to_owned(), 8.0);
        let hooks = plugins.hooks();
        assert!(hooks.fill_mask());
        for side in [512_usize, 2048, 4096, 8192] {
            // two rooms, a wall down the middle with an opening 4 tall
            let mut data = vec![255_u8; side * side];
            for y in (0..side).filter(|y| !(side / 2..side / 2 + 4).contains(y)) {
                data[y * side + side / 2..y * side + side / 2 + 6].fill(0);
            }
            let filled = data.iter().filter(|&&v| v > 0).count();
            let mask = procraft_engine::selection::Mask { width: side as u32, height: side as u32, data };
            let start = Instant::now();
            let closed = crate::pour::closed_by(&hooks, mask, (side as u32 / 4, side as u32 / 4));
            let took = start.elapsed().as_secs_f64() * 1e3;
            let kept = closed.map(|m| m.data.iter().filter(|&&v| v > 0).count());
            let memory = hooks.long_calls(Hook::FillMask).first().map_or(0, |(_, r)| at_work(r).memory_used());
            println!(
                "fill {side}: {took:.0} ms, {:.1} MiB so far, kept {kept:?} of {filled}",
                memory as f64 / (1 << 20) as f64
            );
        }
        println!("{:?}", hooks.take_said());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A gap closed by a real plugin file: with `PROCRAFT_PLUGIN_FILE` naming the ColorDrop tools
    /// plugin and `--ignored`. Two rooms, the same fill through a gap, with the plugin on and off.
    #[test]
    #[ignore]
    fn a_built_gap_plugin_closes_a_real_fill() {
        let bytes = std::fs::read(std::env::var("PROCRAFT_PLUGIN_FILE").unwrap()).unwrap();
        let Some(s) = session() else { return };
        s.set_color(0.0, 0.0, 0.0);
        s.set_brush_size(0.1);
        for (y0, y1) in [(-10.0, 90.0), (110.0, 210.0)] {
            s.stroke_begin(PointerKind::Finger, vec![sample(100.0, y0, 0.0)]);
            s.stroke_add(
                (1..20).map(|i| sample(100.0, y0 + (y1 - y0) * i as f32 / 20.0, f64::from(i) * 8.0)).collect(),
            );
            s.stroke_end(vec![]);
        }
        let dir = store("built-gap");
        let plugins = Plugins::with_store(dir.clone());
        assert_eq!(plugins.install(bytes), None);
        let found = card(&plugins);
        assert!(plugins.set_on(found.id.clone(), true));
        plugins.set_switch(format!("{}.gap_closing", found.id), true);
        assert_eq!(plugins.set_slider("fill_gap".to_owned(), 32.0), Some(32.0));
        s.use_plugins(Arc::clone(&plugins));
        assert!(s.pour_begin(40.0, 100.0, 1.0, 0.0, 0.0, 0.3));
        s.pour_end(true, None);
        assert!(red_at(&s, 40, 100) && red_at(&s, 40, 30));
        assert!(!red_at(&s, 160, 100) && !red_at(&s, 160, 30), "kept to its side");
        s.undo();
        s.await_pour();
        assert!(plugins.set_on(found.id.clone(), false));
        assert!(s.pour_begin(40.0, 100.0, 1.0, 0.0, 0.0, 0.3));
        s.pour_end(true, None);
        assert!(red_at(&s, 40, 100) && red_at(&s, 160, 100), "off, it runs through the gap");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_goes_on_as_applied_and_off_as_unapplied_with_no_restart() {
        let dir = store("apply");
        let plugins = with_offset(&dir);
        let hooks = plugins.hooks();
        assert_eq!(hooks.position(10.0, 10.0), (10.0, 10.0), "imported, it does nothing");
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(plugins.set_slider(format!("{ID}.x"), 7.0), Some(7.0));
        assert_eq!(plugins.set_slider(format!("{ID}.y"), -3.0), Some(-3.0));
        assert_eq!(hooks.position(10.0, 10.0), (17.0, 7.0));
        assert!(plugins.set_on(ID.to_owned(), false));
        assert_eq!(hooks.position(10.0, 10.0), (10.0, 10.0));
        // and back as it was left
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(hooks.position(10.0, 10.0), (17.0, 7.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn applied_it_comes_back_through_a_restart_whatever_order_values_are_read_in() {
        let dir = store("restart");
        let plugins = with_offset(&dir);
        plugins.set_on(ID.to_owned(), true);
        plugins.set_slider(format!("{ID}.x"), 7.0);
        let mut lines = plugins.kept_lines();
        for _ in 0..2 {
            let next = Plugins::with_store(dir.to_owned());
            for line in &lines {
                let (key, value) = line.split_once(' ').unwrap();
                next.read_kept(key.to_owned(), value.to_owned());
            }
            assert_eq!(next.hooks().position(1.0, 1.0), (8.0, 1.0));
            lines.reverse();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removed_it_is_off_the_hook_and_gone() {
        let dir = store("remove");
        let plugins = with_offset(&dir);
        plugins.set_on(ID.to_owned(), true);
        plugins.set_slider(format!("{ID}.x"), 7.0);
        let hooks = plugins.hooks();
        assert_eq!(hooks.position(1.0, 1.0), (8.0, 1.0));
        let keys = plugins.remove(ID.to_owned()).unwrap();
        assert!(keys.contains(&format!("{ID}.x")) && keys.contains(&format!("{ID}.on")), "{keys:?}");
        assert_eq!(hooks.position(1.0, 1.0), (1.0, 1.0));
        assert!(plugins.plugins().is_empty());
        assert!(Plugins::with_store(dir.clone()).plugins().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_version_takes_over_on_the_hook() {
        let dir = store("update");
        let plugins = with_offset(&dir);
        plugins.set_on(ID.to_owned(), true);
        plugins.set_slider(format!("{ID}.x"), 7.0);
        let hooks = plugins.hooks();
        let mut newer = offset_manifest();
        newer["version"] = json!("1.1.0");
        assert_eq!(plugins.install(file(&newer, &offset())), None);
        assert_eq!(card(&plugins).version, "1.1.0");
        assert!(card(&plugins).on);
        assert_eq!(hooks.position(1.0, 1.0), (8.0, 1.0), "on still, with what was set");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a plugin that goes wrong on a sample must come to, whatever went wrong.
    fn stops(name: &str, before: &str, body: &str, reason: &str) {
        let dir = store(name);
        let plugins = turned_on(&dir, before, body);
        let hooks = plugins.hooks();
        assert_eq!(hooks.position(3.0, 4.0), (3.0, 4.0), "the sample goes through unchanged");
        let stopped = card(&plugins);
        assert!(!stopped.on);
        assert_eq!(stopped.problem.as_deref(), Some(reason));
        assert_eq!(hooks.position(5.0, 6.0), (5.0, 6.0), "and it is off the hook");
        assert_eq!(hooks.hooked.load(Ordering::Relaxed), 0);
        // it stays off through a restart, even with its flag kept as on, and says why
        let next = Plugins::with_store(dir.clone());
        next.read_kept(format!("{ID}.on"), "true".to_owned());
        assert_eq!(next.hooks().position(3.0, 4.0), (3.0, 4.0));
        let kept = card(&next);
        assert!(!kept.on);
        assert_eq!(kept.problem.as_deref(), Some(reason));
        // off, it says nothing more; asked again, it gets its chance
        assert!(next.set_on(ID.to_owned(), false));
        assert_eq!(card(&next).problem, None);
        assert_eq!(card(&Plugins::with_store(dir.clone())).problem, None);
        assert!(next.set_on(ID.to_owned(), true));
        assert!(card(&next).on);
        // and removed, nothing of it is left
        assert!(next.remove(ID.to_owned()).is_some());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_that_loops_for_ever_is_stopped_and_switched_off() {
        stops("loop", "", "(loop (br 0)) (i64.const 0)", "Switched off: it took too long.");
    }

    #[test]
    fn a_plugin_that_traps_is_switched_off() {
        stops("trap", "", "(unreachable)", "Switched off: it crashed.");
    }

    #[test]
    fn a_plugin_that_returns_garbage_is_switched_off() {
        // two NaNs
        let nan = "(i64.const 0x7fc000007fc00000)";
        stops("garbage", "", nan, "Switched off: it gave an answer that makes no sense.");
    }

    #[test]
    fn a_plugin_over_the_memory_cap_is_switched_off() {
        let grows = "(drop (memory.grow (i32.const 1000))) (i64.const 0)";
        stops("memory", "(memory 1)", grows, "Switched off: it asked for too much memory.");
    }

    #[test]
    fn a_plugin_stopped_on_a_stroke_is_told_once_by_the_session() {
        let dir = store("told");
        let plugins = turned_on(&dir, "", "(unreachable)");
        let s = CanvasSession::new(200, 200);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(s.take_failure(), None);
        {
            let mut st = s.shared.lock();
            st.view = Some(View::fit((200, 200), (200, 200)));
            let at = st.to_canvas(&[sample(10.0, 20.0, 0.0)]);
            assert_eq!(at.len(), 1);
            let plain = st
                .view
                .and_then(|v| v.to_screen().inverse())
                .unwrap()
                .apply(procraft_engine::geom::Vec2::new(10.0, 20.0));
            assert_eq!(at[0].pos, plain, "the sample goes through unchanged");
        }
        assert_eq!(s.take_failure().as_deref(), Some("A plugin stopped. Switched off: it crashed."));
        assert_eq!(s.take_failure(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_that_can_t_start_stays_off_and_says_why() {
        let dir = store("start");
        let module = position_module("(memory 1000)", "(i64.const 0)");
        let plugins = Plugins::with_store(dir.clone());
        assert_eq!(plugins.install(file(&coded(ID, &module), &module)), None);
        assert!(plugins.set_on(ID.to_owned(), true));
        let stopped = card(&plugins);
        assert!(!stopped.on);
        assert_eq!(stopped.problem.as_deref(), Some("Switched off: it asked for too much memory."));
        assert_eq!(plugins.hooks().hooked.load(Ordering::Relaxed), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stopped_plugin_leaves_the_others_on() {
        let bad = position_module("", "(unreachable)");
        let hooks = Hooks::default();
        let mut good = Running::start(&offset(), &[]).unwrap();
        good.control(0, 2.0).unwrap();
        hooks.put_on("b".to_owned(), good, &[Hook::Position]);
        hooks.put_on("a".to_owned(), Running::start(&bad, &[]).unwrap(), &[Hook::Position]);
        assert_eq!(hooks.position(1.0, 1.0), (3.0, 1.0));
        assert!(!hooks.is_on("a") && hooks.is_on("b"));
        assert_eq!(hooks.take_stopped(), [("a".to_owned(), Stop::Crashed)]);
        assert_eq!(hooks.take_stopped(), []);
    }

    #[test]
    fn plugins_go_in_id_order_however_they_were_applied() {
        // the second moves what the first gave it, so the order shows
        let doubles = position_module(
            "",
            "(i64.or
                (i64.extend_i32_u (i32.reinterpret_f32 (f32.mul (local.get 0) (f32.const 2))))
                (i64.shl (i64.extend_i32_u (i32.reinterpret_f32 (local.get 1))) (i64.const 32)))",
        );
        let offset_by = |x: f32| {
            let mut running = Running::start(&offset(), &[]).unwrap();
            running.control(0, x).unwrap();
            running
        };
        let hooks = Hooks::default();
        hooks.put_on("b".to_owned(), Running::start(&doubles, &[]).unwrap(), &[Hook::Position]);
        hooks.put_on("a".to_owned(), offset_by(1.0), &[Hook::Position]);
        assert_eq!(hooks.position(1.0, 0.0), (4.0, 0.0));
    }

    /// A session on a headless GPU drawing just where the input goes, or none without a GPU.
    fn session() -> Option<Arc<CanvasSession>> {
        let gpu = match Gpu::headless() {
            Ok(gpu) => gpu,
            Err(e) => {
                eprintln!("skipped: {e}");
                return None;
            }
        };
        let s = CanvasSession::new(200, 200);
        {
            let mut st = s.shared.lock();
            st.window_size = (200, 200);
            st.view = Some(View::fit((200, 200), (200, 200)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        s.studio_begin();
        s.set_studio_setting("streamline".to_owned(), 0.0);
        s.studio_end(true);
        Some(s)
    }

    fn sample(x: f32, y: f32, ms: f64) -> PenSample {
        PenSample { x, y, pressure: 1.0, tilt: 0.0, orientation: 0.0, time_ms: ms, roll: 0.0 }
    }

    fn draw(s: &CanvasSession) {
        s.stroke_begin(PointerKind::Stylus, vec![sample(30.0, 50.0, 0.0)]);
        s.stroke_add((1..20).map(|i| sample(30.0 + i as f32 * 6.0, 50.0, f64::from(i) * 8.0)).collect());
        s.stroke_end(vec![sample(150.0, 50.0, 170.0)]);
    }

    /// What the canvas looks like, after a render.
    fn picture(s: &CanvasSession) -> Vec<u8> {
        let mut st = s.shared.lock();
        let layer = st.doc.current();
        st.layer_pixels(layer).unwrap()
    }

    /// The rows of `rgba`, 200 wide, that anything was drawn on.
    fn rows_drawn(rgba: &[u8]) -> Vec<usize> {
        let rows = rgba.chunks(200 * 4).enumerate();
        rows.filter(|(_, row)| row.chunks(4).any(|p| p[3] > 0)).map(|(y, _)| y).collect()
    }

    /// Every hook, by its name in a manifest (the two a manifest never names, `Layers` and
    /// `Brushes`, go by their kinds of layer and brush engines).
    pub(crate) const EVERY_HOOK: &[&str] = &[
        "position",
        "pick_layer",
        "interface_scale",
        "paste_mask",
        "fill_mask",
        "fill_all_layers",
        "radial_segments",
        "hide_undo_notice",
        "interface_colour",
        "pin_panels",
        "opacity_together",
        "randomise_brush",
        "colour_triangle",
        "quickshape_curve",
        "stroke_sound",
        "inbetween",
        "picker",
        "stroke_kept",
        "layer_added",
        "key_shortcut",
        "keep_clear",
        "interface_part_scale",
        "floating_picker_only",
        "hide_notices",
        "floating_panels",
        "gesture_lock",
        "view_look",
        "paper",
        "reference_windows",
        "pressure",
        "brush_preview_colour",
        "layer_label",
    ];

    /// Every hook, `Layers` and `Brushes` too.
    pub(crate) fn every_hook() -> Vec<Hook> {
        let named = EVERY_HOOK.iter().map(|name| Hook::named(name).unwrap_or_else(|| panic!("{name}")));
        named.chain([Hook::Layers, Hook::Brushes]).collect()
    }

    #[test]
    fn a_set_of_hooks_has_room_for_64_each_its_own_bit() {
        let every = every_hook();
        // each hook its own bit, every one of them in the set
        let mut all = 0u64;
        for hook in &every {
            assert_eq!(all & hook.bit(), 0, "{hook:?} shares a bit");
            all |= hook.bit();
        }
        assert_eq!(all.count_ones() as usize, every.len());
        // the set as the app holds it: each hook found by its bit, past the 32nd as before it
        assert_eq!(every.len(), 34, "a new hook goes in the list");
        assert_eq!(all, (1 << 34) - 1, "the first 34 bits: the 32 the set held before it grew, and two past");
        let hooks = Hooks::default();
        let module = wat::parse_str(PRESSES).unwrap();
        hooks.put_on("a".to_owned(), Running::start(&module, &[]).unwrap(), &every);
        assert_eq!(hooks.hooked.load(Ordering::Relaxed), all);
        assert!(every.iter().all(|&hook| hooks.has(hook)));
        // the bit a 35th hook to 64th would take sits in the set the same
        hooks.hooked.store(all | 1 << 34 | 1 << 63, Ordering::Relaxed);
        assert!(every.iter().all(|&hook| hooks.has(hook)));
        assert_eq!(hooks.hooked.load(Ordering::Relaxed).count_ones(), 36);
        hooks.take_off("a");
        assert_eq!(hooks.hooked.load(Ordering::Relaxed), 0);
        assert!(!every.iter().any(|&hook| hooks.has(hook)));
    }

    #[test]
    fn plugins_written_to_interface_8_and_9_still_load_and_answer_on_the_wider_set() {
        const LOCKS: &str = r#"(module
            (func (export "procraft_gesture_lock") (param i32) (result i32) (i32.eqz (local.get 0))))"#;
        for (interface, hook, code) in [(8, "gesture_lock", LOCKS), (9, "pressure", PRESSES)] {
            let dir = store(&format!("interface-{interface}"));
            let module = wat::parse_str(code).unwrap();
            let mut manifest = coded(ID, &module);
            manifest["compat"]["procraft"]["interface"] = json!(interface);
            manifest["hooks"] = json!([hook]);
            let plugins = Plugins::with_store(dir.clone());
            assert_eq!(plugins.install(file(&manifest, &module)), None, "interface {interface}");
            assert!(plugins.set_on(ID.to_owned(), true));
            // on its hook, answering, then read back as the next start reads it, on it again
            for plugins in [Arc::clone(&plugins), restarted(&dir, &plugins)] {
                let hooks = plugins.hooks();
                assert!(card(&plugins).on && card(&plugins).problem.is_none(), "interface {interface}");
                match interface {
                    8 => assert!(hooks.gesture_lock(GESTURE_TURN) && !hooks.gesture_lock(1)),
                    _ => assert_eq!(hooks.pressure(finger_at(50.0)), Some(0.5)),
                }
                assert_eq!(hooks.hooked.load(Ordering::Relaxed), Hook::named(hook).unwrap().bit());
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// What a stroke with no plugin on pays for each hook it passes: reading the set. Printed, and
    /// failing past 50 ns a look (about 5 among a whole test run; a 240 Hz pen sends a sample every
    /// 4,000,000 ns, and passes a few hooks).
    #[test]
    fn a_hook_with_none_on_costs_a_read_of_the_set() {
        let hooks = Hooks::default();
        let every = every_hook();
        const ROUNDS: u32 = 200_000;
        let looked = (0..5)
            .map(|_| {
                let t = Instant::now();
                let mut on = 0u32;
                for i in 0..ROUNDS {
                    let hook = every.get(i as usize % every.len()).copied().unwrap_or(Hook::Position);
                    on += u32::from(std::hint::black_box(&hooks).has(std::hint::black_box(hook)));
                }
                assert_eq!(on, 0);
                t.elapsed().as_secs_f64() * 1e9 / f64::from(ROUNDS)
            })
            .fold(f64::MAX, f64::min);
        println!("a hook with none on looked at in {looked:.2} ns");
        assert!(looked < 50.0, "{looked} ns");
    }

    /// A plugin on the brush preview's hook answering `body` (WAT, given the current colour and
    /// whether the brush is the one in use).
    fn previewing(body: &str) -> Running {
        let text =
            format!(r#"(module (func (export "procraft_brush_preview_colour") (param i32 i32) (result i32) {body}))"#);
        Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap()
    }

    #[test]
    fn the_brush_previews_go_in_the_colour_the_first_plugin_says_and_with_none_on_the_app_s() {
        let hooks = Hooks::default();
        assert_eq!(hooks.brush_preview_colour(0x123456, false), None);
        // the first hook past bit 31: on it, it is found as any other
        assert_eq!(Hook::BrushPreviewColour.bit(), 1 << 32);
        // the current colour, but the brush in use left as the app draws it
        let current = "(select (i32.const -1) (local.get 0) (local.get 1))";
        hooks.put_on("b".to_owned(), previewing(current), &[Hook::BrushPreviewColour]);
        assert!(hooks.listens(Hook::BrushPreviewColour) && !hooks.listens(Hook::Pressure));
        assert_eq!(hooks.brush_preview_colour(0x123456, false), Some(0x123456));
        assert_eq!(hooks.brush_preview_colour(0x123456, true), None);
        // one before it by id that answers comes first; one that leaves it doesn't
        hooks.put_on("a".to_owned(), previewing("(i32.const 0xFF0000)"), &[Hook::BrushPreviewColour]);
        assert_eq!(hooks.brush_preview_colour(0x123456, false), Some(0xFF0000));
        hooks.put_on("a".to_owned(), previewing("(i32.const -1)"), &[Hook::BrushPreviewColour]);
        assert_eq!(hooks.brush_preview_colour(0x123456, false), Some(0x123456));
        assert_eq!(hooks.take_stopped(), Vec::new());
        // asked as the library draws: quick
        let t = Instant::now();
        for i in 0..10_000u32 {
            assert_eq!(hooks.brush_preview_colour(i, false), Some(i));
        }
        let each = t.elapsed() / 10_000;
        println!("the brush previews' colour asked in {each:?}");
        assert!(each < std::time::Duration::from_micros(100), "{each:?}");
        // an answer that is no colour stops its plugin, and the previews go as the app draws them
        for odd in ["(i32.const 0x1000000)", "(i32.const -2)", "(unreachable)"] {
            let hooks = Hooks::default();
            hooks.put_on("odd".to_owned(), previewing(odd), &[Hook::BrushPreviewColour]);
            assert_eq!(hooks.brush_preview_colour(0x123456, false), None, "{odd}");
            assert_eq!(hooks.take_stopped().len(), 1, "{odd}");
            assert!(!hooks.listens(Hook::BrushPreviewColour), "{odd}");
        }
    }

    fn finger_at(x: f32) -> crate::plugin_run::PressureAsked {
        crate::plugin_run::PressureAsked { pointer: 0, first: true, x, y: 0.0, ms: 0.0, pressure: 0.9 }
    }

    #[test]
    fn the_pressure_hook_presses_as_its_plugins_say_in_turn_and_with_none_on_nothing() {
        use crate::plugin_run::PressureAsked;
        // (the last a set of 32 had room for: the set holds 64 now)
        assert_eq!(Hook::Pressure.bit(), 1 << 31);
        let hooks = Hooks::default();
        assert_eq!(hooks.pressure(finger_at(50.0)), None);
        assert_eq!(hooks.pressure_tapers(0), [0.0; 2]);
        hooks.put_on(
            "a".to_owned(),
            Running::start(&wat::parse_str(PRESSES).unwrap(), &[]).unwrap(),
            &[Hook::Pressure],
        );
        assert_eq!(hooks.pressure(finger_at(50.0)), Some(0.5));
        assert_eq!(hooks.pressure(PressureAsked { pointer: 1, ..finger_at(50.0) }), None, "a pen's left");
        assert_eq!((hooks.pressure_tapers(0), hooks.pressure_tapers(1)), ([10.0, 20.0], [0.0; 2]));
        // a sample that isn't a number goes on as it came, and no plugin answers for it
        assert_eq!(hooks.pressure(PressureAsked { x: f32::NAN, ..finger_at(1.0) }), None);
        assert_eq!(hooks.pressure(PressureAsked { pressure: f32::INFINITY, ..finger_at(1.0) }), None);
        assert!(hooks.listens(Hook::Pressure));
        // a second, after it by id: told what the first said (or the sample's own, where it left
        // it); each taper the longest either says
        hooks.put_on(
            "b".to_owned(),
            Running::start(&wat::parse_str(PRESSES_HALF).unwrap(), &[]).unwrap(),
            &[Hook::Pressure],
        );
        assert_eq!(hooks.pressure(finger_at(50.0)), Some(0.25));
        assert_eq!(hooks.pressure(PressureAsked { pointer: 1, ..finger_at(50.0) }), Some(0.45));
        assert_eq!(hooks.pressure_tapers(0), [10.0, 30.0]);
        assert_eq!(hooks.take_stopped(), Vec::new());
    }

    /// Answers the stroke-fill hook: a paint stroke fills (as it ends) while its brush's row is 1.
    const FILLS_BY_ROW: &str = r#"(module
        (global $row (mut f32) (f32.const 0))
        (func (export "procraft_brush_control") (param i32 f32) (global.set $row (local.get 1)))
        (func (export "procraft_stroke_fill") (param i32 i32) (result i32)
            (if (result i32) (i32.and (i32.eqz (local.get 0)) (f32.eq (global.get $row) (f32.const 1)))
                (then (i32.const 2))
                (else (i32.const 0)))))"#;

    /// A plugin on the stroke-fill hook answering `answer` (a WAT body).
    fn stroke_fill_module(answer: &str) -> Vec<u8> {
        let text = format!(r#"(module (func (export "procraft_stroke_fill") (param i32 i32) (result i32) {answer}))"#);
        wat::parse_str(text).unwrap()
    }

    /// A brush whose every row is `row`, as the stroke-fill hook is told it.
    fn brush_rows(row: f32) -> impl Fn(&[String]) -> crate::stroke_sound::SoundBrush {
        move |keys: &[String]| crate::stroke_sound::SoundBrush {
            name: "Pen".to_owned(),
            set: "Inking".to_owned(),
            controls: keys.iter().map(|_| row).collect(),
            key: u64::from(row.to_bits()),
        }
    }

    #[test]
    fn the_stroke_fill_hook_asks_each_plugin_told_its_rows_and_the_first_that_fills_says_how() {
        let hooks = Hooks::default();
        assert_eq!(hooks.stroke_fill(0, 1, brush_rows(1.0)), 0, "none on");
        let by_row = Running::start(&wat::parse_str(FILLS_BY_ROW).unwrap(), &[]).unwrap();
        hooks.put_on("b".to_owned(), by_row, &[Hook::StrokeFill]);
        hooks.brush_keys("b", vec!["plugin:b:fill".to_owned()]);
        assert_eq!(hooks.stroke_fill(0, 1, brush_rows(1.0)), 2);
        assert_eq!(hooks.stroke_fill(0, 1, brush_rows(0.0)), 0, "told the row as it changed");
        assert_eq!(hooks.stroke_fill(1, 1, brush_rows(1.0)), 0, "the eraser");
        // one before it by id that always fills, shown as it grows, says first
        let always = Running::start(&stroke_fill_module("(i32.const 1)"), &[]).unwrap();
        hooks.put_on("a".to_owned(), always, &[Hook::StrokeFill]);
        assert_eq!(hooks.stroke_fill(1, 0, brush_rows(0.0)), 1);
        assert!(hooks.listens(Hook::StrokeFill));
        assert_eq!(hooks.take_stopped(), Vec::new());
    }

    #[test]
    fn a_stroke_fill_answer_that_makes_no_sense_stops_its_plugin() {
        for answer in ["(i32.const 3)", "(i32.const -1)", "(loop (br 0)) (i32.const 0)", "unreachable"] {
            let hooks = Hooks::default();
            let running = Running::start(&stroke_fill_module(answer), &[]).unwrap();
            hooks.put_on("odd".to_owned(), running, &[Hook::StrokeFill]);
            assert_eq!(hooks.stroke_fill(0, 1, brush_rows(0.0)), 0, "{answer}");
            assert!(!hooks.listens(Hook::StrokeFill), "{answer}: still on");
            assert_eq!(hooks.take_stopped().len(), 1, "{answer}");
        }
    }

    #[test]
    fn the_stroke_fill_hook_costs_a_stroke_little_and_nothing_with_none_on() {
        let time = |hooks: &Hooks, runs: u32| {
            let start = Instant::now();
            for i in 0..runs {
                std::hint::black_box(hooks.stroke_fill(std::hint::black_box((i % 2) as i32), 1, brush_rows(1.0)));
            }
            start.elapsed().as_secs_f64() * 1e6 / f64::from(runs)
        };
        let hooks = Hooks::default();
        let none = time(&hooks, 100_000);
        let by_row = Running::start(&wat::parse_str(FILLS_BY_ROW).unwrap(), &[]).unwrap();
        hooks.put_on("b".to_owned(), by_row, &[Hook::StrokeFill]);
        hooks.brush_keys("b", vec!["plugin:b:fill".to_owned()]);
        let on = time(&hooks, 2_000);
        println!("stroke_fill: none on {:.3} µs a stroke, one on {on:.1} µs", none);
        // asked once a stroke: far under a frame even in a debug build
        assert!(none < 1.0, "none on: {none} µs");
        assert!(on < 1000.0, "one on: {on} µs");
    }

    #[test]
    fn a_pressure_or_a_taper_that_makes_no_sense_stops_its_plugin() {
        for (pressure, taper) in [
            ("(f32.const 1.5)", "(f32.const 0)"),
            ("(f32.const nan)", "(f32.const 0)"),
            ("(f32.const 0.5)", "(f32.const -1)"),
            ("(f32.const 0.5)", "(f32.const 1001)"),
            ("(f32.const 0.5)", "(f32.const inf)"),
            ("(loop (br 0)) (f32.const 0.5)", "(f32.const 0)"),
        ] {
            let hooks = Hooks::default();
            let running = Running::start(&pressure_module(pressure, taper), &[]).unwrap();
            hooks.put_on("odd".to_owned(), running, &[Hook::Pressure]);
            let pressed = hooks.pressure(finger_at(10.0));
            let tapers = hooks.pressure_tapers(0);
            assert!(!hooks.listens(Hook::Pressure), "{pressure} {taper}: still on");
            assert_eq!(hooks.take_stopped().len(), 1, "{pressure} {taper}");
            assert_eq!(hooks.pressure(finger_at(10.0)), None);
            assert!(pressed.is_none() || tapers == [0.0; 2], "{pressure} {taper}");
        }
        // below 0 leaves it, 0 and 1 are pressures, and tapers of 0 to 1000 points are taken
        for (said, want) in [(-0.5, None), (-1e30, None), (0.0, Some(0.0)), (1.0, Some(1.0))] {
            let hooks = Hooks::default();
            let running = Running::start(&pressure_module(&format!("(f32.const {said})"), "(f32.const 1000)"), &[]);
            hooks.put_on("fine".to_owned(), running.unwrap(), &[Hook::Pressure]);
            assert_eq!((hooks.pressure(finger_at(10.0)), hooks.pressure_tapers(0)), (want, [1000.0; 2]));
            assert!(hooks.take_stopped().is_empty());
        }
    }

    /// A session as [`session`] gives, a window pixel a point, its brush's size by its pressure and
    /// no taper of its own, sized `size`.
    fn pressing_session(size: f32) -> Option<Arc<CanvasSession>> {
        let s = session()?;
        s.set_display_scale(1.0);
        s.studio_begin();
        s.set_studio_setting("pressure_size".to_owned(), 1.0);
        for taper in ["pen_taper_start", "pen_taper_end", "touch_taper_start", "touch_taper_end"] {
            s.set_studio_setting(taper.to_owned(), 0.0);
        }
        s.studio_end(true);
        s.set_brush_size(size);
        Some(s)
    }

    /// A stroke by `pointer` across the canvas at row 100, from x 20 to 180, `step` window pixels
    /// every 4 ms (pressed 1, as a finger says nothing).
    fn swipe(s: &CanvasSession, pointer: PointerKind, step: f32) {
        s.stroke_begin(pointer, vec![sample(20.0, 100.0, 0.0)]);
        let (mut x, mut ms, mut more) = (20.0, 0.0, Vec::new());
        while x + step < 180.0 {
            x += step;
            ms += 4.0;
            more.push(sample(x, 100.0, ms));
        }
        s.stroke_add(more);
        s.stroke_end(vec![sample(180.0, 100.0, ms + 4.0)]);
    }

    /// How many rows of each column of `rgba` (200 wide) have anything drawn.
    fn column_heights(rgba: &[u8]) -> Vec<usize> {
        (0..200).map(|x| (0..200).filter(|y| rgba.get((y * 200 + x) * 4 + 3).is_some_and(|&a| a > 0)).count()).collect()
    }

    #[test]
    fn finger_pressure_thins_a_fast_finger_and_tapers_it_and_leaves_pens_and_its_absence_as_they_were() {
        const FINGER: &str = "io.github.artcraftandroid.brush-extras";
        let drawn = |plugins: Option<&Arc<Plugins>>, pointer: PointerKind, step: f32| {
            let s = pressing_session(0.35)?;
            if let Some(plugins) = plugins {
                s.use_plugins(Arc::clone(plugins));
            }
            swipe(&s, pointer, step);
            Some(picture(&s))
        };
        let Some(finger) = drawn(None, PointerKind::Finger, 12.0) else { return };
        let pen = drawn(None, PointerKind::Stylus, 12.0).unwrap();
        let dir = store("finger-pressure-strokes");
        let plugins = Plugins::with_bundled(dir.clone());
        // as it comes (off), nothing differs at all, nor on with Finger pressure off
        assert!(drawn(Some(&plugins), PointerKind::Finger, 12.0).unwrap() == finger);
        assert!(plugins.set_on(FINGER.to_owned(), true));
        assert!(drawn(Some(&plugins), PointerKind::Finger, 12.0).unwrap() == finger);
        assert!(plugins.set_switch(format!("{FINGER}.finger_pressure"), true));
        // on: a pen's stroke is as it was
        assert!(drawn(Some(&plugins), PointerKind::Stylus, 12.0).unwrap() == pen);
        // a finger going 3,000 points a second presses lighter, so it is thinner
        let fast = drawn(Some(&plugins), PointerKind::Finger, 12.0).unwrap();
        let (plain_heights, fast_heights) = (column_heights(&finger), column_heights(&fast));
        println!(
            "a finger's stroke 3,000 points a second, rows high at x 40, 100 and 160: {:?} without, {:?} with",
            [plain_heights[40], plain_heights[100], plain_heights[160]],
            [fast_heights[40], fast_heights[100], fast_heights[160]]
        );
        assert!(fast_heights[100] * 10 < plain_heights[100] * 9, "{} {}", fast_heights[100], plain_heights[100]);
        // slowly, with speed counting for nothing: as thick as without in the middle, tapered to
        // points at the ends (in over 20 points, out over 40)
        assert!(plugins.set_slider(format!("{FINGER}.speed"), 0.0).is_some());
        let slow_plain = column_heights(&drawn(None, PointerKind::Finger, 2.0).unwrap());
        let slow = column_heights(&drawn(Some(&plugins), PointerKind::Finger, 2.0).unwrap());
        println!(
            "slowly, rows high at x 22, 30, 100, 150 and 175: {:?} without, {:?} with",
            [22, 30, 100, 150, 175].map(|x| slow_plain[x]),
            [22, 30, 100, 150, 175].map(|x| slow[x])
        );
        assert!(slow[100].abs_diff(slow_plain[100]) <= 1, "{} {}", slow[100], slow_plain[100]);
        assert!(slow[22] * 2 < slow[100] && slow[175] * 2 < slow[100], "{slow:?}");
        assert!(slow[22] < slow[30] && slow[150] > slow[175], "{slow:?}");
        // off again: as if it had never been
        assert!(plugins.set_on(FINGER.to_owned(), false));
        assert!(drawn(Some(&plugins), PointerKind::Finger, 12.0).unwrap() == finger);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stroke_lands_where_the_plugin_says_and_as_it_did_once_it_is_gone() {
        let Some(plain) = session() else { return };
        draw(&plain);
        let drawn = picture(&plain);
        assert!(!rows_drawn(&drawn).is_empty());

        // with the list on the session and nothing on, nothing differs at all
        let dir = store("stroke");
        let plugins = with_offset(&dir);
        let Some(s) = session() else { return };
        s.use_plugins(Arc::clone(&plugins));
        draw(&s);
        assert!(picture(&s) == drawn);

        let Some(s) = session() else { return };
        s.use_plugins(Arc::clone(&plugins));
        plugins.set_on(ID.to_owned(), true);
        plugins.set_slider(format!("{ID}.y"), 40.0);
        draw(&s);
        let moved = rows_drawn(&picture(&s));
        let want: Vec<usize> = rows_drawn(&drawn).iter().map(|y| y + 40).collect();
        assert_eq!(moved, want, "the same stroke, 40 pixels down");

        // off it is as if it had never been
        let Some(s) = session() else { return };
        s.use_plugins(Arc::clone(&plugins));
        plugins.set_on(ID.to_owned(), false);
        draw(&s);
        assert!(picture(&s) == drawn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nanoseconds a sample takes through `hooks`, over a run long enough to read.
    fn cost(hooks: &Hooks) -> f64 {
        const SAMPLES: u32 = 2_000_000;
        let start = Instant::now();
        let mut sum = 0.0_f32;
        for i in 0..SAMPLES {
            sum += hooks.position(std::hint::black_box(i as f32 % 2000.0), 300.0).0;
        }
        std::hint::black_box(sum);
        start.elapsed().as_secs_f64() * 1e9 / f64::from(SAMPLES)
    }

    /// What the hook costs a sample, and what a plugin that spends all its fuel costs: run on a
    /// device with `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn hook_cost() {
        let none = Hooks::default();
        println!("position hook, none on: {:.2} ns a sample", (0..5).map(|_| cost(&none)).fold(f64::MAX, f64::min));

        let with_one = Hooks::default();
        let mut running = Running::start(&offset(), &[]).unwrap();
        running.control(0, 3.0).unwrap();
        with_one.put_on("offset".to_owned(), running, &[Hook::Position]);
        println!("the offset plugin on: {:.1} ns a sample", (0..5).map(|_| cost(&with_one)).fold(f64::MAX, f64::min));

        // the worst a plugin can do to a sample before it is stopped, over and over
        let mut slowest = f64::MAX;
        for _ in 0..50 {
            let mut running = Running::start(&position_module("", "(loop (br 0)) (i64.const 0)"), &[]).unwrap();
            let start = Instant::now();
            assert_eq!(running.position(1.0, 2.0), Err(Stop::TooSlow));
            slowest = slowest.min(start.elapsed().as_secs_f64() * 1000.0);
        }
        println!("a plugin spending all its fuel on a sample: {slowest:.3} ms");

        let picks = Hooks::default();
        picks.put_on(
            "pick".to_owned(),
            Running::start(&wat::parse_str(PICKS).unwrap(), &[1.0]).unwrap(),
            &[Hook::PickLayer],
        );
        let start = Instant::now();
        for _ in 0..1_000_000 {
            std::hint::black_box(picks.pick_layer());
        }
        println!("the pick hook, a plugin on it: {:.1} ns a pick", start.elapsed().as_secs_f64() * 1000.0);

        let scales = Hooks::default();
        scales.put_on(
            "scale".to_owned(),
            Running::start(&wat::parse_str(SCALES).unwrap(), &[1.1]).unwrap(),
            &[Hook::InterfaceScale],
        );
        let start = Instant::now();
        for _ in 0..1_000_000 {
            std::hint::black_box(scales.interface_scale(std::hint::black_box(1600.0), 1000.0));
        }
        println!("the scale hook, a plugin on it: {:.1} ns a frame", start.elapsed().as_secs_f64() * 1000.0);

        let start = Instant::now();
        let mut running = Running::start(&offset(), &[]).unwrap();
        println!("starting a plugin: {:.3} ms", start.elapsed().as_secs_f64() * 1000.0);
        running.control(0, 1.0).unwrap();
    }

    /// What a 4096 square costs going through a paste plugin. With `PROCRAFT_PLUGIN_FILE` naming one
    /// it goes through that, otherwise through [`PASTES`]: run on a device with
    /// `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_paste_of_a_4096_square_costs() {
        let dir = store("paste-cost");
        let plugins = match std::env::var("PROCRAFT_PLUGIN_FILE") {
            Ok(path) => {
                let plugins = Plugins::with_store(dir.clone());
                assert_eq!(plugins.install(std::fs::read(path).unwrap()), None);
                let id = card(&plugins).id;
                assert!(plugins.set_on(id, true));
                plugins
            }
            Err(_) => {
                let plugins = pasting(&dir);
                plugins.set_on(ID.to_owned(), true);
                plugins
            }
        };
        let hooks = plugins.hooks();
        const SIDE: usize = 4096;
        let picture: Vec<u8> = (0..SIDE * SIDE * 4).map(|i| (i % 251) as u8).collect();
        for _ in 0..3 {
            let mut rgba = picture.clone();
            let start = Instant::now();
            assert!(hooks.paste_rows(&mut rgba, SIDE));
            let took = start.elapsed().as_secs_f64();
            println!(
                "4096 square through the plugin: {:.0} ms, {:.0} ns a pixel",
                took * 1e3,
                took * 1e9 / (SIDE * SIDE) as f64
            );
            let mut native = picture.clone();
            let start = Instant::now();
            for px in native.as_chunks_mut::<4>().0 {
                let [r, g, b, _] = px.map(u32::from);
                let grey = ((54 * r + 183 * g + 19 * b + 128) >> 8).min(255) as u8;
                *px = [grey, grey, grey, 255];
            }
            println!("the same, native: {:.0} ms", start.elapsed().as_secs_f64() * 1e3);
            assert!(rgba == native);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same stroke, with the list on the session and without: run on a device with
    /// `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_stroke_costs_the_same_with_nothing_on() {
        let to_canvas_ns = |hooks: Option<Arc<Hooks>>| {
            let s = CanvasSession::new(200, 200);
            let mut st = s.shared.lock();
            st.view = Some(View::fit((200, 200), (200, 200)));
            st.hooks = hooks;
            let samples: Vec<PenSample> = (0..2000).map(|i| sample(i as f32 % 200.0, 50.0, f64::from(i))).collect();
            (0..7)
                .map(|_| {
                    let start = Instant::now();
                    for _ in 0..500 {
                        std::hint::black_box(st.to_canvas(std::hint::black_box(&samples)));
                    }
                    start.elapsed().as_secs_f64() * 1e9 / (500.0 * 2000.0)
                })
                .fold(f64::MAX, f64::min)
        };
        for _ in 0..3 {
            let bare = to_canvas_ns(None);
            let hooked = to_canvas_ns(Some(Arc::new(Hooks::default())));
            println!("to_canvas a sample: {bare:.2} ns without the list, {hooked:.2} ns with it, none on");
        }
    }

    /// A real plugin file, built by `tools/pack_plugin.py`, end to end on the hook. Run with
    /// `--ignored` and `PROCRAFT_PLUGIN_FILE` naming it.
    #[test]
    #[ignore]
    fn a_built_plugin_file_works() {
        let bytes = std::fs::read(std::env::var("PROCRAFT_PLUGIN_FILE").unwrap()).unwrap();
        let dir = store("built");
        let plugins = Plugins::with_store(dir.clone());
        let offer = plugins.inspect(bytes.clone());
        println!("{offer:?}");
        assert_eq!(offer.refused, None);
        assert_eq!(plugins.install(bytes), None);
        let found = card(&plugins);
        assert!(plugins.set_on(found.id.clone(), true));
        let hooks = plugins.hooks();
        assert_eq!(hooks.position(100.0, 100.0), (100.0, 100.0), "at its start it moves nothing");
        for control in &found.controls {
            let crate::plugins::PluginControl::Slider { control } = control else { panic!() };
            plugins.set_slider(control.key.clone(), if control.label == "X" { 12.0 } else { -8.0 });
        }
        assert_eq!(hooks.position(100.0, 100.0), (112.0, 92.0));
        let next = restarted(&dir, &plugins);
        assert_eq!(next.hooks().position(100.0, 100.0), (112.0, 92.0));
        assert!(next.set_on(found.id.clone(), false));
        assert_eq!(next.hooks().position(100.0, 100.0), (100.0, 100.0));
        assert!(next.remove(found.id).is_some());
        assert!(next.plugins().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- plugins made of features (interface 16)

    const FEATURED: &str = "org.example.featured";

    /// A plugin of two features: Offset (the position hook: x moved by its slider, plus 100 for each
    /// of Press's switch it is told, y by the length of the file it was handed) and Press (the
    /// pressure hook: its level, or 0.5 once handed a file, plus a quarter of Offset's switch it is
    /// told; `press` its body's end, to make it go wrong). `grow` pages more memory each sample.
    fn featured_module(press: &str, grow: u32) -> Vec<u8> {
        let text = format!(
            r#"(module
            (memory (export "memory") 1)
            (global $c0 (mut f32) (f32.const 0)) (global $c1 (mut f32) (f32.const 0))
            (global $c2 (mut f32) (f32.const 0)) (global $c3 (mut f32) (f32.const 0))
            (global $file (mut f32) (f32.const 0))
            (func (export "procraft_control") (param $i i32) (param $v f32)
                (if (i32.eq (local.get $i) (i32.const 0)) (then (global.set $c0 (local.get $v))))
                (if (i32.eq (local.get $i) (i32.const 1)) (then (global.set $c1 (local.get $v))))
                (if (i32.eq (local.get $i) (i32.const 2)) (then (global.set $c2 (local.get $v))))
                (if (i32.eq (local.get $i) (i32.const 3)) (then (global.set $c3 (local.get $v)))))
            (func (export "procraft_file_buffer") (param i32 i32) (result i32) (i32.const 0))
            (func (export "procraft_file") (param $i i32) (param $len i32)
                (global.set $file (f32.convert_i32_s (local.get $len))))
            (func (export "procraft_position") (param $x f32) (param $y f32) (result i64)
                (local $nx f32) (local $ny f32)
                (drop (memory.grow (i32.const {grow})))
                (local.set $nx (local.get $x)) (local.set $ny (local.get $y))
                (if (f32.ge (global.get $c0) (f32.const 0.5)) (then
                    (local.set $nx (f32.add (local.get $x)
                        (f32.add (global.get $c1) (f32.mul (global.get $c2) (f32.const 100)))))
                    (local.set $ny (f32.add (local.get $y) (global.get $file)))))
                (i64.or (i64.extend_i32_u (i32.reinterpret_f32 (local.get $nx)))
                    (i64.shl (i64.extend_i32_u (i32.reinterpret_f32 (local.get $ny))) (i64.const 32))))
            (func (export "procraft_stroke_sound")
                (param i32 i32 f32 f32 f32 i32 f32 f32 f32) (result i32) (i32.const 0))
            (func (export "procraft_pressure") (param i32 i32 f32 f32 f32 f32) (result f32)
                (if (result f32) (f32.ge (global.get $c2) (f32.const 0.5))
                    (then (f32.add
                        (select (f32.const 0.5) (global.get $c3) (f32.gt (global.get $file) (f32.const 0)))
                        (f32.mul (global.get $c0) (f32.const 0.25))) {press})
                    (else (f32.const -1))))
            (func (export "procraft_pressure_taper") (param i32 i32) (result f32) (f32.const 0)))"#
        );
        wat::parse_str(text).unwrap()
    }

    /// Its manifest: Offset answering `offset_hooks`, Press the pressure hook with a file under it.
    fn featured_manifest(module: &[u8], offset_hooks: Value) -> Value {
        let mut manifest = coded(FEATURED, module);
        manifest["compat"]["procraft"]["interface"] = json!(16);
        let mut hooks = vec![json!("position"), json!("pressure")];
        if offset_hooks.as_array().is_some_and(|h| h.contains(&json!("stroke_sound"))) {
            hooks.push(json!("stroke_sound"));
        }
        manifest["hooks"] = json!(hooks);
        manifest["controls"] = json!([
            { "key": "offset", "type": "feature", "label": "Offset", "description": "Moves samples.", "hooks": offset_hooks },
            { "key": "x", "type": "slider", "label": "X", "least": -50, "most": 50, "starts": 0, "under": "offset" },
            { "key": "press", "type": "feature", "label": "Press", "description": "Presses.", "hooks": ["pressure"] },
            { "key": "level", "type": "slider", "label": "Level", "least": 0, "most": 0.5, "step": 0.1, "starts": 0.2, "under": "press" },
            { "key": "sound", "type": "file", "label": "Sound", "accepts": "sound", "under": "press" },
        ]);
        manifest
    }

    fn featured_plugins(dir: &str, module: &[u8], offset_hooks: Value) -> Arc<Plugins> {
        let plugins = Plugins::with_store(dir.to_owned());
        assert_eq!(plugins.install(file(&featured_manifest(module, offset_hooks), module)), None);
        plugins
    }

    fn pressed(hooks: &Hooks) -> Option<f32> {
        let asked =
            crate::plugin_run::PressureAsked { pointer: 0, first: true, x: 1.0, y: 1.0, ms: 0.0, pressure: 0.6 };
        hooks.pressure(asked)
    }

    /// The features of `id` with a copy of its code on the hooks, by their switches' places.
    fn copies(hooks: &Hooks, id: &str) -> Vec<Option<usize>> {
        hooks.working().on.iter().filter(|p| p.id == id).map(|p| p.feature).collect()
    }

    #[test]
    fn each_feature_on_runs_in_a_copy_of_its_own_on_its_own_hooks_as_the_plugin_it_was() {
        let dir = store("features");
        let module = featured_module("", 0);
        let plugins = featured_plugins(&dir, &module, json!(["position"]));
        let hooks = plugins.hooks();
        // on, with no feature on: on no hook at all
        assert!(plugins.set_on(FEATURED.to_owned(), true));
        assert_eq!((hooks.position(10.0, 10.0), pressed(&hooks)), ((10.0, 10.0), None));
        assert_eq!(hooks.hooked.load(Ordering::Relaxed), 0);
        assert!(copies(&hooks, FEATURED).is_empty());
        // Offset on: on the position hook alone
        assert!(plugins.set_switch(format!("{FEATURED}.offset"), true));
        assert_eq!(plugins.set_slider(format!("{FEATURED}.x"), 5.0), Some(5.0));
        assert_eq!((hooks.position(10.0, 10.0), pressed(&hooks)), ((15.0, 10.0), None));
        assert_eq!(hooks.hooked.load(Ordering::Relaxed), Hook::Position.bit());
        // Press on too: a copy of its own; each told the other's switch as off, so neither answers
        // for the other (Offset not 100 further, Press not a quarter higher)
        assert!(plugins.set_switch(format!("{FEATURED}.press"), true));
        assert_eq!(copies(&hooks, FEATURED), [Some(0), Some(2)]);
        assert_eq!(hooks.position(10.0, 10.0), (15.0, 10.0));
        assert!((pressed(&hooks).unwrap() - 0.2).abs() < 1e-6);
        assert_eq!(plugins.set_slider(format!("{FEATURED}.level"), 0.4), Some(0.4));
        assert!((pressed(&hooks).unwrap() - 0.4).abs() < 1e-6);
        // the card: each feature a heading, what comes under it while it is on
        let shown = card(&plugins);
        let features: Vec<(&str, &str, usize)> = shown
            .features
            .iter()
            .map(|f| (f.control.label.as_str(), f.description.as_str(), f.controls.len()))
            .collect();
        assert_eq!(features, [("Offset", "Moves samples.", 1), ("Press", "Presses.", 2)]);
        // Offset off: its copy goes, Press's stays
        assert!(plugins.set_switch(format!("{FEATURED}.offset"), false));
        assert_eq!(copies(&hooks, FEATURED), [Some(2)]);
        assert_eq!(hooks.position(10.0, 10.0), (10.0, 10.0));
        assert!(pressed(&hooks).is_some());
        assert_eq!(card(&plugins).features.first().map(|f| f.controls.len()), Some(0));
        // through a restart, as kept
        let next = restarted(&dir, &plugins);
        let next_hooks = next.hooks();
        assert_eq!(copies(&next_hooks, FEATURED), [Some(2)]);
        assert!((pressed(&next_hooks).unwrap() - 0.4).abs() < 1e-6);
        // the plugin off: every copy goes
        assert!(plugins.set_on(FEATURED.to_owned(), false));
        assert!(copies(&hooks, FEATURED).is_empty());
        assert_eq!(hooks.hooked.load(Ordering::Relaxed), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_picked_under_a_feature_goes_to_its_copy_alone() {
        let dir = store("features-files");
        let module = featured_module("", 0);
        let plugins = featured_plugins(&dir, &module, json!(["position"]));
        let hooks = plugins.hooks();
        assert!(plugins.set_on(FEATURED.to_owned(), true));
        assert!(plugins.set_switch(format!("{FEATURED}.offset"), true));
        assert!(plugins.set_switch(format!("{FEATURED}.press"), true));
        let wav = crate::sound_file::write_wav(&[0.25; 800], 8000);
        assert!(plugins.set_sound_file(format!("{FEATURED}.sound"), "Tick".to_owned(), wav));
        // Press was handed it (0.5 from then on), Offset not (its y as it was)
        assert!((pressed(&hooks).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(hooks.position(10.0, 10.0), (10.0, 10.0));
        // and as Press comes on again, or on a start
        assert!(plugins.set_switch(format!("{FEATURED}.press"), false));
        assert!(plugins.set_switch(format!("{FEATURED}.press"), true));
        assert!((pressed(&hooks).unwrap() - 0.5).abs() < 1e-6);
        let next = restarted(&dir, &plugins);
        assert!((pressed(&next.hooks()).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(next.hooks().position(10.0, 10.0), (10.0, 10.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_feature_s_copy_has_the_memory_its_own_hooks_give() {
        // each sample asks for 1 MiB more: the position hook's 16 MiB is spent by the sixteenth,
        // while the sound hook's 128 MB takes forty
        for (hooks_of_offset, lasts) in [(json!(["position"]), false), (json!(["position", "stroke_sound"]), true)] {
            let dir = store("features-memory");
            let module = featured_module("", 16);
            let plugins = featured_plugins(&dir, &module, hooks_of_offset.clone());
            let hooks = plugins.hooks();
            assert!(plugins.set_on(FEATURED.to_owned(), true));
            assert!(plugins.set_switch(format!("{FEATURED}.offset"), true));
            for _ in 0..40 {
                hooks.position(1.0, 1.0);
            }
            assert_eq!(card(&plugins).on, lasts, "{hooks_of_offset}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn one_feature_s_copy_going_wrong_switches_the_whole_plugin_off() {
        let dir = store("features-stop");
        let module = featured_module("(unreachable)", 0);
        let plugins = featured_plugins(&dir, &module, json!(["position"]));
        let hooks = plugins.hooks();
        assert!(plugins.set_on(FEATURED.to_owned(), true));
        assert!(plugins.set_switch(format!("{FEATURED}.offset"), true));
        assert!(plugins.set_switch(format!("{FEATURED}.press"), true));
        assert_eq!(copies(&hooks, FEATURED).len(), 2);
        assert_eq!(pressed(&hooks), None);
        assert!(copies(&hooks, FEATURED).is_empty(), "Offset's copy went too");
        assert_eq!(hooks.position(10.0, 10.0), (10.0, 10.0));
        let card = card(&plugins);
        assert!(!card.on && card.problem.as_deref() == Some("Switched off: it crashed."), "{card:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What Brush extras' per-sample hooks cost a sample, Finger pressure's and Touch position
    /// offset's on, alone and with Brush sounds making its sound on a thread of its own meanwhile:
    /// printed, each the best of three runs. With `PROCRAFT_OLD_PLUGINS` naming a folder holding the
    /// files of the plugins they were made of (`finger-pressure`, `touch-offset` and `brush-sounds`,
    /// `.procraft-plugin` each), those are timed the same way in the same run, and the merged one may
    /// cost a sample a tenth more at most (and a microsecond, for noise). Run alone:
    /// `cargo test -p procraft-core sample_hooks_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn sample_hooks_cost() {
        const SAMPLES: u32 = 50_000;
        // a sample through the pressure hook then the position hook, as a stroke's goes
        let cost = |hooks: &Hooks| {
            let best = (0..3).map(|_| {
                let t = Instant::now();
                let mut sum = 0.0;
                for i in 0..SAMPLES {
                    let ms = i as f32 * 4.0;
                    let x = std::hint::black_box(ms * 1.5 % 2000.0);
                    let asked = crate::plugin_run::PressureAsked {
                        pointer: 0,
                        first: i % 500 == 0,
                        x,
                        y: 300.0,
                        ms,
                        pressure: 0.5,
                    };
                    sum += hooks.pressure(asked).unwrap_or(0.0);
                    sum += hooks.position(x, 300.0).0;
                }
                std::hint::black_box(sum);
                t.elapsed().as_secs_f64() * 1e6 / f64::from(SAMPLES)
            });
            best.fold(f64::MAX, f64::min)
        };
        // the same while a thread asks for the stroke's sound as fast as it can
        let sounding = |hooks: &Arc<Hooks>| {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let thread = {
                let (hooks, stop) = (Arc::clone(hooks), Arc::clone(&stop));
                std::thread::spawn(move || {
                    let motion = crate::stroke_sound::SoundMotion {
                        speed: 1.0,
                        pressure: 1.0,
                        since_ms: 0.0,
                        state: 1,
                        hover: (0.0, 0.0),
                        height: 1000.0,
                    };
                    let brush = crate::stroke_sound::SoundBrush {
                        name: "Pencil".to_owned(),
                        set: "Sketching".to_owned(),
                        controls: vec![0.0],
                        key: 1,
                    };
                    let mut made = 0_u64;
                    while !stop.load(Ordering::Relaxed) {
                        made += hooks.stroke_sound(256, 48_000, &motion, &brush).map_or(0, |s| s.len() as u64);
                    }
                    made
                })
            };
            let each = cost(hooks);
            stop.store(true, Ordering::Relaxed);
            let made = thread.join().unwrap();
            assert!(made > 0, "the sound was made meanwhile");
            each
        };
        let dir = store("sample-cost");
        let plugins = Plugins::with_bundled(dir.clone());
        let hooks = plugins.hooks();
        let none = cost(&hooks);
        const EXTRAS: &str = "io.github.artcraftandroid.brush-extras";
        assert!(plugins.set_on(EXTRAS.to_owned(), true));
        for feature in ["finger_pressure", "touch_offset"] {
            assert!(plugins.set_switch(format!("{EXTRAS}.{feature}"), true));
        }
        assert!(plugins.set_slider(format!("{EXTRAS}.x"), 3.0).is_some());
        let merged = cost(&hooks);
        assert!(plugins.set_switch(format!("{EXTRAS}.brush_sounds"), true));
        let merged_sounding = sounding(&hooks);
        println!("a sample through the pressure and position hooks: {none:.3} µs with none on");
        println!("Brush extras: {merged:.3} µs; {merged_sounding:.3} µs while Brush sounds makes its sound");
        let _ = std::fs::remove_dir_all(&dir);
        // the plugins it was made of, the same way, in the same run
        let Ok(folder) = std::env::var("PROCRAFT_OLD_PLUGINS") else { return };
        let dir = store("sample-cost-old");
        let plugins = Plugins::with_store(dir.clone());
        let hooks = plugins.hooks();
        let old =
            |name: &str| std::fs::read(std::path::Path::new(&folder).join(format!("{name}.procraft-plugin"))).unwrap();
        for name in ["finger-pressure", "touch-offset", "brush-sounds"] {
            assert_eq!(plugins.install(old(name)), None, "{name}");
        }
        for name in ["finger-pressure", "touch-offset"] {
            assert!(plugins.set_on(format!("io.github.artcraftandroid.{name}"), true));
        }
        assert!(plugins.set_slider("io.github.artcraftandroid.touch-offset.x".to_owned(), 3.0).is_some());
        let parts = cost(&hooks);
        assert!(plugins.set_on("io.github.artcraftandroid.brush-sounds".to_owned(), true));
        let parts_sounding = sounding(&hooks);
        println!(
            "the plugins it was made of: {parts:.3} µs; {parts_sounding:.3} µs while Brush sounds makes its sound"
        );
        assert!(merged <= parts * 1.1 + 1.0, "{merged} against {parts}");
        assert!(merged_sounding <= parts_sounding * 1.1 + 1.0, "{merged_sounding} against {parts_sounding}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_brush_cursor_stands_where_a_stroke_s_first_stamp_lands_as_a_plugin_moves_it() {
        let dir = store("cursor-lands");
        let plugins = with_offset(&dir);
        assert!(plugins.set_on(ID.to_owned(), true));
        assert_eq!(plugins.set_slider(format!("{ID}.x"), 12.0), Some(12.0));
        assert_eq!(plugins.set_slider(format!("{ID}.y"), -7.0), Some(-7.0));
        let Some(s) = session() else { return };
        // with no plugins on the session, where the pointer is
        assert_eq!(s.cursor_point(80.0, 90.0), [80.0, 90.0]);
        s.use_plugins(Arc::clone(&plugins));
        let cursor = s.cursor_point(80.0, 90.0);
        assert_eq!(cursor, [92.0, 83.0]);
        // a tap there lays one stamp: its middle, by its pixels, is where the cursor stands
        s.stroke_begin(PointerKind::Stylus, vec![sample(80.0, 90.0, 0.0)]);
        s.stroke_end(vec![]);
        let rgba = picture(&s);
        let (mut weight, mut across, mut down) = (0.0, 0.0, 0.0);
        for (i, pixel) in rgba.chunks(4).enumerate() {
            let a = pixel.get(3).copied().map_or(0.0, f64::from);
            weight += a;
            across += a * ((i % 200) as f64 + 0.5);
            down += a * ((i / 200) as f64 + 0.5);
        }
        assert!(weight > 0.0, "the tap painted");
        let middle = (across / weight, down / weight);
        let off = ((middle.0 - f64::from(cursor[0])).abs(), (middle.1 - f64::from(cursor[1])).abs());
        println!("the cursor at {cursor:?}, the stamp's middle at {middle:?}: {off:?} apart");
        assert!(off.0 < 0.5 && off.1 < 0.5, "{off:?}");
        // off, where the pointer is again
        assert!(plugins.set_on(ID.to_owned(), false));
        assert_eq!(s.cursor_point(80.0, 90.0), [80.0, 90.0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
