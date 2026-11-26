// Glow, as a filter layer: what is below it, changed.

fn fl_luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let l = fl_luma(colour.rgb);
    return vec4<f32>(colour.rgb + l * l * v.value * colour.rgb, colour.a);
}
