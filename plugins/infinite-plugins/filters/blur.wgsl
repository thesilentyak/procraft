// Blur, as a filter layer: what is below it, changed.

fn fl_pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn fl_straight(c: vec4<f32>) -> vec4<f32> {
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 1e-6);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // a Gaussian-weighted disc, wider as the value grows (by its square)
    let r = 1.0 + 63.0 * v.value * v.value;
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    for (var k = 0; k < 80; k++) {
        let t = (f32(k) + 0.5) / 80.0;
        let a = f32(k) * 2.39996323;
        let w = exp(-3.0 * t);
        sum += fl_pre(layer_at(at + vec2<f32>(cos(a), sin(a)) * sqrt(t) * r)) * w;
        weights += w;
    }
    return fl_straight(sum / weights);
}
