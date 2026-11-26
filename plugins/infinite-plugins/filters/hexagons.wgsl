// Hexagons, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // hexagons as wide as the value says, round the canvas's middle, each its middle's colour
    let side = 1.0 + 99.0 * v.value;
    let p = (at - canvas_size() * 0.5) / side;
    let s = vec2<f32>(1.0, 1.7320508);
    let ha = (floor(p / s) + 0.5) * s;
    let hb = (floor((p - s * 0.5) / s) + 0.5) * s + s * 0.5;
    let middle = select(hb, ha, length(p - ha) < length(p - hb));
    return layer_at(middle * side + canvas_size() * 0.5);
}
