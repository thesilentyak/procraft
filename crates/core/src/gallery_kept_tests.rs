//! Every artwork made stays in sight in the gallery: a new one is in it from the moment its first
//! save has written its document, whatever moment the app is ended at; what a run of the app that
//! ended left whole but hidden shows when the gallery next looks, with no Gallery Recovery; and the
//! order the person gave stays, the gallery and the home screen's widget reading it at once.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, PoisonError};

use procraft_engine::document::Document;

use crate::project::cut_short;
use crate::storage::{FullDisk, simulated};
use crate::{ArtworkStore, CanvasSession, GalleryEntry};

/// A gallery folder of the test's own, empty.
fn root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("procraft-kept-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn open(root: &Path) -> Arc<ArtworkStore> {
    ArtworkStore::open(root.to_string_lossy().into_owned())
}

/// The app started again: the notes of what the run before was making (an import, a copy) are a
/// run's that has ended.
fn start_again(root: &Path) -> Arc<ArtworkStore> {
    fn note_ended(at: &Path, depth: u32) {
        for entry in fs::read_dir(at).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() && depth < 3 {
                note_ended(&path, depth + 1);
            } else if path.file_name().is_some_and(|n| n == "import.run") {
                fs::write(&path, "0").unwrap();
            }
        }
    }
    note_ended(root, 0);
    open(root)
}

/// An artwork named `name` in `folder` (a stack's, or the gallery), as a save writes its document.
fn artwork(store: &ArtworkStore, name: &str, folder: Option<String>) -> String {
    let dir = store.new_folder(folder);
    let json = serde_json::json!({ "name": name, "document": { "width": 64, "height": 32 } });
    fs::write(Path::new(&dir).join("artwork.json"), json.to_string()).unwrap();
    dir
}

/// The gallery as it shows: artworks by name, stacks as `name[their artworks]`.
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

/// Every artwork the gallery shows, those in stacks too, by name.
fn all_shown(store: &ArtworkStore) -> Vec<String> {
    let mut names: Vec<String> = store
        .list(None)
        .into_iter()
        .flat_map(|e| match e {
            GalleryEntry::Artwork { artwork } => vec![artwork.name],
            GalleryEntry::Stack { artworks, .. } => artworks.into_iter().map(|a| a.name).collect(),
        })
        .collect();
    names.sort();
    names
}

/// An artwork of three painted layers, named `name`, as a picture made into one is before it shows.
fn painted(name: &str, shade: u8) -> Arc<CanvasSession> {
    let (w, h) = (48, 32);
    let mut doc = Document::new(w, h);
    doc.add_layer();
    doc.add_layer();
    let pixels = doc
        .pixel_layer_ids()
        .enumerate()
        .map(|(k, id)| (id, vec![shade.wrapping_add(40 * k as u8); (w * h * 4) as usize]))
        .collect();
    CanvasSession::with_document(doc, name.to_owned(), pixels)
}

/// What the gallery came to after the app was ended at a step of a save.
#[derive(Debug)]
struct Ended {
    step: usize,
    shown: usize,
    recovered: u32,
}

/// Artworks made one after another, the last one's first save ended (the app stopped) after each
/// of its steps in turn, the app then started again: every artwork whose document was written is
/// in the gallery, whole, with no recovery; ended before that, nothing of it shows (or hides).
#[test]
fn artworks_made_with_the_app_ended_at_any_step_of_their_first_save_are_all_in_the_gallery() {
    const MADE: usize = 4;
    let mut runs: Vec<Ended> = Vec::new();
    for step in 0.. {
        let root = root(&format!("first-save-{step}"));
        let store = open(&root);
        for i in 0..MADE - 1 {
            painted(&format!("Made {i}"), 10 * i as u8).save(store.new_folder(None)).unwrap();
        }
        let last = store.new_folder(None);
        let session = painted("Last", 200);
        cut_short::after(Some(step));
        let ended = catch_unwind(AssertUnwindSafe(|| session.save(last.clone()))).is_err();
        cut_short::after(None);
        drop(session);
        let store = start_again(&root);
        let shown = all_shown(&store);
        if shown.len() == MADE {
            // whole: it opens with every layer it was saved with
            let opened = CanvasSession::open(last.clone()).unwrap();
            let layers: Vec<_> = opened.shared.lock().doc.pixel_layer_ids().collect();
            assert_eq!(layers.len(), 3, "step {step}");
            for id in layers {
                assert!(Path::new(&last).join("layers").join(format!("{}.png", id.0)).is_file(), "step {step}");
            }
        }
        let recovered = store.recover_lost_artworks();
        runs.push(Ended { step, shown: shown.len(), recovered });
        let _ = fs::remove_dir_all(&root);
        if !ended {
            break;
        }
    }
    // the steps: each file written beside its place, then each put in place (the document last)
    let steps = runs.len() - 1;
    let document_written = steps / 2 - 1;
    println!("a first save of {steps} steps, ended after each (artworks made: {MADE}):");
    for r in &runs {
        println!("  ended after step {:2}: {} in the gallery, {} found by recovery", r.step, r.shown, r.recovered);
    }
    assert!(steps >= 8, "a save of three layers, a thumbnail and its document: {steps} steps");
    for r in &runs {
        let made = if r.step >= document_written { MADE } else { MADE - 1 };
        assert_eq!((r.shown, r.recovered), (made, 0), "{r:?}");
    }
}

/// A new artwork made into a stack that went meanwhile (its artworks taken out, so it was taken
/// away, while the app still had it open) goes in the gallery, not into a folder made again
/// without the stack's name file, out of sight.
#[test]
fn a_new_artwork_made_in_a_stack_gone_meanwhile_is_in_the_gallery() {
    let root = root("stack-gone");
    let store = open(&root);
    let mut made = 0;
    for round in 0..3 {
        let stack =
            store.stack(vec![artwork(&store, &format!("A{round}"), None), artwork(&store, &format!("B{round}"), None)]);
        made += 2;
        for e in store.list(Some(stack.clone())) {
            let GalleryEntry::Artwork { artwork } = e else { continue };
            store.move_entry(artwork.dir, 0, Some(store.root()));
        }
        // the gallery read again: the emptied stack goes
        store.list(None);
        assert!(!Path::new(&stack).exists());
        artwork(&store, &format!("New{round}"), Some(stack));
        made += 1;
    }
    let shown = all_shown(&store);
    let recovered = store.recover_lost_artworks();
    println!("artworks made: {made}, in the gallery: {}, found only by recovery: {recovered}", shown.len());
    assert_eq!((shown.len(), recovered), (made, 0), "{shown:?}");
    let _ = fs::remove_dir_all(&root);
}

/// A stack isn't taken away while it holds a folder: an artwork being made in it (its first save
/// not written yet), or one that can't be read (recovery's to mend). Taken away, the first would come
/// back without the stack's name file, hidden, and the second would be lost.
#[test]
fn a_stack_holding_an_artwork_being_made_or_damaged_is_kept() {
    let root = root("stack-kept");
    let store = open(&root);
    // a stack whose last artwork is taken out just as a new one is begun in it
    let stack = store.stack(vec![artwork(&store, "A", None), artwork(&store, "B", None)]);
    let begun = store.new_folder(Some(stack.clone()));
    for e in store.list(Some(stack.clone())) {
        let GalleryEntry::Artwork { artwork } = e else { continue };
        store.move_entry(artwork.dir, 0, Some(store.root()));
    }
    store.list(None);
    // its first save written (as a save makes its folder first)
    fs::create_dir_all(Path::new(&begun).join("layers")).unwrap();
    let json = serde_json::json!({ "name": "Begun", "document": { "width": 8, "height": 8 } });
    fs::write(Path::new(&begun).join("artwork.json"), json.to_string()).unwrap();
    let mut names = shown(&store, None);
    names.sort();
    assert_eq!(names, ["A", "B", "Stack[Begun]"]);

    // a stack left holding only an artwork whose file can't be read, a whole one beside it
    let other = store.stack(vec![artwork(&store, "C", None), artwork(&store, "Damaged", None)]);
    for e in store.list(Some(other.clone())) {
        let GalleryEntry::Artwork { artwork } = e else { continue };
        let file = Path::new(&artwork.dir).join("artwork.json");
        if artwork.name == "Damaged" {
            fs::write(Path::new(&artwork.dir).join("artwork.json.9.tmp"), fs::read(&file).unwrap()).unwrap();
            fs::write(&file, "{\"name\": \"Dam").unwrap();
        } else {
            store.move_entry(artwork.dir, 0, Some(store.root()));
        }
    }
    store.list(None);
    assert!(Path::new(&other).exists(), "a stack holding what can't be read yet is kept");
    assert_eq!(store.recover_lost_artworks(), 1);
    assert_eq!(all_shown(&store), ["A", "B", "Begun", "C", "Damaged"]);
    let _ = fs::remove_dir_all(&root);
}

/// A stack that lost its name file (an earlier version, or the storage full as it was made) shows
/// again as a stack the next time the gallery looks, where it was, its artworks in their order.
#[test]
fn a_stack_that_lost_its_name_file_shows_again_where_it_was() {
    let root = root("stack-named-again");
    let store = open(&root);
    let first = artwork(&store, "First", None);
    let stack = store.stack(vec![artwork(&store, "A", None), artwork(&store, "B", None), artwork(&store, "C", None)]);
    let last = artwork(&store, "Last", None);
    store.move_entry(first, 0, None);
    store.move_entry(stack.clone(), 1, None);
    store.move_entry(last, 2, None);
    let inside = shown(&store, Some(stack.clone()));
    let before = shown(&store, None);
    fs::remove_file(Path::new(&stack).join("stack.json")).unwrap();
    let after = shown(&store, None);
    println!("before: {before:?}\nafter its name file went: {after:?}");
    assert_eq!(after, before);
    assert_eq!(shown(&store, Some(stack)), inside);
    assert_eq!(store.recover_lost_artworks(), 0);
    let _ = fs::remove_dir_all(&root);
}

/// On a full disk, Stack, Duplicate and Gallery Recovery leave every artwork in sight: a stack whose
/// name file can't be written isn't made (its artworks would be moved out of sight), nor a copy.
#[test]
fn stacking_copying_and_recovering_on_a_full_disk_leave_every_artwork_in_sight() {
    let _one = simulated::ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    simulated::fresh();
    let root = root("full-disk");
    let store = open(&root);
    let loose: Vec<String> = (0..4).map(|i| artwork(&store, &format!("Loose {i}"), None)).collect();
    let stack = store.stack(vec![artwork(&store, "In 0", None), artwork(&store, "In 1", None)]);
    // one hidden too deep for the gallery, for recovery to find
    let deep = artwork(&store, "Deep", Some(stack.clone()));
    let deeper = Path::new(&deep).join("inner");
    fs::rename(&deep, root.join("moved")).unwrap();
    fs::create_dir_all(&deeper).unwrap();
    fs::rename(root.join("moved"), &deeper).unwrap();
    let before = all_shown(&store);
    let made = before.len();

    let disk = FullDisk::at(&root, 0);
    store.stack(loose[..2].to_vec());
    let after_stack = all_shown(&store);
    store.duplicate(stack.clone());
    let after_copy = all_shown(&store);
    let recovered = store.recover_lost_artworks();
    let after_recovery = all_shown(&store);
    println!(
        "artworks in sight: {made} before, {} after Stack, {} after Duplicate, {} after recovery ({recovered} found); \
         {} writes refused",
        after_stack.len(),
        after_copy.len(),
        after_recovery.len(),
        disk.refused()
    );
    assert!(disk.refused() > 0);
    assert_eq!(after_stack, before);
    assert_eq!(after_copy, before);
    assert_eq!((after_recovery, recovered), (before.clone(), 0));

    // room again: each works
    disk.make_room(1 << 30);
    store.stack(loose[..2].to_vec());
    store.duplicate(stack);
    assert_eq!(store.recover_lost_artworks(), 1);
    assert_eq!(all_shown(&store).len(), made + 2 + 1);
    drop(disk);
    simulated::fresh();
    let _ = fs::remove_dir_all(&root);
}

/// A copy (Duplicate) the app is ended part way through is taken away when it next starts: never
/// half a stack in the gallery, nor artworks hidden for recovery to find; one ended once whole is
/// in the gallery whole.
#[test]
fn a_copy_ended_part_way_is_taken_away_and_one_ended_whole_is_kept() {
    let mut runs = Vec::new();
    for step in 0.. {
        let root = root(&format!("copy-{step}"));
        let store = open(&root);
        let stack =
            store.stack(vec![artwork(&store, "A", None), artwork(&store, "B", None), artwork(&store, "C", None)]);
        let single = artwork(&store, "Single", None);
        cut_short::after(Some(step));
        let ended = catch_unwind(AssertUnwindSafe(|| {
            store.duplicate(stack.clone());
            store.duplicate(single.clone());
        }))
        .is_err();
        cut_short::after(None);
        let store = start_again(&root);
        let mut names = shown(&store, None);
        names.sort();
        let recovered = store.recover_lost_artworks();
        runs.push((step, names.clone(), recovered));
        let _ = fs::remove_dir_all(&root);
        // the originals, and each copy whole or not at all
        assert!(names.contains(&"Single".to_owned()), "{names:?}");
        assert!(names.iter().filter(|n| n.starts_with("Stack[")).all(|n| n.matches(',').count() == 2), "{names:?}");
        assert!((1..=2).contains(&names.iter().filter(|n| n.starts_with("Stack[")).count()), "{names:?}");
        assert_eq!(recovered, 0, "step {step}: {names:?}");
        if !ended {
            assert_eq!(names.len(), 4, "{names:?}");
            break;
        }
    }
    for (step, names, _) in &runs {
        println!("ended after step {step}: {names:?}");
    }
    assert!(runs.len() >= 5);
}

/// The gallery and the home screen's widget (each its own store) reading the gallery at once,
/// each writing the order as new artworks come in, never take the order away nor cut it short:
/// the order the person gave stays.
#[test]
fn the_gallery_and_the_widget_reading_at_once_keep_the_order_the_person_gave() {
    const MINE: usize = 12;
    const NEW: usize = 40;
    let root = root("two-readers");
    let store = open(&root);
    let mine: Vec<String> = (0..MINE).map(|i| artwork(&store, &format!("Mine {i:02}"), None)).collect();
    // oldest first: the other way round from a new gallery's
    for (i, dir) in mine.iter().enumerate() {
        store.move_entry(dir.clone(), i as i64, None);
    }
    let given: Vec<String> = shown(&store, None);
    assert_eq!(given, (0..MINE).map(|i| format!("Mine {i:02}")).collect::<Vec<_>>());
    let stop = AtomicBool::new(false);
    let (missing, reads) = (AtomicUsize::new(0), AtomicUsize::new(0));
    std::thread::scope(|scope| {
        for _ in 0..3 {
            scope.spawn(|| {
                let widget = open(&root);
                while !stop.load(Ordering::Relaxed) {
                    widget.list(None);
                    reads.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        scope.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                if !root.join("order.txt").is_file() {
                    missing.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        for i in 0..NEW {
            artwork(&store, &format!("New {i:02}"), None);
            store.list(None);
        }
        stop.store(true, Ordering::Relaxed);
    });
    let after: Vec<String> = shown(&store, None).into_iter().filter(|n| n.starts_with("Mine")).collect();
    let in_sight = all_shown(&store).len();
    println!(
        "{NEW} artworks made while 3 widgets read the gallery {} times: order file gone {} times, the given order {}",
        reads.load(Ordering::Relaxed),
        missing.load(Ordering::Relaxed),
        if after == given { "kept" } else { "lost" }
    );
    assert_eq!(in_sight, MINE + NEW);
    assert_eq!(missing.load(Ordering::Relaxed), 0);
    assert_eq!(after, given);
    let _ = fs::remove_dir_all(&root);
}
