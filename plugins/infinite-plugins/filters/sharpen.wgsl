// Sharpen, as a filter layer: what is below it, changed.

fn fl_pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn fl_straight(c: vec4<f32>) -> vec4<f32> {
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 1e-6);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // away from the mean of what is round it, a pixel and a half off each way
    var near = vec4<f32>(0.0);
    var ways = array<vec2<f32>, 4>(vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, -1.0));
    for (var k = 0; k < 4; k++) {
        let d = ways[k];
        near += (fl_pre(layer_at(at + d)) + fl_pre(layer_at(at + d * 2.0))) * 0.125;
    }
    let c = fl_pre(colour);
    return fl_straight(clamp(c + (c - near) * v.value * 2.0, vec4<f32>(0.0), vec4<f32>(1.0)));
}
