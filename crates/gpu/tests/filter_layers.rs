//! Filter layers (interface 17) on a real GPU (skipped where the machine has none): a layer with
//! no paint of its own showing what is below it through a plugin's filter.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::HashMap;
use std::time::Instant;

use procraft_engine::document::{BlendMode, Document, LayerFilter, LayerId};
use procraft_gpu::{
    Canvas, FilterReach, FilterTable, Gpu, LayerFilterProgram, read_layer_filter, read_layer_table_filter,
};

const GREY: &str = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let y = dot(colour.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec4<f32>(vec3<f32>(y), colour.a);
}";

/// The mean of the square `radius` about each pixel, alpha weighted.
const BOX: &str = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let r = i32(v.radius);
    var sum = vec4<f32>(0.0);
    var n = 0.0;
    for (var y = -r; y <= r; y++) {
        for (var x = -r; x <= r; x++) {
            let c = layer_at(at + vec2<f32>(f32(x), f32(y)));
            sum += vec4<f32>(c.rgb * c.a, c.a);
            n += 1.0;
        }
    }
    let m = sum / n;
    return vec4<f32>(select(vec3<f32>(0.0), m.rgb / max(m.a, 1e-6), m.a > 0.0), m.a);
}";

/// Each pixel the colour mirrored across the canvas: it reads anywhere.
const MIRROR: &str = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    return layer_at(vec2<f32>(canvas_size().x - at.x, at.y));
}";

fn gpu() -> Option<Gpu> {
    match Gpu::headless() {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("skipped: {e}");
            None
        }
    }
}

fn made(gpu: &Gpu, text: &str, values: &[(&str, f32)], reach: FilterReach) -> LayerFilterProgram {
    let keys: Vec<String> = values.iter().map(|(k, _)| (*k).to_owned()).collect();
    let program = gpu.make_filter(read_layer_filter(text, &keys).unwrap()).unwrap();
    LayerFilterProgram {
        program,
        values: values.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
        reach,
        table: None,
    }
}

/// Red through row 1 of its curves, green through row 0 of a gradient's colours.
const CURVES: &str = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(table(1, colour.r).r, colour.g, colour.b, colour.a);
}";
const GRADIENT: &str = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(table(0, colour.r).rgb, colour.a);
}";

fn filters(gpu: &Gpu) -> HashMap<String, LayerFilterProgram> {
    let mut all = HashMap::new();
    all.insert("t.grey".to_owned(), made(gpu, GREY, &[], FilterReach::Within(0.0)));
    let by_radius = FilterReach::ByValue { value: 0, times: 1.0, plus: 0.0 };
    all.insert("t.box".to_owned(), made(gpu, BOX, &[("radius", 2.0)], by_radius));
    all.insert("t.mirror".to_owned(), made(gpu, MIRROR, &[], FilterReach::Whole));
    let tabled = |text: &str, table: FilterTable, key: &str| {
        let program = gpu.make_filter(read_layer_table_filter(text, &[]).unwrap()).unwrap();
        LayerFilterProgram {
            program,
            values: Vec::new(),
            reach: FilterReach::Within(0.0),
            table: Some((key.to_owned(), table)),
        }
    };
    all.insert("t.curves".to_owned(), tabled(CURVES, FilterTable::Curves { channels: 5 }, "curves"));
    all.insert("t.gradient".to_owned(), tabled(GRADIENT, FilterTable::Gradient, "gradient"));
    all
}

fn at(px: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

fn near(a: [u8; 4], b: [u8; 4], by: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= by)
}

/// A canvas `size` square whose bottom layer is `colour` all over, with a filter layer running
/// `filter` above it: the canvas, the artwork, the bottom layer and the filter layer.
fn over(gpu: &Gpu, size: u32, colour: [u8; 3], filter: &str) -> (Canvas, Document, LayerId, LayerId) {
    let mut doc = Document::new(size, size);
    let base = doc.current();
    let f = doc.add_filter_layer(LayerFilter::new(filter, []), "Filter");
    let mut canvas = Canvas::new(gpu, &doc).unwrap();
    canvas.set_layer_filters(filters(gpu));
    canvas.put_layer(base, [colour[0], colour[1], colour[2], 255].repeat((size * size) as usize));
    canvas.render(gpu, &mut doc, None);
    (canvas, doc, base, f)
}

#[test]
fn a_filter_layer_shows_what_is_below_it_through_its_filter_as_its_settings_say() {
    let Some(gpu) = gpu() else { return };
    let (mut canvas, mut doc, base, f) = over(&gpu, 16, [255, 0, 0], "t.grey");
    let shown = |canvas: &mut Canvas, doc: &mut Document| {
        canvas.render(&gpu, doc, None);
        at(&canvas.read_composite(&gpu).unwrap(), 16, 8, 8)
    };
    // pure red in its luminance (Rec. 709): 0.2126 of white
    let grey = shown(&mut canvas, &mut doc);
    assert!(near(grey, [54, 54, 54, 255], 1), "{grey:?}");
    // at half its opacity, half way
    doc.set_opacity(f, 0.5);
    canvas.invalidate();
    let half = shown(&mut canvas, &mut doc);
    assert!(near(half, [155, 27, 27, 255], 1), "{half:?}");
    // hidden, nothing
    doc.set_opacity(f, 1.0);
    doc.set_visible(f, false);
    canvas.invalidate();
    assert_eq!(shown(&mut canvas, &mut doc), [255, 0, 0, 255]);
    // by Multiply: the red times its grey
    doc.set_visible(f, true);
    doc.set_blend(f, BlendMode::Multiply);
    canvas.invalidate();
    let multiplied = shown(&mut canvas, &mut doc);
    assert!(near(multiplied, [54, 0, 0, 255], 1), "{multiplied:?}");
    doc.set_blend(f, BlendMode::Normal);
    // through its mask: black on the left half, so only the right half greys
    let mask = doc.add_mask(f).unwrap();
    canvas.render(&gpu, &mut doc, None);
    let half_black: Vec<u8> = (0..16 * 16).flat_map(|i| if i % 16 < 8 { [0, 0, 0, 255] } else { [255; 4] }).collect();
    canvas.put_layer(mask, half_black);
    canvas.render(&gpu, &mut doc, None);
    let px = canvas.read_composite(&gpu).unwrap();
    assert_eq!(at(&px, 16, 3, 8), [255, 0, 0, 255]);
    assert!(near(at(&px, 16, 12, 8), [54, 54, 54, 255], 1));
    // with its filter not given (its plugin off): nothing, its settings kept
    canvas.set_layer_filters(HashMap::new());
    assert_eq!(shown(&mut canvas, &mut doc), [255, 0, 0, 255]);
    assert!(doc.layer(f).unwrap().filter.is_some());
    let _ = base;
}

#[test]
fn a_filter_layer_s_editor_sets_what_its_filter_reads_as_its_table() {
    let Some(gpu) = gpu() else { return };
    let (mut canvas, mut doc, _base, f) = over(&gpu, 16, [200, 100, 50], "t.curves");
    let shown = |canvas: &mut Canvas, doc: &mut Document| {
        canvas.render(&gpu, doc, None);
        at(&canvas.read_composite(&gpu).unwrap(), 16, 8, 8)
    };
    // no curves kept: straight, nothing changes
    assert!(near(shown(&mut canvas, &mut doc), [200, 100, 50, 255], 1));
    // red turned over
    assert!(doc.set_filter_table(f, "curves.1", &[0.0, 1.0, 1.0, 0.0]));
    assert!(near(shown(&mut canvas, &mut doc), [55, 100, 50, 255], 1));
    // a gradient from blue to yellow, by red: 200 of 255 along
    let mut g = doc.layer(f).unwrap().filter.clone().unwrap();
    g.filter = "t.gradient".to_owned();
    assert!(g.set_table("gradient", &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0]));
    assert!(doc.put_filter(f, g));
    assert!(near(shown(&mut canvas, &mut doc), [200, 200, 55, 255], 1));
}

#[test]
fn clipped_to_a_layer_a_filter_layer_changes_only_that_layer() {
    let Some(gpu) = gpu() else { return };
    const S: u32 = 16;
    let mut doc = Document::new(S, S);
    let blue = doc.current();
    let red = doc.add_layer();
    let f = doc.add_filter_layer(LayerFilter::new("t.grey", []), "Filter");
    assert!(doc.can_clip(f));
    doc.set_clipped(f, true);
    let mut canvas = Canvas::new(&gpu, &doc).unwrap();
    canvas.set_layer_filters(filters(&gpu));
    canvas.put_layer(blue, [0, 0, 255, 255].repeat((S * S) as usize));
    // red on the right half alone
    let right: Vec<u8> = (0..S * S).flat_map(|i| if i % S >= S / 2 { [255, 0, 0, 255] } else { [0; 4] }).collect();
    canvas.put_layer(red, right);
    canvas.render(&gpu, &mut doc, None);
    let px = canvas.read_composite(&gpu).unwrap();
    assert_eq!(at(&px, S, 3, 8), [0, 0, 255, 255], "the blue below isn't clipped to");
    assert!(near(at(&px, S, 12, 8), [54, 54, 54, 255], 1), "{:?}", at(&px, S, 12, 8));
    // a layer is never clipped onto it while it isn't clipped itself (it has no paint to show it)
    doc.set_clipped(f, false);
    let above = doc.add_layer();
    assert!(!doc.can_clip(above));
    // nothing merges into it; it merges down into the layer below
    assert_eq!(doc.merge_target(above), None);
    assert_eq!(doc.merge_target(f), Some(red));
}

#[test]
fn merged_down_a_filter_layer_s_filter_goes_into_the_layer_below_alone() {
    let Some(gpu) = gpu() else { return };
    const S: u32 = 16;
    let mut doc = Document::new(S, S);
    let blue = doc.current();
    let red = doc.add_layer();
    let f = doc.add_filter_layer(LayerFilter::new("t.grey", []), "Filter");
    doc.set_opacity(f, 0.5);
    let mut canvas = Canvas::new(&gpu, &doc).unwrap();
    canvas.set_layer_filters(filters(&gpu));
    canvas.put_layer(blue, [0, 0, 255, 255].repeat((S * S) as usize));
    // red on the right half, nothing on the left
    let right: Vec<u8> = (0..S * S).flat_map(|i| if i % S >= S / 2 { [255, 0, 0, 255] } else { [0; 4] }).collect();
    canvas.put_layer(red, right);
    canvas.render(&gpu, &mut doc, None);
    let before = doc.clone();
    assert!(doc.finish_merge_down(f));
    canvas.merge_down(f, red, before, None);
    canvas.render(&gpu, &mut doc, None);
    let px = canvas.read_layer(&gpu, red).unwrap();
    // half way to its grey where it was red; still nothing where it was nothing
    assert!(near(at(&px, S, 12, 8), [155, 27, 27, 255], 1), "{:?}", at(&px, S, 12, 8));
    assert_eq!(at(&px, S, 3, 8)[3], 0);
    assert!(doc.layer(f).is_none());
    // the blue below untouched
    assert_eq!(at(&canvas.read_composite(&gpu).unwrap(), S, 3, 8), [0, 0, 255, 255]);
    // undone, as it was
    canvas.undo();
    canvas.render(&gpu, &mut doc, None);
    assert_eq!(at(&canvas.read_layer(&gpu, red).unwrap(), S, 12, 8), [255, 0, 0, 255]);
}

/// The composite after a change within a small square, drawn around it only, against the whole
/// composite drawn again: the same, and changed out to as far as the filter reaches.
fn drawn_around_as_whole(filter: &str, reaches_far: bool) {
    let Some(gpu) = gpu() else { return };
    const S: u32 = 64;
    let mut doc = Document::new(S, S);
    let base = doc.current();
    let _f = doc.add_filter_layer(LayerFilter::new(filter, []), "Filter");
    doc.set_current(base);
    let mut canvas = Canvas::new(&gpu, &doc).unwrap();
    canvas.set_cache_composites(true);
    canvas.set_layer_filters(filters(&gpu));
    let checks: Vec<u8> = (0..S * S)
        .flat_map(|i| if (i % S / 4 + i / S / 4).is_multiple_of(2) { [20, 120, 220, 255] } else { [240, 200, 10, 255] })
        .collect();
    canvas.put_layer(base, checks);
    canvas.render(&gpu, &mut doc, None);
    let before = canvas.read_composite(&gpu).unwrap();
    // a small black square about (30, 30)
    canvas.put_layer_region(base, 28, 28, 4, 4, [0, 0, 0, 255].repeat(16));
    canvas.render(&gpu, &mut doc, None);
    let around = canvas.read_composite(&gpu).unwrap();
    canvas.invalidate();
    canvas.render(&gpu, &mut doc, None);
    let whole = canvas.read_composite(&gpu).unwrap();
    let differs: Vec<(u32, u32)> = (0..S * S)
        .filter(|i| at(&around, S, i % S, i / S) != at(&whole, S, i % S, i / S))
        .map(|i| (i % S, i / S))
        .collect();
    assert!(differs.is_empty(), "{} pixels differ, the first at {:?}", differs.len(), differs.first());
    // changed beyond the square, as far as the filter carries it
    let (x, y) = if reaches_far { (S - 30, 30) } else { (33, 30) };
    assert_ne!(at(&before, S, x, y), at(&whole, S, x, y));
}

#[test]
fn a_change_drawn_around_shows_as_the_whole_canvas_drawn_again_through_a_filter_that_reaches() {
    drawn_around_as_whole("t.box", false);
}

#[test]
fn a_change_drawn_around_shows_as_the_whole_canvas_drawn_again_through_a_filter_that_reads_anywhere() {
    drawn_around_as_whole("t.mirror", true);
}

#[test]
fn a_filter_layer_s_reach_follows_its_value() {
    let Some(gpu) = gpu() else { return };
    // radius 2 as the filter starts it; 5 as the layer keeps it
    let (mut canvas, mut doc, _base, f) = over(&gpu, 16, [10, 20, 30], "t.box");
    let mut far = doc.layer(f).unwrap().filter.clone().unwrap();
    assert!(far.set("radius", 5.0));
    assert!(doc.put_filter(f, far));
    canvas.invalidate();
    canvas.render(&gpu, &mut doc, None);
    // a flat colour blurred is the colour
    assert!(near(at(&canvas.read_composite(&gpu).unwrap(), 16, 8, 8), [10, 20, 30, 255], 1));
}

#[test]
fn a_filter_layers_filter_is_tried_before_it_is_used() {
    let Some(gpu) = gpu() else { return };
    let (canvas, ..) = over(&gpu, 16, [10, 20, 30], "t.grey");
    let grey = made(&gpu, GREY, &[], FilterReach::Within(0.0));
    assert!(matches!(canvas.try_layer_filter(&gpu, &grey.program), procraft_gpu::FilterRun::Drew(_)));
    // one that never ends runs out of its budget on the pixels tried
    let endless = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
        var c = colour;
        loop { c = c * 0.999; }
        return c;
    }";
    let endless = made(&gpu, endless, &[], FilterReach::Within(0.0));
    assert_eq!(canvas.try_layer_filter(&gpu, &endless.program), procraft_gpu::FilterRun::Spent);
}

/// How long the canvas takes drawn whole with three filter layers over a painted layer, and a
/// brush's square of it: run with `--ignored --nocapture`.
#[test]
#[ignore]
fn filter_layers_cost() {
    let Some(gpu) = gpu() else { return };
    const S: u32 = 2048;
    let mut doc = Document::new(S, S);
    let base = doc.current();
    for name in ["t.grey", "t.box", "t.grey"] {
        doc.add_filter_layer(LayerFilter::new(name, []), "Filter");
    }
    doc.set_current(base);
    let mut canvas = Canvas::new(&gpu, &doc).unwrap();
    canvas.set_cache_composites(true);
    canvas.set_layer_filters(filters(&gpu));
    canvas.put_layer(base, [90, 140, 200, 255].repeat((S * S) as usize));
    canvas.render(&gpu, &mut doc, None);
    let _ = gpu.wait();
    let started = Instant::now();
    for _ in 0..5 {
        canvas.invalidate();
        canvas.render(&gpu, &mut doc, None);
        let _ = gpu.wait();
    }
    let whole = started.elapsed() / 5;
    let started = Instant::now();
    for i in 0..30u32 {
        canvas.put_layer_region(base, 100 + i * 10, 100, 64, 64, [0, 0, 0, 255].repeat(64 * 64));
        canvas.render(&gpu, &mut doc, None);
        let _ = gpu.wait();
    }
    let square = started.elapsed() / 30;
    println!("filter layers: whole {whole:?}, a 64 square {square:?}");
    assert!(whole.as_millis() < 400, "{whole:?}");
    assert!(square.as_millis() < 40, "{square:?}");
}
