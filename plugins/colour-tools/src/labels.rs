//! Colour labels: Colour label, in a layer's options (a group's too), shows a panel of eight
//! colours; the one tapped labels the layer, and No label takes it off. Each layer keeps its label
//! as a mark the app saves with it (1 to 8, the colour's place), and the Layers panel shows the
//! colour on its row as the `layer_label` hook answers. Undo takes a label back as any change. All
//! of it while its switch is on: off, the marks stay and show nothing.

use kit::app::{self, Layer, Panel};

use crate::{COLOUR_LABELS as FEATURE, CONTROLS};

/// The labels' colours (0xRRGGBB), a layer's mark the place of its own, from 1: red, orange, yellow,
/// green, teal, blue, purple and grey.
const COLOURS: [u32; 8] = [0xE5484D, 0xF2862E, 0xEFC11F, 0x46A758, 0x14A6A0, 0x3E7BFA, 0x9A5CD0, 0x8B8D98];

/// The layer the panel labels: the current one as Colour label was tapped.
static LABELLING: core::sync::atomic::AtomicI64 = core::sync::atomic::AtomicI64::new(0);

/// The colour a layer marked `mark` shows: none for a mark that is no label.
fn colour_of(mark: i64) -> Option<u32> {
    usize::try_from(mark).ok()?.checked_sub(1).and_then(|at| COLOURS.get(at)).copied()
}

/// The panel, the label of the layer marked `mark` shown chosen.
fn panel(mark: i64) -> Panel {
    let chosen = colour_of(mark).map(|_| mark as usize - 1);
    Panel::new("Colour label").colours("Colour", &COLOURS, chosen).button("No label")
}

/// Colour label tapped: the panel, for the current layer.
pub fn act() {
    if !CONTROLS.on(FEATURE) {
        return;
    }
    let Some(layer) = app::current_layer() else { return };
    LABELLING.store(layer.0, core::sync::atomic::Ordering::Relaxed);
    panel(app::layer_mark(layer)).show();
}

/// A touch on the panel: row `row` (the colours, or No label) told `value`.
pub fn touched(row: usize, value: f32) {
    if !CONTROLS.on(FEATURE) {
        return;
    }
    let layer = Layer(LABELLING.load(core::sync::atomic::Ordering::Relaxed));
    let mark = match row {
        0 if value >= 0.0 && (value as usize) < COLOURS.len() => value as i64 + 1,
        1 => 0,
        _ => return,
    };
    app::set_layer_mark(layer, mark);
    app::close_panel();
}

/// The colour (0xRRGGBB) the row of a layer marked `mark` shows, -1 for none.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_layer_label(mark: i64) -> i32 {
    if !CONTROLS.on(FEATURE) {
        return -1;
    }
    colour_of(mark).map_or(-1, |c| c as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn each_mark_from_1_to_8_is_its_colour_and_any_other_none() {
        let _turn = crate::fresh(FEATURE);
        assert_eq!(procraft_layer_label(1), 0xE5484D);
        assert_eq!(procraft_layer_label(8), 0x8B8D98);
        for mark in [0, -1, 9, i64::MAX, i64::MIN] {
            assert_eq!(procraft_layer_label(mark), -1, "{mark}");
        }
        // eight, each its own
        let mut seen = COLOURS.to_vec();
        seen.dedup();
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn the_panel_shows_the_layer_s_label_chosen() {
        let json = panel(3).json();
        assert!(json.contains(r#""type":"colours""#) && json.contains(r#""chosen":2"#), "{json}");
        assert!(json.contains(&COLOURS[0].to_string()) && json.contains("No label"), "{json}");
        let json = panel(0).json();
        assert!(!json.contains("chosen"), "{json}");
    }

    #[test]
    fn with_its_switch_off_no_mark_shows_and_its_action_and_panel_do_nothing() {
        let _turn = crate::fresh(FEATURE);
        assert_eq!(procraft_layer_label(3), 0xEFC11F);
        procraft_control(FEATURE as i32, 0.0);
        for mark in [1, 3, 8] {
            assert_eq!(procraft_layer_label(mark), -1, "{mark}");
        }
        // off the app there is no layer either way: the action and the panel go by, nothing set
        LABELLING.store(7, core::sync::atomic::Ordering::Relaxed);
        crate::procraft_action(crate::LABEL as i32);
        crate::procraft_panel_event(0, 2.0);
        assert_eq!(LABELLING.load(core::sync::atomic::Ordering::Relaxed), 7);
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(procraft_layer_label(3), 0xEFC11F);
    }
}
