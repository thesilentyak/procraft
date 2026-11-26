// Fisheye, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // round the point, bulging out (up) or pinched in (down), as far as the shorter side
    let size = canvas_size();
    let middle = vec2<f32>(v.touch_x, v.touch_y);
    let reach = 0.5 * min(size.x, size.y);
    let d = (at - middle) / reach;
    let r = length(d);
    if (r >= 1.0 || r <= 0.0) {
        return colour;
    }
    let s = clamp(v.value, -1.0, 1.0);
    var src = r;
    if (s > 0.0) {
        let k = s * 1.4;
        src = tan(r * atan(k)) / k;
    } else if (s < 0.0) {
        let k = -s * 1.4;
        src = atan(r * k) / atan(k);
    }
    return layer_at(middle + d / r * src * reach);
}
