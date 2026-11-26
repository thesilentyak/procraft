// A pixel art canvas's pixel grid, a fine line along every pixel's edge over the canvas; and the
// checkerboard under it, which its clear pixels show.

struct PixelGridUniform {
    // target pixels to canvas pixels: (a, b, c, d) and (e, f), then the canvas's width and height
    m0: vec4<f32>,
    m1: vec4<f32>,
    // (target pixels to a canvas pixel, a line's width in target pixels, the checkerboard's squares in
    // canvas pixels, 1 when the target encodes sRGB itself)
    v0: vec4<f32>,
    // the lines' colour, premultiplied by their alpha (linear when the target encodes sRGB itself)
    colour: vec4<f32>,
}

@group(0) @binding(0) var<uniform> pg: PixelGridUniform;

@vertex
fn vs_pixel_grid(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_pixel_grid(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<f32>(
        pg.m0.x * pos.x + pg.m0.z * pos.y + pg.m1.x,
        pg.m0.y * pos.x + pg.m0.w * pos.y + pg.m1.y,
    );
    // only over the canvas, and not along its outer edge
    if (p.x <= 0.0 || p.y <= 0.0 || p.x >= pg.m1.z || p.y >= pg.m1.w) {
        discard;
    }
    // how far (target pixels) this pixel's centre is from the nearest pixel edge
    let d = min(abs(p.x - round(p.x)), abs(p.y - round(p.y))) * pg.v0.x;
    let w = pg.v0.y;
    let cover = clamp(min(w, w * 0.5 + 0.5 - d), 0.0, 1.0);
    if (cover <= 0.0) {
        discard;
    }
    return pg.colour * cover;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// The checkerboard over the canvas's rectangle, under its pixels: squares of whole canvas pixels in
// two light greys, so clear pixels show as clear, lined up with the pixels themselves.
@fragment
fn fs_pixel_backdrop(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<f32>(
        pg.m0.x * pos.x + pg.m0.z * pos.y + pg.m1.x,
        pg.m0.y * pos.x + pg.m0.w * pos.y + pg.m1.y,
    );
    if (p.x < 0.0 || p.y < 0.0 || p.x >= pg.m1.z || p.y >= pg.m1.w) {
        discard;
    }
    let cell = floor(p / max(pg.v0.z, 1.0));
    let odd = (i32(cell.x) + i32(cell.y)) & 1;
    var grey = select(vec3<f32>(0.84), vec3<f32>(0.70), odd == 1);
    if (pg.v0.w > 0.5) {
        grey = srgb_to_linear(grey);
    }
    return vec4<f32>(grey, 1.0);
}
