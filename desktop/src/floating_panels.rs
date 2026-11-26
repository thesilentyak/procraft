//! The Layers panel and the brush library torn off into windows of their own, while a plugin lets
//! them float, as the Android app has them (ui/FloatingPanels.kt): each dragged out by a grabber at
//! its top as the colour panel is, then a window over the canvas that stays open while one draws,
//! moved by the strip at its top, sized from its lower right corner and closed by its Close (its
//! button then hangs it from the bar again). Inside, the panel is the same as where it hangs. Where
//! each window is, and how big, is the core's to work out ([`procraft_core::PanelLayout`]): while a
//! plugin lays the panels out, they snap to the window's edges and to each other, fold to a tab by
//! a button beside Close, and those docked may make room for the canvas. The layout is kept with
//! the preferences, a place for each way the window is turned (wider than tall, or not).

use std::sync::Arc;

use egui::emath::TSTransform;
use egui::{Align2, Pos2, Rect, Sense, pos2, vec2};
use procraft_core::{FloatingPanel, FloatingRoom, LayoutPiece, PanelLayout, PieceShown, Plugins};

use crate::look::{Look, Weight, argb, text};

/// The panels that may float, each by its place in [`Floating::allowed`].
pub const PANELS: [FloatingPanel; 2] = [FloatingPanel::Layers, FloatingPanel::Brushes];

/// How far a panel's grabber is dragged before it floats, and how far down its window's grabber is
/// (at the window's own size): what is held as it is dragged out.
const UNDOCK_DRAG: f32 = 30.0;
const GRAB_Y: f32 = 12.0;

/// The floating panels' places (the core's), and what the plugins let them do.
pub struct Floating {
    pub layout: Arc<PanelLayout>,
    /// Which the plugins let float now, as they said at their generation.
    allowed: [bool; 2],
    allowed_for: Option<u64>,
    /// Something changed that is kept with the preferences.
    pub changed: bool,
}

impl Default for Floating {
    fn default() -> Floating {
        Floating { layout: PanelLayout::new(), allowed: [false; 2], allowed_for: None, changed: false }
    }
}

/// `panel`'s place in [`PANELS`].
fn at(panel: FloatingPanel) -> usize {
    panel as usize
}

/// The piece of the layout `panel` is.
pub fn piece(panel: FloatingPanel) -> LayoutPiece {
    match panel {
        FloatingPanel::Layers => LayoutPiece::Layers,
        FloatingPanel::Brushes => LayoutPiece::Brushes,
    }
}

impl Floating {
    /// The layout as it was kept (none: as it starts), and the windows kept before there were
    /// layouts, which a way of turning with nothing kept starts from.
    pub fn kept(layout: Option<&str>, old: [Option<procraft_core::FloatingWindow>; 2]) -> Floating {
        let floating = Floating::default();
        if let Some(kept) = layout {
            floating.layout.read_kept(procraft_core::panel_layout_key(), kept.to_owned());
        }
        for (panel, window) in PANELS.into_iter().zip(old) {
            if let Some(window) = window {
                floating.layout.read_kept(procraft_core::floating_key(panel), procraft_core::floating_kept(window));
            }
        }
        floating
    }

    /// Which panels float, and how they are laid out, as `plugins` say now (asked again only when
    /// they change).
    pub fn follow(&mut self, plugins: &Arc<Plugins>) {
        let now = plugins.generation();
        if self.allowed_for != Some(now) {
            self.allowed_for = Some(now);
            self.allowed = PANELS.map(|p| plugins.floats(p));
        }
        self.layout.follow(Arc::clone(plugins));
    }

    /// Whether a plugin lets `panel` float.
    pub fn allowed(&self, panel: FloatingPanel) -> bool {
        self.allowed.get(at(panel)).copied().unwrap_or(false)
    }

    /// `panel`'s window while it shows: torn off and not closed, and a plugin lets it float.
    pub fn shown(&self, panel: FloatingPanel) -> Option<PieceShown> {
        self.layout.shown(piece(panel))
    }

    /// The room the floating windows go in this frame, told to the layout before any shows.
    pub fn place(&self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let room = room(ctx);
        let bar = crate::panel::bar_shown(ctx);
        self.layout.set_room(room, screen.width(), screen.height(), bar, scale(ctx));
    }

    /// `panel`'s grabber, hanging from its button, dragged from `from` to `pointer` (the window's
    /// points): past [`UNDOCK_DRAG`] it floats, its window as big as it was last left, its grabber
    /// under the pointer and following it until let go; true then (the panel that hung closes).
    pub fn undock(&mut self, panel: FloatingPanel, from: Pos2, pointer: Pos2) -> bool {
        if !self.allowed(panel) || pointer.distance(from) <= UNDOCK_DRAG {
            return false;
        }
        self.layout.open_at(piece(panel), pointer.x, pointer.y, GRAB_Y).is_some()
    }

    /// `piece` folded to its tab, or unfolded.
    pub fn fold(&mut self, piece: LayoutPiece, folded: bool) {
        self.layout.set_folded(piece, folded);
        self.changed = true;
    }

    /// `piece`'s strip or tab (or, `sizing`, its corner) pressed with the pointer at `pointer` (the
    /// window's points): it moves (or is sized) with the pointer until let go.
    pub fn hold(&mut self, piece: LayoutPiece, sizing: bool, pointer: Pos2) {
        self.layout.hold(piece, sizing, pointer.x, pointer.y);
    }

    /// A piece held follows the pointer while it is down, kept once let go.
    pub fn follow_pointer(&mut self, ctx: &egui::Context) {
        if self.layout.holding().is_none() {
            return;
        }
        let (pointer, down) = ctx.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
        match pointer {
            Some(p) if down => {
                self.layout.drag(p.x, p.y);
                ctx.request_repaint();
            }
            _ => self.changed |= self.layout.let_go(),
        }
    }

    /// Where `window` shows, and the space it is drawn in: laid out at its own size from its
    /// top-left, shown at the floating windows' size. Its rect is in that space.
    pub fn space(ctx: &egui::Context, window: PieceShown) -> (Rect, crate::panel::PanelSpace) {
        let k = window.scale;
        let corner = pos2(window.x, window.y);
        let rect = Rect::from_min_size(corner, vec2(window.width, window.height));
        let to_window = TSTransform::from_translation(corner.to_vec2())
            * TSTransform::from_scaling(k)
            * TSTransform::from_translation(-corner.to_vec2());
        (rect, crate::panel::PanelSpace { to_window, screen: to_window.inverse().mul_rect(ctx.content_rect()) })
    }
}

/// A floating panel folded to its tab, `title` and an unfold mark, where `shown` says, at the
/// floating windows' size: what was done to it this frame (a press on it, to move it with the
/// pointer from there; a tap, to unfold it).
pub fn tab(ctx: &egui::Context, look: &mut Look, shown: PieceShown, title: &str, backdrops: &mut Vec<f32>) -> TabTouch {
    let id = egui::Id::unique("floating tab").with(title);
    let (window, space) = Floating::space(ctx, shown);
    crate::panel::drawing_panel(ctx, Some(space));
    let mut touch = TabTouch::default();
    egui::Area::new(id).fixed_pos(window.min).order(egui::Order::Middle).show(ctx, |ui| {
        crate::panel::floating_panel_frame(ui, look, "floating tab", window, backdrops);
        crate::screen::claim(ui, window);
        let held = ui.interact(window, ui.scope_id().with("tab"), Sense::click_and_drag());
        touch.pressed = held.drag_started();
        touch.unfold = held.clicked();
        let ink = argb(crate::panel::PANEL_TITLE);
        text(
            ui.painter(),
            pos2(window.left() + 14.0, window.center().y),
            Align2::LEFT_CENTER,
            title,
            15.0,
            Weight::SemiBold,
            ink,
        );
        // the unfold mark: a chevron opening downward
        let c = pos2(window.right() - 21.0, window.center().y);
        let stroke = egui::Stroke::new(1.6, ink);
        ui.painter().line_segment([c + vec2(-7.0, -3.0), c + vec2(0.0, 3.0)], stroke);
        ui.painter().line_segment([c + vec2(0.0, 3.0), c + vec2(7.0, -3.0)], stroke);
    });
    crate::panel::drawing_panel(ctx, None);
    touch
}

/// What was done to a folded panel's tab this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TabTouch {
    pub pressed: bool,
    pub unfold: bool,
}

/// How big the floating windows show next to the rest of the interface, as a plugin says.
fn scale(ctx: &egui::Context) -> f32 {
    crate::panel::part_scale(ctx, procraft_core::InterfacePart::Floating)
}

/// Where the floating windows may go: the window, clear of the side bar as the panels are.
fn room(ctx: &egui::Context) -> FloatingRoom {
    let screen = ctx.content_rect();
    let (mut left, mut right) = (screen.left(), screen.right());
    if let Some(bar) = crate::panel::side_bar_shown_at(ctx) {
        if bar.center().x < screen.center().x {
            left = left.max(bar.right() + crate::panel::POPOVER_GAP);
        } else {
            right = right.min(bar.left() - crate::panel::POPOVER_GAP);
        }
    }
    FloatingRoom { left, top: screen.top(), right: right.max(left), bottom: screen.bottom() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_floats_only_while_a_plugin_lets_it_and_it_isn_t_closed() {
        let window = procraft_core::FloatingWindow { open: true, x: 10.0, y: 20.0, width: 384.0, height: 520.0 };
        let mut floating = Floating::kept(None, [Some(window), None]);
        let room = FloatingRoom { left: 0.0, top: 0.0, right: 1600.0, bottom: 1000.0 };
        floating.layout.set_room(room, 1600.0, 1000.0, 48.0, 1.0);
        // no plugin lets it
        assert_eq!(floating.shown(FloatingPanel::Layers), None);
        floating.allowed = [true, true];
        let plugins = Plugins::new();
        // (a list with none on lets none float: the layout asks the list)
        floating.layout.follow(Arc::clone(&plugins));
        assert_eq!(floating.shown(FloatingPanel::Layers), None);
        // kept closed where it was, by its Close
        floating.layout.close(LayoutPiece::Layers);
        assert!(!floating.layout.is_open(LayoutPiece::Layers));
        // what is kept reads back
        let kept = floating.layout.kept();
        let again = Floating::kept(Some(&kept), [None, None]);
        assert_eq!(again.layout.kept(), kept);
    }
}
