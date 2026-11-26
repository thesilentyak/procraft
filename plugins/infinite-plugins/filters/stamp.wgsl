// Stamp, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // black where it is darker than the value, else white
    let dark = 1.0 - (colour.r + colour.g + colour.b) / 3.0;
    let ink = smoothstep(v.value - 0.01, v.value + 0.01, dark);
    return vec4<f32>(vec3<f32>(1.0 - ink), colour.a);
}
