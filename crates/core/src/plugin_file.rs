//! A plugin file: a zip with its own extension, holding `craft-extension.json` and exactly the
//! files it lists. All of it is hostile until read, so nothing is written until every entry has
//! been checked, and what is kept is read again in full on every start: a file that no longer
//! holds up counts as absent. A plugin says what it is; what it may do comes from how it got here,
//! which it has no way to claim.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use procraft_engine::zip::ZipReader;
use serde::Deserialize;
use serde_json::Value;

use crate::plugin_run::{self, Hook, MODULE, MOST_MODULE};
use crate::plugins::{
    self, PluginBrushControl, PluginControl, PluginFile, PluginFileKind, PluginFolder, PluginPart, PluginSlider,
    PluginSwitch, PluginTexture,
};

/// The manifest's name in a plugin file, and in its folder.
const MANIFEST: &str = "craft-extension.json";
/// The manifest format read here: a newer one may mean anything, so it is refused.
const SCHEMA: u64 = 1;
/// What a plugin's code is written to, see [`crate::plugin_run`]: a newer one is refused too.
const INTERFACE: u32 = 22;
/// The first interface with features (several plugins' worth in one, each a switch of its own).
const FEATURES_FROM: u32 = 16;
/// The interface filter layers came in.
const FILTER_LAYERS_FROM: u32 = 17;
/// The interface filter layers' editors (curves, a gradient) came in.
const FILTER_EDITORS_FROM: u32 = 18;
/// The interface features taken from a plugin that stays, and brushes under a switch, came in.
const TAKES_FROM: u32 = 19;
/// The interface `folder` controls came in, with the hooks that name one.
const FOLDERS_FROM: u32 = 22;
/// A plugin of features: so many features, controls under each (besides its own switch) and
/// actions under each; and in all so many controls (files among them), hooks and actions.
const MOST_FEATURES: usize = 16;
const MOST_FEATURE_CONTROLS: usize = 16;
const MOST_FEATURED_CONTROLS: usize = 64;
const MOST_FEATURED_HOOKS: usize = 32;
const MOST_FEATURED_ACTIONS: usize = 32;
/// The most values a picker's picture is given (its controls' all).
const MOST_PICTURE_VALUES: usize = 16;
/// What ids of our own start with: no file may claim one.
const OWN_PREFIX: &str = "procraft.";
/// The file, its entries, one entry, all of them unpacked, and the manifest, in bytes.
pub(crate) const MOST_FILE: usize = 32 << 20;
const MOST_ENTRIES: usize = 64;
const MOST_ENTRY: usize = 16 << 20;
const MOST_UNPACKED: usize = 64 << 20;
const MOST_MANIFEST: usize = 64 << 10;
const MOST_SHADER: usize = 64 << 10;
/// A filter's code in its folder.
pub(crate) const FILTER_CODE: &str = "filter.wgsl";
/// A picker's picture's code in its folder (code on the `picker` hook brings it).
pub(crate) const PICTURE_CODE: &str = "picture.wgsl";
/// A plugin's set of brushes in its folder (interface 14): a set file of the app's own, of at most
/// so many brushes.
pub(crate) const BRUSH_SET: &str = "brushes.brushset";
const MOST_SET_BRUSHES: usize = 64;
/// Where each of the filters a plugin lists keeps its code, as `<key>.wgsl`.
const FILTERS_FOLDER: &str = "filters";
/// The most filters a plugin lists, and plugins it takes the place of.
const MOST_FILTERS: usize = 8;
/// How many filter layers a plugin can offer (interface 17), how many values each keeps, and the
/// longest heading one is listed under.
const MOST_LAYER_FILTERS: usize = 64;
const MOST_LAYER_FILTER_VALUES: usize = procraft_gpu::MOST_LAYER_FILTER_VALUES;
const MOST_GROUP: usize = 24;
const MOST_REPLACES: usize = 16;
/// The most features a plugin takes from others that stay.
const MOST_TAKES: usize = 8;
/// How far an entry may shrink in the archive, from this size on: text never gets near it.
const MOST_RATIO: usize = 100;
const RATIO_FROM: usize = 64 << 10;
/// How deep a listed path goes, how long it is, and how many controls and hooks a plugin has.
const MOST_DEPTH: usize = 8;
const MOST_PATH: usize = 200;
const MOST_CONTROLS: usize = 8;
/// Controls, for code written to interface 6 or later that holds no filter (a filter has room for
/// eight values).
const MOST_CONTROLS_6: usize = 16;
/// File controls, besides: each a file someone picks (so far a sound), kept with the plugin.
const MOST_FILES: usize = 8;
/// The folder in a plugin's home where the files picked for its file controls are kept.
const CHOSEN: &str = "chosen";
/// What a plugin's code keeps of its own (`procraft_keep`), a file in its home.
const KEPT: &str = "kept";
/// The folder in a plugin's home where the looks its brush engines gave are kept, each by its
/// engine and the brush's data. It goes whenever a version of the plugin is kept (a new one may draw
/// them otherwise), and with the plugin.
const LOOKS: &str = "looks";
/// The most bytes of one look kept, and of all a plugin's: past it, those read longest ago go.
pub(crate) const MOST_LOOK: usize = 64 << 20;
const MOST_LOOKS: u64 = 256 << 20;
const MOST_HOOKS: usize = 8;
/// The reason a stopped plugin gives, in bytes.
const MOST_STOP_NOTE: usize = 200;
/// What the note of a stop ends with, after the plugin's id.
const STOPPED_NOTE: &str = ".stopped";
/// The list of what the app came with and added, in bytes.
const MOST_ADDED: usize = 64 << 10;
/// A slider's stops: past this a range and step were not meant.
const MOST_STOPS: f64 = 100_000.0;

/// Why a file wasn't taken. Each says so in plain words, once; the detail goes to the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    NotAPlugin,
    TooBig,
    Unsafe,
    Mismatch,
    Damaged,
    Described,
    Permissions,
    Code,
    FilterCode,
    Newer,
    OtherApp,
    NewerInstalled,
    Clash,
    Replaced,
    NotKept,
    Brushes,
}

impl Refusal {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Refusal::NotAPlugin => "This isn't a ProCraft plugin.",
            Refusal::TooBig => "This plugin is too big to import.",
            Refusal::Unsafe => "This plugin file isn't safe to open.",
            Refusal::Mismatch => "This plugin's files don't match its list.",
            Refusal::Damaged => "This plugin file is damaged.",
            Refusal::Described => "This plugin describes itself wrongly.",
            Refusal::Permissions => "This plugin asks for permissions ProCraft doesn't give.",
            Refusal::Code => "This plugin's code can't run in ProCraft.",
            Refusal::FilterCode => "This filter's code can't run in ProCraft.",
            Refusal::Newer => "This plugin needs a newer ProCraft.",
            Refusal::OtherApp => "This plugin isn't for ProCraft.",
            Refusal::NewerInstalled => "A newer version of this plugin is already imported.",
            Refusal::Clash => "Another plugin already holds a setting this one needs.",
            Refusal::Replaced => "A plugin already here takes this one's place.",
            Refusal::NotKept => "This plugin couldn't be saved.",
            Refusal::Brushes => "This plugin's brushes can't be read.",
        }
    }
}

/// A file a manifest lists.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Listed {
    path: String,
    sha256: String,
    size: usize,
}

/// A manifest that has held up, its controls' keys carrying the plugin's id.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Manifest {
    pub id: String,
    pub name: String,
    pub holds: Vec<PluginPart>,
    pub version: String,
    pub publisher: String,
    pub description: String,
    pub experimental: bool,
    /// The interface it is written to (see [`crate::plugin_run`]).
    pub interface: u32,
    /// Where its code steps in, if it holds any.
    pub hooks: Vec<Hook>,
    pub controls: Vec<PluginControl>,
    /// Rows it adds to the brush studio, kept with each brush.
    pub brush_controls: Vec<PluginBrushControl>,
    /// What it does when tapped, and what it may reach then.
    pub actions: Vec<crate::plugins::PluginAction>,
    /// The kinds of artwork it makes.
    pub kinds: Vec<crate::plugin_canvas::PluginKind>,
    /// The kinds of layer it adds (interface 3).
    pub layer_kinds: Vec<crate::plugin_canvas::PluginKind>,
    /// The brush engines it has (interface 4).
    pub brush_engines: Vec<crate::plugin_brushes::BrushEngine>,
    pub permissions: Vec<crate::plugin_commands::PluginPermission>,
    /// Which switch each control and action comes under, its filters and what it takes the place of.
    pub shape: Shape,
    files: Vec<Listed>,
}

/// What a manifest says beyond what its card lists: the switch each control and action comes
/// under, if any (each by its place: shown, listed and run only while that switch is on), the
/// filters it lists, the plugins it takes the place of, and its menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Shape {
    pub under: Vec<Option<usize>>,
    pub action_under: Vec<Option<usize>>,
    /// Where else each control shows, beside the card (interface 8).
    pub places: Vec<plugins::PluginPlace>,
    /// The press that flips each switch, and runs each action, while the app's shortcuts leave it
    /// free (interface 8).
    pub presses: Vec<Option<crate::KeyCombo>>,
    pub action_presses: Vec<Option<crate::KeyCombo>>,
    /// Which controls are kept with each artwork rather than with the plugin (interface 8).
    pub artwork: Vec<bool>,
    /// None for a plugin with one filter, in `filter.wgsl`.
    pub filters: Vec<ListedFilter>,
    pub replaces: Vec<Replaced>,
    /// The features it takes from plugins that stay (interface 19).
    pub takes: Vec<Taken>,
    pub menu: Option<Menu>,
    /// The switch each of its rows in the brush studio comes under: shown only while it is on.
    pub brush_under: Vec<Option<usize>>,
    /// Its features (interface 16), in their order: none for a plugin written without.
    pub features: Vec<Feature>,
    /// The switch its set of brushes comes under (interface 19): given to the library only while
    /// that is on.
    pub brushes_under: Option<usize>,
    /// The plugin it takes the place of whose set of brushes its own is (interface 19): one given
    /// before as that one's isn't given again.
    pub brushes_from: Option<String>,
}

/// One of the features of a plugin made of several (interface 16): a switch at the top of its list,
/// heading everything that comes under it, with words of its own under its name. While it is on,
/// it runs in a copy of the plugin's code of its own, on the hooks it lists, with the memory they
/// give; off, it is on none.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Feature {
    /// Its switch's place among the controls.
    pub at: usize,
    pub description: String,
    pub hooks: Vec<Hook>,
}

/// A menu a plugin with code has somewhere in the interface, from a button of the app's: where,
/// its choices, and the switch it comes under (its button shows only while that is on).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Menu {
    pub place: plugins::PluginMenuPlace,
    pub choices: Vec<MenuChoice>,
    pub under: Option<usize>,
}

/// One of a menu's choices: what picks it (`<id>.<key>`), its words, its icon (path data on the
/// icons' grid, as [`crate::plugin_icon::icon_path`] writes it), and the value it gives each control
/// it sets, by the control's place (a switch 1 or 0).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MenuChoice {
    pub key: String,
    pub label: String,
    pub icon: String,
    pub sets: Vec<(usize, f32)>,
}

/// A filter of several a plugin lists: what opens it (`<id>.<key>`), its name under Adjustments,
/// its code in the plugin's folder, its controls by their place (its panel's, and `v.<key>` each),
/// and the switch it comes under.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ListedFilter {
    pub id: String,
    pub label: String,
    pub code: String,
    pub controls: Vec<usize>,
    pub under: Option<usize>,
    /// Offered as a filter layer (interface 17), not under Adjustments: what it is made of.
    pub layer: Option<LayerShape>,
}

/// A filter layer's filter (interface 17): its values, each kept with each layer of it (sliders and
/// switches, keyed `<id>.<filter>.<key>`), how far from a pixel it reads what is below, and the
/// heading it is listed under in the Layers panel's +.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayerShape {
    pub values: Vec<PluginControl>,
    pub reach: procraft_gpu::FilterReach,
    pub group: String,
    /// What a touch on the canvas sets while its settings are open.
    pub touch: Option<crate::filter_layers::FilterTouch>,
    /// The app's editor its settings open in, if any (interface 18).
    pub editor: Option<crate::filter_layers::FilterEditor>,
}

impl LayerShape {
    /// The names its code reads its values by, in their order: its values', then its touch's.
    pub(crate) fn code_keys(&self) -> Vec<String> {
        let mut keys = value_names(&self.values);
        keys.extend(self.touch.iter().flat_map(|t| t.keys()).map(|k| (*k).to_owned()));
        keys
    }
}

/// A plugin another takes the place of, by its id: the switch its being on turns on, each of its
/// controls by the key its value was kept under, with the place of the control taking it (a file
/// control's file comes too), and whether the marks it kept with layers are this one's now
/// (interface 16).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Replaced {
    pub id: String,
    pub switch: Option<usize>,
    pub controls: Vec<(String, usize)>,
    pub marks: bool,
}

/// A feature it takes from a plugin that stays (interface 19), by that plugin's id: the key that
/// feature's switch was kept under (`<id>.<feature>`), the switch here that takes it (a feature),
/// each of its controls by the key its value was kept under with the place of the control taking
/// it, and whether its filter layers (those under that switch here) answer to the ids they had
/// there (`<id>.<key>`), so the layers an artwork kept of them go on showing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Taken {
    pub id: String,
    pub feature: String,
    pub switch: usize,
    pub controls: Vec<(String, usize)>,
    pub filters: bool,
}

/// How many switches may stand above a control: a switch at the top, one under it and one under
/// that, the top a feature (interface 16); without features, one fewer.
const MOST_ABOVE: usize = 3;

impl Shape {
    /// Whether whatever comes under `under` shows, in a plugin whose controls are `controls`: that
    /// switch is on, and any it comes under.
    pub(crate) fn shows(&self, under: Option<usize>, controls: &[PluginControl]) -> bool {
        let mut next = under;
        // (as read, at most so many stand above anything)
        for _ in 0..=MOST_ABOVE {
            let Some(at) = next else { return true };
            if !matches!(controls.get(at), Some(PluginControl::Switch { control }) if control.on) {
                return false;
            }
            next = self.under.get(at).copied().flatten();
        }
        false
    }

    /// The switch at the top of what stands above `under` (itself, if it comes under nothing).
    fn top_of(&self, under: Option<usize>) -> Option<usize> {
        let mut at = under?;
        for _ in 0..MOST_ABOVE {
            match self.under.get(at).copied().flatten() {
                Some(up) => at = up,
                None => break,
            }
        }
        Some(at)
    }

    /// The feature (its place in [`Shape::features`]) that whatever comes under `under` belongs to.
    pub(crate) fn feature_under(&self, under: Option<usize>) -> Option<usize> {
        let top = self.top_of(under)?;
        self.features.iter().position(|f| f.at == top)
    }

    /// The feature control `at` belongs to: the one it heads, or the one it comes under.
    pub(crate) fn feature_of(&self, at: usize) -> Option<usize> {
        self.feature_under(Some(at))
    }

    /// The switch of the feature control `at` belongs to.
    pub(crate) fn feature_switch_of(&self, at: usize) -> Option<usize> {
        self.feature_of(at).and_then(|f| self.features.get(f)).map(|f| f.at)
    }

    /// Whether the menu shows, in a plugin whose controls are `controls`.
    pub(crate) fn menu_shows(&self, controls: &[PluginControl]) -> bool {
        self.menu.as_ref().is_some_and(|menu| self.shows(menu.under, controls))
    }

    /// The id the marks it keeps with layers go by: that of a plugin it takes the place of whose
    /// marks it keeps, or `id`, its own.
    pub(crate) fn marks_id<'a>(&'a self, id: &'a str) -> &'a str {
        self.replaces.iter().find(|r| r.marks).map_or(id, |r| r.id.as_str())
    }

    /// The id its set of brushes is given by: that of the plugin it takes the place of whose set it
    /// is, or `id`, its own.
    pub(crate) fn brushes_id<'a>(&'a self, id: &'a str) -> &'a str {
        self.brushes_from.as_deref().unwrap_or(id)
    }

    /// The keys the value of control `at` was kept under before it was this one's: by a plugin it
    /// takes the place of, or one it takes a feature from (interface 19).
    pub(crate) fn kept_before(&self, at: usize) -> impl Iterator<Item = &str> {
        let replaced = self.replaces.iter().flat_map(|r| &r.controls);
        let taken = self.takes.iter().flat_map(|t| &t.controls);
        replaced.chain(taken).filter(move |(_, to)| *to == at).map(|(key, _)| key.as_str())
    }

    /// The ids filter layer `filter` (its id here) had before it was this one's: under a feature
    /// taken whose filter layers answer to their ids there (interface 19).
    pub(crate) fn filter_was(&self, filter: &ListedFilter, plugin: &str) -> Vec<String> {
        let Some(key) = filter.id.strip_prefix(plugin).and_then(|rest| rest.strip_prefix('.')) else {
            return Vec::new();
        };
        let feature = self.feature_under(filter.under);
        let theirs =
            self.takes.iter().filter(|t| t.filters && feature.is_some() && self.feature_of(t.switch) == feature);
        theirs.map(|t| format!("{}.{key}", t.id)).collect()
    }
}

/// A plugin file read and checked, ready to keep.
pub(crate) struct Package {
    pub manifest: Manifest,
    text: Vec<u8>,
    files: Vec<(String, Vec<u8>)>,
}

#[cfg(test)]
impl Package {
    /// The file it lists at `path`.
    pub(crate) fn file(&self, path: &str) -> Option<&[u8]> {
        self.files.iter().find(|(name, _)| name == path).map(|(_, data)| data.as_slice())
    }

    pub(crate) fn manifest_text(&self) -> &[u8] {
        &self.text
    }
}

/// A plugin the app came with and added to the store once, as if imported: `off`, it starts off
/// until someone turns it on, as it was added. It stays written down after it is removed, so it
/// isn't added again.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Added {
    pub id: String,
    pub off: bool,
}

#[derive(Deserialize)]
struct Head {
    schema: u64,
}

#[derive(Deserialize)]
struct RawManifest {
    id: String,
    name: String,
    version: String,
    holds: Vec<String>,
    #[serde(default)]
    publisher: Option<RawPublisher>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    experimental: Option<bool>,
    #[serde(default)]
    compat: RawCompat,
    #[serde(default)]
    files: Vec<RawFile>,
    #[serde(default)]
    permissions: Vec<Value>,
    #[serde(default)]
    hooks: Vec<String>,
    #[serde(default)]
    controls: Vec<RawControl>,
    #[serde(default)]
    brush_controls: Vec<RawBrushControl>,
    #[serde(default)]
    actions: Vec<RawAction>,
    #[serde(default)]
    kinds: Vec<RawKind>,
    #[serde(default)]
    layer_kinds: Vec<RawKind>,
    #[serde(default)]
    brush_engines: Vec<RawEngine>,
    #[serde(default)]
    filters: Vec<RawFilter>,
    #[serde(default)]
    replaces: Vec<RawReplaced>,
    #[serde(default)]
    takes: Vec<RawTaken>,
    #[serde(default)]
    brushes: Option<RawBrushes>,
    #[serde(default)]
    menu: Option<RawMenu>,
}

/// A menu of a plugin's own, from a button the app puts at `place`: its choices, each an icon and
/// words, setting some of the plugin's controls; and the key of the switch it comes under.
#[derive(Deserialize)]
struct RawMenu {
    place: String,
    choices: Vec<RawChoice>,
    #[serde(default)]
    under: Option<String>,
}

#[derive(Deserialize)]
struct RawChoice {
    key: String,
    label: String,
    icon: String,
    /// Each control it sets, by its key: a switch's `true` or `false`, a slider's number.
    sets: std::collections::BTreeMap<String, Value>,
}

/// The most choices a menu has, and controls one choice sets.
const MOST_MENU_CHOICES: usize = 8;
const MOST_SETS: usize = 8;

/// The menu `raw` describes for plugin `id`, if it holds up: a place this app has (another is for a
/// newer one), a few choices, each with a key of its own, plain words and an icon that holds up,
/// setting one to a few of `controls` (in their order, `keys` as the manifest names them) that are
/// switches (to `true` or `false`) or sliders (to a number in their range). Only code has one.
fn menu_of(
    id: &str,
    raw: Option<RawMenu>,
    keys: &[String],
    controls: &[PluginControl],
    code: bool,
    under: Option<usize>,
) -> Result<Option<Menu>, Refusal> {
    let Some(raw) = raw else { return Ok(None) };
    if !code || raw.choices.is_empty() || raw.choices.len() > MOST_MENU_CHOICES {
        return Err(Refusal::Described);
    }
    let place = plugins::PluginMenuPlace::named(&raw.place).ok_or(Refusal::Newer)?;
    let mut seen = HashSet::new();
    let mut choices = Vec::new();
    for choice in raw.choices {
        let plain = !choice.label.trim().is_empty() && shows_plainly(&choice.label, 24);
        let icon = crate::plugin_icon::icon_path(&choice.icon);
        let sets_fit = (1..=MOST_SETS).contains(&choice.sets.len());
        if !valid_key(&choice.key) || !seen.insert(choice.key.clone()) || !plain || !sets_fit {
            return Err(Refusal::Described);
        }
        let mut sets = Vec::new();
        for (key, value) in &choice.sets {
            let at = keys.iter().position(|k| k == key).ok_or(Refusal::Described)?;
            let value = match (controls.get(at), value) {
                (Some(PluginControl::Switch { .. }), Value::Bool(on)) => f32::from(u8::from(*on)),
                (Some(PluginControl::Slider { control }), Value::Number(n)) => {
                    let v = n.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or(Refusal::Described)?;
                    if !(control.least..=control.most).contains(&v) {
                        return Err(Refusal::Described);
                    }
                    control.stepped(v)
                }
                _ => return Err(Refusal::Described),
            };
            sets.push((at, value));
        }
        choices.push(MenuChoice {
            key: format!("{id}.{}", choice.key),
            label: choice.label,
            icon: icon.ok_or(Refusal::Described)?,
            sets,
        });
    }
    Ok(Some(Menu { place, choices, under }))
}

/// A filter of several in one plugin (interface 5): its key, its name under Adjustments, the keys
/// of its controls, and the switch it comes under. Its code is `filters/<key>.wgsl`.
#[derive(Deserialize)]
struct RawFilter {
    key: String,
    label: String,
    #[serde(default)]
    controls: Vec<String>,
    #[serde(default)]
    under: Option<String>,
    /// A filter layer's (interface 17): it is one, its values, how far it reads, its heading.
    #[serde(default)]
    layer: bool,
    #[serde(default)]
    values: Vec<RawControl>,
    #[serde(default)]
    reach: Option<Value>,
    #[serde(default)]
    group: Option<String>,
    /// What a touch on the canvas sets: `point`, `angle` or `seed`.
    #[serde(default)]
    touch: Option<String>,
    /// The app's editor its settings open in: `curves` or `gradient` (interface 18).
    #[serde(default)]
    editor: Option<String>,
}

/// How far a filter layer's filter reads (interface 17): canvas pixels, `"whole"` for anywhere, or
/// `{ "value": <a slider's key>, "times": .., "plus": .. }` by one of its values.
fn reach_of(raw: Option<&Value>, values: &[PluginControl]) -> Result<procraft_gpu::FilterReach, Refusal> {
    use procraft_gpu::FilterReach;
    let number = |v: Option<&Value>| v.and_then(Value::as_f64).filter(|n| n.is_finite() && n.abs() <= 1e6);
    match raw {
        None => Ok(FilterReach::Within(0.0)),
        Some(Value::String(s)) if s == "whole" => Ok(FilterReach::Whole),
        Some(n @ Value::Number(_)) => match number(Some(n)) {
            Some(n) if n >= 0.0 => Ok(FilterReach::Within(n as f32)),
            _ => Err(Refusal::Described),
        },
        Some(Value::Object(o)) => {
            let fields = o.keys().all(|k| matches!(k.as_str(), "value" | "times" | "plus"));
            let key = o.get("value").and_then(Value::as_str).ok_or(Refusal::Described)?;
            let slider = |c: &PluginControl| matches!(c, PluginControl::Slider { control } if control.key.rsplit('.').next() == Some(key));
            let at = values.iter().position(slider);
            let times = if o.contains_key("times") { number(o.get("times")) } else { Some(1.0) };
            let plus = if o.contains_key("plus") { number(o.get("plus")) } else { Some(0.0) };
            match (fields, at, times, plus) {
                (true, Some(value), Some(times), Some(plus)) => {
                    Ok(FilterReach::ByValue { value, times: times as f32, plus: plus as f32 })
                }
                _ => Err(Refusal::Described),
            }
        }
        Some(_) => Err(Refusal::Described),
    }
}

/// Filter `f` of plugin `id` as a filter layer offers it (interface 17), if it holds up: a few
/// values of its own, each a slider or a switch shown nowhere else, and a heading to show.
fn layer_shape_of(id: &str, f: &RawFilter, interface: Option<u32>) -> Result<Option<LayerShape>, Refusal> {
    if !f.layer {
        let plain =
            f.values.is_empty() && f.reach.is_none() && f.group.is_none() && f.touch.is_none() && f.editor.is_none();
        return if plain { Ok(None) } else { Err(Refusal::Described) };
    }
    let group = f.group.as_deref().unwrap_or_default();
    let editor = match f.editor.as_deref() {
        None => None,
        Some(name) => Some(crate::filter_layers::FilterEditor::named(name).ok_or(Refusal::Described)?),
    };
    // with an editor, its values may be none; editors came with interface 18
    let least = usize::from(editor.is_none());
    let editors_fit = editor.is_none() || interface.is_some_and(|i| i >= FILTER_EDITORS_FROM);
    let fits = interface.is_some_and(|i| i >= FILTER_LAYERS_FROM)
        && editors_fit
        && f.controls.is_empty()
        && (least..=MOST_LAYER_FILTER_VALUES).contains(&f.values.len())
        && !group.trim().is_empty()
        && shows_plainly(group, MOST_GROUP);
    if !fits {
        return Err(Refusal::Described);
    }
    let mut keys = HashSet::new();
    let mut values = Vec::new();
    for raw in &f.values {
        let elsewhere = raw.keeps.is_some()
            || raw.under.is_some()
            || raw.place.is_some()
            || raw.press.is_some()
            || raw.artwork.is_some();
        if elsewhere || !matches!(raw.kind.as_str(), "slider" | "switch") || !keys.insert(raw.key.as_str()) {
            return Err(Refusal::Described);
        }
        values.push(checked_control(&format!("{id}.{}", f.key), raw.clone())?);
    }
    let reach = reach_of(f.reach.as_ref(), &values)?;
    let touch = match f.touch.as_deref() {
        None => None,
        Some(name) => Some(crate::filter_layers::FilterTouch::named(name).ok_or(Refusal::Described)?),
    };
    let shape = LayerShape { values, reach, group: group.to_owned(), touch, editor };
    // its touch's values are its code's too, and none of its own is called as they are
    let keys = shape.code_keys();
    if keys.len() > MOST_LAYER_FILTER_VALUES || keys.iter().collect::<HashSet<_>>().len() != keys.len() {
        return Err(Refusal::Described);
    }
    Ok(Some(shape))
}

/// A plugin this one takes the place of: its id, the key of the switch its being on turns on, its
/// controls' keys, each with the key of the control here that takes its value, and whether the
/// marks it kept with layers are this one's now (interface 16).
#[derive(Deserialize)]
struct RawReplaced {
    id: String,
    #[serde(default)]
    switch: Option<String>,
    #[serde(default)]
    controls: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    marks: bool,
}

/// A feature this one takes from a plugin that stays (interface 19): that plugin's id, the key of
/// the feature's switch there, the key of the feature here that takes it, the feature's controls'
/// keys there, each with the key of the control here that takes its value, and whether its filter
/// layers answer to the ids they had there.
#[derive(Deserialize)]
struct RawTaken {
    id: String,
    feature: String,
    switch: String,
    #[serde(default)]
    controls: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    filters: bool,
}

/// What a plugin's set of brushes comes under (interface 19): the key of the switch it is given
/// while on, and the id of the plugin it takes the place of whose set it is.
#[derive(Deserialize)]
struct RawBrushes {
    #[serde(default)]
    under: Option<String>,
    #[serde(default)]
    from: Option<String>,
}

/// A brush engine a plugin has: its key, what it is called, and the endings of the names of the
/// brush files it reads.
#[derive(Deserialize)]
struct RawEngine {
    key: String,
    label: String,
    accepts: Vec<String>,
    /// The key of the action that is its brushes' studio, if it has one.
    #[serde(default)]
    studio: Option<String>,
}

/// The most brush engines a plugin has, and file endings each reads.
const MOST_ENGINES: usize = 4;
const MOST_ACCEPTS: usize = 8;

/// A kind of artwork a plugin makes: its key and its name in the gallery's menu (or a kind of
/// layer, and what its thumbnails show it by).
#[derive(Deserialize)]
struct RawKind {
    key: String,
    label: String,
    #[serde(default)]
    badge: Option<RawBadge>,
    #[serde(default)]
    exports: Vec<RawExport>,
}

/// A file a kind of layer's plugin makes of its layers: its key, what Share calls it, its
/// extension and media type.
#[derive(Deserialize)]
struct RawExport {
    key: String,
    label: String,
    extension: String,
    mime: String,
}

/// The most files a kind of layer can be exported as.
const MOST_EXPORTS: usize = 4;

/// Whether `raw` holds up: a key, words to show, a short extension of lower-case letters and digits,
/// and a media type as they are written.
fn export_fits(raw: &RawExport) -> bool {
    let ext = &raw.extension;
    let mime = &raw.mime;
    valid_key(&raw.key)
        && !raw.label.trim().is_empty()
        && shows_plainly(&raw.label, 40)
        && (1..=8).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && mime.len() <= 64
        && mime.split_once('/').is_some_and(|(a, b)| !a.is_empty() && !b.is_empty())
        && mime.bytes().all(|b| b.is_ascii_alphanumeric() || b"/+.-".contains(&b))
}

/// A kind of layer's badge, as its manifest draws it (see [`crate::PluginBadge`]).
#[derive(Deserialize)]
struct RawBadge {
    #[serde(default)]
    lines: Vec<RawBadgeLine>,
    #[serde(default)]
    dots: Vec<f32>,
}

#[derive(Deserialize)]
struct RawBadgeLine {
    points: Vec<f32>,
    #[serde(default)]
    curved: bool,
}

/// The most lines a badge has, numbers a line has, and dots a badge has.
const MOST_BADGE_LINES: usize = 8;
const MOST_LINE_NUMBERS: usize = 64;
const MOST_BADGE_DOTS: usize = 8;

/// The badge `raw` draws, if it holds up: a few lines of whole points (a curved one a start and
/// three for each curve), a few dots, every number in its box.
fn badge_of(raw: RawBadge) -> Option<crate::PluginBadge> {
    let in_box = |v: &[f32]| v.len().is_multiple_of(2) && v.iter().all(|n| n.is_finite() && (0.0..=1.0).contains(n));
    let lines_fit = raw.lines.len() <= MOST_BADGE_LINES
        && raw.lines.iter().all(|l| {
            let points = l.points.len() / 2;
            in_box(&l.points)
                && l.points.len() <= MOST_LINE_NUMBERS
                && points >= 2
                && (!l.curved || (points - 1).is_multiple_of(3))
        });
    let dots_fit = raw.dots.len() <= MOST_BADGE_DOTS * 2 && in_box(&raw.dots);
    (lines_fit && dots_fit && !(raw.lines.is_empty() && raw.dots.is_empty())).then(|| crate::PluginBadge {
        lines: raw.lines.into_iter().map(|l| crate::BadgeLine { points: l.points, curved: l.curved }).collect(),
        dots: raw.dots,
    })
}

/// The most kinds of artwork a plugin may make.
const MOST_KINDS: usize = 4;

/// Something a plugin does when tapped: its key, the words on its row, and where else it's listed.
#[derive(Deserialize)]
struct RawAction {
    key: String,
    label: String,
    #[serde(default)]
    place: Option<String>,
    /// The key of the switch it comes under.
    #[serde(default)]
    under: Option<String>,
    /// The press that runs it.
    #[serde(default)]
    press: Option<String>,
}

/// The most actions a plugin may list.
const MOST_ACTIONS: usize = 8;

/// A row a plugin adds to the brush studio: a choice of a few, kept with each brush; the key of
/// the switch it comes under; and the id of a plugin it takes the place of whose row it is, whose
/// key brushes keep its choice under (interface 16).
#[derive(Deserialize)]
struct RawBrushControl {
    key: String,
    label: String,
    choices: Vec<String>,
    #[serde(default)]
    under: Option<String>,
    #[serde(default)]
    from: Option<String>,
}

/// The most rows a plugin may add to the brush studio, and choices a row may have.
const MOST_BRUSH_CONTROLS: usize = 8;
const MOST_CHOICES: usize = 16;

/// `raw` as a row of plugin `id`'s in the brush studio, if it holds up: its key kept with each
/// brush is `plugin:<id>:<key>`, or, for a row of a plugin it takes the place of (`replaced`
/// names them), `plugin:<that id>:<key>`, so what brushes kept for that one still says.
fn checked_brush_control(
    id: &str,
    raw: RawBrushControl,
    replaced: &[RawReplaced],
    interface: Option<u32>,
) -> Result<PluginBrushControl, Refusal> {
    let plain = |text: &String, most| !text.trim().is_empty() && shows_plainly(text, most);
    let choices = (2..=MOST_CHOICES).contains(&raw.choices.len()) && raw.choices.iter().all(|c| plain(c, 24));
    if !valid_key(&raw.key) || !plain(&raw.label, 40) || !choices {
        return Err(Refusal::Described);
    }
    let kept_by = match raw.from.as_deref() {
        None => id,
        Some(from) if interface.is_some_and(|i| i >= FEATURES_FROM) && replaced.iter().any(|r| r.id == from) => from,
        Some(_) => return Err(Refusal::Described),
    };
    Ok(PluginBrushControl { key: format!("plugin:{kept_by}:{}", raw.key), label: raw.label, choices: raw.choices })
}

#[derive(Deserialize)]
struct RawPublisher {
    name: String,
}

#[derive(Default, Deserialize)]
struct RawCompat {
    procraft: Option<RawAppCompat>,
}

#[derive(Deserialize)]
struct RawAppCompat {
    #[serde(default)]
    min_app: Option<String>,
    #[serde(default)]
    interface: Option<u32>,
}

#[derive(Deserialize)]
struct RawFile {
    path: String,
    sha256: String,
    size: usize,
}

#[derive(Clone, Deserialize)]
struct RawControl {
    key: String,
    #[serde(rename = "type")]
    kind: String,
    label: String,
    #[serde(default)]
    least: Option<f64>,
    #[serde(default)]
    most: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    starts: Option<Value>,
    #[serde(default)]
    live: Option<bool>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    at_least: Option<String>,
    /// A slider's name for each step, from its least.
    #[serde(default)]
    names: Option<Vec<String>>,
    /// What a file control takes: `sound`.
    #[serde(default)]
    accepts: Option<String>,
    #[serde(default)]
    shown_times: Option<f64>,
    /// A setting the app used to have, whose kept value this control takes over.
    #[serde(default)]
    keeps: Option<String>,
    /// The key of the switch it comes under.
    #[serde(default)]
    under: Option<String>,
    /// Where else it shows, beside the card: `canvas`.
    #[serde(default)]
    place: Option<String>,
    /// The press that flips it (a switch).
    #[serde(default)]
    press: Option<String>,
    /// Kept with each artwork rather than with the plugin.
    #[serde(default)]
    artwork: Option<bool>,
    /// A feature's words under its name, and the hooks it answers (interface 16).
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    hooks: Option<Vec<String>>,
    /// The keys of the file controls of pictures a texture offers too.
    #[serde(default)]
    own: Option<Vec<String>>,
}

/// Whether `text`, at most `most` characters, is fit to show: nothing that hides, reorders or
/// imitates what is around it (controls, bidi marks, zero-width and other invisible characters).
fn shows_plainly(text: &str, most: usize) -> bool {
    text.chars().count() <= most && !text.chars().any(|c| c.is_control() || invisible(c))
}

/// Characters drawn as nothing, or that move the text around them.
fn invisible(c: char) -> bool {
    matches!(
        u32::from(c),
        0xAD | 0x34F | 0x61C | 0x115F..=0x1160 | 0x17B4..=0x17B5 | 0x180B..=0x180E
            | 0x200B..=0x200F | 0x2028..=0x202E | 0x2060..=0x206F | 0x3164
            | 0xE000..=0xF8FF | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0 | 0xFFF0..=0xFFFB
            | 0xE0000..=0xE0FFF | 0xF0000..=0x10FFFF
    )
}

/// A name Windows keeps for itself, with or without an extension.
fn windows_keeps(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    let numbered =
        |prefix: &str| stem.strip_prefix(prefix).is_some_and(|n| n.len() == 1 && n.bytes().all(|b| b.is_ascii_digit()));
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul") || numbered("com") || numbered("lpt")
}

/// An id: lowercase, dotted, each part plain, and at least two of them. A moved setting's key has
/// no dot, so no key of an imported plugin can be one, but for a control that says it keeps it. It
/// can't end as a stop's note does, or one plugin's folder would be where another's stop is noted.
fn valid_id(id: &str) -> bool {
    let plain = |part: &str| {
        !part.is_empty()
            && !part.starts_with('-')
            && !part.ends_with('-')
            && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    id.len() <= 100
        && id.split('.').count() >= 2
        && id.split('.').all(plain)
        && !windows_keeps(id)
        && !id.ends_with(STOPPED_NOTE)
}

/// A control's key: it goes in a settings line, after the plugin's id, so it is one plain word.
pub(crate) fn valid_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    key.len() <= 32
        && key != "on"
        && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `MAJOR.MINOR.PATCH` in numbers, also a folder's name.
fn valid_version(version: &str) -> bool {
    version_parts(version).is_some() && !version.contains('-') && version.len() <= 24
}

/// A version's numbers, what comes after a `-` left out.
fn version_parts(version: &str) -> Option<[u32; 3]> {
    let numbers = version.split('-').next()?;
    let mut parts = numbers
        .split('.')
        .map(|p| (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())).then(|| p.parse::<u32>().ok()).flatten());
    let version = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(version)
}

/// Whether `version` comes before `other` (neither being a version says nothing either way).
pub(crate) fn version_is_older(version: &str, other: &str) -> bool {
    version_parts(version).zip(version_parts(other)).is_some_and(|(a, b)| a < b)
}

/// A path inside a plugin: relative, plain at every step, never out of its folder.
fn safe_path(path: &str) -> bool {
    let plain = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && !part.ends_with('.')
            && !windows_keeps(part)
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    path.len() <= MOST_PATH && path.split('/').count() <= MOST_DEPTH && path.split('/').all(plain)
}

fn sha_digits(sha: &str) -> bool {
    sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether the texture controls among `controls` hold up: written to interface 15 or later, each
/// offering pictures of file controls of pictures among them, each once.
fn textures_hold_up(controls: &[PluginControl], interface: Option<u32>) -> bool {
    let picture = |key: &str| {
        controls.iter().any(
            |c| matches!(c, PluginControl::File { control } if control.key == key && control.accepts == PluginFileKind::Picture),
        )
    };
    let mut any = false;
    for c in controls {
        if let PluginControl::Texture { control } = c {
            any = true;
            let distinct: HashSet<&String> = control.own.iter().collect();
            if distinct.len() != control.own.len() || !control.own.iter().all(|k| picture(k)) {
                return false;
            }
        }
    }
    !any || interface.is_some_and(|interface| interface >= 15)
}

/// `control` as the list has it, its key under the plugin's id.
fn checked_control(id: &str, raw: RawControl) -> Result<PluginControl, Refusal> {
    if !valid_key(&raw.key) || raw.label.trim().is_empty() || !shows_plainly(&raw.label, 40) {
        return Err(Refusal::Described);
    }
    // what the app no longer has stays where it was kept, for the plugin that took it over
    let key = match &raw.keeps {
        Some(kept) if plugins::moved(kept) => kept.clone(),
        Some(_) => return Err(Refusal::Described),
        None => format!("{id}.{}", raw.key),
    };
    // only a feature has words under its name and hooks of its own
    let feature = raw.kind == "feature";
    if !feature && (raw.description.is_some() || raw.hooks.is_some()) {
        return Err(Refusal::Described);
    }
    match raw.kind.as_str() {
        // a feature (interface 16): a switch heading what comes under it, off until someone turns
        // it on; it keeps no setting the app used to have
        "feature" => {
            let words = raw.description.as_deref().unwrap_or_default();
            let starts_off = raw.starts.as_ref().is_none_or(|s| *s == Value::Bool(false));
            if raw.keeps.is_some() || !starts_off || !shows_plainly(words, 200) {
                return Err(Refusal::Described);
            }
            let control = PluginSwitch { key, label: raw.label, on: false, starts: false };
            Ok(PluginControl::Switch { control })
        }
        "switch" => {
            let starts = match raw.starts {
                None => false,
                Some(Value::Bool(on)) => on,
                Some(_) => return Err(Refusal::Described),
            };
            let control = PluginSwitch { key, label: raw.label, on: starts, starts };
            Ok(PluginControl::Switch { control })
        }
        // a texture someone picks, of the app's grains or the plugin's own pictures (interface 15)
        "texture" => {
            let starts = match raw.starts {
                _ if raw.keeps.is_some() => return Err(Refusal::Described),
                None => 0,
                Some(Value::Number(n)) => match n.as_i64() {
                    Some(n) if (0..=i64::from(procraft_engine::textures::GRAIN_COUNT)).contains(&n) => n as i32,
                    _ => return Err(Refusal::Described),
                },
                Some(_) => return Err(Refusal::Described),
            };
            let own = raw.own.unwrap_or_default();
            if own.len() > MOST_FILES || !own.iter().all(|k| valid_key(k)) {
                return Err(Refusal::Described);
            }
            let own = own.iter().map(|k| format!("{id}.{k}")).collect();
            Ok(PluginControl::Texture { control: PluginTexture { key, label: raw.label, own, value: starts, starts } })
        }
        // a folder someone picks outside the app, kept with the plugin (interface 22): nothing to
        // start at, and no setting the app used to have
        "folder" => {
            if raw.keeps.is_some() || raw.starts.is_some() || raw.accepts.is_some() {
                return Err(Refusal::Described);
            }
            Ok(PluginControl::Folder { control: PluginFolder { key, label: raw.label, chosen: None, location: None } })
        }
        // a file someone picks, kept with the plugin; it takes no setting the app used to have
        "file" => {
            let accepts = match raw.accepts.as_deref() {
                _ if raw.keeps.is_some() => return Err(Refusal::Described),
                Some("sound") => PluginFileKind::Sound,
                Some("picture") => PluginFileKind::Picture,
                // a kind of file this app doesn't take
                Some(_) => return Err(Refusal::Newer),
                None => return Err(Refusal::Described),
            };
            Ok(PluginControl::File { control: PluginFile { key, label: raw.label, accepts, chosen: None } })
        }
        "slider" => {
            let (least, most, step) =
                (raw.least.unwrap_or(f64::NAN), raw.most.unwrap_or(f64::NAN), raw.step.unwrap_or(1.0));
            let shown_times = raw.shown_times.unwrap_or(1.0);
            let finite = [least, most, step, shown_times].iter().all(|v| v.is_finite() && v.abs() < 1e9);
            if !finite || least >= most || step <= 0.0 || shown_times <= 0.0 || (most - least) / step > MOST_STOPS {
                return Err(Refusal::Described);
            }
            let (unit, at_least) = (raw.unit.unwrap_or_default(), raw.at_least);
            if !shows_plainly(&unit, 8) || at_least.as_deref().is_some_and(|text| !shows_plainly(text, 16)) {
                return Err(Refusal::Described);
            }
            // a name for every step, or none
            let names = raw.names.unwrap_or_default();
            let steps = ((most - least) / step).round() + 1.0;
            let named = |n: &String| !n.trim().is_empty() && shows_plainly(n, 16);
            if !names.is_empty() && (names.len() as f64 != steps || !names.iter().all(named)) {
                return Err(Refusal::Described);
            }
            let starts = match raw.starts {
                None => least,
                Some(value) => value.as_f64().filter(|v| v.is_finite()).ok_or(Refusal::Described)?,
            };
            let mut control = PluginSlider {
                key,
                label: raw.label,
                least: least as f32,
                most: most as f32,
                step: step as f32,
                value: 0.0,
                starts: 0.0,
                shown_times: shown_times as f32,
                unit,
                at_least,
                names,
                live: raw.live.unwrap_or(false),
            };
            // kept to the range and on a step, however it was written
            control.starts = control.stepped(starts as f32);
            control.value = control.starts;
            Ok(PluginControl::Slider { control })
        }
        // a kind of control this app doesn't have
        _ => Err(Refusal::Newer),
    }
}

/// `text` as a manifest, if it holds up.
fn read_manifest(text: &[u8]) -> Result<Manifest, Refusal> {
    if text.len() > MOST_MANIFEST {
        return Err(Refusal::TooBig);
    }
    // the schema first: a newer format is refused as newer, whatever else it holds
    let head: Head = serde_json::from_slice(text).map_err(|_| Refusal::NotAPlugin)?;
    if head.schema > SCHEMA {
        return Err(Refusal::Newer);
    }
    if head.schema < SCHEMA {
        return Err(Refusal::NotAPlugin);
    }
    // (a field given twice is an error here, so a manifest can't say two things to two readers)
    let raw: RawManifest = serde_json::from_slice(text).map_err(|_| Refusal::Described)?;
    // what it may reach, by names this app gives; anything else it asks for is refused
    let mut permissions = Vec::new();
    for asked in &raw.permissions {
        let given =
            asked.as_str().and_then(crate::plugin_commands::PluginPermission::named).ok_or(Refusal::Permissions)?;
        if permissions.contains(&given) {
            return Err(Refusal::Described);
        }
        permissions.push(given);
    }
    let compat = raw.compat.procraft.ok_or(Refusal::OtherApp)?;
    // a part this app doesn't have is for a newer one
    let holds = raw.holds.iter().map(|name| PluginPart::named(name).ok_or(Refusal::Newer));
    let holds = holds.collect::<Result<Vec<_>, _>>()?;
    if compat.interface.is_some_and(|interface| interface > INTERFACE) {
        return Err(Refusal::Newer);
    }
    let publisher = raw.publisher.map(|p| p.name).unwrap_or_default();
    let named = !raw.name.trim().is_empty() && shows_plainly(&raw.name, 60);
    if !valid_id(&raw.id) || !valid_version(&raw.version) || !named {
        return Err(Refusal::Described);
    }
    if !shows_plainly(&raw.description, 200) || !shows_plainly(&publisher, 60) {
        return Err(Refusal::Described);
    }
    if compat.min_app.as_deref().is_some_and(|v| version_parts(v).is_none()) || compat.interface == Some(0) {
        return Err(Refusal::Described);
    }
    let menu_under = raw.menu.as_ref().and_then(|m| m.under.as_deref());
    let (mut shape, menu_under) = shape_of(
        &raw.id,
        &raw.controls,
        &raw.actions,
        &raw.filters,
        &raw.replaces,
        (&raw.takes, raw.brushes.as_ref()),
        &raw.brush_controls,
        menu_under,
        compat.interface,
    )?;
    // made of features (interface 16): each holds what a plugin held
    let featured = !shape.features.is_empty();
    let mut files = Vec::with_capacity(raw.files.len().min(MOST_ENTRIES));
    let mut seen = HashSet::new();
    for file in raw.files {
        let fits = safe_path(&file.path) && file.path != MANIFEST && sha_digits(&file.sha256);
        if !fits || !seen.insert(file.path.to_ascii_lowercase()) {
            return Err(Refusal::Described);
        }
        files.push(Listed { path: file.path, sha256: file.sha256, size: file.size });
    }
    // files and folders besides the rest, and only for code to use (a filter has no way to);
    // folders from interface 22
    let picked = raw.controls.iter().filter(|c| c.kind == "file" || c.kind == "folder").count();
    let folders = raw.controls.iter().any(|c| c.kind == "folder");
    if folders && compat.interface.is_none_or(|interface| interface < FOLDERS_FROM) {
        return Err(Refusal::Described);
    }
    let more = compat.interface.is_some_and(|interface| interface >= 6) && !holds.contains(&PluginPart::Filter);
    let most = if more { MOST_CONTROLS_6 } else { MOST_CONTROLS };
    // (a plugin of features counted already, its files among its controls; each filter it lists
    // reads its own few)
    if (!featured && raw.controls.len() - picked > most) || picked > MOST_FILES {
        return Err(Refusal::Described);
    }
    let filter_reads_all = holds.contains(&PluginPart::Filter) && !featured;
    if picked > 0 && (!holds.contains(&PluginPart::Hooks) || filter_reads_all) {
        return Err(Refusal::Described);
    }
    let mut keys = HashSet::new();
    if !raw.controls.iter().all(|c| keys.insert(c.key.clone())) {
        return Err(Refusal::Described);
    }
    let id = raw.id;
    let keys: Vec<String> = raw.controls.iter().map(|c| c.key.clone()).collect();
    let controls = raw.controls.into_iter().map(|c| checked_control(&id, c)).collect::<Result<Vec<_>, _>>()?;
    // a menu of its own, setting its controls
    shape.menu = menu_of(&id, raw.menu, &keys, &controls, holds.contains(&PluginPart::Hooks), menu_under)?;
    if id.starts_with(OWN_PREFIX) {
        return Err(Refusal::Described);
    }
    let distinct: HashSet<_> = holds.iter().collect();
    if holds.is_empty() || distinct.len() != holds.len() || compat.interface.is_none() {
        return Err(Refusal::Described);
    }
    if !textures_hold_up(&controls, compat.interface) {
        return Err(Refusal::Described);
    }
    // what it says it holds and what it lists agree, so what the import screen tells is all of it
    let has = |part| holds.contains(&part);
    let fits = |path: &str, most: usize| files.iter().find(|f| f.path == path).map(|f| f.size <= most);
    let module = fits(MODULE, MOST_MODULE) == has(PluginPart::Hooks).then_some(true);
    // one filter in `filter.wgsl`, or several listed (interface 5), each with its code
    let listed = !shape.filters.is_empty();
    if listed && (!has(PluginPart::Filter) || compat.interface.is_none_or(|interface| interface < 5)) {
        return Err(Refusal::Described);
    }
    let filter = fits(FILTER_CODE, MOST_SHADER) == (has(PluginPart::Filter) && !listed).then_some(true);
    let filters = shape.filters.iter().all(|f| fits(&f.code, MOST_SHADER) == Some(true));
    // a set of brushes (interface 14), in a file of its own
    if has(PluginPart::Brushes) && compat.interface.is_none_or(|interface| interface < 14) {
        return Err(Refusal::Described);
    }
    let brushes = fits(BRUSH_SET, MOST_ENTRY) == has(PluginPart::Brushes).then_some(true);
    let set_said = shape.brushes_under.is_some() || shape.brushes_from.is_some();
    if set_said && !has(PluginPart::Brushes) {
        return Err(Refusal::Described);
    }
    if !module || !filter || !filters || !brushes {
        return Err(Refusal::Described);
    }
    let mut hooks = Vec::new();
    if has(PluginPart::Hooks) {
        let most_hooks = if featured { MOST_FEATURED_HOOKS } else { MOST_HOOKS };
        if featured && raw.hooks.len() > most_hooks {
            return Err(Refusal::Described);
        }
        // a hook this app doesn't have is a plugin for a newer one
        for name in raw.hooks.iter().take(most_hooks + 1) {
            let hook = Hook::named(name).ok_or(Refusal::Newer)?;
            if hooks.contains(&hook) {
                return Err(Refusal::Described);
            }
            hooks.push(hook);
        }
        // code steps in somewhere, does something when tapped, or makes artworks or layers of its
        // own
        if hooks.is_empty()
            && raw.actions.is_empty()
            && raw.kinds.is_empty()
            && raw.layer_kinds.is_empty()
            && raw.brush_engines.is_empty()
        {
            return Err(Refusal::Described);
        }
    } else if !raw.hooks.is_empty() {
        return Err(Refusal::Described);
    }
    // a plugin of features: code, each feature's hooks the plugin's and each of those some
    // feature's; its filters listed, each with its own few controls; no kinds of its own, which no
    // feature could hold; and a picker's picture has room for all its controls
    if featured {
        let theirs = shape.features.iter().all(|f| f.hooks.iter().all(|h| hooks.contains(h)));
        let held = hooks.iter().all(|h| shape.features.iter().any(|f| f.hooks.contains(h)));
        let kinds = !raw.kinds.is_empty() || !raw.layer_kinds.is_empty() || !raw.brush_engines.is_empty();
        let one_filter = has(PluginPart::Filter) && !listed;
        let picture = hooks.contains(&Hook::Picker) && controls.len() > MOST_PICTURE_VALUES;
        if !has(PluginPart::Hooks) || !theirs || !held || kinds || one_filter || picture {
            return Err(Refusal::Described);
        }
    }
    // a picker brings its picture, and only a picker has one
    if fits(PICTURE_CODE, MOST_SHADER) != hooks.contains(&Hook::Picker).then_some(true) {
        return Err(Refusal::Described);
    }
    // a filter's controls are its own, so none of them can be a moved setting (of a plugin of
    // features, those its filters read)
    let read_by_filters: Vec<&PluginControl> = if featured {
        shape.filters.iter().flat_map(|f| f.controls.iter().filter_map(|&at| controls.get(at))).collect()
    } else {
        controls.iter().collect()
    };
    if has(PluginPart::Filter) && read_by_filters.iter().any(|c| !c.key().starts_with(&format!("{id}."))) {
        return Err(Refusal::Described);
    }
    // rows in the brush studio, for code to read (a filter has no way to)
    let fits_brush =
        raw.brush_controls.is_empty() || (has(PluginPart::Hooks) && (!has(PluginPart::Filter) || featured));
    let mut brush_keys = HashSet::new();
    if !fits_brush
        || raw.brush_controls.len() > MOST_BRUSH_CONTROLS
        || !raw.brush_controls.iter().all(|c| brush_keys.insert(c.key.clone()))
    {
        return Err(Refusal::Described);
    }
    let brush_controls =
        raw.brush_controls.into_iter().map(|c| checked_brush_control(&id, c, &raw.replaces, compat.interface));
    let brush_controls = brush_controls.collect::<Result<Vec<_>, _>>()?;
    // actions and what they reach, for code written to interface 2 or later
    let code = has(PluginPart::Hooks) && compat.interface.is_some_and(|interface| interface >= 2);
    let mut action_keys = HashSet::new();
    let plain = |text: &String| !text.trim().is_empty() && shows_plainly(text, 40);
    let most_actions = if featured { MOST_FEATURED_ACTIONS } else { MOST_ACTIONS };
    let actions_fit = raw.actions.len() <= most_actions
        && raw.actions.iter().all(|a| valid_key(&a.key) && plain(&a.label) && action_keys.insert(a.key.clone()));
    let mut kind_keys = HashSet::new();
    let kinds_fit = raw.kinds.len() <= MOST_KINDS
        && raw.kinds.iter().all(|k| valid_key(&k.key) && plain(&k.label) && kind_keys.insert(k.key.clone()));
    let extras = !raw.actions.is_empty() || !permissions.is_empty() || !raw.kinds.is_empty();
    if !actions_fit || !kinds_fit || (!code && extras) {
        return Err(Refusal::Described);
    }
    let kinds: Vec<crate::plugin_canvas::PluginKind> = raw
        .kinds
        .into_iter()
        .map(|k| crate::plugin_canvas::PluginKind {
            key: format!("{id}.{}", k.key),
            label: k.label,
            badge: None,
            exports: Vec::new(),
        })
        .collect();
    // kinds of layer, for code written to interface 3 or later: on the layers hook while it has any
    let layered = has(PluginPart::Hooks) && compat.interface.is_some_and(|interface| interface >= 3);
    let mut layer_keys = HashSet::new();
    let layer_kinds_fit = raw.layer_kinds.len() <= MOST_KINDS
        && raw.layer_kinds.iter().all(|k| valid_key(&k.key) && plain(&k.label) && layer_keys.insert(k.key.clone()));
    if !layer_kinds_fit || (!layered && !raw.layer_kinds.is_empty()) {
        return Err(Refusal::Described);
    }
    // each with the badge its thumbnails show it by, if it draws one that holds up
    let mut layer_kinds: Vec<crate::plugin_canvas::PluginKind> = Vec::new();
    for k in raw.layer_kinds {
        let badge = match k.badge {
            Some(raw) => Some(badge_of(raw).ok_or(Refusal::Described)?),
            None => None,
        };
        // and the files its plugin makes of its layers
        let mut export_keys = HashSet::new();
        if k.exports.len() > MOST_EXPORTS
            || !k.exports.iter().all(|e| export_fits(e) && export_keys.insert(e.key.clone()))
        {
            return Err(Refusal::Described);
        }
        let key = format!("{id}.{}", k.key);
        let exports = k
            .exports
            .into_iter()
            .map(|e| crate::PluginExport {
                key: format!("{key}.{}", e.key),
                label: e.label,
                extension: e.extension,
                mime: e.mime,
            })
            .collect();
        layer_kinds.push(crate::plugin_canvas::PluginKind { key, label: k.label, badge, exports });
    }
    if !layer_kinds.is_empty() {
        hooks.push(Hook::Layers);
    }
    // brush engines, for code written to interface 4 or later: on the brushes hook while it has any
    let engined = has(PluginPart::Hooks) && compat.interface.is_some_and(|interface| interface >= 4);
    let mut engine_keys = HashSet::new();
    let ending =
        |e: &String| (1..=8).contains(&e.len()) && e.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    let engines_fit = raw.brush_engines.len() <= MOST_ENGINES
        && raw.brush_engines.iter().all(|e| {
            let mut endings = HashSet::new();
            valid_key(&e.key)
                && plain(&e.label)
                && engine_keys.insert(e.key.clone())
                && (1..=MOST_ACCEPTS).contains(&e.accepts.len())
                && e.accepts.iter().all(|a| ending(a) && endings.insert(a.clone()))
                // a studio is one of its actions, which shows a panel
                && e.studio.as_ref().is_none_or(|key| {
                    raw.actions.iter().any(|a| &a.key == key)
                        && permissions.contains(&crate::plugin_commands::PluginPermission::Panel)
                })
        });
    if !engines_fit || (!engined && !raw.brush_engines.is_empty()) {
        return Err(Refusal::Described);
    }
    let brush_engines: Vec<crate::plugin_brushes::BrushEngine> = raw
        .brush_engines
        .into_iter()
        .map(|e| crate::plugin_brushes::BrushEngine {
            key: format!("{id}.{}", e.key),
            label: e.label,
            accepts: e.accepts,
            studio: e.studio.map(|key| format!("{id}.{key}")),
        })
        .collect();
    if !brush_engines.is_empty() {
        hooks.push(Hook::Brushes);
    }
    // a place this app doesn't have is for a newer one
    let mut actions = Vec::new();
    for a in raw.actions {
        let place = match a.place.as_deref() {
            None => crate::plugins::PluginPlace::Card,
            Some(name) => crate::plugins::PluginPlace::named(name).ok_or(Refusal::Newer)?,
        };
        actions.push(crate::plugins::PluginAction { key: format!("{id}.{}", a.key), label: a.label, place });
    }
    Ok(Manifest {
        id,
        name: raw.name,
        holds,
        version: raw.version,
        publisher,
        description: raw.description,
        experimental: raw.experimental.unwrap_or(true),
        interface: compat.interface.unwrap_or(1),
        hooks,
        controls,
        brush_controls,
        actions,
        kinds,
        layer_kinds,
        brush_engines,
        permissions,
        shape,
        files,
    })
}

/// What the manifest of plugin `id` says beyond its card's list (its controls, actions, filters,
/// rows in the brush studio, menu and what it replaces, as written), if it holds up: a control or
/// anything else comes under a switch of the list's top level, or under one that comes under such a
/// switch; a listed filter has a plain key and name and reads controls that aren't files, each once;
/// a plugin it takes the place of has an id beside its own (the same but for the last part), and
/// what took its values is here. And the switch its menu comes under.
///
/// A plugin of features (interface 16) has a feature at the top of its list for each of the plugins
/// it is made of, and everything else comes under one: one switch deeper than without, so a feature
/// can hold what a plugin of switches held.
#[allow(clippy::too_many_arguments)]
fn shape_of(
    id: &str,
    controls: &[RawControl],
    actions: &[RawAction],
    listed: &[RawFilter],
    replacing: &[RawReplaced],
    (taking, brushes): (&[RawTaken], Option<&RawBrushes>),
    brush_rows: &[RawBrushControl],
    menu_under: Option<&str>,
    interface: Option<u32>,
) -> Result<(Shape, Option<usize>), Refusal> {
    let featured = controls.iter().any(|c| c.kind == "feature");
    if featured && interface.is_none_or(|i| i < FEATURES_FROM) {
        return Err(Refusal::Described);
    }
    let position = |key: &str| controls.iter().position(|c| c.key == key);
    // the switches above the control at `at`, nearest first, if they hold up: each a switch (or a
    // feature), a few at most (so a loop never does)
    let above = |at: usize| -> Option<Vec<usize>> {
        let mut chain = Vec::new();
        let mut next = controls.get(at)?.under.as_deref();
        while let Some(key) = next {
            let up = position(key)?;
            let c = controls.get(up)?;
            if !matches!(c.kind.as_str(), "switch" | "feature") || chain.len() >= MOST_ABOVE {
                return None;
            }
            chain.push(up);
            next = c.under.as_deref();
        }
        Some(chain)
    };
    // what something may come under: a switch of the top level or one that comes under one; with
    // features, a feature, or a switch with one or two above it, a feature at the top
    let switch_at = |key: &str| {
        let at =
            position(key).filter(|&at| controls.get(at).is_some_and(|c| c.kind == "switch" || c.kind == "feature"));
        let fits = at.is_some_and(|at| {
            let Some(chain) = above(at) else { return false };
            let kind = controls.get(at).map(|c| c.kind.as_str());
            let top_is_feature = chain.last().and_then(|&top| controls.get(top)).is_some_and(|c| c.kind == "feature");
            if featured {
                (kind == Some("feature") && chain.is_empty())
                    || (kind == Some("switch") && (1..MOST_ABOVE).contains(&chain.len()) && top_is_feature)
            } else {
                kind == Some("switch") && chain.len() < MOST_ABOVE - 1
            }
        });
        at.filter(|_| fits).ok_or(Refusal::Described)
    };
    // with features, everything comes under one; a feature under nothing
    let under_of = |under: Option<&str>| match under {
        Some(key) => switch_at(key).map(Some),
        None if featured => Err(Refusal::Described),
        None => Ok(None),
    };
    let control_at = |key: &str| {
        controls.iter().position(|c| c.key == key && c.kind != "file" && c.kind != "folder").ok_or(Refusal::Described)
    };
    let mut under = Vec::new();
    for c in controls {
        under.push(if c.kind == "feature" {
            if c.under.is_some() {
                return Err(Refusal::Described);
            }
            None
        } else if c.kind == "folder" && c.under.is_none() {
            // a folder may be the whole plugin's, its features' alike, at the top of its card
            // (interface 22)
            None
        } else {
            under_of(c.under.as_deref())?
        });
    }
    let action_under = actions.iter().map(|a| under_of(a.under.as_deref()));
    let action_under = action_under.collect::<Result<Vec<_>, _>>()?;
    let brush_under = brush_rows.iter().map(|r| under_of(r.under.as_deref()));
    let brush_under = brush_under.collect::<Result<Vec<_>, _>>()?;
    let menu_under = menu_under.map(switch_at).transpose()?;
    // where else a control shows: a switch or a slider, on the card or in a place this app has
    let mut places = Vec::new();
    for c in controls {
        let place = match c.place.as_deref() {
            None => plugins::PluginPlace::Card,
            Some(name) => plugins::PluginPlace::named(name).ok_or(Refusal::Newer)?,
        };
        let fits = matches!(place, plugins::PluginPlace::Card | plugins::PluginPlace::Canvas);
        let elsewhere = place != plugins::PluginPlace::Card;
        if !fits || (elsewhere && matches!(c.kind.as_str(), "file" | "texture" | "feature" | "folder")) {
            return Err(Refusal::Described);
        }
        places.push(place);
    }
    // a press for a switch or an action, each press once
    let mut pressed = HashSet::new();
    let mut press_of = |text: Option<&str>, fits: bool| match text {
        None => Ok(None),
        Some(text) => {
            let press = plugin_press(text).filter(|_| fits).ok_or(Refusal::Described)?;
            if !pressed.insert(press.text()) {
                return Err(Refusal::Described);
            }
            Ok(Some(press))
        }
    };
    let presses = controls.iter().map(|c| press_of(c.press.as_deref(), c.kind == "switch"));
    let presses = presses.collect::<Result<Vec<_>, _>>()?;
    let action_presses = actions.iter().map(|a| press_of(a.press.as_deref(), true));
    let action_presses = action_presses.collect::<Result<Vec<_>, _>>()?;
    // kept with each artwork: a switch or a slider that keeps no setting the app used to have
    let artwork: Vec<bool> = controls.iter().map(|c| c.artwork == Some(true)).collect();
    let unkeepable = |c: &RawControl| matches!(c.kind.as_str(), "file" | "feature" | "folder") || c.keeps.is_some();
    if controls.iter().zip(&artwork).any(|(c, &kept)| kept && unkeepable(c)) {
        return Err(Refusal::Described);
    }
    let layered = listed.iter().filter(|f| f.layer).count();
    if listed.len() - layered > MOST_FILTERS || layered > MOST_LAYER_FILTERS || replacing.len() > MOST_REPLACES {
        return Err(Refusal::Described);
    }
    let mut keys = HashSet::new();
    let mut filters = Vec::new();
    for f in listed {
        let plain = !f.label.trim().is_empty() && shows_plainly(&f.label, 40);
        if !valid_key(&f.key) || !plain || !keys.insert(f.key.as_str()) {
            return Err(Refusal::Described);
        }
        let controls = f.controls.iter().map(|key| control_at(key)).collect::<Result<Vec<_>, _>>()?;
        // (a filter has room for so many values)
        let room = !featured || controls.len() <= MOST_CONTROLS;
        if controls.iter().collect::<HashSet<_>>().len() != controls.len() || !room {
            return Err(Refusal::Described);
        }
        filters.push(ListedFilter {
            id: format!("{}.{}", id, f.key),
            label: f.label.clone(),
            code: format!("{FILTERS_FOLDER}/{}.wgsl", f.key),
            controls,
            under: under_of(f.under.as_deref())?,
            layer: layer_shape_of(id, f, interface)?,
        });
    }
    let space = |id: &str| id.rsplit_once('.').map(|(space, _)| space.to_owned());
    let mut ids = HashSet::new();
    let mut replaces = Vec::new();
    // a control a plugin it takes the place of had, its file too: any control here
    let any_at = |key: &str| position(key).ok_or(Refusal::Described);
    for r in replacing {
        let beside = space(&r.id).is_some() && space(&r.id) == space(id);
        if !valid_id(&r.id) || r.id == id || !beside || !ids.insert(r.id.as_str()) {
            return Err(Refusal::Described);
        }
        let mut controls = Vec::new();
        for (old, new) in &r.controls {
            if !valid_key(old) {
                return Err(Refusal::Described);
            }
            let at = if featured { any_at(new)? } else { control_at(new)? };
            controls.push((format!("{}.{old}", r.id), at));
        }
        let switch = r.switch.as_deref().map(switch_at).transpose()?;
        replaces.push(Replaced { id: r.id.clone(), switch, controls, marks: r.marks });
    }
    // the marks of one at most, from interface 16
    let marking = replaces.iter().filter(|r| r.marks).count();
    if marking > 1 || (marking == 1 && interface.is_none_or(|i| i < FEATURES_FROM)) {
        return Err(Refusal::Described);
    }
    // features taken from plugins that stay (interface 19), by a plugin of features: each from a
    // plugin beside it that it doesn't take the place of, once, taken by a feature here, its
    // controls' values by controls here
    let takes_fit = taking.is_empty() || (featured && interface.is_some_and(|i| i >= TAKES_FROM));
    if !takes_fit || taking.len() > MOST_TAKES {
        return Err(Refusal::Described);
    }
    let mut features_taken = HashSet::new();
    let mut takes = Vec::new();
    for t in taking {
        let beside = space(&t.id).is_some() && space(&t.id) == space(id);
        let replaced = replaces.iter().any(|r| r.id == t.id);
        let once = features_taken.insert((t.id.as_str(), t.feature.as_str()));
        if !valid_id(&t.id) || t.id == id || !beside || replaced || !valid_key(&t.feature) || !once {
            return Err(Refusal::Described);
        }
        let switch = position(&t.switch).filter(|&at| controls.get(at).is_some_and(|c| c.kind == "feature"));
        let switch = switch.ok_or(Refusal::Described)?;
        let mut taken = Vec::new();
        for (old, new) in &t.controls {
            if !valid_key(old) {
                return Err(Refusal::Described);
            }
            taken.push((format!("{}.{old}", t.id), any_at(new)?));
        }
        let feature = format!("{}.{}", t.id, t.feature);
        takes.push(Taken { id: t.id.clone(), feature, switch, controls: taken, filters: t.filters });
    }
    // what its brushes come under, and whose set they are (interface 19): one it takes the place of
    let (brushes_under, brushes_from) = match brushes {
        None => (None, None),
        Some(_) if interface.is_none_or(|i| i < TAKES_FROM) => return Err(Refusal::Described),
        Some(b) => {
            let under = b.under.as_deref().map(switch_at).transpose()?;
            let from = match &b.from {
                None => None,
                Some(from) if replaces.iter().any(|r| r.id == *from) => Some(from.clone()),
                Some(_) => return Err(Refusal::Described),
            };
            (under, from)
        }
    };
    let mut shape = Shape {
        under,
        action_under,
        places,
        presses,
        action_presses,
        artwork,
        filters,
        replaces,
        takes,
        menu: None,
        brush_under,
        features: Vec::new(),
        brushes_under,
        brushes_from,
    };
    shape.features = features_of(controls, &shape, actions.len())?;
    Ok((shape, menu_under))
}

/// The features `controls` head (interface 16), each with its words and hooks, in `shape`, which
/// lists `actions` actions; if they hold up: a few, each with a few controls and actions under it
/// and its hooks each once, and the plugin's controls and actions a few in all.
fn features_of(controls: &[RawControl], shape: &Shape, actions: usize) -> Result<Vec<Feature>, Refusal> {
    let mut features = Vec::new();
    for (at, c) in controls.iter().enumerate().filter(|(_, c)| c.kind == "feature") {
        let mut hooks = Vec::new();
        for name in c.hooks.iter().flatten() {
            let hook = Hook::named(name).ok_or(Refusal::Newer)?;
            if hooks.contains(&hook) {
                return Err(Refusal::Described);
            }
            hooks.push(hook);
        }
        features.push(Feature { at, description: c.description.clone().unwrap_or_default(), hooks });
    }
    if features.is_empty() {
        return Ok(features);
    }
    let in_all = controls.len() <= MOST_FEATURED_CONTROLS && actions <= MOST_FEATURED_ACTIONS;
    if features.len() > MOST_FEATURES || !in_all {
        return Err(Refusal::Described);
    }
    let lookup = Shape { features: features.clone(), ..shape.clone() };
    for (f, feature) in features.iter().enumerate() {
        let mine = |under: &Option<usize>| lookup.feature_under(*under) == Some(f);
        let under = (0..controls.len()).filter(|&at| at != feature.at && lookup.feature_of(at) == Some(f)).count();
        let acts = shape.action_under.iter().filter(|u| mine(u)).count();
        if under > MOST_FEATURE_CONTROLS || acts > MOST_ACTIONS {
            return Err(Refusal::Described);
        }
    }
    Ok(features)
}

/// The press a manifest writes for a switch or an action, if it is one a keyboard sends (not one
/// of the view's own moves).
fn plugin_press(text: &str) -> Option<crate::KeyCombo> {
    crate::KeyCombo::read(text).filter(|press| !press.key.starts_with(crate::keys::VIEW_KEY))
}

/// What a filter's code calls `controls`, in their order: their keys, without the plugin's id.
pub(crate) fn value_names(controls: &[PluginControl]) -> Vec<String> {
    controls.iter().map(|control| control.key().rsplit('.').next().unwrap_or("").to_owned()).collect()
}

/// Why `data`, listed by `manifest` under `path`, can't run, if it is code at all: checked by
/// reading it, as nothing in it runs until the plugin is on or the filter opened.
fn code_refused(manifest: &Manifest, path: &str, data: &[u8]) -> Option<Refusal> {
    // a set of brushes: a set file that reads, of a few brushes
    if manifest.holds.contains(&PluginPart::Brushes) && path == BRUSH_SET {
        let count = procraft_engine::brushfile::read_set(data).map_or(0, |(_, brushes)| brushes.len());
        return (!(1..=MOST_SET_BRUSHES).contains(&count)).then_some(Refusal::Brushes);
    }
    if manifest.holds.contains(&PluginPart::Filter) && path == FILTER_CODE {
        let text = std::str::from_utf8(data).map_err(|e| e.to_string());
        let read = text.and_then(|text| procraft_gpu::read_filter(text, &value_names(&manifest.controls)));
        let refused = read.map_err(|why| log::warn!("a filter's code was refused: {why}")).is_err();
        return refused.then_some(Refusal::FilterCode);
    }
    // one of several, given its own controls; a filter layer's, its own values, joined to what a
    // filter layer's filter is
    if let Some(listed) = manifest.shape.filters.iter().find(|f| f.code == path) {
        let controls: Vec<PluginControl> =
            listed.controls.iter().filter_map(|&at| manifest.controls.get(at).cloned()).collect();
        let text = std::str::from_utf8(data).map_err(|e| e.to_string());
        let read = text.and_then(|text| match &listed.layer {
            Some(layer) if layer.editor.is_some() => procraft_gpu::read_layer_table_filter(text, &layer.code_keys()),
            Some(layer) => procraft_gpu::read_layer_filter(text, &layer.code_keys()),
            None => procraft_gpu::read_filter(text, &value_names(&controls)),
        });
        let refused = read.map_err(|why| log::warn!("a filter's code was refused: {why}")).is_err();
        return refused.then_some(Refusal::FilterCode);
    }
    if manifest.hooks.contains(&Hook::Picker) && path == PICTURE_CODE {
        let text = std::str::from_utf8(data).map_err(|e| e.to_string());
        let read = text.and_then(|text| match picture_keys(manifest) {
            Some(keys) => procraft_gpu::read_picture_given(text, &keys).map(|_| ()),
            None => procraft_gpu::read_picture(text).map(|_| ()),
        });
        let refused = read.map_err(|why| log::warn!("a picture's code was refused: {why}")).is_err();
        return refused.then_some(Refusal::Code);
    }
    let code = manifest.holds.contains(&PluginPart::Hooks) && path == MODULE;
    let (controls, actions, kinds) =
        (!manifest.controls.is_empty(), !manifest.actions.is_empty(), !manifest.kinds.is_empty());
    (code && !plugin_run::can_act(data, &manifest.hooks, controls, actions, &manifest.permissions, kinds))
        .then_some(Refusal::Code)
}

/// `bytes` read as a plugin file. Every entry is checked by what it claims before any is read,
/// and what is read can't pass what it claimed, so a bomb costs nothing and nothing is written.
pub(crate) fn read_package(bytes: &[u8]) -> Result<Package, Refusal> {
    if bytes.len() > MOST_FILE {
        return Err(Refusal::TooBig);
    }
    let zip = ZipReader::new(bytes).ok_or(Refusal::NotAPlugin)?;
    let mut seen = HashSet::new();
    let (mut count, mut unpacked) = (0_usize, 0_usize);
    for entry in zip.facts() {
        count += 1;
        unpacked = unpacked.saturating_add(entry.size);
        let manifest_too_big = entry.name == MANIFEST && entry.size > MOST_MANIFEST;
        if count > MOST_ENTRIES || entry.size > MOST_ENTRY || unpacked > MOST_UNPACKED || manifest_too_big {
            return Err(Refusal::TooBig);
        }
        let shrunk = entry.size >= RATIO_FROM && entry.size / entry.compressed.max(1) > MOST_RATIO;
        let path = entry.name == MANIFEST || safe_path(entry.name);
        if !entry.plain || entry.encrypted || shrunk || !path || !seen.insert(entry.name.to_ascii_lowercase()) {
            return Err(Refusal::Unsafe);
        }
    }
    if !zip.names_agree() {
        return Err(Refusal::Unsafe);
    }
    zip.size(MANIFEST).ok_or(Refusal::NotAPlugin)?;
    let text = zip.read(MANIFEST).ok_or(Refusal::Damaged)?;
    let manifest = read_manifest(&text)?;
    // exactly what is listed: nothing more, nothing less
    if seen.len() != manifest.files.len() + 1 {
        return Err(Refusal::Mismatch);
    }
    let mut files = Vec::with_capacity(manifest.files.len());
    for listed in &manifest.files {
        if zip.size(&listed.path) != Some(listed.size) {
            return Err(Refusal::Mismatch);
        }
        let data = zip.read(&listed.path).ok_or(Refusal::Damaged)?;
        if sha256(&data) != listed.sha256 {
            return Err(Refusal::Mismatch);
        }
        if let Some(refusal) = code_refused(&manifest, &listed.path, &data) {
            return Err(refusal);
        }
        files.push((listed.path.clone(), data));
    }
    Ok(Package { manifest, text, files })
}

/// What a plugin file at `path` holds, read if it is a plain file of a size worth reading: a pipe
/// would block for ever, a device has no end.
pub fn read_plugin_file(path: &Path) -> Option<Vec<u8>> {
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(MOST_FILE as u64 + 1).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// Writes `data` as a new file, on the disk when this returns.
fn write_new(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(data)?;
    file.sync_all()
}

/// A name in `dir` nothing else has, for work under way: it starts with a dot, so a start finding
/// one knows it was left behind.
fn spare(dir: &Path, what: &str) -> PathBuf {
    static MADE: AtomicU64 = AtomicU64::new(0);
    dir.join(format!(".{what}-{}-{}", std::process::id(), MADE.fetch_add(1, Ordering::Relaxed)))
}

/// Keeps `package` in `dir` as `<id>/<version>/`, whole or not at all: written apart, then moved
/// into place, and any other version of it goes. Returns its folder.
pub(crate) fn keep(dir: &Path, package: &Package) -> Result<PathBuf, Refusal> {
    let stage = spare(dir, "staging");
    let staged = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&stage)?;
        write_new(&stage.join(MANIFEST), &package.text)?;
        for (path, data) in &package.files {
            let to = stage.join(path);
            if let Some(folder) = to.parent() {
                std::fs::create_dir_all(folder)?;
            }
            write_new(&to, data)?;
        }
        Ok(())
    })();
    if let Err(e) = staged {
        log::warn!("a plugin's files couldn't be written: {e}");
        let _ = std::fs::remove_dir_all(&stage);
        return Err(Refusal::NotKept);
    }
    let home = dir.join(&package.manifest.id);
    let folder = home.join(&package.manifest.version);
    let moved = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&home)?;
        // the old one kept aside until the new one is in place
        let old = spare(dir, "replaced");
        let had = std::fs::rename(&folder, &old).is_ok();
        if let Err(e) = std::fs::rename(&stage, &folder) {
            if had {
                let _ = std::fs::rename(&old, &folder);
            }
            return Err(e);
        }
        if had {
            let _ = std::fs::remove_dir_all(&old);
        }
        Ok(())
    })();
    if let Err(e) = moved {
        log::warn!("a plugin couldn't be moved into place: {e}");
        let _ = std::fs::remove_dir_all(&stage);
        return Err(Refusal::NotKept);
    }
    // other versions go, now this one stands; what it keeps beside them stays
    if let Ok(versions) = std::fs::read_dir(&home) {
        for version in versions.flatten() {
            let name = version.file_name();
            let other = name.to_str().is_some_and(|v| v != package.manifest.version && v != CHOSEN && v != KEPT);
            if other {
                let _ = std::fs::remove_dir_all(version.path());
            }
        }
    }
    Ok(folder)
}

/// Where the file picked for `key`, a control of plugin `id` kept in `dir`, is kept (a sound as a
/// WAV file), and the name it was picked by beside it. They go with the plugin.
pub(crate) fn chosen_paths(dir: &Path, id: &str, key: &str) -> (PathBuf, PathBuf) {
    let folder = dir.join(id).join(CHOSEN);
    (folder.join(format!("{key}.wav")), folder.join(format!("{key}.name")))
}

/// Where the folder picked for `key`, a `folder` control of plugin `id` kept in `dir`, is kept (as
/// the app's folder access reads it), the name it was picked by, and what was written to it and read
/// from it (interface 22). They go with the plugin.
pub(crate) fn chosen_folder_paths(dir: &Path, id: &str, key: &str) -> (PathBuf, PathBuf, PathBuf) {
    let folder = dir.join(id).join(CHOSEN);
    (folder.join(format!("{key}.folder")), folder.join(format!("{key}.name")), folder.join(format!("{key}.noted.json")))
}

/// What plugin `id`, kept in `dir`, keeps of its own, if anything holds up.
pub(crate) fn read_kept(dir: &Path, id: &str) -> Option<Vec<u8>> {
    read_capped(&dir.join(id).join(KEPT), plugin_run::MOST_KEPT)
}

/// Keeps `data` for plugin `id`, kept in `dir`, whole: written beside, then moved into place, so a
/// start finds what it kept before or now, never part of it. It goes with the plugin. False if it
/// couldn't be (the plugin is gone, say).
pub(crate) fn write_kept(dir: &Path, id: &str, data: &[u8]) -> bool {
    let home = dir.join(id);
    if data.len() > plugin_run::MOST_KEPT || !home.is_dir() {
        return false;
    }
    let beside = spare(&home, KEPT);
    let moved = write_new(&beside, data).and_then(|()| std::fs::rename(&beside, home.join(KEPT)));
    if moved.is_err() {
        let _ = std::fs::remove_file(&beside);
    }
    moved.map_err(|e| log::warn!("couldn't keep what a plugin keeps: {e}")).is_ok()
}

/// Where the look engine `key` of plugin `id`, kept in `dir`, gave the brush whose data is `data`
/// is kept.
fn look_path(dir: &Path, id: &str, key: &str, data: u64) -> PathBuf {
    dir.join(id).join(LOOKS).join(format!("{key}-{data:016x}"))
}

/// The look engine `key` of plugin `id`, kept in `dir`, gave the brush whose data is `data`, if it
/// was kept. Read now, it is the last to go.
pub(crate) fn read_look(dir: &Path, id: &str, key: &str, data: u64) -> Option<Vec<u8>> {
    let path = look_path(dir, id, key, data);
    let bytes = read_capped(&path, MOST_LOOK)?;
    if let Ok(file) = std::fs::File::options().write(true).open(&path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
    Some(bytes)
}

/// Keeps `look`, which engine `key` of plugin `id`, kept in `dir`, gave the brush whose data is
/// `data`: written beside, then moved into place, so a read finds all of it or none. Those read
/// longest ago go while all take more than a plugin's may.
pub(crate) fn write_look(dir: &Path, id: &str, key: &str, data: u64, look: &[u8]) {
    let home = dir.join(id);
    if look.len() > MOST_LOOK || !home.is_dir() {
        return;
    }
    let folder = home.join(LOOKS);
    let beside = spare(&folder, LOOKS);
    let written = std::fs::create_dir_all(&folder)
        .and_then(|()| crate::storage::write(&beside, look))
        .and_then(|()| std::fs::rename(&beside, look_path(dir, id, key, data)));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&beside);
        log::warn!("couldn't keep a brush's look: {e}");
        return;
    }
    let Ok(entries) = std::fs::read_dir(&folder) else { return };
    // what an earlier run left half written goes
    let ours = format!("-{}-", std::process::id());
    let mut looks: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { return false };
            if name.starts_with('.') && !name.contains(&ours) {
                let _ = std::fs::remove_file(entry.path());
            }
            !name.starts_with('.')
        })
        .filter_map(|entry| {
            let facts = entry.metadata().ok()?;
            Some((facts.modified().ok()?, facts.len(), entry.path()))
        })
        .collect();
    let mut total: u64 = looks.iter().map(|(_, len, _)| len).sum();
    looks.sort_by_key(|(read, ..)| *read);
    for (_, len, path) in looks {
        if total <= MOST_LOOKS {
            break;
        }
        let _ = std::fs::remove_file(path);
        total -= len;
    }
}

/// Takes plugin `id`'s folder out of `dir`: gone from the next read on, even if what is moved
/// aside can't be deleted now (the next start tries again). False if it stays.
pub(crate) fn forget(dir: &Path, id: &str) -> bool {
    clear_stopped(dir, id);
    let home = dir.join(id);
    let aside = spare(dir, "removed");
    if std::fs::rename(&home, &aside).is_err() {
        return !home.exists();
    }
    let _ = std::fs::remove_dir_all(&aside);
    true
}

/// The paths of the plain files under `folder`, `prefix` before each; none if it holds anything
/// else, or more than a plugin may.
fn files_under(folder: &Path, prefix: &str, depth: usize, found: &mut Vec<String>) -> Option<()> {
    if depth > MOST_DEPTH {
        return None;
    }
    for entry in std::fs::read_dir(folder).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name().into_string().ok()?;
        let path = format!("{prefix}{name}");
        // (a link says it is one here, whatever it points at)
        let kind = entry.file_type().ok()?;
        if kind.is_dir() {
            files_under(&entry.path(), &format!("{path}/"), depth + 1, found)?;
        } else if kind.is_file() {
            found.push(path);
        } else {
            return None;
        }
        if found.len() > MOST_ENTRIES {
            return None;
        }
    }
    Some(())
}

/// The plugin in `folder`, if all of it is as its manifest says and it is `id` at `version`. A
/// folder found to hold up isn't read and checked whole again while each of its files is as it was
/// then (see [`HELD_UP`]).
pub(crate) fn read_folder(folder: &Path, id: &str, version: &str) -> Option<Manifest> {
    let mut found = Vec::new();
    files_under(folder, "", 0, &mut found)?;
    // (as the files are before they are read: one changed meanwhile is read again next time)
    let stamps = stamps_of(folder, &found);
    if let Some(manifest) = held_up(folder, stamps.as_deref()).filter(|m| m.id == id && m.version == version) {
        return Some(manifest);
    }
    let text = read_capped(&folder.join(MANIFEST), MOST_MANIFEST)?;
    let manifest = read_manifest(&text).ok()?;
    if manifest.id != id || manifest.version != version || found.len() != manifest.files.len() + 1 {
        return None;
    }
    for listed in &manifest.files {
        if !found.contains(&listed.path) {
            return None;
        }
        let data = read_capped(&folder.join(&listed.path), listed.size)?;
        if data.len() != listed.size || sha256(&data) != listed.sha256 {
            return None;
        }
        if code_refused(&manifest, &listed.path, &data).is_some() {
            return None;
        }
    }
    if let Some(stamps) = stamps {
        hold_up(folder, stamps, &manifest);
    }
    Some(manifest)
}

/// A file of a plugin's folder as it is on the disk: its path in the folder, its size and when it
/// was last written.
type Stamp = (String, u64, std::time::SystemTime);

/// The plugin folders found to hold up (see [`read_folder`]), each with its files as they were and
/// its manifest. Every part of a plugin (its code, its brushes, its filters) is read through its
/// folder, and the apps take the plugins up again on every change of a setting: each file hashed
/// each time took tens of milliseconds on the interface's thread. A folder whose files are as they
/// were (none added, gone, resized or written since) holds up as it did; any other is checked whole.
static HELD_UP: std::sync::Mutex<Vec<(PathBuf, Vec<Stamp>, Manifest)>> = std::sync::Mutex::new(Vec::new());

/// The most folders [`HELD_UP`] remembers (a plugin's folder each), the oldest forgotten first.
const MOST_HELD_UP: usize = 64;

/// The `found` files of `folder` as they are on the disk now, in order; none if one can't be told.
fn stamps_of(folder: &Path, found: &[String]) -> Option<Vec<Stamp>> {
    let mut stamps: Vec<Stamp> = found
        .iter()
        .map(|path| {
            let meta = std::fs::metadata(folder.join(path)).ok()?;
            Some((path.clone(), meta.len(), meta.modified().ok()?))
        })
        .collect::<Option<_>>()?;
    stamps.sort();
    Some(stamps)
}

/// The manifest of `folder`, if it held up when its files were as `stamps` say they are now.
fn held_up(folder: &Path, stamps: Option<&[Stamp]>) -> Option<Manifest> {
    let stamps = stamps?;
    let held = HELD_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    held.iter().find(|(at, was, _)| at == folder && was.as_slice() == stamps).map(|(.., manifest)| manifest.clone())
}

/// `folder` held up, its files as `stamps` say, its manifest `manifest`.
fn hold_up(folder: &Path, stamps: Vec<Stamp>, manifest: &Manifest) {
    let mut held = HELD_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    held.retain(|(at, ..)| at != folder);
    if held.len() >= MOST_HELD_UP {
        held.remove(0);
    }
    held.push((folder.to_path_buf(), stamps, manifest.clone()));
}

/// The set of brushes (interface 14) of the plugin `id` at `version` in `dir`, if it holds one and
/// all of it holds up as it did.
pub(crate) fn read_brush_set(dir: &Path, id: &str, version: &str) -> Option<Vec<u8>> {
    let folder = dir.join(id).join(version);
    let manifest = read_folder(&folder, id, version)?;
    manifest.holds.contains(&PluginPart::Brushes).then_some(())?;
    read_capped(&folder.join(BRUSH_SET), MOST_ENTRY)
}

/// The hooks and the module of the plugin `id` at `version` in `dir`, if all of it holds up as it
/// did.
pub(crate) fn read_code(dir: &Path, id: &str, version: &str) -> Option<(Vec<Hook>, Vec<u8>)> {
    let folder = dir.join(id).join(version);
    let manifest = read_folder(&folder, id, version)?;
    Some((manifest.hooks, read_capped(&folder.join(MODULE), MOST_MODULE)?))
}

/// The brush engines of plugin `id` at `version` in `dir`, if it holds up as it did.
pub(crate) fn read_brush_engines(dir: &Path, id: &str, version: &str) -> Vec<crate::plugin_brushes::BrushEngine> {
    read_folder(&dir.join(id).join(version), id, version).map(|m| m.brush_engines).unwrap_or_default()
}

/// The picture's code of the picker `id` at `version` in `dir`, if it holds up as it did.
pub(crate) fn read_picture_code(dir: &Path, id: &str, version: &str) -> Option<(String, Option<Vec<String>>)> {
    let folder = dir.join(id).join(version);
    let manifest = read_folder(&folder, id, version)?;
    let text = String::from_utf8(read_capped(&folder.join(PICTURE_CODE), MOST_SHADER)?).ok()?;
    Some((text, picture_keys(&manifest)))
}

/// The keys a picker's picture reads its plugin's controls by, in their order, if it is written to
/// interface 6 or later (one written before is given none).
pub(crate) fn picture_keys(manifest: &Manifest) -> Option<Vec<String>> {
    (manifest.interface >= 6).then(|| value_names(&manifest.controls))
}

/// The manifest of plugin `id` at `version` in `dir` and the code of its filter at `path` in its
/// folder ([`FILTER_CODE`], or a listed filter's), if all of it holds up as it did.
pub(crate) fn read_filter_code(dir: &Path, id: &str, version: &str, path: &str) -> Option<(Manifest, String)> {
    let folder = dir.join(id).join(version);
    let manifest = read_folder(&folder, id, version)?;
    let listed = path == FILTER_CODE || manifest.shape.filters.iter().any(|f| f.code == path);
    let text = String::from_utf8(read_capped(&folder.join(path), MOST_SHADER)?).ok()?;
    listed.then_some((manifest, text))
}

/// The code of each filter layer's filter (interface 17) plugin `id` at `version` in `dir`
/// offers, by its path, if all of the plugin holds up as it did.
pub(crate) fn read_layer_filter_codes(dir: &Path, id: &str, version: &str) -> Option<Vec<(String, String)>> {
    let folder = dir.join(id).join(version);
    let manifest = read_folder(&folder, id, version)?;
    let mut out = Vec::new();
    for f in manifest.shape.filters.iter().filter(|f| f.layer.is_some()) {
        let text = String::from_utf8(read_capped(&folder.join(&f.code), MOST_SHADER)?).ok()?;
        out.push((f.code.clone(), text));
    }
    Some(out)
}

/// Where it is written down which filter is on trial: compiling, or not yet drawn. It is there on
/// a start only if the app ended meanwhile.
fn trying_note(dir: &Path) -> PathBuf {
    dir.join("filter-trying")
}

/// Writes down that filter `id` is on trial, on the disk before anything happens to it.
pub(crate) fn note_trying(dir: &Path, id: &str) {
    if let Err(e) = write_replacing(&trying_note(dir), id.as_bytes()) {
        log::warn!("couldn't note a filter on trial: {e}");
    }
}

pub(crate) fn clear_trying(dir: &Path) {
    let _ = std::fs::remove_file(trying_note(dir));
}

/// `data` as `path`, on the disk when this returns.
fn write_replacing(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(data)?;
    file.sync_all()
}

/// A filter on trial when the app last ended is written down as stopped, `why`.
pub(crate) fn settle_trying(dir: &Path, why: &str) {
    if let Some(id) = read_capped(&trying_note(dir), MOST_STOP_NOTE).and_then(|id| String::from_utf8(id).ok()) {
        log::warn!("the app ended with a filter on trial: {id}");
        note_stopped(dir, &id, why);
    }
    clear_trying(dir);
}

/// Where it is written down that plugin `id` stopped: beside its folder, which holds only what its
/// manifest lists.
fn stopped_note(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}{STOPPED_NOTE}"))
}

/// Writes down why plugin `id` stopped, so it stays off through a restart.
pub(crate) fn note_stopped(dir: &Path, id: &str, why: &str) {
    if let Err(e) = crate::storage::write(stopped_note(dir, id), why) {
        log::warn!("couldn't note a plugin stopped: {e}");
    }
}

/// Why plugin `id` stopped, if it did and nobody has asked it to run since.
pub(crate) fn stopped_why(dir: &Path, id: &str) -> Option<String> {
    String::from_utf8(read_capped(&stopped_note(dir, id), MOST_STOP_NOTE)?).ok()
}

pub(crate) fn clear_stopped(dir: &Path, id: &str) {
    let _ = std::fs::remove_file(stopped_note(dir, id));
}

/// Where it is written down which moved settings someone has been told of, a line each.
fn told_note(dir: &Path) -> PathBuf {
    dir.join("told")
}

/// The moved settings someone has been told of.
pub(crate) fn read_told(dir: &Path) -> Vec<String> {
    let text = read_capped(&told_note(dir), MOST_STOP_NOTE).unwrap_or_default();
    String::from_utf8_lossy(&text).lines().map(str::to_owned).collect()
}

/// Writes down that someone has been told of `told`, so it is said once.
pub(crate) fn note_told(dir: &Path, told: &[String]) {
    let written = std::fs::create_dir_all(dir).and_then(|()| crate::storage::write(told_note(dir), told.join("\n")));
    if let Err(e) = written {
        log::warn!("couldn't note what has been told: {e}");
    }
}

/// Where it is written down what the app came with and added, a plugin a line: its id, then ` off`
/// if it starts off.
fn added_note(dir: &Path) -> PathBuf {
    dir.join("added")
}

/// What the app came with and added, each line that reads as one.
pub(crate) fn read_added(dir: &Path) -> Vec<Added> {
    let text = read_capped(&added_note(dir), MOST_ADDED).unwrap_or_default();
    let text = String::from_utf8_lossy(&text);
    let added = text.lines().filter_map(|line| {
        let (id, off) = line.trim().strip_suffix(" off").map_or((line.trim(), false), |id| (id, true));
        valid_id(id).then(|| Added { id: id.to_owned(), off })
    });
    added.collect()
}

/// Writes down `added`, whole: written beside, then moved into place, so a start finds the list as
/// it was or as it is, never part of one. False if it couldn't be.
pub(crate) fn note_added(dir: &Path, added: &[Added]) -> bool {
    let lines: Vec<String> = added.iter().map(|a| if a.off { format!("{} off", a.id) } else { a.id.clone() }).collect();
    let written = std::fs::create_dir_all(dir).and_then(|()| {
        let beside = spare(dir, "added");
        let moved =
            write_new(&beside, lines.join("\n").as_bytes()).and_then(|()| std::fs::rename(&beside, added_note(dir)));
        if moved.is_err() {
            let _ = std::fs::remove_file(&beside);
        }
        moved
    });
    written.map_err(|e| log::warn!("couldn't note what the app added: {e}")).is_ok()
}

/// Where the order someone put the plugins in on the Plugins page is written down: an id a line,
/// the first at the top.
fn order_note(dir: &Path) -> PathBuf {
    dir.join("order")
}

/// The order the plugins were put in, each line that reads as an id, each id once (its first
/// place); none if it was never written down, or can't be read.
pub(crate) fn read_order(dir: &Path) -> Vec<String> {
    let text = read_capped(&order_note(dir), MOST_ADDED).unwrap_or_default();
    let mut order: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&text).lines().map(str::trim) {
        if valid_id(line) && !order.iter().any(|id| id == line) {
            order.push(line.to_owned());
        }
    }
    order
}

/// Writes down `order`, whole, as [`note_added`] writes its list. False if it couldn't be.
pub(crate) fn note_order(dir: &Path, order: &[String]) -> bool {
    let written = std::fs::create_dir_all(dir).and_then(|()| {
        let beside = spare(dir, "order");
        let moved =
            write_new(&beside, order.join("\n").as_bytes()).and_then(|()| std::fs::rename(&beside, order_note(dir)));
        if moved.is_err() {
            let _ = std::fs::remove_file(&beside);
        }
        moved
    });
    written.map_err(|e| log::warn!("couldn't note the plugins' order: {e}")).is_ok()
}

/// `path`, if it is a plain file of at most `most` bytes.
fn read_capped(path: &Path, most: usize) -> Option<Vec<u8>> {
    let facts = std::fs::symlink_metadata(path).ok()?;
    if !facts.is_file() || facts.len() > most as u64 {
        return None;
    }
    std::fs::read(path).ok()
}

/// Every plugin kept in `dir` that holds up, the newest of each. What was left half done goes; a
/// folder that doesn't hold up is passed over and left, in case the fault is ours.
pub(crate) fn read_all(dir: &Path) -> Vec<Manifest> {
    let mut kept = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return kept };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str() else { continue };
        if id.starts_with('.') {
            let _ = std::fs::remove_dir_all(entry.path());
            continue;
        }
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) || !valid_id(id) {
            continue;
        }
        let Ok(versions) = std::fs::read_dir(entry.path()) else { continue };
        let mut newest: Option<Manifest> = None;
        for version in versions.flatten() {
            // the files picked for its controls and what it keeps, beside its versions; what was
            // left half written goes
            let name = version.file_name();
            if name == CHOSEN || name == KEPT || name == LOOKS {
                continue;
            }
            if name.to_str().is_some_and(|n| n.starts_with('.')) {
                let _ = std::fs::remove_file(version.path());
                continue;
            }
            let folder = version.path();
            let found = version.file_name().to_str().and_then(|v| {
                let plain = version.file_type().is_ok_and(|kind| kind.is_dir());
                plain.then(|| read_folder(&folder, id, v)).flatten()
            });
            match found {
                Some(manifest) if newest.as_ref().is_none_or(|n| version_is_older(&n.version, &manifest.version)) => {
                    newest = Some(manifest);
                }
                Some(_) => {}
                None => log::warn!("a kept plugin doesn't hold up: {}", folder.display()),
            }
        }
        kept.extend(newest);
    }
    kept.sort_by(|a, b| a.id.cmp(&b.id));
    kept
}

/// SHA-256 of `data`, in lowercase hex.
pub(crate) fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut h: [u32; 8] =
        [0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19];
    let mut message = data.to_vec();
    message.push(0x80);
    message.resize(message.len() + (64 + 56 - message.len() % 64) % 64, 0);
    message.extend_from_slice(&(data.len() as u64 * 8).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (word, bytes) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for i in 16..64 {
            let at = |back: usize| w.get(i - back).copied().unwrap_or(0);
            let (a, b) = (at(15), at(2));
            let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            let word = at(16).wrapping_add(s0).wrapping_add(at(7)).wrapping_add(s1);
            if let Some(slot) = w.get_mut(i) {
                *slot = word;
            }
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut last] = h;
        for (k, word) in K.iter().zip(&w) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let t1 = last.wrapping_add(s1).wrapping_add(choice).wrapping_add(*k).wrapping_add(*word);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            last = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (sum, add) in h.iter_mut().zip([a, b, c, d, e, f, g, last]) {
            *sum = sum.wrapping_add(add);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use procraft_engine::zip::ZipWriter;
    use serde_json::json;

    use super::*;

    pub(crate) const SHADER: &[u8] = b"fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> { return colour; }";

    /// A manifest as a test starts from: a filter, valid, listing [`SHADER`].
    pub(crate) fn filter(id: &str) -> Value {
        json!({
            "schema": 1,
            "id": id,
            "name": "A filter",
            "version": "1.0.0",
            "holds": ["filter"],
            "publisher": { "name": "Someone" },
            "description": "Changes a layer.",
            "compat": { "procraft": { "min_app": "0.0.1", "interface": 1 } },
            "files": [{ "path": "filter.wgsl", "sha256": sha256(SHADER), "size": SHADER.len() }],
            "permissions": [],
            "controls": [
                { "key": "cut", "type": "slider", "label": "Cut-off", "least": 0, "most": 1, "step": 0.01, "starts": 0.5 },
                { "key": "keep_colour", "type": "switch", "label": "Keep the colour", "starts": false },
            ],
        })
    }

    /// A manifest for a plugin of `module`, valid, on the position hook, with no controls.
    pub(crate) fn coded(id: &str, module: &[u8]) -> Value {
        json!({
            "schema": 1,
            "id": id,
            "name": "A plugin",
            "version": "1.0.0",
            "holds": ["hooks"],
            "publisher": { "name": "Someone" },
            "description": "Changes the app.",
            "compat": { "procraft": { "min_app": "0.0.1", "interface": 1 } },
            "files": [{ "path": MODULE, "sha256": sha256(module), "size": module.len() }],
            "permissions": [],
            "hooks": ["position"],
        })
    }

    /// A plugin file of `manifest` and `files`, as they are.
    pub(crate) fn pack(manifest: &Value, files: &[(&str, &[u8])]) -> Vec<u8> {
        pack_text(manifest.to_string().as_bytes(), files)
    }

    fn pack_text(manifest: &[u8], files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = ZipWriter::new();
        zip.add(MANIFEST, manifest);
        for (name, data) in files {
            zip.add(name, data);
        }
        zip.finish()
    }

    fn refusal(bytes: &[u8]) -> Refusal {
        read_package(bytes).err().expect("it was taken")
    }

    fn with(mut manifest: Value, path: &str, value: Value) -> Value {
        *manifest.pointer_mut(path).expect(path) = value;
        manifest
    }

    fn a_filter() -> Value {
        filter("org.example.filter")
    }

    fn a_filter_file(manifest: &Value) -> Vec<u8> {
        pack(manifest, &[("filter.wgsl", SHADER)])
    }

    #[test]
    fn sha_256_matches_the_published_vectors() {
        assert_eq!(sha256(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let two_blocks = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(sha256(two_blocks), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        // a million a's: the padding falling at every place in a block on the way
        let million = vec![b'a'; 1_000_000];
        assert_eq!(sha256(&million), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    #[test]
    fn a_good_filter_is_read() {
        let package = read_package(&a_filter_file(&a_filter())).unwrap();
        assert_eq!(package.manifest.id, "org.example.filter");
        assert_eq!(package.files.len(), 1);
    }

    #[test]
    fn what_isn_t_a_plugin_is_said_to_be() {
        assert_eq!(refusal(b"not a zip at all"), Refusal::NotAPlugin);
        let mut zip = ZipWriter::new();
        zip.add("something.txt", b"hello");
        assert_eq!(refusal(&zip.finish()), Refusal::NotAPlugin);
        assert_eq!(refusal(&pack_text(b"{ not json", &[])), Refusal::NotAPlugin);
    }

    #[test]
    fn a_path_out_of_the_folder_is_refused() {
        // in the archive, unlisted, and listed: each way round
        for path in
            ["../evil.txt", "a/../../evil.txt", "/etc/evil", "c:/evil", "a\\..\\evil", "a//b", "a/./b", ".hidden"]
        {
            assert_eq!(
                refusal(&pack(&a_filter(), &[("filter.wgsl", SHADER), (path, b"x")])),
                Refusal::Unsafe,
                "{path}"
            );
            let listed = with(a_filter(), "/files/0/path", json!(path));
            assert_eq!(refusal(&pack(&listed, &[(path, SHADER)])), Refusal::Unsafe, "{path}");
        }
        for name in ["con", "NUL.txt", "lpt1", "com9.wgsl", "aux.wgsl"] {
            assert!(!safe_path(name), "{name}");
        }
    }

    /// The first central entry of `bytes` with these attributes.
    fn with_attributes(mut bytes: Vec<u8>, name: &str, attributes: u32) -> Vec<u8> {
        let mut at = 0;
        loop {
            let found = bytes[at..].windows(4).position(|w| w == [0x50, 0x4B, 0x01, 0x02]).expect("no such entry") + at;
            let len = usize::from(u16::from_le_bytes([bytes[found + 28], bytes[found + 29]]));
            if &bytes[found + 46..found + 46 + len] == name.as_bytes() {
                bytes[found + 38..found + 42].copy_from_slice(&attributes.to_le_bytes());
                return bytes;
            }
            at = found + 4;
        }
    }

    #[test]
    fn a_link_or_a_device_is_refused() {
        let file = a_filter_file(&a_filter());
        let link = with_attributes(file.clone(), "filter.wgsl", 0o120_777 << 16);
        assert_eq!(refusal(&link), Refusal::Unsafe);
        let fifo = with_attributes(file.clone(), "filter.wgsl", 0o010_644 << 16);
        assert_eq!(refusal(&fifo), Refusal::Unsafe);
        let folder = with_attributes(file, "filter.wgsl", 0x10);
        assert_eq!(refusal(&folder), Refusal::Unsafe);
    }

    #[test]
    fn a_bomb_is_refused_by_what_it_claims() {
        // two megabytes of nothing, a few kilobytes in the archive
        let zeros = vec![0_u8; 2 << 20];
        let manifest =
            with(with(a_filter(), "/files/0/sha256", json!(sha256(&zeros))), "/files/0/size", json!(zeros.len()));
        assert_eq!(refusal(&pack(&manifest, &[("filter.wgsl", &zeros)])), Refusal::Unsafe);
    }

    #[test]
    fn too_much_is_refused() {
        // a file over the cap, whatever is in it
        assert_eq!(refusal(&vec![0_u8; MOST_FILE + 1]), Refusal::TooBig);
        // too many entries
        let many: Vec<(String, &[u8])> = (0..=MOST_ENTRIES).map(|i| (format!("f{i}.txt"), &b"x"[..])).collect();
        let entries: Vec<(&str, &[u8])> = many.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        assert_eq!(refusal(&pack(&a_filter(), &entries)), Refusal::TooBig);
        // a manifest past its cap, padded with a field nobody reads
        let padded = with(a_filter(), "/description", json!("x"));
        let mut padded = padded.as_object().unwrap().clone();
        padded.insert("padding".to_owned(), json!("x".repeat(MOST_MANIFEST + 1)));
        assert_eq!(refusal(&pack(&Value::Object(padded), &[("filter.wgsl", SHADER)])), Refusal::TooBig);
    }

    #[test]
    fn a_file_that_isn_t_as_listed_is_refused() {
        // a hash that doesn't match
        let wrong = with(a_filter(), "/files/0/sha256", json!(sha256(b"something else")));
        assert_eq!(refusal(&a_filter_file(&wrong)), Refusal::Mismatch);
        // a size that doesn't
        let wrong = with(a_filter(), "/files/0/size", json!(SHADER.len() + 1));
        assert_eq!(refusal(&a_filter_file(&wrong)), Refusal::Mismatch);
        // a file more than the list has, and one less
        let extra = pack(&a_filter(), &[("filter.wgsl", SHADER), ("extra.txt", b"x")]);
        assert_eq!(refusal(&extra), Refusal::Mismatch);
        assert_eq!(refusal(&pack(&a_filter(), &[])), Refusal::Mismatch);
        // the same name twice, and by case
        let twice = pack(&a_filter(), &[("filter.wgsl", SHADER), ("filter.wgsl", SHADER)]);
        assert_eq!(refusal(&twice), Refusal::Unsafe);
        let cased = pack(&a_filter(), &[("filter.wgsl", SHADER), ("FILTER.wgsl", SHADER)]);
        assert_eq!(refusal(&cased), Refusal::Unsafe);
    }

    #[test]
    fn a_header_that_disagrees_with_the_directory_is_refused() {
        let mut bytes = a_filter_file(&a_filter());
        let header = bytes.windows(11).position(|w| w == b"filter.wgsl").unwrap();
        bytes[header] = b'g';
        assert_eq!(refusal(&bytes), Refusal::Unsafe);
    }

    #[test]
    fn a_damaged_file_is_refused() {
        let mut bytes = pack(&a_filter(), &[("filter.wgsl", SHADER)]);
        // the first byte of the shader's data
        let header = bytes.windows(11).position(|w| w == b"filter.wgsl").unwrap();
        bytes[header + 12] ^= 0xFF;
        assert_eq!(refusal(&bytes), Refusal::Damaged);
    }

    #[test]
    fn duplicates_are_refused() {
        // one control twice
        let control = a_filter()["controls"][0].clone();
        let twice = with(a_filter(), "/controls", json!([control, control]));
        assert_eq!(refusal(&a_filter_file(&twice)), Refusal::Described);
        // a field twice: two readers could believe either
        let text =
            a_filter().to_string().replacen("\"permissions\":[]", "\"permissions\":[],\"permissions\":[\"net\"]", 1);
        assert_eq!(refusal(&pack_text(text.as_bytes(), &[("filter.wgsl", SHADER)])), Refusal::Described);
        // a file twice in the list
        let file = a_filter()["files"][0].clone();
        let twice = with(a_filter(), "/files", json!([file, file]));
        assert_eq!(refusal(&a_filter_file(&twice)), Refusal::Described);
    }

    #[test]
    fn text_that_hides_or_reorders_is_refused() {
        for bad in
            ["evil\u{202E}txt.exe", "zero\u{200B}width", "tag\u{E0041}", "line\nbreak", "bell\u{7}", "soft\u{AD}hyphen"]
        {
            for field in ["/name", "/description", "/publisher/name", "/controls/0/label"] {
                assert_eq!(
                    refusal(&a_filter_file(&with(a_filter(), field, json!(bad)))),
                    Refusal::Described,
                    "{field}"
                );
            }
        }
        // what isn't English is fine
        let named = with(a_filter(), "/name", json!("フィルター Überlagern"));
        assert!(read_package(&a_filter_file(&named)).is_ok());
        for long in ["/name", "/publisher/name"] {
            let text = "x".repeat(61);
            assert_eq!(refusal(&a_filter_file(&with(a_filter(), long, json!(text)))), Refusal::Described);
        }
        let empty = with(a_filter(), "/name", json!("   "));
        assert_eq!(refusal(&a_filter_file(&empty)), Refusal::Described);
    }

    fn slider_of(package: &Package) -> PluginSlider {
        match &package.manifest.controls[0] {
            PluginControl::Slider { control } => control.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_slider_names_every_step_or_none() {
        let named = with(a_filter(), "/controls/0/step", json!(0.5));
        let named = with(named, "/controls/0/starts", json!(0));
        let mut named = with(named, "/controls/0/least", json!(0));
        named["controls"][0]["most"] = json!(1);
        named["controls"][0]["names"] = json!(["Off", "Half", "All"]);
        let slider = slider_of(&read_package(&a_filter_file(&named)).unwrap());
        assert_eq!(slider.names, ["Off", "Half", "All"]);
        for names in [
            json!(["Off", "All"]),
            json!(["Off", "", "All"]),
            json!(["Off", "Ha\u{200B}lf", "All"]),
            json!(["Off", "x".repeat(17), "All"]),
        ] {
            let mut wrong = named.clone();
            wrong["controls"][0]["names"] = names.clone();
            assert_eq!(refusal(&a_filter_file(&wrong)), Refusal::Described, "{names}");
        }
    }

    #[test]
    fn a_start_out_of_range_comes_in() {
        let high = with(a_filter(), "/controls/0/starts", json!(40));
        assert_eq!(slider_of(&read_package(&a_filter_file(&high)).unwrap()).starts, 1.0);
        let low = with(a_filter(), "/controls/0/starts", json!(-3));
        let slider = slider_of(&read_package(&a_filter_file(&low)).unwrap());
        assert_eq!((slider.starts, slider.value), (0.0, 0.0));
        let between = with(a_filter(), "/controls/0/starts", json!(0.504));
        assert!((slider_of(&read_package(&a_filter_file(&between)).unwrap()).starts - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_range_that_makes_no_sense_is_refused() {
        for (path, value) in [
            ("/controls/0/least", json!(2)),
            ("/controls/0/step", json!(0)),
            ("/controls/0/step", json!(-1)),
            ("/controls/0/step", json!(0.000_001)),
            ("/controls/0/most", json!(1e30)),
            ("/controls/0/label", json!("")),
            ("/controls/0/key", json!("on")),
            ("/controls/0/key", json!("with space")),
            ("/controls/0/key", json!("Upper")),
            ("/controls/0/key", json!("../up")),
        ] {
            assert_eq!(
                refusal(&a_filter_file(&with(a_filter(), path, value.clone()))),
                Refusal::Described,
                "{path} {value}"
            );
        }
        let unit = with(
            a_filter(),
            "/controls",
            json!([{
                "key": "a", "type": "slider", "label": "A", "least": 0, "most": 1, "step": 0.5, "unit": "\u{202E}px",
            }]),
        );
        assert_eq!(refusal(&a_filter_file(&unit)), Refusal::Described);
        let at_least = with(
            a_filter(),
            "/controls",
            json!([{
                "key": "a", "type": "slider", "label": "A", "least": 0, "most": 1, "step": 0.5, "at_least": "x\u{200B}",
            }]),
        );
        assert_eq!(refusal(&a_filter_file(&at_least)), Refusal::Described);
    }

    #[test]
    fn a_plugin_s_rows_in_the_brush_studio_are_choices_kept_with_each_brush() {
        let module = crate::plugin_run::tests::position_module("", "(i64.const 0)");
        let mut manifest = coded("org.example.rows", &module);
        manifest["brush_controls"] = json!([{ "key": "kind", "label": "Kind", "choices": ["One", "Two", "Three"] }]);
        let package = read_package(&pack(&manifest, &[(MODULE, module.as_slice())])).unwrap();
        assert_eq!(
            package.manifest.brush_controls,
            [PluginBrushControl {
                key: "plugin:org.example.rows:kind".to_owned(),
                label: "Kind".to_owned(),
                choices: vec!["One".to_owned(), "Two".to_owned(), "Three".to_owned()],
            }]
        );
        for wrong in [
            json!([{ "key": "kind", "label": "Kind", "choices": ["Only"] }]),
            json!([{ "key": "Kind", "label": "Kind", "choices": ["One", "Two"] }]),
            json!([{ "key": "kind", "label": "", "choices": ["One", "Two"] }]),
            json!([{ "key": "kind", "label": "Kind", "choices": ["One", "x".repeat(25)] }]),
            json!([{ "key": "a", "label": "A", "choices": ["1", "2"] }, { "key": "a", "label": "A", "choices": ["1", "2"] }]),
        ] {
            let mut odd = manifest.clone();
            odd["brush_controls"] = wrong.clone();
            assert_eq!(refusal(&pack(&odd, &[(MODULE, module.as_slice())])), Refusal::Described, "{wrong}");
        }
        // a filter has no way to read them
        let mut filter = a_filter();
        filter["brush_controls"] = json!([{ "key": "k", "label": "K", "choices": ["1", "2"] }]);
        assert_eq!(refusal(&a_filter_file(&filter)), Refusal::Described);
    }

    #[test]
    fn file_controls_take_sounds_and_only_code_has_them() {
        let sound = json!({ "key": "own", "type": "file", "label": "Own sound", "accepts": "sound" });
        // a filter can't use a file
        let filter = with(a_filter(), "/controls", json!([sound.clone()]));
        assert_eq!(refusal(&a_filter_file(&filter)), Refusal::Described);
        // code can: up to eight, besides eight others
        let module = crate::plugin_run::tests::position_module(
            r#"(func (export "procraft_control") (param i32 f32))"#,
            "(i64.const 0)",
        );
        let mut manifest = coded("org.example.sounds", &module);
        let mut controls: Vec<Value> = (0..MOST_FILES)
            .map(|i| json!({ "key": format!("f{i}"), "type": "file", "label": "A", "accepts": "sound" }))
            .collect();
        controls.extend((0..MOST_CONTROLS).map(|i| json!({ "key": format!("k{i}"), "type": "switch", "label": "A" })));
        manifest["controls"] = json!(controls);
        let package = read_package(&pack(&manifest, &[(MODULE, module.as_slice())])).unwrap();
        let PluginControl::File { control } = &package.manifest.controls[0] else { panic!() };
        assert_eq!(
            (control.key.as_str(), control.accepts, control.chosen.as_deref()),
            ("org.example.sounds.f0", PluginFileKind::Sound, None)
        );
        let mut more = manifest.clone();
        more["controls"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "key": "f9", "type": "file", "label": "A", "accepts": "sound" }));
        assert_eq!(refusal(&pack(&more, &[(MODULE, module.as_slice())])), Refusal::Described);
        // a kind it doesn't take is a newer app's; none said, or taking an old setting, is wrong
        for (accepts, why) in [(json!("video"), Refusal::Newer), (Value::Null, Refusal::Described)] {
            let mut odd = manifest.clone();
            odd["controls"] = json!([{ "key": "f", "type": "file", "label": "A", "accepts": accepts }]);
            assert_eq!(refusal(&pack(&odd, &[(MODULE, module.as_slice())])), why);
        }
    }

    #[test]
    fn more_controls_than_a_page_shows_are_refused() {
        let switches = |n: usize| -> Vec<Value> {
            (0..n).map(|i| json!({ "key": format!("k{i}"), "type": "switch", "label": "A" })).collect()
        };
        let nine = json!(switches(MOST_CONTROLS + 1));
        assert_eq!(refusal(&a_filter_file(&with(a_filter(), "/controls", nine.clone()))), Refusal::Described);
        // code written to interface 6 has room for sixteen, a filter still for eight
        let module = crate::plugin_run::tests::position_module(
            r#"(func (export "procraft_control") (param i32 f32))"#,
            "(i64.const 0)",
        );
        let of = |interface: u32, controls: &Value| {
            let mut manifest = coded("org.example.plugin", &module);
            manifest["controls"] = controls.clone();
            let manifest = with(manifest, "/compat/procraft/interface", json!(interface));
            read_package(&a_coded_file(&manifest, &module)).map(|p| p.manifest.controls.len())
        };
        assert_eq!(of(5, &nine).err(), Some(Refusal::Described));
        assert_eq!(of(6, &nine).ok(), Some(MOST_CONTROLS + 1));
        assert_eq!(of(6, &json!(switches(MOST_CONTROLS_6))).ok(), Some(MOST_CONTROLS_6));
        assert_eq!(of(6, &json!(switches(MOST_CONTROLS_6 + 1))).err(), Some(Refusal::Described));
        let filter = with(with(a_filter(), "/controls", nine), "/compat/procraft/interface", json!(6));
        assert_eq!(refusal(&a_filter_file(&filter)), Refusal::Described);
    }

    #[test]
    fn a_newer_format_is_refused_as_newer() {
        assert_eq!(refusal(&a_filter_file(&with(a_filter(), "/schema", json!(2)))), Refusal::Newer);
        // whatever else has changed in it
        assert_eq!(refusal(&pack_text(br#"{"schema": 9, "everything": "different"}"#, &[])), Refusal::Newer);
        assert_eq!(refusal(&a_filter_file(&with(a_filter(), "/holds", json!(["tool"])))), Refusal::Newer);
        assert_eq!(refusal(&a_filter_file(&with(a_filter(), "/holds", json!(["unlock"])))), Refusal::Newer);
        let dial = with(a_filter(), "/controls/0/type", json!("dial"));
        assert_eq!(refusal(&a_filter_file(&dial)), Refusal::Newer);
        assert_eq!(refusal(&a_filter_file(&with(a_filter(), "/schema", json!(0)))), Refusal::NotAPlugin);
    }

    #[test]
    fn fields_nobody_knows_are_passed_over() {
        let mut manifest = a_filter().as_object().unwrap().clone();
        manifest.insert("something_new".to_owned(), json!({ "deep": [1, 2, 3] }));
        assert!(read_package(&a_filter_file(&Value::Object(manifest))).is_ok());
    }

    #[test]
    fn asking_for_permissions_is_refused() {
        let asks = with(a_filter(), "/permissions", json!(["network"]));
        assert_eq!(refusal(&a_filter_file(&asks)), Refusal::Permissions);
    }

    #[test]
    fn a_plugin_for_another_app_is_said_to_be() {
        let other = with(a_filter(), "/compat", json!({ "photocraft": { "interface": 1 } }));
        assert_eq!(refusal(&a_filter_file(&other)), Refusal::OtherApp);
    }

    #[test]
    fn an_id_is_plain_and_ours_stay_ours() {
        for bad in ["nodots", "UPPER.case", "a..b", ".a.b", "a.b.", "a/b.c", "a b.c", "-a.b", "con.example", "a.b\\c"] {
            assert_eq!(refusal(&a_filter_file(&with(a_filter(), "/id", json!(bad)))), Refusal::Described, "{bad}");
        }
        let ours = with(a_filter(), "/id", json!("procraft.sneaky"));
        assert_eq!(refusal(&a_filter_file(&ours)), Refusal::Described);
    }

    #[test]
    fn a_version_is_three_numbers() {
        for bad in ["1.0", "1.0.0.0", "1.0.0-beta", "v1.0.0", "1..0", "../1.0.0", "99999999999.0.0"] {
            assert_eq!(refusal(&a_filter_file(&with(a_filter(), "/version", json!(bad)))), Refusal::Described, "{bad}");
        }
        assert!(version_is_older("1.2.3", "1.10.0"));
        assert!(version_is_older("0.0.4-beta", "0.0.5"));
        assert!(!version_is_older("2.0.0", "1.9.9"));
        assert!(!version_is_older("1.0.0", "1.0.0"));
    }

    fn a_module() -> Vec<u8> {
        crate::plugin_run::tests::position_module("", "(i64.const 0)")
    }

    fn a_coded_file(manifest: &Value, module: &[u8]) -> Vec<u8> {
        pack(manifest, &[(MODULE, module)])
    }

    /// The id Lasso fill's brush rows are kept by: the plugin it was, before Infinite Plugins took
    /// its place.
    const LASSO_FILL: &str = "io.github.artcraftandroid.lasso-fill";

    /// Lasso fill's brushes, the set Infinite Plugins' `brushes.brushset` holds: the app's round pen
    /// filling what it goes round (its Fill row on, shown as it's drawn), at one size, unsmoothed,
    /// plain or with one of the app's own grains fixed to the canvas (a tile so many canvas pixels
    /// across), some started at a place of their own each stroke, the scratches multiplied.
    pub(crate) fn fill_brushes() -> Vec<procraft_engine::brush::Brush> {
        use procraft_engine::brush::{Brush, Grain, GrainMode};
        use procraft_engine::textures::grain;
        let fill = |name: &str, grain: Option<(u32, f32)>, jitter: bool, multiply: bool| {
            let grain = grain.map_or_else(Grain::default, |(image, scale)| Grain {
                image: Some(image),
                source: None,
                mode: GrainMode::Texturized,
                scale,
                depth: 1.0,
            });
            let mut brush = Brush {
                name: name.to_owned(),
                spacing: 0.05,
                streamline: 0.0,
                hardness: 1.0,
                pressure_size: 0.0,
                grain,
                ..Brush::round_pen()
            };
            brush.set_setting(&format!("plugin:{LASSO_FILL}:fill"), 1.0);
            if jitter {
                brush.set_setting("grain_offset_jitter", 1.0);
            }
            if multiply {
                brush.set_setting("blend_mode", procraft_engine::document::BlendMode::Multiply.id() as f32);
            }
            brush
        };
        vec![
            fill("Solid Fill", None, false, false),
            fill("Screentone Fill", Some((grain::HALFTONE, 138.0)), false, false),
            fill("Debris Fill", Some((grain::SPECKLE, 512.0)), true, false),
            fill("Grunge Fill", Some((grain::GRUNGE, 512.0)), true, false),
            fill("Scratch Fill", Some((grain::SCRATCHES, 256.0)), true, true),
            fill("Checker Fill", Some((grain::CHECKER, 512.0)), false, false),
        ]
    }

    /// [`fill_brushes`] as a set file called Fills, as the app writes one to share.
    pub(crate) fn fill_brushes_file() -> Vec<u8> {
        let s = crate::CanvasSession::new(100, 100);
        let brushes = fill_brushes();
        let count = brushes.len();
        {
            let mut st = s.shared.lock();
            let origins = vec![None; count];
            let icon = crate::brush_library::IMPORTED_ICON;
            let set =
                crate::brush_library::LibrarySet { name: "Fills".to_owned(), brushes, origins, icon, plugin: None };
            st.library.insert(0, set);
        }
        s.brushes_as_set_file(0, 0..count, "Fills", 1).unwrap()
    }

    /// Where Infinite Plugins keeps Lasso fill's brushes.
    fn fill_brushes_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/infinite-plugins").join(BRUSH_SET)
    }

    /// Writes Infinite Plugins' `brushes.brushset` once [`fill_brushes`] changes (then pack the
    /// plugin again): `cargo test -p procraft-core write_the_fill_brushes -- --ignored`.
    #[test]
    #[ignore]
    fn write_the_fill_brushes() {
        std::fs::write(fill_brushes_path(), fill_brushes_file()).unwrap();
    }

    #[test]
    fn the_lasso_fill_plugin_holds_its_fill_brushes_as_written_now_byte_for_byte() {
        let made = fill_brushes_file();
        assert!(made == fill_brushes_file(), "a set file is written the same each time");
        let kept = std::fs::read(fill_brushes_path()).unwrap();
        assert!(kept == made, "plugins/infinite-plugins/{BRUSH_SET} is old: write_the_fill_brushes, then pack it");
        let (name, brushes) = procraft_engine::brushfile::read_set(&kept).unwrap();
        assert_eq!((name.as_str(), brushes.len()), ("Fills", 6));
    }

    #[test]
    fn a_plugin_s_brushes_are_a_set_file_that_reads_from_interface_14() {
        let module = a_module();
        let set = fill_brushes_file();
        let holding = |set: &[u8], interface: u32| {
            let manifest = with(coded("org.example.brushes", &module), "/holds", json!(["hooks", "brushes"]));
            let manifest = with(manifest, "/compat/procraft/interface", json!(interface));
            let files = json!([
                { "path": MODULE, "sha256": sha256(&module), "size": module.len() },
                { "path": BRUSH_SET, "sha256": sha256(set), "size": set.len() },
            ]);
            read_package(&pack(&with(manifest, "/files", files), &[(MODULE, &module), (BRUSH_SET, set)]))
        };
        let package = holding(&set, 14).unwrap();
        assert_eq!(package.manifest.holds, [PluginPart::Hooks, PluginPart::Brushes]);
        assert_eq!(package.file(BRUSH_SET), Some(set.as_slice()));
        // written to an interface before 14, or a set file that doesn't read: refused
        assert_eq!(holding(&set, 13).err(), Some(Refusal::Described));
        assert_eq!(holding(b"not a set of brushes", 14).err(), Some(Refusal::Brushes));
        // said to hold brushes, its set file not there: refused
        let manifest = with(coded("org.example.brushes", &module), "/holds", json!(["hooks", "brushes"]));
        let manifest = with(manifest, "/compat/procraft/interface", json!(14));
        assert_eq!(read_package(&a_coded_file(&manifest, &module)).err(), Some(Refusal::Described));
    }

    #[test]
    fn a_texture_control_offers_its_plugins_pictures_from_interface_15() {
        let module = crate::plugin_run::tests::position_module(
            r#"(func (export "procraft_control") (param i32 f32))"#,
            "(i64.const 0)",
        );
        let pictures = |own: Value, interface: u32, starts: Value, place: Value| {
            let mut manifest = coded("org.example.paper", &module);
            manifest["compat"]["procraft"]["interface"] = json!(interface);
            manifest["controls"] = json!([
                { "key": "texture", "type": "texture", "label": "Texture", "own": own, "starts": starts, "place": place },
                { "key": "first", "type": "file", "label": "A picture", "accepts": "picture" },
                { "key": "second", "type": "file", "label": "Another", "accepts": "picture" },
                { "key": "sound", "type": "file", "label": "A sound", "accepts": "sound" },
            ]);
            read_package(&pack(&manifest, &[(MODULE, module.as_slice())]))
        };
        let card = json!("card");
        let package = pictures(json!(["first", "second"]), 15, json!(3), card.clone()).unwrap();
        let PluginControl::Texture { control } = &package.manifest.controls[0] else { panic!() };
        assert_eq!(control.own, ["org.example.paper.first", "org.example.paper.second"]);
        assert_eq!((control.value, control.starts), (3, 3));
        assert!(pictures(json!([]), 15, json!(0), card.clone()).is_ok(), "the app's grains alone");
        // before interface 15; offering a sound, a picture not there or one twice; starting past the
        // grains or below none; or shown in the Canvas tab: refused
        let most = procraft_engine::textures::GRAIN_COUNT;
        for (own, interface, starts, place) in [
            (json!(["first"]), 14, json!(0), card.clone()),
            (json!(["sound"]), 15, json!(0), card.clone()),
            (json!(["third"]), 15, json!(0), card.clone()),
            (json!(["first", "first"]), 15, json!(0), card.clone()),
            (json!(["first"]), 15, json!(most + 1), card.clone()),
            (json!(["first"]), 15, json!(-1), card.clone()),
            (json!(["first"]), 15, json!("grunge"), card.clone()),
            (json!(["first"]), 15, json!(0), json!("canvas")),
        ] {
            let read = pictures(own.clone(), interface, starts.clone(), place.clone());
            assert_eq!(read.err(), Some(Refusal::Described), "{own} {interface} {starts} {place}");
        }
    }

    #[test]
    fn a_folder_control_comes_with_interface_22_and_may_be_a_whole_plugin_of_features() {
        let module = wat::parse_str(
            r#"(module (func (export "procraft_control") (param i32 f32))
                (func (export "procraft_artwork_saved") (result i32) (i32.const 0)))"#,
        )
        .unwrap();
        let folders = |interface: u32, folder: Value, holds: Value| {
            let mut manifest = coded("org.example.folders", &module);
            manifest["compat"]["procraft"]["interface"] = json!(interface);
            manifest["hooks"] = json!(["artwork_saved"]);
            manifest["holds"] = holds;
            manifest["controls"] = json!([
                folder,
                { "key": "keep", "type": "feature", "label": "Keep", "description": "Keeps.", "hooks": ["artwork_saved"] },
            ]);
            read_package(&pack(&manifest, &[(MODULE, module.as_slice())]))
        };
        let hooks = json!(["hooks"]);
        let folder = json!({ "key": "folder", "type": "folder", "label": "Folder" });
        let package = folders(22, folder.clone(), hooks.clone()).unwrap();
        let PluginControl::Folder { control } = &package.manifest.controls[0] else { panic!() };
        assert_eq!(
            (control.key.as_str(), control.chosen.as_ref(), control.location.as_ref()),
            ("org.example.folders.folder", None, None)
        );
        // the whole plugin's, under no feature; or one feature's
        assert_eq!(package.manifest.shape.feature_of(0), None);
        let under = json!({ "key": "folder", "type": "folder", "label": "Folder", "under": "keep" });
        assert!(folders(22, under, hooks.clone()).is_ok());
        // before interface 22; elsewhere than the card, kept with each artwork, starting somewhere,
        // keeping a setting the app had, taking files; or with no code: refused
        let refused = [
            (21, folder.clone(), hooks.clone()),
            (22, json!({ "key": "folder", "type": "folder", "label": "Folder", "place": "canvas" }), hooks.clone()),
            (22, json!({ "key": "folder", "type": "folder", "label": "Folder", "artwork": true }), hooks.clone()),
            (22, json!({ "key": "folder", "type": "folder", "label": "Folder", "starts": "/sdcard" }), hooks.clone()),
            (
                22,
                json!({ "key": "folder", "type": "folder", "label": "Folder", "keeps": "interface_size" }),
                hooks.clone(),
            ),
            (22, json!({ "key": "folder", "type": "folder", "label": "Folder", "accepts": "picture" }), hooks.clone()),
            (22, json!({ "key": "on_top", "type": "switch", "label": "A switch" }), hooks.clone()),
        ];
        for (interface, folder, holds) in refused {
            assert_eq!(
                folders(interface, folder.clone(), holds).err(),
                Some(Refusal::Described),
                "{interface} {folder}"
            );
        }
        // a hook this app has, written to a later interface than it reads: for a newer app
        assert_eq!(folders(23, folder, hooks).err(), Some(Refusal::Newer));
    }

    #[test]
    fn a_good_plugin_is_read() {
        let module = a_module();
        let manifest = coded("org.example.plugin", &module);
        let package = read_package(&a_coded_file(&manifest, &module)).unwrap();
        assert_eq!(package.manifest.holds, [PluginPart::Hooks]);
        assert_eq!(package.manifest.hooks, [Hook::Position]);
    }

    #[test]
    fn a_plugin_says_what_it_steps_into_and_what_it_is_written_to() {
        let module = a_module();
        let of = |path: &str, value: Value| {
            refusal(&a_coded_file(&with(coded("org.example.plugin", &module), path, value), &module))
        };
        // a hook or an interface this app doesn't have is for a newer one
        assert_eq!(of("/hooks", json!(["position", "layers"])), Refusal::Newer);
        assert_eq!(of("/compat/procraft/interface", json!(INTERFACE + 1)), Refusal::Newer);
        assert_eq!(of("/hooks", json!([])), Refusal::Described);
        assert_eq!(of("/hooks", json!(["position", "position"])), Refusal::Described);
        assert_eq!(of("/compat/procraft", json!({ "min_app": "0.0.1" })), Refusal::Described);
        // nor ask for what this app doesn't give
        assert_eq!(of("/permissions", json!(["files"])), Refusal::Permissions);
        // and a part that isn't one yet is for a newer one too
        assert_eq!(of("/holds", json!(["module"])), Refusal::Newer);
    }

    #[test]
    fn plugins_written_to_every_interface_so_far_still_load_and_a_new_hook_needs_its_function() {
        let module = a_module();
        for interface in 1..=18 {
            let manifest = with(coded("org.example.plugin", &module), "/compat/procraft/interface", json!(interface));
            assert!(read_package(&a_coded_file(&manifest, &module)).is_ok(), "interface {interface}");
        }
        let fill = |module: &[u8]| {
            let manifest = with(coded("org.example.fill", module), "/hooks", json!(["stroke_fill"]));
            read_package(&a_coded_file(&with(manifest, "/compat/procraft/interface", json!(12)), module))
        };
        let fills = r#"(module (func (export "procraft_stroke_fill") (param i32 i32) (result i32) (i32.const 1)))"#;
        let package = fill(&wat::parse_str(fills).unwrap()).unwrap();
        assert_eq!(package.manifest.hooks, [Hook::StrokeFill]);
        assert!(fill(&module).is_err(), "a module without the hook's function");
        let dim = |module: &[u8]| {
            let manifest = with(coded("org.example.dim", module), "/hooks", json!(["view_dim_others"]));
            read_package(&a_coded_file(&with(manifest, "/compat/procraft/interface", json!(13)), module))
        };
        let dims = r#"(module (func (export "procraft_view_dim_others") (result f32) (f32.const 0.7)))"#;
        assert_eq!(dim(&wat::parse_str(dims).unwrap()).unwrap().manifest.hooks, [Hook::ViewDimOthers]);
        assert!(dim(&module).is_err(), "a module without the dimming hook's function");
    }

    #[test]
    fn a_plugin_lists_what_it_does_when_tapped_and_what_that_reaches() {
        let module = wat::parse_str(crate::plugin_run::tests::ACTS).unwrap();
        let acting = || {
            let mut manifest = coded("org.example.plugin", &module);
            manifest["compat"]["procraft"]["interface"] = json!(2);
            manifest["hooks"] = json!([]);
            manifest["permissions"] = json!(["artwork.write"]);
            manifest["actions"] = json!([{ "key": "grid", "label": "Add a grid layer" }]);
            manifest
        };
        let package = read_package(&a_coded_file(&acting(), &module)).unwrap();
        let grid = crate::plugins::PluginAction {
            key: "org.example.plugin.grid".to_owned(),
            label: "Add a grid layer".to_owned(),
            place: crate::plugins::PluginPlace::Card,
        };
        assert_eq!(package.manifest.actions, [grid]);
        assert_eq!(package.manifest.permissions, [crate::plugin_commands::PluginPermission::ArtworkWrite]);
        let of = |path: &str, value: Value| refusal(&a_coded_file(&with(acting(), path, value), &module));
        // what it imports needs its permission
        assert_eq!(of("/permissions", json!([])), Refusal::Code);
        // one this app doesn't give is refused, one asked twice isn't written right
        assert_eq!(of("/permissions", json!(["artwork.write", "network"])), Refusal::Permissions);
        assert_eq!(of("/permissions", json!(["artwork.write", "artwork.write"])), Refusal::Described);
        // with no hooks and no actions it does nothing
        assert_eq!(of("/actions", json!([])), Refusal::Described);
        // each key once, words to show, and not too many
        let twice = json!([{ "key": "a", "label": "A" }, { "key": "a", "label": "B" }]);
        assert_eq!(of("/actions", twice), Refusal::Described);
        assert_eq!(of("/actions", json!([{ "key": "a", "label": " " }])), Refusal::Described);
        let many: Vec<Value> = (0..9).map(|i| json!({ "key": format!("a{i}"), "label": "A" })).collect();
        assert_eq!(of("/actions", json!(many)), Refusal::Described);
        // listed in the Adjustments menu too; a place this app doesn't have is for a newer one
        let placed = |place: &str| {
            let mut manifest = acting();
            manifest["actions"][0]["place"] = json!(place);
            read_package(&a_coded_file(&manifest, &module))
        };
        let package = placed("adjustments").unwrap();
        assert_eq!(package.manifest.actions[0].place, crate::plugins::PluginPlace::Adjustments);
        for (name, place) in [
            ("layer", crate::plugins::PluginPlace::Layer),
            ("frame", crate::plugins::PluginPlace::Frame),
            ("canvas", crate::plugins::PluginPlace::Canvas),
            ("pen", crate::plugins::PluginPlace::Pen),
            ("layers", crate::plugins::PluginPlace::Layers),
        ] {
            assert_eq!(placed(name).unwrap().manifest.actions[0].place, place);
        }
        assert_eq!(placed("toolbar").err(), Some(Refusal::Newer));
        // a filter neither acts nor reaches anything, nor code written before there were actions
        let filter = with(a_filter(), "/permissions", json!(["brush"]));
        assert_eq!(refusal(&a_filter_file(&filter)), Refusal::Described);
        assert_eq!(of("/compat/procraft/interface", json!(1)), Refusal::Described);
    }

    #[test]
    fn controls_and_actions_may_show_in_the_canvas_tab_take_presses_and_be_kept_with_artworks() {
        // told its controls, and an action
        let told = r#"(func (export "procraft_control") (param i32 f32))
            (func (export "procraft_action") (param i32) (result i32) (i32.const 0))"#;
        let module = crate::plugin_run::tests::position_module(told, "(i64.const 0)");
        let base = || {
            let mut manifest = coded("org.example.placed", &module);
            manifest["compat"]["procraft"]["interface"] = json!(8);
            manifest["controls"] = json!([
                { "key": "shown", "type": "switch", "label": "Shown", "place": "canvas", "press": "alt+g", "artwork": true },
                { "key": "amount", "type": "slider", "label": "Amount", "least": 0, "most": 1, "step": 0.5, "place": "card" },
            ]);
            manifest["actions"] =
                json!([{ "key": "go", "label": "Go", "place": "canvas", "press": "shift+command+r" }]);
            manifest
        };
        let package = read_package(&a_coded_file(&base(), &module)).unwrap();
        let shape = &package.manifest.shape;
        use crate::plugins::PluginPlace::{Canvas, Card};
        assert_eq!(shape.places, [Canvas, Card]);
        let press = |text: &str| crate::KeyCombo::read(text);
        assert_eq!(shape.presses, [press("alt+g"), None]);
        assert_eq!(shape.action_presses, [press("command+shift+r")]);
        assert_eq!(shape.artwork, [true, false]);
        assert_eq!(package.manifest.actions[0].place, Canvas);
        let of = |at: [&str; 3], value: Value| {
            let mut manifest = base();
            let [list, index, field] = at;
            manifest[list][index.parse::<usize>().unwrap()][field] = value;
            refusal(&a_coded_file(&manifest, &module))
        };
        // a control shows on its card or in the Canvas tab; a place this app doesn't have is for a
        // newer one
        assert_eq!(of(["controls", "1", "place"], json!("layer")), Refusal::Described);
        assert_eq!(of(["controls", "1", "place"], json!("toolbar")), Refusal::Newer);
        // a press is a switch's or an action's, one a keyboard sends, each once
        assert_eq!(of(["controls", "1", "press"], json!("alt+h")), Refusal::Described);
        assert_eq!(of(["actions", "0", "press"], json!("alt+g")), Refusal::Described);
        assert_eq!(of(["actions", "0", "press"], json!("view:fit")), Refusal::Described);
        assert_eq!(of(["actions", "0", "press"], json!("hyper+x")), Refusal::Described);
        // kept with an artwork: not a setting the app used to have
        assert_eq!(of(["controls", "0", "keeps"], json!("interface_size")), Refusal::Described);
    }

    /// A picker plugin's file: its module and its picture's code (`picture`, if any).
    pub(crate) fn picker_file(id: &str, picture: Option<&str>) -> Vec<u8> {
        let module = wat::parse_str(crate::plugin_run::tests::PICKS).unwrap();
        let mut manifest = coded(id, &module);
        manifest["hooks"] = json!(["picker"]);
        let mut files: Vec<(&str, &[u8])> = vec![(MODULE, &module)];
        if let Some(text) = picture {
            let listed = json!({ "path": PICTURE_CODE, "sha256": sha256(text.as_bytes()), "size": text.len() });
            manifest["files"].as_array_mut().unwrap().push(listed);
            files.push((PICTURE_CODE, text.as_bytes()));
        }
        pack(&manifest, &files)
    }

    #[test]
    fn a_picker_brings_its_picture_and_only_a_picker_has_one() {
        let picture = crate::plugin_run::tests::PICTURE;
        let package = read_package(&picker_file("org.example.picker", Some(picture))).unwrap();
        assert_eq!(package.manifest.hooks, [Hook::Picker]);
        // without its picture it isn't written right, nor with code that isn't a picture
        assert_eq!(refusal(&picker_file("org.example.picker", None)), Refusal::Described);
        let reaches = "fn picture(at: vec2<f32>) -> vec4<f32> { return layer_at(at); }";
        assert_eq!(refusal(&picker_file("org.example.picker", Some(reaches))), Refusal::Code);
        // and a plugin on another hook can't bring one
        let module = a_module();
        let mut manifest = coded("org.example.plugin", &module);
        let listed = json!({ "path": PICTURE_CODE, "sha256": sha256(picture.as_bytes()), "size": picture.len() });
        manifest["files"].as_array_mut().unwrap().push(listed);
        let file = pack(&manifest, &[(MODULE, &module), (PICTURE_CODE, picture.as_bytes())]);
        assert_eq!(refusal(&file), Refusal::Described);
    }

    /// A plugin that makes dots (`DOTS`), its kinds as `kinds` says.
    pub(crate) fn dots_file(id: &str, kinds: Value) -> Vec<u8> {
        artwork_file(id, kinds, crate::plugin_run::tests::DOTS)
    }

    /// A plugin that makes artworks of marks (`MARKS`), its kinds as `kinds` says.
    pub(crate) fn marks_file(id: &str, kinds: Value) -> Vec<u8> {
        artwork_file(id, kinds, crate::plugin_run::tests::MARKS)
    }

    /// A plugin making artworks of its own, of kinds `kinds`, with the code `text` (WAT).
    fn artwork_file(id: &str, kinds: Value, text: &str) -> Vec<u8> {
        let module = wat::parse_str(text).unwrap();
        let mut manifest = coded(id, &module);
        manifest["compat"]["procraft"]["interface"] = json!(2);
        manifest["hooks"] = json!([]);
        manifest["kinds"] = kinds;
        pack(&manifest, &[(MODULE, &module)])
    }

    /// A brush engine's calls that answer nothing, and an action with a panel to be its studio.
    const ENGINE: &str = r#"(module
        (import "procraft" "show_panel" (func (param i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "procraft_brush_in") (param i32) (result i32) (i32.const 1024))
        (func (export "procraft_brush_out") (result i32) (i32.const 1024))
        (func (export "procraft_brush_import") (param i32 i32) (result i32) (i32.const 0))
        (func (export "procraft_brush_load") (param i32 i32) (result i32) (i32.const -1))
        (func (export "procraft_brush_begin") (param i32) (result i32) (i32.const -1))
        (func (export "procraft_brush_add") (param i32 i32) (result i32) (i32.const 0))
        (func (export "procraft_brush_end") (param i32) (result i32) (i32.const 0))
        (func (export "procraft_action") (param i32) (result i32) (i32.const 0))
        (func (export "procraft_panel_event") (param i32 f32) (result i32) (i32.const 0)))"#;

    #[test]
    fn a_plugin_written_to_interface_4_lists_its_brush_engines() {
        let module = wat::parse_str(ENGINE).unwrap();
        let mut manifest = coded("org.example.pens", &module);
        manifest["compat"]["procraft"]["interface"] = json!(4);
        manifest["hooks"] = json!([]);
        manifest["permissions"] = json!(["panel"]);
        manifest["actions"] = json!([{ "key": "studio", "label": "Pen studio" }]);
        manifest["brush_engines"] =
            json!([{ "key": "pens", "label": "Pens", "accepts": ["pens", "pen2"], "studio": "studio" }]);
        let package = read_package(&a_coded_file(&manifest, &module)).unwrap();
        let engine = crate::plugin_brushes::BrushEngine {
            key: "org.example.pens.pens".to_owned(),
            label: "Pens".to_owned(),
            accepts: vec!["pens".to_owned(), "pen2".to_owned()],
            studio: Some("org.example.pens.studio".to_owned()),
        };
        assert_eq!(package.manifest.brush_engines, [engine]);
        assert_eq!(package.manifest.hooks, [Hook::Brushes], "on the brushes hook, not named");
        let of = |path: &str, value: Value| refusal(&a_coded_file(&with(manifest.clone(), path, value), &module));
        // an older interface has none; endings plain and few, each once; a studio one of its actions
        assert_eq!(of("/compat/procraft/interface", json!(3)), Refusal::Described);
        let engine = |accepts: Value, studio: Value| json!([{ "key": "pens", "label": "Pens", "accepts": accepts, "studio": studio }]);
        assert_eq!(of("/brush_engines", engine(json!([]), json!(null))), Refusal::Described);
        assert_eq!(of("/brush_engines", engine(json!(["Pens"]), json!(null))), Refusal::Described);
        assert_eq!(of("/brush_engines", engine(json!(["a.b"]), json!(null))), Refusal::Described);
        assert_eq!(of("/brush_engines", engine(json!(["pens", "pens"]), json!(null))), Refusal::Described);
        assert_eq!(of("/brush_engines", engine(json!(["pens"]), json!("other"))), Refusal::Described);
        assert!(
            read_package(&a_coded_file(
                &with(manifest.clone(), "/brush_engines", engine(json!(["pens"]), json!(null))),
                &module
            ))
            .is_ok()
        );
        // a studio shows a panel
        assert_eq!(of("/permissions", json!([])), Refusal::Described);
        // the brush data of its own brushes is a permission it may ask for
        assert!(
            read_package(&a_coded_file(
                &with(manifest.clone(), "/permissions", json!(["panel", "brush.data"])),
                &module
            ))
            .is_ok()
        );
        // and code that has no engine can't say it has
        let plain = a_module();
        let mut lacking = coded("org.example.plain", &plain);
        lacking["compat"]["procraft"]["interface"] = json!(4);
        lacking["brush_engines"] = json!([{ "key": "pens", "label": "Pens", "accepts": ["pens"] }]);
        assert_eq!(refusal(&a_coded_file(&lacking, &plain)), Refusal::Code);
    }

    #[test]
    fn a_plugin_lists_the_kinds_of_artwork_it_makes() {
        let dots = json!([{ "key": "dots", "label": "Dots" }]);
        let package = read_package(&dots_file("org.example.dots", dots)).unwrap();
        let kind = crate::plugin_canvas::PluginKind {
            key: "org.example.dots.dots".to_owned(),
            label: "Dots".to_owned(),
            badge: None,
            exports: Vec::new(),
        };
        assert_eq!(package.manifest.kinds, [kind]);
        // each key once, words to show, not too many
        let twice = json!([{ "key": "a", "label": "A" }, { "key": "a", "label": "B" }]);
        assert_eq!(refusal(&dots_file("org.example.dots", twice)), Refusal::Described);
        assert_eq!(refusal(&dots_file("org.example.dots", json!([{ "key": "a", "label": "" }]))), Refusal::Described);
        let many: Vec<Value> = (0..5).map(|i| json!({ "key": format!("k{i}"), "label": "K" })).collect();
        assert_eq!(refusal(&dots_file("org.example.dots", json!(many))), Refusal::Described);
        // and code that makes none can't say it does
        let module = a_module();
        let mut manifest = coded("org.example.plugin", &module);
        manifest["compat"]["procraft"]["interface"] = json!(2);
        manifest["kinds"] = json!([{ "key": "dots", "label": "Dots" }]);
        assert_eq!(refusal(&a_coded_file(&manifest, &module)), Refusal::Code);
    }

    #[test]
    fn a_plugin_written_to_interface_3_lists_the_kinds_of_layer_it_adds() {
        let (_, file) = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "vector-layers").unwrap();
        let package = read_package(file).unwrap();
        let id = "io.github.artcraftandroid.vector-layers";
        // its badge a curve between two nodes
        let curve = crate::BadgeLine { points: vec![0.22, 0.72, 0.38, 0.05, 0.62, 0.95, 0.78, 0.28], curved: true };
        let badge = crate::PluginBadge { lines: vec![curve], dots: vec![0.22, 0.72, 0.78, 0.28] };
        // and an SVG picture of its layers, made by it
        let svg = crate::PluginExport {
            key: format!("{id}.vector.svg"),
            label: "SVG (vector layers)".to_owned(),
            extension: "svg".to_owned(),
            mime: "image/svg+xml".to_owned(),
        };
        let kind = crate::plugin_canvas::PluginKind {
            key: format!("{id}.vector"),
            label: "Vector layer".to_owned(),
            badge: Some(badge),
            exports: vec![svg],
        };
        assert_eq!(package.manifest.layer_kinds, [kind]);
        assert_eq!(package.manifest.hooks, [Hook::Layers], "on the layers hook, not named");
        let module = package.file(MODULE).unwrap().to_vec();
        let manifest: Value = serde_json::from_slice(package.manifest_text()).unwrap();
        let of = |path: &str, value: Value| refusal(&a_coded_file(&with(manifest.clone(), path, value), &module));
        // an older interface has no kinds of layer; each key once, words to show, not too many
        assert_eq!(of("/compat/procraft/interface", json!(2)), Refusal::Described);
        let twice = json!([{ "key": "a", "label": "A" }, { "key": "a", "label": "B" }]);
        assert_eq!(of("/layer_kinds", twice), Refusal::Described);
        assert_eq!(of("/layer_kinds", json!([{ "key": "a", "label": " " }])), Refusal::Described);
        let many: Vec<Value> = (0..5).map(|i| json!({ "key": format!("k{i}"), "label": "K" })).collect();
        assert_eq!(of("/layer_kinds", json!(many)), Refusal::Described);
        // a badge of whole points in its box, curves of a start and three each, or none at all
        let badged = |badge: Value| json!([{ "key": "a", "label": "A", "badge": badge }]);
        assert_eq!(of("/layer_kinds", badged(json!({ "lines": [{ "points": [0, 0, 2, 1] }] }))), Refusal::Described);
        let bent = json!({ "lines": [{ "points": [0, 0, 1, 1, 0, 1], "curved": true }] });
        assert_eq!(of("/layer_kinds", badged(bent)), Refusal::Described);
        assert_eq!(of("/layer_kinds", badged(json!({ "dots": [0.5] }))), Refusal::Described);
        assert_eq!(of("/layer_kinds", badged(json!({}))), Refusal::Described);
        let plain = json!([{ "key": "a", "label": "A" }]);
        let unbadged = read_package(&a_coded_file(&with(manifest.clone(), "/layer_kinds", plain), &module)).unwrap();
        assert_eq!(unbadged.manifest.layer_kinds[0].badge, None);
        // the Layers panel's + shows a kind's badge beside it, or a plug when it draws none
        let listed = package.manifest.layer_kinds[0].clone();
        assert_eq!(crate::plugin_kind_badge(listed.clone()), listed.badge.unwrap());
        assert_eq!(crate::plugin_kind_badge(unbadged.manifest.layer_kinds[0].clone()), crate::PluginBadge::plug());
        // exports with a key, words, a short plain extension and a media type
        let exported = |e: Value| json!([{ "key": "a", "label": "A", "exports": [e] }]);
        let good = json!({ "key": "svg", "label": "SVG", "extension": "svg", "mime": "image/svg+xml" });
        assert!(read_package(&a_coded_file(&with(manifest.clone(), "/layer_kinds", exported(good)), &module)).is_ok());
        let dotted = json!({ "key": "svg", "label": "SVG", "extension": ".svg", "mime": "image/svg+xml" });
        assert_eq!(of("/layer_kinds", exported(dotted)), Refusal::Described);
        let bare = json!({ "key": "svg", "label": "SVG", "extension": "svg", "mime": "svg" });
        assert_eq!(of("/layer_kinds", exported(bare)), Refusal::Described);
        // the layers hook can't be named, and code without its calls can't say it adds any
        assert_eq!(of("/hooks", json!(["layers"])), Refusal::Newer);
        let other = a_module();
        let mut lacking = coded("org.example.plugin", &other);
        lacking["compat"]["procraft"]["interface"] = json!(3);
        lacking["layer_kinds"] = json!([{ "key": "vector", "label": "Vector layer" }]);
        assert_eq!(refusal(&a_coded_file(&lacking, &other)), Refusal::Code);
        // and the import screen says what it lets the plugin do
        assert!(crate::Plugins::new().inspect(file.to_vec()).message.contains("kinds of layer"));
    }

    #[test]
    fn a_control_takes_over_a_moved_setting_and_nothing_else() {
        let module = a_module();
        let with_keeps = |keeps: &str| {
            let mut manifest = coded("org.example.plugin", &module);
            manifest["controls"] = json!([{ "key": "flag", "type": "switch", "label": "On", "keeps": keeps }]);
            a_coded_file(&manifest, &module)
        };
        // (it needs its controls told; here only what it may be kept under is looked at)
        assert_eq!(refusal(&with_keeps("light_interface")), Refusal::Described);
        assert_eq!(refusal(&with_keeps("org.example.other.value")), Refusal::Described);
        assert_eq!(refusal(&with_keeps("eyedropper_layer")), Refusal::Code);
    }

    #[test]
    fn an_id_can_t_end_as_the_note_of_a_stop_does() {
        // or its folder would be where another plugin's stop is written down
        let refused = with(a_filter(), "/id", json!("org.example.stopped"));
        assert_eq!(refusal(&a_filter_file(&refused)), Refusal::Described);
        // the word is fine anywhere else
        assert!(read_package(&a_filter_file(&with(a_filter(), "/id", json!("org.stopped.example")))).is_ok());
        assert!(read_package(&a_filter_file(&with(a_filter(), "/id", json!("org.example.unstopped")))).is_ok());
        // and a folder of that name, however it came to be there, is no plugin and leaves the note be
        let dir = folder("stopped-id");
        let package = read_package(&a_filter_file(&a_filter())).unwrap();
        keep(&dir, &package).unwrap();
        std::fs::rename(dir.join("org.example.filter"), dir.join("org.example.other.stopped")).unwrap();
        assert!(read_all(&dir).is_empty());
        note_stopped(&dir, "org.example.filter", "why");
        assert_eq!(stopped_why(&dir, "org.example.filter").as_deref(), Some("why"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A manifest holding a filter and `module`, each listed.
    fn both(module: &[u8]) -> Value {
        let mut manifest = a_filter();
        manifest["holds"] = json!(["filter", "hooks"]);
        manifest["hooks"] = json!(["position"]);
        manifest["controls"] = json!([]);
        manifest["files"] = json!([
            { "path": FILTER_CODE, "sha256": sha256(SHADER), "size": SHADER.len() },
            { "path": MODULE, "sha256": sha256(module), "size": module.len() },
        ]);
        manifest
    }

    #[test]
    fn a_file_says_what_it_holds_and_lists_just_that() {
        let module = a_module();
        let both_file = |manifest: &Value| pack(manifest, &[(FILTER_CODE, SHADER), (MODULE, &module)]);
        let package = read_package(&both_file(&both(&module))).unwrap();
        assert_eq!(package.manifest.holds, [PluginPart::Filter, PluginPart::Hooks]);
        assert_eq!(package.manifest.hooks, [Hook::Position]);
        // nothing, the same twice, or left out
        assert_eq!(refusal(&both_file(&with(both(&module), "/holds", json!([])))), Refusal::Described);
        let twice = json!(["filter", "hooks", "filter"]);
        assert_eq!(refusal(&both_file(&with(both(&module), "/holds", twice))), Refusal::Described);
        let mut none = both(&module);
        none.as_object_mut().unwrap().remove("holds");
        assert_eq!(refusal(&both_file(&none)), Refusal::Described);
        // code in the file it doesn't say it holds, and the other way round
        let hidden = with(both(&module), "/holds", json!(["filter"]));
        assert_eq!(refusal(&both_file(&hidden)), Refusal::Described);
        let hidden = with(both(&module), "/holds", json!(["hooks"]));
        assert_eq!(refusal(&both_file(&hidden)), Refusal::Described);
        let mut lacking = both(&module);
        lacking["files"].as_array_mut().unwrap().remove(1);
        assert_eq!(refusal(&pack(&lacking, &[(FILTER_CODE, SHADER)])), Refusal::Described);
        // hooks named by what holds none
        assert_eq!(refusal(&a_filter_file(&with_hooks(a_filter()))), Refusal::Described);
        // each part is looked at for what it is
        let not_code = b"nothing a machine would run".as_slice();
        assert_eq!(refusal(&pack(&both(not_code), &[(FILTER_CODE, SHADER), (MODULE, not_code)])), Refusal::Code);
        let mut broken = both(&module);
        let bad = b"fn pixel() {}".as_slice();
        broken["files"][0] = json!({ "path": FILTER_CODE, "sha256": sha256(bad), "size": bad.len() });
        assert_eq!(refusal(&pack(&broken, &[(FILTER_CODE, bad), (MODULE, &module)])), Refusal::FilterCode);
    }

    fn with_hooks(mut manifest: Value) -> Value {
        manifest["hooks"] = json!(["position"]);
        manifest
    }

    #[test]
    fn a_control_can_t_be_called_what_a_flag_is_kept_under() {
        let module = a_module();
        let mut manifest = coded("org.example.plugin", &module);
        manifest["controls"] = json!([{ "key": "on", "type": "switch", "label": "On" }]);
        assert_eq!(refusal(&a_coded_file(&manifest, &module)), Refusal::Described);
    }

    #[test]
    fn a_plugin_needs_its_module_and_a_module_that_can_run() {
        let module = a_module();
        // listed under another name, or too big by its list
        let mut named = coded("org.example.plugin", &module);
        named["files"][0]["path"] = json!("other.wasm");
        assert_eq!(refusal(&pack(&named, &[("other.wasm", &module)])), Refusal::Described);
        let big = with(coded("org.example.plugin", &module), "/files/0/size", json!(MOST_MODULE + 1));
        assert_eq!(refusal(&a_coded_file(&big, &module)), Refusal::Described);
        // not code, code that wants things, code with no controls to be told
        let not_code = b"nothing a machine would run".as_slice();
        let manifest = coded("org.example.plugin", not_code);
        assert_eq!(refusal(&a_coded_file(&manifest, not_code)), Refusal::Code);
        let wants = crate::plugin_run::tests::position_module(r#"(import "env" "open" (func))"#, "(i64.const 0)");
        let manifest = coded("org.example.plugin", &wants);
        assert_eq!(refusal(&a_coded_file(&manifest, &wants)), Refusal::Code);
        let mut manifest = coded("org.example.plugin", &module);
        manifest["controls"] = json!([{ "key": "flag", "type": "switch", "label": "On" }]);
        assert_eq!(refusal(&a_coded_file(&manifest, &module)), Refusal::Code);
    }

    #[test]
    fn a_kept_plugin_with_its_module_changed_counts_as_absent() {
        let dir = folder("plugin");
        let module = a_module();
        let package = read_package(&a_coded_file(&coded("org.example.plugin", &module), &module)).unwrap();
        keep(&dir, &package).unwrap();
        assert_eq!(read_all(&dir).len(), 1);
        assert_eq!(read_code(&dir, "org.example.plugin", "1.0.0"), Some((vec![Hook::Position], module)));
        std::fs::write(dir.join("org.example.plugin/1.0.0").join(MODULE), b"something else").unwrap();
        assert!(read_all(&dir).is_empty());
        assert_eq!(read_code(&dir, "org.example.plugin", "1.0.0"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A kept plugin's folder that held up isn't read and hashed whole again while its files are as
    /// they were: read again for each of its parts (as the apps take the plugins up again on every
    /// change of a setting), it comes back quickly and the same. A file changed since, even to one
    /// of the same size, has it checked whole again, and refused. Prints both times.
    #[test]
    fn a_plugin_folder_that_held_up_is_read_again_quickly_and_a_change_is_still_caught() {
        use std::time::{Duration, Instant};
        let dir = folder("held-up");
        let file = crate::bundled::BUNDLED.iter().find(|(name, _)| *name == "infinite-plugins").unwrap().1;
        let package = read_package(file).unwrap();
        let (id, version) = (package.manifest.id.clone(), package.manifest.version.clone());
        keep(&dir, &package).unwrap();
        let at = dir.join(&id).join(&version);
        let t = Instant::now();
        let first = read_folder(&at, &id, &version).expect("it holds up");
        let checked = t.elapsed();
        let t = Instant::now();
        for _ in 0..10 {
            assert_eq!(read_folder(&at, &id, &version).as_ref(), Some(&first));
        }
        let again = t.elapsed() / 10;
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        println!(
            "a plugin's folder checked whole in {:.1} ms; read again as it was in {:.2} ms",
            ms(checked),
            ms(again)
        );
        assert!(again * 5 < checked, "read again in {:.2} ms against {:.1} ms", ms(again), ms(checked));
        // its module changed to another of the same size: no longer held up
        let module = at.join(MODULE);
        let mut bytes = std::fs::read(&module).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&module, &bytes).unwrap();
        assert_eq!(read_folder(&at, &id, &version), None);
        assert_eq!(read_code(&dir, &id, &version), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_filter_needs_its_shader() {
        let wrong = with(a_filter(), "/files/0/path", json!("filter.txt"));
        assert_eq!(refusal(&pack(&wrong, &[("filter.txt", SHADER)])), Refusal::Described);
    }

    /// Two filters, each under a switch of its own and reading a slider of its own.
    pub(crate) const TINT: &str = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> { return vec4<f32>(colour.rgb * v.amount, colour.a); }";
    pub(crate) const FADE: &str =
        "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> { return vec4<f32>(colour.rgb, colour.a * v.fade); }";

    /// A manifest listing [`TINT`] and [`FADE`] as filters, each under its switch, and its file.
    pub(crate) fn two_filters(id: &str) -> (Value, Vec<u8>) {
        let listed =
            |path: &str, code: &str| json!({ "path": path, "sha256": sha256(code.as_bytes()), "size": code.len() });
        let mut manifest = filter(id);
        manifest["compat"]["procraft"]["interface"] = json!(5);
        manifest["files"] = json!([listed("filters/tint.wgsl", TINT), listed("filters/fade.wgsl", FADE)]);
        manifest["controls"] = json!([
            { "key": "tint", "type": "switch", "label": "Tint" },
            { "key": "amount", "type": "slider", "label": "Amount", "least": 0, "most": 1, "step": 0.1, "starts": 0.5, "under": "tint" },
            { "key": "fade_on", "type": "switch", "label": "Fade" },
            { "key": "fade", "type": "slider", "label": "Fade", "least": 0, "most": 1, "step": 0.1, "starts": 1, "under": "fade_on" },
        ]);
        manifest["filters"] = json!([
            { "key": "tint", "label": "Tint", "controls": ["amount"], "under": "tint" },
            { "key": "fade", "label": "Fade", "controls": ["fade"], "under": "fade_on" },
        ]);
        let file = pack(&manifest, &[("filters/tint.wgsl", TINT.as_bytes()), ("filters/fade.wgsl", FADE.as_bytes())]);
        (manifest, file)
    }

    #[test]
    fn a_plugin_can_list_several_filters_each_with_its_code_and_controls() {
        let (manifest, file) = two_filters("org.example.filters");
        let package = read_package(&file).unwrap();
        let shape = &package.manifest.shape;
        assert_eq!(shape.under, [None, Some(0), None, Some(2)]);
        let ids: Vec<&str> = shape.filters.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["org.example.filters.tint", "org.example.filters.fade"]);
        assert_eq!(
            (shape.filters[1].code.as_str(), &shape.filters[1].controls, shape.filters[1].under),
            ("filters/fade.wgsl", &vec![3], Some(2))
        );
        let of = |path: &str, value: Value| {
            let files = [("filters/tint.wgsl", TINT.as_bytes()), ("filters/fade.wgsl", FADE.as_bytes())];
            refusal(&pack(&with(manifest.clone(), path, value), &files))
        };
        // they came with interface 5
        assert_eq!(of("/compat/procraft/interface", json!(4)), Refusal::Described);
        // each needs its code, which reads only its own controls
        assert_eq!(of("/filters/1/key", json!("other")), Refusal::Described);
        assert_eq!(of("/filters/0/controls", json!(["fade"])), Refusal::FilterCode);
        // a control or a switch that isn't there, or one twice, isn't written right
        assert_eq!(of("/filters/0/controls", json!(["amount", "amount"])), Refusal::Described);
        assert_eq!(of("/filters/0/controls", json!(["gone"])), Refusal::Described);
        assert_eq!(of("/filters/0/under", json!("amount")), Refusal::Described);
        assert_eq!(of("/filters/1/label", json!(" ")), Refusal::Described);
        assert_eq!(of("/filters/1/key", json!("tint")), Refusal::Described);
        // nor is a list beside the one filter.wgsl, or with no filter held
        let mut beside = manifest.clone();
        let one = json!({ "path": "filter.wgsl", "sha256": sha256(SHADER), "size": SHADER.len() });
        beside["files"].as_array_mut().unwrap().push(one);
        let files =
            [("filters/tint.wgsl", TINT.as_bytes()), ("filters/fade.wgsl", FADE.as_bytes()), ("filter.wgsl", SHADER)];
        assert_eq!(refusal(&pack(&beside, &files)), Refusal::Described);
        let module = a_module();
        let mut coded_too = coded("org.example.plugin", &module);
        coded_too["compat"]["procraft"]["interface"] = json!(5);
        coded_too["filters"] = json!([{ "key": "tint", "label": "Tint" }]);
        assert_eq!(refusal(&a_coded_file(&coded_too, &module)), Refusal::Described);
    }

    /// A filter layer's filter: what is below, warmer by its value; and one that blurs by its value.
    pub(crate) const WARM: &str = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> { return vec4<f32>(colour.r + v.value * 0.1, colour.g, colour.b - v.value * 0.1, colour.a); }";
    pub(crate) const SMEAR: &str = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
        let r = v.value * 4.0;
        return (layer_at(at - vec2<f32>(r, 0.0)) + layer_at(at + vec2<f32>(r, 0.0))) * 0.5 + vec4<f32>(v.touch_x * 0.0);
    }";

    /// A manifest offering [`WARM`] and [`SMEAR`] as filter layers, under a switch, and its file.
    pub(crate) fn two_filter_layers(id: &str) -> (Value, Vec<u8>) {
        let listed =
            |path: &str, code: &str| json!({ "path": path, "sha256": sha256(code.as_bytes()), "size": code.len() });
        let mut manifest = filter(id);
        manifest["compat"]["procraft"]["interface"] = json!(17);
        manifest["files"] = json!([listed("filters/warm.wgsl", WARM), listed("filters/smear.wgsl", SMEAR)]);
        manifest["controls"] = json!([{ "key": "layers", "type": "switch", "label": "Filter layers", "starts": true }]);
        let value = |starts: f64| json!({ "key": "value", "type": "slider", "label": "Amount", "least": -1, "most": 1, "step": 0.02, "starts": starts });
        manifest["filters"] = json!([
            { "key": "warm", "label": "Warm", "layer": true, "group": "Colour", "values": [value(0.0)], "under": "layers" },
            { "key": "smear", "label": "Smear", "layer": true, "group": "Filters", "values": [value(0.5)],
              "reach": { "value": "value", "times": 4, "plus": 1 }, "touch": "point", "under": "layers" },
        ]);
        let file = pack(&manifest, &[("filters/warm.wgsl", WARM.as_bytes()), ("filters/smear.wgsl", SMEAR.as_bytes())]);
        (manifest, file)
    }

    #[test]
    fn a_plugin_can_offer_filter_layers_each_with_values_of_its_own_and_how_far_it_reads() {
        use crate::filter_layers::FilterTouch;
        use procraft_gpu::FilterReach;
        let (manifest, file) = two_filter_layers("org.example.layers");
        let package = read_package(&file).unwrap();
        let filters = &package.manifest.shape.filters;
        let layers: Vec<&LayerShape> = filters.iter().filter_map(|f| f.layer.as_ref()).collect();
        assert_eq!(layers.len(), 2);
        assert_eq!(
            (layers[0].group.as_str(), layers[0].reach, layers[0].touch),
            ("Colour", FilterReach::Within(0.0), None)
        );
        assert_eq!(layers[1].reach, FilterReach::ByValue { value: 0, times: 4.0, plus: 1.0 });
        assert_eq!(layers[1].touch, Some(FilterTouch::Point));
        assert_eq!(layers[1].code_keys(), ["value", "touch_x", "touch_y"]);
        // its value is the layer's own: keyed under the filter, starting where it says
        assert_eq!(layers[1].values[0].key(), "org.example.layers.smear.value");
        assert!((layers[1].values[0].number() - 0.5).abs() < 1e-5);
        let of = |path: &str, value: Value| {
            let files = [("filters/warm.wgsl", WARM.as_bytes()), ("filters/smear.wgsl", SMEAR.as_bytes())];
            refusal(&pack(&with(manifest.clone(), path, value), &files))
        };
        // they came with interface 17; an older app says it needs a newer one
        assert_eq!(of("/compat/procraft/interface", json!(16)), Refusal::Described);
        assert_eq!(of("/compat/procraft/interface", json!(INTERFACE + 1)), Refusal::Newer);
        // a heading, a value or few, of their own; none of the plugin's controls
        assert_eq!(of("/filters/0/group", json!(" ")), Refusal::Described);
        assert_eq!(of("/filters/0/group", json!("A heading much longer than it can be")), Refusal::Described);
        assert_eq!(of("/filters/0/values", json!([])), Refusal::Described);
        let mut reading = manifest.clone();
        reading["filters"][0]["controls"] = json!(["layers"]);
        let files = [("filters/warm.wgsl", WARM.as_bytes()), ("filters/smear.wgsl", SMEAR.as_bytes())];
        assert_eq!(refusal(&pack(&reading, &files)), Refusal::Described);
        let shown_elsewhere =
            json!([{ "key": "value", "type": "slider", "label": "A", "least": 0, "most": 1, "under": "layers" }]);
        assert_eq!(of("/filters/0/values", shown_elsewhere), Refusal::Described);
        let picked = json!([{ "key": "value", "type": "file", "label": "A", "accepts": "picture" }]);
        assert_eq!(of("/filters/0/values", picked), Refusal::Described);
        // how far it reads: a number, "whole", or by a slider of its own
        assert_eq!(of("/filters/1/reach", json!(-1)), Refusal::Described);
        assert_eq!(of("/filters/1/reach", json!("far")), Refusal::Described);
        assert_eq!(of("/filters/1/reach", json!({ "value": "gone" })), Refusal::Described);
        assert!(
            read_package(&pack(
                &with(manifest.clone(), "/filters/1/reach", json!("whole")),
                &[("filters/warm.wgsl", WARM.as_bytes()), ("filters/smear.wgsl", SMEAR.as_bytes())]
            ))
            .is_ok()
        );
        // a touch it doesn't have, or its code reading what its touch doesn't give
        assert_eq!(of("/filters/1/touch", json!("pinch")), Refusal::Described);
        assert_eq!(of("/filters/1/touch", json!("angle")), Refusal::FilterCode);
        // a filter under Adjustments keeps to what it had
        assert_eq!(of("/filters/0/layer", json!(false)), Refusal::Described);
        // an editor of the app's (interface 18): its values may be none, and its code reads tables
        let curves = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> { return vec4<f32>(table(1, colour.r).r, colour.g, colour.b, colour.a); }";
        let listed =
            json!({ "path": "filters/curves.wgsl", "sha256": sha256(curves.as_bytes()), "size": curves.len() });
        let mut editing = manifest.clone();
        editing["compat"]["procraft"]["interface"] = json!(18);
        editing["files"].as_array_mut().unwrap().push(listed);
        editing["filters"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "key": "curves", "label": "Curves", "layer": true, "group": "Colour", "editor": "curves" }));
        let files = [
            ("filters/warm.wgsl", WARM.as_bytes()),
            ("filters/smear.wgsl", SMEAR.as_bytes()),
            ("filters/curves.wgsl", curves.as_bytes()),
        ];
        let package = read_package(&pack(&editing, &files)).unwrap();
        let shape = package.manifest.shape.filters[2].layer.clone().unwrap();
        assert_eq!((shape.editor, shape.values.len()), (Some(crate::filter_layers::FilterEditor::Curves), 0));
        let at = |path: &str, value: Value| refusal(&pack(&with(editing.clone(), path, value), &files));
        assert_eq!(at("/compat/procraft/interface", json!(17)), Refusal::Described);
        assert_eq!(at("/filters/2/editor", json!("levels")), Refusal::Described);
        // without one, its code can't read tables, and its values can't be none
        assert_eq!(at("/filters/2/editor", json!(null)), Refusal::Described);
    }

    #[test]
    fn a_menu_sets_its_plugin_s_own_controls_and_draws_icons_that_hold_up() {
        let told = r#"(func (export "procraft_control") (param i32 f32)) (memory"#;
        let module = wat::parse_str(crate::plugin_run::tests::ACTS.replacen("(memory", told, 1)).unwrap();
        let square = "M3 3L21 3L21 21L3 21Z";
        let with_menu = || {
            let mut manifest = coded("org.example.menu", &module);
            manifest["compat"]["procraft"]["interface"] = json!(2);
            manifest["hooks"] = json!([]);
            manifest["permissions"] = json!(["artwork.write"]);
            manifest["actions"] = json!([{ "key": "grid", "label": "Add a grid layer" }]);
            manifest["controls"] = json!([
                { "key": "grid", "type": "switch", "label": "Grid" },
                { "key": "size", "type": "slider", "label": "Size", "least": 1, "most": 9, "step": 1, "starts": 3 },
            ]);
            manifest["menu"] = json!({ "place": "classic", "choices": [
                { "key": "big", "label": "Big grid", "icon": square, "sets": { "grid": true, "size": 9 } },
                { "key": "none", "label": "No grid", "icon": "m2 2 h20 v20 h-20 z", "sets": { "grid": false } },
            ] });
            manifest
        };
        let menu = read_package(&a_coded_file(&with_menu(), &module)).unwrap().manifest.shape.menu.unwrap();
        assert_eq!(menu.place, plugins::PluginMenuPlace::Classic);
        let big = MenuChoice {
            key: "org.example.menu.big".to_owned(),
            label: "Big grid".to_owned(),
            icon: square.to_owned(),
            sets: vec![(0, 1.0), (1, 9.0)],
        };
        let none = MenuChoice {
            key: "org.example.menu.none".to_owned(),
            label: "No grid".to_owned(),
            icon: "M2 2L22 2L22 22L2 22Z".to_owned(),
            sets: vec![(0, 0.0)],
        };
        assert_eq!(menu.choices, [big, none]);
        let of = |path: &str, value: Value| {
            let mut manifest = with_menu();
            let (parent, field) = path.rsplit_once('/').unwrap();
            manifest.pointer_mut(parent).unwrap()[field] = value;
            refusal(&a_coded_file(&manifest, &module))
        };
        // a place this app doesn't have is for a newer one
        assert_eq!(of("/menu/place", json!("somewhere")), Refusal::Newer);
        // each choice its own key, plain words, an icon on the grid, and a control of its own set as it can be
        assert_eq!(of("/menu/choices/1/key", json!("big")), Refusal::Described);
        assert_eq!(of("/menu/choices/1/key", json!("Big!")), Refusal::Described);
        assert_eq!(of("/menu/choices/1/label", json!(" ")), Refusal::Described);
        assert_eq!(of("/menu/choices/1/label", json!("A label far too long to show")), Refusal::Described);
        assert_eq!(of("/menu/choices/1/icon", json!("M0 0L30 30L0 30Z")), Refusal::Described);
        assert_eq!(of("/menu/choices/1/icon", json!("")), Refusal::Described);
        assert_eq!(of("/menu/choices/1/sets", json!({})), Refusal::Described);
        assert_eq!(of("/menu/choices/1/sets", json!({ "other": true })), Refusal::Described);
        assert_eq!(of("/menu/choices/1/sets", json!({ "grid": 1 })), Refusal::Described);
        assert_eq!(of("/menu/choices/1/sets", json!({ "size": true })), Refusal::Described);
        assert_eq!(of("/menu/choices/1/sets", json!({ "size": 10 })), Refusal::Described);
        assert_eq!(of("/menu/choices", json!([])), Refusal::Described);
        let nine: Vec<Value> = (0..9)
            .map(|i| json!({ "key": format!("c{i}"), "label": "C", "icon": square, "sets": { "grid": true } }))
            .collect();
        assert_eq!(of("/menu/choices", Value::Array(nine)), Refusal::Described);
        // a slider's value lands on a step
        let mut stepped = with_menu();
        stepped["menu"]["choices"][0]["sets"] = json!({ "size": 4.4 });
        let menu = read_package(&a_coded_file(&stepped, &module)).unwrap().manifest.shape.menu.unwrap();
        assert_eq!(menu.choices[0].sets, [(1, 4.0)]);
        // only code has one: a filter's controls are its panel's
        let mut filtered = a_filter();
        filtered["menu"] = json!({ "place": "classic", "choices": [
            { "key": "none", "label": "Off", "icon": square, "sets": { "keep_colour": false } },
        ] });
        assert_eq!(refusal(&pack(&filtered, &[(FILTER_CODE, SHADER)])), Refusal::Described);
    }

    #[test]
    fn what_comes_under_a_switch_and_what_a_plugin_replaces_are_written_plainly() {
        // the acting module, told its controls
        let told = r#"(func (export "procraft_control") (param i32 f32)) (memory"#;
        let module = wat::parse_str(crate::plugin_run::tests::ACTS.replacen("(memory", told, 1)).unwrap();
        let acting = || {
            let mut manifest = coded("org.example.new", &module);
            manifest["compat"]["procraft"]["interface"] = json!(2);
            manifest["hooks"] = json!([]);
            manifest["permissions"] = json!(["artwork.write"]);
            manifest["actions"] = json!([{ "key": "grid", "label": "Add a grid layer", "under": "grid" }]);
            manifest["controls"] = json!([
                { "key": "grid", "type": "switch", "label": "Grid" },
                { "key": "size", "type": "slider", "label": "Size", "least": 1, "most": 9, "under": "grid" },
            ]);
            manifest["replaces"] = json!([
                { "id": "org.example.old", "switch": "grid", "controls": { "spacing": "size" } },
                { "id": "org.example.other" },
            ]);
            manifest
        };
        let package = read_package(&a_coded_file(&acting(), &module)).unwrap();
        let shape = package.manifest.shape;
        assert_eq!((shape.under, shape.action_under), (vec![None, Some(0)], vec![Some(0)]));
        let old = Replaced {
            id: "org.example.old".to_owned(),
            switch: Some(0),
            controls: vec![("org.example.old.spacing".to_owned(), 1)],
            marks: false,
        };
        let other = Replaced { id: "org.example.other".to_owned(), switch: None, controls: Vec::new(), marks: false };
        assert_eq!(shape.replaces, [old, other]);
        let of = |path: &str, value: Value| {
            let mut manifest = acting();
            let (parent, field) = path.rsplit_once('/').unwrap();
            manifest.pointer_mut(parent).unwrap()[field] = value;
            refusal(&a_coded_file(&manifest, &module))
        };
        // under a switch at the top of the list or under one there, never a slider, or itself
        assert_eq!(of("/controls/1/under", json!("size")), Refusal::Described);
        assert_eq!(of("/controls/0/under", json!("grid")), Refusal::Described);
        let mut deeper = acting();
        deeper["controls"] = json!([
            { "key": "grid", "type": "switch", "label": "Grid" },
            { "key": "fine", "type": "switch", "label": "Fine", "under": "grid" },
            { "key": "size", "type": "slider", "label": "Size", "least": 1, "most": 9, "under": "fine" },
            { "key": "finer", "type": "switch", "label": "Finer", "under": "fine" },
            { "key": "step", "type": "slider", "label": "Step", "least": 1, "most": 9, "under": "finer" },
        ]);
        assert_eq!(refusal(&a_coded_file(&deeper, &module)), Refusal::Described, "three deep");
        deeper["controls"].as_array_mut().unwrap().truncate(3);
        let shape = read_package(&a_coded_file(&deeper, &module)).unwrap().manifest.shape;
        assert_eq!(shape.under, [None, Some(0), Some(1)]);
        // and it shows while both its switches are on
        let switch = |on: bool| PluginControl::Switch {
            control: PluginSwitch { key: "k".to_owned(), label: "K".to_owned(), on, starts: on },
        };
        let slider = PluginControl::Switch {
            control: PluginSwitch { key: "s".to_owned(), label: "S".to_owned(), on: false, starts: false },
        };
        for (grid, fine, shows) in [(true, true, true), (true, false, false), (false, true, false)] {
            let controls = [switch(grid), switch(fine), slider.clone()];
            assert_eq!(shape.shows(Some(1), &controls), shows, "{grid} {fine}");
        }
        assert_eq!(of("/actions/0/under", json!("size")), Refusal::Described);
        assert_eq!(of("/actions/0/under", json!("nothing")), Refusal::Described);
        // a plugin beside its own id, once, not itself; what took its values is here
        for id in ["org.example.new", "com.example.old", "nodots", "org.example.other"] {
            assert_eq!(of("/replaces/0/id", json!(id)), Refusal::Described, "{id}");
        }
        assert_eq!(of("/replaces/0/switch", json!("size")), Refusal::Described);
        assert_eq!(of("/replaces/0/controls", json!({ "spacing": "gone" })), Refusal::Described);
        assert_eq!(of("/replaces/0/controls", json!({ "Not a key": "size" })), Refusal::Described);
    }

    /// Bytes from a seed, so a failure can be run again.
    fn scrambled(seed: u64, bytes: &mut [u8]) {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        for _ in 0..8 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let at = (state as usize) % bytes.len();
            bytes[at] = (state >> 32) as u8;
        }
    }

    #[test]
    fn no_damage_to_a_file_panics_the_reader() {
        let good = a_filter_file(&a_filter());
        for seed in 1..3000 {
            let mut bytes = good.clone();
            scrambled(seed, &mut bytes);
            // anything but a panic: most are refused, a few bytes nobody reads may change nothing
            let _ = read_package(&bytes).map(|p| p.manifest.id);
            let cut = (seed as usize * 7) % good.len();
            let _ = read_package(good.get(..cut).unwrap_or_default()).map(|p| p.manifest.id);
        }
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("procraft-plugin-file-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn what_is_kept_is_read_back_and_a_bad_file_counts_as_absent() {
        let dir = folder("kept");
        let package = read_package(&a_filter_file(&a_filter())).unwrap();
        let kept = keep(&dir, &package).unwrap();
        assert_eq!(kept, dir.join("org.example.filter").join("1.0.0"));
        assert_eq!(read_all(&dir).len(), 1);
        // a changed byte, then a file more, then one gone: each and it isn't there
        let shader = kept.join("filter.wgsl");
        std::fs::write(&shader, b"changed").unwrap();
        assert!(read_all(&dir).is_empty());
        std::fs::write(&shader, SHADER).unwrap();
        assert_eq!(read_all(&dir).len(), 1);
        std::fs::write(kept.join("extra.txt"), b"x").unwrap();
        assert!(read_all(&dir).is_empty());
        std::fs::remove_file(kept.join("extra.txt")).unwrap();
        std::fs::remove_file(&shader).unwrap();
        assert!(read_all(&dir).is_empty());
        std::fs::write(&shader, SHADER).unwrap();
        // a manifest that says it is something other than where it is kept
        let text = kept.join(MANIFEST);
        let good = std::fs::read(&text).unwrap();
        let other = with(a_filter(), "/version", json!("2.0.0")).to_string();
        std::fs::write(&text, other).unwrap();
        assert!(read_all(&dir).is_empty());
        std::fs::write(&text, good).unwrap();
        assert_eq!(read_all(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_a_kept_plugin_counts_as_absent() {
        let dir = folder("link");
        let kept = keep(&dir, &read_package(&a_filter_file(&a_filter())).unwrap()).unwrap();
        std::fs::remove_file(kept.join("filter.wgsl")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", kept.join("filter.wgsl")).unwrap();
        assert!(read_all(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_a_run_left_behind_goes() {
        let dir = folder("left");
        let staged = dir.join(".staging-1-2");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("half.txt"), b"x").unwrap();
        assert!(read_all(&dir).is_empty());
        assert!(!staged.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_version_replaces_the_old_whole() {
        let dir = folder("replace");
        keep(&dir, &read_package(&a_filter_file(&a_filter())).unwrap()).unwrap();
        let newer = with(a_filter(), "/version", json!("1.1.0"));
        keep(&dir, &read_package(&a_filter_file(&newer)).unwrap()).unwrap();
        let kept = read_all(&dir);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].version, "1.1.0");
        assert!(!dir.join("org.example.filter").join("1.0.0").exists());
        // the same again takes its own place
        keep(&dir, &read_package(&a_filter_file(&newer)).unwrap()).unwrap();
        assert_eq!(read_all(&dir).len(), 1);
        assert!(forget(&dir, "org.example.filter"));
        assert!(read_all(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_a_plain_file_is_read() {
        let dir = folder("plain");
        assert_eq!(read_plugin_file(&dir), None, "a folder");
        assert_eq!(read_plugin_file(&dir.join("missing")), None);
        let file = dir.join("a.procraft-plugin");
        std::fs::write(&file, b"hello").unwrap();
        assert_eq!(read_plugin_file(&file).as_deref(), Some(&b"hello"[..]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_is_passed_over_rather_than_waited_on() {
        let dir = folder("pipe");
        let pipe = dir.join("a.procraft-plugin");
        let made = std::process::Command::new("mkfifo").arg(&pipe).status();
        if made.is_ok_and(|s| s.success()) {
            assert_eq!(read_plugin_file(&pipe), None);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- plugins made of features (interface 16)

    /// A module on the position and pressure hooks, told its controls.
    fn two_hook_module() -> Vec<u8> {
        wat::parse_str(
            r#"(module
            (func (export "procraft_control") (param i32 f32))
            (func (export "procraft_position") (param f32 f32) (result i64) (i64.const 0))
            (func (export "procraft_pressure") (param i32 i32 f32 f32 f32 f32) (result f32) (f32.const -1))
            (func (export "procraft_pressure_taper") (param i32 i32) (result f32) (f32.const 0)))"#,
        )
        .unwrap()
    }

    /// Two features, Offset on the position hook and Press on the pressure hook, each with a switch
    /// and a slider under it, the slider of Press under a switch of its own.
    fn featured() -> Value {
        let mut manifest = coded("org.example.both", &two_hook_module());
        manifest["compat"]["procraft"]["interface"] = json!(16);
        manifest["hooks"] = json!(["position", "pressure"]);
        manifest["controls"] = json!([
            { "key": "offset", "type": "feature", "label": "Offset", "description": "Moves samples.", "hooks": ["position"] },
            { "key": "x", "type": "slider", "label": "X", "least": -9, "most": 9, "under": "offset" },
            { "key": "press", "type": "feature", "label": "Press", "description": "Presses.", "hooks": ["pressure"] },
            { "key": "lighter", "type": "switch", "label": "Lighter", "under": "press" },
            { "key": "softer", "type": "switch", "label": "Softer", "under": "lighter" },
            { "key": "level", "type": "slider", "label": "Level", "least": 0, "most": 1, "step": 0.1, "under": "softer" },
        ]);
        manifest
    }

    #[test]
    fn a_plugin_of_features_heads_each_with_its_switch_its_words_and_its_hooks() {
        let package = read_package(&a_coded_file(&featured(), &two_hook_module())).unwrap();
        let shape = &package.manifest.shape;
        let features: Vec<(usize, &str, &[Hook])> =
            shape.features.iter().map(|f| (f.at, f.description.as_str(), f.hooks.as_slice())).collect();
        assert_eq!(features, [(0, "Moves samples.", &[Hook::Position][..]), (2, "Presses.", &[Hook::Pressure][..])]);
        // a feature is a switch that starts off; what comes under it, three deep
        assert!(
            matches!(&package.manifest.controls[0], PluginControl::Switch { control } if !control.on && !control.starts)
        );
        assert_eq!(shape.under, [None, Some(0), None, Some(2), Some(3), Some(4)]);
        let belongs: Vec<Option<usize>> = (0..6).map(|at| shape.feature_of(at)).collect();
        assert_eq!(belongs, [Some(0), Some(0), Some(1), Some(1), Some(1), Some(1)]);
        assert_eq!(shape.feature_switch_of(5), Some(2));
    }

    #[test]
    fn a_plugin_of_features_is_refused_unless_everything_comes_under_one_within_its_limits() {
        let module = two_hook_module();
        let of = |manifest: Value| read_package(&a_coded_file(&manifest, &module)).err();
        let set = |at: usize, field: &str, value: Value| {
            let mut manifest = featured();
            manifest["controls"][at][field] = value;
            manifest
        };
        assert_eq!(of(featured()), None);
        // before interface 16, a feature is a control the plugin's interface doesn't have
        assert_eq!(of(with(featured(), "/compat/procraft/interface", json!(15))), Some(Refusal::Described));
        // under nothing, or a feature under something
        assert_eq!(of(set(1, "under", Value::Null)), Some(Refusal::Described));
        assert_eq!(of(set(2, "under", json!("offset"))), Some(Refusal::Described));
        // three deep under a feature, not four
        let mut deep = featured();
        let switch = json!({ "key": "deeper", "type": "switch", "label": "Deeper", "under": "softer" });
        deep["controls"].as_array_mut().unwrap().push(switch);
        assert_eq!(of(deep.clone()), None, "three deep");
        let slider =
            json!({ "key": "deepest", "type": "slider", "label": "D", "least": 0, "most": 1, "under": "deeper" });
        deep["controls"].as_array_mut().unwrap().push(slider);
        assert_eq!(of(deep), Some(Refusal::Described), "four deep");
        // a feature's hooks are the plugin's, and each of those some feature's
        assert_eq!(of(set(0, "hooks", json!(["pressure", "position"]))), None);
        assert_eq!(of(set(0, "hooks", json!(["position", "pick_layer"]))), Some(Refusal::Described));
        assert_eq!(of(set(2, "hooks", json!([]))), Some(Refusal::Described));
        assert_eq!(of(set(2, "hooks", json!(["teleport"]))), Some(Refusal::Newer));
        assert_eq!(of(set(2, "hooks", json!(["pressure", "pressure"]))), Some(Refusal::Described));
        // a feature starts off, and is reached on its card alone
        for (field, value) in [
            ("starts", json!(true)),
            ("place", json!("canvas")),
            ("press", json!("alt+q")),
            ("artwork", json!(true)),
            ("keeps", json!("fill_gap")),
        ] {
            assert_eq!(of(set(0, field, value)), Some(Refusal::Described), "{field}");
        }
        assert_eq!(of(set(0, "starts", json!(false))), None);
        // words and hooks are a feature's alone
        assert_eq!(of(set(1, "description", json!("A slider."))), Some(Refusal::Described));
        // sixteen under a feature, and 64 in all
        let mut many = featured();
        let controls = many["controls"].as_array_mut().unwrap();
        controls.extend(
            (0..14).map(|i| json!({ "key": format!("s{i}"), "type": "switch", "label": "S", "under": "press" })),
        );
        assert_eq!(of(many.clone()), Some(Refusal::Described), "seventeen under Press");
        many["controls"].as_array_mut().unwrap().pop();
        assert_eq!(of(many), None, "sixteen");
    }

    #[test]
    fn what_a_plugin_of_features_takes_over_holds_up() {
        let module = two_hook_module();
        let of = |manifest: Value| read_package(&a_coded_file(&manifest, &module));
        let mut manifest = featured();
        manifest["replaces"] = json!([
            { "id": "org.example.old", "switch": "lighter", "controls": { "spacing": "x" }, "marks": true },
            { "id": "org.example.other", "switch": "offset" },
        ]);
        manifest["brush_controls"] = json!([{ "key": "kind", "label": "Kind", "choices": ["A", "B"], "under": "press", "from": "org.example.old" }]);
        let package = of(manifest.clone()).unwrap();
        let shape = &package.manifest.shape;
        assert_eq!(shape.marks_id("org.example.both"), "org.example.old");
        assert_eq!(shape.brush_under, [Some(2)]);
        // a row taken over is kept under the old one's key, so brushes say what they said
        assert_eq!(package.manifest.brush_controls[0].key, "plugin:org.example.old:kind");
        // the marks of one at most; a row from one it takes the place of; with features, a row under one
        let mut two = manifest.clone();
        two["replaces"][1]["marks"] = json!(true);
        assert_eq!(of(two).err(), Some(Refusal::Described));
        let mut stranger = manifest.clone();
        stranger["brush_controls"][0]["from"] = json!("org.example.stranger");
        assert_eq!(of(stranger).err(), Some(Refusal::Described));
        let mut loose = manifest.clone();
        loose["brush_controls"][0].as_object_mut().unwrap().remove("under");
        assert_eq!(of(loose).err(), Some(Refusal::Described));
        // nor before interface 16 (written without features)
        let mut older = coded("org.example.older", &module);
        older["compat"]["procraft"]["interface"] = json!(15);
        older["replaces"] = json!([{ "id": "org.example.old", "marks": true }]);
        assert_eq!(of(older.clone()).err(), Some(Refusal::Described));
        older["replaces"] = json!([{ "id": "org.example.old" }]);
        older["brush_controls"] =
            json!([{ "key": "kind", "label": "Kind", "choices": ["A", "B"], "from": "org.example.old" }]);
        assert_eq!(of(older.clone()).err(), Some(Refusal::Described));
        older["brush_controls"][0].as_object_mut().unwrap().remove("from");
        assert_eq!(of(older).unwrap().manifest.brush_controls[0].key, "plugin:org.example.older:kind");
    }

    #[test]
    fn what_a_plugin_of_features_takes_from_plugins_that_stay_holds_up_from_interface_19() {
        let module = two_hook_module();
        let of = |manifest: Value| read_package(&a_coded_file(&manifest, &module));
        let mut manifest = with(featured(), "/compat/procraft/interface", json!(19));
        manifest["replaces"] = json!([{ "id": "org.example.old", "switch": "offset", "controls": { "spacing": "x" } }]);
        manifest["takes"] = json!([
            { "id": "org.example.stays", "feature": "pressing", "switch": "press", "controls": { "how": "level", "flag": "lighter" } },
            { "id": "org.example.also", "feature": "moving", "switch": "offset", "filters": true },
        ]);
        let package = of(manifest.clone()).unwrap();
        let shape = &package.manifest.shape;
        let taken: Vec<(&str, &str, usize, bool)> =
            shape.takes.iter().map(|t| (t.id.as_str(), t.feature.as_str(), t.switch, t.filters)).collect();
        assert_eq!(
            taken,
            [
                ("org.example.stays", "org.example.stays.pressing", 2, false),
                ("org.example.also", "org.example.also.moving", 0, true)
            ]
        );
        let controls = [("org.example.stays.flag".to_owned(), 3), ("org.example.stays.how".to_owned(), 5)];
        assert_eq!(shape.takes[0].controls, controls);
        // the keys each control's value was kept under before: by one it replaces, or a feature taken
        assert_eq!(shape.kept_before(5).collect::<Vec<_>>(), ["org.example.stays.how"]);
        assert_eq!(shape.kept_before(1).collect::<Vec<_>>(), ["org.example.old.spacing"]);
        assert_eq!(shape.kept_before(4).count(), 0);
        // refused: before interface 19; its own id, one not beside it, one it replaces, or a
        // feature twice; by a switch that isn't a feature, or a control it hasn't; a key not plain
        assert_eq!(of(with(manifest.clone(), "/compat/procraft/interface", json!(18))).err(), Some(Refusal::Described));
        for (path, value) in [
            ("/takes/0/id", json!("org.example.both")),
            ("/takes/0/id", json!("com.other.stays")),
            ("/takes/0/id", json!("org.example.old")),
            ("/takes/1", json!({ "id": "org.example.stays", "feature": "pressing", "switch": "offset" })),
            ("/takes/0/switch", json!("lighter")),
            ("/takes/0/switch", json!("gone")),
            ("/takes/0/controls", json!({ "how": "gone" })),
            ("/takes/0/controls", json!({ "Not a key": "level" })),
            ("/takes/0/feature", json!("Not a key")),
        ] {
            assert_eq!(
                of(with(manifest.clone(), path, value.clone())).err(),
                Some(Refusal::Described),
                "{path} {value}"
            );
        }
        let another = json!({ "id": "org.example.stays", "feature": "moving", "switch": "offset" });
        assert!(of(with(manifest.clone(), "/takes/1", another)).is_ok(), "another of its features");
        // eight at most
        let eight: Vec<Value> =
            (0..8).map(|i| json!({ "id": format!("org.example.p{i}"), "feature": "f", "switch": "press" })).collect();
        assert!(of(with(manifest.clone(), "/takes", json!(eight.clone()))).is_ok());
        let mut nine = eight;
        nine.push(json!({ "id": "org.example.p8", "feature": "f", "switch": "press" }));
        assert_eq!(of(with(manifest.clone(), "/takes", json!(nine))).err(), Some(Refusal::Described));
        // a plugin without features takes none
        let mut plain = coded("org.example.plain", &module);
        plain["compat"]["procraft"]["interface"] = json!(19);
        plain["controls"] = json!([{ "key": "on_too", "type": "switch", "label": "On" }]);
        assert!(of(plain.clone()).is_ok());
        plain["takes"] = json!([{ "id": "org.example.stays", "feature": "f", "switch": "on_too" }]);
        assert_eq!(of(plain).err(), Some(Refusal::Described));
    }

    #[test]
    fn a_plugin_s_brushes_come_under_a_switch_given_as_one_it_replaces_from_interface_19() {
        let module = two_hook_module();
        let set = fill_brushes_file();
        let holding = |brushes: Value, interface: u32| {
            let mut manifest = with(featured(), "/compat/procraft/interface", json!(interface));
            manifest["holds"] = json!(["hooks", "brushes"]);
            manifest["replaces"] = json!([{ "id": "org.example.old", "switch": "offset" }]);
            manifest["brushes"] = brushes;
            let files = json!([
                { "path": MODULE, "sha256": sha256(&module), "size": module.len() },
                { "path": BRUSH_SET, "sha256": sha256(&set), "size": set.len() },
            ]);
            read_package(&pack(&with(manifest, "/files", files), &[(MODULE, &module), (BRUSH_SET, &set)]))
        };
        let given = |package: Package| {
            let shape = package.manifest.shape;
            (shape.brushes_under, shape.brushes_id("org.example.both").to_owned())
        };
        assert_eq!(
            given(holding(json!({ "under": "lighter", "from": "org.example.old" }), 19).unwrap()),
            (Some(3), "org.example.old".to_owned())
        );
        assert_eq!(given(holding(json!({ "under": "press" }), 19).unwrap()), (Some(2), "org.example.both".to_owned()));
        assert_eq!(given(holding(Value::Null, 19).unwrap()), (None, "org.example.both".to_owned()), "as before 19");
        // under no switch, from one it doesn't replace, or before 19: refused
        for (brushes, interface) in [
            (json!({ "under": "x" }), 19),
            (json!({ "under": "gone" }), 19),
            (json!({ "from": "org.example.stranger" }), 19),
            (json!({ "under": "press" }), 18),
        ] {
            assert_eq!(holding(brushes.clone(), interface).err(), Some(Refusal::Described), "{brushes} {interface}");
        }
        // said of a plugin with no set
        let mut no_set = with(featured(), "/compat/procraft/interface", json!(19));
        no_set["brushes"] = json!({ "under": "press" });
        assert_eq!(read_package(&a_coded_file(&no_set, &module)).err(), Some(Refusal::Described));
    }

    #[test]
    fn a_plugin_written_before_features_reads_as_it_did() {
        // a switch under a switch under a switch: two deep at most, as ever
        let module = two_hook_module();
        let mut manifest = coded("org.example.older", &module);
        manifest["compat"]["procraft"]["interface"] = json!(15);
        manifest["controls"] = json!([
            { "key": "a", "type": "switch", "label": "A" },
            { "key": "b", "type": "switch", "label": "B", "under": "a" },
            { "key": "c", "type": "slider", "label": "C", "least": 0, "most": 1, "under": "b" },
        ]);
        let package = read_package(&a_coded_file(&manifest, &module)).unwrap();
        assert!(package.manifest.shape.features.is_empty());
        assert_eq!(package.manifest.shape.under, [None, Some(0), Some(1)]);
        manifest["controls"][2]["type"] = json!("switch");
        manifest["controls"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "key": "d", "type": "switch", "label": "D", "under": "c" }));
        assert_eq!(read_package(&a_coded_file(&manifest, &module)).err(), Some(Refusal::Described));
        // seventeen controls are still too many for it, and a menu or a row under a switch reads
        let mut many = coded("org.example.many", &module);
        many["compat"]["procraft"]["interface"] = json!(15);
        many["controls"] = json!(
            (0..17).map(|i| json!({ "key": format!("s{i}"), "type": "switch", "label": "S" })).collect::<Vec<_>>()
        );
        assert_eq!(read_package(&a_coded_file(&many, &module)).err(), Some(Refusal::Described));
        let mut rows = coded("org.example.rows", &module);
        rows["compat"]["procraft"]["interface"] = json!(15);
        rows["controls"] = json!([{ "key": "a", "type": "switch", "label": "A" }]);
        rows["brush_controls"] = json!([{ "key": "kind", "label": "Kind", "choices": ["A", "B"], "under": "a" }]);
        let package = read_package(&a_coded_file(&rows, &module)).unwrap();
        assert_eq!(package.manifest.shape.brush_under, [Some(0)]);
    }
}
