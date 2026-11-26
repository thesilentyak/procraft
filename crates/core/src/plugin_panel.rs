//! A plugin's own panel (interface 2, the `panel` permission): rows it describes as data, which each
//! app draws as its own cards are drawn, so a plugin's panel looks like the app's. An action shows it
//! (`show_panel` with the rows as JSON text); each touch on a row goes back to the plugin as
//! `procraft_panel_event(row, value)`, in the same copy of its code, which may then show the panel
//! again changed, change the artwork as an action does, or close it.
//!
//! ```json
//! { "title": "Halftone", "rows": [
//!   { "type": "slider", "label": "Dot size", "least": 1, "most": 50, "step": 1, "value": 8, "unit": " px" },
//!   { "type": "switch", "label": "Round dots", "on": true },
//!   { "type": "choice", "label": "Shape", "options": ["Dots", "Lines"], "chosen": 0 },
//!   { "type": "text", "text": "Works on the current layer." },
//!   { "type": "button", "label": "Apply" }
//! ] }
//! ```
//!
//! Rows go by their place in `rows`; a value is a slider's, a switch's (1 on, 0 off), a choice's
//! place, or 1 for a button tapped. A `keys` row shows presses as the platform writes them
//! (`{ "type": "keys", "label": "Undo", "keys": ["command+z"] }`, see [`crate::KeyCombo`]) and is
//! tapped as a button is; a panel with `"key_press": true` asks for the next key press, which goes
//! to the plugin as `procraft_panel_key` rather than to the canvas. A `colours` row (interface 10)
//! shows one to eight colours to tap, the one at `chosen` (if any) ringed:
//! `{ "type": "colours", "label": "Colour", "colours": [15026509, 4130810], "chosen": 1 }`, each
//! 0xRRGGBB; a tap is told as the colour's place.
//!
//! From interface 11 a `brushes` row shows brushes to tap by their sample strokes, each by the key
//! `recent_brush` gives (`{ "type": "brushes", "label": "Brushes", "brushes": [4294967297], "chosen":
//! 0 }`), told as a colour's tap is; and a panel with `"ring": true` shows as a ring round the point
//! its action came from (the pen whose button ran it, or the window's middle), its rows of colours
//! and brushes each a ring about it, the first innermost, with no title or Done: a tap off it closes
//! it.
//!
//! From interface 15 a `control` row shows one of the plugin's own `texture` controls, by its key, as
//! its card does (`{ "type": "control", "key": "texture" }`): set there, it is set as on the card,
//! and the panel isn't told.

use serde::Deserialize;

/// The most rows a panel may have, the most choices one row may give (colours too), and the most
/// presses a row of keys shows.
const MOST_ROWS: usize = 16;
const MOST_OPTIONS: usize = 8;
const MOST_KEYS: usize = 8;
/// The most rows a ring has, and the most colours or brushes on each of them.
const MOST_RINGS: usize = 3;
const MOST_ON_A_RING: usize = 12;

/// A plugin's panel as the apps show it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PluginPanel {
    pub title: String,
    pub rows: Vec<PanelRow>,
    /// It asks for the next key press: the platform hands it to [`crate::Plugins::panel_key`].
    pub key_press: bool,
    /// It shows as a ring round [`PluginPanel::at`] (interface 11): its rows (colours and brushes
    /// alone) each a ring about it, the first innermost, no title or Done, closed by a tap off it.
    pub ring: bool,
    /// Where a ring shows, window pixels: where the pen was whose button ran its action; none, the
    /// window's middle.
    pub at: Option<PanelPoint>,
}

/// A point in the window, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct PanelPoint {
    pub x: f32,
    pub y: f32,
}

/// A brush a panel shows (interface 11): where it is in the library, and its name.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PanelBrush {
    pub set: u32,
    pub index: u32,
    pub name: String,
}

/// One of its rows.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum PanelRow {
    /// Words, a line or a few.
    Text {
        text: String,
    },
    /// A slider from `least` to `most` in steps of `step`, `unit` after its value.
    Slider {
        label: String,
        least: f32,
        most: f32,
        step: f32,
        value: f32,
        unit: String,
    },
    Switch {
        label: String,
        on: bool,
    },
    /// One of `options`, the one at `chosen` picked.
    Choice {
        label: String,
        options: Vec<String>,
        chosen: u32,
    },
    /// A row tapped, its words in the accent colour.
    Button {
        label: String,
    },
    /// `colours` (0xRRGGBB) to tap, the one at `chosen` ringed if any is (interface 10).
    Colours {
        label: String,
        colours: Vec<u32>,
        chosen: Option<u32>,
    },
    /// `brushes` to tap, each shown by its sample stroke, the one at `chosen` ringed if any is
    /// (interface 11).
    Brushes {
        label: String,
        brushes: Vec<PanelBrush>,
        chosen: Option<u32>,
    },
    /// A row tapped, `keys` (presses, written for the platform: "Ctrl+Z  Ctrl+Y", or "None") at
    /// its end.
    Keys {
        label: String,
        keys: String,
    },
    /// One of the plugin's own controls, kept under `key`, shown as its card shows it (interface
    /// 15); set as on the card, not told to the panel.
    Control {
        key: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPanel {
    title: String,
    rows: Vec<RawRow>,
    #[serde(default)]
    key_press: bool,
    #[serde(default)]
    ring: bool,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum RawRow {
    Text {
        text: String,
    },
    Slider {
        label: String,
        least: f32,
        most: f32,
        #[serde(default)]
        step: Option<f32>,
        value: f32,
        #[serde(default)]
        unit: String,
    },
    Switch {
        label: String,
        #[serde(default)]
        on: bool,
    },
    Choice {
        label: String,
        options: Vec<String>,
        #[serde(default)]
        chosen: u32,
    },
    Button {
        label: String,
    },
    Colours {
        label: String,
        colours: Vec<u32>,
        #[serde(default)]
        chosen: Option<u32>,
    },
    Keys {
        label: String,
        keys: Vec<String>,
    },
    Brushes {
        label: String,
        brushes: Vec<i64>,
        #[serde(default)]
        chosen: Option<u32>,
    },
    Control {
        key: String,
    },
}

/// Words that show as they are: some, not too many, and no control characters.
fn plain(text: &str, most: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= most && !text.chars().any(char::is_control)
}

/// The panel `json` describes, if it holds up: a title, 1 to 16 rows, each with words that show
/// and numbers that make sense. A slider's value is put in its range. It shows no brushes.
#[cfg(test)]
pub(crate) fn read_panel(json: &str) -> Option<PluginPanel> {
    read_panel_with(json, &|_| None, &|key| Some(format!("org.example.{key}")))
}

/// The panel `json` describes, as [`read_panel`] reads it, its brushes found by their keys with
/// `brush` (none for a key that is no brush: the panel doesn't hold up), and its controls' keys
/// kept as `control` keeps them (none: it doesn't hold up). A ring has one to three rows, each of
/// colours or brushes, up to twelve on each.
pub(crate) fn read_panel_with(
    json: &str,
    brush: &dyn Fn(i64) -> Option<PanelBrush>,
    control: &dyn Fn(&str) -> Option<String>,
) -> Option<PluginPanel> {
    let raw: RawPanel = serde_json::from_str(json).ok()?;
    let most = if raw.ring { MOST_RINGS } else { MOST_ROWS };
    if !plain(&raw.title, 40) || raw.rows.is_empty() || raw.rows.len() > most {
        return None;
    }
    let ring = raw.ring;
    let rows = raw.rows.into_iter().map(|row| read_row(row, ring, brush, control)).collect::<Option<Vec<_>>>()?;
    if ring && !rows.iter().all(|r| matches!(r, PanelRow::Colours { .. } | PanelRow::Brushes { .. })) {
        return None;
    }
    Some(PluginPanel { title: raw.title, rows, key_press: raw.key_press, ring, at: None })
}

fn read_row(
    raw: RawRow,
    ring: bool,
    brush: &dyn Fn(i64) -> Option<PanelBrush>,
    control: &dyn Fn(&str) -> Option<String>,
) -> Option<PanelRow> {
    let most = if ring { MOST_ON_A_RING } else { MOST_OPTIONS };
    Some(match raw {
        RawRow::Text { text } => {
            // a few lines: line breaks are the only control characters it may have
            let shows = !text.trim().is_empty()
                && text.chars().count() <= 400
                && !text.chars().any(|c| c.is_control() && c != '\n');
            shows.then_some(PanelRow::Text { text })?
        }
        RawRow::Slider { label, least, most, step, value, unit } => {
            let step = step.unwrap_or(0.0);
            let numbers = [least, most, step, value].iter().all(|n| n.is_finite());
            if !plain(&label, 40) || !numbers || least >= most || step < 0.0 || unit.chars().count() > 8 {
                return None;
            }
            let value = value.clamp(least, most);
            PanelRow::Slider { label, least, most, step, value, unit }
        }
        RawRow::Switch { label, on } => plain(&label, 40).then_some(PanelRow::Switch { label, on })?,
        RawRow::Choice { label, options, chosen } => {
            let fits = plain(&label, 40)
                && (1..=MOST_OPTIONS).contains(&options.len())
                && options.iter().all(|o| plain(o, 24))
                && (chosen as usize) < options.len();
            fits.then_some(PanelRow::Choice { label, options, chosen })?
        }
        RawRow::Button { label } => plain(&label, 40).then_some(PanelRow::Button { label })?,
        RawRow::Colours { label, colours, chosen } => {
            let fits = plain(&label, 40)
                && (1..=most).contains(&colours.len())
                && colours.iter().all(|&c| c <= 0xFF_FFFF)
                && chosen.is_none_or(|at| (at as usize) < colours.len());
            fits.then_some(PanelRow::Colours { label, colours, chosen })?
        }
        RawRow::Brushes { label, brushes, chosen } => {
            let fits = plain(&label, 40)
                && (1..=most).contains(&brushes.len())
                && chosen.is_none_or(|at| (at as usize) < brushes.len());
            if !fits {
                return None;
            }
            let brushes = brushes.into_iter().map(brush).collect::<Option<Vec<_>>>()?;
            PanelRow::Brushes { label, brushes, chosen }
        }
        RawRow::Keys { label, keys } => {
            if !plain(&label, 40) || keys.len() > MOST_KEYS {
                return None;
            }
            let presses = keys.iter().map(|k| crate::KeyCombo::read(k)).collect::<Option<Vec<_>>>()?;
            let written: Vec<String> = presses.iter().map(|p| p.label(crate::keys::MAC)).collect();
            let keys = if written.is_empty() { "None".to_owned() } else { written.join("  ") };
            PanelRow::Keys { label, keys }
        }
        RawRow::Control { key } => PanelRow::Control { key: control(&key)? },
    })
}

/// The value row `row` of `panel` takes when touched with `value`, as the plugin is told it: a
/// slider's in its range and on its steps, a switch's 1 or 0, a choice's place, a button's 1. None
/// for a row that takes no touch or isn't there, or a value that is no number.
pub(crate) fn touched(panel: &PluginPanel, row: usize, value: f32) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    match panel.rows.get(row)? {
        PanelRow::Text { .. } => None,
        PanelRow::Slider { least, most, step, .. } => {
            let on_step = if *step > 0.0 { least + ((value - least) / step).round() * step } else { value };
            Some(on_step.clamp(*least, *most))
        }
        PanelRow::Switch { .. } => Some(if value >= 0.5 { 1.0 } else { 0.0 }),
        PanelRow::Choice { options, .. } => {
            let at = value.round();
            (at >= 0.0 && (at as usize) < options.len()).then_some(at)
        }
        PanelRow::Colours { colours, .. } => {
            let at = value.round();
            (at >= 0.0 && (at as usize) < colours.len()).then_some(at)
        }
        PanelRow::Brushes { brushes, .. } => {
            let at = value.round();
            (at >= 0.0 && (at as usize) < brushes.len()).then_some(at)
        }
        PanelRow::Button { .. } | PanelRow::Keys { .. } => Some(1.0),
        // set as on its card
        PanelRow::Control { .. } => None,
    }
}

/// `panel` with row `row` showing `value` (as [`touched`] gave it), so it shows the touch at once,
/// before the plugin answers.
pub(crate) fn with_value(mut panel: PluginPanel, row: usize, value: f32) -> PluginPanel {
    match panel.rows.get_mut(row) {
        Some(PanelRow::Slider { value: shown, .. }) => *shown = value,
        Some(PanelRow::Switch { on, .. }) => *on = value >= 0.5,
        Some(PanelRow::Choice { chosen, .. }) => *chosen = value as u32,
        Some(PanelRow::Colours { chosen, .. } | PanelRow::Brushes { chosen, .. }) => *chosen = Some(value as u32),
        _ => {}
    }
    panel
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALFTONE: &str = r#"{ "title": "Halftone", "rows": [
        { "type": "slider", "label": "Dot size", "least": 1, "most": 50, "step": 1, "value": 80, "unit": " px" },
        { "type": "switch", "label": "Round dots", "on": true },
        { "type": "choice", "label": "Shape", "options": ["Dots", "Lines"], "chosen": 1 },
        { "type": "text", "text": "Works on the current layer.\nOne undo takes it back." },
        { "type": "button", "label": "Apply" }
    ] }"#;

    #[test]
    fn a_panel_is_read_its_numbers_put_right() {
        let panel = read_panel(HALFTONE).unwrap();
        assert_eq!(panel.title, "Halftone");
        assert_eq!(panel.rows.len(), 5);
        // a value past the slider's end shows at its end
        assert!(matches!(panel.rows[0], PanelRow::Slider { value, .. } if value == 50.0));
        assert_eq!(
            panel.rows[2],
            PanelRow::Choice { label: "Shape".into(), options: vec!["Dots".into(), "Lines".into()], chosen: 1 }
        );
        assert_eq!(panel.rows[4], PanelRow::Button { label: "Apply".into() });
    }

    #[test]
    fn a_panel_that_doesn_t_hold_up_is_none() {
        let one = |row: &str| read_panel(&format!(r#"{{ "title": "T", "rows": [{row}] }}"#));
        assert!(one(r#"{ "type": "button", "label": "Go" }"#).is_some());
        // no rows, too many, a kind this app doesn't have, a field it doesn't know
        assert!(read_panel(r#"{ "title": "T", "rows": [] }"#).is_none());
        let many = vec![r#"{ "type": "button", "label": "Go" }"#; 17].join(",");
        assert!(read_panel(&format!(r#"{{ "title": "T", "rows": [{many}] }}"#)).is_none());
        assert!(one(r#"{ "type": "web", "label": "Go" }"#).is_none());
        assert!(one(r#"{ "type": "button", "label": "Go", "script": "x" }"#).is_none());
        // words that don't show, numbers that make no sense
        assert!(one(r#"{ "type": "button", "label": " " }"#).is_none());
        assert!(one(r#"{ "type": "switch", "label": "a\u0007b" }"#).is_none());
        assert!(one(r#"{ "type": "slider", "label": "S", "least": 5, "most": 5, "value": 5 }"#).is_none());
        assert!(one(r#"{ "type": "slider", "label": "S", "least": 0, "most": 1, "step": -1, "value": 0 }"#).is_none());
        assert!(one(r#"{ "type": "choice", "label": "C", "options": [], "chosen": 0 }"#).is_none());
        assert!(one(r#"{ "type": "choice", "label": "C", "options": ["A"], "chosen": 1 }"#).is_none());
        assert!(read_panel(r#"{ "title": "", "rows": [{ "type": "button", "label": "Go" }] }"#).is_none());
        assert!(read_panel("not json").is_none());
    }

    #[test]
    fn a_row_of_colours_is_read_and_a_tap_on_one_is_told_as_its_place() {
        let one = |row: &str| read_panel(&format!(r#"{{ "title": "T", "rows": [{row}] }}"#));
        let panel =
            one(r#"{ "type": "colours", "label": "Colour", "colours": [16711680, 255], "chosen": 1 }"#).unwrap();
        let row = PanelRow::Colours { label: "Colour".into(), colours: vec![0xFF0000, 0x0000FF], chosen: Some(1) };
        assert_eq!(panel.rows, [row]);
        // none chosen
        let none = one(r#"{ "type": "colours", "label": "Colour", "colours": [0] }"#).unwrap();
        assert!(matches!(none.rows[0], PanelRow::Colours { chosen: None, .. }));
        // none, more than eight, one that isn't a colour, a choice past them: it doesn't hold up
        assert!(one(r#"{ "type": "colours", "label": "C", "colours": [] }"#).is_none());
        let nine = ["1"; 9].join(",");
        assert!(one(&format!(r#"{{ "type": "colours", "label": "C", "colours": [{nine}] }}"#)).is_none());
        assert!(one(r#"{ "type": "colours", "label": "C", "colours": [16777216] }"#).is_none());
        assert!(one(r#"{ "type": "colours", "label": "C", "colours": [-1] }"#).is_none());
        assert!(one(r#"{ "type": "colours", "label": "C", "colours": [1, 2], "chosen": 2 }"#).is_none());
        // a tap is its place, and shows chosen at once
        assert_eq!(touched(&none, 0, 0.0), Some(0.0));
        assert_eq!(touched(&panel, 0, 1.2), Some(1.0));
        assert_eq!((touched(&panel, 0, 2.0), touched(&panel, 0, -1.0)), (None, None));
        assert!(matches!(with_value(none, 0, 0.0).rows[0], PanelRow::Colours { chosen: Some(0), .. }));
    }

    /// A brush for each key past 0 (where it is: `key - 1` in set 0), named by it.
    fn brush(key: i64) -> Option<PanelBrush> {
        (key > 0).then(|| PanelBrush { set: 0, index: (key - 1) as u32, name: format!("Brush {key}") })
    }

    #[test]
    fn a_row_of_brushes_is_found_by_their_keys_and_a_tap_on_one_is_told_as_its_place() {
        let one = |row: &str| read_panel_with(&format!(r#"{{ "title": "T", "rows": [{row}] }}"#), &brush, &|_| None);
        let panel = one(r#"{ "type": "brushes", "label": "Brushes", "brushes": [3, 1], "chosen": 1 }"#).unwrap();
        let found = vec![
            PanelBrush { set: 0, index: 2, name: "Brush 3".into() },
            PanelBrush { set: 0, index: 0, name: "Brush 1".into() },
        ];
        assert_eq!(panel.rows, [PanelRow::Brushes { label: "Brushes".into(), brushes: found, chosen: Some(1) }]);
        assert!(!panel.ring && panel.at.is_none());
        // a key that is no brush, none, more than eight, a choice past them: it doesn't hold up
        assert!(one(r#"{ "type": "brushes", "label": "B", "brushes": [1, 0] }"#).is_none());
        assert!(one(r#"{ "type": "brushes", "label": "B", "brushes": [] }"#).is_none());
        let nine = ["1"; 9].join(",");
        assert!(one(&format!(r#"{{ "type": "brushes", "label": "B", "brushes": [{nine}] }}"#)).is_none());
        assert!(one(r#"{ "type": "brushes", "label": "B", "brushes": [1], "chosen": 1 }"#).is_none());
        // read with nothing to find brushes by, none is found
        assert!(
            read_panel(r#"{ "title": "T", "rows": [{ "type": "brushes", "label": "B", "brushes": [1] }] }"#).is_none()
        );
        // a tap is its place, and shows chosen at once
        assert_eq!((touched(&panel, 0, 0.2), touched(&panel, 0, 2.0)), (Some(0.0), None));
        assert!(matches!(with_value(panel, 0, 0.0).rows[0], PanelRow::Brushes { chosen: Some(0), .. }));
    }

    #[test]
    fn a_ring_holds_rows_of_colours_and_brushes_alone_up_to_twelve_on_each() {
        let ring = |rows: &str| {
            read_panel_with(&format!(r#"{{ "title": "T", "ring": true, "rows": [{rows}] }}"#), &brush, &|_| None)
        };
        let twelve = (1..=12).map(|k| k.to_string()).collect::<Vec<_>>().join(",");
        let colours = format!(r#"{{ "type": "colours", "label": "C", "colours": [{twelve}], "chosen": 11 }}"#);
        let brushes = format!(r#"{{ "type": "brushes", "label": "B", "brushes": [{twelve}] }}"#);
        let panel = ring(&format!("{colours},{brushes}")).unwrap();
        assert!(panel.ring && panel.at.is_none());
        assert!(matches!(&panel.rows[0], PanelRow::Colours { colours, chosen: Some(11), .. } if colours.len() == 12));
        assert!(matches!(&panel.rows[1], PanelRow::Brushes { brushes, .. } if brushes.len() == 12));
        // twelve is past a card's eight
        let card = read_panel_with(&format!(r#"{{ "title": "T", "rows": [{colours}] }}"#), &brush, &|_| None);
        assert!(card.is_none());
        // thirteen, a row of another kind, more than three rows: it doesn't hold up
        assert!(ring(&format!(r#"{{ "type": "colours", "label": "C", "colours": [{twelve},13] }}"#)).is_none());
        assert!(ring(&format!(r#"{colours},{{ "type": "button", "label": "Go" }}"#)).is_none());
        assert!(ring(&[colours.as_str(); 4].join(",")).is_none());
        assert!(ring(&[colours.as_str(); 3].join(",")).is_some());
        assert!(ring("").is_none());
    }

    #[test]
    fn a_row_of_keys_is_written_for_the_platform_and_a_panel_may_ask_for_a_key() {
        let json = r#"{ "title": "Undo", "key_press": true, "rows": [
            { "type": "keys", "label": "Now", "keys": ["command+shift+z", "alt+["] },
            { "type": "keys", "label": "None", "keys": [] }
        ] }"#;
        let panel = read_panel(json).unwrap();
        assert!(panel.key_press);
        let written = if crate::keys::MAC { "⇧⌘Z  ⌥[" } else { "Ctrl+Shift+Z  Alt+[" };
        assert_eq!(panel.rows[0], PanelRow::Keys { label: "Now".into(), keys: written.into() });
        assert_eq!(panel.rows[1], PanelRow::Keys { label: "None".into(), keys: "None".into() });
        assert_eq!(touched(&panel, 0, 0.0), Some(1.0), "tapped as a button is");
        // a panel asks for no key unless it says so; a press that isn't one doesn't hold up
        assert!(!read_panel(HALFTONE).unwrap().key_press);
        let one = |keys: &str| {
            read_panel(&format!(r#"{{ "title": "T", "rows": [{{ "type": "keys", "label": "K", "keys": {keys} }}] }}"#))
        };
        assert!(one(r#"["command+z"]"#).is_some());
        assert!(one(r#"["ctrl+z"]"#).is_none());
        assert!(one(r#"["Z"]"#).is_none());
        assert!(one(&format!("[{}]", [r#""a""#; 9].join(","))).is_none(), "nine presses");
    }

    #[test]
    fn a_touch_is_told_as_its_row_takes_it() {
        let panel = read_panel(HALFTONE).unwrap();
        // on the slider's steps and in its range
        assert_eq!(touched(&panel, 0, 7.4), Some(7.0));
        assert_eq!(touched(&panel, 0, 99.0), Some(50.0));
        assert_eq!(touched(&panel, 1, 0.2), Some(0.0));
        assert_eq!(touched(&panel, 2, 1.0), Some(1.0));
        assert_eq!(touched(&panel, 2, 2.0), None, "no third shape");
        assert_eq!(touched(&panel, 3, 1.0), None, "words take no touch");
        assert_eq!(touched(&panel, 4, 0.0), Some(1.0));
        assert_eq!(touched(&panel, 9, 1.0), None);
        assert_eq!(touched(&panel, 0, f32::NAN), None);
        // and shows at once
        let shown = with_value(panel, 1, 0.0);
        assert_eq!(shown.rows[1], PanelRow::Switch { label: "Round dots".into(), on: false });
    }
}
