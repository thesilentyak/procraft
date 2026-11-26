//! The panels hanging from the top bar, as the Android app's (ui/Popover.kt, ui/AppRoot.kt): a
//! frosted body with an arrow pointing up at its button, a soft shadow around it, and a header.

use egui::emath::TSTransform;
use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, Vec2, pos2};
use procraft_core::{HangSide, InterfacePart};
use resvg::tiny_skia;

use crate::icons;
use crate::look::{Look, Shade, Themed, Weight, argb, fill, pc, round_rect, text};

/// A panel's own material over the canvas: what is behind it shows through, blurred.
pub const POPOVER_FROST: Themed = Themed(0xDE1E1E1E, 0xFFE6E6E6);
pub const POPOVER_SHADOW: Themed = Themed(0x38000000, 0x24000000);
pub const POPOVER_SHADOW_RADIUS: f32 = 51.0;
pub const POPOVER_CORNER: f32 = 35.0;
pub const POPOVER_GAP: f32 = 12.0;
pub const ARROW_HEIGHT: f32 = 12.0;
pub const ARROW_SHIFT: f32 = 2.0;
pub const PANEL_TITLE: Themed = Themed(0xFFC7C7C7, 0xFF393939);
pub const PANEL_GLYPH: Themed = Themed(0xFFC8C8C8, 0xFF393939);
pub const CHEVRON_DISC: Themed = Themed(0xFF4C4C4C, 0xFFCFCFCF);
pub const CHEVRON_GLYPH: Themed = Themed(0xFF2E2E2E, 0xFFE6E6E6);

/// The strip a floating panel's window adds over the panel (its grabber and Close), and under it
/// (its corner, which sizes it).
pub const FLOATING_TOP: f32 = 24.0;
pub const FLOATING_BOTTOM: f32 = 20.0;
const GRABBER: Themed = Themed(0x7AC6C6C6, 0xFFB0B0B0);
/// Close: a disc of white over the window with its cross cut out of it (drawn darker).
pub const CLOSE_DISC: u32 = 0x26FFFFFF;
pub const CLOSE_CROSS: u32 = 0x4D000000;

/// Where a panel shows: hanging from its button (where the button shows, in the panels' space),
/// or floating in a window of its own at this rect (in its own space, see
/// [`crate::floating_panels`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hang {
    Button(Rect),
    Floating(Rect),
}

/// What a floating panel's strips had done to them this frame: the grabber strip pressed (its
/// window moves with the pointer from there), the corner pressed (it is sized), Close tapped, the
/// fold button tapped.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FloatingChrome {
    pub moving: bool,
    pub sizing: bool,
    pub closed: bool,
    pub folded: bool,
    /// What its menu chose.
    pub act: Option<WindowAct>,
    /// Holding several panels: the tab tapped, or the one torn out and where the pointer is.
    pub pick: Option<procraft_core::LayoutPiece>,
    pub tear: Option<(procraft_core::LayoutPiece, Pos2)>,
    /// Its collapse button tapped (Collapse to edge).
    pub collapse: bool,
}

/// The side the window drawn next goes into at its collapse button's tap (Collapse to edge), if it
/// may; and whether Collapse All in its menu reads Open All (some window or bar is collapsed), or
/// none with Collapse to edge off.
pub fn window_collapse(ctx: &egui::Context, edge: Option<procraft_core::WindowEdge>, all: Option<bool>) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::unique("window collapse edge"), edge);
        d.insert_temp(egui::Id::unique("window collapse all"), all);
    });
}

fn collapse_edge(ctx: &egui::Context) -> Option<procraft_core::WindowEdge> {
    ctx.data(|d| d.get_temp::<Option<procraft_core::WindowEdge>>(egui::Id::unique("window collapse edge"))).flatten()
}

fn collapse_all(ctx: &egui::Context) -> Option<bool> {
    ctx.data(|d| d.get_temp::<Option<bool>>(egui::Id::unique("window collapse all"))).flatten()
}

/// The tabs of the window drawn next, while it holds several panels: each, its name, and whether
/// it shows; none for a window holding one.
pub fn window_tabs(ctx: &egui::Context, tabs: Vec<(procraft_core::LayoutPiece, &'static str, bool)>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("window tabs"), tabs));
}

/// How far a tab is dragged before it is torn out of its window.
const TAB_TEAR: f32 = 26.0;

/// Where `count` tabs go across the strip of a window at `window`: between its menu button on the
/// left and Close (and `buttons` more beside it: Fold, Collapse to edge's) on the right, each up to
/// 90 wide, narrower as more share the room.
pub(crate) fn tab_cells(window: Rect, count: usize, buttons: usize) -> Vec<Rect> {
    // (the menu's 40 and Close's, and each 40 beside it, 14 in from the window's side, and 4 clear)
    let (left, right) = (window.left() + 58.0, window.right() - 58.0 - 40.0 * buttons as f32);
    let room = (right - left).max(0.0);
    let width = if count == 0 { 0.0 } else { (room / count as f32).min(90.0) };
    let start = (left + right) / 2.0 - width * count as f32 / 2.0;
    (0..count)
        .map(|i| {
            Rect::from_min_size(
                pos2(start + width * i as f32 + 2.0, window.top() + 1.0),
                egui::vec2((width - 4.0).max(0.0), 22.0),
            )
        })
        .collect()
}

/// The tabs across a window's strip at `window` (see [`window_tabs`]): the one tapped, and the one
/// torn out with where the pointer is (the window's points).
fn strip_tabs(
    ui: &Ui,
    window: Rect,
    buttons: usize,
) -> (Option<procraft_core::LayoutPiece>, Option<(procraft_core::LayoutPiece, Pos2)>) {
    let tabs = ui
        .ctx()
        .data(|d| d.get_temp::<Vec<(procraft_core::LayoutPiece, &'static str, bool)>>(egui::Id::unique("window tabs")))
        .unwrap_or_default();
    let (mut pick, mut tear) = (None, None);
    let cells = tab_cells(window, tabs.len(), buttons);
    for ((piece, title, active), r) in tabs.iter().zip(cells) {
        if *active {
            ui.painter().rect_filled(r, 11, Color32::from_rgba_unmultiplied(0, 118, 253, 90));
        }
        // (a title longer than its tab cut at its ends)
        let ink = argb(PANEL_TITLE);
        text(&ui.painter().with_clip_rect(r), r.center(), Align2::CENTER_CENTER, title, 13.0, Weight::Regular, ink);
        // (each by its panel, not its place: one torn out, the next sliding into its place isn't
        // taken for it as the drag goes on)
        let touched = ui.interact(r, ui.scope_id().with(("window tab", *piece)), Sense::click_and_drag());
        if touched.clicked() {
            pick = Some(*piece);
        }
        if touched.dragged()
            && let (Some(p), Some(from)) = (touched.interact_pointer_pos(), ui.input(|i| i.pointer.press_origin()))
        {
            let p = to_window(ui.ctx()) * p;
            if p.distance(from) > TAB_TEAR {
                tear = Some((*piece, p));
            }
        }
    }
    (pick, tear)
}

/// What a floating window's menu chose: fold it, hide it, pin it over the others (or not), or put it
/// back where it first goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAct {
    Fold,
    Hide,
    Pin,
    Reset,
    /// Collapse All (or Open All): every bar and window against a side into it, or back out.
    CollapseAll,
}

/// Whether the window drawn next is pinned over the others (its menu says Unpin).
pub fn window_pinned(ctx: &egui::Context, pinned: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("window pinned"), pinned));
}

/// A window's menu button at `at` (three dots on a disc), and while it is open its menu under it:
/// Fold (while the panels fold), Hide, Pin to Top (Unpin while pinned) and Reset Position. What was
/// chosen this frame.
pub fn window_menu(ui: &Ui, at: Rect) -> Option<WindowAct> {
    let c = at.center();
    ui.painter().circle_filled(c, 8.5, argb(CLOSE_DISC));
    for dx in [-3.6, 0.0, 3.6] {
        ui.painter().circle_filled(c + egui::vec2(dx, 0.0), 1.1, argb(CLOSE_CROSS));
    }
    let ctx = ui.ctx().clone();
    let id = ui.scope_id().with("window menu");
    let mut open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let pressed = ui.interact(at, id.with("button"), Sense::click()).clicked();
    if pressed {
        open = !open;
    }
    let mut act = None;
    if open {
        let pinned = ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("window pinned"))).unwrap_or(false);
        let mut rows = Vec::new();
        if panels_fold(&ctx) {
            rows.push((WindowAct::Fold, "Fold"));
        }
        rows.push((WindowAct::Hide, "Hide"));
        rows.push((WindowAct::Pin, if pinned { "Unpin" } else { "Pin to Top" }));
        rows.push((WindowAct::Reset, "Reset Position"));
        if let Some(any) = collapse_all(&ctx) {
            rows.push((WindowAct::CollapseAll, if any { "Open All" } else { "Collapse All" }));
        }
        // in the window's points, over everything
        let origin = to_window(&ctx) * at.left_bottom();
        let menu = Rect::from_min_size(origin, egui::vec2(180.0, 8.0 + 40.0 * rows.len() as f32));
        egui::Area::new(id.with("menu")).fixed_pos(menu.min).order(egui::Order::Tooltip).show(&ctx, |ui| {
            ui.painter().rect_filled(menu, 12, argb(pc::BAR));
            crate::screen::claim(ui, menu);
            for (i, (choice, label)) in rows.iter().enumerate() {
                let row = Rect::from_min_size(
                    pos2(menu.left(), menu.top() + 4.0 + 40.0 * i as f32),
                    egui::vec2(menu.width(), 40.0),
                );
                if ui.interact(row, id.with(("row", i)), Sense::click()).clicked() {
                    act = Some(*choice);
                }
                text(
                    ui.painter(),
                    pos2(row.left() + 16.0, row.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    17.0,
                    Weight::Regular,
                    argb(pc::MENU_INK),
                );
            }
        });
        // a press anywhere else closes it
        let away = ctx.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !menu.contains(p)));
        if act.is_some() || (away && !pressed) {
            open = false;
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, open));
    act
}

/// The window a panel drawn now floats in (every panel may, while a plugin lets it), in the panels'
/// space, and whether it has a corner to size it by; or none.
pub fn floating_in(ctx: &egui::Context, window: Option<(Rect, bool)>) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::unique("panel floating in"), window);
        d.insert_temp(egui::Id::unique("panel float chrome"), FloatingChrome::default());
        d.insert_temp(egui::Id::unique("panel float wants"), None::<f32>);
    });
}

fn float_window(ctx: &egui::Context) -> Option<(Rect, bool)> {
    ctx.data(|d| d.get_temp::<Option<(Rect, bool)>>(egui::Id::unique("panel floating in"))).flatten()
}

/// What was done to the strips of the panel floating as a window this frame (see [`floating_in`]).
pub fn float_chrome(ctx: &egui::Context) -> FloatingChrome {
    ctx.data(|d| d.get_temp::<FloatingChrome>(egui::Id::unique("panel float chrome"))).unwrap_or_default()
}

/// How tall the panel floating as a window wants its body to be (its own points, its arrow's room
/// included), as it laid itself out this frame: for one that isn't sized, what the layout is told.
pub fn float_wants(ctx: &egui::Context) -> Option<f32> {
    ctx.data(|d| d.get_temp::<Option<f32>>(egui::Id::unique("panel float wants"))).flatten()
}

/// Whether the panel hanging now may be torn off (its grabber shows at its top).
pub fn tearing(ctx: &egui::Context, may: bool) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::unique("panel tears"), may);
        d.insert_temp(egui::Id::unique("panel torn"), None::<(Pos2, Pos2)>);
    });
}

/// The hanging panel's grabber dragged this frame: where the press began and where the pointer is
/// (the window's points).
pub fn torn(ctx: &egui::Context) -> Option<(Pos2, Pos2)> {
    ctx.data(|d| d.get_temp::<Option<(Pos2, Pos2)>>(egui::Id::unique("panel torn"))).flatten()
}

/// A toolbar or an adjustment's controls being drawn now, `piece` in the layout, and where its
/// top-left shows while it floats (the window's points); or none.
pub fn drawing_bottom(ctx: &egui::Context, bottom: Option<(procraft_core::LayoutPiece, Option<Pos2>)>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("bottom drawn"), bottom));
}

/// `rect`, where the toolbar or adjustment's controls drawn now always is: where it shows, at the
/// place the layout has it while it floats. Where it always is is kept for the interface (its size,
/// its grabber and its strip), with the frame it was told in.
pub fn bottom_rect(ctx: &egui::Context, rect: Rect) -> Rect {
    let Some((piece, at)) = ctx
        .data(|d| d.get_temp::<Option<(procraft_core::LayoutPiece, Option<Pos2>)>>(egui::Id::unique("bottom drawn")))
        .flatten()
    else {
        return rect;
    };
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("bottom home").with(piece), (rect, frame)));
    at.map_or(rect, |p| Rect::from_min_size(p, rect.size()))
}

/// Where `piece` (a toolbar or an adjustment's controls) always is, if it was drawn this frame or
/// the one before (the Selection and Transform toolbars are drawn after the windows' strips, which
/// ask).
pub fn bottom_home(ctx: &egui::Context, piece: procraft_core::LayoutPiece) -> Option<Rect> {
    let (rect, frame) = ctx.data(|d| d.get_temp::<(Rect, u64)>(egui::Id::unique("bottom home").with(piece)))?;
    (frame + 1 >= ctx.cumulative_frame_nr()).then_some(rect)
}

/// Whether the floating panels have a fold button this frame, as the plugins laying them out say.
pub fn set_panels_fold(ctx: &egui::Context, fold: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("panels fold"), fold));
}

/// Whether the floating panels have a fold button this frame.
pub fn panels_fold(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("panels fold"))).unwrap_or(false)
}

/// Whether every panel is drawn solid this frame: while a plugin lays the panels out, a window lies
/// over the bars and the other windows and a panel over a window, and only the canvas blurs behind
/// the frost (what else is under it would show through sharp).
pub fn set_panels_solid(ctx: &egui::Context, solid: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("panels solid"), solid));
}

/// Whether every panel is drawn solid this frame.
fn panels_solid(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("panels solid"))).unwrap_or(false)
}

/// A panel's frost: the interface's (what is behind showing through, blurred), or wholly `solid`.
fn frost_colour(solid: bool) -> Color32 {
    solid_if(argb(POPOVER_FROST), solid)
}

/// `frost`, or its tint wholly opaque while `solid`.
fn solid_if(frost: Color32, solid: bool) -> Color32 {
    // (its tint as written, not as it is kept, multiplied by how much of it shows)
    let [r, g, b, _] = frost.to_srgba_unmultiplied();
    if solid { Color32::from_rgb(r, g, b) } else { frost }
}

/// Fold: a disc of white with a bar across it, at `rect`'s middle; whether it was tapped.
pub fn fold_button(ui: &Ui, rect: Rect) -> bool {
    let c = rect.center();
    ui.painter().circle_filled(c, 8.5, argb(CLOSE_DISC));
    ui.painter()
        .line_segment([c - egui::vec2(3.6, 0.0), c + egui::vec2(3.6, 0.0)], egui::Stroke::new(1.3, argb(CLOSE_CROSS)));
    ui.interact(rect, ui.scope_id().with(("fold", rect.min.x as i32, rect.min.y as i32)), Sense::click()).clicked()
}

/// The rect a panel floating in `window` is laid out in as a panel hanging from a button is, its
/// arrow's room above it: its body under the window's top strip, down to its bottom strip.
pub fn floating_rect(window: Rect) -> Rect {
    Rect::from_min_max(
        pos2(window.left(), window.top() + FLOATING_TOP - ARROW_HEIGHT),
        pos2(window.right(), window.bottom() - FLOATING_BOTTOM),
    )
}

/// The area a panel floating in `window` is drawn in: over the canvas, under the panels hanging
/// from the bar, kept to the window and clipped to it as the panel measures it.
pub fn floating_area(ctx: &egui::Context, id: egui::Id, window: Rect) -> egui::Area {
    egui::Area::new(id).fixed_pos(window.min).order(egui::Order::Middle).constrain_to(screen(ctx))
}

/// Draws a floating panel's window at `window` (no arrow): the shadow round it, then its frosted
/// body, whose blur goes into `backdrops`; its layer shown as its space does.
pub fn floating_panel_frame(ui: &Ui, look: &mut Look, slot: &'static str, window: Rect, backdrops: &mut Vec<f32>) {
    let to_window = to_window(ui.ctx());
    ui.ctx().set_transform_layer(ui.layer_id(), to_window);
    floating_frame(ui, look, slot, window, to_window.scaling, backdrops);
}

/// A floating panel's strips at `window`: the grabber in the middle of the top one (the whole strip
/// moves the window) and Close at its right, and two short lines across the lower right corner,
/// which sizes it. What was done to them.
pub fn floating_chrome(ui: &Ui, window: Rect) -> FloatingChrome {
    floating_chrome_of(ui, window, true)
}

/// As [`floating_chrome`], its corner only if it is `sizable`, and its menu at its strip's left.
fn floating_chrome_of(ui: &Ui, window: Rect, sizable: bool) -> FloatingChrome {
    let strip = Rect::from_min_size(window.min, egui::vec2(window.width(), FLOATING_TOP));
    // the whole window, strips and all, the interface's (what it lays out may not reach them)
    crate::screen::claim(ui, window);
    let moving = ui.interact(strip, ui.scope_id().with("floating strip"), Sense::drag()).drag_started();
    // holding several panels: their tabs in place of the grabber
    let several = ui
        .ctx()
        .data(|d| d.get_temp::<Vec<(procraft_core::LayoutPiece, &'static str, bool)>>(egui::Id::unique("window tabs")))
        .is_some_and(|t| t.len() > 1);
    let (pick, tear) = if several {
        strip_tabs(ui, window, right_buttons(ui.ctx()))
    } else {
        let grabber = Rect::from_center_size(pos2(window.center().x, window.top() + 12.0), egui::vec2(40.0, 4.0));
        ui.painter().rect_filled(grabber, 2, argb(GRABBER));
        (None, None)
    };
    let close = Rect::from_center_size(pos2(window.right() - 14.0 - 20.0, window.top() + 22.0), egui::vec2(40.0, 40.0));
    let c = close.center();
    ui.painter().circle_filled(c, 8.5, argb(CLOSE_DISC));
    let stroke = egui::Stroke::new(1.3, argb(CLOSE_CROSS));
    for (a, b) in [(egui::vec2(-3.2, -3.2), egui::vec2(3.2, 3.2)), (egui::vec2(3.2, -3.2), egui::vec2(-3.2, 3.2))] {
        ui.painter().line_segment([c + a, c + b], stroke);
    }
    let closed = ui.interact(close, ui.scope_id().with("floating close"), Sense::click()).clicked();
    // beside Close, while the panels fold
    let fold = panels_fold(ui.ctx());
    let folded = fold && fold_button(ui, close.translate(egui::vec2(-40.0, 0.0)));
    // and beside that, Collapse to edge's
    let collapse = collapse_edge(ui.ctx()).is_some_and(|edge| {
        let at = close.translate(egui::vec2(if fold { -80.0 } else { -40.0 }, 0.0));
        crate::collapse::strip_button(ui, at, edge)
    });
    let act = window_menu(
        ui,
        Rect::from_center_size(pos2(window.left() + 14.0 + 20.0, window.top() + 22.0), egui::vec2(40.0, 40.0)),
    );
    if !sizable {
        return FloatingChrome { moving, sizing: false, closed, folded, act, pick, tear, collapse };
    }
    let corner = Rect::from_min_size(
        pos2(window.right() - 6.0 - 36.0, window.bottom() - FLOATING_BOTTOM),
        egui::vec2(36.0, FLOATING_BOTTOM),
    );
    let end = pos2(corner.right() - 12.0, corner.bottom() - 6.0);
    let line = egui::Stroke::new(2.0, argb(GRABBER));
    for reach in [10.0, 5.0] {
        ui.painter().line_segment([end - egui::vec2(reach, 0.0), end - egui::vec2(0.0, reach)], line);
    }
    let sizing = ui.interact(corner, ui.scope_id().with("floating corner"), Sense::drag()).drag_started();
    FloatingChrome { moving, sizing, closed, folded, act, pick, tear, collapse }
}

/// How many buttons the strip of the window drawn now has left of Close: Fold, and Collapse to
/// edge's.
fn right_buttons(ctx: &egui::Context) -> usize {
    usize::from(panels_fold(ctx)) + usize::from(collapse_edge(ctx).is_some())
}

/// The strips of a toolbar or an adjustment's controls floating (`strip` over it, in the window's
/// points): its grabber (the whole strip moves it), Close, its fold while the panels fold, and its
/// menu. What was done to them.
pub fn strip_chrome(ui: &Ui, strip: Rect) -> FloatingChrome {
    let window = Rect::from_min_size(strip.min, egui::vec2(strip.width(), FLOATING_TOP + FLOATING_BOTTOM));
    floating_chrome_of(ui, window, false)
}

/// The grabbers' ink.
pub fn grabber_ink() -> Themed {
    GRABBER
}

/// A panel hanging from its button, while a plugin lets it float: the grabber at the top of its
/// `body`, and while it is dragged, where the press began and where the pointer is (the window's
/// points).
pub fn undock_grabber(ui: &Ui, body: Rect) -> Option<(Pos2, Pos2)> {
    let grabber = Rect::from_center_size(pos2(body.center().x, body.top() + 6.0), egui::vec2(40.0, 4.0));
    ui.painter().rect_filled(grabber, 2, argb(GRABBER));
    // (narrower than the colour panel's: clear of a title beside it)
    let handle = Rect::from_center_size(pos2(body.center().x, body.top() + 15.0), egui::vec2(100.0, 30.0));
    crate::screen::claim(ui, handle);
    let held = ui.interact(handle, ui.scope_id().with("undock"), Sense::drag());
    let (pointer, from) = (held.interact_pointer_pos()?, ui.input(|i| i.pointer.press_origin())?);
    held.dragged().then(|| (from, to_window(ui.ctx()) * pointer))
}

/// Where the side bar is, for the panels to keep clear of: whether it shows, and on the right.
#[derive(Clone, Copy, Default)]
struct SideBarPlace {
    shown: bool,
    right: bool,
    centre: f32,
}

/// How big each part of the interface shows next to the rest of it this frame, as the plugins say
/// (in [`procraft_core::InterfacePart`]'s order), for what draws a part to read.
pub fn set_part_scales(ctx: &egui::Context, plugins: &procraft_core::Plugins) {
    use procraft_core::InterfacePart as P;
    put_part_scales(
        ctx,
        [P::Floating, P::Menus, P::SideBar, P::TopBar, P::Panels].map(|p| plugins.interface_part_scale(p)),
    );
}

/// How big each part of the interface shows next to the rest of it this frame, in
/// [`InterfacePart`]'s order.
pub fn put_part_scales(ctx: &egui::Context, scales: [f32; 5]) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("interface part scales"), scales));
}

/// How big `part` of the interface shows next to the rest of it this frame (1: as the rest).
pub fn part_scale(ctx: &egui::Context, part: InterfacePart) -> f32 {
    let scales = ctx.data(|d| d.get_temp::<[f32; 5]>(egui::Id::unique("interface part scales")));
    scales.and_then(|s| s.get(part as usize).copied()).unwrap_or(1.0)
}

/// How far down from the window's top the top bar reaches as it shows: what hangs from it hangs
/// from there.
pub fn bar_shown(ctx: &egui::Context) -> f32 {
    pc::BAR_HEIGHT * part_scale(ctx, InterfacePart::TopBar)
}

/// The panels' own space: each is laid out at its own size, under a top bar of its own height, in
/// the window as it measures it at that size, and shows at the panels' size from where the top
/// bar, as it shows, ends at the window's left edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelSpace {
    /// From a panel's points to the window's.
    pub to_window: TSTransform,
    /// The window as a panel measures it: a top bar's height above where the panels hang, the
    /// room under it, and as wide as the window.
    pub screen: Rect,
}

impl PanelSpace {
    /// In `window`, under a top bar reaching `bar_bottom` down, shown `scale` times as big.
    pub fn new(window: Rect, bar_bottom: f32, scale: f32) -> PanelSpace {
        let hang = pos2(window.left(), window.top() + bar_bottom);
        let to_window = crate::chrome::scaled_from(hang, scale);
        let measured = Rect::from_min_max(pos2(window.left(), hang.y - pc::BAR_HEIGHT * scale), window.max);
        PanelSpace { to_window, screen: to_window.inverse().mul_rect(measured) }
    }
}

/// A panel is drawn from now on, in `space`; or, with none, no panel is.
pub fn drawing_panel(ctx: &egui::Context, space: Option<PanelSpace>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("panel space"), space));
}

/// The panels' space while one is drawn.
fn panel_space(ctx: &egui::Context) -> Option<PanelSpace> {
    ctx.data(|d| d.get_temp::<Option<PanelSpace>>(egui::Id::unique("panel space"))).flatten()
}

/// From the points of what is drawn now to the window's: a panel's while one is drawn.
pub fn to_window(ctx: &egui::Context) -> TSTransform {
    panel_space(ctx).map_or(TSTransform::IDENTITY, |s| s.to_window)
}

/// The window as what is drawn now measures it: as a panel does while one is drawn.
pub fn screen(ctx: &egui::Context) -> Rect {
    panel_space(ctx).map_or_else(|| ctx.content_rect(), |s| s.screen)
}

/// Window point `p` (the pointer's, say) where what is drawn now has it.
pub fn in_panel(ctx: &egui::Context, p: Pos2) -> Pos2 {
    to_window(ctx).inverse() * p
}

/// A move of `by` across the window (the pointer's) as what is drawn now has it.
pub fn in_panel_by(ctx: &egui::Context, by: Vec2) -> Vec2 {
    by / to_window(ctx).scaling
}

/// The area a panel at `rect` (arrow included) is drawn in, over the canvas: kept to the window
/// and clipped to it as the panel measures it.
pub fn area(ctx: &egui::Context, id: egui::Id, rect: Rect) -> egui::Area {
    // floating as a window: with the other windows, under the panels hanging from the bar
    let order = if float_window(ctx).is_some() { egui::Order::Middle } else { egui::Order::Foreground };
    egui::Area::new(id).fixed_pos(rect.min).order(order).constrain_to(screen(ctx))
}

/// The side bar shows (or not) on the right (or the left), its centre kept at `centre` (a share of
/// the window's height), for the panels placed this frame.
pub fn side_bar_at(ctx: &egui::Context, shown: bool, right: bool, centre: f32) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("side bar place"), SideBarPlace { shown, right, centre }));
}

/// Where the side bar shows this frame, if it shows.
pub fn side_bar_shown_at(ctx: &egui::Context) -> Option<Rect> {
    let side = ctx.data(|d| d.get_temp::<SideBarPlace>(egui::Id::unique("side bar place")))?;
    let scale = part_scale(ctx, InterfacePart::SideBar);
    side.shown.then(|| {
        crate::chrome::fit_side_bar(ctx.content_rect(), side.right, scale, bar_shown(ctx), side.centre).shown()
    })
}

/// What a panel opened from a button of a bar Edit layout moved hangs from: the bars (which work out
/// where), the bar, and the button's middle in the window.
type HangFrom = Option<(std::sync::Arc<procraft_core::BarLayout>, procraft_core::LayoutBar, Pos2)>;

/// Where the panel open now hangs from a button Edit layout moved, in the panels' space: the side
/// of the bar it is on, its left edge, its top (its bottom, over the bar), its arrow's tip along its
/// edge (from its left, or its top beside the bar) and the room it has. See
/// [`procraft_core::BarLayout::hang`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hung {
    side: HangSide,
    x: f32,
    y: f32,
    arrow: f32,
    room: f32,
}

/// The panel opened from now on hangs from the button whose middle is at `from`'s point on its bar,
/// which Edit layout moved; with none, from where its button always is.
pub fn hang_from(ctx: &egui::Context, from: HangFrom) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::unique("panel hang from"), from);
        d.insert_temp(egui::Id::unique("panel hung"), None::<Hung>);
    });
}

/// Where a panel `width` wide hangs from a button Edit layout moved, while one does (see
/// [`hang_from`]): kept for its frame and body.
fn hang(ctx: &egui::Context, width: f32) -> Option<Hung> {
    let (bars, bar, middle) = ctx.data(|d| d.get_temp::<HangFrom>(egui::Id::unique("panel hang from"))).flatten()?;
    let (place, r) = (bars.shown(bar)?, bars.rect(bar)?);
    let along = if place.vertical { middle.y - r.y } else { middle.x - r.x };
    let to_window = to_window(ctx);
    let k = to_window.scaling;
    let hang = bars.hang(bar, along, width * k, ARROW_SHIFT, POPOVER_GAP, ARROW_HEIGHT * k)?;
    let at = to_window.inverse() * pos2(hang.x, hang.y);
    let hung = Hung { side: hang.side, x: at.x, y: at.y, arrow: hang.arrow / k, room: hang.room / k };
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("panel hung"), Some(hung)));
    Some(hung)
}

fn hung(ctx: &egui::Context) -> Option<Hung> {
    ctx.data(|d| d.get_temp::<Option<Hung>>(egui::Id::unique("panel hung"))).flatten()
}

/// A panel at `rect` (its arrow on top included) under its button, wanting `content` tall (arrow
/// included) and ending `margin` short of the window's edge: as it is; or, hanging from a button
/// Edit layout moved, under, over or beside it, as tall as fits.
pub fn hung_rect(ctx: &egui::Context, rect: Rect, content: f32, margin: f32) -> Rect {
    // floating as a window: under its strip, down to its corner's room
    if let Some((window, sizable)) = float_window(ctx) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("panel float wants"), Some(content)));
        let bottom = if sizable { window.bottom() - FLOATING_BOTTOM } else { window.bottom() };
        return Rect::from_min_max(
            pos2(window.left(), window.top() + FLOATING_TOP - ARROW_HEIGHT),
            pos2(window.right(), bottom),
        );
    }
    let Some(h) = hung(ctx) else { return rect };
    let width = rect.width();
    match h.side {
        HangSide::Below => Rect::from_min_size(pos2(h.x, h.y), Vec2::new(width, content.min(h.room - margin).max(0.0))),
        HangSide::Above => {
            let height = content.min(h.room - margin).max(0.0);
            Rect::from_min_size(pos2(h.x, h.y - height), Vec2::new(width, height))
        }
        HangSide::Left | HangSide::Right => {
            let height = (content - ARROW_HEIGHT).min(h.room).max(0.0);
            Rect::from_min_size(pos2(h.x, h.y), Vec2::new(width + ARROW_HEIGHT, height))
        }
    }
}

/// A panel's body for a panel at `rect` (arrow included), its arrow on the side it hangs from now.
pub fn body_in(ctx: &egui::Context, rect: Rect) -> Rect {
    if float_window(ctx).is_some() {
        return body(rect);
    }
    match hung(ctx).map(|h| h.side) {
        Some(HangSide::Above) => Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - ARROW_HEIGHT)),
        Some(HangSide::Right) => side_body(rect, true),
        Some(HangSide::Left) => side_body(rect, false),
        _ => body(rect),
    }
}

/// Where a panel `width` wide goes, hanging from a button centred at `button_x`: its left edge and
/// its arrow's tip from that edge, in the panels' space while one is drawn. It shows centred on
/// the button (the tip `ARROW_SHIFT` right of the button's centre) and inside the window, clear of
/// the side bar as it shows, on whichever side it is. From a button Edit layout moved, it goes
/// where [`hang_from`] says.
pub fn place(ctx: &egui::Context, width: f32, button_x: f32) -> (f32, f32) {
    if let Some((window, _)) = float_window(ctx) {
        return (window.left(), 0.0);
    }
    if let Some(h) = hang(ctx, width) {
        return (h.x, h.arrow);
    }
    let to_window = to_window(ctx);
    let (k, x) = (to_window.scaling, to_window.translation.x);
    let screen = ctx.content_rect();
    let side = ctx.data(|d| d.get_temp::<SideBarPlace>(egui::Id::unique("side bar place"))).unwrap_or(SideBarPlace {
        shown: true,
        right: false,
        centre: procraft_core::side_bar_first_centre(),
    });
    let side_bar = pc::SIDE_BAR_WIDTH * part_scale(ctx, InterfacePart::SideBar);
    let clear = if side.shown { side_bar + POPOVER_GAP } else { POPOVER_GAP };
    let (min_left, min_right) = if side.right { (POPOVER_GAP, clear) } else { (clear, POPOVER_GAP) };
    // where it shows
    let (tip, shown) = (button_x * k + x + ARROW_SHIFT, width * k);
    let lo = screen.left() + min_left;
    let hi = (screen.right() - min_right - shown).max(lo);
    let left = (tip - shown / 2.0).clamp(lo, hi);
    ((left - x) / k, (tip - left) / k)
}

/// A panel's body (below its arrow) for a panel at `rect` (arrow included).
pub fn body(rect: Rect) -> Rect {
    Rect::from_min_max(pos2(rect.left(), rect.top() + ARROW_HEIGHT), rect.max)
}

/// Draws a panel at `rect` (its arrow on top, the tip `arrow_x` from its left edge; or on the side
/// it hangs from a moved button by, see [`hang_from`]): the shadow round it, then its frosted body,
/// whose blur goes into `backdrops` (window pixels). Its layer shows it as the panels' space does,
/// its icons and shapes drawn for that.
pub fn frame(ui: &Ui, look: &mut Look, slot: &'static str, rect: Rect, arrow_x: f32, backdrops: &mut Vec<f32>) {
    // floating as a window: its frame and strips, what was done to them kept for the interface
    if let Some((window, sizable)) = float_window(ui.ctx()) {
        floating_panel_frame(ui, look, slot, window, backdrops);
        let chrome = floating_chrome_of(ui, window, sizable);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("panel float chrome"), chrome));
        return;
    }
    let to_window = to_window(ui.ctx());
    ui.ctx().set_transform_layer(ui.layer_id(), to_window);
    look.set_zoom(ui.layer_id(), to_window.scaling);
    let side = hung(ui.ctx()).map(|h| h.side);
    let body = body_in(ui.ctx(), rect);
    // hanging under its button, while a plugin lets it float: its grabber tears it off
    let tears = ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::unique("panel tears"))).unwrap_or(false);
    if tears && matches!(side, None | Some(HangSide::Below)) {
        let torn = undock_grabber(ui, body);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("panel torn"), torn));
    }
    let ppp = ui.ctx().pixels_per_point();
    let shown = to_window.mul_rect(body);
    let edges = [shown.left(), shown.top(), shown.right(), shown.bottom(), POPOVER_CORNER * to_window.scaling];
    backdrops.extend(edges.map(|v| v * ppp));
    backdrops.push(1.0);
    // only round the panel: none of it shows through
    let shadow = egui::epaint::Shadow {
        offset: [0, 0],
        blur: POPOVER_SHADOW_RADIUS as u8,
        spread: 0,
        color: argb(POPOVER_SHADOW),
    };
    ui.painter().add(shadow.as_shape(body, POPOVER_CORNER));
    let size = rect.size();
    let solid = panels_solid(ui.ctx());
    let key = ((arrow_x * 100.0).round() as i32, side.map(|s| s as i32), solid);
    look.shape(ui, slot, key, rect, |pixmap, t| {
        let (half, tip) = (ARROW_HEIGHT, 2.0);
        let near = |from: f32| (from - half - tip).clamp(0.0, POPOVER_CORNER);
        match side {
            Some(HangSide::Left | HangSide::Right) => {
                let left = side == Some(HangSide::Right);
                frost(pixmap, side_outline(size, arrow_x, left, POPOVER_CORNER), t, solid);
                return;
            }
            // its arrow down at the button, under it
            Some(HangSide::Above) => {
                let mut pb = tiny_skia::PathBuilder::new();
                let bottom = size.y - ARROW_HEIGHT;
                let corners = [POPOVER_CORNER, POPOVER_CORNER, near(size.x - arrow_x), near(arrow_x)];
                round_rect(&mut pb, Rect::from_min_max(Pos2::ZERO, pos2(size.x, bottom)), corners);
                pb.move_to(arrow_x - half - tip, bottom - 0.5);
                pb.line_to(arrow_x - tip, size.y - tip);
                pb.quad_to(arrow_x, size.y, arrow_x + tip, size.y - tip);
                pb.line_to(arrow_x + half + tip, bottom - 0.5);
                pb.close();
                frost(pixmap, pb, t, solid);
                return;
            }
            Some(HangSide::Below) | None => {}
        }
        let mut pb = tiny_skia::PathBuilder::new();
        // a corner the arrow comes near is rounded less, so that the arrow rises from a straight
        // edge rather than hanging over the curve (at a small interface size, or a button near
        // the window's edge)
        let corners = [near(arrow_x), near(size.x - arrow_x), POPOVER_CORNER, POPOVER_CORNER];
        round_rect(&mut pb, Rect::from_min_max(pos2(0.0, ARROW_HEIGHT), size.to_pos2()), corners);
        pb.move_to(arrow_x - half - tip, ARROW_HEIGHT + 0.5);
        pb.line_to(arrow_x - tip, tip);
        pb.quad_to(arrow_x, 0.0, arrow_x + tip, tip);
        pb.line_to(arrow_x + half + tip, ARROW_HEIGHT + 0.5);
        pb.close();
        frost(pixmap, pb, t, solid);
    });
}

/// Draws a floating panel at `rect` (no arrow): the shadow round it, then its frosted body, whose
/// blur goes into `backdrops` (window pixels). Its layer shows it `scale` times as big, from its
/// top-left, its icons and shapes drawn for that.
pub fn floating_frame(ui: &Ui, look: &mut Look, slot: &'static str, rect: Rect, scale: f32, backdrops: &mut Vec<f32>) {
    look.set_zoom(ui.layer_id(), scale);
    let ppp = ui.ctx().pixels_per_point();
    let shown = Rect::from_min_size(rect.min, rect.size() * scale);
    // (no more than half its height: a folded window's tab, 36 tall, round at its ends, not pinched
    // to points where corners of 35 would cross)
    let corner = frame_corner(rect.size());
    let edges = [shown.left(), shown.top(), shown.right(), shown.bottom(), corner * scale];
    backdrops.extend(edges.map(|v| v * ppp));
    backdrops.push(1.0);
    let shadow = egui::epaint::Shadow {
        offset: [0, 0],
        blur: POPOVER_SHADOW_RADIUS as u8,
        spread: 0,
        color: argb(POPOVER_SHADOW),
    };
    ui.painter().add(shadow.as_shape(rect, corner));
    let size = rect.size();
    let solid = panels_solid(ui.ctx());
    look.shape(ui, slot, ((size.y * 10.0).round() as i32, solid), rect, |pixmap, t| {
        let mut pb = tiny_skia::PathBuilder::new();
        round_rect(&mut pb, Rect::from_min_size(pos2(0.0, 0.0), size), [corner; 4]);
        frost(pixmap, pb, t, solid);
    });
}

/// The corners of a floating window `size` big: the panels' own, no more than half its narrower side.
fn frame_corner(size: egui::Vec2) -> f32 {
    POPOVER_CORNER.min(size.x.min(size.y) / 2.0).max(0.0)
}

/// Draws a panel at `rect` (arrow included) with its arrow on its `left` side, else its right, the
/// tip `arrow_y` from its top, and `corner` rounding: the shadow round it, then its frosted body,
/// whose blur goes into `backdrops` (window pixels). Returns the body.
#[allow(clippy::too_many_arguments)]
pub fn side_frame(
    ui: &Ui,
    look: &mut Look,
    slot: &'static str,
    rect: Rect,
    arrow_y: f32,
    left: bool,
    corner: f32,
    backdrops: &mut Vec<f32>,
) -> Rect {
    let body = side_body(rect, left);
    let ppp = ui.ctx().pixels_per_point();
    backdrops.extend([body.left(), body.top(), body.right(), body.bottom(), corner].map(|v| v * ppp));
    backdrops.push(1.0);
    let shadow = egui::epaint::Shadow {
        offset: [0, 0],
        blur: POPOVER_SHADOW_RADIUS as u8,
        spread: 0,
        color: argb(POPOVER_SHADOW),
    };
    ui.painter().add(shadow.as_shape(body, corner));
    let size = rect.size();
    let solid = panels_solid(ui.ctx());
    let key = ((arrow_y * 100.0).round() as i32, left, (corner * 10.0) as i32, solid);
    look.shape(ui, slot, key, rect, |pixmap, t| frost(pixmap, side_outline(size, arrow_y, left, corner), t, solid));
    body
}

/// As [`side_frame`], filled `colour` (nothing shows through). Returns the body.
#[allow(clippy::too_many_arguments)]
pub fn solid_side_frame(
    ui: &Ui,
    look: &mut Look,
    slot: &'static str,
    rect: Rect,
    arrow_y: f32,
    left: bool,
    corner: f32,
    colour: impl Shade,
) -> Rect {
    let colour = colour.value();
    let body = side_body(rect, left);
    let shadow = egui::epaint::Shadow {
        offset: [0, 0],
        blur: POPOVER_SHADOW_RADIUS as u8,
        spread: 0,
        color: argb(POPOVER_SHADOW),
    };
    ui.painter().add(shadow.as_shape(body, corner));
    let size = rect.size();
    let key = ((arrow_y * 100.0).round() as i32, left, (corner * 10.0) as i32, colour);
    look.shape(ui, slot, key, rect, |pixmap, t| {
        fill(pixmap, side_outline(size, arrow_y, left, corner).finish(), argb(colour), t);
    });
    body
}

/// A side-arrowed panel's body: `rect` less its arrow, on its `left` side or its right.
fn side_body(rect: Rect, left: bool) -> Rect {
    if left {
        Rect::from_min_max(pos2(rect.left() + ARROW_HEIGHT, rect.top()), rect.max)
    } else {
        Rect::from_min_max(rect.min, pos2(rect.right() - ARROW_HEIGHT, rect.bottom()))
    }
}

/// The outline of a panel `size` big (arrow included) with its arrow on its `left` side, else its
/// right, the tip `arrow_y` from its top, rounded `corner`: one shape, filled as one.
fn side_outline(size: egui::Vec2, arrow_y: f32, left: bool, corner: f32) -> tiny_skia::PathBuilder {
    let mut pb = tiny_skia::PathBuilder::new();
    let (half, tip, w) = (ARROW_HEIGHT, 2.0, size.x);
    if left {
        round_rect(&mut pb, Rect::from_min_max(pos2(ARROW_HEIGHT, 0.0), size.to_pos2()), [corner; 4]);
        pb.move_to(ARROW_HEIGHT + 0.5, arrow_y - half - tip);
        pb.line_to(tip, arrow_y - tip);
        pb.quad_to(0.0, arrow_y, tip, arrow_y + tip);
        pb.line_to(ARROW_HEIGHT + 0.5, arrow_y + half + tip);
    } else {
        round_rect(&mut pb, Rect::from_min_max(Pos2::ZERO, pos2(w - ARROW_HEIGHT, size.y)), [corner; 4]);
        pb.move_to(w - ARROW_HEIGHT - 0.5, arrow_y - half - tip);
        pb.line_to(w - tip, arrow_y - tip);
        pb.quad_to(w, arrow_y, w - tip, arrow_y + tip);
        pb.line_to(w - ARROW_HEIGHT - 0.5, arrow_y + half + tip);
    }
    pb.close();
    pb
}

/// Draws a panel at `rect` (arrow included) with its arrow `below` it (else above), the tip
/// `arrow_x` from its left edge, rounded `corner` and filled `colour` (nothing shows through): the
/// shadow round it, then the panel. Returns its body.
#[allow(clippy::too_many_arguments)]
pub fn solid_frame(
    ui: &Ui,
    look: &mut Look,
    slot: &'static str,
    rect: Rect,
    arrow_x: f32,
    below: bool,
    corner: f32,
    colour: impl Shade,
) -> Rect {
    let colour = colour.value();
    let body = if below {
        Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - ARROW_HEIGHT))
    } else {
        Rect::from_min_max(pos2(rect.left(), rect.top() + ARROW_HEIGHT), rect.max)
    };
    let shadow = egui::epaint::Shadow {
        offset: [0, 0],
        blur: POPOVER_SHADOW_RADIUS as u8,
        spread: 0,
        color: argb(POPOVER_SHADOW),
    };
    ui.painter().add(shadow.as_shape(body, corner));
    let size = rect.size();
    let key = ((arrow_x * 100.0).round() as i32, colour, below, (corner * 10.0) as i32);
    look.shape(ui, slot, key, rect, |pixmap, t| {
        let mut pb = tiny_skia::PathBuilder::new();
        let (half, tip, h) = (ARROW_HEIGHT, 2.0, size.y);
        if below {
            round_rect(&mut pb, Rect::from_min_max(Pos2::ZERO, pos2(size.x, h - ARROW_HEIGHT)), [corner; 4]);
            pb.move_to(arrow_x - half - tip, h - ARROW_HEIGHT - 0.5);
            pb.line_to(arrow_x - tip, h - tip);
            pb.quad_to(arrow_x, h, arrow_x + tip, h - tip);
            pb.line_to(arrow_x + half + tip, h - ARROW_HEIGHT - 0.5);
        } else {
            round_rect(&mut pb, Rect::from_min_max(pos2(0.0, ARROW_HEIGHT), size.to_pos2()), [corner; 4]);
            pb.move_to(arrow_x - half - tip, ARROW_HEIGHT + 0.5);
            pb.line_to(arrow_x - tip, tip);
            pb.quad_to(arrow_x, 0.0, arrow_x + tip, tip);
            pb.line_to(arrow_x + half + tip, ARROW_HEIGHT + 0.5);
        }
        pb.close();
        let mut paint = tiny_skia::Paint::default();
        let [r, g, b, a] = argb(colour).to_srgba_unmultiplied();
        paint.set_color_rgba8(r, g, b, a);
        paint.anti_alias = true;
        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, t, None);
        }
    });
    body
}

/// A panel's outline filled with its frosted material (wholly `solid`, see [`frost_colour`]): the
/// arrow and the body are one shape, filled as one, without an even-odd seam.
fn frost(pixmap: &mut tiny_skia::Pixmap, pb: tiny_skia::PathBuilder, t: tiny_skia::Transform, solid: bool) {
    let mut paint = tiny_skia::Paint::default();
    let [r, g, b, a] = frost_colour(solid).to_srgba_unmultiplied();
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, t, None);
    }
}

/// Fills a rounded rectangle (corners `[top left, top right, bottom right, bottom left]`) kept as
/// `slot`: for the parts of a panel that follow its rounded corners.
pub fn rounded(ui: &Ui, look: &mut Look, slot: &'static str, rect: Rect, corners: [f32; 4], colour: Color32) {
    let size = rect.size();
    let key = (corners.map(|c| (c * 10.0) as i32), colour.to_array());
    look.shape(ui, slot, key, rect, |pixmap, t| {
        let mut pb = tiny_skia::PathBuilder::new();
        round_rect(&mut pb, Rect::from_min_size(Pos2::ZERO, size), corners);
        fill(pixmap, pb.finish(), colour, t);
    });
}

/// A panel's header: its title (24 bold) `start` in, with the chevron disc after it. Returns
/// whether the title was tapped.
pub fn header(ui: &Ui, look: &mut Look, top: Rect, title: &str, start: f32, chevron: bool, chevron_on: bool) -> bool {
    header_title(ui, look, top, title, start, chevron, chevron_on).0
}

/// As [`header`]; also where the title and its chevron are, for a menu to hang from.
pub fn header_title(
    ui: &Ui,
    look: &mut Look,
    top: Rect,
    title: &str,
    start: f32,
    chevron: bool,
    chevron_on: bool,
) -> (bool, Rect) {
    let painter = ui.painter();
    let at = pos2(top.left() + start, top.top() + 19.0);
    let words = text(painter, at, Align2::LEFT_TOP, title, 24.0, Weight::Bold, argb(PANEL_TITLE));
    if chevron {
        let centre = pos2(words.right() + 10.0 + 8.5, words.center().y - 1.5);
        painter.circle_filled(centre, 8.5, if chevron_on { argb(pc::ACCENT) } else { argb(CHEVRON_DISC) });
        look.icon(ui, &icons::CHEVRON_DOWN, centre, 13.0, argb(CHEVRON_GLYPH));
    }
    let row = if chevron { Rect::from_min_max(words.min, pos2(words.right() + 27.0, words.bottom())) } else { words };
    (ui.interact(row, ui.scope_id().with(("title", title)), Sense::click()).clicked(), row)
}

/// A light-on-dark tint for a selected row, or none.
pub fn row_fill(selected: bool, idle: impl Shade) -> Color32 {
    if selected { argb(pc::ACCENT) } else { argb(idle) }
}

#[cfg(test)]
mod tests {
    use egui::emath::TSTransform;
    use egui::{Pos2, Rect, pos2, vec2};

    use super::{
        ARROW_SHIFT, POPOVER_GAP, PanelSpace, drawing_panel, place, put_part_scales, side_bar_at, solid_if, tab_cells,
    };
    use crate::look::pc;

    /// While a plugin lays the panels out, a panel over a floating window (or a window over another)
    /// hides it whole: the dark frost (0xDE1E1E1E) let 13% of it through, its text readable, as only
    /// the canvas blurs behind the frost.
    #[test]
    fn a_panel_that_may_lie_over_a_window_hides_it_whole() {
        let frost = egui::Color32::from_rgba_unmultiplied(0x1E, 0x1E, 0x1E, 0xDE);
        assert_eq!(solid_if(frost, false), frost);
        let solid = solid_if(frost, true);
        // its tint as it is, wholly opaque (not the kept, darker, premultiplied one)
        assert_eq!(solid.to_array(), [0x1E, 0x1E, 0x1E, 0xFF]);
        // a window's white text (255) on its ground (30) under it: no level of it shows
        let through = |c: egui::Color32| f32::from(255 - c.a()) / 255.0 * (255.0 - 30.0);
        println!("through the frost: {:.1} levels, solid: {:.1}", through(frost), through(solid));
        assert!(through(frost) > 25.0);
        assert!(through(solid) < 0.5);
    }

    /// A window's tabs across its strip keep clear of its menu button, Close and Fold, however many
    /// it holds: four on the Actions window (389 wide) are 54 wide each, not 86 running under the
    /// buttons at both ends.
    #[test]
    fn a_window_s_tabs_keep_clear_of_its_menu_close_and_fold() {
        let window = Rect::from_min_size(pos2(100.0, 50.0), vec2(389.0, 500.0));
        let button = |x: f32| Rect::from_center_size(pos2(x, window.top() + 22.0), vec2(40.0, 40.0));
        let (menu, close) = (button(window.left() + 34.0), button(window.right() - 34.0));
        let fold = close.translate(vec2(-40.0, 0.0));
        for count in 1..=4 {
            for folds in [false, true] {
                let cells = tab_cells(window, count, usize::from(folds));
                assert_eq!(cells.len(), count);
                for (i, c) in cells.iter().enumerate() {
                    let clear = !c.intersects(menu) && !c.intersects(close) && !(folds && c.intersects(fold));
                    assert!(clear, "{count} tabs, fold {folds}: tab {i} at {c:?}");
                    assert!(c.width() >= 50.0 && c.width() <= 86.0, "{c:?}");
                    if let Some(next) = cells.get(i + 1) {
                        assert!(c.right() < next.left(), "{c:?} {next:?}");
                    }
                }
            }
        }
        // two fit at their full width, in the middle between the buttons as before
        let two = tab_cells(window, 2, 0);
        assert_eq!(two.iter().map(Rect::width).collect::<Vec<_>>(), [86.0, 86.0]);
        assert!((two[0].left() + two[1].right() - window.left() - window.right()).abs() < 1e-3);
        let four = tab_cells(window, 4, 1);
        assert!((four[0].width() - (389.0 - 58.0 - 98.0) / 4.0 + 4.0).abs() < 1e-3, "{four:?}");
    }

    fn near(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn panels_as_big_as_the_rest_are_laid_out_in_the_window_itself() {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
        let space = PanelSpace::new(window, pc::BAR_HEIGHT, 1.0);
        assert_eq!((space.to_window, space.screen), (TSTransform::IDENTITY, window));
        // under a bigger top bar they only hang lower
        let space = PanelSpace::new(window, pc::BAR_HEIGHT * 1.5, 1.0);
        assert_eq!(space.to_window, TSTransform::IDENTITY);
        assert_eq!(space.screen.top() + pc::BAR_HEIGHT, pc::BAR_HEIGHT * 1.5);
        assert_eq!((space.screen.left(), space.screen.right(), space.screen.bottom()), (0.0, 1200.0, 800.0));
    }

    #[test]
    fn panels_bigger_or_smaller_hang_from_the_top_bar_as_it_shows_and_measure_the_window_to_its_edges() {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
        for (bar, scale) in [(1.0, 1.5), (1.25, 0.5), (0.5, 1.5), (1.5, 0.75)] {
            let PanelSpace { to_window, screen } = PanelSpace::new(window, pc::BAR_HEIGHT * bar, scale);
            assert_eq!(to_window.scaling, scale);
            // a panel's top, under its own top bar, shows where the top bar ends
            let top = to_window * pos2(screen.left(), screen.top() + pc::BAR_HEIGHT);
            assert!(near(top, pos2(0.0, pc::BAR_HEIGHT * bar)), "{bar} {scale}: {top:?}");
            assert!(near(to_window * screen.max, window.max), "{bar} {scale}");
        }
    }

    #[test]
    fn a_panel_bigger_or_smaller_points_at_its_button_inside_the_window_clear_of_the_side_bar_as_it_shows() {
        let ctx = egui::Context::default();
        let window = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
        let raw = egui::RawInput { screen_rect: Some(window), ..Default::default() };
        let mut output = ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx();
            let width = 384.0;
            for side in [1.0, 1.5] {
                put_part_scales(ctx, [1.0, 1.0, side, 1.0, 1.0]);
                for right in [false, true] {
                    side_bar_at(ctx, true, right, procraft_core::side_bar_first_centre());
                    for scale in [0.5, 1.0, 1.5] {
                        let space = PanelSpace::new(window, pc::BAR_HEIGHT, scale);
                        drawing_panel(ctx, Some(space));
                        for button in [70.0, 600.0, 1150.0] {
                            let local = (space.to_window.inverse() * pos2(button, 0.0)).x;
                            let (left, tip) = place(ctx, width, local);
                            let shown = |x: f32| (space.to_window * pos2(x, 0.0)).x;
                            let case = format!("{side} {right} {scale} {button}");
                            let clear = pc::SIDE_BAR_WIDTH * side + POPOVER_GAP;
                            let (lo, hi) =
                                if right { (POPOVER_GAP, 1200.0 - clear) } else { (clear, 1200.0 - POPOVER_GAP) };
                            assert!(shown(left) >= lo - 1e-3 && shown(left + width) <= hi + 1e-3, "{case}");
                            if button == 600.0 {
                                assert!((shown(left + tip) - (button + ARROW_SHIFT)).abs() < 1e-3, "{case}");
                            }
                        }
                    }
                }
            }
            drawing_panel(ctx, None);
        });
        output.textures_delta.clear();
    }
}
