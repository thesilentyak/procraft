//! A plugin's folder kept as a sync app keeps one between devices: packages written whole and
//! named after their artworks, never over a file the app didn't write; what changed there brought
//! in, both kept when both changed; nothing in the folder ever taken away; a folder out of reach
//! told once; and nothing at all with the plugin off.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use procraft_engine::document::Document;
use serde_json::json;

use super::*;
use crate::gallery::{ArtworkStore, GalleryEntry};
use crate::plugin_file::sha256;
use crate::plugin_file::tests::pack;
use crate::plugin_run::MODULE;
use crate::plugins::PluginControl;

const ID: &str = "io.github.artcraftandroid.storage-folder";

/// The work as soon as it is asked for: no waiting for saves to stop.
const AT_ONCE: Timing =
    Timing { settle: Duration::ZERO, left: Duration::ZERO, longest: Duration::ZERO, again: Duration::from_millis(5) };

/// A plugin made as the app's Storage folder is: a folder at the top of its card and three features,
/// each naming the folder (control 0) while it is on and one is picked.
const MODULE_TEXT: &str = r#"(module
    (global $c0 (mut f32) (f32.const 0))
    (global $c1 (mut f32) (f32.const 0))
    (global $c2 (mut f32) (f32.const 0))
    (global $c3 (mut f32) (f32.const 0))
    (func (export "procraft_control") (param i32 f32)
        (if (i32.eq (local.get 0) (i32.const 0)) (then (global.set $c0 (local.get 1))))
        (if (i32.eq (local.get 0) (i32.const 1)) (then (global.set $c1 (local.get 1))))
        (if (i32.eq (local.get 0) (i32.const 2)) (then (global.set $c2 (local.get 1))))
        (if (i32.eq (local.get 0) (i32.const 3)) (then (global.set $c3 (local.get 1)))))
    (func $while (param f32) (result i32)
        (if (result i32)
            (i32.and (f32.ne (local.get 0) (f32.const 0)) (f32.ne (global.get $c0) (f32.const 0)))
            (then (i32.const 0)) (else (i32.const -1))))
    (func (export "procraft_artwork_saved") (result i32) (call $while (global.get $c1)))
    (func (export "procraft_video_exported") (result i32) (call $while (global.get $c2)))
    (func (export "procraft_gallery_opened") (result i32) (call $while (global.get $c3))))"#;

fn plugin_file() -> Vec<u8> {
    let module = wat::parse_str(MODULE_TEXT).unwrap();
    let manifest = json!({
        "schema": 1,
        "id": ID,
        "name": "Storage folder",
        "version": "1.0.0",
        "holds": ["hooks"],
        "publisher": { "name": "ProCraft" },
        "description": "Keeps artworks in a folder.",
        "compat": { "procraft": { "interface": 22 } },
        "permissions": [],
        "hooks": ["artwork_saved", "video_exported", "gallery_opened"],
        "files": [{ "path": MODULE, "sha256": sha256(&module), "size": module.len() }],
        "controls": [
            { "key": "folder", "type": "folder", "label": "Folder" },
            { "key": "keep_artworks", "type": "feature", "label": "Keep artworks in a folder",
              "description": "Writes each artwork there.", "hooks": ["artwork_saved"] },
            { "key": "keep_videos", "type": "feature", "label": "Save time-lapse videos there",
              "description": "Puts time-lapse videos there.", "hooks": ["video_exported"] },
            { "key": "bring_in", "type": "feature", "label": "Bring in changes from the folder",
              "description": "Brings in what changed there.", "hooks": ["gallery_opened"] },
        ],
    });
    pack(&manifest, &[(MODULE, &module)])
}

/// A folder of the system's, plain files, watched: each call counted, every file name it ever
/// listed noted, and made to fail its writes partway or to be out of reach.
#[derive(Default)]
struct Watched {
    lists: AtomicU32,
    writes: AtomicU32,
    reads: AtomicU32,
    fail_writes: AtomicBool,
    unreachable: AtomicBool,
    seen: Mutex<BTreeSet<String>>,
}

impl Watched {
    fn calls(&self) -> u32 {
        self.lists.load(Ordering::Relaxed) + self.writes.load(Ordering::Relaxed) + self.reads.load(Ordering::Relaxed)
    }

    fn out_of_reach(&self) -> Result<(), FolderError> {
        if self.unreachable.load(Ordering::Relaxed) {
            return Err(FolderError::Unreachable { reason: "the permission was taken back".to_owned() });
        }
        Ok(())
    }
}

impl FolderAccess for Watched {
    fn list(&self, location: String) -> Result<Vec<FolderEntry>, FolderError> {
        self.lists.fetch_add(1, Ordering::Relaxed);
        self.out_of_reach()?;
        let listed = PlainFolderAccess.list(location)?;
        self.seen.lock().unwrap().extend(listed.iter().map(|e| e.name.clone()));
        Ok(listed)
    }

    fn write_whole(&self, location: String, name: String, from: String) -> Result<FolderEntry, FolderError> {
        self.writes.fetch_add(1, Ordering::Relaxed);
        self.out_of_reach()?;
        if self.fail_writes.load(Ordering::Relaxed) {
            // cut short: half of it beside, as a write that met a full disk leaves it, then let go
            let bytes = std::fs::read(&from).unwrap();
            let beside = Path::new(&location).join(format!(".{name}.partial"));
            std::fs::write(&beside, &bytes[..bytes.len() / 2]).unwrap();
            let _ = std::fs::remove_file(&beside);
            return Err(FolderError::Failed { reason: "no space left on the device".to_owned() });
        }
        PlainFolderAccess.write_whole(location, name, from)
    }

    fn read_whole(&self, location: String, name: String, to: String) -> Result<(), FolderError> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.out_of_reach()?;
        PlainFolderAccess.read_whole(location, name, to)
    }
}

/// A layered package of a little canvas named `name`, its one layer all `colour`.
fn package_bytes(name: &str, colour: [u8; 4]) -> Vec<u8> {
    let doc = Document::new(8, 8);
    let mut pixels = std::collections::HashMap::new();
    pixels.insert(doc.current(), colour.repeat(64));
    let composite = colour.repeat(64);
    procraft_engine::native::write(&procraft_engine::native::Export {
        document: &doc,
        pixels: &pixels,
        composite: &composite,
        thumbnail_png: &[],
        name,
        author: "",
        strokes: 0,
        tracked_seconds: 0.0,
        seed: 1,
        time_lapse: None,
        reference: None,
        reference_picture: None,
        turns: 0,
    })
}

/// The colour of the one layer of the package `bytes`, and its name.
fn package_says(bytes: &[u8]) -> (String, [u8; 4]) {
    let read = procraft_engine::native::read(bytes).unwrap();
    let (_, pixels) = read.pixels.first().unwrap();
    (read.name, [pixels[0], pixels[1], pixels[2], pixels[3]])
}

/// Packages made on the CPU from what an artwork's folder says: its name, and a colour of its file
/// (another each time it is saved differently). Never busy.
struct CpuPackages {
    made: AtomicU32,
}

impl MakesPackages for CpuPackages {
    fn make(&self, _: Option<Arc<CanvasSession>>, dir: &Path, to: &Path) -> Result<bool, String> {
        self.made.fetch_add(1, Ordering::Relaxed);
        let mut bytes = std::fs::read(dir.join("artwork.json")).map_err(|e| e.to_string())?;
        let mut layers: Vec<PathBuf> =
            std::fs::read_dir(dir.join("layers")).into_iter().flatten().flatten().map(|e| e.path()).collect();
        layers.sort();
        for layer in layers {
            bytes.extend(std::fs::read(layer).unwrap_or_default());
        }
        let h = bytes.iter().fold(7u32, |h, &b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
        let colour = [(h & 0xFF) as u8, ((h >> 8) & 0xFF) as u8, ((h >> 16) & 0xFF) as u8, 255];
        std::fs::write(to, package_bytes(&artwork_name(dir), colour)).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// A test's app: its plugins (the storage folder's imported and on), its gallery, the folder
/// picked (watched), and where packages are made.
struct App {
    root: PathBuf,
    plugins: Arc<Plugins>,
    store: Arc<ArtworkStore>,
    folder: PathBuf,
    access: Arc<Watched>,
    cpu: Arc<CpuPackages>,
}

impl App {
    /// With the features `on` turned on (none: the plugin imported but off), packages made on the
    /// CPU, the work at once.
    fn new(name: &str, on: &[&str]) -> App {
        let app = App::bare(name);
        if !on.is_empty() {
            assert!(app.plugins.set_on(ID.to_owned(), true));
            for feature in on {
                assert!(app.plugins.set_switch(format!("{ID}.{feature}"), true), "{feature}");
            }
        }
        app
    }

    fn bare(name: &str) -> App {
        let root = std::env::temp_dir().join(format!("procraft-folders-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let folder = root.join("synced");
        std::fs::create_dir_all(&folder).unwrap();
        let plugins = Plugins::with_store(root.join("plugins").to_string_lossy().into_owned());
        assert_eq!(plugins.install(plugin_file()), None);
        let key = format!("{ID}.folder");
        assert!(plugins.set_folder(key, folder.to_string_lossy().into_owned(), "Synced".to_owned()));
        let access = Arc::new(Watched::default());
        plugins.use_folder_access(
            Arc::clone(&access) as Arc<dyn FolderAccess>,
            root.join("work").to_string_lossy().into_owned(),
        );
        let cpu = Arc::new(CpuPackages { made: AtomicU32::new(0) });
        plugins.folders().use_maker(Arc::clone(&cpu) as Arc<dyn MakesPackages>, AT_ONCE);
        let store = ArtworkStore::open(root.join("gallery").to_string_lossy().into_owned());
        App { root, plugins, store, folder, access, cpu }
    }

    fn folders(&self) -> Arc<Folders> {
        self.plugins.folders()
    }

    /// A new artwork in the gallery named `name`, its layer all `colour`: its folder.
    fn artwork(&self, name: &str, colour: [u8; 4]) -> PathBuf {
        let dir = PathBuf::from(self.store.new_folder(None));
        self.save_over(&dir, name, colour);
        dir
    }

    /// The artwork in `dir` saved again as `name`, its layer all `colour` (no plugin told).
    fn save_over(&self, dir: &Path, name: &str, colour: [u8; 4]) {
        let package = self.root.join("making.procraft");
        std::fs::write(&package, package_bytes(name, colour)).unwrap();
        let s = CanvasSession::open_package(package.to_string_lossy().into_owned()).unwrap();
        s.save(dir.to_string_lossy().into_owned()).unwrap();
    }

    /// The artwork in `dir` was saved: the plugin told, as the session tells it, and its work done.
    fn saved(&self, dir: &Path) {
        self.folders().artwork_saved(Weak::new(), dir);
        assert!(self.folders().wait_done(Duration::from_secs(20)));
    }

    /// The folder's files, by name, with what each holds.
    fn folder_files(&self) -> BTreeMap<String, Vec<u8>> {
        std::fs::read_dir(&self.folder)
            .unwrap()
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap()))
            .collect()
    }

    /// The gallery's artworks, in its order: their names and folders.
    fn gallery(&self) -> Vec<(String, PathBuf)> {
        self.store
            .list(None)
            .into_iter()
            .flat_map(|e| match e {
                GalleryEntry::Artwork { artwork } => vec![artwork],
                GalleryEntry::Stack { artworks, .. } => artworks,
            })
            .map(|a| (a.name, PathBuf::from(a.dir)))
            .collect()
    }

    /// The file `name` in the folder written anew as another device writes it: `bytes`, a few
    /// seconds on.
    fn written_elsewhere(&self, name: &str, bytes: &[u8]) {
        let path = self.folder.join(name);
        std::fs::write(&path, bytes).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(later).unwrap();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The colour of the one layer of the artwork in `dir`, as it opens.
fn artwork_colour(dir: &Path) -> [u8; 4] {
    let s = CanvasSession::open(dir.to_string_lossy().into_owned()).unwrap();
    let mut st = s.shared.lock();
    let (w, h) = (st.doc.width(), st.doc.height());
    let (_, opened) = st.pending_pixels.pop().unwrap();
    let pixels = opened.pixels(w, h).unwrap();
    [pixels[0], pixels[1], pixels[2], pixels[3]]
}

#[test]
fn a_plugin_names_a_folder_by_its_place_and_the_first_by_id_that_names_one_is_used() {
    use crate::plugin_run::{Running, Stop};
    let naming = |hook: &str, at: i32| {
        let text = format!(r#"(module (func (export "procraft_{hook}") (result i32) (i32.const {at})))"#);
        Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap()
    };
    for (hook, name) in [
        (Hook::ArtworkSaved, "artwork_saved"),
        (Hook::VideoExported, "video_exported"),
        (Hook::GalleryOpened, "gallery_opened"),
    ] {
        assert_eq!(naming(name, -1).folder(hook), Ok(None));
        assert_eq!(naming(name, -9).folder(hook), Ok(None));
        assert_eq!(naming(name, 0).folder(hook), Ok(Some(0)));
        assert_eq!(naming(name, 63).folder(hook), Ok(Some(63)));
        assert_eq!(naming(name, 64).folder(hook), Err(Stop::Nonsense), "past its controls");
    }
    assert_eq!(naming("artwork_saved", 0).folder(Hook::GalleryOpened), Ok(None), "a hook it doesn't answer");
    // each a bit of its own in a set of hooks, none another hook's
    let folders = [Hook::ArtworkSaved, Hook::VideoExported, Hook::GalleryOpened];
    let others = crate::hooks::tests::every_hook().iter().fold(0u64, |all, h| all | h.bit());
    let bits = folders.iter().fold(0u64, |all, h| all | h.bit());
    assert_eq!((bits.count_ones(), bits & others), (3, 0));
    assert_eq!(naming("artwork_saved", 0).folder(Hook::Position), Ok(None), "not a folder's hook");
    let hooks = Hooks::default();
    assert_eq!(hooks.folder(Hook::ArtworkSaved), None, "none on");
    hooks.put_on("b".to_owned(), naming("artwork_saved", 2), &[Hook::ArtworkSaved]);
    hooks.put_on("a".to_owned(), naming("artwork_saved", -1), &[Hook::ArtworkSaved]);
    assert_eq!(hooks.folder(Hook::ArtworkSaved), Some(("b".to_owned(), 2)));
    hooks.put_on("a".to_owned(), naming("artwork_saved", 1), &[Hook::ArtworkSaved]);
    assert_eq!(hooks.folder(Hook::ArtworkSaved), Some(("a".to_owned(), 1)));
    assert_eq!(hooks.folder(Hook::VideoExported), None);
}

#[test]
fn a_package_is_named_after_its_artwork_and_never_goes_over_a_file_the_app_didnt_write() {
    let entry = |name: &str, size: u64, modified_ms: i64| FolderEntry { name: name.to_owned(), size, modified_ms };
    let mut records = Records::default();
    // nothing there: its own name, as Share names it
    let listing = vec![entry("Notes.txt", 3, 1)];
    assert_eq!(Folders::file_for(&mut records, &listing, "a", "Dragon"), "Dragon.procraft");
    // someone else's of that name (case aside): one of its own beside it, never over it
    let listing = vec![entry("dragon.PROCRAFT", 10, 1), entry("Dragon_2.procraft", 10, 1)];
    assert_eq!(Folders::file_for(&mut records, &listing, "a", "Dragon"), "Dragon_3.procraft");
    // its own, as written: kept
    records.files.insert(
        "Dragon_3.procraft".to_owned(),
        Record { stamp: Some(Stamp { size: 9, modified: 5 }), artwork: Some("a".to_owned()), ..Record::default() },
    );
    let mut listing = listing;
    listing.push(entry("Dragon_3.procraft", 9, 5));
    assert_eq!(Folders::file_for(&mut records, &listing, "a", "Dragon"), "Dragon_3.procraft");
    // written over there since (another device): let go, never written over, a name of its own
    listing.last_mut().unwrap().modified_ms = 6;
    assert_eq!(Folders::file_for(&mut records, &listing, "a", "Dragon"), "Dragon_4.procraft");
    assert_eq!(records.files.get("Dragon_3.procraft").and_then(|r| r.artwork.clone()), None);
    // renamed: under its new name, the old file let go (left there, no artwork's)
    records.files.insert(
        "Dragon_4.procraft".to_owned(),
        Record { stamp: Some(Stamp { size: 9, modified: 7 }), artwork: Some("a".to_owned()), ..Record::default() },
    );
    listing.push(entry("Dragon_4.procraft", 9, 7));
    assert_eq!(Folders::file_for(&mut records, &listing, "a", "Wyvern"), "Wyvern.procraft");
    assert!(records.files.values().all(|r| r.artwork.is_none()));
    assert!(named_after("Dragon_12.procraft", "Dragon", "_", "procraft"));
    assert!(!named_after("Dragon_x.procraft", "Dragon", "_", "procraft"));
    assert!(!named_after("Dragon.png", "Dragon", "_", "procraft"));
}

#[test]
fn what_was_noted_reads_leniently_and_another_folder_starts_afresh() {
    let dir = std::env::temp_dir().join(format!("procraft-folders-noted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let target =
        Target { key: format!("{ID}.folder"), location: "here".to_owned(), records: Some(dir.join("noted.json")) };
    let mut records = Records { location: "here".to_owned(), ..Records::default() };
    records.files.insert(
        "A.procraft".to_owned(),
        Record {
            stamp: Some(Stamp { size: 1, modified: 2 }),
            artwork: Some("x".to_owned()),
            artwork_stamp: Some(Stamp { size: 3, modified: 4 }),
            video: false,
        },
    );
    records.files.insert(
        "A time-lapse.mp4".to_owned(),
        Record { stamp: Some(Stamp { size: 5, modified: 6 }), video: true, ..Record::default() },
    );
    assert!(records.write(&target));
    assert_eq!(Records::read(&target), records);
    // one bad field costs only itself; an entry with nothing that reads goes
    std::fs::write(
        dir.join("noted.json"),
        r#"{"location":"here","files":{"A.procraft":{"size":"one","modified_ms":2,"artwork":"x"},"B":{"video":1},"C":7}}"#,
    )
    .unwrap();
    let read = Records::read(&target);
    assert_eq!(read.files.len(), 1);
    assert_eq!(read.files.get("A.procraft").map(|r| (r.stamp, r.artwork.clone())), Some((None, Some("x".to_owned()))));
    // unreadable, or another folder's: nothing known, so nothing there is written over
    std::fs::write(dir.join("noted.json"), b"{ not json").unwrap();
    assert!(Records::read(&target).files.is_empty());
    assert!(records.write(&target));
    assert!(Records::read(&Target { location: "elsewhere".to_owned(), ..target }).files.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn twenty_saves_keep_one_whole_package_in_the_folder() {
    let app = App::new("twenty", &["keep_artworks"]);
    let dir = app.artwork("Dragon", [200, 10, 10, 255]);
    let mut colours = BTreeSet::new();
    for i in 0..20u8 {
        app.save_over(&dir, "Dragon", [i * 10, 255 - i * 10, 7, 255]);
        app.saved(&dir);
        let files = app.folder_files();
        assert_eq!(files.keys().collect::<Vec<_>>(), ["Dragon.procraft"], "save {i}");
        // whole and readable, as the artwork now is
        let (name, colour) = package_says(files.get("Dragon.procraft").unwrap());
        assert_eq!(name, "Dragon");
        colours.insert(colour);
    }
    let writes = app.access.writes.load(Ordering::Relaxed);
    println!(
        "20 saves: {writes} packages written, {} distinct, {} made",
        colours.len(),
        app.cpu.made.load(Ordering::Relaxed)
    );
    assert_eq!(writes, 20);
    assert_eq!(colours.len(), 20, "each package was the artwork as saved");
    // nothing half written left beside it, in the folder or where packages are made
    assert!(std::fs::read_dir(&app.folder).unwrap().count() == 1);
    assert_eq!(std::fs::read_dir(app.root.join("work")).unwrap().count(), 0);
}

#[test]
fn saves_close_together_are_one_package_once_they_stop() {
    let app = App::new("settle", &["keep_artworks"]);
    let timing = Timing { settle: Duration::from_millis(150), longest: Duration::from_secs(5), ..AT_ONCE };
    app.folders().use_maker(Arc::clone(&app.cpu) as Arc<dyn MakesPackages>, timing);
    let dir = app.artwork("Settled", [1, 2, 3, 255]);
    let started = Instant::now();
    for _ in 0..20 {
        app.folders().artwork_saved(Weak::new(), &dir);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(app.folders().wait_done(Duration::from_secs(10)));
    let made = app.cpu.made.load(Ordering::Relaxed);
    println!("20 saves 10 ms apart: {made} package, written {:?} after the first", started.elapsed());
    assert_eq!(made, 1);
    assert_eq!(app.folder_files().len(), 1);
}

#[test]
fn a_write_that_fails_leaves_the_last_good_package() {
    let app = App::new("failed", &["keep_artworks"]);
    let dir = app.artwork("Kept", [10, 20, 30, 255]);
    app.saved(&dir);
    let good = app.folder_files();
    assert_eq!(good.len(), 1);
    app.access.fail_writes.store(true, Ordering::Relaxed);
    for i in 0..5u8 {
        app.save_over(&dir, "Kept", [i, i, i, 255]);
        app.saved(&dir);
        // byte for byte as it was, readable, and nothing half written beside it
        assert_eq!(app.folder_files(), good, "failed write {i}");
        assert_eq!(package_says(good.values().next().unwrap()).0, "Kept");
    }
    // not a folder out of reach: nothing told
    assert_eq!(app.folders().take_notice(), None);
    app.access.fail_writes.store(false, Ordering::Relaxed);
    app.save_over(&dir, "Kept", [99, 98, 97, 255]);
    app.saved(&dir);
    assert_ne!(app.folder_files(), good, "written again once writes work");
    assert_eq!(app.access.writes.load(Ordering::Relaxed), 7);
}

#[test]
fn a_package_dropped_into_the_folder_comes_into_the_gallery_once() {
    let app = App::new("dropped", &["bring_in"]);
    let mine = app.artwork("Already here", [5, 5, 5, 255]);
    app.written_elsewhere("From_the_phone.procraft", &package_bytes("From the phone", [0, 128, 255, 255]));
    app.written_elsewhere("notes.txt", b"not a package");
    let brought = app.plugins.gallery_opened(Arc::clone(&app.store));
    assert_eq!((brought.added, brought.updated, brought.copies, brought.notice), (1, 0, 0, None));
    let gallery = app.gallery();
    assert_eq!(gallery.len(), 2);
    let (_, new) = gallery.iter().find(|(n, _)| n == "From the phone").unwrap();
    assert_eq!(artwork_colour(new), [0, 128, 255, 255]);
    assert!(gallery.iter().any(|(_, d)| *d == mine));
    // opened again: nothing new, nothing twice
    for _ in 0..3 {
        assert_eq!(app.plugins.gallery_opened(Arc::clone(&app.store)), FolderBroughtIn::default());
    }
    assert_eq!(app.gallery().len(), 2);
    // deleted from the gallery, it stays deleted until it changes there
    app.store.delete(new.to_string_lossy().into_owned());
    assert_eq!(app.plugins.gallery_opened(Arc::clone(&app.store)).added, 0);
    app.written_elsewhere("From_the_phone.procraft", &package_bytes("From the phone", [1, 2, 3, 255]));
    assert_eq!(app.plugins.gallery_opened(Arc::clone(&app.store)).added, 1);
    assert_eq!(app.access.reads.load(Ordering::Relaxed), 2);
}

/// Brought in on a thread with an iPad dispatch queue's stack (512 KB; an Android thread has 1 MB),
/// as the apps bring artworks in as the gallery opens.
#[test]
fn a_package_comes_in_on_a_queues_small_stack() {
    let app = App::new("small-stack", &["bring_in"]);
    app.written_elsewhere("Small.procraft", &package_bytes("Small", [3, 4, 5, 255]));
    let (plugins, store) = (Arc::clone(&app.plugins), Arc::clone(&app.store));
    let brought = std::thread::Builder::new()
        .name("bringing in on a 512 KB stack".into())
        .stack_size(512 * 1024)
        .spawn(move || plugins.gallery_opened(store))
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(brought.added, 1);
}

#[test]
fn newer_there_comes_in_over_the_gallerys_copy_in_its_place() {
    let app = App::new("newer", &["keep_artworks", "bring_in"]);
    let first = app.artwork("First", [1, 1, 1, 255]);
    let dragon = app.artwork("Dragon", [2, 2, 2, 255]);
    let last = app.artwork("Last", [3, 3, 3, 255]);
    app.saved(&dragon);
    let order = |app: &App| app.gallery().into_iter().map(|(n, _)| n).collect::<Vec<_>>();
    let before = order(&app);
    assert_eq!(before, ["Last", "Dragon", "First"]);
    // the app's own package is no change: nothing comes in
    assert_eq!(app.plugins.gallery_opened(Arc::clone(&app.store)), FolderBroughtIn::default());
    // another device painted on it
    app.written_elsewhere("Dragon.procraft", &package_bytes("Dragon", [40, 50, 60, 255]));
    let brought = app.plugins.gallery_opened(Arc::clone(&app.store));
    assert_eq!((brought.added, brought.updated, brought.copies), (0, 1, 0));
    assert_eq!(order(&app), before, "in its place");
    let now = app.gallery().into_iter().find(|(n, _)| n == "Dragon").unwrap().1;
    assert_ne!(now, dragon);
    assert!(!dragon.exists());
    assert_eq!(artwork_colour(&now), [40, 50, 60, 255]);
    assert!(first.exists() && last.exists());
    // and from then on it is the one written there, over the same file
    app.save_over(&now, "Dragon", [7, 8, 9, 255]);
    app.saved(&now);
    assert_eq!(app.folder_files().keys().collect::<Vec<_>>(), ["Dragon.procraft"]);
    assert_eq!(app.plugins.gallery_opened(Arc::clone(&app.store)), FolderBroughtIn::default());
}

#[test]
fn an_artwork_changed_on_both_sides_is_kept_both_ways() {
    let app = App::new("both", &["keep_artworks", "bring_in"]);
    let dragon = app.artwork("Dragon", [2, 2, 2, 255]);
    app.saved(&dragon);
    // changed here (saved, its package not written yet) and there
    assert!(app.plugins.set_switch(format!("{ID}.keep_artworks"), false));
    app.save_over(&dragon, "Dragon", [11, 12, 13, 255]);
    app.saved(&dragon);
    let theirs = package_bytes("Dragon", [70, 80, 90, 255]);
    app.written_elsewhere("Dragon.procraft", &theirs);
    let brought = app.plugins.gallery_opened(Arc::clone(&app.store));
    assert_eq!((brought.added, brought.updated, brought.copies), (0, 0, 1));
    // the gallery's as it was, and theirs beside it as a copy, named plainly
    assert_eq!(artwork_colour(&dragon), [11, 12, 13, 255]);
    let copy = app.gallery().into_iter().find(|(n, _)| n == "Dragon (from folder)").unwrap().1;
    assert_eq!(artwork_colour(&copy), [70, 80, 90, 255]);
    assert_eq!(app.folder_files().get("Dragon.procraft"), Some(&theirs), "the folder's left as it was");
    // not brought in twice
    assert_eq!(app.plugins.gallery_opened(Arc::clone(&app.store)), FolderBroughtIn::default());
    // writing again: ours goes there (theirs is kept as the copy), and the copy has a file of its own
    assert!(app.plugins.set_switch(format!("{ID}.keep_artworks"), true));
    app.saved(&dragon);
    app.saved(&copy);
    let files = app.folder_files();
    assert_eq!(files.keys().collect::<Vec<_>>(), ["Dragon.procraft", "Dragon_from_folder.procraft"]);
    assert_eq!(package_says(files.get("Dragon_from_folder.procraft").unwrap()).0, "Dragon (from folder)");
    // an artwork open here is changed here: never brought in over
    app.written_elsewhere("Dragon.procraft", &package_bytes("Dragon", [1, 1, 1, 255]));
    let open = CanvasSession::open(dragon.to_string_lossy().into_owned()).unwrap();
    opened(&dragon, Arc::downgrade(&open));
    let brought = app.plugins.gallery_opened(Arc::clone(&app.store));
    assert_eq!((brought.updated, brought.copies), (0, 1));
    assert!(dragon.exists());
}

#[test]
fn nothing_in_the_folder_is_ever_taken_away() {
    let app = App::new("kept", &["keep_artworks", "keep_videos", "bring_in"]);
    app.written_elsewhere("Theirs.procraft", &package_bytes("Theirs", [9, 9, 9, 255]));
    app.written_elsewhere("Dragon.procraft", &package_bytes("Someone else's dragon", [9, 9, 9, 255]));
    app.written_elsewhere("readme.txt", b"hello");
    let a = app.artwork("Dragon", [1, 0, 0, 255]);
    let b = app.artwork("Wyvern", [0, 1, 0, 255]);
    for round in 0..4u8 {
        app.save_over(&a, "Dragon", [round, 0, 0, 255]);
        app.saved(&a);
        app.saved(&b);
        app.plugins.gallery_opened(Arc::clone(&app.store));
        app.written_elsewhere("Wyvern.procraft", &package_bytes("Wyvern", [round, round, 1, 255]));
        app.plugins.gallery_opened(Arc::clone(&app.store));
        // renamed: written under its new name, the old file left
        app.store.rename(a.to_string_lossy().into_owned(), format!("Dragon {round}"));
        app.saved(&a);
        let video = app.root.join("lapse.mp4");
        std::fs::write(&video, [round; 64]).unwrap();
        assert!(app.plugins.video_exported(video.to_string_lossy().into_owned(), "Wyvern".to_owned()));
        assert!(app.folders().wait_done(Duration::from_secs(10)));
        app.store.delete(b.to_string_lossy().into_owned());
    }
    let seen = app.access.seen.lock().unwrap().clone();
    let there: BTreeSet<String> = app.folder_files().into_keys().collect();
    let gone: Vec<&String> = seen.difference(&there).collect();
    println!("{} files seen in the folder over the run, {} there now, {} gone", seen.len(), there.len(), gone.len());
    assert!(gone.is_empty(), "taken away: {gone:?}");
    assert_eq!(app.folder_files().get("readme.txt").map(Vec::as_slice), Some(b"hello".as_slice()));
    let someone = app.folder_files().get("Dragon.procraft").cloned().unwrap();
    assert_eq!(package_says(&someone).0, "Someone else's dragon", "a file the app didn't write is never written over");
}

#[test]
fn a_folder_out_of_reach_is_told_once_and_again_after_it_was_reached() {
    let app = App::new("reach", &["keep_artworks", "bring_in"]);
    let dir = app.artwork("Out of reach", [1, 2, 3, 255]);
    app.access.unreachable.store(true, Ordering::Relaxed);
    let mut told = Vec::new();
    for _ in 0..5 {
        app.saved(&dir);
        told.extend(app.folders().take_notice());
        told.extend(app.plugins.gallery_opened(Arc::clone(&app.store)).notice);
    }
    println!("folder out of reach through 5 saves and 5 gallery openings: told {} time(s)", told.len());
    assert_eq!(told, [FOLDER_UNREACHABLE]);
    // the app goes on: the artwork as it was, nothing in the gallery changed
    assert_eq!(app.gallery().len(), 1);
    assert!(Stamp::of_artwork(&dir).is_some());
    // reached again: works, and the next time it can't be, told again, once
    app.access.unreachable.store(false, Ordering::Relaxed);
    app.saved(&dir);
    assert_eq!(app.folder_files().len(), 1);
    app.access.unreachable.store(true, Ordering::Relaxed);
    let mut again = Vec::new();
    for _ in 0..3 {
        app.saved(&dir);
        again.extend(app.plugins.gallery_opened(Arc::clone(&app.store)).notice);
    }
    assert_eq!(again, [FOLDER_UNREACHABLE]);
    // a folder picked again starts afresh
    app.access.unreachable.store(false, Ordering::Relaxed);
    assert!(app.plugins.set_folder(
        format!("{ID}.folder"),
        app.folder.to_string_lossy().into_owned(),
        "Synced".to_owned()
    ));
    assert_eq!(app.folders().take_notice(), None);
}

#[test]
fn a_session_tells_a_folder_out_of_reach_with_its_other_notices() {
    let app = App::new("session-told", &["keep_artworks"]);
    let s = CanvasSession::new(16, 16);
    s.use_plugins(Arc::clone(&app.plugins));
    app.folders().unreachable("gone");
    assert_eq!(s.take_failure().as_deref(), Some(FOLDER_UNREACHABLE));
    assert_eq!(s.take_failure(), None);
}

#[test]
fn with_the_plugin_off_nothing_goes_to_a_folder_or_comes_from_one() {
    for (name, on) in [("off", &[][..]), ("features-off", &["none"][..])] {
        let app = App::new(name, &[]);
        if on == ["none"] {
            // on, its features each off
            assert!(app.plugins.set_on(ID.to_owned(), true));
        }
        app.written_elsewhere("Theirs.procraft", &package_bytes("Theirs", [9, 9, 9, 255]));
        let dir = app.artwork("Mine", [1, 1, 1, 255]);
        let package = app.root.join("mine.procraft");
        std::fs::write(&package, package_bytes("Mine", [1, 1, 1, 255])).unwrap();
        let s = CanvasSession::open_package(package.to_string_lossy().into_owned()).unwrap();
        s.use_plugins(Arc::clone(&app.plugins));
        for _ in 0..20 {
            s.save(dir.to_string_lossy().into_owned()).unwrap();
        }
        let video = app.root.join("lapse.mp4");
        std::fs::write(&video, [1u8; 16]).unwrap();
        assert!(!app.plugins.video_exported(video.to_string_lossy().into_owned(), "Mine".to_owned()));
        assert_eq!(app.plugins.gallery_opened(Arc::clone(&app.store)), FolderBroughtIn::default());
        assert!(app.folders().wait_done(Duration::from_secs(5)));
        println!("{name}: 20 saves, a video and the gallery: {} calls on the folder", app.access.calls());
        assert_eq!(app.access.calls(), 0, "{name}");
        assert_eq!(app.cpu.made.load(Ordering::Relaxed), 0, "{name}");
        let folders = app.folders();
        let queue = locked(&folders.queue);
        assert!(queue.jobs.is_empty() && !queue.worker, "{name}");
        drop(queue);
        assert_eq!(app.folder_files().keys().collect::<Vec<_>>(), ["Theirs.procraft"]);
        assert_eq!(app.gallery().len(), 1);
    }
}

#[test]
fn each_feature_names_the_folder_only_while_it_is_on_and_one_is_picked() {
    let app = App::new("features", &[]);
    let folders = app.folders();
    let named = |folders: &Folders| {
        [Hook::ArtworkSaved, Hook::VideoExported, Hook::GalleryOpened].map(|h| folders.target(h).is_some())
    };
    assert_eq!(named(&folders), [false; 3], "the plugin off");
    assert!(app.plugins.set_on(ID.to_owned(), true));
    assert_eq!(named(&folders), [false; 3], "on, no feature on");
    assert!(app.plugins.set_switch(format!("{ID}.keep_videos"), true));
    assert_eq!(named(&folders), [false, true, false]);
    assert!(app.plugins.set_switch(format!("{ID}.bring_in"), true));
    assert_eq!(named(&folders), [false, true, true]);
    // no folder picked: none named
    assert!(app.plugins.clear_file(format!("{ID}.folder")));
    assert_eq!(named(&folders), [false; 3]);
    let card = app.plugins.plugins().into_iter().find(|p| p.id == ID).unwrap();
    assert!(matches!(card.top.as_slice(), [PluginControl::Folder { control }] if control.location.is_none()));
    assert!(card.features.iter().all(|f| f.controls.is_empty()));
    // picked again, kept through a start
    let location = app.folder.to_string_lossy().into_owned();
    assert!(app.plugins.set_folder(format!("{ID}.folder"), location.clone(), "Synced".to_owned()));
    let again = Plugins::with_store(app.root.join("plugins").to_string_lossy().into_owned());
    let card = again.plugins().into_iter().find(|p| p.id == ID).unwrap();
    match card.top.as_slice() {
        [PluginControl::Folder { control }] => {
            assert_eq!(
                (control.chosen.as_deref(), control.location.as_deref()),
                (Some("Synced"), Some(location.as_str()))
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_storage_folder_the_app_comes_with_names_its_folder_as_its_features_say() {
    let root = std::env::temp_dir().join(format!("procraft-folders-bundled-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("synced")).unwrap();
    let plugins = Plugins::with_bundled(root.join("plugins").to_string_lossy().into_owned());
    let card = plugins.plugins().into_iter().find(|p| p.id == ID).unwrap();
    assert!(!card.on, "it comes off");
    let folders = plugins.folders();
    let named = |folders: &Folders| {
        [Hook::ArtworkSaved, Hook::VideoExported, Hook::GalleryOpened].map(|h| folders.target(h).is_some())
    };
    let location = root.join("synced").to_string_lossy().into_owned();
    assert!(plugins.set_folder(format!("{ID}.folder"), location.clone(), "Synced".to_owned()));
    assert_eq!(named(&folders), [false; 3], "off");
    assert!(plugins.set_on(ID.to_owned(), true));
    assert_eq!(named(&folders), [false; 3], "on, every feature off");
    for (feature, now) in
        [("keep_artworks", [true, false, false]), ("keep_videos", [true, true, false]), ("bring_in", [true; 3])]
    {
        assert!(plugins.set_switch(format!("{ID}.{feature}"), true));
        assert_eq!(named(&folders), now, "{feature}");
    }
    let target = folders.target(Hook::GalleryOpened).unwrap();
    assert_eq!((target.key.as_str(), target.location.as_str()), (format!("{ID}.folder").as_str(), location.as_str()));
    // the card: the folder at its top, the three features under it with their words
    let card = plugins.plugins().into_iter().find(|p| p.id == ID).unwrap();
    assert!(
        matches!(card.top.as_slice(), [PluginControl::Folder { control }] if control.chosen.as_deref() == Some("Synced"))
    );
    let features: Vec<&str> = card.features.iter().map(|f| f.control.label.as_str()).collect();
    assert_eq!(
        features,
        ["Keep artworks in a folder", "Save time-lapse videos there", "Bring in changes from the folder"]
    );
    assert!(card.features.iter().all(|f| !f.description.is_empty()));
    assert!(card.features.last().unwrap().description.starts_with("Careful"));
    assert!(plugins.clear_file(format!("{ID}.folder")));
    assert_eq!(named(&folders), [false; 3], "no folder picked");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_time_lapse_video_goes_into_the_folder_too() {
    let app = App::new("video", &["keep_videos"]);
    let video = app.root.join("export.mp4");
    std::fs::write(&video, [1u8; 100]).unwrap();
    assert!(app.plugins.video_exported(video.to_string_lossy().into_owned(), "Dragon".to_owned()));
    assert!(app.folders().wait_done(Duration::from_secs(10)));
    assert_eq!(app.folder_files().get("Dragon time-lapse.mp4").map(Vec::len), Some(100));
    // exported again: over its own
    std::fs::write(&video, [2u8; 50]).unwrap();
    assert!(app.plugins.video_exported(video.to_string_lossy().into_owned(), "Dragon".to_owned()));
    assert!(app.folders().wait_done(Duration::from_secs(10)));
    assert_eq!(app.folder_files().get("Dragon time-lapse.mp4").map(Vec::len), Some(50));
    // one there someone else wrote under that name: never over it
    app.written_elsewhere("Wyvern time-lapse.mp4", &[9u8; 10]);
    assert!(app.plugins.video_exported(video.to_string_lossy().into_owned(), "Wyvern".to_owned()));
    assert!(app.folders().wait_done(Duration::from_secs(10)));
    let files = app.folder_files();
    assert_eq!(files.get("Wyvern time-lapse.mp4").map(Vec::len), Some(10));
    assert_eq!(files.get("Wyvern time-lapse 2.mp4").map(Vec::len), Some(50));
    // the video stays where the app made it
    assert!(video.exists());
}

/// The packages written for the folder are Share's, byte for byte: made from the artwork while it
/// is open (20 saves of it, painted between), and from its folder once it was left.
#[test]
fn twenty_saves_of_an_open_artwork_write_shares_package_each_time() {
    use crate::tests::{pixel, sample, session};
    let Some(s) = session(96, 96) else { return };
    let app = App::new("share", &["keep_artworks"]);
    app.folders().use_maker(Arc::new(SharePackages), AT_ONCE);
    let dir = PathBuf::from(app.store.new_folder(None));
    let at = dir.to_string_lossy().into_owned();
    s.set_name("Share's".to_owned());
    s.use_plugins(Arc::clone(&app.plugins));
    let share = app.root.join("share.procraft");
    let mut took = Duration::ZERO;
    for i in 0..20 {
        let y = 8.0 + i as f32 * 4.0;
        s.set_color((i as f32 * 0.37).fract(), 0.5, 0.2);
        s.stroke_begin(crate::PointerKind::Stylus, vec![sample(8.0, y, 0.0)]);
        s.stroke_add(vec![sample(80.0, y + 3.0, 16.0)]);
        s.stroke_end(vec![]);
        let _ = pixel(&s, 0, 0);
        s.save(at.clone()).unwrap();
        let started = Instant::now();
        assert!(app.folders().wait_done(Duration::from_secs(30)));
        took += started.elapsed();
        s.export_package(share.to_string_lossy().into_owned(), seed_of(&dir), time_lapse_of(&dir)).unwrap();
        let files = app.folder_files();
        assert_eq!(files.len(), 1, "save {i}");
        assert_eq!(files.get("Shares.procraft"), Some(&std::fs::read(&share).unwrap()), "save {i}");
    }
    println!("20 saves: 20 packages identical to Share's, {:?} each on average", took / 20);
    assert_eq!(app.access.writes.load(Ordering::Relaxed), 20);
    // left: written again from its folder, as Share makes it of the artwork opened again
    let saved = Arc::clone(&s).save_to_leave(at.clone());
    assert_eq!(saved, crate::project::Leaving::Left);
    s.release_canvas();
    app.saved(&dir);
    let again = CanvasSession::open(at.clone()).unwrap();
    assert!(again.prepare_offscreen());
    again.export_package(share.to_string_lossy().into_owned(), seed_of(&dir), time_lapse_of(&dir)).unwrap();
    assert_eq!(app.folder_files().get("Shares.procraft"), Some(&std::fs::read(&share).unwrap()));
}

/// The package made for a plugin's folder waits for a stroke under way, and never cuts it short.
#[test]
fn a_package_waits_for_a_stroke_under_way() {
    use crate::tests::{pixel, sample, session};
    let Some(s) = session(64, 64) else { return };
    let to = std::env::temp_dir().join(format!("procraft-folders-busy-{}.procraft", std::process::id()));
    s.stroke_begin(crate::PointerKind::Stylus, vec![sample(8.0, 8.0, 0.0)]);
    s.stroke_add(vec![sample(40.0, 20.0, 16.0)]);
    let made = s.export_package_when(to.to_string_lossy().into_owned(), 1, None, true).unwrap();
    assert!(!made && !to.exists(), "not while a stroke is under way");
    assert!(s.shared.lock().stroke.is_some(), "the stroke goes on");
    s.stroke_end(vec![]);
    let _ = pixel(&s, 0, 0);
    assert!(s.export_package_when(to.to_string_lossy().into_owned(), 1, None, true).unwrap());
    assert!(procraft_engine::native::read(&std::fs::read(&to).unwrap()).is_ok());
    let _ = std::fs::remove_file(&to);
}
