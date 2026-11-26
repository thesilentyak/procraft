// Smudge's passes as fragment passes (see smudge.wgsl), on a GPU without compute passes: the
// window is a texture of the layer's part (8 bits a channel), and what the brush carries, how
// much of the paint each pixel has taken and shows, and wet paint's plots are integer textures
// holding each float's bits. Each pass draws a fragment per cell, plot texel or pixel into another
// texture than it reads (copied back after), or (the plots) into one it doesn't read.

@group(0) @binding(1) var work_tex: texture_2d<f32>;
@group(0) @binding(3) var grained_tex: texture_2d<f32>;
@group(0) @binding(4) var amount_tex: texture_2d<u32>;
@group(0) @binding(10) var shown_tex: texture_2d<u32>;
// what the brush carries (its three grids, as the compute pass's buffer lays them, a grid's row a
// texture row), or wet paint's plot the pass carries over from
@group(2) @binding(0) var carry_tex: texture_2d<u32>;
// wet paint's plots for the pass (PLOTS_ROW texels a row), or a dulling stamp's parts' sums
@group(2) @binding(1) var plots_tex: texture_2d<u32>;

// texels a row of the pass's plots, and of a carried-over plot
const PLOTS_ROW: u32 = 1024u;
const CARRIED_ROW: u32 = 128u;

// where canvas pixel `p` is in the window's textures
fn window_texel(p: vec2<i32>) -> vec2<i32> {
    return p - vec2<i32>(st.origin_x, st.origin_y);
}

// Whether texel `t` is one of a texture `size` texels across: a load past a texture gives nothing
// (zero), as Vulkan's and the others' do; OpenGL ES's (in wgpu) aren't checked, and one there reads
// whatever memory lies past the texture
fn within(t: vec2<i32>, size: vec2<u32>) -> bool {
    return all(t >= vec2<i32>(0)) && all(t < vec2<i32>(size));
}

fn work_at(p: vec2<i32>) -> vec4<f32> {
    let t = window_texel(p);
    if (!within(t, textureDimensions(work_tex))) {
        return vec4<f32>(0.0);
    }
    return textureLoad(work_tex, t, 0);
}

fn grained_at(p: vec2<i32>) -> vec4<f32> {
    let t = window_texel(p);
    if (!within(t, textureDimensions(grained_tex))) {
        return vec4<f32>(0.0);
    }
    return textureLoad(grained_tex, t, 0);
}

fn amount_at(p: vec2<i32>) -> f32 {
    let t = window_texel(p);
    if (!within(t, textureDimensions(amount_tex))) {
        return 0.0;
    }
    return bitcast<f32>(textureLoad(amount_tex, t, 0).r);
}

fn shown_at(p: vec2<i32>) -> f32 {
    let t = window_texel(p);
    if (!within(t, textureDimensions(shown_tex))) {
        return 0.0;
    }
    return bitcast<f32>(textureLoad(shown_tex, t, 0).r);
}

// where the `k`th texel is in a texture `width` texels a row
fn linear_texel(k: u32, width: u32) -> vec2<i32> {
    return vec2<i32>(i32(k % width), i32(k / width));
}

fn carry_at(k: i32) -> vec4<f32> {
    return bitcast<vec4<f32>>(textureLoad(carry_tex, linear_texel(u32(k), textureDimensions(carry_tex).x), 0));
}

fn plot_texel(plots: u32, k: u32) -> vec4<f32> {
    if (plots == CARRIED_PLOT) {
        return bitcast<vec4<f32>>(textureLoad(carry_tex, linear_texel(k, CARRIED_ROW), 0));
    }
    return bitcast<vec4<f32>>(textureLoad(plots_tex, linear_texel(k, PLOTS_ROW), 0));
}

// `c` as the nearest 8-bit value each channel (as pack4x8unorm rounds it)
fn to_eight(c: vec4<f32>) -> vec4<f32> {
    return floor(0.5 + 255.0 * clamp(c, vec4<f32>(0.0), vec4<f32>(1.0))) / 255.0;
}

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

// the window's cell at a fragment of a texture laid out as the carry's first grid
fn fragment_cell(pos: vec4<f32>) -> vec2<u32> {
    return vec2<u32>(vec2<i32>(pos.xy) - vec2<i32>(middle() - st.half));
}

// whether cell or texel `i` is one of the dispatch's part's (and of the `n` a side), from its start
fn from_part(i: vec2<u32>, n: u32) -> bool {
    return all(i >= part_at(vec2<u32>(0u))) && in_part(i, n);
}

@fragment
fn fs_carry(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<u32> {
    let i = fragment_cell(pos);
    if (!from_part(i, window_side())) {
        discard;
    }
    return bitcast<vec4<u32>>(kept_paint(carry_cell(i), vec4<f32>(0.0)));
}

// a dulling stamp's parts' sums: part i's at texel 2i, its weight at 2i + 1
@fragment
fn fs_dull_part(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<u32> {
    let x = u32(pos.x);
    let part = dull_part(x / 2u);
    if (x % 2u == 0u) {
        return bitcast<vec4<u32>>(part.sum);
    }
    return vec4<u32>(bitcast<u32>(part.weight), 0u, 0u, 0u);
}

@fragment
fn fs_dull(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<u32> {
    var total = vec4<f32>(0.0);
    var weighed = 0.0;
    for (var k = 0u; k < DULL_PARTS; k = k + 1u) {
        total = total + bitcast<vec4<f32>>(textureLoad(plots_tex, vec2<i32>(i32(2u * k), 0), 0));
        weighed = weighed + bitcast<f32>(textureLoad(plots_tex, vec2<i32>(i32(2u * k + 1u), 0), 0).r);
    }
    return bitcast<vec4<u32>>(kept_paint(dull_colour(total, weighed), vec4<f32>(0.0)));
}

@fragment
fn fs_blur_down(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<u32> {
    let id = fragment_cell(pos);
    let n = window_side();
    if (id.x >= n || id.y >= n) {
        discard;
    }
    return bitcast<vec4<u32>>(kept_paint(blur_down_cell(id), vec4<f32>(0.0)));
}

@fragment
fn fs_blur_across(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<u32> {
    let id = fragment_cell(pos);
    let n = window_side();
    if (id.x >= n || id.y >= n) {
        discard;
    }
    return bitcast<vec4<u32>>(kept_paint(blur_across_cell(id), vec4<f32>(0.0)));
}

// a fragment of the window's textures: its canvas pixel
fn fragment_pixel(pos: vec4<f32>) -> vec2<i32> {
    return vec2<i32>(pos.xy) + vec2<i32>(st.origin_x, st.origin_y);
}

// the pixel as a stamp leaves it (the layer's, or what the stroke keeps as it was laid), and how
// much of the paint it has taken and shows
struct Deposited {
    @location(0) pixel: vec4<f32>,
    @location(1) amount: u32,
    @location(2) shown: u32,
}

@fragment
fn fs_deposit(@builtin(position) pos: vec4<f32>) -> Deposited {
    let p = fragment_pixel(pos);
    let d = deposit(own_dab(), p, found_at(p));
    var o: Deposited;
    o.pixel = select(work_at(p), grained_at(p), d.into_grained);
    o.amount = bitcast<u32>(amount_at(p));
    o.shown = bitcast<u32>(shown_at(p));
    if (!d.laid) {
        return o;
    }
    o.pixel = to_eight(d.out);
    if (d.amount_set) {
        o.amount = bitcast<u32>(d.amount);
    }
    if (d.shown_set) {
        o.shown = bitcast<u32>(d.shown);
    }
    return o;
}

// k/255 for a byte k, the nearest float to it, as a texture of 8 bits a channel reads byte k:
// worked out on its bits, so that no compiler's division rounds it otherwise. k/255 is k's 8 bits
// repeated past the point: its first 24 bits from the leading one, rounded by the next (never a
// tie, as k's bits go on).
fn byte_value(k: u32) -> f32 {
    if (k == 0u) {
        return 0.0;
    }
    if (k >= 255u) {
        return 1.0;
    }
    let bits = k * 0x01010101u;
    let lead = countLeadingZeros(bits);
    let s = bits << lead;
    let m = (s >> 8u) + ((s >> 7u) & 1u);
    return ldexp(f32(m), -24 - i32(lead));
}

// `c` as a texture of 8 bits a channel holds it once drawn with to_eight(c), read back
fn kept_eight(c: vec4<f32>) -> vec4<f32> {
    let k = vec4<u32>(floor(0.5 + 255.0 * clamp(c, vec4<f32>(0.0), vec4<f32>(1.0))));
    return vec4<f32>(byte_value(k.x), byte_value(k.y), byte_value(k.z), byte_value(k.w));
}

// A run of a pass's stamps (`st.run`, as `wet_pass` lists them) laid at once within all their
// bounds (as cs_deposit_run lays them): each fragment goes through those over it in order, as its
// paint stands after each (in 8 bits a channel, as the window's texture keeps it), drawn once.
@fragment
fn fs_deposit_run(@builtin(position) pos: vec4<f32>) -> Deposited {
    let p = fragment_pixel(pos);
    var found = found_at(p);
    var o: Deposited;
    o.pixel = select(found.work, found.grained, st.laid_apart == 2u);
    o.amount = bitcast<u32>(amount_at(p));
    o.shown = bitcast<u32>(shown_at(p));
    for (var k = st.run.x; k < st.run.y; k++) {
        let bbox = wet_pass.stamps[k].bbox;
        if (any(p < bbox.xy) || any(p >= bbox.zw)) {
            continue;
        }
        let d = deposit(pass_dab(k), p, found);
        if (!d.laid) {
            continue;
        }
        if (d.amount_set) {
            found.amount = d.amount;
            o.amount = bitcast<u32>(d.amount);
        }
        if (d.shown_set) {
            found.shown = d.shown;
            o.shown = bitcast<u32>(d.shown);
        }
        o.pixel = to_eight(d.out);
        if (d.into_grained) {
            found.grained = kept_eight(d.out);
        } else {
            found.work = kept_eight(d.out);
        }
    }
    return o;
}

@fragment
fn fs_synthesize(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = fragment_pixel(pos);
    if (!in_bbox(own_dab(), p) || left_out(p)) {
        return work_at(p);
    }
    return to_eight(synthesized(p));
}

// a pass's plots: plot k's texel i at k n n + i.y n + i.x (as the compute pass lays them), each
// fragment walking the pass's stamps up to its own
@fragment
fn fs_wet_plots(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<u32> {
    let n = st.plot;
    let l = u32(pos.y) * PLOTS_ROW + u32(pos.x);
    let k = l / (n * n);
    let r = l % (n * n);
    let i = vec2<u32>(r % n, r / n);
    if (k >= st.count || !from_part(i, n)) {
        discard;
    }
    let t = plot_t(vec2<f32>(i), n);
    var walk = wet_walk_start(t);
    for (var j = 0u; j <= k; j++) {
        walk = wet_walk_on(walk, j, t);
    }
    return bitcast<vec4<u32>>(kept_paint(from_mix(walk.out), vec4<f32>(0.0)));
}

// the plot the next pass carries over from: the last stamp's, in the mix's space
@fragment
fn fs_wet_carried(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<u32> {
    let n = st.plot;
    let l = u32(pos.y) * CARRIED_ROW + u32(pos.x);
    let i = vec2<u32>(l % n, l / n);
    if (l >= n * n || !from_part(i, n)) {
        discard;
    }
    let t = plot_t(vec2<f32>(i), n);
    var walk = wet_walk_start(t);
    for (var j = 0u; j < st.count; j++) {
        walk = wet_walk_on(walk, j, t);
    }
    return bitcast<vec4<u32>>(kept_paint(walk.out, vec4<f32>(0.0)));
}
