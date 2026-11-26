// Vibrance, as a filter layer: what is below it, changed.

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
    let hi = max(c.r, max(c.g, c.b));
    let lo = min(c.r, min(c.g, c.b));
    let sat = select(0.0, (hi - lo) / hi, hi > 0.0);
    let grey = dot(c, vec3<f32>(0.33, 0.34, 0.33));
    // the duller a colour, the more it moves: away from its grey, or toward it
    let amount = abs(f) * (1.0 - sat);
    let away = c + (c - grey) * amount;
    let toward = mix(c, vec3<f32>(grey), amount);
    return vec4<f32>(select(toward, away, f > 0.0), colour.a);
}
