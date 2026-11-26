// Erode, as a filter layer: what is below it, changed.

fn fl_pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn fl_straight(c: vec4<f32>) -> vec4<f32> {
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 1e-6);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // over a disc a few pixels across: rings of samples, more round the farther ones
    let r = 8.0 * (1.0 + v.value);
    var out = fl_pre(colour);
    for (var ring = 1; ring <= 10; ring++) {
        let radius = r * f32(ring) / 10.0;
        let count = 4 + ring * 4;
        for (var k = 0; k < count; k++) {
            let a = 6.2831853 * (f32(k) + 0.5 * f32(ring % 2)) / f32(count);
            let p = fl_pre(layer_at(at + vec2<f32>(cos(a), sin(a)) * radius));
            out = min(out, p);
        }
    }
    return fl_straight(out);
}
