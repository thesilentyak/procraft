// Halftone, as a filter layer: what is below it, changed.

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    // cyan, magenta, yellow and black dots, each on a screen at its own angle
    let size = 0.1 + 0.8 * v.value;
    let period = 6.2831853 / (1.0 - size);
    let c = colour.rgb;
    let k = 1.0 - max(c.r, max(c.g, c.b));
    let inks = select(vec3<f32>(0.0), (1.0 - c - k) / (1.0 - k), k < 1.0);
    let amounts = vec4<f32>(inks, k);
    let angles = vec4<f32>(0.2618, 1.309, 0.1, 0.7854);
    var covered = vec4<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        let t = angles[i];
        let turned = mat2x2<f32>(cos(t), -sin(t), sin(t), cos(t)) * at;
        let cell = fract(turned / period) - 0.5;
        let dist = length(cell) * period;
        let radius = 0.5 * period * sqrt(amounts[i]) * 1.15;
        covered[i] = 1.0 - smoothstep(radius - 0.75, radius + 0.75, dist);
    }
    let rgb = (1.0 - covered.xyz) * (1.0 - covered.w);
    return vec4<f32>(rgb, colour.a);
}
