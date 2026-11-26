// Stamps laid by their stroke's rule in a compute pass (see stamp_rule.wgsl), each pixel written
// once into a buffer copied into the stroke after.

// The part of the canvas worked on: its corner, its tiles across, and the rule (1: gradual,
// 2: towards the transfer); then each tile's (first, count) into the stamp list after them.
@group(0) @binding(1) var<storage, read> rule_stamps: array<f32>;
@group(0) @binding(2) var<storage, read> rule_tiles: array<u32>;
// the stroke as it stands before these stamps, and where the part goes, a row of RULE_ROW
// pixels each, as half floats
@group(0) @binding(3) var rule_before: texture_2d<f32>;
@group(0) @binding(4) var<storage, read_write> rule_out: array<vec2<u32>>;

fn rule_float(b: u32) -> f32 {
    return rule_stamps[b];
}

fn rule_word(i: u32) -> u32 {
    return rule_tiles[i];
}

@compute @workgroup_size(16, 16)
fn cs_stamp_rule(@builtin(workgroup_id) tile: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>) {
    let corner = vec2<i32>(bitcast<i32>(rule_tiles[0]), bitcast<i32>(rule_tiles[1]));
    let at = tile.xy * RULE_TILE + local.xy;
    let px = corner + vec2<i32>(at);
    let size = vec2<i32>(textureDimensions(rule_before));
    if (px.x >= size.x || px.y >= size.y) {
        return;
    }
    // rounded to half floats here, as they are kept: packing leaves the rounding to the GPU's API,
    // and Direct3D rounds otherwise than Vulkan, which a stroke built up frame by frame carried on
    // until a sharp edge or a grain cut it into specks
    let paint = halves(rule_laid(textureLoad(rule_before, px, 0), px, at));
    let o = at.y * RULE_ROW + at.x;
    rule_out[o] = vec2<u32>(pack2x16float(paint.rg), pack2x16float(paint.ba));
}

// A dual brush's part laid by its rule (its stroke or its second), carried from lot to lot: its
// paint kept beside its half floats in full floats (each channel's bits), so that the next stamps
// go on from the sums the last left, as one lot laid with them would, and from those kept in half
// floats only where a lot of the whole stroke laid again would begin. Laid so, the stroke's end
// lays only what follows what was laid and comes out bit for bit as the whole stroke laid again.
@group(0) @binding(5) var rule_carried: texture_2d<u32>;

// where a part's full floats go in `rule_out`, after its half floats: two words of it a pixel,
// RULE_ROW pixels a row
const RULE_CARRY_AT: u32 = RULE_ROW * RULE_ROW;

// The list ends (after its last tile's stamps) with a bit for each tile, 32 a word: set where the
// part's full floats aren't its own in this lot (none laid there since it was cleared, or since the
// lot began): its paint is taken from its half floats there.
@compute @workgroup_size(16, 16)
fn cs_stamp_rule_carried(@builtin(workgroup_id) tile: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>) {
    let corner = vec2<i32>(bitcast<i32>(rule_tiles[0]), bitcast<i32>(rule_tiles[1]));
    let at = tile.xy * RULE_TILE + local.xy;
    let px = corner + vec2<i32>(at);
    let size = vec2<i32>(textureDimensions(rule_before));
    if (px.x >= size.x || px.y >= size.y) {
        return;
    }
    // (the first tile's stamps begin right after the table: its length tells the tiles)
    let tiles = (rule_tiles[4] - 4u) / 2u;
    let last = 4u + 2u * (tiles - 1u);
    let end = rule_tiles[last] + rule_tiles[last + 1u];
    let t = tile.y * rule_tiles[2] + tile.x;
    let fresh = ((rule_tiles[end + t / 32u] >> (t % 32u)) & 1u) != 0u;
    let shown = textureLoad(rule_before, px, 0);
    let kept = bitcast<vec4<f32>>(textureLoad(rule_carried, px, 0));
    // (and as the half floats show it wherever something else drew there since; a GPU that keeps
    // no half floats below their normal range, as an Adreno, shows those as 0)
    let halved_kept = halves(kept);
    let flushed = (abs(halved_kept) < vec4<f32>(6.1035156e-5)) & (shown == vec4<f32>(0.0));
    let start = select(shown, kept, !fresh && all((halved_kept == shown) | flushed));
    let paint = rule_laid(start, px, at);
    let halved = halves(paint);
    let o = at.y * RULE_ROW + at.x;
    rule_out[o] = vec2<u32>(pack2x16float(halved.rg), pack2x16float(halved.ba));
    let bits = bitcast<vec4<u32>>(paint);
    rule_out[RULE_CARRY_AT + 2u * o] = bits.xy;
    rule_out[RULE_CARRY_AT + 2u * o + 1u] = bits.zw;
}
