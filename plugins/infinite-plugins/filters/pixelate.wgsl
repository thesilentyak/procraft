// Pixelate, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // squares as wide as the value says, from the top left, each the colour near its corner
    let side = floor(1.0 + 99.0 * v.value);
    let corner = floor(at / side) * side;
    return layer_at(corner + vec2<f32>(0.5));
}
