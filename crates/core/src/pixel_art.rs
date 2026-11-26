//! A pixel art canvas ([`procraft_engine::document::Document::pixel_art`]), as the session has it:
//! made so, shown with square pixels and a grid, and exported at whole-number scales. Its tools'
//! pixel behaviour hooks in from where each tool starts (see `procraft_engine::pixel`).

use std::path::Path;

use procraft_engine::document::Document;
use procraft_engine::geom::Affine;
use procraft_engine::pixel;

use crate::project::{write_atomically, write_png};
use crate::{CanvasSession, MobileError};

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
