// Smudge's passes as compute passes (see smudge.wgsl): the window is a buffer of packed
// premultiplied RGBA8 pixels, read and written in place, and what the brush carries a buffer of
// its own.

@group(0) @binding(1) var<storage, read_write> work: array<u32>;
@group(0) @binding(2) var<storage, read_write> carry: array<vec4<f32>>;
// wet paint laid apart: the colour its stamps mixed at each pixel, at full strength (see `as_laid`)
@group(0) @binding(3) var<storage, read_write> grained: array<u32>;
// wet paint in the modulated mode: how much each pixel has taken so far; and (its second half) how
// much of the paint laid apart shows
@group(0) @binding(4) var<storage, read_write> amount: array<f32>;

// where canvas pixel `p` is in the window's buffers
fn window_index(p: vec2<i32>) -> u32 {
    return u32(p.y - st.origin_y) * st.stride + u32(p.x - st.origin_x);
}

// A read past the end of a buffer gives nothing (zero), as Vulkan's and the others' do: OpenGL
// ES's (in wgpu) aren't checked, and one there reads whatever memory lies past the buffer, or
// faults.
fn work_at(p: vec2<i32>) -> vec4<f32> {
    let i = window_index(p);
    if (i >= arrayLength(&work)) {
        return vec4<f32>(0.0);
    }
    return unpack4x8unorm(work[i]);
}

fn grained_at(p: vec2<i32>) -> vec4<f32> {
    let i = window_index(p);
    if (i >= arrayLength(&grained)) {
        return vec4<f32>(0.0);
    }
    return unpack4x8unorm(grained[i]);
}

fn amount_word(i: u32) -> f32 {
    if (i >= arrayLength(&amount)) {
        return 0.0;
    }
    return amount[i];
}

fn amount_at(p: vec2<i32>) -> f32 {
    return amount_word(window_index(p));
}

fn shown_at(p: vec2<i32>) -> f32 {
    return amount_word(st.stride * st.rows + window_index(p));
}

fn carry_word(i: u32) -> vec4<f32> {
    if (i >= arrayLength(&carry)) {
        return vec4<f32>(0.0);
    }
    return carry[i];
}

fn carry_at(k: i32) -> vec4<f32> {
    return carry_word(u32(k));
}

fn plot_texel(plots: u32, k: u32) -> vec4<f32> {
    if (plots == CARRIED_PLOT) {
        return carry_word(st.index * CARRIED + k);
    }
    return carry_word(2u * CARRIED + k);
}

@compute @workgroup_size(8, 8)
fn cs_carry(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = part_at(id.xy);
    if (!in_part(i, window_side())) {
        return;
    }
    carry[cell_index(window_cell(i))] = kept_paint(carry_cell(i), vec4<f32>(0.0));
}

var<workgroup> dull_sums: array<vec4<f32>, 64>;
var<workgroup> dull_weights: array<f32, 64>;

@compute @workgroup_size(64)
fn cs_dull(@builtin(local_invocation_index) i: u32) {
    let part = dull_part(i);
    dull_sums[i] = part.sum;
    dull_weights[i] = part.weight;
    workgroupBarrier();
    if (i != 0u) {
        return;
    }
    var total = vec4<f32>(0.0);
    var weighed = 0.0;
    for (var k = 0u; k < DULL_PARTS; k = k + 1u) {
        total = total + dull_sums[k];
        weighed = weighed + dull_weights[k];
    }
    carry[dull_index()] = kept_paint(dull_colour(total, weighed), vec4<f32>(0.0));
}

@compute @workgroup_size(8, 8)
fn cs_blur_down(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = window_side();
    if (id.x >= n || id.y >= n) {
        return;
    }
    let g = i32(st.grid);
    carry[g * g + cell_index(window_cell(id.xy))] = kept_paint(blur_down_cell(id.xy), vec4<f32>(0.0));
}

@compute @workgroup_size(8, 8)
fn cs_blur_across(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = window_side();
    if (id.x >= n || id.y >= n) {
        return;
    }
    let g = i32(st.grid);
    carry[2 * g * g + cell_index(window_cell(id.xy))] = kept_paint(blur_across_cell(id.xy), vec4<f32>(0.0));
}

@compute @workgroup_size(8, 8)
fn cs_deposit(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = st.bbox.xy + vec2<i32>(id.xy);
    let dab = own_dab();
    if (!in_bbox(dab, p)) {
        return;
    }
    let d = deposit(dab, p, found_at(p));
    if (!d.laid) {
        return;
    }
    let k = window_index(p);
    if (d.amount_set) {
        amount[k] = d.amount;
    }
    if (d.shown_set) {
        amount[st.stride * st.rows + k] = d.shown;
    }
    if (d.into_grained) {
        grained[k] = pack4x8unorm(d.out);
        return;
    }
    work[k] = pack4x8unorm(d.out);
}

// A run of a pass's stamps (`st.run`, as `wet_pass` lists them) laid at once over all their bounds
// (`st.bbox`): each pixel goes through those over it in order, as its paint stands after each (in 8
// bits a channel, as the window keeps it), and is written once. It comes out as the stamps laid
// one by one do, each a dispatch of its own.
@compute @workgroup_size(8, 8)
fn cs_deposit_run(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = st.bbox.xy + vec2<i32>(id.xy);
    if (any(p >= st.bbox.zw)) {
        return;
    }
    var found = found_at(p);
    var laid = false;
    var amount_set = false;
    var shown_set = false;
    var word = 0u;
    for (var k = st.run.x; k < st.run.y; k++) {
        let bbox = wet_pass.stamps[k].bbox;
        if (any(p < bbox.xy) || any(p >= bbox.zw)) {
            continue;
        }
        let d = deposit(pass_dab(k), p, found);
        if (!d.laid) {
            continue;
        }
        laid = true;
        if (d.amount_set) {
            found.amount = d.amount;
            amount_set = true;
        }
        if (d.shown_set) {
            found.shown = d.shown;
            shown_set = true;
        }
        // as the window keeps it, and the next stamp finds it
        word = pack4x8unorm(d.out);
        if (d.into_grained) {
            found.grained = unpack4x8unorm(word);
        } else {
            found.work = unpack4x8unorm(word);
        }
    }
    if (!laid) {
        return;
    }
    let i = window_index(p);
    if (amount_set) {
        amount[i] = found.amount;
    }
    if (shown_set) {
        amount[st.stride * st.rows + i] = found.shown;
    }
    if (st.laid_apart == 2u) {
        grained[i] = word;
    } else {
        work[i] = word;
    }
}

@compute @workgroup_size(8, 8)
fn cs_synthesize(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = st.bbox.xy + vec2<i32>(id.xy);
    if (!in_bbox(own_dab(), p) || left_out(p)) {
        return;
    }
    work[window_index(p)] = pack4x8unorm(synthesized(p));
}

@compute @workgroup_size(8, 8)
fn cs_wet_plots(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = st.plot;
    let i = part_at(id.xy);
    if (!in_part(i, n)) {
        return;
    }
    let t = plot_t(vec2<f32>(i), n);
    var walk = wet_walk_start(t);
    for (var k = 0u; k < st.count; k++) {
        walk = wet_walk_on(walk, k, t);
        carry[2u * CARRIED + k * n * n + i.y * n + i.x] = kept_paint(from_mix(walk.out), vec4<f32>(0.0));
    }
    carry[(1u - st.index) * CARRIED + i.y * n + i.x] = kept_paint(walk.out, vec4<f32>(0.0));
}
