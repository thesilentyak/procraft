//! Storage folder: the artworks, and the time-lapse videos exported, kept in a folder you pick as
//! well as in the app, and what changed there brought into the gallery: a folder kept in step
//! between devices by a sync app, say. It's a plugin, not part of the app: `tools/pack_plugin.py`
//! makes a `.procraft-plugin` file of it.
//!
//! It only names its folder on each of its three hooks while that feature is on (interface 22):
//! `artwork_saved` (Keep artworks in a folder), `video_exported` (Save time-lapse videos there) and
//! `gallery_opened` (Bring in changes from the folder). The app does the rest: the package of each
//! artwork, its name, writing it whole, which copy is newer, keeping both when both changed, and
//! never taking anything away from the folder. The interface is written down at the top of
//! `crates/core/src/plugin_run.rs`.

#![no_std]

/// The controls by their place in the manifest, as they start: no folder picked, every feature off.
static CONTROLS: kit::Controls<4> = kit::Controls::new([0.0; 4]);
kit::controls!(CONTROLS);

/// The folder (told 1 while one is picked), then the three features' switches.
pub const FOLDER: usize = 0;
pub const KEEP_ARTWORKS: usize = 1;
pub const KEEP_VIDEOS: usize = 2;
pub const BRING_IN: usize = 3;

/// The folder's place in the manifest while `feature` is on and a folder is picked, else -1: none.
fn folder_while(feature: usize) -> i32 {
    if CONTROLS.on(feature) && CONTROLS.on(FOLDER) { FOLDER as i32 } else { -1 }
}

kit::answer!(procraft_artwork_saved, || folder_while(KEEP_ARTWORKS));
kit::answer!(procraft_video_exported, || folder_while(KEEP_VIDEOS));
kit::answer!(procraft_gallery_opened, || folder_while(BRING_IN));

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
