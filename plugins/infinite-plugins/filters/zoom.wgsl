// Zoom, as a filter layer: what is below it, changed.

fn fl_pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn fl_straight(c: vec4<f32>) -> vec4<f32> {
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 1e-6);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // smeared toward the point, more the farther from it
    let middle = vec2<f32>(v.touch_x, v.touch_y);
    let k = (1.0 + 63.0 * v.value) * v.value / 64.0;
    var sum = vec4<f32>(0.0);
    for (var i = 0; i < 32; i++) {
        let t = f32(i) / 31.0;
        sum += fl_pre(layer_at(at + (middle - at) * t * k * 0.5));
    }
    return fl_straight(sum / 32.0);
}
