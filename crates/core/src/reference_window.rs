//! The Reference window as the artwork keeps it: the session holds what the artwork left and saves
//! it with the artwork, and each app shows it and says what changes. Apart from the document, so
//! that undoing never moves the window.

use std::io::Cursor;
use std::sync::Arc;

use procraft_engine::reference_window::{ReferenceKind, ReferenceWindow};

use crate::{CanvasSession, ReferenceView};

/// The window's imported picture, beside the artwork's layers.
pub(crate) const REFERENCE_PICTURE: &str = "reference.png";

/// The most pixels a window keeps its imported picture with on its longer side: as many as the
/// largest window shows it with on a dense screen. A picture costs a window at most 16 MB of
/// pixels so, however big the photo was (one 4000 square kept whole took 61 MB, and as much again
/// where it was drawn).
pub(crate) const PICTURE_MOST: u32 = 2048;

/// The most Reference windows an artwork has open at once: its own and those a plugin lets be opened
/// besides (the reference windows' hook).
pub(crate) const MOST_REFERENCE_WINDOWS: usize = 8;

/// The picture of window `index` besides the artwork's own (0 the first of them), beside its layers:
/// `reference-2.png` for the second window, and so on.
pub(crate) fn more_picture(index: usize) -> String {
    format!("reference-{}.png", index + 2)
}

/// A window opened besides the artwork's own, kept with it as its own is: where it is and what it
/// shows, and its imported picture, if it has one.
#[derive(Clone, Debug, Default)]
pub(crate) struct MoreReference {
    pub(crate) window: ReferenceWindow,
    pub(crate) picture: Option<Arc<Vec<u8>>>,
}

/// What the window shows. `Model`, the model itself, only on a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ReferenceShows {
    Canvas,
    Image,
    Face,
    Model,
}

/// The window as an app shows it, in points. Until it has been `sized` or `placed` the app chooses
/// (the numbers then mean nothing).
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ReferenceWindowState {
    pub shown: bool,
    pub shows: ReferenceShows,
    pub sized: bool,
    pub width: f32,
    pub height: f32,
    pub placed: bool,
    pub x: f32,
    pub y: f32,
    pub canvas_view: ReferenceView,
    pub image_view: ReferenceView,
}

/// A place for the window, in points.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ReferencePlace {
    pub x: f32,
    pub y: f32,
}

fn view_of([zoom, x, y]: [f32; 3]) -> ReferenceView {
    ReferenceView { zoom, x, y }
}

fn kept_view(v: ReferenceView) -> [f32; 3] {
    if [v.zoom, v.x, v.y].iter().all(|n| n.is_finite()) { [v.zoom, v.x, v.y] } else { [1.0, 0.0, 0.0] }
}

/// `w` as an app shows it, on a `model` or not: an artwork that isn't one shows its canvas where
/// a model would show itself, and a model, which has no face to show, shows its canvas there.
fn state_of(w: &ReferenceWindow, model: bool) -> ReferenceWindowState {
    let [width, height] = w.size.unwrap_or_default();
    let [x, y] = w.at.unwrap_or_default();
    ReferenceWindowState {
        shown: w.shown,
        shows: match (w.kind, model) {
            (ReferenceKind::Image, _) => ReferenceShows::Image,
            (ReferenceKind::Canvas | ReferenceKind::Face, true) if w.three_d => ReferenceShows::Model,
            (ReferenceKind::Face, false) => ReferenceShows::Face,
            (ReferenceKind::Canvas | ReferenceKind::Face, _) => ReferenceShows::Canvas,
        },
        sized: w.size.is_some(),
        width,
        height,
        placed: w.at.is_some(),
        x,
        y,
        canvas_view: view_of(w.canvas_view),
        image_view: view_of(w.image_view),
    }
}

/// `w` as `state` says on a `model` or not, what only a file said staying as it was: the model
/// shown is the canvas kind turned to it, and the canvas on a model the same turned back.
fn take_state(w: &mut ReferenceWindow, state: ReferenceWindowState, model: bool) {
    let pair = |on: bool, a: f32, b: f32| (on && a.is_finite() && b.is_finite()).then_some([a, b]);
    w.shown = state.shown;
    w.kind = match state.shows {
        ReferenceShows::Canvas | ReferenceShows::Model => ReferenceKind::Canvas,
        ReferenceShows::Image => ReferenceKind::Image,
        ReferenceShows::Face => ReferenceKind::Face,
    };
    if model && matches!(state.shows, ReferenceShows::Canvas | ReferenceShows::Model) {
        w.three_d = state.shows == ReferenceShows::Model;
    }
    w.size = pair(state.sized, state.width, state.height);
    w.at = pair(state.placed, state.x, state.y);
    w.canvas_view = kept_view(state.canvas_view);
    w.image_view = kept_view(state.image_view);
}

/// `png` as it shows: a package's picture is kept with the turn it is to be shown at, one of the
/// system's eight orientations (0 is upright). One that can't be read stays as it is.
pub(crate) fn upright(png: Vec<u8>, orientation: i64) -> Vec<u8> {
    if !(1..=7).contains(&orientation) {
        return png;
    }
    let Ok(picture) = image::load_from_memory(&png) else { return png };
    let turned = match orientation {
        1 => picture.rotate180(),
        2 => picture.rotate270(),
        3 => picture.rotate90(),
        4 => picture.fliph(),
        5 => picture.flipv(),
        6 => picture.fliph().rotate270(),
        _ => picture.fliph().rotate90(),
    };
    let mut out = Cursor::new(Vec::new());
    match turned.write_to(&mut out, image::ImageFormat::Png) {
        Ok(()) => out.into_inner(),
        Err(_) => png,
    }
}

/// A picture's size, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ReferencePictureSize {
    pub width: u32,
    pub height: u32,
}

/// The size a window keeps a picture `width` x `height` (pixels) at: as it is up to
/// [`PICTURE_MOST`] on its longer side, else made that big, its shape kept (a pixel at least each
/// way).
#[uniffi::export]
pub fn reference_picture_size(width: u32, height: u32) -> ReferencePictureSize {
    let long = width.max(height);
    if long <= PICTURE_MOST {
        return ReferencePictureSize { width, height };
    }
    let scale = f64::from(PICTURE_MOST) / f64::from(long);
    let side = |v: u32| ((f64::from(v) * scale).round() as u32).clamp(1, PICTURE_MOST);
    ReferencePictureSize { width: side(width), height: side(height) }
}

/// `png` as a window keeps it: one bigger than [`reference_picture_size`] says made that size (a
/// PNG again); one that can't be read, or is no bigger, as it is.
pub(crate) fn kept_picture(png: Vec<u8>) -> Vec<u8> {
    let size = image::ImageReader::new(Cursor::new(&png))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_dimensions().ok());
    let Some((width, height)) = size else { return png };
    let kept = reference_picture_size(width, height);
    if (kept.width, kept.height) == (width, height) {
        return png;
    }
    let Ok(picture) = image::load_from_memory(&png) else { return png };
    let whole = picture.into_rgba8();
    let smaller = image::imageops::resize(&whole, kept.width, kept.height, image::imageops::FilterType::Triangle);
    drop(whole);
    let mut out = Cursor::new(Vec::new());
    match smaller.write_to(&mut out, image::ImageFormat::Png) {
        Ok(()) => out.into_inner(),
        Err(_) => png,
    }
}

/// A window's size, in points: its width, and its picture's height between the bars.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ReferenceSize {
    pub width: f32,
    pub height: f32,
}

/// The size a window `width` wide, its picture `height` tall between the bars, shows at on a
/// screen `screen_width` x `screen_height` (points; not a number: no screen yet): 282 to 650 wide
/// and 230 to 650 tall with its bars, and no bigger than the screen while that leaves it that.
#[uniffi::export]
pub fn reference_window_size(width: f32, height: f32, screen_width: f32, screen_height: f32) -> ReferenceSize {
    let [width, height] =
        procraft_engine::reference_window::fitted_size([width, height], [screen_width, screen_height]);
    ReferenceSize { width, height }
}

/// The size a window shows at until it is sized, on a canvas `canvas_width` x `canvas_height`
/// pixels: 350 wide, and as tall with its bars as the canvas's shape makes it, so the canvas fills
/// it (within the window's least and most); its width and its picture's height between the bars,
/// points, as [`reference_window_size`] takes them.
#[uniffi::export]
pub fn reference_window_first_size(canvas_width: u32, canvas_height: u32) -> ReferenceSize {
    let [width, height] = procraft_engine::reference_window::first_size([canvas_width, canvas_height]);
    ReferenceSize { width, height }
}

/// Where a window `width` x `tall` at (`x`, `y`) goes in a screen `screen_width` x `screen_height`,
/// so that it can be reached: all of it shown (the top left of it, if it is bigger). An artwork
/// may have left it on a screen of another shape.
#[uniffi::export]
pub fn reference_window_fitted(
    x: f32,
    y: f32,
    width: f32,
    tall: f32,
    screen_width: f32,
    screen_height: f32,
) -> ReferencePlace {
    let inside = |at: f32, size: f32, room: f32| {
        let most = (room - size).max(0.0);
        if at.is_finite() { at.clamp(0.0, most) } else { 0.0 }
    };
    ReferencePlace { x: inside(x, width, screen_width), y: inside(y, tall, screen_height) }
}

#[uniffi::export]
impl CanvasSession {
    /// The Reference window as this artwork left it.
    pub fn reference_window(&self) -> ReferenceWindowState {
        let st = self.shared.lock();
        state_of(&st.reference_window, st.scene.is_some())
    }

    /// The window has changed: kept with the artwork, saved with it (a change being an edit, the
    /// autosave has it).
    pub fn set_reference_window(&self, state: ReferenceWindowState) {
        let mut st = self.shared.lock();
        let mut next = st.reference_window.clone();
        take_state(&mut next, state, st.scene.is_some());
        if next != st.reference_window {
            st.reference_window = next;
            st.revision += 1;
        }
    }

    /// The window's imported picture, a PNG; none if it has none.
    pub fn reference_picture(&self) -> Option<Vec<u8>> {
        self.shared.lock().reference_picture.as_deref().cloned()
    }

    /// `png` is now the window's imported picture, kept no bigger than [`reference_picture_size`]
    /// says.
    pub fn set_reference_picture(&self, png: Vec<u8>) {
        let png = kept_picture(png);
        let mut st = self.shared.lock();
        st.reference_picture = Some(Arc::new(png));
        st.revision += 1;
    }

    /// The window's imported picture goes.
    pub fn clear_reference_picture(&self) {
        let mut st = self.shared.lock();
        if st.reference_picture.take().is_some() {
            st.revision += 1;
        }
    }

    /// The whole picture for the Reference windows that show the canvas, as
    /// [`CanvasSession::canvas_thumbnail`] makes it, but read back from the GPU once while the
    /// artwork stays as it is: every window showing the canvas takes the same, so more windows
    /// cost no more reads. None before the canvas exists. Read once the frames have laid what
    /// they lay a share at a time (the stroke's end it is read after), which the read would
    /// otherwise lay at once, frames and touches waiting on it: not from the thread that draws
    /// (both apps read it on a thread of their own).
    pub fn reference_canvas(&self, max_side: u32) -> Option<crate::ThumbnailImage> {
        let _ = self.shared.await_frames_laid();
        let edits = self.edits();
        {
            let st = self.shared.lock();
            if let Some((at, side, picture)) = &st.reference_canvas
                && (*at, *side) == (edits, max_side)
            {
                return Some(picture.as_ref().clone());
            }
        }
        let picture = self.canvas_thumbnail(max_side)?;
        self.shared.lock().reference_canvas = Some((edits, max_side, Arc::new(picture.clone())));
        Some(picture)
    }

    /// How many Reference windows the artwork has: its own (shown or not), then those opened
    /// besides it, while a plugin lets more than one be open (the reference windows' hook).
    pub fn reference_window_count(&self) -> u32 {
        1 + self.shared.lock().more_references.len() as u32
    }

    /// Window `index` as the artwork left it: 0 its own ([`CanvasSession::reference_window`]),
    /// then those opened besides; none past them.
    pub fn reference_window_at(&self, index: u32) -> Option<ReferenceWindowState> {
        let st = self.shared.lock();
        let model = st.scene.is_some();
        match index.checked_sub(1) {
            None => Some(state_of(&st.reference_window, model)),
            Some(more) => st.more_references.get(more as usize).map(|m| state_of(&m.window, model)),
        }
    }

    /// Window `index` has changed (see [`CanvasSession::set_reference_window`]).
    pub fn set_reference_window_at(&self, index: u32, state: ReferenceWindowState) {
        let Some(more) = index.checked_sub(1) else { return self.set_reference_window(state) };
        let mut st = self.shared.lock();
        let model = st.scene.is_some();
        let Some(m) = st.more_references.get_mut(more as usize) else { return };
        let mut next = m.window.clone();
        take_state(&mut next, state, model);
        if next != m.window {
            m.window = next;
            st.revision += 1;
        }
    }

    /// Window `index`'s imported picture, a PNG; none if it has none (or there is no such window).
    pub fn reference_picture_at(&self, index: u32) -> Option<Vec<u8>> {
        let Some(more) = index.checked_sub(1) else { return self.reference_picture() };
        self.shared.lock().more_references.get(more as usize).and_then(|m| m.picture.as_deref().cloned())
    }

    /// `png` is now window `index`'s imported picture (see [`CanvasSession::set_reference_picture`]).
    pub fn set_reference_picture_at(&self, index: u32, png: Vec<u8>) {
        let Some(more) = index.checked_sub(1) else { return self.set_reference_picture(png) };
        let png = kept_picture(png);
        let mut st = self.shared.lock();
        if let Some(m) = st.more_references.get_mut(more as usize) {
            m.picture = Some(Arc::new(png));
            st.revision += 1;
        }
    }

    /// Window `index`'s imported picture goes.
    pub fn clear_reference_picture_at(&self, index: u32) {
        let Some(more) = index.checked_sub(1) else { return self.clear_reference_picture() };
        let mut st = self.shared.lock();
        if st.more_references.get_mut(more as usize).is_some_and(|m| m.picture.take().is_some()) {
            st.revision += 1;
        }
    }

    /// Another Reference window opened besides those the artwork has, as `state` says (shown): its
    /// index. None once the artwork has as many as it may ([`MOST_REFERENCE_WINDOWS`]); how many a
    /// plugin lets be open is the app's to keep to.
    pub fn add_reference_window(&self, state: ReferenceWindowState) -> Option<u32> {
        let mut st = self.shared.lock();
        if 1 + st.more_references.len() >= MOST_REFERENCE_WINDOWS {
            return None;
        }
        let model = st.scene.is_some();
        let mut window = ReferenceWindow::default();
        take_state(&mut window, ReferenceWindowState { shown: true, ..state }, model);
        st.more_references.push(MoreReference { window, picture: None });
        st.revision += 1;
        Some(st.more_references.len() as u32)
    }

    /// Window `index` besides the artwork's own closed, its picture with it: those after it each
    /// take the index before theirs. The artwork's own stays (hide it with its state's `shown`).
    pub fn remove_reference_window(&self, index: u32) {
        let Some(more) = index.checked_sub(1) else { return };
        let mut st = self.shared.lock();
        if (more as usize) < st.more_references.len() {
            st.more_references.remove(more as usize);
            st.revision += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ReferenceWindowState {
        ReferenceWindowState {
            shown: true,
            shows: ReferenceShows::Image,
            sized: true,
            width: 300.0,
            height: 200.0,
            placed: true,
            x: 40.0,
            y: 60.0,
            canvas_view: ReferenceView { zoom: 2.0, x: 5.0, y: 6.0 },
            image_view: ReferenceView { zoom: 3.0, x: 7.0, y: 8.0 },
        }
    }

    /// A folder of its own for test `name`, empty.
    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("procraft-reference-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn path(dir: &std::path::Path) -> String {
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn the_window_and_its_picture_come_back_with_the_artwork() {
        let dir = folder("store");
        let s = CanvasSession::new(64, 48);
        let before = s.edits();
        s.set_reference_window(state());
        s.set_reference_picture(b"a picture".to_vec());
        // a change the autosave sees
        assert_ne!(s.edits(), before);
        s.save(path(&dir)).unwrap();
        assert!(dir.join(REFERENCE_PICTURE).exists());
        let opened = CanvasSession::open(path(&dir)).unwrap();
        assert_eq!(opened.reference_window(), state());
        assert_eq!(opened.reference_picture().as_deref(), Some(&b"a picture"[..]));
        // the same again: nothing changes, nothing is counted
        let edits = opened.edits();
        opened.set_reference_window(state());
        assert_eq!(opened.edits(), edits);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_artwork_that_never_had_a_window_opens_with_none_as_older_ones_do() {
        let dir = folder("never");
        let s = CanvasSession::new(64, 48);
        s.save(path(&dir)).unwrap();
        let json = std::fs::read_to_string(dir.join("artwork.json")).unwrap();
        let kept: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(kept.get("reference").is_none(), "{json}");
        let opened = CanvasSession::open(path(&dir)).unwrap();
        assert!(!opened.reference_window().shown && !opened.reference_window().sized);
        assert_eq!(opened.reference_picture(), None);
        // one whose window can't be read opens as one that never had it
        let broken = json.replacen('{', "{\"reference\":{\"shown\":\"yes\"},", 1);
        std::fs::write(dir.join("artwork.json"), broken).unwrap();
        let opened = CanvasSession::open(path(&dir)).unwrap();
        assert!(!opened.reference_window().shown);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_opened_besides_come_back_with_the_artwork_each_with_its_picture() {
        let dir = folder("more");
        let s = CanvasSession::new(64, 48);
        assert_eq!((s.reference_window_count(), s.reference_window_at(1)), (1, None));
        assert_eq!(s.add_reference_window(ReferenceWindowState { shown: false, ..state() }), Some(1));
        assert_eq!(s.add_reference_window(ReferenceWindowState { x: 90.0, ..state() }), Some(2));
        // shown however asked for
        assert!(s.reference_window_at(1).is_some_and(|w| w.shown));
        s.set_reference_picture_at(2, b"second".to_vec());
        s.save(path(&dir)).unwrap();
        assert!(dir.join(more_picture(1)).exists() && !dir.join(more_picture(0)).exists());
        let opened = CanvasSession::open(path(&dir)).unwrap();
        assert_eq!(opened.reference_window_count(), 3);
        assert_eq!(opened.reference_window_at(2).map(|w| w.x), Some(90.0));
        assert_eq!(opened.reference_picture_at(2).as_deref(), Some(&b"second"[..]));
        assert_eq!((opened.reference_picture_at(1), opened.reference_window_at(3)), (None, None));
        // the artwork's own is as it was
        assert!(!opened.reference_window().shown);
        // the first of them closed: the next takes its index, its picture with it
        opened.remove_reference_window(1);
        assert_eq!(opened.reference_window_count(), 2);
        assert_eq!(opened.reference_picture_at(1).as_deref(), Some(&b"second"[..]));
        opened.save(path(&dir)).unwrap();
        assert!(dir.join(more_picture(0)).exists() && !dir.join(more_picture(1)).exists());
        // eight at once at most
        for _ in 0..10 {
            opened.add_reference_window(state());
        }
        assert_eq!(opened.reference_window_count() as usize, MOST_REFERENCE_WINDOWS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_artworks_keep_windows_of_their_own() {
        let (one, two) = (folder("one"), folder("two"));
        let a = CanvasSession::new(64, 48);
        a.set_reference_window(state());
        a.save(path(&one)).unwrap();
        let b = CanvasSession::new(64, 48);
        let other = ReferenceWindowState { shows: ReferenceShows::Canvas, x: 500.0, ..state() };
        b.set_reference_window(other);
        b.save(path(&two)).unwrap();
        assert_eq!(CanvasSession::open(path(&one)).unwrap().reference_window(), state());
        assert_eq!(CanvasSession::open(path(&two)).unwrap().reference_window(), other);
        let _ = std::fs::remove_dir_all(&one);
        let _ = std::fs::remove_dir_all(&two);
    }

    #[test]
    fn a_picture_taken_away_leaves_the_folder_with_the_next_save() {
        let dir = folder("cleared");
        let s = CanvasSession::new(64, 48);
        s.set_reference_picture(b"a picture".to_vec());
        s.save(path(&dir)).unwrap();
        s.clear_reference_picture();
        s.save(path(&dir)).unwrap();
        assert!(!dir.join(REFERENCE_PICTURE).exists());
        assert_eq!(CanvasSession::open(path(&dir)).unwrap().reference_picture(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_package_s_window_and_picture_come_in_upright() {
        use procraft_engine::document::Document;
        let mut png = Cursor::new(Vec::new());
        let two = image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        image::DynamicImage::ImageRgba8(two).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let window = ReferenceWindow {
            shown: true,
            kind: ReferenceKind::Image,
            size: Some([300.0, 200.0]),
            at: Some([40.0, 60.0]),
            ..ReferenceWindow::default()
        };
        let doc = Document::new(64, 48);
        let bytes = procraft_engine::native::write(&procraft_engine::native::Export {
            document: &doc,
            pixels: &std::collections::HashMap::new(),
            composite: &[255; 64 * 48 * 4],
            thumbnail_png: &[],
            name: "Package",
            author: "",
            strokes: 0,
            tracked_seconds: 0.0,
            seed: 3,
            time_lapse: None,
            reference: Some(&window),
            reference_picture: Some(png.get_ref()),
            turns: 0,
        });
        let package = std::env::temp_dir().join(format!("procraft-reference-package-{}.procraft", std::process::id()));
        std::fs::write(&package, &bytes).unwrap();
        let s = CanvasSession::open_package(package.to_string_lossy().into_owned()).unwrap();
        assert_eq!(
            s.reference_window(),
            ReferenceWindowState {
                shows: ReferenceShows::Image,
                canvas_view: ReferenceView::default(),
                image_view: ReferenceView::default(),
                ..state()
            }
        );
        let shown = image::load_from_memory(&s.reference_picture().unwrap()).unwrap();
        assert_eq!((shown.width(), shown.height()), (2, 1));
        let _ = std::fs::remove_file(&package);
    }

    #[test]
    fn a_state_comes_back_as_it_was_set_and_what_a_file_said_stays() {
        let mut window = ReferenceWindow { corner: 3, docked: true, three_d: true, ..ReferenceWindow::default() };
        take_state(&mut window, state(), false);
        assert_eq!(state_of(&window, false), state());
        assert_eq!((window.corner, window.docked, window.three_d), (3, true, true));
        // the canvas chosen on an artwork that isn't a model leaves a file's 3D as it was
        take_state(&mut window, ReferenceWindowState { shows: ReferenceShows::Canvas, ..state() }, false);
        assert!(window.three_d);
    }

    #[test]
    fn the_model_shows_only_on_a_model() {
        let shows = |kind, three_d, model| {
            let w = ReferenceWindow { kind, three_d, ..ReferenceWindow::default() };
            state_of(&w, model).shows
        };
        use ReferenceKind as K;
        use ReferenceShows as S;
        assert_eq!(shows(K::Canvas, true, true), S::Model);
        assert_eq!(shows(K::Canvas, false, true), S::Canvas);
        // an artwork that isn't a model, left on 3D, shows its canvas
        assert_eq!(shows(K::Canvas, true, false), S::Canvas);
        assert_eq!(shows(K::Image, true, true), S::Image);
        // a model has no face to show
        assert_eq!(shows(K::Face, false, true), S::Canvas);
        assert_eq!(shows(K::Face, true, true), S::Model);
        assert_eq!(shows(K::Face, true, false), S::Face);
        // the model chosen is the canvas kind turned to it; the image keeps that for the canvas
        let mut w = ReferenceWindow::default();
        take_state(&mut w, ReferenceWindowState { shows: S::Model, ..state() }, true);
        assert_eq!((w.kind, w.three_d), (K::Canvas, true));
        take_state(&mut w, ReferenceWindowState { shows: S::Image, ..state() }, true);
        assert_eq!((w.kind, w.three_d), (K::Image, true));
        take_state(&mut w, ReferenceWindowState { shows: S::Canvas, ..state() }, true);
        assert_eq!((w.kind, w.three_d), (K::Canvas, false));
        // one sent on an artwork that isn't a model is its canvas
        take_state(&mut w, ReferenceWindowState { shows: S::Model, ..state() }, false);
        assert_eq!((w.kind, w.three_d, state_of(&w, false).shows), (K::Canvas, false, S::Canvas));
    }

    #[test]
    fn a_package_keeps_the_model_shown_as_its_3d_flag() {
        use procraft_engine::document::Document;
        let window = ReferenceWindow { shown: true, three_d: true, ..ReferenceWindow::default() };
        let doc = Document::new(64, 48);
        let bytes = procraft_engine::native::write(&procraft_engine::native::Export {
            document: &doc,
            pixels: &std::collections::HashMap::new(),
            composite: &[255; 64 * 48 * 4],
            thumbnail_png: &[],
            name: "Package",
            author: "",
            strokes: 0,
            tracked_seconds: 0.0,
            seed: 3,
            time_lapse: None,
            reference: Some(&window),
            reference_picture: None,
            turns: 0,
        });
        let package = std::env::temp_dir().join(format!("procraft-reference-3d-{}.procraft", std::process::id()));
        std::fs::write(&package, &bytes).unwrap();
        // a flat artwork shows its canvas, and keeps the flag to write back
        let s = CanvasSession::open_package(package.to_string_lossy().into_owned()).unwrap();
        assert_eq!(s.reference_window().shows, ReferenceShows::Canvas);
        assert!(s.shared.lock().reference_window.three_d);
        let _ = std::fs::remove_file(&package);
    }

    #[test]
    fn numbers_that_mean_nothing_are_not_kept() {
        let mut window = ReferenceWindow::default();
        let unset = ReferenceWindowState { sized: false, placed: false, ..state() };
        take_state(&mut window, unset, false);
        assert_eq!((window.size, window.at), (None, None));
        let broken = ReferenceWindowState {
            width: f32::NAN,
            x: f32::INFINITY,
            image_view: ReferenceView { zoom: f32::NAN, x: 0.0, y: 0.0 },
            ..state()
        };
        take_state(&mut window, broken, false);
        assert_eq!((window.size, window.at), (None, None));
        assert_eq!(window.image_view, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn a_window_left_off_a_smaller_screen_comes_back_on_it() {
        let place = |x, y| reference_window_fitted(x, y, 350.0, 532.0, 1000.0, 600.0);
        assert_eq!(place(100.0, 20.0), ReferencePlace { x: 100.0, y: 20.0 });
        assert_eq!(place(900.0, 500.0), ReferencePlace { x: 650.0, y: 68.0 });
        assert_eq!(place(-40.0, -9.0), ReferencePlace { x: 0.0, y: 0.0 });
        assert_eq!(place(f32::NAN, f32::INFINITY), ReferencePlace { x: 0.0, y: 0.0 });
        let big = reference_window_fitted(50.0, 50.0, 800.0, 900.0, 600.0, 400.0);
        assert_eq!(big, ReferencePlace { x: 0.0, y: 0.0 });
    }

    /// A picture `width` x `height` as a PNG: a gradient, quick to make and to pack.
    fn picture(width: u32, height: u32) -> Vec<u8> {
        let pixels = image::RgbImage::from_fn(width, height, |x, y| image::Rgb([x as u8, y as u8, (x ^ y) as u8]));
        let mut png = Cursor::new(Vec::new());
        pixels.write_to(&mut png, image::ImageFormat::Png).unwrap();
        png.into_inner()
    }

    #[test]
    fn a_big_picture_is_kept_no_bigger_than_the_largest_window_shows_it() {
        // a 4000 square, a 48 and a 200 megapixel photo, one at the most, one smaller, a strip
        let cases = [
            ((4000, 4000), (2048, 2048)),
            ((8000, 6000), (2048, 1536)),
            ((16320, 12240), (2048, 1536)),
            ((3000, 5000), (1229, 2048)),
            ((2048, 1000), (2048, 1000)),
            ((1000, 500), (1000, 500)),
            ((1, 10000), (1, 2048)),
            ((0, 0), (0, 0)),
        ];
        for ((w, h), (kw, kh)) in cases {
            assert_eq!(reference_picture_size(w, h), ReferencePictureSize { width: kw, height: kh }, "{w} x {h}");
        }
        // a 4000 square's pixels as a window holds them: 16 MiB, where kept whole (all the app
        // made smaller was a picture over 4096) they took 61 MiB, and as much again drawn
        let kept = reference_picture_size(4000, 4000);
        let bytes = |w: u32, h: u32| f64::from(w) * f64::from(h) * 4.0 / f64::from(1 << 20);
        let (now, whole) = (bytes(kept.width, kept.height), bytes(4000, 4000));
        println!(
            "a 4000 x 4000 picture is held at {} x {}: {now:.1} MiB (whole {whole:.1} MiB)",
            kept.width, kept.height
        );
        assert!(now <= 16.0, "{now} MiB");
    }

    #[test]
    fn a_big_picture_given_to_a_window_is_kept_smaller_and_a_small_one_as_it_was() {
        let s = CanvasSession::new(64, 48);
        let big = picture(4000, 3000);
        s.set_reference_picture(big.clone());
        let held = s.reference_picture().unwrap();
        let kept = image::load_from_memory(&held).unwrap();
        assert_eq!((kept.width(), kept.height()), (2048, 1536));
        println!("a 4000 x 3000 PNG of {} KB is kept as one of {} KB", big.len() >> 10, held.len() >> 10);
        assert!(held.len() < big.len() / 2, "{} {}", held.len(), big.len());
        // the windows opened besides it likewise
        assert_eq!(s.add_reference_window(state()), Some(1));
        s.set_reference_picture_at(1, picture(3000, 5000));
        let kept = image::load_from_memory(&s.reference_picture_at(1).unwrap()).unwrap();
        assert_eq!((kept.width(), kept.height()), (1229, 2048));
        // one no bigger, and one that can't be read, are kept byte for byte
        let small = picture(300, 200);
        s.set_reference_picture(small.clone());
        assert_eq!(s.reference_picture(), Some(small));
        s.set_reference_picture_at(1, b"not a picture".to_vec());
        assert_eq!(s.reference_picture_at(1).as_deref(), Some(&b"not a picture"[..]));
    }

    #[test]
    fn a_picture_is_turned_upright_and_a_bad_one_left_alone() {
        // two pixels side by side, red then blue, kept turned a quarter clockwise to show upright
        let mut png = Cursor::new(Vec::new());
        let two = image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        image::DynamicImage::ImageRgba8(two).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        assert_eq!(upright(png.clone(), 0), png);
        assert_eq!(upright(png.clone(), 9), png);
        let turned = image::load_from_memory(&upright(png, 3)).unwrap().to_rgba8();
        assert_eq!(turned.dimensions(), (1, 2));
        assert_eq!(turned.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(upright(b"not a picture".to_vec(), 3), b"not a picture");
    }
}
