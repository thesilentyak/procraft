//! The brush library, as the Android app's (ui/BrushLibrary.kt): sets on the left, the shown set's
//! brushes with a sample stroke on the right. Tapping a set shows it, the shown one for its menu
//! (rename, duplicate, share, delete); tapping a brush chooses it; sliding one left shows Share,
//! Duplicate and Delete; holding one shows its menu (Rename, Duplicate, Share, Delete; in the Recent
//! set Find, Pin or Unpin, Share, Clear), and moving straight on from there drags it to another
//! place or set (the core says how long the hold is, and how soon the move must come). The plus
//! makes a brush or a set, or brings brushes in from a file. Two fingers pinched in on a library go
//! to every library's card, and pinched out on a card open it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align2, Color32, Pos2, Rect, ScrollArea, Sense, TextureHandle, TextureOptions, Ui, UiBuilder, Vec2, pos2, vec2,
};
use procraft_core::{
    BRUSH_MENU_HOLD, BrushDrop, BrushDropOver, BrushFileReading, BrushHoldMove, BrushInfo, BrushSetInfo, CanvasSession,
    ReadBrushes, brush_hold_move, read_brushes_from,
};

use crate::alert::{Alert, Answer};
use crate::icons;
use crate::look::{Look, Shade, Themed, Weight, argb, pc, text};
use crate::menu::{self, Entry, Menu};
use crate::panel::{self, PANEL_GLYPH, PANEL_TITLE};

mod libraries;

const LIBRARY_WIDTH: f32 = 440.0;
const SETS_WIDTH: f32 = 168.0;
const HEADER_HEIGHT: f32 = 52.0;
/// The search field with the space above and below it (out of sight until the lists are scrolled
/// up past their top), its fill and hint, and a found brush's tile.
const SEARCH_BLOCK: f32 = 60.0;
const SEARCH_FILL: u32 = 0x1F767680;
const SEARCH_HINT: u32 = 0x4DEBEBF5;
const RESULT_FILL: u32 = 0x14FFFFFF;
const RESULT_HEIGHT: f32 = 81.0;
/// The clear button's cross, cut through to the panel.
const CLEAR_CROSS: Themed = Themed(0xFF1E1E1E, 0xFFF2F2F2);
const SET_HEIGHT: f32 = 50.0;
const BRUSH_HEIGHT: f32 = 80.0;
const BRUSH_GAP: f32 = 8.0;
/// The library ends this far above the window's bottom edge.
const BOTTOM_MARGIN: f32 = 25.0;
const SET_COLUMN: Themed = Themed(0x00000000, 0xFFE6E6E6);
const SET_CELL: Themed = Themed(0x2E5B5B5B, 0xFFD7D7D7);
const BRUSH_COLUMN: Themed = Themed(0x26000000, 0xFFE0E0E0);
const NAME_PILL: u32 = 0x80000000;
/// The sample stroke's picture: half its own canvas, the cell showing it across its width.
const PREVIEW: (u32, u32) = (530, 162);
/// Sample strokes drawn on a thread of their own at a time, so those still in sight go next.
const PREVIEWS_AT_A_TIME: usize = 4;
/// How often the interface looks for sample strokes drawn while a thread is at them.
const PREVIEW_LOOK: Duration = Duration::from_millis(16);
/// How many sample strokes' pictures are kept on the GPU at most (each [`PREVIEW`] big, 343 KB:
/// 22 MB in all): past it, those out of sight go, the longest unseen first (to come back drawn
/// already, from the strokes the session keeps). Every brush's, as the library is browsed, came
/// to 205 MB for the 598 it ships with.
const PREVIEWS_KEPT: usize = 64;
/// Menus stay below the top bar; the set's menu sits this far left of it.
const MENU_TOP: f32 = 57.0;
const SET_MENU_GAP: f32 = 6.0;
/// The Recent set's menu, of one item, is this much taller than its item and margins (65 in all).
const RECENT_MENU_EXTRA: f32 = 11.0;
/// The set's menu: frosted glass over the canvas, edged.
const SET_MENU_GLASS: menu::Glass =
    menu::Glass { fill: menu::GLASS, edge: Some(menu::EDGE), frost: true, corner: menu::LIBRARY_CORNER, shadow: false };
/// A brush slid left shows three buttons filling its place, this far apart.
const SWIPE_GAP: f32 = 5.5;
const SWIPE_FILL: u32 = 0xFF3F3F3F;
const SWIPE_INK: u32 = 0xFFC5C5C5;
const SWIPE_DELETE: u32 = 0xFFFF3A46;
/// A brush slid right is picked, this fill over its row; it follows the pointer this far.
const PICKED_BRUSH: u32 = 0xFF2C5281;
const PICK_REACH: f32 = 72.0;
/// The set icons to choose from: five across, each cell this big, in a panel this big (edged,
/// nearly solid over the library), 11 left of the set, kept 25 above the window's bottom.
const PICKER_COLUMNS: usize = 5;
const PICKED_SET_ICONS: usize = 42;
const PICKER: (f32, f32) = (250.0, 325.0);
const PICKER_CELL: f32 = 48.0;
const PICKER_INSET: f32 = 11.0;
const PICKER_MARGIN: f32 = 25.0;
const PICKER_GLASS: menu::Glass = menu::Glass {
    fill: Themed(0xFA303335, 0xFA303335),
    edge: Some(menu::EDGE),
    frost: false,
    corner: 26.0,
    shadow: false,
};
/// A press moved this far (points) has gone somewhere; held still this long, a set lifts to be
/// dragged (a brush's menu shows sooner: [`brush_hold`]).
pub(crate) const SLOP: f32 = 8.0;
pub(crate) const HOLD: Duration = Duration::from_millis(400);

/// How long a brush is held still before its menu shows (the core's, as measured).
fn brush_hold() -> Duration {
    Duration::try_from_secs_f64(BRUSH_MENU_HOLD).unwrap_or(HOLD)
}
/// A dragged brush held this long over another set opens it, to be let go among its brushes
/// (measured on the original: 1.76 s).
const SPRING_LOAD: Duration = Duration::from_millis(1800);
/// A press slides its brush once this far sideways (points), going this many times further
/// sideways than up or down: a hand scrolling swings in an arc, and a slide opens onto Delete, so
/// anything less plainly sideways is left to the list.
const SLIDE_SLOP: f32 = 14.0;
const SLIDE_LEAN: f32 = 2.0;
/// Two fingers pinched on a library and let go at this much of their spread at the start (or
/// less) go to every library's card; pinched out on a card past this much, that library opens. A
/// trackpad's pinch is let go once it has been still this long.
const PINCH_TO_LIBRARIES: f32 = 0.66;
const PINCH_TO_LIBRARY: f32 = 2.0;
const PINCH_QUIET: Duration = Duration::from_millis(200);

/// A brush's sample stroke, whether it runs under the name (which then sits on a dark pill), and
/// the frame it was last in sight in.
struct Preview {
    texture: TextureHandle,
    shaded: bool,
    seen: u64,
}

/// A sample stroke drawn on its thread: the brush's, its picture (none if it draws nothing), and
/// whether it runs under the name.
type DrawnPreview = ((u32, u32), Option<(egui::ColorImage, bool)>);

/// A brush file to read: where it is, its name, and whether it is taken out once read (one kept
/// while no canvas was open).
pub struct BrushFileIn {
    pub path: std::path::PathBuf,
    pub name: String,
    pub taken: bool,
}

/// What a plugin's brush engine said of a brush file it had: a title when something went wrong (to
/// be told with an OK), and its text ("120 brushes imported.").
pub type ImportNotice = (Option<String>, String);

/// A batch of brush files read: their brushes (none if Cancel stopped it), and what plugins' brush
/// engines said of them.
type Imported = (Option<Vec<ReadBrushes>>, Vec<ImportNotice>);

/// What reading a batch of brush files came to: whether any brushes came in, whether to say so, and
/// what plugins' brush engines said of them.
pub struct Added {
    pub any: bool,
    pub tell: bool,
    pub notices: Vec<ImportNotice>,
}

/// A file's size as the import card shows it: "199 MB", "4.2 MB", "380 KB".
pub fn file_size(bytes: u64) -> String {
    let mb = bytes as f64 / f64::from(1 << 20);
    if mb >= 10.0 {
        format!("{mb:.0} MB")
    } else if mb >= 1.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{} KB", (bytes as f64 / 1024.0).round().max(1.0))
    }
}

/// What a press on a brush turned out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    Undecided,
    /// Sideways: the brush slides.
    Swipe,
    /// Held still: the brush's menu shows.
    Held,
    /// Held and moved on: the brush is dragged to another place or set.
    Drag,
    /// Up or down first: the list has it, and scrolls.
    Scroll,
}

/// The brushes dragged with brush `held`: those `picked` (all of its set) and it, in their order,
/// when it is one of them (the current brush, `selected`, counts as picked); else it alone.
fn dragged_together(held: (u32, u32), picked: &[(u32, u32)], selected: bool) -> Vec<(u32, u32)> {
    if picked.is_empty() || !(selected || picked.contains(&held)) {
        return vec![held];
    }
    let mut going: Vec<(u32, u32)> = picked.iter().copied().filter(|p| p.0 == held.0).collect();
    if !going.contains(&held) {
        going.push(held);
    }
    going.sort_unstable();
    going
}

/// What brushes dragged in the library are over at `at`, for the core to say where they land
/// ([`procraft_core::brush_drop_place`]): a set's row of `sets` (by place); else, in `list` (the
/// shown set's place, how many brushes it holds and where its list shows; none for the Recent
/// set), the brush of `brushes` (by place) nearest up or down, on its upper or lower half, or with
/// none there the list itself; else nothing.
fn drop_over(
    at: Pos2,
    sets: &[(usize, Rect)],
    list: Option<(u32, u32, Rect)>,
    brushes: &[((u32, u32), Rect)],
) -> Option<BrushDropOver> {
    if let Some(&(set, _)) = sets.iter().find(|(_, r)| r.contains(at)) {
        return Some(BrushDropOver::OnSet { set: u32::try_from(set).ok()? });
    }
    let (shown, count, area) = list?;
    if !area.contains(at) {
        return None;
    }
    // how far up or down from a row (none on it)
    let off = |r: &Rect| (r.top() - at.y).max(at.y - r.bottom()).max(0.0);
    let nearest = brushes.iter().filter(|((set, _), _)| *set == shown).min_by(|a, b| off(&a.1).total_cmp(&off(&b.1)));
    Some(match nearest {
        Some(&((_, index), r)) => BrushDropOver::OnBrush { set: shown, index, lower: at.y > r.center().y },
        None => BrushDropOver::InList { set: shown, count },
    })
}

/// What a press on a brush that has moved `d` (points) from where it went down is, once it can be
/// told: plainly sideways it slides the brush; else, gone up or down, the list has it.
fn way_moved(d: Vec2) -> Option<Way> {
    let (across, along) = (d.x.abs(), d.y.abs());
    if across >= SLIDE_SLOP && across >= along * SLIDE_LEAN {
        Some(Way::Swipe)
    } else if along >= SLOP && across < along * SLIDE_LEAN {
        Some(Way::Scroll)
    } else {
        None
    }
}

/// A press on a brush, followed until let go: which (set and place), where and when it went down,
/// what it became (and when its menu showed, held), how far the brush was slid, where on it the
/// press took hold, how far the pointer has moved since the last frame, and how far the list was
/// scrolled.
#[derive(Clone, Copy)]
struct Press {
    list: f32,
    key: (u32, u32),
    origin: Pos2,
    since: Instant,
    way: Way,
    menu_at: Option<Instant>,
    start: f32,
    grab: Vec2,
    last: Pos2,
}

/// A press on a set, followed until let go: which, where and when it went down, where on it the
/// press took hold, where the pointer is now, whether it was held still long enough to lift the set,
/// and whether it moved first (then it is the list's, to scroll).
#[derive(Clone, Copy)]
struct SetPress {
    set: usize,
    origin: Pos2,
    since: Instant,
    grab: Vec2,
    last: Pos2,
    lifted: bool,
    spent: bool,
}

/// Two fingers pinching on the library: how far (their spread now, a part of what it was), where
/// (in the panel), when they last moved, and whether on a touch screen (else a trackpad).
#[derive(Clone, Copy)]
struct Pinch {
    zoom: f32,
    at: Pos2,
    last: Instant,
    touch: bool,
}

/// What Delete asks about.
#[derive(Clone, Copy)]
enum Deleting {
    Set(u32),
    Brush(u32, u32),
    Picked,
    Library(u32),
}

#[derive(Default)]
pub struct Library {
    /// Floating in a window of its own: what its strips had done to them this frame.
    pub chrome: panel::FloatingChrome,
    /// Hanging from its button, whether a plugin lets it float (its grabber then shows), and while
    /// the grabber is dragged, where the press began and where the pointer is (the window's points).
    pub floats: bool,
    pub undock: Option<(egui::Pos2, egui::Pos2)>,
    /// The set shown, once one is tapped (else the current brush's).
    shown_set: Option<usize>,
    /// The Recent set is shown instead (the brushes last painted with, and those pinned there);
    /// its menu (Clear), and a brush found from it, to scroll to in its own set.
    recent: bool,
    recent_menu: Menu,
    found: Option<(u32, u32)>,
    previews: HashMap<(u32, u32), Option<Preview>>,
    /// The frames shown so far, by which the sample strokes are seen.
    frames: u64,
    /// The brushes in sight without a sample stroke yet, and those being drawn on a thread of
    /// their own (the GPU takes a while over each: the list isn't held up for it).
    wanted: Vec<((u32, u32), String)>,
    drawing: Option<std::sync::mpsc::Receiver<DrawnPreview>>,
    plus_menu: Menu,
    /// The shown set's menu, and which set it is for.
    set_menu: Menu,
    set_menu_for: Option<usize>,
    /// The set's icons to choose from (Customize icon), and whether it has been scrolled to the
    /// set's own.
    icon_picker: Menu,
    picker_scrolled: bool,
    /// A set renamed in place: which, the name as typed, and whether it has been focused.
    renaming_set: Option<(usize, String, bool)>,
    /// A brush's menu (a brush held), and for which brush (in the Recent set, whether it is
    /// pinned there); a brush renamed in place from it: which, the name as typed, and whether it
    /// has been focused.
    brush_menu: Menu,
    brush_menu_for: Option<((u32, u32), Option<bool>)>,
    renaming_brush: Option<((u32, u32), String, bool)>,
    deleting: Alert,
    delete_what: Option<Deleting>,
    /// Every library's card shows rather than the one in use; the title's menu; the plus's menu
    /// over the cards; a card's menu and for which; a library renamed in place (its place, the
    /// name as typed, and whether it has been focused); where the cards were last drawn.
    showing_libraries: bool,
    title_menu: Menu,
    libraries_plus: Menu,
    card_menu: Menu,
    card_menu_for: Option<usize>,
    renaming_library: Option<(usize, String, bool)>,
    card_rects: Vec<(usize, Rect)>,
    /// The cards' brush strokes, drawn small, by library, set and place (none: it can't be).
    card_previews: HashMap<(u32, u32, u32), Option<TextureHandle>>,
    /// Strokes forgotten in the middle of a frame that may still draw them: let go at the next.
    card_retired: Vec<TextureHandle>,
    /// The cards' strokes still to draw, and those being drawn on a thread of their own (another
    /// library's brushes are read from its file, which can be large).
    card_wanted: Vec<(u32, u32, u32)>,
    card_drawing: Option<std::sync::mpsc::Receiver<libraries::CardStroke>>,
    press: Option<Press>,
    set_press: Option<SetPress>,
    pinch: Option<Pinch>,
    /// How far each brush is slid (points, left negative), and the brush slid open.
    slid: HashMap<(u32, u32), f32>,
    swiped: Option<(u32, u32)>,
    /// The brushes of the shown set picked by sliding them right.
    picked: Vec<(u32, u32)>,
    /// Words for the app to show over the canvas (Combine refused), taken by it.
    pub notice: Option<String>,
    /// The brushes a dragged one takes with it (it among them, those picked in its set), in their
    /// order, and the dragged cell's size (its row may go as another set opens); the set it is
    /// over, and since when (held there a moment, it opens).
    dragged_with: Vec<(u32, u32)>,
    dragged_size: Vec2,
    spring: Option<(usize, Instant)>,
    /// How far the brushes list is scrolled.
    list_at: f32,
    /// Where the sets and the shown set's brushes were last drawn, for a dragged brush to land; and
    /// the shown set's list (its place, how many brushes it holds, where it shows), not the Recent
    /// set's.
    set_rects: Vec<(usize, Rect)>,
    brush_rects: Vec<((u32, u32), Rect)>,
    brush_list: Option<(u32, u32, Rect)>,
    /// Where the search's results were last drawn (none while it isn't in use).
    result_rects: Vec<((u32, u32), Rect)>,
    /// The brush shown highlighted last frame: the one in use, where it is listed.
    highlighted: Option<(u32, u32)>,
    /// The library changed: the app keeps it; its sample strokes are drawn again next frame (not
    /// dropped in the middle of one that shows them).
    pub changed: bool,
    stale: bool,
    /// The studio asked for: on the brush now chosen, or on one just made (Cancel throws it away).
    pub studio: Option<Option<(u32, u32)>>,
    /// Brush files read on threads of their own (another app's big ones take a while): each
    /// batch's brushes come back (a file that held none left out; none at all if Cancel stopped
    /// it) with what plugins' brush engines said of them, and whether to say that they came in.
    importing: Vec<(std::sync::mpsc::Receiver<Imported>, bool)>,
    /// A brush file picked from a "+" menu, to be read (with the import card, which the app
    /// shows), and whether to say that its brushes came in.
    picked_file: Option<(std::path::PathBuf, String, bool)>,
    /// The search: what is typed, whether its field has the keys, whether it has come down into
    /// sight, and whether the sets and the brushes were at their tops last frame.
    query: String,
    searching: bool,
    search_down: bool,
    lists_top: (bool, bool),
    /// What the sample strokes are tinted with as a plugin colours them (each brush's, then the
    /// brush in use's), as this frame began; none, as the interface draws them.
    tints: [Option<Color32>; 2],
    /// Whether it was shown this frame and the last (see [`Library::after_frame`]).
    shown_now: bool,
    shown_last: bool,
}

/// A colour (0xRRGGBB) to draw.
pub(crate) fn opaque(rgb: u32) -> Color32 {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color32::from_rgb(r, g, b)
}

impl Library {
    /// The tint of a sample stroke on a row (`selected`: the brush in use's, on the accent), as a
    /// plugin colours it, or as the interface draws it: white, black on the light interface's
    /// rows.
    pub(crate) fn tint(&self, selected: bool) -> Color32 {
        let [each, in_use] = self.tints;
        match if selected { in_use } else { each } {
            Some(tint) => tint,
            None if crate::look::light() && !selected => Color32::BLACK,
            None => Color32::WHITE,
        }
    }

    /// The tool changed: the library shows its brush's set again.
    pub fn tool_changed(&mut self) {
        self.shown_set = None;
    }

    /// The library opens: on the current brush's set (or the Recent set, if that was shown), not
    /// on one tapped last time that the brush didn't come from.
    pub fn opens(&mut self) {
        if !self.recent {
            self.shown_set = None;
        }
    }

    /// A frame ends: the library shown last frame and not in this one has closed (hanging from its
    /// button or floating alike), and the core lets go of the canvas its sample strokes are drawn
    /// on. Its strokes stay, so it opens again at once.
    pub fn after_frame(&mut self, session: &CanvasSession) {
        let shown = std::mem::take(&mut self.shown_now);
        if self.shown_last && !shown {
            session.let_go_of_previews();
        }
        self.shown_last = shown;
    }

    /// A menu or question of the library's is up: a press elsewhere only closes it.
    pub fn modal(&self) -> bool {
        self.plus_menu.is_open()
            || self.set_menu.is_open()
            || self.recent_menu.is_open()
            || self.icon_picker.is_open()
            || self.brush_menu.is_open()
            || self.deleting.is_open()
            || self.title_menu.is_open()
            || self.libraries_plus.is_open()
            || self.card_menu.is_open()
    }

    /// Brush files read where they are on a thread of their own (a brush, set or library file a
    /// brush at a time, never held whole), `card` showing how far it has come (its Cancel stops
    /// it) unless an export has it; their brushes added to the library once read (`added`).
    /// `tell`: say that they came in (else only that they held none).
    pub fn add_brushes(
        &mut self,
        session: &CanvasSession,
        files: Vec<BrushFileIn>,
        tell: bool,
        card: &mut crate::export_card::ExportCard,
    ) {
        if files.is_empty() {
            return;
        }
        let (done, importing) = std::sync::mpsc::channel();
        // plugins' brush engines make theirs of the files there too, with what an import may take;
        // the card says so while they do, with the file's name and size, its pie how much of the
        // file they have read
        let engines = session.brush_engines();
        let reading = BrushFileReading::new();
        let on = Arc::new(std::sync::Mutex::new(String::new()));
        let (shown, named) = (Arc::clone(&reading), Arc::clone(&on));
        let progress = move || match shown.engine_size() {
            0 => shown.progress(),
            _ => shown.engine_progress(),
        };
        let shown = Arc::clone(&reading);
        let detail: crate::export_card::Detail = Box::new(move || {
            let size = shown.engine_size();
            let name = named.lock().map(|n| n.clone()).unwrap_or_default();
            (size > 0).then(|| ("Importing brushes", format!("{name} · {}", file_size(size))))
        });
        let exporting = (!card.busy()).then(|| card.start_importing(progress, detail));
        std::thread::spawn(move || {
            let (read, notices): (Vec<ReadBrushes>, Vec<ImportNotice>) = std::thread::scope(|scope| {
                let (files, reading) = (&files, &reading);
                let worker = scope.spawn(move || {
                    let (mut read, mut notices) = (Vec::new(), Vec::new());
                    for file in files {
                        if let Ok(mut name) = on.lock() {
                            name.clone_from(&file.name);
                        }
                        if let Some(brushes) = read_brushes_from(&file.path, Some(file.name.clone()), reading) {
                            read.push(engines.make_following(brushes, reading));
                            notices.extend(reading.notice().map(|text| (reading.notice_title(), text)));
                        }
                        if file.taken && !reading.cancelled() {
                            let _ = std::fs::remove_file(&file.path);
                        }
                    }
                    (read, notices)
                });
                // Cancel on the card stops the reading at its next brush
                while !worker.is_finished() {
                    if exporting.as_ref().is_some_and(crate::export_card::Exporting::cancelled) {
                        reading.cancel();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                worker.join().unwrap_or_default()
            });
            let cancelled = reading.cancelled();
            if let Some(exporting) = &exporting {
                exporting.ended(!cancelled && !read.is_empty());
            }
            let notices = if cancelled { vec![(None, procraft_core::IMPORT_CANCELLED.to_owned())] } else { notices };
            let _ = done.send(((!cancelled).then_some(read), notices));
        });
        self.importing.push((importing, tell));
    }

    /// A brush file picked from a "+" menu, waiting to be read, and whether to say that its
    /// brushes came in.
    pub fn take_picked_file(&mut self) -> Option<(std::path::PathBuf, String, bool)> {
        self.picked_file.take()
    }

    /// Brushes read meanwhile added to the library, each batch's set shown: whether any came in,
    /// and whether to say so. Not while the library holds brushes or sets by their places (a
    /// menu, a question, a set being renamed, a brush pressed, the studio asked for or open:
    /// `studio_open`), which brushes added at the top would move; they wait for that to end.
    pub fn added(&mut self, session: &CanvasSession, studio_open: bool) -> Vec<Added> {
        let holding = self.modal()
            || self.renaming_set.is_some()
            || self.renaming_brush.is_some()
            || self.press.is_some()
            || self.studio.is_some();
        if studio_open || holding {
            return Vec::new();
        }
        let mut came = Vec::new();
        let mut stopped = Vec::new();
        self.importing.retain(|(r, tell)| match r.try_recv() {
            Ok((Some(read), notices)) => {
                came.push((read, *tell, notices));
                false
            }
            // stopped with Cancel: nothing to add, only that
            Ok((None, notices)) => {
                stopped.push(Added { any: false, tell: false, notices });
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        let added =
            came.into_iter().map(|(read, tell, notices)| Added { any: self.add_read(session, read), tell, notices });
        added.chain(stopped).collect()
    }

    /// Brushes read from files added one after another, each into the set the library shows (or
    /// the current brush's) when it is one of ours from before, that set then shown. Whether any
    /// were.
    fn add_read(&mut self, session: &CanvasSession, read: Vec<ReadBrushes>) -> bool {
        let mut any = false;
        for brushes in read {
            let shown = self.shown_set.filter(|_| !self.recent).unwrap_or(session.current_brush().set as usize) as u32;
            if let Some(set) = session.add_read_brushes(brushes, shown) {
                self.shown_set = Some(set as usize);
                self.recent = false;
                any = true;
            }
        }
        if any {
            self.library_changed();
        }
        any
    }

    /// Brush files are still being read, or their brushes wait to be added.
    pub fn importing(&self) -> bool {
        !self.importing.is_empty()
    }

    /// Waits for brush files still being read and adds their brushes (the artwork is closing:
    /// they are kept with the rest).
    pub fn finish_importing(&mut self, session: &CanvasSession) {
        for (importing, _) in std::mem::take(&mut self.importing) {
            if let Ok((Some(read), _)) = importing.recv() {
                self.add_read(session, read);
            }
        }
    }

    /// Where the brushes dragged by `p` land let go at `at` (none: nowhere; see [`drop_over`]).
    fn drop_place(&self, p: Press, at: Pos2) -> Option<BrushDrop> {
        let over = drop_over(at, &self.set_rects, self.brush_list, &self.brush_rects)?;
        procraft_core::brush_drop_place(p.key.0, self.dragged_indices(p), over)
    }

    /// The places in their set of the brushes `p` drags (it, and those going with it).
    fn dragged_indices(&self, p: Press) -> Vec<u32> {
        if self.dragged_with.is_empty() {
            vec![p.key.1]
        } else {
            self.dragged_with.iter().filter(|k| k.0 == p.key.0).map(|k| k.1).collect()
        }
    }

    /// Whether the brushes dragged by `p` would be copied where the pointer is now (onto another
    /// set, or among the brushes of another set it opened), as the green plus on them says.
    fn drop_copies(&self, p: Press) -> bool {
        self.drop_place(p, p.last).is_some_and(|d| d.copies)
    }

    /// The library changed: its sample strokes are drawn again (they go by place), and it is kept.
    pub fn library_changed(&mut self) {
        self.stale = true;
        self.forget_cards();
        self.slid.clear();
        self.swiped = None;
        self.picked.clear();
        self.brush_menu.close();
        self.changed = true;
    }

    /// Shows the library hanging from `button`; returns where it is (points).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &Arc<CanvasSession>,
        hang: panel::Hang,
        backdrops: &mut Vec<f32>,
    ) -> Rect {
        let screen = panel::screen(ctx);
        let top = screen.top() + pc::BAR_HEIGHT;
        let (rect, arrow_x, area) = match hang {
            panel::Hang::Button(button) => {
                let (left, arrow_x) = panel::place(ctx, LIBRARY_WIDTH, button.center().x);
                let rect = Rect::from_min_size(
                    pos2(left, top),
                    vec2(LIBRARY_WIDTH, (screen.bottom() - BOTTOM_MARGIN - top).max(200.0)),
                );
                let rect = panel::hung_rect(ctx, rect, f32::INFINITY, BOTTOM_MARGIN);
                (rect, arrow_x, panel::area(ctx, egui::Id::unique("brush library"), rect))
            }
            panel::Hang::Floating(window) => (
                panel::floating_rect(window),
                0.0,
                panel::floating_area(ctx, egui::Id::unique("floating brush library"), window),
            ),
        };
        self.shown_now = true;
        // the cards' strokes forgotten last frame are drawn no more
        self.card_retired.clear();
        // nor the sample strokes out of sight past those kept (before any is drawn this frame)
        self.keep_previews_seen(PREVIEWS_KEPT);
        self.frames += 1;
        // tinted as the plugins say by the colour now, the strokes themselves drawn once
        self.tints = [false, true].map(|chosen| session.brush_preview_colour(chosen).map(opaque));
        if std::mem::take(&mut self.stale) {
            self.previews.clear();
            self.drawing = None;
        }
        self.wanted.clear();
        let library = session.brush_library();
        let libraries = session.brush_libraries();
        let place = libraries.iter().position(|l| l.current).unwrap_or(0);
        // every library's card, or the one in use with no sets yet, in place of the lists
        let cards = self.showing_libraries || library.is_empty();
        let current = session.current_brush();
        // with the Recent set shown, the current brush's set takes new brushes
        let shown = self
            .shown_set
            .filter(|_| !self.recent)
            .unwrap_or(current.set as usize)
            .min(library.len().saturating_sub(1));
        let recent = self.recent;
        let recents = if recent { session.recent_brushes() } else { Vec::new() };
        self.picked.retain(|p| !recent && p.0 as usize == shown);
        self.settle_slides(ctx);
        // the set a dragged brush or set is over
        let dragging = self.press.filter(|p| p.way == Way::Drag);
        let lifted = self.set_press.filter(|p| p.lifted);
        let taken = self.press.is_some_and(|p| matches!(p.way, Way::Swipe | Way::Held | Way::Drag));
        // a press on a brush holds the list where it was (stopping it if it was running on) until
        // it has gone up or down
        let held_at = self.press.filter(|p| p.way != Way::Scroll).map(|p| p.list);
        let over_set = dragging
            .map(|p| p.last)
            .or(lifted.map(|p| p.last))
            .and_then(|at| self.set_rects.iter().find(|(_, r)| r.contains(at)).map(|(i, _)| *i))
            .filter(|i| lifted.is_none_or(|p| p.set != *i));
        // a dragged brush held over another set a moment opens it, as a tap on it would: the
        // brush last chosen in it (its first, if none) becomes the one in use
        match (dragging, over_set) {
            (Some(_), Some(i)) if i != shown || recent => match self.spring {
                Some((j, since)) if j == i => {
                    if since.elapsed() >= SPRING_LOAD {
                        self.spring = None;
                        self.shown_set = Some(i);
                        self.recent = false;
                        if let Some(b) = session.brush_set_choice(i as u32) {
                            session.select_brush(b.set, b.index);
                            self.changed = true;
                        }
                    } else {
                        ctx.request_repaint_after(SPRING_LOAD.saturating_sub(since.elapsed()));
                    }
                }
                _ => {
                    self.spring = Some((i, Instant::now()));
                    ctx.request_repaint_after(SPRING_LOAD);
                }
            },
            _ => self.spring = None,
        }
        let mut set_rects = Vec::new();
        let mut brush_rects = Vec::new();
        let mut brush_list = None;
        let mut highlighted = None;
        let mut plus = Rect::NOTHING;
        let mut lists = Rect::NOTHING;
        let mut set_tapped = None;
        // a brush tapped among the search's results
        let mut found_tapped = None;
        let mut recent_row = None;
        let mut recent_tapped = false;
        let mut brush_tapped = None;
        let mut swipe_chose = None;
        let mut title_row = Rect::NOTHING;
        area.show(ctx, |ui| {
            // nothing under the panel takes a touch
            match hang {
                panel::Hang::Button(_) => {
                    ui.allocate_rect(rect, Sense::click());
                    panel::frame(ui, look, "library frame", rect, arrow_x, backdrops);
                    let body = panel::body_in(ui.ctx(), rect);
                    self.undock = if self.floats { panel::undock_grabber(ui, body) } else { None };
                }
                panel::Hang::Floating(window) => {
                    ui.allocate_rect(window, Sense::click());
                    panel::floating_panel_frame(ui, look, "floating library frame", window, backdrops);
                    self.chrome = panel::floating_chrome(ui, window);
                }
            }
            let body = panel::body_in(ui.ctx(), rect);
            // the library in use's name (typed over while it is renamed), or every library's
            let name = libraries.get(place).map_or("Brush library", |l| l.name.as_str());
            let renaming = !self.showing_libraries && self.renaming_library.as_ref().is_some_and(|r| r.0 == place);
            let heading = match (self.showing_libraries, renaming) {
                (true, _) => "Brush libraries",
                (false, true) => "",
                (false, false) => name,
            };
            let (tapped, at) = panel::header_title(ui, look, body, heading, 20.0, true, self.title_menu.is_open());
            title_row = at;
            if tapped {
                self.title_menu.open();
            }
            if renaming {
                let field = Rect::from_min_size(pos2(body.left() + 20.0, body.top() + 17.0), vec2(300.0, 30.0));
                self.rename_field(ui, session, place, field, 24.0);
            }
            // the plus: new brushes and sets, and importing; over the cards, a new library
            plus = Rect::from_min_size(pos2(body.right() - 12.0 - 41.0, body.top() + 12.0), vec2(41.0, 40.0));
            if self.picked.is_empty() || self.showing_libraries {
                look.icon(ui, &icons::PLUS_MEDIUM, plus.center(), 41.0, argb(PANEL_GLYPH));
                if ui.interact(plus, ui.scope_id().with("plus"), Sense::click()).clicked() {
                    if self.showing_libraries {
                        self.libraries_plus.open();
                    } else {
                        self.plus_menu.open();
                    }
                }
            } else {
                self.picked_actions(ui, session, plus);
            }

            lists = Rect::from_min_max(pos2(body.left(), body.top() + HEADER_HEIGHT), body.max);
            if cards {
                self.libraries_body(ui, look, session, &libraries, lists, shown);
                return;
            }
            // the search: out of sight at first, it comes down as a list is scrolled up past
            // its top, and goes as they scroll down again (unless it is in use)
            let in_use = self.searching || !self.query.is_empty();
            // (by the wheel, or a finger or the mouse pulling a list)
            let pulled = self.press.is_some_and(|p| p.way == Way::Scroll) || self.set_press.is_some_and(|p| p.spent);
            let pull = panel::in_panel_by(ctx, ui.input(|i| i.pointer.delta())).y;
            let wheel = ui.input(|i| i.smooth_scroll_delta.y) + if pulled { pull } else { 0.0 };
            let hover = ui.input(|i| i.pointer.hover_pos()).map(|p| panel::in_panel(ctx, p));
            if !in_use && let Some(p) = hover.filter(|p| lists.contains(*p)) {
                let at_top = if p.x < lists.left() + SETS_WIDTH { self.lists_top.0 } else { self.lists_top.1 };
                if wheel > 0.0 && at_top {
                    self.search_down = true;
                } else if wheel < 0.0 {
                    self.search_down = false;
                }
            }
            let shown_down = in_use || self.search_down;
            let down =
                ui.ctx().animate_bool_with_time(egui::Id::unique("library search"), shown_down, 0.2) * SEARCH_BLOCK;
            if down > 0.0 {
                let block = Rect::from_min_size(
                    pos2(lists.left(), lists.top() + down - SEARCH_BLOCK),
                    vec2(lists.width(), SEARCH_BLOCK),
                );
                self.search_field(ui, block, lists);
            }
            lists.min.y += down;
            if in_use {
                if let Some(key) = self.search_results(ui, &library, (current.set, current.index), lists) {
                    found_tapped = Some(key);
                }
            } else {
                self.result_rects.clear();
                let sets = Rect::from_min_size(lists.min, vec2(SETS_WIDTH, lists.height()));
                let brushes = Rect::from_min_max(pos2(lists.left() + SETS_WIDTH, lists.top()), lists.max);
                if !recent && let (Ok(set), Some(held)) = (u32::try_from(shown), library.get(shown)) {
                    brush_list = Some((set, u32::try_from(held.brushes.len()).unwrap_or(u32::MAX), brushes));
                }
                if SET_COLUMN.value() >> 24 != 0 {
                    ui.painter().rect_filled(sets, 0, argb(SET_COLUMN));
                }
                let corner = panel::POPOVER_CORNER;
                panel::rounded(ui, look, "library brushes", brushes, [0.0, 0.0, corner, 0.0], argb(BRUSH_COLUMN));

                let sets_out = ui.scope_builder(UiBuilder::new().max_rect(sets), |ui| {
                    ScrollArea::vertical()
                        .id_salt("library sets")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        // a lifted set is dragged, not the list
                        .scroll_source(if lifted.is_some() {
                            egui::scroll_area::ScrollSource::NONE
                        } else {
                            egui::scroll_area::ScrollSource::default()
                        })
                        .show(ui, |ui| {
                            ui.add_space(12.0);
                            // the brushes last painted with, and those pinned there, above the sets
                            let (response, at) = self.recent_row(ui, look, recent);
                            recent_row = Some(at);
                            recent_tapped = response.clicked();
                            ui.add_space(12.0);
                            for (i, set) in library.iter().enumerate() {
                                let fill = if over_set == Some(i) { Some(argb(pc::ACCENT)) } else { None };
                                // a lifted set is faint where it was
                                let faint = lifted.is_some_and(|p| p.set == i);
                                let (response, at) = ui
                                    .scope(|ui| {
                                        if faint {
                                            ui.multiply_opacity(0.3);
                                        }
                                        self.set_row(ui, look, session, i, set, i == shown && !recent, fill)
                                    })
                                    .inner;
                                set_rects.push((i, at));
                                // a brush found (from a search, or the Recent set) shows its set
                                if self.found.is_some_and(|f| f.0 as usize == i) {
                                    ui.scroll_to_rect(at, None);
                                }
                                if self.set_press.is_none()
                                    && self.press.is_none()
                                    && response.is_pointer_button_down_on()
                                    && let Some(p) = response.interact_pointer_pos()
                                {
                                    self.set_press = Some(SetPress {
                                        set: i,
                                        origin: p,
                                        since: Instant::now(),
                                        grab: p - at.min,
                                        last: p,
                                        lifted: false,
                                        spent: false,
                                    });
                                }
                                if response.clicked() {
                                    set_tapped = Some(i);
                                }
                                ui.add_space(12.0);
                            }
                        })
                });
                let brushes_out = ui.scope_builder(UiBuilder::new().max_rect(brushes), |ui| {
                    let list = ScrollArea::vertical()
                        .id_salt(("library brushes", if recent { usize::MAX } else { shown }))
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        // the list follows a press itself, and carries on when flicked; a brush
                        // slid or dragged keeps it still
                        .scroll_source(if taken {
                            egui::scroll_area::ScrollSource::NONE
                        } else {
                            egui::scroll_area::ScrollSource::ALL
                        });
                    let list = match held_at {
                        Some(at) => list.vertical_scroll_offset(at),
                        None => list,
                    };
                    list.show(ui, |ui| {
                        ui.add_space(9.75);
                        // the Recent set's brushes (pinned there or not), or the shown set's
                        let listed: Vec<(&BrushInfo, Option<bool>)> = if recent {
                            recents
                                .iter()
                                .filter_map(|r| {
                                    let b = library.get(r.set as usize)?.brushes.get(r.index as usize)?;
                                    Some((b, Some(r.pinned)))
                                })
                                .collect()
                        } else {
                            library
                                .get(shown)
                                .map(|set| set.brushes.iter().map(|b| (b, None)).collect())
                                .unwrap_or_default()
                        };
                        for (b, pinned) in listed {
                            // the brush in use, alone: a set opened brings back its own last
                            // choice as the brush, so another set's choice never shows chosen
                            let selected = (b.set, b.index) == (current.set, current.index);
                            if selected {
                                highlighted = Some((b.set, b.index));
                            }
                            let (response, cell) = self.brush_row(ui, look, session, b, selected, pinned);
                            // a brush found from the Recent set is scrolled to in its own
                            if self.found == Some((b.set, b.index)) {
                                self.found = None;
                                ui.scroll_to_rect(cell, Some(egui::Align::Center));
                            }
                            brush_rects.push(((b.set, b.index), cell));
                            if let Some(r) = response {
                                if r.0 {
                                    brush_tapped = Some(((b.set, b.index), selected));
                                }
                                if let Some(chose) = r.1 {
                                    swipe_chose = Some(((b.set, b.index), b.name.clone(), chose));
                                }
                            }
                            ui.add_space(BRUSH_GAP);
                        }
                        ui.add_space(12.0 - BRUSH_GAP);
                    })
                });
                self.list_at = brushes_out.inner.state.clamped_offset().y;
                self.lists_top =
                    (sets_out.inner.state.clamped_offset().y <= 0.5, brushes_out.inner.state.clamped_offset().y <= 0.5);
            }
            // the dragged brush follows the pointer, over the lists
            if let Some(p) = dragging
                && let Some(b) = library.iter().flat_map(|s| &s.brushes).find(|b| (b.set, b.index) == p.key)
            {
                let at = Rect::from_min_size(p.last - p.grab, self.dragged_size);
                let painter = ui.painter_at(lists);
                painter.rect_filled(at, 12, argb(pc::ACCENT).gamma_multiply(0.9));
                self.preview_in(&painter, (b.set, b.index), at, true);
                let name = one_line(ui, &b.name, 14.0, Weight::SemiBold, Color32::WHITE, at.width() - 28.0);
                painter.galley(at.min + vec2(18.0, 11.0), name, Color32::WHITE);
                // several going together: how many
                if self.dragged_with.len() > 1 {
                    let badge = pos2(at.right() - 20.0, at.top() + 20.0);
                    painter.circle_filled(badge, 12.0, Color32::WHITE);
                    let count = self.dragged_with.len().to_string();
                    text(&painter, badge, Align2::CENTER_CENTER, &count, 13.0, Weight::SemiBold, argb(pc::ACCENT));
                }
                // over another set they would be copied into: a green plus on its corner
                if self.drop_copies(p) {
                    let lands = ui.painter_at(lists.expand(12.0));
                    lands.circle_filled(at.right_top(), 12.0, argb(crate::held_layers::LANDS));
                    text(&lands, at.right_top(), Align2::CENTER_CENTER, "+", 15.0, Weight::Medium, Color32::WHITE);
                }
                ctx.request_repaint();
            }
            // and the lifted set
            if let Some(p) = lifted
                && let Some(set) = library.get(p.set)
            {
                let size = self
                    .set_rects
                    .iter()
                    .find(|(s, _)| *s == p.set)
                    .map_or(vec2(SETS_WIDTH - 24.0, SET_HEIGHT), |(_, r)| r.size());
                let at = Rect::from_min_size(p.last - p.grab, size);
                let painter = ui.painter_at(lists);
                painter.rect_filled(at, 12, argb(pc::ACCENT).gamma_multiply(0.9));
                let icon = icons::SET_ICONS.get(set.icon as usize).unwrap_or(&icons::BRUSH_SET);
                look.icon(ui, icon, pos2(at.left() + 24.0, at.center().y), 40.0, Color32::WHITE);
                let left = at.left() + 4.0 + 40.0 + 1.0;
                let name = one_line(ui, &set.name, 14.0, Weight::SemiBold, Color32::WHITE, at.right() - 8.0 - left);
                painter.galley(pos2(left, at.center().y - name.size().y / 2.0), name, Color32::WHITE);
                ctx.request_repaint();
            }
        });
        self.set_rects = set_rects;
        self.brush_rects = brush_rects;
        self.brush_list = brush_list;
        self.highlighted = highlighted;
        self.draw_previews(ctx, session);
        self.follow_press(ctx, session);
        // a set let go after it was lifted was dragged, not tapped
        if self.follow_set_press(ctx, session, shown) {
            set_tapped = None;
        }

        if let Some(i) = set_tapped {
            if i == shown && !recent {
                self.set_menu_for = Some(i);
                self.set_menu.open();
            } else {
                self.shown_set = Some(i);
                self.recent = false;
                // the brush last chosen in it becomes the one in use, as it shows highlighted
                if let Some(b) = session.brush_set_choice(i as u32) {
                    session.select_brush(b.set, b.index);
                    self.changed = true;
                }
            }
        }
        if recent_tapped {
            if recent {
                self.recent_menu.open();
            } else {
                self.recent = true;
                self.swiped = None;
            }
        }
        if let Some((key, selected)) = brush_tapped {
            // with a brush slid open, a tap only closes it; with brushes picked, only lets them go
            if self.swiped.is_some() {
                self.swiped = None;
            } else if !self.picked.is_empty() {
                self.picked.clear();
            } else {
                session.select_brush(key.0, key.1);
                self.changed = true;
                // the brush in use opens in the studio; another becomes the brush
                if selected {
                    self.studio = Some(None);
                }
            }
        }
        // picked from the search, a brush ends it and shows in its own set, chosen there and
        // scrolled to (as the original)
        if let Some(key) = found_tapped {
            self.query.clear();
            self.searching = false;
            self.search_down = false;
            ctx.memory_mut(|m| m.stop_text_input());
            self.recent = false;
            self.shown_set = Some(key.0 as usize);
            self.found = Some(key);
            session.select_brush(key.0, key.1);
            self.changed = true;
        }
        if let Some((key, name, chose)) = swipe_chose {
            self.swiped = None;
            match chose {
                Swiped::Share => {
                    if let Some(bytes) = session.brush_as_file(key.0, key.1) {
                        crate::files::save_file(&bytes, &name, "brush", "Brush");
                    }
                }
                Swiped::Duplicate => {
                    if session.duplicate_brush(key.0, key.1).is_some() {
                        self.library_changed();
                    }
                }
                Swiped::Delete => self.ask_delete(Deleting::Brush(key.0, key.1)),
                // shown in its own set, chosen there
                Swiped::Find => {
                    self.recent = false;
                    self.shown_set = Some(key.0 as usize);
                    self.found = Some(key);
                    session.select_brush(key.0, key.1);
                    self.changed = true;
                }
                Swiped::Pin | Swiped::Unpin => {
                    self.changed |= session.pin_recent_brush(key.0, key.1, matches!(chose, Swiped::Pin));
                }
                Swiped::Clear => self.changed |= session.clear_recent_brush(key.0, key.1),
            }
        }
        self.plus_menu(ctx, look, session, plus, shown, backdrops);
        self.set_menu(ctx, look, session, &library, backdrops);
        self.title_menu(ctx, look, session, title_row, &libraries, backdrops);
        if let Some(row) = recent_row {
            self.recent_menu(ctx, look, session, row, backdrops);
        }
        self.brush_menu(ctx, look, session, backdrops);
        self.libraries_plus_menu(ctx, look, session, plus, backdrops);
        self.card_menu(ctx, look, session, &libraries, backdrops);
        self.follow_pinch(ctx, session, rect, &libraries);
        if self.deleting.show(ctx, look) == Some(Answer::Action) {
            match self.delete_what.take() {
                Some(Deleting::Set(i)) => {
                    if session.delete_brush_set(i) {
                        let at = i as usize;
                        self.shown_set = Some(if shown >= at { shown.saturating_sub(1) } else { shown });
                        self.library_changed();
                    }
                }
                Some(Deleting::Brush(set, index)) if session.delete_brush(set, index) => self.library_changed(),
                Some(Deleting::Picked) => {
                    // from the last up, so the places of the rest hold
                    self.picked.sort_unstable();
                    for (set, index) in self.picked.iter().rev() {
                        session.delete_brush(*set, *index);
                    }
                    self.library_changed();
                }
                Some(Deleting::Library(i)) if session.delete_brush_library(i) => {
                    // the one in use may be another now
                    self.shown_set = None;
                    self.library_changed();
                }
                Some(Deleting::Brush(..) | Deleting::Library(_)) | None => {}
            }
        }
        match hang {
            panel::Hang::Button(_) => rect,
            panel::Hang::Floating(window) => window,
        }
    }

    /// In the place of the `plus`: Delete for the picked brushes, and Combine beside it
    /// for one in the current brush's set, it taking the picked brush as its second.
    fn picked_actions(&mut self, ui: &Ui, session: &CanvasSession, plus: Rect) {
        let (right, top) = (plus.right(), plus.top() + 4.0);
        let word = |ui: &Ui, text: &str, at: Rect, salt: &str| {
            let galley = one_line(ui, text, 15.0, Weight::Regular, argb(SWIPE_INK), at.width());
            let middle = at.center() - galley.size() / 2.0;
            ui.painter().galley(middle, galley, argb(SWIPE_INK));
            ui.interact(at, ui.scope_id().with(salt), Sense::click()).clicked()
        };
        let delete = Rect::from_min_size(pos2(right - 63.0, top), vec2(63.0, 32.0));
        if word(ui, "Delete", delete, "delete picked") {
            self.ask_delete(Deleting::Picked);
        }
        // two picked (the current brush counts): Combine, which a combined one among them refuses
        let current = session.current_brush();
        let pair = match self.picked[..] {
            [(set, index)] if set == current.set => Some(index),
            _ => None,
        };
        if let Some(second) = pair {
            let combine = Rect::from_min_size(pos2(delete.left() - 4.0 - 80.0, top), vec2(80.0, 32.0));
            if word(ui, "Combine", combine, "combine picked") {
                if let Some(refusal) = session.combine_refusal(current.set, current.index, second) {
                    self.notice = Some(refusal);
                } else if session.combine_brushes(current.set, current.index, second) {
                    self.library_changed();
                }
            }
        }
    }

    fn ask_delete(&mut self, what: Deleting) {
        let title = match what {
            Deleting::Set(_) => "Delete this set and its brushes?",
            Deleting::Brush(..) => "Delete this brush?",
            Deleting::Picked => "Delete these brushes?",
            Deleting::Library(_) => "Delete this library?",
        };
        self.delete_what = Some(what);
        self.deleting.open(title, "It can't be brought back.", "Delete", Some(&icons::TRASH));
    }

    /// The plus's menu, hanging below it, their right edges in line: a new brush, a new set, or
    /// brushes from a file.
    #[allow(clippy::too_many_arguments)]
    fn plus_menu(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &Arc<CanvasSession>,
        plus: Rect,
        shown: usize,
        backdrops: &mut Vec<f32>,
    ) {
        let ink = menu::INK;
        // the Recent set holds no brushes of its own to add one to
        let mut entries = vec![
            Entry::Item(&icons::MENU_NEW_BRUSH, "Create new brush", ink),
            Entry::Item(&icons::MENU_NEW_SET, "Create new set", ink),
            Entry::Rule(11.0),
            Entry::Item(&icons::MENU_FOLDER, "Import from Files", ink),
        ];
        if self.recent {
            entries.remove(0);
        }
        let size = vec2(menu::LIBRARY_WIDTH, menu::entries_height(&entries));
        let rect = Rect::from_min_size(pos2(plus.right() - size.x, plus.bottom()), size);
        let mut tapped = None;
        self.plus_menu.show(ctx, "library plus menu", rect, rect.right_top(), menu::OVER_A_PANEL, backdrops, |ui| {
            tapped = menu::entries(ui, look, rect, &entries);
        });
        let Some(Entry::Item(_, label, _)) = tapped.and_then(|t| entries.get(t)) else { return };
        self.plus_menu.close();
        match *label {
            // a new brush, opened in the studio
            "Create new brush" => {
                if let Some(index) = session.new_brush(shown as u32) {
                    session.select_brush(shown as u32, index);
                    self.library_changed();
                    self.studio = Some(Some((shown as u32, index)));
                }
            }
            "Create new set" => {
                let set = session.new_brush_set() as usize;
                self.shown_set = Some(set);
                self.recent = false;
                let name = session.brush_library().get(set).map(|s| s.name.clone()).unwrap_or_default();
                self.renaming_set = Some((set, name, false));
                self.library_changed();
            }
            _ => {
                let Some((path, name)) = crate::files::pick_path("Import brushes") else { return };
                self.shown_set = Some(shown);
                self.picked_file = Some((path, name, false));
            }
        }
    }

    /// The shown set's menu, left of it and centred on it: Rename, Duplicate, Customize icon (its
    /// icons to choose from beside it), Share, Delete.
    fn set_menu(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        library: &[BrushSetInfo],
        backdrops: &mut Vec<f32>,
    ) {
        let Some(i) = self.set_menu_for else { return };
        let Some(set_rect) = self.set_rects.iter().find(|(s, _)| *s == i).map(|(_, r)| *r) else { return };
        let ink = menu::INK;
        let own = library.get(i).map_or(0, |s| s.icon as usize);
        let set_icon = icons::SET_ICONS.get(own).unwrap_or(&icons::BRUSH_SET);
        let entries = [
            Entry::Item(&icons::MENU_RENAME, "Rename", ink),
            Entry::Item(&icons::MENU_DUPLICATE, "Duplicate", ink),
            Entry::More(set_icon, "Customize icon", ink),
            menu::DIVIDER,
            Entry::Item(&icons::MENU_SHARE, "Share", ink),
            // the only set too: the library is then left with none
            menu::DIVIDER,
            Entry::Item(&icons::MENU_TRASH, "Delete", menu::DANGER),
        ];
        let size = vec2(menu::LIBRARY_WIDTH, menu::entries_height(&entries));
        let screen = panel::screen(ctx);
        let top = (set_rect.center().y - size.y / 2.0)
            .clamp(screen.top() + MENU_TOP, (screen.bottom() - size.y).max(screen.top() + MENU_TOP));
        let rect = Rect::from_min_size(pos2(set_rect.left() - SET_MENU_GAP - size.x, top), size);
        let mut tapped = None;
        self.set_menu.show(ctx, "library set menu", rect, rect.right_center(), SET_MENU_GLASS, backdrops, |ui| {
            tapped = menu::entries(ui, look, rect, &entries);
        });
        if !self.set_menu.is_open() {
            self.icon_picker.close();
        }
        // the icons to choose from: under the Customize icon row, or over it with no room below
        let row_top = rect.top() + menu::entry_top(&entries, 2);
        let (row_top, row_bottom) = (row_top, row_top + menu::ITEM_HEIGHT);
        let below = row_bottom + PICKER.1 <= screen.bottom() - PICKER_MARGIN;
        let picker = Rect::from_min_size(
            pos2(set_rect.left() - PICKER_INSET, if below { row_bottom } else { row_top - PICKER.1 }),
            vec2(PICKER.0, PICKER.1),
        );
        let mut picked = None;
        let scrolled = self.picker_scrolled;
        self.icon_picker.show(ctx, "set icon picker", picker, picker.min, PICKER_GLASS, backdrops, |ui| {
            picked = icon_grid(ui, look, picker, own, scrolled);
        });
        self.picker_scrolled = self.icon_picker.is_open();
        if let Some(icon) = picked {
            self.icon_picker.close();
            self.set_menu.close();
            if session.set_brush_set_icon(i as u32, icon as u32) {
                self.library_changed();
            }
            return;
        }
        let Some(Entry::Item(_, label, _) | Entry::More(_, label, _)) = tapped.and_then(|t| entries.get(t)) else {
            return;
        };
        if *label == "Customize icon" {
            if self.icon_picker.is_open() {
                self.icon_picker.close();
            } else {
                self.icon_picker.open();
            }
            return;
        }
        self.set_menu.close();
        match *label {
            "Rename" => {
                let name = library.get(i).map(|s| s.name.clone()).unwrap_or_default();
                self.renaming_set = Some((i, name, false));
            }
            "Duplicate" => {
                if let Some(copy) = session.duplicate_brush_set(i as u32) {
                    self.shown_set = Some(copy as usize);
                    self.library_changed();
                }
            }
            "Share" => {
                let name = library.get(i).map_or("Brushes", |s| s.name.as_str());
                if let Some(bytes) = session.brush_set_as_file(i as u32) {
                    crate::files::save_file(&bytes, name, "brushset", "Brush set");
                }
            }
            _ => self.ask_delete(Deleting::Set(i as u32)),
        }
    }

    /// A held brush's menu, under it, their left edges in line: Rename, Duplicate, Share and
    /// Delete; in the Recent set Find, Pin or Unpin, Share and Clear.
    fn brush_menu(&mut self, ctx: &egui::Context, look: &mut Look, session: &CanvasSession, backdrops: &mut Vec<f32>) {
        let Some((key, pinned)) = self.brush_menu_for else { return };
        let Some(cell) = self.brush_rects.iter().find(|(k, _)| *k == key).map(|(_, r)| *r) else {
            self.brush_menu.close();
            return;
        };
        let ink = menu::INK;
        let mut entries = match pinned {
            Some(pinned) => vec![
                Entry::Item(&icons::SWIPE_FIND, "Find", ink),
                if pinned {
                    Entry::Item(&icons::SWIPE_UNPIN, "Unpin", ink)
                } else {
                    Entry::Item(&icons::SWIPE_PIN, "Pin", ink)
                },
            ],
            None => {
                vec![
                    Entry::Item(&icons::MENU_RENAME, "Rename", ink),
                    Entry::Item(&icons::MENU_DUPLICATE, "Duplicate", ink),
                ]
            }
        };
        entries.extend([menu::DIVIDER, Entry::Item(&icons::MENU_SHARE, "Share", ink), menu::DIVIDER]);
        entries.push(match pinned {
            Some(_) => Entry::Item(&icons::MENU_CLEAR, "Clear", ink),
            None => Entry::Item(&icons::MENU_TRASH, "Delete", menu::DANGER),
        });
        let size = vec2(menu::LIBRARY_WIDTH, menu::entries_height(&entries));
        let screen = panel::screen(ctx);
        let top = cell.bottom().clamp(screen.top() + MENU_TOP, (screen.bottom() - size.y).max(screen.top() + MENU_TOP));
        let rect = Rect::from_min_size(pos2(cell.left(), top), size);
        let mut tapped = None;
        self.brush_menu.show(ctx, "library brush menu", rect, rect.left_top(), menu::OVER_A_PANEL, backdrops, |ui| {
            tapped = menu::entries(ui, look, rect, &entries);
        });
        let Some(Entry::Item(_, label, _)) = tapped.and_then(|t| entries.get(t)) else { return };
        self.brush_menu.close();
        let name = session
            .brush_library()
            .get(key.0 as usize)
            .and_then(|s| s.brushes.get(key.1 as usize))
            .map(|b| b.name.clone());
        let Some(name) = name else { return };
        match *label {
            "Rename" => self.renaming_brush = Some((key, name, false)),
            "Duplicate" => {
                if session.duplicate_brush(key.0, key.1).is_some() {
                    self.library_changed();
                }
            }
            "Share" => {
                if let Some(bytes) = session.brush_as_file(key.0, key.1) {
                    crate::files::save_file(&bytes, &name, "brush", "Brush");
                }
            }
            "Delete" => self.ask_delete(Deleting::Brush(key.0, key.1)),
            // shown in its own set, chosen there
            "Find" => {
                self.recent = false;
                self.shown_set = Some(key.0 as usize);
                self.found = Some(key);
                session.select_brush(key.0, key.1);
                self.changed = true;
            }
            "Pin" | "Unpin" => self.changed |= session.pin_recent_brush(key.0, key.1, *label == "Pin"),
            _ => self.changed |= session.clear_recent_brush(key.0, key.1),
        }
    }

    /// The Recent set's menu, left of its row and centred on it as a set's: Clear alone.
    fn recent_menu(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        row: Rect,
        backdrops: &mut Vec<f32>,
    ) {
        let entries = [Entry::Item(&icons::MENU_CLEAR, "Clear", menu::INK)];
        let size = vec2(menu::LIBRARY_WIDTH, menu::entries_height(&entries) + RECENT_MENU_EXTRA);
        let screen = panel::screen(ctx);
        let top = (row.center().y - size.y / 2.0)
            .clamp(screen.top() + MENU_TOP, (screen.bottom() - size.y).max(screen.top() + MENU_TOP));
        let rect = Rect::from_min_size(pos2(row.left() - SET_MENU_GAP - size.x, top), size);
        let mut tapped = None;
        self.recent_menu.show(ctx, "library recent menu", rect, rect.right_center(), SET_MENU_GLASS, backdrops, |ui| {
            tapped = menu::entries(ui, look, rect.shrink2(vec2(0.0, RECENT_MENU_EXTRA / 2.0)), &entries);
        });
        if tapped.is_some() {
            self.recent_menu.close();
            session.clear_recent_brushes();
            self.changed = true;
        }
    }

    /// The brushes not being slid go back, or open to their buttons, as they were left.
    fn settle_slides(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let widths: HashMap<(u32, u32), f32> = self.brush_rects.iter().map(|(k, r)| (*k, r.width())).collect();
        let held = self.press.filter(|p| p.way == Way::Swipe).map(|p| p.key);
        let mut keys: Vec<(u32, u32)> = self.slid.keys().copied().collect();
        if let Some(open) = self.swiped
            && !keys.contains(&open)
        {
            keys.push(open);
        }
        for key in keys {
            if held == Some(key) {
                continue;
            }
            let to = if self.swiped == Some(key) { -widths.get(&key).copied().unwrap_or(0.0) } else { 0.0 };
            let v = self.slid.entry(key).or_insert(0.0);
            *v += (to - *v) * (dt * 14.0).min(1.0);
            if (to - *v).abs() < 0.5 {
                *v = to;
            } else {
                ctx.request_repaint();
            }
            if *v == 0.0 {
                self.slid.remove(&key);
            }
        }
    }

    /// Follows the press on a brush: moved sideways it slides the brush (let go past a third of
    /// the way, it stays open); moved up or down, the list scrolls; held still, its menu shows, and
    /// moved straight on from there the brush is dragged and, let go, goes to the top of the set it
    /// is over, or to the place of the brush it is over (moved on later, the list scrolls under the
    /// menu, which stays).
    fn follow_press(&mut self, ctx: &egui::Context, session: &CanvasSession) {
        let Some(mut p) = self.press else { return };
        let (at, down) = ctx.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
        let at = at.map_or(p.last, |at| panel::in_panel(ctx, at));
        let d = at - p.origin;
        let width = self.brush_rects.iter().find(|(k, _)| *k == p.key).map_or(0.0, |(_, r)| r.width());
        if down {
            match p.way {
                Way::Undecided => {
                    if let Some(way) = way_moved(d) {
                        p.way = way;
                    } else if d.length() > SLOP {
                        // going somewhere, not yet plain which way: it is no hold
                    } else if p.since.elapsed() >= brush_hold() {
                        // held still: its menu shows
                        p.way = Way::Held;
                        p.menu_at = Some(Instant::now());
                        self.swiped = None;
                        self.open_brush_menu(session, p.key);
                    } else {
                        ctx.request_repaint_after(brush_hold().saturating_sub(p.since.elapsed()));
                    }
                }
                // moved straight on from there, the menu goes and the brush is dragged (the Recent
                // set's brushes stay where they are); moved on later, the list has the press and
                // scrolls, the menu staying
                Way::Held => {
                    let since_menu = p.menu_at.map_or(0.0, |shown| shown.elapsed().as_secs_f64());
                    let what = brush_hold_move(d.length(), SLOP, since_menu, !self.recent);
                    if what == BrushHoldMove::Leaves {
                        p.way = Way::Scroll;
                    } else if what == BrushHoldMove::Drags {
                        p.way = Way::Drag;
                        self.brush_menu.close();
                        let current = session.current_brush();
                        let selected = (current.set, current.index) == p.key;
                        self.dragged_with = dragged_together(p.key, &self.picked, selected);
                        self.dragged_size = self
                            .brush_rects
                            .iter()
                            .find(|(k, _)| *k == p.key)
                            .map_or(vec2(240.0, BRUSH_HEIGHT), |(_, r)| r.size());
                        ctx.request_repaint();
                    }
                }
                Way::Swipe => {
                    // the current brush counts as picked already
                    let current = session.current_brush();
                    let closed = self.swiped != Some(p.key) && !self.recent && p.key != (current.set, current.index);
                    let most = if closed { PICK_REACH } else { 0.0 };
                    self.slid.insert(p.key, (p.start + d.x).clamp(-width, most));
                    ctx.request_repaint();
                }
                Way::Scroll => {}
                Way::Drag => ctx.request_repaint(),
            }
            p.last = at;
            self.press = Some(p);
            return;
        }
        self.press = None;
        match p.way {
            Way::Swipe => {
                let slide = self.slid.get(&p.key).copied().unwrap_or(0.0);
                if slide > PICK_REACH / 2.0 {
                    if let Some(at) = self.picked.iter().position(|k| *k == p.key) {
                        self.picked.remove(at);
                    } else {
                        self.picked.push(p.key);
                    }
                }
                let open = self.swiped == Some(p.key);
                let opened = slide < -width / 3.0;
                if opened != open {
                    self.swiped = opened.then_some(p.key);
                }
            }
            Way::Drag => {
                // onto another set: copied to its top; onto a brush of the set shown not among
                // them: above it or below it as the pointer is on its upper or lower half (the
                // first of them there, the rest after it), copied there from another set, moved
                // along their own; into a list away from its brushes, after the last
                let over = drop_over(at, &self.set_rects, self.brush_list, &self.brush_rects);
                let indices = self.dragged_indices(p);
                self.dragged_with.clear();
                if over.is_some_and(|over| session.drop_brushes_over(p.key.0, indices, over)) {
                    self.library_changed();
                }
            }
            Way::Undecided | Way::Held | Way::Scroll => {}
        }
    }

    /// Two fingers pinching on the library at `rect` (a touch screen's, or a trackpad's under the
    /// pointer): let go pinched in to [`PINCH_TO_LIBRARIES`], every library's card shows; pinched
    /// out past [`PINCH_TO_LIBRARY`] over a card, that library opens. Not while a menu is up.
    fn follow_pinch(
        &mut self,
        ctx: &egui::Context,
        session: &CanvasSession,
        rect: Rect,
        libraries: &[procraft_core::BrushLibraryInfo],
    ) {
        let (touch, trackpad, pointer) = ctx.input(|i| {
            let zooms: Vec<f32> =
                i.events.iter().filter_map(|e| if let egui::Event::Zoom(z) = e { Some(*z) } else { None }).collect();
            let trackpad = (!zooms.is_empty()).then(|| zooms.iter().product::<f32>());
            (i.multi_touch(), trackpad, i.pointer.hover_pos())
        });
        let now = Instant::now();
        let modal = self.modal() || self.press.is_some() || self.set_press.is_some();
        match (touch, trackpad) {
            (Some(t), _) => {
                let at = panel::in_panel(ctx, t.center_pos);
                match self.pinch.as_mut().filter(|p| p.touch) {
                    Some(p) => {
                        p.zoom *= t.zoom_delta;
                        p.at = at;
                        p.last = now;
                    }
                    None if !modal && rect.contains(panel::in_panel(ctx, t.start_pos)) => {
                        self.pinch = Some(Pinch { zoom: t.zoom_delta, at, last: now, touch: true });
                    }
                    None => {}
                }
                return;
            }
            (None, Some(z)) => {
                let Some(at) = pointer.map(|p| panel::in_panel(ctx, p)) else { return };
                match self.pinch.as_mut().filter(|p| !p.touch) {
                    Some(p) => {
                        p.zoom *= z;
                        p.last = now;
                    }
                    None if !modal && rect.contains(at) => {
                        self.pinch = Some(Pinch { zoom: z, at, last: now, touch: false });
                    }
                    None => {}
                }
                ctx.request_repaint_after(PINCH_QUIET);
                return;
            }
            (None, None) => {}
        }
        // the fingers lifted (a trackpad's pinch still for a while): where it ended up
        let Some(p) = self.pinch else { return };
        if !p.touch && p.last.elapsed() < PINCH_QUIET {
            ctx.request_repaint_after(PINCH_QUIET.saturating_sub(p.last.elapsed()));
            return;
        }
        self.pinch = None;
        if self.showing_libraries {
            if p.zoom > PINCH_TO_LIBRARY
                && let Some(&(i, _)) = self.card_rects.iter().find(|(_, r)| r.contains(p.at))
            {
                self.open_library(session, libraries, i);
            }
        } else if p.zoom <= PINCH_TO_LIBRARIES {
            self.showing_libraries = true;
            self.renaming_set = None;
            self.renaming_brush = None;
        }
    }

    /// Brush `key`'s menu opens (a brush held, or clicked with the secondary button).
    fn open_brush_menu(&mut self, session: &CanvasSession, key: (u32, u32)) {
        let pinned = self
            .recent
            .then(|| session.recent_brushes().iter().find(|r| (r.set, r.index) == key).is_some_and(|r| r.pinned));
        self.brush_menu_for = Some((key, pinned));
        self.brush_menu.open();
    }

    /// Follows the press on a set: moved first, it is left to the list; held still, the set lifts
    /// and follows the pointer and, let go over another set, takes its place (the set shown stays
    /// shown). True when a lifted set was let go.
    fn follow_set_press(&mut self, ctx: &egui::Context, session: &CanvasSession, shown: usize) -> bool {
        let Some(mut p) = self.set_press else { return false };
        let (at, down) = ctx.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
        let at = at.map_or(p.last, |at| panel::in_panel(ctx, at));
        if down {
            if p.lifted {
                ctx.request_repaint();
            } else if !p.spent {
                if (at - p.origin).length() > SLOP {
                    p.spent = true;
                } else if p.since.elapsed() >= HOLD {
                    p.lifted = true;
                    ctx.request_repaint();
                } else {
                    ctx.request_repaint_after(HOLD.saturating_sub(p.since.elapsed()));
                }
            }
            p.last = at;
            self.set_press = Some(p);
            return false;
        }
        self.set_press = None;
        if !p.lifted {
            return false;
        }
        if let Some(&(to, _)) = self.set_rects.iter().find(|(_, r)| r.contains(at))
            && to != p.set
            && session.move_brush_set(p.set as u32, to as u32)
        {
            let from = p.set;
            self.shown_set = Some(match shown {
                s if s == from => to,
                s if s > from && s <= to => s - 1,
                s if s >= to && s < from => s + 1,
                s => s,
            });
            self.library_changed();
        }
        true
    }

    /// The Recent set's row, as a set's: its hourglass and its name. Returns its response and where
    /// it is.
    fn recent_row(&mut self, ui: &mut Ui, look: &mut Look, selected: bool) -> (egui::Response, Rect) {
        let width = ui.available_width();
        let (outer, response) = ui.allocate_exact_size(vec2(width, SET_HEIGHT), Sense::click());
        let rect = outer.shrink2(vec2(12.0, 0.0));
        ui.painter().rect_filled(rect, 12, panel::row_fill(selected, SET_CELL));
        let ink = if selected { Color32::WHITE } else { argb(PANEL_TITLE) };
        look.icon(ui, &icons::RECENT_SET, pos2(rect.left() + 24.0, rect.center().y), 40.0, ink);
        let name = one_line(ui, "Recent", 14.0, Weight::SemiBold, ink, rect.width() - 53.0);
        let left = rect.left() + 4.0 + 40.0 + 1.0;
        ui.painter().galley(pos2(left, rect.center().y - name.size().y / 2.0), name, ink);
        (response, rect)
    }

    /// A set: its icon and its name (two lines at most), or a field renaming it. Returns its
    /// response and where it is.
    #[allow(clippy::too_many_arguments)]
    fn set_row(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        i: usize,
        set: &BrushSetInfo,
        selected: bool,
        fill: Option<Color32>,
    ) -> (egui::Response, Rect) {
        let width = ui.available_width();
        let (outer, response) = ui.allocate_exact_size(vec2(width, SET_HEIGHT), Sense::click());
        let rect = outer.shrink2(vec2(12.0, 0.0));
        ui.painter().rect_filled(rect, 12, fill.unwrap_or_else(|| panel::row_fill(selected, SET_CELL)));
        let ink = if selected { Color32::WHITE } else { argb(PANEL_TITLE) };
        // a 40 frame centred 24 in
        let icon = icons::SET_ICONS.get(set.icon as usize).unwrap_or(&icons::BRUSH_SET);
        look.icon(ui, icon, pos2(rect.left() + 24.0, rect.center().y), 40.0, ink);
        let left = rect.left() + 4.0 + 40.0 + 1.0;
        if let Some((_, typed, focused)) = self.renaming_set.as_mut().filter(|(s, _, _)| *s == i) {
            let field = Rect::from_min_max(
                pos2(left, rect.center().y - 10.0),
                pos2(rect.right() - 8.0, rect.center().y + 10.0),
            );
            let mut output = egui::TextEdit::singleline(typed)
                .font(Weight::SemiBold.font(14.0))
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
                let name = typed.clone();
                self.renaming_set = None;
                if session.rename_brush_set(i as u32, name) {
                    self.library_changed();
                }
            }
            return (response, rect);
        }
        let mut job = LayoutJob::default();
        job.append(&set.name, 0.0, format(14.0, Weight::SemiBold, ink));
        job.wrap = TextWrapping {
            max_width: rect.right() - 8.0 - left,
            max_rows: 2,
            break_anywhere: false,
            overflow_character: Some('…'),
        };
        let galley = ui.painter().layout_job(job);
        let at = Align2::LEFT_CENTER.anchor_size(pos2(left, rect.center().y), galley.size());
        ui.painter().galley(at.min, galley, ink);
        (response, rect)
    }

    /// A brush: its sample stroke filling the cell's height (its ends past the cell's sides), under
    /// its name (typed over while it is renamed); slid left, its buttons behind it. `pinned`: in the
    /// Recent set, whether it is pinned there (marked so). Returns whether it was tapped and which
    /// of its buttons was, and where its cell is. Clicked with the secondary button, its menu opens.
    #[allow(clippy::too_many_arguments)]
    fn brush_row(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        session: &CanvasSession,
        b: &BrushInfo,
        selected: bool,
        pinned: Option<bool>,
    ) -> (Option<(bool, Option<Swiped>)>, Rect) {
        let row = ui.available_width();
        let (outer, _) = ui.allocate_exact_size(vec2(row, BRUSH_HEIGHT), Sense::hover());
        let cell = outer.shrink2(vec2(12.0, 0.0));
        if !ui.is_rect_visible(outer) {
            return (None, cell);
        }
        let key = (b.set, b.index);
        let slide = self.slid.get(&key).copied().unwrap_or(0.0);
        let at = cell.translate(vec2(slide, 0.0));
        let mut chose = None;
        if slide < 0.0 {
            chose = swipe_buttons(ui, look, cell, key, pinned);
        }
        // (only a click: a drag goes through to the list, which scrolls by itself)
        let response = ui.interact(at.intersect(cell), ui.scope_id().with(("brush", key)), Sense::click());
        if response.is_pointer_button_down_on()
            && self.press.is_none()
            && let Some(origin) = ui.input(|i| i.pointer.interact_pos()).map(|p| panel::in_panel(ui.ctx(), p))
        {
            let since = Instant::now();
            self.press = Some(Press {
                list: self.list_at,
                key,
                origin,
                since,
                way: Way::Undecided,
                menu_at: None,
                start: slide,
                grab: origin - cell.min,
                last: origin,
            });
        }
        self.want_preview(ui, b, cell);
        let painter = ui.painter_at(cell);
        // the dragged brushes are faint where they were
        let faint = self.press.is_some_and(|p| p.way == Way::Drag) && self.dragged_with.contains(&key);
        let alpha = if faint { 0.3 } else { 1.0 };
        let picked = self.picked.contains(&key);
        if selected {
            painter.rect_filled(at, 12, argb(pc::ACCENT).gamma_multiply(alpha));
        } else if picked {
            painter.rect_filled(at, 12, argb(PICKED_BRUSH).gamma_multiply(alpha));
        }
        let mut faded = painter.clone();
        faded.multiply_opacity(alpha);
        let shaded = self.preview_in(&faded, key, at, selected);
        let ink = if selected || picked { Color32::WHITE } else { argb(PANEL_TITLE) }.gamma_multiply(alpha);
        let name_at = at.min + vec2(18.0, 11.75);
        if self.renaming_brush.as_ref().is_some_and(|(k, _, _)| *k == key) {
            let field = Rect::from_min_size(name_at, vec2(at.width() - 18.0 - 12.0, 18.0));
            self.brush_name_field(ui, session, field, ink);
        } else {
            let galley = one_line(ui, &b.name, 14.0, Weight::SemiBold, ink, at.width() - 18.0 - 10.0);
            if shaded {
                let pill = Rect::from_min_size(name_at, galley.size()).expand2(vec2(6.0, 2.0));
                painter.rect_filled(pill, pill.height() / 2.0, argb(NAME_PILL).gamma_multiply(alpha));
            }
            painter.galley(name_at, galley, ink);
        }
        if response.secondary_clicked() {
            self.swiped = None;
            self.open_brush_menu(session, key);
        }
        // a pinned brush's mark, at the cell's top right
        if pinned == Some(true) {
            let mut marked = ui.new_child(UiBuilder::new().max_rect(cell));
            marked.set_clip_rect(cell.intersect(ui.clip_rect()));
            look.icon(&marked, &icons::PIN_MARK, pos2(at.right() - 11.0 - 8.0, at.top() + 12.5 + 8.0), 16.0, ink);
        }
        let tapped = response.clicked() && self.press.is_none_or(|p| p.way == Way::Undecided);
        (Some((tapped, chose)), cell)
    }

    /// The brush being renamed: its name typed over at `field`, all of it chosen to start with; the
    /// name it has when the field lets go of the keys is its own.
    fn brush_name_field(&mut self, ui: &mut Ui, session: &CanvasSession, field: Rect, ink: Color32) {
        let Some((key, typed, focused)) = self.renaming_brush.as_mut() else { return };
        let mut output = egui::TextEdit::singleline(typed)
            .font(Weight::SemiBold.font(14.0))
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
            let (key, name) = (*key, typed.clone());
            self.renaming_brush = None;
            if session.rename_brush(key.0, key.1, name) {
                self.library_changed();
            }
        }
    }

    /// Asks for a brush's sample stroke when it is in sight and not drawn yet (one drawn is seen).
    fn want_preview(&mut self, ui: &Ui, b: &BrushInfo, cell: Rect) {
        let key = (b.set, b.index);
        if !ui.is_rect_visible(cell) {
            return;
        }
        match self.previews.get_mut(&key) {
            Some(Some(p)) => p.seen = self.frames,
            Some(None) => {}
            None => self.wanted.push((key, b.name.clone())),
        }
    }

    /// Lets go of the sample strokes' pictures out of sight last frame past `most` kept, the
    /// longest unseen first: one scrolled back into sight is asked for again (and comes from the
    /// strokes the session keeps, drawn already).
    fn keep_previews_seen(&mut self, most: usize) {
        let held = self.previews.values().filter(|p| p.is_some()).count();
        if held <= most {
            return;
        }
        // (called before the frame's count goes up: those seen last frame were seen at it)
        let last = self.frames;
        let mut unseen: Vec<(u64, (u32, u32))> = self
            .previews
            .iter()
            .filter_map(|(k, p)| p.as_ref().filter(|p| p.seen < last).map(|p| (p.seen, *k)))
            .collect();
        unseen.sort_unstable();
        for (_, key) in unseen.into_iter().take(held - most) {
            self.previews.remove(&key);
        }
    }

    /// Lets go of every sample stroke's picture, the cards' too (the GPU short of memory): those in
    /// sight are asked for again, and come from the strokes the session keeps. Between frames only,
    /// as one may draw them.
    pub fn let_go_of_previews(&mut self) {
        self.previews.clear();
        self.card_previews.clear();
        self.card_retired.clear();
    }

    /// How many sample strokes' pictures are held.
    #[cfg(test)]
    fn previews_held(&self) -> usize {
        self.previews.values().filter(|p| p.is_some()).count()
    }

    /// The sample strokes of `brushes` (their places and names) for a plugin's panel, as drawn so
    /// far: those not drawn yet are drawn on the library's thread, a frame asked for as they come.
    pub fn panel_strokes(
        &mut self,
        ctx: &egui::Context,
        session: &Arc<CanvasSession>,
        brushes: &[((u32, u32), String)],
    ) -> crate::actions::Strokes {
        for (key, name) in brushes {
            if !self.previews.contains_key(key) && !self.wanted.iter().any(|(k, _)| k == key) {
                self.wanted.push((*key, name.clone()));
            }
        }
        self.draw_previews(ctx, session);
        let drawn = |(key, _): &((u32, u32), String)| Some((*key, self.previews.get(key)?.as_ref()?.texture.id()));
        brushes.iter().filter_map(drawn).collect()
    }

    /// Takes the sample strokes drawn so far, and has those wanted drawn on a thread of their own.
    fn draw_previews(&mut self, ctx: &egui::Context, session: &Arc<CanvasSession>) {
        if let Some(drawing) = &self.drawing {
            loop {
                match drawing.try_recv() {
                    Ok((key, drawn)) => {
                        let preview = drawn.map(|(image, shaded)| {
                            let texture = ctx.load_texture(format!("brush {key:?}"), image, TextureOptions::LINEAR);
                            Preview { texture, shaded, seen: self.frames }
                        });
                        self.previews.insert(key, preview);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.drawing = None;
                        break;
                    }
                }
            }
        }
        let previews = &self.previews;
        self.wanted.retain(|(key, _)| !previews.contains_key(key));
        if self.drawing.is_none() && !self.wanted.is_empty() {
            let (done, drawing) = std::sync::mpsc::channel();
            let batch: Vec<_> = self.wanted.drain(..self.wanted.len().min(PREVIEWS_AT_A_TIME)).collect();
            let session = Arc::clone(session);
            let started = std::thread::Builder::new().name("brush previews".into()).spawn(move || {
                for (key, name) in batch {
                    let drawn = session.brush_preview(key.0, key.1, PREVIEW.0, PREVIEW.1).map(|t| {
                        let shaded = under_name(&t.rgba, &name);
                        let size = [t.width as usize, t.height as usize];
                        (egui::ColorImage::from_rgba_premultiplied(size, &t.rgba), shaded)
                    });
                    // (the library changed meanwhile: these are no longer wanted)
                    if done.send((key, drawn)).is_err() {
                        return;
                    }
                }
            });
            match started {
                Ok(_) => self.drawing = Some(drawing),
                Err(e) => log::warn!("the brushes' sample strokes aren't drawn: {e}"),
            }
        }
        if self.drawing.is_some() {
            ctx.request_repaint_after(PREVIEW_LOOK);
        }
    }

    /// The search field in `block` (shown where it overlaps `clip`): a magnifier, what is typed
    /// (or "Search"), and once typed, a button that clears it. Enter, Escape or a press elsewhere
    /// leaves it.
    fn search_field(&mut self, ui: &mut Ui, block: Rect, clip: Rect) {
        let field = Rect::from_min_max(block.min + vec2(12.0, 10.0), pos2(block.right() - 12.0, block.top() + 50.0));
        let hint = argb(SEARCH_HINT);
        let mut inside = ui.new_child(UiBuilder::new().max_rect(field));
        inside.set_clip_rect(clip.intersect(field));
        let painter = inside.painter().clone();
        painter.rect_filled(field, 10, argb(SEARCH_FILL));
        // a magnifier: a ring and its handle
        let c = pos2(field.left() + 41.0 * 0.47, field.top() + 40.0 * 0.46);
        painter.circle_stroke(c, 7.0, egui::Stroke::new(2.0, hint));
        let d = 7.0 * std::f32::consts::FRAC_1_SQRT_2;
        painter.line_segment([c + vec2(d, d), c + vec2(d + 5.0, d + 5.0)], egui::Stroke::new(2.4, hint));
        let clear = (!self.query.is_empty())
            .then(|| Rect::from_center_size(pos2(field.right() - 5.0 - 9.5, field.center().y), vec2(19.0, 18.0)));
        let words = Rect::from_min_max(
            pos2(field.left() + 41.0, field.top()),
            pos2(clear.map_or(field.right() - 8.0, |r| r.left() - 6.0), field.bottom()),
        );
        if self.query.is_empty() {
            text(&painter, words.left_center(), Align2::LEFT_CENTER, "Search", 17.0, Weight::Regular, hint);
        }
        let edit = egui::TextEdit::singleline(&mut self.query)
            .frame(egui::Frame::NONE)
            .font(Weight::Regular.font(17.0))
            .text_color(Color32::WHITE)
            .align(Align2::LEFT_CENTER)
            .min_size(words.size())
            .desired_width(words.width())
            .show(&mut inside.new_child(UiBuilder::new().max_rect(words)));
        // taking or leaving the keys changes what shows: drawn again now
        if self.searching != edit.response.has_focus() {
            self.searching = edit.response.has_focus();
            ui.ctx().request_repaint();
        }
        if let Some(clear) = clear {
            // a filled circle with a cross through it
            painter.circle_filled(clear.center(), 9.0, hint);
            let k = 3.5;
            for s in [1.0, -1.0] {
                let (a, b) = (clear.center() + vec2(-k, -k * s), clear.center() + vec2(k, k * s));
                painter.line_segment([a, b], egui::Stroke::new(1.8, argb(CLEAR_CROSS)));
            }
            if inside.interact(clear, inside.scope_id().with("clear search"), Sense::click()).clicked() {
                self.query.clear();
            }
        }
    }

    /// The brushes of every set whose name holds the search, two to a row in `area`; returns the
    /// one tapped. None found (or nothing typed yet): "No results".
    fn search_results(
        &mut self,
        ui: &mut Ui,
        library: &[BrushSetInfo],
        current: (u32, u32),
        area: Rect,
    ) -> Option<(u32, u32)> {
        let query = self.query.trim().to_lowercase();
        let found: Vec<&BrushInfo> = if query.is_empty() {
            Vec::new()
        } else {
            library.iter().flat_map(|s| &s.brushes).filter(|b| b.name.to_lowercase().contains(&query)).collect()
        };
        if found.is_empty() {
            let at = pos2(area.center().x, area.top() + 24.0);
            text(ui.painter(), at, Align2::CENTER_TOP, "No results", 15.0, Weight::Medium, argb(PANEL_TITLE));
            return None;
        }
        let mut tapped = None;
        self.result_rects.clear();
        ui.scope_builder(UiBuilder::new().max_rect(area), |ui| {
            ScrollArea::vertical()
                .id_salt("library results")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.add_space(11.0);
                    let width = (area.width() - 12.0 * 2.0 - 6.0) / 2.0;
                    for pair in found.chunks(2) {
                        let (row, _) = ui.allocate_exact_size(vec2(area.width(), RESULT_HEIGHT), Sense::hover());
                        for (k, b) in pair.iter().enumerate() {
                            let tile = Rect::from_min_size(
                                pos2(row.left() + 12.0 + (width + 6.0) * k as f32, row.top()),
                                vec2(width, RESULT_HEIGHT),
                            );
                            let key = (b.set, b.index);
                            let selected = key == current;
                            self.result_rects.push((key, tile));
                            self.want_preview(ui, b, tile);
                            let painter = ui.painter_at(tile);
                            painter.rect_filled(tile, 12, if selected { argb(pc::ACCENT) } else { argb(RESULT_FILL) });
                            let shaded = self.preview_in(&painter, key, tile, selected);
                            let ink = if selected { Color32::WHITE } else { argb(PANEL_TITLE) };
                            let galley = one_line(ui, &b.name, 14.0, Weight::SemiBold, ink, tile.width() - 14.0 - 10.0);
                            let name_at = tile.min + vec2(14.0, 11.0);
                            if shaded {
                                let pill = Rect::from_min_size(name_at, galley.size()).expand2(vec2(6.0, 2.0));
                                painter.rect_filled(pill, pill.height() / 2.0, argb(NAME_PILL));
                            }
                            painter.galley(name_at, galley, ink);
                            if ui.interact(tile, ui.scope_id().with(("result", key)), Sense::click()).clicked() {
                                tapped = Some(key);
                            }
                        }
                        ui.add_space(6.0);
                    }
                    ui.add_space(12.0 - 6.0);
                });
        });
        tapped
    }

    /// Draws brush `key`'s sample stroke filling `at`'s height (white on blue when `selected`,
    /// black on the light interface's rows); returns whether it runs under the name.
    fn preview_in(&self, painter: &egui::Painter, key: (u32, u32), at: Rect, selected: bool) -> bool {
        let Some(Some(p)) = self.previews.get(&key) else { return false };
        let (pw, ph) = (PREVIEW.0 as f32, PREVIEW.1 as f32);
        let k = (at.width() / pw).max(at.height() / ph);
        let shown = Rect::from_center_size(at.center(), vec2(pw * k, ph * k));
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        painter.image(p.texture.id(), shown, uv, self.tint(selected));
        p.shaded
    }
}

/// The set icons to choose from, five across in `rect` (scrolled at first to put `own` a few rows
/// down, unless `scrolled` already): `own` blue. Returns the one tapped.
fn icon_grid(ui: &mut Ui, look: &mut Look, rect: Rect, own: usize, scrolled: bool) -> Option<usize> {
    let inner = rect.shrink(3.0);
    let cell_w = inner.width() / PICKER_COLUMNS as f32;
    let mut picked = None;
    ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
        let mut area = ScrollArea::vertical()
            .id_salt("set icons")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
        if !scrolled {
            // an icon that isn't one of these starts it at the top
            let first = if own < PICKED_SET_ICONS { (own / PICKER_COLUMNS).saturating_sub(3) } else { 0 };
            area = area.vertical_scroll_offset(first as f32 * cell_w);
        }
        area.show(ui, |ui| {
            let rows = PICKED_SET_ICONS.div_ceil(PICKER_COLUMNS);
            let (all, _) = ui.allocate_exact_size(vec2(inner.width(), rows as f32 * cell_w), Sense::hover());
            for k in 0..PICKED_SET_ICONS {
                let (col, row) = (k % PICKER_COLUMNS, k / PICKER_COLUMNS);
                let slot =
                    Rect::from_min_size(all.min + vec2(col as f32 * cell_w, row as f32 * cell_w), vec2(cell_w, cell_w));
                let cell = Rect::from_center_size(slot.center(), vec2(PICKER_CELL, PICKER_CELL));
                if k == own {
                    ui.painter().rect_filled(cell, 21, argb(pc::ACCENT));
                }
                if let Some(icon) = icons::SET_ICONS.get(k) {
                    look.icon(ui, icon, cell.center(), 40.0, Color32::WHITE);
                }
                if ui.interact(cell, ui.scope_id().with(("set icon", k)), Sense::click()).clicked() {
                    picked = Some(k);
                }
            }
        });
    });
    picked
}

/// What a slid brush's buttons do.
#[derive(Clone, Copy)]
enum Swiped {
    Share,
    Duplicate,
    Delete,
    Find,
    Pin,
    Unpin,
    Clear,
}

/// A slid brush's buttons, filling its cell: Share, Duplicate and Delete; in the Recent set
/// (`pinned` there or not) Find, Pin or Unpin, and Clear. Returns the one tapped.
fn swipe_buttons(ui: &Ui, look: &mut Look, cell: Rect, key: (u32, u32), pinned: Option<bool>) -> Option<Swiped> {
    let w = (cell.width() - SWIPE_GAP * 2.0) / 3.0;
    let buttons = match pinned {
        None => [
            (&icons::SWIPE_SHARE, "Share", SWIPE_FILL, SWIPE_INK, Swiped::Share),
            (&icons::SWIPE_DUPLICATE, "Duplicate", SWIPE_FILL, SWIPE_INK, Swiped::Duplicate),
            (&icons::SWIPE_TRASH, "Delete", SWIPE_DELETE, 0xFFFFFFFF, Swiped::Delete),
        ],
        Some(pinned) => [
            (&icons::SWIPE_FIND, "Find", SWIPE_FILL, SWIPE_INK, Swiped::Find),
            if pinned {
                (&icons::SWIPE_UNPIN, "Unpin", SWIPE_FILL, SWIPE_INK, Swiped::Unpin)
            } else {
                (&icons::SWIPE_PIN, "Pin", SWIPE_FILL, SWIPE_INK, Swiped::Pin)
            },
            (&icons::SWIPE_CLEAR, "Clear", SWIPE_FILL, SWIPE_INK, Swiped::Clear),
        ],
    };
    let mut chose = None;
    for (i, (icon, label, fill, ink, does)) in buttons.into_iter().enumerate() {
        let rect =
            Rect::from_min_size(pos2(cell.left() + (w + SWIPE_GAP) * i as f32, cell.top()), vec2(w, cell.height()));
        ui.painter().rect_filled(rect, 16, argb(fill));
        // 19 across, the label under it, the two in the middle (5 down)
        let middle = rect.center().y + 2.5;
        look.icon(ui, icon, pos2(rect.center().x, middle - 9.5 - 1.0), 19.0, argb(ink));
        text(
            ui.painter(),
            pos2(rect.center().x, middle + 1.0),
            Align2::CENTER_TOP,
            label,
            14.0,
            Weight::Medium,
            argb(ink),
        );
        if ui.interact(rect, ui.scope_id().with(("swipe", key, i)), Sense::click()).clicked() {
            chose = Some(does);
        }
    }
    chose
}

fn format(size: f32, weight: Weight, colour: Color32) -> TextFormat {
    TextFormat { font_id: weight.font(size), color: colour, extra_letter_spacing: -0.045 * size, ..Default::default() }
}

/// Text on one line, cut short with an ellipsis past `max_width`.
fn one_line(ui: &Ui, text: &str, size: f32, weight: Weight, colour: Color32, max_width: f32) -> Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    job.append(text, 0.0, format(size, weight, colour));
    job.wrap = TextWrapping { max_width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.painter().layout_job(job)
}

/// Whether a sample stroke (`PREVIEW` big, premultiplied RGBA) runs under its name: the name's box
/// (19.5 in, 14 to 26 down, about 7.7 a letter wide, in a cell the picture fills 80 high) covered
/// more than 2/255 on average.
fn under_name(rgba: &[u8], name: &str) -> bool {
    let (w, h) = (PREVIEW.0 as usize, PREVIEW.1 as usize);
    let k = PREVIEW.1 as f32 / BRUSH_HEIGHT;
    let x0 = ((19.5 + 6.85) * k) as usize;
    let x1 = (((19.5 + 6.85 + 7.7 * name.chars().count() as f32) * k) as usize).min(w);
    let (y0, y1) = (((14.0 * k) as usize).min(h), ((26.0 * k) as usize).min(h));
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let mut cover = 0u64;
    for y in y0..y1 {
        let line = rgba.get((y * w + x0) * 4..(y * w + x1) * 4).unwrap_or(&[]);
        cover += line.as_chunks::<4>().0.iter().map(|&[_, _, _, a]| u64::from(a)).sum::<u64>();
    }
    cover > 2 * ((x1 - x0) * (y1 - y0)) as u64
}

#[cfg(test)]
mod tests {
    use egui::{Event, PointerButton, RawInput, TouchDeviceId, TouchId, TouchPhase};

    use super::*;

    /// The library open with a finger on it, as the window's touches reach the interface.
    struct Finger {
        ctx: egui::Context,
        look: Look,
        library: Library,
        session: Arc<CanvasSession>,
        time: f64,
    }

    impl Finger {
        fn new() -> Finger {
            let session = CanvasSession::new(64, 64);
            let (ctx, look, library) = (egui::Context::default(), Look::default(), Library::default());
            let mut finger = Finger { ctx, look, library, session, time: 0.0 };
            // two frames to lay the list out and find it scrolls
            finger.frame(Vec::new());
            finger.frame(Vec::new());
            finger
        }

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 1.0 / 60.0;
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 700.0))),
                time: Some(self.time),
                events,
                ..RawInput::default()
            };
            let Finger { ctx, look, library, session, .. } = self;
            let mut output = ctx.clone().run_ui(raw, |ui| {
                let button = Rect::from_min_size(pos2(600.0, 10.0), vec2(40.0, 40.0));
                library.show(ui.ctx(), look, session, crate::panel::Hang::Button(button), &mut Vec::new());
            });
            output.textures_delta.clear();
        }

        fn touch(phase: TouchPhase, pos: Pos2) -> Event {
            let (device_id, id) = (TouchDeviceId(1), TouchId(1));
            Event::Touch { device_id, id, phase, pos, force: None }
        }

        fn down(&mut self, at: Pos2) {
            self.frame(vec![
                Self::touch(TouchPhase::Start, at),
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }

        fn to(&mut self, at: Pos2) {
            self.frame(vec![Self::touch(TouchPhase::Move, at), Event::PointerMoved(at)]);
        }

        fn up(&mut self, at: Pos2) {
            self.frame(vec![
                Self::touch(TouchPhase::End, at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
                Event::PointerGone,
            ]);
        }

        /// How far up the list has gone: where its first brush is.
        fn top(&self) -> f32 {
            self.library.brush_rects.first().map_or(0.0, |(_, r)| r.top())
        }

        /// A point on a brush in the middle of the list as it shows (a long set's further brushes
        /// lie past the window).
        fn on_a_brush(&self) -> Pos2 {
            let shown: Vec<Pos2> = self
                .library
                .brush_rects
                .iter()
                .map(|(_, r)| r.center())
                .filter(|c| (0.0..700.0).contains(&c.y))
                .collect();
            shown.get(shown.len() / 3).copied().unwrap_or(Pos2::ZERO)
        }
    }

    /// The library shown in a window of a laptop's size with its sample strokes drawn on the GPU
    /// (one device) and shown through egui-wgpu as the window shows them (another, whose memory is
    /// the interface's alone), scrolled by a finger.
    struct Shown {
        ctx: egui::Context,
        look: Look,
        library: Library,
        session: Arc<CanvasSession>,
        showing: procraft_gpu::Gpu,
        renderer: egui_wgpu::Renderer,
        time: f64,
    }

    impl Shown {
        /// The window's height (points), as a laptop's of 1080 pixels at 1.25 pixels a point.
        const HEIGHT: f32 = 820.0;

        fn frame(&mut self, events: Vec<Event>) {
            self.time += 1.0 / 60.0;
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1500.0, Self::HEIGHT))),
                time: Some(self.time),
                events,
                ..RawInput::default()
            };
            let Shown { ctx, look, library, session, .. } = self;
            let mut output = ctx.clone().run_ui(raw, |ui| {
                let button = Rect::from_min_size(pos2(600.0, 10.0), vec2(40.0, 40.0));
                library.show(ui.ctx(), look, session, crate::panel::Hang::Button(button), &mut Vec::new());
            });
            let (device, queue) = (&self.showing.device, &self.showing.queue);
            for (id, deltas) in &output.textures_delta.set {
                for delta in deltas {
                    self.renderer.update_texture(device, queue, *id, delta);
                }
            }
            for id in &output.textures_delta.free {
                self.renderer.free_texture(id);
            }
            output.textures_delta.clear();
        }

        /// Frames until every sample stroke in sight is drawn and shown, and the list is still.
        fn settle(&mut self) {
            let mut at = f32::NAN;
            for _ in 0..20_000 {
                self.frame(Vec::new());
                let still = self.library.list_at == at;
                at = self.library.list_at;
                if still && self.library.drawing.is_none() && self.library.wanted.is_empty() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        /// A finger drags the brushes up the list by `by` points, slowly (no fling).
        fn drag_up(&mut self, by: f32) {
            let low = Self::HEIGHT - 200.0;
            let Some(from) = self.library.brush_rects.iter().map(|(_, r)| r.center()).find(|c| c.y > low) else {
                return;
            };
            let touch =
                |phase, pos| Event::Touch { device_id: TouchDeviceId(1), id: TouchId(1), phase, pos, force: None };
            let press = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![touch(TouchPhase::Start, from), Event::PointerMoved(from), press(from, true)]);
            let steps = (by / 8.0).ceil() as usize;
            let mut at = from;
            for _ in 0..steps {
                at.y -= 8.0;
                self.frame(vec![touch(TouchPhase::Move, at), Event::PointerMoved(at)]);
            }
            // held still a moment, so it isn't flicked on
            for _ in 0..10 {
                self.frame(vec![touch(TouchPhase::Move, at), Event::PointerMoved(at)]);
            }
            self.frame(vec![touch(TouchPhase::End, at), press(at, false)]);
        }

        /// What the interface's device holds, bytes.
        fn held(&self) -> u64 {
            self.showing.flush();
            self.showing.device.generate_allocator_report().map_or(0, |r| r.total_allocated_bytes)
        }
    }

    /// Every set of the library as it ships browsed, scrolled from its top to its end, each brush's
    /// sample stroke drawn and shown in turn: the pictures held on the GPU stay within those kept
    /// ([`PREVIEWS_KEPT`]), not every brush's (all 598 of them came to 205 MB, which a laptop's
    /// integrated GPU runs out of beside a canvas). `PREVIEWS_MEASURED=1` says what was held after
    /// each set.
    #[test]
    fn browsing_every_set_holds_few_sample_strokes_on_the_gpu() {
        use procraft_gpu::{Gpu, wgpu};
        let (Ok(drawing), Ok(showing)) = (Gpu::headless(), Gpu::headless()) else {
            return eprintln!("skipped: no GPU");
        };
        let session = CanvasSession::new(64, 64);
        session.use_gpu(drawing.clone());
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let renderer = egui_wgpu::Renderer::new(&showing.device, format, egui_wgpu::RendererOptions::default());
        let (ctx, look, library) = (egui::Context::default(), Look::default(), Library::default());
        let mut shown = Shown { ctx, look, library, session, showing, renderer, time: 0.0 };
        // laid out first, its fonts' pictures made: what the strokes hold is counted from there
        shown.frame(Vec::new());
        shown.frame(Vec::new());
        let before = shown.held();
        let drawn = |gpu: &Gpu| {
            gpu.flush();
            gpu.device.generate_allocator_report().map_or(0, |r| r.total_allocated_bytes)
        };
        let drawing_before = drawn(&drawing);
        let sets = shown.session.brush_library();
        let measured = std::env::var_os("PREVIEWS_MEASURED").is_some();
        let (mut most, mut seen) = (0, std::collections::HashSet::new());
        for (i, set) in sets.iter().enumerate() {
            shown.library.shown_set = Some(i);
            shown.settle();
            // scrolled down to the set's end, most of a window's height at a time
            loop {
                let at = shown.library.list_at;
                seen.extend(shown.library.previews.iter().filter(|(_, p)| p.is_some()).map(|(k, _)| *k));
                most = most.max(shown.library.previews_held());
                shown.drag_up(Shown::HEIGHT - 300.0);
                shown.settle();
                if shown.library.list_at <= at + 1.0 {
                    break;
                }
            }
            seen.extend(shown.library.previews.iter().filter(|(_, p)| p.is_some()).map(|(k, _)| *k));
            most = most.max(shown.library.previews_held());
            if measured {
                let mb = shown.held().saturating_sub(before) as f64 / f64::from(1 << 20);
                let held = shown.library.previews_held();
                println!("{:<20} {:3} brushes: {held:3} pictures held, {mb:5.1} MB", set.name, set.brushes.len());
            }
        }
        let brushes: usize = sets.iter().map(|s| s.brushes.len()).sum();
        let picture = u64::from(PREVIEW.0 * PREVIEW.1 * 4);
        let bytes = shown.held().saturating_sub(before);
        let drawing_bytes = drawn(&drawing).saturating_sub(drawing_before);
        println!("the strokes' own device holds {:.1} MB more", drawing_bytes as f64 / f64::from(1 << 20));
        println!(
            "{} of {brushes} brushes' strokes shown in {} sets: at most {most} pictures held, {:.1} MB on the GPU after \
             (every one's would be {:.1} MB)",
            seen.len(),
            sets.len(),
            bytes as f64 / f64::from(1 << 20),
            (seen.len() as u64 * picture) as f64 / f64::from(1 << 20)
        );
        // every brush's stroke shown on the way (the window shows each in turn)
        assert!(seen.len() * 10 >= brushes * 9, "{} of {brushes} brushes' strokes shown", seen.len());
        // those kept and those in sight at once, which is fewer
        assert!(most <= PREVIEWS_KEPT + 16, "{most} pictures held");
        // the pictures kept, half more for the device's alignment, and 16 MB for the meshes' buffers
        // (34 MB on the PC it was written on; every brush's came to 205); where the device can't
        // say, nothing to compare
        let bound = PREVIEWS_KEPT as u64 * picture * 3 / 2 + (16 << 20);
        assert!(bytes <= bound, "{bytes} bytes held on the GPU, past {bound}");
        // and the strokes' own device, which draws them all on the one canvas kept for them, holds
        // as much whatever the number drawn (102 MB there)
        assert!(drawing_bytes <= 160 << 20, "{drawing_bytes} bytes held drawing the strokes");
        assert_eq!(drawing.errors() + shown.showing.errors(), 0);
    }

    #[test]
    fn the_brushes_keep_up_with_a_finger_and_carry_on_when_flicked() {
        let mut f = Finger::new();
        assert!(f.library.brush_rects.len() > 6, "too few brushes to scroll");
        let (start, from) = (f.top(), f.on_a_brush());
        f.down(from);
        // up the list, frame by frame: past a press's slop it moves as far as the finger does
        let mut at = from;
        for _ in 0..4 {
            at.y -= 5.0;
            f.to(at);
        }
        let (mid, finger) = (f.top(), at.y);
        for _ in 0..16 {
            at.y -= 5.0;
            f.to(at);
            let (moved, went) = (mid - f.top(), finger - at.y);
            assert!((moved - went).abs() < 0.01, "the list went {moved} as the finger went {went}");
        }
        assert!(start > f.top());
        // let go moving, it runs on
        f.up(at);
        let lifted = f.top();
        for _ in 0..10 {
            f.frame(Vec::new());
        }
        assert!(f.top() < lifted - 10.0, "stopped dead: {} to {}", lifted, f.top());
    }

    #[test]
    fn the_sample_strokes_show_in_the_colour_a_plugin_gives_by_the_colour_painted_with() {
        const ID: &str = "io.github.artcraftandroid.interface-look";
        let dir = std::env::temp_dir().join(format!("procraft-library-tints-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = procraft_core::Plugins::with_bundled(dir.to_string_lossy().into_owned());
        let mut f = Finger::new();
        f.session.use_plugins(Arc::clone(&plugins));
        f.session.set_color(1.0, 0.0, 0.0);
        f.frame(Vec::new());
        let red = Color32::from_rgb(255, 0, 0);
        // as the interface draws them with the plugin off (it comes so)
        assert_ne!(f.library.tint(false), red);
        assert_eq!(f.library.tint(true), Color32::WHITE);
        // on: each brush's in the colour painted with, the one in use's white on the accent
        assert!(plugins.set_on(ID.to_owned(), true));
        assert!(plugins.set_switch(format!("{ID}.coloured_previews"), true));
        f.frame(Vec::new());
        assert_eq!((f.library.tint(false), f.library.tint(true)), (red, Color32::WHITE));
        // the colour changed, the next frame shows it, the strokes themselves not drawn again
        f.session.set_color(0.0, 0.0, 1.0);
        f.frame(Vec::new());
        assert_eq!(f.library.tint(false), Color32::from_rgb(0, 0, 255));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_press_on_a_brush_holds_the_list_until_it_goes_up_or_down() {
        let mut f = Finger::new();
        let from = f.on_a_brush();
        f.down(from);
        let mut at = from;
        for _ in 0..12 {
            at.y -= 8.0;
            f.to(at);
        }
        f.up(at);
        f.frame(Vec::new());
        // still running on, a press stops it
        let press = f.on_a_brush();
        f.down(press);
        let held = f.top();
        for _ in 0..6 {
            f.frame(Vec::new());
        }
        assert!((f.top() - held).abs() < 0.01, "ran on from {held} to {}", f.top());
        // and however the pointer jumps while it is undecided or slid sideways, the list stays
        f.to(press + vec2(-30.0, 3.0));
        f.to(press + vec2(-60.0, 12.0));
        f.to(press + vec2(-90.0, -9.0));
        assert!((f.top() - held).abs() < 0.01, "moved from {held} to {}", f.top());
        f.up(press + vec2(-90.0, -9.0));
    }

    #[test]
    fn a_press_slides_a_brush_only_going_plainly_sideways() {
        let swipe = |x, y| way_moved(vec2(x, y)) == Some(Way::Swipe);
        let scroll = |x, y| way_moved(vec2(x, y)) == Some(Way::Scroll);
        // straight across, and a little off it, either way
        assert!(swipe(-14.0, 0.0) && swipe(14.0, 0.0) && swipe(-20.0, 9.0) && swipe(20.0, -9.0));
        // straight up or down, and a hand's arc across as far as along, or further
        assert!(scroll(0.0, 8.0) && scroll(0.0, -8.0) && scroll(-12.0, -10.0) && scroll(-19.0, 10.0));
        // too little to tell: it waits
        assert_eq!(way_moved(vec2(-13.0, 7.0)), None);
        assert_eq!(way_moved(vec2(-5.0, 5.0)), None);
    }

    #[test]
    fn a_hand_swinging_across_as_it_scrolls_moves_the_list_and_slides_no_brush() {
        let mut f = Finger::new();
        let (start, from) = (f.top(), f.on_a_brush());
        f.down(from);
        let mut at = from;
        // up and to the left, as a wrist turns: as far across as along at first
        for k in 0..16 {
            at += if k < 3 { vec2(-6.0, -5.0) } else { vec2(-3.0, -8.0) };
            f.to(at);
            assert!(f.library.slid.values().all(|s| *s == 0.0), "a brush slid on a scroll, step {k}");
        }
        f.up(at);
        assert!(f.top() < start - 50.0, "the list stayed: {} to {}", start, f.top());
        assert!(f.library.swiped.is_none(), "a brush opened on a scroll");
    }

    #[test]
    fn a_brush_held_then_moved_onto_another_takes_its_place_and_a_still_hold_shows_the_menu() {
        let mut f = Finger::new();
        let from = f.on_a_brush();
        let (key, cell) = f.library.brush_rects.iter().find(|(_, r)| r.contains(from)).copied().unwrap();
        let name = |f: &Finger, (set, index): (u32, u32)| {
            f.session.brush_library()[set as usize].brushes[index as usize].name.clone()
        };
        let held = name(&f, key);
        // the brush two places further down, as it shows: on its lower half, just below it (in its
        // place once the held one has left its own)
        let (target, onto) = f
            .library
            .brush_rects
            .iter()
            .find(|(k, r)| k.0 == key.0 && k.1 == key.1 + 2 && r.top() > cell.bottom())
            .map(|(k, r)| (*k, r.center() + vec2(0.0, r.height() / 4.0)))
            .unwrap();
        // held still: the menu, and nothing moved
        f.down(from);
        std::thread::sleep(HOLD + Duration::from_millis(50));
        f.frame(Vec::new());
        assert!(f.library.brush_menu.is_open());
        // moved on and down onto the other brush, let go there: the menu went, the brush took that place
        let steps = 6;
        for i in 1..=steps {
            f.to(from + (onto - from) * (i as f32 / steps as f32));
        }
        assert!(!f.library.brush_menu.is_open());
        f.up(onto);
        f.frame(Vec::new());
        assert_eq!(name(&f, target), held);
        assert_ne!(name(&f, key), held);
        assert!(!f.library.brush_menu.is_open());
    }

    /// A brush held past its menu showing and moved on only after the core's drag window isn't
    /// lifted (as measured: presses of 0.7 s and 1 s moved on scrolled the list): the list has the
    /// press and scrolls under the menu, which stays open; let go, nothing is chosen or moved.
    #[test]
    fn a_brush_held_and_moved_on_too_late_scrolls_the_list_under_its_menu() {
        let mut f = Finger::new();
        let names = |f: &Finger| -> Vec<Vec<String>> {
            f.session.brush_library().iter().map(|s| s.brushes.iter().map(|b| b.name.clone()).collect()).collect()
        };
        let (before, chosen) = (names(&f), f.session.current_brush());
        let from = f.on_a_brush();
        f.down(from);
        std::thread::sleep(brush_hold() + Duration::from_millis(50));
        f.frame(Vec::new());
        assert!(f.library.brush_menu.is_open());
        let top = f.top();
        std::thread::sleep(Duration::from_secs_f64(procraft_core::BRUSH_DRAG_WINDOW) + Duration::from_millis(100));
        let mut at = from;
        for _ in 0..10 {
            at.y -= 8.0;
            f.to(at);
        }
        assert!(f.library.press.is_some_and(|p| p.way == Way::Scroll), "not left to the list");
        assert!(f.library.dragged_with.is_empty(), "lifted");
        assert!(f.library.brush_menu.is_open(), "the menu went");
        assert!(f.top() < top - 20.0, "the list stayed: {top} to {}", f.top());
        f.up(at);
        f.frame(Vec::new());
        assert!(f.library.brush_menu.is_open());
        assert_eq!(names(&f), before);
        let now = f.session.current_brush();
        assert_eq!((now.set, now.index), (chosen.set, chosen.index));
    }

    /// What a dragged brush is over: a set's row; in the shown set's list, the brush nearest up or
    /// down and which half of it (a gap or the room below the last going to the nearest); an empty
    /// list itself; nothing elsewhere or over the Recent set's list.
    #[test]
    fn a_dragged_brush_is_over_a_set_or_the_nearest_brush_s_upper_or_lower_half() {
        let sets = [
            (0, Rect::from_min_size(pos2(0.0, 0.0), vec2(150.0, 50.0))),
            (3, Rect::from_min_size(pos2(0.0, 62.0), vec2(150.0, 50.0))),
        ];
        let area = Rect::from_min_max(pos2(170.0, 0.0), pos2(600.0, 500.0));
        // set 2's brushes 0 to 2, 80 tall and 8 apart
        let rows: Vec<((u32, u32), Rect)> = (0..3)
            .map(|i| ((2, i), Rect::from_min_size(pos2(180.0, 10.0 + 88.0 * i as f32), vec2(400.0, 80.0))))
            .collect();
        let list = Some((2, 3, area));
        let over = |x: f32, y: f32| drop_over(pos2(x, y), &sets, list, &rows);
        let brush = |index, lower| Some(BrushDropOver::OnBrush { set: 2, index, lower });
        assert_eq!(over(60.0, 80.0), Some(BrushDropOver::OnSet { set: 3 }));
        assert_eq!(over(60.0, 56.0), None);
        // brush 1 (98 to 178): its upper half, its lower half
        assert_eq!(over(300.0, 120.0), brush(1, false));
        assert_eq!(over(300.0, 160.0), brush(1, true));
        // in the gap under brush 0, nearer it; above the first; far below the last
        assert_eq!(over(300.0, 92.0), brush(0, true));
        assert_eq!(over(300.0, 4.0), brush(0, false));
        assert_eq!(over(300.0, 450.0), brush(2, true));
        // an empty set's list: the list itself
        assert_eq!(
            drop_over(pos2(300.0, 200.0), &sets, Some((5, 0, area)), &[]),
            Some(BrushDropOver::InList { set: 5, count: 0 })
        );
        // off the list, or the Recent set's list (none): nothing
        assert_eq!(over(650.0, 120.0), None);
        assert_eq!(drop_over(pos2(300.0, 120.0), &sets, None, &rows), None);
    }

    /// A brush let go on another of its set's goes just above it from its upper half and just below
    /// it from its lower half, the rest of the set as it was.
    #[test]
    fn a_brush_let_go_on_another_s_upper_half_goes_above_it_and_on_its_lower_half_below() {
        let names = |f: &Finger, set: u32| -> Vec<String> {
            f.session.brush_library()[set as usize].brushes.iter().map(|b| b.name.clone()).collect()
        };
        for lower in [false, true] {
            let mut f = Finger::new();
            let from = f.on_a_brush();
            let key = f.library.brush_rects.iter().find(|(_, r)| r.contains(from)).map(|(k, _)| *k).unwrap();
            let before = names(&f, key.0);
            // the brush just above it: on its upper half the held one goes before it; on its lower
            // half it stays where it is
            let (target, onto) = f
                .library
                .brush_rects
                .iter()
                .find(|(k, _)| k.0 == key.0 && k.1 + 1 == key.1)
                .map(|(k, r)| (*k, r.center() + vec2(0.0, if lower { 1.0 } else { -1.0 } * r.height() / 4.0)))
                .unwrap();
            f.down(from);
            std::thread::sleep(HOLD + Duration::from_millis(50));
            f.frame(Vec::new());
            for i in 1..=6 {
                f.to(from + (onto - from) * (i as f32 / 6.0));
            }
            // along its own set: no green plus
            assert!(!f.library.drop_copies(f.library.press.unwrap()));
            f.up(onto);
            f.frame(Vec::new());
            let mut after = before.clone();
            if !lower {
                after.swap(target.1 as usize, key.1 as usize);
            }
            assert_eq!(names(&f, key.0), after, "lower half {lower}");
        }
    }

    /// A brush held over a new, empty set opens it, and let go in its list it is copied in.
    #[test]
    fn a_brush_held_over_an_empty_set_until_it_opens_is_copied_in_let_go_in_its_list() {
        let mut f = Finger::new();
        assert_eq!(f.session.new_brush_set(), 0);
        f.library.library_changed();
        f.frame(Vec::new());
        f.frame(Vec::new());
        let names = |f: &Finger, set: usize| -> Vec<String> {
            f.session.brush_library()[set].brushes.iter().map(|b| b.name.clone()).collect()
        };
        let from = f.on_a_brush();
        let key = f.library.brush_rects.iter().find(|(_, r)| r.contains(from)).map(|(k, _)| *k).unwrap();
        assert_ne!(key.0, 0);
        let theirs = names(&f, key.0 as usize);
        let over = f.library.set_rects.iter().find(|(s, _)| *s == 0).map(|(_, r)| r.center()).unwrap();
        f.down(from);
        std::thread::sleep(HOLD + Duration::from_millis(50));
        f.frame(Vec::new());
        for i in 1..=6 {
            f.to(from + (over - from) * (i as f32 / 6.0));
        }
        std::thread::sleep(SPRING_LOAD + Duration::from_millis(50));
        f.frame(Vec::new());
        f.frame(Vec::new());
        assert_eq!(f.library.shown_set, Some(0));
        assert!(f.library.brush_rects.is_empty());
        // into its list, away from the sets
        let (_, _, area) = f.library.brush_list.unwrap();
        let inside = area.center();
        for i in 1..=4 {
            f.to(over + (inside - over) * (i as f32 / 4.0));
        }
        assert!(f.library.drop_copies(f.library.press.unwrap()));
        f.up(inside);
        f.frame(Vec::new());
        assert_eq!(names(&f, 0), [theirs[key.1 as usize].clone()]);
        assert_eq!(names(&f, key.0 as usize), theirs);
    }

    /// Brushes picked go with the one held and dragged; held over a set that isn't shown, the set
    /// opens, and let go on one of its brushes they take its place there, in their order.
    #[test]
    fn picked_brushes_dragged_over_a_set_open_it_and_land_among_its_brushes() {
        let mut f = Finger::new();
        let names = |f: &Finger, set: usize| -> Vec<String> {
            f.session.brush_library()[set].brushes.iter().map(|b| b.name.clone()).collect()
        };
        let from = f.on_a_brush();
        let key = f.library.brush_rects.iter().find(|(_, r)| r.contains(from)).map(|(k, _)| *k).unwrap();
        let other =
            f.library.brush_rects.iter().find(|(k, _)| k.0 == key.0 && k.1 == key.1 + 1).map(|(k, _)| *k).unwrap();
        let before = names(&f, key.0 as usize);
        f.library.picked = vec![other, key];
        // a set other than this one, closed
        let (to, over) = f
            .library
            .set_rects
            .iter()
            .find(|(s, r)| *s != key.0 as usize && (0.0..700.0).contains(&r.center().y))
            .map(|(s, r)| (*s, r.center()))
            .unwrap();
        let theirs = names(&f, to);
        f.down(from);
        std::thread::sleep(HOLD + Duration::from_millis(50));
        f.frame(Vec::new());
        for i in 1..=6 {
            f.to(from + (over - from) * (i as f32 / 6.0));
        }
        assert_eq!(f.library.dragged_with, vec![key, other]);
        // over another set they would be copied there, as the green plus on them says
        let press = f.library.press.unwrap();
        assert!(f.library.drop_copies(press));
        // held there a moment: it opens, its brushes shown
        std::thread::sleep(SPRING_LOAD + Duration::from_millis(50));
        f.frame(Vec::new());
        f.frame(Vec::new());
        assert_eq!(f.library.shown_set, Some(to));
        assert!(f.library.brush_rects.iter().all(|(k, _)| k.0 as usize == to));
        // let go on its second brush: both take its place
        let onto = f.library.brush_rects.iter().find(|(k, _)| k.1 == 1).map(|(_, r)| r.center()).unwrap();
        for i in 1..=4 {
            f.to(over + (onto - over) * (i as f32 / 4.0));
        }
        f.up(onto);
        f.frame(Vec::new());
        let now = names(&f, to);
        assert_eq!(now.len(), theirs.len() + 2);
        assert_eq!(now[1..3], [before[key.1 as usize].clone(), before[other.1 as usize].clone()]);
        // copied there: their own set keeps them
        assert_eq!(names(&f, key.0 as usize), before);
    }

    #[test]
    fn a_brush_held_shows_its_menu_moved_on_is_dragged_and_renamed_keeps_its_name() {
        let mut f = Finger::new();
        let from = f.on_a_brush();
        let key = f.library.brush_rects.iter().find(|(_, r)| r.contains(from)).map(|(k, _)| *k).unwrap();
        let chosen = f.session.current_brush();
        let held = |f: &mut Finger| {
            f.down(from);
            std::thread::sleep(HOLD + Duration::from_millis(50));
            f.frame(Vec::new());
        };
        // held still: its menu, for it
        held(&mut f);
        assert!(f.library.brush_menu.is_open());
        assert_eq!(f.library.brush_menu_for.map(|(k, _)| k), Some(key));
        // let go there, the menu stays, and nothing is chosen or moved
        f.up(from);
        f.frame(Vec::new());
        assert!(f.library.brush_menu.is_open());
        assert_eq!((f.session.current_brush().set, f.session.current_brush().index), (chosen.set, chosen.index));
        // (closed as a press elsewhere would, a frame shown without it)
        f.library.brush_menu.close();
        f.frame(Vec::new());
        // held and moved on: the menu goes and the brush is dragged (back to its place: it stays)
        held(&mut f);
        assert!(f.library.brush_menu.is_open());
        f.to(from + vec2(0.0, 20.0));
        assert!(!f.library.brush_menu.is_open());
        assert!(f.library.press.is_some_and(|p| p.way == Way::Drag));
        f.to(from);
        f.up(from);
        // the secondary button opens it too
        let click = |pressed| Event::PointerButton {
            pos: from,
            button: PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        f.frame(vec![Event::PointerMoved(from), click(true)]);
        f.frame(vec![click(false)]);
        assert!(f.library.brush_menu.is_open());
        f.library.brush_menu.close();
        f.frame(Vec::new());
        // Rename: the name typed over, kept when the field lets go
        f.library.renaming_brush = Some((key, "old".into(), false));
        f.frame(Vec::new());
        f.frame(vec![Event::Text("My brush".into())]);
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        f.frame(vec![enter]);
        f.frame(Vec::new());
        assert!(f.library.renaming_brush.is_none());
        let name = f.session.brush_library()[key.0 as usize].brushes[key.1 as usize].name.clone();
        assert_eq!(name, "My brush");
    }

    /// Two fingers on a touch screen pinched about `at`, from `from` apart to `to` apart over ten
    /// frames, then lifted.
    fn pinch(f: &mut Finger, at: Pos2, from: f32, to: f32) {
        let touch =
            |id, phase, pos| Event::Touch { device_id: TouchDeviceId(1), id: TouchId(id), phase, pos, force: None };
        // (the first finger moving the pointer, as the window's touches do)
        let pair = |spread: f32, phase| {
            let first = at - vec2(spread / 2.0, 0.0);
            vec![Event::PointerMoved(first), touch(1, phase, first), touch(2, phase, at + vec2(spread / 2.0, 0.0))]
        };
        f.frame(pair(from, TouchPhase::Start));
        for k in 1..=10 {
            f.frame(pair(from + (to - from) * k as f32 / 10.0, TouchPhase::Move));
        }
        f.frame(pair(to, TouchPhase::End));
        f.frame(Vec::new());
    }

    #[test]
    fn two_fingers_pinched_in_show_every_library_and_out_on_a_card_open_it() {
        let mut f = Finger::new();
        let at = f.on_a_brush();
        // a little: nothing
        pinch(&mut f, at, 200.0, 150.0);
        assert!(!f.library.showing_libraries);
        // to less than two thirds: every library's card
        pinch(&mut f, at, 200.0, 120.0);
        assert!(f.library.showing_libraries);
        f.frame(Vec::new());
        let card = f.library.card_rects.first().map(|(_, r)| r.center()).expect("a card");
        // out a little on a card: nothing; past twice as far apart, it opens
        pinch(&mut f, card, 60.0, 100.0);
        assert!(f.library.showing_libraries);
        pinch(&mut f, card, 60.0, 130.0);
        assert!(!f.library.showing_libraries);
    }

    /// In one set with its brush, another set tapped: the brush last chosen there becomes the one
    /// in use, and it alone shows highlighted. A tap on another of its brushes makes that the
    /// brush (no studio); a tap on the brush in use opens the studio.
    #[test]
    fn a_set_tapped_brings_back_its_brush_and_a_tap_on_another_there_takes_it() {
        let mut f = Finger::new();
        let current = |f: &Finger| (f.session.current_brush().set, f.session.current_brush().index);
        let set_row = |f: &Finger, set: usize| {
            f.library.set_rects.iter().find(|(s, _)| *s == set).map(|(_, r)| r.center()).expect("the set's row")
        };
        let brush_cell = |f: &Finger, key: (u32, u32)| {
            f.library.brush_rects.iter().find(|(k, _)| *k == key).map(|(_, r)| r.center()).expect("the brush's cell")
        };
        let tap = |f: &mut Finger, at: Pos2| {
            f.down(at);
            f.up(at);
            f.frame(Vec::new());
        };
        // a brush chosen in set 1, then in set 0, which shows
        assert!(f.session.select_brush(1, 2));
        assert!(f.session.select_brush(0, 3));
        f.library.tool_changed();
        f.frame(Vec::new());
        assert_eq!(f.library.highlighted, Some((0, 3)));
        // set 1 tapped: its brush is the one in use, and the one highlighted
        let row = set_row(&f, 1);
        tap(&mut f, row);
        assert_eq!(current(&f), (1, 2));
        assert_eq!(f.library.highlighted, Some((1, 2)));
        assert!(f.library.brush_rects.iter().all(|(k, _)| k.0 == 1));
        // another brush there tapped: it is the brush, and the studio stays shut
        let other = brush_cell(&f, (1, 0));
        tap(&mut f, other);
        assert_eq!(current(&f), (1, 0));
        assert_eq!(f.library.highlighted, Some((1, 0)));
        assert!(f.library.studio.is_none());
        // the brush in use tapped: the studio
        tap(&mut f, other);
        assert!(f.library.studio.is_some());
        f.library.studio = None;
        // back in set 0, its own again
        let row = set_row(&f, 0);
        tap(&mut f, row);
        assert_eq!(current(&f), (0, 3));
        assert_eq!(f.library.highlighted, Some((0, 3)));
    }

    /// A set on screen other than set `but`, with brushes, that nothing was chosen in: its choice
    /// is its first brush.
    fn never_chosen(f: &Finger, but: u32) -> usize {
        f.library
            .set_rects
            .iter()
            .filter(|(_, r)| (0.0..650.0).contains(&r.center().y))
            .map(|(s, _)| *s)
            .find(|&s| {
                s as u32 != but
                    && f.session.brush_library()[s].brushes.len() > 1
                    && f.session.brush_set_choice(s as u32).map(|b| b.index) == Some(0)
            })
            .expect("a set never chosen in")
    }

    /// A set never chosen in, opened, takes its first brush as the one in use, lit (the original
    /// keeps one brush per set, the first until one is chosen); a tap on another of its brushes
    /// makes that the brush.
    #[test]
    fn a_set_never_chosen_in_opened_takes_its_first_brush_and_a_tap_takes_another() {
        let mut f = Finger::new();
        let start = f.session.current_brush();
        let set = never_chosen(&f, start.set);
        let row = f.library.set_rects.iter().find(|(s, _)| *s == set).map(|(_, r)| r.center()).unwrap();
        f.down(row);
        f.up(row);
        f.frame(Vec::new());
        assert_eq!((f.session.current_brush().set, f.session.current_brush().index), (set as u32, 0));
        assert_eq!(f.library.highlighted, Some((set as u32, 0)));
        let cell = f.library.brush_rects.iter().find(|(k, _)| k.1 == 1).map(|(_, r)| r.center()).unwrap();
        f.down(cell);
        f.up(cell);
        f.frame(Vec::new());
        assert_eq!((f.session.current_brush().set, f.session.current_brush().index), (set as u32, 1));
        assert!(f.library.studio.is_none());
    }

    /// The library opens on the current brush's set: a set with no brushes, looked at last time
    /// (the brush staying in another), isn't the one it opens on again.
    #[test]
    fn the_library_opens_on_the_current_brush_s_set() {
        let mut f = Finger::new();
        let empty = f.session.new_brush_set() as usize;
        f.library.library_changed();
        f.frame(Vec::new());
        f.frame(Vec::new());
        let start = f.session.current_brush();
        assert_ne!(start.set as usize, empty);
        let row = f.library.set_rects.iter().find(|(s, _)| *s == empty).map(|(_, r)| r.center()).unwrap();
        f.down(row);
        f.up(row);
        f.frame(Vec::new());
        assert_eq!(f.library.shown_set, Some(empty));
        assert!(f.library.brush_rects.is_empty());
        assert_eq!((f.session.current_brush().set, f.session.current_brush().index), (start.set, start.index));
        // shut, and opened again
        f.library.opens();
        f.frame(Vec::new());
        assert!(f.library.brush_rects.iter().all(|(k, _)| k.0 == start.set));
        assert_eq!(f.library.highlighted, Some((start.set, start.index)));
    }

    /// A brush dragged and held over another set opens it as a tap on it would (the original's
    /// set rows take a resting drag as their tap): the brush last chosen there becomes the one in
    /// use, lit; let go away from the library, nothing lands and that stays the brush.
    #[test]
    fn a_set_a_dragged_brush_opens_brings_back_its_brush_as_a_tap_does() {
        let mut f = Finger::new();
        let start = f.session.current_brush();
        let from = f.on_a_brush();
        let key = f.library.brush_rects.iter().find(|(_, r)| r.contains(from)).map(|(k, _)| *k).unwrap();
        // a set other than this one, on screen, its brush 2 chosen there before
        let (to, over) = f
            .library
            .set_rects
            .iter()
            .find(|(s, r)| *s != key.0 as usize && (0.0..650.0).contains(&r.center().y))
            .map(|(s, r)| (*s, r.center()))
            .unwrap();
        assert!(f.session.select_brush(to as u32, 2));
        assert!(f.session.select_brush(start.set, start.index));
        f.library.tool_changed();
        f.frame(Vec::new());
        let sizes: Vec<usize> = f.session.brush_library().iter().map(|s| s.brushes.len()).collect();
        f.down(from);
        std::thread::sleep(HOLD + Duration::from_millis(50));
        f.frame(Vec::new());
        for i in 1..=6 {
            f.to(from + (over - from) * (i as f32 / 6.0));
        }
        // not yet open: the brush as it was
        assert_eq!((f.session.current_brush().set, f.session.current_brush().index), (start.set, start.index));
        std::thread::sleep(SPRING_LOAD + Duration::from_millis(50));
        f.frame(Vec::new());
        f.frame(Vec::new());
        assert_eq!(f.library.shown_set, Some(to));
        assert_eq!((f.session.current_brush().set, f.session.current_brush().index), (to as u32, 2));
        assert_eq!(f.library.highlighted, Some((to as u32, 2)));
        // carried off the library and let go: nothing lands anywhere
        let away = pos2(40.0, 650.0);
        for i in 1..=4 {
            f.to(over + (away - over) * (i as f32 / 4.0));
        }
        f.up(away);
        f.frame(Vec::new());
        assert_eq!(f.session.brush_library().iter().map(|s| s.brushes.len()).collect::<Vec<_>>(), sizes);
        assert_eq!((f.session.current_brush().set, f.session.current_brush().index), (to as u32, 2));
    }

    /// A brush picked from the search ends it and shows in its own set, chosen there and lit (as
    /// the original), wherever the library stood.
    #[test]
    fn a_brush_picked_from_the_search_shows_in_its_own_set() {
        let mut f = Finger::new();
        let start = f.session.current_brush();
        let library = f.session.brush_library();
        let named = |name: &str| library.iter().flat_map(|s| &s.brushes).filter(|o| o.name == name).count();
        // a brush of another set, not its first, with a name no other brush holds
        let picked = library
            .iter()
            .flat_map(|s| &s.brushes)
            .find(|b| b.set != start.set && b.index > 0 && named(&b.name) == 1)
            .cloned()
            .expect("a brush of its own name");
        f.library.query = picked.name.clone();
        f.frame(Vec::new());
        f.frame(Vec::new());
        // the results in place of the lists: the one found, first
        assert!(f.library.brush_rects.is_empty());
        let (key, at) = f.library.result_rects.first().map(|(k, r)| (*k, r.center())).expect("a result");
        assert_eq!(key, (picked.set, picked.index));
        f.down(at);
        f.up(at);
        f.frame(Vec::new());
        f.frame(Vec::new());
        assert!(f.library.query.is_empty() && !f.library.searching);
        assert_eq!((f.session.current_brush().set, f.session.current_brush().index), (picked.set, picked.index));
        assert_eq!(f.library.shown_set, Some(picked.set as usize));
        assert_eq!(f.library.highlighted, Some((picked.set, picked.index)));
        assert!(f.library.brush_rects.iter().all(|(k, _)| k.0 == picked.set));
        assert!(f.library.studio.is_none());
    }

    #[test]
    fn a_brush_slid_sideways_or_tapped_leaves_the_list_where_it_is() {
        let mut f = Finger::new();
        let (start, from) = (f.top(), f.on_a_brush());
        f.down(from);
        let mut at = from;
        for k in 0..10 {
            at += vec2(-8.0, if k % 2 == 0 { 1.0 } else { -1.0 });
            f.to(at);
        }
        f.up(at);
        for _ in 0..5 {
            f.frame(Vec::new());
        }
        assert!((f.top() - start).abs() < 3.0, "the list moved {}", f.top() - start);
        assert!(f.library.slid.values().any(|s| *s < 0.0) || f.library.swiped.is_some(), "the brush didn't slide");
    }
}
