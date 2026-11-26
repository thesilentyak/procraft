//! Trace view: how the layers show on the screen. Its Layer view is Normal (every layer as it is),
//! Solo (the layer you're on alone) or Trace (the layer you're on as it is, the others dimmed as far
//! as its slider says, to trace over them). It's set from its row in the Layers panel, which opens a
//! panel of it, from the Actions panel's Canvas tab or from its card, and Alt+S and Alt+T toggle Solo
//! and Trace (on it, back to Normal). It's the screen alone: what is saved, exported and picked is
//! as it is. Off, every layer shows as it is, and its row and keys do nothing.
//!
//! The app asks the dimming hook how much the layers but the current one are dimmed, as the
//! controls change: none, all the way, or the slider's.

use crate::{CONTROLS, DIM, SOLO_KEY, TRACE_KEY, TRACE_VIEW as FEATURE, VIEW};

/// The Layer view control's steps, by their names.
const VIEWS: [&str; 3] = ["Normal", "Solo", "Trace"];
const NORMAL: f32 = 0.0;
const SOLO: f32 = 1.0;
const TRACE: f32 = 2.0;

/// How much the layers but the current one are dimmed in `view` (a step of Layer view): none for
/// Normal, all the way for Solo, `dim` (0 to 1) for Trace. A view past its steps or a dim that isn't
/// a number dims none.
fn dimmed(view: f32, dim: f32) -> f32 {
    match view.round() {
        SOLO => 1.0,
        TRACE if dim.is_finite() => dim.clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// The view a key for `to` (Solo or Trace) turns `now` into: back to Normal from it, else it.
fn toggled(now: f32, to: f32) -> f32 {
    if now.round() == to { NORMAL } else { to }
}

/// How much the layers but the current one are dimmed on the screen now: none with its switch off.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_view_dim_others() -> f32 {
    if !CONTROLS.on(FEATURE) {
        return 0.0;
    }
    dimmed(CONTROLS.get(VIEW), CONTROLS.get(DIM))
}

/// The app's function that sets one of the plugin's own controls (interface 12).
#[cfg(target_arch = "wasm32")]
mod app {
    #[link(wasm_import_module = "procraft")]
    unsafe extern "C" {
        pub fn set_control(at: i32, value: f32) -> i32;
    }
}

/// Control `at` set to `value`, as on the card (the app keeps it), and told here at once.
fn set(at: usize, value: f32) {
    // SAFETY: plain numbers; the app checks them
    #[cfg(target_arch = "wasm32")]
    unsafe {
        app::set_control(at as i32, value);
    }
    CONTROLS.told(at as i32, value);
}

/// The panel the Layers panel's Layer view row opens, as the controls are now: the view, and how
/// far Trace dims the others.
fn panel() -> kit::app::Panel {
    let view = (CONTROLS.get(VIEW).round().max(0.0) as usize).min(VIEWS.len() - 1);
    let dim = (CONTROLS.get(DIM) * 100.0).round();
    kit::app::Panel::new("Layer view").choice("Layers shown", &VIEWS, view).slider(
        "Trace dims the others",
        10.0,
        95.0,
        5.0,
        dim,
        "%",
    )
}

/// Its action at place `index` tapped: a key toggling Solo or Trace, or its row in the Layers
/// panel opening its panel.
pub fn act(index: usize) {
    if !CONTROLS.on(FEATURE) {
        return;
    }
    match index {
        SOLO_KEY => set(VIEW, toggled(CONTROLS.get(VIEW), SOLO)),
        TRACE_KEY => set(VIEW, toggled(CONTROLS.get(VIEW), TRACE)),
        _ => {
            panel().show();
        }
    }
}

/// A touch on its panel: row `row` (the view, or how far Trace dims) told `value`.
pub fn touched(row: usize, value: f32) {
    if !CONTROLS.on(FEATURE) {
        return;
    }
    match row {
        0 => set(VIEW, value),
        1 => set(DIM, value * 0.01),
        _ => {}
    }
    panel().show();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LAYER_VIEW, procraft_action, procraft_control};

    #[test]
    fn normal_dims_none_solo_all_the_way_and_trace_as_its_slider_says() {
        assert_eq!(dimmed(NORMAL, 0.7), 0.0);
        assert_eq!(dimmed(SOLO, 0.7), 1.0);
        assert_eq!(dimmed(TRACE, 0.7), 0.7);
        assert_eq!(dimmed(TRACE, 0.1), 0.1);
        // a dim past its range no further, one that isn't a number none; a view past its steps
        // (or not a number) is Normal
        assert_eq!(dimmed(TRACE, 3.0), 1.0);
        assert_eq!(dimmed(TRACE, -1.0), 0.0);
        assert_eq!(dimmed(TRACE, f32::NAN), 0.0);
        assert_eq!(dimmed(5.0, 0.7), 0.0);
        assert_eq!(dimmed(f32::NAN, 0.7), 0.0);
    }

    #[test]
    fn a_key_turns_its_view_on_and_back_to_normal() {
        assert_eq!(toggled(NORMAL, TRACE), TRACE);
        assert_eq!(toggled(TRACE, TRACE), NORMAL);
        assert_eq!(toggled(SOLO, TRACE), TRACE);
        assert_eq!(toggled(TRACE, SOLO), SOLO);
        assert_eq!(toggled(SOLO, SOLO), NORMAL);
    }

    #[test]
    fn the_keys_and_the_panel_set_the_view_and_the_app_is_told_the_dim() {
        let _turn = crate::fresh(FEATURE);
        // as it starts: Normal
        assert_eq!(procraft_view_dim_others(), 0.0);
        procraft_action(TRACE_KEY as i32);
        assert_eq!((CONTROLS.get(VIEW), procraft_view_dim_others()), (TRACE, 0.7));
        procraft_action(SOLO_KEY as i32);
        assert_eq!((CONTROLS.get(VIEW), procraft_view_dim_others()), (SOLO, 1.0));
        procraft_action(SOLO_KEY as i32);
        assert_eq!((CONTROLS.get(VIEW), procraft_view_dim_others()), (NORMAL, 0.0));
        // the panel: Trace chosen, then its dim at 40%
        touched(0, TRACE);
        touched(1, 40.0);
        assert!((procraft_view_dim_others() - 0.4).abs() < 1e-6);
        // and as it reads the controls back
        assert!(panel().json().contains(r#""chosen":2"#) && panel().json().contains(r#""value":40"#));
    }

    #[test]
    fn with_its_switch_off_every_layer_shows_and_its_keys_and_panel_do_nothing() {
        let _turn = crate::fresh(FEATURE);
        procraft_action(SOLO_KEY as i32);
        assert_eq!((CONTROLS.get(VIEW), procraft_view_dim_others()), (SOLO, 1.0));
        procraft_control(FEATURE as i32, 0.0);
        assert_eq!(procraft_view_dim_others(), 0.0);
        // the keys, its row and its panel's touches leave the view and the dim as they were
        procraft_action(TRACE_KEY as i32);
        procraft_action(SOLO_KEY as i32);
        procraft_action(LAYER_VIEW as i32);
        touched(0, TRACE);
        touched(1, 40.0);
        assert_eq!((CONTROLS.get(VIEW), CONTROLS.get(DIM)), (SOLO, 0.7));
        procraft_control(FEATURE as i32, 1.0);
        assert_eq!(procraft_view_dim_others(), 1.0);
    }
}
