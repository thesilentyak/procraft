// Invert, as a filter layer: what is below it, changed.

// The centred value (-1..1) as 0..1, 0.5 in the middle.
fn fl_half(value: f32) -> f32 {
    return clamp((value + 1.0) * 0.5, 0.0, 1.0);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let k = fl_half(v.value);
    return vec4<f32>(mix(colour.rgb, 1.0 - colour.rgb, k), colour.a);
}
