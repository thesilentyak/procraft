//! The interface's bars as Edit layout moves them: the top bar's left group (Gallery, Actions,
//! Adjustments, Selection, Transform), its right group (the tools, Layers and the colour), and the
//! side bar (size, modify, opacity, undo and redo). Until one is moved it is where the app always
//! has it; moved, it goes where it was let go, snapping to the window's edges and to the other bars
//! with no gap. Brought within [`TURN_IN`] of the window's left or right edge (and not of its top or
//! bottom) it stands upright against that edge, and it lies down again only once taken farther than
//! [`TURN_OUT`] from it, or with the pointer within half [`TURN_IN`] of the top or the bottom: so it
//! doesn't flicker between the two as it passes, however tall it is. It turns about the finger, the
//! place held staying under it. Each way the window is turned keeps its own; Reset puts them all
//! back.
//!
//! The app tells it each bar's place as it always has it and its size laid down (its length and its
//! thickness), the window's size and how near the bars snap, and draws each where
//! [`BarLayout::shown`] says. Everything is in points as the window measures them.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::floating::FloatingRoom;
use crate::layout::{LayoutRect, snap_moved};

/// What the bars are kept under, with the interface's other settings.
const KEY: &str = "bar_layout";
/// The kept entry naming the bars collapsed into their edges.
const COLLAPSED_WORD: &str = "collapsed";
/// A bar lying down whose end comes this near the window's left or right edge stands upright
/// against it (points) ...
pub const TURN_IN: f32 = 96.0;
/// ... and one standing upright lies down once taken farther than this from the edges (points).
pub const TURN_OUT: f32 = 176.0;

/// A bar of the interface Edit layout moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum LayoutBar {
    TopLeft,
    TopRight,
    SideBar,
    /// The Tool Belt, while a plugin shows it (see [`crate::PanelLayout::belt_shown`]).
    Belt,
}

const BARS: [LayoutBar; 4] = [LayoutBar::TopLeft, LayoutBar::TopRight, LayoutBar::SideBar, LayoutBar::Belt];

impl LayoutBar {
    fn at(self) -> usize {
        match self {
            LayoutBar::TopLeft => 0,
            LayoutBar::TopRight => 1,
            LayoutBar::SideBar => 2,
            LayoutBar::Belt => 3,
        }
    }

    fn word(self) -> &'static str {
        match self {
            LayoutBar::TopLeft => "top_left",
            LayoutBar::TopRight => "top_right",
            LayoutBar::SideBar => "side_bar",
            LayoutBar::Belt => "belt",
        }
    }
}

/// Where a bar shows: its top-left in the window, and whether it stands upright.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct BarPlace {
    pub x: f32,
    pub y: f32,
    pub vertical: bool,
}

/// Which side of a moved bar a panel opened from one of its buttons hangs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HangSide {
    /// Under a bar lying in the window's upper half (its arrow up at the button).
    Below,
    /// Over a bar lying in the window's lower half (its arrow down).
    Above,
    /// Right of a bar standing in the window's left half (its arrow pointing left).
    Right,
    /// Left of a bar standing in the window's right half (its arrow pointing right).
    Left,
}

/// Where a panel opened from a button of a moved bar goes (points as the window measures them):
/// the side of the bar it hangs on; its left edge (the arrow's, beside); its top, or its bottom
/// hanging above; how far along its edge the arrow's tip is (from its left, or from its top
/// beside); and how tall it may be.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct BarHang {
    pub side: HangSide,
    pub x: f32,
    pub y: f32,
    pub arrow: f32,
    pub room: f32,
}

/// Which sides of a bar lie against the window's edges (see [`BarLayout::touching`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct BarSides {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

/// A bar's side this near a window's edge lies against it (points).
const EDGE_NEAR: f32 = 0.5;

/// A side of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum WindowEdge {
    Left,
    Top,
    Right,
    Bottom,
}

/// A bar or a window collapsed into the window's edge (Collapse to edge): the edge it went into;
/// its tab there, which a tap or a short drag out of the edge brings it back by; and how far it slid
/// across the edge to go in (from where it shows), for the app to slide it there and back.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct EdgeCollapse {
    pub edge: WindowEdge,
    pub tab: LayoutRect,
    pub slide_x: f32,
    pub slide_y: f32,
}

/// A collapsed bar's or window's tab: this long along its edge ...
pub const COLLAPSED_TAB_LENGTH: f32 = 56.0;
/// ... and this thick across it (points).
pub const COLLAPSED_TAB_THICKNESS: f32 = 14.0;
/// A tab dragged this far out of its edge brings what went in back out (points).
pub const COLLAPSED_TAB_PULL: f32 = 14.0;
/// The collapse button: this across (points) ...
pub const COLLAPSE_BUTTON: f32 = 24.0;
/// ... this far past the end of its bar.
const COLLAPSE_BUTTON_GAP: f32 = 6.0;

/// Where something at `r` goes into `edge`: its tab on that edge of `bounds`, centred on it along
/// the edge, and how far it slides across the edge of the window (`w` by `h`) to be out of sight.
pub(crate) fn collapse_into(edge: WindowEdge, r: LayoutRect, bounds: FloatingRoom, (w, h): (f32, f32)) -> EdgeCollapse {
    let (long, thick) = (COLLAPSED_TAB_LENGTH, COLLAPSED_TAB_THICKNESS);
    let (cx, cy) = (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let along_x = (cx - long / 2.0).clamp(bounds.left, (bounds.right - long).max(bounds.left));
    let along_y = (cy - long / 2.0).clamp(bounds.top, (bounds.bottom - long).max(bounds.top));
    let (tab, slide_x, slide_y) = match edge {
        WindowEdge::Left => (LayoutRect { x: bounds.left, y: along_y, width: thick, height: long }, -r.right(), 0.0),
        WindowEdge::Right => {
            (LayoutRect { x: bounds.right - thick, y: along_y, width: thick, height: long }, w - r.x, 0.0)
        }
        WindowEdge::Top => (LayoutRect { x: along_x, y: bounds.top, width: long, height: thick }, 0.0, -r.bottom()),
        WindowEdge::Bottom => {
            (LayoutRect { x: along_x, y: bounds.bottom - thick, width: long, height: thick }, 0.0, h - r.y)
        }
    };
    EdgeCollapse { edge, tab, slide_x, slide_y }
}

/// Whether a drag of (`dx`, `dy`) from a tab on `edge` has come far enough out of it to bring back
/// what went in.
pub(crate) fn pulled_out(edge: WindowEdge, dx: f32, dy: f32) -> bool {
    let out = match edge {
        WindowEdge::Left => dx,
        WindowEdge::Right => -dx,
        WindowEdge::Top => dy,
        WindowEdge::Bottom => -dy,
    };
    out >= COLLAPSED_TAB_PULL
}

/// Beside a bar standing upright, how far over the button the panel's top starts, where it can.
const BESIDE_ABOVE: f32 = 100.0;
/// Beside it, the least room a panel is left under its top.
const BESIDE_ROOM: f32 = 320.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    bar: LayoutBar,
    from: (f32, f32),
    start: BarPlace,
    /// Whether it stands upright now, as the drag has turned it.
    upright: bool,
}

#[derive(Debug, Default)]
struct Bars {
    window: (f32, f32),
    snap: f32,
    /// Each bar's length and thickness, laid down.
    sizes: [(f32, f32); 4],
    /// Where the app always has each.
    own: [Option<BarPlace>; 4],
    /// Each moved bar's place, portrait and landscape.
    moved: [[Option<BarPlace>; 4]; 2],
    held: Option<Held>,
    editing: bool,
    /// The bars that stay as they stand, upright or lying down (an app that can't turn them).
    fixed: [bool; 4],
    /// Collapse to edge is on (a plugin says so), and the bars collapsed into their edges in each
    /// way of turning.
    collapsing: bool,
    collapsed: [[bool; 4]; 2],
}

/// The bars' places; see the module's words.
#[derive(Debug, Default, uniffi::Object)]
pub struct BarLayout {
    inner: Mutex<Bars>,
}

/// What the bars are kept under, with the interface's other settings.
#[uniffi::export]
pub fn bar_layout_key() -> String {
    KEY.to_owned()
}

#[uniffi::export]
impl BarLayout {
    #[uniffi::constructor]
    pub fn new() -> Arc<BarLayout> {
        Arc::new(BarLayout::default())
    }

    /// The bars as kept under [`bar_layout_key`], leniently: what doesn't read is passed over.
    pub fn read_kept(&self, kept: String) {
        let mut b = self.lock();
        for bar in kept.split(';') {
            let words: Vec<&str> = bar.split_whitespace().collect();
            let Some((name, places)) = words.split_first() else { continue };
            if *name == COLLAPSED_WORD {
                let mut way = None;
                for word in places {
                    match *word {
                        "P" => way = Some(0),
                        "L" => way = Some(1),
                        word => {
                            let at = BARS.iter().find(|b| b.word() == word).map(|b| b.at());
                            if let Some(slot) = way.zip(at).and_then(|(w, a)| b.collapsed.get_mut(w)?.get_mut(a)) {
                                *slot = true;
                            }
                        }
                    }
                }
                continue;
            }
            let Some(at) = BARS.iter().find(|b| b.word() == *name).map(|b| b.at()) else { continue };
            for place in places.chunks(4) {
                let [way, x, y, vertical] = place else { break };
                let way = match *way {
                    "P" => 0,
                    "L" => 1,
                    _ => continue,
                };
                let number = |w: &str| w.parse::<f32>().ok().filter(|v| v.is_finite());
                let (Some(x), Some(y)) = (number(x), number(y)) else { continue };
                if let Some(slot) = b.moved.get_mut(way).and_then(|m| m.get_mut(at)) {
                    *slot = Some(BarPlace { x, y, vertical: *vertical == "1" });
                }
            }
        }
    }

    /// The bars as kept: each moved bar, and where in each way of turning it was moved to.
    pub fn kept(&self) -> String {
        let b = self.lock();
        let mut bars = Vec::new();
        for bar in BARS {
            let mut words = vec![bar.word().to_owned()];
            for (way, letter) in [(0, "P"), (1, "L")] {
                if let Some(p) = b.moved.get(way).and_then(|m| m.get(bar.at())).copied().flatten() {
                    words.push(format!("{letter} {} {} {}", p.x, p.y, u8::from(p.vertical)));
                }
            }
            if words.len() > 1 {
                bars.push(words.join(" "));
            }
        }
        // the bars collapsed into their edges, each way of turning's (a name older readers pass over)
        let mut collapsed = vec![COLLAPSED_WORD.to_owned()];
        for (way, letter) in [(0, "P"), (1, "L")] {
            let these: Vec<&str> = BARS
                .into_iter()
                .filter(|bar| b.collapsed.get(way).and_then(|c| c.get(bar.at())).copied().unwrap_or(false))
                .map(LayoutBar::word)
                .collect();
            if !these.is_empty() {
                collapsed.push(letter.to_owned());
                collapsed.extend(these.into_iter().map(str::to_owned));
            }
        }
        if collapsed.len() > 1 {
            bars.push(collapsed.join(" "));
        }
        bars.join(";")
    }

    /// The window's size, and how near the bars snap (points; 0 for none).
    pub fn set_window(&self, width: f32, height: f32, snap: f32) {
        let mut b = self.lock();
        let was = b.landscape();
        b.window = (
            if width.is_finite() { width.max(0.0) } else { 0.0 },
            if height.is_finite() { height.max(0.0) } else { 0.0 },
        );
        b.snap = if snap.is_finite() { snap.clamp(0.0, crate::layout::MOST_SNAP) } else { 0.0 };
        if b.landscape() != was {
            b.held = None;
        }
    }

    /// `bar` where the app always has it (`own`), `length` along it and `thickness` across.
    pub fn set_bar(&self, bar: LayoutBar, own: BarPlace, length: f32, thickness: f32) {
        let mut b = self.lock();
        if [own.x, own.y, length, thickness].iter().all(|v| v.is_finite()) && length > 0.0 && thickness > 0.0 {
            if let Some(size) = b.sizes.get_mut(bar.at()) {
                *size = (length, thickness);
            }
            if let Some(slot) = b.own.get_mut(bar.at()) {
                *slot = Some(own);
            }
        }
    }

    /// Where `bar` shows: where it was moved to in this way of turning, inside the window; else where
    /// the app always has it.
    pub fn shown(&self, bar: LayoutBar) -> Option<BarPlace> {
        let b = self.lock();
        b.moved_place(bar).or_else(|| b.own.get(bar.at()).copied().flatten())
    }

    /// What `bar` covers where it shows (see [`BarLayout::shown`]).
    pub fn rect(&self, bar: LayoutBar) -> Option<LayoutRect> {
        let b = self.lock();
        let place = b.moved_place(bar).or_else(|| b.own.get(bar.at()).copied().flatten())?;
        Some(b.rect(bar, place))
    }

    /// Where a panel `width` wide (as it shows, its arrow's `arrow_height` not counted) goes opened
    /// from the button whose middle is `along` from the start of `bar` where it shows (its left,
    /// lying down; its top, standing): see [`HangSide`]. The arrow's tip `shift` past the button's
    /// middle, the panel at least `gap` inside the window. None for a bar not known.
    pub fn hang(
        &self,
        bar: LayoutBar,
        along: f32,
        width: f32,
        shift: f32,
        gap: f32,
        arrow_height: f32,
    ) -> Option<BarHang> {
        let b = self.lock();
        let place = b.moved_place(bar).or_else(|| b.own.get(bar.at()).copied().flatten())?;
        let r = b.rect(bar, place);
        let (w, h) = b.window;
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        let (along, width, shift, gap, arrow_height) =
            (finite(along), finite(width).max(0.0), finite(shift), finite(gap), finite(arrow_height));
        // the other bars where they show, and whether each stands upright
        let others: Vec<(LayoutRect, bool)> = BARS
            .into_iter()
            .filter(|&o| o != bar && !b.gone(o))
            .filter_map(|o| b.moved_place(o).or_else(|| b.own.get(o.at()).copied().flatten()).map(|p| (o, p)))
            .map(|(o, p)| (b.rect(o, p), p.vertical))
            .collect();
        if !place.vertical {
            let tip = r.x + along + shift;
            let below = r.y + r.height / 2.0 < h / 2.0;
            // the band it may take, from the bar to the window's edge: a bar standing in it on
            // either side of the button is kept clear of beside it, where the panel fits between
            let band = if below { (r.bottom(), h) } else { (0.0, r.y) };
            let (mut lo, mut hi) = (gap, w - gap);
            for (o, _) in others.iter().filter(|(o, up)| *up && o.y < band.1 && band.0 < o.bottom()) {
                if o.right() <= tip {
                    lo = lo.max(o.right() + gap);
                } else if o.x >= tip {
                    hi = hi.min(o.x - gap);
                }
            }
            if hi - lo < width {
                (lo, hi) = (gap, w - gap);
            }
            let furthest = hi - width;
            let x = if furthest >= lo { (tip - width / 2.0).clamp(lo, furthest) } else { ((w - width) / 2.0).max(0.0) };
            // as far as the next bar across the panel's way, or the window's edge: one lying, or one
            // standing over the button itself (one standing beside it was stepped round, or there was
            // no room to)
            let span = (x, x + width);
            let across = |o: &LayoutRect, up: bool| {
                o.x < span.1 && span.0 < o.right() && (!up || (o.x <= tip && tip <= o.right()))
            };
            let ways: Vec<LayoutRect> = others.iter().filter(|(o, up)| across(o, *up)).map(|(o, _)| *o).collect();
            let (side, y, room) = if below {
                let floor = ways.iter().filter(|o| o.y >= r.bottom()).map(|o| o.y).fold(h, f32::min);
                (HangSide::Below, r.bottom(), floor - r.bottom())
            } else {
                let ceiling = ways.iter().filter(|o| o.bottom() <= r.y).map(|o| o.bottom()).fold(0.0, f32::max);
                (HangSide::Above, r.y, r.y - ceiling)
            };
            return Some(BarHang { side, x, y, arrow: tip - x, room: room.max(0.0) });
        }
        let tip = r.y + along + shift;
        let right = r.x + r.width / 2.0 < w / 2.0;
        let (side, x) = if right { (HangSide::Right, r.right()) } else { (HangSide::Left, r.x - width - arrow_height) };
        // between the bars lying across its column above and below the button, or the window's edges
        let column = (x, x + width + arrow_height);
        let ways = others.iter().filter(|(o, up)| !*up && o.x < column.1 && column.0 < o.right()).map(|(o, _)| o);
        let ceiling = ways.clone().filter(|o| o.bottom() <= tip).map(|o| o.bottom()).fold(gap, f32::max);
        let floor = ways.filter(|o| o.y >= tip).map(|o| o.y).fold(h - gap, f32::min);
        let lowest = (floor - BESIDE_ROOM).max(ceiling);
        let y = (tip - BESIDE_ABOVE).clamp(ceiling, lowest);
        Some(BarHang { side, x, y, arrow: tip - y, room: (floor - y).max(0.0) })
    }

    /// Which of `bar`'s sides lie against the window's edges where it shows now (as it is dragged,
    /// too): those the app draws square, the rest rounded alike. None for a bar not known.
    pub fn touching(&self, bar: LayoutBar) -> Option<BarSides> {
        let b = self.lock();
        let place = b.moved_place(bar).or_else(|| b.own.get(bar.at()).copied().flatten())?;
        let r = b.rect(bar, place);
        let (w, h) = b.window;
        Some(BarSides {
            left: r.x <= EDGE_NEAR,
            top: r.y <= EDGE_NEAR,
            right: r.right() >= w - EDGE_NEAR,
            bottom: r.bottom() >= h - EDGE_NEAR,
        })
    }

    /// Collapse to edge on (a plugin says so), or off: off, no bar shows collapsed (each is kept as it
    /// was for when it is on again).
    pub fn set_collapsing(&self, on: bool) {
        self.lock().collapsing = on;
    }

    /// The edge `bar` goes into when collapsed, where it shows now: one it lies against, across its
    /// length (a bar standing upright goes into the side it stands against, one lying down into the
    /// top or the bottom, else the side it touches); none away from every edge, or with Collapse to
    /// edge off.
    pub fn collapse_edge(&self, bar: LayoutBar) -> Option<WindowEdge> {
        let b = self.lock();
        b.collapsing.then(|| b.edge_of(bar)).flatten()
    }

    /// Where `bar`'s collapse button goes (a disc [`COLLAPSE_BUTTON`] across): just past its end
    /// along the edge it would go into, the far end (its bottom standing, its right lying), or the
    /// near one where that runs out of the window or lies on another bar (the Tool Belt in the
    /// corner under it); with both ends taken, beside its far end, on its side away from the edge.
    /// None while it can't collapse or is collapsed.
    pub fn collapse_button(&self, bar: LayoutBar) -> Option<LayoutRect> {
        let b = self.lock();
        let edge = b.collapsing.then(|| b.edge_of(bar)).flatten()?;
        if b.is_collapsed(bar) {
            return None;
        }
        let r = b.shown_rect(bar)?;
        let (w, h) = b.window;
        let d = COLLAPSE_BUTTON;
        let gap = COLLAPSE_BUTTON_GAP;
        let rect = |x: f32, y: f32| LayoutRect { x, y, width: d, height: d };
        // past its far end, past its near end, and beside its far end
        let ways = match edge {
            WindowEdge::Left | WindowEdge::Right => {
                let x = r.x + (r.width - d) / 2.0;
                let beside = if edge == WindowEdge::Left { r.right() + gap } else { r.x - gap - d };
                [rect(x, r.bottom() + gap), rect(x, r.y - gap - d), rect(beside, r.bottom() - gap - d)]
            }
            WindowEdge::Top | WindowEdge::Bottom => {
                let y = r.y + (r.height - d) / 2.0;
                let beside = if edge == WindowEdge::Top { r.bottom() + gap } else { r.y - gap - d };
                [rect(r.right() + gap, y), rect(r.x - gap - d, y), rect(r.right() - gap - d, beside)]
            }
        };
        let inside = |c: &LayoutRect| c.x >= 0.0 && c.y >= 0.0 && c.right() <= w && c.bottom() <= h;
        let others: Vec<LayoutRect> =
            BARS.into_iter().filter(|&o| o != bar && !b.gone(o)).filter_map(|o| b.shown_rect(o)).collect();
        let clear = |c: &LayoutRect| !others.iter().any(|o| overlaps(c, o));
        let [far, near, _] = ways;
        let fits = ways.into_iter().find(|c| inside(c) && clear(c));
        // (nowhere clear: past an end that is in the window, as it was)
        Some(fits.unwrap_or(if inside(&far) { far } else { near }))
    }

    /// `bar` collapsed into its edge (see [`BarLayout::collapse_edge`]), or brought back out, in this
    /// way of turning: true when that changed something to keep.
    pub fn set_collapsed(&self, bar: LayoutBar, collapsed: bool) -> bool {
        let mut b = self.lock();
        if collapsed && (!b.collapsing || b.edge_of(bar).is_none()) {
            return false;
        }
        let way = b.way();
        let Some(slot) = b.collapsed.get_mut(way).and_then(|c| c.get_mut(bar.at())) else { return false };
        let changed = *slot != collapsed;
        *slot = collapsed;
        changed
    }

    /// `bar` collapsed into its edge, while Collapse to edge is on: the edge, its tab, and how far it
    /// slid to go in; none while it shows.
    pub fn collapsed(&self, bar: LayoutBar) -> Option<EdgeCollapse> {
        let b = self.lock();
        if !b.gone(bar) {
            return None;
        }
        let edge = b.edge_of(bar)?;
        Some(collapse_into(edge, b.shown_rect(bar)?, b.room(), b.window))
    }

    /// Where `bar` goes when it collapses (as [`BarLayout::collapsed`] would say), collapsed or not:
    /// for an app sliding it in and back out. None while it can't collapse.
    pub fn collapse_place(&self, bar: LayoutBar) -> Option<EdgeCollapse> {
        let b = self.lock();
        let edge = b.collapsing.then(|| b.edge_of(bar)).flatten()?;
        Some(collapse_into(edge, b.shown_rect(bar)?, b.room(), b.window))
    }

    /// A drag from collapsed `bar`'s tab, (`dx`, `dy`) so far: once it comes [`COLLAPSED_TAB_PULL`]
    /// out of the edge the bar comes back out. True when it did.
    pub fn pull_tab(&self, bar: LayoutBar, dx: f32, dy: f32) -> bool {
        let Some(edge) = self.collapsed(bar).map(|c| c.edge) else { return false };
        pulled_out(edge, dx, dy) && self.set_collapsed(bar, false)
    }

    /// Every bar against an edge collapsed into it (`collapsed`), or every bar brought back out: true
    /// when that changed something to keep.
    pub fn set_all_collapsed(&self, collapsed: bool) -> bool {
        let mut changed = false;
        for bar in BARS {
            changed |= self.set_collapsed(bar, collapsed);
        }
        changed
    }

    /// Whether any bar is collapsed now.
    pub fn any_collapsed(&self) -> bool {
        let b = self.lock();
        b.collapsing && BARS.into_iter().any(|bar| b.is_collapsed(bar) && b.edge_of(bar).is_some())
    }

    /// Whether `bar` was moved from where the app has it, in this way of turning.
    pub fn is_moved(&self, bar: LayoutBar) -> bool {
        self.lock().moved_place(bar).is_some()
    }

    /// `bar` stays as it stands wherever it goes (`fixed`), for an app that can't turn it; or turns
    /// against an edge, as every bar does at first.
    pub fn set_fixed(&self, bar: LayoutBar, fixed: bool) {
        if let Some(slot) = self.lock().fixed.get_mut(bar.at()) {
            *slot = fixed;
        }
    }

    /// Edit layout's handles shown on the bars (they move), or not.
    pub fn set_editing(&self, editing: bool) {
        let mut b = self.lock();
        b.editing = editing;
        if !editing {
            b.held = None;
        }
    }

    pub fn editing(&self) -> bool {
        self.lock().editing
    }

    /// `bar`'s handle pressed with the pointer at (`x`, `y`), while editing: it moves with the
    /// pointer until let go.
    pub fn hold(&self, bar: LayoutBar, x: f32, y: f32) -> bool {
        let mut b = self.lock();
        let start = b.moved_place(bar).or_else(|| b.own.get(bar.at()).copied().flatten());
        let Some(start) = start.filter(|_| b.editing && x.is_finite() && y.is_finite()) else { return false };
        // (collapsed, it comes back out to be moved)
        let way = b.way();
        if let Some(slot) = b.collapsed.get_mut(way).and_then(|c| c.get_mut(bar.at())) {
            *slot = false;
        }
        b.held = Some(Held { bar, from: (x, y), start, upright: start.vertical });
        true
    }

    /// The pointer at (`x`, `y`) with a bar held: where it shows now.
    pub fn drag(&self, x: f32, y: f32) -> Option<BarPlace> {
        let mut b = self.lock();
        let held = b.held?;
        if !(x.is_finite() && y.is_finite()) {
            return Some(held.start);
        }
        let place = b.dragged(held, x - held.from.0, y - held.from.1);
        if let Some(h) = b.held.as_mut() {
            h.upright = place.vertical;
        }
        let way = b.way();
        if let Some(slot) = b.moved.get_mut(way).and_then(|m| m.get_mut(held.bar.at())) {
            *slot = Some(place);
        }
        Some(place)
    }

    /// The bar held, if one is.
    pub fn holding(&self) -> Option<LayoutBar> {
        self.lock().held.map(|h| h.bar)
    }

    /// The bar held let go: true when there is something new to keep.
    pub fn let_go(&self) -> bool {
        self.lock().held.take().is_some()
    }

    /// Every bar back where the app always has it, in both ways of turning.
    pub fn reset(&self) {
        let mut b = self.lock();
        b.moved = [[None; 4]; 2];
        b.collapsed = [[false; 4]; 2];
        b.held = None;
    }
}

impl BarLayout {
    /// The bars as `kept` has them, and only those: the others where they always are (a named
    /// layout chosen).
    pub(crate) fn replace_kept(&self, kept: &str) {
        {
            let mut b = self.lock();
            b.moved = Default::default();
            b.collapsed = Default::default();
            b.held = None;
        }
        self.read_kept(kept.to_owned());
    }

    fn lock(&self) -> MutexGuard<'_, Bars> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Bars {
    fn landscape(&self) -> bool {
        self.window.0 >= self.window.1
    }

    fn way(&self) -> usize {
        usize::from(self.landscape())
    }

    fn room(&self) -> FloatingRoom {
        FloatingRoom { left: 0.0, top: 0.0, right: self.window.0, bottom: self.window.1 }
    }

    /// What `bar` covers at `place`.
    fn rect(&self, bar: LayoutBar, place: BarPlace) -> LayoutRect {
        let (length, thickness) = self.sizes.get(bar.at()).copied().unwrap_or((0.0, 0.0));
        let (width, height) = if place.vertical { (thickness, length) } else { (length, thickness) };
        LayoutRect { x: place.x, y: place.y, width, height }
    }

    /// `bar` at `place`, inside the window (from its top-left where it doesn't fit).
    fn inside(&self, bar: LayoutBar, place: BarPlace) -> BarPlace {
        let r = self.rect(bar, place);
        let x = place.x.min(self.window.0 - r.width).max(0.0);
        let y = place.y.min(self.window.1 - r.height).max(0.0);
        BarPlace { x, y, ..place }
    }

    /// What `bar` covers where it shows (moved, or where the app has it).
    fn shown_rect(&self, bar: LayoutBar) -> Option<LayoutRect> {
        let place = self.moved_place(bar).or_else(|| self.own.get(bar.at()).copied().flatten())?;
        Some(self.rect(bar, place))
    }

    /// Whether `bar` is kept collapsed in this way of turning.
    fn is_collapsed(&self, bar: LayoutBar) -> bool {
        self.collapsed.get(self.way()).and_then(|c| c.get(bar.at())).copied().unwrap_or(false)
    }

    /// Whether `bar` is collapsed out of sight now (it takes no room, and other bars and panels may
    /// go where it was).
    fn gone(&self, bar: LayoutBar) -> bool {
        self.collapsing && self.is_collapsed(bar) && self.edge_of(bar).is_some()
    }

    /// The edge `bar` would go into where it shows (see [`BarLayout::collapse_edge`]).
    fn edge_of(&self, bar: LayoutBar) -> Option<WindowEdge> {
        let place = self.moved_place(bar).or_else(|| self.own.get(bar.at()).copied().flatten())?;
        let r = self.rect(bar, place);
        let (w, h) = self.window;
        let (left, top) = (r.x <= EDGE_NEAR, r.y <= EDGE_NEAR);
        let (right, bottom) = (r.right() >= w - EDGE_NEAR, r.bottom() >= h - EDGE_NEAR);
        let sides = [(left, WindowEdge::Left), (right, WindowEdge::Right)];
        let ends = [(top, WindowEdge::Top), (bottom, WindowEdge::Bottom)];
        let order: Vec<(bool, WindowEdge)> = if place.vertical {
            sides.into_iter().chain(ends).collect()
        } else {
            ends.into_iter().chain(sides).collect()
        };
        order.into_iter().find(|(on, _)| *on).map(|(_, edge)| edge)
    }

    fn moved_place(&self, bar: LayoutBar) -> Option<BarPlace> {
        let p = self.moved.get(self.way()).and_then(|m| m.get(bar.at())).copied().flatten()?;
        Some(self.inside(bar, p))
    }

    /// `held` dragged (`dx`, `dy`): moved, upright or lying down as it comes near the edges (see the
    /// module's words), snapped to the window's edges and the other bars, inside the window.
    fn dragged(&self, held: Held, dx: f32, dy: f32) -> BarPlace {
        let bar = held.bar;
        let room = self.room();
        let others: Vec<LayoutRect> = BARS
            .into_iter()
            .filter(|&o| o != bar && !self.gone(o))
            .filter_map(|o| {
                let p = self.moved_place(o).or_else(|| self.own.get(o.at()).copied().flatten())?;
                Some(self.rect(o, p))
            })
            .collect();
        // snapped to the window's edges and the other bars, then apart from those it would overlap,
        // inside the window
        let placed = |vertical: bool, x: f32, y: f32| {
            let inside = self.inside(bar, BarPlace { x, y, vertical });
            let snapped = snap_moved(self.rect(bar, inside), &others, room, -1.0, self.snap);
            let apart = apart(snapped, &others, room);
            self.inside(bar, BarPlace { x: apart.x, y: apart.y, vertical })
        };
        let start = held.start;
        let (x, y) = (start.x + dx, start.y + dy);
        if self.fixed.get(bar.at()).copied().unwrap_or(false) {
            return placed(start.vertical, x, y);
        }
        let thickness = self.sizes.get(bar.at()).map_or(0.0, |s| s.1);
        // the pointer, and where it holds the bar: along its length from its start, and across it
        // (turning, the bar turns about the finger, the place held staying under it)
        let pointer = (held.from.0 + dx, held.from.1 + dy);
        let (along, across) = if start.vertical {
            (held.from.1 - start.y, held.from.0 - start.x)
        } else {
            (held.from.0 - start.x, held.from.1 - start.y)
        };
        let (lying_x, lying_y) = (pointer.0 - along, pointer.1 - across);
        let lying = self.rect(bar, self.inside(bar, BarPlace { x: lying_x, y: lying_y, vertical: false }));
        let near_side = |r: &LayoutRect| (r.x - room.left).min(room.right - r.right());
        let near_end = |r: &LayoutRect| (r.y - room.top).min(room.bottom - r.bottom());
        // lying, it stands up once its end comes within TURN_IN of a side (and it is not within
        // TURN_IN of the top or the bottom); standing, it lies down only once it would lie with its
        // end past TURN_OUT from the sides, or with the pointer within half TURN_IN of the top or the
        // bottom (the pointer's, not the bar's: a bar taller than the window leaves room for is never
        // far from both). Both by where it would lie, so one never undoes the other on the next move.
        let end_of_pointer = (pointer.1 - room.top).min(room.bottom - pointer.1);
        let upright = if held.upright {
            near_side(&lying) <= TURN_OUT && end_of_pointer > TURN_IN / 2.0
        } else {
            near_side(&lying) <= TURN_IN && near_end(&lying) > TURN_IN
        };
        if upright {
            // against the side it is nearest, whatever the drag's x
            let x = if pointer.0 - room.left <= room.right - pointer.0 { room.left } else { room.right - thickness };
            placed(true, x, pointer.1 - along)
        } else {
            placed(false, lying_x, lying_y)
        }
    }
}

/// Whether `a` and `b` overlap (more than touch).
fn overlaps(a: &LayoutRect, b: &LayoutRect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// `rect` moved out of each of `others` it overlaps, the least way out that stays inside `room` (a
/// few times over, for one it is pushed into); as it was where it can't be got out.
fn apart(rect: LayoutRect, others: &[LayoutRect], room: FloatingRoom) -> LayoutRect {
    let fits = |r: &LayoutRect| {
        r.x >= room.left - 0.5
            && r.right() <= room.right + 0.5
            && r.y >= room.top - 0.5
            && r.bottom() <= room.bottom + 0.5
    };
    let mut r = rect;
    for _ in 0..4 {
        let Some(o) = others.iter().find(|o| overlaps(&r, o)) else { return r };
        let ways = [
            LayoutRect { x: o.x - r.width, ..r },
            LayoutRect { x: o.right(), ..r },
            LayoutRect { y: o.y - r.height, ..r },
            LayoutRect { y: o.bottom(), ..r },
        ];
        let moved = |w: &LayoutRect| (w.x - r.x).abs() + (w.y - r.y).abs();
        let Some(best) = ways.iter().filter(|w| fits(w)).min_by(|a, b| moved(a).total_cmp(&moved(b))) else {
            return rect;
        };
        r = *best;
    }
    if others.iter().any(|o| overlaps(&r, o)) { rect } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A window 1600 by 1000 snapping within 16, the top bar's groups 420 and 300 long and 48 thick
    /// at its top corners, the side bar 519 long and 51 thick on its left.
    fn bars() -> Arc<BarLayout> {
        let bars = BarLayout::new();
        bars.set_window(1600.0, 1000.0, 16.0);
        bars.set_bar(LayoutBar::TopLeft, BarPlace { x: 0.0, y: 0.0, vertical: false }, 420.0, 48.0);
        bars.set_bar(LayoutBar::TopRight, BarPlace { x: 1300.0, y: 0.0, vertical: false }, 300.0, 48.0);
        bars.set_bar(LayoutBar::SideBar, BarPlace { x: 0.0, y: 240.0, vertical: true }, 519.0, 51.0);
        bars.set_editing(true);
        bars
    }

    fn at(x: f32, y: f32, vertical: bool) -> BarPlace {
        BarPlace { x, y, vertical }
    }

    #[test]
    fn a_bar_is_where_the_app_has_it_until_moved_and_moves_only_while_editing() {
        let bars = bars();
        assert_eq!(bars.shown(LayoutBar::TopLeft), Some(at(0.0, 0.0, false)));
        assert_eq!(bars.rect(LayoutBar::SideBar), Some(LayoutRect { x: 0.0, y: 240.0, width: 51.0, height: 519.0 }));
        assert!(!bars.is_moved(LayoutBar::TopLeft));
        bars.set_editing(false);
        assert!(!bars.hold(LayoutBar::TopLeft, 10.0, 10.0));
        assert_eq!(bars.drag(300.0, 300.0), None);
        bars.set_editing(true);
        assert!(bars.hold(LayoutBar::TopLeft, 10.0, 10.0));
        // along the top it stays lying down, snapping to the right group with no gap
        let moved = bars.drag(10.0 + 870.0, 10.0).unwrap();
        assert_eq!(moved, at(880.0, 0.0, false));
        assert!(bars.let_go());
        assert!(bars.is_moved(LayoutBar::TopLeft));
    }

    #[test]
    fn a_bar_near_a_side_stands_upright_and_lies_down_again_only_well_away_from_it() {
        let bars = bars();
        // the left half dragged down the left edge: once clear of the top, upright against the edge,
        // 48 wide and 420 tall
        bars.hold(LayoutBar::TopLeft, 10.0, 10.0);
        assert_eq!(bars.drag(10.0, 10.0 + 90.0).unwrap(), at(0.0, 90.0, false), "still near the top");
        let upright = bars.drag(10.0, 10.0 + 400.0).unwrap();
        // (beside the side bar, which stands there: never over it)
        assert_eq!(upright, at(51.0, 400.0, true));
        // taken 150 out from the edge it stays upright (within 176), against the edge...
        let held_out = bars.drag(10.0 + 150.0, 410.0).unwrap();
        assert_eq!(held_out, at(51.0, 400.0, true));
        // ...and lies down past 176
        let free = bars.drag(10.0 + 200.0, 410.0).unwrap();
        assert_eq!(free, at(200.0, 400.0, false));
        // brought back to 90 from the right edge, lying down: upright against it
        let right = bars.drag(10.0 + 1600.0 - 420.0 - 90.0, 410.0).unwrap();
        assert_eq!(right, at(1600.0 - 48.0, 400.0, true));
        bars.let_go();
        // the side bar dragged near the bottom: lying down along it, 519 long and 51 thick
        bars.hold(LayoutBar::SideBar, 20.0, 300.0);
        let down = bars.drag(620.0, 300.0 + 1000.0).unwrap();
        // (lying under the pointer, held where it was held: 60 down its length, 20 across it)
        assert_eq!(down, at(620.0 - 60.0, 1000.0 - 51.0, false));
        // into the bottom left corner: near a side and an end both, it stays as it lies
        let corner = bars.drag(20.0, 300.0 + 1000.0).unwrap();
        assert_eq!(corner, at(0.0, 949.0, false));
        bars.let_go();
        // one the app can't turn stays upright wherever it goes
        bars.set_fixed(LayoutBar::TopRight, true);
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        let kept = bars.drag(1400.0 + 150.0, 10.0 + 400.0).unwrap();
        assert!(!kept.vertical, "{kept:?}");
    }

    #[test]
    fn a_panel_hangs_from_its_button_where_the_bar_was_moved() {
        let bars = bars();
        // the right half where it always is: the Layers panel, 384 wide, under its button 230 along
        let under = bars.hang(LayoutBar::TopRight, 230.0, 384.0, 2.0, 12.0, 12.0).unwrap();
        assert_eq!(under, BarHang { side: HangSide::Below, x: 1204.0, y: 48.0, arrow: 328.0, room: 952.0 });
        // near the window's right edge it stays 12 inside, the arrow still at the button
        assert_eq!(under.x + 384.0, 1588.0);
        // moved to the bottom: over the bar, as tall as the room above it
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        bars.drag(1400.0 - 700.0, 10.0 + 945.0);
        bars.let_go();
        let over = bars.hang(LayoutBar::TopRight, 230.0, 384.0, 2.0, 12.0, 12.0).unwrap();
        assert_eq!(over, BarHang { side: HangSide::Above, x: 640.0, y: 952.0, arrow: 192.0, room: 952.0 });
        // standing against the left edge: right of it, its arrow at the button
        bars.hold(LayoutBar::TopLeft, 10.0, 10.0);
        bars.drag(10.0, 10.0 + 400.0);
        bars.let_go();
        let beside = bars.hang(LayoutBar::TopLeft, 150.0, 390.0, 2.0, 12.0, 12.0).unwrap();
        assert_eq!(beside, BarHang { side: HangSide::Right, x: 51.0 + 48.0, y: 452.0, arrow: 100.0, room: 536.0 });
        // against the right edge: left of it, the arrow's room taken off too; low down, its top
        // goes no lower than leaves it 320 of room
        bars.hold(LayoutBar::TopLeft, 10.0, 410.0);
        assert!(bars.drag(10.0 + 1600.0, 410.0).unwrap().vertical);
        bars.let_go();
        let left = bars.hang(LayoutBar::TopLeft, 400.0, 390.0, 2.0, 12.0, 12.0).unwrap();
        assert_eq!((left.side, left.x), (HangSide::Left, 1600.0 - 48.0 - 390.0 - 12.0));
        assert_eq!((left.y, left.room), (1000.0 - 12.0 - 320.0, 320.0));
        assert_eq!(left.arrow, 400.0 + 402.0 - left.y);
    }

    #[test]
    fn a_panel_hanging_from_a_moved_bar_keeps_clear_of_the_side_bar_standing_beside_it() {
        let bars = bars();
        // the side bar taken to the right edge (1549..1600 by 240..759)...
        bars.hold(LayoutBar::SideBar, 20.0, 400.0);
        assert_eq!(bars.drag(20.0 + 1560.0, 400.0).unwrap(), at(1549.0, 240.0, true));
        bars.let_go();
        // ...and the tools half to the bottom right, short of it (1200..1500 by 952..1000)
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        assert_eq!(bars.drag(1300.0, 10.0 + 952.0).unwrap(), at(1200.0, 952.0, false));
        bars.let_go();
        // Layers up from it (the button 230 along, the panel 384 wide): it would reach 1588 and cover
        // the side bar's foot; it stops 12 short of it instead, the arrow still at the button, the
        // room up to the window's top (the side bar beside it cuts none)
        let up = bars.hang(LayoutBar::TopRight, 230.0, 384.0, 2.0, 12.0, 12.0).unwrap();
        assert_eq!((up.side, up.x, up.y, up.room), (HangSide::Above, 1549.0 - 12.0 - 384.0, 952.0, 952.0));
        assert_eq!(up.x + up.arrow, 1200.0 + 230.0 + 2.0);
        assert!(up.x + 384.0 <= 1549.0 - 12.0);
        // the left half above it on the left (0..420) and the side bar left of a button: stepped
        // round the same way on the left
        bars.reset();
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        bars.drag(1400.0 - 1300.0, 10.0 + 952.0);
        bars.let_go();
        let r = bars.rect(LayoutBar::TopRight).unwrap();
        assert_eq!((r.x, r.y), (0.0, 952.0));
        let left = bars.hang(LayoutBar::TopRight, 30.0, 384.0, 2.0, 12.0, 12.0).unwrap();
        // (the side bar, 0..51, stands over the button at 32: the room stops under it, 952 - 759)
        assert_eq!((left.x, left.room), (12.0, 952.0 - 759.0));
        let further = bars.hang(LayoutBar::TopRight, 130.0, 384.0, 2.0, 12.0, 12.0).unwrap();
        // (one further along, right of the side bar: beside it, and the room to the left half's foot)
        assert_eq!((further.x, further.room), (51.0 + 12.0, 952.0 - 48.0));
    }

    #[test]
    fn a_panel_beside_an_upright_bar_keeps_under_a_bar_lying_across_its_way() {
        let bars = bars();
        // the tools half upright against the right edge (1552..1600 by 110..410: held 100 along it,
        // standing, its top 100 over the finger)...
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        assert_eq!(bars.drag(1560.0, 10.0 + 200.0).unwrap(), at(1552.0, 110.0, true));
        bars.let_go();
        // ...and the left half along the top right over its panels' way (1100..1520 by 0..48)
        bars.hold(LayoutBar::TopLeft, 10.0, 10.0);
        assert_eq!(bars.drag(10.0 + 1100.0, 10.0).unwrap(), at(1100.0, 0.0, false));
        bars.let_go();
        // its first button's panel would start 100 over the button (at 42), into the left half: it
        // starts at that half's foot instead, the room down to 12 from the window's bottom
        let beside = bars.hang(LayoutBar::TopRight, 30.0, 384.0, 2.0, 12.0, 12.0).unwrap();
        assert_eq!((beside.side, beside.x, beside.y), (HangSide::Left, 1552.0 - 384.0 - 12.0, 48.0));
        assert_eq!((beside.arrow, beside.room), (110.0 + 32.0 - 48.0, 1000.0 - 12.0 - 48.0));
        // with the left half back where it was, the panel starts 100 over the button again
        bars.hold(LayoutBar::TopLeft, 1110.0, 10.0);
        bars.drag(10.0, 10.0);
        bars.let_go();
        assert_eq!(bars.hang(LayoutBar::TopRight, 30.0, 384.0, 2.0, 12.0, 12.0).unwrap().y, 42.0);
    }

    #[test]
    fn a_bar_turns_about_the_finger_and_stands_in_a_window_too_short_to_leave_it_room() {
        // a window 1024 by 640 (a tablet's, wider than tall), the side bar 518 long: it can't stand
        // 96 clear of both the top and the bottom
        let bars = BarLayout::new();
        bars.set_window(1024.0, 640.0, 16.0);
        bars.set_bar(LayoutBar::TopLeft, at(0.0, 0.0, false), 420.0, 48.0);
        bars.set_bar(LayoutBar::TopRight, at(724.0, 0.0, false), 300.0, 48.0);
        bars.set_bar(LayoutBar::SideBar, at(0.0, 61.0, true), 518.0, 51.0);
        bars.set_editing(true);
        // held at its middle (25, 320) and taken across to the right edge at the same height, step by
        // step: lying down once out of the left side, the finger on it; upright against the right side,
        // the finger still at its middle; never flicking between the two on the way
        assert!(bars.hold(LayoutBar::SideBar, 25.0, 320.0));
        let mut turns = 0;
        let mut was = true;
        for step in 1..=40 {
            let x = 25.0 + step as f32 * 25.0;
            let place = bars.drag(x, 320.0).unwrap();
            let r = bars.rect(LayoutBar::SideBar).unwrap();
            if place.vertical != was {
                turns += 1;
                was = place.vertical;
            }
            if !place.vertical {
                assert!(r.x <= x && x <= r.right() && r.y <= 320.0 && 320.0 <= r.bottom(), "{x}: {r:?}");
            }
        }
        assert_eq!(turns, 2, "out of the left side lying down, then standing at the right");
        let right = bars.shown(LayoutBar::SideBar).unwrap();
        assert!(right.vertical);
        assert_eq!(right.x, 1024.0 - 51.0);
        // and down along it: standing all the way, its middle under the finger as far as the window lets
        for y in [360.0, 400.0, 440.0, 380.0, 320.0, 280.0] {
            let place = bars.drag(1010.0, y).unwrap();
            assert!(place.vertical, "{y}");
        }
        bars.let_go();
    }

    #[test]
    fn a_bar_against_a_side_collapses_into_it_leaving_a_tab_and_its_room() {
        let bars = bars();
        // Collapse to edge off: nothing goes in
        assert_eq!(bars.collapse_edge(LayoutBar::SideBar), None);
        assert!(!bars.set_collapsed(LayoutBar::SideBar, true));
        assert_eq!(bars.collapse_button(LayoutBar::SideBar), None);
        bars.set_collapsing(true);
        // the side bar (0..51 by 240..759, upright against the left side) goes into the left; the
        // halves, lying along the top, up into it
        assert_eq!(bars.collapse_edge(LayoutBar::SideBar), Some(WindowEdge::Left));
        assert_eq!(bars.collapse_edge(LayoutBar::TopLeft), Some(WindowEdge::Top));
        // their buttons, 24 across: 6 under the side bar's foot, centred across it; 6 past the left
        // half's right end; left of the right half, whose right end is the window's
        let button = |x: f32, y: f32| Some(LayoutRect { x, y, width: 24.0, height: 24.0 });
        assert_eq!(bars.collapse_button(LayoutBar::SideBar), button(13.5, 765.0));
        assert_eq!(bars.collapse_button(LayoutBar::TopLeft), button(426.0, 12.0));
        assert_eq!(bars.collapse_button(LayoutBar::TopRight), button(1270.0, 12.0));
        // collapsed: its tab on the left side across from its middle (499.5), 14 by 56, and slid its
        // own width, 51, to the left to go in; no button
        assert!(bars.set_collapsed(LayoutBar::SideBar, true));
        assert!(!bars.set_collapsed(LayoutBar::SideBar, true), "already in");
        let tab = LayoutRect { x: 0.0, y: 499.5 - 28.0, width: 14.0, height: 56.0 };
        let slid = EdgeCollapse { edge: WindowEdge::Left, tab, slide_x: -51.0, slide_y: 0.0 };
        assert_eq!(bars.collapsed(LayoutBar::SideBar), Some(slid));
        assert_eq!(bars.collapse_button(LayoutBar::SideBar), None);
        assert!(bars.any_collapsed());
        // its room given back: the left half brought down the left side stands against it where the
        // side bar was (0, not 51 beside it as it is with the side bar showing)
        bars.hold(LayoutBar::TopLeft, 10.0, 10.0);
        assert_eq!(bars.drag(10.0, 10.0 + 400.0).unwrap(), at(0.0, 400.0, true));
        bars.let_go();
        // a drag from its tab: 10 out of the side isn't enough, nor any into it; 14 out is
        assert!(!bars.pull_tab(LayoutBar::SideBar, 10.0, 0.0));
        assert!(!bars.pull_tab(LayoutBar::SideBar, -30.0, 0.0));
        assert!(bars.pull_tab(LayoutBar::SideBar, 14.0, 3.0));
        assert_eq!(bars.collapsed(LayoutBar::SideBar), None);
        // (where it goes, said while it shows too, for the app to slide it there and back)
        assert_eq!(bars.collapse_place(LayoutBar::SideBar), Some(slid));
        // a bar away from every edge has no button and doesn't go in
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        bars.drag(1400.0 - 500.0, 10.0 + 300.0);
        bars.let_go();
        assert_eq!(bars.collapse_edge(LayoutBar::TopRight), None);
        assert_eq!(bars.collapse_button(LayoutBar::TopRight), None);
        assert!(!bars.set_collapsed(LayoutBar::TopRight, true));
        // one lying against the bottom goes down into it
        bars.hold(LayoutBar::TopRight, 1000.0, 320.0);
        bars.drag(1000.0, 320.0 + 700.0);
        bars.let_go();
        assert_eq!(bars.collapse_edge(LayoutBar::TopRight), Some(WindowEdge::Bottom));
        assert!(bars.set_collapsed(LayoutBar::TopRight, true));
        let down = bars.collapsed(LayoutBar::TopRight).unwrap();
        assert_eq!((down.tab.y, down.tab.height, down.slide_x, down.slide_y), (1000.0 - 14.0, 14.0, 0.0, 48.0));
    }

    #[test]
    fn a_collapse_button_keeps_off_the_tool_belt_and_the_other_bars() {
        // the side bar moved down the left side onto the Tool Belt in the corner under it (12..132 by
        // 952..988): its foot on the belt's top, 433..952
        let bars = bars();
        bars.set_bar(LayoutBar::Belt, at(12.0, 952.0, false), 120.0, 36.0);
        bars.set_collapsing(true);
        bars.hold(LayoutBar::SideBar, 20.0, 300.0);
        bars.drag(20.0, 700.0);
        bars.let_go();
        let side = bars.rect(LayoutBar::SideBar).unwrap();
        assert_eq!((side.x, side.y, side.bottom()), (0.0, 433.0, 952.0));
        // its button would lie on the belt under its foot: over its top instead, 6 above it
        let button = bars.collapse_button(LayoutBar::SideBar).unwrap();
        assert_eq!((button.x, button.y), (13.5, 433.0 - 6.0 - 24.0));

        // a tablet's window, 1024 by 640, as measured: the top bar 44 thick, the side bar 518.4 long
        // and 51 thick, the belt in the corner; the side bar brought down onto the belt (73.6..592)
        // has no room past either end (its top comes 0.4 short of the top bar's foot), so its button
        // goes beside its foot, 6 out from it and 6 up from its end
        let tablet = BarLayout::new();
        tablet.set_window(1024.0, 640.0, 16.0);
        tablet.set_bar(LayoutBar::TopLeft, at(10.0, 0.0, false), 400.0, 44.0);
        tablet.set_bar(LayoutBar::TopRight, at(700.0, 0.0, false), 314.0, 44.0);
        tablet.set_bar(LayoutBar::SideBar, at(0.0, 61.0, true), 518.4, 51.0);
        tablet.set_bar(LayoutBar::Belt, at(12.0, 592.0, false), 120.0, 36.0);
        tablet.set_editing(true);
        tablet.set_collapsing(true);
        tablet.hold(LayoutBar::SideBar, 25.0, 300.0);
        tablet.drag(25.0, 560.0);
        tablet.let_go();
        let side = tablet.rect(LayoutBar::SideBar).unwrap();
        assert_eq!((side.x, side.bottom()), (0.0, 592.0), "{side:?}");
        assert!(side.y < 44.0 + 30.0, "{side:?}");
        let button = tablet.collapse_button(LayoutBar::SideBar).unwrap();
        assert_eq!((button.x, button.y), (51.0 + 6.0, 592.0 - 6.0 - 24.0));
        // and wherever it stands down the left side, its button is on no other bar, in the window
        let others = [LayoutBar::TopLeft, LayoutBar::TopRight, LayoutBar::Belt];
        for y in (0..=14).map(|i| 300.0 + i as f32 * 20.0) {
            tablet.hold(LayoutBar::SideBar, 25.0, 300.0);
            tablet.drag(25.0, y);
            tablet.let_go();
            let Some(button) = tablet.collapse_button(LayoutBar::SideBar) else { continue };
            for o in others {
                let r = tablet.rect(o).unwrap();
                assert!(!overlaps(&button, &r), "{y}: {button:?} on {o:?} {r:?}");
            }
            assert!(button.x >= 0.0 && button.y >= 0.0 && button.right() <= 1024.0 && button.bottom() <= 640.0);
        }
    }

    #[test]
    fn what_is_collapsed_is_kept_for_each_way_of_turning_and_all_go_at_once() {
        let bars = bars();
        bars.set_collapsing(true);
        // all at once: the three bars, each against an edge
        assert!(bars.set_all_collapsed(true));
        let three = [LayoutBar::TopLeft, LayoutBar::TopRight, LayoutBar::SideBar];
        assert!(three.iter().all(|&b| bars.collapsed(b).is_some()));
        assert!(!bars.set_all_collapsed(true), "nothing more to do");
        // kept (the window is wider than tall), and read back by the next start
        let kept = bars.kept();
        assert_eq!(kept, "collapsed L top_left top_right side_bar");
        let next = super::tests::bars();
        next.set_collapsing(true);
        next.read_kept(kept);
        assert!(three.iter().all(|&b| next.collapsed(b).is_some()));
        // turned the other way: nothing collapsed there
        next.set_window(1000.0, 1600.0, 16.0);
        assert!(three.iter().all(|&b| next.collapsed(b).is_none()));
        // Collapse to edge turned off: everything shows, and is collapsed again when it is back on
        bars.set_collapsing(false);
        assert!(!bars.any_collapsed() && bars.collapsed(LayoutBar::SideBar).is_none());
        bars.set_collapsing(true);
        assert!(bars.any_collapsed());
        // held in Edit Layout, a collapsed bar comes back out to be moved
        bars.hold(LayoutBar::SideBar, 20.0, 300.0);
        assert_eq!(bars.collapsed(LayoutBar::SideBar), None);
        bars.let_go();
        // open all; Reset brings everything back out too
        assert!(bars.set_all_collapsed(false));
        assert!(!bars.any_collapsed());
        bars.set_all_collapsed(true);
        bars.reset();
        assert!(!bars.any_collapsed());
        assert_eq!(bars.kept(), "");
    }

    #[test]
    fn a_bar_is_square_only_on_the_sides_against_the_window_edges_as_it_moves() {
        let bars = bars();
        let sides = |left, top, right, bottom| Some(BarSides { left, top, right, bottom });
        // where the app has them: the side bar against the left edge, the halves against the top
        assert_eq!(bars.touching(LayoutBar::SideBar), sides(true, false, false, false));
        assert_eq!(bars.touching(LayoutBar::TopLeft), sides(true, true, false, false));
        assert_eq!(bars.touching(LayoutBar::TopRight), sides(false, true, true, false));
        // the side bar dragged: still against the edge while upright, then free (rounded all round)
        // the moment it lies down, against the bottom, and against the right edge standing again
        bars.hold(LayoutBar::SideBar, 20.0, 300.0);
        bars.drag(20.0 + 150.0, 300.0);
        assert_eq!(bars.touching(LayoutBar::SideBar), sides(true, false, false, false));
        bars.drag(20.0 + 400.0, 300.0);
        assert_eq!(bars.touching(LayoutBar::SideBar), sides(false, false, false, false));
        bars.drag(620.0, 300.0 + 1000.0);
        assert_eq!(bars.touching(LayoutBar::SideBar), sides(false, false, false, true));
        bars.drag(20.0 + 1560.0, 400.0);
        assert_eq!(bars.touching(LayoutBar::SideBar), sides(false, false, true, false));
        bars.let_go();
        assert_eq!(bars.touching(LayoutBar::SideBar), sides(false, false, true, false));
        // a half in the middle: free
        bars.hold(LayoutBar::TopLeft, 10.0, 10.0);
        bars.drag(10.0 + 500.0, 10.0 + 400.0);
        bars.let_go();
        assert_eq!(bars.touching(LayoutBar::TopLeft), sides(false, false, false, false));
        // a bar the app hasn't told of: none
        assert_eq!(bars.touching(LayoutBar::Belt), None);
    }

    #[test]
    fn a_panel_hanging_over_a_bar_has_the_room_up_to_the_bar_above_it() {
        let bars = bars();
        // the tools half moved to the bottom left, its panel's way crossing the left half (0..420 by 0..48)
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        bars.drag(1400.0 - 1200.0, 10.0 + 945.0);
        bars.let_go();
        let r = bars.rect(LayoutBar::TopRight).unwrap();
        assert_eq!((r.x, r.y), (100.0, 952.0));
        let over = bars.hang(LayoutBar::TopRight, 130.0, 384.0, 2.0, 12.0, 12.0).unwrap();
        assert_eq!((over.side, over.room), (HangSide::Above, 952.0 - 48.0));
        // moved on clear of it: the room to the window's top
        bars.hold(LayoutBar::TopRight, 150.0, 970.0);
        bars.drag(150.0 + 600.0, 970.0);
        bars.let_go();
        let clear = bars.hang(LayoutBar::TopRight, 130.0, 384.0, 2.0, 12.0, 12.0).unwrap();
        assert_eq!((clear.x, clear.room), (640.0, 952.0));
    }

    #[test]
    fn a_bar_standing_upright_stays_against_its_edge_every_step_of_the_drag() {
        let bars = bars();
        // the tools half brought to the right edge: upright against it...
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        let first = bars.drag(1400.0 + 160.0, 10.0 + 400.0).unwrap();
        assert_eq!((first.x, first.vertical), (1600.0 - 48.0, true));
        // ...and there at every step after, however the drag goes along or a little out
        for (dx, dy) in [(170.0, 420.0), (100.0, 440.0), (60.0, 460.0), (30.0, 480.0)] {
            let step = bars.drag(1400.0 + dx, 10.0 + dy).unwrap();
            assert_eq!((step.x, step.vertical), (1600.0 - 48.0, true), "({dx}, {dy})");
        }
        // only taken past 176 from the edge does it lie down, under the pointer as it was held
        let free = bars.drag(1400.0 - 300.0, 10.0 + 480.0).unwrap();
        assert!(!free.vertical, "{free:?}");
        assert_eq!(free.x, 1400.0 - 300.0 - 100.0);
    }

    #[test]
    fn bars_snap_apart_and_never_overlap() {
        let bars = bars();
        // the tools half dropped across the side bar (0..51 by 240..759): out beside it, not over it
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        let place = bars.drag(1400.0 - 1290.0, 10.0 + 400.0).unwrap();
        bars.let_go();
        let r = bars.rect(LayoutBar::TopRight).unwrap();
        let side = bars.rect(LayoutBar::SideBar).unwrap();
        let over = r.x < side.right() && side.x < r.right() && r.y < side.bottom() && side.y < r.bottom();
        assert!(!over, "{place:?} over the side bar {side:?}");
        assert_eq!(r.x, side.right(), "beside it with no gap");
    }

    #[test]
    fn the_tool_belt_moves_as_a_bar_lying_along_the_bottom_or_top_and_upright_against_a_side() {
        let bars = bars();
        // two chips at the bottom left, 12 in
        bars.set_bar(LayoutBar::Belt, BarPlace { x: 12.0, y: 1000.0 - 12.0 - 36.0, vertical: false }, 240.0, 36.0);
        assert_eq!(bars.rect(LayoutBar::Belt), Some(LayoutRect { x: 12.0, y: 952.0, width: 240.0, height: 36.0 }));
        // taken along the bottom it stays lying, its bottom on the window's
        bars.hold(LayoutBar::Belt, 50.0, 970.0);
        assert_eq!(bars.drag(50.0 + 600.0, 970.0 + 30.0).unwrap(), at(612.0, 1000.0 - 36.0, false));
        // brought to the right edge, upright against it
        let upright = bars.drag(50.0 + 1600.0, 970.0 - 500.0).unwrap();
        assert!(upright.vertical, "{upright:?}");
        assert_eq!(bars.rect(LayoutBar::Belt).map(|r| (r.width, r.height)), Some((36.0, 240.0)));
        bars.let_go();
        // kept with the others, and back at the bottom left on Reset
        let next = BarLayout::new();
        next.read_kept(bars.kept());
        assert!(bars.kept().contains("belt"), "{}", bars.kept());
        bars.reset();
        assert!(!bars.is_moved(LayoutBar::Belt));
        assert_eq!(bars.shown(LayoutBar::Belt), Some(at(12.0, 952.0, false)));
    }

    #[test]
    fn each_way_of_turning_keeps_its_own_and_reset_puts_them_back() {
        let bars = bars();
        bars.hold(LayoutBar::TopRight, 1400.0, 10.0);
        bars.drag(1400.0, 10.0 + 945.0);
        bars.let_go();
        let landscape = bars.shown(LayoutBar::TopRight).unwrap();
        assert_eq!(landscape, at(1300.0, 952.0, false));
        // portrait: where the app has it
        bars.set_window(1000.0, 1600.0, 16.0);
        bars.set_bar(LayoutBar::TopRight, at(700.0, 0.0, false), 300.0, 48.0);
        assert_eq!(bars.shown(LayoutBar::TopRight), Some(at(700.0, 0.0, false)));
        let kept = bars.kept();
        assert_eq!(kept, "top_right L 1300 952 0");
        let next = BarLayout::new();
        next.read_kept(kept);
        next.set_window(1600.0, 1000.0, 16.0);
        next.set_bar(LayoutBar::TopRight, at(1300.0, 0.0, false), 300.0, 48.0);
        assert_eq!(next.shown(LayoutBar::TopRight), Some(landscape));
        next.reset();
        assert_eq!(next.shown(LayoutBar::TopRight), Some(at(1300.0, 0.0, false)));
        assert_eq!(next.kept(), "");
        // what doesn't read is passed over
        next.read_kept("nothing L 1 2 0;top_left X 1 2 0;top_left L nan 2 0;side_bar L 5".to_owned());
        assert_eq!(next.kept(), "");
    }
}
