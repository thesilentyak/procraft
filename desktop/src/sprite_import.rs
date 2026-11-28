//! Insert a sprite sheet (Actions › Add): the picked sheet with each frame it found boxed, the
//! cleaned frames playing, the animation's name, and folded under Not right? the grid and the
//! background (docs/pixel-art/05-sprite-import-design.md).

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use egui::{Align2, Color32, Id, Order, Rect, Sense, Stroke, TextureHandle, TextureOptions, pos2, vec2};
use procraft_core::{CanvasSession, SpriteSheetImport, SpriteSheetInserted, SpriteSheetLook, ThumbnailImage};

use crate::look::{Themed, Weight, argb, pc, text};
use crate::panel::PANEL_TITLE;

const FILL: Themed = Themed(0xFF383838, 0xFFE6E6E6);
const GROUP: Themed = Themed(0x420C0C0C, 0xFFD8D8D8);
const DIM: Themed = Themed(0x99FFFFFF, 0x99000000);
const WIDTH: f32 = 640.0;
const SHEET: [f32; 2] = [380.0, 240.0];

/// How the import ended.
pub enum Ended {
    Cancelled,
    Added(SpriteSheetInserted),
    /// The frames couldn't be added (no room for the layers, or the canvas couldn't grow).
    Failed,
}

/// A sprite sheet being imported.
pub struct SheetImport {
    sheet: Arc<SpriteSheetImport>,
    look: Option<SpriteSheetLook>,
    name: String,
    across: u32,
    down: u32,
    background: Option<u32>,
    more: bool,
    picking: bool,
    read: bool,
    shown: Option<TextureHandle>,
    frames: Vec<TextureHandle>,
    started: Instant,
    /// The frames (by number, from 0) left out of the import: clicked off in the sheet.
    skipped: Vec<u32>,
}

impl SheetImport {
    /// Asks for a sheet picture; none when the user cancels, else the import or why the picture
    /// can't be read.
    pub fn pick() -> Option<Result<SheetImport, String>> {
        let path = crate::files::dialog()
            .set_title("Insert a sprite sheet")
            .add_filter("Pictures", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
            .pick_file()?;
        Some(SheetImport::open(&path))
    }

    fn open(path: &Path) -> Result<SheetImport, String> {
        let picture = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
        let image = ThumbnailImage { width: picture.width(), height: picture.height(), rgba: picture.into_raw() };
        let sheet = SpriteSheetImport::new(image, path.to_string_lossy().into_owned());
        Ok(SheetImport {
            name: sheet.suggested_name(),
            sheet,
            look: None,
            across: 0,
            down: 0,
            background: None,
            more: false,
            picking: false,
            read: false,
            shown: None,
            frames: Vec::new(),
            started: Instant::now(),
            skipped: Vec::new(),
        })
    }

    fn reread(&mut self, ctx: &egui::Context, session: &Arc<CanvasSession>) {
        self.look = self.sheet.look(session.clone(), self.across, self.down, self.background);
        let texture = |name: &str, t: &ThumbnailImage| {
            let size = [t.width as usize, t.height as usize];
            ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, &t.rgba), TextureOptions::NEAREST)
        };
        self.shown = self.look.as_ref().map(|l| texture("sprite sheet", &l.shown));
        self.frames = self
            .look
            .as_ref()
            .map(|l| l.frames.iter().enumerate().map(|(k, f)| texture(&format!("sheet frame {k}"), f)).collect())
            .unwrap_or_default();
        self.started = Instant::now();
        self.skipped.clear();
        self.read = true;
    }

    /// Shows the import window over everything; how it ended once it has.
    pub fn show(&mut self, ctx: &egui::Context, session: &Arc<CanvasSession>) -> Option<Ended> {
        if !self.read {
            self.reread(ctx, session);
        }
        let screen = ctx.content_rect();
        let height =
            8.0 + 30.0 + SHEET[1] + 12.0 + 22.0 + 10.0 + 44.0 + 8.0 + 36.0 + if self.more { 104.0 } else { 0.0 } + 52.0;
        let rect = Rect::from_center_size(screen.center(), vec2(WIDTH, height));
        let mut ended = None;
        let mut again = false;
        egui::Area::new(Id::unique("sprite sheet import")).fixed_pos(screen.min).order(Order::Foreground).show(
            ctx,
            |ui| {
                // nothing under the window takes a press; a press beside it cancels
                if ui.allocate_rect(screen, Sense::click()).clicked() && !self.picking {
                    ended = Some(Ended::Cancelled);
                }
                ui.painter().rect_filled(screen, 0, Color32::from_black_alpha(90));
                ui.allocate_rect(rect, Sense::click_and_drag());
                ui.painter().rect_filled(rect, 16, argb(FILL));
                let (left, right) = (rect.left() + 20.0, rect.right() - 20.0);
                let mut y = rect.top() + 18.0;
                text(
                    ui.painter(),
                    pos2(left, y),
                    Align2::LEFT_TOP,
                    "Insert a sprite sheet",
                    20.0,
                    Weight::Bold,
                    argb(PANEL_TITLE),
                );
                y += 30.0;
                // the sheet, each frame boxed and numbered; while picking, a click takes its colour
                let sheet_box = Rect::from_min_size(pos2(left, y), vec2(SHEET[0], SHEET[1]));
                ui.painter().rect_filled(sheet_box, 8, argb(GROUP));
                if let (Some(look), Some(shown)) = (self.look.as_ref(), self.shown.as_ref()) {
                    let k = (SHEET[0] / look.picture_width as f32).min(SHEET[1] / look.picture_height as f32);
                    let drawn = Rect::from_center_size(
                        sheet_box.center(),
                        vec2(look.picture_width as f32 * k, look.picture_height as f32 * k),
                    );
                    ui.painter().image(
                        shown.id(),
                        drawn,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    for (n, p) in look.places.iter().enumerate() {
                        let r = Rect::from_min_size(
                            drawn.min + vec2(p.x as f32 * k, p.y as f32 * k),
                            vec2(p.width as f32 * k, p.height as f32 * k),
                        );
                        let left_out = u32::try_from(n).is_ok_and(|n| self.skipped.contains(&n));
                        if left_out {
                            ui.painter().rect_filled(r, 0, Color32::from_black_alpha(176));
                        }
                        let edge = if left_out { Color32::GRAY } else { argb(pc::ACCENT) };
                        ui.painter().rect_stroke(r, 0, Stroke::new(2.0, edge), egui::StrokeKind::Inside);
                        text(
                            ui.painter(),
                            r.min + vec2(4.0, 2.0),
                            Align2::LEFT_TOP,
                            &format!("{}", n + 1),
                            12.0,
                            Weight::Bold,
                            Color32::WHITE,
                        );
                    }
                    let click = ui.interact(drawn, ui.scope_id().with("sheet picture"), Sense::click());
                    // not picking, a click on a frame leaves it out, or puts it back
                    if !self.picking
                        && click.clicked()
                        && let Some(at) = click.interact_pointer_pos()
                    {
                        let (px, py) = ((at.x - drawn.left()) / k, (at.y - drawn.top()) / k);
                        let hit = look.places.iter().position(|p| {
                            px >= p.x as f32
                                && px < (p.x + p.width) as f32
                                && py >= p.y as f32
                                && py < (p.y + p.height) as f32
                        });
                        if let Some(hit) = hit.and_then(|h| u32::try_from(h).ok()) {
                            match self.skipped.iter().position(|&s| s == hit) {
                                Some(i) => {
                                    self.skipped.remove(i);
                                }
                                None => self.skipped.push(hit),
                            }
                        }
                    }
                    if self.picking
                        && click.clicked()
                        && let Some(at) = click.interact_pointer_pos()
                    {
                        let sx = ((at.x - drawn.left()) / drawn.width() * look.shown.width as f32) as usize;
                        let sy = ((at.y - drawn.top()) / drawn.height() * look.shown.height as f32) as usize;
                        let i = (sy.min(look.shown.height as usize - 1) * look.shown.width as usize
                            + sx.min(look.shown.width as usize - 1))
                            * 4;
                        if let Some(&[r, g, b]) = look.shown.rgba.get(i..i + 3) {
                            self.background = Some((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
                            self.picking = false;
                            again = true;
                        }
                    }
                }
                // the frames, cleaned, playing at the artwork's speed
                let play_box = Rect::from_min_max(pos2(sheet_box.right() + 14.0, y), pos2(right, y + SHEET[1]));
                ui.painter().rect_filled(play_box, 8, argb(GROUP));
                let kept: Vec<&TextureHandle> = self
                    .frames
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| u32::try_from(*k).is_ok_and(|k| !self.skipped.contains(&k)))
                    .map(|(_, f)| f)
                    .collect();
                if let Some(look) = self.look.as_ref().filter(|_| !kept.is_empty()) {
                    let fps = look.fps.max(1) as f32;
                    let at = (self.started.elapsed().as_secs_f32() * fps) as usize % kept.len();
                    if let Some(frame) = kept.get(at) {
                        let [w, h] = frame.size().map(|v| v.max(1) as f32);
                        let inner = play_box.shrink(12.0);
                        let k = (inner.width() / w).min(inner.height() / h);
                        let r = Rect::from_center_size(inner.center(), vec2(w * k, h * k));
                        ui.painter().image(
                            frame.id(),
                            r,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / fps));
                } else {
                    text(
                        ui.painter(),
                        play_box.center(),
                        Align2::CENTER_CENTER,
                        "No sprites found",
                        14.0,
                        Weight::Regular,
                        argb(DIM),
                    );
                }
                y += SHEET[1] + 12.0;
                let said = match self.look.as_ref() {
                    None => "Nothing was found. Try Not right? below: set the grid, or pick the background.".to_owned(),
                    Some(look) => {
                        let n = look.places.len().saturating_sub(self.skipped.len());
                        let (w, h) = look.frames.first().map_or((0, 0), |f| (f.width, f.height));
                        let mut said = format!("{n} {}, {w} × {h}", if n == 1 { "frame" } else { "frames" });
                        if !self.skipped.is_empty() {
                            said += &format!(" ({} left out)", self.skipped.len());
                        }
                        if look.pixel_size > 1.5 {
                            said += &format!(", drawn ×{:.1}", look.pixel_size);
                        }
                        if look.too_big {
                            said += ". Too big for a pixel art canvas (1024 a side at most).";
                        } else if look.grows {
                            said += &format!(
                                ". The canvas will grow to {} × {} to fit.",
                                look.canvas_width, look.canvas_height
                            );
                        }
                        said
                    }
                };
                text(ui.painter(), pos2(left, y), Align2::LEFT_TOP, &said, 14.0, Weight::Regular, argb(DIM));
                y += 22.0 + 10.0;
                // the name
                let field = Rect::from_min_max(pos2(left, y), pos2(right, y + 44.0));
                ui.painter().rect_filled(field, 12, argb(GROUP));
                ui.put(
                    field.shrink2(vec2(14.0, 10.0)),
                    egui::TextEdit::singleline(&mut self.name)
                        .font(Weight::Regular.font(17.0))
                        .text_color(argb(PANEL_TITLE))
                        .frame(egui::Frame::NONE)
                        .char_limit(40)
                        .hint_text("Name: idle, walk, attack\u{2026}"),
                );
                y += 44.0 + 8.0;
                let more = Rect::from_min_size(pos2(left, y), vec2(140.0, 28.0));
                let label = if self.more { "Not right? \u{25B4}" } else { "Not right? \u{25BE}" };
                text(
                    ui.painter(),
                    pos2(left, more.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    15.0,
                    Weight::Regular,
                    argb(pc::ACCENT),
                );
                if ui.interact(more, ui.scope_id().with("sheet more"), Sense::click()).clicked() {
                    self.more = !self.more;
                }
                y += 36.0;
                if self.more {
                    let row = |ui: &mut egui::Ui, y: f32, label: &str| {
                        text(
                            ui.painter(),
                            pos2(left, y + 14.0),
                            Align2::LEFT_CENTER,
                            label,
                            15.0,
                            Weight::Regular,
                            argb(PANEL_TITLE),
                        );
                    };
                    row(ui, y, "Grid");
                    let mut x = left + 110.0;
                    for (k, which) in ["Across", "Down"].into_iter().enumerate() {
                        let value = if k == 0 { self.across } else { self.down };
                        text(
                            ui.painter(),
                            pos2(x, y + 14.0),
                            Align2::LEFT_CENTER,
                            which,
                            14.0,
                            Weight::Regular,
                            argb(DIM),
                        );
                        x += 56.0;
                        let minus = Rect::from_min_size(pos2(x, y), vec2(28.0, 28.0));
                        let shown = if value == 0 { "Auto".to_owned() } else { value.to_string() };
                        let plus = Rect::from_min_size(pos2(x + 28.0 + 44.0, y), vec2(28.0, 28.0));
                        text(
                            ui.painter(),
                            minus.center(),
                            Align2::CENTER_CENTER,
                            "\u{2212}",
                            20.0,
                            Weight::Regular,
                            argb(pc::ACCENT),
                        );
                        text(
                            ui.painter(),
                            pos2(x + 28.0 + 22.0, y + 14.0),
                            Align2::CENTER_CENTER,
                            &shown,
                            15.0,
                            Weight::Regular,
                            argb(PANEL_TITLE),
                        );
                        text(
                            ui.painter(),
                            plus.center(),
                            Align2::CENTER_CENTER,
                            "+",
                            20.0,
                            Weight::Regular,
                            argb(pc::ACCENT),
                        );
                        let step =
                            if ui.interact(minus, ui.scope_id().with(("sheet minus", k)), Sense::click()).clicked() {
                                Some(value.saturating_sub(1))
                            } else if ui.interact(plus, ui.scope_id().with(("sheet plus", k)), Sense::click()).clicked()
                            {
                                Some((value + 1).min(16))
                            } else {
                                None
                            };
                        if let Some(v) = step {
                            if k == 0 {
                                self.across = v;
                                if v > 0 && self.down == 0 {
                                    self.down = 1;
                                }
                            } else {
                                self.down = v;
                                if v > 0 && self.across == 0 {
                                    self.across = 1;
                                }
                            }
                            again = true;
                        }
                        x += 28.0 * 2.0 + 44.0 + 24.0;
                    }
                    y += 30.0;
                    text(
                        ui.painter(),
                        pos2(left, y),
                        Align2::LEFT_TOP,
                        "Auto finds frames by the gaps between them; set a grid when sprites touch.",
                        13.0,
                        Weight::Regular,
                        argb(DIM),
                    );
                    y += 26.0;
                    row(ui, y, "Background");
                    let swatch = Rect::from_min_size(pos2(left + 110.0, y), vec2(28.0, 28.0));
                    let colour = self.background.or(self.look.as_ref().and_then(|l| l.background));
                    match colour {
                        Some(c) => {
                            ui.painter().rect_filled(
                                swatch,
                                6,
                                Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8),
                            );
                        }
                        None => {
                            ui.painter().rect_stroke(swatch, 6, Stroke::new(1.0, argb(DIM)), egui::StrokeKind::Inside);
                        }
                    }
                    let said = match (colour, self.background) {
                        (None, _) => "Transparent",
                        (Some(_), None) => "Found",
                        (Some(_), Some(_)) => "Picked",
                    };
                    text(
                        ui.painter(),
                        pos2(swatch.right() + 10.0, y + 14.0),
                        Align2::LEFT_CENTER,
                        said,
                        14.0,
                        Weight::Regular,
                        argb(DIM),
                    );
                    let pick = Rect::from_min_size(pos2(swatch.right() + 90.0, y), vec2(160.0, 28.0));
                    let pick_label = if self.picking { "Click the sheet\u{2026}" } else { "Pick from the sheet" };
                    text(
                        ui.painter(),
                        pos2(pick.left(), pick.center().y),
                        Align2::LEFT_CENTER,
                        pick_label,
                        15.0,
                        Weight::Regular,
                        argb(pc::ACCENT),
                    );
                    if ui.interact(pick, ui.scope_id().with("sheet pick"), Sense::click()).clicked() {
                        self.picking = !self.picking;
                    }
                    if self.background.is_some() {
                        let auto = Rect::from_min_size(pos2(pick.right() + 12.0, y), vec2(50.0, 28.0));
                        text(
                            ui.painter(),
                            pos2(auto.left(), auto.center().y),
                            Align2::LEFT_CENTER,
                            "Auto",
                            15.0,
                            Weight::Regular,
                            argb(pc::ACCENT),
                        );
                        if ui.interact(auto, ui.scope_id().with("sheet auto"), Sense::click()).clicked() {
                            self.background = None;
                            again = true;
                        }
                    }
                    y += 48.0;
                }
                // Cancel and Import
                let n = self
                    .look
                    .as_ref()
                    .filter(|l| !l.too_big)
                    .map_or(0, |l| l.places.len().saturating_sub(self.skipped.len()));
                let import = if n > 0 {
                    format!("Import {n} {}", if n == 1 { "frame" } else { "frames" })
                } else {
                    "Import".to_owned()
                };
                let import_rect = Rect::from_min_max(pos2(right - 170.0, y), pos2(right, y + 38.0));
                let cancel_rect =
                    Rect::from_min_max(pos2(import_rect.left() - 110.0, y), pos2(import_rect.left() - 10.0, y + 38.0));
                ui.painter().rect_filled(import_rect, 10, if n > 0 { argb(pc::ACCENT) } else { argb(GROUP) });
                text(
                    ui.painter(),
                    import_rect.center(),
                    Align2::CENTER_CENTER,
                    &import,
                    16.0,
                    Weight::Regular,
                    Color32::WHITE,
                );
                text(
                    ui.painter(),
                    cancel_rect.center(),
                    Align2::CENTER_CENTER,
                    "Cancel",
                    16.0,
                    Weight::Regular,
                    argb(pc::ACCENT),
                );
                if ui.interact(cancel_rect, ui.scope_id().with("sheet cancel"), Sense::click()).clicked() {
                    ended = Some(Ended::Cancelled);
                }
                if n > 0 && ui.interact(import_rect, ui.scope_id().with("sheet import"), Sense::click()).clicked() {
                    ended = Some(match self.sheet.insert(session.clone(), self.name.clone(), self.skipped.clone()) {
                        Some(done) => Ended::Added(done),
                        None => Ended::Failed,
                    });
                }
            },
        );
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ended = Some(Ended::Cancelled);
        }
        if again {
            self.reread(ctx, session);
        }
        ended
    }
}
