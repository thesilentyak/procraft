// Exposure, as a filter layer: what is below it, changed.

// The centred value (-1..1) as 0..1, 0.5 in the middle.
fn fl_half(value: f32) -> f32 {
    return clamp((value + 1.0) * 0.5, 0.0, 1.0);
}

// The colour adjustments' value v (0..1, 0.5 none) eased: gentle about the middle, steep at the ends.
fn fl_ease(v: f32) -> f32 {
    if (v < 0.5) {
        return 0.5 * sqrt(2.0 * v);
    }
    let t = 2.0 * v - 1.0;
    return 0.5 + 0.5 * t * t;
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let u = fl_ease(fl_half(v.value));
    let k = select(2.0 * u, 0.5 + u, u < 0.5);
    return vec4<f32>(colour.rgb * k, colour.a);
}
