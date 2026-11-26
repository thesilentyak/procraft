//! The list the Layers panel's + opens for a filter layer (interface 17): a sheet over everything,
//! as the Textures sheet is, with the filters the plugins on offer under their headings, in the
//! order they list them. A click picks one; Cancel, or a click off the sheet, closes it.

use egui::{Align2, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use procraft_core::PluginLayerFilter;

use crate::look::{Themed, Weight, argb, text};

const VEIL: u32 = 0x80000000;
const SHEET: Themed = Themed(0xFF1C1C1E, 0xFFF2F2F7);
const TITLE: Themed = Themed(0xFFC1C1C1, 0xFF3A3A3A);
const DIM: u32 = 0xFF868686;
const INK: Themed = Themed(0xFFFFFFFF, 0xFF000000);
const RULE: Themed = Themed(0xFF3A3A3C, 0xFFD1D1D6);
/// A heading's height and a filter's row's.
const HEADING: f32 = 38.0;
const ROW: f32 = 44.0;

/// What a click on the sheet came to.
pub enum Picked {
    /// It is still open.
    Open,
    /// Closed without a filter.
    Cancelled,
    /// Filter `id` picked: a layer of it to add.
    Filter(String),
}

/// The sheet, while open: the filters offered when it opened.
pub struct FilterList {
    filters: Vec<PluginLayerFilter>,
}

/// `filters` under their headings, each heading once, where its first filter comes.
fn grouped(filters: &[PluginLayerFilter]) -> Vec<(&str, Vec<&PluginLayerFilter>)> {
    let mut groups: Vec<(&str, Vec<&PluginLayerFilter>)> = Vec::new();
    for f in filters {
        match groups.iter_mut().find(|(g, _)| *g == f.group) {
            Some((_, list)) => list.push(f),
            None => groups.push((f.group.as_str(), vec![f])),
        }
    }
    groups
}

impl FilterList {
    pub fn new(filters: Vec<PluginLayerFilter>) -> FilterList {
        FilterList { filters }
    }

    /// Draws the sheet over the window.
    pub fn show(&mut self, ctx: &egui::Context) -> Picked {
        let screen = ctx.content_rect();
        let mut picked = Picked::Open;
        egui::Area::new(egui::Id::unique("filter layers")).order(egui::Order::Foreground).fixed_pos(screen.min).show(
            ctx,
            |ui| {
                ui.set_min_size(screen.size());
                picked = self.sheet(ui, screen);
            },
        );
        picked
    }

    fn sheet(&self, ui: &mut Ui, screen: Rect) -> Picked {
        ui.painter().rect_filled(screen, 0, argb(VEIL));
        let veil = ui.interact(screen, ui.scope_id().with("filter layers veil"), Sense::click_and_drag());
        let height = (screen.height() - 148.0).clamp(240.0, 640.0);
        let sheet = Rect::from_min_size(pos2(screen.center().x - 220.0, screen.top() + 74.0), vec2(440.0, height));
        ui.painter().rect_filled(sheet, 26, argb(SHEET));
        if veil.clicked() && veil.interact_pointer_pos().is_some_and(|p| !sheet.contains(p)) {
            return Picked::Cancelled;
        }
        let head = Rect::from_min_size(pos2(sheet.left() + 19.0, sheet.top() + 30.0), vec2(sheet.width() - 38.0, 39.0));
        let title_at = pos2(head.left(), head.center().y);
        text(ui.painter(), title_at, Align2::LEFT_CENTER, "Filter Layer", 28.0, Weight::Bold, argb(TITLE));
        let cancel_at = pos2(head.right(), head.center().y);
        let cancel = text(ui.painter(), cancel_at, Align2::RIGHT_CENTER, "Cancel", 17.0, Weight::Regular, argb(DIM));
        if ui.interact(cancel.expand(8.0), ui.scope_id().with("filter layers cancel"), Sense::click()).clicked() {
            return Picked::Cancelled;
        }
        let list = Rect::from_min_max(
            pos2(sheet.left() + 19.0, head.bottom() + 14.0),
            pos2(sheet.right() - 19.0, sheet.bottom() - 19.0),
        );
        let mut picked = Picked::Open;
        ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (group, filters) in grouped(&self.filters) {
                    let (rect, _) = ui.allocate_exact_size(vec2(list.width(), HEADING), Sense::hover());
                    let at = pos2(rect.left(), rect.bottom() - 10.0);
                    text(ui.painter(), at, Align2::LEFT_BOTTOM, group, 15.0, Weight::Bold, argb(DIM));
                    for f in filters {
                        let (rect, response) = ui.allocate_exact_size(vec2(list.width(), ROW), Sense::click());
                        if response.hovered() {
                            ui.painter().rect_filled(rect, 8, argb(RULE));
                        }
                        let at = pos2(rect.left() + 12.0, rect.center().y);
                        text(ui.painter(), at, Align2::LEFT_CENTER, &f.label, 17.0, Weight::Regular, argb(INK));
                        let rule = [pos2(rect.left() + 12.0, rect.bottom()), pos2(rect.right(), rect.bottom())];
                        ui.painter().line_segment(rule, egui::Stroke::new(1.0, argb(RULE)));
                        if response.clicked() {
                            picked = Picked::Filter(f.id.clone());
                        }
                    }
                }
            });
        });
        picked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: &str, group: &str) -> PluginLayerFilter {
        PluginLayerFilter { id: id.to_owned(), label: id.to_owned(), group: group.to_owned() }
    }

    #[test]
    fn filters_come_under_their_headings_in_the_order_they_are_listed() {
        let filters = [f("a", "Colour"), f("b", "Filters"), f("c", "Colour")];
        let groups = grouped(&filters);
        let names: Vec<(&str, Vec<&str>)> =
            groups.iter().map(|(g, list)| (*g, list.iter().map(|f| f.id.as_str()).collect())).collect();
        assert_eq!(names, vec![("Colour", vec!["a", "c"]), ("Filters", vec!["b"])]);
    }
}
