// Stereo, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // red, green and blue each from a third of a turn apart, as far as the value's square says
    let far = 256.0 * v.value * v.value;
    let size = canvas_size();
    var rgb = vec3<f32>(0.0);
    var a = 0.0;
    var turns = array<f32, 3>(0.5235988, 2.6179939, 4.712389);
    for (var i = 0; i < 3; i++) {
        let t = turns[i];
        let p = at + vec2<f32>(cos(t), sin(t)) * far;
        var c = vec4<f32>(0.0);
        if (p.x >= 0.0 && p.y >= 0.0 && p.x < size.x && p.y < size.y) {
            c = layer_at(p);
        }
        rgb[i] = c[i] * c.a;
        a += c.a / 3.0;
    }
    return vec4<f32>(select(vec3<f32>(0.0), rgb / a, a > 0.0), a);
}
