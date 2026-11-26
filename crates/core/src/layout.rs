//! Where the floating panels go: the Layers panel and the brush library torn off while a plugin on
//! the `floating_panels` hook lets them, and from interface 20 every other panel it lets float (the
//! Actions panel, the Adjustments menu, an adjustment's own controls, the Selection and Transform
//! toolbars and Liquify's panel); the colour panel dragged out by its grabber while a plugin on the
//! `panel_layout` hook lays the panels out (interface 20); and the Reference window, which the
//! others snap against and which snaps as it moves (its place is the artwork's, kept there).
//!
//! Windows hold up to [`MOST_TABS`] panels as tabs: one let go with its strip on another's (its top
//! middle within [`TAB_ZONE`] of that one's top) joins it, showing; a tab dragged out floats again.
//! The window is where its first panel's place is; each tab shows there at its own size.
//!
//! Each window can be folded to a tab, hidden (its button, or showing the hidden ones, brings it
//! back), pinned over the others, and put back where it first goes (Reset Position): the panels
//! from the right of the top bar against the window's right edge under it, those from its left
//! against the left edge, and the toolbars and an adjustment's controls at the bottom in the middle.
//!
//! The app holds one [`PanelLayout`], tells it the room the panels go in, how big they show and
//! where the pointer is, and draws each where [`PanelLayout::shown`] says. With the plugins on the
//! layout's hook, a panel moved or sized near the window's edges, the top bar's bottom or another
//! panel snaps to it edge to edge, with no gap; one docked against an edge stays against it as the
//! room changes; it folds to a tab; and those docked against the window's edges take the room they
//! cover from the drawing area. Each orientation keeps its own places, and each app its own.
//!
//! Everything here is in points as the window measures them, but the panels' sizes, which are in
//! their own points (shown at the floating windows' size, a plugin's, times them).

use std::sync::{Arc, Mutex, MutexGuard};

use crate::bars::{EdgeCollapse, WindowEdge, collapse_into, pulled_out};
use crate::floating::{FloatingPanel, FloatingRoom, FloatingWindow, floating_sizes};
use crate::plugins::Plugins;

/// How near an edge a panel's edge is to be against it (points): what a sum of fractions leaves.
const ON_EDGE: f32 = 0.5;
/// The farthest a plugin may have the panels snap from (points).
pub(crate) const MOST_SNAP: f32 = 64.0;
/// A folded panel's tab, in its own points: its name and an unfold mark.
pub(crate) const TAB_WIDTH: f32 = 148.0;
pub(crate) const TAB_HEIGHT: f32 = 36.0;
/// What the layout is kept under, with the interface's other settings.
const KEY: &str = "panel_layout";
/// The Tool Belt's chips, and its height, in its own points; and how far it is in from the room's
/// left and bottom edges (window points).
pub(crate) const BELT_CHIP: f32 = 120.0;
pub(crate) const BELT_HEIGHT: f32 = 36.0;
pub(crate) const BELT_GAP: f32 = 12.0;
/// The most panels a window holds as tabs.
pub(crate) const MOST_TABS: usize = 4;
/// How near a window's top middle comes to another's top (points), within its width, for it to
/// join it as a tab when let go.
pub(crate) const TAB_ZONE: f32 = 40.0;

/// A window holding several panels as tabs: the first holds the window's place; the one showing.
#[derive(Clone, Debug, PartialEq)]
struct Group {
    pieces: Vec<LayoutPiece>,
    active: LayoutPiece,
}

/// A piece of the interface the layout places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum LayoutPiece {
    /// The Layers panel floating.
    Layers,
    /// The brush library floating.
    Brushes,
    /// The colour panel floating.
    Colours,
    /// The Reference window (placed by the app, kept with the artwork).
    Reference,
    /// The Actions panel floating.
    Actions,
    /// The Adjustments menu floating.
    Adjustments,
    /// The controls of the adjustment being made floating (its panel or bar, Recolor's too).
    Adjusting,
    /// The Selection toolbar floating.
    Selection,
    /// The Transform toolbar floating.
    Transform,
    /// Liquify's panel floating.
    Liquify,
}

/// The pieces the layout keeps the places of, in the order they are kept (and drawn, until one is
/// pressed: it goes over the others).
const KEPT: [LayoutPiece; 9] = [
    LayoutPiece::Layers,
    LayoutPiece::Brushes,
    LayoutPiece::Colours,
    LayoutPiece::Actions,
    LayoutPiece::Adjustments,
    LayoutPiece::Adjusting,
    LayoutPiece::Selection,
    LayoutPiece::Transform,
    LayoutPiece::Liquify,
];
/// How many there are.
const PIECES: usize = KEPT.len();

/// Where a piece first goes, and goes back to by Reset Position: against the window's right edge
/// under the top bar, against its left edge, or at its bottom in the middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Home {
    Right,
    Left,
    Bottom,
}

impl LayoutPiece {
    /// Its place among those kept.
    fn kept_at(self) -> Option<usize> {
        KEPT.iter().position(|&p| p == self)
    }

    fn word(self) -> &'static str {
        match self {
            LayoutPiece::Layers => "layers",
            LayoutPiece::Brushes => "brushes",
            LayoutPiece::Colours => "colours",
            LayoutPiece::Reference => "reference",
            LayoutPiece::Actions => "actions",
            LayoutPiece::Adjustments => "adjustments",
            LayoutPiece::Adjusting => "adjusting",
            LayoutPiece::Selection => "selection",
            LayoutPiece::Transform => "transform",
            LayoutPiece::Liquify => "liquify",
        }
    }

    fn of_word(word: &str) -> Option<LayoutPiece> {
        KEPT.into_iter().find(|p| p.word() == word)
    }

    /// The panel the `floating_panels` hook asks about by its own name, for those it does.
    fn panel(self) -> Option<FloatingPanel> {
        match self {
            LayoutPiece::Layers => Some(FloatingPanel::Layers),
            LayoutPiece::Brushes => Some(FloatingPanel::Brushes),
            _ => None,
        }
    }

    /// Its number on the `floating_panels` hook: the Layers panel 0 and the brush library 1, and from
    /// interface 20 the Actions panel 2, the Adjustments menu 3, an adjustment's controls 4, the
    /// Selection toolbar 5, the Transform toolbar 6 and Liquify's panel 7. The colour panel floats
    /// by the app's own grabber, and the Reference window is the artwork's.
    fn hook_number(self) -> Option<i32> {
        match self {
            LayoutPiece::Layers => Some(0),
            LayoutPiece::Brushes => Some(1),
            LayoutPiece::Actions => Some(2),
            LayoutPiece::Adjustments => Some(3),
            LayoutPiece::Adjusting => Some(4),
            LayoutPiece::Selection => Some(5),
            LayoutPiece::Transform => Some(6),
            LayoutPiece::Liquify => Some(7),
            LayoutPiece::Colours | LayoutPiece::Reference => None,
        }
    }

    /// Whether it may be a tab of a window holding others: the panels, not the colour panel (whose
    /// window is its own), a toolbar or an adjustment's controls.
    fn tabbed(self) -> bool {
        matches!(self, LayoutPiece::Layers | LayoutPiece::Brushes | LayoutPiece::Actions | LayoutPiece::Adjustments)
    }

    fn home(self) -> Home {
        match self {
            LayoutPiece::Actions | LayoutPiece::Adjustments => Home::Left,
            LayoutPiece::Adjusting | LayoutPiece::Selection | LayoutPiece::Transform | LayoutPiece::Liquify => {
                Home::Bottom
            }
            LayoutPiece::Layers | LayoutPiece::Brushes | LayoutPiece::Colours | LayoutPiece::Reference => Home::Right,
        }
    }
}

/// How big a piece is, in its own points: as wide and tall as it first floats, the least and most
/// it may be sized to (a piece that isn't sized is as big as the app says it is, see
/// [`PanelLayout::set_size`]), and whether it has a corner to size it by.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct PieceSizes {
    pub width: f32,
    pub least_width: f32,
    pub most_width: f32,
    pub height: f32,
    pub least_height: f32,
    pub sizable: bool,
}

/// How big `piece` floats at first and may be sized to (see [`PieceSizes`]): the Layers panel and the
/// brush library as [`crate::floating_sizes`] says, the Actions panel 390 wide and sized only in
/// height; the rest as big as they hang, as the app tells the layout.
#[uniffi::export]
pub fn piece_sizes(piece: LayoutPiece) -> PieceSizes {
    let told = |width: f32, height: f32| PieceSizes {
        width,
        least_width: width,
        most_width: width,
        height,
        least_height: height,
        sizable: false,
    };
    match piece.panel() {
        Some(panel) => {
            let s = floating_sizes(panel);
            PieceSizes {
                width: s.width,
                least_width: s.least_width,
                most_width: s.most_width,
                height: s.height,
                least_height: s.least_height,
                sizable: true,
            }
        }
        None => match piece {
            LayoutPiece::Actions => PieceSizes {
                width: 390.0,
                least_width: 390.0,
                most_width: 390.0,
                height: 600.0,
                least_height: 300.0,
                sizable: true,
            },
            LayoutPiece::Colours => told(260.0, 351.0),
            LayoutPiece::Adjustments => told(330.0, 480.0),
            LayoutPiece::Adjusting => told(600.0, 200.0),
            LayoutPiece::Selection | LayoutPiece::Transform => told(560.0, 64.0),
            LayoutPiece::Liquify => told(700.0, 200.0),
            _ => told(TAB_WIDTH, TAB_HEIGHT),
        },
    }
}

/// A rectangle in the window, in points as it measures them.
#[derive(Clone, Copy, Debug, Default, PartialEq, uniffi::Record)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    pub(crate) fn right(&self) -> f32 {
        self.x + self.width
    }

    pub(crate) fn bottom(&self) -> f32 {
        self.y + self.height
    }

    fn finite(&self) -> bool {
        [self.x, self.y, self.width, self.height].iter().all(|v| v.is_finite())
    }
}

/// Which edges of the room a piece is against: the window's, clear of the side bar (the top bar's
/// bottom counts as the top).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct LayoutDock {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl LayoutDock {
    fn letters(self) -> String {
        let letters: String = [(self.left, 'l'), (self.top, 't'), (self.right, 'r'), (self.bottom, 'b')]
            .into_iter()
            .filter_map(|(on, c)| on.then_some(c))
            .collect();
        if letters.is_empty() { "-".to_owned() } else { letters }
    }

    fn of_letters(letters: &str) -> LayoutDock {
        LayoutDock {
            left: letters.contains('l'),
            top: letters.contains('t'),
            right: letters.contains('r'),
            bottom: letters.contains('b'),
        }
    }

    fn side(self) -> bool {
        self.left || self.right
    }
}

/// What the drawing area gives up on each side of the window to the panels docked there (points).
#[derive(Clone, Copy, Debug, Default, PartialEq, uniffi::Record)]
pub struct LayoutInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// How the panels are laid out, as the plugins on the `panel_layout` hook say (interface 20): with
/// none on, as the app always had them.
#[derive(Clone, Copy, Debug, Default, PartialEq, uniffi::Record)]
pub struct PanelLayoutLook {
    /// A plugin is on the hook: the colour panel floating goes where the layout keeps it too.
    pub on: bool,
    /// How near (points) a panel comes to an edge or another panel before it snaps to it; 0 for
    /// no snapping.
    pub snap: f32,
    /// Each floating panel has a fold button, folding it to a tab.
    pub fold: bool,
    /// The panels docked against the window's edges take that room from the drawing area.
    pub make_room: bool,
    /// The interface's bars may be moved: Edit Layout in the Canvas tab, and the bars where it left
    /// them (see [`crate::BarLayout`]).
    pub edit: bool,
    /// The Tool Belt shows: floating panels let go on it are parked there as chips (see
    /// [`PanelLayout::belt_shown`]).
    pub belt: bool,
    /// Collapse to edge: a bar or a window against a side of the window has a button sliding it into
    /// that side, a tab left there bringing it back (see [`PanelLayout::piece_collapsed`] and
    /// [`crate::BarLayout::collapsed`]).
    pub collapse: bool,
}

/// The Tool Belt as it shows: its top-left in the window, its size in its own points (shown `scale`
/// times as big), and the panels parked on it, in order, each a chip [`BELT_CHIP`] wide.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct BeltShown {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub pieces: Vec<LayoutPiece>,
}

/// A piece as it shows: its top-left in the window (points as it measures them), its size in its own
/// points (shown `scale` times as big; a folded panel's is its tab's), whether it is folded to its
/// tab, and which edges of the room it is against.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct PieceShown {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub folded: bool,
    pub dock: LayoutDock,
}

impl PieceShown {
    /// What it covers in the window.
    fn rect(&self) -> LayoutRect {
        LayoutRect { x: self.x, y: self.y, width: self.width * self.scale, height: self.height * self.scale }
    }
}

/// A kept piece's place in one orientation: its top-left in the window, its size in its own points,
/// whether it is folded, and the edges it was left against. Kept with its top-left as a share of the
/// window's width and height, so a window sized differently (on a desktop, the screen split) has it
/// in the same place in proportion; used in window points.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spot {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    folded: bool,
    dock: LayoutDock,
    /// Collapsed into the side it is docked to (Collapse to edge).
    collapsed: bool,
}

/// A piece being moved by its strip (or its tab), or sized from its corner: which, where the
/// pointer was when it was pressed, and the piece then.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    piece: LayoutPiece,
    sizing: bool,
    from: (f32, f32),
    start: PieceShown,
    /// It was moved (torn off, or dragged since it was pressed): let go on another's strip, a tab.
    moved: bool,
    /// Where the pointer is now (window points).
    at: (f32, f32),
}

#[derive(Debug)]
struct Inner {
    look: PanelLayoutLook,
    /// Which panels the plugins let float (by their place among those kept: the colour panel always
    /// may), as they said at their generation.
    floats: [bool; PIECES],
    told: Option<u64>,
    room: FloatingRoom,
    window: (f32, f32),
    bar: f32,
    scale: f32,
    /// Each kept piece is open (its window shows), whatever the orientation.
    open: [Option<bool>; PIECES],
    /// Each kept piece's place, portrait and landscape.
    spots: [[Option<Spot>; PIECES]; 2],
    /// Each kept piece hidden (open, but not shown until shown again), and pinned over the others.
    hidden: [bool; PIECES],
    pinned: [bool; PIECES],
    /// The pieces from the bottom up: the last pressed goes on top (the pinned over all of them).
    stack: Vec<LayoutPiece>,
    /// The windows holding panels as tabs.
    groups: Vec<Group>,
    /// The panels parked on the Tool Belt, in order; and where it shows, as the app says (moved as a
    /// bar, see [`crate::BarLayout`]), else at the room's bottom left.
    belt: Vec<LayoutPiece>,
    belt_place: Option<LayoutRect>,
    /// The Layers panel's and the brush library's windows as an app kept them before there were
    /// layouts (one for both orientations): what an orientation with nothing kept starts from.
    old: [Option<FloatingWindow>; 2],
    /// Each piece's size in its own points as the app says it is (the colour panel's as its mode
    /// has it, a toolbar's as its buttons have it): for the pieces that aren't sized.
    sizes: [(f32, f32); PIECES],
    reference: Option<LayoutRect>,
    held: Option<Held>,
    /// A tab torn out of its window and held still: it, and the tabs its window had. Until it is let
    /// go the strips go on showing those tabs, so that what drags it (the tab pressed, in an app
    /// that lays the strip out anew) is still there to the end of the drag.
    tearing: Option<(LayoutPiece, Vec<LayoutPiece>)>,
    /// A chip dragged off the Tool Belt, still held: it, and where it was among the chips. Until it is
    /// let go the belt goes on showing it there, so that what drags it (the chip pressed, in an app
    /// that lays the belt out anew) is still there to the end of the drag.
    unparking: Option<(LayoutPiece, usize)>,
}

impl Default for Inner {
    fn default() -> Inner {
        Inner {
            look: PanelLayoutLook::default(),
            floats: KEPT.map(|p| p == LayoutPiece::Colours),
            told: None,
            room: FloatingRoom { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 },
            window: (1.0, 1.0),
            bar: 0.0,
            scale: 1.0,
            open: [None; PIECES],
            spots: [[None; PIECES]; 2],
            hidden: [false; PIECES],
            pinned: [false; PIECES],
            stack: KEPT.to_vec(),
            groups: Vec::new(),
            belt: Vec::new(),
            belt_place: None,
            old: [None; 2],
            sizes: KEPT.map(|p| {
                let s = piece_sizes(p);
                (s.width, s.height)
            }),
            reference: None,
            held: None,
            tearing: None,
            unparking: None,
        }
    }
}

/// The floating panels' places and what is being done to them; see the module's words.
#[derive(Debug, Default, uniffi::Object)]
pub struct PanelLayout {
    inner: Mutex<Inner>,
}

/// What the layout is kept under, with the interface's other settings.
#[uniffi::export]
pub fn panel_layout_key() -> String {
    KEY.to_owned()
}

#[uniffi::export]
impl PanelLayout {
    #[uniffi::constructor]
    pub fn new() -> Arc<PanelLayout> {
        Arc::new(PanelLayout::default())
    }

    /// A setting kept with the interface's: the layout's own (see [`panel_layout_key`]), and the
    /// Layers panel's and the brush library's windows as they were kept before
    /// ([`crate::floating_key`]), which an orientation with nothing kept of its own starts from.
    /// Anything else, or what doesn't read, is passed over.
    pub fn read_kept(&self, key: String, value: String) {
        let mut inner = self.lock();
        if key == KEY {
            inner.read(&value);
            return;
        }
        for (at, panel) in [FloatingPanel::Layers, FloatingPanel::Brushes].into_iter().enumerate() {
            if key == crate::floating_key(panel)
                && let Some(slot) = inner.old.get_mut(at)
            {
                *slot = crate::floating_read(value.clone());
            }
        }
    }

    /// The layout as it is kept under [`panel_layout_key`]: each kept piece, whether it is open, and
    /// its place in each orientation it has one in.
    pub fn kept(&self) -> String {
        self.lock().kept()
    }

    /// What the plugins say now: which panels float, and how the layout goes (asked again only as
    /// they change).
    pub fn follow(&self, plugins: Arc<Plugins>) {
        let now = plugins.generation();
        let mut inner = self.lock();
        if inner.told != Some(now) {
            inner.told = Some(now);
            inner.look = plugins.panel_layout();
            inner.floats = KEPT.map(|p| p.hook_number().is_none_or(|n| plugins.floats_number(n)));
        }
    }

    /// The look as told (for an app's tests and for what is drawn without asking the plugins).
    pub fn look(&self) -> PanelLayoutLook {
        self.lock().look
    }

    /// The room the panels go in (the window clear of the side bar), the window's size and the top
    /// bar's bottom (points as the window measures them), and the floating panels' size. The window
    /// wider than tall is landscape: each way has its own places. A piece held goes as the room
    /// turns.
    pub fn set_room(&self, room: FloatingRoom, width: f32, height: f32, bar: f32, scale: f32) {
        let mut inner = self.lock();
        let was = inner.landscape();
        inner.room = room;
        inner.window = (if width.is_finite() { width } else { 0.0 }, if height.is_finite() { height } else { 0.0 });
        inner.bar = if bar.is_finite() { bar } else { 0.0 };
        inner.scale = shown_scale(scale);
        if inner.landscape() != was {
            inner.held = None;
        }
    }

    /// The colour panel's size floating, in its own points, as its mode has it.
    pub fn set_colours_size(&self, width: f32, height: f32) {
        self.set_size(LayoutPiece::Colours, width, height);
    }

    /// How big `piece` is in its own points, as the app lays it out now: for a piece that isn't
    /// sized (see [`piece_sizes`]), what it shows at. A size that isn't one is passed over.
    pub fn set_size(&self, piece: LayoutPiece, width: f32, height: f32) {
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return;
        }
        let mut inner = self.lock();
        if let Some(size) = piece.kept_at().and_then(|at| inner.sizes.get_mut(at)) {
            *size = (width, height);
        }
    }

    /// Whether the plugins let `piece` float now: the colour panel while one lays the panels out, the
    /// others while one on the `floating_panels` hook lets them. Where none does, it hangs where it
    /// always does.
    pub fn may_float(&self, piece: LayoutPiece) -> bool {
        self.lock().may(piece)
    }

    /// `piece`'s window hidden (it stays open, unseen, until shown again, or its button is pressed),
    /// or shown again.
    pub fn set_hidden(&self, piece: LayoutPiece, hidden: bool) {
        let mut inner = self.lock();
        let owner = inner.owner(piece);
        if let Some(slot) = owner.kept_at().and_then(|at| inner.hidden.get_mut(at)) {
            *slot = hidden;
        }
        if hidden && inner.held.is_some_and(|h| h.piece == piece) {
            inner.held = None;
        }
    }

    /// Whether `piece`'s window is hidden.
    pub fn is_hidden(&self, piece: LayoutPiece) -> bool {
        self.lock().is_hidden(piece)
    }

    /// The windows hidden now that would show: what showing the hidden windows brings back.
    pub fn hidden_pieces(&self) -> Vec<LayoutPiece> {
        let inner = self.lock();
        KEPT.into_iter().filter(|&p| inner.is_hidden(p) && inner.may(p) && inner.is_open(p)).collect()
    }

    /// Every hidden window shown again.
    pub fn show_hidden(&self) {
        self.lock().hidden = [false; PIECES];
    }

    /// `piece`'s window pinned over the others (or not): drawn over every window that isn't, however
    /// they are pressed.
    pub fn set_pinned(&self, piece: LayoutPiece, pinned: bool) {
        let mut inner = self.lock();
        let owner = inner.owner(piece);
        if let Some(slot) = owner.kept_at().and_then(|at| inner.pinned.get_mut(at)) {
            *slot = pinned;
        }
    }

    /// Whether `piece`'s window is pinned over the others.
    pub fn is_pinned(&self, piece: LayoutPiece) -> bool {
        self.lock().is_pinned(piece)
    }

    /// `piece` goes over the other windows (pressed, it does so by itself).
    pub fn raise(&self, piece: LayoutPiece) {
        self.lock().raise(piece);
    }

    /// The windows showing, in the order they are drawn, from the bottom up: as they were last
    /// pressed, those pinned over the rest.
    pub fn order(&self) -> Vec<LayoutPiece> {
        let inner = self.lock();
        let mut showing: Vec<LayoutPiece> = inner.stack.iter().copied().filter(|&p| inner.shown(p).is_some()).collect();
        showing.sort_by_key(|&p| inner.is_pinned(p));
        showing
    }

    /// `piece` put back where it first goes in this orientation, unfolded and as big as it first is
    /// (see the module's words): where it shows now, whether or not it is open.
    pub fn reset_place(&self, piece: LayoutPiece) -> Option<PieceShown> {
        let mut inner = self.lock();
        piece.kept_at()?;
        // a tab goes back as a window of its own, and one parked comes off the belt
        inner.untab(piece);
        inner.belt.retain(|&p| p != piece);
        let sizes = inner.sizes_of(piece);
        let k = inner.scale;
        let (width, height) = (sizes.width * k, sizes.height * k);
        let room = inner.room;
        let top = if inner.bar > room.top { inner.bar } else { room.top };
        let (x, y) = match piece.home() {
            Home::Right => (room.right - width, top),
            Home::Left => (room.left, top),
            Home::Bottom => ((room.left + room.right - width) / 2.0, room.bottom - height),
        };
        let rect = inside_room(LayoutRect { x, y, width, height }, room);
        let spot = inner.place(piece, rect, false);
        inner.set_spot(piece, spot);
        if inner.held.is_some_and(|h| h.piece == piece) {
            inner.held = None;
        }
        Some(inner.shown_spot(piece, spot))
    }

    /// Where the Reference window shows now (window points), or none while it doesn't.
    pub fn set_reference(&self, rect: Option<LayoutRect>) {
        self.lock().reference = rect.filter(LayoutRect::finite);
    }

    /// Where `piece` shows, while it does: the Layers panel and the brush library open while a plugin
    /// lets them float, the colour panel open while one is on the layout's hook, inside the room as
    /// it is now (it may have shrunk since the piece was left: what is docked stays against its edge).
    /// The Reference window is the app's to show.
    pub fn shown(&self, piece: LayoutPiece) -> Option<PieceShown> {
        self.lock().shown(piece)
    }

    /// Whether `piece` is kept open, whether or not it may show now.
    pub fn is_open(&self, piece: LayoutPiece) -> bool {
        self.lock().is_open(piece)
    }

    /// `piece` torn off with the pointer at (`x`, `y`): it opens, as big as it was last left (or as
    /// it hangs), the middle of its grabber, `grab` of its own points down its top, under the pointer,
    /// and follows it until let go.
    pub fn open_at(&self, piece: LayoutPiece, x: f32, y: f32, grab: f32) -> Option<PieceShown> {
        self.lock().opened_at(piece, x, y, grab)
    }

    /// The panels `piece`'s window holds as tabs, the first holding its place; none while it holds
    /// it alone. While a tab torn out is still held, its window's tabs as they were, on that window's
    /// strip and on the torn one's alike.
    pub fn tabs_of(&self, piece: LayoutPiece) -> Vec<LayoutPiece> {
        let inner = self.lock();
        if let Some((torn, before)) = &inner.tearing
            && inner.held.is_some_and(|h| h.piece == *torn)
            && before.contains(&piece)
        {
            return before.clone();
        }
        inner.group_of(piece).map(|g| g.pieces.clone()).unwrap_or_default()
    }

    /// The Tool Belt as it shows, while a plugin says so: at the bottom left of the room, a chip for
    /// each panel parked on it (as wide as one while none is).
    /// (Where it always is, in the room as it is now, however the app moved it as a bar: the app
    /// shows it moved, and tells where with [`PanelLayout::set_belt_place`].)
    pub fn belt_shown(&self) -> Option<BeltShown> {
        let inner = self.lock();
        let r = inner.belt_home()?;
        Some(BeltShown {
            x: r.x,
            y: r.y,
            width: r.width / inner.scale,
            height: r.height / inner.scale,
            scale: inner.scale,
            pieces: inner.belt_chips(),
        })
    }

    /// Where the Tool Belt shows now (window points), moved where Edit layout left it, or none for
    /// the room's bottom left: what a panel is let go on to park it.
    pub fn set_belt_place(&self, rect: Option<LayoutRect>) {
        self.lock().belt_place = rect.filter(LayoutRect::finite);
    }

    /// `piece`, parked on the Tool Belt, taken off: it shows where it was before it was parked.
    pub fn unpark(&self, piece: LayoutPiece) -> Option<PieceShown> {
        let mut inner = self.lock();
        if !inner.belt.contains(&piece) {
            return None;
        }
        inner.belt.retain(|&p| p != piece);
        inner.raise(piece);
        inner.shown(piece)
    }

    /// `piece`, parked on the Tool Belt, dragged off with the pointer at (`x`, `y`): a window under
    /// the pointer, following it as one torn off does (see [`PanelLayout::open_at`]).
    pub fn unpark_at(&self, piece: LayoutPiece, x: f32, y: f32, grab: f32) -> Option<PieceShown> {
        let mut inner = self.lock();
        let at = inner.belt.iter().position(|&p| p == piece);
        inner.belt.retain(|&p| p != piece);
        let shown = inner.opened_at(piece, x, y, grab);
        inner.unparking = at.filter(|_| shown.is_some()).map(|at| (piece, at));
        shown
    }

    /// `piece`, a tab of its window, shown there (the others' tabs hide).
    pub fn set_active_tab(&self, piece: LayoutPiece) {
        let mut inner = self.lock();
        if let Some(group) = inner.groups.iter_mut().find(|g| g.pieces.contains(&piece)) {
            group.active = piece;
            inner.raise(piece);
        }
    }

    /// `piece`, a tab of its window, torn out with the pointer at (`x`, `y`): a window of its own,
    /// following the pointer as one torn off does (see [`PanelLayout::open_at`]). The window keeps
    /// its other tabs (one left alone is a window as any other).
    pub fn tear_tab(&self, piece: LayoutPiece, x: f32, y: f32, grab: f32) -> Option<PieceShown> {
        let mut inner = self.lock();
        let before = inner.group_of(piece).map(|g| g.pieces.clone());
        inner.untab(piece);
        let shown = inner.opened_at(piece, x, y, grab);
        inner.tearing = before.filter(|_| shown.is_some()).map(|tabs| (piece, tabs));
        shown
    }
}

impl Inner {
    /// See [`PanelLayout::open_at`].
    fn opened_at(&mut self, piece: LayoutPiece, x: f32, y: f32, grab: f32) -> Option<PieceShown> {
        let inner = self;
        let at = piece.kept_at()?;
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        let k = inner.scale;
        let top = y - if grab.is_finite() { grab } else { 0.0 } * k;
        let sizes = inner.sizes_of(piece);
        let (width, height) = match inner.spot(piece) {
            Some(s) if sizes.sizable => (s.width, s.height),
            _ => (sizes.width, sizes.height),
        };
        let rect = LayoutRect { x: x - width * k / 2.0, y: top, width: width * k, height: height * k };
        let spot = inner.place(piece, rect, false);
        inner.set_spot(piece, spot);
        if let Some(open) = inner.open.get_mut(at) {
            *open = Some(true);
        }
        if let Some(hidden) = inner.hidden.get_mut(at) {
            *hidden = false;
        }
        inner.raise(piece);
        let shown = inner.shown_spot(piece, spot);
        // (dragged from where it was meant to be: kept inside the room at first, it comes back under
        // the finger as the finger moves on)
        let meant = PieceShown { x: rect.x, y: rect.y, ..shown };
        inner.held = Some(Held { piece, sizing: false, from: (x, y), start: meant, moved: true, at: (x, y) });
        Some(shown)
    }

    /// The Tool Belt where it shows (window points), while a plugin says so.
    fn belt_rect(&self) -> Option<LayoutRect> {
        if !self.look.belt {
            return None;
        }
        self.belt_place.or_else(|| self.belt_home())
    }

    /// Where the Tool Belt always is (window points): the room's bottom left as it is now (turned,
    /// the new one's), as long as its chips (one while none is).
    fn belt_home(&self) -> Option<LayoutRect> {
        if !self.look.belt {
            return None;
        }
        let k = self.scale;
        let chips = self.belt_chips().len().max(1) as f32;
        let (width, height) = (chips * BELT_CHIP * k, BELT_HEIGHT * k);
        Some(LayoutRect { x: self.room.left + BELT_GAP, y: self.room.bottom - BELT_GAP - height, width, height })
    }

    /// The chips on the Tool Belt: those parked, and one dragged off still held, where it was.
    fn belt_chips(&self) -> Vec<LayoutPiece> {
        let mut chips = self.belt.clone();
        if let Some((piece, at)) = self.unparking
            && self.held.is_some_and(|h| h.piece == piece)
            && !chips.contains(&piece)
        {
            chips.insert(at.min(chips.len()), piece);
        }
        chips
    }

    /// `piece`, just let go with the pointer at `at`, parked on the Tool Belt if the pointer was on
    /// it (within [`TAB_ZONE`]): back where it was when it was pressed, for when it is taken off.
    /// Whether it was.
    fn park(&mut self, piece: LayoutPiece, start: PieceShown, at: (f32, f32)) -> bool {
        if !piece.tabbed() || self.belt.contains(&piece) {
            return false;
        }
        let Some(belt) = self.belt_rect() else { return false };
        let (x, y) = at;
        let on = x >= belt.x - TAB_ZONE
            && x <= belt.right() + TAB_ZONE
            && y >= belt.y - TAB_ZONE
            && y <= belt.bottom() + TAB_ZONE;
        if !on {
            return false;
        }
        // a window of tabs parks whole, each of its tabs a chip (in its order), each kept where the
        // window was for when it is taken off
        let pieces = self.group_of(piece).map_or_else(|| vec![piece], |g| g.pieces.clone());
        self.groups.retain(|g| !g.pieces.contains(&piece));
        for p in pieces {
            let spot = self.place(p, start.rect(), start.folded);
            self.set_spot(p, spot);
            if !self.belt.contains(&p) {
                self.belt.push(p);
            }
        }
        true
    }

    /// The window holding `piece` as a tab, if one does.
    fn group_of(&self, piece: LayoutPiece) -> Option<&Group> {
        self.groups.iter().find(|g| g.pieces.contains(&piece))
    }

    /// The piece whose place, hiding and pinning `piece`'s window has: its window's first tab, or
    /// itself.
    fn owner(&self, piece: LayoutPiece) -> LayoutPiece {
        self.group_of(piece).and_then(|g| g.pieces.first().copied()).unwrap_or(piece)
    }

    /// `piece` taken out of the window it is a tab of (if it is one): the next tab shows; the first
    /// gone, the next holds the window's place; one left alone is a window as any other.
    fn untab(&mut self, piece: LayoutPiece) {
        let Some(at) = self.groups.iter().position(|g| g.pieces.contains(&piece)) else { return };
        let Some(group) = self.groups.get_mut(at) else { return };
        let was_first = group.pieces.first() == Some(&piece);
        group.pieces.retain(|&p| p != piece);
        if group.active == piece
            && let Some(&next) = group.pieces.first()
        {
            group.active = next;
        }
        let (next, alone) = (group.pieces.first().copied(), group.pieces.len() < 2);
        // the window's place goes to its next first tab, in each orientation, as are its hiding
        // and pinning
        if was_first
            && let Some(next) = next
            && let (Some(from), Some(to)) = (piece.kept_at(), next.kept_at())
        {
            for way in 0..2 {
                let spot = self.spots.get(way).and_then(|s| s.get(from)).copied().flatten();
                if let Some(slot) = self.spots.get_mut(way).and_then(|s| s.get_mut(to)) {
                    *slot = spot;
                }
            }
            let (hidden, pinned) = (self.hidden.get(from).copied(), self.pinned.get(from).copied());
            if let (Some(slot), Some(v)) = (self.hidden.get_mut(to), hidden) {
                *slot = v;
            }
            if let (Some(slot), Some(v)) = (self.pinned.get_mut(to), pinned) {
                *slot = v;
            }
        }
        if alone {
            self.groups.remove(at);
        }
    }

    /// `piece`, just let go with the pointer at `pointer`, joining the window whose strip it was let
    /// go on as a tab (see [`TAB_ZONE`]): its own strip there, or the pointer (a tall window kept
    /// inside the room can't bring its strip down to a strip low in the window, but the pointer
    /// can). The window's tabs and its own (if it held some), at most [`MOST_TABS`].
    fn tab_in(&mut self, piece: LayoutPiece, pointer: (f32, f32)) {
        if !piece.tabbed() {
            return;
        }
        let Some(moved) = self.shown(piece).filter(|s| !s.folded) else { return };
        let r = moved.rect();
        let middle = r.x + r.width / 2.0;
        let mine: Vec<LayoutPiece> = self.group_of(piece).map_or_else(|| vec![piece], |g| g.pieces.clone());
        let (px, py) = pointer;
        let target = self.stack.iter().rev().copied().find(|&o| {
            !mine.contains(&o)
                && o.tabbed()
                && self.shown(o).is_some_and(|s| {
                    let w = s.rect();
                    let strip = middle >= w.x && middle <= w.right() && (r.y - w.y).abs() <= TAB_ZONE;
                    let pointed = px >= w.x && px <= w.right() && (py - w.y).abs() <= TAB_ZONE;
                    !s.folded && (strip || pointed)
                })
        });
        let Some(target) = target else { return };
        let theirs: Vec<LayoutPiece> = self.group_of(target).map_or_else(|| vec![target], |g| g.pieces.clone());
        if theirs.len() + mine.len() > MOST_TABS {
            return;
        }
        self.groups.retain(|g| !g.pieces.contains(&piece) && !g.pieces.contains(&target));
        let pieces: Vec<LayoutPiece> = theirs.into_iter().chain(mine).collect();
        self.groups.push(Group { pieces, active: piece });
        self.raise(piece);
    }
}

#[uniffi::export]
impl PanelLayout {
    /// `piece`'s strip or tab (or, `sizing`, its corner) pressed with the pointer at (`x`, `y`): it
    /// moves (or is sized) with the pointer until let go. The Reference window is held as the app
    /// last said it shows. False for a piece that doesn't show.
    pub fn hold(&self, piece: LayoutPiece, sizing: bool, x: f32, y: f32) -> bool {
        let mut inner = self.lock();
        let start = match piece {
            LayoutPiece::Reference => inner.reference.map(|r| PieceShown {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
                scale: 1.0,
                folded: false,
                dock: LayoutDock::default(),
            }),
            _ => {
                // (collapsed, it comes back out to be moved)
                if let Some(spot) = inner.spot(piece).filter(|s| s.collapsed) {
                    inner.set_spot(piece, Spot { collapsed: false, ..spot });
                }
                inner.shown(piece)
            }
        };
        let Some(start) = start.filter(|_| x.is_finite() && y.is_finite()) else { return false };
        // a piece without a corner and a folded one aren't sized
        let sizing = sizing && piece.kept_at().is_some() && inner.sizes_of(piece).sizable && !start.folded;
        inner.held = Some(Held { piece, sizing, from: (x, y), start, moved: false, at: (x, y) });
        inner.raise(piece);
        true
    }

    /// The pointer moved to (`x`, `y`) with a piece held: where that piece shows now, moved or sized
    /// from where it was pressed, kept inside the room and snapped as the plugins say. None with
    /// nothing held.
    pub fn drag(&self, x: f32, y: f32) -> Option<PieceShown> {
        let mut inner = self.lock();
        let held = inner.held?;
        if !(x.is_finite() && y.is_finite()) {
            return Some(held.start);
        }
        if let Some(h) = inner.held.as_mut() {
            h.moved |= (x, y) != h.from;
            h.at = (x, y);
        }
        Some(inner.dragged(held, x - held.from.0, y - held.from.1))
    }

    /// The piece held let go. True when there is something new to keep ([`PanelLayout::kept`]):
    /// a kept piece was held (the Reference window is kept with the artwork).
    pub fn let_go(&self) -> bool {
        let mut inner = self.lock();
        inner.tearing = None;
        inner.unparking = None;
        let Some(held) = inner.held.take() else { return false };
        if held.piece.kept_at().is_none() {
            return false;
        }
        // moved and let go on the Tool Belt: parked there; on another window's strip: a tab of it
        if !held.sizing && held.moved && !inner.park(held.piece, held.start, held.at) {
            inner.tab_in(held.piece, held.at);
        }
        true
    }

    /// Whether a piece is held.
    pub fn holding(&self) -> Option<LayoutPiece> {
        self.lock().held.map(|h| h.piece)
    }

    /// `piece` closed: kept where it was for when it opens again (shown, if it was hidden). A tab
    /// closes alone, the window's next tab showing.
    pub fn close(&self, piece: LayoutPiece) {
        let mut inner = self.lock();
        inner.untab(piece);
        inner.belt.retain(|&p| p != piece);
        if let Some(open) = piece.kept_at().and_then(|at| inner.open.get_mut(at)) {
            *open = Some(false);
        }
        if let Some(hidden) = piece.kept_at().and_then(|at| inner.hidden.get_mut(at)) {
            *hidden = false;
        }
        if inner.held.is_some_and(|h| h.piece == piece) {
            inner.held = None;
        }
    }

    /// `piece` folded to its tab, or unfolded, in this orientation (while the plugins let panels
    /// fold): its tab stays where its top-left (its top-right, against the right edge) was.
    pub fn set_folded(&self, piece: LayoutPiece, folded: bool) {
        let mut inner = self.lock();
        if !inner.look.fold && folded {
            return;
        }
        if let Some(spot) = inner.spot(piece) {
            inner.set_spot(piece, Spot { folded, ..spot });
        }
    }

    /// The side of the window `piece`'s window goes into when collapsed: the left or the right side it
    /// is docked to (not folded); none away from both, or with Collapse to edge off.
    pub fn piece_collapse_edge(&self, piece: LayoutPiece) -> Option<WindowEdge> {
        let inner = self.lock();
        inner.look.collapse.then(|| inner.window_edge(piece)).flatten()
    }

    /// `piece`'s window collapsed into its side (see [`PanelLayout::piece_collapse_edge`]), or brought
    /// back out, in this orientation (a window of tabs goes as one): true when that changed something
    /// to keep.
    pub fn set_piece_collapsed(&self, piece: LayoutPiece, collapsed: bool) -> bool {
        let mut inner = self.lock();
        inner.collapse(piece, collapsed)
    }

    /// `piece`'s window collapsed into its side, while Collapse to edge is on: the side, its tab on
    /// the room's edge there, and how far it slid to go in (from where [`PanelLayout::shown`] has
    /// it); none while it shows.
    pub fn piece_collapsed(&self, piece: LayoutPiece) -> Option<EdgeCollapse> {
        let inner = self.lock();
        if !inner.gone(piece) {
            return None;
        }
        let edge = inner.window_edge(piece)?;
        let rect = inner.shown(piece)?.rect();
        Some(collapse_into(edge, rect, inner.room, inner.window))
    }

    /// Where `piece`'s window goes when it collapses (as [`PanelLayout::piece_collapsed`] would say),
    /// collapsed or not: for an app sliding it in and back out. None while it can't collapse.
    pub fn piece_collapse_place(&self, piece: LayoutPiece) -> Option<EdgeCollapse> {
        let inner = self.lock();
        let edge = inner.look.collapse.then(|| inner.window_edge(piece)).flatten()?;
        let rect = inner.shown(piece)?.rect();
        Some(collapse_into(edge, rect, inner.room, inner.window))
    }

    /// A drag from collapsed `piece`'s tab, (`dx`, `dy`) so far: once it comes out of the side far
    /// enough, the window comes back out. True when it did.
    pub fn pull_piece_tab(&self, piece: LayoutPiece, dx: f32, dy: f32) -> bool {
        let mut inner = self.lock();
        let Some(edge) = inner.gone(piece).then(|| inner.window_edge(piece)).flatten() else { return false };
        pulled_out(edge, dx, dy) && inner.collapse(piece, false)
    }

    /// Every window against a side collapsed into it (`collapsed`), or every one brought back out:
    /// true when that changed something to keep.
    pub fn set_all_collapsed(&self, collapsed: bool) -> bool {
        let mut inner = self.lock();
        let mut changed = false;
        for piece in KEPT {
            changed |= inner.collapse(piece, collapsed);
        }
        changed
    }

    /// Whether any window is collapsed into its side now.
    pub fn any_collapsed(&self) -> bool {
        let inner = self.lock();
        KEPT.into_iter().any(|p| inner.gone(p))
    }

    /// What the panels docked against the window's edges take from the drawing area, while the
    /// plugins say they make room: none else. A panel against the left or right edge, and those beside
    /// it in a row from there, take the room from that edge to their far side; one against the top
    /// (under the top bar) or the bottom alone, from there to its far side. A folded panel takes none.
    pub fn canvas_insets(&self) -> LayoutInsets {
        let inner = self.lock();
        if !inner.look.make_room {
            return LayoutInsets::default();
        }
        let shown: Vec<LayoutRect> = inner.shown_rects(None, false);
        insets_of(&shown, inner.window, inner.room, inner.bar)
    }
}

impl PanelLayout {
    /// The places as `kept` has them (see [`PanelLayout::kept`]), and only those: the pieces it
    /// doesn't have closed, with no place of their own (a named layout chosen).
    pub(crate) fn replace_kept(&self, kept: &str) {
        let mut inner = self.lock();
        inner.open = [None; PIECES];
        inner.spots = [[None; PIECES]; 2];
        inner.hidden = [false; PIECES];
        inner.pinned = [false; PIECES];
        inner.old = [None; 2];
        inner.held = None;
        inner.groups.clear();
        inner.belt.clear();
        inner.read(kept);
    }

    /// Every piece let float, as though the plugins said so (for tests elsewhere in the core).
    #[cfg(test)]
    pub(crate) fn let_every_piece_float(&self) {
        self.lock().floats = [true; PIECES];
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Inner {
    fn landscape(&self) -> bool {
        self.window.0 >= self.window.1
    }

    fn way(&self) -> usize {
        usize::from(self.landscape())
    }

    /// Whether the plugins let `piece` float now (see [`PanelLayout::may_float`]).
    fn may(&self, piece: LayoutPiece) -> bool {
        match piece {
            LayoutPiece::Colours => self.look.on,
            _ => piece.kept_at().and_then(|at| self.floats.get(at)).copied().unwrap_or(false),
        }
    }

    fn is_hidden(&self, piece: LayoutPiece) -> bool {
        self.owner(piece).kept_at().and_then(|at| self.hidden.get(at)).copied().unwrap_or(false)
    }

    fn is_pinned(&self, piece: LayoutPiece) -> bool {
        self.owner(piece).kept_at().and_then(|at| self.pinned.get(at)).copied().unwrap_or(false)
    }

    fn raise(&mut self, piece: LayoutPiece) {
        if piece.kept_at().is_some() {
            self.stack.retain(|&p| p != piece);
            self.stack.push(piece);
        }
    }

    /// How big `piece` may be, in its own points: as [`piece_sizes`] says for one that is sized;
    /// as the app says it is for one that isn't.
    fn sizes_of(&self, piece: LayoutPiece) -> PieceSizes {
        let sizes = piece_sizes(piece);
        if sizes.sizable {
            return sizes;
        }
        let (width, height) =
            piece.kept_at().and_then(|at| self.sizes.get(at)).copied().unwrap_or((sizes.width, sizes.height));
        PieceSizes { width, least_width: width, most_width: width, height, least_height: height, sizable: false }
    }

    fn is_open(&self, piece: LayoutPiece) -> bool {
        let Some(at) = piece.kept_at() else { return false };
        let old = self.old.get(at).copied().flatten().map(|w| w.open);
        self.open.get(at).copied().flatten().or(old).unwrap_or(false)
    }

    /// `piece`'s place in this orientation, in window points: its own, else the other orientation's,
    /// else (the Layers panel and the brush library) the window an app kept before there were
    /// layouts.
    fn spot(&self, piece: LayoutPiece) -> Option<Spot> {
        // a tab's is its window's
        let at = self.owner(piece).kept_at()?;
        let way = self.way();
        let own = self.spots.get(way).and_then(|s| s.get(at)).copied().flatten().map(|s| self.of_share(s));
        let other = self.spots.get(1 - way).and_then(|s| s.get(at)).copied().flatten().map(|s| self.of_share(s));
        let old = self.old.get(at).copied().flatten().map(|w| Spot {
            x: w.x,
            y: w.y,
            width: w.width,
            height: w.height,
            folded: false,
            dock: LayoutDock::default(),
            collapsed: false,
        });
        own.or(other).or(old)
    }

    /// `spot` (window points) kept as `piece`'s place in this orientation.
    fn set_spot(&mut self, piece: LayoutPiece, spot: Spot) {
        let way = self.way();
        let kept = self.as_share(spot);
        let owner = self.owner(piece);
        if let Some(slot) = owner.kept_at().and_then(|at| self.spots.get_mut(way).and_then(|s| s.get_mut(at))) {
            *slot = Some(kept);
        }
    }

    /// The window's width and height to take shares of: never nothing.
    fn sides(&self) -> (f32, f32) {
        (self.window.0.max(1.0), self.window.1.max(1.0))
    }

    /// `spot` (window points) with its top-left as shares of the window, as it is kept.
    fn as_share(&self, spot: Spot) -> Spot {
        let (w, h) = self.sides();
        Spot { x: spot.x / w, y: spot.y / h, ..spot }
    }

    /// `spot` as kept, its top-left back in window points (to a 64th of a point, so a place kept
    /// comes back where it was left).
    fn of_share(&self, spot: Spot) -> Spot {
        let (w, h) = self.sides();
        let point = |v: f32| (v * 64.0).round() / 64.0;
        Spot { x: point(spot.x * w), y: point(spot.y * h), ..spot }
    }

    /// The side `piece`'s window would go into: the one it is docked to, while it shows unfolded (a
    /// window that may hold tabs: the Layers panel, the library, the Actions panel, the Adjustments
    /// menu).
    fn window_edge(&self, piece: LayoutPiece) -> Option<WindowEdge> {
        if !piece.tabbed() {
            return None;
        }
        let shown = self.shown(piece).filter(|s| !s.folded)?;
        match (shown.dock.left, shown.dock.right) {
            (true, _) => Some(WindowEdge::Left),
            (false, true) => Some(WindowEdge::Right),
            _ => None,
        }
    }

    /// Whether `piece`'s window is collapsed out of sight now (it takes no room, and the others may go
    /// where it was).
    fn gone(&self, piece: LayoutPiece) -> bool {
        self.look.collapse && self.spot(piece).is_some_and(|s| s.collapsed) && self.window_edge(piece).is_some()
    }

    /// `piece`'s window collapsed or brought back out: whether that changed anything.
    fn collapse(&mut self, piece: LayoutPiece, collapsed: bool) -> bool {
        if collapsed && (!self.look.collapse || self.window_edge(piece).is_none()) {
            return false;
        }
        let Some(spot) = self.spot(piece).filter(|s| s.collapsed != collapsed) else { return false };
        // (only a window that shows, as a window of its own or the tab showing in one)
        if self.shown(piece).is_none() {
            return false;
        }
        self.set_spot(piece, Spot { collapsed, ..spot });
        true
    }

    fn shown(&self, piece: LayoutPiece) -> Option<PieceShown> {
        piece.kept_at()?;
        if !self.may(piece) || !self.is_open(piece) || self.is_hidden(piece) {
            return None;
        }
        // a tab not showing in its window, or a panel parked on the Tool Belt while it shows
        if self.group_of(piece).is_some_and(|g| g.active != piece) || (self.look.belt && self.belt.contains(&piece)) {
            return None;
        }
        let spot = self.spot(piece)?;
        Some(self.shown_spot(piece, spot))
    }

    /// What the pieces showing cover, but `but` (folded ones by their tabs, unless not `folded_too`),
    /// the Reference window among them: what a piece held snaps against.
    fn shown_rects(&self, but: Option<LayoutPiece>, folded_too: bool) -> Vec<LayoutRect> {
        let mut rects: Vec<LayoutRect> = KEPT
            .into_iter()
            .filter(|&p| Some(p) != but && !self.gone(p))
            .filter_map(|p| self.shown(p))
            .filter(|s| folded_too || !s.folded)
            .map(|s| s.rect())
            .collect();
        if but != Some(LayoutPiece::Reference) {
            rects.extend(self.reference);
        }
        rects
    }

    /// `spot` as `piece` shows in the room now: its size within what it may be (one that isn't sized
    /// as the app says it is), against the edges it was docked to, inside the room. Folded, its tab.
    fn shown_spot(&self, piece: LayoutPiece, spot: Spot) -> PieceShown {
        let k = self.scale;
        let sizes = self.sizes_of(piece);
        let spot = if sizes.sizable { spot } else { Spot { width: sizes.width, height: sizes.height, ..spot } };
        if spot.folded {
            let tab = tab_of(&spot, k);
            let placed = docked_place(tab, spot.dock, self.room, self.bar);
            return PieceShown {
                x: placed.x,
                y: placed.y,
                width: TAB_WIDTH,
                height: TAB_HEIGHT,
                scale: k,
                folded: true,
                dock: docked(placed, self.room, self.bar),
            };
        }
        let window = if sizes.sizable {
            let window = FloatingWindow { open: true, x: spot.x, y: spot.y, width: spot.width, height: spot.height };
            let sized = fitted(sizes, window, self.room, k);
            let rect = docked_place(
                LayoutRect { x: sized.x, y: sized.y, width: sized.width * k, height: sized.height * k },
                spot.dock,
                self.room,
                self.bar,
            );
            FloatingWindow { x: rect.x, y: rect.y, ..sized }
        } else {
            let rect = LayoutRect { x: spot.x, y: spot.y, width: spot.width * k, height: spot.height * k };
            let placed = docked_place(rect, spot.dock, self.room, self.bar);
            FloatingWindow { open: true, x: placed.x, y: placed.y, width: spot.width, height: spot.height }
        };
        let rect = LayoutRect { x: window.x, y: window.y, width: window.width * k, height: window.height * k };
        PieceShown {
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
            scale: k,
            folded: false,
            dock: docked(rect, self.room, self.bar),
        }
    }

    /// A spot for `piece` showing at `rect` (window points; a folded piece's tab): its size back in
    /// its own points, docked to the edges it is against.
    fn place(&self, piece: LayoutPiece, rect: LayoutRect, folded: bool) -> Spot {
        let k = self.scale;
        let dock = docked(rect, self.room, self.bar);
        if !folded {
            let (width, height) = (rect.width / k, rect.height / k);
            return Spot { x: rect.x, y: rect.y, width, height, folded, dock, collapsed: false };
        }
        // the panel's own top-left, from its tab
        let sizes = self.sizes_of(piece);
        let (width, height) = match self.spot(piece) {
            Some(s) if sizes.sizable => (s.width, s.height),
            _ => (sizes.width, sizes.height),
        };
        let x = if dock.right && !dock.left { rect.right() - width * k } else { rect.x };
        let y = if dock.bottom && !dock.top { rect.bottom() - height * k } else { rect.y };
        Spot { x, y, width, height, folded, dock, collapsed: false }
    }

    fn dragged(&mut self, held: Held, dx: f32, dy: f32) -> PieceShown {
        let start = held.start;
        let k = start.scale;
        let reach = self.look.snap;
        let others = self.shown_rects(Some(held.piece), true);
        let rect = start.rect();
        let shown = if held.sizing {
            let sizes = self.sizes_of(held.piece);
            if !sizes.sizable {
                return start;
            }
            let window =
                FloatingWindow { open: true, x: start.x, y: start.y, width: start.width, height: start.height };
            let sized = resized(sizes, window, dx, dy, self.room, k);
            let raw = LayoutRect { x: sized.x, y: sized.y, width: sized.width * k, height: sized.height * k };
            let snapped = snap_sized(raw, &others, self.room, reach);
            let most_height = ((self.room.bottom - snapped.y) / k).max(sizes.least_height);
            let width = (snapped.width / k).clamp(sizes.least_width, sizes.most_width);
            let height = (snapped.height / k).clamp(sizes.least_height, most_height);
            LayoutRect { width: width * k, height: height * k, ..snapped }
        } else {
            let moved = LayoutRect { x: rect.x + dx, y: rect.y + dy, ..rect };
            // a tall window (over half the room) taken down past the room's bottom gets shorter, so it
            // goes on under the finger (no shorter than it may be)
            let sizes = self.sizes_of(held.piece);
            let tall = rect.height > (self.room.bottom - self.room.top) / 2.0;
            let moved =
                if held.piece != LayoutPiece::Reference && sizes.sizable && tall && moved.bottom() > self.room.bottom {
                    let height = (self.room.bottom - moved.y).max(sizes.least_height * k).min(rect.height);
                    LayoutRect { height, ..moved }
                } else {
                    moved
                };
            let inside = inside_room(moved, self.room);
            let snapped = snap_moved(inside, &others, self.room, self.bar, reach);
            inside_room(snapped, self.room)
        };
        match held.piece {
            LayoutPiece::Reference => {
                PieceShown { x: shown.x, y: shown.y, dock: docked(shown, self.room, self.bar), ..start }
            }
            piece => {
                let spot = self.place(piece, shown, start.folded);
                self.set_spot(piece, spot);
                self.shown_spot(piece, spot)
            }
        }
    }

    fn kept(&self) -> String {
        let mut pieces = Vec::new();
        for (at, piece) in KEPT.into_iter().enumerate() {
            let mut words = vec![piece.word().to_owned(), u8::from(self.is_open(piece)).to_string()];
            for (way, letter) in [(0, "P"), (1, "L")] {
                if let Some(s) = self.spots.get(way).and_then(|s| s.get(at)).copied().flatten() {
                    words.push(format!(
                        "{letter} {} {} {} {} {} {}",
                        s.x,
                        s.y,
                        s.width,
                        s.height,
                        // (folded 1, collapsed into its side 2)
                        u8::from(s.folded) | (u8::from(s.collapsed) << 1),
                        s.dock.letters()
                    ));
                }
            }
            // hidden and pinned last, so an app from before them takes the rest
            let flags: String = [(self.is_hidden(piece), 'h'), (self.is_pinned(piece), 'p')]
                .into_iter()
                .filter_map(|(on, c)| on.then_some(c))
                .collect();
            if !flags.is_empty() {
                words.push(format!("F {flags}"));
            }
            let seen = self.spots.iter().any(|s| s.get(at).copied().flatten().is_some()) || self.is_open(piece);
            if seen {
                pieces.push(words.join(" "));
            }
        }
        // each window of tabs: its tabs, the one showing marked (an app from before tabs passes it over)
        for group in &self.groups {
            let words: Vec<String> = group
                .pieces
                .iter()
                .map(|&p| if p == group.active { format!("*{}", p.word()) } else { p.word().to_owned() })
                .collect();
            pieces.push(format!("tabs {}", words.join(" ")));
        }
        // the panels parked on the Tool Belt (an app from before it passes this over)
        if !self.belt.is_empty() {
            let words: Vec<&str> = self.belt.iter().map(|p| p.word()).collect();
            pieces.push(format!("belt {}", words.join(" ")));
        }
        pieces.join(";")
    }

    /// The layout as [`Inner::kept`] writes it, leniently: a piece or a place that doesn't read is
    /// passed over, the rest taken.
    fn read(&mut self, kept: &str) {
        for piece in kept.split(';') {
            if let Some(tabs) = piece.trim().strip_prefix("tabs ") {
                self.read_tabs(tabs);
                continue;
            }
            if let Some(belt) = piece.trim().strip_prefix("belt ") {
                for parked in belt.split_whitespace().filter_map(LayoutPiece::of_word) {
                    if parked.tabbed() && !self.belt.contains(&parked) {
                        self.belt.push(parked);
                    }
                }
                continue;
            }
            let mut words = piece.split_whitespace();
            let Some(piece) = words.next().and_then(LayoutPiece::of_word) else { continue };
            let Some(at) = piece.kept_at() else { continue };
            let open = match words.next() {
                Some("1") => true,
                Some("0") => false,
                _ => continue,
            };
            if let Some(slot) = self.open.get_mut(at) {
                *slot = Some(open);
            }
            let mut words: Vec<&str> = words.collect();
            // the flags at the end: hidden, pinned
            if let [.., f, flags] = words[..]
                && f == "F"
            {
                if let Some(slot) = self.hidden.get_mut(at) {
                    *slot = flags.contains('h');
                }
                if let Some(slot) = self.pinned.get_mut(at) {
                    *slot = flags.contains('p');
                }
                words.truncate(words.len() - 2);
            }
            for place in words.chunks(7) {
                let [way, x, y, width, height, folded, dock] = place else { break };
                let way = match *way {
                    "P" => 0,
                    "L" => 1,
                    _ => continue,
                };
                let number = |w: &str| w.parse::<f32>().ok().filter(|v| v.is_finite());
                let (Some(x), Some(y), Some(width), Some(height)) =
                    (number(x), number(y), number(width), number(height))
                else {
                    continue;
                };
                if width <= 0.0 || height <= 0.0 {
                    continue;
                }
                let state = folded.parse::<u8>().unwrap_or(0);
                let (folded, collapsed) = (state & 1 == 1, state & 2 == 2);
                let spot = Spot { x, y, width, height, folded, dock: LayoutDock::of_letters(dock), collapsed };
                if let Some(slot) = self.spots.get_mut(way).and_then(|s| s.get_mut(at)) {
                    *slot = Some(spot);
                }
            }
        }
    }
}

/// `window` (sizes in its own points) within what `sizes` allow, no taller than `room` at `k`, and
/// inside it from its top-left; what isn't a number goes to the room's top-left at its first size.
fn fitted(sizes: PieceSizes, window: FloatingWindow, room: FloatingRoom, k: f32) -> FloatingWindow {
    let or = |v: f32, otherwise: f32| if v.is_finite() { v } else { otherwise };
    let (left, top) = (or(room.left, 0.0), or(room.top, 0.0));
    let (right, bottom) = (or(room.right, left).max(left), or(room.bottom, top).max(top));
    let most_height = ((bottom - top) / k).max(sizes.least_height);
    let width = or(window.width, sizes.width).clamp(sizes.least_width, sizes.most_width);
    let height = or(window.height, sizes.height).clamp(sizes.least_height, most_height);
    let x = or(window.x, left).min(right - width * k).max(left);
    let y = or(window.y, top).min(bottom - height * k).max(top);
    FloatingWindow { open: window.open, x, y, width, height }
}

/// `window` sized from its lower right corner, dragged (`dx`, `dy`) in window points: no wider or
/// taller than reaches the room's right and bottom, within what `sizes` allow.
fn resized(sizes: PieceSizes, window: FloatingWindow, dx: f32, dy: f32, room: FloatingRoom, k: f32) -> FloatingWindow {
    let sized = FloatingWindow { width: window.width + dx / k, height: window.height + dy / k, ..window };
    let width = sized.width.min(((room.right - window.x) / k).max(sizes.least_width));
    let height = sized.height.min(((room.bottom - window.y) / k).max(sizes.least_height));
    fitted(sizes, FloatingWindow { width, height, ..sized }, room, k)
}

impl Inner {
    /// A window of tabs as [`Inner::kept`] writes it: 2 to [`MOST_TABS`] panels not in another,
    /// the one showing marked; else passed over.
    fn read_tabs(&mut self, words: &str) {
        let mut pieces = Vec::new();
        let mut active = None;
        for word in words.split_whitespace() {
            let (showing, word) = word.strip_prefix('*').map_or((false, word), |w| (true, w));
            let Some(piece) = LayoutPiece::of_word(word) else { return };
            if pieces.contains(&piece) || self.group_of(piece).is_some() {
                return;
            }
            if showing {
                active = Some(piece);
            }
            pieces.push(piece);
        }
        if (2..=MOST_TABS).contains(&pieces.len())
            && let Some(first) = pieces.first().copied()
        {
            self.groups.push(Group { active: active.unwrap_or(first), pieces });
        }
    }
}

/// A scale a piece may show at: the floating windows' size, a plugin's, or 1.
fn shown_scale(scale: f32) -> f32 {
    if scale.is_finite() { scale.clamp(0.25, 4.0) } else { 1.0 }
}

/// A folded panel's tab, for its `spot` shown at `k`: at its top-left, or at its top-right while it is
/// docked right (and at its bottom while docked to the bottom alone), so it stays against the edge.
fn tab_of(spot: &Spot, k: f32) -> LayoutRect {
    let (w, h) = (TAB_WIDTH * k, TAB_HEIGHT * k);
    let x = if spot.dock.right && !spot.dock.left { spot.x + spot.width * k - w } else { spot.x };
    let y = if spot.dock.bottom && !spot.dock.top { spot.y + spot.height * k - h } else { spot.y };
    LayoutRect { x, y, width: w, height: h }
}

/// `rect` against the edges of `room` it was docked to (followed as the room changes; the top the
/// nearer of the room's and the top bar's bottom, `bar`), then inside the room (or as much of it as
/// fits, from its top-left).
fn docked_place(rect: LayoutRect, dock: LayoutDock, room: FloatingRoom, bar: f32) -> LayoutRect {
    let mut r = rect;
    if dock.left {
        r.x = room.left;
    } else if dock.right {
        r.x = room.right - r.width;
    }
    if dock.top {
        r.y = if bar > room.top && (r.y - bar).abs() < (r.y - room.top).abs() { bar } else { room.top };
    } else if dock.bottom {
        r.y = room.bottom - r.height;
    }
    inside_room(r, room)
}

/// `rect` moved inside `room`, from its top-left where it doesn't fit; what isn't a number goes to
/// the room's top-left.
fn inside_room(rect: LayoutRect, room: FloatingRoom) -> LayoutRect {
    let or = |v: f32, otherwise: f32| if v.is_finite() { v } else { otherwise };
    let (left, top) = (or(room.left, 0.0), or(room.top, 0.0));
    let (right, bottom) = (or(room.right, left).max(left), or(room.bottom, top).max(top));
    let x = or(rect.x, left).min(right - rect.width).max(left);
    let y = or(rect.y, top).min(bottom - rect.height).max(top);
    LayoutRect { x, y, ..rect }
}

/// Which edges of `room` `rect` is against (to half a point); the top bar's bottom, `bar`, counts as
/// the top.
pub(crate) fn docked(rect: LayoutRect, room: FloatingRoom, bar: f32) -> LayoutDock {
    let on = |a: f32, b: f32| (a - b).abs() <= ON_EDGE;
    LayoutDock {
        left: on(rect.x, room.left),
        top: on(rect.y, room.top) || (bar > room.top && on(rect.y, bar)),
        right: on(rect.right(), room.right),
        bottom: on(rect.bottom(), room.bottom),
    }
}

/// Of `candidates` (where the rect's x or y would go, and how far its edge is from the line it goes
/// to), the nearest within `reach`, the first of equals (the room's edges come first).
fn nearest(candidates: &[(f32, f32)], reach: f32) -> Option<f32> {
    let mut best: Option<(f32, f32)> = None;
    for &(to, far) in candidates {
        if far.is_finite() && far <= reach && best.is_none_or(|(_, b)| far < b) {
            best = Some((to, far));
        }
    }
    best.map(|(to, _)| to)
}

/// Whether the spans `a` and `b` overlap (more than touching).
fn overlapping(a: (f32, f32), b: (f32, f32)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Whether the span `a` ends within `reach` of where `b` starts, or starts within it of where `b`
/// ends: one after the other along that axis.
fn after(a: (f32, f32), b: (f32, f32), reach: f32) -> bool {
    (a.0 - b.1).abs() <= reach || (b.0 - a.1).abs() <= reach
}

/// `rect`, moved by the pointer, snapped as the layout snaps a piece moved: each axis on its own, the
/// nearest of its edges to a line within `reach` going onto it. The lines: the room's edges (and the
/// top bar's bottom, `bar`, for its top); another piece's far edge, beside it with no gap, where the
/// two overlap the other way; and its same edge, the two in line, where one comes after the other
/// the other way. A reach of 0 snaps nothing.
pub(crate) fn snap_moved(
    rect: LayoutRect,
    others: &[LayoutRect],
    room: FloatingRoom,
    bar: f32,
    reach: f32,
) -> LayoutRect {
    // (a reach that isn't a number snaps nothing either)
    if reach.is_nan() || reach <= 0.0 || !rect.finite() {
        return rect;
    }
    let r = rect;
    let mut xs = vec![(room.left, (r.x - room.left).abs()), (room.right - r.width, (r.right() - room.right).abs())];
    for o in others {
        if overlapping((r.y, r.bottom()), (o.y, o.bottom())) {
            xs.push((o.right(), (r.x - o.right()).abs()));
            xs.push((o.x - r.width, (r.right() - o.x).abs()));
        }
        if after((r.y, r.bottom()), (o.y, o.bottom()), reach) {
            xs.push((o.x, (r.x - o.x).abs()));
            xs.push((o.right() - r.width, (r.right() - o.right()).abs()));
        }
    }
    let r = LayoutRect { x: nearest(&xs, reach).unwrap_or(r.x), ..r };
    let mut ys = vec![(room.top, (r.y - room.top).abs())];
    if bar > room.top {
        ys.push((bar, (r.y - bar).abs()));
    }
    ys.push((room.bottom - r.height, (r.bottom() - room.bottom).abs()));
    for o in others {
        if overlapping((r.x, r.right()), (o.x, o.right())) {
            ys.push((o.bottom(), (r.y - o.bottom()).abs()));
            ys.push((o.y - r.height, (r.bottom() - o.y).abs()));
        }
        if after((r.x, r.right()), (o.x, o.right()), reach) {
            ys.push((o.y, (r.y - o.y).abs()));
            ys.push((o.bottom() - r.height, (r.bottom() - o.bottom()).abs()));
        }
    }
    LayoutRect { y: nearest(&ys, reach).unwrap_or(r.y), ..r }
}

/// `rect`, sized from its lower right corner, snapped as the layout snaps a piece sized: its right and
/// bottom edges each onto the nearest line within `reach` (the room's, another piece's near edge where
/// the two overlap the other way, or its same edge where one comes after the other), its top-left
/// staying.
pub(crate) fn snap_sized(rect: LayoutRect, others: &[LayoutRect], room: FloatingRoom, reach: f32) -> LayoutRect {
    // (a reach that isn't a number snaps nothing either)
    if reach.is_nan() || reach <= 0.0 || !rect.finite() {
        return rect;
    }
    let r = rect;
    let mut rights = vec![(room.right, (r.right() - room.right).abs())];
    let mut bottoms = vec![(room.bottom, (r.bottom() - room.bottom).abs())];
    for o in others {
        if overlapping((r.y, r.bottom()), (o.y, o.bottom())) && o.x >= r.x {
            rights.push((o.x, (r.right() - o.x).abs()));
        }
        if after((r.y, r.bottom()), (o.y, o.bottom()), reach) {
            rights.push((o.right(), (r.right() - o.right()).abs()));
        }
        if overlapping((r.x, r.right()), (o.x, o.right())) && o.y >= r.y {
            bottoms.push((o.y, (r.bottom() - o.y).abs()));
        }
        if after((r.x, r.right()), (o.x, o.right()), reach) {
            bottoms.push((o.bottom(), (r.bottom() - o.bottom()).abs()));
        }
    }
    let width = nearest(&rights, reach).map_or(r.width, |right| right - r.x);
    let height = nearest(&bottoms, reach).map_or(r.height, |bottom| bottom - r.y);
    LayoutRect { width, height, ..r }
}

/// What the drawing area gives up to `rects` (the pieces showing unfolded, window points) in a window
/// `window` wide and tall, the panels going in `room` under a top bar ending at `bar`: see
/// [`PanelLayout::canvas_insets`].
pub(crate) fn insets_of(rects: &[LayoutRect], window: (f32, f32), room: FloatingRoom, bar: f32) -> LayoutInsets {
    let docks: Vec<LayoutDock> = rects.iter().map(|r| docked(*r, room, bar)).collect();
    let dock = |i: usize| docks.get(i).copied().unwrap_or_default();
    let touching = |a: f32, b: f32| (a - b).abs() <= ON_EDGE;
    let across = |o: &LayoutRect, r: &LayoutRect| overlapping((r.y, r.bottom()), (o.y, o.bottom()));
    let down = |o: &LayoutRect, r: &LayoutRect| overlapping((r.x, r.right()), (o.x, o.right()));
    let left = row(rects, |i| dock(i).left, |o, r| touching(r.x, o.right()) && across(o, r));
    let right = row(rects, |i| dock(i).right, |o, r| touching(r.right(), o.x) && across(o, r));
    // the top and the bottom: what isn't in a row from a side already
    let beside = |i: usize| left.get(i).copied().unwrap_or(false) || right.get(i).copied().unwrap_or(false);
    let top =
        row(rects, |i| dock(i).top && !dock(i).side() && !beside(i), |o, r| touching(r.y, o.bottom()) && down(o, r));
    let bottom =
        row(rects, |i| dock(i).bottom && !dock(i).side() && !beside(i), |o, r| touching(r.bottom(), o.y) && down(o, r));
    let of = |inside: &[bool]| rects.iter().zip(inside).filter_map(|(r, &is)| is.then_some(*r)).collect::<Vec<_>>();
    let most =
        |rs: Vec<LayoutRect>, edge: fn(&LayoutRect) -> f32| rs.iter().map(edge).fold(f32::NEG_INFINITY, f32::max);
    let least = |rs: Vec<LayoutRect>, edge: fn(&LayoutRect) -> f32| rs.iter().map(edge).fold(f32::INFINITY, f32::min);
    let room_of = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
    LayoutInsets {
        left: room_of(most(of(&left), LayoutRect::right)),
        top: room_of(most(of(&top), LayoutRect::bottom)),
        right: room_of(window.0 - least(of(&right), |r| r.x)),
        bottom: room_of(window.1 - least(of(&bottom), |r| r.y)),
    }
}

/// Which of `rects` are in a row from an edge: those `seed` says are against it, and each `next` says
/// joins one already in (beside it, the two overlapping the other way).
fn row(
    rects: &[LayoutRect],
    seed: impl Fn(usize) -> bool,
    next: impl Fn(&LayoutRect, &LayoutRect) -> bool,
) -> Vec<bool> {
    let mut inside: Vec<bool> = (0..rects.len()).map(seed).collect();
    loop {
        let mut grew = false;
        for (i, r) in rects.iter().enumerate() {
            if inside.get(i).copied().unwrap_or(true) {
                continue;
            }
            if rects.iter().zip(&inside).any(|(o, &is)| is && next(o, r)) {
                if let Some(slot) = inside.get_mut(i) {
                    *slot = true;
                }
                grew = true;
            }
        }
        if !grew {
            return inside;
        }
    }
}

#[cfg(test)]
mod tests {
    use procraft_engine::geom::Vec2;

    use super::*;

    const ROOM: FloatingRoom = FloatingRoom { left: 0.0, top: 0.0, right: 1600.0, bottom: 1000.0 };
    const BAR: f32 = 48.0;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> LayoutRect {
        LayoutRect { x, y, width, height }
    }

    #[test]
    fn a_piece_snaps_to_the_room_s_edges_within_the_reach_and_not_past_it() {
        let size = (384.0, 520.0);
        let at = |x: f32, y: f32| snap_moved(rect(x, y, size.0, size.1), &[], ROOM, BAR, 16.0);
        // 15.9 from the left edge snaps onto it, 16.1 doesn't
        assert_eq!(at(15.9, 300.0).x, 0.0);
        assert_eq!(at(16.1, 300.0).x, 16.1);
        // its right edge onto the room's right
        assert_eq!(at(1600.0 - 384.0 - 10.0, 300.0).x, 1216.0);
        // its top onto the top bar's bottom, or the window's top; its bottom onto the bottom
        assert_eq!(at(500.0, 60.0).y, 48.0);
        assert_eq!(at(500.0, 7.0).y, 0.0);
        assert_eq!(at(500.0, 1000.0 - 520.0 - 12.0).y, 480.0);
        // in the middle it stays where it is
        assert_eq!(at(600.0, 300.0), rect(600.0, 300.0, 384.0, 520.0));
        // no reach, no snapping
        assert_eq!(snap_moved(rect(3.0, 3.0, 384.0, 520.0), &[], ROOM, BAR, 0.0), rect(3.0, 3.0, 384.0, 520.0));
    }

    #[test]
    fn pieces_snap_side_by_side_with_no_gap_and_in_line() {
        // the Layers panel docked right under the top bar
        let layers = rect(1216.0, 48.0, 384.0, 520.0);
        // the library dropped 12 points left of it, overlapping it down the screen: beside it
        let brushes = snap_moved(rect(1216.0 - 440.0 - 12.0, 100.0, 440.0, 600.0), &[layers], ROOM, BAR, 16.0);
        assert_eq!(brushes.x + brushes.width, layers.x, "no gap");
        // 10 under its top: in line with it
        let brushes = snap_moved(rect(1216.0 - 440.0 - 12.0, 58.0, 440.0, 600.0), &[layers], ROOM, BAR, 16.0);
        assert_eq!((brushes.right(), brushes.y), (1216.0, 48.0));
        // dropped under it, 9 below and 7 left of in line: under it, in line
        let under = snap_moved(rect(1209.0, 48.0 + 520.0 + 9.0, 384.0, 300.0), &[layers], ROOM, BAR, 16.0);
        assert_eq!((under.x, under.y), (1216.0, 568.0));
        // far from it the other way, it doesn't snap beside it
        let away = snap_moved(rect(1216.0 - 300.0 - 10.0, 700.0, 300.0, 200.0), &[layers], ROOM, BAR, 16.0);
        assert_eq!(away.x, 906.0, "no overlap down the screen: nothing beside");
    }

    #[test]
    fn a_piece_sized_snaps_its_corner_and_keeps_its_top_left() {
        let layers = rect(1216.0, 48.0, 384.0, 520.0);
        // the library's right edge dragged to 10 short of the Layers panel: onto it
        let sized = snap_sized(rect(500.0, 48.0, 706.0, 400.0), &[layers], ROOM, 16.0);
        assert_eq!(sized, rect(500.0, 48.0, 716.0, 400.0));
        // its bottom to 14 short of the room's: onto it
        let sized = snap_sized(rect(100.0, 100.0, 300.0, 886.0), &[], ROOM, 16.0);
        assert_eq!(sized, rect(100.0, 100.0, 300.0, 900.0));
        // in line with the Layers panel's bottom, beside it
        let sized = snap_sized(rect(800.0, 48.0, 416.0, 515.0), &[layers], ROOM, 16.0);
        assert_eq!(sized.bottom(), 568.0);
    }

    #[test]
    fn a_piece_knows_the_edges_it_is_against() {
        assert_eq!(
            docked(rect(1216.0, 48.0, 384.0, 952.0), ROOM, BAR),
            LayoutDock { left: false, top: true, right: true, bottom: true }
        );
        assert_eq!(
            docked(rect(0.2, 300.0, 384.0, 100.0), ROOM, BAR),
            LayoutDock { left: true, ..LayoutDock::default() }
        );
        assert_eq!(
            docked(rect(0.6, 300.0, 384.0, 100.0), ROOM, BAR),
            LayoutDock::default(),
            "more than half a point off"
        );
        for dock in [
            LayoutDock::default(),
            LayoutDock { left: true, top: true, right: false, bottom: true },
            LayoutDock { left: false, top: false, right: true, bottom: false },
        ] {
            assert_eq!(LayoutDock::of_letters(&dock.letters()), dock);
        }
    }

    #[test]
    fn panels_docked_against_the_edges_take_their_room_from_the_drawing_area() {
        let window = (1600.0, 1000.0);
        // a column 384 wide against the right edge: 384 off the right
        let layers = rect(1216.0, 48.0, 384.0, 520.0);
        assert_eq!(insets_of(&[layers], window, ROOM, BAR), LayoutInsets { right: 384.0, ..LayoutInsets::default() });
        // the library beside it, in a row from the edge: 384 + 440
        let brushes = rect(776.0, 100.0, 440.0, 600.0);
        assert_eq!(insets_of(&[layers, brushes], window, ROOM, BAR).right, 824.0);
        // one stacked under it against the edge: still 384
        let under = rect(1216.0, 568.0, 384.0, 300.0);
        assert_eq!(insets_of(&[layers, under], window, ROOM, BAR).right, 384.0);
        // one floating free takes nothing
        assert_eq!(insets_of(&[rect(600.0, 300.0, 384.0, 520.0)], window, ROOM, BAR), LayoutInsets::default());
        // against the left edge of a room clear of a side bar 60 wide: to its far side
        let room = FloatingRoom { left: 60.0, ..ROOM };
        assert_eq!(insets_of(&[rect(60.0, 200.0, 260.0, 351.0)], window, room, BAR).left, 320.0);
        // along the bottom alone: its height; in a corner, its side
        assert_eq!(insets_of(&[rect(600.0, 800.0, 400.0, 200.0)], window, ROOM, BAR).bottom, 200.0);
        let corner = insets_of(&[rect(1216.0, 480.0, 384.0, 520.0)], window, ROOM, BAR);
        assert_eq!((corner.right, corner.bottom), (384.0, 0.0));
        // hanging under the top bar alone: to its bottom
        assert_eq!(insets_of(&[rect(600.0, 48.0, 400.0, 100.0)], window, ROOM, BAR).top, 148.0);
    }

    /// A layout with a room 1600 by 1000 (landscape) under a top bar 48 down, snapping within 16.
    fn laid_out(look: PanelLayoutLook) -> Arc<PanelLayout> {
        let layout = PanelLayout::new();
        layout.lock().look = look;
        layout.lock().floats = [true; PIECES];
        layout.set_room(ROOM, 1600.0, 1000.0, BAR, 1.0);
        layout
    }

    const SNAPPING: PanelLayoutLook = PanelLayoutLook {
        on: true,
        snap: 16.0,
        fold: true,
        make_room: true,
        edit: false,
        belt: false,
        collapse: false,
    };

    #[test]
    fn a_panel_torn_off_follows_the_pointer_snapping_and_is_kept_where_it_was_let_go() {
        let layout = laid_out(SNAPPING);
        // torn off with its grabber 12 down under the pointer
        let first = layout.open_at(LayoutPiece::Layers, 800.0, 112.0, 12.0).unwrap();
        assert_eq!((first.x, first.y, first.width, first.height), (608.0, 100.0, 384.0, 520.0));
        // dragged to 10 short of the right edge and 8 under the top bar: onto both
        let near = layout.drag(800.0 + (1216.0 - 10.0 - 608.0), 112.0 - 44.0).unwrap();
        assert_eq!((near.x, near.y), (1216.0, 48.0));
        assert_eq!(near.dock, LayoutDock { left: false, top: true, right: true, bottom: false });
        assert!(layout.let_go());
        assert_eq!(layout.shown(LayoutPiece::Layers), Some(near));
        // the room narrowed (the side bar moved to the right): it stays against the right edge
        let narrower = FloatingRoom { right: 1520.0, ..ROOM };
        layout.set_room(narrower, 1600.0, 1000.0, BAR, 1.0);
        assert_eq!(layout.shown(LayoutPiece::Layers).map(|s| (s.x, s.y)), Some((1136.0, 48.0)));
        // its room from the drawing area: from its left to the window's right
        assert_eq!(layout.canvas_insets().right, 464.0);
        // without make room, none
        layout.lock().look.make_room = false;
        assert_eq!(layout.canvas_insets(), LayoutInsets::default());
    }

    #[test]
    fn a_panel_held_by_its_corner_is_sized_snapping_within_what_it_may_be() {
        let layout = laid_out(SNAPPING);
        layout.open_at(LayoutPiece::Brushes, 400.0, 112.0, 12.0).unwrap();
        layout.let_go();
        let start = layout.shown(LayoutPiece::Brushes).unwrap();
        assert_eq!((start.x, start.y, start.width, start.height), (180.0, 100.0, 440.0, 600.0));
        // the corner dragged to 12 short of the bottom: onto it
        assert!(layout.hold(LayoutPiece::Brushes, true, 620.0, 700.0));
        let sized = layout.drag(640.0, 700.0 + 288.0).unwrap();
        assert_eq!((sized.x, sized.y, sized.width, sized.height), (180.0, 100.0, 460.0, 900.0));
        // no narrower than its least
        let least = layout.drag(0.0, 0.0).unwrap();
        assert_eq!((least.width, least.height), (380.0, 280.0));
        layout.let_go();
        // the colour panel isn't sized, but moved
        layout.open_at(LayoutPiece::Colours, 1000.0, 400.0, 7.5).unwrap();
        layout.let_go();
        assert!(layout.hold(LayoutPiece::Colours, true, 1000.0, 400.0));
        let moved = layout.drag(1010.0, 420.0).unwrap();
        assert_eq!((moved.width, moved.height), (260.0, 351.0));
        assert_eq!((moved.x, moved.y), (880.0, 412.5));
    }

    const COLLAPSING: PanelLayoutLook = PanelLayoutLook { collapse: true, ..SNAPPING };

    #[test]
    fn a_window_against_a_side_collapses_into_it_and_gives_its_room_back() {
        let layout = laid_out(COLLAPSING);
        // the Layers panel at the right side under the bar (1216, 48, 384 by 520), making room
        let layers = opened(&layout, LayoutPiece::Layers);
        assert_eq!(at(layers), (1216.0, 48.0, 384.0, 520.0));
        assert_eq!(layout.piece_collapse_edge(LayoutPiece::Layers), Some(WindowEdge::Right));
        let room = layout.canvas_insets();
        assert_eq!(room.right, 384.0);
        // collapsed: its tab on the right side across from its middle (308), 14 by 56; slid 384 to
        // the right to go in; the room given back
        assert!(layout.set_piece_collapsed(LayoutPiece::Layers, true));
        let tab = LayoutRect { x: 1600.0 - 14.0, y: 308.0 - 28.0, width: 14.0, height: 56.0 };
        let slid = EdgeCollapse { edge: WindowEdge::Right, tab, slide_x: 384.0, slide_y: 0.0 };
        assert_eq!(layout.piece_collapsed(LayoutPiece::Layers), Some(slid));
        assert_eq!(layout.canvas_insets(), LayoutInsets::default());
        assert!(layout.any_collapsed());
        // the colour window against a side doesn't (only windows that hold tabs do)
        assert_eq!(layout.piece_collapse_edge(LayoutPiece::Colours), None);
        // a window away from the sides doesn't go in
        layout.open_at(LayoutPiece::Brushes, 800.0, 400.0, 0.0);
        layout.let_go();
        assert_eq!(layout.piece_collapse_edge(LayoutPiece::Brushes), None);
        assert!(!layout.set_piece_collapsed(LayoutPiece::Brushes, true));
        // kept, and read back by the next start
        let next = laid_out(COLLAPSING);
        next.read_kept(panel_layout_key(), layout.kept());
        assert_eq!(next.piece_collapsed(LayoutPiece::Layers), Some(slid));
        // a drag from its tab: 10 out of the side isn't enough, 14 is; the room made again
        assert!(!layout.pull_piece_tab(LayoutPiece::Layers, -10.0, 0.0));
        assert!(layout.pull_piece_tab(LayoutPiece::Layers, -14.0, 2.0));
        assert_eq!(layout.piece_collapsed(LayoutPiece::Layers), None);
        assert_eq!(layout.piece_collapse_place(LayoutPiece::Layers), Some(slid));
        // (the library, docked to the bottom, makes room there now too)
        assert_eq!(layout.canvas_insets().right, room.right);
        // all at once, the one against a side
        assert!(layout.set_all_collapsed(true));
        assert!(layout.piece_collapsed(LayoutPiece::Layers).is_some());
        assert!(layout.piece_collapsed(LayoutPiece::Brushes).is_none());
        // Collapse to edge off: it shows, and is collapsed again once it is back on
        layout.lock().look = SNAPPING;
        assert!(layout.piece_collapsed(LayoutPiece::Layers).is_none() && !layout.any_collapsed());
        assert_eq!(layout.canvas_insets().right, room.right);
        layout.lock().look = COLLAPSING;
        assert!(layout.any_collapsed());
        // held by its strip, it comes back out to be moved
        assert!(layout.hold(LayoutPiece::Layers, false, 1408.0, 60.0));
        assert_eq!(layout.piece_collapsed(LayoutPiece::Layers), None);
        layout.let_go();
        // open all
        layout.set_all_collapsed(true);
        assert!(layout.set_all_collapsed(false));
        assert!(!layout.any_collapsed());
    }

    #[test]
    fn a_folded_panel_is_a_tab_that_stays_against_its_edge_and_takes_no_room() {
        let layout = laid_out(SNAPPING);
        layout.open_at(LayoutPiece::Layers, 1408.0, 60.0, 12.0).unwrap();
        let docked = layout.drag(1408.0, 60.0).unwrap();
        assert_eq!((docked.x, docked.y), (1216.0, 48.0));
        layout.let_go();
        assert_eq!(layout.canvas_insets().right, 384.0);
        layout.set_folded(LayoutPiece::Layers, true);
        let tab = layout.shown(LayoutPiece::Layers).unwrap();
        // at its top-right, against the edge
        assert_eq!((tab.x, tab.y, tab.width, tab.height, tab.folded), (1600.0 - 148.0, 48.0, 148.0, 36.0, true));
        assert_eq!(layout.canvas_insets(), LayoutInsets::default());
        // moved by the tab to the middle and unfolded there
        assert!(layout.hold(LayoutPiece::Layers, true, 1500.0, 60.0));
        let moved = layout.drag(900.0, 360.0).unwrap();
        assert_eq!((moved.x, moved.y, moved.folded), (852.0, 348.0, true));
        layout.let_go();
        layout.set_folded(LayoutPiece::Layers, false);
        let unfolded = layout.shown(LayoutPiece::Layers).unwrap();
        assert_eq!((unfolded.x, unfolded.y, unfolded.width, unfolded.height), (852.0, 348.0, 384.0, 520.0));
        // no fold while the plugins don't let panels fold
        layout.lock().look.fold = false;
        layout.set_folded(LayoutPiece::Layers, true);
        assert!(!layout.shown(LayoutPiece::Layers).unwrap().folded);
    }

    #[test]
    fn each_orientation_keeps_its_own_places_and_reads_them_back() {
        let layout = laid_out(SNAPPING);
        layout.open_at(LayoutPiece::Layers, 1408.0, 60.0, 12.0).unwrap();
        layout.drag(1408.0, 60.0);
        layout.let_go();
        // turned to portrait: from the landscape place, fitted to the narrower room
        let portrait = FloatingRoom { left: 0.0, top: 0.0, right: 1000.0, bottom: 1600.0 };
        layout.set_room(portrait, 1000.0, 1600.0, BAR, 1.0);
        let adopted = layout.shown(LayoutPiece::Layers).unwrap();
        assert_eq!((adopted.x, adopted.y), (616.0, 48.0), "still against the right edge");
        // moved to the left edge in portrait
        layout.hold(LayoutPiece::Layers, false, 700.0, 60.0);
        let left = layout.drag(90.0, 60.0).unwrap();
        assert_eq!(left.x, 0.0);
        layout.let_go();
        // back to landscape: where it was there
        layout.set_room(ROOM, 1600.0, 1000.0, BAR, 1.0);
        assert_eq!(layout.shown(LayoutPiece::Layers).map(|s| (s.x, s.y)), Some((1216.0, 48.0)));
        // kept, and read back by the next start, both ways
        let kept = layout.kept();
        // (its top-left as shares of the window: 48 of 1600 down, 1216 of 1600 across and 48 of 1000)
        assert_eq!(kept, "layers 1 P 0 0.03 384 520 0 lt L 0.76 0.048 384 520 0 tr");
        let next = laid_out(SNAPPING);
        next.read_kept(panel_layout_key(), kept.clone());
        assert_eq!(next.kept(), kept);
        assert_eq!(next.shown(LayoutPiece::Layers).map(|s| (s.x, s.y)), Some((1216.0, 48.0)));
        next.set_room(portrait, 1000.0, 1600.0, BAR, 1.0);
        assert_eq!(next.shown(LayoutPiece::Layers).map(|s| (s.x, s.y)), Some((0.0, 48.0)));
        // closed, it is kept closed where it was
        next.close(LayoutPiece::Layers);
        assert_eq!(next.shown(LayoutPiece::Layers), None);
        assert!(next.kept().starts_with("layers 0 P 0 0.03"));
    }

    #[test]
    fn a_window_sized_differently_keeps_a_free_panel_in_proportion_and_a_docked_one_at_its_edge() {
        let layout = laid_out(SNAPPING);
        // the Layers panel left free at 600, 300; the library against the right edge, 300 down
        layout.open_at(LayoutPiece::Layers, 792.0, 312.0, 12.0).unwrap();
        assert_eq!(layout.drag(792.0, 312.0).map(|s| (s.x, s.y)), Some((600.0, 300.0)));
        layout.let_go();
        layout.open_at(LayoutPiece::Brushes, 1380.0, 312.0, 12.0).unwrap();
        assert_eq!(layout.drag(1380.0, 312.0).map(|s| (s.x, s.y)), Some((1160.0, 300.0)));
        layout.let_go();
        // the desktop's window made 1200 by 800 (wider than tall still)
        layout.set_room(FloatingRoom { left: 0.0, top: 0.0, right: 1200.0, bottom: 800.0 }, 1200.0, 800.0, BAR, 1.0);
        let free = layout.shown(LayoutPiece::Layers).unwrap();
        assert_eq!((free.x, free.y), (450.0, 240.0), "three quarters as far, both ways");
        let docked = layout.shown(LayoutPiece::Brushes).unwrap();
        assert_eq!((docked.x, docked.y, docked.dock.right), (760.0, 200.0, true), "still against the right edge");
        // and back to its size: where each was
        layout.set_room(ROOM, 1600.0, 1000.0, BAR, 1.0);
        assert_eq!(layout.shown(LayoutPiece::Layers).map(|s| (s.x, s.y)), Some((600.0, 300.0)));
        assert_eq!(layout.shown(LayoutPiece::Brushes).map(|s| (s.x, s.y)), Some((1160.0, 300.0)));
    }

    #[test]
    fn the_windows_kept_before_layouts_are_where_each_orientation_starts() {
        let layout = laid_out(PanelLayoutLook::default());
        let old = FloatingWindow { open: true, x: 120.0, y: 80.0, width: 400.0, height: 500.0 };
        layout.read_kept(crate::floating_key(FloatingPanel::Layers), crate::floating_kept(old));
        let closed = FloatingWindow { open: false, ..old };
        layout.read_kept(crate::floating_key(FloatingPanel::Brushes), crate::floating_kept(closed));
        let shown = layout.shown(LayoutPiece::Layers).unwrap();
        assert_eq!((shown.x, shown.y, shown.width, shown.height), (120.0, 80.0, 400.0, 500.0));
        assert_eq!(layout.shown(LayoutPiece::Brushes), None, "closed as it was kept");
        assert!(!layout.is_open(LayoutPiece::Brushes));
        // nothing is written for a piece never moved; once moved, the layout's own
        assert_eq!(layout.kept(), "layers 1");
        layout.hold(LayoutPiece::Layers, false, 300.0, 90.0);
        layout.drag(310.0, 90.0);
        assert!(layout.let_go());
        assert_eq!(layout.kept(), "layers 1 L 0.08125 0.08 400 500 0 -");
        // what the layout keeps comes first, whichever was read first
        let next = laid_out(PanelLayoutLook::default());
        next.read_kept(panel_layout_key(), layout.kept());
        next.read_kept(crate::floating_key(FloatingPanel::Layers), crate::floating_kept(old));
        assert_eq!(next.shown(LayoutPiece::Layers).map(|s| s.x), Some(130.0));
    }

    #[test]
    fn what_doesn_t_read_is_passed_over() {
        let layout = laid_out(PanelLayoutLook::default());
        for bad in [
            "",
            ";;",
            "nothing 1",
            "layers",
            "layers 2 L 0 0 384 520 0 -",
            "reference 1 L 0 0 300 300 0 -",
            "layers 1 X 0 0 384 520 0 -",
            "layers 1 L nan 0 384 520 0 -",
            "layers 1 L 0 0 0 520 0 -",
            "layers 1 L 0 0 384",
        ] {
            layout.read_kept(panel_layout_key(), bad.to_owned());
        }
        // what did read: Layers open (one said so), with no place
        assert_eq!(layout.kept(), "layers 1");
        // a good piece after a bad one
        layout.read_kept(panel_layout_key(), "brushes x;brushes 1 L 10 20 440 600 1 r".to_owned());
        assert!(layout.kept().contains("brushes 1 L 10 20 440 600 1 r"));
    }

    #[test]
    fn with_no_plugin_on_the_layout_the_panels_float_as_they_always_did() {
        let layout = laid_out(PanelLayoutLook::default());
        // the colour panel isn't the layout's
        assert_eq!(layout.open_at(LayoutPiece::Colours, 500.0, 500.0, 7.5).map(|s| s.folded), Some(false));
        assert_eq!(layout.shown(LayoutPiece::Colours), None);
        // a floating panel moved to 5 off an edge stays 5 off it
        layout.open_at(LayoutPiece::Layers, 800.0, 112.0, 12.0).unwrap();
        let moved = layout.drag(800.0 - 603.0, 112.0).unwrap();
        assert_eq!(moved.x, 5.0);
        assert!(layout.let_go());
        // nothing folds, nothing takes the drawing area's room
        layout.set_folded(LayoutPiece::Layers, true);
        assert!(!layout.shown(LayoutPiece::Layers).unwrap().folded);
        assert_eq!(layout.canvas_insets(), LayoutInsets::default());
        // the same move as floating.rs makes it
        let window = FloatingWindow { open: true, x: 608.0, y: 100.0, width: 384.0, height: 520.0 };
        let old = crate::floating_moved(FloatingPanel::Layers, window, -603.0, 0.0, ROOM, 1.0);
        assert_eq!((old.x, old.y), (moved.x, moved.y));
        // with no plugin letting it float, it doesn't show
        layout.lock().floats = KEPT.map(|p| p == LayoutPiece::Colours);
        assert_eq!(layout.shown(LayoutPiece::Layers), None);
    }

    #[test]
    fn docked_panels_making_room_fit_the_canvas_in_what_is_left_and_keep_its_middle() {
        let s = crate::CanvasSession::new(400, 300);
        {
            let mut st = s.shared.lock();
            st.window_size = (1600, 1000);
            st.view = Some(st.fitted_view());
        }
        let view = |s: &crate::CanvasSession| s.shared.lock().view.unwrap().to_screen();
        // fitted in the whole window: 3.33 times, 133 in from the left
        let whole = view(&s);
        assert!((whole.a - 1000.0 / 300.0).abs() < 1e-4 && (whole.e - 133.333).abs() < 0.01, "{whole:?}");
        // all 0, as with no plugin docking panels: nothing moves
        s.set_room_insets(0.0, 0.0, 0.0, 0.0);
        assert_eq!(view(&s), whole);
        // a panel 384 wide docked right: fitted again in the 1216 left, against its left
        s.set_room_insets(0.0, 0.0, 384.0, 0.0);
        let fitted = view(&s);
        assert!(
            (fitted.a - 3.04).abs() < 1e-4 && fitted.e.abs() < 0.01 && (fitted.f - 44.0).abs() < 0.01,
            "{fitted:?}"
        );
        // zoomed in, then a panel 200 wide docked left too: it moves right by half that
        s.shared.lock().view.as_mut().unwrap().gesture(Vec2::new(608.0, 500.0), Vec2::new(0.0, 0.0), 2.0, 0.0);
        let zoomed = view(&s);
        s.set_room_insets(200.0, 0.0, 384.0, 0.0);
        let moved = view(&s);
        assert_eq!((moved.a, moved.f), (zoomed.a, zoomed.f), "the zoom kept");
        assert!((moved.e - zoomed.e - 100.0).abs() < 1e-3, "{} {}", moved.e, zoomed.e);
        // what Fit Canvas Inside Interface keeps clear already counts: the more of the two
        s.set_fit_insets(300.0, 0.0, 0.0, 0.0);
        s.set_room_insets(250.0, 0.0, 384.0, 0.0);
        assert!((view(&s).e - moved.e).abs() < 1e-3, "inside the interface's 300 already");
        assert_eq!(s.shared.lock().kept_clear(), [300.0, 0.0, 384.0, 0.0]);
        // what isn't a number is none
        s.set_room_insets(f32::NAN, f32::INFINITY, -5.0, 0.0);
        assert_eq!(s.shared.lock().kept_clear(), [300.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn with_the_plugins_the_app_comes_with_off_nothing_is_laid_out() {
        let dir = std::env::temp_dir().join(format!("procraft-layout-off-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = Plugins::with_bundled(dir.to_string_lossy().into_owned());
        assert!(!plugins.plugins().is_empty() && plugins.plugins().iter().all(|p| !p.on));
        assert_eq!(plugins.panel_layout(), PanelLayoutLook::default());
        let layout = PanelLayout::new();
        layout.follow(Arc::clone(&plugins));
        layout.set_room(ROOM, 1600.0, 1000.0, BAR, 1.0);
        // no panel floats, the colour panel isn't the layout's, nothing folds or makes room
        assert_eq!(layout.open_at(LayoutPiece::Layers, 800.0, 112.0, 12.0).map(|s| s.folded), Some(false));
        layout.let_go();
        assert_eq!(layout.shown(LayoutPiece::Layers), None);
        layout.open_at(LayoutPiece::Colours, 800.0, 112.0, 7.5);
        layout.let_go();
        assert_eq!(layout.shown(LayoutPiece::Colours), None);
        layout.set_folded(LayoutPiece::Layers, true);
        assert_eq!(layout.canvas_insets(), LayoutInsets::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `piece` torn off and let go, then put back where it first goes: where it shows.
    fn opened(layout: &PanelLayout, piece: LayoutPiece) -> PieceShown {
        layout.open_at(piece, 800.0, 400.0, 0.0).unwrap();
        layout.let_go();
        layout.reset_place(piece).unwrap()
    }

    /// `piece` pressed at (`x`, `y`), dragged by (`dx`, `dy`) and let go: where it shows.
    fn moved(layout: &PanelLayout, piece: LayoutPiece, (x, y): (f32, f32), (dx, dy): (f32, f32)) -> PieceShown {
        assert!(layout.hold(piece, false, x, y), "{piece:?}");
        let shown = layout.drag(x + dx, y + dy).unwrap();
        layout.let_go();
        shown
    }

    fn at(s: PieceShown) -> (f32, f32, f32, f32) {
        (s.x, s.y, s.width, s.height)
    }

    #[test]
    fn every_panel_floats_at_once_snapping_beside_the_others_folding_and_making_room() {
        let layout = laid_out(SNAPPING);
        // each where it first goes: from the right of the top bar against the right edge under it,
        // from its left against the left edge, a toolbar at the bottom in the middle
        let layers = opened(&layout, LayoutPiece::Layers);
        assert_eq!(at(layers), (1216.0, 48.0, 384.0, 520.0));
        assert!(layers.dock.right && layers.dock.top);
        let actions = opened(&layout, LayoutPiece::Actions);
        assert_eq!(at(actions), (0.0, 48.0, 390.0, 600.0));
        assert!(actions.dock.left && actions.dock.top);
        let selection = opened(&layout, LayoutPiece::Selection);
        assert_eq!(at(selection), (520.0, 936.0, 560.0, 64.0));
        assert!(selection.dock.bottom && !selection.dock.left && !selection.dock.right);
        // the colour panel taken down from the right edge comes under the Layers panel with no gap
        let colours = opened(&layout, LayoutPiece::Colours);
        assert_eq!((colours.x, colours.y), (1340.0, 48.0));
        let colours = moved(&layout, LayoutPiece::Colours, (1350.0, 58.0), (0.0, 525.0));
        assert_eq!(at(colours), (1340.0, 568.0, 260.0, 351.0));
        // the library dragged out into the middle floats free there
        opened(&layout, LayoutPiece::Brushes);
        let brushes = moved(&layout, LayoutPiece::Brushes, (1170.0, 58.0), (-470.0, 242.0));
        assert_eq!(at(brushes), (690.0, 290.0, 440.0, 600.0));
        assert_eq!(brushes.dock, LayoutDock::default());
        // the Adjustments menu dropped 5 right of the Actions panel goes beside it, in line with it
        opened(&layout, LayoutPiece::Adjustments);
        let adjustments = moved(&layout, LayoutPiece::Adjustments, (10.0, 58.0), (395.0, 5.0));
        assert_eq!(at(adjustments), (390.0, 48.0, 330.0, 480.0));
        // the Transform toolbar raised from the bottom, then folded to its tab where its top-left was
        opened(&layout, LayoutPiece::Transform);
        let transform = moved(&layout, LayoutPiece::Transform, (530.0, 946.0), (0.0, -300.0));
        assert_eq!((transform.x, transform.y), (520.0, 636.0));
        layout.set_folded(LayoutPiece::Transform, true);
        let tab = layout.shown(LayoutPiece::Transform).unwrap();
        assert_eq!((at(tab), tab.folded), ((520.0, 636.0, TAB_WIDTH, TAB_HEIGHT), true));
        // seven at once, drawn as they were last pressed
        let order = [
            LayoutPiece::Layers,
            LayoutPiece::Actions,
            LayoutPiece::Selection,
            LayoutPiece::Colours,
            LayoutPiece::Brushes,
            LayoutPiece::Adjustments,
            LayoutPiece::Transform,
        ];
        assert_eq!(layout.order(), order);
        // the room they take: the Actions panel and the menu beside it in a row from the left (720),
        // the Layers and colour panels from the right (384), the Selection toolbar from the bottom
        // (64); the library free and the Transform tab take none
        assert_eq!(layout.canvas_insets(), LayoutInsets { left: 720.0, top: 0.0, right: 384.0, bottom: 64.0 });
        // the room given up again with the menu folded
        layout.set_folded(LayoutPiece::Adjustments, true);
        assert_eq!(layout.canvas_insets().left, 390.0);
    }

    #[test]
    fn every_panel_keeps_its_place_for_each_way_the_screen_turns_and_as_the_window_is_sized() {
        let layout = laid_out(SNAPPING);
        for piece in [LayoutPiece::Layers, LayoutPiece::Actions, LayoutPiece::Selection] {
            opened(&layout, piece);
        }
        // two free in the middle
        layout.open_at(LayoutPiece::Adjustments, 765.0, 300.0, 0.0);
        layout.let_go();
        layout.open_at(LayoutPiece::Brushes, 1000.0, 300.0, 0.0);
        layout.let_go();
        let shown = |piece| at(layout.shown(piece).unwrap());
        assert_eq!(shown(LayoutPiece::Adjustments), (600.0, 300.0, 330.0, 480.0));
        assert_eq!(shown(LayoutPiece::Brushes), (780.0, 300.0, 440.0, 600.0));
        // turned: each starts from the other way's place, in proportion; those docked stay at their edges
        layout.set_room(FloatingRoom { left: 0.0, top: 0.0, right: 1000.0, bottom: 1600.0 }, 1000.0, 1600.0, BAR, 1.0);
        assert_eq!(shown(LayoutPiece::Layers), (616.0, 48.0, 384.0, 520.0));
        assert_eq!(shown(LayoutPiece::Actions), (0.0, 48.0, 390.0, 600.0));
        assert_eq!(shown(LayoutPiece::Selection), (325.0, 1536.0, 560.0, 64.0));
        assert_eq!(shown(LayoutPiece::Adjustments), (375.0, 480.0, 330.0, 480.0));
        assert_eq!(shown(LayoutPiece::Brushes), (487.5, 480.0, 440.0, 600.0));
        // moved this way...
        let menu = moved(&layout, LayoutPiece::Adjustments, (400.0, 500.0), (100.0, 200.0));
        assert_eq!((menu.x, menu.y), (475.0, 680.0));
        // ...it leaves the other way's place as it was
        layout.set_room(ROOM, 1600.0, 1000.0, BAR, 1.0);
        assert_eq!(shown(LayoutPiece::Adjustments), (600.0, 300.0, 330.0, 480.0));
        layout.set_room(FloatingRoom { left: 0.0, top: 0.0, right: 1000.0, bottom: 1600.0 }, 1000.0, 1600.0, BAR, 1.0);
        assert_eq!(shown(LayoutPiece::Adjustments), (475.0, 680.0, 330.0, 480.0));
        // a smaller window the same way up: free panels in proportion, docked ones at their edges
        let smaller = FloatingRoom { left: 0.0, top: 0.0, right: 1200.0, bottom: 800.0 };
        layout.set_room(smaller, 1200.0, 800.0, BAR, 1.0);
        assert_eq!(shown(LayoutPiece::Adjustments), (450.0, 240.0, 330.0, 480.0));
        assert_eq!(shown(LayoutPiece::Layers), (816.0, 48.0, 384.0, 520.0));
        assert_eq!(shown(LayoutPiece::Selection), (390.0, 736.0, 560.0, 64.0));
        assert_eq!(shown(LayoutPiece::Actions), (0.0, 48.0, 390.0, 600.0));
        // kept, and read back by the next start
        let next = laid_out(SNAPPING);
        next.read_kept(panel_layout_key(), layout.kept());
        next.set_room(smaller, 1200.0, 800.0, BAR, 1.0);
        for piece in KEPT {
            assert_eq!(next.shown(piece), layout.shown(piece), "{piece:?}");
        }
    }

    #[test]
    fn each_window_folds_hides_pins_over_the_others_and_goes_back_where_it_first_goes() {
        let layout = laid_out(SNAPPING);
        for piece in [LayoutPiece::Layers, LayoutPiece::Brushes, LayoutPiece::Actions, LayoutPiece::Adjustments] {
            opened(&layout, piece);
        }
        // hidden: open but unseen, until shown again
        layout.set_hidden(LayoutPiece::Brushes, true);
        assert_eq!(layout.shown(LayoutPiece::Brushes), None);
        assert!(layout.is_open(LayoutPiece::Brushes) && layout.is_hidden(LayoutPiece::Brushes));
        assert_eq!(layout.hidden_pieces(), [LayoutPiece::Brushes]);
        assert!(!layout.order().contains(&LayoutPiece::Brushes));
        layout.show_hidden();
        assert!(layout.shown(LayoutPiece::Brushes).is_some());
        assert!(layout.hidden_pieces().is_empty());
        // pinned: over the others however they are pressed
        layout.set_pinned(LayoutPiece::Actions, true);
        assert!(layout.hold(LayoutPiece::Layers, false, 1300.0, 60.0));
        layout.let_go();
        let order = [LayoutPiece::Brushes, LayoutPiece::Adjustments, LayoutPiece::Layers, LayoutPiece::Actions];
        assert_eq!(layout.order(), order);
        // Reset Position: back where it first goes, unfolded
        let away = moved(&layout, LayoutPiece::Adjustments, (10.0, 58.0), (600.0, 400.0));
        assert_ne!((away.x, away.y), (0.0, 48.0));
        let back = layout.reset_place(LayoutPiece::Adjustments).unwrap();
        assert_eq!((back.x, back.y, back.dock.left, back.dock.top), (0.0, 48.0, true, true));
        layout.set_folded(LayoutPiece::Actions, true);
        assert!(layout.shown(LayoutPiece::Actions).unwrap().folded);
        let back = layout.reset_place(LayoutPiece::Actions).unwrap();
        assert_eq!((at(back), back.folded), ((0.0, 48.0, 390.0, 600.0), false));
        // hidden and pinned are kept, and read back
        layout.set_hidden(LayoutPiece::Brushes, true);
        let kept = layout.kept();
        assert!(kept.contains(" F p") && kept.contains(" F h"), "{kept}");
        let next = laid_out(SNAPPING);
        next.read_kept(panel_layout_key(), kept);
        assert!(next.is_pinned(LayoutPiece::Actions) && next.is_hidden(LayoutPiece::Brushes));
        assert!(!next.is_pinned(LayoutPiece::Layers) && !next.is_hidden(LayoutPiece::Layers));
        // what was kept before there were flags reads as it did
        let before = laid_out(SNAPPING);
        before.read_kept(panel_layout_key(), "layers 1 L 0.76 0.048 384 520 0 tr".to_owned());
        assert_eq!(at(before.shown(LayoutPiece::Layers).unwrap()), (1216.0, 48.0, 384.0, 520.0));
        assert!(!before.is_pinned(LayoutPiece::Layers) && !before.is_hidden(LayoutPiece::Layers));
        // closed, a hidden window shows when it opens again
        layout.close(LayoutPiece::Brushes);
        assert!(!layout.is_hidden(LayoutPiece::Brushes));
    }

    #[test]
    fn only_the_panels_with_a_corner_are_sized_by_it() {
        let layout = laid_out(SNAPPING);
        opened(&layout, LayoutPiece::Actions);
        // the Actions panel is sized in height only
        assert!(layout.hold(LayoutPiece::Actions, true, 380.0, 640.0));
        let sized = layout.drag(480.0, 740.0).unwrap();
        layout.let_go();
        assert_eq!(at(sized), (0.0, 48.0, 390.0, 700.0));
        // the Adjustments menu, which isn't sized, moves instead, as big as the app says it is
        layout.set_size(LayoutPiece::Adjustments, 330.0, 420.0);
        layout.open_at(LayoutPiece::Adjustments, 965.0, 300.0, 0.0);
        layout.let_go();
        assert!(layout.hold(LayoutPiece::Adjustments, true, 1120.0, 700.0));
        let menu = layout.drag(1220.0, 700.0).unwrap();
        layout.let_go();
        assert_eq!(at(menu), (900.0, 300.0, 330.0, 420.0));
        // what isn't a size is passed over
        layout.set_size(LayoutPiece::Adjustments, f32::NAN, 10.0);
        layout.set_size(LayoutPiece::Adjustments, 0.0, 10.0);
        assert_eq!(at(layout.shown(LayoutPiece::Adjustments).unwrap()), (900.0, 300.0, 330.0, 420.0));
    }

    #[test]
    fn with_no_plugin_letting_them_float_every_panel_hangs_where_it_always_does() {
        let layout = PanelLayout::new();
        layout.set_room(ROOM, 1600.0, 1000.0, BAR, 1.0);
        for piece in KEPT {
            assert!(!layout.may_float(piece), "{piece:?}");
            layout.open_at(piece, 800.0, 400.0, 0.0);
            layout.let_go();
            assert_eq!(layout.shown(piece), None, "{piece:?}");
        }
        assert!(layout.order().is_empty());
        assert_eq!(layout.canvas_insets(), LayoutInsets::default());
        // the plugins letting the Actions panel alone float
        let one = PanelLayout::new();
        one.lock().floats = KEPT.map(|p| p == LayoutPiece::Actions || p == LayoutPiece::Colours);
        one.set_room(ROOM, 1600.0, 1000.0, BAR, 1.0);
        let floating: Vec<LayoutPiece> = KEPT.into_iter().filter(|&p| one.may_float(p)).collect();
        assert_eq!(floating, [LayoutPiece::Actions], "the colour panel only while a plugin lays the panels out");
        // and each piece's number on the hook: the two from before, then the rest from 2
        let numbers: Vec<Option<i32>> = KEPT.into_iter().map(LayoutPiece::hook_number).collect();
        assert_eq!(numbers, [Some(0), Some(1), None, Some(2), Some(3), Some(4), Some(5), Some(6), Some(7)]);
    }

    /// `piece` torn off with the middle of its top at (`middle`, `top`) and let go: a tab of the
    /// window whose strip that is on, if one's is.
    fn dropped(layout: &PanelLayout, piece: LayoutPiece, middle: f32, top: f32) {
        layout.open_at(piece, middle, top, 0.0).unwrap();
        layout.let_go();
    }

    #[test]
    fn a_window_let_go_on_another_s_strip_becomes_its_tab_and_a_tab_torn_out_floats_again() {
        let layout = laid_out(SNAPPING);
        let layers = opened(&layout, LayoutPiece::Layers);
        assert_eq!(at(layers), (1216.0, 48.0, 384.0, 520.0));
        // the library floating apart, then let go with its strip on the Layers panel's
        layout.open_at(LayoutPiece::Brushes, 800.0, 400.0, 0.0);
        layout.let_go();
        assert!(layout.tabs_of(LayoutPiece::Brushes).is_empty());
        let over = moved(&layout, LayoutPiece::Brushes, (600.0, 410.0), (610.0, -340.0));
        assert_eq!((over.x, over.y), (1160.0, 48.0), "snapped onto the right edge and under the bar");
        let tabs = [LayoutPiece::Layers, LayoutPiece::Brushes];
        assert_eq!(layout.tabs_of(LayoutPiece::Layers), tabs);
        assert_eq!(layout.tabs_of(LayoutPiece::Brushes), tabs);
        // it shows, in the Layers panel's window at the size it has there; the Layers panel waits
        assert_eq!(layout.shown(LayoutPiece::Layers), None);
        assert_eq!(at(layout.shown(LayoutPiece::Brushes).unwrap()), (1216.0, 48.0, 384.0, 520.0));
        assert_eq!(layout.order().last(), Some(&LayoutPiece::Brushes));
        // the other tab chosen
        layout.set_active_tab(LayoutPiece::Layers);
        assert_eq!(layout.shown(LayoutPiece::Layers), Some(layers));
        assert_eq!(layout.shown(LayoutPiece::Brushes), None);
        // the window moved by its strip takes its tabs with it
        let window = moved(&layout, LayoutPiece::Layers, (1300.0, 60.0), (-400.0, 300.0));
        layout.set_active_tab(LayoutPiece::Brushes);
        let brushes = layout.shown(LayoutPiece::Brushes).unwrap();
        assert_eq!((brushes.x, brushes.y), (window.x, window.y));
        // torn out: a window of its own under the pointer, the Layers panel alone again
        let torn = layout.tear_tab(LayoutPiece::Brushes, 400.0, 700.0, 0.0).unwrap();
        layout.let_go();
        assert_eq!((torn.x, torn.y, torn.width, torn.height), (180.0, 400.0, 440.0, 600.0));
        assert!(layout.tabs_of(LayoutPiece::Layers).is_empty());
        assert_eq!(
            (layout.shown(LayoutPiece::Layers).unwrap().x, layout.shown(LayoutPiece::Layers).unwrap().y),
            (window.x, window.y)
        );
    }

    #[test]
    fn a_tall_window_joins_a_strip_low_in_the_window_where_the_pointer_lets_it_go() {
        // the Layers panel (520 tall) at the bottom on the left, its strip at 1000 - 520 = 480
        let low_panel = |layout: &PanelLayout| {
            dropped(layout, LayoutPiece::Layers, 400.0, 480.0);
            layout.shown(LayoutPiece::Layers).unwrap().rect()
        };
        let layout = laid_out(SNAPPING);
        let low = low_panel(&layout);
        assert_eq!(low.y, 480.0);
        // the Adjustments menu held low on it, 400 under its strip, and brought so that the finger is
        // on that strip: its own strip 400 higher, nowhere near it
        let menu = opened(&layout, LayoutPiece::Adjustments);
        let grip = (menu.x + 100.0, menu.y + 400.0);
        assert!(layout.hold(LayoutPiece::Adjustments, false, grip.0, grip.1));
        let pointer = (low.x + low.width / 2.0, low.y + 10.0);
        let held = layout.drag(pointer.0, pointer.1).unwrap();
        assert!(low.y - held.y > TAB_ZONE, "{held:?}");
        // let go with the pointer on that strip: a tab of it all the same
        layout.let_go();
        assert_eq!(layout.tabs_of(LayoutPiece::Adjustments), [LayoutPiece::Layers, LayoutPiece::Adjustments]);
        // let go with the pointer well under that strip (and the window's own away from it): no tab
        let apart = laid_out(SNAPPING);
        low_panel(&apart);
        opened(&apart, LayoutPiece::Adjustments);
        assert!(apart.hold(LayoutPiece::Adjustments, false, grip.0, grip.1));
        apart.drag(pointer.0, pointer.1 + 200.0);
        apart.let_go();
        assert!(apart.tabs_of(LayoutPiece::Adjustments).is_empty());
    }

    #[test]
    fn a_tab_torn_out_leaves_the_strips_as_they_were_until_it_is_let_go() {
        let layout = laid_out(SNAPPING);
        opened(&layout, LayoutPiece::Layers);
        dropped(&layout, LayoutPiece::Brushes, 1408.0, 53.0);
        let two = [LayoutPiece::Layers, LayoutPiece::Brushes];
        assert_eq!(layout.tabs_of(LayoutPiece::Layers), two);
        // the brush library (showing) torn out and dragged on: both strips still show both tabs,
        // so the tab pressed stays on the strip to the end of the drag (in an app laying the strip
        // out anew, one gone mid-drag ends it there)
        layout.tear_tab(LayoutPiece::Brushes, 900.0, 300.0, 0.0).unwrap();
        layout.drag(800.0, 400.0).unwrap();
        assert_eq!(layout.tabs_of(LayoutPiece::Layers), two);
        assert_eq!(layout.tabs_of(LayoutPiece::Brushes), two);
        // its window follows the pointer, out of the Layers panel's
        let torn = layout.shown(LayoutPiece::Brushes).unwrap();
        assert_eq!((torn.x + torn.width / 2.0, torn.y), (800.0, 400.0));
        assert!(layout.shown(LayoutPiece::Layers).is_some());
        // let go: each a window of its own
        layout.let_go();
        assert!(layout.tabs_of(LayoutPiece::Layers).is_empty());
        assert!(layout.tabs_of(LayoutPiece::Brushes).is_empty());
        // and a window only moved (not a tab torn out) changes no strip as it goes
        dropped(&layout, LayoutPiece::Brushes, 1408.0, 53.0);
        assert!(layout.hold(LayoutPiece::Brushes, false, 1408.0, 60.0));
        layout.drag(1300.0, 300.0);
        assert_eq!(layout.tabs_of(LayoutPiece::Layers), two);
        layout.let_go();
    }

    #[test]
    fn either_tab_tears_out_and_follows_the_pointer_while_the_brush_library_shows() {
        for torn in [LayoutPiece::Brushes, LayoutPiece::Layers] {
            // the library let go on the Layers panel's strip: a tab of its window, showing, the only
            // window drawn (the Layers panel waits)
            let layout = laid_out(SNAPPING);
            opened(&layout, LayoutPiece::Layers);
            dropped(&layout, LayoutPiece::Brushes, 1408.0, 53.0);
            let two = [LayoutPiece::Layers, LayoutPiece::Brushes];
            assert_eq!(layout.tabs_of(LayoutPiece::Brushes), two);
            assert_eq!(layout.order(), [LayoutPiece::Brushes]);
            // the tab pulled down out of the strip, then on 430 down and 400 left, step by step: its
            // window under the pointer all the way (its strip 12 over it; across, its middle at the
            // pointer but for snapping, within 16, to the other window's side), both strips as they were
            layout.tear_tab(torn, 1300.0, 80.0, 12.0).unwrap();
            for step in 1..=10 {
                let (x, y) = (1300.0 - step as f32 * 40.0, 80.0 + step as f32 * 43.0);
                let shown = layout.drag(x, y).unwrap();
                assert_eq!(shown.y, y - 12.0, "{torn:?} {step}");
                assert!((shown.x + shown.width / 2.0 - x).abs() <= SNAPPING.snap, "{torn:?} {step}: {shown:?}");
                assert_eq!(layout.tabs_of(LayoutPiece::Layers), two, "{torn:?} {step}");
                assert_eq!(layout.tabs_of(LayoutPiece::Brushes), two, "{torn:?} {step}");
            }
            // both drawn now, the torn one over the other
            assert_eq!(layout.order().last(), Some(&torn));
            assert_eq!(layout.order().len(), 2);
            // let go there: a window of its own where it was let go, the other alone where the two were
            let held = layout.shown(torn).unwrap();
            assert!(layout.let_go());
            assert_eq!(layout.shown(torn), Some(held), "{torn:?}");
            assert!(layout.tabs_of(LayoutPiece::Layers).is_empty() && layout.tabs_of(LayoutPiece::Brushes).is_empty());
            let other = if torn == LayoutPiece::Layers { LayoutPiece::Brushes } else { LayoutPiece::Layers };
            assert_eq!(layout.shown(other).map(|s| (s.x, s.y)), Some((1216.0, 48.0)), "{torn:?}");
        }
    }

    #[test]
    fn a_window_holds_four_tabs_at_most_and_windows_side_by_side_stay_apart() {
        let layout = laid_out(SNAPPING);
        opened(&layout, LayoutPiece::Layers);
        // beside it, its top in line: not a tab
        dropped(&layout, LayoutPiece::Brushes, 1216.0 - 220.0, 48.0);
        assert!(layout.tabs_of(LayoutPiece::Brushes).is_empty());
        layout.close(LayoutPiece::Brushes);
        // three dropped on its strip, 5 under its top: four tabs, the last dropped showing
        for piece in [LayoutPiece::Actions, LayoutPiece::Adjustments, LayoutPiece::Brushes] {
            dropped(&layout, piece, 1408.0, 53.0);
        }
        let four = [LayoutPiece::Layers, LayoutPiece::Actions, LayoutPiece::Adjustments, LayoutPiece::Brushes];
        assert_eq!(layout.tabs_of(LayoutPiece::Layers), four);
        assert!(layout.shown(LayoutPiece::Brushes).is_some());
        assert!(four[..3].iter().all(|&p| layout.shown(p).is_none()));
        // the colour panel, its window its own, isn't one
        dropped(&layout, LayoutPiece::Colours, 1408.0, 53.0);
        assert!(layout.tabs_of(LayoutPiece::Colours).is_empty());
        assert!(layout.shown(LayoutPiece::Colours).is_some());
        // kept, and read back by the next start
        let kept = layout.kept();
        assert!(kept.contains("tabs layers actions adjustments *brushes"), "{kept}");
        let next = laid_out(SNAPPING);
        next.read_kept(panel_layout_key(), kept);
        assert_eq!(next.tabs_of(LayoutPiece::Actions), four);
        assert_eq!(next.shown(LayoutPiece::Brushes), layout.shown(LayoutPiece::Brushes));
        // a tab closed alone, the first showing then; a window left with one is a window again
        layout.close(LayoutPiece::Brushes);
        assert_eq!(layout.tabs_of(LayoutPiece::Layers), four[..3]);
        assert!(layout.shown(LayoutPiece::Layers).is_some());
        layout.close(LayoutPiece::Actions);
        layout.close(LayoutPiece::Adjustments);
        assert!(layout.tabs_of(LayoutPiece::Layers).is_empty());
        // a Reset Position takes a tab out as a window of its own
        dropped(&layout, LayoutPiece::Actions, 1408.0, 53.0);
        assert_eq!(layout.tabs_of(LayoutPiece::Layers).len(), 2);
        let home = layout.reset_place(LayoutPiece::Actions).unwrap();
        assert_eq!((home.x, home.y), (0.0, 48.0));
        assert!(layout.tabs_of(LayoutPiece::Layers).is_empty());
        // what doesn't read is passed over
        let lenient = laid_out(SNAPPING);
        lenient.read_kept(panel_layout_key(), "tabs layers;tabs layers nonsense;tabs actions actions".to_owned());
        assert!(lenient.tabs_of(LayoutPiece::Layers).is_empty() && lenient.tabs_of(LayoutPiece::Actions).is_empty());
    }

    #[test]
    fn panels_let_go_on_the_tool_belt_park_there_and_come_off_where_they_were() {
        let layout = laid_out(SNAPPING);
        // with no plugin saying so, no belt
        assert_eq!(layout.belt_shown(), None);
        layout.lock().look.belt = true;
        // empty, at the bottom left, a chip wide
        let empty = layout.belt_shown().unwrap();
        assert_eq!((empty.x, empty.y, empty.width, empty.height), (12.0, 1000.0 - 12.0 - 36.0, 120.0, 36.0));
        assert!(empty.pieces.is_empty());
        // the Layers panel dragged by its strip onto it: parked, kept where it was pressed for later
        let layers = opened(&layout, LayoutPiece::Layers);
        let park = |piece: LayoutPiece, from: (f32, f32), to: (f32, f32)| {
            assert!(layout.hold(piece, false, from.0, from.1));
            layout.drag(to.0, to.1);
            assert!(layout.let_go());
        };
        park(LayoutPiece::Layers, (1300.0, 60.0), (60.0, 970.0));
        assert_eq!(layout.shown(LayoutPiece::Layers), None);
        assert_eq!(layout.belt_shown().unwrap().pieces, [LayoutPiece::Layers]);
        // a second: two chips
        opened(&layout, LayoutPiece::Actions);
        park(LayoutPiece::Actions, (100.0, 60.0), (150.0, 960.0));
        let belt = layout.belt_shown().unwrap();
        assert_eq!((belt.width, belt.pieces), (240.0, vec![LayoutPiece::Layers, LayoutPiece::Actions]));
        // a tap takes one off where it was; dragged off, it goes under the pointer
        assert_eq!(layout.unpark(LayoutPiece::Layers), Some(layers));
        let off = layout.unpark_at(LayoutPiece::Actions, 800.0, 400.0, 0.0).unwrap();
        layout.let_go();
        assert_eq!((off.x, off.y), (605.0, 400.0));
        assert!(layout.belt_shown().unwrap().pieces.is_empty());
        // the colour panel, its window its own, isn't parked
        opened(&layout, LayoutPiece::Colours);
        park(LayoutPiece::Colours, (1350.0, 60.0), (60.0, 970.0));
        assert!(layout.shown(LayoutPiece::Colours).is_some());
        // kept: what was parked comes back parked
        park(LayoutPiece::Layers, (1300.0, 60.0), (60.0, 970.0));
        let kept = layout.kept();
        assert!(kept.contains("belt layers"), "{kept}");
        let next = laid_out(SNAPPING);
        next.lock().look.belt = true;
        next.read_kept(panel_layout_key(), kept);
        assert_eq!(next.belt_shown().unwrap().pieces, [LayoutPiece::Layers]);
        assert_eq!(next.shown(LayoutPiece::Layers), None);
        // moved to the top right (as a bar, by the app): what is let go there parks
        layout.unpark(LayoutPiece::Layers);
        layout.set_belt_place(Some(LayoutRect { x: 1300.0, y: 48.0, width: 120.0, height: 36.0 }));
        park(LayoutPiece::Layers, (1300.0, 60.0), (1350.0, 70.0));
        assert_eq!(layout.belt_shown().unwrap().pieces, [LayoutPiece::Layers]);
        // the belt gone (no plugin saying so): what was parked shows as a window
        layout.lock().look.belt = false;
        assert_eq!(layout.belt_shown(), None);
        assert_eq!(layout.shown(LayoutPiece::Layers), Some(layers));
    }

    #[test]
    fn the_tool_belt_keeps_to_the_room_s_bottom_left_and_a_chip_dragged_off_stays_until_let_go() {
        let layout = laid_out(SNAPPING);
        layout.lock().look.belt = true;
        // the app shows it where it always is and says so (a bar it may move)
        let shown = layout.belt_shown().unwrap();
        let tell = |b: &BeltShown| {
            let place = LayoutRect { x: b.x, y: b.y, width: b.width * b.scale, height: b.height * b.scale };
            layout.set_belt_place(Some(place));
        };
        tell(&shown);
        // two parked: two chips wide, though the place told was one chip wide
        let park = |piece: LayoutPiece, from: (f32, f32), to: (f32, f32)| {
            assert!(layout.hold(piece, false, from.0, from.1));
            layout.drag(to.0, to.1);
            assert!(layout.let_go());
        };
        opened(&layout, LayoutPiece::Layers);
        park(LayoutPiece::Layers, (1300.0, 60.0), (60.0, 970.0));
        tell(&layout.belt_shown().unwrap());
        opened(&layout, LayoutPiece::Actions);
        park(LayoutPiece::Actions, (100.0, 60.0), (150.0, 960.0));
        let two = layout.belt_shown().unwrap();
        assert_eq!((two.width, two.pieces.len()), (240.0, 2), "a second park grows the belt");
        tell(&two);
        // a chip dragged off: a window under the finger, following it, the chip staying on the belt
        // (where it was) until let go, then gone, the window where it was let go
        // (low on the window, kept inside the room: its top 1000 - 520 up, not 888 under the finger)
        let off = layout.unpark_at(LayoutPiece::Layers, 300.0, 900.0, 12.0).unwrap();
        assert_eq!((off.x + off.width / 2.0, off.y), (300.0, 1000.0 - 520.0));
        assert_eq!(layout.belt_shown().unwrap().pieces, [LayoutPiece::Layers, LayoutPiece::Actions]);
        let under = layout.drag(800.0, 300.0).unwrap();
        layout.let_go();
        assert_eq!(layout.belt_shown().unwrap().pieces, [LayoutPiece::Actions]);
        assert_eq!(layout.shown(LayoutPiece::Layers), Some(under));
        // (under the finger as it was meant to be, its grabber 12 under it: not 408 higher, as it
        // would be moved from where the room kept it at first)
        assert_eq!((under.x + under.width / 2.0, under.y), (800.0, 300.0 - 12.0));
        // turned the other way: at the new bottom left, the place told for the old way notwithstanding
        layout.set_room(FloatingRoom { left: 0.0, top: 0.0, right: 1000.0, bottom: 1600.0 }, 1000.0, 1600.0, BAR, 1.0);
        let portrait = layout.belt_shown().unwrap();
        assert_eq!((portrait.x, portrait.y), (12.0, 1600.0 - 12.0 - 36.0));
    }

    #[test]
    fn a_window_of_tabs_parks_whole_and_a_tall_window_goes_down_under_the_finger() {
        let layout = laid_out(SNAPPING);
        layout.lock().look.belt = true;
        // the library a tab of the Layers panel's window: let go on the belt, both park
        opened(&layout, LayoutPiece::Layers);
        dropped(&layout, LayoutPiece::Brushes, 1408.0, 53.0);
        assert_eq!(layout.tabs_of(LayoutPiece::Layers).len(), 2);
        assert!(layout.hold(LayoutPiece::Brushes, false, 1408.0, 60.0));
        layout.drag(60.0, 970.0);
        assert!(layout.let_go());
        assert_eq!(layout.belt_shown().unwrap().pieces, [LayoutPiece::Layers, LayoutPiece::Brushes]);
        assert!(layout.shown(LayoutPiece::Layers).is_none() && layout.shown(LayoutPiece::Brushes).is_none());
        assert!(layout.tabs_of(LayoutPiece::Layers).is_empty());
        // each taken off on its own
        layout.unpark(LayoutPiece::Brushes);
        assert!(layout.shown(LayoutPiece::Brushes).is_some());
        assert_eq!(layout.belt_shown().unwrap().pieces, [LayoutPiece::Layers]);
        // the library (as tall as its window was, over half the room) held by its strip and taken down
        // 500: 500 lower, its strip still under the finger, shorter to stay in the room; a short one
        // (the Adjustments menu) stops at the bottom
        let tall = layout.shown(LayoutPiece::Brushes).unwrap();
        assert!(tall.height > 500.0, "{tall:?}");
        assert!(layout.hold(LayoutPiece::Brushes, false, tall.x + 100.0, tall.y + 10.0));
        let down = layout.drag(tall.x + 100.0, tall.y + 10.0 + 500.0).unwrap();
        layout.let_go();
        assert_eq!((down.y, down.y + down.height), (tall.y + 500.0, 1000.0));
        assert!(down.height < tall.height);
        let short = opened(&layout, LayoutPiece::Adjustments);
        assert!(layout.hold(LayoutPiece::Adjustments, false, short.x + 100.0, short.y + 10.0));
        let stopped = layout.drag(short.x + 100.0, short.y + 10.0 + 900.0).unwrap();
        layout.let_go();
        assert_eq!((stopped.y + stopped.height, stopped.height), (1000.0, short.height));
    }

    #[test]
    fn the_reference_window_snaps_and_is_snapped_against_but_kept_by_the_app() {
        let layout = laid_out(SNAPPING);
        layout.set_reference(Some(rect(1200.0, 600.0, 400.0, 300.0)));
        // held and dragged to 10 off the bottom: onto it
        assert!(layout.hold(LayoutPiece::Reference, false, 1300.0, 610.0));
        let shown = layout.drag(1300.0, 700.0).unwrap();
        assert_eq!((shown.x, shown.y), (1200.0, 700.0));
        assert!(!layout.let_go(), "the artwork keeps it");
        // a panel dropped beside it snaps to it
        layout.set_reference(Some(rect(1200.0, 700.0, 400.0, 300.0)));
        layout.open_at(LayoutPiece::Colours, 1060.0, 712.0, 7.5).unwrap();
        let beside = layout.drag(1060.0 + 8.0, 712.0).unwrap();
        assert_eq!(beside.x + beside.width, 1200.0);
        layout.let_go();
        // and it makes room docked against the right edge
        assert_eq!(layout.canvas_insets().right, 400.0 + 260.0);
    }
}
