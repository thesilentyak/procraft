// Kaleidoscope, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // wedges about the canvas's middle, each a mirror of the next, the picture moved by the point
    let size = canvas_size();
    let middle = size * 0.5;
    let wedge = 6.2831853 / (1.0 + 11.0 * v.value);
    let d = at - middle;
    let r = length(d);
    var t = atan2(d.y, d.x) + 3.1415927;
    let n = floor(t / wedge);
    t = t - n * wedge;
    if (n % 2.0 >= 1.0) {
        t = wedge - t;
    }
    var p = middle + vec2<f32>(cos(t), sin(t)) * r + (vec2<f32>(v.touch_x, v.touch_y) - middle);
    // past the edges, mirrored back
    p = abs(p);
    p = size - abs(size - (p % (2.0 * size)));
    return layer_at(p);
}
