//! The brush library: its sets and brushes, choosing the brush for the current tool, and preview
//! strokes for the library panel.

use procraft_engine::document::Document;
use procraft_engine::geom::Vec2;
use procraft_engine::input::Pointer;
use procraft_engine::preview;
use procraft_engine::stroke::{InputSettings, LEAST_ROUNDNESS, MOST_ROUNDNESS, Stamp, StrokeBuilder};
use procraft_gpu::{Canvas, StrokeStyle, StrokeTool};

use crate::CanvasSession;
use crate::layers::ThumbnailImage;

/// The brush cursor's fill while the pen hovers ([`CanvasSession::hover_fill`]): how big, a share
/// of the cursor's diameter, and how strong, a share of the opacity painted with.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct HoverFill {
    pub size: f32,
    pub opacity: f32,
}

/// Where a shape's outline is traced: where its paint is a quarter strong.
const OUTLINE_LEVEL: f32 = 0.25;

/// The last brush stamp pictured, by what it was made from (shape image, own image, hardness,
/// roundness, rotation, size): the size preview asks each frame.
#[allow(clippy::type_complexity)]
static STAMPS: std::sync::Mutex<Option<((Option<u32>, Option<u64>, [u32; 3], usize), Vec<u8>)>> =
    std::sync::Mutex::new(None);

/// The last brush outline traced, by what it was traced from (shape image, own image, roundness,
/// rotation): a cursor asks each frame.
#[allow(clippy::type_complexity)]
static OUTLINES: std::sync::Mutex<Option<((Option<u32>, Option<u64>, u32, u32), Vec<Vec<f32>>)>> =
    std::sync::Mutex::new(None);

/// Held while a brush's sample stroke is drawn (see [`CanvasSession::brush_preview`]).
static PREVIEWS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Held by the brush studio's ask for its sample stroke while it waits for its turn and draws it
/// (see [`CanvasSession::studio_preview`]).
static STUDIO_PREVIEW_TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Whether a sample stroke the GPU couldn't draw has been said in the log.
static PREVIEW_FAILURE_SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What the canvas the sample strokes are drawn on may take of the GPU's memory, bytes, at its
/// most (its layers and stroke textures, its brushes' images, and the allocator's blocks for them:
/// a first set's strokes took 132 MB of a downlevel GPU's): with less than that and what the
/// system keeps free left, it isn't kept between strokes.
const PREVIEW_CANVAS_BYTES: u64 = 128 << 20;

/// Whether the sample strokes' canvas let go for want of memory has been said in the log.
static PREVIEW_SHORT_SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// How long the canvas the sample strokes are drawn on is kept after the last is drawn: through a
/// set looked through, its rows asking for their strokes a few at a time as they come into sight,
/// and no longer. Kept until the system said it was short (which, from Android 14, it never says to
/// an app in front), a heavy set's left 130 MB and more of the GPU's memory held while the artwork
/// stayed open, and a phone of 4 GB closed the app as its Gouache and Oils opened.
const PREVIEW_CANVAS_KEPT: std::time::Duration = std::time::Duration::from_secs(2);

/// What each buffer the sample strokes' canvas grew for a stroke's work may keep of it for the
/// next, bytes (see `Canvas::let_go_of_work_past`): a light brush's stroke needs no more, and a wet
/// one's left eight lists of tiles of 8 MB.
const PREVIEW_WORK_KEPT: u64 = 1 << 20;

/// Milliseconds into the run (from the first time this is asked).
fn run_ms() -> u64 {
    static STARTED: std::sync::LazyLock<std::time::Instant> = std::sync::LazyLock::new(std::time::Instant::now);
    u64::try_from(STARTED.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// How often the thread waiting to let go of the sample strokes' canvas looks again while one is
/// waiting for its turn or being drawn.
const PREVIEW_PENDING_LOOK: std::time::Duration = std::time::Duration::from_millis(250);

/// A sample stroke asked for, from its wait for its turn to its end: the canvas the strokes are
/// drawn on stays while any is (another's stroke drawn first took seconds on a weak GPU), and
/// goes [`PREVIEW_CANVAS_KEPT`] after the last has ended (see [`watch_previews`]).
struct PreviewPending<'a>(&'a crate::Shared);

impl<'a> PreviewPending<'a> {
    fn new(shared: &'a crate::Shared) -> PreviewPending<'a> {
        shared.previews_pending.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        PreviewPending(shared)
    }
}

impl Drop for PreviewPending<'_> {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering::SeqCst;
        self.0.preview_drawn_at.store(run_ms(), SeqCst);
        self.0.previews_drawn.fetch_add(1, SeqCst);
        self.0.previews_pending.fetch_sub(1, SeqCst);
    }
}

/// Lets go of the canvas the sample strokes are drawn on, kept for the next, once none has been
/// asked for or drawn for [`PREVIEW_CANVAS_KEPT`]: one thread a session waits for that, as long as
/// strokes are drawn now and then.
fn watch_previews(shared: &std::sync::Arc<crate::Shared>) {
    use std::sync::atomic::Ordering::SeqCst;
    // (counted before the thread is asked for: one ending as this comes sees it, below)
    shared.previews_drawn.fetch_add(1, SeqCst);
    if shared.previews_watched.swap(true, SeqCst) {
        return;
    }
    let weak = std::sync::Arc::downgrade(shared);
    let kept = u64::try_from(PREVIEW_CANVAS_KEPT.as_millis()).unwrap_or(u64::MAX);
    let watch = move || {
        loop {
            let Some(shared) = weak.upgrade() else { return };
            let seen = shared.previews_drawn.load(SeqCst);
            let due = shared.preview_drawn_at.load(SeqCst).saturating_add(kept);
            let now = run_ms();
            let wait = if shared.previews_pending.load(SeqCst) > 0 {
                Some(PREVIEW_PENDING_LOOK)
            } else {
                (now < due).then(|| std::time::Duration::from_millis(due - now))
            };
            if let Some(wait) = wait {
                drop(shared);
                std::thread::sleep(wait);
                continue;
            }
            // none asked for since, as the lock it is kept under says (one drawn holds it till
            // then, and is still asked for as it is kept)
            let canvas = {
                let mut st = shared.lock();
                let idle = shared.previews_drawn.load(SeqCst) == seen && shared.previews_pending.load(SeqCst) == 0;
                if !idle {
                    continue;
                }
                st.preview.take()
            };
            drop(canvas);
            shared.previews_watched.store(false, SeqCst);
            // one kept meanwhile, which found this still watching, is watched on here
            if shared.previews_drawn.load(SeqCst) == seen || shared.previews_watched.swap(true, SeqCst) {
                return;
            }
        }
    };
    let thread = std::thread::Builder::new().name("procraft-previews-kept".to_owned());
    if let Err(e) = thread.spawn(watch) {
        shared.previews_watched.store(false, SeqCst);
        log::warn!("the sample strokes' canvas is kept until the library closes: {e}");
    }
}

/// Draws the pictures `brushes` stand in with that are still to be drawn (each brush's shape and
/// grain, and its second's), on a thread of their own: a brush chosen is ready by its first stroke,
/// which otherwise waits for them (see [`pictures_to_draw`]). What a stroke or preview wants first
/// is drawn once, by whichever gets to it first.
pub(crate) fn warm_pictures<'a>(brushes: impl IntoIterator<Item = &'a procraft_engine::brush::Brush>) {
    let waiting = undrawn(brushes);
    if waiting.is_empty() {
        return;
    }
    let thread = std::thread::Builder::new().name("procraft-brush-pictures".to_owned());
    if let Err(e) = thread.spawn(move || draw_side_by_side(&waiting)) {
        log::warn!("drawing a brush's pictures ahead: {e}");
    }
}

/// Draws `brush`'s pictures still to be drawn now, side by side (see [`warm_pictures`]).
pub(crate) fn draw_pictures(brush: &procraft_engine::brush::Brush) {
    draw_side_by_side(&undrawn([brush]));
}

/// `brush`'s pictures still to be drawn (its shape and grain, and its second's), to draw with
/// [`draw_these`] before the canvas is held: a stroke begun just after its brush was chosen drew
/// them under the canvas's lock, the frames held up meanwhile (35 to 140 ms for a dual brush's
/// four).
pub(crate) fn pictures_to_draw(brush: &procraft_engine::brush::Brush) -> Vec<procraft_engine::brush::Pixels> {
    undrawn([brush])
}

/// Draws `pictures` now, side by side (each once, whoever else draws it meanwhile).
pub(crate) fn draw_these(pictures: &[procraft_engine::brush::Pixels]) {
    if !pictures.is_empty() {
        draw_side_by_side(pictures);
    }
}

/// The pictures of `brushes` (each one's shape and grain, and its second's) still to be drawn.
fn undrawn<'a>(
    brushes: impl IntoIterator<Item = &'a procraft_engine::brush::Brush>,
) -> Vec<procraft_engine::brush::Pixels> {
    brushes
        .into_iter()
        .flat_map(|b| [Some(b), b.secondary.as_deref()])
        .flatten()
        .flat_map(|b| [&b.shape.source, &b.grain.source])
        .flatten()
        .filter(|s| !s.pixels.is_drawn())
        .map(|s| s.pixels.clone())
        .collect()
}

/// Draws `pictures` side by side, as many at once as there are cores and memory for (see
/// `procraft_engine::par`), one at a time where the system is short of memory (an old phone's, or
/// a 32-bit build's): each holds a few pictures' worth while it is drawn.
fn draw_side_by_side(pictures: &[procraft_engine::brush::Pixels]) {
    if pictures.len() <= 1 || crate::device::short_of_memory(PICTURES_AT_ONCE) {
        for p in pictures {
            drop(p.shared());
        }
        return;
    }
    procraft_engine::par::side_by_side_within(pictures, PICTURE_WORK, |p| drop(p.shared()));
}

/// About what a brush's picture holds while it is drawn (its pictures in floats, and the image it
/// is made from), bytes; and what drawing a few at once takes, which a system short of it draws
/// one at a time.
const PICTURE_WORK: usize = 16 << 20;
const PICTURES_AT_ONCE: u64 = 64 << 20;

impl crate::State {
    /// The tools' brushes' pictures drawn ahead (see [`warm_pictures`]).
    pub(crate) fn warm_tools_brushes(&self) {
        use crate::ToolKind;
        warm_pictures([ToolKind::Paint, ToolKind::Smudge, ToolKind::Erase].map(|tool| self.brush(tool)));
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct BrushInfo {
    pub set: u32,
    pub index: u32,
    pub name: String,
}

/// A brush in the Recent set: where it is in the library, and whether it is pinned there.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RecentBrush {
    pub set: u32,
    pub index: u32,
    pub name: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct BrushSetInfo {
    pub name: String,
    pub brushes: Vec<BrushInfo>,
    /// Which of the set icons it shows.
    pub icon: u32,
    /// The brush last chosen in it, if one has been.
    pub chosen: Option<u32>,
}

/// How many of the library's sample strokes, each `width` x `height` at four bytes a pixel, the app
/// keeps as pictures on a device of `memory` bytes (its last drawn; a row showing one let go draws
/// it again from the stroke the core keeps packed): a 64th of the memory's worth, at least 48 (a
/// few screens of rows) and at most 400 (more than the stock library). Unbounded, every set looked
/// through held all of them, some 100 MB of pictures on a tablet of 2 or 3 GB.
#[uniffi::export]
pub fn brush_previews_kept(memory: u64, width: u32, height: u32) -> u32 {
    let each = u64::from(width.max(1)) * u64::from(height.max(1)) * 4;
    u32::try_from(memory / 64 / each).unwrap_or(u32::MAX).clamp(48, 400)
}

/// How near a slider value pulls onto a notch, and how many notches a slider holds.
pub const NOTCH_NEAR: f32 = 0.04;
pub const NOTCH_MOST: usize = 4;

#[uniffi::export]
impl CanvasSession {
    pub fn brush_library(&self) -> Vec<BrushSetInfo> {
        let st = self.shared.lock();
        st.library
            .iter()
            .enumerate()
            .map(|(s, set)| BrushSetInfo {
                name: set.name.to_owned(),
                icon: set.icon,
                chosen: st.set_choice(s).map(|i| i as u32),
                brushes: set
                    .brushes
                    .iter()
                    .enumerate()
                    .map(|(i, b)| BrushInfo { set: s as u32, index: i as u32, name: b.name.clone() })
                    .collect(),
            })
            .collect()
    }

    /// The brush the current tool uses.
    pub fn current_brush(&self) -> BrushInfo {
        let st = self.shared.lock();
        let (set, index) = st.brush_ref(st.tool);
        BrushInfo { set: set as u32, index: index as u32, name: st.brush(st.tool).name.clone() }
    }

    /// The current tool's brush's own outline, for a brush cursor in its shape: loops of x, y
    /// pairs in brush diameters about its middle (y down, as on the screen), squashed across by
    /// its roundness and turned by its fixed rotation; none for the round tip (a circle).
    pub fn brush_outline(&self) -> Vec<Vec<f32>> {
        let (key, pixels) = {
            let st = self.shared.lock();
            let shape = &st.brush(st.tool).shape;
            let key =
                (shape.image, shape.source.as_ref().map(|s| s.id), shape.roundness.to_bits(), shape.rotation.to_bits());
            let pixels = shape.source.as_ref().map(|s| s.pixels.clone());
            (key, pixels)
        };
        let (image, source, roundness, rotation) = key;
        if image.is_none() && source.is_none() {
            return Vec::new();
        }
        let mut kept = OUTLINES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((k, loops)) = kept.as_ref()
            && *k == key
        {
            return loops.clone();
        }
        let side = procraft_engine::textures::TEXTURE_SIZE;
        let traced = match (pixels, image) {
            (Some(own), _) => procraft_engine::outline::shape_outline(&own, side, OUTLINE_LEVEL),
            (None, Some(i)) => procraft_engine::textures::shape_image(i)
                .map(|shape| procraft_engine::outline::shape_outline(&shape.pixels, side, OUTLINE_LEVEL))
                .unwrap_or_default(),
            (None, None) => Vec::new(),
        };
        // as a stamp is drawn: across its full size, squashed across, turned on the screen
        let (roundness, turn) = (f32::from_bits(roundness), f32::from_bits(rotation));
        let roundness = if roundness.is_finite() { roundness.clamp(LEAST_ROUNDNESS, MOST_ROUNDNESS) } else { 1.0 };
        let (sin, cos) = if turn.is_finite() { turn.sin_cos() } else { (0.0, 1.0) };
        let loops: Vec<Vec<f32>> = traced
            .iter()
            .map(|ring| {
                ring.iter()
                    .flat_map(|p| {
                        let (x, y) = (p.x - 0.5, (p.y - 0.5) * roundness);
                        [x * cos - y * sin, x * sin + y * cos]
                    })
                    .collect()
            })
            .collect();
        *kept = Some((key, loops.clone()));
        loops
    }

    /// What the brush cursor's fill shows while the pen hovers, if the current tool's brush's
    /// Hover fill says to fill it: the stamp as it would paint at the brush's estimated pressure
    /// (its Hover setting), how big (a share of the cursor's diameter) and how strong (a share of
    /// the opacity slider's). A brush whose size or opacity follows the pressure fills smaller or
    /// fainter there; one that follows neither, all of the cursor at the opacity painted with.
    pub fn hover_fill(&self) -> Option<HoverFill> {
        let st = self.shared.lock();
        let brush = st.brush(st.tool);
        if brush.setting("hover_fill").round() < 1.0 {
            return None;
        }
        let pressure = brush.setting("hover_pressure");
        let pressure = if pressure.is_finite() { pressure.clamp(0.01, 1.0) } else { 0.44 };
        let [size, opacity, ..] = brush.pressures(pressure);
        let (size, opacity) = brush.factors(size, opacity);
        // the size goes down to the brush's least, not to nothing (as a stroke's stamps do)
        let diameter = brush.diameter(st.shown_sliders().size);
        let least = (brush.min_size.min(brush.biggest()) / diameter.max(0.5)).clamp(0.0, 1.0);
        let size = least + (1.0 - least) * size.clamp(0.0, 1.0);
        let fill = HoverFill { size, opacity: opacity.clamp(0.0, 1.0) };
        (fill.size.is_finite() && fill.opacity.is_finite()).then_some(fill)
    }

    /// The current tool's brush's stamp as it paints, `side` x `side` pixels of coverage (0..255):
    /// its shape image, or the round tip by its hardness, squashed across by its roundness and
    /// turned by its fixed rotation, filling the square as a stamp fills its diameter. For the
    /// size slider's preview.
    pub fn brush_stamp(&self, side: u32) -> Vec<u8> {
        let side = side.clamp(1, 1024) as usize;
        let (key, source) = {
            let st = self.shared.lock();
            let brush = st.brush(st.tool);
            let shape = &brush.shape;
            let key = (
                shape.image,
                shape.source.as_ref().map(|s| s.id),
                [brush.hardness, shape.roundness, shape.rotation].map(f32::to_bits),
                side,
            );
            (key, shape.source.as_ref().map(|s| s.pixels.clone()))
        };
        let mut kept = STAMPS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((k, stamp)) = kept.as_ref()
            && *k == key
        {
            return stamp.clone();
        }
        let (image, _, [hardness, roundness, turn], _) = key;
        let (hardness, roundness, turn) = (f32::from_bits(hardness), f32::from_bits(roundness), f32::from_bits(turn));
        let pixels = match (source, image) {
            (Some(own), _) => Some(own.to_vec()),
            (None, Some(i)) => procraft_engine::textures::shape_image(i).map(|s| s.pixels),
            (None, None) => None,
        };
        let roundness = if roundness.is_finite() { roundness.clamp(LEAST_ROUNDNESS, MOST_ROUNDNESS) } else { 1.0 };
        let (sin, cos) = if turn.is_finite() { turn.sin_cos() } else { (0.0, 1.0) };
        let size = procraft_engine::textures::TEXTURE_SIZE;
        // averaged down to about the picture's size first, so a small picture keeps fine detail's
        // weight rather than catching a few of its pixels
        let (picture, n) = match &pixels {
            Some(p) if p.len() >= size * size => {
                let mut at = (p.clone(), size);
                while at.1 / 2 >= side.max(1) && at.1 > 1 {
                    at = (halved(&at.0, at.1), at.1 / 2);
                }
                (Some(at.0), at.1)
            }
            _ => (None, 0),
        };
        let r = side as f32 / 2.0;
        let mut out = vec![0u8; side * side];
        for (i, o) in out.iter_mut().enumerate() {
            let (px, py) = ((i % side) as f32 + 0.5 - r, (i / side) as f32 + 0.5 - r);
            // back into the stamp's own frame: turned back, then unsquashed across
            let (x, y) = (px * cos + py * sin, (-px * sin + py * cos) / roundness);
            let cover = match &picture {
                Some(pic) => {
                    let (u, v) = ((x / side as f32 + 0.5) * n as f32 - 0.5, (y / side as f32 + 0.5) * n as f32 - 0.5);
                    bilinear(pic, n, u, v)
                }
                None => {
                    // the round tip as the stamp shader draws it: a one-pixel rim, softened from
                    // its hardness outwards
                    let d = x.hypot(y);
                    let rim = (r - d + 0.5).clamp(0.0, 1.0);
                    let t = ((d - hardness.min(0.999) * r) / (r + 0.5 - hardness.min(0.999) * r)).clamp(0.0, 1.0);
                    rim.min(1.0 - t * t * (3.0 - 2.0 * t))
                }
            };
            *o = (cover.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        *kept = Some((key, out.clone()));
        out
    }

    /// The current tool's brush's control `key`, as the studio shows it.
    pub fn current_brush_setting(&self, key: String) -> f32 {
        let st = self.shared.lock();
        st.brush(st.tool).setting(&key)
    }

    /// The current tool's brush's saved sizes (`size`) or opacities: its slider's notches.
    pub fn slider_notches(&self, size: bool) -> Vec<f32> {
        let st = self.shared.lock();
        st.brush(st.tool).notches.list(notch_tool(st.tool), size).to_vec()
    }

    /// Saves `value` on the current tool's size or opacity slider (its brush keeps it); false if
    /// the slider is full or a notch is there already.
    pub fn add_slider_notch(&self, size: bool, value: f32) -> bool {
        let mut st = self.shared.lock();
        let tool = st.tool;
        st.brush_mut(tool).is_some_and(|b| b.notches.add(notch_tool(tool), size, value))
    }

    /// Takes away the notch `value` is on, on the current tool's size or opacity slider.
    pub fn remove_slider_notch(&self, size: bool, value: f32) -> bool {
        let mut st = self.shared.lock();
        let tool = st.tool;
        st.brush_mut(tool).is_some_and(|b| b.notches.remove(notch_tool(tool), size, value))
    }

    /// The notch `value` is on (within [`NOTCH_NEAR`]), of the size or opacity slider, if any: a
    /// slider dragged there is pulled onto it.
    pub fn slider_notch_at(&self, size: bool, value: f32) -> Option<f32> {
        self.slider_notches(size)
            .into_iter()
            .filter(|n| (n - value).abs() < NOTCH_NEAR)
            .min_by(|a, b| (a - value).abs().total_cmp(&(b - value).abs()))
    }

    /// Whether the slider's panel offers to save its value: not when it is on a notch or the
    /// slider holds [`NOTCH_MOST`] already.
    pub fn can_save_slider_notch(&self, size: bool) -> bool {
        let info = self.info();
        let value = if size { info.size } else { info.opacity };
        self.slider_notch_at(size, value).is_none() && self.slider_notches(size).len() < NOTCH_MOST
    }

    /// The slider panel's button: saves the slider's value as a notch, or takes away the notch it
    /// is on.
    pub fn toggle_slider_notch(&self, size: bool) {
        let info = self.info();
        let value = if size { info.size } else { info.opacity };
        match self.slider_notch_at(size, value) {
            Some(n) => self.remove_slider_notch(size, n),
            None => self.add_slider_notch(size, value),
        };
    }

    /// Chooses the brush the current tool uses.
    pub fn select_brush(&self, set: u32, index: u32) -> bool {
        let mut st = self.shared.lock();
        let (set, index) = (set as usize, index as usize);
        if st.library.get(set).and_then(|s| s.brushes.get(index)).is_none() {
            return false;
        }
        // the set left keeps showing the brush it had, the new one this
        let before = st.brush_ref(st.tool);
        st.note_choice(before);
        match st.tool {
            crate::ToolKind::Paint => st.paint_brush = (set, index),
            crate::ToolKind::Erase => st.erase_brush = (set, index),
            crate::ToolKind::Smudge => st.smudge_brush = (set, index),
        }
        st.note_choice((set, index));
        warm_pictures([st.brush(st.tool)]);
        st.load_brush_pictures();
        // a brush a plugin's brush engine draws: its look made now, on a thread of its own, for its
        // first stroke
        if let Some(hooks) = st.hooks.clone() {
            crate::plugin_brushes::look_of(&hooks, st.brush(st.tool), false);
        }
        true
    }

    /// The brush last chosen in `set`, or its first brush if none has been or that one is gone;
    /// none for a set with no brushes. A set opened in the library, tapped or rested on by a dragged
    /// brush, makes it the current tool's brush at once, so the brush shown highlighted is always
    /// the one in use (as the original keeps one brush per set, the first until one is chosen).
    pub fn brush_set_choice(&self, set: u32) -> Option<BrushInfo> {
        let st = self.shared.lock();
        let brushes = &st.library.get(set as usize)?.brushes;
        let index = st.set_choice(set as usize).filter(|&i| i < brushes.len()).unwrap_or(0);
        let brush = brushes.get(index)?;
        Some(BrushInfo { set, index: index as u32, name: brush.name.clone() })
    }

    /// The colour (0xRRGGBB) the brush library shows its sample strokes in, if a plugin gives one by
    /// the current colour: for the brush in use (`chosen`, its row on the accent) or any other. None:
    /// as the app shows them (white, or black on the light interface's rows). The strokes stay
    /// white, so the library tints them with it as it draws them, and draws none again.
    pub fn brush_preview_colour(&self, chosen: bool) -> Option<u32> {
        let (hooks, colour) = {
            let st = self.shared.lock();
            (st.hooks.clone()?, st.color)
        };
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u32;
        let [r, g, b] = colour.map(byte);
        hooks.brush_preview_colour((r << 16) | (g << 8) | b, chosen)
    }

    /// A sample stroke drawn with the brush, white on transparent, fitted into `width` × `height`
    /// pixels.
    pub fn brush_preview(&self, set: u32, index: u32, width: u32, height: u32) -> Option<ThumbnailImage> {
        let brush = self.shared.lock().library.get(set as usize)?.brushes.get(index as usize)?.clone();
        self.drawn_preview(brush, width, height)
    }

    /// Brush `index` of `set`'s sample stroke for a file being written: the one the library drew,
    /// if it is kept, else drawn but not kept (a file of a whole library would push out every
    /// stroke the library keeps, as it has none of them yet, one by one).
    pub(crate) fn brush_preview_for_file(
        &self,
        set: u32,
        index: u32,
        width: u32,
        height: u32,
    ) -> Option<ThumbnailImage> {
        let brush = self.shared.lock().library.get(set as usize)?.brushes.get(index as usize)?.clone();
        self.sample_stroke(brush, width, height, false)
    }

    /// The sample stroke of the brush open in the studio (a dual brush's whole), as
    /// [`CanvasSession::brush_preview`] draws one, for the studio to show as its controls change.
    /// A slider being dragged asks again at every step: only the newest ask is drawn, and one still
    /// waiting for its turn when a newer comes gives up (none), so the strokes drawn never pile up
    /// behind the slider (on a slow GPU they took the frames' place, then its memory).
    pub fn studio_preview(&self, width: u32, height: u32) -> Option<ThumbnailImage> {
        use std::sync::atomic::Ordering::SeqCst;
        let ask = self.shared.studio_previews.fetch_add(1, SeqCst).wrapping_add(1);
        let _turn = STUDIO_PREVIEW_TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.shared.studio_previews.load(SeqCst) != ask {
            return None;
        }
        let brush = {
            let st = self.shared.lock();
            let (set, index) = st.studio.as_ref()?.0;
            st.library.get(set)?.brushes.get(index)?.clone()
        };
        self.sample_stroke(brush, width, height, false)
    }

    /// The same of the first or `second` part alone of the dual brush open in the studio.
    pub fn studio_part_preview(&self, second: bool, width: u32, height: u32) -> Option<ThumbnailImage> {
        let brush = self.shared.lock().studio_part(second)?;
        self.sample_stroke(brush, width, height, false)
    }

    /// Lets go of the canvas the brushes' sample strokes are drawn on, with all it holds (100 MB
    /// and more of the GPU's memory after a heavy set's): for when the brush library closes. The
    /// strokes drawn stay kept; the next one drawn makes another canvas. It goes by itself a
    /// moment after the last stroke drawn on it as well, and with the artwork's canvas.
    pub fn let_go_of_previews(&self) {
        let canvas = self.shared.lock().preview.take();
        drop(canvas);
    }
}

impl CanvasSession {
    /// What the canvas the sample strokes are drawn on holds, while it is kept: its textures, the
    /// buffers made for its strokes' work, and of those its lists of tiles, bytes. For tests.
    #[doc(hidden)]
    pub fn preview_canvas_bytes(&self) -> Option<[u64; 3]> {
        let st = self.shared.lock();
        let (canvas, _) = st.preview.as_ref()?;
        Some([canvas.texture_bytes(), canvas.work_buffer_bytes(), canvas.rule_list_bytes()])
    }
}

/// Where a tool's notches are kept on a brush.
pub(crate) fn notch_tool(tool: crate::ToolKind) -> usize {
    procraft_engine::brush::Notches::tool(tool == crate::ToolKind::Paint, tool == crate::ToolKind::Smudge)
}

/// A square grey picture `n` pixels a side averaged down to half that.
fn halved(pixels: &[u8], n: usize) -> Vec<u8> {
    let h = n / 2;
    let at = |x: usize, y: usize| u32::from(pixels.get(y * n + x).copied().unwrap_or(0));
    (0..h * h)
        .map(|i| {
            let (x, y) = (i % h * 2, i / h * 2);
            ((at(x, y) + at(x + 1, y) + at(x, y + 1) + at(x + 1, y + 1) + 2) / 4) as u8
        })
        .collect()
}

/// A square grey picture `n` pixels a side read between its pixels at (`u`, `v`) (pixel centres
/// on whole numbers), 0..1; nothing outside it.
fn bilinear(pixels: &[u8], n: usize, u: f32, v: f32) -> f32 {
    let at = |x: i64, y: i64| {
        if x < 0 || y < 0 || x >= n as i64 || y >= n as i64 {
            0.0
        } else {
            f32::from(pixels.get(y as usize * n + x as usize).copied().unwrap_or(0)) / 255.0
        }
    };
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
    let bottom = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// The key a plugin knows a brush by (interface 11): where it is in the library, its set's place
/// (past 0) in the high half and its own in the low. It holds while the library stays as it is.
pub(crate) fn plugin_brush_key(set: u32, index: u32) -> i64 {
    ((i64::from(set) + 1) << 32) | i64::from(index)
}

impl CanvasSession {
    /// The brush plugin key `key` stands for (see [`plugin_brush_key`]), if one is there now.
    pub(crate) fn plugin_brush(&self, key: i64) -> Option<crate::PanelBrush> {
        let set = u32::try_from((key >> 32) - 1).ok()?;
        let index = u32::try_from(key & 0xFFFF_FFFF).ok()?;
        let st = self.shared.lock();
        let brush = st.library.get(set as usize)?.brushes.get(index as usize)?;
        Some(crate::PanelBrush { set, index, name: brush.name.clone() })
    }

    /// The key of the brush at `at` of the Recent set (its pinned brushes first, then the latest
    /// painted with), 0 past its end.
    pub(crate) fn plugin_recent_brush(&self, at: usize) -> i64 {
        self.recent_brushes().get(at).map_or(0, |b| plugin_brush_key(b.set, b.index))
    }

    /// The key of the brush the current tool uses.
    pub(crate) fn plugin_current_brush(&self) -> i64 {
        let st = self.shared.lock();
        let (set, index) = st.brush_ref(st.tool);
        u32::try_from(set).ok().zip(u32::try_from(index).ok()).map_or(0, |(s, i)| plugin_brush_key(s, i))
    }

    /// The brush plugin key `key` stands for chosen for the current tool, as a tap in the library
    /// chooses it; false if there is none there.
    pub(crate) fn plugin_choose_brush(&self, key: i64) -> bool {
        let Some(brush) = self.plugin_brush(key) else { return false };
        self.select_brush(brush.set, brush.index)
    }

    /// The colour at `at` of the colour history (0 the latest), 0xRRGGBB; none past its end.
    pub(crate) fn plugin_recent_colour(&self, at: usize) -> Option<u32> {
        let history = self.shared.lock().colour_history.clone()?;
        history.colours().get(at).map(|c| c & 0xFF_FFFF)
    }

    /// `brush`'s sample stroke (see [`CanvasSession::brush_preview`]).
    /// `style` reads `brush`'s grain and shape from its file's images at their own size, as a
    /// stroke does (they are read out first: nothing is drawn under the lock).
    fn use_full_sources(
        &self,
        gpu: &procraft_gpu::Gpu,
        canvas: &mut Canvas,
        style: &mut StrokeStyle,
        brush: &procraft_engine::brush::Brush,
    ) {
        if let Some(full) = brush.grain_file_now().and_then(|f| f.full.as_ref()) {
            let pixels = self.shared.lock().grain_store.pixels(full);
            if let Some(pixels) = pixels {
                canvas.use_full_grain(gpu, style, full.id, full.side, &pixels);
            }
        }
        if let Some(full) = brush.shape_file_now().map(|f| &f.full) {
            let pixels = self.shared.lock().grain_store.pixels(full);
            if let Some(pixels) = pixels {
                canvas.use_full_shape(gpu, style, full.id, full.side, &pixels);
            }
        }
    }

    pub(crate) fn drawn_preview(
        &self,
        brush: procraft_engine::brush::Brush,
        width: u32,
        height: u32,
    ) -> Option<ThumbnailImage> {
        self.sample_stroke(brush, width, height, true)
    }

    /// `brush`'s sample stroke (see [`CanvasSession::brush_preview`]): the one drawn already from
    /// a brush looking as it does, if it is kept, else drawn now, and kept if `keep` says (the
    /// library's are; the studio's, a new one at every step of a slider, aren't).
    fn sample_stroke(
        &self,
        brush: procraft_engine::brush::Brush,
        width: u32,
        height: u32,
        keep: bool,
    ) -> Option<ThumbnailImage> {
        let (w, h) = (width.clamp(16, 2048), height.clamp(16, 1024));
        let (cw, ch) = (preview::CANVAS.0 as u32, preview::CANVAS.1 as u32);
        // a brush a plugin's brush engine draws, while that is on, is drawn by it
        let hooks = self
            .shared
            .lock()
            .hooks
            .clone()
            .filter(|h| brush.engine.as_ref().is_some_and(|e| h.engine(&e.engine).is_some()));
        // the preview it came with, while it is as it came: a blank file preview (some imported
        // brushes ship one) is no preview, so the brush draws its own
        if let Some((tw, th, rgba)) =
            brush.thumbnail.as_deref().filter(|_| hooks.is_none()).and_then(crate::brush_files::thumbnail_pixels)
        {
            let (pixels, _) = rgba.as_chunks::<4>();
            if pixels.iter().any(|p| p[3] > 0) {
                return Some(ThumbnailImage { width: w, height: h, rgba: preview::fit_image(&rgba, (tw, th), w, h) });
            }
        }
        // drawn already from a brush looking as this one does: that stroke (one a plugin's brush
        // engine draws is drawn again, as the plugin may have changed)
        let key = hooks.is_none().then(|| crate::drawn_previews::look_key(&brush));
        let kept_stroke = |key: Option<u64>| {
            let png = self.shared.lock().drawn_previews.find(key?)?;
            let rgba = crate::drawn_previews::unpacked(cw, ch, &png)?;
            Some(ThumbnailImage { width: w, height: h, rgba: preview::fit(&rgba, w, h) })
        };
        if let Some(kept) = kept_stroke(key) {
            return Some(kept);
        }
        // one at a time, on the canvas kept for them: drawn side by side, each took a canvas of its
        // own, its shape and grain arrays made again with it (a library's card asks for a dozen
        // strokes at once); the canvas staying while it waits
        let _pending = PreviewPending::new(&self.shared);
        let _drawing = PREVIEWS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // (drawn meanwhile, by an ask for it made before this one)
        if let Some(kept) = kept_stroke(key) {
            return Some(kept);
        }
        if key.is_some() {
            self.shared.lock().drawn_previews.missed += 1;
        }
        // what it needs taken under the lock; drawn without it, so painting goes on meanwhile
        let (gpu, kept) = {
            let mut st = self.shared.lock();
            (st.gpu.clone()?, st.preview.take())
        };
        // a device lost draws nothing (and what it draws with can't be made there)
        if gpu.is_lost() {
            return None;
        }
        // a device short of memory (an old tablet's, the canvas open) lets go of what draws them
        // after each, rather than hold it while the library is looked through and be closed by
        // the system; each stroke still comes out (one at a time, as ever)
        let short = crate::device::short_of_memory(PREVIEW_CANVAS_BYTES);
        if short && !PREVIEW_SHORT_SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log::info!(
                "the brushes' sample strokes are drawn on a canvas let go after each: the device is short of memory"
            );
        }
        // drawn on a canvas of the sample stroke's own, then fitted into the preview; what the GPU
        // couldn't do (no memory for it, the device lost meanwhile) costs this preview, never the
        // app: none is shown, the canvas goes, and the first such is said
        let drawn = gpu.checked(|| {
            let (mut canvas, mut doc) = match kept {
                Some(kept) => kept,
                None => {
                    let mut doc = Document::new(cw, ch);
                    doc.background_visible = false;
                    let mut canvas = Box::new(Canvas::new(&gpu, &doc).ok()?);
                    // drawn beside the window's frames: its work goes to the GPU a short piece at
                    // a time, each waited for (a weak GPU took seconds over a wet brush's stroke,
                    // and the frame that queued behind it held the canvas all that while)
                    canvas.pace(true);
                    (canvas, doc)
                }
            };
            let rgba = self.draw_preview(&brush, hooks.as_ref(), &gpu, &mut canvas, &mut doc);
            let whole = !canvas.take_work_left_out();
            Some((rgba, whole, canvas, doc))
        });
        let failed = |why: &str| {
            if !PREVIEW_FAILURE_SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log::error!("a brush's sample stroke couldn't be drawn: {why}");
            }
            None
        };
        let (rgba, whole, mut canvas, doc) = match drawn {
            Ok(drawn) => drawn?,
            Err(e) => return failed(&e.to_string()),
        };
        // kept for the next (one drawn meanwhile elsewhere already is), unless memory is short or
        // the artwork's canvas has gone meanwhile; with no more of what its work grew to than a
        // light stroke wants, and only for a moment after the last
        if !short {
            canvas.let_go_of_work_past(&gpu, PREVIEW_WORK_KEPT);
            let kept = {
                let mut st = self.shared.lock();
                let open = !st.closed;
                if open {
                    st.preview.get_or_insert((canvas, doc));
                }
                open
            };
            if kept {
                watch_previews(&self.shared);
            }
        }
        if !whole {
            return failed("part of it was left out");
        }
        let mut rgba = rgba?;
        // kept, it shows as it will when taken again (a white stroke as its coverage)
        if let Some(key) = key.filter(|_| keep)
            && let Some(png) = crate::drawn_previews::packed(cw, ch, &rgba)
        {
            rgba = crate::drawn_previews::unpacked(cw, ch, &png).unwrap_or(rgba);
            self.shared.lock().drawn_previews.keep(key, png);
        }
        Some(ThumbnailImage { width: w, height: h, rgba: preview::fit(&rgba, w, h) })
    }

    /// `brush`'s sample stroke drawn on `canvas` (its layer wiped first), white: the canvas's pixels
    /// (by the brush engine that made it, while that is on and `hooks` has it).
    fn draw_preview(
        &self,
        brush: &procraft_engine::brush::Brush,
        hooks: Option<&std::sync::Arc<crate::hooks::Hooks>>,
        gpu: &procraft_gpu::Gpu,
        canvas: &mut Canvas,
        doc: &mut Document,
    ) -> Option<Vec<u8>> {
        let (cw, ch) = (preview::CANVAS.0 as u32, preview::CANVAS.1 as u32);
        let layer = doc.current();
        canvas.wipe_layer(layer);
        if let Some(hooks) = hooks
            && let Some(drawn) = self.engine_preview(hooks, brush, gpu, canvas, doc)
        {
            return Some(drawn);
        }
        let mut style = StrokeStyle::for_brush(brush, StrokeTool::Paint, [1.0, 1.0, 1.0], 1.0, false);
        // it mixes wet only where the brush's preview says to
        if brush.setting("preview_wet_mix") < 0.5 {
            style = style.dry();
        }
        style.preview = true;
        // its wet edges pooling as on the canvas, where the pad shows them only as a firmer rim
        style.sample = true;
        draw_pictures(brush);
        canvas.use_sources(gpu, &mut style, brush);
        self.use_full_sources(gpu, canvas, &mut style, brush);
        // a dual brush's second, drawn along the same sample stroke at its size beside the first's
        let stamp_only = brush.setting("preview_stamp") >= 0.5;
        let second = brush.secondary.as_deref().filter(|_| !stamp_only && style.takes_second()).and_then(|second| {
            let second = second.clone().as_second();
            let mut second_style =
                StrokeStyle::for_brush(&second, StrokeTool::Paint, [1.0, 1.0, 1.0], 1.0, false).as_second();
            second_style.preview = true;
            second_style.sample = true;
            canvas.use_sources(gpu, &mut second_style, &second);
            self.use_full_sources(gpu, canvas, &mut second_style, &second);
            let mode = brush.setting("dual_blend").round().max(0.0) as u32;
            let classic = brush.setting("classic_combine") >= 0.5;
            (!second_style.in_place()).then(|| {
                let threshold = second_style.alpha_threshold;
                let dual = procraft_gpu::SecondStroke { style: second_style, mode, threshold, classic };
                let diameter = preview::second_diameter(brush, &second);
                (second, dual, diameter)
            })
        });
        match &second {
            Some((_, dual, _)) => canvas.begin_dual_stroke(layer, style, *dual),
            None => canvas.begin_stroke(layer, style),
        }
        if stamp_only {
            // the brush's stamp on its own in the middle, its preview size of the way from its
            // least size to its most (a big one reaching past the preview, as there)
            let (most, share) = (brush.biggest(), brush.setting("preview_size").clamp(0.0, 1.0));
            let least = brush.min_size.clamp(0.0, most);
            let side = (least + (most - least) * share).max(1.0);
            canvas.add_stamps(&[Stamp::round(Vec2::new(cw as f32 / 2.0, ch as f32 / 2.0), side / 2.0, 1.0)]);
        } else {
            let samples = preview::samples(brush);
            // drawn as the pad draws: the tilt acting only if the brush previews it, the lean moving
            // no stamps; by a pen that tells its pressure and how it is turned (a brush turned as
            // the pen is held turns its stamps so, not with the stroke) but not how far it leans,
            // so no lean shades or sizes the stamps and the tilt switches are out of the way; its
            // pressure taken as it comes, as its path is (no streamline holds either back)
            let tilts = brush.setting("preview_tilt") >= 0.5;
            let pen = InputSettings { tilt_known: false, ..InputSettings::default() };
            let sample_stroke = |part: procraft_engine::brush::Brush, diameter: f32| {
                let part = procraft_engine::brush::Brush { pressure_smoothing: 0.0, ..part };
                StrokeBuilder::new(part, diameter, Pointer::Stylus).with_input(pen.clone()).steady().on_pad(tilts)
            };
            if let Some((second, _, diameter)) = second {
                let mut stroke = sample_stroke(second, diameter);
                canvas.add_second_stamps(&stroke.add(&samples));
                canvas.replace_second_stamps(&stroke.finish());
            }
            let mut stroke = sample_stroke(brush.clone(), preview::diameter(brush));
            // (its first stamps as they turned to the way it goes, so its end lays only the rest)
            let added = stroke.add(&samples);
            canvas.add_stamps(&if stroke.take_turned() { stroke.placed_stamps() } else { added });
            canvas.replace_stamps(&stroke.finish());
        }
        canvas.end_stroke();
        canvas.render(gpu, doc, None);
        canvas.forget_history();
        canvas.read_composite(gpu).ok()
    }

    /// `brush`'s sample stroke drawn by the brush engine that made it, white, at its preview size,
    /// on the preview's canvas (its layer wiped): the canvas's pixels; none if the engine doesn't
    /// draw it.
    fn engine_preview(
        &self,
        hooks: &std::sync::Arc<crate::hooks::Hooks>,
        brush: &procraft_engine::brush::Brush,
        gpu: &procraft_gpu::Gpu,
        canvas: &mut Canvas,
        doc: &mut Document,
    ) -> Option<Vec<u8>> {
        let diameter = preview::diameter(brush);
        let curve = procraft_engine::brush::SIZE_CURVE;
        let start = brush.diameter(brush.setting("preview_size").clamp(0.0, 1.0).powf(1.0 / curve));
        let starting = crate::plugin_brushes::Starting {
            colour: [1.0; 3],
            secondary: [0.0; 3],
            diameter,
            start_diameter: start,
            opacity: 1.0,
            tool: 0,
            pointer: 1,
            seed: PREVIEW_SEED,
            under_at: None,
            turn: 0.0,
        };
        // (drawn off the interface's threads, it waits for the look being made, showing none
        // meanwhile)
        let o = crate::plugin_brushes::engine_stroke(
            hooks,
            brush,
            StrokeTool::Paint,
            starting,
            (false, true),
            canvas,
            gpu,
            true,
        )?;
        let layer = doc.current();
        match o.second {
            Some(second) => canvas.begin_dual_stroke(layer, o.style, second),
            None => canvas.begin_stroke(layer, o.style),
        }
        let samples = preview::samples(brush);
        let make =
            |half| StrokeBuilder::new(brush.clone(), diameter, Pointer::Stylus).steady().with_engine(Box::new(half));
        let mut first = make(o.first);
        if o.second.is_some() {
            let mut second = make(o.second_half);
            canvas.add_second_stamps(&second.add(&samples));
            canvas.replace_second_stamps(&second.finish());
        }
        canvas.add_stamps(&first.add(&samples));
        canvas.replace_stamps(&first.finish());
        canvas.end_stroke();
        canvas.render(gpu, doc, None);
        canvas.forget_history();
        canvas.read_composite(gpu).ok()
    }
}

/// What an engine's preview strokes are seeded with: the same each time.
const PREVIEW_SEED: u32 = 0x5eed_7e57;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use procraft_engine::brush::Brush;

    use crate::CanvasSession;

    /// A 2 GB tablet keeps 97 of the library's 530 x 162 strokes (32 MB), a 3 GB one 146, an 8 GB
    /// one 390; a 1 GB one still a few screens' worth (48), and none past 400.
    #[test]
    fn the_library_keeps_strokes_by_the_device_s_memory() {
        const GB: u64 = 1 << 30;
        let kept = |gb: f64| super::brush_previews_kept((gb * GB as f64) as u64, 530, 162);
        assert_eq!([kept(1.0), kept(2.0), kept(3.0), kept(8.0), kept(64.0)], [48, 97, 146, 390, 400]);
        let held = |n: u32| u64::from(n) * 530 * 162 * 4;
        assert!(held(kept(2.0)) <= 2 * GB / 64, "{} MB", held(kept(2.0)) >> 20);
        assert_eq!(super::brush_previews_kept(0, 0, 0), 48);
    }

    /// Hovering, the cursor fills as the brush would paint at its estimated pressure: a brush whose
    /// opacity follows the pressure fills faintly at 44% and the picture shows through (#332),
    /// one whose size does fills less of the cursor, one that follows neither fills it all at the
    /// opacity painted with; a brush whose Hover fill is None fills nothing.
    #[test]
    fn the_hover_fill_is_the_stamp_at_the_brushs_estimated_pressure() {
        let s = CanvasSession::new(64, 64);
        let set = |settings: &[(&str, f32)]| {
            s.studio_begin();
            for &(key, value) in settings {
                s.set_studio_setting(key.into(), value);
            }
            s.studio_end(true);
            s.hover_fill()
        };
        assert_eq!(set(&[("hover_fill", 0.0)]), None);
        let plain =
            set(&[("hover_fill", 1.0), ("hover_pressure", 0.44), ("pressure_size", 0.0), ("pressure_opacity", 0.0)]);
        assert_eq!(plain, Some(super::HoverFill { size: 1.0, opacity: 1.0 }));
        let fading = set(&[("pressure_opacity", 1.0)]).unwrap_or(super::HoverFill { size: 0.0, opacity: 1.0 });
        assert!(fading.opacity < 0.6 && fading.opacity > 0.1 && fading.size == 1.0, "{fading:?}");
        // pressed harder in the estimate, stronger
        let harder = set(&[("hover_pressure", 0.9)]).unwrap_or(fading);
        assert!(harder.opacity > fading.opacity + 0.2, "{harder:?} against {fading:?}");
        let thinning = set(&[("pressure_opacity", 0.0), ("pressure_size", 1.0), ("hover_pressure", 0.44)])
            .unwrap_or(super::HoverFill { size: 1.0, opacity: 0.0 });
        assert!(thinning.size < 0.7 && thinning.opacity == 1.0, "{thinning:?}");
        eprintln!("hover fill: plain {plain:?}, by opacity {fading:?} (at 90% {harder:?}), by size {thinning:?}");
    }

    /// A brush file whose own preview is blank (a brush imported from another app can bring one):
    /// the library draws the brush instead of showing nothing.
    #[test]
    fn a_blank_brush_file_preview_is_drawn_from_the_brush() {
        let s = CanvasSession::new(64, 64);
        if !s.prepare_offscreen() {
            return;
        }
        let mut brush = Brush::round_pen();
        let blank = crate::project::rgba_png(8, 8, &[0u8; 8 * 8 * 4]);
        brush.thumbnail = Some(Arc::from(blank));
        let preview = s.drawn_preview(brush, 240, 60).expect("a preview");
        let painted = preview.rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
        assert!(painted > 100, "{painted} pixels");
    }

    /// The canvas the sample strokes are drawn on stays while a stroke waits for its turn behind
    /// another's (another artwork's here, held longer than the canvas is kept with none asked for),
    /// and goes a moment after the last has ended: let go of while one waited, the next was drawn
    /// on a canvas made again.
    #[test]
    fn the_strokes_canvas_stays_while_a_stroke_waits_its_turn() {
        use std::time::{Duration, Instant};
        let s = CanvasSession::new(64, 64);
        if !s.prepare_offscreen() {
            return;
        }
        let kept = |s: &CanvasSession| s.shared.lock().preview.is_some();
        assert!(s.drawn_preview(Brush::round_pen(), 106, 32).is_some());
        assert!(kept(&s), "kept for the next");
        let (stayed, drawn) = {
            let turn = super::PREVIEWS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let asking = std::thread::spawn({
                let s = Arc::clone(&s);
                // (another look: drawn, not found kept)
                move || s.drawn_preview(Brush { hardness: 0.5, ..Brush::round_pen() }, 106, 32).is_some()
            });
            std::thread::sleep(super::PREVIEW_CANVAS_KEPT + Duration::from_secs(1));
            let stayed = kept(&s);
            drop(turn);
            (stayed, asking.join().unwrap_or(false))
        };
        let ended = Instant::now();
        assert!(stayed, "let go of while a stroke waited its turn");
        assert!(drawn, "drawn once its turn came");
        while kept(&s) {
            assert!(ended.elapsed() < Duration::from_secs(30), "still kept after {:?}", ended.elapsed());
            std::thread::sleep(Duration::from_millis(20));
        }
        // (from its end, a moment after the turn came)
        assert!(ended.elapsed() >= Duration::from_millis(1500), "let go of after {:?}", ended.elapsed());
    }
}
