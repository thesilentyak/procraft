// Edges, as a filter layer: what is below it, changed.

// How strongly the picture changes about `at`: the largest of each channel's (alpha's too) gradient.
fn fl_edge(at: vec2<f32>) -> f32 {
    var gx = vec4<f32>(0.0);
    var gy = vec4<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let c = layer_at(at + vec2<f32>(f32(x), f32(y)));
            let p = vec4<f32>(c.rgb * c.a, c.a);
            let wx = f32(x) * select(1.0, 2.0, y == 0);
            let wy = f32(y) * select(1.0, 2.0, x == 0);
            gx += p * wx;
            gy += p * wy;
        }
    }
    let g = sqrt(gx * gx + gy * gy);
    return max(max(g.r, g.g), max(g.b, g.a));
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // black lines where the picture changes, on nothing
    let w = 0.5 + 0.5 * v.value;
    let a = smoothstep(0.8 - 0.8 * w, 1.0, fl_edge(at));
    return vec4<f32>(0.0, 0.0, 0.0, a);
}
