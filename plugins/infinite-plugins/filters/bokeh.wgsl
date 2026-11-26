// Bokeh, as a filter layer: what is below it, changed.

fn fl_pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

fn fl_straight(c: vec4<f32>) -> vec4<f32> {
    return select(vec4<f32>(0.0), vec4<f32>(c.rgb / c.a, c.a), c.a > 1e-6);
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // the mean of a disc, light colours counting for more, so highlights open into discs
    let r = 2.0 + 46.0 * v.value;
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    for (var k = 0; k < 96; k++) {
        let t = (f32(k) + 0.5) / 96.0;
        let a = f32(k) * 2.39996323;
        let p = layer_at(at + vec2<f32>(cos(a), sin(a)) * sqrt(t) * r);
        let l = dot(p.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        let w = 1.0 + 4.0 * l * l;
        sum += fl_pre(p) * w;
        weights += w;
    }
    return fl_straight(sum / weights);
}
