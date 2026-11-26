//! Collapse to edge, as the desktop shows it: a bar or a window sliding into a side of the window
//! and back out, the slim tab left on the side, and the button that sends it in. The core says
//! where each goes and what is collapsed; this draws it and hears the taps.

use egui::{Id, Rect, Sense, Vec2, pos2, vec2};
use procraft_core::{EdgeCollapse, LayoutRect, WindowEdge};

use crate::look::{argb, pc};

/// How long a slide into a side or back out takes (seconds), easing out either way.
pub const SLIDE_TIME: f32 = 0.2;

/// How far into its side something collapsing is (0 showing, 1 gone), easing out as it goes either
/// way, under `id`.
pub fn slid(ctx: &egui::Context, id: Id, collapsed: bool) -> f32 {
    let t = ctx.animate_bool_with_time(id, collapsed, SLIDE_TIME);
    eased(t, collapsed)
}

/// `t` (0 to 1, as far as the time) eased out toward where it goes: going in (`collapsed`), fast and
/// then slowing; coming out, the same from the other end.
pub fn eased(t: f32, collapsed: bool) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if collapsed { 1.0 - (1.0 - t) * (1.0 - t) } else { t * t }
}

/// How far something going `slid` of the way into its side (as `place` says) is moved.
pub fn offset(place: &EdgeCollapse, slid: f32) -> Vec2 {
    vec2(place.slide_x, place.slide_y) * slid
}

/// `r` as egui has it.
pub fn rect(r: LayoutRect) -> Rect {
    Rect::from_min_size(pos2(r.x, r.y), vec2(r.width, r.height))
}

/// A chevron at `c`, `size` across, pointing into `edge`.
fn chevron(painter: &egui::Painter, c: egui::Pos2, edge: WindowEdge, size: f32, ink: egui::Color32) {
    let (along, out) = match edge {
        WindowEdge::Left => (vec2(0.0, 1.0), vec2(-1.0, 0.0)),
        WindowEdge::Right => (vec2(0.0, 1.0), vec2(1.0, 0.0)),
        WindowEdge::Top => (vec2(1.0, 0.0), vec2(0.0, -1.0)),
        WindowEdge::Bottom => (vec2(1.0, 0.0), vec2(0.0, 1.0)),
    };
    let half = size / 2.0;
    let tip = c + out * half * 0.5;
    let stroke = egui::Stroke::new(1.6, ink);
    painter.line_segment([tip - out * half + along * half, tip], stroke);
    painter.line_segment([tip, tip - out * half - along * half], stroke);
}

/// A bar's or a window's collapse button at `rect` (the window's points): a disc on the bars' glass,
/// a chevron in it pointing into `edge`. Whether it was tapped.
pub fn button(ctx: &egui::Context, id: Id, rect: Rect, edge: WindowEdge) -> bool {
    let mut tapped = false;
    egui::Area::new(id).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
        crate::screen::claim(ui, rect);
        ui.painter().circle_filled(rect.center(), rect.width() / 2.0, argb(pc::BAR));
        chevron(ui.painter(), rect.center(), edge, rect.width() * 0.4, argb(pc::BAR_GLYPH));
        tapped = ui.interact(rect, id.with("tap"), Sense::click()).clicked();
    });
    tapped
}

/// [`button`] drawn in `ui` (a window's strip): whether it was tapped.
pub fn strip_button(ui: &egui::Ui, rect: Rect, edge: WindowEdge) -> bool {
    let c = rect.center();
    ui.painter().circle_filled(c, 8.5, argb(crate::panel::CLOSE_DISC));
    chevron(ui.painter(), c, edge, 7.0, argb(crate::panel::CLOSE_CROSS));
    ui.interact(rect, ui.scope_id().with(("collapse", rect.min.x as i32)), Sense::click()).clicked()
}

/// What was done to a collapsed bar's or window's tab this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TabTouch {
    /// Tapped: what went in comes back out.
    pub tapped: bool,
    /// Dragged: how far from where it was pressed (the core says when that brings it out).
    pub pulled: Option<Vec2>,
}

/// The tab of what went into a side (`collapse`): a slim pill on the side with a grip across it,
/// fading in as what went in gets there (`slid`).
pub fn tab(ctx: &egui::Context, id: Id, collapse: &EdgeCollapse, slid: f32) -> TabTouch {
    let r = rect(collapse.tab);
    let mut touch = TabTouch::default();
    egui::Area::new(id).fixed_pos(r.min).order(egui::Order::Foreground).show(ctx, |ui| {
        crate::screen::claim(ui, r);
        let corner = r.width().min(r.height()) / 2.0;
        ui.painter().rect_filled(r, corner, argb(pc::BAR).gamma_multiply(slid));
        // the grip: a short line along the tab
        let along = if r.height() > r.width() { vec2(0.0, 1.0) } else { vec2(1.0, 0.0) };
        let half = along * r.width().max(r.height()) * 0.25;
        let ink = argb(pc::BAR_GLYPH).gamma_multiply(slid);
        ui.painter().line_segment([r.center() - half, r.center() + half], egui::Stroke::new(2.0, ink));
        let held = ui.interact(r, id.with("tab"), Sense::click_and_drag());
        touch.tapped = held.clicked();
        if held.dragged()
            && let (Some(p), Some(from)) = (held.interact_pointer_pos(), ui.input(|i| i.pointer.press_origin()))
        {
            touch.pulled = Some(p - from);
        }
    });
    touch
}

#[cfg(test)]
mod tests {
    use super::eased;

    /// The slide eases out both ways: most of the way in the first half of its time, and the last
    /// stretch slowly; coming out, the same from the other end.
    #[test]
    fn a_slide_eases_out_going_in_and_coming_out() {
        assert_eq!((eased(0.0, true), eased(1.0, true)), (0.0, 1.0));
        assert_eq!((eased(1.0, false), eased(0.0, false)), (1.0, 0.0));
        // halfway through its time going in, three quarters of the way there
        assert!((eased(0.5, true) - 0.75).abs() < 1e-6);
        // coming out (t falling from 1), halfway through its time three quarters of the way back
        assert!((eased(0.5, false) - 0.25).abs() < 1e-6);
        // never past either end
        assert_eq!((eased(-1.0, true), eased(2.0, true)), (0.0, 1.0));
    }
}
