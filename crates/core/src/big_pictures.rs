//! Our shapes and grains as big pictures, for a stamp or a grain's tile bigger than brushes hold
//! them ([`TEXTURE_SIZE`]). Each has one size of its own: the stock image's it stands in for (the
//! power of two nearest it, up to [`SHARPEST`]), or [`SHARPEST`] for one of ours. Made once, when
//! a brush that reads them is first chosen (or a brush that names them is imported), on a thread of
//! their own, and kept in files beside the brush library under a key of the code that draws them
//! ([`KEY`]: a build that draws them differently makes them again); none is made for brushes not
//! chosen, so a first start does no more than the reference's loading of its chosen brush's images.
//! A brush chosen reads its pictures in, and a stamp past [`TEXTURE_SIZE`] reads its picture
//! magnified or through its smaller levels, whatever its size, the GPU holding the last few.
//! Nothing is drawn while a stroke waits: one not made yet is made before any other, and a stroke
//! meanwhile reads the picture brushes hold.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use procraft_engine::brush::{Brush, GrainMode, Pixels};
use procraft_engine::textures::{self, SHARPEST, TEXTURE_SIZE};

/// How much a stamp or tile may be magnified before it reads the big picture: at most this many
/// canvas pixels to each texel of the one brushes hold (a quarter more is hardly seen).
const MAGNIFIED_MOST: f32 = 1.25;

/// How many bytes of big pictures are held in memory at once (four at 2048 a side with their
/// levels, a brush's and its second's shapes and grains; or more smaller ones), beside those the
/// GPU holds.
pub(crate) const HELD_BYTES: usize = 24 << 20;

/// What drawing one `side` pixels across holds for a while beyond the picture itself, bytes a
/// pixel (floats, their blurs and their order): a system short of that makes a smaller one.
const WORK_PER_PIXEL: u64 = 32;

/// How many threads a picture made ahead (not wanted by a brush chosen) is drawn on: the other
/// cores are left to painting meanwhile (a first start's). One wanted is drawn on all.
const AHEAD_THREADS: usize = 2;

/// The start of the name of the folder beside the library the pictures are kept in, the rest
/// [`KEY`] (another way of keeping them would be told apart by another start).
const FOLDER: &str = "big-pictures-";

/// What tells the code that draws the pictures from another build's (see `build.rs`).
pub(crate) const KEY: &str = env!("PROCRAFT_DRAWING_KEY");

/// One of our pictures big: what tells it apart, its side, and how it is drawn.
#[derive(Clone)]
pub(crate) struct Big {
    pub(crate) id: u64,
    pub(crate) side: u32,
    draw: Draw,
}

#[derive(Clone)]
enum Draw {
    Shape(u32),
    Grain(u32),
    Source(Pixels),
}

impl Big {
    /// Drawn at its side, or (the system short of memory for that) the largest that fits, down to
    /// twice [`TEXTURE_SIZE`]: its side and pixels; none if it can't be.
    fn drawn(&self) -> Option<(u32, Vec<u8>)> {
        let mut side = self.side as usize;
        while side > 2 * TEXTURE_SIZE && crate::device::short_of_memory((side * side) as u64 * WORK_PER_PIXEL) {
            side /= 2;
        }
        let pixels = match &self.draw {
            Draw::Shape(i) => textures::shape_at(*i, side),
            Draw::Grain(i) => textures::grain_at(*i, side),
            Draw::Source(pixels) => pixels.sharper(side),
        }?;
        (pixels.len() == side * side).then_some((side as u32, pixels))
    }
}

/// A big picture ready: its side, and a grey byte a pixel, rows top to bottom, with its smaller
/// levels after it (see [`procraft_gpu::grey_levels`]), so that a stroke only copies them.
#[derive(Clone)]
pub(crate) struct Picture {
    pub(crate) side: u32,
    pub(crate) levels: Arc<Vec<u8>>,
}

impl Picture {
    /// `pixels`, `side` square, ready; none if they aren't that.
    fn of(side: u32, pixels: &[u8]) -> Option<Picture> {
        Some(Picture { side, levels: Arc::new(procraft_gpu::grey_levels(pixels, side)?) })
    }
}

/// Whether a stamp or tile `pixels` canvas pixels across reads the big picture rather than the
/// one brushes hold.
pub(crate) fn past_held(pixels: f32) -> bool {
    pixels.is_finite() && pixels > TEXTURE_SIZE as f32 * MAGNIFIED_MOST
}

/// `brush`'s shape big, if it is one of ours that shows more so: none for the round tip, a brush
/// file's own image, a picture of the user's own, or one soft all through.
pub(crate) fn shape_of(brush: &Brush) -> Option<Big> {
    if brush.shape_file_now().is_some() {
        return None;
    }
    let (draw, side, what) = match (&brush.shape.source, brush.shape.image) {
        (Some(source), _) => (Draw::Source(source.pixels.clone()), source.pixels.most_side(), source.id),
        (None, Some(i)) if textures::shape_sharpens(i) => (Draw::Shape(i), SHARPEST, u64::from(i)),
        _ => return None,
    };
    (side > TEXTURE_SIZE).then(|| Big { id: id_of(0, what, side), side: side as u32, draw })
}

/// `brush`'s grain big, as [`shape_of`] says (none for a grain that isn't there).
pub(crate) fn grain_of(brush: &Brush) -> Option<Big> {
    if brush.grain_file_now().is_some_and(|f| f.full.is_some()) || brush.grain.depth <= 0.0 {
        return None;
    }
    let (draw, side, what) = match (&brush.grain.source, brush.grain.image) {
        (Some(source), _) => (Draw::Source(source.pixels.clone()), source.pixels.most_side(), source.id),
        (None, Some(i)) if textures::grain_sharpens(i) => (Draw::Grain(i), SHARPEST, u64::from(i)),
        _ => return None,
    };
    (side > TEXTURE_SIZE).then(|| Big { id: id_of(1, what, side), side: side as u32, draw })
}

/// The grain's tile on the canvas (pixels) under `brush`'s stamps `diameter` canvas pixels across:
/// moving with the stamps, from their size; fixed to the canvas, its own.
pub(crate) fn grain_tile(brush: &Brush, diameter: f32) -> f32 {
    if brush.grain.mode == GrainMode::Moving || brush.grain_each_stamp() {
        let square = diameter / if brush.round_tip() { procraft_engine::brushfile::ROUND_TIP_SHARE } else { 1.0 };
        brush.grain.moving_tile(square, brush.setting("grain_zoom"), 1.0)
    } else {
        brush.grain_tile()
    }
}

/// The big pictures `brush` (and its second) can read at its most size: those to have ready.
pub(crate) fn reached_by(brush: &Brush) -> Vec<Big> {
    let second = brush.secondary.as_deref().map(|b| b.clone().as_second());
    std::iter::once(brush)
        .chain(second.as_ref())
        .flat_map(|b| {
            let most = b.diameter(1.0);
            let shape = shape_of(b).filter(|_| past_held(most));
            let grain = grain_of(b).filter(|_| past_held(grain_tile(b, most)));
            [shape, grain]
        })
        .flatten()
        .collect()
}

impl crate::State {
    /// The big pictures kept in the folder beside the library in `dir` from now on, and the tools'
    /// brushes' read in from there where they are kept: none is made here (a brush's are made when
    /// it is chosen, or when a stroke of it first wants one).
    pub(crate) fn big_pictures_kept(&self, dir: &Path) {
        use_folder(dir);
        let tools = [crate::ToolKind::Paint, crate::ToolKind::Erase, crate::ToolKind::Smudge].map(|t| self.brush(t));
        load(tools.into_iter().flat_map(reached_by).collect(), false);
    }
}

impl crate::CanvasSession {
    /// The sides of the big pictures the brush at `set`, `index` in the library reads at its most
    /// size (and its second's); none for one that reads none, or isn't there. For tests.
    #[doc(hidden)]
    pub fn big_picture_sides(&self, set: u32, index: u32) -> Vec<u32> {
        let st = self.shared.lock();
        let brush = st.library.get(set as usize).and_then(|s| s.brushes.get(index as usize));
        brush.map(|b| reached_by(b).iter().map(|big| big.side).collect()).unwrap_or_default()
    }
}

/// What tells a big picture apart: FNV-1a of what it is (a shape or a grain), which (our
/// picture's place, or a stand-in's id) and its side.
fn id_of(kind: u64, what: u64, side: usize) -> u64 {
    [0x4249_4750_4943_5400 | kind, what, side as u64]
        .into_iter()
        .flat_map(u64::to_le_bytes)
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

#[derive(Default)]
struct Store {
    /// Where they are kept (this build's folder), once known.
    folder: Option<PathBuf>,
    /// Folders of other builds' pictures, to take away.
    stale: Vec<PathBuf>,
    /// In memory, the latest wanted last.
    held: Vec<(u64, Picture)>,
    /// To be made, the first first.
    waiting: VecDeque<Big>,
    /// Those to hold in memory once made: a chosen brush's.
    wanted: HashSet<u64>,
    /// Being made.
    making: Option<u64>,
    /// Whether a thread is making them.
    working: bool,
}

struct Shared {
    store: Mutex<Store>,
    /// Told each time one is made, and when the making stops.
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The pictures' store: the app's one; in a test, its thread's own (so tests running side by side
/// keep theirs apart).
#[cfg(not(test))]
fn shared() -> Arc<Shared> {
    static SHARED: std::sync::LazyLock<Arc<Shared>> =
        std::sync::LazyLock::new(|| Arc::new(Shared { store: Mutex::default(), changed: Condvar::new() }));
    Arc::clone(&SHARED)
}

#[cfg(test)]
std::thread_local! {
    static SHARED: Arc<Shared> = Arc::new(Shared { store: Mutex::default(), changed: Condvar::new() });
    /// Pictures not made at once when wanted (a test of the app's way: made aside).
    pub(crate) static ASIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// No big picture read (a test's strokes as brushes hold their pictures, to set beside them).
    pub(crate) static OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn shared() -> Arc<Shared> {
    SHARED.with(Arc::clone)
}

/// Whether a picture wanted and not made is made at once: in tests, so that their strokes draw
/// the same each time (but those of the app's way).
fn at_once() -> bool {
    #[cfg(test)]
    let aside = ASIDE.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let aside = true;
    !aside
}

/// Keeps the pictures in the folder beside the library in `dir` from now on (those of other
/// builds there taken away).
pub(crate) fn use_folder(dir: &Path) {
    use_folder_keyed(dir, KEY);
}

/// [`use_folder`] for pictures kept under `key`.
pub(crate) fn use_folder_keyed(dir: &Path, key: &str) {
    let name = format!("{FOLDER}{key}");
    let folder = dir.join(&name);
    let shared = shared();
    let mut store = shared.lock();
    if store.folder.as_deref() == Some(folder.as_path()) {
        return;
    }
    let others = std::fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|entry| {
        let other = entry.file_name().to_str()?.to_owned();
        (other.starts_with(FOLDER) && other != name).then(|| entry.path())
    });
    store.stale.extend(others);
    store.folder = Some(folder);
    // those held came from elsewhere, or were made before there was a folder: kept
    let start = !store.stale.is_empty() && !store.working;
    if start {
        store.working = true;
    }
    drop(store);
    if start {
        work(&shared);
    }
}

/// `big` ready: held in memory, or read from its file; else none, and it is made before any
/// other (or, in a test, made now).
pub(crate) fn ready(big: &Big) -> Option<Picture> {
    #[cfg(test)]
    if OFF.with(std::cell::Cell::get) {
        return None;
    }
    let shared = shared();
    let folder = {
        let mut store = shared.lock();
        if let Some(picture) = take_held(&mut store, big.id) {
            return Some(picture);
        }
        store.folder.clone()
    };
    if let Some(picture) = folder.as_deref().and_then(|f| read(f, big.id)) {
        hold(&mut shared.lock(), big.id, picture.clone());
        return Some(picture);
    }
    if at_once() {
        let (side, pixels) = big.drawn()?;
        if let Some(folder) = folder.as_deref() {
            write(folder, big.id, &pixels);
        }
        let picture = Picture::of(side, &pixels)?;
        hold(&mut shared.lock(), big.id, picture.clone());
        return Some(picture);
    }
    ask(&shared, std::slice::from_ref(big), true);
    None
}

/// A brush chosen: its big pictures (`bigs`) read into memory off this thread, and (`make`) those
/// not made yet made before any other.
pub(crate) fn load(bigs: Vec<Big>, make: bool) {
    if bigs.is_empty() || at_once() {
        return;
    }
    let shared = shared();
    let (unheld, folder): (Vec<Big>, _) = {
        let mut store = shared.lock();
        let unheld = bigs.into_iter().filter(|b| take_held(&mut store, b.id).is_none()).collect();
        (unheld, store.folder.clone())
    };
    if unheld.is_empty() {
        return;
    }
    let reader = Arc::clone(&shared);
    let read_them = move || {
        let mut unmade = Vec::new();
        for big in unheld {
            match folder.as_deref().and_then(|f| read(f, big.id)) {
                Some(picture) => hold(&mut reader.lock(), big.id, picture),
                None => unmade.push(big),
            }
        }
        if make {
            ask(&reader, &unmade, true);
        }
    };
    let thread = std::thread::Builder::new().name("procraft-big-pictures-read".to_owned());
    if let Err(e) = thread.spawn(read_them) {
        log::warn!("reading a brush's big pictures: {e}");
    }
}

/// Makes `bigs` (those not kept already) on a thread of their own, after any wanted first: an
/// imported brush's.
pub(crate) fn make_ahead(bigs: Vec<Big>) {
    let shared = shared();
    if shared.lock().folder.is_none() || at_once() {
        return;
    }
    ask(&shared, &bigs, false);
}

/// Puts `bigs` among those to make (`first`: before the others, and held once made), starting
/// the thread that makes them if it isn't going.
fn ask(shared: &Arc<Shared>, bigs: &[Big], first: bool) {
    if bigs.is_empty() {
        return;
    }
    let mut store = shared.lock();
    for big in bigs.iter().rev() {
        if store.held.iter().any(|(id, _)| *id == big.id) {
            continue;
        }
        if first {
            store.wanted.insert(big.id);
        }
        if store.making == Some(big.id) {
            continue;
        }
        let queued = store.waiting.iter().position(|b| b.id == big.id);
        match (queued, first) {
            (Some(_), false) => {}
            (Some(at), true) => {
                if let Some(b) = store.waiting.remove(at) {
                    store.waiting.push_front(b);
                }
            }
            (None, true) => store.waiting.push_front(big.clone()),
            (None, false) => store.waiting.push_back(big.clone()),
        }
    }
    let start = !store.working && !store.waiting.is_empty();
    if start {
        store.working = true;
    }
    drop(store);
    if start {
        work(shared);
    }
}

/// Starts the thread that takes other builds' folders away and makes those waiting, one at a time.
fn work(shared: &Arc<Shared>) {
    let worker = Arc::clone(shared);
    let thread = std::thread::Builder::new().name("procraft-big-pictures".to_owned());
    if let Err(e) = thread.spawn(move || make_waiting(&worker)) {
        log::warn!("making the brushes' big pictures: {e}");
        shared.lock().working = false;
    }
}

fn make_waiting(shared: &Shared) {
    let started = std::time::Instant::now();
    let mut made = 0;
    loop {
        let (big, folder, wanted, stale) = {
            let mut store = shared.lock();
            let stale = std::mem::take(&mut store.stale);
            let next = if stale.is_empty() { store.waiting.pop_front() } else { None };
            match next {
                Some(big) => {
                    store.making = Some(big.id);
                    let wanted = store.wanted.contains(&big.id);
                    (Some(big), store.folder.clone(), wanted, stale)
                }
                None if stale.is_empty() => {
                    store.working = false;
                    store.making = None;
                    shared.changed.notify_all();
                    if made > 0 {
                        log::info!("{made} big brush pictures made in {} ms", started.elapsed().as_millis());
                    }
                    return;
                }
                None => (None, None, false, stale),
            }
        };
        for old in stale {
            if let Err(e) = std::fs::remove_dir_all(&old) {
                log::warn!("taking away another build's brush pictures: {e}");
            }
        }
        let Some(big) = big else { continue };
        // kept already (by an earlier start), and not wanted in memory: nothing to do
        let kept = folder.as_deref().is_some_and(|f| file(f, big.id).is_file());
        let picture = if kept && !wanted {
            None
        } else if let Some(picture) = folder.as_deref().and_then(|f| read(f, big.id)) {
            Some(picture)
        } else {
            let drawn =
                if wanted { big.drawn() } else { procraft_engine::par::within_threads(AHEAD_THREADS, || big.drawn()) };
            made += usize::from(drawn.is_some());
            if let (Some((_, pixels)), Some(folder)) = (&drawn, folder.as_deref()) {
                write(folder, big.id, pixels);
            }
            // ready in memory only for a brush chosen (by now)
            let wanted = wanted || shared.lock().wanted.contains(&big.id);
            drawn.filter(|_| wanted).and_then(|(side, pixels)| Picture::of(side, &pixels))
        };
        let mut store = shared.lock();
        store.making = None;
        if store.wanted.remove(&big.id)
            && let Some(picture) = picture
        {
            // (told, so that a chosen brush's pictures can be seen coming before the others)
            log::info!(
                "a chosen brush's big picture ready ({} a side) {} ms in, {} others waiting",
                picture.side,
                started.elapsed().as_millis(),
                store.waiting.len()
            );
            hold(&mut store, big.id, picture);
        }
        shared.changed.notify_all();
    }
}

/// `id`'s picture if it is held in memory, made the latest wanted.
fn take_held(store: &mut Store, id: u64) -> Option<Picture> {
    let at = store.held.iter().position(|(held, _)| *held == id)?;
    let entry = store.held.remove(at);
    let picture = entry.1.clone();
    store.held.push(entry);
    Some(picture)
}

/// Holds `picture` as `id`'s, letting go of the longest unwanted past [`HELD_BYTES`].
fn hold(store: &mut Store, id: u64, picture: Picture) {
    store.held.retain(|(other, _)| *other != id);
    store.held.push((id, picture));
    while store.held.len() > 1 && held_in(store) > HELD_BYTES {
        store.held.remove(0);
    }
}

fn held_in(store: &Store) -> usize {
    store.held.iter().map(|(_, p)| p.levels.len()).sum()
}

/// Lets go of the pictures held in memory (the system short of it): read again when next wanted.
pub(crate) fn let_go() {
    shared().lock().held.clear();
}

/// How many pictures are still to be made, the one being made with them.
#[doc(hidden)]
pub fn big_pictures_left() -> usize {
    let shared = shared();
    let store = shared.lock();
    store.waiting.len() + usize::from(store.making.is_some())
}

/// Waits until none is left to make, `most` at the most; whether none is.
#[doc(hidden)]
pub fn wait_for_big_pictures(most: std::time::Duration) -> bool {
    let shared = shared();
    let until = std::time::Instant::now() + most;
    let mut store = shared.lock();
    while store.working {
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return false;
        }
        store = shared.changed.wait_timeout(store, left).unwrap_or_else(PoisonError::into_inner).0;
    }
    true
}

/// The file `id`'s picture is kept in, in `folder`: its pixels packed as LZ4 (a quarter of the
/// bytes or less, and unpacked in a few milliseconds).
fn file(folder: &Path, id: u64) -> PathBuf {
    folder.join(format!("{id:016x}.lz4"))
}

/// The side of a picture `bytes` long, if it is one (a square of a power of two past
/// [`TEXTURE_SIZE`], up to [`SHARPEST`]).
fn side_of(bytes: usize) -> Option<u32> {
    let side = bytes.isqrt();
    (side * side == bytes && side.is_power_of_two() && side > TEXTURE_SIZE && side <= SHARPEST).then_some(side as u32)
}

/// `id`'s picture as kept in `folder`; none if it isn't, or isn't whole.
fn read(folder: &Path, id: u64) -> Option<Picture> {
    let packed = std::fs::read(file(folder, id)).ok()?;
    let pixels = procraft_engine::applz4::decode(&packed, SHARPEST * SHARPEST)?;
    Picture::of(side_of(pixels.len())?, &pixels)
}

/// Keeps `pixels` as `id`'s picture in `folder`, whole or not at all.
fn write(folder: &Path, id: u64, pixels: &[u8]) {
    let packed = procraft_engine::applz4::encode(pixels);
    let written = std::fs::create_dir_all(folder)
        .map_err(|e| crate::project::file_error("keeping a brush's big picture", e))
        .and_then(|()| {
            crate::project::write_atomically(&file(folder, id), |p| {
                crate::storage::write(p, &packed)
                    .map_err(|e| crate::project::file_error("keeping a brush's big picture", e))
            })
        });
    if let Err(e) = written {
        log::warn!("{e}");
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use procraft_engine::brush::{Brush, Source, Taper};
    use procraft_engine::textures::{SHARPEST, TEXTURE_SIZE, grain, shape};

    use super::*;
    use crate::{CanvasSession, PenSample, PointerKind};

    /// A folder of its own for a test's pictures, empty.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("procraft-big-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Makes this thread's pictures aside, as the app does, until dropped.
    struct Aside;

    impl Aside {
        fn on() -> Aside {
            ASIDE.with(|a| a.set(true));
            Aside
        }
    }

    impl Drop for Aside {
        fn drop(&mut self) {
            ASIDE.with(|a| a.set(false));
            let shared = shared();
            let mut store = shared.lock();
            store.waiting.clear();
            drop(store);
            wait_for_big_pictures(Duration::from_secs(120));
            *shared.lock() = Store::default();
        }
    }

    #[test]
    fn a_stamp_past_what_brushes_hold_reads_the_big_picture() {
        assert!(!past_held(200.0));
        assert!(!past_held(320.0));
        assert!(past_held(321.0));
        assert!(past_held(9000.0));
        assert!(!past_held(f32::NAN));
        assert!(!past_held(f32::INFINITY));
    }

    #[test]
    fn only_our_pictures_that_show_more_have_big_ones_each_of_one_size() {
        let with_shape = |image: Option<u32>| {
            let mut b = Brush::round_pen();
            b.shape.image = image;
            b
        };
        // the round tip is drawn as it is at any size; a soft round shows nothing more
        assert!(shape_of(&with_shape(None)).is_none());
        assert!(shape_of(&with_shape(Some(shape::SOFT_ROUND))).is_none());
        // a block does, at the most side ours are drawn at
        let block = shape_of(&with_shape(Some(shape::BLOCK))).unwrap();
        assert_eq!(block.side, SHARPEST as u32);
        // a picture of the user's own goes no bigger than it is
        let mut own = with_shape(None);
        own.shape.source = Source::new(vec![255; TEXTURE_SIZE * TEXTURE_SIZE]);
        assert!(shape_of(&own).is_none());
        // a grain, and none for a grain that isn't there
        let mut b = Brush::round_pen();
        b.grain.image = Some(grain::GRUNGE);
        b.grain.depth = 1.0;
        assert_eq!(grain_of(&b).map(|w| w.side), Some(SHARPEST as u32));
        b.grain.depth = 0.0;
        assert!(grain_of(&b).is_none());
        // each told apart
        let other = shape_of(&with_shape(Some(shape::STAR))).unwrap();
        assert_ne!(block.id, other.id);
        // a stock brush's stand-in: the side of the stock image it stands in for, as far as a power
        // of two goes (the most sides measured are 1024 and 2048)
        let sets = crate::stock_library::shipped();
        let sides: HashSet<u32> = sets.iter().flat_map(|s| &s.brushes).filter_map(shape_of).map(|b| b.side).collect();
        assert!(sides.contains(&1024) && sides.contains(&2048), "{sides:?}");
        assert!(sides.iter().all(|s| s.is_power_of_two() && *s > TEXTURE_SIZE as u32 && *s <= SHARPEST as u32));
    }

    /// The key the pictures are kept under is the drawing code's: worked out again from those
    /// files, it is the one built in, so a build whose drawing differs keeps its pictures apart.
    #[test]
    fn the_key_is_the_drawing_code_s() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut key = 0xcbf2_9ce4_8422_2325u64;
        for path in ["../engine/src/textures.rs", "../engine/src/stock_tones.rs", "src/stand_ins.rs"] {
            let text = std::fs::read(root.join(path)).unwrap();
            for byte in path.bytes().chain(text.into_iter().filter(|&b| b != b'\r')) {
                key = (key ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        assert_eq!(format!("{key:016x}"), KEY);
    }

    /// Kept under another build's key, a picture isn't read: that build's folder is taken away and
    /// the picture is made again under this one's.
    #[test]
    fn a_build_that_draws_them_differently_makes_them_again() {
        let _aside = Aside::on();
        let dir = folder("key");
        let mut b = Brush::round_pen();
        b.shape.image = Some(shape::STAR);
        let big = shape_of(&b).unwrap();
        // as an older build kept it: another picture under the same id
        let old = dir.join(format!("{FOLDER}0123456789abcdef"));
        write(&old, big.id, &vec![7; big.side as usize * big.side as usize]);
        use_folder_keyed(&dir, "0123456789abcdef");
        assert_eq!(ready(&big).map(|p| p.levels[0]), Some(7));
        let_go();
        // this build: the old folder goes, and the picture is made again
        use_folder(&dir);
        assert!(ready(&big).is_none());
        assert!(wait_for_big_pictures(Duration::from_secs(120)));
        assert!(!old.exists());
        let made = ready(&big).unwrap();
        assert_eq!(made.levels[..SHARPEST * SHARPEST], textures::shape_at(shape::STAR, SHARPEST).unwrap());
        assert!(file(&dir.join(format!("{FOLDER}{KEY}")), big.id).is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A picture made ahead, on a few threads, is the one made for a brush chosen, on all.
    #[test]
    fn a_picture_made_ahead_on_a_few_threads_is_the_one_made_on_all() {
        let brush =
            shared_library().iter().flat_map(|s| &s.brushes).find_map(|b| shape_of(b).filter(|g| g.side == 1024));
        let big = brush.unwrap();
        let ahead = procraft_engine::par::within_threads(AHEAD_THREADS, || big.drawn()).unwrap();
        assert!(ahead == big.drawn().unwrap());
    }

    /// Wanted and not made, a picture is made off the thread that asked (which isn't held up), first
    /// among those waiting, and kept; those held in memory never pass their bytes.
    #[test]
    fn a_picture_not_made_is_made_aside_first_and_held_within_bounds() {
        let _aside = Aside::on();
        let dir = folder("first");
        use_folder(&dir);
        let big_of = |i: u32| {
            let mut b = Brush::round_pen();
            b.shape.image = Some(i);
            shape_of(&b).unwrap()
        };
        // many waiting, then one wanted: it comes first
        make_ahead([shape::STAR, shape::BLOCK, shape::RING, shape::DIAMOND].map(big_of).to_vec());
        let heart = big_of(shape::HEART);
        let started = Instant::now();
        assert!(ready(&heart).is_none());
        let asked = started.elapsed();
        assert!(asked < Duration::from_millis(100), "asking took {asked:?}");
        let drawn = loop {
            if let Some(p) = ready(&heart) {
                break p;
            }
            assert!(started.elapsed() < Duration::from_secs(120), "not made");
            std::thread::sleep(Duration::from_millis(5));
        };
        let took = started.elapsed();
        let left = big_pictures_left();
        eprintln!("the wanted picture made in {} ms, {left} of the others left", took.as_millis());
        assert!(left >= 2, "{left} left: the wanted one wasn't first");
        assert_eq!(drawn.levels[..SHARPEST * SHARPEST], textures::shape_at(shape::HEART, SHARPEST).unwrap());
        // many more held: never past the bytes, the latest held
        let shared = shared();
        for k in 0..8u64 {
            let mut store = shared.lock();
            hold(&mut store, u64::MAX - k, Picture::of(2048, &vec![0; 2048 * 2048]).unwrap());
            assert!(held_in(&store) <= HELD_BYTES, "{} bytes held", held_in(&store));
        }
        assert!(shared.lock().held.iter().any(|(id, _)| *id == u64::MAX - 7));
        drop(_aside);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The side of the canvas test strokes are drawn on.
    const SIDE: u32 = 1024;

    /// A session on a GPU canvas [`SIDE`] square, fitted to its window; none without a GPU.
    fn gpu_session() -> Option<Arc<CanvasSession>> {
        let gpu = procraft_gpu::Gpu::headless().ok()?;
        let s = CanvasSession::new(SIDE, SIDE);
        {
            let mut st = s.shared.lock();
            st.window_size = (SIDE, SIDE);
            st.view = Some(procraft_engine::view::View::fit((SIDE, SIDE), (SIDE, SIDE)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        Some(s)
    }

    fn sample(k: f32) -> PenSample {
        let side = SIDE as f32;
        PenSample {
            x: side * (0.2 + 0.6 * k),
            y: side * (0.5 + 0.12 * (k * 6.0).sin()),
            pressure: 0.55 + 0.45 * k,
            tilt: 0.0,
            orientation: 0.0,
            time_ms: f64::from(k) * 600.0,
            roll: 0.0,
        }
    }

    /// A stroke of the current brush across the canvas at full size, its seed `seed`: the layer's
    /// pixels once it is laid.
    fn stroke(s: &CanvasSession, seed: u32) -> Option<Vec<u8>> {
        s.shared.lock().strokes = seed;
        s.set_brush_size(1.0);
        s.set_color(0.2, 0.4, 0.8);
        s.stroke_begin(PointerKind::Stylus, vec![sample(0.0)]);
        for i in 1..24 {
            s.stroke_add(vec![sample(i as f32 / 24.0)]);
        }
        s.stroke_end(vec![sample(1.0)]);
        let mut guard = s.shared.lock();
        let st = &mut *guard;
        let id = st.doc.current();
        let (canvas, gpu) = (st.canvas.as_mut()?, st.gpu.as_ref()?);
        canvas.render_finished(gpu, &mut st.doc);
        canvas.read_layer(gpu, id).ok()
    }

    /// The brushes a session starts with that read big pictures at their most size: their places,
    /// themselves and those pictures.
    fn big_brushes() -> Vec<((u32, u32), Brush, Vec<Big>)> {
        let mut found = Vec::new();
        for (set, brushes) in shared_library().iter().enumerate() {
            for (index, brush) in brushes.brushes.iter().enumerate() {
                let bigs = reached_by(brush);
                if !bigs.is_empty() {
                    found.push(((set as u32, index as u32), brush.clone(), bigs));
                }
            }
        }
        found
    }

    /// How many pixels of `a` and `b` (RGBA) differ, and by how much at the most.
    fn apart(a: &[u8], b: &[u8]) -> (usize, u8) {
        let mut n = 0;
        let mut most = 0;
        for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
            let d = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            n += usize::from(d > 0);
            most = most.max(d);
        }
        (n, most)
    }

    /// Each of `brushes` (places in a session's library): its pictures made ahead into a folder,
    /// none held in memory, it is chosen and a stroke drawn at once; then, its pictures read in,
    /// the same stroke drawn again. The two lay the same pixels. Each brush's name, the pixels
    /// apart and by how much; none without a GPU.
    fn first_and_later(name: &str, count: Option<usize>) -> Option<Vec<(String, usize, u8)>> {
        let _aside = Aside::on();
        // (the full run's kept for the next, on another backend)
        let dir =
            if count.is_some() { folder(name) } else { std::env::temp_dir().join(format!("procraft-big-{name}")) };
        use_folder(&dir);
        procraft_gpu::Gpu::headless().ok()?;
        let mut brushes = big_brushes();
        if let Some(count) = count {
            // a few of each kind: a big shape, a big grain, a dual brush, a wet one
            let mut picked: Vec<_> = Vec::new();
            let kinds: [&dyn Fn(&Brush) -> bool; 4] =
                [&|b| shape_of(b).is_some(), &|b| grain_of(b).is_some(), &|b| b.secondary.is_some(), &|b| {
                    b.mixes_wet()
                }];
            for kind in kinds {
                let mut n = 0;
                for b in &brushes {
                    if n < count.div_ceil(4)
                        && kind(&b.1)
                        && !picked.iter().any(|p: &((u32, u32), Brush, Vec<Big>)| p.0 == b.0)
                    {
                        picked.push(b.clone());
                        n += 1;
                    }
                }
            }
            brushes = picked;
        }
        let all: Vec<Big> = brushes.iter().flat_map(|b| b.2.iter().cloned()).collect();
        let started = Instant::now();
        make_ahead(all);
        assert!(wait_for_big_pictures(Duration::from_secs(3600)), "not made");
        eprintln!("{} brushes' pictures made ahead in {:.1} s", brushes.len(), started.elapsed().as_secs_f64());
        let mut out = Vec::new();
        for ((set, index), brush, bigs) in brushes {
            let brush = brush.name;
            let_go();
            let s = gpu_session()?;
            assert!(s.select_brush(set, index));
            let first = stroke(&s, 7)?;
            s.undo();
            // read in by now (as choosing it reads them)
            let started = Instant::now();
            while bigs.iter().any(|b| !shared().lock().held.iter().any(|(id, _)| *id == b.id)) {
                assert!(started.elapsed() < Duration::from_secs(30), "{brush}: not read in");
                std::thread::sleep(Duration::from_millis(5));
            }
            let later = stroke(&s, 7)?;
            let (n, most) = apart(&first, &later);
            out.push((brush, n, most));
        }
        drop(_aside);
        if count.is_some() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        Some(out)
    }

    /// The issue: a big brush chosen and drawn with at once read the pictures brushes hold, its
    /// later strokes the sharper ones, up to 227 levels apart. Now its pictures are ready before it
    /// is chosen, and its first stroke is its later ones, pixel for pixel: a few brushes of each kind.
    #[test]
    fn a_big_brush_drawn_with_at_once_lays_the_pixels_its_later_strokes_do() {
        let Some(results) = first_and_later("first-later", Some(8)) else { return };
        assert!(results.len() >= 6, "{} brushes", results.len());
        for (brush, n, most) in &results {
            eprintln!("{brush}: {n} pixels apart, {most} levels at the most");
        }
        assert!(results.iter().all(|r| r.1 == 0), "{results:?}");
    }

    /// [`a_big_brush_drawn_with_at_once_lays_the_pixels_its_later_strokes_do`] for every brush the
    /// app comes with that reads big pictures (minutes: run with `--ignored`, and `WGPU_BACKEND`
    /// vulkan or gl).
    #[test]
    #[ignore]
    fn every_big_brush_drawn_with_at_once_lays_the_pixels_its_later_strokes_do() {
        let Some(results) = first_and_later("first-later-all", None) else { return };
        let apart: Vec<_> = results.iter().filter(|r| r.1 > 0).collect();
        eprintln!("{} brushes, {} apart: {apart:?}", results.len(), apart.len());
        assert!(apart.is_empty());
    }

    /// From choosing the brush at `place` to its first stamps laid on the GPU (the canvas made, as
    /// it is when a brush is chosen), on a session of its own; none without a GPU.
    fn choose_and_stamp(place: (u32, u32)) -> Option<Duration> {
        let s = gpu_session()?;
        s.set_brush_size(1.0);
        let started = Instant::now();
        assert!(s.select_brush(place.0, place.1));
        s.set_brush_size(1.0);
        s.stroke_begin(PointerKind::Stylus, vec![sample(0.0)]);
        s.stroke_add(vec![sample(0.05)]);
        let mut guard = s.shared.lock();
        let st = &mut *guard;
        let (canvas, gpu) = (st.canvas.as_mut()?, st.gpu.as_ref()?);
        canvas.render(gpu, &mut st.doc, None);
        gpu.wait().ok()?;
        Some(started.elapsed())
    }

    /// The place, name and pictures of the brush a session starts with that reads the most big
    /// pictures (a shape and a grain, or its second's too); none without a GPU.
    fn biggest_brush() -> Option<((u32, u32), String, Vec<Big>)> {
        procraft_gpu::Gpu::headless().ok()?;
        let brushes = big_brushes();
        brushes.into_iter().rev().max_by_key(|b| b.2.len()).map(|(place, brush, bigs)| (place, brush.name, bigs))
    }

    /// Choosing a big brush whose pictures are made reads them in milliseconds, as the reference
    /// reads its brush's images when one is chosen: from choosing it to its first stamps laid on
    /// the GPU, its pictures read from their files (none held in memory), against the round pen
    /// (none), and with them held already.
    #[test]
    fn a_big_brush_chosen_lays_its_first_stamps_in_milliseconds() {
        let _aside = Aside::on();
        let dir = folder("pick");
        use_folder(&dir);
        let Some((place, name, bigs)) = biggest_brush() else { return };
        let sides: Vec<u32> = bigs.iter().map(|b| b.side).collect();
        make_ahead(bigs);
        assert!(wait_for_big_pictures(Duration::from_secs(600)), "not made");
        let pen = crate::brush_library::place_of(&shared_library(), (1, 0));
        let pen = (pen.0 as u32, pen.1 as u32);
        // the quickest of a few (the GPU's own start-up aside)
        let quickest = |place: (u32, u32), held: bool| {
            (0..3)
                .filter_map(|_| {
                    if !held {
                        let_go();
                    }
                    choose_and_stamp(place)
                })
                .min()
        };
        let (Some(from_files), Some(held), Some(round)) =
            (quickest(place, false), quickest(place, true), quickest(pen, false))
        else {
            return;
        };
        eprintln!(
            "{name} ({sides:?}): chosen to its first stamps in {} ms with its pictures read from \
             their files, {} ms held; the round pen {} ms",
            from_files.as_millis(),
            held.as_millis(),
            round.as_millis()
        );
        // milliseconds, not seconds: reading four and putting them on the GPU (a debug build's,
        // 160-180 ms alone, 450 beside the whole suite) is under a second, where making them
        // takes seconds each
        assert!(from_files < round + Duration::from_secs(1), "{from_files:?} against {round:?}");
        assert!(held < from_files + Duration::from_millis(100), "{held:?} against {from_files:?}");
        drop(_aside);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// However many big brushes are drawn with, the pictures held stay within bounds: in memory
    /// [`HELD_BYTES`], on the GPU the last three shapes and three grains (as many as the reference
    /// holds, six).
    #[test]
    fn the_pictures_held_stay_within_bounds_however_many_brushes_draw() {
        let _aside = Aside::on();
        let dir = folder("bounds");
        use_folder(&dir);
        let Some(s) = gpu_session() else { return };
        let brushes: Vec<_> = big_brushes().into_iter().step_by(7).take(6).collect();
        make_ahead(brushes.iter().flat_map(|b| b.2.iter().cloned()).collect());
        assert!(wait_for_big_pictures(Duration::from_secs(600)), "not made");
        let mut most = (0, 0, 0);
        for ((set, index), brush, _) in &brushes {
            assert!(s.select_brush(*set, *index));
            stroke(&s, 3);
            let held = held_in(&shared().lock());
            let (shapes, grains) = {
                let st = s.shared.lock();
                st.canvas.as_deref().map(procraft_gpu::Canvas::full_images_held).unwrap_or_default()
            };
            eprintln!(
                "{}: {} MB held in memory, {shapes} shapes and {grains} grains on the GPU",
                brush.name,
                held >> 20
            );
            most = (most.0.max(held), most.1.max(shapes), most.2.max(grains));
        }
        assert!(most.0 <= HELD_BYTES && most.1 <= 3 && most.2 <= 3, "{most:?}");
        drop(_aside);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Opening the library makes no big pictures, as the reference loads a brush's images only when
    /// it is chosen: none waits to be made and none is kept. Choosing a brush that reads them makes
    /// its own, and only those, kept in their files.
    #[test]
    fn opening_the_library_makes_none_and_choosing_a_brush_makes_its_own() {
        let _aside = Aside::on();
        let dir = folder("on-demand");
        let s = CanvasSession::new(64, 64);
        s.shared.lock().big_pictures_kept(&dir);
        let kept = || std::fs::read_dir(dir.join(format!("{FOLDER}{KEY}"))).map_or(0, Iterator::count);
        assert_eq!((big_pictures_left(), kept()), (0, 0));
        let ((set, index), brush, bigs) = big_brushes().into_iter().next().unwrap();
        let ids: HashSet<u64> = bigs.iter().map(|b| b.id).collect();
        let started = Instant::now();
        assert!(s.select_brush(set, index));
        while ids.iter().any(|id| !shared().lock().held.iter().any(|(held, _)| held == id)) {
            assert!(started.elapsed() < Duration::from_secs(300), "not made");
            std::thread::sleep(Duration::from_millis(5));
        }
        eprintln!(
            "{}: its {} big pictures made {} ms after it was chosen",
            brush.name,
            ids.len(),
            started.elapsed().as_millis()
        );
        assert!(wait_for_big_pictures(Duration::from_secs(60)));
        assert_eq!(kept(), ids.len());
        drop(_aside);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The library a session starts with.
    fn shared_library() -> Vec<crate::brush_library::LibrarySet> {
        crate::brush_library::LibrarySet::shipped(&crate::stock_library::shipped())
    }

    /// Before its pictures are made (the first start), a big brush chosen and drawn with at once
    /// draws (never blank) with the pictures brushes hold, without waiting for them; they are made
    /// before any other waiting, and its strokes from then on read them.
    #[test]
    fn a_stroke_begun_before_its_pictures_are_made_draws_at_once_and_they_come_first() {
        let _aside = Aside::on();
        let dir = folder("interim");
        use_folder(&dir);
        let Some((place, name, bigs)) = biggest_brush() else { return };
        // many others waiting already, as on a first start
        let others: Vec<Big> = big_brushes().into_iter().take(12).flat_map(|b| b.2).collect();
        make_ahead(others);
        let Some(s) = gpu_session() else { return };
        let started = Instant::now();
        let first = stroke_after_choosing(&s, place);
        let took = started.elapsed();
        let first = first.unwrap();
        let ink = first.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
        eprintln!(
            "{name}: chosen and drawn with before its pictures were made in {} ms, {ink} pixels inked",
            took.as_millis()
        );
        assert!(ink > 1000, "{ink} pixels inked");
        // its pictures come first
        let made = Instant::now();
        while bigs.iter().any(|b| !shared().lock().held.iter().any(|(id, _)| *id == b.id)) {
            assert!(made.elapsed() < Duration::from_secs(300), "not made");
            std::thread::sleep(Duration::from_millis(5));
        }
        let left = big_pictures_left();
        eprintln!("its pictures made {} ms later, {left} others left", made.elapsed().as_millis());
        assert!(left > 0, "the chosen brush's pictures weren't made first");
        drop(_aside);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn stroke_after_choosing(s: &CanvasSession, place: (u32, u32)) -> Option<Vec<u8>> {
        assert!(s.select_brush(place.0, place.1));
        stroke(s, 7)
    }

    /// The side of the canvas [`one_stamp`] draws on.
    const CANVAS: usize = 1024;

    /// One stamp of `brush` in white at full size (`diameter` pixels across, its least and most
    /// sizes), in the middle of a canvas [`CANVAS`] square, its pictures as brushes hold them or
    /// (`big`) the big ones: the layer's alpha (0..1), row by row; none without a GPU.
    fn one_stamp(brush: Brush, diameter: f32, big: bool) -> Option<Vec<f32>> {
        one_stamp_at(brush, diameter, big, (0.0, 0.0))
    }

    /// [`one_stamp`] with its middle `offset` canvas pixels from the canvas's.
    fn one_stamp_at(mut brush: Brush, diameter: f32, big: bool, offset: (f32, f32)) -> Option<Vec<f32>> {
        let gpu = procraft_gpu::Gpu::headless().ok()?;
        let side = CANVAS as u32;
        let s = CanvasSession::new(side, side);
        {
            let mut st = s.shared.lock();
            st.window_size = (side, side);
            st.view = Some(procraft_engine::view::View::fit((side, side), (side, side)));
            st.gpu = Some(gpu);
            st.ensure_canvas();
            (brush.pen_taper, brush.touch_taper) = (Taper::NONE, Taper::NONE);
            brush.streamline = 0.0;
            // far apart, so that it can be big (one stamp is drawn)
            brush.spacing = 1.0;
            (brush.min_size, brush.max_size) = (diameter, diameter);
            let tool = st.tool;
            *st.brush_mut(tool)? = brush;
        }
        s.set_brush_size(1.0);
        s.set_color(1.0, 1.0, 1.0);
        OFF.with(|off| off.set(!big));
        let middle = side as f32 / 2.0;
        let (x, y) = (middle + offset.0, middle + offset.1);
        let at = PenSample { x, y, pressure: 1.0, tilt: 0.0, orientation: 0.0, time_ms: 0.0, roll: 0.0 };
        s.stroke_begin(PointerKind::Finger, vec![at]);
        s.stroke_end(vec![]);
        OFF.with(|off| off.set(false));
        let mut guard = s.shared.lock();
        let st = &mut *guard;
        let id = st.doc.current();
        let (canvas, gpu) = (st.canvas.as_mut()?, st.gpu.as_ref()?);
        canvas.render_finished(gpu, &mut st.doc);
        let rgba = canvas.read_layer(gpu, id).ok()?;
        Some(rgba.chunks(4).map(|p| f32::from(p[3]) / 255.0).collect())
    }

    /// The width of the left edge of a block stamp `diameter` pixels across, from a tenth of full to
    /// nine tenths (canvas pixels, the median over the rows through its middle), drawn with our
    /// shape as brushes hold it or (`big`) its big picture; none without a GPU.
    fn block_edge(diameter: f32, big: bool) -> Option<f32> {
        let mut block = Brush { name: "Block".to_owned(), ..Brush::round_pen() };
        block.shape.image = Some(shape::BLOCK);
        let alpha = one_stamp(block, diameter, big)?;
        let a = |x: usize, y: usize| alpha.get(y * CANVAS + x).copied().unwrap_or(0.0);
        // where along a row its value first passes `level`, between pixels
        let crossing = |y: usize, level: f32| {
            (1..CANVAS / 2).find(|&x| a(x, y) >= level).map(|x| {
                let (before, now) = (a(x - 1, y), a(x, y));
                x as f32 - 1.0 + (level - before) / (now - before).max(1e-6)
            })
        };
        let mut widths: Vec<f32> = (480..545).filter_map(|y| Some(crossing(y, 0.9)? - crossing(y, 0.1)?)).collect();
        widths.sort_by(f32::total_cmp);
        widths.get(widths.len() / 2).copied()
    }

    /// A block 500 pixels across: its edge, a ramp of a few texels, magnified twice over as brushes
    /// hold it (some 5.6 pixels); read from its big picture (2048, through its smaller levels) as
    /// crisp as at the size brushes hold (some 3), and so however big.
    #[test]
    fn a_block_500_pixels_across_has_a_crisp_edge() {
        let (Some(before), Some(after)) = (block_edge(500.0, false), block_edge(500.0, true)) else { return };
        eprintln!("the block's edge at 500 px: {before:.2} px as held, {after:.2} px from its big picture");
        assert!(before > 4.5, "{before}");
        assert!(after < 3.5 && after < before * 0.6, "{after} against {before}");
        let Some(big) = block_edge(1400.0, true) else { return };
        eprintln!("at 1400 px: {big:.2} px");
        assert!(big < 3.5, "{big}");
    }

    /// How steep `alpha` (a canvas [`CANVAS`] square) is from pixel to pixel over the 256 square in
    /// its middle: the mean of each step squared.
    fn steepness(alpha: &[f32]) -> f64 {
        let a = |x: usize, y: usize| f64::from(alpha.get(y * CANVAS + x).copied().unwrap_or(0.0));
        let (from, to) = (CANVAS / 2 - 128, CANVAS / 2 + 128);
        let mut sum = 0.0;
        for y in from..to {
            for x in from..to {
                sum += (a(x + 1, y) - a(x, y)).powi(2) + (a(x, y + 1) - a(x, y)).powi(2);
            }
        }
        sum / ((to - from) * (to - from)) as f64
    }

    /// The Grunge grain on a big tile (1000 pixels, fixed to the canvas) under a round stamp 800
    /// pixels across: as brushes hold it magnified four times over, its flecks soft blobs; from
    /// its big picture it has its finer noise and crisp flecks, and it is as dark.
    #[test]
    fn grunge_on_a_big_tile_is_sharper() {
        let grunge = || {
            let mut b = Brush { name: "Grunge".to_owned(), ..Brush::round_pen() };
            b.hardness = 1.0;
            b.grain.image = Some(grain::GRUNGE);
            b.grain.depth = 1.0;
            b.grain.scale = 1000.0;
            b
        };
        let (Some(before), Some(after)) = (one_stamp(grunge(), 800.0, false), one_stamp(grunge(), 800.0, true)) else {
            return;
        };
        let mean = |a: &[f32]| a.iter().map(|&v| f64::from(v)).sum::<f64>() / a.len() as f64;
        let (steep_before, steep_after) = (steepness(&before), steepness(&after));
        let times = steep_after / steep_before;
        let (ink_before, ink_after) = (mean(&before), mean(&after));
        eprintln!("grunge on a 1000 px tile: steepness {steep_before:.5} as held, {steep_after:.5} big");
        eprintln!("({times:.1} times as steep); ink {ink_before:.4} and {ink_after:.4}");
        assert!(times > 2.0, "{steep_after} against {steep_before}");
        assert!((ink_after / ink_before - 1.0).abs() < 0.05, "{ink_after} against {ink_before}");
    }

    /// A stock brush's stand-in for a stock tip is the same picture big: averaged back down, within
    /// a few levels of the one brushes hold.
    #[test]
    fn a_stand_in_s_big_picture_is_the_same_picture() {
        let sets = crate::stock_library::shipped();
        let mut tried = 0;
        for brush in sets.iter().flat_map(|s| &s.brushes) {
            let Some(source) = brush.shape.source.as_ref() else { continue };
            if brush.shape.image != Some(shape::STOCK_GRAINY_SQUARE) || tried >= 3 {
                continue;
            }
            let Some(big) = shape_of(brush).filter(|b| b.side >= 1024) else { continue };
            let side = big.side as usize;
            let (drawn_side, pixels) = big.drawn().unwrap();
            assert_eq!(drawn_side as usize, side);
            let small = source.pixels.shared();
            let k = side / TEXTURE_SIZE;
            let mut apart = 0.0f64;
            for y in 0..TEXTURE_SIZE {
                for x in 0..TEXTURE_SIZE {
                    let sum: u32 = (0..k * k).map(|j| u32::from(pixels[(y * k + j / k) * side + x * k + j % k])).sum();
                    apart += (f64::from(sum) / (k * k) as f64 - f64::from(small[y * TEXTURE_SIZE + x])).abs();
                }
            }
            let apart = apart / (TEXTURE_SIZE * TEXTURE_SIZE) as f64;
            eprintln!("{}: big at {side}, averaged down {apart:.2} levels from its own", brush.name);
            assert!(apart < 12.0, "{}: {apart}", brush.name);
            tried += 1;
        }
        assert_eq!(tried, 3);
    }

    /// What the brushes the app comes with take to have their big pictures made ahead (a first
    /// start's work, on a thread of its own): how many, of which sides, how long each takes and in
    /// all, and how many bytes they are kept in, against how small they would pack (minutes: run
    /// with `--ignored`; `PICTURES_MOST` makes only that many).
    #[test]
    #[ignore]
    fn what_making_the_big_pictures_ahead_takes() {
        let sets = crate::stock_library::shipped();
        let mut all: Vec<(Big, bool)> = Vec::new();
        let mut reaching = 0;
        for brush in sets.iter().flat_map(|s| &s.brushes) {
            let reached = reached_by(brush);
            reaching += usize::from(!reached.is_empty());
            for big in reached {
                if !all.iter().any(|(b, _)| b.id == big.id) {
                    all.push((big, true));
                }
            }
            // those it has but doesn't reach at its most size (a brush made bigger in the studio)
            let second = brush.secondary.as_deref().map(|b| b.clone().as_second());
            for b in std::iter::once(brush).chain(second.as_ref()) {
                for big in [shape_of(b), grain_of(b)].into_iter().flatten() {
                    if !all.iter().any(|(a, _)| a.id == big.id) {
                        all.push((big, false));
                    }
                }
            }
        }
        let count = sets.iter().map(|s| s.brushes.len()).sum::<usize>();
        let reached: Vec<&Big> = all.iter().filter(|(_, r)| *r).map(|(b, _)| b).collect();
        let bytes = |bigs: &[&Big]| bigs.iter().map(|b| (b.side as usize).pow(2)).sum::<usize>();
        let unreached: Vec<&Big> = all.iter().filter(|(_, r)| !*r).map(|(b, _)| b).collect();
        eprintln!(
            "{count} brushes, {reaching} reach big pictures at their most size: {} pictures, {} MB; {} more not \
             reached, {} MB",
            reached.len(),
            bytes(&reached) >> 20,
            unreached.len(),
            bytes(&unreached) >> 20
        );
        for side in [512, 1024, 2048] {
            let n = reached.iter().filter(|b| b.side == side).count();
            eprintln!("  {n} at {side}");
        }
        let most: usize = std::env::var("PICTURES_MOST").ok().and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
        let started = Instant::now();
        let (mut made, mut raw, mut packed, mut slowest) = (0, 0, 0, Duration::ZERO);
        let (mut kept, mut packing, mut unpacking) = (0, Duration::ZERO, Duration::ZERO);
        for big in reached.iter().take(most) {
            let one = Instant::now();
            let Some((side, pixels)) = big.drawn() else { continue };
            slowest = slowest.max(one.elapsed());
            made += 1;
            raw += pixels.len();
            // as a PNG would pack it: each row less the one above, deflated
            let side = side as usize;
            let rows: Vec<u8> = (0..pixels.len())
                .map(|i| pixels[i].wrapping_sub(if i >= side { pixels[i - side] } else { 0 }))
                .collect();
            packed += miniz_oxide::deflate::compress_to_vec(&rows, 6).len();
            // as kept on disk
            let t = Instant::now();
            let lz4 = procraft_engine::applz4::encode(&pixels);
            packing += t.elapsed();
            let t = Instant::now();
            assert_eq!(procraft_engine::applz4::decode(&lz4, SHARPEST * SHARPEST).as_deref(), Some(&pixels[..]));
            unpacking += t.elapsed();
            kept += lz4.len();
        }
        let took = started.elapsed();
        eprintln!(
            "{made} made in {:.1} s ({} ms each on average, {} ms the slowest): {} MB of pixels, {} MB \
             as PNGs would pack them",
            took.as_secs_f64(),
            took.as_millis() / made.max(1) as u128,
            slowest.as_millis(),
            raw >> 20,
            packed >> 20
        );
        eprintln!(
            "kept as LZ4: {} MB ({} ms packing each, {} ms unpacking each on average)",
            kept >> 20,
            packing.as_millis() / made.max(1) as u128,
            unpacking.as_millis() / made.max(1) as u128
        );
    }

    /// The edge ramp of `alpha` (a canvas [`CANVAS`] square): the pixels between a tenth and nine
    /// tenths of its fullest over the length of its half contour (crossings, a quarter of pi of
    /// them), canvas pixels.
    fn edge_ramp(alpha: &[f32]) -> f32 {
        let top = alpha.iter().copied().fold(0.0f32, f32::max).max(1e-6);
        let at = |x: usize, y: usize| alpha.get(y * CANVAS + x).copied().unwrap_or(0.0) / top;
        let (mut between, mut crossings) = (0usize, 0usize);
        for y in 0..CANVAS {
            for x in 0..CANVAS {
                let v = at(x, y);
                between += usize::from(v > 0.1 && v < 0.9);
                crossings += usize::from(x + 1 < CANVAS && (v >= 0.5) != (at(x + 1, y) >= 0.5));
                crossings += usize::from(y + 1 < CANVAS && (v >= 0.5) != (at(x, y + 1) >= 0.5));
            }
        }
        between as f32 / (crossings as f32 * std::f32::consts::FRAC_PI_4).max(1.0)
    }

    /// The edge ramp of `alpha` (as [`edge_ramp`]) by its steepness where it crosses half its
    /// fullest: the median of eight tenths over the steepness there, the ramp a straight edge that
    /// steep would have (a texture inside it aside).
    fn edge_slope_ramp(alpha: &[f32]) -> f32 {
        let top = alpha.iter().copied().fold(0.0f32, f32::max).max(1e-6);
        let at = |x: usize, y: usize| alpha.get(y * CANVAS + x).copied().unwrap_or(0.0) / top;
        let mut ramps = Vec::new();
        for y in 1..CANVAS - 1 {
            for x in 1..CANVAS - 1 {
                let v = at(x, y);
                let crosses = ((v >= 0.5) != (at(x + 1, y) >= 0.5)) || ((v >= 0.5) != (at(x, y + 1) >= 0.5));
                let (gx, gy) = ((at(x + 1, y) - at(x - 1, y)) / 2.0, (at(x, y + 1) - at(x, y - 1)) / 2.0);
                let steep = gx.hypot(gy);
                if crosses && steep > 1e-6 {
                    ramps.push(0.8 / steep);
                }
            }
        }
        ramps.sort_by(f32::total_cmp);
        ramps.get(ramps.len() / 2).copied().unwrap_or(f32::NAN)
    }

    /// The stock brushes' stand-in for the stock shape under `key` (the first brush's that stands in
    /// for it) and a brush drawing with it alone; none if no brush does.
    fn stand_in_brush(key: u64) -> Option<(procraft_engine::brushfile::StandIn, Brush)> {
        let stand_in =
            crate::stock_brushes::SETS.iter().flat_map(|s| s.brushes).find_map(|b| b.shape.filter(|s| s.key == key))?;
        let (image, source, hardness) =
            crate::brush_files::shape_for(&procraft_engine::brushfile::Texture::Drawn(stand_in));
        let mut brush = Brush { name: "Ramp".to_owned(), ..Brush::round_pen() };
        brush.shape.image = image;
        brush.shape.source = source;
        if let Some(h) = hardness {
            brush.hardness = h;
        }
        Some((stand_in, brush))
    }

    /// One stamp of `brush` (as [`one_stamp`], its big pictures read) whose square is `square`
    /// canvas pixels: a round tip as big a share of it as a stock tip is.
    fn square_stamp(brush: &Brush, square: f32) -> Option<Vec<f32>> {
        square_stamp_at(brush, square, (0.0, 0.0))
    }

    /// [`square_stamp`] with its middle `offset` canvas pixels from the canvas's.
    fn square_stamp_at(brush: &Brush, square: f32, offset: (f32, f32)) -> Option<Vec<f32>> {
        let d = if brush.round_tip() { square * procraft_engine::brushfile::ROUND_TIP_SHARE } else { square };
        one_stamp_at(brush.clone(), d, true, offset)
    }

    /// The edge of `brush`'s stamp whose square is `square` canvas pixels: its ramp from a tenth
    /// to nine tenths ([`edge_ramp`]) as its middle lies on a pixel's corner, its edge's middle and
    /// between (a stamp a few pixels across shows them differently, by up to half a pixel), on
    /// average, at the least and at the most; the same by its slope ([`edge_slope_ramp`]) on
    /// average; and how far across it is at half ([`across_at_half`]). None without a GPU.
    fn edge_of(brush: &Brush, square: f32) -> Option<Edge> {
        let mut edge = Edge { ramp: 0.0, least: f32::MAX, most: 0.0, slope: 0.0, across: 0.0 };
        let offsets = [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5), (0.5, 0.5)];
        for offset in offsets {
            let alpha = square_stamp_at(brush, square, offset)?;
            let ramp = edge_ramp(&alpha);
            (edge.least, edge.most) = (edge.least.min(ramp), edge.most.max(ramp));
            edge.ramp += ramp / offsets.len() as f32;
            edge.slope += edge_slope_ramp(&alpha) / offsets.len() as f32;
            edge.across += across_at_half(&alpha) / offsets.len() as f32;
        }
        Some(edge)
    }

    /// What [`edge_of`] finds of a stamp's edge, canvas pixels.
    #[derive(Debug)]
    struct Edge {
        ramp: f32,
        least: f32,
        most: f32,
        slope: f32,
        across: f32,
    }

    /// How far across `alpha`'s stamp (a canvas [`CANVAS`] square) is where it paints half its
    /// fullest: the side of a disc as big.
    fn across_at_half(alpha: &[f32]) -> f32 {
        let top = alpha.iter().copied().fold(0.0f32, f32::max).max(1e-6);
        let inked = alpha.iter().filter(|&&v| v >= 0.5 * top).count();
        2.0 * (inked as f32 / std::f32::consts::PI).sqrt()
    }

    /// The stock hard round tip's key (what 17 of the brushes the app comes with stand in for).
    const HARD_TIP: u64 = 0xe114_279e_198b_3aec;

    /// The stock small point's key (a pen's and a pencil's).
    const SMALL_POINT: u64 = 0x79d5_ed1a_9f14_cee3;

    /// How far a stamp's quad reaches past its square each way (canvas pixels; `stamp.wgsl`): a
    /// shape's picture fills the quad.
    const QUAD_REACH: f32 = std::f32::consts::FRAC_1_SQRT_2;

    /// The stand-in for the stock hard round tip: our disc, read as a stamp reads a picture, its edge
    /// from a tenth to nine tenths about a pixel and a half at every size, as the stock tip's
    /// measures (1.57, 1.51, 1.42 and 1.55 pixels at 20, 60, 300 and 1000 across), where the round
    /// tip drawn by its hardness gave 0.56, 1.01, 3.91 and 12.4; and as wide as the stock tip's
    /// stamp, its disc as much of its quad as the stock tip's is of its square.
    #[test]
    fn a_hard_tip_s_edge_is_a_pixel_and_a_half_at_every_size() {
        let (_, brush) = stand_in_brush(HARD_TIP).unwrap();
        assert!(!brush.round_tip());
        let mut found = Vec::new();
        for (square, stock) in [(20.0, 1.57), (60.0, 1.51), (300.0, 1.42), (1000.0, 1.55)] {
            let Some(e) = edge_of(&brush, square) else { return };
            let stamp = textures::HARD_ROUND_SHARE * (square + 2.0 * QUAD_REACH);
            eprintln!(
                "hard tip {square} px: edge {:.2} px ({:.2} to {:.2}; {:.2} by slope; stock {stock}), {:.2} px across at half (stock stamp {stamp:.2})",
                e.ramp, e.least, e.most, e.slope, e.across
            );
            found.push((square, e, stamp));
        }
        assert!(found.iter().all(|(_, e, _)| (1.1..=2.0).contains(&e.ramp)), "{found:?}");
        assert!(found.iter().all(|(_, e, stamp)| (e.across - stamp).abs() < 0.75), "{found:?}");
    }

    /// The stand-in for the stock small point: our soft dot, its edge as wide as the stock point's
    /// at every size, within a tenth (3.82, 5.71, 16.44 and 54.36 pixels at 60, 100, 300 and 1000
    /// across; at 20, 1.70, within a third: the dot is 6 pixels across there, and its edge shows by
    /// a third differently as it lies between pixels), where the square of grain it fell to gave
    /// 13.4 at 100 and 130 at 1000; and it paints as much as the stock point holds (0.059 of its
    /// square), not the grain's square.
    #[test]
    fn the_small_point_s_edge_is_the_stock_point_s() {
        let (stand_in, brush) = stand_in_brush(SMALL_POINT).unwrap();
        assert_eq!(stand_in.pick, procraft_engine::brushfile::Pick::Point);
        let mut found = Vec::new();
        for (square, stock, within) in
            [(20.0, 1.70, 0.33), (60.0, 3.82, 0.1), (100.0, 5.71, 0.1), (300.0, 16.44, 0.1), (1000.0, 54.36, 0.1)]
        {
            let (Some(e), Some(alpha)) = (edge_of(&brush, square), square_stamp(&brush, square)) else { return };
            // its paint over its square's
            let ink = alpha.iter().map(|&v| f64::from(v)).sum::<f64>() / f64::from(square + 2.0 * QUAD_REACH).powi(2);
            eprintln!(
                "small point {square} px: edge {:.2} px ({:.2} to {:.2}; {:.2} by slope; stock {stock}), paint {ink:.4} of its square",
                e.ramp, e.least, e.most, e.slope
            );
            found.push((square, e.ramp / stock, within, ink));
        }
        assert!(found.iter().all(|f| (f.1 - 1.0).abs() <= f.2), "{found:?}");
        assert!(found.iter().all(|f| (f.3 / 0.0592 - 1.0).abs() < 0.08), "{found:?}");
    }

    /// The edge ramps of one stamp of the stock shapes `RAMP_KEYS` names (their keys, in hex,
    /// commas between) at a few sizes (`RAMP_SIZES`: the square a stamp of the stock image fills, canvas pixels;
    /// a round tip as big a share of it as the stock tip is), its grain off: what the pictures our
    /// stand-ins draw give, to set beside the stock images' own (measured apart).
    #[test]
    #[ignore]
    fn edge_ramps_of_stock_shapes() {
        let keys: Vec<u64> = std::env::var("RAMP_KEYS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|k| u64::from_str_radix(k.trim(), 16).ok())
            .collect();
        let sizes: Vec<f32> = std::env::var("RAMP_SIZES")
            .unwrap_or_else(|_| "100,300,600,1000".to_owned())
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        for key in keys {
            let Some((stand_in, brush)) = stand_in_brush(key) else {
                eprintln!("{key:016x}: none");
                continue;
            };
            let ramps: Vec<String> = sizes
                .iter()
                .map(|&d| edge_of(&brush, d).map_or("-".to_owned(), |e| format!("{:.2}/{:.2}", e.ramp, e.slope)))
                .collect();
            eprintln!("{key:016x} {:?}: {}", stand_in.pick, ramps.join("	"));
        }
    }
}
