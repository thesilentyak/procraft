//! The app's functions a plugin's actions call, each as its permissions give: the artwork's size,
//! layers and pixels (`artwork.read`), adding layers, naming them and painting on them
//! (`artwork.write`), the brush (`brush`), a short message (`notice`) and a panel of its own
//! (`panel`, see [`Panel`]). They answer only while an action runs, or a touch on its panel is
//! told; everything one of those changes is one step undo takes back.
//!
//! A module imports only the functions it calls, so a plugin asks for the permissions of those.

/// A layer, by the id the app gives it (never 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Layer(pub i64);

impl Layer {
    fn of(id: i64) -> Option<Layer> {
        (id > 0).then_some(Layer(id))
    }
}

/// The functions as the app hands them over.
#[cfg(target_arch = "wasm32")]
mod raw {
    #[link(wasm_import_module = "procraft")]
    unsafe extern "C" {
        pub fn canvas_size() -> i64;
        pub fn layer_count() -> i32;
        pub fn layer_at(index: i32) -> i64;
        pub fn current_layer() -> i64;
        pub fn read_pixels(layer: i64, x: i32, y: i32, w: i32, h: i32, to: i32) -> i32;
        pub fn add_layer() -> i64;
        pub fn write_pixels(layer: i64, x: i32, y: i32, w: i32, h: i32, from: i32) -> i32;
        pub fn set_layer_name(layer: i64, at: i32, len: i32) -> i32;
        pub fn set_layer_opacity(layer: i64, opacity: f32) -> i32;
        pub fn set_layer_visible(layer: i64, on: i32) -> i32;
        pub fn layer_mark(layer: i64) -> i64;
        pub fn set_layer_mark(layer: i64, mark: i64) -> i32;
        pub fn brush_colour() -> i32;
        pub fn set_brush_colour(rgb: i32);
        pub fn brush_size() -> f32;
        pub fn set_brush_size(size: f32);
        pub fn brush_opacity() -> f32;
        pub fn set_brush_opacity(opacity: f32);
        pub fn recent_colour(at: i32) -> i32;
        pub fn recent_brush(at: i32) -> i64;
        pub fn current_brush() -> i64;
        pub fn choose_brush(key: i64) -> i32;
        pub fn notice(at: i32, len: i32);
        pub fn show_panel(at: i32, len: i32) -> i32;
        pub fn close_panel();
    }
}

/// Off the app (a plugin's own tests on the computer) there is no artwork: each answers as if none
/// were open.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::missing_safety_doc)]
mod raw {
    pub unsafe fn canvas_size() -> i64 {
        0
    }
    pub unsafe fn layer_count() -> i32 {
        0
    }
    pub unsafe fn layer_at(_: i32) -> i64 {
        0
    }
    pub unsafe fn current_layer() -> i64 {
        0
    }
    pub unsafe fn read_pixels(_: i64, _: i32, _: i32, _: i32, _: i32, _: i32) -> i32 {
        0
    }
    pub unsafe fn add_layer() -> i64 {
        0
    }
    pub unsafe fn write_pixels(_: i64, _: i32, _: i32, _: i32, _: i32, _: i32) -> i32 {
        0
    }
    pub unsafe fn set_layer_name(_: i64, _: i32, _: i32) -> i32 {
        0
    }
    pub unsafe fn set_layer_opacity(_: i64, _: f32) -> i32 {
        0
    }
    pub unsafe fn set_layer_visible(_: i64, _: i32) -> i32 {
        0
    }
    pub unsafe fn layer_mark(_: i64) -> i64 {
        0
    }
    pub unsafe fn set_layer_mark(_: i64, _: i64) -> i32 {
        0
    }
    pub unsafe fn brush_colour() -> i32 {
        0
    }
    pub unsafe fn set_brush_colour(_: i32) {}
    pub unsafe fn brush_size() -> f32 {
        0.0
    }
    pub unsafe fn set_brush_size(_: f32) {}
    pub unsafe fn brush_opacity() -> f32 {
        0.0
    }
    pub unsafe fn set_brush_opacity(_: f32) {}
    pub unsafe fn recent_colour(_: i32) -> i32 {
        -1
    }
    pub unsafe fn recent_brush(_: i32) -> i64 {
        0
    }
    pub unsafe fn current_brush() -> i64 {
        0
    }
    pub unsafe fn choose_brush(_: i64) -> i32 {
        0
    }
    pub unsafe fn notice(_: i32, _: i32) {}
    pub unsafe fn show_panel(_: i32, _: i32) -> i32 {
        0
    }
    pub unsafe fn close_panel() {}
}

/// Where `bytes` are in the module's memory, as the app reads it.
fn at(bytes: &[u8]) -> i32 {
    bytes.as_ptr() as usize as i32
}

/// The canvas's size, pixels: (0, 0) with no artwork.
pub fn canvas_size() -> (u32, u32) {
    let both = unsafe { raw::canvas_size() } as u64;
    (both as u32, (both >> 32) as u32)
}

/// The layers, bottom first (groups are layers too).
pub fn layers() -> impl Iterator<Item = Layer> {
    let count = unsafe { raw::layer_count() };
    (0..count).filter_map(|index| Layer::of(unsafe { raw::layer_at(index) }))
}

/// The layer being painted on.
pub fn current_layer() -> Option<Layer> {
    Layer::of(unsafe { raw::current_layer() })
}

/// The pixels of `layer` in the rectangle at (`x`, `y`), `w` × `h`, premultiplied RGBA8 rows; none
/// if the rectangle isn't wholly on the canvas or the layer isn't there.
pub fn layer_pixels(layer: Layer, x: i32, y: i32, w: u32, h: u32) -> Option<Vec<u8>> {
    let mut to = vec![0_u8; w as usize * h as usize * 4];
    let read = unsafe { raw::read_pixels(layer.0, x, y, w as i32, h as i32, to.as_mut_ptr() as usize as i32) };
    (read != 0).then_some(to)
}

/// A new empty layer above the current one, made current; none if the canvas has no room.
pub fn add_layer() -> Option<Layer> {
    Layer::of(unsafe { raw::add_layer() })
}

/// `rgba` (premultiplied RGBA8 rows, `w` × `h`) written over `layer` at (`x`, `y`). False if the
/// rectangle isn't wholly on the canvas, `rgba` is the wrong size, or the layer can't be painted on
/// (it is locked, or isn't a pixel layer).
pub fn paint(layer: Layer, x: i32, y: i32, w: u32, h: u32, rgba: &[u8]) -> bool {
    rgba.len() == w as usize * h as usize * 4
        && unsafe { raw::write_pixels(layer.0, x, y, w as i32, h as i32, at(rgba)) } != 0
}

/// `layer` named `name`.
pub fn name_layer(layer: Layer, name: &str) -> bool {
    unsafe { raw::set_layer_name(layer.0, at(name.as_bytes()), name.len() as i32) != 0 }
}

/// `layer`'s opacity, 0 to 1.
pub fn set_layer_opacity(layer: Layer, opacity: f32) -> bool {
    unsafe { raw::set_layer_opacity(layer.0, opacity) != 0 }
}

/// `layer` shown or hidden.
pub fn show_layer(layer: Layer, on: bool) -> bool {
    unsafe { raw::set_layer_visible(layer.0, i32::from(on)) != 0 }
}

/// The number the plugin keeps with `layer` (`artwork.read`, interface 10): 0 for none.
pub fn layer_mark(layer: Layer) -> i64 {
    unsafe { raw::layer_mark(layer.0) }
}

/// `mark` kept with `layer` for the plugin, saved with the artwork (`artwork.write`, interface 10):
/// 0 takes it off. The `layer_label` hook is told it.
pub fn set_layer_mark(layer: Layer, mark: i64) -> bool {
    unsafe { raw::set_layer_mark(layer.0, mark) != 0 }
}

/// The brush's colour, 0xRRGGBB.
pub fn brush_colour() -> u32 {
    unsafe { raw::brush_colour() as u32 }
}

/// The brush's colour set, 0xRRGGBB.
pub fn set_brush_colour(rgb: u32) {
    unsafe { raw::set_brush_colour((rgb & 0xFF_FFFF) as i32) }
}

/// The current tool's size slider, 0 to 1.
pub fn brush_size() -> f32 {
    unsafe { raw::brush_size() }
}

pub fn set_brush_size(size: f32) {
    unsafe { raw::set_brush_size(size) }
}

/// The current tool's opacity slider, 0 to 1.
pub fn brush_opacity() -> f32 {
    unsafe { raw::brush_opacity() }
}

pub fn set_brush_opacity(opacity: f32) {
    unsafe { raw::set_brush_opacity(opacity) }
}

/// A brush, by the key the app gives it (interface 11): where it is in the library, so it holds while
/// the library stays as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Brush(pub i64);

/// The colour at `at` of the colour history (`brush`, interface 11), 0 the latest painted with,
/// 0xRRGGBB; none past its end.
pub fn recent_colour(at: usize) -> Option<u32> {
    let at = i32::try_from(at).ok()?;
    let rgb = unsafe { raw::recent_colour(at) };
    u32::try_from(rgb).ok()
}

/// The brush at `at` of the Recent set (`brush`, interface 11): its pinned brushes first, then the
/// latest painted with; none past its end.
pub fn recent_brush(at: usize) -> Option<Brush> {
    let at = i32::try_from(at).ok()?;
    let key = unsafe { raw::recent_brush(at) };
    (key != 0).then_some(Brush(key))
}

/// The brush the current tool uses (`brush`, interface 11).
pub fn current_brush() -> Option<Brush> {
    let key = unsafe { raw::current_brush() };
    (key != 0).then_some(Brush(key))
}

/// `brush` chosen for the current tool, as a tap in the library chooses it (`brush`, interface 11):
/// false if it isn't there any more.
pub fn choose_brush(brush: Brush) -> bool {
    unsafe { raw::choose_brush(brush.0) != 0 }
}

/// `words` shown for a moment, as the app's own short messages are.
pub fn say(words: &str) {
    unsafe { raw::notice(at(words.as_bytes()), words.len() as i32) }
}

/// A panel of the plugin's own, as it describes it: rows the app draws as its own cards' rows,
/// shown once the action (or the touch) that shows it ends. Its rows go by their place, from 0.
///
/// ```ignore
/// kit::app::Panel::new("Halftone")
///     .slider("Dot size", 1.0, 50.0, 1.0, size, " px")
///     .switch("Round dots", round)
///     .button("Apply")
///     .show();
/// ```
pub struct Panel {
    title: String,
    rows: Vec<String>,
    key_press: bool,
    ring: bool,
}

/// `text` as a JSON string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl Panel {
    pub fn new(title: &str) -> Panel {
        Panel { title: title.to_owned(), rows: Vec::new(), key_press: false, ring: false }
    }

    /// Words, a line or a few (`\n` between them).
    pub fn text(mut self, text: &str) -> Panel {
        self.rows.push(format!(r#"{{"type":"text","text":{}}}"#, quoted(text)));
        self
    }

    /// A slider from `least` to `most` in steps of `step` (0: none), at `value`, `unit` after it.
    pub fn slider(mut self, label: &str, least: f32, most: f32, step: f32, value: f32, unit: &str) -> Panel {
        self.rows.push(format!(
            r#"{{"type":"slider","label":{},"least":{least},"most":{most},"step":{step},"value":{value},"unit":{}}}"#,
            quoted(label),
            quoted(unit)
        ));
        self
    }

    pub fn switch(mut self, label: &str, on: bool) -> Panel {
        self.rows.push(format!(r#"{{"type":"switch","label":{},"on":{on}}}"#, quoted(label)));
        self
    }

    /// One of `options`, the one at `chosen` picked.
    pub fn choice(mut self, label: &str, options: &[&str], chosen: usize) -> Panel {
        let options: Vec<String> = options.iter().map(|o| quoted(o)).collect();
        self.rows.push(format!(
            r#"{{"type":"choice","label":{},"options":[{}],"chosen":{chosen}}}"#,
            quoted(label),
            options.join(",")
        ));
        self
    }

    /// A row of `colours` (0xRRGGBB, one to eight) to tap, the one at `chosen` ringed (interface
    /// 10): a tap is told as the colour's place.
    pub fn colours(mut self, label: &str, colours: &[u32], chosen: Option<usize>) -> Panel {
        let colours: Vec<String> = colours.iter().map(|c| (c & 0xFF_FFFF).to_string()).collect();
        let chosen = chosen.map(|at| format!(r#","chosen":{at}"#)).unwrap_or_default();
        self.rows.push(format!(
            r#"{{"type":"colours","label":{},"colours":[{}]{chosen}}}"#,
            quoted(label),
            colours.join(",")
        ));
        self
    }

    /// A row of `brushes` (one to eight, twelve on a ring) to tap, each shown by its sample stroke,
    /// the one at `chosen` ringed (interface 11): a tap is told as the brush's place.
    pub fn brushes(mut self, label: &str, brushes: &[Brush], chosen: Option<usize>) -> Panel {
        let brushes: Vec<String> = brushes.iter().map(|b| b.0.to_string()).collect();
        let chosen = chosen.map(|at| format!(r#","chosen":{at}"#)).unwrap_or_default();
        self.rows.push(format!(
            r#"{{"type":"brushes","label":{},"brushes":[{}]{chosen}}}"#,
            quoted(label),
            brushes.join(",")
        ));
        self
    }

    /// It shows as a ring round the point its action came from (interface 11): the pen whose
    /// button ran it, or the window's middle. Its rows, colours and brushes alone (one to three),
    /// are each a ring about it, the first innermost; it has no title or Done, and a tap off it
    /// closes it.
    pub fn ring(mut self) -> Panel {
        self.ring = true;
        self
    }

    /// A row that is tapped, its words in the accent colour.
    pub fn button(mut self, label: &str) -> Panel {
        self.rows.push(format!(r#"{{"type":"button","label":{}}}"#, quoted(label)));
        self
    }

    /// A row that is tapped, `presses` at its end written as the platform writes them ("Ctrl+Z", or
    /// "⌘Z"; "None" for none). Eight at most.
    pub fn keys(mut self, label: &str, presses: &[crate::keys::Press]) -> Panel {
        let presses: Vec<String> = presses.iter().map(|p| quoted(&p.text())).collect();
        self.rows.push(format!(r#"{{"type":"keys","label":{},"keys":[{}]}}"#, quoted(label), presses.join(",")));
        self
    }

    /// It asks for the next key press, which goes to the plugin's `procraft_panel_key` (see
    /// [`crate::panel_key!`]) rather than to the canvas.
    pub fn key_press(mut self) -> Panel {
        self.key_press = true;
        self
    }

    /// The panel as the app reads it.
    pub fn json(&self) -> String {
        let asks = if self.key_press { r#","key_press":true"# } else { "" };
        let ring = if self.ring { r#","ring":true"# } else { "" };
        format!(r#"{{"title":{},"rows":[{}]{asks}{ring}}}"#, quoted(&self.title), self.rows.join(","))
    }

    /// Shows it (or shows it again, changed); false if the app found it doesn't hold up (no rows, too
    /// many, words that don't show).
    pub fn show(&self) -> bool {
        let json = self.json();
        unsafe { raw::show_panel(at(json.as_bytes()), json.len() as i32) != 0 }
    }
}

/// The plugin's panel closed.
pub fn close_panel() {
    unsafe { raw::close_panel() }
}

/// Exports `procraft_panel_event`: a touch on the plugin's panel calls `$touched` (a
/// `fn(row: usize, value: f32)`) with the row's place and its value (a slider's, a switch's 1 or 0,
/// a choice's place, 1 for a button).
#[macro_export]
macro_rules! panel {
    ($touched:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn procraft_panel_event(row: i32, value: f32) -> i32 {
            let touched: fn(usize, f32) = $touched;
            touched(row.max(0) as usize, value);
            0
        }
    };
}

/// Exports `procraft_action`: tapping the plugin's action at place `i` in its manifest's `actions`
/// calls `$act` (a `fn(usize)`) with `i`.
#[macro_export]
macro_rules! actions {
    ($act:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn procraft_action(index: i32) -> i32 {
            let act: fn(usize) = $act;
            act(index.max(0) as usize);
            0
        }
    };
}

/// What a plugin on the `stroke_kept` or `layer_added` hook is told of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A stroke was kept.
    StrokeKept,
    /// A layer was added from the Layers panel.
    LayerAdded,
}

/// Exports `procraft_event`: each event the plugin's hooks tell of calls `$told` (a `fn(Event)`),
/// which acts on the artwork as an action does (and should be quick).
#[macro_export]
macro_rules! events {
    ($told:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn procraft_event(kind: i32) -> i32 {
            let told: fn($crate::app::Event) = $told;
            match kind {
                1 => told($crate::app::Event::StrokeKept),
                2 => told($crate::app::Event::LayerAdded),
                _ => {}
            }
            0
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_is_written_as_the_app_reads_it() {
        let panel = Panel::new("Half \"tone\"")
            .slider("Dot size", 1.0, 50.0, 1.0, 8.0, " px")
            .switch("Round", true)
            .choice("Shape", &["Dots", "Lines"], 1)
            .text("One\nTwo")
            .button("Apply");
        let json = panel.json();
        assert!(json.starts_with(r#"{"title":"Half \"tone\"","rows":[{"type":"slider","label":"Dot size","least":1,"#));
        assert!(json.contains(r#"{"type":"switch","label":"Round","on":true}"#));
        assert!(json.contains(r#""options":["Dots","Lines"],"chosen":1"#));
        assert!(json.contains(r#"{"type":"text","text":"One\nTwo"}"#));
        assert!(json.ends_with(r#"{"type":"button","label":"Apply"}]}"#));
        // off the app, nothing shows
        assert!(!panel.show());
        let undo = crate::keys::Press::read("command+z").unwrap();
        let asking = Panel::new("Undo").keys("Now", &[undo]).keys("Default", &[]).key_press();
        assert_eq!(
            asking.json(),
            r#"{"title":"Undo","rows":[{"type":"keys","label":"Now","keys":["command+z"]},{"type":"keys","label":"Default","keys":[]}],"key_press":true}"#
        );
        let ring = Panel::new("Palette")
            .colours("Colours", &[0xFF0000], None)
            .brushes("Brushes", &[Brush(1 << 32), Brush(7)], Some(1))
            .ring();
        assert_eq!(
            ring.json(),
            r#"{"title":"Palette","rows":[{"type":"colours","label":"Colours","colours":[16711680]},{"type":"brushes","label":"Brushes","brushes":[4294967296,7],"chosen":1}],"ring":true}"#
        );
        // off the app, nothing is recent and no brush is in use
        assert_eq!((recent_colour(0), recent_brush(0), current_brush()), (None, None, None));
        assert!(!choose_brush(Brush(1 << 32)));
    }
}
