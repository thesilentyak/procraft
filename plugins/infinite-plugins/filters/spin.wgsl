// Spin, as a filter layer: what is below it, changed.

fn fl_pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn fl_straight(c: vec4<f32>) -> vec4<f32> {
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 1e-6);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // smeared round the point, one way above the middle and the other below
    let middle = vec2<f32>(v.touch_x, v.touch_y);
    let d = at - middle;
    let turn = v.value * 0.35;
    var sum = vec4<f32>(0.0);
    for (var i = 0; i < 32; i++) {
        let t = turn * f32(i) / 31.0;
        let r = mat2x2<f32>(cos(t), sin(t), -sin(t), cos(t)) * d;
        sum += fl_pre(layer_at(middle + r));
    }
    return fl_straight(sum / 32.0);
}
