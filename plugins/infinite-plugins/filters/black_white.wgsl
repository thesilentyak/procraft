// Black & White, as a filter layer: what is below it, changed.

// sRGB to linear light and back.
fn fl_lin(c: vec3<f32>) -> vec3<f32> {
    let k = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(pow((k + 0.055) / 1.055, vec3<f32>(2.4)), k / 12.92, k <= vec3<f32>(0.04045));
}

fn fl_enc(c: vec3<f32>) -> vec3<f32> {
    let k = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(k, vec3<f32>(1.0 / 2.4)) - 0.055, k * 12.92, k <= vec3<f32>(0.0031308));
}

fn fl_f(t: f32) -> f32 {
    return select(t / 0.12841855 + 4.0 / 29.0, pow(max(t, 0.0), 1.0 / 3.0), t > 0.008856452);
}

fn fl_finv(t: f32) -> f32 {
    return select(0.12841855 * (t - 4.0 / 29.0), t * t * t, t > 6.0 / 29.0);
}

// sRGB to CIE L*a*b* (D65) and back.
fn fl_lab(c: vec3<f32>) -> vec3<f32> {
    let l = fl_lin(c);
    let x = dot(l, vec3<f32>(0.4124564, 0.3575761, 0.1804375)) / 0.95047;
    let y = dot(l, vec3<f32>(0.2126729, 0.7151522, 0.0721750));
    let z = dot(l, vec3<f32>(0.0193339, 0.1191920, 0.9503041)) / 1.08883;
    let fx = fl_f(x);
    let fy = fl_f(y);
    let fz = fl_f(z);
    return vec3<f32>(116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz));
}

fn fl_rgb(lab: vec3<f32>) -> vec3<f32> {
    let fy = (lab.x + 16.0) / 116.0;
    let fx = fy + lab.y / 500.0;
    let fz = fy - lab.z / 200.0;
    let x = fl_finv(fx) * 0.95047;
    let y = fl_finv(fy);
    let z = fl_finv(fz) * 1.08883;
    let l = vec3<f32>(
        3.2404542 * x - 1.5371385 * y - 0.4985314 * z,
        -0.9692660 * x + 1.8760108 * y + 0.0415560 * z,
        0.0556434 * x - 0.2040259 * y + 1.0572252 * z,
    );
    return clamp(fl_enc(l), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> {
    let c = colour.rgb;
    // a grey as light as the colour is, or toward its brightest or darkest channel
    let y = dot(fl_lin(c), vec3<f32>(0.2126729, 0.7151522, 0.0721750));
    let grey = fl_enc(vec3<f32>(y)).r;
    let hi = max(c.r, max(c.g, c.b));
    let lo = min(c.r, min(c.g, c.b));
    let toward = select(lo, hi, v.value > 0.0);
    return vec4<f32>(vec3<f32>(mix(grey, toward, abs(v.value))), colour.a);
}
