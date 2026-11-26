// Manga, as a filter layer: what is below it, changed.

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
    // black lines where the picture changes sharply; else one of three tones by its lightness
    let edge = fl_edge(at);
    if (edge > 0.5) {
        return vec4<f32>(0.0, 0.0, 0.0, colour.a);
    }
    let l = dot(colour.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)) + (v.value - 0.5);
    var tone = 1.0;
    if (l < 0.25) {
        tone = 20.0 / 255.0;
    } else if (l < 0.4) {
        // a fine screen of diagonal lines, two greys
        let stripe = fract((at.x + at.y) / 4.0) < 0.5;
        tone = select(0.85, 0.35, stripe);
    }
    return vec4<f32>(vec3<f32>(tone), colour.a);
}
