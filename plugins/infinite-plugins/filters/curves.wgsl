// Curves, as a filter layer: red, green and blue each through its own curve, then the lightness
// moved as the gamma curve moves it, and the alpha through its curve (rows 1, 2, 3, 0 and 4 of
// what the curves editor set).
fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    var rgb = vec3<f32>(table(1, colour.r).r, table(2, colour.g).r, table(3, colour.b).r);
    let y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    rgb = rgb + (table(0, y).r - y);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), table(4, colour.a).r);
}
