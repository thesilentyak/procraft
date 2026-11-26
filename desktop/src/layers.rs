//! The layers panel, as the Android app's (ui/LayersPanel.kt): top layer first, each row its
//! thumbnail, name, blend letter and visibility box; groups with their line and arrow; the
//! background under them. Tap a layer to make it current, the current one for its options (where
//! it is renamed in place), its blend letter for its blend mode and opacity, its box to show or
//! hide it (hold it to show it alone), the plus for a new layer; hold a row and drag it to move it
//! (with a mouse, just drag it up or down), out past the panel's side to lift it out and drop it
//! on the canvas or, through the gallery, into another artwork;
//! slide it left for lock, duplicate and delete, or a little right to pick it. On a model: each
//! texture set's name over its meshes (each its picture and box), the open mesh's layers under it,
//! and under a layer its channels (colour, roughness, metalness), one of which a click paints on.

use std::collections::HashMap;
use std::time::Instant;

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat, TextWrapping};
use egui::{Align2, Color32, Pos2, Rect, ScrollArea, Sense, TextureHandle, TextureOptions, Ui, UiBuilder, pos2, vec2};
use procraft_core::{
    BlendCoast, BlendModeInfo, CanvasSession, LayerControl, LayerInfo, LayerOption, LayerOptionRow, LayerType,
    ModelInfo, NoticeKind, RowDrop, ShownRow, ThumbnailImage, blend_centred, blend_coast, blend_coast_at,
    blend_pull_for, blend_pulled, blend_tap, blend_tap_at,
};
use resvg::tiny_skia;

use crate::alert::{Alert, Answer};
use crate::colours::Colours;
use crate::icons;
use crate::look::{Look, Themed, Weight, argb, pc, round_rect, text};
use crate::menu::{self, Entry, Menu};
use crate::overlays::ease;
use crate::panel::{self, PANEL_GLYPH, PANEL_TITLE};

pub(crate) const LAYERS_WIDTH: f32 = 384.0;
const HEADER_HEIGHT: f32 = 64.0;
const ROW_HEIGHT: f32 = 69.0;
/// A layer with a mask, and the mask, are a little shorter.
const MASKED_ROW_HEIGHT: f32 = 65.0;
const ROW_GAP: f32 = 1.0;
const LIST_CORNER: f32 = 8.0;
/// A group's row height, and how far in each level of group sits.
const GROUP_HEIGHT: f32 = 44.0;
const GROUP_INDENT: f32 = 21.0;
const GROUP_LINE: u32 = 0xFF4E4E4E;
/// A closed group's row is taller: its line (20 under the row's top and 20 over its bottom) opens
/// into a window 4 in from them, on its layers' rows drawn small; opening or closing, over this
/// long (seconds).
const GROUP_CLOSED_HEIGHT: f32 = 82.0;
const GROUP_LINE_INSET: f32 = 20.0;
const GROUP_WINDOW_INSET: f32 = 4.0;
const GROUP_TURN: f32 = 0.15;
/// A row's thumbnail (a group's line) held over by a dragged row long enough to take it.
const HELD_OUTLINE: Color32 = Color32::WHITE;
const MASK_OWNER_ROW: u32 = 0xFF2C5281;
const PICKED_ROW: u32 = 0xFF2C5281;
const LOCKED_LETTER: u32 = 0xFF808080;
const LAYER_ROW: Themed = Themed(0x0DFFFFFF, 0xFFD8D8D8);
const BACKGROUND_ROW: Themed = Themed(0x0DFFFFFF, 0xFFD8D8D8);
/// How tall a plugin's action's row under the background's is (as that one is).
const ACTION_ROW: f32 = 50.0;
const CHECK_FILL: Themed = Themed(0xFFC7C7C7, 0xFF575757);
pub(crate) const THUMB_BACKGROUND: Themed = Themed(0xFF4F4F4F, 0xFFC8C8C8);
/// A text layer's glyph on its thumbnail.
const TEXT_GLYPH: Themed = Themed(0xFFB8B8B8, 0xFF000000);
const THUMB: (f32, f32) = (93.0, 61.0);
/// On a model: a texture set's name heading its meshes (16 under the set before it), an open
/// mesh's row, a mesh's picture's tile; a layer's channels under it, each a tile.
const SET_HEADER: f32 = 41.0;
const SET_GAP: f32 = 16.0;
const SUBMESH_OPEN: u32 = 0xFF25344A;
const SUBMESH_TILE: u32 = 0xFF434246;
const TRAY_HEIGHT: f32 = 159.0;
const TRAY_FILL: u32 = 0xFF303032;
const TILE_FILL: u32 = 0xFF3B3B3D;
const CHANNELS: [&str; 3] = ["Color", "Roughness", "Metallic"];
/// Thumbnails are read this many pixels on their longer side, a few a frame.
const THUMB_SIDE: u32 = 192;
const THUMBS_A_FRAME: usize = 4;
/// The options sit this far left of the panel's left edge, as wide as this, never higher than this
/// from the window's top.
const OPTIONS_LEFT: f32 = -183.0;
const OPTIONS_WIDTH: f32 = 194.0;
/// Delete on a locked layer's row says this.
const LOCKED_DELETE: &str = "A locked layer can’t be deleted. Unlock it first.";
const OPTIONS_TOP: f32 = 57.0;
const OPTIONS_CORNER: f32 = 24.0;
/// The blend editor under a row: the opacity and its slider, then the list of modes (nine rows
/// showing, the layer's mode in the middle).
const BLEND_EDITOR_HEIGHT: f32 = 22.0 + 23.0 + 20.0 + 11.0 + BLEND_LIST_HEIGHT + 15.0;
const BLEND_LIST_HEIGHT: f32 = 349.0;
const BLEND_ROW: f32 = 38.8;
const BLEND_GLYPH_INK: u32 = 0xFF808080;
const SLIDER_REST: Themed = Themed(0xFF5A5A5A, 0xFFBEBEBE);
/// Opening, the blend editor grows over this long; closing, it goes faster.
const BLEND_OPEN: f32 = 0.25;
const BLEND_CLOSE: f32 = 0.1;
/// A wheel or trackpad left alone this long (seconds) lets the modes go, to settle on a row.
const BLEND_WHEEL_PAUSE: f64 = 0.12;
/// A row slid left shows its buttons, each this wide, this far apart; slid right this far, it is
/// picked.
const SWIPE_BUTTON: f32 = 82.0;
const SWIPE_GAP: f32 = 2.0;
const SWIPE_FILL: u32 = 0xFF3F3F3F;
const SWIPE_INK: u32 = 0xFFC1C1C1;
const SWIPE_DELETE: u32 = 0xFFFF473D;
const PICK_REACH: f32 = 72.0;
/// A press moved this far (points) slides its row; held still this long, it is a hold.
const SLOP: f32 = 8.0;
const HOLD: std::time::Duration = std::time::Duration::from_millis(400);
/// A dragged row this near an end of the list scrolls it, at most this far a second.
const DRAG_EDGE: f32 = 56.0;
const DRAG_SCROLL: f32 = 480.0;
/// A dragged row this far out past the panel's side is lifted out of it, to drop elsewhere.
const LIFT_REACH: f32 = 24.0;

/// What a press on a row turned out to be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Way {
    Undecided,
    /// Sideways: the row slides.
    Swipe,
    /// Held: the row is dragged up or down to move it.
    Reorder,
    /// Moved up or down first: nothing.
    Ignored,
}

/// A press on a row, followed until let go: whose, where and when it went down, what it became,
/// how far the row was slid as it went down, how far it has been dragged down, and whether it is a
/// mouse's (not a finger's).
#[derive(Clone, Copy)]
struct RowPress {
    id: u64,
    origin: Pos2,
    since: Instant,
    way: Way,
    start: f32,
    dy: f32,
    mouse: bool,
}

/// The blend editor open under a layer's row: how far its modes are scrolled (points, the first
/// in the band at 0; past an end while stretched) and how they are moving.
struct Blend {
    id: u64,
    scroll: f32,
    motion: BlendMotion,
}

/// How a blend editor's modes are moving.
#[derive(Clone, Copy)]
enum BlendMotion {
    /// At rest on a row.
    Resting,
    /// Held, pressed with the list where a pull of `start` (points, unstretched) takes it, and
    /// following the finger from there, last at `y`; `caught` when it was moving as it was
    /// pressed, for letting go without dragging then only to stop it.
    Held { start: f32, caught: bool, y: f32 },
    /// Scrolled by a wheel or trackpad, last at `last` (seconds).
    Wheel { last: f64 },
    /// Let go at `since` (seconds), moving on as `coast` says.
    Coasting { coast: BlendCoast, since: f64 },
    /// A tapped mode scrolling from `from` to the band at `to` since `since` (seconds): the list
    /// takes no touches and shows no modes on the way.
    Tapped { from: f32, to: f32, since: f64 },
}

impl Blend {
    /// Moving under a finger or a wheel, or by itself after them: each mode reaching the band
    /// shows on the layer, the whole movement one undo step.
    fn moving(&self) -> bool {
        matches!(self.motion, BlendMotion::Held { .. } | BlendMotion::Wheel { .. } | BlendMotion::Coasting { .. })
    }

    /// Where a finger or a wheel has to have taken the list (unstretched) for it to be where it is.
    fn pull(&self, rows: u32) -> f32 {
        blend_pull_for(self.scroll, BLEND_ROW, rows, BLEND_LIST_HEIGHT)
    }

    /// Taken to `pull`: there between the ends, stretched past them.
    fn pull_to(&mut self, pull: f32, rows: u32) {
        self.scroll = blend_pulled(pull, BLEND_ROW, rows, BLEND_LIST_HEIGHT);
    }

    /// Let go at `now` (seconds) going `speed` (points a second, down the list).
    fn let_go(&mut self, speed: f32, now: f64, rows: u32) {
        self.motion = BlendMotion::Coasting { coast: blend_coast(self.scroll, speed, BLEND_ROW, rows), since: now };
    }

    /// Moves on to `now` (seconds): a wheel left alone lets go, and what moves by itself goes on.
    fn advance(&mut self, now: f64, rows: u32) {
        if let BlendMotion::Wheel { last, .. } = self.motion
            && now - last >= BLEND_WHEEL_PAUSE
        {
            self.let_go(0.0, now, rows);
        }
        let step = match self.motion {
            BlendMotion::Coasting { coast, since } => blend_coast_at(coast, (now - since) as f32),
            BlendMotion::Tapped { from, to, since } => blend_tap_at(from, to, (now - since) as f32),
            _ => return,
        };
        self.scroll = step.position;
        if step.done {
            self.motion = BlendMotion::Resting;
        }
    }
}

#[derive(Default)]
pub struct Layers {
    /// Floating in a window of its own: what its strips had done to them this frame.
    pub chrome: panel::FloatingChrome,
    /// Hanging from its button, whether a plugin lets it float (its grabber then shows), and while
    /// the grabber is dragged, where the press began and where the pointer is (the window's points).
    pub floats: bool,
    pub undock: Option<(egui::Pos2, egui::Pos2)>,
    /// Each layer's thumbnail and the pixels' version it shows.
    thumbnails: HashMap<u64, (u64, TextureHandle)>,
    /// The current layer's options: whose (kept while they fade), and their coming and going.
    options: Option<u64>,
    options_menu: Menu,
    /// The control layer choices (None, Line art, Scribble): whose, where from, and their coming
    /// and going.
    control_for: Option<u64>,
    control_rect: Option<Rect>,
    control_pending: bool,
    control_menu: Menu,
    /// The title's menu, and Delete's question.
    title_menu: Menu,
    /// The +'s menu, while plugins add kinds of layer: a plain layer or one of each kind.
    add_menu: Menu,
    deleting: Alert,
    /// Asked before a layer the meshes of a texture set share goes, and which.
    deleting_shared: Alert,
    shared_going: Option<u64>,
    /// Delete on a locked layer's row: why not.
    locked: Alert,
    /// A layer renamed in place: which, the name as typed, and whether it has been focused.
    renaming: Option<(u64, String, bool)>,
    blend: Option<Blend>,
    /// A press on a row; how far each row is slid (points, left negative); the row slid open.
    press: Option<RowPress>,
    slid: HashMap<u64, f32>,
    swiped: Option<u64>,
    /// The rows that go with a dragged one (its mask, a group's layers), where the rows lay last
    /// frame, the list's box, and how far to scroll it.
    dragged_rows: Vec<u64>,
    /// Where the dragged row lands if let go now, and the row under its middle and since when
    /// (held over it a moment, it goes onto it).
    drop_at: Option<RowDrop>,
    over: Option<(u64, Instant)>,
    /// Each closed group's layers, as the list would show them open, with how far in each sits
    /// under it: drawn small in its window.
    closed_contents: HashMap<u64, Vec<(LayerInfo, u32)>>,
    last_rows: HashMap<u64, Rect>,
    list: Option<Rect>,
    /// The panel's box and its rows where the window has them, as last shown: where layers held out
    /// of it drop.
    box_on_window: Option<Rect>,
    rows_on_window: HashMap<u64, Rect>,
    /// A row dragged out past the panel's side: its layer, to be lifted out and held (the app takes
    /// it).
    pub lifted: Option<u64>,
    scroll_by: f32,
    /// A press on a row's visibility box: whose, since when, and whether holding it has acted.
    box_press: Option<(u64, Instant, bool)>,
    /// The fingers on a touch screen, two put on two rows being pinched, and two put on one row.
    touches: HashMap<u64, Pos2>,
    pinch: Option<Pinch>,
    twin: Option<Twin>,
    /// What the panel asks of the app: a notice to show, a layer's contents copied, the chosen
    /// layers shared, a text layer's text edited.
    pub notice: Option<(String, bool, procraft_core::NoticeKind)>,
    pub copied: Option<ThumbnailImage>,
    pub share: bool,
    pub edit_text: Option<u64>,
    /// The plugins' actions for the current layer's options, and the one tapped there, to run.
    pub plugin_actions: Vec<procraft_core::PluginAction>,
    /// The plugins' actions for the panel itself: a row each under the background's (interface 12).
    pub panel_actions: Vec<procraft_core::PluginAction>,
    /// The kinds of layer the plugins on add, which the + offers beside a plain layer.
    pub layer_kinds: Vec<procraft_core::PluginKind>,
    /// The filter layers the plugins on offer (interface 17): the + offers a Filter Layer that
    /// lists them; asked to list them, and asked for the current filter layer's settings.
    pub layer_filters: Vec<procraft_core::PluginLayerFilter>,
    pub filter_list: bool,
    pub filter_settings: bool,
    pub run_action: Option<String>,
    /// Layer Opacity asked for the current layer (two fingers tapped on its row).
    pub layer_opacity: bool,
    /// The background's colour asked for: its row tapped anywhere but on its box.
    pub pick_background: bool,
    /// On a model: the mesh open (its texture set's layers under it), the layer whose channels
    /// show, the channel whose colour shows, each channel's own colour (hue, saturation,
    /// brightness) kept while another is painted, the meshes' and channels' pictures, and the
    /// frame the panel last showed in (the meshes are pictured anew as it opens).
    open_mesh: Option<usize>,
    /// How far each mesh's row is slid (points, left negative), the one slid open, and a press on
    /// one: where along it, and how far it was slid then.
    mesh_slid: HashMap<usize, f32>,
    mesh_swiped: Option<usize>,
    mesh_press: Option<(usize, f32, f32)>,
    tray_for: Option<u64>,
    channel: u32,
    channel_colours: [Option<[f32; 3]>; 3],
    mesh_pictures: HashMap<usize, TextureHandle>,
    channel_pictures: HashMap<(u32, u64), TextureHandle>,
    /// The layer whose tray is pictured, as it was then: an undo or a stroke pictures it afresh.
    channel_pictured: Option<(u64, u64)>,
    last_frame: u64,
}

/// What the list shows: a layer's row (its corners, how far in it sits, whether its texture set's
/// meshes share it), or on a model a texture set's name or a mesh's row.
enum Item<'a> {
    Set { name: String, first: bool },
    Mesh { index: usize, name: String, open: bool, corners: (f32, f32) },
    Layer { l: &'a LayerInfo, corners: (f32, f32), indent: f32, shared: bool },
}

/// Whether the layers shown are a texture set's that several meshes share.
fn shared_by_meshes(session: &CanvasSession) -> bool {
    let painted = session.texture_set();
    session.model_info().is_some_and(|m| m.submeshes.iter().filter(|s| s.texture_set == painted).count() > 1)
}

/// The list's items in order: the layers (top first), or on a model each texture set's name and
/// meshes, the open mesh's layers under it when its set is the one `painted` on.
fn items<'a>(
    shown: &[&'a LayerInfo],
    model: Option<&ModelInfo>,
    open_mesh: Option<usize>,
    painted: u32,
) -> Vec<Item<'a>> {
    let Some(model) = model else {
        let last = shown.len().saturating_sub(1);
        return shown
            .iter()
            .enumerate()
            .map(|(i, l)| Item::Layer {
                l,
                // the list's ends round; the rows between are square
                corners: (if i == 0 { LIST_CORNER } else { 0.0 }, if i == last { LIST_CORNER } else { 0.0 }),
                indent: 0.0,
                shared: false,
            })
            .collect();
    };
    let mut out = Vec::new();
    for (set, name) in model.texture_sets.iter().enumerate() {
        let meshes: Vec<(usize, &procraft_core::SubmeshInfo)> =
            model.submeshes.iter().enumerate().filter(|(_, m)| m.texture_set as usize == set).collect();
        if meshes.is_empty() {
            continue;
        }
        out.push(Item::Set { name: name.clone(), first: set == 0 });
        let count = meshes.len();
        for (k, (index, m)) in meshes.into_iter().enumerate() {
            let last_mesh = k + 1 == count;
            let under = open_mesh == Some(index) && set as u32 == painted && !shown.is_empty();
            let corners = (if k == 0 { LIST_CORNER } else { 0.0 }, if last_mesh && !under { LIST_CORNER } else { 0.0 });
            out.push(Item::Mesh { index, name: m.name.clone(), open: under, corners });
            if under {
                let n = shown.len();
                for (i, l) in shown.iter().enumerate() {
                    let end = last_mesh && i + 1 == n;
                    let corners = (0.0, if end { LIST_CORNER } else { 0.0 });
                    out.push(Item::Layer { l, corners, indent: GROUP_INDENT, shared: count > 1 });
                }
            }
        }
    }
    out
}

impl Layers {
    /// Another artwork is shown: its thumbnails are read anew.
    pub fn forget(&mut self) {
        self.thumbnails.clear();
        self.options = None;
        self.options_menu.close();
        self.control_menu.close();
        self.control_for = None;
        self.control_pending = false;
        self.title_menu.close();
        self.add_menu.close();
        self.renaming = None;
        self.blend = None;
        self.press = None;
        self.slid.clear();
        self.swiped = None;
        self.open_mesh = None;
        self.tray_for = None;
        self.channel = 0;
        self.channel_colours = [None; 3];
        self.mesh_pictures.clear();
        self.channel_pictures.clear();
    }

    /// A menu of the panel's is open: a press elsewhere only closes it.
    pub fn modal(&self) -> bool {
        self.options_menu.is_open()
            || self.title_menu.is_open()
            || self.add_menu.is_open()
            || self.deleting.is_open()
            || self.deleting_shared.is_open()
            || self.locked.is_open()
    }

    /// Shows the panel hanging from its button, or floating in a window of its own, as `hang`
    /// says; returns where it is (points).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        colours: &mut Colours,
        hang: panel::Hang,
        backdrops: &mut Vec<f32>,
    ) -> Rect {
        let screen = panel::screen(ctx);
        let (left, arrow_x) = match hang {
            panel::Hang::Button(button) => panel::place(ctx, LAYERS_WIDTH, button.center().x),
            panel::Hang::Floating(window) => (window.left(), 0.0),
        };
        // on a model: its meshes pictured as it is seen when the panel opens
        let model = session.model_info();
        let chosen_tile = model.is_some() && session.channel_tile_chosen();
        let frame = ctx.cumulative_frame_nr();
        if frame > self.last_frame + 1 {
            self.mesh_pictures.clear();
            self.channel_pictures.clear();
        }
        self.last_frame = frame;
        let top = screen.top() + pc::BAR_HEIGHT;
        let layers = session.layers();
        let versions: HashMap<u64, u64> = session.layer_versions().into_iter().map(|v| (v.id, v.version)).collect();
        let modes = procraft_core::blend_modes();
        // a closed group's layers are left out
        let collapsed: Vec<u64> =
            layers.iter().filter(|l| l.kind == LayerType::Group && l.collapsed).map(|l| l.id).collect();
        let parents: HashMap<u64, Option<u64>> = layers.iter().map(|l| (l.id, l.parent)).collect();
        let hidden = |l: &LayerInfo| {
            let mut at = l.parent;
            for _ in 0..64 {
                let Some(p) = at else { return false };
                if collapsed.contains(&p) {
                    return true;
                }
                at = parents.get(&p).copied().flatten();
            }
            false
        };
        let shown: Vec<&LayerInfo> = layers.iter().filter(|l| !hidden(l)).collect();
        self.closed_contents = closed_contents(&layers);
        let masked = |l: &LayerInfo| layers.iter().any(|m| m.kind == LayerType::Mask && m.parent == Some(l.id));
        // a closed group's row grows as it closes
        let height_of = |l: &LayerInfo| match l.kind {
            LayerType::Group => {
                let closed =
                    ctx.animate_bool_with_time(egui::Id::unique(("group closed", l.id)), l.collapsed, GROUP_TURN);
                GROUP_HEIGHT + (GROUP_CLOSED_HEIGHT - GROUP_HEIGHT) * closed
            }
            LayerType::Mask => MASKED_ROW_HEIGHT,
            LayerType::Paint if masked(l) => MASKED_ROW_HEIGHT,
            LayerType::Paint => ROW_HEIGHT,
        };
        // each row's blend editor, open or opening or closing, and how far
        let open_blend = self.blend.as_ref().map(|b| b.id);
        let blend_shown: HashMap<u64, f32> = shown
            .iter()
            .filter(|l| l.kind != LayerType::Mask)
            .map(|l| {
                let open = open_blend == Some(l.id);
                let id = egui::Id::unique(("blend editor", l.id));
                let k = ctx.animate_bool_with_time(id, open, if open { BLEND_OPEN } else { BLEND_CLOSE });
                (l.id, ease(k))
            })
            .filter(|(_, k)| *k > 0.0)
            .collect();
        let editors: f32 = blend_shown.values().map(|k| k * BLEND_EDITOR_HEIGHT).sum();
        self.settle_slides(ctx, &shown);
        let items = items(&shown, model.as_ref(), self.open_mesh, session.texture_set());
        let tray_for = self.tray_for.filter(|_| model.is_some());
        let item_height = |item: &Item| match item {
            Item::Set { first, .. } => SET_HEADER + if *first { 0.0 } else { SET_GAP },
            Item::Mesh { .. } => ROW_HEIGHT + ROW_GAP,
            Item::Layer { l, .. } => height_of(l) + ROW_GAP + if tray_for == Some(l.id) { TRAY_HEIGHT } else { 0.0 },
        };
        // a model has no background row; the plugins' actions' rows come under it, each as tall
        let below = if model.is_some() {
            12.0
        } else {
            10.0 + 50.0 + self.panel_actions.len() as f32 * (10.0 + ACTION_ROW) + 12.0
        };
        let rows: f32 = items.iter().map(item_height).sum::<f32>() + editors + below;
        // as tall as its rows, up to the window's bottom less a gap
        let most = screen.bottom() - panel::POPOVER_GAP - top;
        let height = (panel::ARROW_HEIGHT + HEADER_HEIGHT + rows).min(most);
        let (rect, area) = match hang {
            panel::Hang::Button(_) => {
                let rect = Rect::from_min_size(pos2(left, top), vec2(LAYERS_WIDTH, height));
                let rect = panel::hung_rect(ctx, rect, panel::ARROW_HEIGHT + HEADER_HEIGHT + rows, panel::POPOVER_GAP);
                (rect, panel::area(ctx, egui::Id::unique("layers panel"), rect))
            }
            // floating, as big as its window, whatever its rows
            panel::Hang::Floating(window) => {
                (panel::floating_rect(window), panel::floating_area(ctx, egui::Id::unique("floating layers"), window))
            }
        };
        let any_picked = layers.iter().any(|l| l.picked);
        let current_mask_parent = layers.iter().find(|l| l.current && l.kind == LayerType::Mask).and_then(|l| l.parent);
        let current = layers.iter().find(|l| l.current).map(|l| l.id);
        // where the rows are, for the options to sit by theirs
        let mut row_rects: HashMap<u64, Rect> = HashMap::new();
        let mut tapped: Option<&LayerInfo> = None;
        let mut blend_tapped: Option<u64> = None;
        let mut title = Rect::NOTHING;
        let mut plus = Rect::NOTHING;
        // the only layer (masks aside) is cleared rather than deleted
        let alone = layers.iter().filter(|l| l.kind != LayerType::Mask).count() == 1;

        area.show(ctx, |ui| {
            match hang {
                panel::Hang::Button(_) => {
                    ui.allocate_rect(rect, Sense::click());
                    panel::frame(ui, look, "layers frame", rect, arrow_x, backdrops);
                    let body = panel::body_in(ui.ctx(), rect);
                    self.undock = if self.floats { panel::undock_grabber(ui, body) } else { None };
                }
                panel::Hang::Floating(window) => {
                    ui.allocate_rect(window, Sense::click());
                    panel::floating_panel_frame(ui, look, "floating layers frame", window, backdrops);
                    self.chrome = panel::floating_chrome(ui, window);
                }
            }
            let body = panel::body_in(ui.ctx(), rect);
            // the chevron turns blue while layers are picked
            let (title_tapped, row) = panel::header_title(ui, look, body, "Layers", 24.0, true, any_picked);
            title = row;
            if title_tapped {
                self.title_menu.open();
            }
            plus = Rect::from_min_size(pos2(body.right() - 18.0 - 40.0, body.top() + 12.0), vec2(40.0, 41.0));
            look.icon(ui, &icons::PLUS_MEDIUM, plus.center(), 41.0, argb(PANEL_GLYPH));
            if ui.interact(plus, ui.scope_id().with("new layer"), Sense::click()).clicked() {
                // with plugins' kinds of layer or filter layers on, it asks which
                if !self.layer_kinds.is_empty() || !self.layer_filters.is_empty() {
                    self.add_menu.open();
                } else if session.add_layer() == 0 {
                    crate::files::no_room(session);
                }
            }
            let list = Rect::from_min_max(
                pos2(body.left() + 12.0, body.top() + HEADER_HEIGHT),
                pos2(body.right() - 12.0, body.bottom()),
            );
            let mut read = 0;
            self.list = Some(list);
            ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
                ScrollArea::vertical()
                    .id_salt("layers list")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        // at once: a finger's drag or a dragged row's at the edge, each frame
                        ui.scroll_with_delta_animation(
                            vec2(0.0, -std::mem::take(&mut self.scroll_by)),
                            egui::style::ScrollAnimation::none(),
                        );
                        let mut dragged = Vec::new();
                        let mut squeezed = Vec::new();
                        for item in &items {
                            let (l, (t, b), indent, shared) = match item {
                                Item::Set { name, first } => {
                                    set_header(ui, name, *first);
                                    continue;
                                }
                                Item::Mesh { index, name, open, corners } => {
                                    self.submesh_row(ui, look, session, colours, *index, name, *open, *corners);
                                    ui.add_space(ROW_GAP);
                                    continue;
                                }
                                Item::Layer { l, corners, indent, shared } => (*l, *corners, *indent, *shared),
                            };
                            let height = height_of(l);
                            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
                            row_rects.insert(l.id, row);
                            // the current mask's layer, or the current layer's mask
                            let owner = current_mask_parent == Some(l.id)
                                || (l.kind == LayerType::Mask && l.parent.is_some() && l.parent == current);
                            // on a model with a channel's tile chosen, the tile is blue, not the row
                            let chosen = l.current && !chosen_tile;
                            let fill = if chosen {
                                argb(pc::ACCENT)
                            } else if l.picked {
                                argb(PICKED_ROW)
                            } else if owner {
                                argb(MASK_OWNER_ROW)
                            } else {
                                argb(LAYER_ROW)
                            };
                            let looks = RowLook {
                                corners: (t, b),
                                fill,
                                version: versions.get(&l.id).copied(),
                                masked: masked(l),
                                indent,
                                shared,
                                cube: (model.is_some() && l.kind != LayerType::Mask)
                                    .then_some(l.current && chosen_tile),
                                chosen,
                                held: self.drop_at == Some(RowDrop::Onto { target: l.id }),
                            };
                            if self.dragged_rows.contains(&l.id) {
                                // held and dragged: drawn where it is dragged to, over the rest
                                dragged.push((l, row, looks));
                            } else if let Some((dy, upper)) = self.pinch.as_ref().and_then(|p| p.moved(l.id)) {
                                // pinched: drawn after the rest, moved part of its finger's way
                                squeezed.push((l, row.translate(vec2(0.0, dy)), looks, upper));
                            } else {
                                let slide = self.slid.get(&l.id).copied().unwrap_or(0.0);
                                let way = self.making_way(l.id);
                                let aside =
                                    ui.ctx().animate_value_with_time(ui.scope_id().with(("aside", l.id)), way, 0.3);
                                let row = row.translate(vec2(0.0, aside));
                                let at = row.translate(vec2(slide, 0.0));
                                if slide < 0.0 {
                                    self.swipe_buttons(ui, session, l, row, alone);
                                }
                                let id = ui.scope_id().with(("row", l.id));
                                let response = ui.interact(at.intersect(row), id, Sense::click_and_drag());
                                if response.is_pointer_button_down_on()
                                    && self.press.is_none()
                                    && let Some(origin) =
                                        ui.input(|i| i.pointer.interact_pos()).map(|p| panel::in_panel(ctx, p))
                                {
                                    let since = Instant::now();
                                    let way = Way::Undecided;
                                    // a tablet's pen moving the mouse pointer goes as a finger
                                    let mouse = !ui.input(|i| i.any_touches()) && crate::pen::mouse_pen().is_none();
                                    self.press =
                                        Some(RowPress { id: l.id, origin, since, way, start: slide, dy: 0.0, mouse });
                                }
                                let mut row_ui = ui.new_child(UiBuilder::new().max_rect(row));
                                row_ui.set_clip_rect(row.intersect(ui.clip_rect()));
                                if self.draw_row(&mut row_ui, look, session, l, at, looks, &modes, &mut read) {
                                    blend_tapped = Some(l.id);
                                }
                                // a tap, not a slide or a hold
                                if response.clicked() && self.press.is_none_or(|p| p.way == Way::Undecided) {
                                    tapped = Some(l);
                                    self.swiped = None;
                                }
                            }
                            if tray_for == Some(l.id) {
                                let (tray, _) =
                                    ui.allocate_exact_size(vec2(ui.available_width(), TRAY_HEIGHT), Sense::hover());
                                self.channel_tray(ui, session, colours, l, tray);
                            }
                            ui.add_space(ROW_GAP);
                            // its blend modes and opacity open under it, the panel growing as
                            // they come
                            if let Some(&k) = blend_shown.get(&l.id) {
                                let (space, _) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), k * BLEND_EDITOR_HEIGHT),
                                    Sense::hover(),
                                );
                                let editor = Rect::from_min_size(space.min, vec2(space.width(), BLEND_EDITOR_HEIGHT));
                                let mut ui = ui.new_child(UiBuilder::new().max_rect(editor));
                                ui.set_clip_rect(space.intersect(ui.clip_rect()));
                                ui.multiply_opacity(k);
                                self.blend_editor(&mut ui, look, session, l, editor, &modes);
                            }
                        }
                        if model.is_none() {
                            ui.add_space(10.0);
                            self.pick_background |= background_row(ui, look, session);
                            for action in &self.panel_actions {
                                ui.add_space(10.0);
                                if action_row(ui, &action.label) {
                                    self.run_action = Some(action.key.clone());
                                }
                            }
                        }
                        ui.add_space(12.0);
                        // the pinched rows over the rest, the upper over the lower
                        squeezed.sort_by_key(|s| s.3);
                        for (l, at, looks, _) in squeezed {
                            let mut row_ui = ui.new_child(UiBuilder::new().max_rect(at));
                            self.draw_row(&mut row_ui, look, session, l, at, looks, &modes, &mut read);
                        }
                        for (l, row, looks) in dragged {
                            let at = row.translate(vec2(0.0, self.press.map_or(0.0, |p| p.dy)));
                            let mut row_ui = ui.new_child(UiBuilder::new().max_rect(at));
                            self.draw_row(&mut row_ui, look, session, l, at, looks, &modes, &mut read);
                        }
                    });
            });
        });
        self.follow_press(ctx, session, &layers, &row_rects, rect);
        self.last_rows.clone_from(&row_rects);
        let to_window = panel::to_window(ctx);
        self.box_on_window = Some(to_window.mul_rect(rect));
        self.rows_on_window = row_rects.iter().map(|(id, r)| (*id, to_window.mul_rect(*r))).collect();
        self.follow_pinch(ctx, session, &row_rects);
        // a layer tapped: picked layers give way to it; the current one shows its options (or
        // hides them); any other becomes the current one
        if let Some(l) = tapped {
            if chosen_tile && l.kind != LayerType::Group {
                // on a model the row means the layer itself: its colour, no channel's tile
                session.leave_channel_tiles();
                self.follow_channel(session, colours);
                if !l.current {
                    session.select_layer(l.id);
                }
            } else if any_picked || !l.current {
                session.select_layer(l.id);
                self.options_menu.close();
                self.control_menu.close();
                self.control_for = None;
            } else if self.options_menu.is_open() {
                self.options_menu.close();
            } else {
                self.options = Some(l.id);
                self.options_menu.open();
                self.control_menu.close();
                self.control_for = None;
                if self.blend.take().is_some() {
                    session.finish_layer_blend();
                }
            }
        }
        if let Some(id) = blend_tapped {
            self.options_menu.close();
            self.control_menu.close();
            self.control_for = None;
            let open = self.blend.as_ref().is_some_and(|b| b.id == id);
            if self.blend.is_some() {
                session.finish_layer_blend();
            }
            self.blend = (!open).then(|| {
                let at = layers
                    .iter()
                    .find(|l| l.id == id)
                    .map_or(0, |l| modes.iter().position(|m| m.id == l.blend).unwrap_or(0));
                Blend { id, scroll: at as f32 * BLEND_ROW, motion: BlendMotion::Resting }
            });
        }
        if let Some(l) = self.options.and_then(|id| layers.iter().find(|l| l.id == id)) {
            let row_y = row_rects.get(&l.id).map_or(rect.center().y, |r| r.center().y);
            self.layer_options(ctx, look, session, l, rect, row_y, backdrops);
        }
        self.control_options(ctx, look, session, backdrops);
        self.title_menu(ctx, look, session, title, any_picked, backdrops);
        self.add_menu(ctx, look, session, plus, backdrops);
        // the title menu's Delete asks first
        // asked first; then, as measured, a locked one among them stops it, said why
        if self.deleting.show(ctx, look) == Some(Answer::Action) {
            if session.chosen_locked() {
                self.locked.tell("Locked layer selection", LOCKED_DELETE, Some(&icons::PAGE_LOCKED));
            } else {
                session.delete_picked();
            }
        }
        if self.deleting_shared.show(ctx, look) == Some(Answer::Action)
            && let Some(id) = self.shared_going.take()
        {
            _ = session.delete_layer(id);
        }
        self.locked.show(ctx, look);
        match hang {
            panel::Hang::Button(_) => rect,
            panel::Hang::Floating(window) => window,
        }
    }

    /// The title's menu, under it: Select All; Group and Merge (with layers picked); Share and
    /// Duplicate; Delete (asking first).
    fn title_menu(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        title: Rect,
        picked: bool,
        backdrops: &mut Vec<f32>,
    ) {
        let ink = menu::INK;
        let mut entries = vec![Entry::Item(&icons::MENU_SELECT_ALL, "Select All", ink), menu::DIVIDER];
        if picked {
            entries.push(Entry::Item(&icons::MENU_GROUP, "Group", ink));
            entries.push(Entry::Item(&icons::MENU_MERGE, "Merge", ink));
            entries.push(menu::DIVIDER);
        }
        entries.push(Entry::Item(&icons::MENU_SHARE, "Share", ink));
        entries.push(Entry::Item(&icons::MENU_DUPLICATE, "Duplicate", ink));
        entries.push(menu::DIVIDER);
        entries.push(Entry::Item(&icons::MENU_TRASH, "Delete", menu::DANGER));
        let size = vec2(menu::LIBRARY_WIDTH, menu::entries_height(&entries));
        let screen = panel::screen(ctx);
        let left = (title.center().x - size.x / 2.0).clamp(screen.left(), (screen.right() - size.x).max(screen.left()));
        let rect = Rect::from_min_size(pos2(left, title.bottom().max(screen.top() + OPTIONS_TOP)), size);
        let mut tapped = None;
        self.title_menu.show(
            ctx,
            "layers title menu",
            rect,
            pos2(rect.center().x, rect.top()),
            menu::OVER_A_PANEL,
            backdrops,
            |ui| {
                tapped = menu::entries(ui, look, rect, &entries);
            },
        );
        let Some(Entry::Item(_, label, _)) = tapped.and_then(|i| entries.get(i)) else { return };
        self.title_menu.close();
        match *label {
            "Select All" => _ = session.pick_all_layers(),
            "Group" => _ = session.group_picked(),
            "Merge" => _ = session.merge_picked(),
            "Share" => self.share = true,
            "Duplicate" => {
                if !session.duplicate_picked() {
                    crate::files::no_room(session);
                }
            }
            _ => self.deleting.open("Delete these layers?", "Undo brings them back.", "Delete", Some(&icons::TRASH)),
        }
    }

    /// The +'s menu, under it, while plugins add kinds of layer: a plain layer, then one of each
    /// kind, named as its plugin names it.
    fn add_menu(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        plus: Rect,
        backdrops: &mut Vec<f32>,
    ) {
        if self.layer_kinds.is_empty() && self.layer_filters.is_empty() {
            self.add_menu.close();
        }
        let entries = add_entries(&self.layer_kinds, !self.layer_filters.is_empty());
        let size = vec2(menu::LIBRARY_WIDTH, menu::entries_height(&entries));
        let screen = panel::screen(ctx);
        let left = (plus.right() - size.x).clamp(screen.left(), (screen.right() - size.x).max(screen.left()));
        let rect = Rect::from_min_size(pos2(left, plus.bottom().max(screen.top() + OPTIONS_TOP)), size);
        let mut tapped = None;
        self.add_menu.show(
            ctx,
            "layers add menu",
            rect,
            pos2(rect.right(), rect.top()),
            menu::OVER_A_PANEL,
            backdrops,
            |ui| {
                tapped = menu::entries(ui, look, rect, &entries);
            },
        );
        let Some(at) = tapped.filter(|i| *i < entries.len()) else { return };
        self.add_menu.close();
        // the last, with filter layers offered: their list
        if !self.layer_filters.is_empty() && at + 1 == entries.len() {
            self.filter_list = true;
            return;
        }
        let added = match at.checked_sub(1).and_then(|k| self.layer_kinds.get(k)) {
            Some(kind) => session.add_plugin_layer(kind.key.clone()),
            None => session.add_layer(),
        };
        if added == 0 {
            crate::files::no_room(session);
        }
    }

    /// A row's fill and what is on it, at `at` (where it is slid or dragged to); true if its blend
    /// letter was tapped.
    #[allow(clippy::too_many_arguments)]
    fn draw_row(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        l: &LayerInfo,
        at: Rect,
        looks: RowLook,
        modes: &[BlendModeInfo],
        read: &mut usize,
    ) -> bool {
        let (t, b) = looks.corners;
        ui.painter().rect_filled(
            at,
            egui::CornerRadius { nw: t as u8, ne: t as u8, sw: b as u8, se: b as u8 },
            looks.fill,
        );
        // the colour a plugin labels it with, down its left edge
        if let Some(label) = l.label {
            let edge = Rect::from_min_size(at.min, vec2(LABEL_WIDTH, at.height()));
            let corners = egui::CornerRadius { nw: t as u8, sw: b as u8, ..Default::default() };
            ui.painter().rect_filled(edge, corners, labelled(label));
        }
        let ink = if looks.chosen || l.picked { Color32::WHITE } else { argb(PANEL_TITLE) };
        let box_fill = if looks.chosen || l.picked { Color32::WHITE } else { argb(CHECK_FILL) };
        if l.kind == LayerType::Group {
            self.group_row(ui, look, session, l, at.translate(vec2(looks.indent, 0.0)), ink, box_fill, looks.held);
            false
        } else {
            self.layer_row(ui, look, session, l, at, ink, box_fill, looks, modes, read)
        }
    }

    /// One of a model's meshes: its picture, its name, the arrow that opens it (its texture set's
    /// layers under it) and its box (shown or hidden). A click anywhere but the box opens or
    /// closes it.
    #[allow(clippy::too_many_arguments)]
    fn submesh_row(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        colours: &mut Colours,
        index: usize,
        name: &str,
        open: bool,
        (t, b): (f32, f32),
    ) {
        let (place, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click_and_drag());
        // slid left it shows Lock (or Unlock); a little right, it joins the meshes picked
        let reach = SWIPE_BUTTON + 6.0;
        let locked = session.submeshes_locked().get(index).copied().unwrap_or(false);
        let swiped = self.mesh_swiped == Some(index);
        let mut slide = self.mesh_slid.get(&index).copied().unwrap_or(if swiped { -reach } else { 0.0 });
        let pressed = ui.input(|i| i.pointer.press_origin().filter(|_| i.pointer.primary_down()));
        let pressed = pressed.map(|p| panel::in_panel(ui.ctx(), p));
        if let Some(origin) = pressed.filter(|p| self.mesh_press.is_none() && place.contains(*p)) {
            self.mesh_press = Some((index, origin.x, slide));
        }
        let mut let_go = false;
        if let Some((_, origin, start)) = self.mesh_press.filter(|p| p.0 == index) {
            let (down, at) = ui.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos()));
            if let Some(at) = at.filter(|_| down).map(|p| panel::in_panel(ui.ctx(), p)) {
                slide = (start + at.x - origin).clamp(-reach, if swiped { 0.0 } else { PICK_REACH });
            }
            let_go = !down;
        }
        if let_go {
            self.mesh_press = None;
            if slide > PICK_REACH / 2.0 {
                if !session.pick_more_submeshes(index as u32) {
                    self.notice =
                        Some(("Those meshes aren't in the same texture set".to_owned(), true, NoticeKind::Problem));
                }
                self.mesh_swiped = None;
            } else {
                self.mesh_swiped = (slide < -reach / 2.0).then_some(index);
            }
            slide = if self.mesh_swiped == Some(index) { -reach } else { 0.0 };
        }
        self.mesh_slid.insert(index, slide);
        if slide < 0.0 {
            let rect = Rect::from_min_max(pos2(place.right() - SWIPE_BUTTON, place.top()), place.max);
            let label = if locked { "Unlock" } else { "Lock" };
            ui.painter().rect_filled(rect, 8, argb(SWIPE_FILL));
            text(ui.painter(), rect.center(), Align2::CENTER_CENTER, label, 15.0, Weight::Medium, argb(SWIPE_INK));
            if ui.interact(rect, ui.scope_id().with(("mesh lock", index)), Sense::click()).clicked() {
                session.set_submesh_locked(index as u32, !locked);
                self.notice =
                    Some((if locked { "Unlock mesh" } else { "Lock mesh" }.to_owned(), false, NoticeKind::Done));
                self.mesh_swiped = None;
                self.mesh_slid.remove(&index);
            }
        }
        let row = place.translate(vec2(slide, 0.0));
        // slid, it stops at the list's edge
        let clip = ui.clip_rect();
        ui.set_clip_rect(clip.intersect(place));
        let corners = egui::CornerRadius { nw: t as u8, ne: t as u8, sw: b as u8, se: b as u8 };
        let picked = session.active_submeshes().contains(&(index as u32));
        ui.painter().rect_filled(row, corners, if open || picked { argb(SUBMESH_OPEN) } else { argb(LAYER_ROW) });
        let tile =
            Rect::from_center_size(pos2(row.left() + 4.0 + THUMB.0 / 2.0, row.center().y), vec2(THUMB.0, THUMB.1));
        ui.painter().rect_filled(tile, 4, argb(SUBMESH_TILE));
        if !self.mesh_pictures.contains_key(&index)
            && ui.is_rect_visible(tile)
            && let Some(t) = session.submesh_thumbnail(index as u32, THUMB_SIDE)
        {
            let image = egui::ColorImage::from_rgba_premultiplied([t.width as usize, t.height as usize], &t.rgba);
            let texture = ui.ctx().load_texture(format!("mesh {index}"), image, TextureOptions::LINEAR);
            self.mesh_pictures.insert(index, texture);
        }
        if let Some(texture) = self.mesh_pictures.get(&index) {
            fit_picture(ui, texture, tile.shrink(4.0));
        }
        let ink = argb(PANEL_TITLE);
        let visibility = Rect::from_min_size(pos2(row.right() - 8.0 - 44.0, row.center().y - 23.5), vec2(44.0, 47.0));
        let arrow = Rect::from_min_size(pos2(visibility.left() - 44.0 + 8.0, row.center().y - 25.5), vec2(44.0, 51.0));
        let left = tile.right() + 29.0;
        let g = galley(ui, name, 17.0, Weight::Regular, ink, arrow.left() - 6.0 - left);
        ui.painter().galley(pos2(left, row.center().y - g.size().y / 2.0), g, ink);
        look.icon(ui, if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, arrow.center(), 16.0, ink);
        if locked {
            look.icon(ui, &icons::PAGE_LOCKED, pos2(arrow.left() - 4.0, row.center().y), 16.0, ink);
        }
        let hidden = session.submeshes_hidden().get(index).copied().unwrap_or(false);
        check_box(ui, look, visibility.center(), !hidden, argb(CHECK_FILL));
        if ui.interact(visibility, ui.scope_id().with(("mesh box", index)), Sense::click()).clicked() {
            session.set_submesh_hidden(index as u32, !hidden);
            self.notice =
                Some((if hidden { "Show Submesh" } else { "Hide Submesh" }.to_owned(), false, NoticeKind::Done));
        } else if response.clicked() {
            self.open_mesh(session, colours, index);
        }
        ui.set_clip_rect(clip);
    }

    /// Layer `id`'s content in `channel` replaced by a picture chosen now.
    fn replace_map(&mut self, session: &CanvasSession, colours: &mut Colours, id: u64, channel: u32) {
        let Some(path) = crate::files::dialog().set_title("Replace Material Map Content").pick_file() else { return };
        let Ok(picture) = image::open(&path) else {
            self.notice = Some(("This picture can't be opened.".to_owned(), true, NoticeKind::Problem));
            return;
        };
        let rgba = picture.to_rgba8();
        let image = ThumbnailImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() };
        if session.replace_material_map(id, channel, image) {
            self.notice = Some(("Replace Material Map Content".to_owned(), false, NoticeKind::Done));
            self.channel_pictures.retain(|(_, l), _| *l != id);
            self.follow_channel(session, colours);
        }
    }

    /// The mesh picked on the model is the one open here.
    pub fn follow_picked_mesh(&mut self, session: &CanvasSession) {
        self.open_mesh = session.active_submeshes().first().map(|&i| i as usize);
    }

    /// Opens mesh `index` (its texture set's layers under it, that set painted on), or closes it.
    fn open_mesh(&mut self, session: &CanvasSession, colours: &mut Colours, index: usize) {
        if self.open_mesh == Some(index) {
            self.open_mesh = None;
            return;
        }
        let set = session.model_info().and_then(|m| m.submeshes.get(index).map(|s| s.texture_set));
        let Some(set) = set else { return };
        self.open_mesh = Some(index);
        if set != session.texture_set() {
            // a texture set is opened on its colour, no tile chosen
            self.choose_channel(session, colours, 0);
            self.tray_for = None;
            session.choose_texture_set(set);
        }
        session.pick_submesh(index as u32);
    }

    /// Paints on `channel` of the texture set from now on (its layers stay the layers), with that
    /// channel's own colour.
    fn choose_channel(&mut self, session: &CanvasSession, colours: &mut Colours, channel: u32) {
        if channel == session.model_channel() || channel > 2 {
            return;
        }
        session.set_model_channel(channel);
        self.follow_channel(session, colours);
    }

    /// The channel painted on as the session has it now (a tile chosen, or its choice undone or
    /// redone), shown with its own colour: roughness and metalness start black.
    pub fn follow_channel(&mut self, session: &CanvasSession, colours: &mut Colours) {
        let now = session.model_channel();
        if now == self.channel {
            return;
        }
        if let Some(slot) = self.channel_colours.get_mut(self.channel as usize) {
            *slot = Some(colours.hsv);
        }
        self.channel = now;
        let kept = self.channel_colours.get(now as usize).copied().flatten();
        if let Some(hsv) = kept.or((now != 0).then_some([0.0, 0.0, 0.0])) {
            colours.hsv = hsv;
            colours.send(session);
        }
    }

    /// Shows layer `id`'s channels under it (or hides them), pictured afresh.
    fn toggle_tray(&mut self, id: u64) {
        self.tray_for = if self.tray_for == Some(id) { None } else { Some(id) };
        self.channel_pictures.retain(|(_, l), _| *l != id);
    }

    /// A model's layer's channels in `tray`: Color, Roughness and Metallic, each its picture of
    /// the layer; the one painted on (of the current layer) blue. A click paints on it.
    fn channel_tray(&mut self, ui: &Ui, session: &CanvasSession, colours: &mut Colours, l: &LayerInfo, tray: Rect) {
        ui.painter().rect_filled(tray, 0, argb(TRAY_FILL));
        let inner = tray.shrink(4.0);
        let w = (inner.width() - 12.0) / 3.0;
        let painted = session.model_channel();
        let tile_chosen = session.channel_tile_chosen();
        let now = session.layer_versions().into_iter().find(|v| v.id == l.id).map(|v| (l.id, v.version));
        if now != self.channel_pictured {
            self.channel_pictured = now;
            self.channel_pictures.retain(|(_, id), _| *id != l.id);
        }
        for (c, label) in CHANNELS.iter().enumerate() {
            let c = c as u32;
            let tile =
                Rect::from_min_size(pos2(inner.left() + (w + 6.0) * c as f32, inner.top()), vec2(w, inner.height()));
            let chosen = l.current && tile_chosen && painted == c;
            ui.painter().rect_filled(tile, 6, if chosen { argb(pc::ACCENT) } else { argb(TILE_FILL) });
            let picture = Rect::from_min_size(tile.min + vec2(6.0, 6.0), vec2(w - 12.0, 102.0));
            ui.painter().rect_filled(picture, 3, argb(THUMB_BACKGROUND));
            if !self.channel_pictures.contains_key(&(c, l.id))
                && let Some(t) = session.channel_layer_thumbnail(c, l.id, THUMB_SIDE)
            {
                let image = egui::ColorImage::from_rgba_premultiplied([t.width as usize, t.height as usize], &t.rgba);
                let texture = ui.ctx().load_texture(format!("channel {c} {}", l.id), image, TextureOptions::LINEAR);
                self.channel_pictures.insert((c, l.id), texture);
            }
            if let Some(texture) = self.channel_pictures.get(&(c, l.id)) {
                fit_picture(ui, texture, picture);
            }
            let ink = if chosen { Color32::WHITE } else { argb(PANEL_TITLE) };
            text(
                ui.painter(),
                pos2(picture.left() + 2.0, picture.bottom() + 9.0),
                Align2::LEFT_TOP,
                label,
                15.0,
                Weight::Regular,
                ink,
            );
            let pressed = ui.interact(tile, ui.scope_id().with(("channel tile", c, l.id)), Sense::click());
            if pressed.clicked() {
                if !l.current {
                    session.select_layer(l.id);
                }
                session.choose_channel_tile(c);
                self.follow_channel(session, colours);
            }
            // the other button: a picture replaces what the channel holds
            if pressed.secondary_clicked() {
                self.replace_map(session, colours, l.id, c);
            }
        }
    }

    /// The buttons a row slid left shows behind it: Lock (or Unlock), Duplicate (not for a mask)
    /// and Delete (Clear for the only layer).
    fn swipe_buttons(&mut self, ui: &Ui, session: &CanvasSession, l: &LayerInfo, row: Rect, alone: bool) {
        let mut buttons = vec![(if l.locked { "Unlock" } else { "Lock" }, SWIPE_FILL, SWIPE_INK)];
        if l.kind != LayerType::Mask {
            buttons.push(("Duplicate", SWIPE_FILL, SWIPE_INK));
        }
        buttons.push((if alone { "Clear" } else { "Delete" }, SWIPE_DELETE, 0xFFFFFFFF));
        let mut right = row.right();
        for (label, fill, ink) in buttons.into_iter().rev() {
            let rect = Rect::from_min_max(pos2(right - SWIPE_BUTTON, row.top()), pos2(right, row.bottom()));
            right -= SWIPE_BUTTON + SWIPE_GAP;
            ui.painter().rect_filled(rect, 8, argb(fill));
            text(ui.painter(), rect.center(), Align2::CENTER_CENTER, label, 15.0, Weight::Medium, argb(ink));
            if ui.interact(rect, ui.scope_id().with(("swipe", l.id, label)), Sense::click()).clicked() {
                match label {
                    "Lock" | "Unlock" => {
                        let said = session.lock_said(l.id, !l.locked);
                        if session.set_layer_locked(l.id, !l.locked) {
                            self.notice = Some((said, false, NoticeKind::Done));
                        }
                    }
                    "Duplicate" => _ = session.duplicate_layer(l.id),
                    "Clear" => {
                        session.select_layer(l.id);
                        session.clear_layer();
                    }
                    // as measured, a locked one is told about, not deleted
                    _ if l.locked => {
                        self.locked.tell("Locked layer selection", LOCKED_DELETE, Some(&icons::PAGE_LOCKED))
                    }
                    _ if shared_by_meshes(session) => {
                        self.shared_going = Some(l.id);
                        let on_all = format!("\u{201c}{}\u{201d} is on every mesh of this texture set.", l.name);
                        self.deleting_shared.open("Delete shared layer?", &on_all, "Delete", Some(&icons::TRASH));
                    }
                    _ => _ = session.delete_layer(l.id),
                }
                self.swiped = None;
            }
        }
    }

    /// Two fingers put on two rows and pinched together to [`PINCH_MERGE`] of how far apart they
    /// were: as a finger lets go, those layers and every one between merge (as on the tablet).
    fn follow_pinch(&mut self, ctx: &egui::Context, session: &CanvasSession, rows: &HashMap<u64, Rect>) {
        let touches: Vec<(egui::TouchId, egui::TouchPhase, Pos2)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Touch { id, phase, pos, .. } => Some((*id, *phase, *pos)),
                    _ => None,
                })
                .collect()
        });
        for (id, phase, pos) in touches {
            match phase {
                egui::TouchPhase::Start | egui::TouchPhase::Move => {
                    self.touches.insert(id.0, pos);
                }
                egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                    self.touches.remove(&id.0);
                    if let Some(p) = self.pinch.take()
                        && p.pinched
                        && phase == egui::TouchPhase::End
                    {
                        session.merge_between(p.upper, p.lower);
                    }
                }
            }
        }
        let mut fingers: Vec<(u64, Pos2)> = self.touches.iter().map(|(id, p)| (*id, *p)).collect();
        fingers.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
        let [(top_id, top), (bottom_id, bottom)] = fingers[..] else {
            // let go: two fingers that tapped a row, still and quick, ask for its Layer Opacity
            if let Some(t) = self.twin.take()
                && !t.done
                && !t.moved
                && t.since.elapsed().as_secs_f32() <= TWIN_TAP_S
            {
                session.select_layer(t.id);
                self.layer_opacity = true;
            }
            return;
        };
        let row_at = |p: Pos2| rows.iter().find(|(_, r)| r.contains(p)).map(|(id, _)| *id);
        let middle = pos2((top.x + bottom.x) / 2.0, (top.y + bottom.y) / 2.0);
        if self.pinch.is_none() {
            match &mut self.twin {
                None => {
                    // both on one paint layer's row
                    if let (Some(a), Some(b)) = (row_at(top), row_at(bottom))
                        && a == b
                        && session.layers().iter().any(|l| l.id == a && l.kind == LayerType::Paint)
                    {
                        self.twin =
                            Some(Twin { id: a, since: Instant::now(), from: middle, moved: false, done: false });
                        // not a one-finger press on it any more
                        self.press = None;
                    }
                }
                Some(t) => {
                    let d = middle - t.from;
                    if d.length() > TWIN_SLOP {
                        t.moved = true;
                    }
                    if !t.done && d.x > TWIN_SWIPE && d.x > 2.0 * d.y.abs() {
                        // swiped right: its Alpha Lock on or off
                        t.done = true;
                        if let Some(l) = session.layers().into_iter().find(|l| l.id == t.id) {
                            session.set_layer_alpha_lock(l.id, !l.alpha_lock);
                        }
                    } else if !t.done && !t.moved && t.since.elapsed().as_secs_f32() >= TWIN_HOLD_S {
                        // held: its contents selected
                        t.done = true;
                        session.select_layer_contents(t.id);
                    }
                    if !t.done {
                        ctx.request_repaint_after(std::time::Duration::from_millis(50));
                    }
                }
            }
            if self.twin.is_some() {
                return;
            }
        }
        match &mut self.pinch {
            None => {
                if let (Some(upper), Some(lower)) = (row_at(top), row_at(bottom))
                    && upper != lower
                {
                    self.pinch = Some(Pinch {
                        upper,
                        lower,
                        fingers: (top_id, bottom_id),
                        starts: (top.y, bottom.y),
                        by: (0.0, 0.0),
                        pinched: false,
                    });
                }
            }
            Some(p) => {
                let at = |id: u64, start: f32| self.touches.get(&id).map_or(start, |q| q.y);
                let (upper, lower) = (at(p.fingers.0, p.starts.0), at(p.fingers.1, p.starts.1));
                p.by = (upper - p.starts.0, lower - p.starts.1);
                if (lower - upper).abs() <= (p.starts.1 - p.starts.0) * PINCH_MERGE {
                    p.pinched = true;
                }
                ctx.request_repaint();
            }
        }
    }

    /// The rows not being slid go back, or open to their buttons, as they were left.
    fn settle_slides(&mut self, ctx: &egui::Context, shown: &[&LayerInfo]) {
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        for l in shown {
            if self.press.is_some_and(|p| p.id == l.id && p.way == Way::Swipe) {
                continue;
            }
            let to = if self.swiped == Some(l.id) { -swipe_reach(l) } else { 0.0 };
            let Some(v) = self.slid.get_mut(&l.id) else {
                if to != 0.0 {
                    self.slid.insert(l.id, 0.0);
                    ctx.request_repaint();
                }
                continue;
            };
            *v += (to - *v) * (dt * 14.0).min(1.0);
            if (to - *v).abs() < 0.5 {
                *v = to;
            } else {
                ctx.request_repaint();
            }
            if *v == 0.0 {
                self.slid.remove(&l.id);
            }
        }
    }

    /// How far row `id` stands aside for the rows being dragged: between where they were and where
    /// they land it takes the room they left, so the gap shows where they will land.
    fn making_way(&self, id: u64) -> f32 {
        let Some(row) = self.last_rows.get(&id) else { return 0.0 };
        if self.press.is_none() || self.dragged_rows.contains(&id) {
            return 0.0;
        }
        let moved: Vec<&Rect> = self.dragged_rows.iter().filter_map(|r| self.last_rows.get(r)).collect();
        if moved.is_empty() {
            return 0.0;
        }
        let top = moved.iter().fold(f32::INFINITY, |t, r| t.min(r.top()));
        let bottom = moved.iter().fold(f32::NEG_INFINITY, |b, r| b.max(r.bottom()));
        let room = bottom - top + ROW_GAP;
        // the line between the rows they land between; held onto a row, or over their own place,
        // none moves
        let slot = match self.drop_at {
            Some(RowDrop::Above { target }) => self.last_rows.get(&target).map(|r| r.top()),
            Some(RowDrop::Below { target }) => self.last_rows.get(&target).map(|r| r.bottom()),
            Some(RowDrop::Bottom) => Some(f32::INFINITY),
            _ => None,
        };
        let Some(slot) = slot else { return 0.0 };
        if row.bottom() <= top && row.top() >= slot {
            room
        } else if row.top() >= bottom && row.bottom() <= slot {
            -room
        } else {
            0.0
        }
    }

    /// Where layers let go at window point `at` land by the panel's rows (`layer`: the one of
    /// them held, back among its own rows): none off the panel, or with it closed.
    pub fn drop_at(&self, session: &CanvasSession, layer: Option<u64>, at: Pos2) -> Option<RowDrop> {
        if !self.box_on_window.is_some_and(|b| b.contains(at)) {
            return None;
        }
        Some(session.row_drop(layer, shown_rows(&self.rows_on_window), at.y, None))
    }

    /// The row under the dragged row's middle at `y`, held over since when, and the row held
    /// over long enough to go onto it, if any.
    fn held_over(&mut self, rows: &HashMap<u64, Rect>, y: f32) -> Option<u64> {
        let under = rows
            .iter()
            .find(|(id, r)| !self.dragged_rows.contains(id) && y >= r.top() && y < r.bottom())
            .map(|(id, _)| *id);
        if self.over.map(|(id, _)| id) != under {
            self.over = under.map(|id| (id, Instant::now()));
        }
        self.over
            .filter(|(_, since)| since.elapsed().as_secs_f32() >= procraft_core::ROW_HOLD_SECONDS)
            .map(|(id, _)| id)
    }

    /// Layer `id`'s thumbnail as the panel shows it, if it has been read, and its row on the window.
    pub fn row_look(&self, id: u64) -> (Option<TextureHandle>, Option<Rect>) {
        (self.thumbnails.get(&id).map(|(_, t)| t.clone()), self.rows_on_window.get(&id).copied())
    }

    /// The panel closed: no rows to drop on.
    pub fn closed(&mut self, session: &CanvasSession) {
        if let Some(blend) = self.blend.as_mut().filter(|b| b.moving()) {
            blend.motion = BlendMotion::Resting;
            session.finish_layer_blend();
        }
        self.box_on_window = None;
        self.rows_on_window.clear();
    }

    /// Follows the press on a row: moved sideways it slides the row (let go past half its buttons,
    /// it stays open; slid right past half the way, it picks the layer); held still (or, with a
    /// mouse, moved up or down: a finger or pen doing that scrolls the list, which follows it), the
    /// row is dragged up and down and, let go, the layer moves next to the row it is over.
    fn follow_press(
        &mut self,
        ctx: &egui::Context,
        session: &CanvasSession,
        layers: &[LayerInfo],
        rows: &HashMap<u64, Rect>,
        panel_box: Rect,
    ) {
        let Some(mut p) = self.press else { return };
        let (at, down) = ctx.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
        let d = at.map_or(p.origin, |at| panel::in_panel(ctx, at)) - p.origin;
        let l = layers.iter().find(|l| l.id == p.id);
        let reach = l.map_or(0.0, swipe_reach);
        let open = self.swiped == Some(p.id);
        if down {
            match p.way {
                Way::Undecided => {
                    let sideways = d.x.abs() > d.y.abs();
                    if d.length() > SLOP && (sideways || !p.mouse) {
                        p.way = if sideways { Way::Swipe } else { Way::Ignored };
                        // up or down: the list follows the finger from where it was put down
                        if !sideways {
                            self.scroll_by -= d.y;
                            ctx.request_repaint();
                        }
                    } else if p.since.elapsed() >= HOLD || d.length() > SLOP {
                        // a mask stays with its layer
                        if l.is_some_and(|l| l.kind != LayerType::Mask) {
                            p.way = Way::Reorder;
                            self.swiped = None;
                            self.dragged_rows = session.moving_with(p.id);
                            if !self.dragged_rows.contains(&p.id) {
                                self.dragged_rows.push(p.id);
                            }
                        } else {
                            p.way = Way::Ignored;
                        }
                    } else {
                        ctx.request_repaint_after(HOLD.saturating_sub(p.since.elapsed()));
                    }
                }
                Way::Swipe => {
                    // closed, it goes right as far as picking takes
                    let most = if open { 0.0 } else { PICK_REACH };
                    self.slid.insert(p.id, (p.start + d.x).clamp(-reach, most));
                    ctx.request_repaint();
                }
                Way::Reorder => {
                    // out past the panel's side, the row (with what goes with it) is lifted out,
                    // held by the pointer from then on wherever it goes
                    let x = at.map_or(p.origin.x, |at| panel::in_panel(ctx, at).x);
                    if x < panel_box.left() - LIFT_REACH || x > panel_box.right() + LIFT_REACH {
                        self.lifted = Some(p.id);
                        self.press = None;
                        self.dragged_rows.clear();
                        (self.drop_at, self.over) = (None, None);
                        return;
                    }
                    // the row scrolled with the list stays under the pointer
                    let scrolled =
                        self.last_rows.get(&p.id).zip(rows.get(&p.id)).map_or(0.0, |(was, row)| was.top() - row.top());
                    p.origin.y -= scrolled;
                    p.dy = d.y + scrolled;
                    // held near an end of the list it scrolls, faster the nearer
                    if let (Some(list), Some(row)) = (self.list, rows.get(&p.id)) {
                        let y = row.center().y + p.dy;
                        let towards = if y < list.top() + DRAG_EDGE {
                            (y - list.top() - DRAG_EDGE) / DRAG_EDGE
                        } else if y > list.bottom() - DRAG_EDGE {
                            (y - list.bottom() + DRAG_EDGE) / DRAG_EDGE
                        } else {
                            0.0
                        };
                        self.scroll_by = towards.clamp(-1.0, 1.0) * DRAG_SCROLL * ctx.input(|i| i.stable_dt);
                    }
                    // where it lands if let go now: the gap opens there, or a row held over is outlined
                    if let Some(row) = rows.get(&p.id) {
                        let y = row.center().y + p.dy;
                        let held = self.held_over(rows, y);
                        self.drop_at = Some(session.row_drop(Some(p.id), shown_rows(rows), y, held));
                    }
                    ctx.request_repaint();
                }
                // a finger's: the row has the drag (not the list's own drag to scroll), the list
                // follows it
                Way::Ignored if !p.mouse => {
                    self.scroll_by -= panel::in_panel_by(ctx, ctx.input(|i| i.pointer.delta())).y;
                    ctx.request_repaint();
                }
                Way::Ignored => {}
            }
            self.press = Some(p);
            return;
        }
        self.press = None;
        self.dragged_rows.clear();
        self.drop_at = None;
        match p.way {
            Way::Swipe => {
                let slide = self.slid.get(&p.id).copied().unwrap_or(0.0);
                if slide > PICK_REACH / 2.0 {
                    session.pick_layer(p.id);
                } else {
                    let opened = slide < -reach / 2.0;
                    if opened != open {
                        self.swiped = opened.then_some(p.id);
                    }
                }
            }
            Way::Reorder => {
                // dropped where its middle came to rest, placed as the core places it
                let Some(row) = rows.get(&p.id) else { return };
                let y = row.center().y + p.dy;
                let held = self.held_over(rows, y);
                self.over = None;
                session.drop_row(p.id, session.row_drop(Some(p.id), shown_rows(rows), y, held));
            }
            Way::Undecided | Way::Ignored => {}
        }
    }

    /// A row's visibility box at `rect`: tapped, the layer shows or hides (or, with a layer shown
    /// alone, every layer shows again); held, it shows alone (or, alone already, every layer shows
    /// again), a notice saying which.
    fn visibility_touch(&mut self, ui: &Ui, session: &CanvasSession, l: &LayerInfo, rect: Rect) {
        let response = ui.interact(rect, ui.scope_id().with(("visible", l.id)), Sense::click_and_drag());
        if response.is_pointer_button_down_on() {
            let (id, since, done) = self.box_press.get_or_insert((l.id, Instant::now(), false));
            if *id == l.id && !*done {
                let held = since.elapsed();
                if held >= HOLD {
                    *done = true;
                    if let Some(said) = session.hold_visibility(l.id) {
                        self.notice = Some((said, false, NoticeKind::Done));
                    }
                } else {
                    ui.ctx().request_repaint_after(HOLD - held);
                }
            }
        }
        let held = self.box_press.is_some_and(|(id, _, done)| id == l.id && done);
        // as measured, the notice band says so
        if response.clicked()
            && !held
            && let Some(said) = session.tap_visibility(l.id)
        {
            self.notice = Some((said, false, NoticeKind::Done));
        }
        if !response.is_pointer_button_down_on() && self.box_press.is_some_and(|(id, _, _)| id == l.id) {
            self.box_press = None;
        }
    }

    /// The current layer's options, left of the panel and centred on its row (kept below the top
    /// bar): what doesn't apply to it is left out; lock, duplicate and delete are on the row's
    /// swipe.
    #[allow(clippy::too_many_arguments)]
    fn layer_options(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        l: &LayerInfo,
        panel: Rect,
        row_y: f32,
        backdrops: &mut Vec<f32>,
    ) {
        let rows: Vec<LayerOptionRow> = session.layer_options(l.id);
        // a plugin's actions for the layer, after the app's own
        let actions = std::mem::take(&mut self.plugin_actions);
        let count = rows.len() + actions.len();
        // as measured, an empty one (a locked group's or mask's) is a pill 64 tall
        let height = (7.0 * 2.0 + menu::OPTION_HEIGHT * count as f32).max(64.0);
        let screen = panel::screen(ctx);
        let lowest = (screen.bottom() - height).max(screen.top() + OPTIONS_TOP);
        let top = (row_y - height / 2.0).clamp(screen.top() + OPTIONS_TOP, lowest);
        let rect = Rect::from_min_size(pos2(panel.left() + OPTIONS_LEFT, top), vec2(OPTIONS_WIDTH, height));
        let mut chosen = None;
        let mut action = None;
        self.options_menu.show(
            ctx,
            "layer options",
            rect,
            pos2(rect.right(), rect.center().y),
            // empty, as measured, the frost alone, untinted
            menu::Glass {
                corner: OPTIONS_CORNER,
                fill: if count == 0 { Themed(0, 0) } else { menu::FROSTED.fill },
                ..menu::FROSTED
            },
            backdrops,
            |ui| {
                let places: Vec<_> = menu::option_rows(rect.left(), rect.top() + 7.0, rect.width(), count).collect();
                for (&row, r) in places.iter().zip(&rows) {
                    if menu::option(ui, look, row, &r.label, r.switch_on.unwrap_or(false), r.enabled, false) {
                        chosen = Some(r.option);
                    }
                }
                for (&row, a) in places.iter().skip(rows.len()).zip(&actions) {
                    if menu::option(ui, look, row, &a.label, false, true, false) {
                        action = Some(a.key.clone());
                    }
                }
            },
        );
        if let Some(key) = action {
            self.options_menu.close();
            self.run_action = Some(key);
            return;
        }
        // every row closes the menu, a switch's too (it shows ticked next time)
        let Some(does) = chosen else { return };
        self.options_menu.close();
        match does {
            LayerOption::Rename => self.renaming = Some((l.id, l.name.clone(), false)),
            LayerOption::Ungroup => _ = session.ungroup(l.id),
            LayerOption::Flatten => _ = session.flatten_group(l.id),
            LayerOption::Select => _ = session.select_layer_contents(l.id),
            LayerOption::Copy => self.copied = session.copy_layer_contents(l.id).or(self.copied.take()),
            LayerOption::Fill => _ = session.fill_layer(l.id),
            LayerOption::Clear => {
                session.select_layer(l.id);
                session.clear_layer();
            }
            LayerOption::Invert => _ = session.invert_layer(l.id),
            LayerOption::AlphaLock => _ = session.set_layer_alpha_lock(l.id, !l.alpha_lock),
            LayerOption::Mask => _ = session.add_mask(l.id),
            LayerOption::ClippingMask => _ = session.set_layer_clipped(l.id, !l.clipped),
            LayerOption::DrawingAssist => _ = session.set_guide_assisted(!l.assisted),
            LayerOption::Reference => _ = session.set_layer_reference(l.id, !l.reference),
            LayerOption::ControlLayer => {
                self.control_for = Some(l.id);
                self.control_rect = Some(rect);
                self.control_pending = true;
            }
            LayerOption::EditText => self.edit_text = Some(l.id),
            LayerOption::FilterSettings => {
                session.select_layer(l.id);
                self.filter_settings = true;
            }
            LayerOption::Rasterize => {
                session.rasterize_layer(l.id);
                self.notice =
                    Some((if l.text { "Text rasterized" } else { "Layer rasterized" }.into(), true, NoticeKind::Done));
            }
            // a plugin's layer: its strokes recoloured, or given the paint brush, as strokes
            LayerOption::Recolour | LayerOption::ApplyBrush => {
                let colour = does == LayerOption::Recolour;
                session.select_layer(l.id);
                session.restyle_plugin_items(colour, !colour, !colour, !colour);
            }
            LayerOption::MergeDown => _ = session.merge_down(l.id),
            LayerOption::CombineDown => _ = session.combine_down(l.id),
            LayerOption::MergeMask => _ = session.merge_mask(l.id),
        }
    }

    /// The Control layer choices, beside the options: None, Line art, Scribble. Chosen, layer `id`
    /// guides generation as that (or no longer does).
    fn control_options(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        backdrops: &mut Vec<f32>,
    ) {
        if self.control_pending {
            self.control_pending = false;
            if self.control_for.is_some() {
                self.control_menu.open();
            }
        }
        if !self.control_menu.is_open() {
            self.control_for = None;
            return;
        }
        let Some(id) = self.control_for else { return };
        let Some(from) = self.control_rect else { return };
        let Some(l) = session.layers().into_iter().find(|l| l.id == id) else {
            self.control_for = None;
            return;
        };
        let choices = [
            (None, "None"),
            (Some(LayerControl::LineArt), "Line art"),
            (Some(LayerControl::Scribble), "Scribble"),
            (Some(LayerControl::Depth), "Depth"),
        ];
        let size = vec2(238.0, 7.0 * 2.0 + menu::OPTION_HEIGHT * choices.len() as f32);
        let screen = ctx.content_rect();
        let left = (from.left() - size.x - 6.0)
            .clamp(screen.left() + 6.0, (screen.right() - size.x - 6.0).max(screen.left() + 6.0));
        let top =
            from.top().clamp(screen.top() + OPTIONS_TOP, (screen.bottom() - size.y).max(screen.top() + OPTIONS_TOP));
        let rect = Rect::from_min_size(pos2(left, top), size);
        let mut chosen = None;
        self.control_menu.show(
            ctx,
            "control layer options",
            rect,
            pos2(rect.right(), rect.center().y),
            menu::FROSTED,
            backdrops,
            |ui| {
                let places = menu::option_rows(rect.left(), rect.top() + 7.0, rect.width(), choices.len());
                for (row, (kind, label)) in places.zip(choices) {
                    if menu::option(ui, look, row, label, l.control == kind, true, false) {
                        chosen = Some(kind);
                    }
                }
            },
        );
        let Some(kind) = chosen else { return };
        self.control_menu.close();
        self.control_for = None;
        session.set_layer_control(id, kind);
    }

    /// The blend editor under a layer's row: its opacity, read and slid, then its blend modes, the
    /// layer's in the middle; scrolled, the mode settling in the middle becomes the layer's, and a
    /// tap on one scrolls it there.
    fn blend_editor(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        l: &LayerInfo,
        rect: Rect,
        modes: &[BlendModeInfo],
    ) {
        let ink = argb(PANEL_TITLE);
        let left = rect.left() + 13.0;
        let right = rect.right() - 13.0;
        let y = rect.top() + 22.0 + 23.0 / 2.0;
        text(ui.painter(), pos2(left, y), Align2::LEFT_CENTER, "Opacity", 17.0, Weight::Regular, ink);
        let reading =
            if l.opacity >= 0.995 { "Max".to_owned() } else { format!("{}%", (l.opacity * 100.0).round() as i32) };
        text(ui.painter(), pos2(right, y), Align2::RIGHT_CENTER, &reading, 17.0, Weight::Regular, ink);
        // a thin blue slider with a white knob, set by dragging anywhere along it
        let slider = Rect::from_min_max(
            pos2(left, rect.top() + 22.0 + 23.0 + 2.0),
            pos2(right, rect.top() + 22.0 + 23.0 + 18.0),
        );
        let response = ui.interact(slider, ui.scope_id().with(("opacity", l.id)), Sense::click_and_drag());
        let held = response.is_pointer_button_down_on();
        if let Some(p) = response.interact_pointer_pos().filter(|_| held || response.clicked()) {
            if let Some(blend) = self.blend.as_mut().filter(|b| b.id == l.id && b.moving()) {
                blend.motion = BlendMotion::Resting;
                session.finish_layer_blend();
            }
            let v = ((p.x - slider.left()) / slider.width()).clamp(0.0, 1.0);
            session.set_layer_opacity(l.id, v, !held);
        } else if response.drag_stopped() {
            session.set_layer_opacity(l.id, l.opacity, true);
        }
        let (cy, h) = (slider.center().y, 3.0);
        let track = Rect::from_min_max(pos2(slider.left(), cy - h / 2.0), pos2(slider.right(), cy + h / 2.0));
        ui.painter().rect_filled(track, h / 2.0, argb(SLIDER_REST));
        ui.painter().rect_filled(
            Rect::from_min_size(track.min, vec2(track.width() * l.opacity, h)),
            h / 2.0,
            argb(pc::ACCENT),
        );
        let knob = vec2(26.0, 15.0);
        let x = (slider.left() + slider.width() * l.opacity)
            .clamp(slider.left() + knob.x / 2.0, slider.right() - knob.x / 2.0);
        ui.painter().rect_filled(Rect::from_center_size(pos2(x, cy), knob), knob.y / 2.0, Color32::WHITE);

        // the modes: a picker scrolled to put one in the middle
        let list =
            Rect::from_min_size(pos2(rect.left(), slider.bottom() + 2.0 + 11.0), vec2(rect.width(), BLEND_LIST_HEIGHT));
        ui.painter().rect_filled(list, 12, argb(LAYER_ROW));
        let Some(b) = self.blend.as_mut().filter(|b| b.id == l.id) else {
            // closing: as it was
            let at = modes.iter().position(|m| m.id == l.blend).unwrap_or(0) as f32 * BLEND_ROW;
            blend_rows(ui, look, list, modes, at);
            return;
        };
        let rows = modes.len() as u32;
        let was_moving = b.moving();
        let pad = (BLEND_LIST_HEIGHT - BLEND_ROW) / 2.0;
        let response = ui.interact(list, ui.scope_id().with(("modes", l.id)), Sense::click_and_drag());
        let now = ui.input(|i| i.time);
        let wheel = if response.hovered() {
            // The picker owns the wheel here, inside the scrollable Layers panel.
            ui.input_mut(|i| std::mem::take(&mut i.smooth_scroll_delta.y))
        } else {
            0.0
        };

        if matches!(b.motion, BlendMotion::Tapped { .. }) {
            // a tapped mode on its way to the band: touches and the wheel do nothing meanwhile
        } else if response.is_pointer_button_down_on() {
            // it follows the finger from where it was pressed
            let origin = ui.input(|i| i.pointer.press_origin());
            let (start, caught, mut y) = match b.motion {
                BlendMotion::Held { start, caught, y } => (start, caught, y),
                _ => (b.pull(rows), b.moving(), origin.map_or(list.center().y, |o| o.y)),
            };
            if let Some((o, p)) = origin.zip(response.interact_pointer_pos()) {
                b.pull_to(start + o.y - p.y, rows);
                y = p.y;
            }
            b.motion = BlendMotion::Held { start, caught, y };
        } else if wheel != 0.0 {
            b.pull_to(b.pull(rows) - wheel, rows);
            b.motion = BlendMotion::Wheel { last: now };
        } else if let BlendMotion::Held { caught, y, .. } = b.motion {
            // let go: a tap on a row away from the band makes it the mode at once and scrolls it
            // there; else the list moves on as it was going (a tap on a moving list only stops it)
            let row = ((y - list.top() - pad + b.scroll) / BLEND_ROW).floor();
            let tapped = (response.clicked() && !caught && row >= 0.0)
                .then(|| Some((row as usize, blend_tap(row as u32, b.scroll, BLEND_ROW, rows)?)))
                .flatten();
            if let Some((i, to)) = tapped {
                if let Some(m) = modes.get(i) {
                    session.set_layer_blend(l.id, m.id);
                }
                b.motion = BlendMotion::Tapped { from: b.scroll, to, since: now };
            } else {
                let scale = ui.ctx().layer_transform_from_global(ui.layer_id()).map_or(1.0, |t| t.scaling);
                let speed = if response.clicked() { 0.0 } else { ui.input(|i| -i.pointer.velocity().y) * scale };
                b.let_go(speed, now, rows);
            }
        } else if matches!(b.motion, BlendMotion::Resting) {
            // Undo or redo can change the applied mode while the picker is open.
            b.scroll = modes.iter().position(|m| m.id == l.blend).unwrap_or(0) as f32 * BLEND_ROW;
        }

        b.advance(now, rows);
        // while it moves (and as it stops), the mode in the band shows on the layer
        let tapped = matches!(b.motion, BlendMotion::Tapped { .. });
        if (was_moving || b.moving()) && !tapped {
            let centred = blend_centred(b.scroll, BLEND_ROW, rows) as usize;
            if let Some(m) = modes.get(centred).filter(|m| m.id != l.blend) {
                session.preview_layer_blend(l.id, m.id);
                ui.ctx().request_repaint();
            }
        }
        if was_moving && !b.moving() {
            session.finish_layer_blend();
        }
        if !matches!(b.motion, BlendMotion::Resting | BlendMotion::Held { .. }) {
            ui.ctx().request_repaint();
        }
        blend_rows(ui, look, list, modes, b.scroll);
    }

    /// A layer's row: its thumbnail, its name (a padlock before it when locked, what it is under it),
    /// its blend letter and its visibility box.
    #[allow(clippy::too_many_arguments)]
    fn layer_row(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        l: &LayerInfo,
        row: Rect,
        ink: Color32,
        box_fill: Color32,
        looks: RowLook,
        modes: &[BlendModeInfo],
        read: &mut usize,
    ) -> bool {
        let (version, masked) = (looks.version, looks.masked);
        let mask = l.kind == LayerType::Mask;
        let mut blend_tapped = false;
        let mut x = row.left() + looks.indent + GROUP_INDENT * l.depth as f32;
        if l.clipped {
            look.icon(ui, &icons::CLIP, pos2(x + 8.0, row.center().y), 16.0, ink);
            x += 18.0;
        }
        // a mask's thumbnail and its layer's join into one
        let shift = if mask {
            2.0
        } else if masked {
            -2.0
        } else {
            0.0
        };
        let thumb =
            Rect::from_center_size(pos2(x + 4.0 + THUMB.0 / 2.0, row.center().y + shift), vec2(THUMB.0, THUMB.1));
        self.thumbnail(ui, look, session, l, thumb, version, read);
        if looks.held {
            ui.painter().rect_stroke(thumb, 4, egui::Stroke::new(2.0, HELD_OUTLINE), egui::StrokeKind::Inside);
        }
        let name_left = thumb.right();
        let blend =
            Rect::from_min_size(pos2(row.right() - 8.0 - 44.0 - 44.0 + 8.0, row.center().y - 23.5), vec2(44.0, 47.0));
        let visibility = Rect::from_min_size(pos2(row.right() - 8.0 - 44.0, row.center().y - 23.5), vec2(44.0, 47.0));
        // a model's layer: the button that shows its channels, blue while one of them is painted;
        // laid out narrower than it is touched, so the name keeps its room up to the icon
        let cube = looks
            .cube
            .map(|lit| (Rect::from_center_size(pos2(blend.left() - 8.0 - 6.0, row.center().y), vec2(44.0, 47.0)), lit));
        let name_right = match cube {
            Some((c, _)) => c.center().x - 6.0 - 10.0,
            None if mask => visibility.left() - 36.0,
            None => blend.left() - 8.0 - if l.control.is_some() { 20.0 } else { 0.0 },
        };
        // what the layer is, under its name (which stays in the middle): a reference layer, one
        // drawing assist is on for, a private one, a model's layer its texture set's meshes share
        let tag = if l.reference && l.assisted {
            Some("Assisted Reference")
        } else if l.reference && looks.shared {
            Some("Shared Reference")
        } else if l.reference {
            Some("Reference")
        } else if looks.shared {
            Some("Shared")
        } else if l.assisted {
            Some("Assisted")
        } else if l.private {
            Some("Private")
        } else {
            None
        };
        self.name(ui, look, session, l, pos2(name_left, row.center().y), name_right, ink);
        if let Some(tag) = tag {
            let g = galley(ui, tag, 13.0, Weight::Regular, ink, name_right - name_left - 29.0);
            ui.painter().galley(pos2(name_left + 29.0, row.center().y + 17.5 - g.size().y / 2.0), g, ink);
        }
        if let Some((c, lit)) = cube {
            look.icon(ui, &icons::TAB3_D, c.center(), 48.0, if lit { argb(pc::ACCENT) } else { ink });
            if ui.interact(c, ui.scope_id().with(("channels", l.id)), Sense::click()).clicked() {
                self.toggle_tray(l.id);
            }
        }
        if !mask {
            if let Some(kind) = l.control {
                control_badge(ui.painter(), pos2(blend.left() - 10.0, row.center().y), kind, ink);
            }
            // a locked layer's mode is greyed
            let letter = if l.locked && !l.current && !l.picked { argb(LOCKED_LETTER) } else { ink };
            let code = blend_code(modes, l.blend);
            crate::look::text(
                ui.painter(),
                blend.center(),
                Align2::CENTER_CENTER,
                &code,
                14.0,
                Weight::SemiBold,
                letter,
            );
            // it takes drags too, so a press on it is never the row's
            blend_tapped = ui.interact(blend, ui.scope_id().with(("blend", l.id)), Sense::click_and_drag()).clicked();
        }
        check_box(ui, look, visibility.center(), l.visible && !l.soloed_out, box_fill);
        self.visibility_touch(ui, session, l, visibility);
        blend_tapped
    }

    /// A row's name (see [`locked_name`]), or, being renamed, a field for it with the name chosen;
    /// let go of, the name is kept.
    #[allow(clippy::too_many_arguments)]
    fn name(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        l: &LayerInfo,
        at: Pos2,
        right: f32,
        ink: Color32,
    ) {
        let Some((id, typed, focused)) = self.renaming.as_mut().filter(|(id, _, _)| *id == l.id) else {
            locked_name(ui, look, l, at, right, ink);
            return;
        };
        if l.locked {
            look.icon(ui, &icons::LOCK_ROW, pos2(at.x + 3.0 + 8.5, at.y), 17.0, ink);
        }
        let field = Rect::from_min_max(pos2(at.x + 29.0, at.y - 12.0), pos2(right - 6.0, at.y + 12.0));
        let mut output = egui::TextEdit::singleline(typed)
            .font(Weight::Regular.font(17.0))
            .text_color(ink)
            .frame(egui::Frame::NONE)
            .margin(vec2(0.0, 0.0))
            .desired_width(field.width())
            .show(&mut ui.new_child(UiBuilder::new().max_rect(field)));
        if !*focused {
            // the whole name chosen, to type over
            output.response.request_focus();
            let all = CCursorRange::two(CCursor::new(0), CCursor::new(typed.chars().count()));
            output.state.cursor.set_char_range(Some(all));
            output.state.store(ui.ctx(), output.response.id);
            *focused = true;
        } else if output.response.lost_focus() {
            let (id, name) = (*id, typed.clone());
            self.renaming = None;
            session.rename_layer(id, name);
        }
    }

    /// A group's row: where a thumbnail would be, a line while it is open or, closed (the row
    /// taller), a window on its layers' rows drawn small; its name, its open or closed arrow and
    /// its box. Held over by a dragged row long enough to take it, the line or window is outlined.
    #[allow(clippy::too_many_arguments)]
    fn group_row(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        l: &LayerInfo,
        row: Rect,
        ink: Color32,
        box_fill: Color32,
        held: bool,
    ) {
        let x = row.left() + GROUP_INDENT * l.depth as f32;
        // 0 open, 1 closed: the line's 20 above and below become 4, the line the window between
        let closed = ((row.height() - GROUP_HEIGHT) / (GROUP_CLOSED_HEIGHT - GROUP_HEIGHT)).clamp(0.0, 1.0);
        let inset = GROUP_LINE_INSET + (GROUP_WINDOW_INSET - GROUP_LINE_INSET) * closed;
        let corner = 2.0 + 2.0 * closed;
        let line = Rect::from_min_max(pos2(x + 4.0, row.top() + inset), pos2(x + 4.0 + THUMB.0, row.bottom() - inset));
        ui.painter().rect_filled(line, corner, argb(GROUP_LINE));
        if closed > 0.0 {
            self.group_contents(ui, look, session, l.id, line);
        }
        if held {
            ui.painter().rect_stroke(line, corner, egui::Stroke::new(2.0, HELD_OUTLINE), egui::StrokeKind::Inside);
        }
        let visibility = Rect::from_min_size(
            pos2(row.right() - 8.0 - 44.0, row.center().y - GROUP_HEIGHT / 2.0),
            vec2(44.0, GROUP_HEIGHT),
        );
        let arrow =
            Rect::from_min_size(pos2(visibility.left() - 44.0 + 8.0, visibility.top()), vec2(44.0, GROUP_HEIGHT));
        self.name(ui, look, session, l, pos2(line.right(), row.center().y), arrow.left() - 8.0, ink);
        let icon = if l.collapsed { &icons::CHEVRON_RIGHT } else { &icons::CHEVRON_DOWN };
        look.icon(ui, icon, arrow.center(), 16.0, ink);
        if ui.interact(arrow, ui.scope_id().with(("fold", l.id)), Sense::click()).clicked() {
            session.set_group_collapsed(l.id, !l.collapsed);
        }
        check_box(ui, look, visibility.center(), l.visible && !l.soloed_out, box_fill);
        self.visibility_touch(ui, session, l, visibility);
    }

    /// A closed group's layers' rows in its `window`, drawn small to fit its width less 1 on each
    /// side, from its top: their fills, thumbnails, names, blend letters and boxes, as their rows
    /// look in the list.
    fn group_contents(&mut self, ui: &mut Ui, look: &mut Look, session: &CanvasSession, group: u64, window: Rect) {
        let Some(rows) = self.closed_contents.get(&group).cloned() else { return };
        let mut mini = ui.new_child(UiBuilder::new().max_rect(window));
        mini.set_clip_rect(window.intersect(ui.clip_rect()));
        let width = self.list.map_or(LAYERS_WIDTH - 24.0, |l| l.width());
        let scale = (window.width() - 2.0) / width;
        let versions: HashMap<u64, u64> = session.layer_versions().into_iter().map(|v| (v.id, v.version)).collect();
        let modes = procraft_core::blend_modes();
        let masked = |l: &LayerInfo| rows.iter().any(|(m, _)| m.kind == LayerType::Mask && m.parent == Some(l.id));
        let mut y = window.top() + 1.0;
        let mut read = 0;
        for (l, depth) in &rows {
            let group = l.kind == LayerType::Group;
            let mask = l.kind == LayerType::Mask;
            let full = if group {
                GROUP_HEIGHT
            } else if mask || masked(l) {
                MASKED_ROW_HEIGHT
            } else {
                ROW_HEIGHT
            };
            let row = Rect::from_min_size(pos2(window.left() + 1.0, y), vec2(width, full) * scale);
            y += (full + ROW_GAP) * scale;
            if row.top() > window.bottom() {
                break;
            }
            let fill = if l.current {
                argb(pc::ACCENT)
            } else if l.picked {
                argb(PICKED_ROW)
            } else {
                argb(LAYER_ROW)
            };
            mini.painter().rect_filled(row, 0, fill);
            if let Some(label) = l.label {
                let edge = Rect::from_min_size(row.min, vec2(LABEL_WIDTH * scale, row.height()));
                mini.painter().rect_filled(edge, 0, labelled(label));
            }
            let ink = if l.current || l.picked { Color32::WHITE } else { argb(PANEL_TITLE) };
            let left = row.left() + (GROUP_INDENT * *depth as f32 + 4.0) * scale;
            let name_left = left + THUMB.0 * scale;
            if group {
                let line = Rect::from_center_size(
                    pos2(left + THUMB.0 * scale / 2.0, row.center().y),
                    vec2(THUMB.0, 4.0) * scale,
                );
                mini.painter().rect_filled(line, 2.0 * scale, argb(GROUP_LINE));
            } else {
                let shift = if mask {
                    2.0
                } else if masked(l) {
                    -2.0
                } else {
                    0.0
                };
                let thumb = Rect::from_center_size(
                    pos2(left + THUMB.0 * scale / 2.0, row.center().y + shift * scale),
                    vec2(THUMB.0, THUMB.1) * scale,
                );
                self.thumbnail(&mini, look, session, l, thumb, versions.get(&l.id).copied(), &mut read);
            }
            let name = galley(
                &mini,
                &l.name,
                17.0 * scale,
                Weight::Regular,
                ink,
                (row.right() - 96.0 * scale - name_left).max(1.0),
            );
            mini.painter().galley(pos2(name_left + 29.0 * scale, row.center().y - name.size().y / 2.0), name, ink);
            let right = row.right() - 8.0 * scale;
            if !group && !mask {
                let code = blend_code(&modes, l.blend);
                let letter = if l.locked && !l.current && !l.picked { argb(LOCKED_LETTER) } else { ink };
                let at = pos2(right - 44.0 * scale - 22.0 * scale + 8.0 * scale, row.center().y);
                text(mini.painter(), at, Align2::CENTER_CENTER, &code, 14.0 * scale, Weight::SemiBold, letter);
            }
            let box_at = pos2(right - 22.0 * scale, row.center().y);
            let box_fill = if l.current || l.picked { Color32::WHITE } else { argb(CHECK_FILL) };
            let check = Rect::from_center_size(box_at, vec2(18.0, 18.0) * scale);
            if l.visible && !l.soloed_out {
                mini.painter().rect_filled(check, 5.0 * scale, box_fill);
            } else {
                mini.painter().rect_stroke(
                    check,
                    5.0 * scale,
                    egui::Stroke::new(1.5 * scale, box_fill),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }

    /// A layer's thumbnail, read again when its pixels' version is new: what is drawn, fitted on
    /// its grey (a mask's on white, an alpha-locked layer's on a checkerboard); a text layer's is
    /// its glyph.
    #[allow(clippy::too_many_arguments)]
    fn thumbnail(
        &mut self,
        ui: &Ui,
        look: &mut Look,
        session: &CanvasSession,
        l: &LayerInfo,
        rect: Rect,
        version: Option<u64>,
        read: &mut usize,
    ) {
        let painter = ui.painter_at(rect);
        let corner = 4u8;
        if l.alpha_lock {
            painter.rect_filled(rect, corner, Color32::WHITE);
            let cell = 6.0;
            let (cols, rows) = ((rect.width() / cell).ceil() as i32, (rect.height() / cell).ceil() as i32);
            for y in 0..rows {
                for x in 0..cols {
                    if (x + y) % 2 == 1 {
                        let r =
                            Rect::from_min_size(rect.min + vec2(x as f32 * cell, y as f32 * cell), vec2(cell, cell));
                        painter.rect_filled(r.intersect(rect), 0, Color32::from_gray(0xD6));
                    }
                }
            }
        } else if l.kind == LayerType::Mask {
            painter.rect_filled(rect, corner, Color32::WHITE);
        } else {
            painter.rect_filled(rect, corner, argb(THUMB_BACKGROUND));
        }
        if l.text {
            look.icon_in(ui, &icons::TEXT_LAYER, rect, argb(TEXT_GLYPH));
            return;
        }
        let stale = version.is_some_and(|v| self.thumbnails.get(&l.id).is_none_or(|(at, _)| *at != v));
        if stale && ui.is_rect_visible(rect) {
            if *read < THUMBS_A_FRAME {
                *read += 1;
                if let (Some(t), Some(v)) = (session.layer_thumbnail(l.id, THUMB_SIDE), version) {
                    let image =
                        egui::ColorImage::from_rgba_premultiplied([t.width as usize, t.height as usize], &t.rgba);
                    // small drawings show their pixels
                    let texture = ui.ctx().load_texture(format!("layer {}", l.id), image, TextureOptions::NEAREST);
                    self.thumbnails.insert(l.id, (v, texture));
                }
            } else {
                ui.ctx().request_repaint();
            }
        }
        if let Some((_, t)) = self.thumbnails.get(&l.id) {
            let size = t.size_vec2();
            let fit = (rect.width() / size.x.max(1.0)).min(rect.height() / size.y.max(1.0));
            let shown = Rect::from_center_size(rect.center(), size * fit);
            painter.image(t.id(), shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        // a plugin's layer: its kind's badge in its corner, as its manifest draws it
        if let Some(badge) = &l.plugin_badge {
            plugin_badge(&painter, rect, badge);
        }
    }
}

/// The +'s menu's rows: a plain layer, then each kind of layer, named as its plugin names it, beside
/// its badge (or a plug, when it draws none).
fn add_entries(kinds: &[procraft_core::PluginKind], filters: bool) -> Vec<Entry<'_>> {
    let mut entries = vec![Entry::Item(&icons::MENU_NEW_LAYER, "Layer", menu::INK)];
    let badged = |k: &procraft_core::PluginKind| procraft_core::plugin_kind_badge(k.clone());
    entries.extend(kinds.iter().map(|k| Entry::Badge(badged(k), k.label.as_str(), menu::INK)));
    // the filter layers the plugins offer, in a list of their own
    if filters {
        entries.push(Entry::Badge(procraft_core::PluginBadge::filter(), "Filter Layer", menu::INK));
    }
    entries
}

/// How wide the colour a plugin labels a layer with shows, down the left edge of its row.
const LABEL_WIDTH: f32 = 5.0;

/// A label's colour (0xRRGGBB) to draw.
fn labelled(label: u32) -> Color32 {
    let [_, r, g, b] = label.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

/// A plugin's layer's badge (its drawing in white over a dark rounded box) in the bottom right
/// corner of its thumbnail's `rect`.
fn plugin_badge(painter: &egui::Painter, rect: Rect, badge: &procraft_core::PluginBadge) {
    let area = Rect::from_min_size(rect.right_bottom() - vec2(3.0 + 18.0, 3.0 + 14.0), vec2(18.0, 14.0));
    painter.rect_filled(area, 4u8, Color32::from_black_alpha(0x99));
    badge_drawing(painter, area, badge, Color32::WHITE, 1.5);
}

/// A plugin's badge's lines and dots in `ink` over `area` (its box, 0 to 1 across and down): its
/// lines `width` thick, its dots squares twice that across.
pub fn badge_drawing(
    painter: &egui::Painter,
    area: Rect,
    badge: &procraft_core::PluginBadge,
    ink: Color32,
    width: f32,
) {
    let at = |x: f32, y: f32| pos2(area.left() + area.width() * x, area.top() + area.height() * y);
    let stroke = egui::Stroke::new(width, ink);
    for line in &badge.lines {
        let points: Vec<Pos2> = line.points.as_chunks::<2>().0.iter().map(|&[x, y]| at(x, y)).collect();
        if points.len() < 2 {
            continue;
        }
        if line.curved {
            for piece in points.windows(4).step_by(3) {
                let &[a, b, c, d] = piece else { continue };
                let curve = egui::epaint::CubicBezierShape::from_points_stroke(
                    [a, b, c, d],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                );
                painter.add(curve);
            }
        } else {
            painter.add(egui::Shape::line(points, stroke));
        }
    }
    for &[x, y] in badge.dots.as_chunks::<2>().0 {
        painter.rect_filled(Rect::from_center_size(at(x, y), vec2(width * 2.0, width * 2.0)), 0u8, ink);
    }
}

/// A model's texture set's name, heading its meshes: 41 tall, 16 under the set before it.
fn set_header(ui: &mut Ui, name: &str, first: bool) {
    if !first {
        ui.add_space(SET_GAP);
    }
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), SET_HEADER), Sense::hover());
    let ink = argb(PANEL_TITLE);
    let g = galley(ui, name, 14.0, Weight::SemiBold, ink, row.width() - 32.0);
    ui.painter().galley(row.min + vec2(16.0, 12.0), g, ink);
}

/// A control layer's badge, left of its blend letter: a pair of clean strokes for line art, a
/// loose zigzag for scribble, three level lines each shorter than the one below for depth (ground
/// going into the distance).
fn control_badge(painter: &egui::Painter, at: Pos2, kind: LayerControl, ink: Color32) {
    let stroke = egui::Stroke::new(1.6, ink);
    match kind {
        LayerControl::LineArt => {
            painter.line_segment([pos2(at.x - 6.0, at.y + 5.0), pos2(at.x + 2.0, at.y - 6.0)], stroke);
            painter.line_segment([pos2(at.x - 2.0, at.y + 6.0), pos2(at.x + 6.0, at.y - 3.0)], stroke);
        }
        LayerControl::Scribble => {
            let points = (0..=6)
                .map(|i| {
                    let x = at.x - 6.0 + i as f32 * 2.0;
                    let y = at.y + if i % 2 == 0 { -4.0 } else { 4.0 };
                    pos2(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
        LayerControl::Depth => {
            for (half, y) in [(6.0, 5.0), (4.0, 0.0), (2.0, -5.0)] {
                painter.line_segment([pos2(at.x - half, at.y + y), pos2(at.x + half, at.y + y)], stroke);
            }
        }
    }
}

/// `texture` fitted in `rect`, centred.
fn fit_picture(ui: &Ui, texture: &TextureHandle, rect: Rect) {
    let size = texture.size_vec2();
    let fit = (rect.width() / size.x.max(1.0)).min(rect.height() / size.y.max(1.0));
    let shown = Rect::from_center_size(rect.center(), size * fit);
    ui.painter().image(texture.id(), shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

/// A row's name, where it always is: 29 in from `at`, ending by `right`; a small padlock in the gap
/// before it when locked.
fn locked_name(ui: &Ui, look: &mut Look, l: &LayerInfo, at: Pos2, right: f32, ink: Color32) {
    if l.locked {
        look.icon(ui, &icons::LOCK_ROW, pos2(at.x + 3.0 + 8.5, at.y), 17.0, ink);
    }
    let g = galley(ui, &l.name, 17.0, Weight::Regular, ink, (right - at.x - 29.0 - 6.0).max(10.0));
    ui.painter().galley(pos2(at.x + 29.0, at.y - g.size().y / 2.0), g, ink);
}

/// How a row looks: its corners (top, bottom), its fill, its thumbnail's version and whether its
/// layer has a mask.
#[derive(Clone, Copy)]
struct RowLook {
    corners: (f32, f32),
    fill: Color32,
    version: Option<u64>,
    masked: bool,
    /// On a model: how far in it sits under its mesh, whether its texture set's meshes share it,
    /// and its channels' button (lit while one of them is painted). Whether it shows as chosen.
    indent: f32,
    shared: bool,
    cube: Option<bool>,
    chosen: bool,
    /// Held over by a dragged row long enough to take it: its thumbnail (a group's line) outlined.
    held: bool,
}

/// The panel's rows where they are laid out, as the core places a drop by them.
fn shown_rows(rows: &HashMap<u64, Rect>) -> Vec<ShownRow> {
    rows.iter().map(|(id, r)| ShownRow { id: *id, top: r.top(), bottom: r.bottom() }).collect()
}

/// Each closed group's layers as the list would show them open (a group closed inside it keeps
/// its own out), top first, each with how many groups in it sits under the closed one.
fn closed_contents(layers: &[LayerInfo]) -> HashMap<u64, Vec<(LayerInfo, u32)>> {
    let parent = |id: u64| layers.iter().find(|l| l.id == id).and_then(|l| l.parent);
    // the groups `l` is in, innermost first (a mask by its layer's)
    let groups = |l: &LayerInfo| {
        let mut out = Vec::new();
        let mut at = if l.kind == LayerType::Mask { l.parent.and_then(parent) } else { l.parent };
        while let Some(g) = at {
            if out.contains(&g) || out.len() > layers.len() {
                break;
            }
            out.push(g);
            at = parent(g);
        }
        out
    };
    let closed = |id: u64| layers.iter().any(|g| g.id == id && g.kind == LayerType::Group && g.collapsed);
    let mut contents: HashMap<u64, Vec<(LayerInfo, u32)>> = HashMap::new();
    for l in layers {
        let around = groups(l);
        for (k, g) in around.iter().enumerate() {
            if !closed(*g) {
                continue;
            }
            // shown in the window only if no group between it and this one is closed
            if around.iter().take(k).all(|inner| !closed(*inner)) {
                contents.entry(*g).or_default().push((l.clone(), k as u32));
            }
        }
    }
    contents
}

/// How far a row slides left to show its buttons: 6 past them.
fn swipe_reach(l: &LayerInfo) -> f32 {
    let buttons = if l.kind == LayerType::Mask { 2.0 } else { 3.0 };
    SWIPE_BUTTON * buttons + SWIPE_GAP * (buttons - 1.0) + 6.0
}

/// The modes in `list`, scrolled `scroll` from the first in the band: the band fixed in the
/// middle and the rows sliding under it, each name and glyph white where the band covers it and
/// grey elsewhere (a row half in it half and half).
fn blend_rows(ui: &Ui, look: &mut Look, list: Rect, modes: &[BlendModeInfo], scroll: f32) {
    let pad = (BLEND_LIST_HEIGHT - BLEND_ROW) / 2.0;
    let band = Rect::from_min_size(list.min + vec2(0.0, pad), vec2(list.width(), BLEND_ROW));
    ui.painter_at(list).rect_filled(band, 0, argb(pc::ACCENT));
    let above = Rect::from_min_max(list.min, pos2(list.right(), band.top()));
    let below = Rect::from_min_max(pos2(list.left(), band.bottom()), list.max);
    for (i, m) in modes.iter().enumerate() {
        let top = band.top() + i as f32 * BLEND_ROW - scroll;
        let row = Rect::from_min_size(pos2(list.left(), top), vec2(list.width(), BLEND_ROW));
        let glyph = icons::BLEND_GLYPHS.iter().find(|(name, _)| *name == m.name).map(|(_, glyph)| glyph);
        for (part, on) in [(above, false), (band, true), (below, false)] {
            // (a sliver of a hundredth of a point, from rounding, is nothing to draw)
            let shown = row.intersect(part);
            if shown.width() <= 0.0 || shown.height() < 0.01 {
                continue;
            }
            let painter = ui.painter_at(shown);
            let (ink, tint) =
                if on { (Color32::WHITE, Color32::WHITE) } else { (argb(PANEL_TITLE), argb(BLEND_GLYPH_INK)) };
            text(
                &painter,
                pos2(row.left() + 12.0, row.center().y),
                Align2::LEFT_CENTER,
                &m.name,
                17.0,
                Weight::Regular,
                ink,
            );
            if let Some(glyph) = glyph {
                look.icon_on(ui, &painter, glyph, pos2(row.right() - 31.5, row.center().y), 26.0, tint);
            }
        }
    }
}

/// The background: its colour and whether it shows. True when the row is tapped anywhere but on its
/// box, for its colour to be picked.
fn background_row(ui: &mut Ui, look: &mut Look, session: &CanvasSession) -> bool {
    let background = session.background();
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
    ui.painter().rect_filled(row, 17, argb(BACKGROUND_ROW));
    let swatch = Rect::from_min_size(pos2(row.left() + 4.0, row.center().y - 21.0), vec2(92.0, 42.0));
    let colour = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    // hidden, the swatch shows no colour: a faint white over the row
    let fill = if background.visible {
        Color32::from_rgb(colour(background.r), colour(background.g), colour(background.b))
    } else {
        Color32::from_white_alpha(15)
    };
    ui.painter().rect_filled(swatch, 14, fill);
    let ink = argb(PANEL_TITLE);
    let visibility = Rect::from_min_size(pos2(row.right() - 8.0 - 44.0, row.center().y - 20.5), vec2(44.0, 41.0));
    let g = galley(ui, "Background color", 17.0, Weight::Regular, ink, visibility.left() - swatch.right() - 14.0);
    ui.painter().galley(pos2(swatch.right() + 14.0, row.center().y - g.size().y / 2.0), g, ink);
    check_box(ui, look, visibility.center(), background.visible, argb(CHECK_FILL));
    if ui.interact(visibility, ui.scope_id().with("background visible"), Sense::click()).clicked() {
        session.set_background_visible(!background.visible);
        return false;
    }
    let rest = Rect::from_min_max(row.min, pos2(visibility.left(), row.max.y));
    ui.interact(rest, ui.scope_id().with("background colour"), Sense::click()).clicked()
}

/// A row of a plugin's action under the background's, drawn as that is, its label alone: true when
/// tapped.
fn action_row(ui: &mut Ui, label: &str) -> bool {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), ACTION_ROW), Sense::click());
    ui.painter().rect_filled(row, 17, argb(BACKGROUND_ROW));
    let ink = argb(PANEL_TITLE);
    let g = galley(ui, label, 17.0, Weight::Regular, ink, row.width() - 36.0);
    ui.painter().galley(pos2(row.left() + 18.0, row.center().y - g.size().y / 2.0), g, ink);
    response.clicked()
}

/// A visibility box: filled with a check cut out of it when shown, an outline when hidden.
fn check_box(ui: &Ui, look: &mut Look, centre: Pos2, checked: bool, fill: Color32) {
    let rect = Rect::from_center_size(centre, vec2(18.0, 18.0));
    let slot = match (checked, fill == Color32::WHITE) {
        (true, true) => "check on white",
        (true, false) => "check on grey",
        (false, true) => "check off white",
        (false, false) => "check off grey",
    };
    look.shape(ui, slot, fill.to_array(), rect, |pixmap, t| {
        let mut paint = tiny_skia::Paint::default();
        let [r, g, b, a] = fill.to_srgba_unmultiplied();
        paint.set_color_rgba8(r, g, b, a);
        paint.anti_alias = true;
        let mut pb = tiny_skia::PathBuilder::new();
        if checked {
            round_rect(&mut pb, Rect::from_min_size(Pos2::ZERO, vec2(18.0, 18.0)), [5.0; 4]);
            if let Some(p) = pb.finish() {
                pixmap.fill_path(&p, &paint, tiny_skia::FillRule::Winding, t, None);
            }
            // the tick is cut out of the box: the row shows through it (a 24 frame shown 14 across)
            let k = 14.0 / 24.0;
            let mut tick = tiny_skia::PathBuilder::new();
            tick.move_to(5.5, 12.5);
            tick.line_to(9.5, 16.5);
            tick.line_to(18.5, 7.5);
            if let Some(p) = tick.finish() {
                let cut = tiny_skia::Paint {
                    blend_mode: tiny_skia::BlendMode::Clear,
                    anti_alias: true,
                    ..Default::default()
                };
                let stroke = tiny_skia::Stroke {
                    width: 3.2,
                    line_cap: tiny_skia::LineCap::Round,
                    line_join: tiny_skia::LineJoin::Round,
                    ..Default::default()
                };
                let at = t.pre_translate(2.0, 2.0).pre_scale(k, k);
                pixmap.stroke_path(&p, &cut, &stroke, at, None);
            }
        } else {
            round_rect(&mut pb, Rect::from_min_size(pos2(0.75, 0.75), vec2(16.5, 16.5)), [4.25; 4]);
            if let Some(p) = pb.finish() {
                let stroke = tiny_skia::Stroke { width: 1.5, ..Default::default() };
                pixmap.stroke_path(&p, &paint, &stroke, t, None);
            }
        }
    });
}

/// The letters on a layer's row for its mode: a two-word name's initials (the second small), a
/// one-word name's first letter, or its first two where one letter is already taken.
fn blend_code(modes: &[BlendModeInfo], blend: u32) -> String {
    const TWO_LETTERS: [&str; 6] = ["Darken", "Shade", "Lighten", "Subtract", "Divide", "Saturation"];
    let Some(name) = modes.iter().find(|m| m.id == blend).map(|m| m.name.as_str()) else { return "N".into() };
    if TWO_LETTERS.contains(&name) {
        return name.chars().take(2).collect();
    }
    let mut words = name.split(' ');
    match (words.next(), words.next()) {
        (Some(a), Some(b)) => a.chars().take(1).chain(b.chars().take(1).flat_map(char::to_lowercase)).collect(),
        (Some(a), None) => a.chars().take(1).collect(),
        _ => "N".into(),
    }
}

/// Text on one line, cut short with an ellipsis past `max_width`.
fn galley(
    ui: &Ui,
    text: &str,
    size: f32,
    weight: Weight,
    colour: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    let format = TextFormat {
        font_id: weight.font(size),
        color: colour,
        extra_letter_spacing: -0.045 * size,
        ..Default::default()
    };
    job.append(text, 0.0, format);
    job.wrap = TextWrapping { max_width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.painter().layout_job(job)
}

/// Two rows pinched to this share of how far apart they were merge (a pinch a third closed merges,
/// opened again or not).
const PINCH_MERGE: f32 = 0.65;
/// How much of its finger's way a pinched row moves (as measured, about a quarter to a third).
const SQUEEZE_FOLLOW: f32 = 0.3;

/// Two fingers on one row tap it within this long (seconds), hold it from this long still, move
/// past this to be no tap or hold, and swipe it this far right (points).
const TWIN_TAP_S: f32 = 0.4;
const TWIN_HOLD_S: f32 = 0.5;
const TWIN_SLOP: f32 = 8.0;
const TWIN_SWIPE: f32 = 40.0;

/// Two fingers put on one row: its layer, since when, where their middle started, whether they
/// have moved, and whether they have done what they do.
struct Twin {
    id: u64,
    since: Instant,
    from: Pos2,
    moved: bool,
    done: bool,
}

/// Two fingers put on two rows: those rows' layers (the upper and the lower), the fingers' ids and
/// where they went down, how far each has gone since, and whether they came close enough that,
/// let go, the rows merge.
struct Pinch {
    upper: u64,
    lower: u64,
    fingers: (u64, u64),
    starts: (f32, f32),
    by: (f32, f32),
    pinched: bool,
}

impl Pinch {
    /// How far row `id` is moved, if it is one of the two, and whether it is the upper.
    fn moved(&self, id: u64) -> Option<(f32, bool)> {
        if id == self.upper {
            Some((self.by.0 * SQUEEZE_FOLLOW, true))
        } else if id == self.lower {
            Some((self.by.1 * SQUEEZE_FOLLOW, false))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, RawInput};

    #[test]
    fn blend_highlight_stays_centered_while_rows_move() {
        let (ctx, mut look) = (egui::Context::default(), Look::default());
        crate::look::install_fonts(&ctx);
        let modes = procraft_core::blend_modes();
        let list = Rect::from_min_size(pos2(12.0, 12.0), vec2(360.0, BLEND_LIST_HEIGHT));
        for scroll in [0.0, 2.4 * BLEND_ROW, 5.0 * BLEND_ROW] {
            let mut output = ctx.clone().run_ui(RawInput::default(), |ui| {
                blend_rows(ui, &mut look, list, &modes, scroll);
            });
            output.textures_delta.clear();
            let mut highlights = output.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == argb(pc::ACCENT) => Some(rect.rect),
                _ => None,
            });
            let highlight = highlights.next().expect("the center highlight is drawn");
            assert!((highlight.center().y - list.center().y).abs() < 0.01);
            assert!((highlight.height() - BLEND_ROW).abs() < 0.01);
            assert!(highlights.next().is_none());
        }
    }

    /// A mode's name is white where the band covers it and grey elsewhere: a row half in the band
    /// is drawn half white, half grey, split at the band's edge.
    #[test]
    fn a_blend_name_half_in_the_band_is_white_only_where_the_band_covers_it() {
        let (ctx, mut look) = (egui::Context::default(), Look::default());
        crate::look::install_fonts(&ctx);
        let modes = procraft_core::blend_modes();
        let list = Rect::from_min_size(pos2(12.0, 12.0), vec2(360.0, BLEND_LIST_HEIGHT));
        let pad = (BLEND_LIST_HEIGHT - BLEND_ROW) / 2.0;
        let band = Rect::from_min_size(list.min + vec2(0.0, pad), vec2(list.width(), BLEND_ROW));
        for scroll in [2.5 * BLEND_ROW, 2.25 * BLEND_ROW, 3.0 * BLEND_ROW, -20.0] {
            let mut output = ctx.clone().run_ui(RawInput::default(), |ui| {
                blend_rows(ui, &mut look, list, &modes, scroll);
            });
            output.textures_delta.clear();
            let mut split = 0;
            for (i, m) in modes.iter().enumerate() {
                let row = Rect::from_min_size(
                    pos2(list.left(), band.top() + i as f32 * BLEND_ROW - scroll),
                    vec2(list.width(), BLEND_ROW),
                );
                // the name's pieces: where each is drawn, and in which colour
                let pieces: Vec<(Rect, Color32)> = output
                    .shapes
                    .iter()
                    .filter_map(|c| match &c.shape {
                        egui::Shape::Text(t) if t.galley.text() == m.name => Some((c.clip_rect, t.fallback_color)),
                        _ => None,
                    })
                    .collect();
                let white: Vec<Rect> = pieces.iter().filter(|(_, c)| *c == Color32::WHITE).map(|(r, _)| *r).collect();
                let grey: Vec<Rect> = pieces.iter().filter(|(_, c)| *c == argb(PANEL_TITLE)).map(|(r, _)| *r).collect();
                assert_eq!(white.len() + grey.len(), pieces.len(), "{}: {pieces:?}", m.name);
                // white just where the band covers the row, grey over the rest of it shown
                let covered = row.intersect(band);
                if covered.height() >= 0.01 {
                    assert_eq!(white, vec![covered], "{} at {scroll}", m.name);
                } else {
                    assert!(white.is_empty(), "{} at {scroll}: {white:?}", m.name);
                }
                assert!(grey.iter().all(|r| r.intersect(band).height() < 0.01), "{} at {scroll}: {grey:?}", m.name);
                let shown = row.intersect(list).height().max(0.0);
                let drawn = grey.iter().chain(&white).map(|r| r.height()).sum::<f32>();
                assert!((drawn - shown).abs() < 0.02, "{} at {scroll}: {drawn} of {shown}", m.name);
                if !white.is_empty() && !grey.is_empty() {
                    split += 1;
                }
            }
            // between two rows both are split, on a row none, stretched past the top the first
            let expected = match scroll {
                s if s < 0.0 => 1,
                s if (s / BLEND_ROW - (s / BLEND_ROW).round()).abs() < 1e-4 => 0,
                _ => 2,
            };
            assert_eq!(split, expected, "at {scroll}");
        }
    }

    /// A blend editor open on a new artwork's first layer, drawn a frame at a time, 1/60 s apart.
    struct BlendEditorRun {
        ctx: egui::Context,
        look: Look,
        session: std::sync::Arc<CanvasSession>,
        layers: Layers,
        modes: Vec<BlendModeInfo>,
        id: u64,
        frame: u32,
    }

    impl BlendEditorRun {
        /// Opened on the first mode in the list.
        fn new(session: std::sync::Arc<CanvasSession>) -> BlendEditorRun {
            let (ctx, look) = (egui::Context::default(), Look::default());
            crate::look::install_fonts(&ctx);
            let id = session.layers()[0].id;
            let modes = procraft_core::blend_modes();
            let mut run = BlendEditorRun { ctx, look, session, layers: Layers::default(), modes, id, frame: 0 };
            run.put(0);
            // a frame first, for the editor's parts to be there to touch
            run.frame(Vec::new());
            run
        }

        /// The layer's mode the list's `index`th, there in the band.
        fn put(&mut self, index: usize) {
            self.session.set_layer_blend(self.id, self.modes[index].id);
            let scroll = index as f32 * BLEND_ROW;
            self.layers.blend = Some(Blend { id: self.id, scroll, motion: BlendMotion::Resting });
        }

        /// The middle of the band, `rows` rows down from it.
        fn row(&self, rows: f32) -> Pos2 {
            // the list's top: under the opacity line and slider, 11 points down
            pos2(150.0, 22.0 + 23.0 + 18.0 + 2.0 + 11.0 + BLEND_LIST_HEIGHT / 2.0 + rows * BLEND_ROW)
        }

        /// One frame with `events`: then the layer's mode, and how far the list is scrolled.
        fn frame(&mut self, events: Vec<Event>) -> (u32, f32) {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 640.0))),
                time: Some(f64::from(self.frame) / 60.0),
                events,
                ..RawInput::default()
            };
            self.frame += 1;
            let l = self.session.layers().into_iter().find(|l| l.id == self.id).unwrap();
            let rect = Rect::from_min_size(Pos2::ZERO, vec2(384.0, BLEND_EDITOR_HEIGHT));
            let (layers, look, session, modes) = (&mut self.layers, &mut self.look, &self.session, &self.modes);
            let mut output = self.ctx.clone().run_ui(raw, |ui| layers.blend_editor(ui, look, session, &l, rect, modes));
            output.textures_delta.clear();
            (self.mode(), self.layers.blend.as_ref().unwrap().scroll)
        }

        fn mode(&self) -> u32 {
            self.session.layers().into_iter().find(|l| l.id == self.id).unwrap().blend
        }

        fn resting(&self) -> bool {
            matches!(self.layers.blend.as_ref().unwrap().motion, BlendMotion::Resting)
        }

        fn press(at: Pos2, pressed: bool) -> Event {
            Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
        }

        /// A press at `from`, moved `by` points down in `steps` steps a frame apart, held still
        /// `still` frames, then let go: each frame's mode and scroll, the last after letting go.
        fn drag(&mut self, from: Pos2, by: f32, steps: u32, still: u32) -> Vec<(u32, f32)> {
            let mut seen = vec![self.frame(vec![Event::PointerMoved(from), Self::press(from, true)])];
            let mut at = from;
            for _ in 0..steps {
                at.y += by / steps as f32;
                seen.push(self.frame(vec![Event::PointerMoved(at)]));
            }
            for _ in 0..still {
                seen.push(self.frame(Vec::new()));
            }
            seen.push(self.frame(vec![Self::press(at, false)]));
            seen
        }

        /// Frames with nothing done until the list rests (at most `most` of them): each frame's
        /// mode and scroll.
        fn settle(&mut self, most: usize) -> Vec<(u32, f32)> {
            let mut seen = Vec::new();
            while !self.resting() {
                assert!(seen.len() < most, "still moving after {most} frames");
                seen.push(self.frame(Vec::new()));
            }
            seen
        }

        /// A click at `at`: the mode and scroll after it.
        fn click(&mut self, at: Pos2) -> (u32, f32) {
            self.frame(vec![Event::PointerMoved(at), Self::press(at, true)]);
            self.frame(vec![Self::press(at, false)])
        }
    }

    /// Flicked at 1000 points a second, the modes coast about 500 points, slowing straight onto a
    /// row: each frame they move less than the last (the last about a quarter point onto the row
    /// at most), never turning, with no second move to centre them.
    #[test]
    fn a_flicked_blend_list_coasts_about_500_points_straight_onto_a_row() {
        let rows = procraft_core::blend_modes().len() as u32;
        let mut blend = Blend { id: 1, scroll: 0.0, motion: BlendMotion::Resting };
        blend.let_go(1000.0, 0.0, rows);
        let (mut last, mut last_moved, mut frames) = (0.0f32, f32::MAX, 0);
        while blend.moving() {
            frames += 1;
            assert!(frames < 120 * 10, "still coasting after 10 s");
            blend.advance(f64::from(frames) / 120.0, rows);
            let moved = blend.scroll - last;
            let most = if blend.moving() { last_moved + 5e-4 } else { 0.3 };
            assert!((0.0..=most).contains(&moved), "sped up or turned at frame {frames}: {moved} after {last_moved}");
            (last, last_moved) = (blend.scroll, moved);
        }
        assert!((blend.scroll - 500.0).abs() <= 50.0, "coasted {} points", blend.scroll);
        assert_eq!(blend.scroll, (blend.scroll / BLEND_ROW).round() * BLEND_ROW, "between rows");
        eprintln!("flicked at 1000 pt/s: {} pt, at rest after {:.2} s", blend.scroll, frames as f32 / 120.0);
    }

    /// Pulled past either end the modes stretch, about 48 points for a 100 point pull and 84 for
    /// 200 (no hard stop), the end mode staying the layer's; let go, they spring back.
    #[test]
    fn the_blend_list_stretches_past_either_end_and_springs_back() {
        let mut run = BlendEditorRun::new(CanvasSession::new(64, 64));
        let first = run.mode();
        // the first mode in the band, pulled down past the top: 100 points, then 100 more
        let seen = run.drag(run.row(0.0), 100.0, 25, 0);
        let at_100 = -seen[seen.len() - 2].1;
        assert!((at_100 - 48.0).abs() < 2.0, "100 points stretched {at_100}");
        run.settle(120);
        let seen = run.drag(run.row(0.0), 200.0, 50, 0);
        let at_200 = -seen[seen.len() - 2].1;
        assert!((at_200 - 84.0).abs() < 2.5, "200 points stretched {at_200}");
        assert!(seen.iter().all(|(mode, _)| *mode == first), "the first mode stays");
        assert_eq!(run.settle(120).last().unwrap().1, 0.0);
        eprintln!("stretched: 100 pt {at_100:.1} pt, 200 pt {at_200:.1} pt");
        // the last mode in the band, pulled up past the bottom
        let last = (run.modes.len() - 1) as f32 * BLEND_ROW;
        run.put(run.modes.len() - 1);
        let seen = run.drag(run.row(0.0), -100.0, 25, 0);
        let stretched = seen[seen.len() - 2].1 - last;
        assert!((stretched - 48.0).abs() < 2.0, "100 points stretched {stretched}");
        assert_eq!(run.settle(120).last().unwrap().1, last);
    }

    /// Dragged slowly through the modes, each shows on the layer as it reaches the band, in turn;
    /// let go, the list rests on a row and its mode is the layer's.
    #[test]
    fn a_dragged_blend_list_shows_each_mode_reaching_the_band() {
        let mut run = BlendEditorRun::new(CanvasSession::new(64, 64));
        let mut seen = run.drag(run.row(0.0), -3.3 * BLEND_ROW, 64, 20);
        seen.extend(run.settle(600));
        let mut shown: Vec<u32> = Vec::new();
        for (mode, _) in &seen {
            if shown.last() != Some(mode) {
                shown.push(*mode);
            }
        }
        let (mode, scroll) = *seen.last().unwrap();
        let row = (scroll / BLEND_ROW).round();
        assert_eq!(scroll, row * BLEND_ROW, "rests between rows");
        assert!(row >= 3.0, "dragged 3.3 rows, rests on row {row}");
        let expected: Vec<u32> = run.modes[..=row as usize].iter().map(|m| m.id).collect();
        assert_eq!(shown, expected, "each mode in turn");
        assert_eq!(mode, run.modes[row as usize].id);
    }

    /// A tap on a mode below the band makes it the layer's at once, with none between shown on
    /// the way; the list takes 0.3 s to bring it to the band, ignoring presses meanwhile. A tap on
    /// the band's own mode does nothing.
    #[test]
    fn a_tapped_blend_mode_applies_at_once_with_none_between_shown() {
        let mut run = BlendEditorRun::new(CanvasSession::new(64, 64));
        let target = run.modes[3].id;
        let (mode, scroll) = run.click(run.row(3.0));
        assert_eq!((mode, scroll), (target, 0.0), "the mode at once, the list not yet moved");
        let mut seen = vec![(mode, scroll)];
        // a drag meanwhile does nothing
        seen.extend(run.drag(run.row(0.0), 60.0, 6, 0));
        seen.extend(run.settle(60));
        let names: Vec<String> =
            seen.iter().map(|(m, _)| run.modes.iter().find(|i| i.id == *m).unwrap().name.clone()).collect();
        assert!(seen.iter().all(|(mode, _)| *mode == target), "modes shown: {names:?}");
        assert_eq!(seen.last().unwrap().1, 3.0 * BLEND_ROW);
        // 0.3 s: 18 frames after the tap's
        let arrived = seen.iter().position(|(_, scroll)| *scroll == 3.0 * BLEND_ROW).unwrap();
        assert!((17..=19).contains(&arrived), "arrived after {arrived} frames");
        // the band's own mode: nothing
        assert_eq!(run.click(run.row(0.0)), (target, 3.0 * BLEND_ROW));
        assert!(run.resting());
        eprintln!("tapped 3 rows down: shown {:?}, in the band after {arrived} frames", names.first());
    }

    /// A movement through the modes is one undo step, "blend mode", and a tap after it another.
    #[test]
    fn a_blend_movement_is_one_undo_step_and_a_tap_another() {
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return eprintln!("skipped: no GPU") };
        // the first mode put before there are steps to undo
        let mut run = BlendEditorRun::new(CanvasSession::new(64, 64));
        run.session.use_gpu(gpu);
        run.session.show_in(200, 200);
        let first = run.mode();
        run.drag(run.row(0.0), -2.2 * BLEND_ROW, 40, 20);
        let (moved, _) = *run.settle(600).last().unwrap();
        assert_ne!(moved, first);
        let (tapped, _) = run.click(run.row(2.0));
        run.settle(60);
        assert_ne!(tapped, moved);
        assert_eq!(run.session.undo().as_deref(), Some("blend mode"));
        assert_eq!(run.mode(), moved);
        assert_eq!(run.session.undo().as_deref(), Some("blend mode"));
        assert_eq!(run.mode(), first);
        // no third step
        run.session.undo();
        assert_eq!(run.mode(), first);
    }

    /// A tap on the background's row, on its box or left of it: whether the row asked for its
    /// colour, and whether the background shows after.
    fn tap_background_row(on_box: bool) -> (bool, bool) {
        let (ctx, mut look) = (egui::Context::default(), Look::default());
        crate::look::install_fonts(&ctx);
        let session = CanvasSession::new(64, 64);
        let (mut row, mut asked) = (Rect::NOTHING, false);
        let press = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for step in 0..6 {
            // the box's middle 8 + 22 in from the row's right; the name half way along the row
            let at = if on_box { pos2(row.right() - 30.0, row.center().y) } else { row.center() };
            let events = match step {
                2 => vec![Event::PointerMoved(at)],
                3 => vec![press(at, true)],
                4 => vec![press(at, false)],
                _ => Vec::new(),
            };
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(384.0, 200.0))),
                time: Some(f64::from(step) / 60.0),
                events,
                ..RawInput::default()
            };
            let mut output = ctx.clone().run_ui(raw, |ui| {
                row = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 50.0));
                asked |= background_row(ui, &mut look, &session);
            });
            output.textures_delta.clear();
        }
        (asked, session.background().visible)
    }

    /// A finger dragged 180 points up a row of a long list, quickly (before a hold picks the row
    /// up): how far the row moved up, points.
    fn finger_drag_up_a_row() -> f32 {
        let (ctx, mut look) = (egui::Context::default(), Look::default());
        crate::look::install_fonts(&ctx);
        let session = CanvasSession::new(64, 64);
        for _ in 0..24 {
            session.add_layer();
        }
        let (mut layers, mut colours) = (Layers::default(), Colours::default());
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
        let button = Rect::from_min_size(pos2(1000.0, 10.0), vec2(40.0, 40.0));
        let mut step = 0;
        let mut frame = |layers: &mut Layers, events: Vec<Event>| {
            let raw = RawInput {
                screen_rect: Some(screen),
                time: Some(f64::from(step) / 60.0),
                events,
                ..RawInput::default()
            };
            step += 1;
            let mut output = ctx.clone().run_ui(raw, |ui| {
                layers.show(
                    ui.ctx(),
                    &mut look,
                    &session,
                    &mut colours,
                    crate::panel::Hang::Button(button),
                    &mut Vec::new(),
                );
            });
            output.textures_delta.clear();
        };
        for _ in 0..4 {
            frame(&mut layers, Vec::new());
        }
        // a row half way down the list as shown
        let list = layers.list.unwrap();
        let mut rows: Vec<(u64, Rect)> =
            layers.last_rows.iter().filter(|(_, r)| list.contains(r.center())).map(|(id, r)| (*id, *r)).collect();
        rows.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        let (id, row) = rows[rows.len() / 2];
        let touch = |phase, pos| Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: None,
        };
        let button_at = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        // as egui-winit gives a touch: the touch, and the pointer it moves
        let mut at = row.center();
        frame(&mut layers, vec![touch(egui::TouchPhase::Start, at), Event::PointerMoved(at), button_at(at, true)]);
        for _ in 0..15 {
            at.y -= 12.0;
            frame(&mut layers, vec![touch(egui::TouchPhase::Move, at), Event::PointerMoved(at)]);
        }
        frame(&mut layers, vec![touch(egui::TouchPhase::End, at), button_at(at, false), Event::PointerGone]);
        for _ in 0..4 {
            frame(&mut layers, Vec::new());
        }
        layers.last_rows.get(&id).map_or(0.0, |after| row.top() - after.top())
    }

    #[test]
    fn a_finger_dragged_up_a_row_scrolls_the_list() {
        let moved = finger_drag_up_a_row();
        assert!(moved > 120.0, "the row moved {moved} points up");
    }

    #[test]
    fn the_plus_menu_offers_a_new_layer_then_each_kind_of_layer_beside_its_badge() {
        let curve = procraft_core::BadgeLine { points: vec![0.2, 0.7, 0.4, 0.1, 0.6, 0.9, 0.8, 0.3], curved: true };
        let badge = procraft_core::PluginBadge { lines: vec![curve], dots: vec![0.2, 0.7, 0.8, 0.3] };
        let kind = |key: &str, label: &str, badge| procraft_core::PluginKind {
            key: key.to_owned(),
            label: label.to_owned(),
            badge,
            exports: Vec::new(),
        };
        let kinds = [
            kind("org.example.curves.curve", "Curves", Some(badge.clone())),
            kind("org.example.plain.plain", "Plain", None),
        ];
        // with filter layers offered, Filter Layer last, beside the filter badge
        let with_filters = add_entries(&kinds, true);
        let filter = procraft_core::PluginBadge::filter();
        assert!(matches!(with_filters.last(), Some(Entry::Badge(b, "Filter Layer", _)) if *b == filter));
        let entries = add_entries(&kinds, false);
        assert_eq!(entries.len(), 3);
        let first = matches!(&entries[0], Entry::Item(icon, "Layer", _) if icon.name == "menunewlayer");
        assert!(first, "a plain layer first, beside the new layer glyph");
        assert!(matches!(&entries[1], Entry::Badge(b, "Curves", _) if *b == badge), "a kind beside its badge");
        let plug = procraft_core::PluginBadge::plug();
        assert!(matches!(&entries[2], Entry::Badge(b, "Plain", _) if *b == plug), "a plug when a kind draws none");
        // drawn, a click on the last row picks it
        let (ctx, mut look) = (egui::Context::default(), Look::default());
        crate::look::install_fonts(&ctx);
        let rect = Rect::from_min_size(pos2(20.0, 20.0), vec2(menu::LIBRARY_WIDTH, menu::entries_height(&entries)));
        let at = pos2(rect.center().x, rect.top() + menu::entry_top(&entries, 2) + menu::ITEM_HEIGHT / 2.0);
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut tapped = None;
        for step in 0..6 {
            let events = match step {
                2 => vec![Event::PointerMoved(at)],
                3 => vec![press(true)],
                4 => vec![press(false)],
                _ => Vec::new(),
            };
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                time: Some(f64::from(step) / 60.0),
                events,
                ..RawInput::default()
            };
            let mut output = ctx.clone().run_ui(raw, |ui| {
                tapped = tapped.or(menu::entries(ui, &mut look, rect, &entries));
            });
            output.textures_delta.clear();
        }
        assert_eq!(tapped, Some(2), "the plain kind's row");
    }

    /// The plugins' actions for the Layers panel are rows under the background's, as on the tablet:
    /// the panel grows to show them (not left below its bottom, to be found only by scrolling), and
    /// a tap on one runs it.
    #[test]
    fn the_plugins_rows_under_the_background_show_in_the_panel_and_run_when_tapped() {
        let (ctx, mut look) = (egui::Context::default(), Look::default());
        crate::look::install_fonts(&ctx);
        let session = CanvasSession::new(64, 64);
        let (mut layers, mut colours) = (Layers::default(), Colours::default());
        let action = |key: &str, label: &str| procraft_core::PluginAction {
            key: key.to_owned(),
            label: label.to_owned(),
            place: procraft_core::PluginPlace::Layers,
        };
        layers.panel_actions = vec![action("p.settings", "Paper"), action("p.views", "Layer view")];
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
        let button = Rect::from_min_size(pos2(1000.0, 10.0), vec2(40.0, 40.0));
        let mut step = 0;
        let mut frame = |layers: &mut Layers, events: Vec<Event>| {
            let raw = RawInput {
                screen_rect: Some(screen),
                time: Some(f64::from(step) / 60.0),
                events,
                ..RawInput::default()
            };
            step += 1;
            let mut output = ctx.clone().run_ui(raw, |ui| {
                layers.show(
                    ui.ctx(),
                    &mut look,
                    &session,
                    &mut colours,
                    crate::panel::Hang::Button(button),
                    &mut Vec::new(),
                );
            });
            output.textures_delta.clear();
            output.shapes
        };
        let mut shapes = Vec::new();
        for _ in 0..30 {
            shapes = frame(&mut layers, Vec::new());
        }
        let panel = layers.box_on_window.expect("the panel shows");
        // each row's label drawn whole inside the panel, and not clipped away
        let shown = |label: &str| {
            shapes.iter().find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    let at = Rect::from_min_size(t.pos, t.galley.size());
                    Some((at, c.clip_rect.contains_rect(at)))
                }
                _ => None,
            })
        };
        let mut middles = Vec::new();
        for label in ["Background color", "Paper", "Layer view"] {
            let (at, whole) = shown(label).unwrap_or_else(|| panic!("{label} drawn"));
            assert!(
                whole && panel.contains_rect(at),
                "{label} at {at:?}, in the panel {panel:?}, not clipped: {whole}"
            );
            middles.push(at.center());
        }
        assert!(middles[0].y < middles[1].y && middles[1].y < middles[2].y, "under the background, in order");
        // Layer view's row tapped: its action runs
        let press = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let at = middles[2];
        frame(&mut layers, vec![Event::PointerMoved(at)]);
        frame(&mut layers, vec![press(at, true)]);
        frame(&mut layers, vec![press(at, false)]);
        assert_eq!(layers.run_action.as_deref(), Some("p.views"));
    }

    #[test]
    fn the_background_row_tapped_asks_for_its_colour_and_its_box_tapped_hides_it() {
        assert_eq!(tap_background_row(false), (true, true), "the row: its colour, still shown");
        assert_eq!(tap_background_row(true), (false, false), "the box: hidden, no colour asked");
    }

    /// Top down: x, a closed group (c, b), z. Then x's row dragged with a mouse until its middle is
    /// `into` the way down the group's row (0 its top, 1 its bottom), held there `hold`, and let
    /// go. Returns the group x ends up in, the ids top down, and the group's id.
    fn drag_by_a_closed_group(into: f32, hold: std::time::Duration) -> (Option<u64>, Vec<u64>, u64) {
        let (ctx, mut look) = (egui::Context::default(), Look::default());
        crate::look::install_fonts(&ctx);
        let session = CanvasSession::new(64, 64);
        session.add_layer();
        let c = session.add_layer();
        let group = session.combine_down(c).unwrap();
        session.select_layer(group);
        let x = session.add_layer();
        session.set_group_collapsed(group, true);
        let (mut layers, mut colours) = (Layers::default(), Colours::default());
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
        let button = Rect::from_min_size(pos2(1000.0, 10.0), vec2(40.0, 40.0));
        let mut step = 0;
        let mut frame = |layers: &mut Layers, events: Vec<Event>| {
            let raw = RawInput {
                screen_rect: Some(screen),
                time: Some(f64::from(step) / 60.0),
                events,
                ..RawInput::default()
            };
            step += 1;
            let mut output = ctx.clone().run_ui(raw, |ui| {
                layers.show(
                    ui.ctx(),
                    &mut look,
                    &session,
                    &mut colours,
                    crate::panel::Hang::Button(button),
                    &mut Vec::new(),
                );
            });
            output.textures_delta.clear();
        };
        for _ in 0..12 {
            frame(&mut layers, Vec::new());
        }
        let (from, to) = (layers.last_rows[&x], layers.last_rows[&group]);
        assert!(to.height() > GROUP_HEIGHT + 30.0, "a closed group's row is taller: {to:?}");
        let press = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut at = from.center();
        frame(&mut layers, vec![Event::PointerMoved(at), press(at, true)]);
        let goal = to.top() + to.height() * into;
        for _ in 0..20 {
            at.y += (goal - from.center().y) / 20.0;
            frame(&mut layers, vec![Event::PointerMoved(at)]);
        }
        let until = Instant::now() + hold;
        while Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(50));
            frame(&mut layers, vec![Event::PointerMoved(at)]);
        }
        frame(&mut layers, vec![Event::PointerMoved(at)]);
        frame(&mut layers, vec![press(at, false)]);
        for _ in 0..2 {
            frame(&mut layers, Vec::new());
        }
        let after = session.layers();
        let parent = after.iter().find(|l| l.id == x).and_then(|l| l.parent);
        (parent, after.iter().map(|l| l.id).collect(), group)
    }

    #[test]
    fn a_row_let_go_under_a_closed_group_stays_out_of_it_and_held_onto_it_goes_in() {
        let (parent, order, group) = drag_by_a_closed_group(0.75, std::time::Duration::ZERO);
        assert_eq!(parent, None, "under the group's middle: out of it, under it ({order:?})");
        assert_eq!(order.iter().position(|id| *id == group), Some(0));
        let (parent, order, group) = drag_by_a_closed_group(0.75, std::time::Duration::from_millis(700));
        assert_eq!(parent, Some(group), "held over its row: in it ({order:?})");
    }
}
