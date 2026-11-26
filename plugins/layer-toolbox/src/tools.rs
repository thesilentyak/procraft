//! Layer tools, each on while its switch is (under the feature's): with several layers picked in
//! the Layers panel, changing one's opacity sets them all to the same, as one undo step; with a
//! mask selected, Paste puts the picture's brightness into the mask; and the eyedropper reads the
//! current layer alone, not the whole canvas. Off, each is as the app has it.
//!
//! The app asks whether the picked layers go together and which pixel the eyedropper reads, and
//! where a pasted picture goes; if into the mask, it hands the picture over a row at a time and
//! writes back what it gets. The layers and reading a pixel stay the app's.

use crate::{CONTROLS, EYEDROPPER, LAYER_TOOLS as FEATURE, OPACITY, PASTE};

/// Whether switch `switch` is on, under the feature's.
fn on(switch: usize) -> bool {
    CONTROLS.on(FEATURE) && CONTROLS.on(switch)
}

kit::yes!(procraft_opacity_together, || on(OPACITY));
// whether a picture pasted with a mask current goes into the mask
kit::yes!(procraft_paste_mask, || on(PASTE));
// whether the eyedropper reads the current layer
kit::yes!(procraft_pick_layer, || on(EYEDROPPER));

/// One premultiplied pixel, red in its low byte, as its brightness, solid: what is clear counts as
/// black, so it hides.
fn brightness(pixel: u32) -> u32 {
    let (r, g, b) = (pixel & 255, pixel >> 8 & 255, pixel >> 16 & 255);
    let grey = ((54 * r + 183 * g + 19 * b + 128) >> 8).min(255);
    0xff00_0000 | (grey * 0x01_0101)
}

/// Where a row of `bytes` goes: pages grown past what the module has, so nothing else here is in
/// them, and kept for the next.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn procraft_row_buffer(bytes: i32) -> i32 {
    use core::sync::atomic::{AtomicI32, Ordering};
    /// Where the row buffer is, and how many pages of memory it has.
    static BUFFER: AtomicI32 = AtomicI32::new(-1);
    static PAGES: AtomicI32 = AtomicI32::new(0);
    const PAGE: i32 = 65536;
    let pages = (bytes + PAGE - 1) / PAGE;
    if pages > PAGES.load(Ordering::Relaxed) {
        let at = core::arch::wasm32::memory_grow(0, pages as usize);
        if at == usize::MAX {
            return -1;
        }
        BUFFER.store(at as i32 * PAGE, Ordering::Relaxed);
        PAGES.store(pages, Ordering::Relaxed);
    }
    BUFFER.load(Ordering::Relaxed)
}

/// The `width` pixels at `at` as what the mask holds, in place.
///
/// # Safety
/// The app calls this with what [`procraft_row_buffer`] gave it, holding a row of that length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn procraft_paste_row(at: i32, width: i32) {
    let row = unsafe { core::slice::from_raw_parts_mut(at as *mut u32, width as usize) };
    for pixel in row {
        *pixel = brightness(*pixel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn a_pixel_comes_out_as_its_brightness_and_solid() {
        assert_eq!(brightness(0xffff_ffff), 0xffff_ffff);
        // clear counts as black, which hides
        assert_eq!(brightness(0), 0xff00_0000);
        // red, green and blue by how bright each looks
        assert_eq!(brightness(0xff00_00ff), 0xff36_3636);
        assert_eq!(brightness(0xff00_ff00), 0xffb6_b6b6);
        assert_eq!(brightness(0xffff_0000), 0xff13_1313);
    }

    #[test]
    fn each_tool_answers_only_while_its_switch_is_on() {
        let _turn = crate::fresh(FEATURE);
        let said = || (procraft_opacity_together(), procraft_paste_mask(), procraft_pick_layer());
        assert_eq!(said(), (0, 0, 0));
        for (switch, on) in [(OPACITY, (1, 0, 0)), (PASTE, (1, 1, 0)), (EYEDROPPER, (1, 1, 1))] {
            procraft_control(switch as i32, 1.0);
            assert_eq!(said(), on);
        }
        procraft_control(PASTE as i32, 0.0);
        assert_eq!(said(), (1, 0, 1));
    }

    #[test]
    fn with_its_switch_off_each_tool_is_the_app_s_own() {
        let _turn = crate::fresh(FEATURE);
        let said = || (procraft_opacity_together(), procraft_paste_mask(), procraft_pick_layer());
        for switch in [OPACITY, PASTE, EYEDROPPER] {
            procraft_control(switch as i32, 1.0);
        }
        assert_eq!(said(), (1, 1, 1));
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!(said(), (0, 0, 0));
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(said(), (1, 1, 1));
    }
}
