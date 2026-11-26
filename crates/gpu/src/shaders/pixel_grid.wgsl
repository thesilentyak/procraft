// A pixel art canvas's pixel grid: a fine line along every pixel's edge, over the canvas.

struct PixelGridUniform {
    // target pixels to canvas pixels: (a, b, c, d) and (e, f), then the canvas's width and height
    m0: vec4<f32>,
    m1: vec4<f32>,
    // (target pixels to a canvas pixel, a line's width in target pixels, unused, unused)
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
