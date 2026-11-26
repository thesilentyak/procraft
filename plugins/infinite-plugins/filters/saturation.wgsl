// Saturation, as a filter layer: what is below it, changed.

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
    let f = 2.0 * (fl_ease(fl_half(v.value)) - 0.5);
    let c = colour.rgb;
    let grey = dot(c, vec3<f32>(0.33, 0.34, 0.33));
    // up, away from its grey (twice as far at the top); down, toward it (all grey at the bottom)
    let away = c + (c - grey) * f;
    let toward = mix(c, vec3<f32>(grey), -f);
    return vec4<f32>(select(toward, away, f > 0.0), colour.a);
}
