//! A pixel art canvas ([`procraft_engine::document::Document::pixel_art`]), as the session has it:
//! made so, shown with square pixels and a grid, and exported at whole-number scales. Its tools'
//! pixel behaviour hooks in from where each tool starts (see `procraft_engine::pixel`).

use std::path::Path;

use procraft_engine::document::Document;
use procraft_engine::geom::Affine;
use procraft_engine::pixel;

use procraft_engine::document::LayerId;
use procraft_engine::geom::{IRect, Vec2};
use procraft_engine::pixel::{Ink, Perfect, Stroke};

use crate::project::{write_atomically, write_png};
use crate::{CanvasSession, MobileError, PenSample, PointerKind, State, ToolKind};

/// A canvas pixel shown this many points across or more shows the pixel grid.
pub(crate) const GRID_FROM: f32 = 6.0;

/// How strongly the pixel grid shows when a canvas pixel is `across` points: none below
/// [`GRID_FROM`], fading in over its first doubling.
pub(crate) fn grid_strength(across: f32) -> f32 {
    if !across.is_finite() || across < GRID_FROM { 0.0 } else { ((across - GRID_FROM) / GRID_FROM).min(1.0) }
}

/// The pixel grid's strength for `doc` shown through `to_screen` on a screen of `display_scale`
/// pixels a point: none but on a pixel art canvas.
pub(crate) fn grid_for(doc: &Document, to_screen: Affine, display_scale: f32) -> f32 {
    if doc.pixel_art { grid_strength(to_screen.scale_factor() / display_scale.max(0.01)) } else { 0.0 }
}

/// From how many target pixels to a canvas pixel `doc` shows its pixels as squares: at every zoom
/// on a pixel art canvas, else from 100% (`display_scale`).
pub(crate) fn crisp_from(doc: &Document, display_scale: f32) -> f32 {
    if doc.pixel_art { 0.0 } else { display_scale }
}

/// The square sizes New canvas offers for pixel art.
#[uniffi::export]
pub fn pixel_art_sizes() -> Vec<u32> {
    pixel::SIZES.to_vec()
}

/// The longest side a pixel art canvas may have.
#[uniffi::export]
pub fn pixel_art_most_edge() -> u32 {
    pixel::MOST_EDGE
}

/// The scales a pixel art canvas exports PNGs at.
#[uniffi::export]
pub fn pixel_art_export_scales() -> Vec<u32> {
    pixel::EXPORT_SCALES.to_vec()
}

#[uniffi::export]
impl CanvasSession {
    /// Makes a new artwork pixel art, its background hidden (clear, as pixel art starts): called
    /// as it is made, before anything is drawn, so it's no step to undo. False if it's larger than
    /// a pixel art canvas may be.
    pub fn make_pixel_art(&self) -> bool {
        let mut st = self.shared.lock();
        if st.doc.width() > pixel::MOST_EDGE || st.doc.height() > pixel::MOST_EDGE {
            return false;
        }
        st.doc.pixel_art = true;
        st.doc.background_visible = false;
        true
    }

    /// Whether this is a pixel art canvas.
    pub fn is_pixel_art(&self) -> bool {
        self.shared.lock().doc.pixel_art
    }

    /// Writes the flattened picture as a PNG to `path`, each pixel a `scale` × `scale` square, as
    /// a pixel art canvas exports (straight alpha, with the canvas's profile).
    pub fn export_png_scaled(&self, path: String, scale: u32) -> Result<(), MobileError> {
        let (w, h, straight, icc) = self.finished_picture()?;
        self.exported(0.6);
        let scale = scale.clamp(1, 64);
        let too_big = || crate::project::file_error("exporting", "the picture would be too large");
        let big = pixel::scaled_up(&straight, w, h, scale).ok_or_else(too_big)?;
        drop(straight);
        let (bw, bh) = (w.checked_mul(scale).ok_or_else(too_big)?, h.checked_mul(scale).ok_or_else(too_big)?);
        self.exported(0.8);
        write_atomically(Path::new(&path), |p| write_png(p, bw, bh, &big, &icc))?;
        self.exported(1.0);
        Ok(())
    }
}

/// Where a stroke is: starting (with what it's drawn with), going on, ending, or called off (a
/// second finger made it a gesture).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Touch {
    Down(PointerKind),
    Moved,
    Up,
    Off,
}

/// A pixel art canvas's pencil or eraser stroke in progress.
pub(crate) struct PixelStroke {
    layer: LayerId,
    stroke: Stroke,
    footprint: Vec<(i32, i32)>,
    /// A one-pixel line's path, kept pixel-perfect.
    perfect: Option<Perfect>,
    last: Option<(i32, i32)>,
}

impl State {
    /// On a pixel art canvas, the paint and erase tools set whole pixels themselves: true if this
    /// stroke is theirs (taken here), false to leave it to the brushes.
    pub(crate) fn pixel_touch(&mut self, touch: Touch, samples: &[PenSample]) -> bool {
        if let Touch::Down(pointer) = touch {
            self.pixel_stroke = None;
            match self.pixel_stroke_begins(pointer) {
                Some(stroke) => self.pixel_stroke = Some(stroke),
                None => return false,
            }
        }
        let Some(mut on) = self.pixel_stroke.take() else { return false };
        let points: Vec<(i32, i32)> = self
            .to_canvas(samples)
            .iter()
            .filter(|s| s.pos.x.is_finite() && s.pos.y.is_finite())
            .map(|s| (s.pos.x.floor() as i32, s.pos.y.floor() as i32))
            .collect();
        if touch != Touch::Off {
            for p in points {
                let from = on.last.unwrap_or(p);
                for q in procraft_engine::pixel::line(from, p).into_iter().skip(usize::from(on.last.is_some())) {
                    self.pixel_draw(&mut on, q);
                }
                on.last = Some(p);
            }
            if let Some((rect, part)) = on.stroke.take_changed() {
                self.rewrite_layer(on.layer, IRect::new(rect[0], rect[1], rect[2], rect[3]), part);
            }
        }
        match touch {
            Touch::Down(_) | Touch::Moved => self.pixel_stroke = Some(on),
            // one step undo takes back; none for a stroke that changed nothing, or was called off
            Touch::Up | Touch::Off => {
                if let Some(canvas) = self.canvas.as_mut() {
                    canvas.collapse_steps();
                    if touch == Touch::Off || !on.stroke.any() {
                        canvas.undo();
                        canvas.forget_redo();
                    }
                }
                self.revision += 1;
            }
        }
        true
    }

    /// A pixel stroke as one starts with `pointer`, if it's the pixel pencil's: on a pixel art
    /// canvas, painting or erasing a paint layer that can be painted.
    fn pixel_stroke_begins(&mut self, pointer: PointerKind) -> Option<PixelStroke> {
        if !self.doc.pixel_art || self.plugin_artwork.is_some() || self.cloning || self.adjusting {
            return None;
        }
        let tool = if pointer == PointerKind::StylusEraser { ToolKind::Erase } else { self.tool };
        let id = self.doc.current();
        let layer = self.doc.layer(id).cloned()?;
        let paintable = layer.kind == procraft_engine::document::LayerKind::Paint
            && !self.doc.is_locked(id)
            && !layer.drawn_from_data();
        if tool == ToolKind::Smudge || !paintable {
            return None;
        }
        let sliders = self.sliders(tool);
        let width = pixel::pencil_width(sliders.size);
        let ink = match tool {
            ToolKind::Erase => Ink::Erase(sliders.opacity),
            _ => Ink::Paint(self.color, sliders.opacity),
        };
        let pixels = self.layer_pixels(id)?;
        let mask = self.selection.as_ref().map(|m| m.data.clone());
        let stroke = Stroke::new(self.doc.width(), self.doc.height(), pixels, ink, layer.alpha_lock, mask)?;
        self.canvas.as_mut()?.mark_steps();
        Some(PixelStroke {
            layer: id,
            stroke,
            footprint: pixel::footprint(width),
            perfect: (width == 1).then(Perfect::default),
            last: None,
        })
    }

    /// The pencil at pixel `p`: its footprint set there and where the symmetry copies it, a
    /// one-pixel line kept pixel-perfect.
    fn pixel_draw(&self, on: &mut PixelStroke, p: (i32, i32)) {
        let (go, corner) = match on.perfect.as_mut() {
            Some(perfect) => perfect.add(p),
            None => (true, None),
        };
        if let Some(c) = corner {
            for q in self.pixel_copies(c) {
                on.stroke.unset(q);
            }
        }
        if go {
            for &(dx, dy) in &on.footprint {
                for q in self.pixel_copies((p.0 + dx, p.1 + dy)) {
                    on.stroke.set(q);
                }
            }
        }
    }

    /// Pixel `p` and where the drawing guide's symmetry copies it, each a whole pixel.
    fn pixel_copies(&self, p: (i32, i32)) -> Vec<(i32, i32)> {
        let middle = Vec2::new(p.0 as f32 + 0.5, p.1 as f32 + 0.5);
        let mut out = vec![p];
        for (at, _, _) in self.doc.guide.copies(middle) {
            let q = (at.x.floor() as i32, at.y.floor() as i32);
            if !out.contains(&q) {
                out.push(q);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_fades_in_from_six_points() {
        assert_eq!(grid_strength(4.0), 0.0);
        assert_eq!(grid_strength(6.0), 0.0);
        assert_eq!(grid_strength(9.0), 0.5);
        assert_eq!(grid_strength(40.0), 1.0);
        assert_eq!(grid_strength(f32::NAN), 0.0);
    }

    #[test]
    fn a_pixel_art_canvas_is_made_clear_and_marked() {
        let s = CanvasSession::new(32, 32);
        assert!(!s.is_pixel_art());
        assert!(s.make_pixel_art());
        assert!(s.is_pixel_art());
        assert!(!s.shared.lock().doc.background_visible);
        let big = CanvasSession::new(2048, 64);
        assert!(!big.make_pixel_art(), "too large for pixel art");
    }
}
