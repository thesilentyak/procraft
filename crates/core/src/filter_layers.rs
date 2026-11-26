//! Filter layers (interface 17): a layer with no paint of its own that shows what is below it run
//! through a filter a plugin offers. The layer keeps the filter's name (`<plugin id>.<key>`) and
//! its values (`LayerFilter` in the document), so it saves, undoes, duplicates and groups as any
//! layer, and its opacity, blend mode, mask, clipping and visibility say how it shows. The canvas
//! draws it (procraft-gpu's `filter_layers.rs`); this gives the canvas each filter, made from the
//! plugin's code the first time an artwork has a layer of it, and opens a layer's values in the
//! filter's panel, as an adjustment is open: what is set there shows at once, Apply keeps it as one
//! step and Cancel puts it back. With its plugin off or gone, the layer shows nothing, its values
//! kept as they are.
//!
//! A filter with an editor (interface 18) opens the app's own Curves or Gradient Map panel instead,
//! on the layer: what is set there (five curves, alpha last; or a gradient's stops) is kept with
//! the layer as lists, which the canvas bakes for the filter's code to read.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use procraft_engine::document::{Document, LayerFilter, LayerId};
use procraft_gpu::{FilterReach, FilterRun, FilterTable, LayerFilterProgram};

use crate::plugin_file::{self, value_names};
use crate::plugins::PluginControl;
use crate::{CanvasSession, State};

/// What an undo step of a filter layer's settings is called.
const SETTINGS_STEP: &str = "Filter Settings";
/// What adding one is called.
const NEW_STEP: &str = "new layer";

/// A filter layer a plugin on offers, for the Layers panel's +: its id (`<plugin id>.<key>`), its
/// name, and the heading it is listed under.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginLayerFilter {
    pub id: String,
    pub label: String,
    pub group: String,
}

/// What a touch on the canvas sets while a filter layer's settings are open, as its filter says:
/// a point (the touch's, canvas pixels; starting in the canvas's middle), an angle (the drag's, in
/// radians, 0 along the canvas's top and a quarter turn down; starting at 0), or a seed for its
/// randomness (drawn afresh by a tap; starting at 0). Its code reads them as `touch_x` and
/// `touch_y`, `touch_angle`, or `touch_seed`, after its own values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FilterTouch {
    Point,
    Angle,
    Seed,
}

/// The app's editor a filter layer's settings open in (interface 18): Curves, its five curves kept
/// as lists `curves.0` (all) to `curves.4` (alpha), x and y points; or Gradient Map, its stops kept
/// as list `gradient`, place, red, green and blue each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FilterEditor {
    Curves,
    Gradient,
}

/// A new gradient's stops: black, grey and white.
const FIRST_STOPS: [f32; 12] = [0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0];

impl FilterEditor {
    pub(crate) fn named(name: &str) -> Option<FilterEditor> {
        match name {
            "curves" => Some(FilterEditor::Curves),
            "gradient" => Some(FilterEditor::Gradient),
            _ => None,
        }
    }

    /// The key its lists go by, and how the canvas bakes them.
    fn table(self) -> (String, FilterTable) {
        match self {
            FilterEditor::Curves => {
                ("curves".to_owned(), FilterTable::Curves { channels: crate::adjusting::CURVES as u32 })
            }
            FilterEditor::Gradient => ("gradient".to_owned(), FilterTable::Gradient),
        }
    }

    /// Its lists as a new layer keeps them: straight curves, or black, grey and white.
    fn starting(self) -> Vec<(String, Vec<f32>)> {
        match self {
            FilterEditor::Curves => {
                (0..crate::adjusting::CURVES).map(|n| (format!("curves.{n}"), vec![0.0, 0.0, 1.0, 1.0])).collect()
            }
            FilterEditor::Gradient => vec![("gradient".to_owned(), FIRST_STOPS.to_vec())],
        }
    }
}

/// The most a seed a tap draws can be.
const MOST_SEED: f32 = 1024.0;

impl FilterTouch {
    pub(crate) fn named(name: &str) -> Option<FilterTouch> {
        match name {
            "point" => Some(FilterTouch::Point),
            "angle" => Some(FilterTouch::Angle),
            "seed" => Some(FilterTouch::Seed),
            _ => None,
        }
    }

    /// The names its values go by.
    pub(crate) fn keys(self) -> &'static [&'static str] {
        match self {
            FilterTouch::Point => &["touch_x", "touch_y"],
            FilterTouch::Angle => &["touch_angle"],
            FilterTouch::Seed => &["touch_seed"],
        }
    }

    /// Its values as a new layer on a canvas `width` × `height` keeps them.
    fn starting(self, width: u32, height: u32) -> Vec<(String, f32)> {
        let values: Vec<f32> = match self {
            FilterTouch::Point => vec![width as f32 / 2.0, height as f32 / 2.0],
            FilterTouch::Angle | FilterTouch::Seed => vec![0.0],
        };
        self.keys().iter().map(|k| (*k).to_owned()).zip(values).collect()
    }
}

/// What a filter layer's filter is made of, as the plugin offering it lists it.
#[derive(Clone, Debug)]
pub(crate) struct LayerFilterDef {
    pub plugin: String,
    pub label: String,
    /// Its values as they start (keyed `<id>.<filter>.<key>`), in the order its code reads them.
    pub values: Vec<PluginControl>,
    pub reach: FilterReach,
    pub touch: Option<FilterTouch>,
    pub editor: Option<FilterEditor>,
    pub text: Arc<str>,
    /// Where the plugins are kept, to note a filter on trial in.
    pub store: Option<PathBuf>,
}

impl LayerFilterDef {
    /// Each value's key, as a layer keeps it and its code reads it: its own, then its touch's.
    fn keys(&self) -> Vec<String> {
        let mut keys = value_names(&self.values);
        keys.extend(self.touch.iter().flat_map(|t| t.keys()).map(|k| (*k).to_owned()));
        keys
    }

    /// Its values as a new layer of it on a canvas `width` × `height` keeps them: where they start.
    fn starting(&self, width: u32, height: u32) -> Vec<(String, f32)> {
        let mut values: Vec<(String, f32)> =
            value_names(&self.values).into_iter().zip(self.values.iter().map(PluginControl::number)).collect();
        values.extend(self.touch.map(|t| t.starting(width, height)).unwrap_or_default());
        values
    }
}

/// A filter made for the canvas, from the code it was made of; none if that didn't hold up on this
/// device (not tried again until the code changes).
pub(crate) struct MadeFilter {
    text: Arc<str>,
    program: Option<LayerFilterProgram>,
}

/// The filter layers' part of the session's state.
#[derive(Default)]
pub(crate) struct FilterLayers {
    /// What the plugins on offer, as the session last took them up.
    pub(crate) defs: Arc<HashMap<String, LayerFilterDef>>,
    made: HashMap<String, MadeFilter>,
    /// The filters the canvas has, by name, each with the code it was made of.
    given: BTreeMap<String, Arc<str>>,
    /// The canvas they were given to.
    given_to: Option<u64>,
    /// The filter layer just added, and the artwork's revision then: its settings opened next, with
    /// nothing changed between, are its first, and Cancel there takes the layer away again.
    just_added: Option<(LayerId, u64)>,
}

impl State {
    /// Gives the canvas the filters its filter layers run, made now if they weren't: those a
    /// plugin on offers. A filter that won't compile, or runs out of its budget on the pixels it
    /// is tried on, is left out (and said so in the log), so its layers show nothing. One the canvas
    /// has stays while it is offered: the work queued as its layer went (merged down into the layer
    /// below) runs it yet.
    pub(crate) fn give_layer_filters(&mut self) {
        let (Some(gpu), Some(canvas_id)) = (self.gpu.clone(), self.canvas.as_deref().map(procraft_gpu::Canvas::id))
        else {
            return;
        };
        let defs = Arc::clone(&self.filter_layers.defs);
        let kept = self.filter_layers.given.keys().filter(|_| self.filter_layers.given_to == Some(canvas_id));
        let mut wanted: Vec<&str> = self
            .doc
            .layers()
            .iter()
            .filter_map(|l| l.filter.as_ref())
            .map(|f| f.filter.as_str())
            .chain(kept.map(String::as_str))
            .filter(|name| defs.contains_key(*name))
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        for name in &wanted {
            let Some(def) = defs.get(*name) else { continue };
            let fresh = self.filter_layers.made.get(*name).is_none_or(|m| m.text != def.text);
            if !fresh {
                continue;
            }
            let Some(canvas) = self.canvas.as_ref() else { return };
            let program = make(&gpu, canvas, name, def);
            self.filter_layers.made.insert((*name).to_owned(), MadeFilter { text: Arc::clone(&def.text), program });
        }
        let given: BTreeMap<String, Arc<str>> = wanted
            .iter()
            .filter_map(|name| {
                let made = self.filter_layers.made.get(*name)?;
                made.program.as_ref().map(|_| ((*name).to_owned(), Arc::clone(&made.text)))
            })
            .collect();
        if given == self.filter_layers.given && self.filter_layers.given_to == Some(canvas_id) {
            return;
        }
        let programs: HashMap<String, LayerFilterProgram> = given
            .keys()
            .filter_map(|name| {
                let program = self.filter_layers.made.get(name)?.program.clone()?;
                Some((name.clone(), program))
            })
            .collect();
        if let Some(canvas) = self.canvas.as_mut() {
            canvas.set_layer_filters(programs);
        }
        self.filter_layers.given = given;
        self.filter_layers.given_to = Some(canvas_id);
    }

    /// The filter layers the plugins on offer from now on.
    pub(crate) fn offer_layer_filters(&mut self, defs: HashMap<String, LayerFilterDef>) {
        self.filter_layers.defs = Arc::new(defs);
        self.give_layer_filters();
        self.needs_frame = true;
    }

    /// Layer `id`'s filter's name as the plugins call it (its own while no plugin on offers it).
    pub(crate) fn filter_label(&self, filter: &LayerFilter) -> String {
        self.filter_layers.defs.get(&filter.filter).map_or_else(|| filter.filter.clone(), |d| d.label.clone())
    }
}

/// Filter `name` made from `def` for this device and tried on `canvas`, noted as on trial while it
/// is: a driver that falls over on it takes the app with it, and the plugin is off when it starts
/// again.
fn make(
    gpu: &procraft_gpu::Gpu,
    canvas: &procraft_gpu::Canvas,
    name: &str,
    def: &LayerFilterDef,
) -> Option<LayerFilterProgram> {
    let keys = def.keys();
    let read =
        if def.editor.is_some() { procraft_gpu::read_layer_table_filter } else { procraft_gpu::read_layer_filter };
    let code = match read(&def.text, &keys) {
        Ok(code) => code,
        Err(why) => {
            log::warn!("a filter layer's filter was refused: {name}: {why}");
            return None;
        }
    };
    if let Some(store) = &def.store {
        plugin_file::note_trying(store, &def.plugin);
    }
    let made = gpu.make_filter(code);
    let tried = made.as_ref().ok().map(|program| canvas.try_layer_filter(gpu, program));
    if let Some(store) = &def.store {
        plugin_file::clear_trying(store);
    }
    let program = match (made, tried) {
        (Ok(program), Some(FilterRun::Drew(_))) => program,
        (Ok(_), run) => {
            log::warn!("a filter layer's filter stopped as it was tried: {name}: {run:?}");
            return None;
        }
        (Err(e), _) => {
            log::warn!("a filter layer's filter wouldn't compile: {name}: {e}");
            return None;
        }
    };
    // (a touch's values, where a layer has none, as on a canvas of no size)
    let starts = def.values.iter().map(PluginControl::number).chain(std::iter::repeat(0.0));
    let values = keys.into_iter().zip(starts).collect();
    Some(LayerFilterProgram { program, values, reach: def.reach, table: def.editor.map(FilterEditor::table) })
}

/// A filter layer's settings open in its filter's panel.
pub(crate) struct LayerEdit {
    pub(crate) layer: LayerId,
    pub(crate) name: String,
    /// Its values as set, each starting where it was when the panel opened.
    pub(crate) controls: Vec<PluginControl>,
    /// The artwork as it was then.
    before: Document,
    /// Opened as its layer was added: Cancel takes the layer away again.
    pub(crate) added: bool,
}

impl LayerEdit {
    /// The values `filter` keeps, in `def`'s controls, each starting so.
    fn of(layer: LayerId, filter: &LayerFilter, def: &LayerFilterDef, before: Document) -> LayerEdit {
        let mut controls = def.values.clone();
        for control in &mut controls {
            let key = value_names(std::slice::from_ref(control)).pop().unwrap_or_default();
            if let Some(v) = filter.value(&key) {
                set_number(control, v);
            }
            control.make_starting();
        }
        LayerEdit { layer, name: def.label.clone(), controls, before, added: false }
    }

    /// Its values onto its layer in `doc`; false if that is no filter layer any more.
    fn write(&self, doc: &mut Document) -> bool {
        let keys = value_names(&self.controls);
        let mut all = true;
        for (key, control) in keys.iter().zip(&self.controls) {
            all &= doc.set_filter_value(self.layer, key, control.number());
        }
        all
    }
}

/// `control` set to `value`, as a layer keeps it (a switch on from 0.5).
fn set_number(control: &mut PluginControl, value: f32) {
    match control {
        PluginControl::Switch { control } => control.on = value >= 0.5,
        PluginControl::Slider { control } => control.value = control.stepped(value),
        PluginControl::File { .. } | PluginControl::Texture { .. } | PluginControl::Folder { .. } => {}
    }
}

impl State {
    /// The filter layer's settings open, with their values as set, drawn now.
    pub(crate) fn draw_layer_edit(&mut self) {
        let Some(edit) = self.filters.layer.as_ref() else { return };
        if edit.write(&mut self.doc)
            && let Some(canvas) = self.canvas.as_mut()
        {
            canvas.composite_again();
        }
        self.needs_frame = true;
    }

    /// Ends the filter layer's settings open: `commit` keeps what was set as one step (if anything
    /// changed), otherwise its values go back. They can be opened again (Reset, Undo) as they began.
    pub(crate) fn end_layer_edit(&mut self, commit: bool) {
        let Some(edit) = self.filters.layer.take() else { return };
        let was = edit.before.layer(edit.layer).and_then(|l| l.filter.clone());
        let now = self.doc.layer(edit.layer).and_then(|l| l.filter.clone());
        if commit && was != now {
            if let Some(canvas) = self.canvas.as_mut() {
                canvas.name_next_step(SETTINGS_STEP);
                canvas.record_document(edit.before.clone());
            }
            self.revision += 1;
        } else if was != now {
            if let Some(was) = was {
                self.doc.put_filter(edit.layer, was);
            }
            if let Some(canvas) = self.canvas.as_mut() {
                canvas.composite_again();
            }
        }
        self.filters.keep_layer_edit(edit);
        self.needs_frame = true;
    }
}

impl CanvasSession {
    /// The touch the filter layer's settings open take, if they take one.
    fn filter_layer_touch_kind(&self) -> Option<FilterTouch> {
        let st = self.shared.lock();
        let edit = st.filters.layer.as_ref()?;
        let filter = st.doc.layer(edit.layer)?.filter.as_ref()?;
        st.filter_layers.defs.get(&filter.filter)?.touch
    }

    /// Whether a filter layer's settings are open, its filter taking touches on the canvas.
    pub(crate) fn filter_layer_touched(&self) -> bool {
        self.filter_layer_touch_kind().is_some()
    }

    /// A touch on the canvas dragged from window point `from` to `to`: the filter layer's point is
    /// where it is now, or its angle the drag's. False if it took none.
    pub(crate) fn filter_layer_drag(&self, from: (f32, f32), to: (f32, f32)) -> bool {
        let Some(touch) = self.filter_layer_touch_kind() else { return false };
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let (Some(a), Some(b)) = (st.window_to_canvas(from.0, from.1), st.window_to_canvas(to.0, to.1)) else {
            return false;
        };
        let values = match touch {
            FilterTouch::Point => vec![("touch_x", b.x), ("touch_y", b.y)],
            FilterTouch::Angle => vec![("touch_angle", (b.y - a.y).atan2(b.x - a.x))],
            FilterTouch::Seed => return false,
        };
        st.touch_filter_layer(&values);
        self.shared.frame(st);
        true
    }

    /// A tap on the canvas at window point `at`: the filter layer's point goes there, or its seed is
    /// drawn afresh. False if it took none.
    pub(crate) fn filter_layer_tap(&self, at: (f32, f32)) -> bool {
        let Some(touch) = self.filter_layer_touch_kind() else { return false };
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let values = match touch {
            FilterTouch::Point => match st.window_to_canvas(at.0, at.1) {
                Some(p) => vec![("touch_x", p.x), ("touch_y", p.y)],
                None => return false,
            },
            FilterTouch::Angle => return false,
            FilterTouch::Seed => {
                let was =
                    st.doc.layer(st.doc.current()).and_then(|l| l.filter.as_ref()?.value("touch_seed")).unwrap_or(0.0);
                vec![("touch_seed", next_seed(was, at))]
            }
        };
        st.touch_filter_layer(&values);
        self.shared.frame(st);
        true
    }
}

/// A seed drawn afresh from the last one and where the tap was: another whole number below
/// [`MOST_SEED`].
fn next_seed(was: f32, (x, y): (f32, f32)) -> f32 {
    let mut h = (was.to_bits() as u64) ^ ((x.to_bits() as u64) << 21) ^ ((y.to_bits() as u64) << 42);
    // (splitmix64)
    h = h.wrapping_add(0x9E37_79B9_7F4A_7C15);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    let seed = (h % MOST_SEED as u64) as f32;
    if seed == was { (seed + 1.0) % MOST_SEED } else { seed }
}

impl State {
    /// The filter layer whose settings are open given `values` by a touch, shown at once.
    fn touch_filter_layer(&mut self, values: &[(&str, f32)]) {
        let Some(edit) = self.filters.layer.as_ref() else { return };
        let layer = edit.layer;
        let mut any = false;
        for (key, value) in values {
            any |= self.doc.set_filter_value(layer, key, *value);
        }
        if any {
            self.adjust.touch();
            if let Some(canvas) = self.canvas.as_mut() {
                canvas.composite_again();
            }
        }
        self.needs_frame = true;
    }
}

#[uniffi::export]
impl CanvasSession {
    /// A new filter layer running filter `id` (as [`crate::Plugins::layer_filters`] lists it) above
    /// the current layer (in its group), named after the filter and made current, its values where
    /// the filter starts them. 0 if no plugin on offers it, or the canvas has no room for another.
    /// Its settings opened next ([`Self::filter_layer_settings`]) are its first: Cancel there takes
    /// it away again, as if it was never added.
    pub fn add_filter_layer(&self, id: String) -> u64 {
        let def = self.shared.lock().filter_layers.defs.get(&id).cloned();
        let Some(def) = def else { return 0 };
        let (width, height) = {
            let st = self.shared.lock();
            (st.doc.width(), st.doc.height())
        };
        let mut filter = LayerFilter::new(&id, def.starting(width, height));
        for (key, numbers) in def.editor.map(FilterEditor::starting).unwrap_or_default() {
            filter.set_table(&key, &numbers);
        }
        let id = self
            .change(NEW_STEP, |d| {
                // named after its filter alone
                d.has_room(1).then(|| d.add_filter_layer(filter, &def.label).0)
            })
            .unwrap_or(0);
        if id != 0 {
            let mut st = self.shared.lock();
            st.filter_layers.just_added = Some((LayerId(id), st.revision));
            // its filter made now, so whatever draws the canvas next shows it
            st.give_layer_filters();
            drop(st);
            self.tell_plugins(crate::plugin_run::Hook::LayerAdded);
        }
        id
    }

    /// Opens the current layer's filter settings in its filter's panel, as an adjustment is
    /// open (keeping one open first): its values show at once as they are set, Apply keeps them
    /// as one step, Cancel puts them back (and takes away a layer just added, opened as it was
    /// added). False if the current layer is no filter layer, or no plugin on offers its filter.
    pub fn filter_layer_settings(&self) -> bool {
        self.adjustment_end(true);
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        let current = st.doc.current();
        let Some(filter) = st.doc.layer(current).and_then(|l| l.filter.clone()) else { return false };
        let Some(def) = st.filter_layers.defs.get(&filter.filter).cloned() else { return false };
        if st.doc.is_locked(current) {
            return false;
        }
        let mut edit = LayerEdit::of(current, &filter, &def, st.doc.clone());
        // its first settings, opened as it was added with nothing changed since
        let revision = st.revision;
        edit.added = st.filter_layers.just_added.take() == Some((current, revision));
        match def.editor {
            Some(editor) => open_editor(st, edit, editor, &filter),
            None => {
                st.adjust.open_plugin();
                st.filters.open_layer_edit(edit);
            }
        }
        self.shared.frame(st);
        true
    }
}

/// `edit` open in the app's `editor` on its layer, which keeps `filter`: its curves as the layer
/// keeps them, or the library's gradient with its stops chosen (the first, with none).
fn open_editor(st: &mut State, edit: LayerEdit, editor: FilterEditor, filter: &LayerFilter) {
    match editor {
        FilterEditor::Curves => {
            let curves = std::array::from_fn(|n| {
                let numbers = filter.table(&format!("curves.{n}")).unwrap_or_default();
                let points: Vec<(f32, f32)> = numbers.as_chunks::<2>().0.iter().map(|&[x, y]| (x, y)).collect();
                if points.len() < 2 { crate::adjusting::straight() } else { points }
            });
            st.adjust.open_layer_curves(curves);
        }
        FilterEditor::Gradient => {
            let kept = filter.table("gradient").unwrap_or_default();
            let same =
                |stops: &[f32]| stops.len() == kept.len() && stops.iter().zip(kept).all(|(a, b)| (a - b).abs() < 1e-3);
            let list = st.adjust.gradient_list();
            let at = list.iter().position(|g| same(&crate::gradients::Gradients::map_stops(g))).unwrap_or(0);
            st.adjust.open_layer_gradient(at);
        }
    }
    st.filters.open_layer_edit(edit);
}

impl CanvasSession {
    /// The filter layer whose curves are open given `curves` (all, red, green, blue, alpha; x and y
    /// points), shown at once; false if none is open.
    pub(crate) fn set_layer_curves(&self, curves: &[Vec<f32>]) -> bool {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let Some(layer) = st.filters.layer.as_ref().map(|e| e.layer) else { return false };
        for (n, points) in curves.iter().enumerate() {
            st.doc.set_filter_table(layer, &format!("curves.{n}"), points);
        }
        st.layer_editor_drawn();
        self.shared.frame(st);
        true
    }

    /// The filter layer whose gradient is open given `stops` (place, red, green, blue each), shown
    /// at once; false if none is open.
    pub(crate) fn set_layer_gradient(&self, stops: &[f32]) -> bool {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let Some(layer) = st.filters.layer.as_ref().map(|e| e.layer) else { return false };
        st.doc.set_filter_table(layer, "gradient", stops);
        st.layer_editor_drawn();
        self.shared.frame(st);
        true
    }

    /// The current filter layer's editor open again, as its settings were when they last opened
    /// (Reset, Undo, Apply); false if they weren't the last open, or it has none.
    pub(crate) fn filter_layer_editor_again(&self) -> bool {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let current = st.doc.current();
        let Some(filter) = st.doc.layer(current).and_then(|l| l.filter.clone()) else { return false };
        let Some(editor) = st.filter_layers.defs.get(&filter.filter).and_then(|d| d.editor) else { return false };
        let Some(edit) = st.filters.take_last_layer(current) else { return false };
        let edit = edit.again(&st.doc);
        open_editor(st, edit, editor, &filter);
        self.shared.frame(st);
        true
    }
}

impl CanvasSession {
    /// Filter layer `layer`, just added and cancelled in its first settings, taken away again: its
    /// adding undone, with nothing to redo.
    pub(crate) fn take_back_added(&self, layer: LayerId) {
        let undone = self.undo();
        let gone = self.shared.lock().doc.layer(layer).is_none();
        if gone && undone.as_deref() == Some(NEW_STEP) {
            if let Some(canvas) = self.shared.lock().canvas.as_mut() {
                canvas.forget_redo();
            }
        } else {
            log::warn!("a filter layer just added wasn't the last step: {undone:?} undone, the layer gone {gone}");
        }
    }
}

impl State {
    /// What a filter layer's editor set, drawn.
    fn layer_editor_drawn(&mut self) {
        if let Some(canvas) = self.canvas.as_mut() {
            canvas.composite_again();
        }
        self.needs_frame = true;
    }
}

impl LayerEdit {
    /// It again, its values where they began.
    pub(crate) fn again(mut self, doc: &Document) -> LayerEdit {
        for control in &mut self.controls {
            control.back_to_start();
        }
        self.before = doc.clone();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procraft_engine::document::Document;

    fn slider(key: &str, least: f32, most: f32, starts: f32) -> PluginControl {
        PluginControl::Slider {
            control: crate::plugins::PluginSlider {
                key: format!("org.example.f.{key}"),
                label: key.to_owned(),
                least,
                most,
                step: 0.01,
                value: starts,
                starts,
                shown_times: 1.0,
                unit: String::new(),
                at_least: None,
                names: Vec::new(),
                live: true,
            },
        }
    }

    fn def() -> LayerFilterDef {
        LayerFilterDef {
            plugin: "org.example".to_owned(),
            label: "Brighter".to_owned(),
            values: vec![slider("amount", -1.0, 1.0, 0.25), slider("mix", 0.0, 1.0, 1.0)],
            reach: FilterReach::Within(0.0),
            touch: Some(FilterTouch::Point),
            editor: None,
            text: Arc::from("fn pixel(c: vec4<f32>, at: vec2<f32>) -> vec4<f32> { return c; }"),
            store: None,
        }
    }

    #[test]
    fn a_tap_draws_another_seed_each_time() {
        let mut seeds = vec![0.0_f32];
        for i in 0..50 {
            let next = next_seed(*seeds.last().unwrap(), (i as f32, 3.0));
            assert!((0.0..MOST_SEED).contains(&next) && next.fract() == 0.0 && next != *seeds.last().unwrap());
            seeds.push(next);
        }
        seeds.sort_by(f32::total_cmp);
        seeds.dedup();
        assert!(seeds.len() > 40, "{}", seeds.len());
    }

    #[test]
    fn a_layer_s_edit_starts_at_its_values_and_writes_them_back() {
        let mut doc = Document::new(8, 8);
        let d = def();
        let id = doc.add_filter_layer(LayerFilter::new("org.example.f", d.starting(8, 6)), "Brighter 2");
        // its values where the filter starts them, its point in the canvas's middle
        let kept = doc.layer(id).unwrap().filter.clone().unwrap();
        assert_eq!((kept.value("amount"), kept.value("mix")), (Some(0.25), Some(1.0)));
        assert_eq!((kept.value("touch_x"), kept.value("touch_y")), (Some(4.0), Some(3.0)));
        assert_eq!(d.keys(), ["amount", "mix", "touch_x", "touch_y"]);
        assert!(doc.set_filter_value(id, "amount", -0.5));
        let filter = doc.layer(id).unwrap().filter.clone().unwrap();
        let mut edit = LayerEdit::of(id, &filter, &d, doc.clone());
        assert_eq!(edit.controls[0].number(), -0.5, "as the layer keeps it");
        set_number(&mut edit.controls[0], 0.731);
        assert!(edit.write(&mut doc));
        let kept = doc.layer(id).unwrap().filter.as_ref().unwrap().value("amount").unwrap();
        assert!((kept - 0.73).abs() < 1e-5, "on its steps: {kept}");
        // again: back where it began
        let edit = edit.again(&doc);
        assert_eq!(edit.controls[0].number(), -0.5);
        // an ordinary layer takes none
        let plain = doc.add_layer();
        assert!(!doc.set_filter_value(plain, "amount", 0.1));
    }

    #[test]
    fn cancel_in_a_filter_layer_s_first_settings_takes_it_away_and_it_is_named_after_its_filter() {
        const INFINITE: &str = "io.github.artcraftandroid.infinite-plugins";
        let dir = std::env::temp_dir().join(format!("procraft-filter-layer-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = crate::Plugins::with_bundled(dir.to_string_lossy().into_owned());
        assert!(plugins.set_on(INFINITE.to_owned(), true));
        assert!(plugins.set_switch(format!("{INFINITE}.filter_layers"), true));
        let Some(s) = crate::tests::session(32, 32) else { return };
        s.use_plugins(Arc::clone(&plugins));
        s.set_color(1.0, 0.0, 0.0);
        let base = s.layers().last().unwrap().id;
        assert!(s.fill_layer(base));
        let shown = |s: &CanvasSession| crate::tests::pixel(s, 16, 16);
        let redo = |s: &CanvasSession| s.shared.lock().canvas.as_ref().is_some_and(|c| c.can_redo());
        let count = s.layers().len();
        let invert = format!("{INFINITE}.invert");
        // added: named after its filter alone, and shown at once as its settings open
        let layer = s.add_filter_layer(invert.clone());
        assert_ne!(layer, 0);
        assert_eq!(s.layers().into_iter().find(|l| l.id == layer).map(|l| l.name).as_deref(), Some("Invert"));
        assert!(s.filter_layer_settings());
        assert_eq!(shown(&s), [0, 255, 255, 255]);
        // Cancel: it goes again, the canvas as it was, nothing to redo
        assert!(s.adjustment_end(false));
        assert_eq!(s.layers().len(), count);
        assert!(s.layers().iter().all(|l| l.id != layer));
        assert_eq!(shown(&s), [255, 0, 0, 255]);
        assert!(!redo(&s), "its adding isn't there to redo");
        // Apply keeps it; Cancel in its settings opened again later leaves it, its value as kept
        let kept = s.add_filter_layer(invert.clone());
        assert!(s.filter_layer_settings());
        assert!(s.adjustment_end(true));
        assert!(s.filter_layer_settings());
        assert!(s.adjustment_end(false));
        assert!(s.layers().iter().any(|l| l.id == kept));
        assert_eq!(shown(&s), [0, 255, 255, 255]);
        // added, then something else changed before its settings open: not its first, so Cancel
        // leaves it too
        let other = s.add_filter_layer(invert);
        assert!(s.rename_layer(other, "Mine".to_owned()));
        assert!(s.filter_layer_settings());
        assert!(s.adjustment_end(false));
        assert!(s.layers().iter().any(|l| l.id == other && l.name == "Mine"));
        assert_eq!(s.layers().len(), count + 2);
        assert_eq!(shown(&s), [255, 0, 0, 255], "inverted twice");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
