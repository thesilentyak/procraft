// Sepia, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let c = colour.rgb;
    let s = min(vec3<f32>(
        dot(c, vec3<f32>(0.393, 0.769, 0.189)),
        dot(c, vec3<f32>(0.349, 0.686, 0.168)),
        dot(c, vec3<f32>(0.272, 0.534, 0.131)),
    ), vec3<f32>(1.0));
    return vec4<f32>(mix(c, s, v.value), colour.a);
}
