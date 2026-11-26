// Lens Flare, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let size = canvas_size();
    let span = length(size);
    let light = vec2<f32>(v.touch_x, v.touch_y);
    let middle = size * 0.5;
    let d = length(at - light) / span;
    // a glow round the light, a faint ring, and ghosts along the line through the middle
    var add = vec3<f32>(1.0, 0.9, 0.7) * exp(-d * d / 0.003) * 1.2;
    let q = (d - 0.12) / 0.01;
    add += vec3<f32>(0.6, 0.7, 1.0) * exp(-q * q) * 0.25;
    var alongs = array<f32, 4>(0.6, 1.1, 1.5, 1.9);
    var radii = array<f32, 4>(0.025, 0.05, 0.018, 0.07);
    for (var k = 0; k < 4; k++) {
        let along = alongs[k];
        let radius = radii[k];
        let ghost = light + (middle - light) * along;
        let g = length(at - ghost) / span;
        add += vec3<f32>(1.0, 0.75, 0.45) * smoothstep(radius, radius * 0.6, g) * 0.18;
    }
    let lit = add * v.value * 1.6;
    // added over what is there, where there is anything
    let rgb = colour.rgb * colour.a + lit;
    let a = max(colour.a, min(max(lit.r, max(lit.g, lit.b)), 1.0));
    return vec4<f32>(select(vec3<f32>(0.0), rgb / a, a > 0.0), a);
}
