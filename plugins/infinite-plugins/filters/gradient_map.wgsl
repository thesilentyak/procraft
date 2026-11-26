// Gradient Map, as a filter layer: each colour's lightness mapped through the gradient the editor
// set (row 0), its alpha kept.
fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let l = dot(colour.rgb, vec3<f32>(0.30, 0.59, 0.11));
    return vec4<f32>(table(0, l).rgb, colour.a);
}
