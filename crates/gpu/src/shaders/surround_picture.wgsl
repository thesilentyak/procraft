// A picture round the canvas in place of the surround's grid: fitted to cover its place, over the
// surround's colour where it is see-through.

struct PictureUniform {
    // where the picture lies, target pixels: left, top, right, bottom
    v0: vec4<f32>,
    // (the surround's colour, sRGB; 1 when the target encodes sRGB itself)
    v1: vec4<f32>,
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
@group(1) @binding(0) var<uniform> pu: PictureUniform;

@vertex
fn vs_picture(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_picture(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let span = max(pu.v0.zw - pu.v0.xy, vec2<f32>(1.0));
    let uv = clamp((pos.xy - pu.v0.xy) / span, vec2<f32>(0.0), vec2<f32>(1.0));
    // straight sRGB, over the surround's colour by its alpha
    let seen = textureSampleLevel(picture, picture_sampler, uv, 0.0);
    var rgb = mix(pu.v1.rgb, seen.rgb, seen.a);
    if (pu.v1.w > 0.5) {
        rgb = srgb_to_linear(rgb);
    }
    return vec4<f32>(rgb, 1.0);
}
