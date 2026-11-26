// Stained Glass, as a filter layer: what is below it, changed.

// A number 0..1 for whole pixel p and seed: the same each time, unlike for the next pixel.
fn fl_hash(p: vec2<f32>, seed: f32) -> f32 {
    let x = u32(max(p.x, 0.0));
    let y = u32(max(p.y, 0.0));
    let s = u32(max(seed, 0.0));
    var h = (x * 73856093u) ^ (y * 19349663u) ^ (s * 83492791u);
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return f32(h) / 4294967295.0;
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // cells round points scattered over the canvas, each flat in its point's colour: fewer and
    // larger as the value grows
    let across = 10.0 + 512.0 * (1.0 - v.value) * (1.0 - v.value);
    let cell = canvas_size().y / across;
    let p = at / cell;
    let home = floor(p);
    var best = 1e9;
    var nearest = home;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let c = home + vec2<f32>(f32(x), f32(y));
            let point = c + vec2<f32>(fl_hash(c + 4096.0, 1.0), fl_hash(c + 4096.0, 2.0));
            let d = length(point - p);
            if (d < best) {
                best = d;
                nearest = point;
            }
        }
    }
    return layer_at(nearest * cell);
}
