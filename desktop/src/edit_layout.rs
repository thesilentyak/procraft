//! Edit layout on the desktop, as the Android app has it (ui/FloatingPanels.kt's EditLayoutHandles):
//! while a plugin lets the interface's bars move, Edit Layout in the Canvas tab outlines the top bar's
//! two halves and the side bar, each dragged anywhere by its outline; Reset puts them back, Done ends
//! it. Where each goes is the core's to work out ([`procraft_core::BarLayout`]); here the app tells
//! it where each bar always is, and draws the handles.

use std::sync::Arc;

use egui::{Align2, Pos2, Rect, Sense, pos2, vec2};
use procraft_core::{BarLayout, BarPlace, LayoutBar, PanelLayoutLook};

use crate::look::{Weight, argb, pc, text};

/// How near (points) a bar moved by Edit layout snaps at least: to an edge, to turn against it.
const EDIT_SNAP: f32 = 16.0;

/// The bars as kept (none: each where it always is). The desktop's side bar doesn't lie down.
pub fn kept_bars(kept: Option<&str>) -> Arc<BarLayout> {
    let bars = BarLayout::new();
    if let Some(kept) = kept {
        bars.read_kept(kept.to_owned());
    }
    bars.set_fixed(LayoutBar::SideBar, true);
    bars
}

/// Where the bars moved by Edit layout show this frame: the top bar's halves (both, if either was
/// moved), and the side bar's top-left; none where each always is.
pub struct Moved {
    pub halves: Option<[BarPlace; 2]>,
    pub side_bar: Option<Pos2>,
}

/// The bars where they always are in this window, told to `bars`; and where those moved show, while
/// the plugins (`look`) let the bars move.
pub fn place_bars(
    ctx: &egui::Context,
    bars: &BarLayout,
    look: &PanelLayoutLook,
    right: bool,
    centre: f32,
    sliding: [bool; 2],
) -> Moved {
    let window = ctx.content_rect();
    bars.set_window(window.width(), window.height(), look.snap.max(EDIT_SNAP));
    bars.set_collapsing(look.collapse);
    let k = crate::panel::part_scale(ctx, procraft_core::InterfacePart::TopBar);
    let width = window.width() / k;
    let end = (pc::BAR_MARGIN - crate::chrome::HALF_END) * k;
    let (left, right_length) =
        (crate::chrome::half_length(true, width) * k, crate::chrome::half_length(false, width) * k);
    let thick = pc::BAR_HEIGHT * k;
    bars.set_bar(
        LayoutBar::TopLeft,
        BarPlace { x: window.left() + end, y: window.top(), vertical: false },
        left,
        thick,
    );
    let x = window.right() - end - right_length;
    bars.set_bar(LayoutBar::TopRight, BarPlace { x, y: window.top(), vertical: false }, right_length, thick);
    let scale = crate::panel::part_scale(ctx, procraft_core::InterfacePart::SideBar);
    let side = crate::chrome::fit_side_bar(window, right, scale, crate::panel::bar_shown(ctx), centre).shown();
    let own = BarPlace { x: side.left(), y: side.top(), vertical: true };
    bars.set_bar(LayoutBar::SideBar, own, side.height(), side.width());
    let moved = |bar: LayoutBar| (look.edit && bars.is_moved(bar)).then(|| bars.shown(bar)).flatten();
    // (each on glass of its own while moved, or sliding into its side or out of it)
    let apart = moved(LayoutBar::TopLeft).is_some() || moved(LayoutBar::TopRight).is_some() || sliding.contains(&true);
    let halves = if apart {
        bars.shown(LayoutBar::TopLeft).zip(bars.shown(LayoutBar::TopRight)).map(|(l, r)| [l, r])
    } else {
        None
    };
    Moved { halves, side_bar: moved(LayoutBar::SideBar).map(|p| pos2(p.x, p.y)) }
}

/// What Edit layout's handles did this frame: a bar let go, to keep; or Done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    Kept,
    Ended,
    /// Show Hidden: the windows hidden from their menus shown again.
    ShowHidden,
    /// Collapse All (true) or Open All (false).
    CollapseAll(bool),
    /// A named layout chosen, saved under a name, renamed (from, to) or deleted.
    Choose(String),
    Save(String),
    Rename(String, String),
    Delete(String),
}

/// The named layouts' list, while it is open: each by its name (a tap brings it back, the one last
/// chosen ticked) with Rename and Delete, then a field for a name and Save (Rename while one is
/// renamed). Over the row of buttons at `row`'s top.
fn layouts_list(ctx: &egui::Context, named: &procraft_core::NamedLayouts, row: Rect) -> Option<Done> {
    let id = egui::Id::unique("named layouts");
    let names = named.names();
    let chosen = named.chosen();
    let height = 8.0 + 44.0 * (names.len() as f32 + 1.0);
    let list = Rect::from_min_size(pos2(row.center().x - 180.0, row.top() - 12.0 - height), vec2(360.0, height));
    let mut done = None;
    let mut saved = false;
    let mut entry = ctx.data(|d| d.get_temp::<String>(id.with("text"))).unwrap_or_else(|| named.next_name());
    let mut renaming = ctx.data(|d| d.get_temp::<Option<String>>(id.with("renaming"))).flatten();
    egui::Area::new(id).fixed_pos(list.min).order(egui::Order::Tooltip).show(ctx, |ui| {
        ui.painter().rect_filled(list, 16.0, argb(crate::panel::POPOVER_FROST));
        crate::screen::claim(ui, list);
        let ink = argb(pc::MENU_INK);
        let accent = argb(pc::ACCENT);
        for (i, name) in names.iter().enumerate() {
            let r =
                Rect::from_min_size(pos2(list.left(), list.top() + 4.0 + 44.0 * i as f32), vec2(list.width(), 44.0));
            let label =
                if chosen.as_deref() == Some(name.as_str()) { format!("\u{2713} {name}") } else { name.clone() };
            text(
                ui.painter(),
                pos2(r.left() + 16.0, r.center().y),
                Align2::LEFT_CENTER,
                &label,
                17.0,
                Weight::Regular,
                ink,
            );
            let delete = Rect::from_min_size(pos2(r.right() - 76.0, r.top()), vec2(70.0, 44.0));
            let rename = delete.translate(vec2(-76.0, 0.0));
            text(ui.painter(), rename.center(), Align2::CENTER_CENTER, "Rename", 15.0, Weight::Regular, accent);
            text(ui.painter(), delete.center(), Align2::CENTER_CENTER, "Delete", 15.0, Weight::Regular, accent);
            if ui.interact(delete, id.with(("delete", i)), Sense::click()).clicked() {
                done = Some(Done::Delete(name.clone()));
            } else if ui.interact(rename, id.with(("rename", i)), Sense::click()).clicked() {
                renaming = Some(name.clone());
                entry.clone_from(name);
            } else if ui
                .interact(
                    Rect::from_min_max(r.min, pos2(rename.left(), r.bottom())),
                    id.with(("choose", i)),
                    Sense::click(),
                )
                .clicked()
            {
                done = Some(Done::Choose(name.clone()));
            }
        }
        // the name, and Save (or Rename)
        let last = Rect::from_min_size(pos2(list.left(), list.bottom() - 4.0 - 44.0), vec2(list.width(), 44.0));
        let field = Rect::from_min_max(
            pos2(last.left() + 12.0, last.top() + 7.0),
            pos2(last.right() - 100.0, last.bottom() - 7.0),
        );
        ui.put(field, egui::TextEdit::singleline(&mut entry));
        let save = Rect::from_min_max(pos2(field.right() + 8.0, last.top()), last.max);
        let word = if renaming.is_some() { "Rename" } else { "Save" };
        text_at(ui, save, word, accent);
        if ui.interact(save, id.with("save"), Sense::click()).clicked() {
            done = Some(match renaming.take() {
                Some(from) => Done::Rename(from, entry.clone()),
                None => Done::Save(entry.clone()),
            });
            saved = true;
        }
    });
    ctx.data_mut(|d| {
        // saved: the next frame offers the next free name (as the list has the one saved by then)
        if saved {
            d.remove::<String>(id.with("text"));
        } else {
            d.insert_temp(id.with("text"), entry);
        }
        d.insert_temp(id.with("renaming"), renaming);
    });
    done
}

/// `word` in the middle of `rect`, in `ink`.
fn text_at(ui: &egui::Ui, rect: Rect, word: &str, ink: egui::Color32) {
    text(ui.painter(), rect.center(), Align2::CENTER_CENTER, word, 17.0, Weight::Regular, ink);
}

/// Edit layout's handles: an outline round each bar that drags it (held, it follows the pointer
/// until let go), and Reset and Done at the bottom.
pub fn handles(
    ctx: &egui::Context,
    bars: &BarLayout,
    hidden: bool,
    named: &procraft_core::NamedLayouts,
    belt: bool,
    collapse: Option<bool>,
) -> Option<Done> {
    let mut done = None;
    // held, it follows the pointer
    if bars.holding().is_some() {
        let (pointer, down) = ctx.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
        match pointer {
            Some(p) if down => {
                bars.drag(p.x, p.y);
                ctx.request_repaint();
            }
            _ => {
                if bars.let_go() {
                    done = Some(Done::Kept);
                }
            }
        }
    }
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("edit layout")).fixed_pos(screen.min).order(egui::Order::Foreground).show(
        ctx,
        |ui| {
            let accent = argb(pc::ACCENT);
            // (the Tool Belt's while it shows)
            let belt = belt.then_some(LayoutBar::Belt);
            for bar in [LayoutBar::TopLeft, LayoutBar::TopRight, LayoutBar::SideBar].into_iter().chain(belt) {
                let Some(r) = bars.rect(bar) else { continue };
                let rect = Rect::from_min_size(pos2(r.x, r.y), vec2(r.width, r.height));
                ui.painter().rect_stroke(rect, 12.0, egui::Stroke::new(2.0, accent), egui::StrokeKind::Inside);
                // (its presses, the drag and the let-go, never the canvas's)
                crate::screen::claim(ui, rect);
                let held = ui.interact(rect, ui.scope_id().with(("edit layout", bar as u8)), Sense::click_and_drag());
                if held.drag_started()
                    && let Some(from) = ui.input(|i| i.pointer.press_origin())
                {
                    bars.hold(bar, from.x, from.y);
                }
            }
        },
    );
    // Layouts, (Show Hidden while windows are hidden,) Reset and Done, at the bottom in the middle,
    // over the floating windows
    egui::Area::new(egui::Id::unique("edit layout row")).fixed_pos(screen.min).order(egui::Order::Tooltip).show(
        ctx,
        |ui| {
            // (and Collapse All, or Open All while any is collapsed, with Collapse to edge on)
            let mut labels = vec!["Layouts"];
            labels.extend(hidden.then_some("Show Hidden"));
            labels.extend(collapse.map(|any| if any { "Open All" } else { "Collapse All" }));
            labels.extend(["Reset", "Done"]);
            let wide = |l: &str| matches!(l, "Show Hidden" | "Collapse All" | "Open All");
            let widths: Vec<f32> = labels.iter().map(|l| if wide(l) { 130.0 } else { 90.0 }).collect();
            let whole: f32 = widths.iter().sum();
            let row = Rect::from_center_size(pos2(screen.center().x, screen.bottom() - 28.0 - 22.0), vec2(whole, 44.0));
            ui.painter().rect_filled(row, 22.0, argb(crate::panel::POPOVER_FROST));
            crate::screen::claim(ui, row);
            let ink = argb(pc::ACCENT);
            let mut left = row.left();
            for (label, width) in labels.into_iter().zip(widths) {
                let cell = Rect::from_min_size(pos2(left, row.top()), vec2(width, 44.0));
                left += width;
                text(ui.painter(), cell.center(), Align2::CENTER_CENTER, label, 17.0, Weight::Regular, ink);
                if ui.interact(cell, ui.scope_id().with(("edit layout", label)), Sense::click()).clicked() {
                    let list = egui::Id::unique("named layouts open");
                    if label == "Layouts" {
                        let open = ui.ctx().data(|d| d.get_temp::<bool>(list)).unwrap_or(false);
                        ui.ctx().data_mut(|d| d.insert_temp(list, !open));
                        continue;
                    }
                    done = Some(match label {
                        "Show Hidden" => Done::ShowHidden,
                        "Collapse All" => Done::CollapseAll(true),
                        "Open All" => Done::CollapseAll(false),
                        "Reset" => {
                            bars.reset();
                            Done::Kept
                        }
                        _ => Done::Ended,
                    });
                }
            }
        },
    );
    // the named layouts' list over the row, while it is open
    let open = ctx.data(|d| d.get_temp::<bool>(egui::Id::unique("named layouts open"))).unwrap_or(false);
    if open {
        let row = Rect::from_center_size(pos2(screen.center().x, screen.bottom() - 28.0 - 22.0), vec2(360.0, 44.0));
        if let Some(chosen) = layouts_list(ctx, named, row) {
            done = Some(chosen);
        }
    }
    done
}
