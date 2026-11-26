//! The Textures sheet a plugin's texture control opens (interface 15): a sheet over everything, as
//! the Source Library is, with the app's grains, then the pictures picked for the plugin's own file
//! controls the texture offers, then a tile that picks another into the first of them with none. A
//! click chooses one; a right click on one of the plugin's pictures offers to delete it (its file
//! control cleared).

use std::sync::OnceLock;

use egui::epaint::RectShape;
use egui::{Align2, Color32, Rect, Sense, Stroke, TextureHandle, Ui, UiBuilder, pos2, vec2};
use procraft_core::{PluginControl, PluginTexture, Plugins};

use crate::look::{Themed, Weight, argb, pc, text};

const VEIL: u32 = 0x80000000;
const SHEET: Themed = Themed(0xFF1C1C1E, 0xFFF2F2F7);
const TITLE: Themed = Themed(0xFFC1C1C1, 0xFF3A3A3A);
const DIM: u32 = 0xFF868686;
const EDGE: Themed = Themed(0xFF5A5A5A, 0xFFC8C8C8);
/// A tile, the gaps between them, and a row's height with its name under it.
const CELL: f32 = 147.0;
const CELL_GAP: f32 = 53.0;
const CELL_HEIGHT: f32 = 146.0 + 5.0 + 21.0;
const CELL_ROW_GAP: f32 = 18.0;
/// How big the plugin's pictures are made to show.
const THUMBNAIL: u32 = 256;

/// The app's grains' names, by their number in the Source Library (its Blank first), made once.
fn grain_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| procraft_core::source_library(true).into_iter().map(|image| image.name).collect())
}

/// The texture control kept under `key`, among `plugins`' controls.
fn texture_of(plugins: &Plugins, key: &str) -> Option<PluginTexture> {
    plugins.plugins().into_iter().flat_map(|p| p.controls).find_map(|c| match c {
        PluginControl::Texture { control } if control.key == key => Some(control),
        _ => None,
    })
}

/// What the picture picked for the file control kept under `key` is called, if one is.
fn picked_name(plugins: &Plugins, key: &str) -> Option<String> {
    plugins.plugins().into_iter().flat_map(|p| p.controls).find_map(|c| match c {
        PluginControl::File { control } if control.key == key => control.chosen,
        _ => None,
    })
}

/// The texture `control` has chosen, in words: a grain's name, the name of a picture of the
/// plugin's, or None.
pub fn chosen_name(plugins: &Plugins, control: &PluginTexture) -> String {
    let named = match usize::try_from(control.value) {
        Ok(0) => None,
        Ok(n) => grain_names().get(n).cloned(),
        Err(_) => {
            let own = usize::try_from(-1 - control.value).ok().and_then(|k| control.own.get(k));
            own.and_then(|key| picked_name(plugins, key))
        }
    };
    named.unwrap_or_else(|| "None".to_owned())
}

/// The sheet, while open: the control it picks for, the grains' pictures, and the plugin's own
/// pictures by their file controls (with what each is called and its picture, if one is picked).
pub struct Textures {
    key: String,
    grains: Vec<(String, TextureHandle)>,
    own: Vec<(String, Option<(String, TextureHandle)>)>,
    /// A right click's offer to delete one of the plugin's pictures: its place, and where.
    deleting: Option<(usize, egui::Pos2)>,
}

impl Textures {
    /// The sheet for the texture control kept under `key`; none if there is none.
    pub fn open(ctx: &egui::Context, plugins: &Plugins, key: &str) -> Option<Textures> {
        let control = texture_of(plugins, key)?;
        let grains = procraft_core::source_library(true)
            .into_iter()
            .filter_map(|image| Some((image.name, grey(ctx, &image.pixels)?)))
            .collect();
        let mut sheet = Textures { key: key.to_owned(), grains, own: Vec::new(), deleting: None };
        sheet.read_own(ctx, plugins, &control);
        Some(sheet)
    }

    /// The plugin's own pictures read again, as they are now.
    fn read_own(&mut self, ctx: &egui::Context, plugins: &Plugins, control: &PluginTexture) {
        self.own = control
            .own
            .iter()
            .map(|key| {
                let picture = picked_name(plugins, key).zip(plugins.picture_thumbnail(key.to_string(), THUMBNAIL));
                let shown = picture.map(|(name, p)| {
                    let image =
                        egui::ColorImage::from_rgba_unmultiplied([p.width as usize, p.height as usize], &p.pixels);
                    (name, ctx.load_texture("plugin texture", image, egui::TextureOptions::LINEAR))
                });
                (key.clone(), shown)
            })
            .collect();
    }

    /// Draws the sheet over the window: whether it closed (Cancel, or a texture chosen) and whether
    /// the plugins changed (a texture chosen, a picture added or deleted), to keep.
    pub fn show(&mut self, ctx: &egui::Context, plugins: &Plugins) -> (bool, bool) {
        let screen = ctx.content_rect();
        let Some(control) = texture_of(plugins, &self.key) else { return (true, false) };
        let mut done = (false, false);
        egui::Area::new(egui::Id::unique("plugin textures")).order(egui::Order::Foreground).fixed_pos(screen.min).show(
            ctx,
            |ui| {
                ui.set_min_size(screen.size());
                done = self.sheet(ui, screen, plugins, &control);
            },
        );
        done
    }

    fn sheet(&mut self, ui: &mut Ui, screen: Rect, plugins: &Plugins, control: &PluginTexture) -> (bool, bool) {
        ui.painter().rect_filled(screen, 0, argb(VEIL));
        let _ = ui.interact(screen, ui.scope_id().with("textures veil"), Sense::click_and_drag());
        let sheet = Rect::from_min_size(pos2(screen.center().x - 290.0, screen.top() + 74.0), vec2(580.0, 575.0));
        ui.painter().rect_filled(sheet, 26, argb(SHEET));
        let head = Rect::from_min_size(pos2(sheet.left() + 19.0, sheet.top() + 40.0), vec2(sheet.width() - 38.0, 39.0));
        text(
            ui.painter(),
            pos2(head.left(), head.center().y),
            Align2::LEFT_CENTER,
            "Textures",
            32.0,
            Weight::Bold,
            argb(TITLE),
        );
        let cancel_at = pos2(head.right(), head.center().y);
        let cancel = text(ui.painter(), cancel_at, Align2::RIGHT_CENTER, "Cancel", 17.0, Weight::Regular, argb(DIM));
        if ui.interact(cancel.expand(8.0), ui.scope_id().with("textures cancel"), Sense::click()).clicked() {
            return (true, false);
        }
        // the grains (their Blank is none), then the plugin's pictures, then a tile for another
        enum Tile<'a> {
            Grain(usize, &'a str, &'a TextureHandle),
            Own(usize, &'a str, &'a TextureHandle),
            Add,
        }
        let mut tiles: Vec<Tile<'_>> =
            self.grains.iter().enumerate().map(|(n, (name, t))| Tile::Grain(n, name, t)).collect();
        tiles.extend(self.own.iter().enumerate().filter_map(|(k, (_, p))| p.as_ref().map(|(n, t)| Tile::Own(k, n, t))));
        let free = self.own.iter().position(|(_, p)| p.is_none());
        if free.is_some() {
            tiles.push(Tile::Add);
        }
        let grid = Rect::from_min_max(
            pos2(sheet.left() + 16.0, head.bottom() + 20.0),
            pos2(sheet.right() - 16.0, sheet.bottom()),
        );
        let step = (grid.width() - 2.0 * CELL_GAP) / 3.0 + CELL_GAP;
        let mut chosen = None;
        let mut add = false;
        let mut delete_at = None;
        ui.scope_builder(UiBuilder::new().max_rect(grid), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("plugin textures")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    ui.add_space(11.0);
                    for (r, row) in tiles.chunks(3).enumerate() {
                        if r > 0 {
                            ui.add_space(CELL_ROW_GAP);
                        }
                        let (line, _) = ui.allocate_exact_size(vec2(grid.width(), CELL_HEIGHT), Sense::hover());
                        for (c, tile) in row.iter().enumerate() {
                            let square =
                                Rect::from_min_size(pos2(line.left() + step * c as f32, line.top()), vec2(CELL, 146.0));
                            ui.painter().rect_filled(square, 8, Color32::BLACK);
                            let (name, value) = match tile {
                                Tile::Grain(n, name, t) | Tile::Own(n, name, t) => {
                                    ui.painter().add(
                                        RectShape::filled(square, 8, Color32::WHITE)
                                            .with_texture(t.id(), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))),
                                    );
                                    let value = match tile {
                                        Tile::Own(k, ..) => -1 - *k as i32,
                                        _ => *n as i32,
                                    };
                                    (*name, Some(value))
                                }
                                Tile::Add => {
                                    let at = square.center();
                                    text(
                                        ui.painter(),
                                        at,
                                        Align2::CENTER_CENTER,
                                        "+",
                                        48.0,
                                        Weight::Regular,
                                        argb(pc::ACCENT),
                                    );
                                    ("Add a picture", None)
                                }
                            };
                            let edge = if value == Some(control.value) { argb(pc::ACCENT) } else { argb(EDGE) };
                            let width = if value == Some(control.value) { 3.0 } else { 1.0 };
                            ui.painter().rect_stroke(square, 8, Stroke::new(width, edge), egui::StrokeKind::Inside);
                            let label = Rect::from_min_size(pos2(square.left(), square.bottom()), vec2(CELL, 26.0));
                            let painter = ui.painter().with_clip_rect(label.intersect(ui.clip_rect()));
                            let at = pos2(square.left() + 3.0, square.bottom() + 5.0);
                            text(&painter, at, Align2::LEFT_TOP, name, 13.0, Weight::Bold, argb(TITLE));
                            let response = ui.interact(
                                square.union(label),
                                ui.scope_id().with(("texture tile", r, c)),
                                Sense::click(),
                            );
                            if response.clicked() {
                                match value {
                                    Some(value) => chosen = Some(value),
                                    None => add = true,
                                }
                            }
                            if let (Tile::Own(k, ..), true) = (tile, response.secondary_clicked()) {
                                delete_at = response.interact_pointer_pos().map(|p| (*k, p));
                            }
                        }
                    }
                    ui.add_space(24.0);
                });
        });
        if delete_at.is_some() {
            self.deleting = delete_at;
        }
        // another picture, into the first of the plugin's with none, and chosen
        if add && let Some(k) = free {
            let key = self.own.get(k).map(|(key, _)| key.clone()).unwrap_or_default();
            if let Some((bytes, name)) = crate::files::pick_control_picture(&control.label) {
                if plugins.set_picture_file(key, name, bytes) {
                    plugins.set_texture(control.key.clone(), -1 - k as i32);
                    return (true, true);
                }
                crate::files::say("This picture couldn't be used. Pick a PNG or a JPEG.");
            }
        }
        if let Some(value) = chosen {
            plugins.set_texture(control.key.clone(), value);
            return (true, true);
        }
        // Delete, offered by a right click on one of the plugin's pictures
        if let Some((k, at)) = self.deleting {
            let offer = Rect::from_min_size(at, vec2(180.0, 44.0));
            ui.painter().rect_filled(offer, 12, argb(SHEET));
            ui.painter().rect_stroke(offer, 12, Stroke::new(1.0, argb(EDGE)), egui::StrokeKind::Inside);
            let ink = Color32::from_rgb(0xFF, 0x45, 0x3A);
            text(ui.painter(), offer.center(), Align2::CENTER_CENTER, "Delete picture", 17.0, Weight::Regular, ink);
            let response = ui.interact(offer, ui.scope_id().with("texture delete"), Sense::click());
            if response.clicked() {
                self.deleting = None;
                if let Some((key, _)) = self.own.get(k) {
                    plugins.clear_file(key.clone());
                }
                self.read_own(ui.ctx(), plugins, control);
                return (false, true);
            }
            if ui.input(|i| i.pointer.any_click()) && !response.hovered() {
                self.deleting = None;
            }
        }
        (false, false)
    }
}

/// A square greyscale image as its greys; none if it is empty or not square.
fn grey(ctx: &egui::Context, pixels: &[u8]) -> Option<TextureHandle> {
    let n = (pixels.len() as f64).sqrt() as usize;
    if n == 0 || n * n != pixels.len() {
        return None;
    }
    let rgba: Vec<u8> = pixels.iter().flat_map(|&v| [v, v, v, 255]).collect();
    let image = egui::ColorImage::from_rgba_unmultiplied([n, n], &rgba);
    Some(ctx.load_texture("texture grain", image, egui::TextureOptions::LINEAR))
}
