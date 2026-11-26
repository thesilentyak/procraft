//! Layers lifted out of one artwork to drop into another (or back into the same one): what they
//! are and their pixels, held apart from either canvas while the drag goes on, so the artwork they
//! came from can be closed and another opened before they are let go.

use std::collections::HashMap;
use std::sync::Arc;

use procraft_engine::document::{Landing, Layer, LayerId, LayerKind};
use procraft_engine::geom::IRect;

use crate::CanvasSession;

/// Where dropped layers go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LayerDrop {
    /// Above the current layer (with its mask) or group, in its group: let go over another
    /// artwork's canvas.
    AboveCurrent,
    /// At the top of the stack: let go over the canvas of the artwork they came from.
    Top,
    /// By the rows of the layers panel, where [`CanvasSession::row_drop`] (with no layer dragged)
    /// placed them: let go over the panel.
    Row { place: crate::RowDrop },
}

/// A layer's pixels where it is painted: the rectangle on its canvas and that rectangle's rows
/// (premultiplied RGBA8).
#[derive(Clone, Debug)]
struct Part {
    rect: IRect,
    rgba: Vec<u8>,
}

impl Part {
    /// The pixel at (`x`, `y`) of the canvas it came from, if it is inside the part.
    fn at(&self, x: i32, y: i32) -> Option<[u8; 4]> {
        let r = self.rect;
        if x < r.x0 || y < r.y0 || x >= r.x1 || y >= r.y1 {
            return None;
        }
        let i = ((y - r.y0) as usize * r.width() as usize + (x - r.x0) as usize) * 4;
        self.rgba.get(i..i + 4).and_then(|p| p.try_into().ok())
    }
}

/// Layers and groups lifted out of an artwork, each with what goes with it (a layer's mask, a
/// group's layers): their settings as the artwork had them, and the pixels of those with any, read
/// off its canvas as they were lifted.
#[derive(uniffi::Object)]
pub struct LayerBundle {
    /// The size of the canvas they came from.
    width: u32,
    height: u32,
    /// Bottom first, each item standing alone (in no group).
    layers: Vec<Layer>,
    /// The pixels of each layer painted anywhere (the others are clear).
    pixels: HashMap<LayerId, Part>,
}

#[uniffi::export]
impl LayerBundle {
    /// How many layers and groups it holds as a drag counts them (what is in a group is the
    /// group's).
    pub fn items(&self) -> u32 {
        self.item_ids().len() as u32
    }

    /// The top one's name.
    pub fn name(&self) -> String {
        let top = self.item_ids().last().copied();
        self.layers.iter().find(|l| Some(l.id) == top).map(|l| l.name.clone()).unwrap_or_default()
    }

    /// How many layers with pixels it holds (masks too): the room dropping it takes.
    pub fn layer_count(&self) -> u32 {
        self.layers.iter().filter(|l| l.kind != LayerKind::Group).count() as u32
    }
}

impl LayerBundle {
    /// The items: the layers not inside another of them (nor masks), bottom first.
    fn item_ids(&self) -> Vec<LayerId> {
        let ids: Vec<LayerId> = self.layers.iter().map(|l| l.id).collect();
        self.layers
            .iter()
            .filter(|l| l.kind != LayerKind::Mask && l.parent.is_none_or(|p| !ids.contains(&p)))
            .map(|l| l.id)
            .collect()
    }
}

/// How a bundle's pixels go on a canvas: as they were on one the same size; from another size,
/// what is painted of its layers (all of them together, so they stay in place on each other)
/// centred, scaled down to fit if it is larger.
#[derive(Clone, Copy, Debug)]
struct Fit {
    /// The canvas it goes on.
    width: u32,
    height: u32,
    /// What of the canvas they came from is placed, where its top left corner goes, and how much
    /// it is scaled. None: they go as they were.
    moved: Option<(IRect, (i32, i32), f32)>,
}

impl Fit {
    fn new(bundle: &LayerBundle, width: u32, height: u32) -> Fit {
        if (bundle.width, bundle.height) == (width, height) {
            return Fit { width, height, moved: None };
        }
        // a mask shows all of its layer but where it is painted dark: what is painted of the
        // layers is what places them
        let painted = bundle
            .layers
            .iter()
            .filter(|l| l.kind == LayerKind::Paint)
            .filter_map(|l| bundle.pixels.get(&l.id).map(|p| p.rect))
            .reduce(IRect::union)
            .unwrap_or_else(|| IRect::from_size(bundle.width, bundle.height));
        let (w, h) = (painted.width().max(1) as f32, painted.height().max(1) as f32);
        let scale = (width as f32 / w).min(height as f32 / h).min(1.0);
        let (sw, sh) = ((w * scale).round().max(1.0) as i32, (h * scale).round().max(1.0) as i32);
        let at = ((width as i32 - sw) / 2, (height as i32 - sh) / 2);
        Fit { width, height, moved: Some((painted, at, scale)) }
    }

    /// Whether the layers are put somewhere else than they were (text and plugins' items, laid out
    /// on the canvas, can't follow: such layers come as ordinary ones).
    fn moves(&self) -> bool {
        self.moved.is_some()
    }

    /// Where `part` goes, and its pixels there; a `mask` comes whole, white (showing) wherever
    /// nothing of it is placed. None if nothing of it lands on the canvas (a layer: it stays clear).
    fn place(&self, part: Option<&Part>, mask: bool) -> Option<(IRect, Vec<u8>)> {
        let full = IRect::from_size(self.width, self.height);
        let Some((from, at, scale)) = self.moved else {
            if !mask {
                let part = part?;
                return (part.rect.intersect(full) == part.rect && !part.rect.is_empty())
                    .then(|| (part.rect, part.rgba.clone()));
            }
            let mut whole = white(full);
            if let Some(part) = part {
                let r = part.rect.intersect(full);
                for y in r.y0..r.y1 {
                    for x in r.x0..r.x1 {
                        put(&mut whole, full, x, y, part.at(x, y));
                    }
                }
            }
            return Some((full, whole));
        };
        // the box `from` lands in, and of it what one part covers
        let to = |r: IRect| {
            IRect::new(
                at.0 + ((r.x0 - from.x0) as f32 * scale).floor() as i32,
                at.1 + ((r.y0 - from.y0) as f32 * scale).floor() as i32,
                at.0 + ((r.x1 - from.x0) as f32 * scale).ceil() as i32,
                at.1 + ((r.y1 - from.y0) as f32 * scale).ceil() as i32,
            )
            .intersect(full)
        };
        let placed = to(from);
        // each pixel there takes the nearest of the part's (scaling only ever shrinks)
        let source = |x: i32, y: i32| {
            let sx = from.x0 + (((x - at.0) as f32 + 0.5) / scale).floor() as i32;
            let sy = from.y0 + (((y - at.1) as f32 + 0.5) / scale).floor() as i32;
            (sx.clamp(from.x0, from.x1 - 1), sy.clamp(from.y0, from.y1 - 1))
        };
        if mask {
            let mut whole = white(full);
            for y in placed.y0..placed.y1 {
                for x in placed.x0..placed.x1 {
                    let (sx, sy) = source(x, y);
                    put(&mut whole, full, x, y, part.and_then(|p| p.at(sx, sy)));
                }
            }
            return Some((full, whole));
        }
        let part = part?;
        let rect = to(part.rect.intersect(from));
        if rect.is_empty() {
            return None;
        }
        let mut out = vec![0u8; rect.width() as usize * rect.height() as usize * 4];
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let (sx, sy) = source(x, y);
                put(&mut out, rect, x, y, part.at(sx, sy));
            }
        }
        Some((rect, out))
    }
}

/// `rect`'s pixels all white and opaque: a mask showing all of its layer.
fn white(rect: IRect) -> Vec<u8> {
    vec![255u8; rect.width() as usize * rect.height() as usize * 4]
}

/// Sets the pixel at canvas (`x`, `y`) of `out` (`rect`'s rows) to `px`, if there is one.
fn put(out: &mut [u8], rect: IRect, x: i32, y: i32, px: Option<[u8; 4]>) {
    let Some(px) = px else { return };
    let i = ((y - rect.y0) as usize * rect.width() as usize + (x - rect.x0) as usize) * 4;
    if let Some(to) = out.get_mut(i..i + 4) {
        to.copy_from_slice(&px);
    }
}

#[uniffi::export]
impl CanvasSession {
    /// Layer `id` lifted out to drop elsewhere, with what a drag of its row takes along (its mask,
    /// a group's layers; when it is current or picked with others picked, all of them together):
    /// their settings and pixels, apart from this canvas from now on. None if there is no such
    /// layer, or its pixels can't be read.
    pub fn lift_layers(&self, id: u64) -> Option<Arc<LayerBundle>> {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        st.flush();
        let id = LayerId(id);
        let together = !st.doc.picked().is_empty() && (st.doc.current() == id || st.doc.is_picked(id));
        let items = if together { st.doc.chosen_whole() } else { vec![id] };
        let layers = st.doc.items_layers(&items);
        if layers.is_empty() {
            return None;
        }
        let (width, height) = (st.doc.width(), st.doc.height());
        let with_pixels: Vec<LayerId> = layers.iter().filter(|l| l.kind != LayerKind::Group).map(|l| l.id).collect();
        let (Some(canvas), Some(gpu)) = (st.canvas.as_ref(), st.gpu.as_ref()) else { return None };
        // only what is painted of each is read back (a layer with nothing on it, nothing)
        let whole = IRect::from_size(width, height);
        let mut pixels = HashMap::new();
        for (id, painted) in with_pixels.iter().zip(canvas.painted_boxes_or_why(gpu, &with_pixels)) {
            let rect = match painted {
                Ok(Some(rect)) => rect.intersect(whole),
                Ok(None) => continue,
                Err(_) => whole,
            };
            if rect.is_empty() {
                continue;
            }
            let rgba = canvas.read_layer_part(gpu, *id, rect).ok()?;
            pixels.insert(*id, Part { rect, rgba });
        }
        Some(Arc::new(LayerBundle { width, height, layers, pixels }))
    }

    /// `bundle`'s layers dropped here at `place` as one undo step: copies with their settings
    /// (blend mode, opacity, masks, clipping, names, a group with what is in it) and pixels. From a
    /// canvas of another size, what is painted of them is centred, scaled down to fit if it is
    /// larger, all of them alike (text and plugins' layers then come as ordinary ones). They become
    /// what is chosen, the top one current; returns its id. None if there is no room for them.
    pub fn drop_layers(&self, bundle: Arc<LayerBundle>, place: LayerDrop) -> Option<u64> {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        if !st.doc.has_room(bundle.layer_count() as usize) {
            return None;
        }
        let fit = Fit::new(&bundle, st.doc.width(), st.doc.height());
        let mut layers = bundle.layers.clone();
        if fit.moves() {
            for l in &mut layers {
                l.text = None;
                l.plugin = None;
            }
        }
        let landing = match place {
            LayerDrop::AboveCurrent => Landing::AboveCurrent,
            LayerDrop::Top => Landing::Top,
            // nothing of theirs is here to stay by: at the top then
            LayerDrop::Row { place } => place.engine().map_or(Landing::Top, Landing::Row),
        };
        let before = st.doc.clone();
        let pairs = st.doc.insert_copies(&layers, landing)?;
        let parts: Vec<(LayerId, IRect, Vec<u8>)> = layers
            .iter()
            .zip(&pairs)
            .filter(|(l, _)| l.kind != LayerKind::Group)
            .filter_map(|(l, (old, copy))| {
                let (rect, rgba) = fit.place(bundle.pixels.get(old), l.kind == LayerKind::Mask)?;
                Some((*copy, rect, rgba))
            })
            .collect();
        if let Some(canvas) = st.canvas.as_mut() {
            canvas.name_next_step(if bundle.item_ids().len() > 1 { "insert layers" } else { "insert layer" });
            canvas.write_layer_parts_and_document(parts, before);
        }
        st.revision += 1;
        self.shared.frame(st);
        Some(st.doc.current().0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(rect: IRect, px: [u8; 4]) -> Part {
        Part { rect, rgba: px.repeat(rect.width() as usize * rect.height() as usize) }
    }

    fn bundle(width: u32, height: u32, parts: Vec<(Layer, Option<Part>)>) -> LayerBundle {
        let pixels = parts.iter().filter_map(|(l, p)| p.clone().map(|p| (l.id, p))).collect();
        LayerBundle { width, height, layers: parts.into_iter().map(|(l, _)| l).collect(), pixels }
    }

    fn layers(n: usize) -> Vec<Layer> {
        let mut d = procraft_engine::document::Document::new(10, 10);
        let mut ids = vec![d.current()];
        for _ in 1..n {
            ids.push(d.add_layer());
        }
        d.items_layers(&ids)
    }

    #[test]
    fn on_a_canvas_the_same_size_pixels_land_where_they_were() {
        let l = layers(1).remove(0);
        let b = bundle(100, 80, vec![(l, Some(part(IRect::new(10, 20, 30, 25), [9, 9, 9, 9])))]);
        let fit = Fit::new(&b, 100, 80);
        assert!(!fit.moves());
        let (rect, rgba) = fit.place(b.pixels.values().next(), false).unwrap();
        assert_eq!(rect, IRect::new(10, 20, 30, 25));
        assert_eq!(rgba.len(), 20 * 5 * 4);
        // clear: nothing to write
        assert!(fit.place(None, false).is_none());
        // a mask comes whole, white where it has nothing
        let (rect, rgba) = fit.place(None, true).unwrap();
        assert_eq!(rect, IRect::from_size(100, 80));
        assert!(rgba.iter().all(|&v| v == 255));
    }

    #[test]
    fn from_a_bigger_canvas_what_is_painted_is_centred_and_fitted_for_all_alike() {
        let [a, b, mask]: [Layer; 3] = layers(3).try_into().unwrap();
        let mask = Layer { kind: LayerKind::Mask, parent: Some(b.id), ..mask };
        // two layers painted in opposite corners of a 400 × 200 canvas: together 300 × 100
        let left = part(IRect::new(50, 50, 100, 100), [255, 0, 0, 255]);
        let right = part(IRect::new(300, 100, 350, 150), [0, 0, 255, 255]);
        let dark = part(IRect::new(0, 0, 400, 200), [0, 0, 0, 255]);
        let bundle = bundle(400, 200, vec![(a, Some(left)), (b, Some(right)), (mask, Some(dark))]);
        let fit = Fit::new(&bundle, 150, 150);
        assert!(fit.moves());
        // halved to 150 × 50, centred: rows 50 to 100
        let parts: Vec<&Part> = bundle.layers.iter().filter_map(|l| bundle.pixels.get(&l.id)).collect();
        let (r, _) = fit.place(Some(parts[0]), false).unwrap();
        assert_eq!(r, IRect::new(0, 50, 25, 75));
        let (r, _) = fit.place(Some(parts[1]), false).unwrap();
        assert_eq!(r, IRect::new(125, 75, 150, 100));
        // the mask: dark where its layers were placed, white around
        let (r, m) = fit.place(Some(parts[2]), true).unwrap();
        assert_eq!(r, IRect::from_size(150, 150));
        assert_eq!(&m[(10 * 150 + 10) * 4..][..4], &[255; 4]);
        assert_eq!(&m[(60 * 150 + 10) * 4..][..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn from_a_smaller_canvas_it_keeps_its_size_in_the_middle() {
        let l = layers(1).remove(0);
        let b = bundle(10, 10, vec![(l, Some(part(IRect::new(2, 2, 6, 4), [1, 2, 3, 4])))]);
        let fit = Fit::new(&b, 100, 100);
        let (r, rgba) = fit.place(b.pixels.values().next(), false).unwrap();
        assert_eq!(r, IRect::new(48, 49, 52, 51));
        assert!(rgba.chunks(4).all(|p| p == [1, 2, 3, 4]));
    }

    #[test]
    fn a_bundle_counts_its_items_and_names_the_top_one() {
        let mut d = procraft_engine::document::Document::new(10, 10);
        let one = d.current();
        let two = d.add_layer();
        d.rename(two, "Top");
        d.add_mask(two);
        let b = bundle(10, 10, d.items_layers(&[one, two]).into_iter().map(|l| (l, None)).collect());
        assert_eq!(b.items(), 2);
        assert_eq!(b.layer_count(), 3);
        assert_eq!(b.name(), "Top");
    }
}
