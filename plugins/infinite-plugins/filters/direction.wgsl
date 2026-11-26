// Direction, as a filter layer: what is below it, changed.

fn fl_pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn fl_straight(c: vec4<f32>) -> vec4<f32> {
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 1e-6);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // smeared along the drag's angle, longer as the value grows
    let length_ = (1.0 + 127.0 * v.value) * v.value;
    let along = vec2<f32>(cos(v.touch_angle), sin(v.touch_angle));
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < 33; k++) {
        let t = f32(k) / 32.0 - 0.5;
        sum += fl_pre(layer_at(at + along * t * length_));
    }
    return fl_straight(sum / 33.0);
}
