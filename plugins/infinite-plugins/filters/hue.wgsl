// Hue, as a filter layer: what is below it, changed.

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

// RGB to hue (0..1), saturation and value, and back.
fn fl_hsv(c: vec3<f32>) -> vec3<f32> {
    let hi = max(c.r, max(c.g, c.b));
    let lo = min(c.r, min(c.g, c.b));
    let d = hi - lo;
    var h = 0.0;
    if (d > 0.0) {
        if (hi == c.r) {
            h = (c.g - c.b) / d;
        } else if (hi == c.g) {
            h = 2.0 + (c.b - c.r) / d;
        } else {
            h = 4.0 + (c.r - c.g) / d;
        }
        h = fract(h / 6.0);
    }
    return vec3<f32>(h, select(0.0, d / hi, hi > 0.0), hi);
}

fn fl_from_hsv(h: vec3<f32>) -> vec3<f32> {
    let p = fract(vec3<f32>(5.0, 3.0, 1.0) / 6.0 + h.x) * 6.0;
    return h.z - h.z * h.y * clamp(min(p, 4.0 - p), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let u = fl_ease(fl_half(v.value));
    var h = fl_hsv(colour.rgb);
    // half a turn at either end
    h.x = fract(h.x - (u - 0.5) + 1.0);
    return vec4<f32>(fl_from_hsv(h), colour.a);
}
