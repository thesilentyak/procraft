// Smooth, as a filter layer: what is below it, changed.

fn fl_pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn fl_straight(c: vec4<f32>) -> vec4<f32> {
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 1e-6);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // a small Gaussian blur that leaves out what differs from the pixel: edges stay
    let r = i32(ceil(1.0 + 3.0 * v.value));
    let sigma = (1.0 + 3.0 * v.value) * 0.6;
    let cut = 0.2 * v.value * v.value;
    let c = colour;
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    for (var y = -r; y <= r; y++) {
        for (var x = -r; x <= r; x++) {
            let p = layer_at(at + vec2<f32>(f32(x), f32(y)));
            let far = abs(p - c);
            if (max(max(far.r, far.g), max(far.b, far.a)) < cut || (x == 0 && y == 0)) {
                let w = exp(-f32(x * x + y * y) / (2.0 * sigma * sigma));
                sum += fl_pre(p) * w;
                weights += w;
            }
        }
    }
    return fl_straight(sum / weights);
}
