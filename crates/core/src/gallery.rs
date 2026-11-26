//! The gallery's artworks, the same in every app: one folder each (see project.rs for what is
//! inside) in the app's artworks folder, some in stacks (a folder of them, named in stack.json).
//! Each folder keeps the order of what is in it in order.txt: new folders go first, newest first,
//! and keep their place from then on, so changing an artwork doesn't move it; only a drag does.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const ARTWORK_FILE: &str = "artwork.json";
const STACK_FILE: &str = "stack.json";
/// The folders' order, one name a line, first first.
const ORDER_FILE: &str = "order.txt";
/// A file an import keeps in the folder it is making an artwork in until it is done, saying which
/// run of the app (its process) makes it: the folder isn't an artwork until it goes, and one left by
/// a run that has ended (the app stopped mid-import) is taken away when the gallery next opens.
const IMPORTING: &str = "import.run";
const UNTITLED: &str = "Untitled Artwork";
const STACK: &str = "Stack";
/// The stack the gallery's recovery puts what it finds in.
const RECOVERED: &str = "Recovered Artworks";

/// Held while the gallery's order or its folders are changed, or its order read to be kept, by any
/// store in the app (the gallery's, and the home screen widget's reading it at the same moment):
/// one never writes over what another just kept with an older copy, nor takes away a stack another
/// is putting an artwork in. A thread holding it may take it again.
static KEEPING: parking_lot::ReentrantMutex<()> = parking_lot::const_reentrant_mutex(());

fn keeping() -> parking_lot::ReentrantMutexGuard<'static, ()> {
    KEEPING.lock()
}

/// A saved artwork: its folder, its name and size, when it was last changed (milliseconds since
/// 1970), and how many quarter turns clockwise it shows turned by (its thumbnail is kept so), as
/// its canvas was left.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct GalleryArtwork {
    pub dir: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub modified_ms: i64,
    pub turns: u32,
}

/// Something the gallery shows: an artwork, or a stack of them (in their own order).
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum GalleryEntry {
    Artwork { artwork: GalleryArtwork },
    Stack { dir: String, name: String, artworks: Vec<GalleryArtwork> },
}

impl GalleryEntry {
    fn dir(&self) -> &str {
        match self {
            GalleryEntry::Artwork { artwork } => &artwork.dir,
            GalleryEntry::Stack { dir, .. } => dir,
        }
    }
}

/// Under this wide a window the gallery's columns go by its width, not by which way it is turned.
const GALLERY_NARROW: f32 = 600.0;
/// The width a column takes in a narrow window, one column more than fit.
const GALLERY_COLUMN: f32 = 256.0;

/// How many columns the gallery's grid has in a window `width` wide (`landscape`: wider than
/// tall): three upright and four on its side; a narrower window (beside another, or a phone) one
/// more than fit 256 wide, so its artworks stay a size to see: two on a phone upright.
#[uniffi::export]
pub fn gallery_columns(width: f32, landscape: bool) -> u32 {
    if width < GALLERY_NARROW {
        (width / GALLERY_COLUMN).floor().max(0.0) as u32 + 1
    } else if landscape {
        4
    } else {
        3
    }
}

/// One of the buttons in the gallery's title row (in its capsule, or More's list).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GalleryAction {
    Select,
    Import,
    Photo,
    Stack,
    Preview,
    Share,
    Duplicate,
    Delete,
    /// The rest, in a list under it.
    More,
}

/// A button in the capsule: what it does, its name, how wide it is.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct GalleryButton {
    pub action: GalleryAction,
    pub label: String,
    pub width: f32,
}

/// The gallery's title row laid out for the window: the capsule's buttons (`more` holds those
/// that don't fit, in More's list), where the capsule's left edge is and how wide it is, and the
/// title's size as a share of its own (it shrinks to fit beside the capsule, never under 0.3) and
/// the most it may be wide (past that, at 0.3, it is cut short).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct GalleryHeaderFit {
    pub buttons: Vec<GalleryButton>,
    pub more: Vec<GalleryAction>,
    pub capsule_left: f32,
    pub capsule_width: f32,
    pub title_scale: f32,
    pub title_room: f32,
}

/// Under this wide a window, More alone holds every button.
const HEADER_NARROW: f32 = 440.0;
/// The room right of the title, + left out, that all the buttons need; and selecting.
const HEADER_ROOM_ALL: f32 = 300.0;
const HEADER_ROOM_ALL_SELECTING: f32 = 560.0;
/// The window's edge to + (or Select's round button that ends it), how wide that is, and the gap
/// before it (13 selecting); the title's gap before the capsule; how small the title may shrink.
const HEADER_END: f32 = 20.0;
const HEADER_ROUND: f32 = 44.0;
const HEADER_TITLE_GAP: f32 = 12.0;
const HEADER_TITLE_LEAST: f32 = 0.3;
/// More alone in the capsule (its glyph, no name).
pub const GALLERY_MORE_ALONE: f32 = 56.0;

fn button(action: GalleryAction) -> GalleryButton {
    let (label, width) = match action {
        GalleryAction::Select => ("Select", 69.0),
        GalleryAction::Import => ("Import", 71.0),
        GalleryAction::Photo => ("Photo", 66.0),
        GalleryAction::Stack => ("Stack", 65.0),
        GalleryAction::Preview => ("Preview", 80.0),
        GalleryAction::Share => ("Share", 65.0),
        GalleryAction::Duplicate => ("Duplicate", 91.0),
        GalleryAction::Delete => ("Delete", 70.0),
        GalleryAction::More => ("More", 61.0),
    };
    GalleryButton { action, label: label.to_owned(), width }
}

/// The gallery's title row in a window `width` wide (`landscape`: wider than tall): the title
/// starts `title_x` in and is `title_width` wide at its own size (36), `selecting` or not, in a
/// stack or not. With room, every button shows (selecting, all five only on a window on its side);
/// with less, some and More; under 440 wide, More alone. The title then shrinks to fit.
#[uniffi::export]
pub fn gallery_header_fit(
    width: f32,
    title_x: f32,
    title_width: f32,
    selecting: bool,
    landscape: bool,
    in_stack: bool,
) -> GalleryHeaderFit {
    use GalleryAction::*;
    let room = width - (title_x + title_width) - HEADER_ROUND;
    let all: Vec<GalleryAction> = if !selecting {
        vec![Select, Import, Photo]
    } else if in_stack {
        vec![Preview, Share, Duplicate, Delete]
    } else {
        vec![Stack, Preview, Share, Duplicate, Delete]
    };
    let (shown, more): (Vec<GalleryAction>, Vec<GalleryAction>) = if width < HEADER_NARROW {
        (vec![More], all)
    } else if (selecting && landscape && room >= HEADER_ROOM_ALL_SELECTING) || (!selecting && room >= HEADER_ROOM_ALL) {
        (all, Vec::new())
    } else if selecting {
        // More's list in its own order: Preview, Stack (none in a stack), Duplicate
        let more = if in_stack { vec![Preview, Duplicate] } else { vec![Preview, Stack, Duplicate] };
        (vec![Share, Delete, More], more)
    } else {
        (vec![Select, More], vec![Import, Photo])
    };
    let mut buttons: Vec<GalleryButton> = shown.into_iter().map(button).collect();
    if let [alone] = buttons.as_mut_slice()
        && alone.action == More
    {
        alone.width = GALLERY_MORE_ALONE;
    }
    let capsule_width: f32 = buttons.iter().map(|b| b.width).sum();
    let gap = if selecting { 13.0 } else { 12.0 };
    let capsule_left = width - HEADER_END - HEADER_ROUND - gap - capsule_width;
    let title_room = (capsule_left - HEADER_TITLE_GAP - title_x).max(0.0);
    let title_scale = if title_width > 0.0 { (title_room / title_width).clamp(HEADER_TITLE_LEAST, 1.0) } else { 1.0 };
    GalleryHeaderFit { buttons, more, capsule_left, capsule_width, title_scale, title_room }
}

/// How long a dragged artwork rests on a stack, or on the way back out of the stack open (its
/// title), before that opens with the drag going on, milliseconds.
pub const GALLERY_SPRING_MS: i64 = 600;

#[uniffi::export]
pub fn gallery_spring_ms() -> i64 {
    GALLERY_SPRING_MS
}

/// What a drag in the gallery rests on, for what opens when rested on long enough: a stack (by
/// folder), or "" the way back out of the stack open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragRest {
    /// What it rests on, since when (milliseconds), and whether that has opened already.
    on: Option<(String, i64, bool)>,
}

impl DragRest {
    /// The drag is over `on` (none: over nothing that opens) at `now_ms`: true the moment it has
    /// rested there `GALLERY_SPRING_MS`, once; moved off and back, it waits again.
    pub fn over(&mut self, on: Option<&str>, now_ms: i64) -> bool {
        let Some(on) = on else {
            self.on = None;
            return false;
        };
        match &mut self.on {
            Some((was, since, opened)) if was == on => {
                if !*opened && now_ms - *since >= GALLERY_SPRING_MS {
                    *opened = true;
                    return true;
                }
                false
            }
            _ => {
                self.on = Some((on.to_owned(), now_ms, false));
                false
            }
        }
    }

    /// What it rests on and has not opened yet.
    pub fn resting_on(&self) -> Option<&str> {
        self.on.as_ref().filter(|(_, _, opened)| !opened).map(|(on, _, _)| on.as_str())
    }

    /// How long until what it rests on opens (none: nothing to wait for).
    pub fn waiting(&self, now_ms: i64) -> Option<i64> {
        self.on
            .as_ref()
            .filter(|(_, _, opened)| !opened)
            .map(|(_, since, _)| (since + GALLERY_SPRING_MS - now_ms).max(0))
    }
}

/// What Select's actions act on, the same in every app.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct GalleryPick {
    /// The entries picked (folders), in the order shown: what Duplicate and Delete act on, a stack
    /// whole.
    pub dirs: Vec<String>,
    /// What Preview shows and Share shares: the artworks picked in the order shown, a picked
    /// stack's all of them in its own order.
    pub artworks: Vec<GalleryArtwork>,
    /// How many of those picked are stacks.
    pub stacks: u32,
    /// Stack puts two or more together (artworks or stacks); a stack has no Stack of its own.
    pub can_stack: bool,
    /// Delete asks first: its question and what it says.
    pub delete_title: String,
    pub delete_message: String,
}

fn artworks_told(n: usize) -> String {
    if n == 1 { "1 artwork".to_owned() } else { format!("{n} artworks") }
}

/// Of the `entries` shown (the gallery's, or `in_stack` a stack's), those `picked` (by folder) as
/// Select's actions take them; folders no longer shown are left out.
#[uniffi::export]
pub fn gallery_pick(entries: Vec<GalleryEntry>, picked: Vec<String>, in_stack: bool) -> GalleryPick {
    let chosen: Vec<&GalleryEntry> = entries.iter().filter(|e| picked.iter().any(|p| p == e.dir())).collect();
    let artworks: Vec<GalleryArtwork> = chosen
        .iter()
        .flat_map(|e| match e {
            GalleryEntry::Artwork { artwork } => vec![artwork.clone()],
            GalleryEntry::Stack { artworks, .. } => artworks.clone(),
        })
        .collect();
    let stacks: Vec<(&str, usize)> = chosen
        .iter()
        .filter_map(|e| match e {
            GalleryEntry::Stack { name, artworks, .. } => Some((name.as_str(), artworks.len())),
            GalleryEntry::Artwork { .. } => None,
        })
        .collect();
    let n = chosen.len();
    let undone = "This can't be undone.";
    let (delete_title, delete_message) = match stacks.as_slice() {
        [] => (format!("Delete {}?", artworks_told(n)), undone.to_owned()),
        [(name, count)] if n == 1 => {
            let go = if *count == 1 { "goes" } else { "go" };
            (format!("Delete \u{201c}{name}\u{201d}?"), format!("Its {} {go} with it. {undone}", artworks_told(*count)))
        }
        _ if stacks.len() == n => {
            (format!("Delete {n} stacks?"), format!("Their {} go with them. {undone}", artworks_told(artworks.len())))
        }
        _ => (
            format!("Delete {n} items?"),
            format!("The stacks' artworks go with them, {} in all. {undone}", artworks_told(artworks.len())),
        ),
    };
    GalleryPick {
        dirs: chosen.iter().map(|e| e.dir().to_owned()).collect(),
        artworks,
        stacks: stacks.len() as u32,
        can_stack: !in_stack && n >= 2,
        delete_title,
        delete_message,
    }
}

/// The order of the folders `present` (name and time changed), given the order `saved` before:
/// those not placed yet first, newest first, then the rest as they were.
fn arrange(present: &[(String, i64)], saved: &[String]) -> Vec<String> {
    let mut placed: Vec<String> = Vec::new();
    for name in saved {
        if present.iter().any(|(p, _)| p == name) && !placed.contains(name) {
            placed.push(name.clone());
        }
    }
    let mut new: Vec<&(String, i64)> = present.iter().filter(|(p, _)| !placed.contains(p)).collect();
    new.sort_by_key(|p| std::cmp::Reverse(p.1));
    new.into_iter().map(|(p, _)| p.clone()).chain(placed).collect()
}

/// `order` with `name` taken out and put back at `to` (its place in the order without it).
fn moved(order: &[String], name: &str, to: i64) -> Vec<String> {
    let mut rest: Vec<String> = order.iter().filter(|n| *n != name).cloned().collect();
    let at = to.clamp(0, rest.len() as i64) as usize;
    rest.insert(at, name.to_owned());
    rest
}

fn millis(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn modified(path: &Path) -> i64 {
    fs::metadata(path).and_then(|m| m.modified()).map_or(0, millis)
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// An import under way in a folder (see [`IMPORTING`]).
pub(crate) struct Importing(PathBuf);

impl Importing {
    /// An import into the folder `dir` begins (made if it isn't there).
    pub(crate) fn begin(dir: &Path) -> Importing {
        let mark = dir.join(IMPORTING);
        let _ = fs::create_dir_all(dir);
        if let Err(e) = crate::storage::write(&mark, std::process::id().to_string()) {
            log::warn!("noting an import: {e}");
        }
        Importing(mark)
    }

    /// The import is whole: its folder is an artwork. (One that fails leaves its note: its folder
    /// is taken away by the app, or else when the gallery next opens.) A note that can't be taken
    /// away at once (something else reading it a moment) is tried again: left, the folder would
    /// be hidden, then taken away.
    pub(crate) fn done(self) {
        let mut waits = [10, 50, 200].into_iter();
        loop {
            match fs::remove_file(&self.0) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    log::warn!("an import's note: {e}");
                    let Some(ms) = waits.next() else { return };
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                }
                _ => return,
            }
        }
    }
}

/// Takes away the folders of imports a run of the app that has ended left half made (it was stopped
/// during them): in the gallery and in its stacks.
fn sweep_imports(root: &Path) {
    let own = std::process::id().to_string();
    let inside = |d: &PathBuf| if d.join(STACK_FILE).is_file() { folders(d) } else { Vec::new() };
    let dirs: Vec<PathBuf> = folders(root).iter().flat_map(|d| std::iter::once(d.clone()).chain(inside(d))).collect();
    for dir in dirs {
        if let Ok(run) = fs::read_to_string(dir.join(IMPORTING))
            && run.trim() != own
        {
            log::warn!("an import cut short by the app stopping, taken away: {}", dir.display());
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

fn folders(at: &Path) -> Vec<PathBuf> {
    fs::read_dir(at).map_or_else(
        |_| Vec::new(),
        |entries| entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect(),
    )
}

/// A folder name no other has: 128 random bits written as a UUID.
fn fresh_name() -> String {
    use std::hash::{BuildHasher, Hasher};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let half = |k: u8| {
        // each RandomState is keyed afresh
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(now);
        h.write_u8(k);
        h.finish()
    };
    let (a, b) = (half(0), half(1));
    // the version (4) and variant bits of a random UUID
    let a = (a & !0xF000) | 0x4000;
    let b = (b & !(0xC << 60)) | (0x8 << 60);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xFFFF,
        a & 0xFFFF,
        b >> 48,
        b & 0xFFFF_FFFF_FFFF
    )
}

/// The folder `from` copied to `to`, the file named `last` in it (if there is one) after the rest.
fn copy_dir(from: &Path, to: &Path, last: Option<&str>) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target, None)?;
        } else if last.is_none_or(|last| entry.file_name() != last) {
            fs::copy(entry.path(), target)?;
        }
    }
    if let Some(last) = last.map(|l| from.join(l)).filter(|l| l.is_file()) {
        // whole or not at all: one cut short (the storage full) would be an artwork that can't be read
        let to = to.join(name_of(&last));
        let beside = to.with_extension("copying");
        fs::copy(&last, &beside).and_then(|_| fs::rename(&beside, &to)).inspect_err(|_| {
            let _ = fs::remove_file(&beside);
        })?;
    }
    Ok(())
}

/// The artwork in `from` copied to `to`, its own file last (a copy half made isn't an artwork in
/// the gallery yet) and as recent as the original's (not worked on yet). Noted as being made until
/// it is whole, as an import is: one the app stopped during is taken away when the gallery next
/// opens, not left taking room.
fn copy_artwork(from: &Path, to: &Path) -> std::io::Result<()> {
    let making = Importing::begin(to);
    // a copy that couldn't be finished (the storage full) goes whole: it would only take room
    copy_dir(from, to, Some(ARTWORK_FILE)).inspect_err(|e| {
        log::warn!("copying an artwork: {e}");
        let _ = fs::remove_dir_all(to);
    })?;
    crate::project::cut_short::step();
    let when = fs::metadata(from.join(ARTWORK_FILE)).and_then(|m| m.modified());
    if let (Ok(when), Ok(file)) = (when, fs::File::options().write(true).open(to.join(ARTWORK_FILE))) {
        let _ = file.set_modified(when);
    }
    making.done();
    Ok(())
}

fn read_artwork(dir: &Path) -> Option<GalleryArtwork> {
    // still being imported
    if dir.join(IMPORTING).exists() {
        return None;
    }
    let file = dir.join(ARTWORK_FILE);
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&file).ok()?).ok()?;
    let document = json.get("document")?;
    let side = |k: &str| document.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
    Some(GalleryArtwork {
        dir: dir.to_string_lossy().into_owned(),
        name: json.get("name").and_then(serde_json::Value::as_str).unwrap_or(UNTITLED).to_owned(),
        width: side("width"),
        height: side("height"),
        modified_ms: modified(&file),
        turns: json.get("turns").and_then(serde_json::Value::as_u64).map_or(0, |t| (t % 4) as u32),
    })
}

/// A first save of the artwork in `dir` cut short once all of it was written, put in place (see
/// `project::finish_first_save`): not one being imported. True if it was.
fn finish_first_save(dir: &Path) -> bool {
    let _keeping = keeping();
    !dir.join(IMPORTING).exists() && crate::project::finish_first_save(dir)
}

/// Mends an artwork folder whose artwork file is gone or unreadable from a whole copy a save cut
/// short left beside it (`artwork.<n>.tmp`, the newest that reads), the unreadable one kept as
/// `artwork.json.damaged`. True if it reads now.
fn mend_artwork(dir: &Path) -> bool {
    if read_artwork(dir).is_some() || (finish_first_save(dir) && read_artwork(dir).is_some()) {
        return true;
    }
    let whole = |p: &Path| {
        fs::read(p)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .is_some_and(|j| j.get("document").is_some())
    };
    let left: Option<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    let name = name_of(p);
                    name.starts_with("artwork.") && name.ends_with(".tmp") && p.is_file() && whole(p)
                })
                .max_by_key(|p| modified(p))
        })
        .ok()
        .flatten();
    let Some(left) = left else { return false };
    let file = dir.join(ARTWORK_FILE);
    if file.exists() {
        let _ = fs::rename(&file, dir.join(format!("{ARTWORK_FILE}.damaged")));
    }
    fs::rename(&left, &file).is_ok() && read_artwork(dir).is_some()
}

/// Keeps the order of what is in `at`, whole or not at all: each write beside the file under a name
/// of its own, so two written at once (the gallery and the home screen's widget each reading it)
/// can't cut each other's short or take the order away; one that fails leaves it as it was.
fn write_order(at: &Path, order: &[String]) {
    let text: String = order.iter().map(|n| format!("{n}\n")).collect();
    let kept = crate::project::write_atomically(&at.join(ORDER_FILE), |p| {
        crate::storage::write(p, &text).map_err(|e| crate::project::file_error("keeping the order", e))
    });
    if let Err(e) = kept {
        log::warn!("{e}");
    }
}

/// Names the stack in `dir` (written whole); false if it couldn't be.
fn write_stack_name(dir: &Path, name: &str) -> bool {
    let name = if name.trim().is_empty() { STACK } else { name.trim() };
    let text = serde_json::json!({ "name": name }).to_string();
    let named = crate::project::write_atomically(&dir.join(STACK_FILE), |p| {
        crate::storage::write(p, &text).map_err(|e| crate::project::file_error("naming the stack", e))
    });
    named.inspect_err(|e| log::warn!("{e}")).is_ok()
}

/// A folder in the gallery that is neither an artwork nor a stack but holds artworks: a stack
/// whose name file went (or never came: the storage full as it was made). Named again, so they
/// show; not one being made (a copy of a stack, noted until it is whole).
fn stack_lost_its_name(dir: &Path) -> bool {
    let _keeping = keeping();
    !dir.join(STACK_FILE).exists()
        && !dir.join(ARTWORK_FILE).exists()
        && !dir.join(IMPORTING).exists()
        && folders(dir).iter().any(|d| read_artwork(d).is_some())
        && write_stack_name(dir, STACK)
}

/// The artworks in the app's own storage.
#[derive(uniffi::Object)]
pub struct ArtworkStore {
    root: PathBuf,
}

impl ArtworkStore {
    fn at(&self, folder: Option<String>) -> PathBuf {
        folder.map_or_else(|| self.root.clone(), PathBuf::from)
    }

    fn parent_of(&self, dir: &Path) -> PathBuf {
        dir.parent().map_or_else(|| self.root.clone(), Path::to_path_buf)
    }

    fn read_stack(&self, dir: &Path) -> Option<GalleryEntry> {
        let file = dir.join(STACK_FILE);
        if !file.is_file() {
            return None;
        }
        let artworks: Vec<GalleryArtwork> = self
            .entries(dir)
            .into_iter()
            .filter_map(|e| match e {
                GalleryEntry::Artwork { artwork } => Some(artwork),
                GalleryEntry::Stack { .. } => None,
            })
            .collect();
        if artworks.is_empty() {
            // an empty stack goes away; not one holding an artwork being made (its folder would come
            // back without the stack's name file, hidden) or one that can't be read (recovery's)
            let _keeping = keeping();
            if folders(dir).is_empty() {
                let _ = fs::remove_dir_all(dir);
            }
            return None;
        }
        let json: Option<serde_json::Value> = fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let name = json.as_ref().and_then(|j| j.get("name")).and_then(serde_json::Value::as_str).unwrap_or(STACK);
        Some(GalleryEntry::Stack { dir: dir.to_string_lossy().into_owned(), name: name.to_owned(), artworks })
    }

    /// What is in `at` in its order; in the gallery, stacks too. What a run of the app that ended
    /// left whole but hidden is shown again: an artwork whose first save was cut short once all of
    /// it was written, and a stack whose name file went.
    fn entries(&self, at: &Path) -> Vec<GalleryEntry> {
        let dirs = folders(at);
        let top = at == self.root;
        let read: Vec<GalleryEntry> = dirs
            .iter()
            .filter_map(|d| {
                if let Some(artwork) = read_artwork(d) {
                    return Some(GalleryEntry::Artwork { artwork });
                }
                if top && let Some(stack) = self.read_stack(d) {
                    return Some(stack);
                }
                // (a stack with nothing to show is all it is)
                if d.join(STACK_FILE).is_file() {
                    return None;
                }
                if finish_first_save(d)
                    && let Some(artwork) = read_artwork(d)
                {
                    return Some(GalleryEntry::Artwork { artwork });
                }
                if top && stack_lost_its_name(d) { self.read_stack(d) } else { None }
            })
            .collect();
        let order = Self::order_with(at, &dirs, &read);
        order.iter().filter_map(|n| read.iter().find(|e| name_of(Path::new(e.dir())) == *n).cloned()).collect()
    }

    /// Every folder in `at` in order, those that can't be read this moment (being written) too.
    fn order(&self, at: &Path) -> Vec<String> {
        let dirs = folders(at);
        let read: Vec<GalleryEntry> =
            dirs.iter().filter_map(|d| read_artwork(d).map(|artwork| GalleryEntry::Artwork { artwork })).collect();
        Self::order_with(at, &dirs, &read)
    }

    /// The order of the folders `dirs` in `at` (`read` those read), kept up to date on disk.
    fn order_with(at: &Path, dirs: &[PathBuf], read: &[GalleryEntry]) -> Vec<String> {
        let _keeping = keeping();
        let saved: Vec<String> = fs::read_to_string(at.join(ORDER_FILE))
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect())
            .unwrap_or_default();
        // a folder being written keeps its place; if new, its folder's time places it
        let present: Vec<(String, i64)> = dirs
            .iter()
            .map(|d| {
                let when = read.iter().find(|e| Path::new(e.dir()) == d).map_or_else(
                    || modified(d),
                    |e| match e {
                        GalleryEntry::Artwork { artwork } => artwork.modified_ms,
                        GalleryEntry::Stack { artworks, .. } => {
                            artworks.iter().map(|a| a.modified_ms).max().unwrap_or_else(|| modified(d))
                        }
                    },
                );
                (name_of(d), when)
            })
            .collect();
        let order = arrange(&present, &saved);
        if order != saved {
            write_order(at, &order);
        }
        order
    }

    /// The artwork folder `new` takes the place of `old` in their folder's order (the two side by
    /// side, `old` about to go): an artwork brought up to date from a plugin's folder stays where it
    /// was in the gallery.
    pub(crate) fn put_in_place_of(&self, old: &Path, new: &Path) {
        let at = self.parent_of(old);
        let (old, new) = (name_of(old), name_of(new));
        let order: Vec<String> =
            self.order(&at).into_iter().filter(|n| *n != new).map(|n| if n == old { new.clone() } else { n }).collect();
        write_order(&at, &order);
    }

    /// A new stack in `at` holding `artworks` (moved into it), where `place` was: by default the
    /// first of them, and they keep the gallery's order; with `place`, in the order given.
    fn make_stack(&self, artworks: &[PathBuf], place: Option<&Path>) -> PathBuf {
        let _keeping = keeping();
        let before = self.order(&self.root);
        let names: Vec<String> = artworks.iter().map(|a| name_of(a)).collect();
        let inside: Vec<String> = if place.is_some() {
            names.clone()
        } else {
            before.iter().filter(|n| names.contains(n)).cloned().collect()
        };
        let first = place.map(name_of).or_else(|| inside.first().cloned());
        let at = first.and_then(|f| before.iter().position(|n| *n == f)).unwrap_or(0);
        let dir = self.root.join(fresh_name());
        // named first: artworks moved into a folder without its name file would be out of sight
        // (the storage full: no stack, the artworks left where they are)
        if fs::create_dir_all(&dir).is_err() || !write_stack_name(&dir, STACK) {
            let _ = fs::remove_dir_all(&dir);
            return dir;
        }
        for a in artworks {
            let _ = fs::rename(a, dir.join(name_of(a)));
        }
        write_order(&dir, &inside);
        let mut rest: Vec<String> = before.iter().filter(|n| !names.contains(n)).cloned().collect();
        rest.push(name_of(&dir));
        let to = before.iter().take(at).filter(|n| !names.contains(n)).count();
        write_order(&self.root, &moved(&rest, &name_of(&dir), to as i64));
        dir
    }
}

#[uniffi::export]
impl ArtworkStore {
    /// The artworks kept in `root` (made if it isn't there).
    #[uniffi::constructor]
    pub fn open(root: String) -> Arc<ArtworkStore> {
        let root = PathBuf::from(root);
        let _ = fs::create_dir_all(&root);
        sweep_imports(&root);
        Arc::new(ArtworkStore { root })
    }

    pub fn root(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    /// What is in `folder` (a stack's, or none for the gallery) in its order; empty stacks go
    /// away.
    pub fn list(&self, folder: Option<String>) -> Vec<GalleryEntry> {
        self.entries(&self.at(folder))
    }

    /// The artwork changed last, in the gallery or a stack (each is saved when it is left).
    pub fn most_recent(&self) -> Option<GalleryArtwork> {
        self.list(None)
            .into_iter()
            .flat_map(|e| match e {
                GalleryEntry::Artwork { artwork } => vec![artwork],
                GalleryEntry::Stack { artworks, .. } => artworks,
            })
            .max_by_key(|a| a.modified_ms)
    }

    /// A new, empty folder for an artwork in `folder` (a stack's, or none for the gallery). A stack
    /// gone meanwhile (emptied, deleted, the app holding on to it) isn't made again without its
    /// name file, which would hide the artwork: it goes in the gallery.
    pub fn new_folder(&self, folder: Option<String>) -> String {
        let _keeping = keeping();
        let at = self.at(folder.filter(|f| Path::new(f).join(STACK_FILE).is_file()));
        let dir = at.join(fresh_name());
        let _ = fs::create_dir_all(&dir);
        dir.to_string_lossy().into_owned()
    }

    pub fn delete(&self, dir: String) {
        let _ = fs::remove_dir_all(dir);
    }

    /// A copy of the artwork in `dir`, under the same name, right after it; a stack's copy holds
    /// a copy of each of its artworks, in its order. Returns its folder (none there if no copy could
    /// be made, the storage full: nothing half made is left).
    pub fn duplicate(&self, dir: String) -> String {
        let dir = PathBuf::from(dir);
        let at = self.parent_of(&dir);
        let copy = at.join(fresh_name());
        if dir.join(STACK_FILE).is_file() {
            // each artwork in a folder of its own name, so no two in the gallery share one; the
            // stack's own file last, and noted as being made until then: a copy half made isn't a
            // stack in the gallery yet (one the app stopped during is taken away when it next opens)
            let making = Importing::begin(&copy);
            let inside: Vec<String> = self
                .order(&dir)
                .iter()
                .filter(|name| read_artwork(&dir.join(name)).is_some())
                .filter_map(|name| {
                    let fresh = fresh_name();
                    copy_artwork(&dir.join(name), &copy.join(&fresh)).ok().map(|()| fresh)
                })
                .collect();
            write_order(&copy, &inside);
            let named = || {
                let file =
                    fs::read(dir.join(STACK_FILE)).map_err(|e| crate::project::file_error("naming the stack", e));
                file.and_then(|bytes| {
                    crate::project::write_atomically(&copy.join(STACK_FILE), |p| {
                        crate::storage::write(p, &bytes).map_err(|e| crate::project::file_error("naming the stack", e))
                    })
                })
                .inspect_err(|e| log::warn!("{e}"))
                .is_ok()
            };
            // none of them copied, or its name not kept (the storage full): no copy left
            if inside.is_empty() || !named() {
                let _ = fs::remove_dir_all(&copy);
                return copy.to_string_lossy().into_owned();
            }
            crate::project::cut_short::step();
            making.done();
        } else if copy_artwork(&dir, &copy).is_err() {
            return copy.to_string_lossy().into_owned();
        }
        // right after the original, in the order as it is now (not as it was before the copy, which
        // a long copy would write over what was kept meanwhile)
        let _keeping = keeping();
        let order: Vec<String> = self.order(&at).into_iter().filter(|n| *n != name_of(&copy)).collect();
        let place = order.iter().position(|n| *n == name_of(&dir)).map_or(0, |i| i + 1);
        write_order(&at, &moved(&order, &name_of(&copy), place as i64));
        copy.to_string_lossy().into_owned()
    }

    /// The artwork or stack in `dir` moved to `to` (its place in the order without it) in
    /// `into`: the gallery (none) or a stack's folder, by default the one it is in; from a stack
    /// to the gallery, it is taken out of it.
    pub fn move_entry(&self, dir: String, to: i64, into: Option<String>) {
        let _keeping = keeping();
        let dir = PathBuf::from(dir);
        let into = into.map_or_else(|| self.parent_of(&dir), PathBuf::from);
        if dir.parent() != Some(into.as_path()) && fs::rename(&dir, into.join(name_of(&dir))).is_err() {
            return;
        }
        write_order(&into, &moved(&self.order(&into), &name_of(&dir), to));
    }

    /// The artwork in `dir` (from the gallery) dropped onto `onto`: an artwork makes a stack with
    /// it there, this one first; a stack takes it in first.
    pub fn put_onto(&self, dir: String, onto: String) {
        let _keeping = keeping();
        let (dir, onto) = (PathBuf::from(dir), PathBuf::from(onto));
        if !onto.join(STACK_FILE).is_file() {
            self.make_stack(&[dir, onto.clone()], Some(&onto));
            return;
        }
        let inside = self.order(&onto);
        if fs::rename(&dir, onto.join(name_of(&dir))).is_err() {
            return;
        }
        let mut order = vec![name_of(&dir)];
        order.extend(inside);
        write_order(&onto, &order);
    }

    /// The artwork in `dir` named `name`.
    pub fn rename(&self, dir: String, name: String) {
        let file = Path::new(&dir).join(ARTWORK_FILE);
        let Some(mut json) = fs::read(&file).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        else {
            return;
        };
        if let Some(o) = json.as_object_mut() {
            o.insert("name".into(), name.trim().into());
            // written whole or not at all: cut short, the artwork couldn't be opened
            let text = json.to_string();
            let _ = crate::project::write_atomically(&file, |p| {
                crate::storage::write(p, &text).map_err(|e| crate::project::file_error("renaming the artwork", e))
            });
        }
    }

    /// A new stack in the gallery holding the artworks in `dirs` (moved into it), where the first
    /// of them was, in the gallery's order; returns its folder.
    pub fn stack(&self, dirs: Vec<String>) -> String {
        let artworks: Vec<PathBuf> = dirs.into_iter().map(PathBuf::from).collect();
        self.make_stack(&artworks, None).to_string_lossy().into_owned()
    }

    /// The entries in `dirs` (artworks and stacks of the gallery, as Select picks them) put
    /// together in one stack where the first of them was, holding all their artworks in the
    /// gallery's order (a stack's in its own). Stacks never nest, so the first stack picked takes
    /// in the rest and keeps its name; the others go once emptied. With no stack picked it is a
    /// new stack, as `stack` makes. Returns its folder.
    pub fn stack_picked(&self, dirs: Vec<String>) -> String {
        let _keeping = keeping();
        let order = self.order(&self.root);
        let wanted: Vec<String> =
            dirs.iter().map(PathBuf::from).filter(|d| self.parent_of(d) == self.root).map(|d| name_of(&d)).collect();
        let picked: Vec<PathBuf> = order.iter().filter(|n| wanted.contains(n)).map(|n| self.root.join(n)).collect();
        let Some(into) = picked.iter().find(|p| p.join(STACK_FILE).is_file()).cloned() else {
            return self.make_stack(&picked, None).to_string_lossy().into_owned();
        };
        // each moved whole or left where it is (a folder of that name there already, or a move
        // that failed): nothing is lost either way
        let take_in = |from: &Path| {
            let to = into.join(name_of(from));
            !to.exists() && fs::rename(from, &to).is_ok()
        };
        let mut inside: Vec<String> = Vec::new();
        for p in &picked {
            if *p == into {
                inside.extend(self.order(p));
            } else if p.join(STACK_FILE).is_file() {
                inside.extend(self.order(p).into_iter().filter(|name| take_in(&p.join(name))));
                // emptied: no artwork's folder left in it
                if folders(p).is_empty() {
                    let _ = fs::remove_dir_all(p);
                }
            } else if take_in(p) {
                inside.push(name_of(p));
            }
        }
        write_order(&into, &inside);
        // where the first picked was, the others out of the gallery's order
        let gone = |n: &String| picked.iter().any(|p| name_of(p) == *n && *p != into && !p.exists());
        let at = picked.first().and_then(|f| order.iter().position(|n| *n == name_of(f))).unwrap_or(0);
        let to = order.iter().take(at).filter(|n| !picked.iter().any(|p| name_of(p) == **n)).count();
        let rest: Vec<String> = order.iter().filter(|n| !gone(n)).cloned().collect();
        write_order(&self.root, &moved(&rest, &name_of(&into), to as i64));
        into.to_string_lossy().into_owned()
    }

    /// The stack in `dir` named `name` ("Stack" if it is empty).
    pub fn rename_stack(&self, dir: String, name: String) {
        write_stack_name(Path::new(&dir), &name);
    }

    /// Looks for artworks the gallery can't show and puts them in a new stack, "Recovered
    /// Artworks", first in the gallery: artworks in a folder that is neither an artwork nor a
    /// stack (a stack that lost its name file) or kept deeper than the gallery looks, and
    /// artworks whose file is gone or unreadable but which keep a whole one from a save cut short.
    /// Returns how many it found.
    pub fn recover_lost_artworks(&self) -> u32 {
        let _keeping = keeping();
        let shown: Vec<PathBuf> = self
            .list(None)
            .iter()
            .flat_map(|e| match e {
                GalleryEntry::Artwork { artwork } => vec![PathBuf::from(&artwork.dir)],
                GalleryEntry::Stack { artworks, .. } => artworks.iter().map(|a| PathBuf::from(&a.dir)).collect(),
            })
            .collect();
        // every folder down to three deep: one not shown that holds an artwork (mended if it can
        // be) is lost; the others are looked into
        let mut lost: Vec<PathBuf> = Vec::new();
        let mut look = vec![(self.root.clone(), 0)];
        while let Some((at, depth)) = look.pop() {
            for dir in folders(&at) {
                if !shown.contains(&dir) && mend_artwork(&dir) {
                    lost.push(dir);
                } else if depth < 2 {
                    look.push((dir, depth + 1));
                }
            }
        }
        if lost.is_empty() {
            return 0;
        }
        let stack = self.root.join(fresh_name());
        // named first: moved into a folder without its name file, they would be lost again
        if fs::create_dir_all(&stack).is_err() || !write_stack_name(&stack, RECOVERED) {
            let _ = fs::remove_dir_all(&stack);
            return 0;
        }
        let moved = lost.iter().filter(|a| fs::rename(a, stack.join(name_of(a))).is_ok()).count();
        // first in the gallery
        let order: Vec<String> = std::iter::once(name_of(&stack)).chain(self.order(&self.root)).collect();
        write_order(&self.root, &order);
        moved as u32
    }
}

/// What a zip file holds, told from its names alone (read from the file's end, so a big artwork
/// isn't read to find out): how Import takes it.
#[derive(uniffi::Enum, Debug, PartialEq, Eq)]
pub enum ZipKind {
    /// A model's artwork, whole.
    ModelArtwork,
    /// Not an archive that can be read.
    NotZip,
    Brushes,
    Swatches,
    /// A model, at `name` in the zip, its files beside it.
    Model {
        name: String,
    },
    /// An artwork file of ours, a layered package, or anything else.
    Other,
}

#[uniffi::export]
pub fn zip_file_kind(path: String) -> ZipKind {
    let Some(names) = procraft_engine::zip::names_in_file(Path::new(&path)) else { return ZipKind::NotZip };
    let names = || names.iter().map(String::as_str);
    if crate::brush_files::brush_names(names()) {
        ZipKind::Brushes
    } else if names().any(|n| n.rsplit('/').next() == Some(procraft_engine::palette::SWATCHES_ENTRY)) {
        ZipKind::Swatches
    } else if crate::scene3d::is_model_artwork(names()) {
        ZipKind::ModelArtwork
    } else if let Some(name) = crate::scene3d::model_name_among(names()) {
        ZipKind::Model { name }
    } else {
        ZipKind::Other
    }
}

/// Whether `name` is one of the files an artwork as one file holds (see [`unpack_artwork_file`]),
/// inside its folder.
fn artwork_file_entry(name: &str) -> bool {
    let inside =
        !name.contains('\\') && !name.contains(':') && name.split('/').all(|p| !p.is_empty() && p != "." && p != "..");
    inside
        && (name == ARTWORK_FILE
            || name == "thumbnail.png"
            || (name.starts_with("layers/") && name.ends_with(".png"))
            || (name.starts_with("timelapse/") && (name.ends_with(".jpg") || name == "timelapse/off"))
            || name == "about/photo.jpg"
            || name == "about/signature.json")
}

/// An artwork as one file, as early versions of the app shared it (its folder: the document, every
/// layer, the thumbnail and the time-lapse, in a zip at `path`), unzipped into the empty folder
/// `dir`: only the files an artwork has are taken, read an entry at a time, none outside the
/// folder. False if it isn't one (no `artwork.json` in it), or couldn't be written (nothing then
/// kept in `dir`). Still read; artworks are shared as packages now.
#[uniffi::export]
pub fn unpack_artwork_file(path: String, dir: String) -> bool {
    let Some(zip) = procraft_engine::zip::ZipReader::open(Path::new(&path)) else { return false };
    if !zip.names().any(|n| n == ARTWORK_FILE) {
        return false;
    }
    let root = Path::new(&dir);
    // the folder isn't an artwork until it is whole (taken away if the app stops meanwhile)
    let importing = Importing::begin(root);
    let wrote = zip.names().filter(|n| artwork_file_entry(n)).try_for_each(|name| {
        let bytes = zip.read(name).ok_or(())?;
        let to = root.join(name);
        to.parent().map_or(Ok(()), fs::create_dir_all).map_err(|_| ())?;
        fs::write(to, bytes).map_err(|_| ())
    });
    if wrote.is_err() {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let _ = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
            }
        }
        return false;
    }
    importing.done();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_artwork_file_unzips_only_an_artworks_files() {
        let dir = std::env::temp_dir().join(format!("procraft-artwork-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = |name: &str, entries: &[(&str, &str)]| {
            let mut w = procraft_engine::zip::ZipWriter::new();
            for (n, b) in entries {
                w.add(n, b.as_bytes());
            }
            let path = dir.join(name);
            fs::write(&path, w.finish()).unwrap();
            path.to_string_lossy().into_owned()
        };
        let old = file(
            "old.zip",
            &[
                ("artwork.json", "{}"),
                ("layers/1.png", "one"),
                ("layers/2.png", "two"),
                ("thumbnail.png", "thumb"),
                ("timelapse/0001.jpg", "frame"),
                ("../escaped.png", "no"),
                ("layers/../../up.png", "no"),
                ("notes.txt", "no"),
            ],
        );
        let into = dir.join("artwork");
        fs::create_dir_all(&into).unwrap();
        assert!(unpack_artwork_file(old, into.to_string_lossy().into_owned()));
        let mut kept: Vec<String> = Vec::new();
        for sub in ["", "layers", "timelapse"] {
            for e in fs::read_dir(into.join(sub)).unwrap().flatten() {
                if e.path().is_file() {
                    kept.push(format!("{sub}/{}", e.file_name().to_string_lossy()));
                }
            }
        }
        kept.sort();
        assert_eq!(kept, ["/artwork.json", "/thumbnail.png", "layers/1.png", "layers/2.png", "timelapse/0001.jpg"]);
        assert_eq!(fs::read(into.join("layers/2.png")).unwrap(), b"two");
        assert!(!dir.join("escaped.png").exists() && !dir.join("up.png").exists());
        // a package, or any other zip, isn't one: nothing written
        let package = file("package.zip", &[("Document.archive", "x"), ("1A2B/Chunk.lz4", "x")]);
        let empty = dir.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(!unpack_artwork_file(package, empty.to_string_lossy().into_owned()));
        assert_eq!(fs::read_dir(&empty).unwrap().count(), 0);
        assert!(!unpack_artwork_file(
            dir.join("missing.zip").to_string_lossy().into_owned(),
            empty.to_string_lossy().into_owned()
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_zip_file_is_told_by_its_names() {
        let dir = std::env::temp_dir().join(format!("procraft-zip-kind-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let zip = |file: &str, entries: &[&str]| {
            let mut w = procraft_engine::zip::ZipWriter::new();
            for e in entries {
                w.add(e, b"x");
            }
            let path = dir.join(file);
            fs::write(&path, w.finish()).unwrap();
            zip_file_kind(path.to_string_lossy().into_owned())
        };
        let kinds = [
            zip("set.zip", &["brushset.plist", "a/Brush.archive"]),
            zip("one.zip", &["b/Brush.archive", "b/Shape.png"]),
            zip("colours.zip", &["Swatches.json"]),
            zip("model.zip", &["__MACOSX/._chair.obj", "chair/chair.obj", "chair/chair.mtl"]),
            zip("art.zip", &["Document.archive", "chair.obj", "QuickLook/Thumbnail.png"]),
            zip("other.zip", &["notes.txt"]),
        ];
        fs::write(dir.join("plain.zip"), b"PK not really").unwrap();
        let plain = zip_file_kind(dir.join("plain.zip").to_string_lossy().into_owned());
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            kinds,
            [
                ZipKind::Brushes,
                ZipKind::Brushes,
                ZipKind::Swatches,
                ZipKind::Model { name: "chair/chair.obj".to_owned() },
                ZipKind::Other,
                ZipKind::Other,
            ]
        );
        assert_eq!(plain, ZipKind::NotZip);
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    fn present(v: &[(&str, i64)]) -> Vec<(String, i64)> {
        v.iter().map(|(n, t)| ((*n).to_owned(), *t)).collect()
    }

    #[test]
    fn with_no_order_yet_the_newest_come_first() {
        assert_eq!(arrange(&present(&[("a", 10), ("b", 30), ("c", 20)]), &[]), names(&["b", "c", "a"]));
    }

    #[test]
    fn a_changed_artwork_keeps_its_place_and_a_new_one_goes_first() {
        // "a" was changed last, but it was placed already; "d" is new
        let now = present(&[("a", 99), ("b", 30), ("c", 20), ("d", 50)]);
        assert_eq!(arrange(&now, &names(&["c", "a", "b"])), names(&["d", "c", "a", "b"]));
    }

    #[test]
    fn gone_folders_and_repeats_leave_the_order() {
        let now = present(&[("a", 1), ("b", 2)]);
        assert_eq!(arrange(&now, &names(&["b", "x", "a", "b"])), names(&["b", "a"]));
    }

    #[test]
    fn a_move_takes_the_entry_out_and_puts_it_back_at_its_new_place() {
        let order = names(&["a", "b", "c", "d"]);
        assert_eq!(moved(&order, "a", 2), names(&["b", "c", "a", "d"]));
        assert_eq!(moved(&order, "d", 0), names(&["d", "a", "b", "c"]));
        // past either end it goes to that end
        assert_eq!(moved(&order, "a", 99), names(&["b", "c", "d", "a"]));
        assert_eq!(moved(&order, "c", -1), names(&["c", "a", "b", "d"]));
        // a name not there yet is put in
        assert_eq!(moved(&order, "e", 1), names(&["a", "e", "b", "c", "d"]));
    }

    #[test]
    fn fresh_names_differ() {
        let (a, b) = (fresh_name(), fresh_name());
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
    }

    fn artwork(store: &ArtworkStore, name: &str, folder: Option<String>) -> String {
        let dir = store.new_folder(folder);
        let json = serde_json::json!({ "name": name, "document": { "width": 64, "height": 32 } });
        fs::write(Path::new(&dir).join(ARTWORK_FILE), json.to_string()).unwrap();
        dir
    }

    fn shown(store: &ArtworkStore, folder: Option<String>) -> Vec<String> {
        store
            .list(folder)
            .into_iter()
            .map(|e| match e {
                GalleryEntry::Artwork { artwork } => artwork.name,
                GalleryEntry::Stack { name, artworks, .. } => {
                    format!("{name}[{}]", artworks.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(","))
                }
            })
            .collect()
    }

    /// [`shown`] with each stack's artworks in name order (two made in the same moment have no
    /// order of their own), the entries too.
    fn shown_sorted(store: &ArtworkStore) -> Vec<String> {
        let mut entries: Vec<String> = store
            .list(None)
            .into_iter()
            .map(|e| match e {
                GalleryEntry::Artwork { artwork } => artwork.name,
                GalleryEntry::Stack { name, artworks, .. } => {
                    let mut names: Vec<&str> = artworks.iter().map(|a| a.name.as_str()).collect();
                    names.sort_unstable();
                    format!("{name}[{}]", names.join(","))
                }
            })
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn recovery_stacks_artworks_the_gallery_cannot_show() {
        let root = std::env::temp_dir().join(format!("procraft-recover-{}-{}", std::process::id(), fresh_name()));
        let store = ArtworkStore::open(root.to_string_lossy().into_owned());
        artwork(&store, "Shown", None);
        // nothing lost: nothing done
        assert_eq!(store.recover_lost_artworks(), 0);
        assert_eq!(shown(&store, None), ["Shown"]);
        // whole but hidden, they show again with no recovery: a stack that lost its name file, and
        // a first save cut short (no artwork file, a whole one beside it all it left)
        let stack = store.stack(vec![artwork(&store, "A", None), artwork(&store, "B", None)]);
        fs::remove_file(Path::new(&stack).join(STACK_FILE)).unwrap();
        let cut = store.new_folder(None);
        let json = serde_json::json!({ "name": "Cut", "document": { "width": 8, "height": 8 } });
        fs::write(Path::new(&cut).join("artwork.3.tmp"), json.to_string()).unwrap();
        // one kept inside a stack's artwork, too deep to be shown
        let kept = store.stack(vec![artwork(&store, "Outer", None)]);
        let outer = store.list(Some(kept.clone()))[0].dir().to_owned();
        // (made there by hand: the store makes new folders only in the gallery and its stacks)
        let deep = Path::new(&outer).join(fresh_name());
        fs::create_dir_all(&deep).unwrap();
        let json = serde_json::json!({ "name": "Deep", "document": { "width": 8, "height": 8 } });
        fs::write(deep.join(ARTWORK_FILE), json.to_string()).unwrap();
        // one whose artwork file can't be read, a whole one from a save cut short beside it
        let damaged = artwork(&store, "Damaged", None);
        let whole = fs::read(Path::new(&damaged).join(ARTWORK_FILE)).unwrap();
        fs::write(Path::new(&damaged).join("artwork.json.7.tmp"), whole).unwrap();
        fs::write(Path::new(&damaged).join(ARTWORK_FILE), "{\"name\": \"Dam").unwrap();
        // a save cut short mid-write leaves a part of one: nothing to mend
        let torn = store.new_folder(None);
        fs::write(Path::new(&torn).join("artwork.4.tmp"), "{\"name\": \"Tor").unwrap();
        assert_eq!(shown_sorted(&store), ["Cut", "Shown", "Stack[A,B]", "Stack[Outer]"]);
        assert_eq!(store.recover_lost_artworks(), 2);
        let after = shown(&store, None);
        assert_eq!(after.len(), 5);
        // the stack first, holding them
        assert!(after[0].starts_with("Recovered Artworks["), "{after:?}");
        let mut found: Vec<&str> =
            after[0].trim_start_matches("Recovered Artworks[").trim_end_matches(']').split(',').collect();
        found.sort_unstable();
        assert_eq!(found, ["Damaged", "Deep"]);
        // the unreadable file kept aside
        let recovered = store.list(None).into_iter().find_map(|e| match e {
            GalleryEntry::Stack { artworks, .. } => artworks.into_iter().find(|a| a.name == "Damaged"),
            GalleryEntry::Artwork { .. } => None,
        });
        assert!(Path::new(&recovered.unwrap().dir).join("artwork.json.damaged").is_file());
        assert!(Path::new(&torn).join("artwork.4.tmp").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn gallery_work_at_random_loses_no_artwork_and_never_nests_a_stack() {
        for seed in 0u64..20 {
            let root =
                std::env::temp_dir().join(format!("procraft-gallery-fuzz-{}-{}", std::process::id(), fresh_name()));
            let store = ArtworkStore::open(root.to_string_lossy().into_owned());
            let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = |n: usize| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state % n.max(1) as u64) as usize
            };
            // every artwork's name, as many times as there are artworks of it
            let mut names: Vec<String> = Vec::new();
            let mut made = 0;
            let mut done: Vec<usize> = Vec::new();
            for _ in 0..40 {
                let gallery = store.list(None);
                let stacks: Vec<(String, Vec<GalleryArtwork>)> = gallery
                    .iter()
                    .filter_map(|e| match e {
                        GalleryEntry::Stack { dir, artworks, .. } => Some((dir.clone(), artworks.clone())),
                        GalleryEntry::Artwork { .. } => None,
                    })
                    .collect();
                let loose: Vec<String> = gallery
                    .iter()
                    .filter_map(|e| match e {
                        GalleryEntry::Artwork { artwork } => Some(artwork.dir.clone()),
                        GalleryEntry::Stack { .. } => None,
                    })
                    .collect();
                let stacked: Vec<String> = stacks.iter().flat_map(|(_, a)| a.iter().map(|x| x.dir.clone())).collect();
                let all: Vec<String> = loose.iter().chain(&stacked).cloned().collect();
                let op = next(10);
                done.push(op);
                match op {
                    // a new artwork, in the gallery or in a stack
                    0 | 1 => {
                        made += 1;
                        let name = format!("a{made}");
                        let folder =
                            if next(3) == 0 { stacks.get(next(stacks.len())).map(|s| s.0.clone()) } else { None };
                        artwork(&store, &name, folder);
                        names.push(name);
                    }
                    2 if !all.is_empty() => {
                        let dir = &all[next(all.len())];
                        let name = name_in(dir);
                        store.duplicate(dir.clone());
                        names.push(name);
                    }
                    3 if !all.is_empty() => {
                        let dir = &all[next(all.len())];
                        let name = name_in(dir);
                        store.delete(dir.clone());
                        let at = names.iter().position(|n| *n == name).unwrap();
                        names.remove(at);
                    }
                    // moved within the gallery or within its stack
                    4 if !gallery.is_empty() => {
                        let dir = gallery[next(gallery.len())].dir().to_owned();
                        store.move_entry(dir, next(gallery.len() + 1) as i64, None);
                    }
                    5 if !stacked.is_empty() => {
                        let dir = stacked[next(stacked.len())].clone();
                        let to = if next(2) == 0 { Some(store.root()) } else { None };
                        store.move_entry(dir, next(gallery.len() + 2) as i64, to);
                    }
                    // an artwork of the gallery dropped onto another of its entries
                    6 if !loose.is_empty() && gallery.len() > 1 => {
                        let dir = loose[next(loose.len())].clone();
                        let onto = gallery[next(gallery.len())].dir().to_owned();
                        if onto != dir {
                            store.put_onto(dir, onto);
                        }
                    }
                    7 if loose.len() > 1 => {
                        let picked: Vec<String> = loose.iter().filter(|_| next(2) == 0).cloned().collect();
                        if !picked.is_empty() {
                            store.stack(picked);
                        }
                    }
                    8 if !all.is_empty() => {
                        let dir = &all[next(all.len())];
                        let old = name_in(dir);
                        made += 1;
                        let name = format!("a{made}");
                        store.rename(dir.clone(), name.clone());
                        let at = names.iter().position(|n| *n == old).unwrap();
                        names[at] = name;
                    }
                    9 if !stacks.is_empty() => store.rename_stack(stacks[next(stacks.len())].0.clone(), "S".into()),
                    _ => {}
                }
                // what is listed is every artwork there is, each once, and no stack empty
                let mut listed: Vec<String> = Vec::new();
                let mut dirs: Vec<String> = Vec::new();
                for e in store.list(None) {
                    match e {
                        GalleryEntry::Artwork { artwork } => {
                            listed.push(artwork.name);
                            dirs.push(artwork.dir);
                        }
                        GalleryEntry::Stack { artworks, .. } => {
                            assert!(!artworks.is_empty(), "seed {seed}: an empty stack after {done:?}");
                            for a in artworks {
                                listed.push(a.name);
                                dirs.push(a.dir);
                            }
                        }
                    }
                }
                let mut want = names.clone();
                want.sort();
                listed.sort();
                assert_eq!(listed, want, "seed {seed}: after {done:?}");
                // on disk: each artwork's folder listed once, no stack within a stack
                let mut on_disk: Vec<String> = Vec::new();
                for entry in fs::read_dir(&root).unwrap().flatten() {
                    let path = entry.path();
                    if path.join(ARTWORK_FILE).is_file() {
                        on_disk.push(path.to_string_lossy().into_owned());
                    } else if path.join(STACK_FILE).is_file() {
                        for inner in fs::read_dir(&path).unwrap().flatten() {
                            let inner = inner.path();
                            assert!(
                                !inner.join(STACK_FILE).is_file(),
                                "seed {seed}: a stack in a stack after {done:?}"
                            );
                            if inner.join(ARTWORK_FILE).is_file() {
                                on_disk.push(inner.to_string_lossy().into_owned());
                            }
                        }
                    }
                }
                on_disk.sort();
                dirs.sort();
                let canon = |v: &[String]| {
                    v.iter().map(|d| PathBuf::from(d).components().collect::<PathBuf>()).collect::<Vec<_>>()
                };
                assert_eq!(canon(&dirs), canon(&on_disk), "seed {seed}: after {done:?}");
            }
            let _ = fs::remove_dir_all(&root);
        }
    }

    /// A gallery holding, in this order: A, "One"[B, C], D, "Two"[E]; and their folders by name.
    fn mixed_gallery() -> (PathBuf, Arc<ArtworkStore>, HashMap<&'static str, String>) {
        let root = std::env::temp_dir().join(format!("procraft-pick-{}-{}", std::process::id(), fresh_name()));
        let store = ArtworkStore::open(root.to_string_lossy().into_owned());
        let mut dirs = HashMap::new();
        for name in ["A", "B", "C", "D", "E"] {
            dirs.insert(name, artwork(&store, name, None));
        }
        let one = store.stack(vec![dirs["B"].clone(), dirs["C"].clone()]);
        store.rename_stack(one.clone(), "One".into());
        let two = store.stack(vec![dirs["E"].clone()]);
        store.rename_stack(two.clone(), "Two".into());
        for (i, d) in [&dirs["A"], &one, &dirs["D"], &two].into_iter().enumerate() {
            store.move_entry(d.clone(), i as i64, None);
        }
        // B and C in that order in their stack
        let inside: Vec<String> = store.list(Some(one.clone())).iter().map(|e| e.dir().to_owned()).collect();
        let b = inside.iter().find(|d| name_in(d) == "B").unwrap().clone();
        store.move_entry(b, 0, None);
        dirs.insert("One", one);
        dirs.insert("Two", two);
        assert_eq!(shown(&store, None), names(&["A", "One[B,C]", "D", "Two[E]"]));
        (root, store, dirs)
    }

    use std::collections::HashMap;

    #[test]
    fn select_takes_stacks_and_artworks_together() {
        let (root, store, dirs) = mixed_gallery();
        let entries = store.list(None);
        let pick = |p: &[&str], in_stack: bool| {
            gallery_pick(entries.clone(), p.iter().map(|n| dirs[n].clone()).collect(), in_stack)
        };
        let artwork_names = |p: &GalleryPick| p.artworks.iter().map(|a| a.name.clone()).collect::<Vec<_>>();
        // picked in any order, they act in the order shown, a stack's artworks all of them
        let both = pick(&["Two", "A", "One"], false);
        assert_eq!(both.dirs, [dirs["A"].clone(), dirs["One"].clone(), dirs["Two"].clone()]);
        assert_eq!(artwork_names(&both), names(&["A", "B", "C", "E"]));
        assert_eq!(both.stacks, 2);
        assert!(both.can_stack);
        assert_eq!(both.delete_title, "Delete 3 items?");
        assert_eq!(both.delete_message, "The stacks' artworks go with them, 4 artworks in all. This can't be undone.");
        // a stack and an artwork can be stacked; one alone can't, nor anything inside a stack
        assert!(pick(&["One", "D"], false).can_stack);
        assert!(!pick(&["One"], false).can_stack);
        assert!(!pick(&["A", "D"], true).can_stack);
        // what Delete asks
        let one = pick(&["One"], false);
        assert_eq!(
            (one.delete_title.as_str(), one.delete_message.as_str()),
            ("Delete \u{201c}One\u{201d}?", "Its 2 artworks go with it. This can't be undone.")
        );
        let two = pick(&["Two"], false);
        assert_eq!(two.delete_message, "Its 1 artwork goes with it. This can't be undone.");
        let stacks = pick(&["One", "Two"], false);
        assert_eq!(
            (stacks.delete_title.as_str(), stacks.delete_message.as_str()),
            ("Delete 2 stacks?", "Their 3 artworks go with them. This can't be undone.")
        );
        assert_eq!(pick(&["D"], false).delete_title, "Delete 1 artwork?");
        assert_eq!(pick(&["A", "D"], false).delete_title, "Delete 2 artworks?");
        // a folder no longer shown is left out
        let mut gone = vec![dirs["A"].clone(), root.join("not-there").to_string_lossy().into_owned()];
        gone.push(dirs["D"].clone());
        assert_eq!(gallery_pick(entries.clone(), gone, false).dirs.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stack_with_stacks_picked_merges_into_the_first_stack_where_the_first_was() {
        let (root, store, dirs) = mixed_gallery();
        let picked = vec![dirs["Two"].clone(), dirs["D"].clone(), dirs["One"].clone(), dirs["A"].clone()];
        let into = store.stack_picked(picked);
        // the first stack picked takes in the rest in the order shown, keeping its name, where A
        // (the first picked) was; the emptied stack is gone
        assert_eq!(Path::new(&into), Path::new(&dirs["One"]));
        assert_eq!(shown(&store, None), names(&["One[A,B,C,D,E]"]));
        assert!(!Path::new(&dirs["Two"]).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stack_with_picks_around_others_keeps_the_others_in_their_places() {
        let (root, store, dirs) = mixed_gallery();
        // D and Two: Two takes D in, where D was
        store.stack_picked(vec![dirs["D"].clone(), dirs["Two"].clone()]);
        assert_eq!(shown(&store, None), names(&["A", "One[B,C]", "Two[D,E]"]));
        // two stacks: the first takes the other's artworks after its own
        store.stack_picked(vec![dirs["Two"].clone(), dirs["One"].clone()]);
        assert_eq!(shown(&store, None), names(&["A", "One[B,C,D,E]"]));
        // only artworks: a new stack, as before
        let e = store.list(None);
        let lone = artwork(&store, "F", None);
        store.move_entry(lone.clone(), 2, None);
        let made = store.stack_picked(vec![lone, e[0].dir().to_owned()]);
        assert_ne!(Path::new(&made), Path::new(&dirs["One"]));
        assert_eq!(shown(&store, None), names(&["Stack[A,F]", "One[B,C,D,E]"]));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stack_never_overwrites_an_artwork_already_in_the_stack() {
        let (root, store, dirs) = mixed_gallery();
        // a folder in "One" of the same name as A's (no two should share one, but if they did)
        let clash = Path::new(&dirs["One"]).join(name_of(Path::new(&dirs["A"])));
        fs::create_dir_all(&clash).unwrap();
        let json = serde_json::json!({ "name": "Clash", "document": { "width": 8, "height": 8 } });
        fs::write(clash.join(ARTWORK_FILE), json.to_string()).unwrap();
        store.stack_picked(vec![dirs["A"].clone(), dirs["One"].clone()]);
        // A stays where it was, whole; the one in the stack is untouched
        assert_eq!(name_in(&dirs["A"]), "A");
        assert_eq!(name_in(&clash.to_string_lossy()), "Clash");
        let mut all: Vec<String> = shown(&store, None);
        all.sort();
        assert_eq!(all, names(&["A", "D", "One[Clash,B,C]", "Two[E]"]));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_drag_resting_on_the_way_back_or_a_stack_opens_it_once() {
        let mut rest = DragRest::default();
        // on the way back: nothing until it has rested there long enough, then once
        assert!(!rest.over(Some(""), 1000));
        assert_eq!(rest.resting_on(), Some(""));
        assert_eq!(rest.waiting(1100), Some(GALLERY_SPRING_MS - 100));
        assert!(!rest.over(Some(""), 1000 + GALLERY_SPRING_MS - 1));
        assert!(rest.over(Some(""), 1000 + GALLERY_SPRING_MS));
        assert!(!rest.over(Some(""), 1000 + GALLERY_SPRING_MS * 3));
        assert_eq!(rest.resting_on(), None);
        // moved onto a stack it waits again; off everything, it forgets
        assert!(!rest.over(Some("stack"), 5000));
        assert!(!rest.over(None, 5300));
        assert!(!rest.over(Some("stack"), 5400));
        assert!(!rest.over(Some("stack"), 5400 + GALLERY_SPRING_MS - 1));
        assert!(rest.over(Some("stack"), 5400 + GALLERY_SPRING_MS));
        // from one to another, the time starts again
        assert!(!rest.over(Some("a"), 0));
        assert!(!rest.over(Some("b"), GALLERY_SPRING_MS));
        assert!(rest.over(Some("b"), GALLERY_SPRING_MS * 2));
    }

    #[test]
    fn an_artwork_taken_out_of_its_stack_goes_where_it_is_let_go() {
        let (root, store, dirs) = mixed_gallery();
        let in_stack = |name: &str| -> String {
            store
                .list(Some(dirs["One"].clone()))
                .iter()
                .map(|e| e.dir().to_owned())
                .find(|d| name_in(d) == name)
                .unwrap()
        };
        // let go between D and Two in the gallery: out of its stack, there, whole
        let b = in_stack("B");
        store.move_entry(b.clone(), 3, Some(store.root()));
        assert_eq!(shown(&store, None), names(&["A", "One[C]", "D", "B", "Two[E]"]));
        let now = root.join(name_of(Path::new(&b)));
        assert_eq!(name_in(&now.to_string_lossy()), "B");
        assert!(!Path::new(&b).exists());
        // let go on the way back: to the gallery's front; the emptied stack goes
        store.move_entry(in_stack("C"), 0, Some(store.root()));
        assert_eq!(shown(&store, None), names(&["C", "A", "D", "B", "Two[E]"]));
        assert!(!Path::new(&dirs["One"]).exists());
        let _ = fs::remove_dir_all(&root);
    }

    fn actions(fit: &GalleryHeaderFit) -> Vec<GalleryAction> {
        fit.buttons.iter().map(|b| b.action).collect()
    }

    /// "ProCraft" at its own size, about as wide as it is drawn.
    const TITLE: f32 = 150.0;

    #[test]
    fn a_tablet_title_row_is_laid_out_as_it_always_was() {
        use GalleryAction::*;
        // tablets upright and on their side: Select, Import and Photo, the title at its size; the
        // capsule 12 left of + (44, 20 in)
        for (width, landscape) in [(640.0, false), (744.0, false), (800.0, false), (1024.0, true), (1280.0, true)] {
            let fit = gallery_header_fit(width, 32.0, TITLE, false, landscape, false);
            assert_eq!(actions(&fit), [Select, Import, Photo], "{width}");
            assert!(fit.more.is_empty());
            assert_eq!(fit.capsule_width, 69.0 + 71.0 + 66.0);
            assert_eq!(fit.capsule_left, width - 20.0 - 44.0 - 12.0 - fit.capsule_width);
            assert_eq!(fit.title_scale, 1.0, "{width}");
            // selecting: on its side Stack, Preview, Share, Duplicate and Delete; upright Share,
            // Delete and More (Preview, Stack, Duplicate); the round button 13 right of them
            let picking = gallery_header_fit(width, 32.0, TITLE, true, landscape, false);
            if landscape {
                assert_eq!(actions(&picking), [Stack, Preview, Share, Duplicate, Delete], "{width}");
                assert!(picking.more.is_empty());
            } else {
                assert_eq!(actions(&picking), [Share, Delete, More], "{width}");
                assert_eq!(picking.more, [Preview, Stack, Duplicate]);
            }
            assert_eq!(picking.capsule_left, width - 20.0 - 44.0 - 13.0 - picking.capsule_width);
            assert_eq!(picking.title_scale, 1.0, "{width}");
        }
        // in a stack, selecting: no Stack, in the row or under More
        let fit = gallery_header_fit(1024.0, 60.0, TITLE, true, true, true);
        assert_eq!(actions(&fit), [Preview, Share, Duplicate, Delete]);
        let fit = gallery_header_fit(744.0, 60.0, TITLE, true, false, true);
        assert_eq!(fit.more, [Preview, Duplicate]);
    }

    #[test]
    fn a_narrow_window_keeps_the_title_on_one_line_beside_fewer_buttons() {
        use GalleryAction::*;
        // a phone upright (411 wide): More alone holds them all; the title fits at its size
        let fit = gallery_header_fit(411.0, 32.0, TITLE, false, false, false);
        assert_eq!(actions(&fit), [More]);
        assert_eq!(fit.more, [Select, Import, Photo]);
        assert_eq!(fit.capsule_width, GALLERY_MORE_ALONE);
        assert_eq!(fit.title_scale, 1.0);
        assert!(32.0 + TITLE + 12.0 <= fit.capsule_left);
        let picking = gallery_header_fit(411.0, 32.0, TITLE, true, false, false);
        assert_eq!(actions(&picking), [More]);
        assert_eq!(picking.more, [Stack, Preview, Share, Duplicate, Delete]);
        // a window too narrow to give the room 300: Select and More (Import, Photo)
        let fit = gallery_header_fit(500.0, 32.0, TITLE, false, false, false);
        assert_eq!(actions(&fit), [Select, More]);
        assert_eq!(fit.more, [Import, Photo]);
        assert!(fit.title_room >= TITLE);
        // on its side but not 560 of room: Share, Delete and More
        let picking = gallery_header_fit(700.0, 32.0, TITLE, true, true, false);
        assert_eq!(actions(&picking), [Share, Delete, More]);
        // a long stack name shrinks to fit beside the buttons, no smaller than 0.3, and the room
        // it gets never runs under the capsule
        for width in [300.0, 360.0, 411.0, 500.0, 640.0, 900.0] {
            for selecting in [false, true] {
                let fit = gallery_header_fit(width, 60.0, 900.0, selecting, width > 600.0, true);
                assert!(fit.title_scale >= 0.3 && fit.title_scale <= 1.0);
                assert!(60.0 + fit.title_room + 12.0 <= fit.capsule_left + 0.01 || fit.title_room == 0.0);
                if fit.title_scale > 0.3 && fit.title_scale < 1.0 {
                    assert!((900.0 * fit.title_scale - fit.title_room).abs() < 0.01, "{width} {fit:?}");
                }
                assert!(fit.capsule_left + fit.capsule_width + 44.0 + 20.0 <= width + 13.01);
            }
        }
    }

    #[test]
    fn a_tablet_gallery_keeps_its_columns_and_a_narrow_window_gets_bigger_cells() {
        // tablets as always: three across upright, four on their side
        for (width, landscape, columns) in [
            (640.0, false, 3),
            (744.0, false, 3),
            (800.0, false, 3),
            (1024.0, true, 4),
            (1133.0, true, 4),
            (1280.0, true, 4),
        ] {
            assert_eq!(gallery_columns(width, landscape), columns, "{width}");
        }
        // a phone upright (and a window as narrow): two across, each cell's picture box (the
        // pitch less 40 across, less 82 down) a usable size, where three left it tiny
        for width in [320.0, 360.0, 393.0, 411.0, 480.0] {
            let columns = gallery_columns(width, false);
            assert_eq!(columns, 2, "{width}");
            let pitch = width / columns as f32;
            assert!(pitch - 40.0 >= 120.0 && pitch - 82.0 >= 78.0, "{width}");
        }
        // half a tablet: three; very narrow: one; a phone on its side: four
        assert_eq!(gallery_columns(560.0, false), 3);
        assert_eq!(gallery_columns(240.0, false), 1);
        assert_eq!(gallery_columns(851.0, true), 4);
        // never fewer columns for a wider window, upright
        let mut last = 0;
        for w in (200..1400).step_by(10) {
            let n = gallery_columns(w as f32, false);
            assert!(n >= last, "{w}");
            last = n;
        }
    }

    /// The name kept in the artwork's file in `dir`.
    fn name_in(dir: &str) -> String {
        let text = fs::read_to_string(Path::new(dir).join(ARTWORK_FILE)).unwrap();
        serde_json::from_str::<serde_json::Value>(&text).unwrap()["name"].as_str().unwrap().to_owned()
    }

    #[test]
    fn artworks_are_listed_stacked_moved_duplicated_and_renamed() {
        let root = std::env::temp_dir().join(format!("procraft-gallery-{}-{}", std::process::id(), fresh_name()));
        let store = ArtworkStore::open(root.to_string_lossy().into_owned());
        let a = artwork(&store, "A", None);
        let b = artwork(&store, "B", None);
        let c = artwork(&store, "C", None);
        // placed newest first (they may share a time: put them in a known order)
        store.move_entry(a.clone(), 0, None);
        store.move_entry(b.clone(), 1, None);
        store.move_entry(c.clone(), 2, None);
        assert_eq!(shown(&store, None), names(&["A", "B", "C"]));
        let list = store.list(None);
        assert!(
            matches!(&list[0], GalleryEntry::Artwork { artwork } if artwork.width == 64 && artwork.height == 32),
            "{list:?}"
        );
        // a duplicate goes right after its original, with all its files
        fs::create_dir_all(Path::new(&a).join("layers")).unwrap();
        fs::write(Path::new(&a).join("layers").join("1.bin"), [7u8; 3]).unwrap();
        let copy = store.duplicate(a.clone());
        assert_eq!(shown(&store, None), names(&["A", "A", "B", "C"]));
        assert_eq!(fs::read(Path::new(&copy).join("layers").join("1.bin")).unwrap(), [7u8; 3]);
        assert!(Path::new(&copy).join(ARTWORK_FILE).is_file());
        // C dropped onto B makes a stack where B was, C first
        store.put_onto(c.clone(), b.clone());
        let stack = match &store.list(None)[2] {
            GalleryEntry::Stack { dir, .. } => dir.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(shown(&store, None), names(&["A", "A", "Stack[C,B]"]));
        store.rename_stack(stack.clone(), "Mine".into());
        store.rename(a.clone(), "  First ".into());
        assert_eq!(shown(&store, None), names(&["First", "A", "Mine[C,B]"]));
        // out of the stack to the front of the gallery
        let c_now = Path::new(&stack).join(name_of(Path::new(&c))).to_string_lossy().into_owned();
        store.move_entry(c_now, 0, Some(store.root()));
        assert_eq!(shown(&store, None), names(&["C", "First", "A", "Mine[B]"]));
        // the last one out, the stack goes
        let b_now = Path::new(&stack).join(name_of(Path::new(&b))).to_string_lossy().into_owned();
        store.move_entry(b_now, 4, Some(store.root()));
        assert_eq!(shown(&store, None), names(&["C", "First", "A", "B"]));
        assert!(!Path::new(&stack).exists());
        // two picked make a stack where the first of them was, in the gallery's order
        let list = store.list(None);
        let dir = |i: usize| list[i].dir().to_owned();
        store.stack(vec![dir(3), dir(1)]);
        assert_eq!(shown(&store, None), names(&["C", "Stack[First,B]", "A"]));
        assert!(store.most_recent().is_some());
        // a stack duplicated: right after it, a copy of each of its artworks in its order, each
        // in a folder of its own
        let stack = store.list(None)[1].dir().to_owned();
        store.rename_stack(stack.clone(), "Pair".into());
        let copy = store.duplicate(stack.clone());
        assert_eq!(shown(&store, None), names(&["C", "Pair[First,B]", "Pair[First,B]", "A"]));
        let inside = |dir: &str| -> Vec<String> {
            store.list(Some(dir.to_owned())).iter().map(|e| name_of(Path::new(e.dir()))).collect()
        };
        let (was, now) = (inside(&stack), inside(&copy));
        assert_eq!(now.len(), 2);
        assert!(now.iter().all(|n| !was.contains(n)), "{was:?} {now:?}");
        // deleted, its artworks go with it
        store.delete(copy.clone());
        assert_eq!(shown(&store, None), names(&["C", "Pair[First,B]", "A"]));
        let _ = fs::remove_dir_all(&root);
    }
}
