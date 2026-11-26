//! Strokes that fill what they enclose, as a plugin on the stroke-fill hook asks (see
//! [`crate::hooks::Hooks::stroke_fill`]). The app's, whichever plugin asks: the stroke's path,
//! smoothed as its brush smooths it and closed by a straight edge back to where it began, fills by
//! the non-zero winding rule, one coat however often it winds round (a loop drawn twice is no
//! darker, both lobes of a figure eight fill). It is laid as the stroke's paint would be: its colour
//! at the brush's opacity times the opacity slider, edges smoothed, taking the paper and the
//! brush's grain fixed to the canvas, inside the selection, keeping the alpha lock, erasing with
//! the eraser; one undo step, named [`STEP`]. Each copy a symmetry makes fills too.

use std::sync::{Arc, mpsc};

use procraft_engine::geom::{IRect, Vec2};
use procraft_engine::input::Sample;
use procraft_engine::selection::Mask;
use procraft_engine::stroke::StrokeBuilder;

use crate::{PointerKind, State, ToolKind};

/// What undo calls a fill.
pub(crate) const STEP: &str = "Lasso fill";

/// How a fill stroke shows while it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FillShown {
    /// The region it encloses so far, filled, as it grows.
    Growing,
    /// As a plain stroke, filled as it ends.
    AtLift,
}

impl FillShown {
    /// As the hook answers it: 1 growing, 2 at the lift; none for 0 (it paints as usual).
    fn of(said: u8) -> Option<FillShown> {
        match said {
            1 => Some(FillShown::Growing),
            2 => Some(FillShown::AtLift),
            _ => None,
        }
    }
}

/// The open stroke filling what it encloses.
pub(crate) struct Filling {
    pub(crate) shown: FillShown,
    /// Its path, smoothed as the brush smooths it (see [`StrokeBuilder::fill_path`]).
    path: StrokeBuilder,
    /// Where the pen is now (canvas pixels): the path, which catches up with it only as the pen
    /// lifts, is shown going on to it, so that what shows as it grows is what is laid.
    pen: Option<Vec2>,
    /// What the region last shown was made of.
    shown_at: Option<MadeOf>,
    /// The region being made on a thread of its own as it grows (a big one takes tens of
    /// milliseconds, which the frames, on the desktop the window's thread, would wait for), and
    /// what it is made of: the first frame after it is made shows it.
    making: Option<(Option<MadeOf>, mpsc::Receiver<Region>)>,
    /// What is selected as it began (it can't change while the stroke goes on), shared with the
    /// regions being made.
    selection: Option<Arc<Mask>>,
}

/// What a fill's region shown was made of: how many of the path's points, and where the pen was.
type MadeOf = (usize, Option<Vec2>);

/// A region a fill lays: its box and how much of each pixel there (see [`coverage`]); none if it
/// fills none.
type Region = Option<(IRect, Vec<u8>)>;

/// What a fill's region is made from, apart from the canvas's state: the outlines (its path and
/// each copy the symmetry makes of it, canvas pixels), the canvas's size and the selection.
struct RegionOf {
    outlines: Vec<Vec<Vec2>>,
    width: u32,
    height: u32,
    selection: Option<Arc<Mask>>,
}

impl RegionOf {
    /// The region the outlines fill together, inside the selection.
    fn region(&self) -> Region {
        let (rect, mut coverage) = coverage(&self.outlines, self.width, self.height)?;
        if let Some(mask) = &self.selection {
            let selected = crate::layer_ops::mask_part(mask, rect);
            for (c, &m) in coverage.iter_mut().zip(&selected) {
                *c = ((u16::from(*c) * u16::from(m) + 127) / 255) as u8;
            }
            if !coverage.iter().any(|&c| c > 0) {
                return None;
            }
        }
        Some((rect, coverage))
    }
}

/// Where the last of `samples` that is a place is.
fn pen_of(samples: &[Sample]) -> Option<Vec2> {
    samples.iter().rev().map(|s| s.pos).find(|p| p.is_finite())
}

/// The region `outlines` (canvas pixels) fill on a `width` × `height` canvas, each by the non-zero
/// winding rule and all of them together: the box (a pixel to spare for the smoothed edge, on the
/// canvas) and how much of each pixel there (255 all of it), rows from the top. None if they
/// fill none.
pub(crate) fn coverage(outlines: &[Vec<Vec2>], width: u32, height: u32) -> Option<(IRect, Vec<u8>)> {
    let points = || outlines.iter().filter(|o| o.len() >= 3).flatten().filter(|p| p.is_finite());
    let (mut l, mut t, mut r, mut b) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in points() {
        (l, t, r, b) = (l.min(p.x), t.min(p.y), r.max(p.x), b.max(p.y));
    }
    if !(l.is_finite() && t.is_finite() && r.is_finite() && b.is_finite()) {
        return None;
    }
    let side = |v: f32| v.clamp(-1e6, 1e6) as i32;
    let rect =
        IRect::new(side((l - 1.0).floor()), side((t - 1.0).floor()), side((r + 1.0).ceil()), side((b + 1.0).ceil()))
            .intersect(IRect::from_size(width, height));
    if rect.is_empty() {
        return None;
    }
    let mut mask = tiny_skia::Mask::new(rect.width(), rect.height())?;
    let (ox, oy) = (rect.x0 as f32, rect.y0 as f32);
    for outline in outlines.iter().filter(|o| o.len() >= 3) {
        let mut path = tiny_skia::PathBuilder::new();
        let mut each = outline.iter().filter(|p| p.is_finite());
        let Some(first) = each.next() else { continue };
        path.move_to(first.x - ox, first.y - oy);
        for p in each {
            path.line_to(p.x - ox, p.y - oy);
        }
        path.close();
        // each outline by its own winding, laid over the others: a copy mirrored by a symmetry
        // winds the other way, and still fills
        if let Some(path) = path.finish() {
            mask.fill_path(&path, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
        }
    }
    let data = mask.data().to_vec();
    data.iter().any(|&c| c > 0).then_some((rect, data))
}

impl State {
    /// Whether a stroke beginning now with `tool` by `pointer` on `layer` fills what it encloses,
    /// as the plugins on the stroke-fill hook say, and how it shows. Never a smudge, nor on the
    /// brush studio's pad, an adjustment painted on, a model or a plugin's own layer.
    pub(crate) fn fill_asked(
        &self,
        tool: ToolKind,
        pointer: PointerKind,
        layer: procraft_engine::document::LayerId,
    ) -> Option<FillShown> {
        let hooks = self.hooks.as_ref()?;
        let number = match tool {
            ToolKind::Paint => 0,
            ToolKind::Erase => 1,
            ToolKind::Smudge => return None,
        };
        if self.pad_strokes.is_some() || self.adjusting || self.plugin_layer_kind(layer).is_some() {
            return None;
        }
        let pointer = crate::pressure::pointer_number(pointer);
        FillShown::of(hooks.stroke_fill(number, pointer, |keys| self.fill_brush(tool, keys)))
    }

    /// The brush `tool` draws with, as the stroke-fill hook is told it: its name, its set's and the
    /// values of the rows `keys`.
    fn fill_brush(&self, tool: ToolKind, keys: &[String]) -> crate::stroke_sound::SoundBrush {
        let (set, index) = self.brush_ref(tool);
        let set = self.library.get(set);
        let name = set.and_then(|s| s.brushes.get(index)).map_or("", |b| b.name.as_str()).to_owned();
        let set = set.map_or("", |s| s.name.as_str()).to_owned();
        let brush = self.brush(tool);
        let controls: Vec<f32> = keys.iter().map(|k| brush.setting(k)).collect();
        let mut hash = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&(&name, &set, controls.iter().map(|v| v.to_bits()).collect::<Vec<_>>()), &mut hash);
        crate::stroke_sound::SoundBrush { name, set, controls, key: std::hash::Hasher::finish(&hash) }
    }

    /// Starts filling with the stroke `builder` begins, shown as `shown` says, from its first
    /// `samples` (canvas pixels).
    pub(crate) fn begin_fill(&mut self, shown: FillShown, builder: &StrokeBuilder, samples: &[Sample]) {
        let mut path = builder.fill_path();
        path.add(samples);
        let selection = self.selection.as_ref().filter(|m| !m.is_empty()).map(|m| Arc::new(m.clone()));
        let pen = pen_of(samples);
        self.filling = Some(Box::new(Filling { shown, path, pen, shown_at: None, making: None, selection }));
    }

    /// More of the fill stroke's samples (canvas pixels): its path goes on. Whether its own stamps
    /// are left out (it shows as its region grows; the next frame fills that).
    pub(crate) fn fill_add(&mut self, samples: &[Sample]) -> bool {
        let Some(filling) = self.filling.as_mut() else { return false };
        filling.path.add(samples);
        filling.pen = pen_of(samples).or(filling.pen);
        filling.shown == FillShown::Growing
    }

    /// The region the fill stroke encloses so far shown, if its path went on since it was last
    /// shown (once a frame, however many samples came): made on a thread of its own, one at a time,
    /// and shown by the first frame after it is made (frames go on being asked for meanwhile); the
    /// path as it went on meanwhile is made next.
    pub(crate) fn show_fill(&mut self) {
        let Some(filling) = self.filling.as_mut().filter(|f| f.shown == FillShown::Growing) else { return };
        if let Some((made_of, made)) = filling.making.take() {
            match made.try_recv() {
                Ok(region) => {
                    filling.shown_at = made_of;
                    self.lay_fill_region(region);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    filling.making = Some((made_of, made));
                    self.needs_frame = true;
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {}
            }
        }
        let Some(filling) = self.filling.as_ref() else { return };
        let mut path = filling.path.path();
        let now = Some((path.len(), filling.pen));
        if filling.shown_at == now {
            return;
        }
        // on to where the pen is, as the path will be once it lifts
        if let Some(pen) = filling.pen.filter(|p| path.last() != Some(p)) {
            path.push(pen);
        }
        let of = self.region_of(&path, filling.selection.clone());
        let (sent, made) = mpsc::channel();
        let making = std::thread::Builder::new().name("procraft-fill-region".into()).spawn(move || {
            // (the stroke may have ended meanwhile: then nothing waits for it)
            let _ = sent.send(of.region());
        });
        let Some(filling) = self.filling.as_mut() else { return };
        match making {
            Ok(_) => {
                filling.making = Some((now, made));
                self.needs_frame = true;
            }
            // no thread to make it on: made here
            Err(e) => {
                log::warn!("a fill's region made on the frames' thread: {e}");
                filling.shown_at = now;
                let region = self.fill_region(&path);
                self.lay_fill_region(region);
            }
        }
    }

    /// `region` shown as the open fill stroke's (none: nothing yet).
    fn lay_fill_region(&mut self, region: Region) {
        if let Some(canvas) = self.canvas.as_mut() {
            let (rect, coverage) = region.unwrap_or((IRect::EMPTY, Vec::new()));
            canvas.fill_stroke(rect, coverage);
        }
    }

    /// Whether the open fill stroke's region is still being made (see [`Self::show_fill`]).
    #[cfg(test)]
    pub(crate) fn fill_being_made(&self) -> bool {
        self.filling.as_ref().is_some_and(|f| f.making.is_some())
    }

    /// Ends the fill stroke with its last `samples` (canvas pixels): its region laid as one step,
    /// or nothing if it encloses nothing. False if no fill stroke is open.
    pub(crate) fn end_fill(&mut self, samples: &[Sample]) -> bool {
        let Some(mut filling) = self.filling.take() else { return false };
        filling.path.add(samples);
        filling.path.finish();
        self.stroke = None;
        self.second = None;
        // (a region still being made as it grew is let go)
        let region = self.region_of(&filling.path.path(), filling.selection.clone()).region();
        let Some(canvas) = self.canvas.as_mut() else { return true };
        match region {
            Some((rect, coverage)) => {
                canvas.fill_stroke(rect, coverage);
                canvas.name_next_step(STEP);
                canvas.end_stroke();
            }
            None => canvas.cancel_stroke(),
        }
        true
    }

    /// The region `path` (canvas pixels) and each copy the symmetry makes of it enclose, inside
    /// the selection (see [`coverage`]).
    fn fill_region(&self, path: &[Vec2]) -> Region {
        let selection = self.selection.as_ref().filter(|m| !m.is_empty()).map(|m| Arc::new(m.clone()));
        self.region_of(path, selection).region()
    }

    /// What the region `path` (canvas pixels) and each copy the symmetry makes of it enclose,
    /// inside `selection`, is made from.
    fn region_of(&self, path: &[Vec2], selection: Option<Arc<Mask>>) -> RegionOf {
        let (width, height) = (self.doc.width(), self.doc.height());
        if path.len() < 3 {
            return RegionOf { outlines: Vec::new(), width, height, selection };
        }
        let guide = self.doc.stroke_guide();
        let copied: Vec<Vec<(Vec2, f32, bool)>> = path.iter().map(|p| guide.copies(*p)).collect();
        let count = copied.iter().map(Vec::len).min().unwrap_or(0);
        let mut outlines = vec![path.to_vec()];
        for k in 0..count {
            outlines.push(copied.iter().filter_map(|c| c.get(k).map(|(p, _, _)| *p)).collect());
        }
        RegionOf { outlines, width, height, selection }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A circle's outline about (`x`, `y`), `r` across, `n` points, once round (`turns` 1) or more.
    pub(crate) fn circle(x: f32, y: f32, r: f32, n: usize, turns: usize) -> Vec<Vec2> {
        (0..n * turns)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                Vec2::new(x + r * a.cos(), y + r * a.sin())
            })
            .collect()
    }

    #[test]
    fn a_loop_drawn_round_twice_fills_once_as_a_loop_drawn_once() {
        let once = coverage(&[circle(50.0, 50.0, 30.0, 90, 1)], 100, 100).unwrap();
        let twice = coverage(&[circle(50.0, 50.0, 30.0, 90, 2)], 100, 100).unwrap();
        assert_eq!(once, twice);
        // solid inside, nothing well outside, smoothed at the rim
        let (rect, data) = &once;
        let at = |x: i32, y: i32| data[((y - rect.y0) * rect.width() as i32 + (x - rect.x0)) as usize];
        assert_eq!(at(50, 50), 255);
        assert_eq!(at(50, 22), 255);
        assert_eq!(at(21, 21), 0);
        let smoothed = data.iter().filter(|&&c| (1..255).contains(&c)).count();
        assert!(smoothed > 60, "a smoothed rim: {smoothed} pixels");
    }

    #[test]
    fn both_lobes_of_a_figure_eight_fill() {
        // round the left lobe one way, the right lobe the other
        let mut eight = Vec::new();
        for i in 0..120 {
            let a = i as f32 / 120.0 * std::f32::consts::TAU;
            eight.push(Vec2::new(50.0 + 40.0 * a.sin(), 50.0 + 20.0 * (2.0 * a).sin()));
        }
        let (rect, data) = coverage(&[eight], 100, 100).unwrap();
        let at = |x: i32, y: i32| data[((y - rect.y0) * rect.width() as i32 + (x - rect.x0)) as usize];
        assert_eq!(at(30, 50 - 8), 255, "one lobe");
        assert_eq!(at(70, 50 + 8), 255, "the other");
        // even-odd would fill them too; a loop inside a loop the same way round tells them apart
        let mut nested = circle(50.0, 50.0, 40.0, 120, 1);
        nested.extend(circle(50.0, 50.0, 15.0, 60, 1));
        let (rect, data) = coverage(&[nested], 100, 100).unwrap();
        let at = |x: i32, y: i32| data[((y - rect.y0) * rect.width() as i32 + (x - rect.x0)) as usize];
        assert_eq!(at(50, 50), 255, "wound twice there: filled, not a hole");
    }

    #[test]
    fn the_path_is_closed_by_a_straight_edge_back_to_its_start() {
        // three sides of a square: the fourth closes it
        let u = vec![Vec2::new(10.0, 10.0), Vec2::new(10.0, 90.0), Vec2::new(90.0, 90.0), Vec2::new(90.0, 10.0)];
        let (rect, data) = coverage(&[u], 100, 100).unwrap();
        assert_eq!(rect, IRect::new(9, 9, 91, 91));
        let at = |x: i32, y: i32| data[((y - rect.y0) * rect.width() as i32 + (x - rect.x0)) as usize];
        assert_eq!((at(50, 11), at(50, 50), at(50, 89)), (255, 255, 255));
        assert_eq!(at(50, 9), 0, "nothing past the closing edge");
    }

    #[test]
    fn mirrored_copies_fill_where_they_overlap_and_nothing_fills_off_the_canvas() {
        let right = circle(60.0, 50.0, 20.0, 60, 1);
        let mirrored: Vec<Vec2> = right.iter().map(|p| Vec2::new(100.0 - p.x, p.y)).collect();
        let (rect, data) = coverage(&[right, mirrored], 100, 100).unwrap();
        let at = |x: i32, y: i32| data[((y - rect.y0) * rect.width() as i32 + (x - rect.x0)) as usize];
        assert_eq!(at(50, 50), 255, "where both cover it");
        assert_eq!(coverage(&[circle(-100.0, -100.0, 20.0, 60, 1)], 100, 100), None);
        assert_eq!(coverage(&[vec![Vec2::new(1.0, 1.0), Vec2::new(5.0, 5.0)]], 100, 100), None);
        assert_eq!(coverage(&[vec![Vec2::new(f32::NAN, 1.0); 5]], 100, 100), None);
    }

    use std::sync::Arc;

    use crate::CanvasSession;
    use crate::hooks::Hooks;
    use crate::plugin_run::{Hook, Running};
    use crate::tests::{frame_of, no_streamline, pixel, sample, session};

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const RED: [u8; 4] = [255, 0, 0, 255];

    /// A plugin on the stroke-fill hook answering `answer` (a WAT body, given `tool` and
    /// `pointer` as locals 0 and 1) put on `s`.
    fn filling(s: &CanvasSession, answer: &str) -> Arc<Hooks> {
        let hooks = Arc::new(Hooks::default());
        let text = format!(r#"(module (func (export "procraft_stroke_fill") (param i32 i32) (result i32) {answer}))"#);
        let running = Running::start(&wat::parse_str(text).unwrap(), &[]).unwrap();
        hooks.put_on("org.example.fill".to_owned(), running, &[Hook::StrokeFill]);
        s.shared.lock().hooks = Some(Arc::clone(&hooks));
        hooks
    }

    /// A pen stroke through `points` (window pixels, which are canvas pixels here), a sample each.
    fn draw(s: &CanvasSession, points: &[Vec2]) {
        let at = |i: usize, p: &Vec2| sample(p.x, p.y, i as f64 * 8.0);
        let Some(first) = points.first() else { return };
        s.stroke_begin(PointerKind::Stylus, vec![at(0, first)]);
        for (i, p) in points.iter().enumerate().skip(1) {
            s.stroke_add(vec![at(i, p)]);
        }
        s.stroke_end(vec![]);
    }

    /// A red session 100 pixels square, its brush drawing just where the pen goes.
    fn red() -> Option<Arc<CanvasSession>> {
        let s = session(100, 100)?;
        no_streamline(&s);
        s.set_color(1.0, 0.0, 0.0);
        Some(s)
    }

    /// Every pixel of `s`'s composite, as `pixel` reads it, a few apart.
    fn picture(s: &CanvasSession) -> Vec<[u8; 4]> {
        (0..100)
            .step_by(3)
            .flat_map(|y| (0..100).step_by(3).map(move |x| (x, y)))
            .map(|(x, y)| pixel(s, x, y))
            .collect()
    }

    fn square_u() -> Vec<Vec2> {
        // three sides of a square, drawn: the fourth closes it
        let mut u = Vec::new();
        for i in 0..=30 {
            u.push(Vec2::new(20.0, 20.0 + 2.0 * i as f32));
        }
        for i in 1..=30 {
            u.push(Vec2::new(20.0 + 2.0 * i as f32, 80.0));
        }
        for i in 1..=30 {
            u.push(Vec2::new(80.0, 80.0 - 2.0 * i as f32));
        }
        u
    }

    #[test]
    fn a_stroke_a_plugin_says_fills_lays_what_it_encloses_as_one_step_at_the_lift() {
        let Some(s) = red() else { return };
        filling(&s, "(i32.const 2)");
        draw(&s, &square_u());
        // inside, filled; the closing edge's side too; outside, nothing
        assert_eq!(pixel(&s, 50, 50), RED);
        assert_eq!(pixel(&s, 50, 22), RED);
        assert_eq!(pixel(&s, 50, 10), WHITE);
        assert_eq!(pixel(&s, 90, 50), WHITE);
        // one step, named so
        assert_eq!(s.undo().as_deref(), Some(STEP));
        assert_eq!(pixel(&s, 50, 50), WHITE);
        assert!(!s.shared.lock().canvas.as_ref().unwrap().can_undo(), "nothing more to undo");
    }

    #[test]
    fn a_loop_drawn_twice_lays_what_a_loop_drawn_once_does_and_a_figure_eight_both_lobes() {
        let (Some(once), Some(twice), Some(eight)) = (red(), red(), red()) else { return };
        for s in [&once, &twice, &eight] {
            filling(s, "(i32.const 2)");
        }
        // half as opaque, so that a second coat would show
        for s in [&once, &twice] {
            s.set_brush_opacity(0.5);
        }
        draw(&once, &circle(50.0, 50.0, 30.0, 90, 1));
        draw(&twice, &circle(50.0, 50.0, 30.0, 90, 2));
        let half = pixel(&once, 50, 50);
        assert!(half[0] == 255 && (126..=129).contains(&half[1]), "{half:?}");
        // inside, one coat, no hole
        for (x, y) in [(50, 50), (30, 50), (70, 50), (50, 30), (50, 70), (36, 36)] {
            assert_eq!(pixel(&twice, x, y), pixel(&once, x, y), "no darker, no hole at {x}, {y}");
        }
        // the rim as near as the path's points lie round it the second time
        let most = picture(&once)
            .iter()
            .zip(picture(&twice))
            .map(|(a, b)| a.iter().zip(b).map(|(x, y)| x.abs_diff(y)).max().unwrap_or(0))
            .max()
            .unwrap_or(0);
        println!("a loop drawn twice: its rim within {most} levels of one drawn once");
        assert!(most <= 64, "{most}");
        let lobes: Vec<Vec2> = (0..=120)
            .map(|i| i as f32 / 120.0 * std::f32::consts::TAU)
            .map(|a| Vec2::new(50.0 + 40.0 * a.sin(), 50.0 + 20.0 * (2.0 * a).sin()))
            .collect();
        draw(&eight, &lobes);
        assert_eq!((pixel(&eight, 30, 42), pixel(&eight, 70, 58)), (RED, RED));
    }

    /// The pixels [`picture`] reads.
    type Picture = Vec<[u8; 4]>;

    /// What two fills of the same square lay, one undone before the other, the brush's grain a
    /// grunge fixed to the canvas, started somewhere new each stroke (`jitter`) or not.
    fn two_grained_fills(jitter: bool) -> Option<(Picture, Picture)> {
        use procraft_engine::brush::{Grain, GrainMode};
        let s = red()?;
        filling(&s, "(i32.const 2)");
        {
            let mut st = s.shared.lock();
            let tool = st.tool;
            let brush = st.brush_mut(tool)?;
            let grunge = Some(procraft_engine::textures::grain::GRUNGE);
            brush.grain = Grain { image: grunge, source: None, mode: GrainMode::Texturized, scale: 64.0, depth: 1.0 };
            brush.set_setting("grain_offset_jitter", if jitter { 1.0 } else { 0.0 });
        }
        draw(&s, &square_u());
        let first = picture(&s);
        s.undo();
        draw(&s, &square_u());
        Some((first, picture(&s)))
    }

    #[test]
    fn a_fill_whose_brush_starts_its_grain_somewhere_new_each_stroke_does() {
        let Some((first, again)) = two_grained_fills(true) else { return };
        let Some((plain, plain_again)) = two_grained_fills(false) else { return };
        // with Offset jitter each fill's grain starts somewhere new; without, where the last did
        assert!(first != again, "the grain moved between fills");
        assert!(plain == plain_again);
        assert!(first != plain || again != plain);
        // the places a stroke's seed gives spread evenly over the tile
        let starts: Vec<[f32; 2]> = (0..1000u32).map(|i| crate::grain_start(i.wrapping_mul(0x9e37_79b9))).collect();
        assert!(starts.iter().flatten().all(|t| (0.0..1.0).contains(t)));
        let mean = |k: usize| starts.iter().map(|s| s[k]).sum::<f32>() / starts.len() as f32;
        assert!((mean(0) - 0.5).abs() < 0.05 && (mean(1) - 0.5).abs() < 0.05, "{} {}", mean(0), mean(1));
    }

    #[test]
    fn a_fill_is_as_opaque_as_the_brush_and_slider_and_keeps_inside_the_selection() {
        let Some(s) = red() else { return };
        filling(&s, "(i32.const 2)");
        s.set_brush_opacity(0.5);
        draw(&s, &square_u());
        let [r, g, b, a] = pixel(&s, 50, 50);
        assert_eq!((r, a), (255, 255));
        assert!((126..=129).contains(&g) && g == b, "half red over white: {g}");
        let Some(s) = red() else { return };
        filling(&s, "(i32.const 2)");
        s.select_shape(false, 10.0, 10.0, 40.0, 40.0, crate::selection::SelectionJoin::Add);
        draw(&s, &square_u());
        assert_eq!(pixel(&s, 30, 30), RED);
        assert_eq!(pixel(&s, 60, 60), WHITE, "outside the selection");
    }

    #[test]
    fn a_fill_keeps_a_locked_layer_s_clear_pixels_clear_and_the_eraser_takes_away_what_it_encloses() {
        let Some(s) = red() else { return };
        // a red bar, then the layer locked, then blue filled over more than it
        draw(&s, &[Vec2::new(10.0, 50.0), Vec2::new(90.0, 50.0)]);
        let layer = s.shared.lock().doc.current().0;
        assert!(s.set_layer_alpha_lock(layer, true));
        s.set_color(0.0, 0.0, 1.0);
        filling(&s, "(i32.const 2)");
        draw(&s, &square_u());
        assert_eq!(pixel(&s, 50, 50), [0, 0, 255, 255], "the paint there recoloured");
        assert_eq!(pixel(&s, 50, 30), WHITE, "a clear pixel stays clear");
        // the eraser (tool 1) fills only when the plugin says, here always
        assert!(s.set_layer_alpha_lock(layer, false));
        filling(&s, "(i32.const 1)");
        s.set_tool(ToolKind::Erase);
        draw(&s, &square_u());
        assert_eq!(pixel(&s, 50, 50), WHITE, "taken away");
        assert_eq!(pixel(&s, 15, 50), RED, "outside it the bar stays as it was");
    }

    #[test]
    fn a_plugin_that_says_no_leaves_strokes_as_they_were_and_a_smudge_is_never_asked() {
        let (Some(plain), Some(asked)) = (red(), red()) else { return };
        // says fill only for tool 2, which is never asked: a smudge
        filling(&asked, "(select (i32.const 1) (i32.const 0) (i32.eq (local.get 0) (i32.const 2)))");
        for s in [&plain, &asked] {
            draw(s, &square_u());
        }
        assert_eq!(pixel(&plain, 50, 50), WHITE);
        assert!(picture(&plain) == picture(&asked));
    }

    #[test]
    fn a_fill_shown_as_it_grows_shows_what_it_lays() {
        let (Some(shown), Some(laid)) = (red(), red()) else { return };
        filling(&shown, "(i32.const 1)");
        filling(&laid, "(i32.const 2)");
        // drawn but not lifted: the frame shows the region so far, closed back to the start
        let u = square_u();
        shown.stroke_begin(PointerKind::Stylus, vec![sample(u[0].x, u[0].y, 0.0)]);
        for (i, p) in u.iter().enumerate().skip(1) {
            shown.stroke_add(vec![sample(p.x, p.y, i as f64 * 8.0)]);
        }
        let mut growing = frame_of(&shown, 100);
        let made_by = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while shown.shared.lock().fill_being_made() && std::time::Instant::now() < made_by {
            growing = frame_of(&shown, 100);
        }
        shown.stroke_end(vec![]);
        let ended = frame_of(&shown, 100);
        draw(&laid, &u);
        // as it grew, as it was laid: shown on to where the pen was, as the lift's catching up lays
        // it, but for the path's own points along that last stretch (a few rim pixels)
        let (grew, laid_px) = (growing.as_chunks::<4>().0, ended.as_chunks::<4>().0);
        let differ = grew.iter().zip(laid_px).filter(|(a, b)| a != b).count();
        let most = grew
            .iter()
            .zip(laid_px)
            .map(|(a, b)| a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0))
            .max()
            .unwrap_or(0);
        println!("a fill as it grew and as it was laid: {differ} of 10,000 pixels differ, by {most} levels at most");
        assert!(differ <= 60, "{differ}");
        assert!(ended == frame_of(&laid, 100), "as a fill shown plainly lays it");
        let at = |x: usize, y: usize| ended[(y * 100 + x) * 4..][..4].to_vec();
        assert_eq!(at(50, 50), RED.to_vec());
    }

    /// A big fill shown as it grows (a loop 1800 pixels across on a canvas 2048 square): its region,
    /// which takes tens of milliseconds to make (more in a debug build), is made on a thread of its
    /// own, so the frames, the desktop's on its window's thread, never wait for it: the frame's part
    /// in it holds the calling thread a few milliseconds at most. Once made it shows, as it lays.
    /// Prints both times.
    #[test]
    fn a_big_fill_s_region_is_made_beside_the_frames_as_it_grows() {
        use std::time::{Duration, Instant};
        let Some(s) = session(2048, 2048) else { return };
        no_streamline(&s);
        s.set_color(1.0, 0.0, 0.0);
        filling(&s, "(i32.const 1)");
        let rim = circle(1024.0, 1024.0, 900.0, 240, 1);
        s.stroke_begin(PointerKind::Stylus, vec![sample(rim[0].x, rim[0].y, 0.0)]);
        // a frame's part (the region shown) every four samples, as frames come while it is drawn
        let (mut longest, mut frames) = (Duration::ZERO, 0);
        for (i, p) in rim.iter().enumerate().skip(1) {
            s.stroke_add(vec![sample(p.x, p.y, i as f64 * 8.0)]);
            if i % 4 == 0 {
                let t = Instant::now();
                s.shared.lock().show_fill();
                longest = longest.max(t.elapsed());
                frames += 1;
            }
        }
        // what making the region of the whole loop takes, here
        let making = {
            let st = s.shared.lock();
            let t = Instant::now();
            assert!(st.fill_region(&rim).is_some());
            t.elapsed()
        };
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        println!(
            "a fill 1800 px across growing: the frames' part {:.2} ms at most over {frames}; making its region {:.1} ms",
            ms(longest),
            ms(making)
        );
        // made, it shows: the middle red before the lift, and after it
        let middle = |frame: &[u8]| frame[(1024 * 2048 + 1024) * 4..][..4].to_vec();
        let (made_by, mut drawn, mut frame) = (Instant::now() + Duration::from_secs(20), 0, Vec::new());
        while (drawn < 2 || s.shared.lock().fill_being_made()) && Instant::now() < made_by {
            frame = frame_of(&s, 2048);
            drawn += 1;
        }
        assert_eq!(middle(&frame), RED.to_vec(), "shown as it grows");
        s.stroke_end(vec![]);
        assert_eq!(middle(&frame_of(&s, 2048)), RED.to_vec(), "laid");
        assert!(longest < Duration::from_millis(8), "a frame waited {:.1} ms for the region", ms(longest));
        assert!(longest * 4 < making, "the frames' part {:.2} ms against {:.1} ms", ms(longest), ms(making));
    }

    /// What filling a region costs, by its size: run with `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn fill_cost() {
        for side in [256.0_f32, 1024.0, 2048.0, 4096.0] {
            let loop_ = circle(side / 2.0, side / 2.0, side * 0.45, (side * 3.0) as usize, 1);
            let start = std::time::Instant::now();
            let runs = 5;
            for _ in 0..runs {
                std::hint::black_box(coverage(
                    std::hint::black_box(std::slice::from_ref(&loop_)),
                    side as u32,
                    side as u32,
                ));
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
            println!("a fill {side} px across, {} points: {ms:.2} ms", loop_.len());
        }
    }
}
