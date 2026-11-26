//! Layers lifted out of the layers panel (a row dragged out past its side), held by the pointer
//! while it goes on, as the tablet's are held by a finger while the other hand opens another
//! artwork: held a moment over the top bar's Gallery, the gallery shows; held a moment over an
//! artwork there, it opens; let go over a canvas, they drop there (the core's `LayerBundle` keeps
//! them apart from the artwork they came from meanwhile). Let go over the gallery, or Escape, and
//! they go nowhere.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, CornerRadius, Id, LayerId, Order, Pos2, Rect, TextureHandle, Vec2, pos2, vec2};
use procraft_core::LayerBundle;

use crate::look::{Weight, argb, pc, text};

/// How long held layers stay over the gallery's button, or an artwork, before it opens.
const SPRING: Duration = Duration::from_millis(700);
/// The row held, as a layer's row is: its size, corner and thumbnail.
const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 69.0;
const CORNER: u8 = 8;
const THUMB: Vec2 = vec2(93.0, 61.0);
/// The badge saying letting go lands them (a copy), and the one counting several.
pub(crate) const LANDS: u32 = 0xFF30D158;
const COUNT: u32 = 0xFFFF3B30;

/// What held layers stay over that opens if they stay long enough.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Spring {
    /// The top bar's Gallery.
    Gallery,
    /// An artwork in the gallery (its folder).
    Artwork(String),
}

/// What became of held layers this frame.
pub enum Outcome {
    Holding,
    /// They stayed over what opens long enough.
    Open(Spring),
    /// Let go there (window points).
    Dropped(Pos2),
    /// Let go nowhere (Escape).
    Gone,
}

pub struct Held {
    pub bundle: Arc<LayerBundle>,
    /// The row lifted, and the artwork it came from (its folder in the gallery).
    pub layer: u64,
    pub from: Option<String>,
    /// How many layers and groups they are (a group with what is in it counting once).
    pub items: u32,
    name: String,
    thumbnail: Option<TextureHandle>,
    /// Where the pointer holds the row, from its corner.
    grab: Vec2,
    /// What they stay over, and since when.
    hover: Option<(Spring, Instant)>,
}

impl Held {
    pub fn new(
        bundle: Arc<LayerBundle>,
        layer: u64,
        from: Option<String>,
        thumbnail: Option<TextureHandle>,
        grab: Vec2,
    ) -> Held {
        let (items, name) = (bundle.items(), bundle.name());
        Held { bundle, layer, from, items, name, thumbnail, grab, hover: None }
    }

    /// Follows the pointer this frame: the row held drawn under it (a plus on it where letting go
    /// `lands` them), a stay over `under` timed; whether it was let go (and where), or stayed over
    /// what opens long enough.
    pub fn frame(&mut self, ctx: &egui::Context, under: Option<Spring>, lands: bool) -> Outcome {
        let (at, down, escape) =
            ctx.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down(), i.key_pressed(egui::Key::Escape)));
        if escape {
            return Outcome::Gone;
        }
        let Some(at) = at else { return Outcome::Holding };
        if !down {
            return Outcome::Dropped(at);
        }
        self.draw(ctx, at, lands);
        ctx.request_repaint();
        match (&self.hover, under) {
            (Some((over, since)), Some(now)) if *over == now => {
                if since.elapsed() >= SPRING {
                    self.hover = None;
                    return Outcome::Open(now);
                }
            }
            (_, now) => self.hover = now.map(|n| (n, Instant::now())),
        }
        Outcome::Holding
    }

    /// The row held, its corner `grab` from `at`: blue as the current row, its thumbnail and name,
    /// a count if they are several, and a green plus where letting go lands them.
    fn draw(&self, ctx: &egui::Context, at: Pos2, lands: bool) {
        let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("held layers")));
        let row = Rect::from_min_size(at - self.grab, vec2(WIDTH, HEIGHT));
        painter.rect_filled(
            row.translate(vec2(0.0, 4.0)).expand(2.0),
            CornerRadius::same(CORNER),
            Color32::from_black_alpha(60),
        );
        painter.rect_filled(row, CornerRadius::same(CORNER), argb(pc::ACCENT));
        let thumb = Rect::from_min_size(pos2(row.left() + 4.0, row.center().y - THUMB.y / 2.0), THUMB);
        painter.rect_filled(thumb, CornerRadius::same(4), argb(crate::layers::THUMB_BACKGROUND));
        if let Some(t) = &self.thumbnail {
            let size = t.size_vec2();
            let fit = (thumb.width() / size.x.max(1.0)).min(thumb.height() / size.y.max(1.0));
            let shown = Rect::from_center_size(thumb.center(), size * fit);
            painter.image(t.id(), shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        let name_at = pos2(thumb.right() + 12.0, row.center().y);
        let clip = Rect::from_min_max(pos2(name_at.x, row.top()), pos2(row.right() - 12.0, row.bottom()));
        text(
            &painter.with_clip_rect(clip),
            name_at,
            Align2::LEFT_CENTER,
            &self.name,
            17.0,
            Weight::Regular,
            Color32::WHITE,
        );
        let badge = |centre: Pos2, fill: u32, label: &str| {
            painter.circle_filled(centre, 12.0, argb(fill));
            text(&painter, centre, Align2::CENTER_CENTER, label, 15.0, Weight::Medium, Color32::WHITE);
        };
        if self.items > 1 {
            badge(row.left_top() + vec2(4.0, 4.0), COUNT, &self.items.to_string());
        }
        if lands {
            badge(row.right_top() + vec2(-4.0, 4.0), LANDS, "+");
        }
    }
}
