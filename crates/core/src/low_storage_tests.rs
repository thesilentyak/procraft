//! A device nearly out of storage: an artwork opens, paints, keeps itself saved and closes on a
//! disk that is full, nothing crashing; its last save stays byte for byte as it was, nothing half
//! written left beside it; the time-lapse pauses; and the person is told once.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use procraft_engine::view::View;

use crate::storage::{FullDisk, NO_ROOM, simulated};
use crate::tests::{pixel, sample, session};
use crate::timelapse::TimeLapse;
use crate::{CanvasSession, PenSample, PointerKind};

/// The canvas's side, pixels: big enough that a save never fits in the room the tests leave.
const SIDE: u32 = 512;
const STROKES: usize = 100;

/// A folder's files, by their paths in it, with what each holds.
type Folder = BTreeMap<PathBuf, Vec<u8>>;

/// Every file under `dir`, by its path there, with what it holds.
fn files(dir: &Path) -> Folder {
    fn walk(root: &Path, at: &Path, out: &mut Folder) {
        for entry in std::fs::read_dir(at).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(path.strip_prefix(root).unwrap().to_path_buf(), std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// Stroke `i` of a run: across the canvas, each its own colour, and the time-lapse asked for a
/// frame after it as the apps ask.
fn paint(s: &CanvasSession, lapse: &Arc<TimeLapse>, i: usize) -> bool {
    let f = i as f32;
    s.set_color((f * 0.37).fract(), (f * 0.61).fract(), (f * 0.13).fract());
    let y = 40.0 + (f * 7.3) % (SIDE as f32 - 80.0);
    s.stroke_begin(PointerKind::Stylus, vec![sample(30.0, y, 0.0)]);
    for k in 1..12 {
        let x = 30.0 + k as f32 * 38.0;
        s.stroke_add(vec![PenSample { pressure: 0.6, ..sample(x, y + (x / 50.0 + f).sin() * 30.0, k as f64 * 8.0) }]);
    }
    s.stroke_end(vec![]);
    let _ = pixel(s, 0, 0);
    s.record_time_lapse(Arc::clone(lapse))
}

/// What a run on a full disk came to.
struct FullRun {
    room: u64,
    opened: Duration,
    stroke_most: Duration,
    autosave_looks: u64,
    frames_tried: usize,
    frames_paused: usize,
    refused: u64,
    taken: u64,
    told: Vec<String>,
}

/// An artwork saved with a few strokes and time-lapse frames on a disk with room, then opened on
/// a full one (`share` of its smallest frame's bytes left: no frame fits, nor a save), painted with [`STROKES`] strokes while it keeps itself saved and
/// records its time-lapse, and closed; with its folder as saved before (to compare) and after.
fn on_a_full_disk(share: (u64, u64)) -> Option<(FullRun, Folder, Folder)> {
    let root = std::env::temp_dir().join(format!("procraft-full-disk-{}-{}", share.0, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.join("artwork");
    let dir_text = dir.to_string_lossy().into_owned();
    // saved where there is room: three strokes, and their frames
    let first = session(SIDE, SIDE)?;
    let gpu = first.shared.lock().gpu.clone()?;
    let lapse = TimeLapse::new(dir_text.clone());
    for i in 0..3 {
        assert!(paint(&first, &lapse, i), "a frame kept while there is room");
    }
    first.save(dir_text.clone()).unwrap();
    drop((first, lapse));
    let before = files(&dir);
    let frame_least = before
        .iter()
        .filter(|(p, _)| p.extension().is_some_and(|e| e == "jpg"))
        .map(|(_, bytes)| bytes.len() as u64)
        .min()
        .unwrap();
    let room = frame_least * share.0 / share.1;

    // the disk full; the marks of what is being opened, and the colours painted with, kept on it too
    let disk = FullDisk::at(&root, room);
    crate::open_marks::use_file_on_this_thread(Some(root.join("opening.txt")));
    let started = Instant::now();
    let s = CanvasSession::open(dir_text.clone()).expect("opens with no room to write");
    s.use_gpu(gpu);
    {
        let mut st = s.shared.lock();
        st.window_size = (SIDE, SIDE);
        st.view = Some(View::fit((SIDE, SIDE), (SIDE, SIDE)));
    }
    s.use_colour_history(crate::history::ColourHistory::open(root.join("colours.json").to_string_lossy().into_owned()));
    let lapse = TimeLapse::new(dir_text.clone());
    let opened = started.elapsed();
    let frames_before = lapse.count();
    Arc::clone(&s).keep_saved(dir_text.clone(), Duration::from_millis(20), Duration::from_millis(5));

    let mut told = Vec::new();
    let mut take = |s: &CanvasSession, lapse: &TimeLapse| {
        told.extend(s.take_save_failure().and_then(crate::save_failure_to_tell));
        told.extend(lapse.take_problem());
    };
    let (mut stroke_most, mut frames_tried, mut frames_paused) = (Duration::ZERO, 0, 0);
    for i in 0..STROKES {
        let paused = lapse.waits_for_room();
        let t = Instant::now();
        let kept = paint(&s, &lapse, 3 + i);
        stroke_most = stroke_most.max(t.elapsed());
        assert!(!kept, "no frame kept on a full disk");
        if paused {
            frames_paused += 1
        } else {
            frames_tried += 1
        }
        take(&s, &lapse);
        // the autosave gets its turns between strokes, as it does between a person's
        std::thread::sleep(Duration::from_millis(5));
    }
    // the autosave's look after the last stroke
    let edits = s.edits();
    let until = Instant::now() + Duration::from_secs(20);
    while s.shared.counts.autosave_looks.load(std::sync::atomic::Ordering::Relaxed) < 2 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(200));
    take(&s, &lapse);
    assert_eq!(s.edits(), edits);
    assert_eq!(s.edits_saved(), None, "no save fits");
    // every stroke still there, from memory
    assert!(s.layers().iter().any(|l| s.copy_layer_contents(l.id).is_some_and(|c| c.rgba.iter().any(|&v| v != 0))));

    // closing: the autosave ended, the last save tried, the session let go
    s.stop_autosave();
    let closing = s.save(dir_text.clone());
    let reason = closing.as_ref().err().map(ToString::to_string).unwrap_or_default();
    assert!(crate::storage::says_full(&reason), "the closing save failed for want of room: {reason}");
    told.extend(crate::save_failure_to_tell(reason));
    let autosave_looks = s.shared.counts.autosave_looks.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(lapse.count(), frames_before, "no frame counted that wasn't kept");
    drop((s, lapse));
    crate::open_marks::use_file_on_this_thread(None);
    let after = files(&dir);
    let run = FullRun {
        room,
        opened,
        stroke_most,
        autosave_looks,
        frames_tried,
        frames_paused,
        refused: disk.refused(),
        taken: disk.taken(),
        told,
    };
    drop(disk);
    let _ = std::fs::remove_dir_all(&root);
    Some((run, before, after))
}

/// On a disk with no room, with a little, and with somewhat more (all short of a save's): the
/// artwork opens, takes 100 strokes as the autosave and the time-lapse keep failing, and closes,
/// nothing crashing; its folder is then byte for byte as last saved, nothing half written left in
/// it; the time-lapse tried a frame once and paused; and the notice was told exactly once.
#[test]
fn an_artwork_opens_paints_and_closes_on_a_full_disk_with_its_last_save_whole() {
    let _one = simulated::ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    for share in [(0, 1), (1, 4), (3, 4)] {
        simulated::fresh();
        let Some((run, before, after)) = on_a_full_disk(share) else { return };
        eprintln!(
            "{} bytes of room: opened in {:?}; {} strokes, the longest {:?}; {} autosave looks; time-lapse frames: {} tried, {} skipped paused; {} writes refused, {} bytes taken; told {:?}",
            run.room,
            run.opened,
            STROKES,
            run.stroke_most,
            run.autosave_looks,
            run.frames_tried,
            run.frames_paused,
            run.refused,
            run.taken,
            run.told,
        );
        assert!(before.keys().any(|p| p.starts_with("layers")) && before.contains_key(Path::new("artwork.json")));
        let changed: Vec<&PathBuf> = after.keys().filter(|p| before.get(*p) != after.get(*p)).collect();
        let gone: Vec<&PathBuf> = before.keys().filter(|p| !after.contains_key(*p)).collect();
        assert!(changed.is_empty() && gone.is_empty(), "changed {changed:?}, gone {gone:?}");
        assert_eq!(before, after, "the last save byte for byte as it was");
        assert_eq!(run.told, [NO_ROOM], "told exactly once, plainly");
        assert!(run.refused >= 2, "the autosave and the time-lapse both met the full disk ({} refused)", run.refused);
        assert!(run.autosave_looks >= 1);
        assert_eq!(run.frames_tried, 1, "one frame tried, then the time-lapse paused");
        assert_eq!(run.frames_paused, STROKES - 1);
        assert!(run.opened < Duration::from_secs(10), "{:?}", run.opened);
    }
    simulated::fresh();
}

/// Once room is made, the next save works and says so: the time-lapse records again, and a disk
/// that fills again is told again, once.
#[test]
fn room_made_saves_again_and_a_disk_that_fills_again_is_told_again_once() {
    let _one = simulated::ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    simulated::fresh();
    let root = std::env::temp_dir().join(format!("procraft-room-again-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.join("artwork").to_string_lossy().into_owned();
    let Some(s) = session(256, 256) else { return };
    let lapse = TimeLapse::new(dir.clone());
    // the disk full, five strokes each saved and recorded as they end: what was told
    let fill_and_paint = |from: usize| {
        let disk = FullDisk::at(&root, 0);
        let mut told = Vec::new();
        for i in from..from + 5 {
            paint(&s, &lapse, i);
            let failed = s.save(dir.clone()).err().map(|e| e.to_string());
            told.extend(failed.and_then(crate::save_failure_to_tell));
            told.extend(lapse.take_problem());
        }
        assert!(lapse.waits_for_room(), "paused");
        (disk, told)
    };
    let (disk, told) = fill_and_paint(0);
    assert_eq!(told, [NO_ROOM]);
    // room made: the save works, the time-lapse records again
    drop(disk);
    s.save(dir.clone()).unwrap();
    assert!(!crate::storage::full());
    assert!(!lapse.waits_for_room());
    let frames = lapse.count();
    assert!(paint(&s, &lapse, 10), "recording again");
    assert_eq!(lapse.count(), frames + 1);
    // full again: told again, once
    let (disk, told) = fill_and_paint(20);
    assert_eq!(told, [NO_ROOM]);
    // its frame kept while there was room is still a picture: no video of it is made only to be refused
    assert!(lapse.segment_plan().is_none(), "a video planned on a full disk");
    drop(disk);
    s.save(dir.clone()).unwrap();
    assert!(lapse.segment_plan().is_some_and(|p| !p.frames.is_empty()), "planned once a save finds room");
    drop((s, lapse));
    let _ = std::fs::remove_dir_all(&root);
    simulated::fresh();
}

/// Leaving an artwork whose save fails for want of room never loses it: it stays open with every
/// stroke and is asked about (stay, or leave without saving, saying what that loses); kept saved
/// again, it saves on its own once there is room. Leaving without saving, the person's choice,
/// leaves its last save byte for byte as it was. One never saved says leaving loses it.
#[test]
fn leaving_with_a_failed_save_keeps_the_canvas_and_leaving_without_saving_keeps_the_last_save() {
    let _one = simulated::ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    simulated::fresh();
    let root = std::env::temp_dir().join(format!("procraft-leave-unsaved-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.join("artwork");
    let dir_text = dir.to_string_lossy().into_owned();
    let Some(s) = session(256, 256) else { return };
    let lapse = TimeLapse::new(dir_text.clone());
    let contents = |s: &CanvasSession| -> Vec<Vec<u8>> {
        s.layers().iter().filter_map(|l| s.copy_layer_contents(l.id)).map(|c| c.rgba).collect()
    };
    for i in 0..3 {
        paint(&s, &lapse, i);
    }
    assert_eq!(Arc::clone(&s).save_to_leave(dir_text.clone()), crate::Leaving::Left);
    let saved = files(&dir);

    // the disk full: leaving keeps it open, every stroke there, and asks
    let disk = FullDisk::at(&root, 0);
    for i in 3..8 {
        paint(&s, &lapse, i);
    }
    let (edits, painted) = (s.edits(), contents(&s));
    let started = Instant::now();
    let left = Arc::clone(&s).save_to_leave(dir_text.clone());
    let took = started.elapsed();
    let crate::Leaving::Stays { title, message } = left else { panic!("left with work only in memory") };
    eprintln!("a failed save to leave: asked in {took:?}: {title} / {message}");
    assert_eq!(title, "Not enough storage");
    assert_eq!(
        message,
        "Free some space to keep saving. Leaving without saving loses the changes made since it was last saved."
    );
    assert_eq!(s.edits(), edits);
    assert!(contents(&s) == painted, "every stroke still on the canvas");
    assert_eq!(files(&dir), saved, "the last save as it was");

    // room made: kept saved again, it saves on its own (a stroke more is the change it looks at)
    drop(disk);
    paint(&s, &lapse, 8);
    let edits = s.edits();
    let saved_by_itself = (0..20_000).any(|_| {
        std::thread::sleep(Duration::from_millis(1));
        s.edits_saved() == Some(edits)
    });
    assert!(saved_by_itself, "the autosave saved what leaving couldn't once there was room");
    assert!(!crate::storage::full(), "a save worked: room again");
    let saved = files(&dir);
    assert_ne!(saved.get(Path::new("artwork.json")), None);

    // full again, the person leaves without saving: the last save stays whole
    let disk = FullDisk::at(&root, 0);
    paint(&s, &lapse, 9);
    assert!(matches!(Arc::clone(&s).save_to_leave(dir_text.clone()), crate::Leaving::Stays { .. }));
    s.stop_autosave();
    drop((s, lapse));
    assert_eq!(files(&dir), saved, "left without saving: the last save byte for byte as it was");

    // never saved: leaving without saving loses the artwork, and says so
    let Some(new) = session(64, 64) else { return };
    new.stroke_begin(PointerKind::Finger, vec![sample(20.0, 20.0, 0.0)]);
    new.stroke_end(vec![]);
    let fresh = root.join("never-saved").to_string_lossy().into_owned();
    let crate::Leaving::Stays { message, .. } = Arc::clone(&new).save_to_leave(fresh) else { panic!("left unsaved") };
    assert_eq!(message, "Free some space to keep saving. Leaving without saving loses this artwork.");
    new.stop_autosave();
    drop((disk, new));
    let _ = std::fs::remove_dir_all(&root);
    simulated::fresh();
}
