//! Edit layout: the interface's panels and bars where one wants them. Its feature Movable panels
//! lets the Layers panel and the brush library float in windows of their own (the `floating_panels`
//! hook), and has the floating panels, the colour panel and the Reference window among them, snap
//! to the window's edges and to each other, fold to a tab and, docked against an edge, make room for
//! the canvas (`panel_layout`, interface 20). All panels float lets every other panel float too (the
//! same hook's numbers from 2, interface 20). Panels stay open leaves panels open while one paints
//! (`pin_panels`). Its feature Edit layout lets the interface's bars be moved anywhere, from Edit
//! Layout in the Canvas tab (the same hook's part 3). Its feature Collapse to edge has a bar or a
//! window against a side slide into it at a button's tap, a tab left there (part 5). It's a plugin,
//! not part of the app:
//! `tools/pack_plugin.py` makes a `.procraft-plugin` file of it.
//!
//! The app does the rest: the windows, the snapping, the tabs, the room, and keeping each panel's
//! place for each way the screen turns. The interface is written down at the top of
//! `crates/core/src/plugin_run.rs`.

/// The controls by their place in the manifest, as they start: the features off, both panels
/// allowed to float, snapping within 16 points, folding, no room made, panels closing as usual.
const STARTS: [f32; 12] = [0.0, 1.0, 1.0, 1.0, 16.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
static CONTROLS: kit::Controls<12> = kit::Controls::new(STARTS);
kit::controls!(CONTROLS);

/// Movable panels' switch, then its own: whether the Layers panel and the brush library may float,
/// whether panels snap and how near (points), whether they fold to a tab, and whether those docked
/// make room for the canvas. Then All panels float's switch, and Panels stay open's.
pub const MOVABLE_PANELS: usize = 0;
pub const FLOAT_LAYERS: usize = 1;
pub const FLOAT_BRUSHES: usize = 2;
pub const SNAP: usize = 3;
pub const SNAP_DISTANCE: usize = 4;
pub const FOLD: usize = 5;
pub const MAKE_ROOM: usize = 6;
pub const ALL_PANELS: usize = 7;
pub const PIN_PANELS: usize = 8;
/// Tool Belt's switch, then Edit layout's, then Collapse to edge's.
pub const TOOL_BELT: usize = 9;
pub const EDIT_LAYOUT: usize = 10;
pub const COLLAPSE_EDGE: usize = 11;

/// The farthest the app lets panels snap from (points).
const MOST_SNAP: f32 = 64.0;

/// The panels Movable panels lets float, by their number: the Layers panel, then the brush library.
const FLOATING: [usize; 2] = [FLOAT_LAYERS, FLOAT_BRUSHES];

/// The other panels All panels float lets float (interface 20): the Actions panel 2, the Adjustments
/// menu 3, an adjustment's controls 4, the Selection toolbar 5, the Transform toolbar 6 and Liquify's
/// panel 7.
const OTHERS: std::ops::RangeInclusive<i32> = 2..=7;

/// Whether panel `panel` may be torn off into a window of its own: the Layers panel and the library
/// as Movable panels' switches say, the others while All panels float is on; a panel it doesn't know
/// stays where it hangs, as every panel does with the features off.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_floating_panel(panel: i32) -> i32 {
    if OTHERS.contains(&panel) {
        return i32::from(CONTROLS.on(ALL_PANELS));
    }
    let switch = usize::try_from(panel).ok().and_then(|at| FLOATING.get(at).copied());
    i32::from(CONTROLS.on(MOVABLE_PANELS) && switch.is_some_and(|switch| CONTROLS.on(switch)))
}

// a touch on the canvas with a panel open paints, the panel staying, while its feature is on
kit::yes!(procraft_pin_panels, || CONTROLS.on(PIN_PANELS));

/// How the floating panels are laid out, `part` by part: how near they snap (0, points; 0 with
/// snapping off), whether they fold to a tab (1) and whether those docked make room (2), 1 or 0,
/// while Movable panels is on; whether the bars may be moved (3), while Edit layout is; whether the
/// Tool Belt shows (4) and whether bars and windows collapse into the window's sides (5), each by its
/// own feature. Below 0 for a part it doesn't know, or with its feature off: as the app has it.
#[unsafe(no_mangle)]
pub extern "C" fn procraft_panel_layout(part: i32) -> f32 {
    if part == 3 {
        return if CONTROLS.on(EDIT_LAYOUT) { 1.0 } else { -1.0 };
    }
    // the Tool Belt, by its own feature
    if part == 4 {
        return if CONTROLS.on(TOOL_BELT) { 1.0 } else { -1.0 };
    }
    // Collapse to edge, by its own feature
    if part == 5 {
        return if CONTROLS.on(COLLAPSE_EDGE) { 1.0 } else { -1.0 };
    }
    if !CONTROLS.on(MOVABLE_PANELS) {
        return -1.0;
    }
    let yes = |switch: usize| if CONTROLS.on(switch) { 1.0 } else { 0.0 };
    match part {
        0 if CONTROLS.on(SNAP) => {
            let distance = CONTROLS.get(SNAP_DISTANCE);
            if distance.is_nan() { 0.0 } else { distance.clamp(0.0, MOST_SNAP) }
        }
        0 => 0.0,
        1 => yes(FOLD),
        2 => yes(MAKE_ROOM),
        _ => -1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests one at a time, as they share the controls.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A test's turn: the controls as they start, then the feature on.
    fn fresh() -> std::sync::MutexGuard<'static, ()> {
        let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        for (at, start) in STARTS.iter().enumerate() {
            procraft_control(at as i32, *start);
        }
        procraft_control(MOVABLE_PANELS as i32, 1.0);
        turn
    }

    fn layout() -> [f32; 4] {
        [0, 1, 2, 3].map(|part| procraft_panel_layout(part))
    }

    #[test]
    fn the_layout_goes_as_its_switches_say() {
        let _turn = fresh();
        // as it starts: snapping within 16, folding, no room made, and nothing said of a part it doesn't know
        assert_eq!(layout(), [16.0, 1.0, 0.0, -1.0]);
        procraft_control(SNAP_DISTANCE as i32, 30.0);
        procraft_control(MAKE_ROOM as i32, 1.0);
        procraft_control(FOLD as i32, 0.0);
        assert_eq!(layout(), [30.0, 0.0, 1.0, -1.0]);
        procraft_control(SNAP as i32, 0.0);
        assert_eq!(layout()[0], 0.0);
        // a distance past what the app takes is taken to its most, none that isn't a number
        procraft_control(SNAP as i32, 1.0);
        procraft_control(SNAP_DISTANCE as i32, 500.0);
        assert_eq!(layout()[0], 64.0);
        // parts it doesn't know
        assert_eq!((procraft_panel_layout(-1), procraft_panel_layout(99)), (-1.0, -1.0));
    }

    #[test]
    fn with_the_features_off_it_leaves_everything_as_the_app_has_it() {
        let _turn = fresh();
        procraft_control(MAKE_ROOM as i32, 1.0);
        assert_eq!((procraft_floating_panel(0), procraft_floating_panel(1), procraft_pin_panels()), (1, 1, 0));
        procraft_control(MOVABLE_PANELS as i32, 0.0);
        assert_eq!(layout(), [-1.0; 4]);
        assert_eq!((procraft_floating_panel(0), procraft_floating_panel(1)), (0, 0));
        // panels stay open by their own feature, whatever Movable panels says
        procraft_control(PIN_PANELS as i32, 1.0);
        assert_eq!(procraft_pin_panels(), 1);
        procraft_control(PIN_PANELS as i32, 0.0);
        assert_eq!(procraft_pin_panels(), 0);
    }

    #[test]
    fn the_bars_move_while_edit_layout_is_on_whatever_movable_panels_says() {
        let _turn = fresh();
        assert_eq!(procraft_panel_layout(3), -1.0);
        procraft_control(EDIT_LAYOUT as i32, 1.0);
        assert_eq!(layout(), [16.0, 1.0, 0.0, 1.0]);
        // (the app runs each feature in a copy of its own, the other's switch told off)
        procraft_control(MOVABLE_PANELS as i32, 0.0);
        assert_eq!(layout(), [-1.0, -1.0, -1.0, 1.0]);
        procraft_control(EDIT_LAYOUT as i32, 0.0);
    }

    #[test]
    fn each_panel_floats_while_its_switch_is_on() {
        let _turn = fresh();
        let said = || (procraft_floating_panel(0), procraft_floating_panel(1));
        assert_eq!(said(), (1, 1));
        procraft_control(FLOAT_LAYERS as i32, 0.0);
        assert_eq!(said(), (0, 1));
        procraft_control(FLOAT_BRUSHES as i32, 0.0);
        assert_eq!(said(), (0, 0));
        // panels it doesn't know
        assert_eq!((procraft_floating_panel(-1), procraft_floating_panel(8)), (0, 0));
    }

    #[test]
    fn the_tool_belt_shows_while_its_feature_is_on_whatever_the_others_say() {
        let _turn = fresh();
        assert_eq!(procraft_panel_layout(4), -1.0);
        procraft_control(TOOL_BELT as i32, 1.0);
        procraft_control(MOVABLE_PANELS as i32, 0.0);
        assert_eq!(procraft_panel_layout(4), 1.0);
        assert_eq!(layout(), [-1.0; 4]);
        procraft_control(TOOL_BELT as i32, 0.0);
        assert_eq!(procraft_panel_layout(4), -1.0);
    }

    #[test]
    fn bars_and_windows_collapse_into_the_sides_while_collapse_to_edge_is_on_whatever_the_others_say() {
        let _turn = fresh();
        // off as it starts: nothing said, as the app has it
        assert_eq!(procraft_panel_layout(5), -1.0);
        // on (in a copy of its own, the others told off there)
        procraft_control(COLLAPSE_EDGE as i32, 1.0);
        procraft_control(MOVABLE_PANELS as i32, 0.0);
        assert_eq!(procraft_panel_layout(5), 1.0);
        assert_eq!(layout(), [-1.0; 4]);
        assert_eq!(procraft_panel_layout(4), -1.0);
        procraft_control(COLLAPSE_EDGE as i32, 0.0);
        assert_eq!(procraft_panel_layout(5), -1.0);
    }

    #[test]
    fn every_other_panel_floats_while_all_panels_float_is_on_and_only_then() {
        let _turn = fresh();
        let others = || (2..=7).map(|n| procraft_floating_panel(n)).collect::<Vec<_>>();
        // off as it starts: only the Layers panel and the library, as before
        assert_eq!(others(), [0; 6]);
        assert_eq!((procraft_floating_panel(0), procraft_floating_panel(1)), (1, 1));
        // on (the app runs it in a copy of its own, Movable panels told off there)
        procraft_control(ALL_PANELS as i32, 1.0);
        procraft_control(MOVABLE_PANELS as i32, 0.0);
        assert_eq!(others(), [1; 6]);
        assert_eq!((procraft_floating_panel(0), procraft_floating_panel(1)), (0, 0));
        // and nothing of the layout from it
        assert_eq!(layout(), [-1.0; 4]);
        procraft_control(ALL_PANELS as i32, 0.0);
        assert_eq!(others(), [0; 6]);
    }
}
