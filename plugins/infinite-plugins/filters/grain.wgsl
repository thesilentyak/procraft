// Grain, as a filter layer: what is below it, changed.

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
    // grey noise, a value for each pixel, drawn afresh by a tap
    let n = fl_hash(floor(at), v.touch_seed);
    return vec4<f32>(colour.rgb + v.value * (n - 0.5), colour.a);
}
