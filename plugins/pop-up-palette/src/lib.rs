//! Pop-up palette: a ring of the colours and brushes painted with last that opens where the pen is,
//! as a pen's button set to it in Prefs, Pen buttons is pressed (or in the window's middle, from
//! its card). The colour in use and the history after it make the inner ring, the Recent set's
//! brushes (the pinned first) the outer, each shown by its sample stroke; the ones in use are
//! ringed. A colour or a brush tapped is taken, and the ring closes unless its switch says
//! otherwise; a tap off it closes it. It's a plugin, not part of the app: `tools/pack_plugin.py`
//! makes a `.procraft-plugin` file of it.

use std::sync::Mutex;

use kit::app::{self, Brush, Panel};

/// The controls by their place, as the manifest starts them: how many colours and brushes the ring
/// shows, and whether it closes once one is picked.
static CONTROLS: kit::Controls<3> = kit::Controls::new([8.0, 8.0, 1.0]);
kit::controls!(CONTROLS);
const COLOURS: usize = 0;
const BRUSHES: usize = 1;
const CLOSE: usize = 2;

/// The most a ring shows of each.
const MOST_COLOURS: usize = 10;
const MOST_BRUSHES: usize = 12;

/// What the ring shows: its colours (0xRRGGBB) and its brushes, either empty when it has no row of
/// them.
#[derive(Clone, Debug, Default, PartialEq)]
struct Ring {
    colours: Vec<u32>,
    brushes: Vec<Brush>,
}

/// The ring shown last, which a tap on it picks from.
static SHOWN: Mutex<Ring> = Mutex::new(Ring { colours: Vec::new(), brushes: Vec::new() });

/// How many its slider at `control` asks for, up to `most`.
fn wanted(control: usize, most: usize) -> usize {
    let n = CONTROLS.get(control);
    if n.is_finite() && n > 0.0 { (n.round() as usize).min(most) } else { 0 }
}

/// The ring: the colour `current` first, then the `recent` ones (newest first) as far as `colours`
/// go, each once; the `recent_brushes` as far as `brushes` go, the brush `in_use` alone when there
/// are none. With neither asked for, the colour in use alone, so there is a ring to close.
fn ring(
    current: u32,
    recent: impl Iterator<Item = u32>,
    colours: usize,
    recent_brushes: impl Iterator<Item = Brush>,
    brushes: usize,
    in_use: Option<Brush>,
) -> Ring {
    let mut shown = Ring::default();
    if colours > 0 {
        shown.colours.push(current & 0xFF_FFFF);
        for rgb in recent {
            if shown.colours.len() >= colours {
                break;
            }
            if !shown.colours.contains(&rgb) {
                shown.colours.push(rgb);
            }
        }
    }
    if brushes > 0 {
        shown.brushes = recent_brushes.take(brushes).collect();
        if shown.brushes.is_empty() {
            shown.brushes.extend(in_use);
        }
    }
    if shown.colours.is_empty() && shown.brushes.is_empty() {
        shown.colours.push(current & 0xFF_FFFF);
    }
    shown
}

/// The panel showing `ring`, the colour `current` and the brush `in_use` ringed where they are.
fn panel(ring: &Ring, current: u32, in_use: Option<Brush>) -> Panel {
    let mut panel = Panel::new("Pop-up palette").ring();
    if !ring.colours.is_empty() {
        let chosen = ring.colours.iter().position(|&c| c == current & 0xFF_FFFF);
        panel = panel.colours("Recent colours", &ring.colours, chosen);
    }
    if !ring.brushes.is_empty() {
        let chosen = in_use.and_then(|b| ring.brushes.iter().position(|&r| r == b));
        panel = panel.brushes("Recent brushes", &ring.brushes, chosen);
    }
    panel
}

kit::actions!(|_| {
    let (current, in_use) = (app::brush_colour(), app::current_brush());
    let shown = ring(
        current,
        (0..MOST_COLOURS).map_while(app::recent_colour),
        wanted(COLOURS, MOST_COLOURS),
        (0..MOST_BRUSHES).map_while(app::recent_brush),
        wanted(BRUSHES, MOST_BRUSHES),
        in_use,
    );
    panel(&shown, current, in_use).show();
    *SHOWN.lock().unwrap_or_else(|e| e.into_inner()) = shown;
});

kit::panel!(|row, value| {
    let shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if value < 0.0 {
        return;
    }
    let at = value as usize;
    // the rows as shown: the colours' first, if there are any
    let brushes_row = usize::from(!shown.colours.is_empty());
    if row == brushes_row && !shown.brushes.is_empty() {
        let Some(&brush) = shown.brushes.get(at) else { return };
        app::choose_brush(brush);
    } else if row == 0 && !shown.colours.is_empty() {
        let Some(&rgb) = shown.colours.get(at) else { return };
        app::set_brush_colour(rgb);
    } else {
        return;
    }
    if CONTROLS.on(CLOSE) {
        app::close_panel();
    } else {
        panel(&shown, app::brush_colour(), app::current_brush()).show();
    }
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_starts_with_the_colour_in_use_then_the_history_each_once() {
        let brushes = [Brush(1 << 32), Brush((1 << 32) | 1), Brush(2 << 32)];
        let recent = [0x445566, 0x112233, 0x778899, 0x445566];
        let shown = ring(0x112233, recent.into_iter(), 3, brushes.into_iter(), 2, None);
        assert_eq!(shown.colours, [0x112233, 0x445566, 0x778899]);
        assert_eq!(shown.brushes, brushes[..2]);
        // as many as asked, and as there are
        let few = ring(0x000001, [0x000002].into_iter(), 8, core::iter::empty(), 8, Some(Brush(9)));
        assert_eq!(few.colours, [1, 2]);
        assert_eq!(few.brushes, [Brush(9)], "the brush in use when nothing is recent");
        // none of one: no row of it; none of either: the colour in use alone
        let no_colours = ring(5, [6].into_iter(), 0, brushes.into_iter(), 1, None);
        assert_eq!((no_colours.colours.len(), no_colours.brushes.len()), (0, 1));
        let nothing = ring(5, [6].into_iter(), 0, brushes.into_iter(), 0, None);
        assert_eq!(nothing, Ring { colours: vec![5], brushes: Vec::new() });
    }

    #[test]
    fn its_panel_is_a_ring_of_colours_then_brushes_the_ones_in_use_chosen() {
        let shown = Ring { colours: vec![0xFF0000, 0x00FF00], brushes: vec![Brush(1 << 32), Brush(7)] };
        let json = panel(&shown, 0x00FF00, Some(Brush(7))).json();
        assert_eq!(
            json,
            r#"{"title":"Pop-up palette","rows":[{"type":"colours","label":"Recent colours","colours":[16711680,65280],"chosen":1},{"type":"brushes","label":"Recent brushes","brushes":[4294967296,7],"chosen":1}],"ring":true}"#
        );
        // brushes alone, none in use there
        let brushes = Ring { colours: Vec::new(), brushes: vec![Brush(7)] };
        let json = panel(&brushes, 0, None).json();
        assert!(json.contains(r#""rows":[{"type":"brushes""#) && !json.contains("chosen"), "{json}");
    }

    #[test]
    fn its_sliders_ask_for_as_many_as_they_say_up_to_the_most() {
        assert_eq!((wanted(COLOURS, MOST_COLOURS), wanted(BRUSHES, MOST_BRUSHES)), (8, 8));
        procraft_control(COLOURS as i32, 40.0);
        procraft_control(BRUSHES as i32, -3.0);
        assert_eq!((wanted(COLOURS, MOST_COLOURS), wanted(BRUSHES, MOST_BRUSHES)), (MOST_COLOURS, 0));
        procraft_control(COLOURS as i32, 8.0);
        procraft_control(BRUSHES as i32, 8.0);
    }
}
