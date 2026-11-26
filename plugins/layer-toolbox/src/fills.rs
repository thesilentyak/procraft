//! Layer fills, each there while its switch is on (under the feature's). Grid layer: its action adds
//! a layer named "Grid" over the current one, with lines across and down it in the brush's colour,
//! as far apart and as wide as its sliders say; one undo takes it all back. Paper texture: a filter
//! under Adjustments, `filters/paper.wgsl`, which puts paper behind the layer's paint, its panel
//! opening at the sliders here.
//!
//! It shows a plugin's action beside a filter: what the action may reach (the canvas's size, adding
//! and painting a layer, the brush's colour) its manifest asks for, and the plugin kit's `app`
//! calls it. The app lists the action, and the filter, only while its switch is on.

use kit::app;

use crate::{CONTROLS, GRID, LAYER_FILLS as FEATURE, LINE, SPACING};

/// Whether the grid's action does anything: its switch, under the feature's.
fn grid_on() -> bool {
    CONTROLS.on(FEATURE) && CONTROLS.on(GRID)
}

/// Add a grid layer tapped: the layer, its lines in the brush's colour.
pub fn add_grid() {
    if !grid_on() {
        return;
    }
    let (width, height) = app::canvas_size();
    if width == 0 || height == 0 {
        return;
    }
    let Some(layer) = app::add_layer() else {
        app::say("There's no room for another layer.");
        return;
    };
    app::name_layer(layer, "Grid");
    let spacing = (CONTROLS.get(SPACING).round() as u32).max(2);
    let line = (CONTROLS.get(LINE).round() as u32).clamp(1, spacing - 1);
    let rgb = app::brush_colour();
    let ink = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
    // each line a strip of its own: the lines down, then across
    let strip = |w: u32, h: u32| ink.repeat(w as usize * h as usize);
    for x in (0..width).step_by(spacing as usize) {
        let w = line.min(width - x);
        app::paint(layer, x as i32, 0, w, height, &strip(w, height));
    }
    for y in (0..height).step_by(spacing as usize) {
        let h = line.min(height - y);
        app::paint(layer, 0, y as i32, width, h, &strip(width, h));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procraft_control;

    #[test]
    fn with_no_artwork_it_does_nothing() {
        let _turn = crate::fresh(FEATURE);
        procraft_control(GRID as i32, 1.0);
        add_grid();
    }

    #[test]
    fn with_its_switch_off_the_grid_s_action_does_nothing() {
        let _turn = crate::fresh(FEATURE);
        procraft_control(GRID as i32, 1.0);
        assert!(grid_on());
        procraft_control(FEATURE as i32, 0.0);
        assert!(!grid_on());
        // tapped, it goes by (off the app there's no artwork either way)
        crate::procraft_action(crate::ADD_GRID as i32);
        procraft_control(FEATURE as i32, 1.0);
        procraft_control(GRID as i32, 0.0);
        assert!(!grid_on());
    }
}
