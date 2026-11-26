// What a filter layer's filter with an editor (interface 18) is joined to: as plugin_filter_layer_17
// is, and given `table` too, what its editor set baked into rows of 256 (a curve's heights in red,
// a gradient's colours). Kept as it is once released, as plugin_filter_1.wgsl is.
// `layer_at` reads that picture no further from the pixel being made than the filter says it
// reaches (host_u.c.z canvas pixels each way; below 0, anywhere): drawing the canvas again around
// a change makes only so much more of what is below again. Everything named host_ is the app's.

struct HostUniform {
    // the values, in the order the filter lists them
    a: vec4<f32>,
    b: vec4<f32>,
    // (the canvas's width, height, unused, unused)
    size: vec4<f32>,
    // (the probe's width and height, how far the filter reaches, unused)
    c: vec4<f32>,
}

@group(0) @binding(0) var host_copy: texture_2d<f32>;
@group(1) @binding(0) var<uniform> host_u: HostUniform;
@group(2) @binding(0) var host_table: texture_2d<f32>;

struct HostValues { /*VALUES*/ }
var<private> v: HostValues;

// What this pixel may still spend; -1 once it ran out. Every loop pays from it as it goes round.
var<private> host_budget: i32 = /*BUDGET*/;
// The middle of the pixel being made, canvas pixels.
var<private> host_at: vec2<f32>;

@vertex
fn host_vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn host_straight(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(c.rgb / c.a, c.a);
}

// What is below at `at`, in canvas pixels, colour straight: past an edge, the edge's; past the
// filter's reach, the nearest point within it.
fn layer_at(at: vec2<f32>) -> vec4<f32> {
    var host_to = at;
    if (host_u.c.z >= 0.0) {
        host_to = clamp(at, host_at - vec2<f32>(host_u.c.z), host_at + vec2<f32>(host_u.c.z));
    }
    let most = vec2<i32>(host_u.size.xy) - vec2<i32>(1);
    let i = clamp(vec2<i32>(floor(host_to)), vec2<i32>(0), most);
    return host_straight(textureLoad(host_copy, i, 0));
}

fn canvas_size() -> vec2<f32> {
    return host_u.size.xy;
}

// Row `row` of what the editor set, at `x` (0..1): past its last row, its last.
fn table(row: i32, x: f32) -> vec4<f32> {
    let rows = i32(textureDimensions(host_table).y);
    let i = clamp(i32(round(clamp(x, 0.0, 1.0) * 255.0)), 0, 255);
    return textureLoad(host_table, vec2<i32>(i, clamp(row, 0, rows - 1)), 0);
}

// What pixel i of what is below becomes, premultiplied. A pixel that ran out of its budget, or
// came back with what isn't a colour, stays as it was.
fn host_filtered(i: vec2<i32>) -> vec4<f32> {
    v = HostValues(/*READ*/);
    host_at = vec2<f32>(i) + vec2<f32>(0.5);
    let was = textureLoad(host_copy, i, 0);
    let made = pixel(host_straight(was), host_at);
    if (host_budget < 0 || !all(abs(made) < vec4<f32>(3.0e38))) {
        return was;
    }
    let colour = clamp(made, vec4<f32>(0.0), vec4<f32>(1.0));
    return vec4<f32>(colour.rgb * colour.a, colour.a);
}

@fragment
fn host_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return host_filtered(vec2<i32>(pos.xy));
}

// A sample spread over the canvas that says only whether a pixel ran out of its budget.
@fragment
fn host_probe(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let i = vec2<i32>(pos.xy * host_u.size.xy / host_u.c.xy);
    let made = host_filtered(i);
    return vec4<f32>(select(0.0, 1.0, host_budget < 0), made.a * 0.0, 0.0, 1.0);
}
