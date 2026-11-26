// Emboss, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // lit from toward the lower right: what rises toward the light is lighter, warm; away, cool
    let toward = normalize(canvas_size() * vec2<f32>(0.8, 1.0) - at + vec2<f32>(1e-3));
    let a = layer_at(at - toward * 1.5);
    let b = layer_at(at + toward * 1.5);
    let ha = dot(a.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)) * a.a;
    let hb = dot(b.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)) * b.a;
    let s = clamp(0.5 + (hb - ha) * v.value * 3.0, 0.0, 1.0);
    let tint = select(vec3<f32>(0.85, 0.9, 1.0), vec3<f32>(1.0, 0.95, 0.85), s >= 0.5);
    let shade = vec3<f32>(s) * mix(vec3<f32>(1.0), tint, abs(s - 0.5) * 2.0);
    // soft light over the colour
    let c = colour.rgb;
    let soft = select(
        c + (2.0 * shade - 1.0) * (sqrt(c) - c),
        c - (1.0 - 2.0 * shade) * c * (1.0 - c),
        shade <= vec3<f32>(0.5),
    );
    return vec4<f32>(soft, colour.a);
}
