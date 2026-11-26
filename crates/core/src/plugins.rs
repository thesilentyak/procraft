//! What the app does beyond its measured look and behaviour: its plugins, on one list and one page.
//! A plugin is a file. The files the app comes with for now ([`crate::bundled`]) are added once, as
//! if imported, off, and are then like any other. With none on the app is as measured. A plugin's
//! manifest says what it holds: code run in a sandbox on the [hooks](crate::hooks) it asks for, a
//! filter, or both. See [`crate::plugin_file`] for what is asked of a file.
//!
//! All are plain data, a name, a line on what it does and its controls, so the page draws any of
//! them without knowing what it holds.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::KeyCombo;
use crate::hooks::Hooks;
use crate::plugin_file::{self, Added, Manifest, Refusal, Replaced, Shape, Taken};
use crate::plugin_run::{Hook, Running, Stop};

/// A setting the app used to have and a plugin now brings: what it was kept under, as it started
/// (a switch off is 0), and its name. Its value stays there for the plugin, whose control says so.
struct Moved {
    key: &'static str,
    starts: f32,
    name: &'static str,
}

const MOVED: &[Moved] = &[
    Moved { key: "eyedropper_layer", starts: 0.0, name: "Eyedropper picks from layer" },
    Moved { key: "interface_size", starts: 1.0, name: "Interface size" },
    Moved { key: "paste_into_mask", starts: 0.0, name: "Paste into masks" },
    Moved { key: "fill_gap", starts: 0.0, name: "ColorDrop gap closing" },
];

impl Moved {
    /// Whether a value kept as `value` is one someone changed.
    fn changed(&self, value: &str) -> bool {
        let number = match value {
            "true" => Some(1.0),
            "false" => Some(0.0),
            _ => value.parse::<f32>().ok(),
        };
        number.is_some_and(|number| number != self.starts)
    }
}

/// Whether `key` is a setting the app used to have.
pub(crate) fn moved(key: &str) -> bool {
    MOVED.iter().any(|moved| moved.key == key)
}

/// Whether `value`, kept under `key`, is a setting the app used to have that someone changed.
fn moved_and_changed(key: &str, value: &str) -> bool {
    MOVED.iter().any(|moved| moved.key == key && moved.changed(value))
}

/// Why a filter the app ended on, or that stopped the GPU, is off.
pub(crate) const STOPPED_DRIVER: &str = "Switched off: it stopped the graphics driver.";
/// Why a filter whose files no longer hold up can't open.
const FILTER_DAMAGED: &str = "This filter's files don't hold up.";
/// What is told when the plugins' order couldn't be kept.
const ORDER_UNKEPT: &str = "The plugins' order couldn't be saved.";

/// Where one came from. The route decides this, never the plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PluginSource {
    File,
    Catalogue,
}

/// What a plugin holds, as its manifest says: one of these or several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum PluginPart {
    /// Code, running on its hooks while the plugin is on.
    Hooks,
    /// A filter, under Adjustments while the plugin is on.
    Filter,
    /// A set of brushes, added to the brush library the first time the plugin is on (interface 14).
    Brushes,
}

impl PluginPart {
    /// The part a manifest calls `name`, if this app has it.
    pub(crate) fn named(name: &str) -> Option<PluginPart> {
        match name {
            "hooks" => Some(PluginPart::Hooks),
            "filter" => Some(PluginPart::Filter),
            "brushes" => Some(PluginPart::Brushes),
            _ => None,
        }
    }
}

/// A setting on or off. `key` is what its value is kept under: one key to a value, across all of
/// them (a plugin's start with its id).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginSwitch {
    pub key: String,
    pub label: String,
    pub on: bool,
    /// As it starts.
    pub starts: bool,
}

/// A setting along a range, in steps from its `least`.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginSlider {
    pub key: String,
    pub label: String,
    pub least: f32,
    pub most: f32,
    pub step: f32,
    pub value: f32,
    /// As it starts.
    pub starts: f32,
    /// How it reads: the value times this, then `unit` (0.85 as 85%).
    pub shown_times: f32,
    pub unit: String,
    /// What it reads at its least, if not that ("Off").
    pub at_least: Option<String>,
    /// A name for each step from its least, read in place of the number (none: the numbers).
    pub names: Vec<String>,
    /// Its value goes in as it moves; otherwise as it lets go, which what changes the page needs.
    pub live: bool,
}

/// What a file control takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PluginFileKind {
    /// A sound: a recording, kept as a WAV file.
    Sound,
    /// A picture, a PNG or a JPEG, kept as it came (interface 11).
    Picture,
}

/// A file someone picks, kept with the plugin; `chosen` is the name it was picked by, if one is.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginFile {
    pub key: String,
    pub label: String,
    pub accepts: PluginFileKind,
    pub chosen: Option<String>,
}

/// A folder someone picks outside the app (interface 22), kept with the plugin, for the app to keep
/// files in or read them from as the plugin's hooks say (see `folders.rs`). While one is picked,
/// `chosen` is its name as the app's folder picker gave it, and `location` where it is, as the app's
/// folder access reads it (a document tree's address, a path, a bookmark).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginFolder {
    pub key: String,
    pub label: String,
    pub chosen: Option<String>,
    pub location: Option<String>,
}

/// A texture someone picks (interface 15): one of the app's own grains, by its number in the Source
/// Library (from 1; 0, its Blank, for none), or the picture picked for one of the plugin's `file`
/// controls of pictures it offers (`own`, their keys in order), as -1 less its place there.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginTexture {
    pub key: String,
    pub label: String,
    pub own: Vec<String>,
    pub value: i32,
    /// As it starts.
    pub starts: i32,
}

impl PluginTexture {
    /// `value`, if it names a texture this offers; none (0) if it doesn't.
    pub(crate) fn within(&self, value: f32) -> i32 {
        let v = value.round();
        let least = -(self.own.len() as f32);
        let most = procraft_engine::textures::GRAIN_COUNT as f32;
        if v.is_finite() && (least..=most).contains(&v) { v as i32 } else { 0 }
    }
}

/// A picture, RGBA rows from the top (not premultiplied), `width` × `height`.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginPicture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Something a plugin does when its row is tapped: `key` is `<id>.<key>`. Its card lists it, and
/// `place` may list it somewhere else too.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginAction {
    pub key: String,
    pub label: String,
    pub place: PluginPlace,
}

/// Where else a plugin's action is listed, beside its card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PluginPlace {
    /// Only on its card.
    Card,
    /// In the Adjustments menu's Plugins list, with the filters.
    Adjustments,
    /// In the current layer's options, after the app's own.
    Layer,
    /// In Animation Assist's frame options, after the app's own; run with that frame's layer
    /// current.
    Frame,
    /// In the Actions panel's Canvas tab, after the app's own rows (interface 8): an action, or a
    /// switch or a slider shown as on the card.
    Canvas,
    /// Among what Prefs › Pen buttons sets a pen's button to (interface 11): an action run where
    /// the pen is as its button is pressed, see [`Plugins::run_action_at`].
    Pen,
    /// In the Layers panel, a row of its own under the background's, by its label (interface 12).
    Layers,
}

impl PluginPlace {
    /// The place a manifest names; none for a name this app doesn't have.
    pub(crate) fn named(name: &str) -> Option<PluginPlace> {
        match name {
            "card" => Some(PluginPlace::Card),
            "adjustments" => Some(PluginPlace::Adjustments),
            "layer" => Some(PluginPlace::Layer),
            "frame" => Some(PluginPlace::Frame),
            "canvas" => Some(PluginPlace::Canvas),
            "pen" => Some(PluginPlace::Pen),
            "layers" => Some(PluginPlace::Layers),
            _ => None,
        }
    }
}

/// A row a plugin adds to the brush studio's Properties: a choice of a few, by their place, kept
/// with each brush under `key` (`plugin:<id>:<key>`; 0 the first choice, as a brush starts).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginBrushControl {
    pub key: String,
    pub label: String,
    pub choices: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum PluginControl {
    Switch {
        control: PluginSwitch,
    },
    Slider {
        control: PluginSlider,
    },
    File {
        control: PluginFile,
    },
    Texture {
        control: PluginTexture,
    },
    /// A folder someone picks (interface 22).
    Folder {
        control: PluginFolder,
    },
}

/// One as its page shows it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: PluginSource,
    pub holds: Vec<PluginPart>,
    /// As its file says.
    pub version: String,
    pub publisher: String,
    /// Tagged as experimental on its page.
    pub experimental: bool,
    /// Whether it is doing what it does.
    pub on: bool,
    /// What `on` is kept under.
    pub on_key: String,
    /// Why it does nothing, to show on it.
    pub problem: Option<String>,
    /// What its card shows, while it is on. A filter's own are on its panel.
    pub controls: Vec<PluginControl>,
    /// Rows it adds to the brush studio, while it is on.
    pub brush_controls: Vec<PluginBrushControl>,
    /// What it does when tapped, while it is on, and what it may reach then.
    pub actions: Vec<PluginAction>,
    /// The kinds of artwork it makes, while it is on (the gallery's + lists them).
    pub kinds: Vec<crate::PluginKind>,
    /// The kinds of layer it adds, while it is on (the Layers panel's + lists them).
    pub layer_kinds: Vec<crate::PluginKind>,
    pub permissions: Vec<crate::plugin_commands::PluginPermission>,
    /// The features it is made of (interface 16), each with what comes under it while it is on;
    /// none for a plugin written without. Its controls and actions are then all of theirs, but for
    /// `top`.
    pub features: Vec<PluginFeature>,
    /// A plugin of features' controls that come under none of them (interface 22, its `folder`
    /// controls), shown at the top of its card while it is on, before its features; none for a
    /// plugin written without features.
    pub top: Vec<PluginControl>,
}

/// One of the features of a plugin made of several (interface 16), as its card shows it: its
/// switch, its words, and what comes under it while it is on (its controls and actions, as the
/// card shows them).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginFeature {
    pub control: PluginSwitch,
    pub description: String,
    pub controls: Vec<PluginControl>,
    pub actions: Vec<PluginAction>,
}

impl PluginInfo {
    fn holds(&self, part: PluginPart) -> bool {
        self.holds.contains(&part)
    }
}

/// What a plugin file is, for the one screen that asks whether to import it. `refused`: it can't
/// be, and why; nothing else is set.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginOffer {
    pub name: String,
    /// Its version, publisher marked as not checked, and a sentence on what it can do.
    pub message: String,
    pub refused: Option<String>,
}

impl PluginControl {
    pub(crate) fn key(&self) -> &str {
        match self {
            PluginControl::Switch { control } => &control.key,
            PluginControl::Slider { control } => &control.key,
            PluginControl::File { control } => &control.key,
            PluginControl::Texture { control } => &control.key,
            PluginControl::Folder { control } => &control.key,
        }
    }

    /// Takes the value `text`, kept as [`Plugins::read_kept`] reads it, if it is its kind of value.
    fn read(&mut self, text: &str) {
        match self {
            PluginControl::Switch { control } => {
                if let Ok(on) = text.parse() {
                    control.on = on;
                }
            }
            PluginControl::Slider { control } => {
                if let Some(read) = text.parse::<f32>().ok().filter(|v| v.is_finite()) {
                    control.value = control.stepped(read);
                }
            }
            PluginControl::Texture { control } => {
                if let Ok(read) = text.parse::<f32>() {
                    control.value = control.within(read);
                }
            }
            // what was picked is kept with the plugin, not with the settings
            PluginControl::File { .. } | PluginControl::Folder { .. } => {}
        }
    }

    /// Back as it starts.
    pub(crate) fn back_to_start(&mut self) {
        match self {
            PluginControl::Switch { control } => control.on = control.starts,
            PluginControl::Slider { control } => control.value = control.starts,
            PluginControl::Texture { control } => control.value = control.starts,
            PluginControl::File { .. } | PluginControl::Folder { .. } => {}
        }
    }

    /// Starting as it is now: a filter layer's panel opening at the values the layer keeps.
    pub(crate) fn make_starting(&mut self) {
        match self {
            PluginControl::Switch { control } => control.starts = control.on,
            PluginControl::Slider { control } => control.starts = control.value,
            PluginControl::Texture { control } => control.starts = control.value,
            PluginControl::File { .. } | PluginControl::Folder { .. } => {}
        }
    }

    /// It as it is, and starting so: a filter's panel opening at the values on its card.
    fn starting_as_it_is(mut self) -> PluginControl {
        match &mut self {
            PluginControl::Switch { control } => control.starts = control.on,
            PluginControl::Slider { control } => control.starts = control.value,
            PluginControl::Texture { control } => control.starts = control.value,
            PluginControl::File { .. } | PluginControl::Folder { .. } => {}
        }
        self
    }

    /// Its value as a plugin or a filter is told it.
    pub(crate) fn number(&self) -> f32 {
        match self {
            PluginControl::Switch { control } => f32::from(u8::from(control.on)),
            PluginControl::Slider { control } => control.value,
            PluginControl::File { control } => f32::from(u8::from(control.chosen.is_some())),
            PluginControl::Texture { control } => control.value as f32,
            PluginControl::Folder { control } => f32::from(u8::from(control.location.is_some())),
        }
    }

    /// Its value as kept, read back with [`Plugins::read_kept`].
    fn kept(&self) -> String {
        match self {
            PluginControl::Switch { control } => control.on.to_string(),
            PluginControl::Slider { control } => control.value.to_string(),
            PluginControl::File { control } => control.chosen.is_some().to_string(),
            PluginControl::Texture { control } => control.value.to_string(),
            PluginControl::Folder { control } => control.location.is_some().to_string(),
        }
    }

    /// `other`'s value taken over, if it is the same kind of control.
    fn take_value(&mut self, other: &PluginControl) {
        match (self, other) {
            (PluginControl::Switch { control }, PluginControl::Switch { control: from }) => control.on = from.on,
            (PluginControl::Slider { control }, PluginControl::Slider { control: from }) => {
                control.value = control.stepped(from.value);
            }
            (PluginControl::File { control }, PluginControl::File { control: from }) => {
                control.chosen.clone_from(&from.chosen)
            }
            (PluginControl::Texture { control }, PluginControl::Texture { control: from }) => {
                control.value = control.within(from.value as f32);
            }
            (PluginControl::Folder { control }, PluginControl::Folder { control: from }) => {
                control.chosen.clone_from(&from.chosen);
                control.location.clone_from(&from.location);
            }
            _ => {}
        }
    }
}

impl PluginSlider {
    /// `value` kept to the range, on a step. In doubles, so a step lands on the number it reads as
    /// (1 exactly, at 100%). A plugin's range comes as it comes: a bad one mustn't panic.
    pub(crate) fn stepped(&self, value: f32) -> f32 {
        let (least, most, step) = (f64::from(self.least), f64::from(self.most), f64::from(self.step));
        let kept = f64::from(value).max(least).min(most);
        if step > 0.0 { (least + ((kept - least) / step).round() * step).min(most) as f32 } else { kept as f32 }
    }
}

/// Where `slider` lands at `along` (0..1) of its way.
#[uniffi::export]
pub fn plugin_slider_at(slider: PluginSlider, along: f32) -> f32 {
    slider.stepped(slider.least + along.clamp(0.0, 1.0) * (slider.most - slider.least))
}

/// How far along its way (0..1) `value` is on `slider`.
#[uniffi::export]
pub fn plugin_slider_along(slider: PluginSlider, value: f32) -> f32 {
    let range = slider.most - slider.least;
    if range > 0.0 { ((value - slider.least) / range).clamp(0.0, 1.0) } else { 0.0 }
}

/// `slider`'s steps as choices, when every step has a name: shown as a menu of them, not as a
/// slider, since nothing lies between them. Otherwise none.
#[uniffi::export]
pub fn plugin_slider_choices(slider: PluginSlider) -> Vec<String> {
    let steps = if slider.step > 0.0 { ((slider.most - slider.least) / slider.step).round() } else { -1.0 };
    let wanted = steps + 1.0;
    if steps >= 1.0 && wanted <= slider.names.len() as f32 {
        slider.names.iter().take(wanted as usize).cloned().collect()
    } else {
        Vec::new()
    }
}

/// Which of `slider`'s choices `value` is.
#[uniffi::export]
pub fn plugin_slider_chosen(slider: PluginSlider, value: f32) -> u32 {
    if slider.step > 0.0 { ((slider.stepped(value) - slider.least) / slider.step).round().max(0.0) as u32 } else { 0 }
}

/// The value of `slider`'s choice `index`.
#[uniffi::export]
pub fn plugin_slider_choice(slider: PluginSlider, index: u32) -> f32 {
    slider.stepped(slider.least + index as f32 * slider.step)
}

/// `value` on `slider` as it reads, to as many places as its step needs.
#[uniffi::export]
pub fn plugin_slider_shown(slider: PluginSlider, value: f32) -> String {
    if let Some(text) = slider.at_least.as_ref().filter(|_| value <= slider.least) {
        return text.clone();
    }
    let place = ((value - slider.least) / slider.step).round();
    if let Some(name) = (place >= 0.0).then_some(place as usize).and_then(|i| slider.names.get(i)) {
        return name.clone();
    }
    let step = slider.step * slider.shown_times;
    let whole = |v: f32| (v - v.round()).abs() < 1e-3;
    let places = if whole(step) {
        0
    } else if whole(step * 10.0) {
        1
    } else {
        2
    };
    // a value that reads as nothing reads without a sign: a step from a range below 0 lands a
    // hair under 0 (-1 and fifty steps of .02 in f32's doubles), which would read "-0%"
    let shown = value * slider.shown_times;
    let at = 10f32.powi(places as i32);
    let shown = if (shown * at).round() == 0.0 { 0.0 } else { shown };
    format!("{:.*}{}", places, shown, slider.unit)
}

impl Plugins {
    /// The file the picture round the canvas is read from, as a plugin on the surround picture's
    /// hook says (interface 11): one of its file controls of pictures with one picked. None for
    /// none.
    pub(crate) fn surround_picture_file(&self) -> Option<std::path::PathBuf> {
        let (id, at) = self.hooks.surround_picture()?;
        self.picture_file(&id, at)
    }

    /// The file the picture picked for plugin `id`'s control `at` is kept in: one of its `file`
    /// controls of pictures, with one picked, while it is on. None for none.
    pub(crate) fn picture_file(&self, id: &str, at: u32) -> Option<std::path::PathBuf> {
        let store = self.store.as_deref()?;
        let list = self.listed();
        let plugin = list.files.iter().find(|p| p.id == id && p.on)?;
        match plugin.controls.get(at as usize)? {
            PluginControl::File { control }
                if control.accepts == PluginFileKind::Picture && control.chosen.is_some() =>
            {
                Some(plugin_file::chosen_paths(store, id, &control.key).0)
            }
            _ => None,
        }
    }

    /// What a plugin that is on gives `press` to now, by its manifest: the key of one of its
    /// switches (`Err`) or actions (`Ok`), each while its card shows it. None: no plugin's.
    fn pressed(&self, press: &KeyCombo) -> Option<Result<String, String>> {
        let press = KeyCombo::of(&press.key, press.command, press.shift, press.alt);
        let list = self.listed();
        list.files.iter().filter(|p| p.on && p.problem.is_none()).find_map(|plugin| {
            let shape = list.shapes.get(&plugin.id)?;
            let shows =
                |under: &[Option<usize>], at: usize| shape.shows(under.get(at).copied().flatten(), &plugin.controls);
            let control = shape.presses.iter().position(|p| p.as_ref() == Some(&press));
            if let Some(at) = control.filter(|&at| shows(&shape.under, at)) {
                return Some(Err(plugin.controls.get(at)?.key().to_owned()));
            }
            let action = shape.action_presses.iter().position(|p| p.as_ref() == Some(&press));
            let at = action.filter(|&at| shows(&shape.action_under, at))?;
            Some(Ok(plugin.actions.get(at)?.key.clone()))
        })
    }
}

/// How the side bar's slider tracks look as the plugins say (interface 11): each part none where
/// they leave it as the app has it.
#[derive(Clone, Copy, Debug, Default, PartialEq, uniffi::Record)]
pub struct SliderLook {
    /// How opaque the tracks are, 0 to 1 (the app's own: about .79 dark, .75 light).
    pub track_opacity: Option<f32>,
    /// Whether what is behind them is frosted as the bar's glass is, rather than softened.
    pub frosted: Option<bool>,
}

/// The most bytes of a picture a file control keeps (interface 11).
pub(crate) const MOST_PICTURE: usize = 32 << 20;

/// Whether `slider` holds whole numbers: kept as one where what it is kept as matters.
#[uniffi::export]
pub fn plugin_slider_whole(slider: PluginSlider) -> bool {
    slider.step > 0.0 && slider.step.fract() == 0.0 && slider.least.fract() == 0.0
}

/// The most of a plugin file worth reading, in bytes: one more is enough to refuse it.
#[uniffi::export]
pub fn plugin_file_most_bytes() -> u64 {
    plugin_file::MOST_FILE as u64
}

/// `manifest` as a list entry. One that only adds a filter starts on, as that changes nothing
/// already there; any other starts off, for someone to turn on.
/// The names of the files picked for `plugin`'s file controls, as kept in `store`.
fn read_chosen(store: &std::path::Path, plugin: &mut PluginInfo) {
    let id = plugin.id.clone();
    let text = |path: &std::path::Path| {
        std::fs::read_to_string(path).ok().map(|n| n.trim().to_owned()).filter(|n| !n.is_empty())
    };
    for control in &mut plugin.controls {
        if let PluginControl::File { control } = control {
            let (file, name) = plugin_file::chosen_paths(store, &id, &control.key);
            control.chosen = file.is_file().then(|| text(&name).unwrap_or_else(|| "Recording".to_owned()));
        }
        // a folder: where it is, and its name (interface 22)
        if let PluginControl::Folder { control } = control {
            let (at, name, _) = plugin_file::chosen_folder_paths(store, &id, &control.key);
            control.location = text(&at);
            control.chosen = control.location.as_ref().map(|_| text(&name).unwrap_or_else(|| "Folder".to_owned()));
        }
    }
}

fn from_file(manifest: Manifest) -> (PluginInfo, Shape) {
    let only_filter = manifest.holds == [PluginPart::Filter];
    // one filter's controls are its panel's alone; filters it lists have theirs on its card
    let on_panel = only_filter && manifest.shape.filters.is_empty();
    let info = PluginInfo {
        on_key: format!("{}.on", manifest.id),
        id: manifest.id,
        name: manifest.name,
        description: manifest.description,
        source: PluginSource::File,
        holds: manifest.holds,
        version: manifest.version,
        publisher: manifest.publisher,
        experimental: manifest.experimental,
        on: only_filter,
        problem: None,
        controls: if on_panel { Vec::new() } else { manifest.controls },
        brush_controls: manifest.brush_controls,
        actions: manifest.actions,
        kinds: manifest.kinds,
        layer_kinds: manifest.layer_kinds,
        permissions: manifest.permissions,
        features: Vec::new(),
        top: Vec::new(),
    };
    (info, manifest.shape)
}

/// The parts of the interface a plugin may show bigger or smaller than the rest
/// ([`Plugins::interface_part_scale`]), each by its number on the `interface_part_scale` hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum InterfacePart {
    /// Windows that float over the canvas: the colour panel dragged out, and any other.
    Floating = 0,
    /// Popovers and menus: what drops down from a button or a row.
    Menus = 1,
    /// The side bar.
    SideBar = 2,
    /// The bar across the top.
    TopBar = 3,
    /// The panels that hang from the top bar's buttons.
    Panels = 4,
}

/// The notices the interface shows, by what they say ([`Plugins::shows_notice`]); the first two by
/// their number on the `hide_notices` hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum NoticeKind {
    /// What undo or redo took back or brought back.
    Step = 0,
    /// Anything else done: copied, pasted, cleared, merged, applied, switched on or off.
    Done = 1,
    /// Something that went wrong, or couldn't be kept: it always shows.
    Problem = 2,
}

/// Where in the interface a plugin's menu is, from a button of the app's
/// ([`Plugins::menu_choices`]), by its name in a manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum PluginMenuPlace {
    /// The colour panel's Classic picker, from a button in its lower left corner.
    Classic,
}

impl PluginMenuPlace {
    /// The place a manifest calls `name`, if this app has it.
    pub(crate) fn named(name: &str) -> Option<PluginMenuPlace> {
        match name {
            "classic" => Some(PluginMenuPlace::Classic),
            _ => None,
        }
    }
}

/// One of the choices a plugin's menu offers: what picks it ([`Plugins::choose`]), its words, its
/// icon (SVG path data on a 24-unit grid, filled even-odd in the interface's ink), and whether it
/// stands now (every control it sets is as it sets it).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginChoice {
    pub key: String,
    pub label: String,
    pub icon: String,
    pub chosen: bool,
}

/// What a press of a plugin's own did (its manifest's `press`, see [`Plugins::plugin_press`]).
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum PluginPressed {
    /// A switch flipped: the plugins changed, as a switch set on its card changes them.
    Switched { control: PluginControl },
    /// An action to run as its row runs it ([`Plugins::run_action`]), by its key.
    Action { key: String },
}

/// `plugin` as its card shows it: what comes under a switch that is off left out.
fn shown(plugin: &PluginInfo, shape: Option<&Shape>) -> PluginInfo {
    let Some(shape) = shape else { return plugin.clone() };
    let shows = |under: &[Option<usize>], at: usize| shape.shows(under.get(at).copied().flatten(), &plugin.controls);
    let controls = plugin.controls.iter().enumerate().filter(|(at, _)| shows(&shape.under, *at));
    let actions = plugin.actions.iter().enumerate().filter(|(at, _)| shows(&shape.action_under, *at));
    let rows = plugin.brush_controls.iter().enumerate().filter(|(at, _)| shows(&shape.brush_under, *at));
    // each feature's switch, and what comes under it as the card shows it
    let features = shape.features.iter().enumerate().filter_map(|(f, feature)| {
        let Some(PluginControl::Switch { control }) = plugin.controls.get(feature.at) else { return None };
        let mine = |at: usize| at != feature.at && shape.feature_of(at) == Some(f) && shows(&shape.under, at);
        let controls = plugin.controls.iter().enumerate().filter(|(at, _)| mine(*at));
        let acts = |at: usize| shape.feature_under(shape.action_under.get(at).copied().flatten()) == Some(f);
        let actions = plugin.actions.iter().enumerate().filter(|(at, _)| acts(*at) && shows(&shape.action_under, *at));
        Some(PluginFeature {
            control: control.clone(),
            description: feature.description.clone(),
            controls: controls.map(|(_, c)| c.clone()).collect(),
            actions: actions.map(|(_, a)| a.clone()).collect(),
        })
    });
    // a plugin of features' own, under none of them (interface 22)
    let featured = !shape.features.is_empty();
    let top = plugin.controls.iter().enumerate().filter(|(at, _)| featured && shape.feature_of(*at).is_none());
    PluginInfo {
        controls: controls.map(|(_, c)| c.clone()).collect(),
        actions: actions.map(|(_, a)| a.clone()).collect(),
        brush_controls: rows.map(|(_, r)| r.clone()).collect(),
        features: features.collect(),
        top: top.map(|(_, c)| c.clone()).collect(),
        ..plugin.clone()
    }
}

/// The plugin of `files` taking the place of one as `find` says, and how, by their `shapes`.
fn replacing<'a>(
    files: &'a mut [PluginInfo],
    shapes: &HashMap<String, Shape>,
    find: impl Fn(&Replaced) -> bool,
) -> Option<(&'a mut PluginInfo, Replaced)> {
    files.iter_mut().find_map(|plugin| {
        let replaced = shapes.get(&plugin.id)?.replaces.iter().find(|r| find(r))?.clone();
        Some((plugin, replaced))
    })
}

/// The plugin of `files` taking a feature from one that stays as `find` says, and how, by their
/// `shapes` (interface 19).
fn taking<'a>(
    files: &'a mut [PluginInfo],
    shapes: &HashMap<String, Shape>,
    find: impl Fn(&Taken) -> bool,
) -> Option<(&'a mut PluginInfo, Taken)> {
    files.iter_mut().find_map(|plugin| {
        let taken = shapes.get(&plugin.id)?.takes.iter().find(|t| find(t))?.clone();
        Some((plugin, taken))
    })
}

/// Each plugin of `files` that took a feature from a plugin that stays, both of which were read
/// back on (`taken_on`), on (interface 19), with the switch above that which took it: unless its
/// own on or off was read back (`flagged`). Each turned on is written down as `carried`, as one
/// taking another's place is.
fn take_on(
    files: &mut [PluginInfo],
    shapes: &HashMap<String, Shape>,
    taken_on: &HashSet<String>,
    flagged: &HashSet<String>,
    carried: &mut HashSet<String>,
) {
    for plugin in files.iter_mut() {
        let Some(shape) = shapes.get(&plugin.id) else { continue };
        let both = |t: &Taken| taken_on.contains(&format!("{}.on", t.id)) && taken_on.contains(&t.feature);
        if shape.takes.iter().any(both) && !flagged.contains(&plugin.id) && plugin.problem.is_none() {
            plugin.on = true;
            carried.insert(plugin.id.clone());
        }
    }
}

/// What `from` had set, taken over by `to`, which takes its place as `replaced` says: on if it was,
/// the switch that stands for it with it, and the values of its controls.
fn carry(to: &mut PluginInfo, replaced: &Replaced, from: &PluginInfo, shape: Option<&Shape>) {
    let was_on = from.on && from.problem.is_none();
    if was_on {
        to.on = to.problem.is_none();
        if let Some(PluginControl::Switch { control }) = replaced.switch.and_then(|at| to.controls.get_mut(at)) {
            control.on = true;
        }
        // with the switches that comes under (a feature, interface 16)
        on_above(to, shape, replaced.switch, &HashSet::new(), &mut HashSet::new());
    }
    for (key, at) in &replaced.controls {
        if let (Some(was), Some(control)) = (from.controls.iter().find(|c| c.key() == key), to.controls.get_mut(*at)) {
            // (a file of its own picked already stays)
            if !matches!(control, PluginControl::File { control } if control.chosen.is_some()) {
                control.take_value(was);
            }
        }
    }
    // a setting the app used to have, kept under the same key by both
    for control in &mut to.controls {
        if let Some(was) = from.controls.iter().find(|c| c.key() == control.key()) {
            control.take_value(was);
        }
    }
}

/// The switches control `at` of `plugin` comes under (by its `shape`) turned on, but for those whose
/// own values were read back (`read`): what was on under them is on again. Each set is written
/// down in `switched`.
fn on_above(
    plugin: &mut PluginInfo,
    shape: Option<&Shape>,
    at: Option<usize>,
    read: &HashSet<String>,
    switched: &mut HashSet<String>,
) {
    let Some(shape) = shape else { return };
    let mut next = at.and_then(|at| shape.under.get(at).copied().flatten());
    // (as read, a few at most)
    for _ in 0..4 {
        let Some(up) = next else { break };
        if let Some(PluginControl::Switch { control }) = plugin.controls.get_mut(up)
            && !read.contains(&control.key)
        {
            control.on = true;
            switched.insert(control.key.clone());
        }
        next = shape.under.get(up).copied().flatten();
    }
}

/// The files picked for `from`'s file controls that `replaced` maps to file controls of `to`, each
/// written whole beside the one taking it, kept in `store`, where `to` has none picked of its own:
/// false if one couldn't be, so that nothing of `from` goes yet.
fn move_chosen(store: &std::path::Path, to: &PluginInfo, replaced: &Replaced, from: &PluginInfo) -> bool {
    for (key, at) in &replaced.controls {
        let was = from.controls.iter().find(|c| c.key() == key);
        let (Some(PluginControl::File { control: was }), Some(PluginControl::File { control: now })) =
            (was, to.controls.get(*at))
        else {
            continue;
        };
        if was.chosen.is_none() || now.chosen.is_some() {
            continue;
        }
        let (old_file, old_name) = plugin_file::chosen_paths(store, &from.id, &was.key);
        let (new_file, new_name) = plugin_file::chosen_paths(store, &to.id, &now.key);
        let copied = |old: &std::path::Path, new: &std::path::Path| {
            let bytes = std::fs::read(old).map_err(|e| crate::project::file_error("reading", e))?;
            let written = |tmp: &std::path::Path| {
                crate::storage::write(tmp, &bytes).map_err(|e| crate::project::file_error("saving", e))
            };
            crate::project::write_atomically(new, written)
        };
        let made = new_file.parent().is_some_and(|folder| std::fs::create_dir_all(folder).is_ok());
        // the name it was picked by may be missing: then it's called as any is
        if !made || copied(&old_file, &new_file).is_err() {
            log::warn!("a file picked for {key} couldn't be carried over");
            return false;
        }
        let _ = copied(&old_name, &new_name);
    }
    true
}

/// What the offer says `manifest` can do.
fn can_do(manifest: &Manifest) -> String {
    let filter = manifest.holds.contains(&PluginPart::Filter);
    let adds = filter.then_some(if manifest.shape.filters.len() > 1 {
        "This plugin adds filters to Adjustments, which change only the layer you use them on."
    } else {
        "This plugin adds a filter to Adjustments, which changes only the layer you use it on."
    });
    let brushes = manifest
        .holds
        .contains(&PluginPart::Brushes)
        .then_some("This plugin comes with a set of brushes, added to your brush library when you turn it on.");
    let sentences: Vec<&str> = adds
        .into_iter()
        .chain(brushes)
        .chain(manifest.hooks.iter().map(|hook| hook.sentence()))
        .chain(manifest.permissions.iter().map(|permission| permission.sentence()))
        .collect();
    let artwork = manifest.permissions.iter().any(|p| {
        matches!(
            p,
            crate::plugin_commands::PluginPermission::ArtworkRead
                | crate::plugin_commands::PluginPermission::ArtworkWrite
        )
    });
    let reach =
        if filter || artwork { "your files or the internet" } else { "your files, your layers or the internet" };
    format!("{} It can't reach {reach}.", sentences.join(" "))
}

/// A filter whose files have held up again, for opening: of plugin `plugin`.
pub(crate) struct FilterToOpen {
    pub plugin: String,
    pub name: String,
    pub controls: Vec<PluginControl>,
    pub code: procraft_gpu::FilterCode,
}

#[derive(Debug)]
struct Listed {
    files: Vec<PluginInfo>,
    /// Moved settings read back with no plugin yet to take them: key and value, kept as they are.
    moved: Vec<(String, String)>,
    /// The notice about them has been asked for: a start has one chance to say it.
    notice_asked: bool,
    /// The plugins whose on or off has been read back.
    flagged: HashSet<String>,
    /// What the app came with and added, as written down in the store.
    added: Vec<Added>,
    /// What each says beyond its card, by its id.
    shapes: HashMap<String, Shape>,
    /// The plugins on or off as those they took the place of were read back to be.
    carried: HashSet<String>,
    /// The keys of the controls whose own values were read back.
    read: HashSet<String>,
    /// The switches set as the on or off of a plugin they stand for was read back.
    flag_switched: HashSet<String>,
    /// Of the plugins features were taken from and those features' switches (interface 19), the
    /// keys read back on: `<id>.on` and `<id>.<feature>`.
    taken_on: HashSet<String>,
    /// The order someone put the plugins in on the Plugins page, by id, as written down in the
    /// store; those not in it come after, as they are listed.
    order: Vec<String>,
    /// That the order couldn't be kept has been said, since it last could be.
    order_unkept_said: bool,
}

impl Listed {
    /// The plugins in the order the Plugins page shows them: those put in an order first, in it,
    /// then the rest as they are listed. Only the page goes by it: the hooks go through what is on
    /// in id order, whatever it is.
    fn in_order(&self) -> Vec<&PluginInfo> {
        let place = |p: &PluginInfo| self.order.iter().position(|id| *id == p.id).unwrap_or(usize::MAX);
        let mut shown: Vec<&PluginInfo> = self.files.iter().collect();
        // (stable: those in no order keep theirs)
        shown.sort_by_key(|p| place(p));
        shown
    }

    /// Whether plugin `id` is one the app added that starts off until someone turns it on.
    fn added_off(&self, id: &str) -> bool {
        self.added.iter().any(|added| added.off && added.id == id)
    }

    /// The one holding the control kept under `key`, and the control's place in it.
    fn control_mut(&mut self, key: &str) -> Option<(&mut PluginInfo, usize)> {
        self.files.iter_mut().find_map(|plugin| {
            let at = plugin.controls.iter().position(|c| c.key() == key)?;
            Some((plugin, at))
        })
    }

    /// `plugin` as its card shows it.
    fn shown(&self, plugin: &PluginInfo) -> PluginInfo {
        shown(plugin, self.shapes.get(&plugin.id))
    }

    /// The id of the plugin here taking the place of `id`, and how.
    fn taking_place_of(&self, id: &str) -> Option<(String, Replaced)> {
        self.files.iter().find_map(|p| {
            let replaced = self.shapes.get(&p.id)?.replaces.iter().find(|r| r.id == id)?;
            Some((p.id.clone(), replaced.clone()))
        })
    }

    /// Each plugin that another here takes the place of goes, from the list and from `store`, what
    /// it had set taken over by the one taking its place. The ids of those gone.
    fn give_way(&mut self, store: &std::path::Path) -> Vec<String> {
        let pairs: Vec<(String, Replaced)> = self
            .files
            .iter()
            .filter_map(|p| Some((p.id.clone(), self.shapes.get(&p.id)?.replaces.clone())))
            .flat_map(|(to, replaces)| replaces.into_iter().map(move |r| (to.clone(), r)))
            .collect();
        let mut gone = Vec::new();
        for (to, replaced) in pairs {
            // (one already gone itself takes no place)
            let here = |id: &str| self.files.iter().position(|p| p.id == id);
            let (Some(into), Some(at)) = (here(&to), here(&replaced.id)) else { continue };
            // the files picked for it first, or it stays until they can be
            let moved = match (self.files.get(into), self.files.get(at)) {
                (Some(into), Some(from)) => move_chosen(store, into, &replaced, from),
                _ => false,
            };
            if !moved || !plugin_file::forget(store, &replaced.id) {
                continue;
            }
            let from = self.files.remove(at);
            self.shapes.remove(&from.id);
            if let Some(plugin) = self.files.iter_mut().find(|p| p.id == to) {
                carry(plugin, &replaced, &from, self.shapes.get(&to));
            }
            // on the Plugins page where it was, if it was put somewhere
            if let Some(place) = self.order.iter().position(|id| *id == from.id) {
                if self.order.contains(&to) {
                    self.order.remove(place);
                } else if let Some(id) = self.order.get_mut(place) {
                    id.clone_from(&to);
                }
                let _ = plugin_file::note_order(store, &self.order);
            }
            gone.push(from.id);
        }
        gone
    }
}

/// The lists, with each control's value as it stands. They live here and in no static, so a
/// plugin goes the moment it is removed. The app keeps the values with its other interface
/// settings, each under its control's key; the files live in `store`.
#[derive(Debug, uniffi::Object)]
pub struct Plugins {
    list: Mutex<Listed>,
    /// The folder imported plugins are kept in, `<id>/<version>/` each. None: none can be.
    store: Option<PathBuf>,
    /// The plugins that are on, running.
    hooks: Arc<Hooks>,
    /// A plugin's panel showing, with the copy of its code that showed it.
    panel: Mutex<Option<OpenPanel>>,
    /// The artwork open, the last to take the plugins up: the controls kept with an artwork are
    /// its, and setting one keeps it with it.
    artwork: Mutex<std::sync::Weak<crate::CanvasSession>>,
    /// The controls plugins' code set of their own since the app last took them (interface 12).
    controls_set: Mutex<Vec<PluginControl>>,
    /// The folders plugins name, and the work for them (interface 22).
    folders: Arc<crate::folders::Folders>,
}

/// A plugin's panel open: whose, what it shows, and the copy of its code its touches go to (under a
/// lock of its own: what the panel shows is read on every frame, while a touch may take seconds).
struct OpenPanel {
    id: String,
    shows: crate::PluginPanel,
    running: Arc<Mutex<Running>>,
    /// Where its action came from, window pixels: where a ring shows.
    at: Option<crate::PanelPoint>,
}

impl std::fmt::Debug for OpenPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenPanel").field("id", &self.id).field("shows", &self.shows).finish_non_exhaustive()
    }
}

impl Plugins {
    /// The lists, with whatever stopped on a hook since they were last read shown as stopped.
    fn listed(&self) -> MutexGuard<'_, Listed> {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        for (id, stop) in self.hooks.take_stopped() {
            if let Some(plugin) = list.files.iter_mut().find(|p| p.id == id) {
                plugin.on = false;
                plugin.problem = Some(stop.text().to_owned());
            }
        }
        list
    }

    fn kept_in(store: Option<PathBuf>) -> Arc<Plugins> {
        let manifests = store.as_deref().map(plugin_file::read_all).unwrap_or_default();
        let (mut files, mut shapes) = (Vec::new(), HashMap::new());
        for manifest in manifests {
            let (plugin, shape) = from_file(manifest);
            shapes.insert(plugin.id.clone(), shape);
            files.push(plugin);
        }
        let added = store.as_deref().map(plugin_file::read_added).unwrap_or_default();
        let order = store.as_deref().map(plugin_file::read_order).unwrap_or_default();
        if let Some(store) = &store {
            plugin_file::settle_trying(store, STOPPED_DRIVER);
            for plugin in &mut files {
                plugin.problem = plugin_file::stopped_why(store, &plugin.id);
                plugin.on &= plugin.problem.is_none();
                read_chosen(store, plugin);
            }
        }
        // what the app added starts off until someone turns it on, whatever it holds
        for plugin in &mut files {
            plugin.on &= !added.iter().any(|added| added.off && added.id == plugin.id);
        }
        let mut list = Listed {
            files,
            moved: Vec::new(),
            notice_asked: false,
            flagged: HashSet::new(),
            added,
            shapes,
            carried: HashSet::new(),
            read: HashSet::new(),
            flag_switched: HashSet::new(),
            taken_on: HashSet::new(),
            order,
            order_unkept_said: false,
        };
        // what one here takes the place of goes, once, if it is still here
        if let Some(store) = &store {
            list.give_way(store);
        }
        let hooks = Arc::new(Hooks::kept_in(store.clone()));
        Arc::new_cyclic(|me| Plugins {
            list: Mutex::new(list),
            folders: crate::folders::Folders::new(me.clone(), Arc::clone(&hooks)),
            hooks,
            store,
            panel: Mutex::new(None),
            artwork: Mutex::new(std::sync::Weak::new()),
            controls_set: Mutex::new(Vec::new()),
        })
    }

    /// The folders plugins name (interface 22).
    pub(crate) fn folders(&self) -> Arc<crate::folders::Folders> {
        Arc::clone(&self.folders)
    }

    /// The folder picked for plugin `id`'s control `at`: one of its `folder` controls, with one
    /// picked, while it is on (interface 22). None for none.
    pub(crate) fn folder_target(&self, id: &str, at: u32) -> Option<crate::folders::Target> {
        let list = self.listed();
        let plugin = list.files.iter().find(|p| p.id == id && p.on && p.problem.is_none())?;
        let PluginControl::Folder { control } = plugin.controls.get(at as usize)? else { return None };
        let location = control.location.clone()?;
        let records = self.store.as_deref().map(|store| plugin_file::chosen_folder_paths(store, id, &control.key).2);
        Some(crate::folders::Target { key: control.key.clone(), location, records })
    }

    /// Adds each of `files`, which the app comes with, the first time it comes with it: as if
    /// imported, but off, and from then on like any other plugin. One the store holds older takes
    /// the newer file, as an import of it would, keeping what was set. It is written down that each
    /// was added, before it is, so one removed since stays removed.
    pub(crate) fn add_bundled(&self, files: &[(&str, &[u8])]) {
        let Some(store) = &self.store else { return };
        // each to take in, and whether it is new
        let mut taking = Vec::new();
        {
            let mut list = self.listed();
            let mut added = list.added.clone();
            for (name, bytes) in files {
                let manifest = match plugin_file::read_package(bytes) {
                    Ok(package) => package.manifest,
                    Err(refusal) => {
                        log::warn!("{name}, which comes with the app, doesn't hold up: {refusal:?}");
                        continue;
                    }
                };
                let kept = list.files.iter().find(|p| p.id == manifest.id).map(|p| p.version.as_str());
                let noted = added.iter().any(|added| added.id == manifest.id);
                if !noted {
                    // one imported before the app came with it goes on as it was
                    added.push(Added { id: manifest.id.clone(), off: kept.is_none() });
                }
                let take = match kept {
                    // removed since it was added
                    None => !noted,
                    // taken again only if this one is newer
                    Some(kept) => plugin_file::version_is_older(kept, &manifest.version),
                };
                if take {
                    taking.push((*name, *bytes, manifest.id, kept.is_none()));
                }
            }
            if added != list.added {
                if !plugin_file::note_added(store, &added) {
                    return;
                }
                list.added = added;
            }
        }
        let mut failed = Vec::new();
        for (name, bytes, id, new) in taking {
            if let Err(refusal) = self.take_in(bytes) {
                log::warn!("{name}, which comes with the app, couldn't be added: {refusal:?}");
                if new {
                    failed.push(id);
                }
            }
        }
        // to be tried again on the next start
        if !failed.is_empty() {
            let mut list = self.listed();
            let mut added = list.added.clone();
            added.retain(|added| !failed.contains(&added.id));
            if plugin_file::note_added(store, &added) {
                list.added = added;
            }
        }
    }

    /// `id` off its hooks, and its panel closed.
    fn take_off(&self, id: &str) {
        self.hooks.take_off(id);
        let mut panel = self.open_panel();
        if panel.as_ref().is_some_and(|open| open.id == id) {
            *panel = None;
        }
    }

    fn open_panel(&self) -> MutexGuard<'_, Option<OpenPanel>> {
        self.panel.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Puts `plugin` on its hooks, if it has code, is on and isn't there yet. One that can't start
    /// goes off, and says why.
    fn start(&self, plugin: &mut PluginInfo, shape: Option<&Shape>) {
        if !plugin.holds(PluginPart::Hooks) || !plugin.on {
            return;
        }
        // a plugin of features: each that is on, in a copy of its own
        if let Some(shape) = shape.filter(|shape| !shape.features.is_empty()) {
            for f in 0..shape.features.len() {
                self.start_feature(plugin, shape, f);
            }
            return;
        }
        if self.hooks.is_on(&plugin.id) {
            return;
        }
        let values: Vec<f32> = plugin.controls.iter().map(PluginControl::number).collect();
        let store = self.store.as_deref();
        let marks = shape.map_or(plugin.id.as_str(), |shape| shape.marks_id(&plugin.id)).to_owned();
        match self.start_code(&plugin.id, &plugin.version, &values, &plugin.permissions, None, &marks) {
            Ok((running, hooks)) => {
                let copy =
                    crate::hooks::OnCopy { id: plugin.id.clone(), feature: None, features: Arc::from([]), marks };
                self.hooks.put_on_copy(copy, running, &hooks);
                // a copy of its own for its events
                if hooks.iter().any(|h| matches!(h, Hook::StrokeKept | Hook::LayerAdded))
                    && let Ok((events, _)) =
                        self.start_code(&plugin.id, &plugin.version, &values, &plugin.permissions, None, &plugin.id)
                {
                    self.hooks.give_events(&plugin.id, None, events);
                }
                self.hooks.brush_keys(&plugin.id, plugin.brush_controls.iter().map(|c| c.key.clone()).collect());
                // the kinds of layer it adds, for the sessions' layers of them to go through it
                self.hooks.layer_kinds(&plugin.id, plugin.layer_kinds.clone());
                // its brush engines, for brush files and the brushes made of them, and how a copy of
                // its code of its own starts for their imports
                if hooks.contains(&Hook::Brushes)
                    && let Some(store) = store
                {
                    let engines = plugin_file::read_brush_engines(store, &plugin.id, &plugin.version);
                    self.hooks.brush_engines(&plugin.id, engines);
                    if let Some((_, bytes)) = plugin_file::read_code(store, &plugin.id, &plugin.version) {
                        let kept = plugin_file::read_kept(store, &plugin.id);
                        let (id, values, permissions, on) =
                            (plugin.id.clone(), values.clone(), plugin.permissions.clone(), hooks.clone());
                        let copier: crate::hooks::Copier = Arc::new(move |memory| {
                            let mut running = Running::start_told_within(
                                &bytes,
                                &values,
                                &on,
                                &permissions,
                                kept.as_deref(),
                                memory,
                            )?;
                            running.own(&id);
                            Ok(running)
                        });
                        self.hooks.give_copier(&plugin.id, copier);
                    }
                }
                // a picker's picture, for the sessions to draw
                if hooks.contains(&Hook::Picker)
                    && let Some((text, keys)) =
                        store.and_then(|store| plugin_file::read_picture_code(store, &plugin.id, &plugin.version))
                {
                    self.hooks.give_picture(&plugin.id, text, keys, values.clone());
                }
                // the files picked for it, handed over
                for (at, control) in plugin.controls.iter().enumerate() {
                    if let (Some(store), PluginControl::File { control }) = (store, control)
                        && control.chosen.is_some()
                        && let Ok(bytes) = std::fs::read(plugin_file::chosen_paths(store, &plugin.id, &control.key).0)
                    {
                        self.hooks.give_file(&plugin.id, None, at, Some(&bytes));
                    }
                }
            }
            Err(stop) => {
                self.hooks.failed(&plugin.id, stop);
                plugin.on = false;
                plugin.problem = Some(stop.text().to_owned());
            }
        }
    }

    /// Puts the copy of `plugin`'s code for its feature `f` (by `shape`, interface 16) on that
    /// feature's hooks, if the plugin and the feature are on, the feature answers any and its copy
    /// isn't there yet: told every control as it is but the other features' switches, as off, with
    /// the memory its hooks give, its files handed to it. One that can't start takes the whole
    /// plugin off, and says why.
    fn start_feature(&self, plugin: &mut PluginInfo, shape: &Shape, f: usize) {
        let Some(feature) = shape.features.get(f) else { return };
        let on = matches!(plugin.controls.get(feature.at), Some(PluginControl::Switch { control }) if control.on);
        let ready = plugin.on && plugin.problem.is_none() && !feature.hooks.is_empty();
        if !on || !ready || self.hooks.feature_on(&plugin.id, feature.at) {
            return;
        }
        let features: Arc<[usize]> = shape.features.iter().map(|f| f.at).collect();
        let others = |at: usize| at != feature.at && features.contains(&at);
        let values: Vec<f32> =
            plugin.controls.iter().enumerate().map(|(at, c)| if others(at) { 0.0 } else { c.number() }).collect();
        let marks = shape.marks_id(&plugin.id).to_owned();
        let (id, version, permissions) = (plugin.id.clone(), plugin.version.clone(), plugin.permissions.clone());
        let start = || self.start_code(&id, &version, &values, &permissions, Some(&feature.hooks), &marks);
        match start() {
            Ok((running, hooks)) => {
                let features = Arc::clone(&features);
                let copy =
                    crate::hooks::OnCopy { id: id.clone(), feature: Some(feature.at), features, marks: marks.clone() };
                self.hooks.put_on_copy(copy, running, &hooks);
                if hooks.iter().any(|h| matches!(h, Hook::StrokeKept | Hook::LayerAdded))
                    && let Ok((events, _)) = start()
                {
                    self.hooks.give_events(&id, Some(feature.at), events);
                }
                self.hooks.brush_keys(&id, plugin.brush_controls.iter().map(|c| c.key.clone()).collect());
                let store = self.store.as_deref();
                if hooks.contains(&Hook::Picker)
                    && let Some((text, keys)) =
                        store.and_then(|store| plugin_file::read_picture_code(store, &id, &version))
                {
                    self.hooks.give_picture(&id, text, keys, values.clone());
                }
                // the files picked for what comes under it, handed to it alone
                for (at, control) in
                    plugin.controls.iter().enumerate().filter(|(at, _)| shape.feature_of(*at) == Some(f))
                {
                    if let (Some(store), PluginControl::File { control }) = (store, control)
                        && control.chosen.is_some()
                        && let Ok(bytes) = std::fs::read(plugin_file::chosen_paths(store, &id, &control.key).0)
                    {
                        self.hooks.give_file(&id, Some(feature.at), at, Some(&bytes));
                    }
                }
            }
            Err(stop) => {
                self.hooks.failed(&id, stop);
                self.hooks.take_off(&id);
                plugin.on = false;
                plugin.problem = Some(stop.text().to_owned());
            }
        }
    }

    /// A feature's switch, control `at` of `plugin`, set (by `shape`, interface 16): its copy of the
    /// code comes on its hooks, or goes.
    fn feature_switched(&self, plugin: &mut PluginInfo, shape: Option<&Shape>, at: usize) {
        let Some(shape) = shape else { return };
        let Some(f) = shape.features.iter().position(|f| f.at == at) else { return };
        if matches!(plugin.controls.get(at), Some(PluginControl::Switch { control }) if control.on) {
            self.start_feature(plugin, shape, f);
        } else {
            self.hooks.take_off_feature(&plugin.id, at);
        }
    }

    /// The code of plugin `id` at `version` started, told `values`, what it keeps and (on the key
    /// hook) the app's shortcuts, reaching what `permissions` give; with its hooks.
    ///
    /// A feature's copy (interface 16) is on `only` those hooks, with the memory they give; and its
    /// layers' marks go by `marks`.
    fn start_code(
        &self,
        id: &str,
        version: &str,
        values: &[f32],
        permissions: &[crate::plugin_commands::PluginPermission],
        only: Option<&[Hook]>,
        marks: &str,
    ) -> Result<(Running, Vec<Hook>), Stop> {
        let store = self.store.as_deref().ok_or(Stop::Damaged)?;
        let (hooks, bytes) = plugin_file::read_code(store, id, version).ok_or(Stop::Damaged)?;
        let hooks = only.map_or(hooks, <[Hook]>::to_vec);
        let kept = plugin_file::read_kept(store, id);
        let mut running = Running::start_told(&bytes, values, &hooks, permissions, kept.as_deref())?;
        running.own(id);
        running.own_marks(marks);
        Ok((running, hooks))
    }

    /// What `running`, a copy of `id`'s code that has just acted, says to keep: kept with the
    /// plugin, and handed to its other copies.
    fn keep_from(&self, id: &str, running: &mut Running) -> Result<(), Stop> {
        if let Some(data) = running.take_keep()? {
            if let Some(store) = &self.store {
                plugin_file::write_kept(store, id, &data);
            }
            self.hooks.give_kept(id, &data);
        }
        self.set_from(id, running)
    }

    /// The controls of its own `id`'s code (`running`) set while its action ran or its panel was
    /// touched (interface 12): each a switch or a slider set as on its card (one kept with each
    /// artwork, for the artwork open), its hooks told, and `running` told it as set. A place that is
    /// no switch or slider of its is left as it is.
    fn set_from(&self, id: &str, running: &mut Running) -> Result<(), Stop> {
        for (at, value) in running.take_set_controls() {
            let found = {
                let list = self.listed();
                let plugin = list.files.iter().find(|p| p.id == id && p.on);
                plugin.and_then(|p| p.controls.get(at)).map(|c| (c.key().to_owned(), c.clone()))
            };
            let set = match found {
                Some((key, PluginControl::Switch { .. })) => {
                    let on = value >= 0.5;
                    self.set_switch(key, on).then_some(if on { 1.0 } else { 0.0 })
                }
                Some((key, PluginControl::Slider { .. })) => self.set_slider(key, value),
                _ => None,
            };
            if let Some(set) = set {
                running.control(at, set)?;
                let now = self.listed().files.iter().find(|p| p.id == id).and_then(|p| p.controls.get(at).cloned());
                let mut kept = self.controls_set.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(now) = now {
                    kept.retain(|c| c.key() != now.key());
                    kept.push(now);
                }
            }
        }
        Ok(())
    }

    /// What a copy of `id`'s code showing the panel (`running`) answered a touch or a key press
    /// with: the panel shown changed, or closed, unless another took its place meanwhile. False once
    /// it is closed.
    fn panel_answered(
        &self,
        id: &str,
        running: &Arc<Mutex<Running>>,
        answer: Result<(Option<crate::PluginPanel>, bool), Stop>,
    ) -> bool {
        let mut open = self.open_panel();
        let same = open.as_ref().is_some_and(|panel| Arc::ptr_eq(&panel.running, running));
        match answer {
            Ok((shows, false)) => {
                if let (true, Some(shows), Some(panel)) = (same, shows, open.as_mut()) {
                    panel.shows = shows;
                }
                same
            }
            Ok((_, true)) => {
                if same {
                    *open = None;
                }
                false
            }
            Err(stop) => {
                if same {
                    *open = None;
                }
                drop(open);
                self.hooks.stopped(id, stop);
                false
            }
        }
    }

    /// The hooks, every plugin that's on on them: a list read back from where it was kept starts its
    /// plugins here, once all its values are in and something will go through them.
    pub(crate) fn hooks(&self) -> Arc<Hooks> {
        let mut list = self.listed();
        let Listed { files, shapes, .. } = &mut *list;
        for plugin in files.iter_mut() {
            let shape = shapes.get(&plugin.id);
            self.start(plugin, shape);
        }
        Arc::clone(&self.hooks)
    }

    /// What `change` makes of the control kept under `key`, if there is one. A running plugin is
    /// told the new value.
    fn change<T>(&self, key: &str, change: impl FnOnce(&mut PluginControl) -> Option<T>) -> Option<T> {
        let mut list = self.listed();
        let Listed { files, shapes, .. } = &mut *list;
        let (plugin, at) = files.iter_mut().find_map(|plugin| {
            let at = plugin.controls.iter().position(|c| c.key() == key)?;
            Some((plugin, at))
        })?;
        let changed = plugin.controls.get_mut(at).and_then(change)?;
        let mut kept = None;
        if let Some(control) = plugin.controls.get(at) {
            self.hooks.control(&plugin.id, at, control.number());
            if shapes.get(&plugin.id).is_some_and(|shape| shape.artwork.get(at) == Some(&true)) {
                kept = Some((key.to_owned(), control.number()));
            }
        }
        let shape = shapes.get(&plugin.id);
        self.feature_switched(plugin, shape, at);
        drop(list);
        if let Some((key, value)) = kept {
            self.keep_with_artwork(key, value);
        }
        Some(changed)
    }

    /// `value` kept with the artwork open under `key`, a control's kept with each artwork.
    fn keep_with_artwork(&self, key: String, value: f32) {
        let artwork = self.artwork.lock().unwrap_or_else(|e| e.into_inner()).upgrade();
        if let Some(session) = artwork {
            session.keep_plugin_value(key, value);
        }
    }

    /// `session` is the artwork open: the controls kept with an artwork take the values it keeps
    /// (those it keeps none for, their `starts`), the plugins told, the first time it comes here.
    /// Each plugin's are set whether it is on or not, so it starts with them.
    pub(crate) fn take_up_artwork(&self, session: &Arc<crate::CanvasSession>) {
        {
            let mut artwork = self.artwork.lock().unwrap_or_else(|e| e.into_inner());
            if artwork.upgrade().is_some_and(|open| Arc::ptr_eq(&open, session)) {
                return;
            }
            *artwork = Arc::downgrade(session);
        }
        let kept = session.plugin_values();
        let mut list = self.listed();
        let Listed { files, shapes, .. } = &mut *list;
        for plugin in files.iter_mut() {
            let Some(shape) = shapes.get(&plugin.id) else { continue };
            for (at, control) in plugin.controls.iter_mut().enumerate() {
                if shape.artwork.get(at) != Some(&true) {
                    continue;
                }
                // (or under the key it was kept by before it was this one's, interface 19)
                let before = || shape.kept_before(at).find_map(|key| kept.get(key).copied());
                let value = kept.get(control.key()).copied().or_else(before);
                let changed = match control {
                    PluginControl::Switch { control } => {
                        let on = value.map_or(control.starts, |v| v >= 0.5);
                        std::mem::replace(&mut control.on, on) != on
                    }
                    PluginControl::Slider { control } => {
                        let to = control.stepped(value.unwrap_or(control.starts));
                        std::mem::replace(&mut control.value, to) != to
                    }
                    PluginControl::Texture { control } => {
                        let to = value.map_or(control.starts, |v| control.within(v));
                        std::mem::replace(&mut control.value, to) != to
                    }
                    PluginControl::File { .. } | PluginControl::Folder { .. } => false,
                };
                if changed {
                    self.hooks.control(&plugin.id, at, control.number());
                }
            }
        }
    }

    /// The control kept under `key`, as it is now.
    fn control(&self, key: &str) -> Option<PluginControl> {
        let mut list = self.listed();
        let (plugin, at) = list.control_mut(key)?;
        plugin.controls.get(at).cloned()
    }

    /// Each value as a line of a settings file, its key and then what [`Plugins::read_kept`] reads.
    pub fn kept_lines(&self) -> Vec<String> {
        let list = self.listed();
        // (a file picked is kept with its plugin)
        let values = list
            .files
            .iter()
            .flat_map(|p| &p.controls)
            .filter(|c| !matches!(c, PluginControl::File { .. } | PluginControl::Folder { .. }));
        let values = values.map(|c| format!("{} {}", c.key(), c.kept()));
        let flags = list.files.iter().map(|p| format!("{} {}", p.on_key, p.on));
        values.chain(flags).chain(list.moved.iter().map(|(key, value)| format!("{key} {value}"))).collect()
    }

    /// The set of brushes each plugin on holds (interface 14), while the switch it comes under is
    /// on (interface 19), by the id it is given by: its own, or that of the plugin it takes the
    /// place of whose set it is. Its set file, as kept.
    pub(crate) fn brush_sets(&self) -> Vec<(String, std::sync::Arc<[u8]>)> {
        let Some(store) = &self.store else { return Vec::new() };
        let holders: Vec<(String, String, String)> = {
            let list = self.listed();
            let holding = list.files.iter().filter(|p| p.on && p.problem.is_none() && p.holds(PluginPart::Brushes));
            let shown = |p: &PluginInfo| {
                let shape = list.shapes.get(&p.id);
                shape.is_none_or(|shape| shape.shows(shape.brushes_under, &p.controls))
            };
            let given_as = |p: &PluginInfo| -> String {
                list.shapes.get(&p.id).map_or_else(|| p.id.clone(), |s| s.brushes_id(&p.id).to_owned())
            };
            holding.filter(|p| shown(p)).map(|p| (p.id.clone(), p.version.clone(), given_as(p))).collect()
        };
        let sets = holders.into_iter().map(|(id, version, given_as)| {
            let set = plugin_file::read_brush_set(store, &id, &version);
            set.map(|set| (given_as, set.into()))
        });
        sets.flatten().collect()
    }

    /// Takes `bytes` in, as `install` says.
    fn take_in(&self, bytes: &[u8]) -> Result<(), Refusal> {
        let package = plugin_file::read_package(bytes)?;
        self.can_take(&package.manifest)?;
        let store = self.store.as_ref().ok_or(Refusal::NotKept)?;
        let folder = plugin_file::keep(store, &package)?;
        // as the next start will find it, or not at all
        let (id, version) = (&package.manifest.id, &package.manifest.version);
        let manifest = plugin_file::read_folder(&folder, id, version).ok_or(Refusal::NotKept)?;
        let (mut fresh, shape) = from_file(manifest);
        let id = fresh.id.clone();
        read_chosen(store, &mut fresh);
        let mut list = self.listed();
        fresh.on &= !list.added_off(&fresh.id);
        // a moved setting read back goes to the plugin that takes it over
        for control in &mut fresh.controls {
            if let Some(at) = list.moved.iter().position(|(key, _)| key == control.key()) {
                let (_, value) = list.moved.remove(at);
                control.read(&value);
            }
        }
        match list.files.iter_mut().find(|p| p.id == fresh.id) {
            Some(old) => {
                // an update keeps what was set, for what it still has
                for control in &mut fresh.controls {
                    if let Some(was) = old.controls.iter().find(|c| c.key() == control.key()) {
                        control.take_value(was);
                    }
                }
                fresh.on = old.on;
                // what stopped gets its chance with the new file
                self.take_off(&fresh.id);
                plugin_file::clear_stopped(store, &fresh.id);
                self.start(&mut fresh, Some(&shape));
                *old = fresh;
            }
            None => list.files.push(fresh),
        }
        list.shapes.insert(id.clone(), shape);
        // what it takes the place of goes, what that had set taken over (on, if that was, from
        // when the hooks are next asked for)
        for gone in list.give_way(store) {
            self.take_off(&gone);
        }
        Ok(())
    }

    /// The filter `id` as it can be opened; what to tell if it can't. `id` is its plugin's, or for
    /// one of several a plugin lists, its own (`<plugin>.<key>`): that one opens at the values its
    /// card has.
    pub(crate) fn filter_to_open(&self, id: &str) -> Result<FilterToOpen, String> {
        let list = self.listed();
        let found = list.files.iter().filter(|p| p.holds(PluginPart::Filter)).find_map(|p| {
            let listed = list.shapes.get(&p.id).map(|shape| shape.filters.as_slice()).unwrap_or_default();
            if listed.is_empty() {
                (p.id == id).then_some((p, None))
            } else {
                listed.iter().find(|f| f.id == id && f.layer.is_none()).map(|f| (p, Some(f)))
            }
        });
        let (plugin, listed) = found.ok_or_else(|| FILTER_DAMAGED.to_owned())?;
        if let Some(problem) = &plugin.problem {
            return Err(problem.clone());
        }
        let store = self.store.as_deref().ok_or_else(|| FILTER_DAMAGED.to_owned())?;
        let path = listed.map_or(plugin_file::FILTER_CODE, |f| f.code.as_str());
        let read = plugin_file::read_filter_code(store, &plugin.id, &plugin.version, path);
        let (manifest, text) = read.ok_or_else(|| FILTER_DAMAGED.to_owned())?;
        let (name, controls) = match listed {
            None => (manifest.name, manifest.controls),
            Some(f) => {
                let controls = f.controls.iter().filter_map(|&at| plugin.controls.get(at).cloned());
                (f.label.clone(), controls.map(PluginControl::starting_as_it_is).collect())
            }
        };
        let code = procraft_gpu::read_filter(&text, &plugin_file::value_names(&controls));
        let code = code.map_err(|why| {
            log::warn!("a kept filter's code was refused: {why}");
            FILTER_DAMAGED.to_owned()
        })?;
        Ok(FilterToOpen { plugin: plugin.id.clone(), name, controls, code })
    }

    /// What each filter layer the plugins on offer is made of, by its id, its code read from the
    /// plugin's folder (once for each plugin), which must hold up as it did.
    pub(crate) fn layer_filter_defs(&self) -> std::collections::HashMap<String, crate::filter_layers::LayerFilterDef> {
        let list = self.listed();
        let mut out = std::collections::HashMap::new();
        let Some(store) = self.store.as_deref() else { return out };
        for plugin in list.files.iter().filter(|p| p.on && p.problem.is_none()) {
            let Some(shape) = list.shapes.get(&plugin.id) else { continue };
            let offered: Vec<&plugin_file::ListedFilter> =
                shape.filters.iter().filter(|f| f.layer.is_some() && shape.shows(f.under, &plugin.controls)).collect();
            if offered.is_empty() {
                continue;
            }
            let Some(codes) = plugin_file::read_layer_filter_codes(store, &plugin.id, &plugin.version) else {
                log::warn!("a plugin's filter layers didn't hold up: {}", plugin.id);
                continue;
            };
            for f in offered {
                let (Some(layer), Some((_, text))) = (&f.layer, codes.iter().find(|(path, _)| *path == f.code)) else {
                    continue;
                };
                let def = crate::filter_layers::LayerFilterDef {
                    plugin: plugin.id.clone(),
                    label: f.label.clone(),
                    values: layer.values.clone(),
                    reach: layer.reach,
                    touch: layer.touch,
                    editor: layer.editor,
                    text: Arc::from(text.as_str()),
                    store: Some(store.to_path_buf()),
                };
                // the layers an artwork kept of it by an id it had before go on showing (interface 19)
                for was in shape.filter_was(f, &plugin.id) {
                    out.entry(was).or_insert_with(|| def.clone());
                }
                out.insert(f.id.clone(), def);
            }
        }
        out
    }

    /// Filter `id` is on trial: if the app ends before it has drawn, it is put down to it.
    pub(crate) fn filter_trying(&self, id: &str) {
        if let Some(store) = &self.store {
            plugin_file::note_trying(store, id);
        }
    }

    /// No filter is on trial any more.
    pub(crate) fn filter_tried(&self) {
        if let Some(store) = &self.store {
            plugin_file::clear_trying(store);
        }
    }

    /// Switches off the plugin of filter `id`, all of it, `why` on its card and through a restart,
    /// until someone turns it on.
    pub(crate) fn stop_filter(&self, id: &str, why: &str) {
        if let Some(store) = &self.store {
            plugin_file::note_stopped(store, id, why);
        }
        self.take_off(id);
        if let Some(filter) = self.listed().files.iter_mut().find(|p| p.id == id) {
            filter.on = false;
            filter.problem = Some(why.to_owned());
        }
    }

    /// Whether what is here lets a plugin of `manifest` in: an older file doesn't replace a newer,
    /// a moved setting has one holder (besides one it takes the place of), and one here taking its
    /// place keeps it.
    fn can_take(&self, manifest: &Manifest) -> Result<(), Refusal> {
        let list = self.listed();
        if list.taking_place_of(&manifest.id).is_some() {
            return Err(Refusal::Replaced);
        }
        let replaced = |id: &str| manifest.shape.replaces.iter().any(|r| r.id == id);
        let others = || list.files.iter().filter(|p| p.id != manifest.id && !replaced(&p.id)).flat_map(|p| &p.controls);
        if manifest.controls.iter().any(|control| others().any(|other| other.key() == control.key())) {
            return Err(Refusal::Clash);
        }
        let kept = list.files.iter().find(|p| p.id == manifest.id);
        if kept.is_some_and(|k| plugin_file::version_is_older(&manifest.version, &k.version)) {
            return Err(Refusal::NewerInstalled);
        }
        Ok(())
    }
}

#[uniffi::export]
impl Plugins {
    /// Nothing, and nowhere to keep a file.
    #[uniffi::constructor]
    pub fn new() -> Arc<Plugins> {
        Plugins::kept_in(None)
    }

    /// Every file kept in the folder `dir` that holds up.
    #[uniffi::constructor]
    pub fn with_store(dir: String) -> Arc<Plugins> {
        Plugins::kept_in(Some(PathBuf::from(dir)))
    }

    /// The app's plugins: every file kept in the folder `dir` that holds up, with the files the app
    /// comes with added there first if it never added them before.
    #[uniffi::constructor]
    pub fn with_bundled(dir: String) -> Arc<Plugins> {
        let plugins = Plugins::kept_in(Some(PathBuf::from(dir)));
        plugins.add_bundled(crate::bundled::BUNDLED);
        plugins
    }

    /// What the Plugins page shows: every plugin imported, each as its card shows it (what comes
    /// under a switch that is off left out).
    pub fn plugins(&self) -> Vec<PluginInfo> {
        let list = self.listed();
        list.in_order().into_iter().map(|p| list.shown(p)).collect()
    }

    /// Moves plugin `id` to place `to` on the Plugins page (past the end: the last), the order
    /// written down whole in the store for the next start. Only the page goes by it. What to tell
    /// if it couldn't be kept, said once until it can be (the page shows the order meanwhile);
    /// nothing for an id that isn't here.
    pub fn move_plugin(&self, id: String, to: u32) -> Option<String> {
        let mut list = self.listed();
        let shown: Vec<String> = list.in_order().into_iter().map(|p| p.id.clone()).collect();
        let mut order = shown.clone();
        let from = order.iter().position(|o| *o == id)?;
        let moved = order.remove(from);
        order.insert((to as usize).min(order.len()), moved);
        if order == shown {
            return None;
        }
        let kept = self.store.as_deref().is_none_or(|store| plugin_file::note_order(store, &order));
        list.order = order;
        if kept {
            list.order_unkept_said = false;
            return None;
        }
        (!std::mem::replace(&mut list.order_unkept_said, true)).then(|| ORDER_UNKEPT.to_owned())
    }

    /// The actions listed in `place` now, of the plugins that are on.
    pub fn actions_in(&self, place: PluginPlace) -> Vec<PluginAction> {
        let list = self.listed();
        let on = list.files.iter().filter(|p| p.on && p.problem.is_none()).map(|p| list.shown(p));
        on.flat_map(|p| p.actions.into_iter().filter(|a| a.place == place)).collect()
    }

    /// The controls shown in `place` now besides their cards (interface 8), of the plugins that are
    /// on, in the order the Plugins page lists them: each as its card shows it, set as on its card.
    pub fn controls_in(&self, place: PluginPlace) -> Vec<PluginControl> {
        if place == PluginPlace::Card {
            return Vec::new();
        }
        let list = self.listed();
        let mut placed = Vec::new();
        for plugin in list.in_order().into_iter().filter(|p| p.on && p.problem.is_none()) {
            let Some(shape) = list.shapes.get(&plugin.id) else { continue };
            for (at, control) in plugin.controls.iter().enumerate() {
                let shows = shape.shows(shape.under.get(at).copied().flatten(), &plugin.controls);
                if shows && shape.places.get(at) == Some(&place) {
                    placed.push(control.clone());
                }
            }
        }
        placed
    }

    /// A hardware keyboard's `press` on the canvas, as a plugin rebinding keys took it
    /// ([`Plugins::key_press`]), that none of the app's shortcuts take, as a plugin that is on gives
    /// it to one of its switches or actions in its manifest (interface 8): the switch flipped (for
    /// the app to keep as one set on its card), or the action for the app to run. None: no
    /// plugin's.
    pub fn plugin_press(&self, press: KeyCombo) -> Option<PluginPressed> {
        match self.pressed(&press)? {
            Ok(key) => Some(PluginPressed::Action { key }),
            Err(key) => {
                let on = matches!(self.control(&key), Some(PluginControl::Switch { control }) if control.on);
                self.set_switch(key.clone(), !on);
                Some(PluginPressed::Switched { control: self.control(&key)? })
            }
        }
    }

    /// Whether `press` (as a plugin rebinding keys took it) goes to a plugin before the app's own
    /// shortcuts: a press with Alt and without the command key that a plugin that is on gives one
    /// of its switches or actions now ([`Plugins::plugin_press`]). The app's own shortcuts name no
    /// such press; they take Alt and a key as the key alone (S, selection, for Alt+S) only for want
    /// of a plugin's.
    pub fn plugin_press_first(&self, press: KeyCombo) -> bool {
        press.alt && !press.command && self.pressed(&press).is_some()
    }

    /// The filters that can be opened now: the Adjustments menu's, which has none to show with none
    /// imported. Each is as its plugin, but for one of several a plugin lists, which is there while
    /// the switch it comes under is on: its `id` is what opens it, its `name` its own, and its
    /// `controls` those of its panel.
    pub fn filters(&self) -> Vec<PluginInfo> {
        let list = self.listed();
        let ready = |p: &&PluginInfo| p.holds(PluginPart::Filter) && p.on && p.problem.is_none();
        let mut filters = Vec::new();
        for plugin in list.files.iter().filter(ready) {
            let listed = list.shapes.get(&plugin.id).map(|shape| shape.filters.as_slice()).unwrap_or_default();
            if listed.is_empty() {
                filters.push(plugin.clone());
            }
            let shape = list.shapes.get(&plugin.id);
            // (a filter layer's is offered in the Layers panel instead)
            let shown = |f: &&plugin_file::ListedFilter| {
                f.layer.is_none() && shape.is_none_or(|shape| shape.shows(f.under, &plugin.controls))
            };
            for filter in listed.iter().filter(shown) {
                filters.push(PluginInfo {
                    id: filter.id.clone(),
                    name: filter.label.clone(),
                    controls: filter.controls.iter().filter_map(|&at| plugin.controls.get(at).cloned()).collect(),
                    actions: Vec::new(),
                    ..plugin.clone()
                });
            }
        }
        filters
    }

    /// A value read back from where it was kept, as text. A key nothing has, or anything that
    /// isn't its kind of value, leaves things as they are. A plugin that stopped stays off, whatever
    /// its flag says, until someone asks it to run.
    ///
    /// A setting the app used to have, that someone changed, starts the plugin the app added to
    /// hold it on, so the app goes on as they left it: unless that plugin's own on or off is read
    /// back too, in whatever order, which says instead; and the switch it comes under, unless its
    /// own is read back.
    ///
    /// What was kept for a plugin another took the place of goes to that one: on while any of
    /// those was, with the switch standing for each, and their values to the controls that took
    /// them; unless its own on or off, or those controls' own values, are read back, in whatever
    /// order, which say instead.
    ///
    /// What was kept for a feature another took from a plugin that stays goes to that one too
    /// (interface 19): the feature's switch to the switch that took it and its values to the
    /// controls that took them, and that one on while the plugin and the feature both were; unless
    /// its own are read back, in whatever order.
    pub fn read_kept(&self, key: String, value: String) {
        let mut list = self.listed();
        let Listed { files, moved: unheld, flagged, added, shapes, carried, read, flag_switched, taken_on, .. } =
            &mut *list;
        // a plugin a feature was taken from, or that feature's switch: on, it may turn on the one
        // that took it
        let takers = || shapes.values().flat_map(|shape| &shape.takes);
        if takers().any(|t| key == format!("{}.on", t.id) || key == t.feature) {
            if value.parse::<bool>() == Ok(true) {
                taken_on.insert(key.clone());
            } else {
                taken_on.remove(&key);
            }
            take_on(files, shapes, taken_on, flagged, carried);
        }
        // (a key a plugin here holds is its own)
        let held = files.iter().any(|p| p.controls.iter().any(|c| c.key() == key));
        if !held && let Some((plugin, taken)) = taking(files, shapes, |t| t.feature == key) {
            if let (Ok(on), Some(PluginControl::Switch { control })) =
                (value.parse::<bool>(), plugin.controls.get_mut(taken.switch))
                && !read.contains(&control.key)
            {
                control.on = on;
                flag_switched.insert(control.key.clone());
            }
            return;
        }
        if !held && let Some((plugin, taken)) = taking(files, shapes, |t| t.controls.iter().any(|(old, _)| *old == key))
        {
            let at = taken.controls.iter().find(|(old, _)| *old == key).map(|(_, at)| *at);
            if let Some(control) = at.and_then(|at| plugin.controls.get_mut(at))
                && !read.contains(control.key())
            {
                control.read(&value);
            }
            return;
        }
        if let Some(plugin) = files.iter_mut().find(|p| p.on_key == key) {
            if let Ok(on) = value.parse::<bool>() {
                plugin.on = on && plugin.problem.is_none();
                flagged.insert(plugin.id.clone());
            }
            return;
        }
        // what was kept for a plugin another took the place of goes to that one: its on or off to
        // it (on while any of those was) and to the switch that stands for it, its values to the
        // controls that took them; unless their own were read
        if let Some(id) = key.strip_suffix(".on")
            && let Some((plugin, replaced)) = replacing(files, shapes, |r| r.id == id)
        {
            let Ok(on) = value.parse::<bool>() else { return };
            if !flagged.contains(&plugin.id) {
                let first = carried.insert(plugin.id.clone());
                let on = on && plugin.problem.is_none();
                plugin.on = on || (plugin.on && !first);
            }
            if let Some(PluginControl::Switch { control }) = replaced.switch.and_then(|at| plugin.controls.get_mut(at))
                && !read.contains(&control.key)
            {
                control.on = on;
                flag_switched.insert(control.key.clone());
            }
            // on, with the switches that comes under (a feature, interface 16)
            if on {
                let shape = shapes.get(&plugin.id);
                on_above(plugin, shape, replaced.switch, read, flag_switched);
            }
            return;
        }
        if let Some((plugin, replaced)) = replacing(files, shapes, |r| r.controls.iter().any(|(old, _)| *old == key)) {
            let at = replaced.controls.iter().find(|(old, _)| *old == key).map(|(_, at)| *at);
            if let Some(control) = at.and_then(|at| plugin.controls.get_mut(at))
                && !read.contains(control.key())
            {
                control.read(&value);
            }
            return;
        }
        let held = files.iter_mut().find_map(|plugin| {
            let at = plugin.controls.iter().position(|c| c.key() == key)?;
            Some((plugin, at))
        });
        if let Some((plugin, at)) = held {
            if let Some(control) = plugin.controls.get_mut(at) {
                control.read(&value);
            }
            read.insert(key.clone());
            let added_off = added.iter().any(|added| added.off && added.id == plugin.id);
            let by_flags = flagged.contains(&plugin.id) || carried.contains(&plugin.id);
            if added_off && moved_and_changed(&key, &value) && !by_flags {
                plugin.on = plugin.problem.is_none();
                // with the switch it comes under, and the one that comes under
                let shape = shapes.get(&plugin.id);
                let mut under = shape.and_then(|shape| shape.under.get(at).copied().flatten());
                while let Some(above) = under {
                    if let Some(PluginControl::Switch { control }) = plugin.controls.get_mut(above)
                        && !read.contains(&control.key)
                        && !flag_switched.contains(&control.key)
                    {
                        control.on = true;
                    }
                    // (two deep at most, as read)
                    under =
                        shape.and_then(|shape| shape.under.get(above).copied().flatten()).filter(|&next| next != above);
                }
            }
        } else if moved(&key) {
            unheld.retain(|(kept, _)| *kept != key);
            unheld.push((key, value));
        }
    }

    /// Words for someone who changed a setting that now comes as a plugin, once: it is written down
    /// that they were told. Their value stays where it was kept, for the plugin to pick up.
    pub fn take_moved_notice(&self) -> Option<String> {
        let store = self.store.as_deref()?;
        let mut list = self.listed();
        if std::mem::replace(&mut list.notice_asked, true) {
            return None;
        }
        let mut told = plugin_file::read_told(store);
        let untold = MOVED.iter().filter(|moved| {
            let changed = list.moved.iter().any(|(key, value)| key == moved.key && moved.changed(value));
            changed && !told.iter().any(|key| key == moved.key)
        });
        let untold: Vec<&Moved> = untold.collect();
        let (last, rest) = untold.split_last()?;
        told.extend(untold.iter().map(|moved| moved.key.to_owned()));
        plugin_file::note_told(store, &told);
        if rest.is_empty() {
            let name = last.name;
            return Some(format!(
                "{name} now comes as a plugin. Your setting is kept: import the plugin and it picks it up."
            ));
        }
        let names = rest.iter().map(|moved| moved.name).collect::<Vec<_>>().join(", ");
        let name = last.name;
        Some(format!(
            "{names} and {name} now come as plugins. Your settings are kept: import each plugin and it picks them up."
        ))
    }

    /// Sets the switch kept under `key`; false if there is none.
    pub fn set_switch(&self, key: String, on: bool) -> bool {
        self.change(&key, |control| match control {
            PluginControl::Switch { control } => {
                control.on = on;
                Some(())
            }
            _ => None,
        })
        .is_some()
    }

    /// Sets the slider kept under `key`; what it landed on, to keep, if there is one.
    pub fn set_slider(&self, key: String, value: f32) -> Option<f32> {
        self.change(&key, |control| match control {
            PluginControl::Slider { control } if value.is_finite() => {
                control.value = control.stepped(value);
                Some(control.value)
            }
            _ => None,
        })
    }

    /// Sets the texture control kept under `key` to `value` (see [`PluginTexture`]); what it
    /// landed on (none, 0, for a texture it doesn't offer), if there is one.
    pub fn set_texture(&self, key: String, value: i32) -> Option<i32> {
        self.change(&key, |control| match control {
            PluginControl::Texture { control } => {
                control.value = control.within(value as f32);
                Some(control.value)
            }
            _ => None,
        })
    }

    /// The picture picked for the file control of pictures kept under `key`, shrunk to fit `side`
    /// pixels each way (2 to 1024), for a texture's thumbnail; none while none is picked, or if it
    /// doesn't read.
    pub fn picture_thumbnail(&self, key: String, side: u32) -> Option<PluginPicture> {
        let store = self.store.as_deref()?;
        let path = {
            let list = self.listed();
            let plugin = list.files.iter().find(|p| p.controls.iter().any(|c| c.key() == key))?;
            let file = plugin.controls.iter().find_map(|c| match c {
                PluginControl::File { control } if control.key == key => Some(control),
                _ => None,
            })?;
            (file.accepts == PluginFileKind::Picture && file.chosen.is_some())
                .then(|| plugin_file::chosen_paths(store, &plugin.id, &file.key).0)?
        };
        let bytes = std::fs::read(path).ok()?;
        let (width, height, rgba) = crate::plugin_commands::decoded_image(&bytes)?;
        let (width, height, pixels) = crate::surround::within(width, height, rgba, side.clamp(2, 1024))?;
        Some(PluginPicture { width, height, pixels })
    }

    /// The file control kept under `key` takes the sound `wav` (a WAV file's bytes) picked by `name`:
    /// kept with its plugin (a minute of it at most, in one channel) for its plugin to play. False
    /// if there is no such control, the plugins can't be kept, or `wav` is no sound this reads.
    pub fn set_sound_file(&self, key: String, name: String, wav: Vec<u8>) -> bool {
        let Some(store) = self.store.as_deref() else { return false };
        let Some((samples, rate)) = crate::sound_file::read_wav(&wav) else { return false };
        let name: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
        let name = if name.trim().is_empty() { "Recording".to_owned() } else { name.trim().to_owned() };
        let mut list = self.listed();
        let Some((plugin, at)) = list.control_mut(&key) else { return false };
        let id = plugin.id.clone();
        let Some(PluginControl::File { control }) = plugin.controls.get_mut(at) else { return false };
        let (file, named) = plugin_file::chosen_paths(store, &id, &key);
        let kept_wav = crate::sound_file::write_wav(&samples, rate);
        let written = |tmp: &std::path::Path| {
            crate::storage::write(tmp, &kept_wav).map_err(|e| crate::project::file_error("saving", e))
        };
        let kept = file.parent().is_some_and(|folder| std::fs::create_dir_all(folder).is_ok())
            && crate::project::write_atomically(&file, written).is_ok()
            && crate::storage::write(&named, &name).is_ok();
        if !kept {
            log::warn!("a sound for {key} couldn't be kept");
            return false;
        }
        control.chosen = Some(name);
        let feature = list.shapes.get(&id).and_then(|shape| shape.feature_switch_of(at));
        self.hooks.control(&id, at, 1.0);
        self.hooks.give_file(&id, feature, at, Some(&kept_wav));
        true
    }

    /// The file control kept under `key` takes the picture `bytes` (a PNG or a JPEG) picked by
    /// `name`: kept with its plugin as it came (interface 11). False if there is no such control or
    /// it takes no pictures, the plugins can't be kept, or `bytes` is no picture this reads or is
    /// past [`MOST_PICTURE`].
    pub fn set_picture_file(&self, key: String, name: String, bytes: Vec<u8>) -> bool {
        let Some(store) = self.store.as_deref() else { return false };
        if bytes.len() > MOST_PICTURE || crate::plugin_commands::decoded_image(&bytes).is_none() {
            return false;
        }
        let name: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
        let name = if name.trim().is_empty() { "Picture".to_owned() } else { name.trim().to_owned() };
        let mut list = self.listed();
        let Some((plugin, at)) = list.control_mut(&key) else { return false };
        let id = plugin.id.clone();
        let Some(PluginControl::File { control }) = plugin.controls.get_mut(at) else { return false };
        if control.accepts != PluginFileKind::Picture {
            return false;
        }
        let (file, named) = plugin_file::chosen_paths(store, &id, &key);
        let written = |tmp: &std::path::Path| {
            crate::storage::write(tmp, &bytes).map_err(|e| crate::project::file_error("saving", e))
        };
        let kept = file.parent().is_some_and(|folder| std::fs::create_dir_all(folder).is_ok())
            && crate::project::write_atomically(&file, written).is_ok()
            && crate::storage::write(&named, &name).is_ok();
        if !kept {
            log::warn!("a picture for {key} couldn't be kept");
            return false;
        }
        control.chosen = Some(name);
        self.hooks.control(&id, at, 1.0);
        true
    }

    /// The file control kept under `key` lets its file go, or the folder control its folder (what
    /// is in the folder stays as it is). False if there is no such control.
    pub fn clear_file(&self, key: String) -> bool {
        let mut list = self.listed();
        let Some((plugin, at)) = list.control_mut(&key) else { return false };
        let id = plugin.id.clone();
        if let Some(PluginControl::Folder { control }) = plugin.controls.get_mut(at) {
            if let Some(store) = self.store.as_deref() {
                let (location, name, records) = plugin_file::chosen_folder_paths(store, &id, &key);
                for path in [location, name, records] {
                    let _ = std::fs::remove_file(path);
                }
            }
            control.chosen = None;
            control.location = None;
            self.hooks.control(&id, at, 0.0);
            self.folders.chosen_again();
            return true;
        }
        let Some(PluginControl::File { control }) = plugin.controls.get_mut(at) else { return false };
        if let Some(store) = self.store.as_deref() {
            let (file, named) = plugin_file::chosen_paths(store, &id, &key);
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_file(&named);
        }
        control.chosen = None;
        let feature = list.shapes.get(&id).and_then(|shape| shape.feature_switch_of(at));
        self.hooks.control(&id, at, 0.0);
        self.hooks.give_file(&id, feature, at, None);
        true
    }

    /// The folder control kept under `key` takes the folder at `location`, as the app's folder access
    /// reads it (see [`Plugins::use_folder_access`]), shown as `name` (interface 22): kept with its
    /// plugin, and what was noted of another folder forgotten. False if there is no such control, the
    /// plugins can't be kept, or `location` is nothing.
    pub fn set_folder(&self, key: String, location: String, name: String) -> bool {
        let Some(store) = self.store.as_deref() else { return false };
        if location.trim().is_empty() || location.contains('\0') {
            return false;
        }
        let name: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
        let name = if name.trim().is_empty() { "Folder".to_owned() } else { name.trim().to_owned() };
        let mut list = self.listed();
        let Some((plugin, at)) = list.control_mut(&key) else { return false };
        let id = plugin.id.clone();
        let Some(PluginControl::Folder { control }) = plugin.controls.get_mut(at) else { return false };
        let (kept_at, named, records) = plugin_file::chosen_folder_paths(store, &id, &key);
        let write = |path: &std::path::Path, text: &str| {
            crate::project::write_atomically(path, |tmp| {
                crate::storage::write(tmp, text).map_err(|e| crate::project::file_error("saving", e))
            })
            .is_ok()
        };
        let kept = kept_at.parent().is_some_and(|folder| std::fs::create_dir_all(folder).is_ok())
            && write(&kept_at, &location)
            && write(&named, &name);
        if !kept {
            log::warn!("a folder for {key} couldn't be kept");
            return false;
        }
        if control.location.as_deref() != Some(location.as_str()) {
            let _ = std::fs::remove_file(records);
        }
        control.chosen = Some(name);
        control.location = Some(location);
        self.hooks.control(&id, at, 1.0);
        self.folders.chosen_again();
        true
    }

    /// The app's way into the folders picked for plugins' `folder` controls, and a folder of its own
    /// (in its cache) where files are made before they go into one (interface 22). Until it is given,
    /// nothing goes to a folder or comes from one.
    pub fn use_folder_access(&self, access: Arc<dyn crate::folders::FolderAccess>, work: String) {
        self.folders.use_access(access, PathBuf::from(work));
    }

    /// The gallery opened, its artworks in `store` (interface 22): the packages in the folder a
    /// plugin on the `gallery_opened` hook names come in, those that changed there since the app last
    /// wrote or read them or that it never had, never over an artwork changed here too (that one
    /// comes as a copy beside it); and what to tell once of a folder that can't be reached. A long
    /// call, not for the main thread: with none on the hook, it does nothing.
    pub fn gallery_opened(&self, store: Arc<crate::gallery::ArtworkStore>) -> crate::folders::FolderBroughtIn {
        self.folders.gallery_opened(&store)
    }

    /// A time-lapse video of the artwork named `name` was exported to `path` (interface 22): copied
    /// to the folder a plugin on the `video_exported` hook names, before long, from a thread of its
    /// own. False if none names one (nothing to wait for then).
    pub fn video_exported(&self, path: String, name: String) -> bool {
        self.folders.video_exported(std::path::Path::new(&path), &name)
    }

    /// Runs the action `key` (`<id>.<key>`) of a plugin that is on, on the artwork `session`: what
    /// it changes is one step undo takes back. It runs in a copy of the plugin of its own, told its
    /// controls, so its hooks go on meanwhile. A long call, not for the main thread. False if no
    /// plugin on has it, or the plugin stopped (it goes off, and says why).
    pub fn run_action(&self, key: String, session: Arc<crate::CanvasSession>) -> bool {
        self.run_action_from(key, session, None)
    }

    /// Runs the action `key` as [`Plugins::run_action`] does, come from window pixel (`x`, `y`):
    /// where the pen was whose button runs it (interface 11). A panel it shows as a ring shows
    /// there.
    pub fn run_action_at(&self, key: String, session: Arc<crate::CanvasSession>, x: f32, y: f32) -> bool {
        let at = (x.is_finite() && y.is_finite()).then_some(crate::PanelPoint { x, y });
        self.run_action_from(key, session, at)
    }

    fn run_action_from(&self, key: String, session: Arc<crate::CanvasSession>, at: Option<crate::PanelPoint>) -> bool {
        let found = {
            let list = self.listed();
            list.files.iter().filter(|p| p.on && p.problem.is_none()).find_map(|p| {
                let index = p.actions.iter().position(|a| a.key == key)?;
                // (one under a switch that is off isn't there)
                let shape = list.shapes.get(&p.id);
                let under = shape.and_then(|shape| shape.action_under.get(index).copied().flatten());
                if !shape.is_none_or(|shape| shape.shows(under, &p.controls)) {
                    return None;
                }
                let values: Vec<f32> = p.controls.iter().map(PluginControl::number).collect();
                let marks = shape.map_or(p.id.as_str(), |shape| shape.marks_id(&p.id)).to_owned();
                Some((p.id.clone(), p.version.clone(), index, values, p.permissions.clone(), marks))
            })
        };
        let Some((id, version, index, values, permissions, marks)) = found else { return false };
        // another action closes a panel open
        *self.open_panel() = None;
        let started = self.start_code(&id, &version, &values, &permissions, None, &marks);
        let ran = started.and_then(|(mut running, _)| {
            let step = session.plugin_step();
            let ran = running.action(index, Arc::clone(&session));
            drop(step);
            ran?;
            self.keep_from(&id, &mut running)?;
            Ok(running)
        });
        match ran {
            Ok(mut running) => {
                // a panel it showed stays open, with this copy of its code
                if let (Some(shows), false) = running.take_panel() {
                    let running = Arc::new(Mutex::new(running));
                    *self.open_panel() = Some(OpenPanel { id, shows, running, at });
                }
                true
            }
            Err(stop) => {
                // (the list takes it off when next read)
                self.hooks.stopped(&id, stop);
                false
            }
        }
    }

    /// The kinds of artwork the plugins on make, for the gallery's +.
    pub fn kinds(&self) -> Vec<crate::PluginKind> {
        let list = self.listed();
        let on = list.files.iter().filter(|p| p.on && p.problem.is_none());
        on.flat_map(|p| p.kinds.iter().cloned()).collect()
    }

    /// The filter layers the plugins on offer (interface 17), for the Layers panel's +: each by
    /// its id (`<plugin id>.<key>`), its name and the heading it comes under, in the order the
    /// plugins list them.
    pub fn layer_filters(&self) -> Vec<crate::PluginLayerFilter> {
        let list = self.listed();
        let mut out = Vec::new();
        for plugin in list.files.iter().filter(|p| p.on && p.problem.is_none()) {
            let Some(shape) = list.shapes.get(&plugin.id) else { continue };
            for f in shape.filters.iter().filter(|f| shape.shows(f.under, &plugin.controls)) {
                if let Some(layer) = &f.layer {
                    let (id, label, group) = (f.id.clone(), f.label.clone(), layer.group.clone());
                    out.push(crate::PluginLayerFilter { id, label, group });
                }
            }
        }
        out
    }

    /// The kinds of layer the plugins on add, for the Layers panel's +.
    pub fn layer_kinds(&self) -> Vec<crate::PluginKind> {
        let list = self.listed();
        let on = list.files.iter().filter(|p| p.on && p.problem.is_none());
        on.flat_map(|p| p.layer_kinds.iter().cloned()).collect()
    }

    /// An artwork of kind `kind` (`<id>.<key>`) opened, in a copy of its plugin's code of its own:
    /// a new one `width` × `height`, or the one its saved `data` holds. None if no plugin on makes
    /// that kind, or it couldn't (one that stops goes off, and says why).
    pub fn open_canvas(
        &self,
        kind: String,
        width: u32,
        height: u32,
        data: Option<Vec<u8>>,
    ) -> Option<Arc<crate::PluginCanvas>> {
        let found = {
            let list = self.listed();
            list.files.iter().filter(|p| p.on && p.problem.is_none()).find_map(|p| {
                p.kinds.iter().any(|k| k.key == kind).then(|| {
                    let values: Vec<f32> = p.controls.iter().map(PluginControl::number).collect();
                    (p.id.clone(), p.version.clone(), values, p.permissions.clone())
                })
            })
        };
        let (id, version, values, permissions) = found?;
        let running = match self.start_code(&id, &version, &values, &permissions, None, &id) {
            Ok((running, _)) => running,
            Err(stop) => {
                self.hooks.stopped(&id, stop);
                return None;
            }
        };
        match crate::PluginCanvas::open(id.clone(), Arc::clone(&self.hooks), running, width, height, data.as_deref()) {
            Ok(canvas) => {
                let canvas = Arc::new(canvas);
                // told its plugin's controls as they change
                self.hooks.keep_canvas(&id, &canvas);
                Some(canvas)
            }
            Err(stop) => {
                if let Some(stop) = stop {
                    self.hooks.stopped(&id, stop);
                }
                None
            }
        }
    }

    /// The panel a plugin's action showed, while it is open.
    pub fn panel(&self) -> Option<crate::PluginPanel> {
        self.open_panel().as_ref().map(|open| crate::PluginPanel { at: open.at, ..open.shows.clone() })
    }

    /// Row `row` of the panel open touched with `value` (a slider's, a switch's 1 or 0, a choice's
    /// place, 1 for a button), on the artwork `session`: the plugin is told, and what it changes is
    /// one step undo takes back. It may show the panel changed, or close it. A long call, not for the
    /// main thread. False once the panel is closed (by the plugin, or as it stopped).
    pub fn panel_touched(&self, row: u32, value: f32, session: Arc<crate::CanvasSession>) -> bool {
        let row = row as usize;
        // the touch shows at once, and the code is fetched to be told
        let (id, running, value) = {
            let mut open = self.open_panel();
            let Some(panel) = open.as_mut() else { return false };
            let Some(value) = crate::plugin_panel::touched(&panel.shows, row, value) else { return true };
            panel.shows = crate::plugin_panel::with_value(panel.shows.clone(), row, value);
            (panel.id.clone(), Arc::clone(&panel.running), value)
        };
        let mut code = running.lock().unwrap_or_else(|e| e.into_inner());
        let step = session.plugin_step();
        let ran = code.panel_event(row, value, Arc::clone(&session));
        drop(step);
        let answer = ran.and_then(|()| {
            let shown = code.take_panel();
            self.keep_from(&id, &mut code)?;
            Ok(shown)
        });
        drop(code);
        // what it answered, unless that panel was closed meanwhile (or another took its place)
        self.panel_answered(&id, &running, answer)
    }

    /// The key `press` (its key named as [`crate::key_shortcut`] takes it) while the panel open asks
    /// for one: told to its plugin, on the artwork `session`, as a touch on it is (it may show the
    /// panel changed, or close it). The panel asked for that press alone, and asks again for another.
    /// A long call, not for the main thread. False if no panel open asked for it.
    pub fn panel_key(&self, press: KeyCombo, session: Arc<crate::CanvasSession>) -> bool {
        let press = KeyCombo::of(&press.key, press.command, press.shift, press.alt);
        let (id, running) = {
            let mut open = self.open_panel();
            let Some(panel) = open.as_mut().filter(|panel| panel.shows.key_press && press.named_plainly()) else {
                return false;
            };
            panel.shows.key_press = false;
            (panel.id.clone(), Arc::clone(&panel.running))
        };
        let mut code = running.lock().unwrap_or_else(|e| e.into_inner());
        let step = session.plugin_step();
        let ran = code.panel_key(&press, Arc::clone(&session));
        drop(step);
        let answer = ran.and_then(|()| {
            let shown = code.take_panel();
            self.keep_from(&id, &mut code)?;
            Ok(shown)
        });
        drop(code);
        self.panel_answered(&id, &running, answer);
        true
    }

    /// The panel open closed (its close button, or a tap away from it).
    pub fn close_panel(&self) {
        *self.open_panel() = None;
    }

    /// Turns a plugin on or off; false if no file is `id`. It goes on its hooks or off them at once,
    /// no restart: off them the app is as it was, and its values wait for it. One that can't start
    /// stays off, and says why.
    pub fn set_on(&self, id: String, on: bool) -> bool {
        let mut list = self.listed();
        let Listed { files, shapes, .. } = &mut *list;
        let Some(plugin) = files.iter_mut().find(|p| p.id == id) else { return false };
        plugin.on = on;
        plugin.problem = None;
        if let Some(store) = &self.store {
            plugin_file::clear_stopped(store, &id);
        }
        self.take_off(&id);
        self.start(plugin, shapes.get(&id));
        true
    }

    /// What the plugin file `bytes` is, for the screen that asks whether to import it. Nothing is
    /// kept, and nothing in it runs.
    pub fn inspect(&self, bytes: Vec<u8>) -> PluginOffer {
        let offered = plugin_file::read_package(&bytes).and_then(|package| {
            self.can_take(&package.manifest)?;
            Ok(package.manifest)
        });
        match offered {
            Ok(manifest) => {
                let publisher = if manifest.publisher.is_empty() { "none given" } else { &manifest.publisher };
                let mut sentence = can_do(&manifest);
                // what it takes the place of, by name, if it is here
                let names: Vec<String> = {
                    let list = self.listed();
                    let here = |id: &str| list.files.iter().find(|p| p.id == id).map(|p| p.name.clone());
                    manifest.shape.replaces.iter().filter_map(|r| here(&r.id)).collect()
                };
                if let Some((last, rest)) = names.split_last() {
                    let all = if rest.is_empty() { last.clone() } else { format!("{} and {last}", rest.join(", ")) };
                    sentence.push_str(&format!(" It takes the place of {all}, keeping what was set."));
                }
                PluginOffer {
                    message: format!("Version {}\nPublisher: {publisher} (not checked)\n{sentence}", manifest.version),
                    name: manifest.name,
                    refused: None,
                }
            }
            Err(refusal) => {
                log::warn!("a plugin file was refused: {refusal:?}");
                PluginOffer { name: String::new(), message: String::new(), refused: Some(refusal.text().to_owned()) }
            }
        }
    }

    /// Imports the plugin file `bytes`, read in full again, so nothing needs to have gone before;
    /// why not, if it wasn't. It shows at once, no restart. A file of a plugin already here
    /// replaces it, keeping what was set, unless it is older.
    pub fn install(&self, bytes: Vec<u8>) -> Option<String> {
        let taken = self.take_in(&bytes);
        taken.err().map(|refusal| {
            log::warn!("a plugin file wasn't imported: {refusal:?}");
            refusal.text().to_owned()
        })
    }

    /// Removes the imported plugin `id`: its folder goes, at once, no restart. The keys to take out
    /// of where values are kept; none if it can't be removed.
    pub fn remove(&self, id: String) -> Option<Vec<String>> {
        let mut list = self.listed();
        let at = list.files.iter().position(|p| p.id == id)?;
        if let Some(store) = &self.store
            && !plugin_file::forget(store, &id)
        {
            return None;
        }
        self.take_off(&id);
        let gone = list.files.remove(at);
        list.flagged.remove(&id);
        list.carried.remove(&id);
        // what was kept for those it took the place of goes too
        let replaced = list.shapes.remove(&id).map(|shape| shape.replaces).unwrap_or_default();
        let theirs = replaced.into_iter().flat_map(|r| {
            let on = format!("{}.on", r.id);
            std::iter::once(on).chain(r.controls.into_iter().map(|(key, _)| key))
        });
        // one the app added stays written down, so it isn't added again; imported again, it is as
        // any import
        if list.added_off(&id)
            && let Some(store) = &self.store
        {
            let mut added = list.added.clone();
            added.iter_mut().filter(|added| added.id == id).for_each(|added| added.off = false);
            if plugin_file::note_added(store, &added) {
                list.added = added;
            }
        }
        let keys = gone.controls.iter().map(|c| c.key().to_owned());
        Some(keys.chain([gone.on_key.clone()]).chain(theirs).collect())
    }

    /// How big the interface shows in a window `width` × `height` points at 1: as a plugin says, or
    /// as measured.
    pub fn interface_scale(&self, width: f32, height: f32) -> f32 {
        self.hooks().interface_scale(width, height)
    }

    /// Whether a plugin keeps the interface clear of the status bar and the screen's cutouts, a
    /// camera's: the app then lays it out inside them. Where there are none, as on a desktop, it
    /// changes nothing.
    pub fn keep_clear(&self) -> bool {
        self.hooks().keep_clear()
    }

    /// Whether a plugin draws in-betweens: Animation Assist's frame options then offer In-between
    /// ([`crate::CanvasSession::add_inbetween`]).
    pub fn tweens(&self) -> bool {
        self.hooks().tweens()
    }

    /// Whether a plugin makes the sound of the stroke being drawn: the app plays
    /// [`crate::CanvasSession::stroke_sound`] while one does.
    pub fn makes_stroke_sound(&self) -> bool {
        self.hooks().makes_stroke_sound()
    }

    /// Whether a plugin on draws the colour panel's Classic square its own way: its picture
    /// ([`crate::CanvasSession::picker_picture`]) and where touches land on it.
    pub fn picker_on(&self) -> bool {
        self.hooks().picker().is_some()
    }

    /// The saturation and brightness the picker plugin says point (`x`, `y`) of the Classic square
    /// picks (0..1, y down): `[s, v]`, or none with no picker plugin on.
    pub fn picker_pick(&self, x: f32, y: f32) -> Option<Vec<f32>> {
        self.hooks().picker_pick(x, y).map(|(s, v)| vec![s, v])
    }

    /// Where the picker plugin says saturation `s` and brightness `v` sit on the Classic square
    /// (0..1, y down): `[x, y]`, or none with no picker plugin on.
    pub fn picker_point(&self, s: f32, v: f32) -> Option<Vec<f32>> {
        self.hooks().picker_point(s, v).map(|(x, y)| vec![x, y])
    }

    /// The hue (0..1) the picker plugin says a touch starting at point (`x`, `y`) of the Classic
    /// square picks (0..1, y down); none where it picks saturation and brightness, or with no picker
    /// plugin on that picks hues itself.
    pub fn picker_hue(&self, x: f32, y: f32) -> Option<f32> {
        self.hooks().picker_hue(x, y)
    }

    /// Where the picker plugin says hue `h`'s knob sits on the Classic square (0..1, y down): `[x,
    /// y]`, or none while the app's hue slider has the hue (it shows only then).
    pub fn picker_hue_point(&self, h: f32) -> Option<Vec<f32>> {
        self.hooks().picker_hue_point(h).map(|(x, y)| vec![x, y])
    }

    /// A number that changes whenever a plugin comes on or off or its controls change: what a plugin
    /// answered, or a picture drawn by one, before it changed may not be what it answers now.
    pub fn generation(&self) -> u64 {
        self.hooks().generation()
    }

    /// How big `part` of the interface shows next to the rest of it, on top of
    /// [`Self::interface_scale`], as plugins say (1: as the rest).
    pub fn interface_part_scale(&self, part: InterfacePart) -> f32 {
        self.hooks().interface_part_scale(part as i32)
    }

    /// Whether a plugin has the floating colour window show Classic's picker alone: no title, mode
    /// icons or the other pickers.
    pub fn floating_picker_only(&self) -> bool {
        self.hooks().floating_picker_only()
    }

    /// Whether the notice of `kind` shows: one of something gone wrong always does; one of
    /// something done unless a plugin hides it (what undo and redo did, by the undo notice's hook
    /// too).
    pub fn shows_notice(&self, kind: NoticeKind) -> bool {
        let hooks = self.hooks();
        match kind {
            NoticeKind::Problem => true,
            NoticeKind::Step => !hooks.hide_undo_notice() && !hooks.hide_notice(kind as i32),
            NoticeKind::Done => !hooks.hide_notice(kind as i32),
        }
    }

    /// How many Reference windows may be open at once: the most a plugin on the reference
    /// windows' hook says, 1 with none.
    pub fn reference_windows(&self) -> u32 {
        self.hooks().reference_windows()
    }

    /// Whether a plugin lets `panel` be torn off into a window of its own.
    pub fn floats(&self, panel: crate::floating::FloatingPanel) -> bool {
        self.hooks().floating_panel(panel as i32)
    }

    /// Whether a plugin lets the panel numbered `number` on the `floating_panels` hook float (see
    /// [`crate::LayoutPiece`]).
    pub(crate) fn floats_number(&self, number: i32) -> bool {
        self.hooks().floating_panel(number)
    }

    /// The choices the menus at `place` offer, of each plugin on that has one there (in order of
    /// id), as their manifests list them. None: the app shows no button for them.
    pub fn menu_choices(&self, place: PluginMenuPlace) -> Vec<PluginChoice> {
        let list = self.listed();
        let mut on: Vec<&PluginInfo> = list.files.iter().filter(|p| p.on && p.problem.is_none()).collect();
        on.sort_by(|a, b| a.id.cmp(&b.id));
        let mut choices = Vec::new();
        for plugin in on {
            let Some(shape) = list.shapes.get(&plugin.id) else { continue };
            let Some(menu) = shape.menu.as_ref() else { continue };
            // (one under a switch that is off isn't there)
            if menu.place != place || !shape.menu_shows(&plugin.controls) {
                continue;
            }
            for choice in &menu.choices {
                let chosen =
                    choice.sets.iter().all(|&(at, value)| plugin.controls.get(at).is_some_and(|c| c.number() == value));
                choices.push(PluginChoice {
                    key: choice.key.clone(),
                    label: choice.label.clone(),
                    icon: choice.icon.clone(),
                    chosen,
                });
            }
        }
        choices
    }

    /// The controls plugins' code set of their own (`set_control`, interface 12) since this was last
    /// asked, as they are now: for the app to keep as it keeps a switch or a slider set on the card,
    /// and to have the artwork follow the plugins again.
    pub fn take_controls_set(&self) -> Vec<PluginControl> {
        std::mem::take(&mut *self.controls_set.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// The menu choice `key` ([`PluginChoice::key`]) picked: each control it sets set so, as if on
    /// its card, its plugin told. The controls it changed, as they are now, for the app to keep
    /// (as it keeps a switch or a slider set on the card); none for a choice no plugin on offers.
    pub fn choose(&self, key: String) -> Vec<PluginControl> {
        let mut list = self.listed();
        let Listed { files, shapes, .. } = &mut *list;
        let found = files.iter_mut().filter(|p| p.on && p.problem.is_none()).find_map(|plugin| {
            let shape = shapes.get(&plugin.id)?;
            let choice = shape.menu.as_ref()?.choices.iter().find(|c| c.key == key)?;
            shape.menu_shows(&plugin.controls).then(|| (plugin, choice.sets.clone()))
        });
        let Some((plugin, sets)) = found else { return Vec::new() };
        let artwork: Vec<bool> = shapes.get(&plugin.id).map(|shape| shape.artwork.clone()).unwrap_or_default();
        let mut changed = Vec::new();
        let mut kept = Vec::new();
        for (at, value) in sets {
            let Some(control) = plugin.controls.get_mut(at) else { continue };
            if control.number() == value {
                continue;
            }
            match control {
                PluginControl::Switch { control } => control.on = value != 0.0,
                PluginControl::Slider { control } => control.value = control.stepped(value),
                PluginControl::Texture { control } => control.value = control.within(value),
                PluginControl::File { .. } | PluginControl::Folder { .. } => continue,
            }
            self.hooks.control(&plugin.id, at, control.number());
            if artwork.get(at) == Some(&true) {
                kept.push((control.key().to_owned(), control.number()));
            }
            changed.push(control.clone());
            let shape = shapes.get(&plugin.id);
            self.feature_switched(plugin, shape, at);
        }
        drop(list);
        for (key, value) in kept {
            self.keep_with_artwork(key, value);
        }
        changed
    }

    /// Whether a plugin has the colour panel's Classic show a triangle (saturation and brightness
    /// between the pure hue, white and black) in place of its square.
    pub fn colour_triangle(&self) -> bool {
        self.hooks().colour_triangle()
    }

    /// Whether a plugin sets brushes' settings at random: the brush studio shows Randomise.
    pub fn randomises_brushes(&self) -> bool {
        self.hooks().randomises_brushes()
    }

    /// The accent (0xRRGGBB), the blue of what is chosen and switched on, if a plugin gives the
    /// interface another; none: as measured.
    pub fn accent_colour(&self) -> Option<u32> {
        self.hooks().interface_colour(0)
    }

    /// How the side bar's slider tracks look, as the plugins on the slider look's hook say
    /// (interface 11).
    pub fn slider_look(&self) -> SliderLook {
        let hooks = self.hooks();
        SliderLook {
            track_opacity: hooks.slider_look(crate::hooks::SLIDER_OPACITY),
            frosted: hooks.slider_look(crate::hooks::SLIDER_FROSTED).map(|v| v >= 0.5),
        }
    }

    /// How the floating panels are laid out, as the plugins on the panel layout's hook say
    /// (interface 20): with none on, as the app always had them.
    pub fn panel_layout(&self) -> crate::layout::PanelLayoutLook {
        use crate::hooks::{LAYOUT_BELT, LAYOUT_COLLAPSE, LAYOUT_EDIT, LAYOUT_FOLD, LAYOUT_ROOM, LAYOUT_SNAP};
        let hooks = self.hooks();
        if !hooks.listens(crate::plugin_run::Hook::PanelLayout) {
            return crate::layout::PanelLayoutLook::default();
        }
        let [snap, fold, room, edit, belt, collapse] =
            [LAYOUT_SNAP, LAYOUT_FOLD, LAYOUT_ROOM, LAYOUT_EDIT, LAYOUT_BELT, LAYOUT_COLLAPSE]
                .map(|p| hooks.panel_layout(p));
        let yes = |said: Option<f32>| said.is_some_and(|v| v >= 0.5);
        crate::layout::PanelLayoutLook {
            // the panels are laid out while a plugin says anything of how; the bars alone don't
            on: snap.is_some() || fold.is_some() || room.is_some(),
            snap: snap.unwrap_or(0.0),
            fold: yes(fold),
            make_room: yes(room),
            edit: yes(edit),
            belt: yes(belt),
            collapse: yes(collapse),
        }
    }

    /// Whether the colour panel's sliders step one unit at a tap on either end (a degree of hue, a
    /// percent, a level of red, green or blue), as a plugin on the colour panel's hook says
    /// (interface 12).
    pub fn colour_steps(&self) -> bool {
        self.hooks().colour_panel(crate::hooks::COLOUR_STEPS)
    }

    /// Whether the colour panel's swatch shows the colour as the panel opened beside the colour now,
    /// a tap on the first going back to it, as a plugin on the colour panel's hook says (interface
    /// 12).
    pub fn colour_before(&self) -> bool {
        self.hooks().colour_panel(crate::hooks::COLOUR_BEFORE)
    }

    /// A hardware keyboard's `press` on the canvas (its key named as [`crate::key_shortcut`] takes
    /// it) as a plugin rebinding keys takes it: itself, another press, or none at all (it does
    /// nothing). With none on, itself.
    pub fn key_press(&self, press: KeyCombo) -> Option<KeyCombo> {
        let press = KeyCombo::of(&press.key, press.command, press.shift, press.alt);
        if !press.named_plainly() {
            return Some(press);
        }
        self.hooks().key_shortcut(press)
    }

    /// The shortcut on a press: what [`crate::key_shortcut`] says of the press a plugin rebinding
    /// keys takes it as ([`Plugins::key_press`]), unless a plugin's switch or action takes it first
    /// ([`Plugins::plugin_press_first`]).
    pub fn key_shortcut(
        &self,
        key: String,
        command: bool,
        shift: bool,
        alt: bool,
        transforming: bool,
    ) -> Option<crate::KeyShortcut> {
        let press = self.key_press(KeyCombo { key, command, shift, alt })?;
        if self.plugin_press_first(press.clone()) {
            return None;
        }
        crate::key_shortcut(press.key, press.command, press.shift, press.alt, transforming)
    }

    /// The keyboard shortcuts' list as [`crate::key_shortcut_list`] has it, with the keys a plugin
    /// rebinding them says do each now.
    pub fn key_shortcut_list(&self, desktop: bool, mac: bool) -> Vec<crate::ShortcutGroup> {
        let hooks = self.hooks();
        let mut groups = if hooks.rebinds_keys() {
            crate::keys::shortcut_list_now(desktop, mac, |press| hooks.key_shortcut_keys(press))
        } else {
            crate::key_shortcut_list(desktop, mac)
        };
        // the presses plugins give their switches and actions, under their names
        let rows: Vec<crate::ShortcutRow> = self
            .plugin_presses()
            .into_iter()
            .map(|(press, does)| crate::ShortcutRow { keys: press.label(mac), combos: vec![press], does })
            .collect();
        if !rows.is_empty() {
            groups.push(crate::ShortcutGroup { heading: "Plugins".to_owned(), rows });
        }
        groups
    }
}

impl Plugins {
    /// Each press a plugin that is on gives one of its switches or actions (each while it shows),
    /// with what it does: the switch's or the action's words.
    fn plugin_presses(&self) -> Vec<(KeyCombo, String)> {
        let list = self.listed();
        let mut presses = Vec::new();
        for plugin in list.in_order().into_iter().filter(|p| p.on && p.problem.is_none()) {
            let Some(shape) = list.shapes.get(&plugin.id) else { continue };
            let shows =
                |under: &[Option<usize>], at: usize| shape.shows(under.get(at).copied().flatten(), &plugin.controls);
            for (at, press) in shape.presses.iter().enumerate() {
                if let (Some(press), Some(PluginControl::Switch { control })) = (press, plugin.controls.get(at))
                    && shows(&shape.under, at)
                {
                    presses.push((press.clone(), control.label.clone()));
                }
            }
            for (at, press) in shape.action_presses.iter().enumerate() {
                if let (Some(press), Some(action)) = (press, plugin.actions.get(at))
                    && shows(&shape.action_under, at)
                {
                    presses.push((press.clone(), action.label.clone()));
                }
            }
        }
        presses
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::plugin_file::tests::{SHADER, coded, filter, pack};
    use crate::plugin_run::tests::position_module;

    /// A slider in steps of 5% from 75% to 125%.
    fn size_slider() -> PluginSlider {
        PluginSlider {
            key: "size".to_owned(),
            label: "Size".to_owned(),
            least: 0.75,
            most: 1.25,
            step: 0.05,
            value: 1.0,
            starts: 1.0,
            shown_times: 100.0,
            unit: "%".to_owned(),
            at_least: None,
            names: Vec::new(),
            live: false,
        }
    }

    /// A slider of whole pixels, from Off.
    fn gap_slider() -> PluginSlider {
        PluginSlider {
            key: "gap".to_owned(),
            label: "Gap".to_owned(),
            least: 0.0,
            most: 32.0,
            step: 1.0,
            value: 0.0,
            starts: 0.0,
            shown_times: 1.0,
            unit: " px".to_owned(),
            at_least: Some("Off".to_owned()),
            names: Vec::new(),
            live: false,
        }
    }

    /// The filters imported.
    fn filters(plugins: &Plugins) -> Vec<PluginInfo> {
        plugins.plugins().into_iter().filter(|p| p.holds.contains(&PluginPart::Filter)).collect()
    }

    /// One from elsewhere: nothing in the app asks about it.
    fn stranger() -> PluginInfo {
        PluginInfo {
            features: Vec::new(),
            top: Vec::new(),
            brush_controls: Vec::new(),
            actions: Vec::new(),
            kinds: Vec::new(),
            layer_kinds: Vec::new(),
            permissions: Vec::new(),
            id: "someone.stranger".to_owned(),
            name: "Stranger".to_owned(),
            description: "No code answers to it.".to_owned(),
            source: PluginSource::File,
            holds: vec![PluginPart::Hooks],
            version: "1.0.0".to_owned(),
            publisher: "Someone".to_owned(),
            experimental: false,
            on: true,
            on_key: "someone.stranger.on".to_owned(),
            problem: None,
            controls: vec![
                PluginControl::Switch {
                    control: PluginSwitch {
                        key: "someone.stranger.switch".to_owned(),
                        label: "On".to_owned(),
                        on: false,
                        starts: false,
                    },
                },
                PluginControl::Slider {
                    control: PluginSlider {
                        key: "someone.stranger.slider".to_owned(),
                        label: "Amount".to_owned(),
                        least: 0.0,
                        most: 10.0,
                        step: 0.5,
                        value: 2.0,
                        starts: 2.0,
                        shown_times: 1.0,
                        unit: " px".to_owned(),
                        at_least: None,
                        names: Vec::new(),
                        live: true,
                    },
                },
            ],
        }
    }

    /// A folder of its own, empty, for a test's plugins.
    fn store(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("procraft-plugins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.to_string_lossy().into_owned()
    }

    fn at(dir: &str) -> Arc<Plugins> {
        Plugins::with_store(dir.to_owned())
    }

    fn filter_file(manifest: &Value) -> Vec<u8> {
        pack(manifest, &[("filter.wgsl", SHADER)])
    }

    fn dir_holds(dir: &str, path: &str) -> bool {
        std::path::Path::new(dir).join(path).exists()
    }

    #[test]
    fn nothing_is_in_the_app_until_a_file_is_imported() {
        let plugins = Plugins::new();
        assert!(plugins.plugins().is_empty());
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 1.0);
        assert!(plugins.kept_lines().is_empty());
    }

    #[test]
    fn a_kept_value_comes_back_usable() {
        let read = |text: &str| {
            let plugins = Plugins::new();
            plugins.listed().files.push(stranger());
            plugins.read_kept("someone.stranger.slider".to_owned(), text.to_owned());
            match plugins.control("someone.stranger.slider") {
                Some(PluginControl::Slider { control }) => control.value,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(read("3.3"), 3.5);
        assert_eq!(read("90"), 10.0);
        assert_eq!(read("-1"), 0.0);
        for bad in ["NaN", "inf", "-inf", "", "big", "true"] {
            assert_eq!(read(bad), 2.0, "{bad}");
        }
        let plugins = Plugins::new();
        plugins.listed().files.push(stranger());
        plugins.read_kept("someone.stranger.switch".to_owned(), "yes".to_owned());
        assert!(
            matches!(plugins.control("someone.stranger.switch"), Some(PluginControl::Switch { control }) if !control.on)
        );
    }

    #[test]
    fn what_is_kept_reads_back_the_same() {
        let plugins = Plugins::new();
        plugins.listed().files.push(stranger());
        assert_eq!(plugins.set_slider("someone.stranger.slider".to_owned(), 7.0), Some(7.0));
        let lines = plugins.kept_lines();
        assert!(lines.contains(&"someone.stranger.slider 7".to_owned()), "{lines:?}");
        let again = Plugins::new();
        again.listed().files.push(stranger());
        for line in &lines {
            let (key, value) = line.split_once(' ').unwrap();
            again.read_kept(key.to_owned(), value.to_owned());
        }
        assert_eq!(again.plugins(), plugins.plugins());
    }

    #[test]
    fn a_slider_moves_in_steps_with_100_in_the_middle() {
        let size = size_slider();
        assert_eq!(plugin_slider_at(size.clone(), 0.0), 0.75);
        assert_eq!(plugin_slider_at(size.clone(), 0.5), 1.0);
        assert_eq!(plugin_slider_at(size.clone(), 1.0), 1.25);
        // the nearest step, from anywhere along it
        assert_eq!(plugin_slider_at(size.clone(), 0.51), 1.0);
        assert_eq!(plugin_slider_at(size.clone(), 0.56), 1.05);
        assert_eq!(plugin_slider_along(size.clone(), 1.0), 0.5);
        // every step exactly the number it reads as, and back to where it was
        let steps = [0.75, 0.8, 0.85, 0.9, 0.95, 1.0, 1.05, 1.1, 1.15, 1.2, 1.25];
        for (i, step) in steps.into_iter().enumerate() {
            let along = i as f32 / 10.0;
            assert_eq!(plugin_slider_at(size.clone(), along), step);
            assert!((plugin_slider_along(size.clone(), step) - along).abs() < 1e-5, "{along}");
        }
    }

    #[test]
    fn a_slider_reads_as_its_step_needs() {
        let size = size_slider();
        assert_eq!(plugin_slider_shown(size.clone(), 0.85), "85%");
        assert_eq!(plugin_slider_shown(size, 1.0), "100%");
        let PluginControl::Slider { control } = stranger().controls[1].clone() else { panic!() };
        assert_eq!(plugin_slider_shown(control.clone(), 2.5), "2.5 px");
        let hundredths = PluginSlider { step: 0.05, ..control };
        assert_eq!(plugin_slider_shown(hundredths, 2.25), "2.25 px");
    }

    #[test]
    fn a_slider_from_below_0_reads_0_at_0_without_a_sign() {
        // a filter layer's Hue, -100% to 100% in steps of 2: its 0 lands a hair under 0
        let hue = PluginSlider { least: -1.0, most: 1.0, step: 0.02, starts: 0.0, ..size_slider() };
        let zero = hue.stepped(0.0);
        assert!(zero <= 0.0 && zero.abs() < 1e-6, "{zero}");
        assert_eq!(plugin_slider_shown(hue.clone(), zero), "0%");
        assert_eq!(plugin_slider_shown(hue.clone(), -0.0), "0%");
        assert_eq!(plugin_slider_shown(hue.clone(), plugin_slider_at(hue.clone(), 0.5)), "0%");
        // anything that reads as a number still has its sign
        assert_eq!(plugin_slider_shown(hue.clone(), hue.stepped(-0.02)), "-2%");
        assert_eq!(plugin_slider_shown(hue.clone(), hue.stepped(0.58)), "58%");
        assert_eq!(plugin_slider_shown(hue.clone(), -1.0), "-100%");
        // in tenths too
        let tenths = PluginSlider { step: 0.001, ..hue };
        assert_eq!(plugin_slider_shown(tenths.clone(), -0.0004), "0.0%");
        assert_eq!(plugin_slider_shown(tenths, -0.0006), "-0.1%");
    }

    #[test]
    fn a_slider_can_read_as_a_word_at_its_least() {
        let gap = gap_slider();
        assert_eq!(plugin_slider_shown(gap.clone(), 0.0), "Off");
        assert_eq!(plugin_slider_shown(gap.clone(), 1.0), "1 px");
        assert_eq!(plugin_slider_shown(gap.clone(), 32.0), "32 px");
        assert_eq!(plugin_slider_at(gap.clone(), 0.5), 16.0);
        assert_eq!(plugin_slider_along(gap, 8.0), 0.25);
    }

    #[test]
    fn a_slider_can_name_each_step() {
        let names = ["By brush", "Eraser", "Pencil"].map(str::to_owned).to_vec();
        let sound = PluginSlider { least: 0.0, most: 2.0, step: 1.0, at_least: None, names, ..gap_slider() };
        assert_eq!(plugin_slider_shown(sound.clone(), 0.0), "By brush");
        assert_eq!(plugin_slider_shown(sound.clone(), 1.2), "Eraser");
        assert_eq!(plugin_slider_shown(sound, 2.0), "Pencil");
    }

    #[test]
    fn a_slider_named_at_every_step_is_a_choice() {
        let names = ["Eraser", "Pencil", "Pen"].map(str::to_owned).to_vec();
        let sound = PluginSlider { least: 1.0, most: 3.0, step: 1.0, at_least: None, names, ..gap_slider() };
        assert_eq!(plugin_slider_choices(sound.clone()), ["Eraser", "Pencil", "Pen"]);
        assert_eq!(plugin_slider_chosen(sound.clone(), 1.0), 0);
        assert_eq!(plugin_slider_chosen(sound.clone(), 3.0), 2);
        assert_eq!(plugin_slider_chosen(sound.clone(), 9.0), 2);
        assert_eq!(plugin_slider_choice(sound.clone(), 1), 2.0);
        assert_eq!(plugin_slider_choice(sound.clone(), 7), 3.0);
        // a step left unnamed, numbers, or a range of one: a slider still
        let short = PluginSlider { most: 4.0, ..sound.clone() };
        assert!(plugin_slider_choices(short).is_empty());
        assert!(plugin_slider_choices(gap_slider()).is_empty());
        let one = PluginSlider { most: 1.0, ..sound.clone() };
        assert!(plugin_slider_choices(one).is_empty());
        let flat = PluginSlider { step: 0.0, ..sound };
        assert!(plugin_slider_choices(flat).is_empty());
    }

    #[test]
    fn a_count_is_kept_as_a_whole_number() {
        assert!(plugin_slider_whole(gap_slider()));
        assert!(!plugin_slider_whole(size_slider()));
    }

    #[test]
    fn a_key_nothing_has_is_left_alone() {
        let plugins = Plugins::new();
        plugins.listed().files.push(stranger());
        let before = plugins.plugins();
        assert!(!plugins.set_switch("nobody".to_owned(), true));
        assert_eq!(plugins.set_slider("nobody".to_owned(), 1.0), None);
        plugins.read_kept("nobody".to_owned(), "true".to_owned());
        plugins.read_kept("nobody.on".to_owned(), "false".to_owned());
        assert!(!plugins.set_on("nobody".to_owned(), true));
        assert_eq!(plugins.remove("nobody".to_owned()), None);
        // nor does a value of the wrong kind reach a control
        assert!(!plugins.set_switch("someone.stranger.slider".to_owned(), true));
        assert_eq!(plugins.set_slider("someone.stranger.slider".to_owned(), f32::NAN), None);
        assert_eq!(plugins.plugins(), before);
    }

    #[test]
    fn a_slider_described_badly_panics_nowhere() {
        let bad = |least, most, step| PluginSlider {
            key: "bad".to_owned(),
            label: String::new(),
            least,
            most,
            step,
            value: 0.0,
            starts: 0.0,
            shown_times: 1.0,
            unit: String::new(),
            at_least: None,
            names: Vec::new(),
            live: false,
        };
        for slider in [bad(5.0, 1.0, 1.0), bad(1.0, 1.0, 0.0), bad(0.0, 1.0, -1.0), bad(f32::NAN, f32::NAN, f32::NAN)] {
            let _ = slider.stepped(0.5);
            let _ = plugin_slider_at(slider.clone(), f32::NAN);
            let _ = plugin_slider_along(slider.clone(), 0.5);
            let _ = plugin_slider_shown(slider, 0.5);
        }
    }

    #[test]
    fn a_plugin_nothing_answers_to_shows_and_does_nothing() {
        let plugins = Plugins::new();
        plugins.listed().files.push(stranger());
        assert_eq!(plugins.plugins().len(), 1);
        // its values change and are kept like any other's, and touch nothing else
        assert!(plugins.set_switch("someone.stranger.switch".to_owned(), true));
        assert_eq!(plugins.set_slider("someone.stranger.slider".to_owned(), 3.3), Some(3.5));
    }

    #[test]
    fn a_filter_is_imported_kept_through_a_restart_and_removed() {
        let dir = store("filter");
        let plugins = at(&dir);
        assert!(filters(&plugins).is_empty());
        // it shows at once, and runs nothing
        assert_eq!(plugins.install(filter_file(&filter("org.example.filter"))), None);
        let card = filters(&plugins).remove(0);
        assert_eq!(card.source, PluginSource::File);
        assert_eq!((card.version.as_str(), card.publisher.as_str()), ("1.0.0", "Someone"));
        assert_eq!(card.problem, None);
        assert!(card.on && card.experimental);
        assert!(card.controls.is_empty(), "its own are on its panel");
        assert_eq!(card.on_key, "org.example.filter.on");
        assert!(dir_holds(&dir, "org.example.filter/1.0.0/filter.wgsl"));
        // through a restart
        let next = at(&dir);
        assert_eq!(filters(&next).len(), 1);
        for line in plugins.kept_lines() {
            let (key, value) = line.split_once(' ').unwrap();
            next.read_kept(key.to_owned(), value.to_owned());
        }
        assert_eq!(filters(&next), filters(&plugins));
        // removed: gone at once, no restart, and from the disk
        let keys = next.remove("org.example.filter".to_owned()).unwrap();
        assert!(keys.contains(&"org.example.filter.on".to_owned()));
        assert!(filters(&next).is_empty());
        assert!(!dir_holds(&dir, "org.example.filter"));
        assert!(filters(&at(&dir)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file of a plugin with code and the slider `key`, at `version`.
    fn slider_file(id: &str, key: &str, version: &str) -> Vec<u8> {
        let module = position_module(r#"(func (export "procraft_control") (param i32 f32))"#, "(i64.const 0)");
        let mut manifest = coded(id, &module);
        manifest["version"] = json!(version);
        manifest["controls"] = json!([{
            "key": key, "type": "slider", "label": "Cut-off", "least": 0, "most": 20, "step": 0.25, "starts": 3
        }]);
        pack(&manifest, &[(crate::plugin_run::MODULE, &module)])
    }

    #[test]
    fn a_plugin_can_t_reach_a_moved_value_without_saying_so() {
        let dir = store("collide");
        let plugins = at(&dir);
        plugins.read_kept("fill_gap".to_owned(), "11".to_owned());
        assert_eq!(plugins.install(slider_file("org.example.plugin", "fill_gap", "1.0.0")), None);
        let key = "org.example.plugin.fill_gap";
        let value = |plugins: &Plugins| match plugins.control(key) {
            Some(PluginControl::Slider { control }) => control.value,
            other => panic!("{other:?}"),
        };
        assert_eq!(value(&plugins), 3.0, "its own value is its own");
        assert!(plugins.kept_lines().contains(&"fill_gap 11".to_owned()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_filter_is_enabled_and_disabled_and_it_is_kept() {
        let dir = store("enabled");
        let plugins = at(&dir);
        plugins.install(filter_file(&filter("org.example.filter")));
        assert!(plugins.set_on("org.example.filter".to_owned(), false));
        assert!(!filters(&plugins)[0].on);
        assert!(plugins.kept_lines().contains(&"org.example.filter.on false".to_owned()));
        let next = at(&dir);
        assert!(filters(&next)[0].on, "as imported");
        next.read_kept("org.example.filter.on".to_owned(), "false".to_owned());
        assert!(!filters(&next)[0].on);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_offer_says_plainly_what_it_is() {
        let plugins = at(&store("offer"));
        let offer = plugins.inspect(filter_file(&filter("org.example.filter")));
        assert_eq!(offer.refused, None);
        assert_eq!(offer.name, "A filter");
        assert!(offer.message.contains("Version 1.0.0"), "{offer:?}");
        assert!(offer.message.contains("Someone (not checked)"), "{offer:?}");
        assert!(offer.message.contains("only the layer you use it on"), "{offer:?}");
        // nothing was kept by asking
        assert!(filters(&plugins).is_empty());
        let refused = plugins.inspect(b"nothing".to_vec());
        assert_eq!(refused.refused.as_deref(), Some("This isn't a ProCraft plugin."));
        assert!(refused.name.is_empty());
    }

    #[test]
    fn the_offer_of_a_plugin_says_what_it_can_and_can_t_do() {
        let module = position_module("", "(i64.const 0)");
        let mut manifest = coded("org.example.plugin", &module);
        manifest["name"] = json!("Offset");
        let plugins = at(&store("plugin-offer"));
        let offer = plugins.inspect(pack(&manifest, &[(crate::plugin_run::MODULE, &module)]));
        assert_eq!(offer.refused, None);
        assert_eq!(offer.name, "Offset");
        assert!(offer.message.contains("Someone (not checked)"), "{offer:?}");
        assert!(offer.message.contains("where your strokes land"), "{offer:?}");
        assert!(offer.message.contains("can't reach your files"), "{offer:?}");
        assert!(plugins.plugins().is_empty());
    }

    /// A plugin taking over the moved eyedropper setting, as a file.
    fn eyedropper_file(id: &str) -> Vec<u8> {
        let text = r#"(module (func (export "procraft_control") (param i32 f32))
            (func (export "procraft_pick_layer") (result i32) (i32.const 0)))"#;
        let module = wat::parse_str(text).unwrap();
        let mut manifest = coded(id, &module);
        manifest["hooks"] = json!(["pick_layer"]);
        manifest["controls"] = json!([
            { "key": "layer", "type": "switch", "label": "Layer", "keeps": "eyedropper_layer" },
        ]);
        pack(&manifest, &[(crate::plugin_run::MODULE, &module)])
    }

    fn switch_on(plugins: &Plugins, key: &str) -> Option<bool> {
        match plugins.control(key) {
            Some(PluginControl::Switch { control }) => Some(control.on),
            _ => None,
        }
    }

    #[test]
    fn someone_who_changed_a_moved_setting_is_told_once_and_keeps_it() {
        let dir = store("moved");
        let plugins = at(&dir);
        plugins.read_kept("eyedropper_layer".to_owned(), "true".to_owned());
        let said = plugins.take_moved_notice().unwrap();
        assert!(said.starts_with("Eyedropper picks from layer now comes as a plugin."), "{said}");
        // once, in this run and the next
        assert_eq!(plugins.take_moved_notice(), None);
        let next = at(&dir);
        next.read_kept("eyedropper_layer".to_owned(), "true".to_owned());
        assert_eq!(next.take_moved_notice(), None);
        // nothing is installed for them, and the value goes on being kept
        assert!(next.plugins().is_empty());
        assert!(next.kept_lines().contains(&"eyedropper_layer true".to_owned()));
        // the plugin picks it up on import, and keeps it where it was, through a restart
        assert_eq!(next.install(eyedropper_file("org.example.pick")), None);
        assert_eq!(switch_on(&next, "eyedropper_layer"), Some(true));
        let lines = next.kept_lines();
        assert_eq!(lines.iter().filter(|line| line.starts_with("eyedropper_layer ")).count(), 1, "{lines:?}");
        let again = at(&dir);
        again.read_kept("eyedropper_layer".to_owned(), "true".to_owned());
        assert_eq!(switch_on(&again, "eyedropper_layer"), Some(true));
        // removed, the value goes with it
        let keys = again.remove("org.example.pick".to_owned()).unwrap();
        assert!(keys.contains(&"eyedropper_layer".to_owned()), "{keys:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_moved_setting_nobody_changed_says_nothing() {
        let dir = store("moved-unchanged");
        let plugins = at(&dir);
        plugins.read_kept("eyedropper_layer".to_owned(), "false".to_owned());
        assert_eq!(plugins.take_moved_notice(), None);
        assert_eq!(at(&dir).take_moved_notice(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_moved_setting_with_its_plugin_here_says_nothing() {
        let dir = store("moved-held");
        let plugins = at(&dir);
        assert_eq!(plugins.install(eyedropper_file("org.example.pick")), None);
        let next = at(&dir);
        next.read_kept("eyedropper_layer".to_owned(), "true".to_owned());
        assert_eq!(next.take_moved_notice(), None);
        assert_eq!(switch_on(&next, "eyedropper_layer"), Some(true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_moved_setting_has_one_holder() {
        let dir = store("moved-clash");
        let plugins = at(&dir);
        assert_eq!(plugins.install(eyedropper_file("org.example.pick")), None);
        let said = plugins.inspect(eyedropper_file("org.example.other")).refused;
        assert_eq!(said.as_deref(), Some("Another plugin already holds a setting this one needs."));
        assert_eq!(plugins.install(eyedropper_file("org.example.other")), said);
        // and a newer version of the holder is still welcome
        assert_eq!(plugins.install(eyedropper_file("org.example.pick")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_with_code_and_a_filter_share_one_list() {
        let dir = store("pages");
        let plugins = at(&dir);
        let module = position_module("", "(i64.const 0)");
        let file = pack(&coded("org.example.plugin", &module), &[(crate::plugin_run::MODULE, &module)]);
        assert_eq!(plugins.install(file), None);
        assert_eq!(plugins.install(filter_file(&filter("org.example.filter"))), None);
        let cards = plugins.plugins();
        assert_eq!(cards.len(), 2);
        assert!(cards[0].holds == [PluginPart::Hooks] && cards[0].source == PluginSource::File);
        assert!(!cards[0].on && cards[0].problem.is_none(), "it starts off: {:?}", cards[0]);
        assert_eq!(cards[0].on_key, "org.example.plugin.on");
        assert!(cards[1].holds == [PluginPart::Filter] && cards[1].on, "it only adds, so it starts on");
        assert_eq!(filters(&plugins).len(), 1);
        // and through a restart
        let next = at(&dir);
        assert_eq!((next.plugins().len(), filters(&next).len()), (2, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    const BOTH: &str = "org.example.both";

    /// A file holding a filter and the code `answer` for every sample, with the filter's controls.
    fn both_file(answer: &str) -> Vec<u8> {
        let module = position_module(r#"(func (export "procraft_control") (param i32 f32))"#, answer);
        let mut manifest = filter(BOTH);
        manifest["holds"] = json!(["filter", "hooks"]);
        manifest["hooks"] = json!(["position"]);
        let sha256 = crate::plugin_file::sha256;
        manifest["files"] = json!([
            { "path": "filter.wgsl", "sha256": sha256(SHADER), "size": SHADER.len() },
            { "path": crate::plugin_run::MODULE, "sha256": sha256(&module), "size": module.len() },
        ]);
        pack(&manifest, &[("filter.wgsl", SHADER), (crate::plugin_run::MODULE, &module)])
    }

    /// Where a sample ends up: (5, 6) with no code on it, the corner with.
    const UNTOUCHED: (f32, f32) = (5.0, 6.0);
    const CORNER: (f32, f32) = (0.0, 0.0);

    fn landing(plugins: &Plugins) -> (f32, f32) {
        plugins.hooks().position(UNTOUCHED.0, UNTOUCHED.1)
    }

    #[test]
    fn a_plugin_holding_a_filter_and_code_is_one_card_with_one_on_off() {
        let dir = store("both");
        let plugins = at(&dir);
        assert_eq!(plugins.install(both_file("(i64.const 0)")), None);
        let cards = plugins.plugins();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].holds, [PluginPart::Filter, PluginPart::Hooks]);
        assert_eq!(cards[0].controls.len(), 2, "its code has controls, so they are on its card");
        // as anything that changes what is there, it starts off, both parts
        assert!(!cards[0].on);
        assert!(plugins.filters().is_empty());
        assert_eq!(landing(&plugins), UNTOUCHED);
        // on, both are
        assert!(plugins.set_on(BOTH.to_owned(), true));
        assert_eq!(plugins.filters().len(), 1);
        assert_eq!(landing(&plugins), CORNER);
        // what is set on the card is its code's: the filter's panel opens as it starts
        assert_eq!(plugins.set_slider(format!("{BOTH}.cut"), 0.25), Some(0.25));
        let opened = plugins.filter_to_open(BOTH).map_err(|why| why.to_string()).map(|open| open.controls);
        assert_eq!(opened.unwrap()[0].number(), 0.5);
        // through a restart, with its flag kept
        let lines = plugins.kept_lines();
        let next = at(&dir);
        for line in &lines {
            let (key, value) = line.split_once(' ').unwrap();
            next.read_kept(key.to_owned(), value.to_owned());
        }
        assert_eq!(next.filters().len(), 1);
        assert_eq!(landing(&next), CORNER);
        // off, neither is, and the app is as it was
        assert!(plugins.set_on(BOTH.to_owned(), false));
        assert!(plugins.filters().is_empty());
        assert_eq!(landing(&plugins), UNTOUCHED);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_is_stopped_whole_whichever_part_stopped() {
        let dir = store("both-stops");
        let plugins = at(&dir);
        assert_eq!(plugins.install(both_file("(i64.const 0)")), None);
        assert!(plugins.set_on(BOTH.to_owned(), true));
        assert_eq!(landing(&plugins), CORNER);
        // its filter stops the driver: its code doesn't run on regardless
        plugins.stop_filter(BOTH, STOPPED_DRIVER);
        assert_eq!(landing(&plugins), UNTOUCHED);
        assert!(plugins.filters().is_empty());
        let card = plugins.plugins().remove(0);
        assert_eq!((card.on, card.problem.as_deref()), (false, Some(STOPPED_DRIVER)));
        // nor through a restart, whatever its flag says
        let next = at(&dir);
        next.read_kept(card.on_key.clone(), "true".to_owned());
        assert_eq!(landing(&next), UNTOUCHED);
        assert!(next.filters().is_empty());
        // asked to, both come back
        assert!(plugins.set_on(BOTH.to_owned(), true));
        assert_eq!(landing(&plugins), CORNER);
        assert_eq!(plugins.filters().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
        // its code stops on a sample, an answer that isn't a number: its filter goes with it
        let dir = store("both-stops-code");
        let plugins = at(&dir);
        assert_eq!(plugins.install(both_file("(i64.const 0x7fc00000)")), None);
        assert!(plugins.set_on(BOTH.to_owned(), true));
        assert_eq!(plugins.filters().len(), 1);
        assert_eq!(landing(&plugins), UNTOUCHED, "what it was handed goes on as it came");
        assert!(plugins.filters().is_empty());
        assert!(!plugins.plugins()[0].on);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_offer_of_both_says_all_it_can_do_and_no_more() {
        let plugins = at(&store("both-offer"));
        let offer = plugins.inspect(both_file("(i64.const 0)"));
        assert_eq!(offer.refused, None);
        assert!(offer.message.contains("adds a filter to Adjustments"), "{offer:?}");
        assert!(offer.message.contains("where your strokes land"), "{offer:?}");
        assert!(offer.message.contains("can't reach your files or the internet"), "{offer:?}");
        assert!(!offer.message.contains("your layers"), "its filter changes one: {offer:?}");
    }

    /// The files in the folder `PROCRAFT_PLUGIN_DIR`, as the packer leaves them: each is taken, and
    /// turns on and off. `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn packed_files_are_taken_and_turn_on_and_off() {
        let Ok(folder) = std::env::var("PROCRAFT_PLUGIN_DIR") else { return };
        let dir = store("packed");
        let plugins = at(&dir);
        let mut files = 0;
        for file in std::fs::read_dir(folder).unwrap() {
            let bytes = std::fs::read(file.unwrap().path()).unwrap();
            let offer = plugins.inspect(bytes.clone());
            println!("{}: {}", offer.name, offer.message.replace('\n', " | "));
            assert_eq!(offer.refused, None);
            assert_eq!(plugins.install(bytes), None, "{}", offer.name);
            files += 1;
        }
        let cards = plugins.plugins();
        assert_eq!(cards.len(), files);
        for card in cards {
            assert!(plugins.set_on(card.id.clone(), true));
            let on = plugins.plugins().into_iter().find(|p| p.id == card.id).unwrap();
            assert!(on.on && on.problem.is_none(), "{on:?}");
            assert!(plugins.set_on(card.id.clone(), false));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_keeps_what_was_set_and_an_older_file_is_refused() {
        let dir = store("update");
        let plugins = at(&dir);
        let id = "org.example.plugin";
        assert_eq!(plugins.install(slider_file(id, "cut", "1.2.0")), None);
        plugins.set_slider("org.example.plugin.cut".to_owned(), 0.25);
        // older: refused, on the screen and if asked anyway
        let older = slider_file(id, "cut", "1.0.0");
        let said = Some("A newer version of this plugin is already imported.".to_owned());
        assert_eq!(plugins.inspect(older.clone()).refused, said);
        assert_eq!(plugins.install(older), said);
        // newer: takes over, what was set stays
        assert_eq!(plugins.install(slider_file(id, "cut", "1.3.0")), None);
        assert_eq!(plugins.plugins()[0].version, "1.3.0");
        let cut = plugins.control("org.example.plugin.cut");
        assert!(matches!(cut, Some(PluginControl::Slider { control }) if (control.value - 0.25).abs() < 1e-6));
        assert_eq!(at(&dir).plugins().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_keeps_what_the_plugin_keeps_and_the_files_picked_for_it() {
        let dir = store("update-kept");
        let plugins = at(&dir);
        let id = "org.example.plugin";
        assert_eq!(plugins.install(slider_file(id, "cut", "1.2.0")), None);
        let store = std::path::Path::new(&dir);
        assert_eq!(plugin_file::read_kept(store, id), None);
        assert!(plugin_file::write_kept(store, id, b"bound"));
        let (picked, _) = plugin_file::chosen_paths(store, id, "sound");
        std::fs::create_dir_all(picked.parent().unwrap()).unwrap();
        std::fs::write(&picked, b"RIFF").unwrap();
        assert_eq!(plugins.install(slider_file(id, "cut", "1.3.0")), None);
        assert_eq!(plugin_file::read_kept(store, id).as_deref(), Some(&b"bound"[..]));
        assert!(picked.is_file(), "the file picked stays");
        // a start reads it as it was, the files beside its version passed over
        let next = at(&dir);
        assert_eq!(next.plugins().len(), 1);
        assert_eq!(next.plugins()[0].version, "1.3.0");
        // too much isn't kept, nor anything for a plugin that isn't there
        assert!(!plugin_file::write_kept(store, id, &vec![0; crate::plugin_run::MOST_KEPT + 1]));
        assert!(!plugin_file::write_kept(store, "org.example.gone", b"x"));
        assert_eq!(plugin_file::read_kept(store, id).as_deref(), Some(&b"bound"[..]));
        // it goes with the plugin
        assert!(plugins.remove(id.to_owned()).is_some());
        assert_eq!(plugin_file::read_kept(store, id), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refused_file_leaves_nothing_behind() {
        let dir = store("refused");
        let plugins = at(&dir);
        let mut asks = filter("org.example.filter");
        asks["permissions"] = json!(["network"]);
        let said = plugins.install(filter_file(&asks));
        assert_eq!(said.as_deref(), Some("This plugin asks for permissions ProCraft doesn't give."));
        assert!(filters(&plugins).is_empty());
        assert!(!dir_holds(&dir, "org.example.filter"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_nowhere_to_keep_it_a_file_isn_t_taken() {
        let file = filter_file(&filter("org.example.filter"));
        assert_eq!(Plugins::new().install(file).as_deref(), Some("This plugin couldn't be saved."));
    }

    const COLOUR_TOOLS: &str = "io.github.artcraftandroid.colour-tools";
    const INTERFACE_LOOK: &str = "io.github.artcraftandroid.interface-look";
    const VIEW_TOOLS: &str = "io.github.artcraftandroid.view-tools";
    const BRUSH_EXTRAS: &str = "io.github.artcraftandroid.brush-extras";
    const LAYER_TOOLBOX: &str = "io.github.artcraftandroid.layer-toolbox";
    const INFINITE: &str = "io.github.artcraftandroid.infinite-plugins";
    const EDIT_LAYOUT: &str = "io.github.artcraftandroid.edit-layout";
    /// The features the plugins these named once are now, by the plugin holding them.
    const EFFECTS: &str = LAYER_TOOLBOX;
    const COLORDROP: &str = COLOUR_TOOLS;
    const INTERFACE: &str = INTERFACE_LOOK;
    const LAYER_TOOLS: &str = LAYER_TOOLBOX;
    const LAYER_FILLS: &str = LAYER_TOOLBOX;
    const TRIANGLE: &str = COLOUR_TOOLS;
    const TOUCH: &str = BRUSH_EXTRAS;

    /// The app's plugins kept in `dir`, the feature `feature` of `id` on (the plugin not yet).
    fn featured_app(dir: &str, id: &str, feature: &str) -> Arc<Plugins> {
        let plugins = app_at(dir);
        assert!(plugins.set_switch(format!("{id}.{feature}"), true), "{id}.{feature}");
        plugins
    }

    /// Each feature of each plugin of features in `plugins` on.
    fn all_features_on(plugins: &Plugins) {
        for card in plugins.plugins() {
            for feature in card.features {
                assert!(plugins.set_switch(feature.control.key, true));
            }
        }
    }

    /// The app's plugins kept in `dir`, what it comes with added.
    fn app_at(dir: &str) -> Arc<Plugins> {
        Plugins::with_bundled(dir.to_owned())
    }

    /// The app's plugins kept in `dir`, as an app that comes with `files` adds them.
    fn app_with(dir: &str, files: &[(&str, Vec<u8>)]) -> Arc<Plugins> {
        let plugins = Plugins::kept_in(Some(PathBuf::from(dir)));
        let files: Vec<(&str, &[u8])> = files.iter().map(|(name, bytes)| (*name, bytes.as_slice())).collect();
        plugins.add_bundled(&files);
        plugins
    }

    /// The file of `name` that the app comes with, said to be `version`.
    fn bundled_file(name: &str, version: &str) -> Vec<u8> {
        let (_, bytes) = crate::bundled::BUNDLED.iter().find(|(n, _)| *n == name).unwrap();
        let package = plugin_file::read_package(bytes).unwrap();
        let mut manifest: Value = serde_json::from_slice(package.manifest_text()).unwrap();
        manifest["version"] = json!(version);
        let listed = package.manifest.shape.filters.iter().map(|f| f.code.as_str());
        let kept =
            [plugin_file::FILTER_CODE, crate::plugin_run::MODULE, plugin_file::PICTURE_CODE, plugin_file::BRUSH_SET];
        let inside = kept.into_iter().chain(listed);
        let files: Vec<(&str, &[u8])> = inside.filter_map(|path| Some((path, package.file(path)?))).collect();
        pack(&manifest, &files)
    }

    /// Every file the app comes with, as it comes.
    fn all_bundled() -> Vec<(&'static str, Vec<u8>)> {
        crate::bundled::BUNDLED.iter().map(|(name, bytes)| (*name, bytes.to_vec())).collect()
    }

    fn card(plugins: &Plugins, id: &str) -> PluginInfo {
        plugins.plugins().into_iter().find(|p| p.id == id).unwrap()
    }

    fn has(plugins: &Plugins, id: &str) -> bool {
        plugins.plugins().iter().any(|p| p.id == id)
    }

    fn slider_value(plugins: &Plugins, key: &str) -> f32 {
        match plugins.control(key) {
            Some(PluginControl::Slider { control }) => control.value,
            other => panic!("{other:?}"),
        }
    }

    fn read_lines(plugins: &Plugins, lines: &[String]) {
        for line in lines {
            let (key, value) = line.split_once(' ').unwrap();
            plugins.read_kept(key.to_owned(), value.to_owned());
        }
    }

    #[test]
    fn the_first_start_adds_what_the_app_comes_with_as_files_all_off() {
        let dir = store("bundled");
        let plugins = app_at(&dir);
        let cards = plugins.plugins();
        assert_eq!(cards.len(), crate::bundled::BUNDLED.len());
        for card in &cards {
            // as an import: a card like any other, kept in the store
            assert_eq!(card.source, PluginSource::File, "{card:?}");
            assert!(!card.on && card.problem.is_none(), "{card:?}");
            assert!(dir_holds(&dir, &format!("{}/{}", card.id, card.version)), "{}", card.id);
        }
        // off, the app is as it was
        assert!(plugins.filters().is_empty());
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 1.0);
        assert_eq!(landing(&plugins), UNTOUCHED);
        assert!(!plugins.hooks().pick_layer());
        assert!(!plugins.hooks().paste_mask() && !plugins.hooks().fill_mask());
        assert_eq!(plugins.take_moved_notice(), None);
        // and so on the next start, the filters too, with nothing kept for them
        let next = app_at(&dir);
        assert_eq!(next.plugins(), cards);
        assert!(next.filters().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_artwork_of_a_plugin_s_kind_takes_touches_draws_saves_and_loads() {
        let dir = store("canvas-kind");
        let plugins = Plugins::with_store(dir.clone());
        let id = "org.example.dots";
        let file = crate::plugin_file::tests::dots_file(id, serde_json::json!([{ "key": "dots", "label": "Dots" }]));
        assert_eq!(plugins.install(file), None);
        assert!(plugins.kinds().is_empty(), "off until turned on");
        assert!(plugins.set_on(id.to_owned(), true));
        let kind = format!("{id}.dots");
        let dots = crate::PluginKind { key: kind.clone(), label: "Dots".to_owned(), badge: None, exports: Vec::new() };
        assert_eq!(plugins.kinds(), [dots]);
        let canvas = plugins.open_canvas(kind.clone(), 40, 30, None).expect("opened");
        assert_eq!((canvas.width(), canvas.height()), (40, 30));
        let red_at = |c: &crate::PluginCanvas, x: usize, y: usize| {
            let rgba = c.picture();
            rgba[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4] == [255, 0, 0, 255]
        };
        assert!(!red_at(&canvas, 20, 15), "nothing yet");
        assert!(canvas.touch(0, 20.0, 15.0, 1.0));
        assert!(red_at(&canvas, 20, 15) && !red_at(&canvas, 2, 2));
        // what it saved goes back, as undo does
        let saved = canvas.save().expect("saved");
        assert_eq!(saved.len(), 8);
        assert!(canvas.touch(0, 6.0, 6.0, 1.0));
        assert!(red_at(&canvas, 6, 6) && !red_at(&canvas, 20, 15));
        assert!(canvas.load(saved.clone()));
        assert!(red_at(&canvas, 20, 15) && !red_at(&canvas, 6, 6));
        // and opens as it was
        let again = plugins.open_canvas(kind.clone(), 40, 30, Some(saved)).expect("reopened");
        assert!(red_at(&again, 20, 15));
        // a kind no plugin on makes opens nothing
        assert!(plugins.open_canvas(format!("{id}.other"), 40, 30, None).is_none());
        assert!(plugins.set_on(id.to_owned(), false));
        assert!(plugins.open_canvas(kind, 40, 30, None).is_none());
        assert!(plugins.kinds().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_whose_artwork_crashes_goes_off_and_says_why() {
        let dir = store("canvas-crash");
        let plugins = Plugins::with_store(dir.clone());
        let id = "org.example.dots";
        let touch = "(f32.store (i32.const 0) (local.get 1)) (f32.store (i32.const 4) (local.get 2)) (i32.const 1))";
        let text = crate::plugin_run::tests::DOTS.replace(touch, "(unreachable))");
        assert_ne!(text, crate::plugin_run::tests::DOTS);
        let module = wat::parse_str(text).unwrap();
        let mut manifest = crate::plugin_file::tests::coded(id, &module);
        manifest["compat"]["procraft"]["interface"] = serde_json::json!(2);
        manifest["hooks"] = serde_json::json!([]);
        manifest["kinds"] = serde_json::json!([{ "key": "dots", "label": "Dots" }]);
        let file = crate::plugin_file::tests::pack(&manifest, &[(crate::plugin_run::MODULE, &module)]);
        assert_eq!(plugins.install(file), None);
        assert!(plugins.set_on(id.to_owned(), true));
        let canvas = plugins.open_canvas(format!("{id}.dots"), 40, 30, None).unwrap();
        assert!(!canvas.touch(0, 5.0, 5.0, 1.0));
        assert_eq!(canvas.problem().as_deref(), Some("Switched off: it crashed."));
        let card = plugins.plugins().into_iter().find(|p| p.id == id).unwrap();
        assert!(!card.on);
        assert_eq!(card.problem.as_deref(), Some("Switched off: it crashed."));
        // and it takes nothing more
        assert!(!canvas.touch(0, 6.0, 6.0, 1.0));
        assert!(canvas.save().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_the_app_added_runs_as_any_imported() {
        let dir = store("bundled-run");
        let plugins = app_at(&dir);
        all_features_on(&plugins);
        for card in plugins.plugins() {
            assert!(plugins.set_on(card.id.clone(), true));
            let on = self::card(&plugins, &card.id);
            assert!(on.on && on.problem.is_none(), "{on:?}");
            if card.holds.contains(&PluginPart::Hooks) {
                assert!(plugins.hooks().is_on(&card.id), "{}", card.name);
            }
        }
        // each filter there while its switch is on, and each opens
        assert!(plugins.filters().is_empty(), "each waits for its switch");
        for switch in ["extract_lines", "outline", "posterise"] {
            assert!(plugins.set_switch(format!("{EFFECTS}.{switch}"), true));
        }
        assert!(plugins.set_switch(format!("{LAYER_FILLS}.paper"), true));
        let filters = plugins.filters();
        let names: Vec<&str> = filters.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Paper texture", "Extract lines", "Outline", "Posterise"]);
        for filter in &filters {
            let opened = plugins.filter_to_open(&filter.id).map(|open| (open.name, open.controls));
            assert_eq!(opened, Ok((filter.name.clone(), filter.controls.clone())));
        }
        // each does what it says, by its controls, its sliders doing nothing with their switch off
        assert_eq!(plugins.set_slider(format!("{TOUCH}.x"), 10.0), Some(10.0));
        assert_eq!(landing(&plugins), (UNTOUCHED.0 + 10.0, UNTOUCHED.1));
        assert!(plugins.set_slider("interface_size".to_owned(), 0.9).is_some());
        assert!(plugins.set_switch(format!("{INTERFACE}.clear"), true));
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 1.0);
        assert!(!plugins.keep_clear());
        assert!(plugins.set_switch(format!("{INTERFACE}.size_on"), true));
        assert!((plugins.interface_scale(1600.0, 1000.0) - 0.9).abs() < 1e-6);
        assert!(plugins.keep_clear());
        assert!(plugins.set_switch(format!("{INTERFACE}.clear"), false));
        assert!(!plugins.keep_clear());
        assert!((plugins.interface_scale(1600.0, 1000.0) - 0.9).abs() < 1e-6, "the slider is its own");
        assert!(plugins.set_switch("eyedropper_layer".to_owned(), true));
        assert!(plugins.hooks().pick_layer());
        assert!(plugins.set_switch("paste_into_mask".to_owned(), true));
        assert!(plugins.hooks().paste_mask());
        assert!(!plugins.hooks().opacity_together());
        assert!(plugins.set_switch(format!("{LAYER_TOOLS}.opacity"), true));
        assert!(plugins.hooks().opacity_together());
        assert_eq!(plugins.set_slider("fill_gap".to_owned(), 4.0), Some(4.0));
        assert!(!plugins.hooks().fill_mask());
        assert!(plugins.set_switch(format!("{COLORDROP}.gap_closing"), true));
        assert!(plugins.hooks().fill_mask() && !plugins.hooks().fill_all_layers());
        assert!(plugins.set_switch(format!("{COLORDROP}.all_layers"), true));
        assert!(plugins.hooks().fill_all_layers());
        // the accent, at the sliders' start the blue it was, then red, then grey
        assert_eq!(plugins.accent_colour(), None);
        assert!(plugins.set_switch(format!("{INTERFACE}.colour"), true));
        assert_eq!(plugins.accent_colour(), Some(0x0076FD));
        assert!(plugins.set_slider(format!("{INTERFACE}.hue"), 0.0).is_some());
        assert_eq!(plugins.accent_colour(), Some(0xFD0000));
        assert!(plugins.set_slider(format!("{INTERFACE}.saturation"), 0.0).is_some());
        assert_eq!(plugins.accent_colour(), Some(0xFDFDFD));
        assert!(!plugins.hooks().hide_undo_notice());
        assert!(plugins.set_switch(format!("{INTERFACE}.hide_undo"), true));
        assert!(plugins.hooks().hide_undo_notice());
        // the triangle draws Classic, and says where a touch lands: its right corner is the pure hue
        assert!(plugins.picker_on());
        let corner = plugins.picker_pick(0.933, 0.5).unwrap();
        assert!(corner.iter().all(|share| (share - 1.0).abs() < 1e-3), "{corner:?}");
        // the hue slider has the hue until the ring does, then a touch on the ring picks it
        assert_eq!(plugins.picker_hue_point(0.0), None);
        assert_eq!(plugins.picker_hue(0.97, 0.5), None);
        assert!(plugins.set_switch(format!("{TRIANGLE}.ring"), true));
        let knob = plugins.picker_hue_point(0.25).unwrap();
        assert!((knob[0] - 0.5).abs() < 1e-3 && (knob[1] - 0.05).abs() < 1e-3, "{knob:?}");
        assert!(plugins.picker_hue(0.97, 0.5).is_some_and(|h| !(1e-3..=0.999).contains(&h)));
        assert_eq!(plugins.picker_hue(0.5, 0.5), None, "the triangle's middle");
        // the floating window: smaller with Interface size's slider, and the picker alone with its switch
        assert!(!plugins.floating_picker_only());
        assert!(plugins.set_switch(format!("{TRIANGLE}.picker_only"), true));
        assert!(plugins.floating_picker_only());
        // the parts of the interface each on top of the whole, while they're sized separately
        assert!(plugins.set_slider(format!("{INTERFACE}.floating"), 0.5).is_some());
        assert!(plugins.set_slider(format!("{INTERFACE}.top_bar"), 1.5).is_some());
        assert_eq!(plugins.interface_part_scale(InterfacePart::Floating), 1.0);
        assert!(plugins.set_switch(format!("{INTERFACE}.parts"), true));
        assert_eq!(plugins.interface_part_scale(InterfacePart::Floating), 0.5);
        assert_eq!(plugins.interface_part_scale(InterfacePart::TopBar), 1.5);
        assert_eq!(plugins.interface_part_scale(InterfacePart::Panels), 1.0);
        assert!((plugins.interface_scale(1600.0, 1000.0) - 0.9).abs() < 1e-6, "the whole as it was");
        // the triangle's own switch off: Classic is the app's square again, and the window as it was
        assert!(plugins.set_switch(format!("{TRIANGLE}.triangle"), false));
        assert!(!plugins.picker_on() && !plugins.floating_picker_only());
        assert_eq!(plugins.picker_pick(0.933, 0.5), None);
        assert!(plugins.set_switch(format!("{TRIANGLE}.triangle"), true));
        assert!(plugins.picker_on());
        // kept, so through a restart
        let next = app_at(&dir);
        read_lines(&next, &plugins.kept_lines());
        assert_eq!(next.plugins(), plugins.plugins());
        assert_eq!(landing(&next), (UNTOUCHED.0 + 10.0, UNTOUCHED.1));
        assert_eq!(next.filters().len(), 4);
        assert_eq!(next.accent_colour(), Some(0xFDFDFD));
        // and off again, none does
        for card in plugins.plugins() {
            assert!(plugins.set_on(card.id, false));
        }
        assert!(plugins.filters().is_empty());
        assert_eq!(landing(&plugins), UNTOUCHED);
        assert_eq!(plugins.interface_scale(1600.0, 1000.0), 1.0);
        assert_eq!(plugins.accent_colour(), None);
        assert!(!plugins.picker_on());
        assert!(!plugins.hooks().pick_layer() && !plugins.hooks().paste_mask() && !plugins.hooks().fill_mask());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_card_shows_what_comes_under_a_switch_only_while_it_is_on() {
        let dir = store("bundled-under");
        let plugins = app_at(&dir);
        let labels = |plugins: &Plugins, id: &str| {
            let card = card(plugins, id);
            let controls = card.controls.iter().map(|c| match c {
                PluginControl::Switch { control } => control.label.clone(),
                PluginControl::Slider { control } => control.label.clone(),
                PluginControl::File { control } => control.label.clone(),
                PluginControl::Texture { control } => control.label.clone(),
                PluginControl::Folder { control } => control.label.clone(),
            });
            controls.chain(card.actions.iter().map(|a| a.label.clone())).collect::<Vec<_>>()
        };
        // a feature for each of the plugins it was made of, named as they were
        let features = ["Colour triangle", "Colour labels", "ColorDrop tools"];
        assert_eq!(labels(&plugins, COLOUR_TOOLS), features);
        assert!(plugins.set_switch(format!("{COLOUR_TOOLS}.colordrop"), true));
        let colordrop = ["ColorDrop tools", "ColorDrop fills by all layers", "ColorDrop gap closing"];
        assert_eq!(labels(&plugins, COLOUR_TOOLS)[2..], colordrop);
        assert!(plugins.set_switch(format!("{COLOUR_TOOLS}.gap_closing"), true));
        assert_eq!(labels(&plugins, COLOUR_TOOLS)[2..], [&colordrop[..], &["Gap"]].concat());
        // each a heading on the card, its words under it, what comes under it while it is on
        let colour = card(&plugins, COLOUR_TOOLS);
        let headings: Vec<(&str, bool, usize)> =
            colour.features.iter().map(|f| (f.control.label.as_str(), f.control.on, f.controls.len())).collect();
        assert_eq!(
            headings,
            [("Colour triangle", false, 0), ("Colour labels", false, 0), ("ColorDrop tools", true, 3)]
        );
        assert!(colour.features.iter().all(|f| !f.description.is_empty()));
        let looks = ["Interface", "Slider look", "Canvas surround", "Coloured brush previews"];
        assert_eq!(labels(&plugins, INTERFACE_LOOK), looks);
        assert!(plugins.set_switch(format!("{INTERFACE}.interface"), true));
        let interface = ["Interface size", "Interface colour", "Hide undo notices", "Hide all action notices"];
        assert_eq!(labels(&plugins, INTERFACE_LOOK)[1..5], interface);
        assert!(plugins.set_switch(format!("{INTERFACE}.size_on"), true));
        assert!(plugins.set_switch(format!("{INTERFACE}.colour"), true));
        let all = [
            "Interface size",
            "Size",
            "Size parts separately",
            "UI stays below status bar",
            "Interface colour",
            "Hue",
            "Saturation",
            "Hide undo notices",
            "Hide all action notices",
        ];
        assert_eq!(labels(&plugins, INTERFACE_LOOK)[1..10], all);
        // a switch under a switch under a feature, and what comes under it while all are on
        assert!(plugins.set_switch(format!("{INTERFACE}.parts"), true));
        let parts = ["Floating panels", "Popovers and menus", "Side bar", "Top bar", "Panels"];
        let with_parts: Vec<&str> = all[..3].iter().chain(&parts).chain(&all[3..]).copied().collect();
        assert_eq!(labels(&plugins, INTERFACE_LOOK)[1..15], with_parts);
        assert!(plugins.set_switch(format!("{INTERFACE}.size_on"), false));
        assert_eq!(labels(&plugins, INTERFACE_LOOK)[1..3], ["Interface size", "Interface colour"]);
        // the feature off: none of it, its values as they were
        assert!(plugins.set_switch(format!("{INTERFACE}.interface"), false));
        assert_eq!(labels(&plugins, INTERFACE_LOOK), looks);
        assert!(switch_of(&plugins, &format!("{INTERFACE}.parts")));
        let tools = ["Layer tools", "Opacity of picked layers", "Paste into masks", "Eyedropper picks from layer"];
        assert!(plugins.set_switch(format!("{LAYER_TOOLBOX}.layer_tools"), true));
        assert_eq!(labels(&plugins, LAYER_TOOLBOX)[..4], tools);
        assert!(plugins.set_switch(format!("{LAYER_TOOLBOX}.effects"), true));
        assert!(plugins.set_switch(format!("{EFFECTS}.extract_lines"), true));
        let lines = ["Effects", "Extract lines", "Paper", "Softness", "Keep the colour", "Outline", "Posterise"];
        assert_eq!(labels(&plugins, LAYER_TOOLBOX)[5..12], lines);
        assert_eq!(labels(&plugins, LAYER_TOOLBOX).len(), 12, "Filter layers are Infinite Plugins' now");
        // Edit layout: its panels and its bars; Movable panels with what it does
        let edit_features =
            ["Movable panels", "All panels float", "Panels stay open", "Tool Belt", "Edit layout", "Collapse to edge"];
        assert_eq!(labels(&plugins, EDIT_LAYOUT), edit_features);
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.movable_panels"), true));
        let movable = [
            "Movable panels",
            "Layers panel can float",
            "Brush library can float",
            "Snap to edges and panels",
            "Snap distance",
            "Panels fold to a tab",
            "Docked panels make room",
        ];
        assert_eq!(labels(&plugins, EDIT_LAYOUT)[..7], movable);
        assert!(card(&plugins, EDIT_LAYOUT).features.iter().all(|f| !f.description.is_empty()));
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.movable_panels"), false));
        // Infinite Plugins: a feature for each of what painters asked for
        let infinite = ["Lasso fill", "Paper", "Trace view", "Filter layers", "Colour extras"];
        assert_eq!(labels(&plugins, INFINITE), infinite);
        assert!(card(&plugins, INFINITE).features.iter().all(|f| !f.description.is_empty()));
        assert!(plugins.set_switch(format!("{INFINITE}.trace_view"), true));
        assert_eq!(labels(&plugins, INFINITE)[2..5], ["Trace view", "Layer view", "Trace dims the others"]);
        assert!(plugins.set_switch(format!("{INFINITE}.trace_view"), false));
        // an action too, and it doesn't run with its switch off
        assert!(plugins.set_switch(format!("{LAYER_TOOLBOX}.layer_fills"), true));
        assert_eq!(labels(&plugins, LAYER_TOOLBOX)[4..7], ["Layer fills", "Grid layer", "Paper texture"]);
        assert!(plugins.set_on(LAYER_FILLS.to_owned(), true));
        assert!(plugins.actions_in(PluginPlace::Card).is_empty());
        let session = crate::CanvasSession::new(64, 64);
        assert!(!plugins.run_action(format!("{LAYER_FILLS}.add"), Arc::clone(&session)));
        assert!(plugins.set_switch(format!("{LAYER_FILLS}.grid"), true));
        let grid = ["Layer fills", "Grid layer", "Spacing", "Line width", "Paper texture"];
        assert_eq!(labels(&plugins, LAYER_TOOLBOX)[4..9], grid);
        assert_eq!(labels(&plugins, LAYER_TOOLBOX).last().map(String::as_str), Some("Add a grid layer"));
        let fills =
            card(&plugins, LAYER_TOOLBOX).features.into_iter().find(|f| f.control.label == "Layer fills").unwrap();
        assert_eq!(fills.actions.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), ["Add a grid layer"]);
        assert_eq!(plugins.actions_in(PluginPlace::Card).len(), 1);
        // with its feature off, it doesn't either
        assert!(plugins.set_switch(format!("{LAYER_TOOLBOX}.layer_fills"), false));
        assert!(plugins.actions_in(PluginPlace::Card).is_empty());
        assert!(plugins.set_switch(format!("{LAYER_TOOLBOX}.layer_fills"), true));
        // what is hidden keeps its value, and is kept
        assert_eq!(plugins.set_slider(format!("{LAYER_FILLS}.spacing"), 50.0), Some(50.0));
        assert!(plugins.set_switch(format!("{LAYER_FILLS}.grid"), false));
        assert!(plugins.kept_lines().contains(&format!("{LAYER_FILLS}.spacing 50")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundled_filter_layers_show_what_is_below_changed_and_keep_their_values_with_the_layer() {
        const INVERT: &str = "io.github.artcraftandroid.infinite-plugins.invert";
        let dir = store("bundled-filter-layers");
        let plugins = app_at(&dir);
        // off as it comes, and with its feature off: none offered
        assert!(plugins.layer_filters().is_empty());
        assert!(plugins.set_on(INFINITE.to_owned(), true));
        assert!(plugins.layer_filters().is_empty());
        assert!(plugins.set_switch(format!("{INFINITE}.filter_layers"), true));
        let offered = plugins.layer_filters();
        assert_eq!(offered.len(), 38);
        assert_eq!(offered.iter().filter(|f| f.group == "Colour").count(), 11);
        let names = |range: std::ops::Range<usize>| offered[range].iter().map(|f| f.label.clone()).collect::<Vec<_>>();
        assert_eq!(names(0..3), ["Brightness", "Contrast", "Exposure"]);
        assert_eq!(names(8..11), ["Hue", "Curves", "Gradient Map"]);
        assert_eq!(names(35..38), ["Direction", "Zoom", "Spin"]);
        // each offered as a layer, none under Adjustments
        assert!(plugins.filters().iter().all(|f| !offered.iter().any(|o| o.id == f.id)));
        let Some(s) = crate::tests::session(32, 32) else { return };
        s.use_plugins(Arc::clone(&plugins));
        s.set_color(1.0, 0.0, 0.0);
        let base = s.layers().last().unwrap().id;
        assert!(s.fill_layer(base));
        let shown = |s: &crate::CanvasSession| crate::tests::pixel(s, 16, 16);
        assert_eq!(shown(&s), [255, 0, 0, 255]);
        // an Invert layer: all of it as it starts, red to cyan
        let layer = s.add_filter_layer(INVERT.to_owned());
        assert_ne!(layer, 0);
        let row = s.layers().into_iter().find(|l| l.id == layer).unwrap();
        assert_eq!((row.current, row.filter.as_deref()), (true, Some("Invert")));
        assert_eq!(shown(&s), [0, 255, 255, 255]);
        // its settings: the value shows as it is set, Apply keeps it as one step
        assert!(s.filter_layer_settings());
        let panel = s.adjustment_shown().plugin.unwrap();
        assert_eq!((panel.name.as_str(), panel.controls.len()), ("Invert", 1));
        let key = format!("{INVERT}.value");
        let landed = s.plugin_filter_slider(key.clone(), 0.0, false).unwrap();
        assert!(landed.abs() < 1e-5, "{landed}");
        let grey = shown(&s);
        assert!(grey[..3].iter().all(|c| c.abs_diff(128) <= 1), "half inverted: {grey:?}");
        assert!(s.adjustment_end(true));
        assert_eq!(s.undo().as_deref(), Some("Filter Settings"));
        assert_eq!(shown(&s), [0, 255, 255, 255], "undone, as it was");
        // Cancel puts it back
        assert!(s.filter_layer_settings());
        let landed = s.plugin_filter_slider(key, -1.0, true).unwrap();
        assert!((landed + 1.0).abs() < 1e-5, "{landed}");
        assert_eq!(shown(&s), [255, 0, 0, 255]);
        assert!(s.adjustment_end(false));
        assert_eq!(shown(&s), [0, 255, 255, 255]);
        // merged down, its filter goes into the layer below
        assert!(row.can_merge);
        // with the feature off, the layer shows nothing, its value kept
        assert!(plugins.set_switch(format!("{INFINITE}.filter_layers"), false));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(shown(&s), [255, 0, 0, 255]);
        assert!(!s.filter_layer_settings());
        assert!(plugins.set_switch(format!("{INFINITE}.filter_layers"), true));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(shown(&s), [0, 255, 255, 255]);
        // merged down: the layer below inverted, the filter layer gone, all as shown before
        let own = |s: &crate::CanvasSession| {
            let px = s.shared.lock().layer_pixels(procraft_engine::document::LayerId(base)).unwrap();
            px[4 * (16 * 32 + 16)..4 * (16 * 32 + 16) + 4].to_vec()
        };
        assert_eq!(own(&s), [255, 0, 0, 255]);
        assert!(s.merge_down(layer));
        assert!(s.layers().iter().all(|l| l.id != layer));
        assert_eq!(own(&s), [0, 255, 255, 255], "the layer below took the filter");
        assert_eq!(shown(&s), [0, 255, 255, 255]);
        // undone, the layer below as it was and the filter layer over it again
        s.undo();
        assert_eq!(own(&s), [255, 0, 0, 255], "the layer below as it was");
        assert_eq!(shown(&s), [0, 255, 255, 255]);
        // Curves: the app's Curves panel on the layer, with alpha; red turned over shows black
        s.delete_layer(layer);
        assert_eq!(shown(&s), [255, 0, 0, 255]);
        let curves = s.add_filter_layer(format!("{INFINITE}.curves"));
        assert_ne!(curves, 0);
        assert_eq!(shown(&s), [255, 0, 0, 255], "straight curves change nothing");
        assert!(s.filter_layer_settings());
        let open = s.adjustment_shown();
        assert_eq!((open.adjustment, open.curve_channels, open.plugin), (Some(crate::Adjustment::Curves), 5, None));
        s.adjustment_curve_channel(1);
        s.adjustment_curve_move(0, 0.0, 1.0);
        s.adjustment_curve_move(1, 1.0, 0.0);
        assert_eq!(shown(&s), [0, 0, 0, 255]);
        assert!(s.adjustment_end(true));
        assert_eq!(s.undo().as_deref(), Some("Filter Settings"));
        assert_eq!(shown(&s), [255, 0, 0, 255]);
        // Gradient Map: red's lightness (0.30) through black, grey and white
        s.delete_layer(curves);
        let mapped = s.add_filter_layer(format!("{INFINITE}.gradient_map"));
        assert_ne!(mapped, 0);
        let grey = shown(&s);
        assert!(grey[..3].iter().all(|c| c.abs_diff(77) <= 2), "{grey:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_bundled_filter_layer_s_code_holds_up_and_reads_within_its_reach() {
        let dir = store("bundled-filter-layer-codes");
        let plugins = app_at(&dir);
        assert!(plugins.set_on(INFINITE.to_owned(), true));
        assert!(plugins.set_switch(format!("{INFINITE}.filter_layers"), true));
        let defs = plugins.layer_filter_defs();
        // each by its id, and by the id it had in Layer tools, which artworks kept its layers by
        assert_eq!(defs.len(), 2 * 38);
        for (name, def) in &defs {
            let key = name.rsplit('.').next().unwrap();
            let now = &defs[&format!("{INFINITE}.{key}")];
            assert_eq!((def.label.as_str(), &def.text, def.plugin.as_str()), (now.label.as_str(), &now.text, INFINITE));
            assert!(name.starts_with(&format!("{INFINITE}.")) || name.starts_with(&format!("{LAYER_TOOLBOX}.")));
        }
        for (name, def) in &defs {
            let keys = crate::plugin_file::value_names(&def.values);
            let mut keys = keys;
            keys.extend(def.touch.iter().flat_map(|t| t.keys()).map(|k| (*k).to_owned()));
            let read = if def.editor.is_some() {
                procraft_gpu::read_layer_table_filter
            } else {
                procraft_gpu::read_layer_filter
            };
            let code = read(&def.text, &keys);
            assert!(code.is_ok(), "{name}: {:?}", code.err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_filter_of_several_opens_at_its_card_s_values_and_resets_to_them() {
        let dir = store("listed-filters");
        let plugins = at(&dir);
        let (_, file) = crate::plugin_file::tests::two_filters("org.example.filters");
        assert_eq!(plugins.install(file), None);
        // it only adds, so it is on, but each filter waits for its switch
        assert!(card(&plugins, "org.example.filters").on);
        assert!(plugins.filters().is_empty());
        assert!(plugins.set_switch("org.example.filters.fade_on".to_owned(), true));
        let filters = plugins.filters();
        assert_eq!(filters.len(), 1);
        assert_eq!((filters[0].id.as_str(), filters[0].name.as_str()), ("org.example.filters.fade", "Fade"));
        assert_eq!(plugins.set_slider("org.example.filters.fade".to_owned(), 0.3), Some(0.3));
        let opened = plugins.filter_to_open("org.example.filters.fade").map_err(|why| why.to_string()).unwrap();
        assert_eq!(opened.plugin, "org.example.filters");
        assert_eq!(opened.controls.len(), 1);
        match &opened.controls[0] {
            PluginControl::Slider { control } => assert_eq!((control.value, control.starts), (0.3, 0.3)),
            other => panic!("{other:?}"),
        }
        // its switch off, it isn't listed; the plugin's own id opens none of them
        assert!(plugins.set_switch("org.example.filters.fade_on".to_owned(), false));
        assert!(plugins.filters().is_empty());
        assert!(plugins.filter_to_open("org.example.filters").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A plugin of code, `id`, with `controls` and `more` in its manifest.
    fn coded_file(id: &str, controls: Value, more: &[(&str, Value)]) -> Vec<u8> {
        let module = position_module(r#"(func (export "procraft_control") (param i32 f32))"#, "(i64.const 0)");
        let mut manifest = coded(id, &module);
        manifest["controls"] = controls;
        for (field, value) in more {
            manifest[*field] = value.clone();
        }
        pack(&manifest, &[(crate::plugin_run::MODULE, &module)])
    }

    const OLD_A: &str = "org.example.old-a";
    const OLD_B: &str = "org.example.old-b";
    const NEW: &str = "org.example.new";

    /// Two plugins as an app came with them, A with a slider and B a filter, as files.
    fn the_old_two() -> Vec<(&'static str, Vec<u8>)> {
        let a = json!([{ "key": "amount", "type": "slider", "label": "Amount", "least": 0, "most": 10, "starts": 2 }]);
        vec![("old-a", coded_file(OLD_A, a, &[])), ("old-b", filter_file(&filter(OLD_B)))]
    }

    /// The one that takes their place: a switch for each, A's slider under A's.
    fn the_new_one() -> (&'static str, Vec<u8>) {
        let controls = json!([
            { "key": "a", "type": "switch", "label": "Old A" },
            { "key": "amount", "type": "slider", "label": "Amount", "least": 0, "most": 10, "starts": 2, "under": "a" },
            { "key": "b", "type": "switch", "label": "Old B" },
        ]);
        let replaces = json!([
            { "id": OLD_A, "switch": "a", "controls": { "amount": "amount" } },
            { "id": OLD_B, "switch": "b" },
        ]);
        ("new", coded_file(NEW, controls, &[("replaces", replaces)]))
    }

    fn switch_of(plugins: &Plugins, key: &str) -> bool {
        matches!(plugins.control(key), Some(PluginControl::Switch { control }) if control.on)
    }

    #[test]
    fn one_taking_the_place_of_others_takes_what_they_had_set_in_whatever_order_it_is_read() {
        let dir = store("replaces");
        // an earlier app: A on with its slider moved, B off
        let earlier = app_with(&dir, &the_old_two());
        let mut lines = vec![format!("{OLD_A}.on true"), format!("{OLD_A}.amount 7"), format!("{OLD_B}.on false")];
        read_lines(&earlier, &lines);
        // the next comes with the new one alone: the old ones go, from the disk too, once
        for order in [false, true] {
            let next = app_with(&dir, &[the_new_one()]);
            assert!(!has(&next, OLD_A) && !has(&next, OLD_B));
            assert!(!dir_holds(&dir, OLD_A) && !dir_holds(&dir, OLD_B));
            if order {
                lines.reverse();
            }
            read_lines(&next, &lines);
            assert!(card(&next, NEW).on, "reversed: {order}");
            assert!(switch_of(&next, &format!("{NEW}.a")) && !switch_of(&next, &format!("{NEW}.b")));
            assert_eq!(slider_value(&next, &format!("{NEW}.amount")), 7.0);
            assert!(next.hooks().is_on(NEW));
        }
        // its own values, kept since, say instead, read before or after
        for own_first in [true, false] {
            let next = app_at_with_new(&dir);
            let own = [format!("{NEW}.a false"), format!("{NEW}.amount 4"), format!("{NEW}.on false")];
            if own_first {
                read_lines(&next, &own);
            }
            read_lines(&next, &lines);
            if !own_first {
                read_lines(&next, &own);
            }
            assert!(!card(&next, NEW).on && !switch_of(&next, &format!("{NEW}.a")), "own first: {own_first}");
            assert_eq!(slider_value(&next, &format!("{NEW}.amount")), 4.0);
        }
        // with all the old ones off, it stays off
        let off = app_at_with_new(&dir);
        read_lines(&off, &[format!("{OLD_A}.on false"), format!("{OLD_B}.on false")]);
        assert!(!card(&off, NEW).on);
        // B on alone: on, with B's switch, A's not
        let b = app_at_with_new(&dir);
        read_lines(&b, &[format!("{OLD_A}.on false"), format!("{OLD_B}.on true")]);
        assert!(card(&b, NEW).on && switch_of(&b, &format!("{NEW}.b")) && !switch_of(&b, &format!("{NEW}.a")));
        // what is written down from then on is its own
        let kept = b.kept_lines();
        assert!(kept.contains(&format!("{NEW}.on true")) && kept.contains(&format!("{NEW}.b true")), "{kept:?}");
        assert!(kept.iter().all(|line| !line.starts_with("org.example.old")), "{kept:?}");
        // an old one can't come back beside it; removed, it takes what was kept for those too
        let said = Some("A plugin already here takes this one's place.".to_owned());
        assert_eq!(b.install(the_old_two().remove(0).1), said);
        let keys = b.remove(NEW.to_owned()).unwrap();
        for key in [format!("{NEW}.on"), format!("{OLD_A}.on"), format!("{OLD_A}.amount"), format!("{OLD_B}.on")] {
            assert!(keys.contains(&key), "{key}: {keys:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The app's plugins in `dir` as an app that comes with the new one alone adds them.
    fn app_at_with_new(dir: &str) -> Arc<Plugins> {
        app_with(dir, &[the_new_one()])
    }

    #[test]
    fn imported_where_the_others_are_on_it_takes_their_place_as_they_were_at_once() {
        let dir = store("replaces-import");
        let plugins = at(&dir);
        for (_, file) in the_old_two() {
            assert_eq!(plugins.install(file), None);
        }
        assert!(plugins.set_on(OLD_A.to_owned(), true));
        assert_eq!(plugins.set_slider(format!("{OLD_A}.amount"), 9.0), Some(9.0));
        // B, a filter, started on
        let offer = plugins.inspect(the_new_one().1);
        assert!(
            offer.message.contains("It takes the place of A plugin and A filter, keeping what was set."),
            "{offer:?}"
        );
        assert_eq!(plugins.install(the_new_one().1), None);
        assert!(!has(&plugins, OLD_A) && !has(&plugins, OLD_B) && !plugins.hooks().is_on(OLD_A));
        assert!(card(&plugins, NEW).on && plugins.hooks().is_on(NEW));
        assert!(switch_of(&plugins, &format!("{NEW}.a")) && switch_of(&plugins, &format!("{NEW}.b")));
        assert_eq!(slider_value(&plugins, &format!("{NEW}.amount")), 9.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_the_app_added_and_someone_removed_stays_removed() {
        let dir = store("bundled-removed");
        let plugins = app_at(&dir);
        assert!(plugins.set_on(COLORDROP.to_owned(), true));
        assert_eq!(plugins.set_slider("fill_gap".to_owned(), 6.0), Some(6.0));
        let keys = plugins.remove(EFFECTS.to_owned()).unwrap();
        assert!(keys.contains(&format!("{EFFECTS}.on")) && keys.contains(&format!("{EFFECTS}.reach")), "{keys:?}");
        assert!(keys.contains(&"io.github.artcraftandroid.outline.on".to_owned()), "{keys:?}");
        assert!(keys.contains(&"io.github.artcraftandroid.effects.reach".to_owned()), "{keys:?}");
        let keys = plugins.remove(COLORDROP.to_owned()).unwrap();
        assert!(keys.contains(&"fill_gap".to_owned()) && keys.contains(&format!("{COLORDROP}.on")), "{keys:?}");
        assert!(!has(&plugins, EFFECTS) && !has(&plugins, COLORDROP) && !plugins.hooks().is_on(COLORDROP));
        assert!(!dir_holds(&dir, EFFECTS) && !dir_holds(&dir, COLORDROP));
        // through a restart, and an app that comes with a newer one
        let lines = plugins.kept_lines();
        let next = app_at(&dir);
        read_lines(&next, &lines);
        assert!(!has(&next, EFFECTS) && !has(&next, COLORDROP));
        assert_eq!(next.plugins().len(), crate::bundled::BUNDLED.len() - 2);
        let mut newer = all_bundled();
        newer.retain(|(name, _)| *name != "layer-toolbox");
        newer.push(("layer-toolbox", bundled_file("layer-toolbox", "9.0.0")));
        assert!(!has(&app_with(&dir, &newer), EFFECTS));
        // imported again, it is as any import: off until turned on, and on once it is
        assert_eq!(next.install(bundled_file("layer-toolbox", "1.0.0")), None);
        assert!(!card(&next, EFFECTS).on);
        assert!(next.set_on(EFFECTS.to_owned(), true));
        read_lines(&next, &[format!("{EFFECTS}.on true")]);
        assert!(card(&next, EFFECTS).on);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_app_adds_what_it_never_added_and_updates_what_is_there() {
        let dir = store("bundled-newer");
        let mut first = all_bundled();
        first.retain(|(name, _)| *name != "brush-extras");
        let plugins = app_with(&dir, &first);
        assert!(!has(&plugins, TOUCH));
        assert!(plugins.set_on(EFFECTS.to_owned(), true));
        assert!(plugins.set_on(COLORDROP.to_owned(), true));
        assert_eq!(plugins.set_slider("fill_gap".to_owned(), 7.0), Some(7.0));
        let lines = plugins.kept_lines();
        // the next comes with touch offset, and newer ones of these two (newer than any the app has come with)
        let mut next = all_bundled();
        next.retain(|(name, _)| !["layer-toolbox", "colour-tools"].contains(name));
        next.push(("layer-toolbox", bundled_file("layer-toolbox", "9.0.0")));
        next.push(("colour-tools", bundled_file("colour-tools", "9.0.1")));
        let updated = app_with(&dir, &next);
        read_lines(&updated, &lines);
        assert!(!card(&updated, TOUCH).on);
        let (effects, colordrop) = (card(&updated, EFFECTS), card(&updated, COLORDROP));
        assert_eq!((effects.version.as_str(), effects.on), ("9.0.0", true));
        assert_eq!((colordrop.version.as_str(), colordrop.on), ("9.0.1", true));
        assert_eq!(slider_value(&updated, "fill_gap"), 7.0);
        // one newer than the app's stays as it is
        let older = app_at(&dir);
        assert_eq!(card(&older, EFFECTS).version, "9.0.0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_imported_before_the_app_came_with_it_is_left_as_it_was() {
        let dir = store("bundled-imported");
        // imported and turned on before
        let earlier = at(&dir);
        assert_eq!(earlier.install(bundled_file("layer-toolbox", "1.0.0")), None);
        assert!(earlier.set_on(EFFECTS.to_owned(), true));
        let lines = earlier.kept_lines();
        assert_eq!(at(&dir).install(bundled_file("brush-extras", "0.5.0")), None);
        let plugins = app_at(&dir);
        read_lines(&plugins, &lines);
        assert!(card(&plugins, EFFECTS).on);
        // an older one is updated
        let touch = card(&plugins, TOUCH);
        assert_eq!((touch.version.as_str(), touch.on), ("1.0.0", false));
        assert!(!card(&plugins, COLOUR_TOOLS).on);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_moved_setting_someone_changed_starts_what_the_app_added_on_with_it() {
        let dir = store("bundled-moved");
        // as an earlier version kept them: the gap and the eyedropper changed, the rest not
        let plugins = app_at(&dir);
        plugins.read_kept("fill_gap".to_owned(), "5".to_owned());
        plugins.read_kept("eyedropper_layer".to_owned(), "true".to_owned());
        plugins.read_kept("interface_size".to_owned(), "1.0".to_owned());
        plugins.read_kept("paste_into_mask".to_owned(), "false".to_owned());
        assert!(card(&plugins, COLORDROP).on && card(&plugins, LAYER_TOOLS).on);
        assert_eq!(slider_value(&plugins, "fill_gap"), 5.0);
        // with the switches the gap comes under, its feature too
        assert!(switch_of(&plugins, &format!("{COLORDROP}.gap_closing")));
        assert!(switch_of(&plugins, &format!("{COLORDROP}.colordrop")));
        assert!(switch_of(&plugins, &format!("{LAYER_TOOLS}.layer_tools")));
        assert!(plugins.hooks().fill_mask() && plugins.hooks().pick_layer() && !plugins.hooks().paste_mask());
        assert!(!card(&plugins, INTERFACE).on);
        // nothing to import: it's here, and on
        assert_eq!(plugins.take_moved_notice(), None);
        let lines = plugins.kept_lines();
        assert!(lines.contains(&format!("{COLORDROP}.on true")), "{lines:?}");
        assert_eq!(lines.iter().filter(|line| line.starts_with("fill_gap ")).count(), 1, "{lines:?}");
        // on the next start with only the setting kept, as one platform keeps them
        let next = app_at(&dir);
        next.read_kept("fill_gap".to_owned(), "5".to_owned());
        assert!(card(&next, COLORDROP).on);
        // the size too, from where it was kept as a number
        next.read_kept("interface_size".to_owned(), "0.85".to_owned());
        assert!(card(&next, INTERFACE).on && switch_of(&next, &format!("{INTERFACE}.interface")));
        assert!((next.interface_scale(1600.0, 1000.0) - 0.85).abs() < 1e-6);
        // its own on or off says instead, read before or after
        for flag_first in [true, false] {
            let next = app_at(&dir);
            let flag = || next.read_kept(format!("{COLORDROP}.on"), "false".to_owned());
            if flag_first {
                flag();
            }
            next.read_kept("fill_gap".to_owned(), "5".to_owned());
            if !flag_first {
                flag();
            }
            assert!(!card(&next, COLORDROP).on, "flag first: {flag_first}");
            assert_eq!(slider_value(&next, "fill_gap"), 5.0);
        }
        // and the switch its own, kept off
        let next = app_at(&dir);
        next.read_kept(format!("{COLORDROP}.gap_closing"), "false".to_owned());
        next.read_kept("fill_gap".to_owned(), "5".to_owned());
        assert!(card(&next, COLORDROP).on && !switch_of(&next, &format!("{COLORDROP}.gap_closing")));
        // nor does one that stopped come back on for it
        app_at(&dir).stop_filter(COLORDROP, STOPPED_DRIVER);
        let again = app_at(&dir);
        again.read_kept("fill_gap".to_owned(), "5".to_owned());
        assert!(!card(&again, COLORDROP).on);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_triangle_s_menu_switches_its_shape_and_what_it_sets_is_kept() {
        let dir = store("bundled-menu");
        let plugins = featured_app(&dir, COLOUR_TOOLS, "colour_triangle");
        // nothing at Classic while it's off
        assert!(plugins.menu_choices(PluginMenuPlace::Classic).is_empty());
        assert!(plugins.choose(format!("{TRIANGLE}.ring")).is_empty(), "none offers it");
        assert!(plugins.set_on(TRIANGLE.to_owned(), true));
        let chosen = |plugins: &Plugins| {
            let choices = plugins.menu_choices(PluginMenuPlace::Classic);
            choices.iter().filter(|c| c.chosen).map(|c| c.label.clone()).collect::<Vec<_>>()
        };
        let choices = plugins.menu_choices(PluginMenuPlace::Classic);
        let labels: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Square", "Triangle", "Hue ring"]);
        assert!(choices.iter().all(|c| c.icon.starts_with('M') && c.icon.ends_with('Z')), "each its icon");
        // the triangle starts on, its ring off
        assert_eq!(chosen(&plugins), ["Triangle"]);
        assert_eq!(plugins.picker_hue_point(0.25), None);
        // the ring: its switch set, and told back to be kept
        let changed = plugins.choose(format!("{TRIANGLE}.ring"));
        let keys: Vec<&str> = changed.iter().map(PluginControl::key).collect();
        assert_eq!(keys, [format!("{TRIANGLE}.ring")]);
        assert_eq!(chosen(&plugins), ["Hue ring"]);
        assert!(plugins.picker_hue_point(0.25).is_some(), "the ring has the hue");
        // the square: Classic is the app's own again
        let changed = plugins.choose(format!("{TRIANGLE}.square"));
        assert!(matches!(changed.as_slice(), [PluginControl::Switch { control }] if !control.on));
        assert_eq!(chosen(&plugins), ["Square"]);
        assert!(!plugins.picker_on());
        // picked again, nothing changes
        assert!(plugins.choose(format!("{TRIANGLE}.square")).is_empty());
        // kept, so through a restart, as if set on its card
        let next = app_at(&dir);
        read_lines(&next, &plugins.kept_lines());
        assert_eq!(chosen(&next), ["Square"]);
        assert!(!next.picker_on());
        assert!(!next.choose(format!("{TRIANGLE}.ring")).is_empty());
        assert_eq!(chosen(&next), ["Hue ring"]);
        // off, or with Colour triangle off, it offers nothing
        assert!(next.set_switch(format!("{TRIANGLE}.colour_triangle"), false));
        assert!(next.menu_choices(PluginMenuPlace::Classic).is_empty());
        assert!(next.choose(format!("{TRIANGLE}.square")).is_empty());
        assert!(next.set_switch(format!("{TRIANGLE}.colour_triangle"), true));
        assert!(!next.menu_choices(PluginMenuPlace::Classic).is_empty());
        assert!(next.set_on(TRIANGLE.to_owned(), false));
        assert!(next.menu_choices(PluginMenuPlace::Classic).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_interface_hides_every_notice_of_something_done_but_what_went_wrong() {
        let dir = store("bundled-notices");
        let plugins = featured_app(&dir, INTERFACE, "interface");
        let shown = |plugins: &Plugins| {
            [NoticeKind::Step, NoticeKind::Done, NoticeKind::Problem].map(|k| plugins.shows_notice(k))
        };
        // with it off, every notice shows
        assert_eq!(shown(&plugins), [true; 3]);
        assert!(plugins.set_on(INTERFACE.to_owned(), true));
        assert_eq!(shown(&plugins), [true; 3], "each waits for its switch");
        // undo's alone, then every action's; what went wrong always shows
        assert!(plugins.set_switch(format!("{INTERFACE}.hide_undo"), true));
        assert_eq!(shown(&plugins), [false, true, true]);
        assert!(plugins.set_switch(format!("{INTERFACE}.hide_undo"), false));
        assert!(plugins.set_switch(format!("{INTERFACE}.hide_notices"), true));
        assert_eq!(shown(&plugins), [false, false, true]);
        // kept, so through a restart
        let next = app_at(&dir);
        read_lines(&next, &plugins.kept_lines());
        assert_eq!(shown(&next), [false, false, true]);
        assert!(next.set_on(INTERFACE.to_owned(), false));
        assert_eq!(shown(&next), [true; 3]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edit_layout_lays_the_panels_out_and_lets_the_bars_move_as_its_switches_say() {
        use crate::floating::FloatingPanel;
        use crate::layout::PanelLayoutLook;
        let dir = store("bundled-edit-layout");
        let plugins = featured_app(&dir, EDIT_LAYOUT, "movable_panels");
        let floats = |plugins: &Plugins| [FloatingPanel::Layers, FloatingPanel::Brushes].map(|p| plugins.floats(p));
        // off: as the app always had it
        assert_eq!(plugins.panel_layout(), PanelLayoutLook::default());
        assert_eq!(floats(&plugins), [false; 2]);
        // on: snapping within 16, folding, no room made; both panels float
        assert!(plugins.set_on(EDIT_LAYOUT.to_owned(), true));
        let look = PanelLayoutLook {
            on: true,
            snap: 16.0,
            fold: true,
            make_room: false,
            edit: false,
            belt: false,
            collapse: false,
        };
        assert_eq!(plugins.panel_layout(), look);
        assert_eq!(floats(&plugins), [true; 2]);
        // the other panels (by their numbers on the hook, interface 20) as they always are: All panels
        // float is off as it starts
        assert!((2..=7).all(|n| !plugins.floats_number(n)));
        let layout = crate::PanelLayout::new();
        layout.follow(Arc::clone(&plugins));
        let others = [
            crate::LayoutPiece::Actions,
            crate::LayoutPiece::Adjustments,
            crate::LayoutPiece::Adjusting,
            crate::LayoutPiece::Selection,
            crate::LayoutPiece::Transform,
            crate::LayoutPiece::Liquify,
        ];
        assert!(others.iter().all(|&p| !layout.may_float(p)), "they hang where they always do");
        assert!(layout.may_float(crate::LayoutPiece::Layers));
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.all_panels"), true));
        assert!((2..=7).all(|n| plugins.floats_number(n)));
        assert!(!plugins.floats_number(8), "a panel it doesn't know");
        layout.follow(Arc::clone(&plugins));
        assert!(others.iter().all(|&p| layout.may_float(p)));
        assert_eq!(floats(&plugins), [true; 2], "the Layers panel and the library by their own switches");
        // on by itself, Movable panels off: the others float, with no layout of their own
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.movable_panels"), false));
        assert!((2..=7).all(|n| plugins.floats_number(n)));
        assert_eq!(floats(&plugins), [false; 2]);
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.all_panels"), false));
        assert!((2..=7).all(|n| !plugins.floats_number(n)));
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.movable_panels"), true));
        assert!(!plugins.hooks().pin_panels(), "panels stay open by a feature of their own");
        // as its switches say
        assert_eq!(plugins.set_slider(format!("{EDIT_LAYOUT}.snap_distance"), 30.0), Some(30.0));
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.make_room"), true));
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.fold"), false));
        let moved = PanelLayoutLook { snap: 30.0, fold: false, make_room: true, ..look };
        assert_eq!(plugins.panel_layout(), moved);
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.snap"), false));
        assert_eq!(plugins.panel_layout().snap, 0.0);
        // Panels stay open and Edit layout, each by its own feature
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.pin_panels"), true));
        assert!(plugins.hooks().pin_panels());
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.edit_layout"), true));
        assert!(plugins.panel_layout().edit);
        // the Tool Belt by its own feature
        assert!(!plugins.panel_layout().belt);
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.tool_belt"), true));
        assert!(plugins.panel_layout().belt);
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.tool_belt"), false));
        // Edit layout alone: the bars move, the panels as the app has them
        assert!(plugins.set_switch(format!("{EDIT_LAYOUT}.movable_panels"), false));
        let bars_only = PanelLayoutLook { edit: true, ..PanelLayoutLook::default() };
        assert_eq!(plugins.panel_layout(), bars_only);
        assert_eq!(floats(&plugins), [false; 2]);
        // kept, through a restart
        let next = app_at(&dir);
        read_lines(&next, &plugins.kept_lines());
        assert_eq!(next.panel_layout(), bars_only);
        assert!(next.hooks().pin_panels());
        assert!(next.set_on(EDIT_LAYOUT.to_owned(), false));
        assert_eq!(next.panel_layout(), PanelLayoutLook::default());
        assert!(!next.hooks().pin_panels());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What Interface look kept for Floating panels and Panels stay open, which Edit layout takes:
    /// the switch there, its values (the key there, as kept, and the key here), and the switch here.
    type LookValue = (&'static str, &'static str, &'static str);
    const FROM_LOOK: &[(&str, &[LookValue], &str)] = &[
        (
            "floating_panels",
            &[("float_layers", "true", "float_layers"), ("float_brushes", "false", "float_brushes")],
            "movable_panels",
        ),
        ("pin_panels", &[], "pin_panels"),
    ];

    #[test]
    fn what_interface_look_kept_for_floating_panels_and_panels_staying_open_comes_back_under_edit_layout() {
        use crate::floating::FloatingPanel;
        let dir = store("edit-layout-taken");
        for (feature, values, now) in FROM_LOOK {
            let lines_of = |on: bool, feature_on: bool| {
                let mut lines =
                    vec![format!("{INTERFACE_LOOK}.on {on}"), format!("{INTERFACE_LOOK}.{feature} {feature_on}")];
                lines.extend(values.iter().map(|(key, value, _)| format!("{INTERFACE_LOOK}.{key} {value}")));
                lines
            };
            // Interface look and the feature on, in either order: Edit layout on with it, its values
            // taken; Interface look on as it was
            for reversed in [false, true] {
                let plugins = app_at(&dir);
                let mut lines = lines_of(true, true);
                if reversed {
                    lines.reverse();
                }
                read_lines(&plugins, &lines);
                assert!(card(&plugins, EDIT_LAYOUT).on, "{feature}");
                assert!(card(&plugins, INTERFACE_LOOK).on, "{feature}");
                assert!(switch_of(&plugins, &format!("{EDIT_LAYOUT}.{now}")), "{feature}");
                for (key, value, here) in *values {
                    let got = kept_value(&plugins, &format!("{EDIT_LAYOUT}.{here}"));
                    assert!(same_value(&got, value), "{key} as {here}: {got}, not {value}");
                }
                // kept as Edit layout's from then on, through the next start
                let kept = plugins.kept_lines();
                assert!(kept.contains(&format!("{EDIT_LAYOUT}.on true")), "{kept:?}");
                let old = |line: &String| {
                    line.starts_with(&format!("{INTERFACE_LOOK}.{feature} "))
                        || values.iter().any(|(key, ..)| line.starts_with(&format!("{INTERFACE_LOOK}.{key} ")))
                };
                assert!(!kept.iter().any(old), "{kept:?}");
                let next = app_at(&dir);
                read_lines(&next, &kept);
                assert_eq!(next.plugins(), plugins.plugins(), "{feature}");
            }
            // Interface look off, or the feature: Edit layout stays off, the switch as it was
            for (on, feature_on) in [(false, true), (true, false)] {
                let plugins = app_at(&dir);
                read_lines(&plugins, &lines_of(on, feature_on));
                assert!(!card(&plugins, EDIT_LAYOUT).on, "{feature}: {on} {feature_on}");
                assert_eq!(switch_of(&plugins, &format!("{EDIT_LAYOUT}.{now}")), feature_on, "{feature}");
            }
        }
        // both at once: one Edit layout, the Layers panel floating and not the library, and panels
        // staying open
        let plugins = app_at(&dir);
        let mut lines = vec![format!("{INTERFACE_LOOK}.on true")];
        for (feature, values, _) in FROM_LOOK {
            lines.push(format!("{INTERFACE_LOOK}.{feature} true"));
            lines.extend(values.iter().map(|(key, value, _)| format!("{INTERFACE_LOOK}.{key} {value}")));
        }
        read_lines(&plugins, &lines);
        assert!(card(&plugins, EDIT_LAYOUT).on);
        assert_eq!([FloatingPanel::Layers, FloatingPanel::Brushes].map(|p| plugins.floats(p)), [true, false]);
        assert!(plugins.hooks().pin_panels());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn panels_float_while_the_floating_panels_switches_say() {
        use crate::floating::FloatingPanel;
        const FLOATING: &str = EDIT_LAYOUT;
        let dir = store("bundled-floating");
        let plugins = featured_app(&dir, FLOATING, "movable_panels");
        let floats = |plugins: &Plugins| [FloatingPanel::Layers, FloatingPanel::Brushes].map(|p| plugins.floats(p));
        // with it off nothing floats; on, both do at once, each while its switch is
        assert_eq!(floats(&plugins), [false; 2]);
        assert!(plugins.set_on(FLOATING.to_owned(), true));
        assert_eq!(floats(&plugins), [true, true]);
        assert!(plugins.set_switch(format!("{FLOATING}.float_layers"), false));
        assert_eq!(floats(&plugins), [false, true]);
        // kept, so through a restart
        let next = app_at(&dir);
        read_lines(&next, &plugins.kept_lines());
        assert_eq!(floats(&next), [false, true]);
        assert!(next.set_on(FLOATING.to_owned(), false));
        assert_eq!(floats(&next), [false; 2]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session on a window, its canvas fitted in it.
    fn viewed() -> Arc<crate::CanvasSession> {
        let s = crate::CanvasSession::new(100, 100);
        {
            let mut st = s.shared.lock();
            st.window_size = (400, 400);
            st.view = Some(procraft_engine::view::View::fit((100, 100), (400, 400)));
        }
        s
    }

    fn press(plugins: &Plugins, text: &str) -> Option<PluginPressed> {
        plugins.plugin_press(KeyCombo::read(text).unwrap())
    }

    #[test]
    fn a_plugin_s_press_with_alt_comes_before_the_app_s_key_taken_alone() {
        use crate::KeyShortcut::{Erase, Selection, Undo};
        // Trace view's Solo is on Alt+S, which the app's S (selection) took as S
        let dir = store("bundled-alt-s");
        let plugins = featured_app(&dir, INFINITE, "trace_view");
        let shortcut = |key: &str, alt: bool| plugins.key_shortcut(key.to_owned(), false, false, alt, false);
        let first = |text: &str| plugins.plugin_press_first(KeyCombo::read(text).unwrap());
        // off, Alt+S is the app's selection, as S is
        assert_eq!(shortcut("s", true), Some(Selection));
        assert!(!first("alt+s"));
        assert!(plugins.set_on(INFINITE.to_owned(), true));
        // on, Alt+S and Alt+T are Solo's and Trace's; S alone is still the app's
        for (text, action) in [("alt+s", "solo"), ("alt+t", "trace")] {
            assert!(first(text), "{text}");
            assert_eq!(press(&plugins, text), Some(PluginPressed::Action { key: format!("{INFINITE}.{action}") }));
        }
        assert_eq!(shortcut("s", true), None);
        assert_eq!(shortcut("s", false), Some(Selection));
        // the command key's presses, and Alt with a key no plugin names, are the app's as before
        assert_eq!(plugins.key_shortcut("z".to_owned(), true, false, false, false), Some(Undo));
        assert!(!first("command+alt+s"));
        assert_eq!(shortcut("e", true), Some(Erase));
        // the feature off, Alt+S is the app's again
        assert!(plugins.set_switch(format!("{INFINITE}.trace_view"), false));
        assert!(!first("alt+s"));
        assert_eq!(shortcut("s", true), Some(Selection));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_reset_turns_the_canvas_from_the_canvas_tab_or_its_press_and_locks_pinching() {
        const ROTATION: &str = VIEW_TOOLS;
        let dir = store("bundled-rotation");
        let plugins = featured_app(&dir, ROTATION, "rotation_reset");
        // off, the Canvas tab has nothing of it and its press is no one's
        assert!(
            plugins.controls_in(PluginPlace::Canvas).is_empty() && plugins.actions_in(PluginPlace::Canvas).is_empty()
        );
        assert_eq!(press(&plugins, "alt+r"), None);
        assert!(plugins.set_on(ROTATION.to_owned(), true));
        let reset = format!("{ROTATION}.reset");
        let keys = |controls: Vec<PluginControl>| controls.iter().map(|c| c.key().to_owned()).collect::<Vec<_>>();
        assert_eq!(
            keys(plugins.controls_in(PluginPlace::Canvas)),
            [format!("{ROTATION}.lock"), format!("{ROTATION}.no_fit")]
        );
        let actions: Vec<String> = plugins.actions_in(PluginPlace::Canvas).into_iter().map(|a| a.key).collect();
        assert_eq!(actions, [reset.as_str()]);
        assert!(plugins.controls_in(PluginPlace::Card).is_empty());
        // Alt+R runs it, and the keyboard's list says so
        assert_eq!(press(&plugins, "alt+r"), Some(PluginPressed::Action { key: reset.clone() }));
        assert_eq!(press(&plugins, "r"), None);
        let list = plugins.key_shortcut_list(false, false);
        let ours = list.iter().find(|g| g.heading == "Plugins").unwrap();
        assert_eq!(ours.rows.iter().map(|r| r.does.as_str()).collect::<Vec<_>>(), ["Reset rotation"]);
        // run on a turned canvas: upright, or the nearest right angle, the middle staying put
        let s = viewed();
        s.view_gesture(200.0, 200.0, 0.0, 0.0, 2.0, 1.2);
        let middle = s.shared.lock().view.unwrap().screen_to_canvas(procraft_engine::geom::Vec2::new(200.0, 200.0));
        let settled = |s: &crate::CanvasSession| {
            let mut st = s.shared.lock();
            st.view = st.view_motion.take().map(|m| m.to).or(st.view);
            st.view.unwrap()
        };
        assert!(plugins.run_action(reset.clone(), Arc::clone(&s)));
        let v = settled(&s);
        assert!(
            v.rotation().abs() < 1e-5
                && (v.zoom() / 2.0 - procraft_engine::view::View::fit((100, 100), (400, 400)).zoom()).abs() < 1e-3
        );
        assert_eq!(
            v.screen_to_canvas(procraft_engine::geom::Vec2::new(200.0, 200.0)).map(|p| (p.x.round(), p.y.round())),
            middle.map(|p| (p.x.round(), p.y.round()))
        );
        assert!(plugins.set_slider(format!("{ROTATION}.to"), 1.0).is_some());
        s.view_gesture(200.0, 200.0, 0.0, 0.0, 1.0, 1.2);
        assert!(plugins.run_action(reset, Arc::clone(&s)));
        assert!((settled(&s).rotation() - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        // the switch keeps two fingers from turning the canvas
        assert!(!plugins.hooks().gesture_lock(crate::hooks::GESTURE_TURN) && !s.gesture_turn_locked());
        assert!(plugins.set_switch(format!("{ROTATION}.lock"), true));
        s.use_plugins(Arc::clone(&plugins));
        assert!(plugins.hooks().gesture_lock(crate::hooks::GESTURE_TURN) && s.gesture_turn_locked());
        // asked as each finger lands, and each key that isn't the app's: quick
        let hooks = plugins.hooks();
        let t = std::time::Instant::now();
        for _ in 0..1000 {
            assert!(hooks.gesture_lock(crate::hooks::GESTURE_TURN));
        }
        let asked = t.elapsed() / 1000;
        let t = std::time::Instant::now();
        for _ in 0..200 {
            assert!(press(&plugins, "alt+r").is_some());
        }
        let pressed = t.elapsed() / 200;
        println!("the gesture lock asked in {asked:?}, a plugin's press found in {pressed:?}");
        // its other switch keeps a quick pinch from fitting the canvas, the turn's lock aside
        assert!(!hooks.gesture_lock(crate::hooks::GESTURE_FIT));
        assert!(plugins.set_switch(format!("{ROTATION}.no_fit"), true));
        assert!(hooks.gesture_lock(crate::hooks::GESTURE_FIT) && hooks.gesture_lock(crate::hooks::GESTURE_TURN));
        assert!(plugins.set_switch(format!("{ROTATION}.lock"), false));
        assert!(hooks.gesture_lock(crate::hooks::GESTURE_FIT) && !hooks.gesture_lock(crate::hooks::GESTURE_TURN));
        assert!(!hooks.gesture_lock(2), "a part it doesn't know");
        assert!(asked < std::time::Duration::from_micros(200), "{asked:?}");
        assert!(pressed < std::time::Duration::from_millis(1), "{pressed:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn value_check_shows_the_canvas_in_greys_from_its_switch_or_its_press() {
        const VALUES: &str = VIEW_TOOLS;
        let dir = store("bundled-values");
        let plugins = featured_app(&dir, VALUES, "value_check");
        let s = viewed();
        s.use_plugins(Arc::clone(&plugins));
        assert!(plugins.set_on(VALUES.to_owned(), true));
        // on, but its switch off: the colours as they are
        assert_eq!(plugins.hooks().view_look(), None);
        // Alt+G flips it: each colour the grey of its luminance, on the screen alone
        let flipped = press(&plugins, "alt+g");
        assert!(
            matches!(flipped, Some(PluginPressed::Switched { control: PluginControl::Switch { ref control } }) if control.on)
        );
        let grey = plugins.hooks().view_look().unwrap();
        assert_eq!(grey, procraft_gpu::GREY_LOOK);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(s.shared.lock().shown_look(), Some(procraft_gpu::GREY_LOOK));
        // asked as the plugins change (twelve entries): quick
        let hooks = plugins.hooks();
        let t = std::time::Instant::now();
        for _ in 0..200 {
            assert!(hooks.view_look().is_some());
        }
        let asked = t.elapsed() / 200;
        println!("the view look asked in {asked:?}");
        assert!(asked < std::time::Duration::from_millis(1), "{asked:?}");
        // stronger contrast spreads the greys from a middle grey; the grey view goes on top
        assert!(plugins.set_slider(format!("{VALUES}.contrast"), 2.0).is_some());
        let strong = plugins.hooks().view_look().unwrap();
        let mid = crate::view_look::through(&strong, [0.18; 3]);
        assert!(mid.iter().all(|v| (v - 0.18).abs() < 1e-5), "{mid:?}");
        // and off again: nothing shown through
        assert!(matches!(press(&plugins, "alt+g"), Some(PluginPressed::Switched { .. })));
        assert_eq!(plugins.hooks().view_look(), None);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(s.shared.lock().shown_look(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_paper_is_kept_with_each_artwork_and_given_to_its_canvas() {
        const PAPER: &str = INFINITE;
        let dir = store("bundled-paper");
        let plugins = featured_app(&dir, PAPER, "paper");
        assert!(plugins.set_on(PAPER.to_owned(), true));
        let paper_of = |s: &crate::CanvasSession| s.shared.lock().plugin_paper.clone();
        // an artwork starts with none
        let one = crate::CanvasSession::new(64, 48);
        one.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&one), None);
        // chosen for it: given to its canvas, and kept with it
        let edits = one.edits();
        assert!(plugins.set_slider(format!("{PAPER}.kind"), 3.0).is_some());
        assert!(plugins.set_slider(format!("{PAPER}.colour"), 2.0).is_some());
        assert_ne!(one.edits(), edits);
        // its tile drawn once a change, and given as drawn after: what the app asks each time the
        // plugins change costs little (`paper_tile_cost` bounds the drawing)
        let hooks = plugins.hooks();
        let t = std::time::Instant::now();
        let drawn = hooks.paper();
        let first = t.elapsed();
        // (the quickest of five asks: other work on the machine may hold up one alone)
        let kept = (0..5)
            .map(|_| {
                let t = std::time::Instant::now();
                let again = hooks.paper();
                let kept = t.elapsed();
                assert!(drawn.is_some() && drawn == again);
                kept
            })
            .min()
            .unwrap_or_default();
        println!("the paper asked in {first:?} drawing its tile, in {kept:?} drawn");
        assert!(kept * 10 < first, "{kept:?} drawn, {first:?} drawing its tile");
        one.use_plugins(Arc::clone(&plugins));
        let given = paper_of(&one).unwrap();
        assert_eq!(given.tile.len(), procraft_engine::textures::TEXTURE_SIZE.pow(2));
        assert!(given.tile.iter().any(|&h| h < 128) && given.tile.iter().any(|&h| h > 128));
        assert_eq!((given.strength, given.scale, given.depth), (0.6, 1.0, 0.3));
        let cream = [0xF4, 0xEC, 0xD8].map(|c| c as f32 / 255.0);
        assert_eq!(given.colour, Some(cream));
        let saved = std::env::temp_dir().join(format!("procraft-paper-artwork-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&saved);
        one.save(saved.to_string_lossy().into_owned()).unwrap();
        // another artwork has its own: none until chosen
        let two = crate::CanvasSession::new(64, 48);
        two.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&two), None);
        assert!(
            matches!(plugins.control(&format!("{PAPER}.kind")), Some(PluginControl::Slider { control }) if control.value == 0.0)
        );
        // the first opened again has its paper back
        let opened = crate::CanvasSession::open(saved.to_string_lossy().into_owned()).unwrap();
        opened.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&opened), Some(given));
        // None takes it away
        assert!(plugins.set_slider(format!("{PAPER}.kind"), 0.0).is_some());
        opened.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&opened), None);
        let _ = std::fs::remove_dir_all(&saved);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paper_from_the_textures_is_the_apps_grain_or_a_picture_of_your_own_and_its_panel_shows_it() {
        const PAPER: &str = INFINITE;
        let dir = store("bundled-paper-textures");
        let plugins = featured_app(&dir, PAPER, "paper");
        let s = crate::CanvasSession::new(64, 48);
        assert!(plugins.set_on(PAPER.to_owned(), true));
        // the artwork's settings taken up first, as an artwork opening does
        s.use_plugins(Arc::clone(&plugins));
        let paper_of = |s: &crate::CanvasSession| s.shared.lock().plugin_paper.clone();
        let texture = format!("{PAPER}.texture");
        // Textures, none picked: no paper
        assert!(plugins.set_slider(format!("{PAPER}.kind"), 7.0).is_some());
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&s), None);
        // the app's grain 19 of the Source Library: its tile, as it is
        assert_eq!(plugins.set_texture(texture.clone(), 19), Some(19));
        s.use_plugins(Arc::clone(&plugins));
        let n = procraft_engine::textures::TEXTURE_SIZE;
        assert_eq!(paper_of(&s).unwrap().tile, procraft_engine::textures::grain_at(18, n).unwrap());
        // a picture of your own picked into its second place, and picked from the textures: the
        // tile made of it (dark on its left half, light on its right)
        let side = 40_usize;
        let rgba: Vec<u8> =
            (0..side * side).flat_map(|i| if i % side < side / 2 { [60, 60, 60, 255] } else { [220; 4] }).collect();
        let png = crate::project::png_of(side as u32, side as u32, &rgba).unwrap();
        assert!(plugins.set_picture_file(format!("{PAPER}.picture2"), "weave.png".to_owned(), png));
        assert_eq!(plugins.set_texture(texture.clone(), -2), Some(-2));
        s.use_plugins(Arc::clone(&plugins));
        let given = paper_of(&s).unwrap();
        assert_eq!((given.tile[100 * n + 60], given.tile[100 * n + 200]), (0, 255));
        // its row in the Layers panel shows the texture's row under the Paper's
        assert!(plugins.run_action(format!("{PAPER}.settings"), Arc::clone(&s)));
        let rows = plugins.panel().unwrap().rows;
        assert_eq!(rows.get(1), Some(&crate::PanelRow::Control { key: texture.clone() }));
        // none picked again: no paper
        assert_eq!(plugins.set_texture(texture, 0), Some(0));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&s), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paper_of_your_own_picture_any_colour_and_a_tooth_reach_the_canvas_and_off_leave_it_as_it_was() {
        const PAPER: &str = INFINITE;
        let dir = store("bundled-paper-own");
        let plugins = featured_app(&dir, PAPER, "paper");
        let s = crate::CanvasSession::new(64, 48);
        s.use_plugins(Arc::clone(&plugins));
        let paper_of = |s: &crate::CanvasSession| s.shared.lock().plugin_paper.clone();
        // as it comes, off: no paper, whatever it is set to
        assert!(plugins.set_slider(format!("{PAPER}.kind"), 6.0).is_some());
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&s), None);
        assert!(plugins.set_on(PAPER.to_owned(), true));
        // Your own paper with no picture picked: none
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&s), None);
        // a picture, dark on its left half and light on its right: the tile made of it
        let side = 40_usize;
        let rgba: Vec<u8> =
            (0..side * side).flat_map(|i| if i % side < side / 2 { [60, 60, 60, 255] } else { [220; 4] }).collect();
        let png = crate::project::png_of(side as u32, side as u32, &rgba).unwrap();
        assert!(plugins.set_picture_file(format!("{PAPER}.picture"), "paper.png".to_owned(), png));
        let t = std::time::Instant::now();
        s.use_plugins(Arc::clone(&plugins));
        let made = t.elapsed();
        let given = paper_of(&s).unwrap();
        let n = procraft_engine::textures::TEXTURE_SIZE;
        assert_eq!(given.tile.len(), n * n);
        assert_eq!((given.tile[100 * n + 60], given.tile[100 * n + 200]), (0, 255));
        // made once: asked again, the same tile kept
        let first = s.shared.lock().paper_picture.clone().unwrap();
        let t = std::time::Instant::now();
        s.use_plugins(Arc::clone(&plugins));
        let kept = t.elapsed();
        println!("a paper's picture made its tile in {made:?}, kept in {kept:?}");
        assert!(Arc::ptr_eq(&first, s.shared.lock().paper_picture.as_ref().unwrap()));
        assert_eq!(paper_of(&s).unwrap().tile, given.tile);
        // its tooth and a colour of its own, as its controls say
        assert!(plugins.set_slider(format!("{PAPER}.tooth"), 0.5).is_some());
        assert!(plugins.set_switch(format!("{PAPER}.own_colour"), true));
        assert!(plugins.set_slider(format!("{PAPER}.brightness"), 0.5).is_some());
        s.use_plugins(Arc::clone(&plugins));
        let given = paper_of(&s).unwrap();
        assert_eq!(given.tooth, 0.5);
        let colour = given.colour.unwrap();
        assert!(colour.iter().all(|c| (0.43..0.51).contains(c)) && colour[0] > colour[2], "{colour:?}");
        // a paper drawn here again: its own tile, no picture kept
        assert!(plugins.set_slider(format!("{PAPER}.kind"), 1.0).is_some());
        s.use_plugins(Arc::clone(&plugins));
        assert_ne!(paper_of(&s).unwrap().tile, given.tile);
        assert!(s.shared.lock().paper_picture.is_none());
        // its feature off, the plugin on: none
        assert!(plugins.set_switch(format!("{PAPER}.paper"), false));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&s), None);
        assert!(plugins.set_switch(format!("{PAPER}.paper"), true));
        s.use_plugins(Arc::clone(&plugins));
        assert!(paper_of(&s).is_some());
        // off: as it was
        assert!(plugins.set_on(PAPER.to_owned(), false));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(paper_of(&s), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What drawing Paper's tile costs (paper 3, the first ask after it is chosen), the best of
    /// five, printed; fails past 150 ms (drawn point by point it took 200 ms here, four times as
    /// long on a tablet). Run alone (other work on the machine slows it):
    /// `cargo test -p procraft-core paper_tile_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn paper_tile_cost() {
        const PAPER: &str = INFINITE;
        let dir = store("bundled-paper-cost");
        let plugins = featured_app(&dir, PAPER, "paper");
        assert!(plugins.set_on(PAPER.to_owned(), true));
        let mut times = Vec::new();
        for _ in 0..5 {
            // another paper drawn between, so each time this one is drawn again
            for (paper, timed) in [(1.0, false), (3.0, true)] {
                assert!(plugins.set_slider(format!("{PAPER}.kind"), paper).is_some());
                let hooks = plugins.hooks();
                let t = std::time::Instant::now();
                assert!(hooks.paper().is_some());
                if timed {
                    times.push(t.elapsed());
                }
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        let best = times.iter().copied().min().unwrap_or_default();
        println!("paper 3's tile drawn in {best:?} at best: {times:?}");
        assert!(best < std::time::Duration::from_millis(150), "{best:?}");
    }

    #[test]
    fn reference_windows_lets_as_many_be_open_as_it_says() {
        const WINDOWS: &str = VIEW_TOOLS;
        let dir = store("bundled-references");
        let plugins = featured_app(&dir, WINDOWS, "reference_windows");
        assert_eq!(plugins.reference_windows(), 1);
        assert!(plugins.set_on(WINDOWS.to_owned(), true));
        assert_eq!(plugins.reference_windows(), 3);
        assert!(plugins.set_slider(format!("{WINDOWS}.windows"), 6.0).is_some());
        assert_eq!(plugins.reference_windows(), 6);
        // asked each frame on a desktop: quick
        let t = std::time::Instant::now();
        for _ in 0..1000 {
            assert_eq!(plugins.reference_windows(), 6);
        }
        let asked = t.elapsed() / 1000;
        println!("how many reference windows asked in {asked:?}");
        assert!(asked < std::time::Duration::from_micros(200), "{asked:?}");
        assert!(plugins.set_on(WINDOWS.to_owned(), false));
        assert_eq!(plugins.reference_windows(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coloured_previews_tint_the_library_s_strokes_in_the_colour_or_its_own() {
        const PREVIEWS: &str = INTERFACE_LOOK;
        let dir = store("bundled-coloured-previews");
        let plugins = featured_app(&dir, PREVIEWS, "coloured_previews");
        let s = crate::CanvasSession::new(10, 10);
        s.use_plugins(Arc::clone(&plugins));
        s.set_color(0.2, 0.4, 0.6);
        let tints = || (s.brush_preview_colour(false), s.brush_preview_colour(true));
        // it comes off: as the app draws them
        assert_eq!(tints(), (None, None));
        // on: in the colour painted with, the brush in use as the app draws it
        assert!(plugins.set_on(PREVIEWS.to_owned(), true));
        assert_eq!(tints(), (Some(0x336699), None));
        s.set_color(1.0, 0.0, 0.0);
        assert_eq!(tints(), (Some(0xFF0000), None));
        // the brush in use too, while its switch says
        assert!(plugins.set_switch(format!("{PREVIEWS}.chosen"), true));
        assert_eq!(tints(), (Some(0xFF0000), Some(0xFF0000)));
        // a colour of its own: it starts at 200°, 60% and 100%, then as its sliders say
        assert!(plugins.set_switch(format!("{PREVIEWS}.own_colour"), true));
        assert_eq!(tints(), (Some(0x66CCFF), Some(0x66CCFF)));
        let slider = |key: &str, value: f32| plugins.set_slider(format!("{PREVIEWS}.{key}"), value).unwrap();
        slider("preview_hue", 120.0);
        slider("preview_saturation", 1.0);
        slider("preview_brightness", 0.5);
        assert_eq!(tints().0, Some(0x008000));
        // asked as the library draws, each frame it shows: quick
        let t = std::time::Instant::now();
        for _ in 0..2000 {
            assert!(s.brush_preview_colour(false).is_some());
        }
        let each = t.elapsed() / 2000;
        println!("the library's tint asked in {each:?}");
        assert!(each < std::time::Duration::from_micros(200), "{each:?}");
        // off again: as the app draws them
        assert!(plugins.set_on(PREVIEWS.to_owned(), false));
        assert_eq!(tints(), (None, None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn colour_labels_label_a_layer_or_a_group_from_its_options_shown_on_its_row() {
        const LABELS: &str = COLOUR_TOOLS;
        let dir = store("bundled-colour-labels");
        let plugins = featured_app(&dir, LABELS, "colour_labels");
        let s = crate::CanvasSession::new(10, 10);
        s.use_plugins(Arc::clone(&plugins));
        let layer = s.add_layer();
        let current = || s.layers().into_iter().find(|l| l.current).unwrap();
        // it comes off: nothing in a layer's options
        assert!(plugins.actions_in(PluginPlace::Layer).is_empty());
        assert!(plugins.set_on(LABELS.to_owned(), true));
        let actions: Vec<String> = plugins.actions_in(PluginPlace::Layer).into_iter().map(|a| a.key).collect();
        assert_eq!(actions, [format!("{LABELS}.label")]);
        // Colour label: a panel of its eight colours, none chosen, and No label
        assert!(plugins.run_action(format!("{LABELS}.label"), Arc::clone(&s)));
        let panel = plugins.panel().expect("shown");
        assert_eq!(panel.title, "Colour label");
        let crate::PanelRow::Colours { colours, chosen, .. } = &panel.rows[0] else { panic!("{:?}", panel.rows) };
        assert_eq!((colours.len(), *chosen), (8, None));
        let blue = colours[5];
        assert_eq!(panel.rows[1], crate::PanelRow::Button { label: "No label".into() });
        // the sixth labels the layer blue, its row shows it, and the panel closes
        assert!(!plugins.panel_touched(0, 5.0, Arc::clone(&s)));
        assert_eq!(plugins.panel(), None);
        assert_eq!((current().id, current().label), (layer, Some(blue)));
        // kept as the mark Colour labels kept, so artworks labelled before show theirs still
        assert_eq!(s.layer_mark("io.github.artcraftandroid.colour-labels", layer), 6);
        assert_eq!(s.layer_mark(COLOUR_TOOLS, layer), 0);
        // shown again, it shows the layer's label chosen; No label takes it off
        assert!(plugins.run_action(format!("{LABELS}.label"), Arc::clone(&s)));
        let panel = plugins.panel().unwrap();
        assert!(matches!(panel.rows[0], crate::PanelRow::Colours { chosen: Some(5), .. }));
        assert!(!plugins.panel_touched(1, 1.0, Arc::clone(&s)));
        assert_eq!(current().label, None);
        // the other layers have none, and a label stays with the layer while the plugin is off
        assert!(plugins.run_action(format!("{LABELS}.label"), Arc::clone(&s)));
        assert!(!plugins.panel_touched(0, 0.0, Arc::clone(&s)));
        assert!(s.layers().iter().filter(|l| l.id != layer).all(|l| l.label.is_none()));
        assert!(plugins.set_on(LABELS.to_owned(), false));
        assert_eq!(current().label, None);
        assert!(plugins.set_on(LABELS.to_owned(), true));
        assert_eq!(current().label, Some(colours[0]));
        // a group too, its row shown so
        let group = s.group_picked();
        assert!(group != 0 && s.select_layer(group));
        assert!(plugins.run_action(format!("{LABELS}.label"), Arc::clone(&s)));
        assert!(!plugins.panel_touched(0, 3.0, Arc::clone(&s)));
        let row = s.layers().into_iter().find(|l| l.id == group).unwrap();
        assert_eq!((row.kind, row.label), (crate::LayerType::Group, Some(colours[3])));
        // a layer an artwork kept a label with before shows it
        let earlier = s.add_layer();
        assert!(s.set_layer_mark("io.github.artcraftandroid.colour-labels", earlier, 2));
        let row = s.layers().into_iter().find(|l| l.id == earlier).unwrap();
        assert_eq!(row.label, Some(colours[1]));
        // with Colour labels off, none shows, and the marks stay
        assert!(plugins.set_switch(format!("{COLOUR_TOOLS}.colour_labels"), false));
        assert!(s.layers().iter().all(|l| l.label.is_none()));
        assert_eq!(s.layer_mark("io.github.artcraftandroid.colour-labels", earlier), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pop_up_palette_opens_a_ring_of_recent_colours_and_brushes_at_the_pen() {
        const PALETTE: &str = "io.github.artcraftandroid.pop-up-palette";
        let dir = store("bundled-pop-up-palette");
        let plugins = app_at(&dir);
        let s = Arc::new(crate::CanvasSession::new(10, 10));
        s.use_plugins(Arc::clone(&plugins));
        let history = crate::ColourHistory::new();
        s.use_colour_history(Arc::clone(&history));
        for rgb in [0x0000FF, 0x00FF00, 0xFF0000] {
            history.painted(rgb);
        }
        s.set_color(0.0, 1.0, 0.0);
        for place in [(0, 3), (0, 1)] {
            s.shared.lock().note_recent(place);
        }
        assert!(s.select_brush(0, 1));
        let open = format!("{PALETTE}.open");
        // it comes off: nothing for Pen buttons
        assert!(plugins.actions_in(PluginPlace::Pen).is_empty());
        assert!(plugins.set_on(PALETTE.to_owned(), true));
        let listed: Vec<String> = plugins.actions_in(PluginPlace::Pen).into_iter().map(|a| a.label).collect();
        assert_eq!(listed, ["Pop-up palette"]);
        // at the pen: the colour in use, then the history after it; the Recent set's brushes, newest
        // first; the ones in use ringed
        let t = std::time::Instant::now();
        assert!(plugins.run_action_at(open.clone(), Arc::clone(&s), 300.0, 200.0));
        let opened = t.elapsed();
        let panel = plugins.panel().unwrap();
        assert!(panel.ring);
        assert_eq!(panel.at, Some(crate::PanelPoint { x: 300.0, y: 200.0 }));
        assert_eq!(
            panel.rows[0],
            crate::PanelRow::Colours {
                label: "Recent colours".into(),
                colours: vec![0x00FF00, 0xFF0000, 0x0000FF],
                chosen: Some(0)
            }
        );
        let crate::PanelRow::Brushes { brushes, chosen, .. } = &panel.rows[1] else { panic!("{:?}", panel.rows) };
        assert_eq!(brushes.iter().map(|b| (b.set, b.index)).collect::<Vec<_>>(), [(0, 1), (0, 3)]);
        assert_eq!(*chosen, Some(0));
        // a brush tapped: in use, and the ring closes
        let t = std::time::Instant::now();
        assert!(!plugins.panel_touched(1, 1.0, Arc::clone(&s)));
        let picked = t.elapsed();
        assert_eq!((s.current_brush().set, s.current_brush().index), (0, 3));
        assert!(plugins.panel().is_none());
        // kept open: a colour tapped paints in it, and the ring shows it chosen
        assert!(plugins.set_switch(format!("{PALETTE}.close"), false));
        assert!(plugins.run_action_at(open.clone(), Arc::clone(&s), 1.0, 2.0));
        assert!(plugins.panel_touched(0, 2.0, Arc::clone(&s)));
        assert_eq!(crate::history::hex(s.shared.lock().color), 0x0000FF);
        let panel = plugins.panel().unwrap();
        assert!(matches!(&panel.rows[0], crate::PanelRow::Colours { chosen: Some(2), .. }), "{:?}", panel.rows);
        assert!(matches!(&panel.rows[1], crate::PanelRow::Brushes { chosen: Some(1), .. }), "{:?}", panel.rows);
        // its sliders: fewer colours, no brushes
        plugins.set_slider(format!("{PALETTE}.colours"), 2.0).unwrap();
        plugins.set_slider(format!("{PALETTE}.brushes"), 0.0).unwrap();
        let t = std::time::Instant::now();
        assert!(plugins.run_action(open.clone(), Arc::clone(&s)));
        // the quicker of its two openings, so a busy machine's pauses don't count against it
        let opened = opened.min(t.elapsed());
        let panel = plugins.panel().unwrap();
        assert_eq!(panel.rows.len(), 1);
        assert!(matches!(&panel.rows[0], crate::PanelRow::Colours { colours, .. } if colours.len() == 2));
        println!("the palette opened in {opened:?} at the quicker, a brush picked in {picked:?}");
        assert!(opened < std::time::Duration::from_millis(50), "{opened:?}");
        assert!(picked < std::time::Duration::from_millis(50), "{picked:?}");
        // off: nothing for Pen buttons, and its action runs no more
        assert!(plugins.set_on(PALETTE.to_owned(), false));
        assert!(plugins.actions_in(PluginPlace::Pen).is_empty());
        assert!(!plugins.run_action_at(open, Arc::clone(&s), 1.0, 2.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_surround_gives_the_surround_its_colour_or_the_picture_picked_for_it() {
        const SURROUND: &str = INTERFACE_LOOK;
        let dir = store("bundled-canvas-surround");
        let plugins = featured_app(&dir, SURROUND, "canvas_surround");
        let s = crate::CanvasSession::new(10, 10);
        s.use_plugins(Arc::clone(&plugins));
        let surround = || s.shared.lock().surround();
        // it comes off: the interface's own grey, dark or light
        assert_eq!(surround(), crate::SURROUND);
        // on: its colour, the accent left as it is
        assert!(plugins.set_on(SURROUND.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        let near = |c: [f32; 3], rgb: u32| {
            let want = [rgb >> 16, (rgb >> 8) & 0xFF, rgb & 0xFF].map(|v| v as f32 / 255.0);
            c.iter().zip(want).all(|(c, w)| (c - w).abs() <= 1.0 / 255.0 + 1e-6)
        };
        assert!(near(surround(), 0x4D4945), "{:?}", surround());
        assert_eq!(plugins.accent_colour(), None);
        // black, from its sliders
        plugins.set_slider(format!("{SURROUND}.surround_brightness"), 0.0).unwrap();
        s.use_plugins(Arc::clone(&plugins));
        assert!(near(surround(), 0x000000), "{:?}", surround());
        // a picture: only with its switch on and one picked
        let picture = format!("{SURROUND}.picture");
        let png = crate::project::png_of(2, 2, &[0, 0, 255, 255].repeat(4)).unwrap();
        assert!(plugins.set_picture_file(picture.clone(), "blue.png".to_owned(), png));
        assert_eq!(plugins.surround_picture_file(), None, "its switch off");
        assert!(plugins.set_switch(format!("{SURROUND}.own_picture"), true));
        let file = plugins.surround_picture_file().unwrap();
        assert!(file.is_file());
        s.use_plugins(Arc::clone(&plugins));
        let shown = s.shared.lock().surround_picture.as_ref().map(|p| (p.width, p.height));
        assert_eq!(shown, Some((2, 2)));
        // a sound control takes no picture; the picture let go, none
        assert!(plugins.clear_file(picture));
        assert_eq!(plugins.surround_picture_file(), None);
        // off again: the interface's own
        assert!(plugins.set_on(SURROUND.to_owned(), false));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(surround(), crate::SURROUND);
        assert!(s.shared.lock().surround_picture.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slider_look_makes_the_tracks_see_through_and_frosted_as_its_controls_say() {
        const LOOK: &str = INTERFACE_LOOK;
        let dir = store("bundled-slider-look");
        let plugins = featured_app(&dir, LOOK, "slider_look");
        // it comes off: the app's own
        assert_eq!(plugins.slider_look(), SliderLook::default());
        assert!(plugins.set_on(LOOK.to_owned(), true));
        assert_eq!(plugins.slider_look(), SliderLook { track_opacity: Some(0.35), frosted: Some(true) });
        plugins.set_slider(format!("{LOOK}.slider_opacity"), 0.8).unwrap();
        assert!(plugins.set_switch(format!("{LOOK}.frosted"), false));
        let look = plugins.slider_look();
        assert!(look.track_opacity.is_some_and(|a| (a - 0.8).abs() < 1e-6), "{look:?}");
        assert_eq!(look.frosted, Some(false));
        assert!(plugins.set_on(LOOK.to_owned(), false));
        assert_eq!(plugins.slider_look(), SliderLook::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    const FINGER_PRESSURE: &str = BRUSH_EXTRAS;

    /// The pressure `hooks` give the last sample of a stroke by `pointer` (0 a finger) moving across
    /// at `speed` window points a second, a sample every 1/120 s for 400 ms.
    fn pressed_at(hooks: &crate::hooks::Hooks, pointer: i32, speed: f32) -> Option<f32> {
        let mut said = None;
        for i in 0..=48 {
            let ms = i as f32 * 1000.0 / 120.0;
            let asked = crate::plugin_run::PressureAsked {
                pointer,
                first: i == 0,
                x: speed * ms / 1000.0,
                y: 0.0,
                ms,
                pressure: 0.6,
            };
            said = hooks.pressure(asked);
        }
        said
    }

    #[test]
    fn finger_pressure_presses_a_finger_by_its_speed_tapers_it_and_leaves_pens_alone() {
        let dir = store("bundled-finger-pressure");
        let plugins = featured_app(&dir, FINGER_PRESSURE, "finger_pressure");
        let hooks = plugins.hooks();
        // as it comes, off: nothing pressed, nothing tapered
        assert_eq!(pressed_at(&hooks, 0, 1000.0), None);
        assert_eq!(hooks.pressure_tapers(0), [0.0; 2]);
        assert!(plugins.set_on(FINGER_PRESSURE.to_owned(), true));
        // the faster, the lighter: by the speed in window points a second
        let speeds = [0.0, 200.0, 500.0, 1000.0, 2000.0, 3000.0];
        let curve: Vec<f32> = speeds.iter().map(|&v| pressed_at(&hooks, 0, v).unwrap()).collect();
        println!("finger pressure at {speeds:?} points a second: {curve:?}");
        let want = [1.0, 0.976, 0.874, 0.685, 0.496, 0.433];
        for ((got, want), speed) in curve.iter().zip(want).zip(speeds) {
            assert!((got - want).abs() < 0.005, "at {speed}: {got}, not {want}");
        }
        // tapered in over 20 points and out over 40, a finger's stroke alone
        assert_eq!(hooks.pressure_tapers(0), [20.0, 40.0]);
        // asked for every sample: quick, even unoptimised (`finger_pressure_sample_cost` times it)
        let t = std::time::Instant::now();
        for _ in 0..20 {
            assert!(pressed_at(&hooks, 0, 1000.0).is_some());
        }
        let each = t.elapsed() / (20 * 49);
        println!("a finger's sample pressed in {each:?}");
        assert!(each < std::time::Duration::from_micros(200), "{each:?}");
        // a pen, its eraser end and a mouse draw as they do
        for pointer in [1, 2, 3] {
            assert_eq!(pressed_at(&hooks, pointer, 1000.0), None, "{pointer}");
            assert_eq!(hooks.pressure_tapers(pointer), [0.0; 2], "{pointer}");
        }
        // its sliders: how much speed counts, the least and most pressure, the tapers
        let slider = |key: &str, value: f32| plugins.set_slider(format!("{FINGER_PRESSURE}.{key}"), value).unwrap();
        slider("speed", 0.0);
        assert_eq!(pressed_at(&hooks, 0, 3000.0), Some(1.0));
        slider("speed", 1.0);
        slider("least", 0.3);
        slider("most", 0.8);
        let still = pressed_at(&hooks, 0, 0.0).unwrap();
        assert!((still - 0.8).abs() < 1e-5, "{still}");
        let flying = pressed_at(&hooks, 0, 20_000.0).unwrap();
        assert!((flying - 0.3).abs() < 0.01, "{flying}");
        slider("start", 0.0);
        slider("end", 120.0);
        assert_eq!(hooks.pressure_tapers(0), [0.0, 120.0]);
        // off again: as it came
        assert!(plugins.set_on(FINGER_PRESSURE.to_owned(), false));
        assert_eq!((pressed_at(&hooks, 0, 1000.0), hooks.pressure_tapers(0)), (None, [0.0; 2]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What Finger pressure costs a sample, on the hook, with it on and a finger moving; and with it
    /// off, the list on the session: printed, and failing past 20 µs a sample (a 240 Hz pen sends one
    /// every 4,000). Run alone, in release: `cargo test --release -p procraft-core
    /// finger_pressure_sample_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn finger_pressure_sample_cost() {
        let dir = store("bundled-finger-pressure-cost");
        let plugins = featured_app(&dir, FINGER_PRESSURE, "finger_pressure");
        let hooks = plugins.hooks();
        const SAMPLES: u32 = 200_000;
        let cost = |pointer: i32| {
            let t = std::time::Instant::now();
            let mut sum = 0.0;
            for i in 0..SAMPLES {
                let ms = i as f32 * 4.0;
                let asked = crate::plugin_run::PressureAsked {
                    pointer,
                    first: i % 500 == 0,
                    x: std::hint::black_box(ms * 1.5 % 2000.0),
                    y: 300.0,
                    ms,
                    pressure: 0.5,
                };
                sum += hooks.pressure(asked).unwrap_or(0.0);
            }
            std::hint::black_box(sum);
            t.elapsed().as_secs_f64() * 1e6 / f64::from(SAMPLES)
        };
        let off = (0..3).map(|_| cost(0)).fold(f64::MAX, f64::min);
        assert!(plugins.set_on(FINGER_PRESSURE.to_owned(), true));
        let finger = (0..3).map(|_| cost(0)).fold(f64::MAX, f64::min);
        let pen = (0..3).map(|_| cost(1)).fold(f64::MAX, f64::min);
        println!("a sample through the pressure hook: {off:.3} µs off, {finger:.3} µs a finger's, {pen:.3} µs a pen's");
        assert!(finger < 20.0 && pen < 20.0, "{finger} {pen}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    const COLOUR_EXTRAS: &str = INFINITE;

    #[test]
    fn colour_extras_steps_the_sliders_and_shows_the_colour_before_each_by_its_switch() {
        let dir = store("bundled-colour-extras");
        let plugins = featured_app(&dir, COLOUR_EXTRAS, "colour_extras");
        // as it comes, off: the panel as it is
        assert!(!card(&plugins, COLOUR_EXTRAS).on);
        assert_eq!((plugins.colour_steps(), plugins.colour_before()), (false, false));
        assert!(plugins.set_on(COLOUR_EXTRAS.to_owned(), true));
        // on, each by its switch, both off as it starts
        assert_eq!((plugins.colour_steps(), plugins.colour_before()), (false, false));
        assert!(plugins.set_switch(format!("{COLOUR_EXTRAS}.steps"), true));
        assert_eq!((plugins.colour_steps(), plugins.colour_before()), (true, false));
        assert!(plugins.set_switch(format!("{COLOUR_EXTRAS}.compare"), true));
        assert_eq!((plugins.colour_steps(), plugins.colour_before()), (true, true));
        // off again: as it came, its switches kept for next time
        assert!(plugins.set_on(COLOUR_EXTRAS.to_owned(), false));
        assert_eq!((plugins.colour_steps(), plugins.colour_before()), (false, false));
        assert!(plugins.set_on(COLOUR_EXTRAS.to_owned(), true));
        assert_eq!((plugins.colour_steps(), plugins.colour_before()), (true, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    const TRACE_VIEW: &str = INFINITE;

    #[test]
    fn trace_view_shows_the_layer_alone_or_the_others_dimmed_from_its_keys_its_choice_or_its_row() {
        let dir = store("bundled-trace-view");
        let plugins = featured_app(&dir, TRACE_VIEW, "trace_view");
        let s = viewed();
        s.use_plugins(Arc::clone(&plugins));
        let near = |a: f32, b: f32| (a - b).abs() < 1e-6;
        let dim = |plugins: &Plugins| plugins.hooks().view_dim_others();
        // as it comes, off: nothing dimmed, and its keys no one's
        assert!(!card(&plugins, TRACE_VIEW).on);
        assert_eq!((dim(&plugins), s.shared.lock().plugin_dim), (0.0, 0.0));
        assert_eq!(press(&plugins, "alt+t"), None);
        assert!(plugins.set_on(TRACE_VIEW.to_owned(), true));
        // on, Normal as it starts: nothing dimmed; a row in the Layers panel, a choice in the Canvas tab
        assert_eq!(dim(&plugins), 0.0);
        let rows: Vec<String> = plugins.actions_in(PluginPlace::Layers).into_iter().map(|a| a.key).collect();
        assert_eq!(rows, [format!("{TRACE_VIEW}.views")]);
        let keys = |controls: Vec<PluginControl>| controls.iter().map(|c| c.key().to_owned()).collect::<Vec<_>>();
        assert_eq!(keys(plugins.controls_in(PluginPlace::Canvas)), [format!("{TRACE_VIEW}.view")]);
        // Alt+T: Trace, the others dimmed 70%, on the screen alone
        let trace = format!("{TRACE_VIEW}.trace");
        assert_eq!(press(&plugins, "alt+t"), Some(PluginPressed::Action { key: trace.clone() }));
        assert!(plugins.run_action(trace.clone(), Arc::clone(&s)));
        assert!(near(dim(&plugins), 0.7));
        s.use_plugins(Arc::clone(&plugins));
        assert!(near(s.shared.lock().plugin_dim, 0.7));
        // Alt+S: Solo, the others not shown at all; again: Normal
        let solo = format!("{TRACE_VIEW}.solo");
        assert_eq!(press(&plugins, "alt+s"), Some(PluginPressed::Action { key: solo.clone() }));
        assert!(plugins.run_action(solo.clone(), Arc::clone(&s)));
        assert_eq!(dim(&plugins), 1.0);
        assert!(plugins.run_action(solo, Arc::clone(&s)));
        assert_eq!(dim(&plugins), 0.0);
        // asked as the plugins change: quick
        assert!(plugins.run_action(trace, Arc::clone(&s)));
        let hooks = plugins.hooks();
        let t = std::time::Instant::now();
        for _ in 0..1000 {
            assert!(hooks.view_dim_others() > 0.0);
        }
        let asked = t.elapsed() / 1000;
        println!("the dimming hook asked in {asked:?}");
        assert!(asked < std::time::Duration::from_micros(200), "{asked:?}");
        // the choice in the Canvas tab, and Trace's dim from its slider
        assert!(plugins.set_slider(format!("{TRACE_VIEW}.view"), 0.0).is_some());
        assert_eq!(dim(&plugins), 0.0);
        assert!(plugins.set_slider(format!("{TRACE_VIEW}.view"), 2.0).is_some());
        assert!(plugins.set_slider(format!("{TRACE_VIEW}.dim"), 0.4).is_some());
        assert!(near(dim(&plugins), 0.4));
        // the plugin off: nothing dimmed
        assert!(plugins.set_on(TRACE_VIEW.to_owned(), false));
        assert_eq!(dim(&plugins), 0.0);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(s.shared.lock().plugin_dim, 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lasso_fill_s_brushes_join_the_library_once_it_is_open_and_once_only() {
        let dir = store("bundled-lasso-fill-brushes");
        let brushes = std::path::Path::new(&dir).join("brushes").to_string_lossy().into_owned();
        let plugins = featured_app(&dir, LASSO_FILL, "lasso_fill");
        let fills = |s: &crate::CanvasSession| s.shared.lock().library.iter().filter(|set| set.name == "Fills").count();
        // the import screen says it comes with brushes
        let file = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "infinite-plugins").unwrap().1;
        assert!(plugins.inspect(file.to_vec()).message.contains("comes with a set of brushes"));
        // as it comes, off: no brushes of its
        let s = viewed();
        assert!(!s.open_brushes(brushes.clone()));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&s), 0);
        assert!(!s.take_brushes_changed());
        let sets = s.shared.lock().library.len();
        // on, Lasso fill's switch off: none either
        assert!(plugins.set_switch(format!("{LASSO_FILL}.lasso_fill"), false));
        assert!(plugins.set_on(LASSO_FILL.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&s), 0);
        assert!(!s.take_brushes_changed());
        // its switch on: its Fills at the top of the library, each filling by the plugin's row; the
        // app told the library changed, every set a place on, to read it again
        assert!(plugins.set_switch(format!("{LASSO_FILL}.lasso_fill"), true));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(s.shared.lock().library.len(), sets + 1);
        assert!(s.take_brushes_changed());
        assert!(!s.take_brushes_changed(), "told once");
        let row = card(&plugins, LASSO_FILL).brush_controls[0].key.clone();
        {
            let st = s.shared.lock();
            let set = &st.library[0];
            let names: Vec<&str> = set.brushes.iter().map(|b| b.name.as_str()).collect();
            assert_eq!(set.name, "Fills");
            assert_eq!(
                names,
                ["Solid Fill", "Screentone Fill", "Debris Fill", "Grunge Fill", "Scratch Fill", "Checker Fill"]
            );
            assert!(set.brushes.iter().all(|b| b.setting(&row) == 1.0), "{row}");
            // the solid one plain, the rest with a grain
            let grained = |b: &procraft_engine::brush::Brush| b.grain.image.is_some() || b.grain.source.is_some();
            assert!(!grained(&set.brushes[0]) && set.brushes[1..].iter().all(grained));
        }
        // again: nothing given again, nothing to read again; off, the set goes (see
        // `lasso_fill_s_brushes_come_and_go_with_its_switch_as_they_were_left`), and on again it
        // comes back: one set
        s.use_plugins(Arc::clone(&plugins));
        assert!(!s.take_brushes_changed(), "nothing given again, nothing to read again");
        assert!(plugins.set_on(LASSO_FILL.to_owned(), false));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&s), 0);
        assert!(plugins.set_on(LASSO_FILL.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&s), 1);
        assert_eq!(s.shared.lock().library.len(), sets + 1);
        assert!(s.take_brushes_changed(), "the library read again");
        // kept with the library: the brushes opened again have it, and aren't given it twice (told
        // the plugins before the brushes open, as Android does)
        let t = viewed();
        t.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&t), 0, "not given before the brushes are open");
        assert!(t.open_brushes(brushes.clone()));
        assert_eq!(fills(&t), 1);
        // deleted, it stays deleted
        let at = t.shared.lock().library.iter().position(|set| set.name == "Fills").unwrap();
        assert!(t.delete_brush_set(at as u32));
        assert!(t.keep_brushes(brushes.clone()));
        let u = viewed();
        assert!(u.open_brushes(brushes));
        u.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&u), 0);
        // brushes of their own elsewhere are given it once they open
        let elsewhere = std::path::Path::new(&dir).join("elsewhere").to_string_lossy().into_owned();
        let v = viewed();
        v.use_plugins(Arc::clone(&plugins));
        assert!(!v.open_brushes(elsewhere));
        assert_eq!(fills(&v), 1);
        // brushes given the set by the Lasso fill plugin before Infinite Plugins took its place
        // aren't given it again: it is the same set
        let before = std::path::Path::new(&dir).join("before");
        std::fs::create_dir_all(&before).unwrap();
        std::fs::write(before.join("plugin-sets.txt"), "io.github.artcraftandroid.lasso-fill\n").unwrap();
        let w = viewed();
        w.use_plugins(Arc::clone(&plugins));
        assert!(!w.open_brushes(before.to_string_lossy().into_owned()));
        assert_eq!(fills(&w), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A feature's brushes come and go with its switch, and with its plugin: off, the set is hidden,
    /// not deleted: it waits in a file of its own beside the brushes, as it was left (a brush
    /// renamed, one put in it, one moved to its top, the set moved down the library), and is out of
    /// the library, which is kept without it (a restart finds it still hidden); the paint tool on one
    /// of its brushes goes to another. On again, it shows again as it was left, at the place it had,
    /// and the file it waited in goes. A set given by an earlier version (not marked as the
    /// plugin's) is found by its name and comes and goes too; a copy of the set is the person's own
    /// and stays.
    #[test]
    fn lasso_fill_s_brushes_come_and_go_with_its_switch_as_they_were_left() {
        let dir = store("bundled-lasso-fill-come-and-go");
        let brushes = std::path::Path::new(&dir).join("brushes");
        let parked = || std::fs::read_dir(brushes.join("plugin-sets")).map_or(0, |d| d.flatten().count());
        let brushes = brushes.to_string_lossy().into_owned();
        let plugins = featured_app(&dir, LASSO_FILL, "lasso_fill");
        assert!(plugins.set_on(LASSO_FILL.to_owned(), true));
        let named =
            |s: &crate::CanvasSession, name: &str| s.brush_library().iter().filter(|set| set.name == name).count();
        let fills = |s: &crate::CanvasSession| named(s, "Fills");
        let switch = |on: bool| assert!(plugins.set_switch(format!("{LASSO_FILL}.lasso_fill"), on));
        let s = viewed();
        s.use_plugins(Arc::clone(&plugins));
        assert!(!s.open_brushes(brushes.clone()));
        let sets = s.brush_library().len();
        assert_eq!((fills(&s), s.brush_library()[0].name.as_str()), (1, "Fills"));
        let _ = s.take_brushes_changed();
        // a brush of it renamed, one of the stock brushes put in it, and the paint tool on its second
        assert!(s.rename_brush(0, 0, "My Solid".to_owned()));
        let stock = s.brush_library()[1].brushes[0].name.clone();
        assert!(s.move_brush(1, 0, 0, 1));
        assert!(s.select_brush(0, 2));
        assert_eq!(s.current_brush().name, "Screentone Fill");
        let left: Vec<String> = s.brush_library()[0].brushes.iter().map(|b| b.name.clone()).collect();
        assert_eq!(left[..3], ["My Solid".to_owned(), stock.clone(), "Screentone Fill".to_owned()]);
        // one of its brushes moved to its top, and the set moved down the library
        assert!(s.move_brush(0, 4, 0, 0));
        assert!(s.move_brush_set(0, 2));
        let left: Vec<String> = s.brush_library()[2].brushes.iter().map(|b| b.name.clone()).collect();
        assert_eq!(left[..4], ["Grunge Fill".to_owned(), "My Solid".to_owned(), stock, "Screentone Fill".to_owned()]);
        assert_eq!(s.current_brush().name, "Screentone Fill");
        // the switch off: the set goes, the paint tool on another brush, the set kept aside
        switch(false);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!((fills(&s), s.brush_library().len()), (0, sets - 1));
        assert!(s.take_brushes_changed(), "the library read again");
        assert_ne!(s.current_brush().name, "Screentone Fill");
        assert_eq!(parked(), 1);
        // the library kept without it: opened again, still away
        let t = viewed();
        t.use_plugins(Arc::clone(&plugins));
        assert!(t.open_brushes(brushes.clone()));
        assert_eq!(fills(&t), 0);
        // on again: shown again as it was left, at the place it had, and its file gone
        switch(true);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!((fills(&s), s.brush_library().len()), (1, sets));
        assert!(s.take_brushes_changed());
        assert_eq!(s.brush_library()[2].name, "Fills");
        let back: Vec<String> = s.brush_library()[2].brushes.iter().map(|b| b.name.clone()).collect();
        assert_eq!(back, left);
        // (its file goes once the library is kept with it, on a thread of its own)
        crate::brush_library::plugin_sets_kept();
        assert_eq!(parked(), 0);
        // and opened again (a restart), there
        let u = viewed();
        u.use_plugins(Arc::clone(&plugins));
        assert!(u.open_brushes(brushes.clone()));
        assert_eq!(fills(&u), 1);
        // the plugin off: it goes too; on, back
        assert!(plugins.set_on(LASSO_FILL.to_owned(), false));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&s), 0);
        assert!(plugins.set_on(LASSO_FILL.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&s), 1);
        // a copy of it stays when the switch goes off, as the person's own
        let at = s.brush_library().iter().position(|set| set.name == "Fills").unwrap() as u32;
        let copy = s.duplicate_brush_set(at).unwrap();
        assert_eq!(fills(&s), 2);
        switch(false);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&s), 1, "the copy stays");
        assert!(s.delete_brush_set(copy));
        switch(true);
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&s), 1);
        assert!(s.keep_brushes(brushes.clone()));
        // a set given before sets were marked as a plugin's (the mark taken out of the library's
        // file, as an earlier version kept it) comes and goes too
        let library = std::path::Path::new(&brushes).join("library.json");
        let text = std::fs::read_to_string(&library).unwrap();
        let unmarked = text.replace(&format!(r#","plugin":"{}""#, "io.github.artcraftandroid.lasso-fill"), "");
        assert_ne!(unmarked, text, "the library's file marks the set as the plugin's");
        std::fs::write(&library, unmarked).unwrap();
        let v = viewed();
        v.use_plugins(Arc::clone(&plugins));
        assert!(v.open_brushes(brushes.clone()));
        assert_eq!(fills(&v), 1);
        switch(false);
        v.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&v), 0, "found by its name");
        switch(true);
        v.use_plugins(Arc::clone(&plugins));
        assert_eq!(fills(&v), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Switching each of Infinite Plugins' features on and off, as the apps do it (the switch set,
    /// then the canvas told the plugins and a frame drawn): how long the calling thread (the
    /// interface's) is held each time, and as the canvas is told the plugins again with nothing
    /// changed (as the desktop does on every change of a setting). Prints each. Before, every part
    /// of the plugin (its code, its brushes, its filters) was read and hashed whole each time it
    /// was read, and the library kept as Lasso fill's brushes came or went: a feature switched on
    /// held a debug build's calling thread 150 to 500 ms, and telling the canvas again 60 to 200 ms.
    /// Bounds (a debug build's): a feature switched under 150 ms, telling it again under 50 ms; a
    /// paper chosen, its tile drawn by the plugin's own code, is printed.
    #[test]
    fn switching_infinite_plugins_features_on_holds_the_calling_thread_briefly() {
        use std::time::{Duration, Instant};
        let Some(s) = crate::tests::session(1600, 1600) else { return };
        let dir = store("bundled-switch-timing");
        let brushes = std::path::Path::new(&dir).join("brushes").to_string_lossy().into_owned();
        let plugins = app_at(&dir);
        s.use_plugins(Arc::clone(&plugins));
        s.open_brushes(brushes);
        let frame = |s: &crate::CanvasSession| {
            let t = Instant::now();
            let _ = crate::tests::frame_of(s, 1600);
            t.elapsed()
        };
        frame(&s);
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let mut longest = (Duration::ZERO, String::new());
        let time = |what: String, s: &crate::CanvasSession, change: &dyn Fn()| -> Duration {
            // (what an earlier step keeps on a thread of its own done first: nobody flips a switch
            // again within the tenth of a second that takes)
            crate::brush_library::plugin_sets_kept();
            let t = Instant::now();
            change();
            let switched = t.elapsed();
            let t = Instant::now();
            s.use_plugins(Arc::clone(&plugins));
            let told = t.elapsed();
            let drawn = frame(s);
            eprintln!("{what}: switch {:.1} ms, told {:.1} ms, frame {:.1} ms", ms(switched), ms(told), ms(drawn));
            switched + told + drawn
        };
        let mut switch = |what: String, change: &dyn Fn()| {
            let held = time(what.clone(), &s, change);
            if held > longest.0 {
                longest = (held, what);
            }
        };
        switch("plugin on".into(), &|| assert!(plugins.set_on(INFINITE.to_owned(), true)));
        for feature in ["lasso_fill", "paper", "trace_view", "filter_layers", "colour_extras"] {
            let key = format!("{INFINITE}.{feature}");
            switch(format!("{feature} on"), &|| assert!(plugins.set_switch(key.clone(), true)));
            switch(format!("{feature} off"), &|| assert!(plugins.set_switch(key.clone(), false)));
            switch(format!("{feature} on again"), &|| assert!(plugins.set_switch(key.clone(), true)));
        }
        // told again with nothing changed, every feature on
        let again = (0..3).map(|_| time("told again".into(), &s, &|| ())).max().unwrap_or_default();
        // a paper chosen, and the layer view's steps
        let slider = |key: &str, value: f32| assert!(plugins.set_slider(format!("{INFINITE}.{key}"), value).is_some());
        time("paper Cold press".into(), &s, &|| slider("kind", 1.0));
        time("paper Rough".into(), &s, &|| slider("kind", 3.0));
        time("layer view Solo".into(), &s, &|| slider("view", 1.0));
        time("layer view Trace".into(), &s, &|| slider("view", 2.0));
        eprintln!("longest switch: {} ({:.1} ms); told again {:.1} ms", longest.1, ms(longest.0), ms(again));
        crate::brush_library::plugin_sets_kept();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            longest.0 < Duration::from_millis(150),
            "{} held the calling thread {:.1} ms",
            longest.1,
            ms(longest.0)
        );
        assert!(again < Duration::from_millis(50), "told again, the calling thread held {:.1} ms", ms(again));
    }

    const LASSO_FILL: &str = INFINITE;

    #[test]
    fn lasso_fill_fills_every_stroke_while_its_switch_is_on_and_a_brush_s_own_by_its_row() {
        // the brush in use, its Fill row at `fill` (0 Off, 1 Fill what it encloses) and its Fill
        // shown row at `shown` (0 as it's drawn, 1 as the pen lifts)
        let brush = |fill: f32, shown: f32| {
            move |keys: &[String]| crate::stroke_sound::SoundBrush {
                name: "Studio pen".to_owned(),
                set: "Inking".to_owned(),
                controls: keys.iter().map(|k| if k.ends_with(":shown") { shown } else { fill }).collect(),
                key: u64::from(fill.to_bits()) << 32 | u64::from(shown.to_bits()),
            }
        };
        let dir = store("bundled-lasso-fill");
        let plugins = featured_app(&dir, LASSO_FILL, "lasso_fill");
        let hooks = plugins.hooks();
        // as it comes, off: no stroke fills, whatever the brush says
        assert!(!card(&plugins, LASSO_FILL).on);
        assert_eq!(hooks.stroke_fill(0, 1, brush(1.0, 0.0)), 0);
        assert!(!hooks.listens(crate::plugin_run::Hook::StrokeFill));
        assert!(plugins.set_on(LASSO_FILL.to_owned(), true));
        // on, its switch off and the brush's row Off: strokes paint as they did
        let row: Vec<(String, usize)> =
            card(&plugins, LASSO_FILL).brush_controls.iter().map(|c| (c.label.clone(), c.choices.len())).collect();
        assert_eq!(row, [("Fill".to_owned(), 2), ("Fill shown".to_owned(), 2)]);
        assert_eq!(hooks.stroke_fill(0, 1, brush(0.0, 0.0)), 0);
        // a brush whose Fill row says: its strokes fill, shown as they grow, or filled as the pen
        // lifts as its Fill shown row says; the eraser's only with Eraser too
        assert_eq!(hooks.stroke_fill(0, 1, brush(1.0, 0.0)), 1);
        assert_eq!(hooks.stroke_fill(0, 1, brush(1.0, 1.0)), 2);
        assert_eq!(hooks.stroke_fill(1, 1, brush(1.0, 0.0)), 0);
        assert!(plugins.set_switch(format!("{LASSO_FILL}.eraser"), true));
        assert_eq!(hooks.stroke_fill(1, 2, brush(1.0, 0.0)), 1);
        // every stroke while Lasso fill is on, a finger's too; with Show the fill while drawing
        // off, shown as a plain stroke and filled as it ends
        assert!(plugins.set_switch(format!("{LASSO_FILL}.every"), true));
        assert!(plugins.set_switch(format!("{LASSO_FILL}.growing"), false));
        assert_eq!((hooks.stroke_fill(0, 0, brush(0.0, 0.0)), hooks.stroke_fill(1, 0, brush(0.0, 0.0))), (2, 2));
        // asked once a stroke: quick, even unoptimised
        let t = std::time::Instant::now();
        for i in 0..1000 {
            std::hint::black_box(hooks.stroke_fill(i % 2, 1, brush((i % 2) as f32, 0.0)));
        }
        let each = t.elapsed() / 1000;
        println!("a stroke asked whether it fills in {each:?}");
        assert!(each < std::time::Duration::from_micros(500), "{each:?}");
        // off again: as it came
        assert!(plugins.set_on(LASSO_FILL.to_owned(), false));
        assert_eq!(hooks.stroke_fill(0, 1, brush(1.0, 0.0)), 0);
        assert!(!hooks.listens(crate::plugin_run::Hook::StrokeFill));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_nowhere_to_keep_them_nothing_is_added() {
        let plugins = Plugins::new();
        plugins.add_bundled(crate::bundled::BUNDLED);
        assert!(plugins.plugins().is_empty());
    }

    #[test]
    fn a_kept_plugin_tampered_with_counts_as_absent_and_nothing_breaks() {
        let dir = store("tampered");
        let plugins = at(&dir);
        assert_eq!(plugins.install(filter_file(&filter("org.example.filter"))), None);
        let shader = std::path::Path::new(&dir).join("org.example.filter/1.0.0/filter.wgsl");
        std::fs::write(shader, b"fn pixel() {}").unwrap();
        let next = at(&dir);
        assert!(filters(&next).is_empty());
        // a value kept for it finds nothing to go to
        next.read_kept("org.example.filter.cut".to_owned(), "0.1".to_owned());
        assert!(filters(&next).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    const A: &str = "org.example.a";
    const B: &str = "org.example.b";
    const C: &str = "org.example.c";

    /// The ids on the Plugins page, the top first.
    fn page(plugins: &Plugins) -> Vec<String> {
        plugins.plugins().into_iter().map(|p| p.id).collect()
    }

    /// A store holding filters `ids`, imported in that order.
    fn store_of(name: &str, ids: &[&str]) -> String {
        let dir = store(name);
        let plugins = at(&dir);
        for id in ids {
            assert_eq!(plugins.install(filter_file(&filter(id))), None);
        }
        dir
    }

    /// What is left half written in `dir`.
    fn left_beside(dir: &str) -> Vec<String> {
        let names = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned());
        names.filter(|n| n.starts_with('.')).collect()
    }

    #[test]
    fn plugins_moved_on_the_page_stay_in_that_order_through_a_restart() {
        let dir = store_of("order", &[A, B, C]);
        let plugins = at(&dir);
        assert_eq!(page(&plugins), [A, B, C]);
        // the last to the top, then the first of the others past the end: the last
        assert_eq!(plugins.move_plugin(C.to_owned(), 0), None);
        assert_eq!(page(&plugins), [C, A, B]);
        assert_eq!(plugins.move_plugin(A.to_owned(), 99), None);
        assert_eq!(page(&plugins), [C, B, A]);
        // nothing for one that isn't here, nor for one put where it is
        assert_eq!(plugins.move_plugin("org.example.gone".to_owned(), 0), None);
        assert_eq!(plugins.move_plugin(B.to_owned(), 1), None);
        assert_eq!(page(&plugins), [C, B, A]);
        // kept whole, nothing left beside it
        let next = at(&dir);
        assert_eq!(page(&next), [C, B, A]);
        assert!(left_beside(&dir).is_empty(), "{:?}", left_beside(&dir));
        // only the page goes by it: the Adjustments menu lists them as ever
        let menu: Vec<String> = next.filters().into_iter().map(|p| p.id).collect();
        assert_eq!(menu, [A, B, C]);
        // one imported since comes after them; one removed leaves the rest as they were
        assert_eq!(next.install(filter_file(&filter("org.example.d"))), None);
        assert!(next.remove(B.to_owned()).is_some());
        assert_eq!(page(&next), [C, A, "org.example.d"]);
        assert_eq!(page(&at(&dir)), [C, A, "org.example.d"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_store_kept_before_the_order_was_reads_as_it_did() {
        // imported in no order: listed by id, as before
        let dir = store_of("no-order", &[C, A, B]);
        assert!(!dir_holds(&dir, "order"));
        assert_eq!(page(&at(&dir)), [A, B, C]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_order_damaged_anyhow_keeps_what_it_can_and_the_rest_come_after() {
        let dir = store_of("order-damaged", &[A, B, C]);
        let note = std::path::Path::new(&dir).join("order");
        // junk, an id twice, one gone, blank lines and spaces: C, then the rest as listed
        std::fs::write(
            &note,
            "not an id\n\n  org.example.c  \nORG.EXAMPLE.A\norg.example.gone\norg.example.c\n\u{fffd}",
        )
        .unwrap();
        assert_eq!(page(&at(&dir)), [C, A, B]);
        // too big to be one: none
        std::fs::write(&note, format!("{B}\n").repeat(10_000)).unwrap();
        assert_eq!(page(&at(&dir)), [A, B, C]);
        // not a file: none, and moving one says once that it isn't kept
        std::fs::remove_file(&note).unwrap();
        std::fs::create_dir_all(note.join("in the way")).unwrap();
        let plugins = at(&dir);
        assert_eq!(page(&plugins), [A, B, C]);
        assert_eq!(plugins.move_plugin(B.to_owned(), 0).as_deref(), Some(ORDER_UNKEPT));
        assert_eq!(plugins.move_plugin(C.to_owned(), 0), None, "said once");
        assert_eq!(page(&plugins), [C, B, A], "shown meanwhile");
        assert!(left_beside(&dir).is_empty(), "{:?}", left_beside(&dir));
        // kept again once it can be, and said again if it can't afterwards
        std::fs::remove_dir_all(&note).unwrap();
        assert_eq!(plugins.move_plugin(A.to_owned(), 0), None);
        assert_eq!(page(&at(&dir)), [A, C, B]);
        std::fs::remove_file(&note).unwrap();
        std::fs::create_dir_all(note.join("in the way")).unwrap();
        assert_eq!(plugins.move_plugin(B.to_owned(), 0).as_deref(), Some(ORDER_UNKEPT));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- the plugins the app came with, made into five of features

    /// A plugin the app came with before (or one those took the place of), as its settings were
    /// kept: its id's last part, the plugin taking its place now and the switches its being on turns
    /// on there (a feature first), and each of its values someone changed, as kept (its key's last
    /// part, or a setting the app used to have, whole), with the key's last part it is kept under now.
    struct Old {
        name: &'static str,
        now: &'static str,
        switches: &'static [&'static str],
        values: &'static [(&'static str, &'static str, &'static str)],
    }

    const OLD: &[Old] = &[
        Old {
            name: "colour-triangle",
            now: COLOUR_TOOLS,
            switches: &["colour_triangle"],
            values: &[
                ("triangle", "true", "triangle"),
                ("ring", "true", "ring"),
                ("picker_only", "true", "picker_only"),
            ],
        },
        Old { name: "colour-labels", now: COLOUR_TOOLS, switches: &["colour_labels"], values: &[] },
        Old {
            name: "colour-extras",
            now: INFINITE,
            switches: &["colour_extras"],
            values: &[("steps", "true", "steps"), ("compare", "true", "compare")],
        },
        Old {
            name: "colordrop-tools",
            now: COLOUR_TOOLS,
            switches: &["colordrop"],
            values: &[
                ("all_layers", "true", "all_layers"),
                ("gap_closing", "true", "gap_closing"),
                ("fill_gap", "6", "fill_gap"),
            ],
        },
        Old { name: "fill-all-layers", now: COLOUR_TOOLS, switches: &["colordrop", "all_layers"], values: &[] },
        Old { name: "gap-closing", now: COLOUR_TOOLS, switches: &["colordrop", "gap_closing"], values: &[] },
        Old {
            name: "interface",
            now: INTERFACE_LOOK,
            switches: &["interface"],
            values: &[
                ("size_on", "true", "size_on"),
                ("interface_size", "0.85", "interface_size"),
                ("parts", "true", "parts"),
                ("floating", "0.5", "floating"),
                ("menus", "1.2", "menus"),
                ("side_bar", "0.8", "side_bar"),
                ("top_bar", "1.1", "top_bar"),
                ("panels", "0.9", "panels"),
                ("clear", "true", "clear"),
                ("colour", "true", "colour"),
                ("hue", "10", "hue"),
                ("saturation", "0.5", "saturation"),
                ("hide_undo", "true", "hide_undo"),
                ("hide_notices", "true", "hide_notices"),
            ],
        },
        Old {
            name: "slider-look",
            now: INTERFACE_LOOK,
            switches: &["slider_look"],
            values: &[("opacity", "0.6", "slider_opacity"), ("frosted", "false", "frosted")],
        },
        Old {
            name: "canvas-surround",
            now: INTERFACE_LOOK,
            switches: &["canvas_surround"],
            values: &[
                ("hue", "100", "surround_hue"),
                ("saturation", "0.5", "surround_saturation"),
                ("brightness", "0.2", "surround_brightness"),
                ("own", "true", "own_picture"),
            ],
        },
        Old {
            name: "floating-panels",
            now: EDIT_LAYOUT,
            switches: &["movable_panels"],
            values: &[("layers", "false", "float_layers"), ("brushes", "false", "float_brushes")],
        },
        Old {
            name: "coloured-previews",
            now: INTERFACE_LOOK,
            switches: &["coloured_previews"],
            values: &[
                ("own", "true", "own_colour"),
                ("hue", "50", "preview_hue"),
                ("saturation", "0.3", "preview_saturation"),
                ("brightness", "0.5", "preview_brightness"),
                ("chosen", "true", "chosen"),
            ],
        },
        Old {
            name: "interface-size",
            now: INTERFACE_LOOK,
            switches: &["interface", "size_on"],
            values: &[("clear", "true", "clear")],
        },
        Old {
            name: "interface-colour",
            now: INTERFACE_LOOK,
            switches: &["interface", "colour"],
            values: &[("hue", "20", "hue"), ("saturation", "0.4", "saturation")],
        },
        Old { name: "hide-undo-notice", now: INTERFACE_LOOK, switches: &["interface", "hide_undo"], values: &[] },
        Old { name: "pin-panels", now: EDIT_LAYOUT, switches: &["pin_panels"], values: &[] },
        Old {
            name: "value-check",
            now: VIEW_TOOLS,
            switches: &["value_check"],
            values: &[("values", "true", "values"), ("contrast", "1.5", "contrast")],
        },
        Old {
            name: "trace-view",
            now: INFINITE,
            switches: &["trace_view"],
            values: &[("view", "2", "view"), ("dim", "0.5", "dim")],
        },
        Old {
            name: "reference-windows",
            now: VIEW_TOOLS,
            switches: &["reference_windows"],
            values: &[("most", "5", "windows")],
        },
        Old {
            name: "rotation-reset",
            now: VIEW_TOOLS,
            switches: &["rotation_reset"],
            values: &[("to", "1", "to"), ("lock", "true", "lock"), ("no_fit", "true", "no_fit")],
        },
        Old { name: "randomise-brush", now: BRUSH_EXTRAS, switches: &["randomise"], values: &[] },
        Old {
            name: "brush-sounds",
            now: BRUSH_EXTRAS,
            switches: &["brush_sounds"],
            values: &[
                ("volume", "0.5", "volume"),
                ("sound", "3", "sound"),
                ("eraser", "false", "eraser"),
                ("eraser_sound", "4", "eraser_sound"),
            ],
        },
        Old {
            name: "finger-pressure",
            now: BRUSH_EXTRAS,
            switches: &["finger_pressure"],
            values: &[
                ("speed", "0.3", "speed"),
                ("start", "50", "start"),
                ("end", "10", "end"),
                ("least", "0.2", "least"),
                ("most", "0.9", "most"),
            ],
        },
        Old {
            name: "touch-offset",
            now: BRUSH_EXTRAS,
            switches: &["touch_offset"],
            values: &[("x", "12", "x"), ("y", "-7", "y")],
        },
        Old { name: "quickshape-curves", now: BRUSH_EXTRAS, switches: &["quickshape_curves"], values: &[] },
        Old {
            name: "layer-tools",
            now: LAYER_TOOLBOX,
            switches: &["layer_tools"],
            values: &[
                ("opacity", "true", "opacity"),
                ("paste_into_mask", "true", "paste_into_mask"),
                ("eyedropper_layer", "true", "eyedropper_layer"),
            ],
        },
        Old {
            name: "layer-fills",
            now: LAYER_TOOLBOX,
            switches: &["layer_fills"],
            values: &[
                ("grid", "true", "grid"),
                ("spacing", "50", "spacing"),
                ("line", "3", "line"),
                ("paper", "true", "paper"),
                ("grain", "0.7", "grain"),
                ("scale", "5", "scale"),
                ("warmth", "0.6", "warmth"),
            ],
        },
        Old {
            name: "effects",
            now: LAYER_TOOLBOX,
            switches: &["effects"],
            values: &[
                ("extract_lines", "true", "extract_lines"),
                ("cut", "0.3", "cut"),
                ("soft", "0.2", "soft"),
                ("keep_colour", "true", "keep_colour"),
                ("outline", "true", "outline"),
                ("reach", "5", "reach"),
                ("posterise", "true", "posterise"),
                ("steps", "6", "steps"),
            ],
        },
        Old { name: "opacity-together", now: LAYER_TOOLBOX, switches: &["layer_tools", "opacity"], values: &[] },
        Old { name: "paste-into-masks", now: LAYER_TOOLBOX, switches: &["layer_tools"], values: &[] },
        Old { name: "eyedropper-layer", now: LAYER_TOOLBOX, switches: &["layer_tools"], values: &[] },
        Old {
            name: "grid-layer",
            now: LAYER_TOOLBOX,
            switches: &["layer_fills", "grid"],
            values: &[("spacing", "80", "spacing"), ("line", "4", "line")],
        },
        Old { name: "paper-texture", now: LAYER_TOOLBOX, switches: &["layer_fills", "paper"], values: &[] },
        Old { name: "extract-lines", now: LAYER_TOOLBOX, switches: &["effects", "extract_lines"], values: &[] },
        Old { name: "outline", now: LAYER_TOOLBOX, switches: &["effects", "outline"], values: &[] },
        Old { name: "posterise", now: LAYER_TOOLBOX, switches: &["effects", "posterise"], values: &[] },
        Old {
            name: "lasso-fill",
            now: INFINITE,
            switches: &["lasso_fill"],
            values: &[("every", "true", "every"), ("growing", "false", "growing"), ("eraser", "true", "eraser")],
        },
        Old {
            name: "paper",
            now: INFINITE,
            switches: &["paper"],
            values: &[
                ("paper", "3", "kind"),
                ("colour", "2", "colour"),
                ("grain", "0.4", "grain"),
                ("size", "2", "size"),
                ("shown", "0.5", "shown"),
                ("tooth", "0.25", "tooth"),
                ("own_colour", "true", "own_colour"),
                ("hue", "200", "hue"),
                ("saturation", "0.3", "saturation"),
                ("brightness", "0.8", "brightness"),
                ("texture", "19", "texture"),
            ],
        },
    ];

    /// The key a value kept as `key` (its last part, or a setting the app used to have) of plugin
    /// `id` is kept under.
    fn kept_key(id: &str, key: &str) -> String {
        if moved(key) { key.to_owned() } else { format!("{id}.{key}") }
    }

    /// Whether a value kept as `got` is `wanted`, a number on its step as near as a float has it.
    fn same_value(got: &str, wanted: &str) -> bool {
        match (got.parse::<f32>(), wanted.parse::<f32>()) {
            (Ok(a), Ok(b)) => (a - b).abs() < 1e-4,
            _ => got == wanted,
        }
    }

    /// What control `key` holds as kept.
    fn kept_value(plugins: &Plugins, key: &str) -> String {
        plugins.control(key).map(|c| c.kept()).unwrap_or_else(|| panic!("no {key}"))
    }

    #[test]
    fn what_each_plugin_the_app_came_with_kept_comes_back_under_the_plugin_taking_its_place() {
        let dir = store("condensed");
        // every one the app came with before is taken over by one it comes with now, which says so
        for old in OLD {
            let id = format!("io.github.artcraftandroid.{}", old.name);
            let plugins = app_at(&dir);
            assert!(has(&plugins, old.now), "{}", old.now);
            let replaces = &plugins.listed().shapes.get(old.now).map(|s| s.replaces.clone()).unwrap_or_default();
            assert!(replaces.iter().any(|r| r.id == id), "{} takes the place of {id}", old.now);
            // on, with its values changed, as a start reads them back, in either order
            for reversed in [false, true] {
                let plugins = app_at(&dir);
                let mut lines = vec![format!("{id}.on true")];
                lines.extend(old.values.iter().map(|(key, value, _)| format!("{} {value}", kept_key(&id, key))));
                if reversed {
                    lines.reverse();
                }
                read_lines(&plugins, &lines);
                assert!(card(&plugins, old.now).on, "{id}: {} on", old.now);
                for switch in old.switches {
                    assert!(switch_of(&plugins, &format!("{}.{switch}", old.now)), "{id}: {switch}");
                }
                for (key, value, now) in old.values {
                    let got = kept_value(&plugins, &kept_key(old.now, now));
                    assert!(same_value(&got, value), "{id}: {key} as {now}: {got}, not {value}");
                }
                // and from then on kept as its own: its old keys no more
                let kept = plugins.kept_lines();
                assert!(kept.contains(&format!("{}.on true", old.now)), "{kept:?}");
                assert!(kept.iter().all(|line| !line.starts_with(&format!("{id}."))), "{id}: {kept:?}");
                // through the next start, as kept now
                let next = app_at(&dir);
                read_lines(&next, &kept);
                assert_eq!(next.plugins(), plugins.plugins(), "{id}");
            }
            // off, it turns on nothing, but what it had set is still taken
            let plugins = app_at(&dir);
            let mut lines = vec![format!("{id}.on false")];
            lines.extend(old.values.iter().map(|(key, value, _)| format!("{} {value}", kept_key(&id, key))));
            read_lines(&plugins, &lines);
            assert!(!card(&plugins, old.now).on, "{id} off");
            for (_, value, now) in old.values {
                let got = kept_value(&plugins, &kept_key(old.now, now));
                assert!(same_value(&got, value), "{id} off: {now}: {got}, not {value}");
            }
        }
        // fourteen in all, each off as the app comes
        let plugins = app_at(&dir);
        assert_eq!(plugins.plugins().len(), 14);
        assert!(plugins.plugins().iter().all(|p| !p.on));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A plugin as the app came with one before, `name`, with `controls`, as a file: a module that
    /// takes files, on the position hook.
    fn old_file(name: &str, controls: Value) -> Vec<u8> {
        let module = wat::parse_str(
            r#"(module (memory (export "memory") 1)
            (func (export "procraft_control") (param i32 f32))
            (func (export "procraft_file_buffer") (param i32 i32) (result i32) (i32.const 0))
            (func (export "procraft_file") (param i32 i32))
            (func (export "procraft_position") (param f32 f32) (result i64) (i64.const 0)))"#,
        )
        .unwrap();
        let mut manifest = coded(&format!("io.github.artcraftandroid.{name}"), &module);
        manifest["compat"]["procraft"]["interface"] = json!(11);
        manifest["controls"] = controls;
        pack(&manifest, &[(crate::plugin_run::MODULE, &module)])
    }

    /// Brush sounds as it came, its recordings among its controls.
    fn old_sounds() -> Vec<u8> {
        old_file(
            "brush-sounds",
            json!([
                { "key": "volume", "type": "slider", "label": "Volume", "least": 0, "most": 1, "step": 0.01, "starts": 1 },
                { "key": "pencil_recording", "type": "file", "label": "Pencil recording", "accepts": "sound" },
                { "key": "pen_recording", "type": "file", "label": "Pen recording", "accepts": "sound" },
            ]),
        )
    }

    const OLD_SOUNDS: &str = "io.github.artcraftandroid.brush-sounds";

    #[test]
    fn an_old_plugin_kept_on_the_disk_goes_whole_its_recordings_with_the_one_taking_its_place() {
        let dir = store("condensed-disk");
        let store_path = std::path::Path::new(&dir);
        // as an earlier app left it: Brush sounds on, a recording picked, its volume down
        let earlier = at(&dir);
        assert_eq!(earlier.install(old_sounds()), None);
        assert!(earlier.set_on(OLD_SOUNDS.to_owned(), true));
        assert_eq!(earlier.set_slider(format!("{OLD_SOUNDS}.volume"), 0.25), Some(0.25));
        let wav = crate::sound_file::write_wav(&[0.25; 800], 8000);
        assert!(earlier.set_sound_file(format!("{OLD_SOUNDS}.pencil_recording"), "Scratch".to_owned(), wav.clone()));
        let lines = earlier.kept_lines();
        // the app comes with Brush extras now: the old one goes from the disk, its recording first
        let now = app_with(&dir, &all_bundled());
        read_lines(&now, &lines);
        assert!(!has(&now, OLD_SOUNDS) && !dir_holds(&dir, OLD_SOUNDS));
        let extras = card(&now, BRUSH_EXTRAS);
        assert!(extras.on && switch_of(&now, &format!("{BRUSH_EXTRAS}.brush_sounds")), "{extras:?}");
        assert_eq!(slider_value(&now, &format!("{BRUSH_EXTRAS}.volume")), 0.25);
        let key = format!("{BRUSH_EXTRAS}.pencil_recording");
        let (file, name) = plugin_file::chosen_paths(store_path, BRUSH_EXTRAS, &key);
        assert_eq!(std::fs::read(&file).ok(), Some(crate::sound_file::write_wav(&[0.25; 800], 8000)));
        assert_eq!(std::fs::read_to_string(&name).ok().as_deref(), Some("Scratch"));
        let chosen = |plugins: &Plugins, key: &str| match plugins.control(key) {
            Some(PluginControl::File { control }) => control.chosen,
            other => panic!("{other:?}"),
        };
        assert_eq!(chosen(&now, &key).as_deref(), Some("Scratch"));
        assert_eq!(chosen(&now, &format!("{BRUSH_EXTRAS}.pen_recording")), None);
        // and so on the next start
        let next = app_at(&dir);
        read_lines(&next, &now.kept_lines());
        assert_eq!(chosen(&next, &key).as_deref(), Some("Scratch"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_plugin_whose_recordings_cant_be_carried_yet_stays_until_they_can() {
        let dir = store("condensed-stays");
        let store_path = std::path::Path::new(&dir);
        let earlier = at(&dir);
        assert_eq!(earlier.install(old_sounds()), None);
        let wav = crate::sound_file::write_wav(&[0.5; 800], 8000);
        assert!(earlier.set_sound_file(format!("{OLD_SOUNDS}.pencil_recording"), "Scratch".to_owned(), wav));
        // where the new one's recordings go, something in the way
        let blocked = store_path.join(BRUSH_EXTRAS);
        std::fs::create_dir_all(&blocked).unwrap();
        std::fs::write(blocked.join("chosen"), b"in the way").unwrap();
        let now = app_with(&dir, &all_bundled());
        // both here: nothing of the old one lost, its recording where it was
        assert!(has(&now, OLD_SOUNDS) && has(&now, BRUSH_EXTRAS));
        let (old_file, _) =
            plugin_file::chosen_paths(store_path, OLD_SOUNDS, &format!("{OLD_SOUNDS}.pencil_recording"));
        assert!(old_file.is_file());
        // out of the way, the next start carries it over and the old one goes
        std::fs::remove_file(blocked.join("chosen")).unwrap();
        let next = app_at(&dir);
        assert!(!has(&next, OLD_SOUNDS) && !dir_holds(&dir, OLD_SOUNDS));
        let (file, _) =
            plugin_file::chosen_paths(store_path, BRUSH_EXTRAS, &format!("{BRUSH_EXTRAS}.pencil_recording"));
        assert!(file.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_brush_s_sound_row_kept_for_brush_sounds_is_brush_extras_own() {
        let dir = store("condensed-rows");
        let plugins = app_at(&dir);
        assert!(plugins.set_on(BRUSH_EXTRAS.to_owned(), true));
        // shown with its feature alone, kept under the key brushes kept it under before
        assert!(card(&plugins, BRUSH_EXTRAS).brush_controls.is_empty());
        assert!(plugins.set_switch(format!("{BRUSH_EXTRAS}.brush_sounds"), true));
        let rows = card(&plugins, BRUSH_EXTRAS).brush_controls;
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["plugin:io.github.artcraftandroid.brush-sounds:sound"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- what moved into Infinite Plugins (interface 19)

    /// A feature taken into Infinite Plugins from a plugin that stays: that plugin, the feature's
    /// key there, and its values someone changed as kept there (its key's last part), each with the
    /// key's last part it is kept under now.
    /// A value someone changed: its key's last part there, as kept, and its key's last part now.
    type TakenValue = (&'static str, &'static str, &'static str);

    const TAKEN: &[(&str, &str, &[TakenValue])] = &[
        (VIEW_TOOLS, "trace_view", &[("view", "2", "view"), ("dim", "0.4", "dim")]),
        (LAYER_TOOLBOX, "filter_layers", &[]),
        (COLOUR_TOOLS, "colour_extras", &[("steps", "true", "steps"), ("compare", "true", "compare")]),
    ];

    #[test]
    fn what_a_feature_taken_from_a_plugin_that_stays_kept_comes_back_under_infinite_plugins() {
        let dir = store("infinite-taken");
        for (from, feature, values) in TAKEN {
            // as an earlier app kept it: the plugin's on or off, the feature's switch, its values
            let lines_of = |on: bool, feature_on: bool| {
                let mut lines = vec![format!("{from}.on {on}"), format!("{from}.{feature} {feature_on}")];
                lines.extend(values.iter().map(|(key, value, _)| format!("{from}.{key} {value}")));
                lines
            };
            let taken = |plugins: &Plugins, what: &str| {
                for (key, value, now) in *values {
                    let got = kept_value(plugins, &format!("{INFINITE}.{now}"));
                    assert!(same_value(&got, value), "{what}: {key} as {now}: {got}, not {value}");
                }
            };
            // the plugin and the feature on, in either order: Infinite Plugins on with the feature,
            // its values taken; the plugin it came from on as it was
            for reversed in [false, true] {
                let plugins = app_at(&dir);
                let mut lines = lines_of(true, true);
                if reversed {
                    lines.reverse();
                }
                read_lines(&plugins, &lines);
                assert!(card(&plugins, INFINITE).on, "{from}.{feature}");
                assert!(switch_of(&plugins, &format!("{INFINITE}.{feature}")), "{feature}");
                assert!(card(&plugins, from).on, "{from}");
                taken(&plugins, feature);
                // and from then on kept as its own, its old keys no more
                let kept = plugins.kept_lines();
                assert!(kept.contains(&format!("{INFINITE}.on true")), "{kept:?}");
                assert!(kept.contains(&format!("{INFINITE}.{feature} true")), "{kept:?}");
                let old = |line: &String| {
                    line.starts_with(&format!("{from}.{feature} "))
                        || values.iter().any(|(key, ..)| line.starts_with(&format!("{from}.{key} ")))
                };
                assert!(!kept.iter().any(old), "{kept:?}");
                // through the next start, as kept now
                let next = app_at(&dir);
                read_lines(&next, &kept);
                assert_eq!(next.plugins(), plugins.plugins(), "{feature}");
            }
            // the plugin off, or the feature: Infinite Plugins stays off, the feature's switch as
            // it was, its values taken all the same
            for (on, feature_on) in [(false, true), (true, false), (false, false)] {
                let plugins = app_at(&dir);
                read_lines(&plugins, &lines_of(on, feature_on));
                assert!(!card(&plugins, INFINITE).on, "{feature}: {on} {feature_on}");
                assert_eq!(switch_of(&plugins, &format!("{INFINITE}.{feature}")), feature_on, "{feature}");
                taken(&plugins, feature);
            }
            // its own on or off read back too, before or after, says instead
            for first in [false, true] {
                let plugins = app_at(&dir);
                let mut lines = lines_of(true, true);
                lines.insert(if first { 0 } else { lines.len() }, format!("{INFINITE}.on false"));
                read_lines(&plugins, &lines);
                assert!(!card(&plugins, INFINITE).on, "{feature}: its own first {first}");
                assert!(switch_of(&plugins, &format!("{INFINITE}.{feature}")));
            }
        }
        // all three at once, with Lasso fill's plugin off: one Infinite Plugins, those three on
        let plugins = app_at(&dir);
        let mut lines = vec!["io.github.artcraftandroid.lasso-fill.on false".to_owned()];
        for (from, feature, _) in TAKEN {
            lines.extend([format!("{from}.on true"), format!("{from}.{feature} true")]);
        }
        read_lines(&plugins, &lines);
        assert!(card(&plugins, INFINITE).on);
        let on: Vec<String> = card(&plugins, INFINITE)
            .features
            .iter()
            .filter(|f| f.control.on)
            .map(|f| f.control.label.clone())
            .collect();
        assert_eq!(on, ["Trace view", "Filter layers", "Colour extras"]);
        // and each does what it did: Trace dims, the colour panel steps, filter layers are offered
        assert!(plugins.set_slider(format!("{INFINITE}.view"), 2.0).is_some());
        assert!((plugins.hooks().view_dim_others() - 0.7).abs() < 1e-6);
        assert!(plugins.set_switch(format!("{INFINITE}.steps"), true));
        assert!(plugins.colour_steps() && !plugins.colour_before());
        assert_eq!(plugins.layer_filters().len(), 38);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_artwork_s_filter_layer_kept_by_its_layer_tools_id_shows_as_it_did() {
        const OLD_INVERT: &str = "io.github.artcraftandroid.layer-toolbox.invert";
        let dir = store("infinite-old-filter-layer");
        let plugins = featured_app(&dir, INFINITE, "filter_layers");
        let Some(s) = crate::tests::session(32, 32) else { return };
        s.set_color(1.0, 0.0, 0.0);
        let base = s.layers().last().unwrap().id;
        assert!(s.fill_layer(base));
        let shown = |s: &crate::CanvasSession| crate::tests::pixel(s, 16, 16);
        // off: no filter offered by its old id either
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(s.add_filter_layer(OLD_INVERT.to_owned()), 0);
        // on: a layer as an earlier app kept it, by Layer tools' id, shows inverted
        assert!(plugins.set_on(INFINITE.to_owned(), true));
        s.use_plugins(Arc::clone(&plugins));
        let layer = s.add_filter_layer(OLD_INVERT.to_owned());
        assert_ne!(layer, 0);
        assert_eq!(shown(&s), [0, 255, 255, 255]);
        let row = s.layers().into_iter().find(|l| l.id == layer).unwrap();
        assert_eq!(row.filter.as_deref(), Some("Invert"));
        // its settings (Infinite Plugins' own) open and set it as ever, the layer keeping its id
        assert!(s.filter_layer_settings());
        let landed = s.plugin_filter_slider(format!("{INFINITE}.invert.value"), 0.0, false).unwrap();
        assert!(landed.abs() < 1e-5, "{landed}");
        assert!(s.adjustment_end(true));
        let grey = shown(&s);
        assert!(grey[..3].iter().all(|c| c.abs_diff(128) <= 1), "half inverted: {grey:?}");
        let kept = s.shared.lock().doc.layer(procraft_engine::document::LayerId(layer)).unwrap().filter.clone();
        assert_eq!(kept.map(|f| f.filter).as_deref(), Some(OLD_INVERT));
        // the feature off: it shows nothing, as any filter layer whose filter is off
        assert!(plugins.set_switch(format!("{INFINITE}.filter_layers"), false));
        s.use_plugins(Arc::clone(&plugins));
        assert_eq!(shown(&s), [255, 0, 0, 255]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_artwork_s_paper_kept_by_the_paper_plugin_s_keys_comes_back_and_its_own_say_instead() {
        const OLD: &str = "io.github.artcraftandroid.paper";
        let dir = store("infinite-old-paper-artwork");
        let plugins = featured_app(&dir, INFINITE, "paper");
        assert!(plugins.set_on(INFINITE.to_owned(), true));
        let kind = |plugins: &Plugins| slider_value(plugins, &format!("{INFINITE}.kind"));
        // an artwork the Paper plugin kept Rough on, in Cream, its grain at 40%
        let one = crate::CanvasSession::new(64, 48);
        for (key, value) in [("paper", 3.0), ("colour", 2.0), ("grain", 0.4)] {
            one.keep_plugin_value(format!("{OLD}.{key}"), value);
        }
        one.use_plugins(Arc::clone(&plugins));
        assert_eq!(kind(&plugins), 3.0);
        assert_eq!(slider_value(&plugins, &format!("{INFINITE}.colour")), 2.0);
        assert!((slider_value(&plugins, &format!("{INFINITE}.grain")) - 0.4).abs() < 1e-6);
        let given = one.shared.lock().plugin_paper.clone().unwrap();
        assert!((given.strength - 0.4).abs() < 1e-6, "{}", given.strength);
        let cream = [0xF4, 0xEC, 0xD8].map(|c| c as f32 / 255.0);
        assert_eq!(given.colour, Some(cream));
        // what isn't kept starts as it starts
        assert_eq!(slider_value(&plugins, &format!("{INFINITE}.size")), 1.0);
        // set again now, kept under its own key, which says instead from then on
        assert!(plugins.set_slider(format!("{INFINITE}.kind"), 1.0).is_some());
        let values = one.plugin_values();
        assert_eq!((values[&format!("{INFINITE}.kind")], values[&format!("{OLD}.paper")]), (1.0, 3.0));
        let two = crate::CanvasSession::new(64, 48);
        for (key, value) in values {
            two.keep_plugin_value(key, value);
        }
        two.use_plugins(Arc::clone(&plugins));
        assert_eq!(kind(&plugins), 1.0);
        // an artwork with none kept: none
        let three = crate::CanvasSession::new(64, 48);
        three.use_plugins(Arc::clone(&plugins));
        assert_eq!((kind(&plugins), three.shared.lock().plugin_paper.clone()), (0.0, None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_paper_kept_on_the_disk_goes_whole_its_pictures_with_infinite_plugins() {
        const OLD: &str = "io.github.artcraftandroid.paper";
        let dir = store("infinite-old-paper-disk");
        let store_path = std::path::Path::new(&dir);
        // as an earlier app left it: Paper on, Your own paper chosen, a picture picked for its third
        let earlier = at(&dir);
        let old = old_file(
            "paper",
            json!([
                { "key": "paper", "type": "slider", "label": "Paper", "least": 0, "most": 7, "step": 1, "starts": 0 },
                { "key": "picture", "type": "file", "label": "Your own paper", "accepts": "picture" },
                { "key": "picture3", "type": "file", "label": "Your own paper 3", "accepts": "picture" },
            ]),
        );
        assert_eq!(earlier.install(old), None);
        assert!(earlier.set_on(OLD.to_owned(), true));
        assert_eq!(earlier.set_slider(format!("{OLD}.paper"), 6.0), Some(6.0));
        let png = crate::project::png_of(4, 4, &[200; 64]).unwrap();
        assert!(earlier.set_picture_file(format!("{OLD}.picture3"), "weave.png".to_owned(), png.clone()));
        let lines = earlier.kept_lines();
        // the app comes with Infinite Plugins now: the old one goes from the disk, its picture first
        let now = app_with(&dir, &all_bundled());
        read_lines(&now, &lines);
        assert!(!has(&now, OLD) && !dir_holds(&dir, OLD));
        assert!(card(&now, INFINITE).on && switch_of(&now, &format!("{INFINITE}.paper")));
        assert_eq!(slider_value(&now, &format!("{INFINITE}.kind")), 6.0);
        let key = format!("{INFINITE}.picture3");
        let (file, name) = plugin_file::chosen_paths(store_path, INFINITE, &key);
        assert_eq!(std::fs::read(&file).ok(), Some(png));
        assert_eq!(std::fs::read_to_string(&name).ok().as_deref(), Some("weave.png"));
        let chosen = |plugins: &Plugins, key: &str| match plugins.control(key) {
            Some(PluginControl::File { control }) => control.chosen,
            other => panic!("{other:?}"),
        };
        assert_eq!(chosen(&now, &key).as_deref(), Some("weave.png"));
        assert_eq!(chosen(&now, &format!("{INFINITE}.picture")), None);
        // and so on the next start
        let next = app_at(&dir);
        read_lines(&next, &now.kept_lines());
        assert_eq!(chosen(&next, &key).as_deref(), Some("weave.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lasso_fill_s_brush_rows_are_kept_under_the_key_its_brushes_kept_them_by() {
        let dir = store("infinite-rows");
        let plugins = featured_app(&dir, INFINITE, "lasso_fill");
        assert!(plugins.set_on(INFINITE.to_owned(), true));
        let keys: Vec<String> = card(&plugins, INFINITE).brush_controls.into_iter().map(|r| r.key).collect();
        assert_eq!(
            keys,
            ["plugin:io.github.artcraftandroid.lasso-fill:fill", "plugin:io.github.artcraftandroid.lasso-fill:shown"]
        );
        // with its feature off, none
        assert!(plugins.set_switch(format!("{INFINITE}.lasso_fill"), false));
        assert!(card(&plugins, INFINITE).brush_controls.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
