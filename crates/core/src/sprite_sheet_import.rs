//! Insert a sprite sheet: a sheet picture from anywhere read into clean, lined-up frames (the
//! engine's `sprite_import` does the reading) and added as a named animation, the canvas grown when
//! the sprite is bigger than it (docs/pixel-art/05-sprite-import-design.md).

use std::sync::Arc;

use parking_lot::Mutex;
use procraft_engine::animation::Animation;
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
        let (canvas_width, canvas_height) = grown(canvas, (sheet.width, sheet.height));
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
            grows: (canvas_width, canvas_height) != canvas,
        };
        *self.read.lock() = Some(sheet);
        Some(look)
    }

    /// Adds the sheet as last read (read as it comes if not yet) to `session`'s artwork: its frames
    /// as an animation called `name`, after the one holding the current frame, centred on the
    /// canvas, which grows first if the sprite is bigger. One undo step takes the frames away, and
    /// a second the growing. None when nothing could be added.
    pub fn insert(&self, session: Arc<CanvasSession>, name: String) -> Option<SpriteSheetInserted> {
        if self.read.lock().is_none() {
            self.look(session.clone(), 0, 0, None)?;
        }
        let sheet = self.read.lock().clone()?;
        let count = u32::try_from(sheet.frames.len()).ok().filter(|&n| n > 0)?;
        let now = {
            let st = session.shared.lock();
            (st.doc.width(), st.doc.height())
        };
        let (width, height) = grown(now, (sheet.width, sheet.height));
        let grow = (width, height) != now;
        if grow {
            let (x, y) = ((width - now.0) / 2, (height - now.1) / 2);
            let done =
                session.crop_canvas(-(x as f32), -(y as f32), width as f32, height as f32, 0.0, false, width, height);
            if !done {
                return None;
            }
        }
        let (x, y) = ((width - sheet.width) / 2, (height - sheet.height) / 2);
        let area = IRect::new(
            i32::try_from(x).ok()?,
            i32::try_from(y).ok()?,
            i32::try_from(x + sheet.width).ok()?,
            i32::try_from(y + sheet.height).ok()?,
        );
        let pixels: Vec<u8> = sheet.frames.concat();
        let name = name.trim();
        let name: String = if name.is_empty() { self.name.clone() } else { name.to_owned() };
        let name: String = name.chars().take(procraft_engine::animation::MOST_TAG_NAME).collect();
        session.add_animation_frames("import sprite sheet", area, &pixels, count, &name, &name)?;
        Some(SpriteSheetInserted { frames: count, grown: grow })
    }
}

/// The canvas's size to hold a sprite: as it is when the sprite fits, else just bigger than the
/// sprite each way that's short, by the same amount on both sides.
fn grown((width, height): (u32, u32), (sprite_width, sprite_height): (u32, u32)) -> (u32, u32) {
    if sprite_width <= width && sprite_height <= height {
        return (width, height);
    }
    let side = |now: u32, need: u32| {
        if need <= now {
            return now;
        }
        let more = need + MARGIN - now;
        now + more + more % 2
    };
    (side(width, sprite_width), side(height, sprite_height))
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
        let done = import.insert(s.clone(), " walk ".to_owned()).expect("inserted");
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
        assert_eq!((look.canvas_width, look.canvas_height), (10, 12));
        let done = import.insert(s.clone(), String::new()).expect("inserted");
        assert!(done.grown);
        let st = s.shared.lock();
        assert_eq!((st.doc.width(), st.doc.height()), (10, 12));
    }

    #[test]
    fn the_canvas_grows_evenly() {
        assert_eq!(grown((64, 64), (50, 35)), (64, 64));
        assert_eq!(grown((32, 32), (50, 35)), (54, 40));
        assert_eq!(grown((32, 33), (31, 35)), (32, 39));
    }
}
