//! Insert a sprite sheet: a sheet picture from anywhere read into clean, lined-up frames (the
//! engine's `sprite_import` does the reading) and added as a named animation standing on the current
//! frame's feet line, the canvas grown when the frames don't fit
//! (docs/pixel-art/05-sprite-import-design.md).

use std::sync::Arc;

use parking_lot::Mutex;
use procraft_engine::animation::{self, Animation};
use procraft_engine::geom::IRect;
use procraft_engine::sprite_import::{self, Options, Sheet};

use crate::{CanvasSession, ThumbnailImage};

/// The longest side of the sheet picture the import sheet shows.
const SHOWN_SIDE: u32 = 1024;
/// Room left round a sprite the canvas grows to fit.
const MARGIN: u32 = 4;

/// A frame's place in the sheet picture, in the picture's own pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SheetPlace {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// What reading a sheet found, for the import sheet to show.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SpriteSheetLook {
    /// The sheet picture, made smaller to show (its places are in `picture_width` × `picture_height`).
    pub shown: ThumbnailImage,
    pub picture_width: u32,
    pub picture_height: u32,
    /// Each frame's place, in reading order.
    pub places: Vec<SheetPlace>,
    /// The cleaned frames, all one size, to play.
    pub frames: Vec<ThumbnailImage>,
    /// The frames' speed, as the artwork will play them.
    pub fps: u32,
    /// The background colour taken away (0xRRGGBB), none when the picture had its own transparency.
    pub background: Option<u32>,
    /// Picture pixels to one art pixel (1 for real pixels).
    pub pixel_size: f32,
    /// The canvas's size after the import: bigger than now when the sprite needs it.
    pub canvas_width: u32,
    pub canvas_height: u32,
    /// The canvas will grow to fit the sprite.
    pub grows: bool,
    /// The sprite won't fit even grown: a pixel art canvas can't be more than 1024 a side.
    pub too_big: bool,
}

/// What an import did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SpriteSheetInserted {
    pub frames: u32,
    /// The canvas was made bigger to fit the sprite.
    pub grown: bool,
}

/// A sheet picture being imported: read again whenever its grid or background changes, inserted
/// as it was last read.
#[derive(uniffi::Object)]
pub struct SpriteSheetImport {
    picture: ThumbnailImage,
    name: String,
    read: Mutex<Option<Sheet>>,
}

#[uniffi::export]
impl SpriteSheetImport {
    /// A sheet picture (straight RGBA) and the name of the file it came from.
    #[uniffi::constructor]
    pub fn new(picture: ThumbnailImage, file_name: String) -> Arc<SpriteSheetImport> {
        Arc::new(SpriteSheetImport { picture, name: sprite_import::name_from_file(&file_name), read: Mutex::new(None) })
    }

    /// The animation's name, from the file's.
    pub fn suggested_name(&self) -> String {
        self.name.clone()
    }

    /// Reads the sheet for `session`'s artwork (pixel art or not): cut into `across × down` equal
    /// cells when both are set (else by the gaps between sprites), with `background` (0xRRGGBB) as
    /// its background when set (else the colours round its edge). None when nothing was found.
    pub fn look(
        &self,
        session: Arc<CanvasSession>,
        across: u32,
        down: u32,
        background: Option<u32>,
    ) -> Option<SpriteSheetLook> {
        let (pixel_art, fps, canvas) = {
            let st = session.shared.lock();
            let fps = st.doc.animation.as_ref().map_or(Animation::starting(false, st.doc.pixel_art).fps, |a| a.fps);
            (st.doc.pixel_art, fps, (st.doc.width(), st.doc.height()))
        };
        let options = Options {
            pixel_art,
            grid: (across > 0 && down > 0).then_some((across, down)),
            background: background.map(|c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]),
        };
        let p = &self.picture;
        let sheet = sprite_import::read(&p.rgba, p.width, p.height, &options)?;
        let placed = placement(&session, (sheet.width, sheet.height));
        let (canvas_width, canvas_height) = placed.as_ref().map_or(canvas, |p| p.size);
        let look = SpriteSheetLook {
            shown: shown(p),
            picture_width: p.width,
            picture_height: p.height,
            places: sheet
                .places
                .iter()
                .map(|q| SheetPlace { x: q.x, y: q.y, width: q.width, height: q.height })
                .collect(),
            frames: sheet
                .frames
                .iter()
                .map(|rgba| ThumbnailImage { width: sheet.width, height: sheet.height, rgba: rgba.clone() })
                .collect(),
            fps,
            background: sheet.background.map(|[r, g, b]| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)),
            pixel_size: sheet.pixel_size,
            canvas_width,
            canvas_height,
            grows: placed.as_ref().is_some_and(|p| p.grow_to.is_some()),
            too_big: placed.is_none(),
        };
        *self.read.lock() = Some(sheet);
        Some(look)
    }

    /// Adds the sheet as last read (read as it comes if not yet) to `session`'s artwork, leaving out
    /// the frames numbered in `skipped` (from 0): its frames as an animation called `name`, after
    /// the one holding the current frame, standing on the current frame's feet line (the bottom of
    /// what's drawn on it, centred on that; the canvas's bottom and middle on an empty one). The
    /// canvas grows first where the frames would reach past it. One undo step takes the frames
    /// away, and a second the growing. None when nothing could be added.
    pub fn insert(&self, session: Arc<CanvasSession>, name: String, skipped: Vec<u32>) -> Option<SpriteSheetInserted> {
        if self.read.lock().is_none() {
            self.look(session.clone(), 0, 0, None)?;
        }
        let sheet = self.read.lock().clone()?;
        let frames: Vec<&Vec<u8>> = sheet
            .frames
            .iter()
            .enumerate()
            .filter(|(k, _)| u32::try_from(*k).is_ok_and(|k| !skipped.contains(&k)))
            .map(|(_, f)| f)
            .collect();
        let count = u32::try_from(frames.len()).ok().filter(|&n| n > 0)?;
        let placed = placement(&session, (sheet.width, sheet.height))?;
        if let Some(to) = placed.grow_to {
            let done = session.crop_canvas(
                to.x0 as f32,
                to.y0 as f32,
                to.width() as f32,
                to.height() as f32,
                0.0,
                false,
                to.width(),
                to.height(),
            );
            if !done {
                return None;
            }
        }
        let pixels: Vec<u8> = frames.into_iter().flatten().copied().collect();
        let name = name.trim();
        let name: String = if name.is_empty() { self.name.clone() } else { name.to_owned() };
        let name: String = name.chars().take(procraft_engine::animation::MOST_TAG_NAME).collect();
        session.add_animation_frames("import sprite sheet", placed.area, &pixels, count, &name, &name)?;
        Some(SpriteSheetInserted { frames: count, grown: placed.grow_to.is_some() })
    }
}

/// Where a sheet's frames go.
#[derive(Debug, PartialEq, Eq)]
struct Placement {
    /// The canvas to grow to, in the canvas's present pixels; none when the frames fit.
    grow_to: Option<IRect>,
    /// The frames' area, in the canvas's pixels after any growing.
    area: IRect,
    /// The canvas's size after any growing.
    size: (u32, u32),
}

/// Where frames `width × height` (their feet at their bottom row's middle) go in `session`'s
/// artwork: standing on the current frame's feet line, the canvas grown where they'd reach past
/// it. None when a pixel art canvas would have to grow past its largest size.
fn placement(session: &CanvasSession, (width, height): (u32, u32)) -> Option<Placement> {
    let mut guard = session.shared.lock();
    let st = &mut *guard;
    let (w, h) = (i32::try_from(st.doc.width()).ok()?, i32::try_from(st.doc.height()).ok()?);
    let frame = animation::current_frame(&st.doc)
        .and_then(|i| animation::frames(&st.doc).get(i).copied())
        .unwrap_or_else(|| st.doc.current());
    let drawn = st.painted_box(frame).filter(|b| !b.is_empty());
    let pixel_art = st.doc.pixel_art;
    drop(guard);
    let (cx, bottom) = drawn.map_or((w / 2, h - MARGIN as i32), |b| ((b.x0 + b.x1) / 2, b.y1));
    feet_placement((w, h), (cx, bottom), (width, height), pixel_art)
}

/// [`placement`] worked out: a canvas `w × h`, the feet line's middle at `(cx, bottom)`.
fn feet_placement(
    (w, h): (i32, i32),
    (cx, bottom): (i32, i32),
    (width, height): (u32, u32),
    pixel_art: bool,
) -> Option<Placement> {
    let (fw, fh) = (i32::try_from(width).ok()?, i32::try_from(height).ok()?);
    let area = IRect::new(cx - fw / 2, bottom - fh, cx - fw / 2 + fw, bottom);
    let m = MARGIN as i32;
    let to = IRect::new(
        if area.x0 < 0 { area.x0 - m } else { 0 },
        if area.y0 < 0 { area.y0 - m } else { 0 },
        if area.x1 > w { area.x1 + m } else { w },
        if area.y1 > h { area.y1 + m } else { h },
    );
    let size = (to.width(), to.height());
    let most = procraft_engine::pixel::MOST_EDGE;
    if pixel_art && (size.0 > most || size.1 > most) {
        return None;
    }
    let grows = to != IRect::new(0, 0, w, h);
    Some(Placement {
        grow_to: grows.then_some(to),
        area: IRect::new(area.x0 - to.x0, area.y0 - to.y0, area.x1 - to.x0, area.y1 - to.y0),
        size,
    })
}

/// The picture made at most [`SHOWN_SIDE`] on its longest side, each shown pixel the picture's
/// pixel under its middle.
fn shown(p: &ThumbnailImage) -> ThumbnailImage {
    let longest = p.width.max(p.height).max(1);
    if longest <= SHOWN_SIDE {
        return p.clone();
    }
    let scale = f64::from(longest) / f64::from(SHOWN_SIDE);
    let (w, h) =
        (((f64::from(p.width) / scale).round() as u32).max(1), ((f64::from(p.height) / scale).round() as u32).max(1));
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        let sy = (((f64::from(y) + 0.5) * scale) as u32).min(p.height - 1);
        for x in 0..w {
            let sx = (((f64::from(x) + 0.5) * scale) as u32).min(p.width - 1);
            let i = (sy as usize * p.width as usize + sx as usize) * 4;
            rgba.extend_from_slice(p.rgba.get(i..i + 4).unwrap_or(&[0; 4]));
        }
    }
    ThumbnailImage { width: w, height: h, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procraft_gpu::Gpu;

    fn session(width: u32, height: u32, pixel_art: bool) -> Option<Arc<CanvasSession>> {
        let gpu = Gpu::headless().map_err(|e| eprintln!("skipped: {e}")).ok()?;
        let s = CanvasSession::new(width, height);
        {
            let mut st = s.shared.lock();
            st.doc.pixel_art = pixel_art;
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        Some(s)
    }

    /// A sheet of `count` 6 × 8 sprites, each blown up `scale` times on grey, apart by a gap.
    fn sheet(count: u32, scale: u32) -> ThumbnailImage {
        let (cell_w, cell_h) = (10 * scale, 12 * scale);
        let (w, h) = (count * cell_w, cell_h);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let (ax, ay) = ((x % cell_w) / scale, y / scale);
                let inside = (2..8).contains(&ax) && (2..10).contains(&ay);
                let edge = ax == 2 || ax == 7 || ay == 2 || ay == 9;
                let c = match (inside, edge) {
                    (true, true) => [20, 10, 30, 255],
                    (true, false) => [220, 180, 40, 255],
                    _ => [90, 90, 90, 255],
                };
                rgba.extend_from_slice(&c);
            }
        }
        ThumbnailImage { width: w, height: h, rgba }
    }

    #[test]
    fn a_sheet_comes_in_as_a_named_animation_after_the_current_one() {
        let Some(s) = session(32, 32, true) else { return };
        let import = SpriteSheetImport::new(sheet(4, 6), "/x/hero_walk.png".to_owned());
        assert_eq!(import.suggested_name(), "hero walk");
        let look = import.look(s.clone(), 0, 0, None).expect("a look");
        assert_eq!(look.places.len(), 4);
        assert_eq!((look.frames.first().map(|f| (f.width, f.height))), Some((6, 8)));
        assert_eq!((look.canvas_width, look.canvas_height), (32, 32));
        assert_eq!(look.fps, 10);
        assert_eq!(look.background, Some(0x5A5A5A));
        let done = import.insert(s.clone(), " walk ".to_owned(), Vec::new()).expect("inserted");
        assert_eq!(done, SpriteSheetInserted { frames: 4, grown: false });
        let names = s.animation_names();
        assert!(names.iter().any(|a| a.name == "walk" && a.count == 4), "{names:?}");
        // one undo takes them away
        assert!(s.undo().is_some());
        assert!(s.animation_names().iter().all(|a| a.name != "walk"));
    }

    #[test]
    fn a_sprite_bigger_than_the_canvas_grows_it() {
        let Some(s) = session(4, 4, true) else { return };
        let import = SpriteSheetImport::new(sheet(2, 5), "16418.jpg".to_owned());
        assert_eq!(import.suggested_name(), "imported");
        let look = import.look(s.clone(), 0, 0, None).expect("a look");
        assert!(look.grows && !look.too_big);
        let done = import.insert(s.clone(), String::new(), vec![0]).expect("inserted");
        assert_eq!(done.frames, 1, "the first frame left out");
        assert!(done.grown);
        let st = s.shared.lock();
        assert_eq!((st.doc.width(), st.doc.height()), (look.canvas_width, look.canvas_height));
        assert!(st.doc.width() >= 6 && st.doc.height() >= 8);
    }

    #[test]
    fn frames_stand_on_the_feet_line_and_grow_the_canvas_only_where_they_reach_past() {
        // fits: centred on the feet line's middle, standing on it
        let p = feet_placement((64, 64), (32, 60), (20, 10), true).expect("placed");
        assert_eq!(p, Placement { grow_to: None, area: IRect::new(22, 50, 42, 60), size: (64, 64) });
        // a taller one on the same line keeps its feet there, the canvas grown upwards only
        let p = feet_placement((64, 64), (32, 60), (20, 70), true).expect("placed");
        assert_eq!(p.grow_to, Some(IRect::new(0, -14, 64, 64)));
        assert_eq!(p.area, IRect::new(22, 4, 42, 74));
        assert_eq!(p.size, (64, 78));
        // past a pixel art canvas's largest size: refused; another canvas may grow
        assert!(feet_placement((1000, 64), (500, 60), (1100, 10), true).is_none());
        assert!(feet_placement((1000, 64), (500, 60), (1100, 10), false).is_some());
    }
}
