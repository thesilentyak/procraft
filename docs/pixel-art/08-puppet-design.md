# Pixel art in ProCraft: puppets (design)

How the [puppet proposal](07-puppet-proposal.md) is built: a character split into parts (or drawn
in parts), joined at its joints, posed at a few moments, and baked into ordinary frames. The
proposal's principles hold: new files with thin hooks, mobile first, the steps done for the user,
every guess changeable afterwards.

Status: a draft for the user to agree (2026-11-27). Written to go to ProCraft, except the PC's help
(Part G), which is this fork's.

## A. What's kept, and where

### A1. The puppet in the artwork

- A puppet is a **layer group** (the **puppet group**) and an entry in the document's new
  `puppets` list (`Document::puppets`, saved in the artwork's JSON like `animation`; a file
  without it has none). The group is **hidden** while not being worked on, so it isn't a frame
  (Animation Assist's frames are the visible top-level layers) and doesn't show in the picture.
- Its **parts** are the group's members: a paint layer is a part; a group inside it is a **swap
  part**, each of its layers an alternative drawing. The parts' order in the group is their
  drawing order.
- New engine module `crates/engine/src/puppet.rs`:

```rust
pub struct Puppet {
    pub group: LayerId,
    pub parts: Vec<Part>,          // one per member of the group, in no particular order
    pub moves: Vec<Move>,
    pub pixel_steps: bool,         // whole pixels and RotSprite (set from the canvas when made)
}
pub struct Part {
    pub layer: LayerId,            // the part's layer, or its swap group
    pub kind: PartKind,            // Head, Body, Arm, Hand, Leg, Foot, Tail, Other (its name)
    pub joint: [f32; 2],           // the point it turns about, in canvas pixels, at rest
    pub parent: Option<LayerId>,   // the part it hangs from; none for the root
}
pub struct Move {
    pub name: String,              // also the baked animation's name
    pub length: u32,               // frames, 2 to 64
    pub keys: Vec<Key>,            // by frame, at most one a frame
}
pub struct Key {
    pub frame: u32,
    pub poses: Vec<PartPose>,      // a pose for every part
}
pub struct PartPose {
    pub layer: LayerId,
    pub turn: f32,                 // degrees, about the joint, relative to its parent
    pub shift: [f32; 2],           // canvas pixels, relative to its parent
    pub drawing: Option<LayerId>,  // a swap part's drawing showing
}
```

- **Repairs on reading** (as `Animation::repaired`): parts whose layer is gone are dropped (their
  children hang from the dropped part's parent); a group member without a part gets one
  (kind Other, joint its centre, parent guessed); a key's missing poses are made at rest; a
  puppet whose group is gone is dropped.
- Layers made, deleted or moved in and out of the group by the layers panel are picked up by
  the same repair, so the panel stays the place to reorder or remove parts.

### A2. Names

`names.md` gets: **Puppet**, **Puppet group**, **Part**, **Swap part**, **Joint**, **Root**,
**Move**, **Key**, **Puppet mode** with its three steps **Parts**, **Joints**, **Pose**, **Fill
behind**, **Bake**.

## B. Making a puppet

- The **layer options menu** (`LayersPanel.kt`, `desktop/src/layers.rs`) gets **Make a Puppet**,
  for a paint layer or a group (not a mask, a text layer, or a group already a puppet). A puppet
  group's menu has **Open Puppet** in its place.
- **From a paint layer** (a finished character): a new group named "Puppet" takes the layer's
  place, the layer inside it renamed "body", and puppet mode opens at **Parts**.
- **From a group** (drawn in parts): it becomes the puppet group as it is, its members parts,
  joints and parents guessed (C1), and puppet mode opens at **Joints** to check them.
- One undo step each way: `Make a Puppet` undoes the group, the entry and the mode together.

## C. Puppet mode

A mode over the canvas, like the transform tool's: the side bar and the top bar's tools stay, the
frame strip makes way for the **puppet bar** along the bottom. At the bar's left, the three steps
as segments: **Parts · Joints · Pose**, any of them chosen at any time; at its right, **Done**,
which hides the group again and leaves the mode. The canvas shows the puppet alone: the artwork's
solo is set to the puppet group while the mode is open (and given back after), so other frames
and layers don't show.

### C1. Parts

The picture at rest; the drawing tools work on the current part as anywhere else (the group made
visible for the step).

- **Pick**: the bar's first button, on by default in this step. A tap selects the part under the
  finger (D1), joined to what's selected (more taps add; the selection tools' Subtract and the
  Freehand lasso tidy it).
- **Make a part**: what's selected moves (cut) from the layer it's on to a new layer inside the
  puppet just above it, and a small sheet asks what it is: chips **Head, Body, Arm, Hand, Leg,
  Foot, Tail, Other**, a part named e.g. "arm 2" if there's an "arm" already. Then the gap it left
  is shown (D2) and **Fill behind** is offered.
- **Fill behind**: fills the gap the last part left in the layer under it (D2). With the
  generation feature and a server, a second button **Fill with AI…** opens the Generate popover
  with the gap selected, as Generate's Fill on a selection does now (G2).
- **Add a drawing** (a part's menu, from its row in the bar's parts list or a long press on it):
  makes the part a swap part, its layer copied as a second drawing to change (a fist from a hand,
  shut eyes from open ones). More drawings the same way.
- The bar's **parts list** (a chip per part, in drawing order, tap to make it current) shows
  what's been made; the layers panel shows the same layers.

### C2. Joints

The picture at rest with a dot at each joint and a thin line from each part's joint to its
parent's; the current part's outline lit.

- **Guessed** when parts are made or the step opens with parts that have none (C3), never
  changed by later guesses once the user has moved them.
- **Drag a dot** to move the joint (snapped to pixel corners and middles on pixel art).
- **Tap a part** to make it current; the bar shows its name, kind and **Hangs from**: a chip row
  of the other parts (and **Nothing** for the root). One part is the root; making another the root
  hangs the old root from it.

### C3. The guesses

- **Root**: the part with the most pixels (or the kind Body, if one is).
- **Parent**: for each other part, the part it touches most along its edge (pixels within 2 of
  each other), preferring parts nearer the root; ties to the root. No loops: parts are hung in
  order of their distance (in touches) from the root.
- **Joint**: the middle of where it touches its parent; with nothing touching, the point of the
  part nearest the parent.
- Hands and feet are usually small and at the ends; the guesses don't use kinds beyond the root,
  so a creature's tail or a balloon works as well as an arm.

### C4. Pose

The posed puppet, drawn live (E2), its joints as dots; above the bar, the **move's timeline**.

- **Moves**: a chip per move at the bar's left (the first, "idle", made with the puppet); **+**
  adds one (asked for a name, "walk" offered, as animations are named); a move's long press gives
  Rename, Length (2 to 64), Duplicate and Delete.
- **Timeline**: a cell per frame, keys as dots, the frame shown lit; tap to show a frame, drag
  along it to scrub. The move's frames play at the artwork's speed (Animation Assist's, 10 a second
  on pixel art).
- **Posing**: a drag on a part turns it about its joint (the angle between where the drag started
  and where it is, seen from the joint); a drag on the **root** moves the whole puppet; a drag on
  a part's **dot** shifts that part (a nudge, kept as `shift`; whole pixels on pixel art). A swap
  part's tap shows its drawings as chips; picking one sets it. Each change goes to the key at the
  frame shown, made there (from the pose shown) if there's none. Two fingers still move and zoom
  the canvas.
- **Keys**: a long press on a key's dot gives Copy pose, Paste pose, Delete; **Paste** at a frame
  without a key makes one. A new move starts with one key at frame 0, the rest pose.
- **Play** (▶ on the bar) loops the move live; a tap anywhere stops it.
- **Bake** (F): the move into frames.
- **Undo**: every posing drag, key change, joint drag, move edit and part made is one undo step
  (the document's own, `self.change`), so the usual two-finger tap and the undo button work.

## D. Splitting a picture

### D1. Picking a part (engine: `puppet::pick`)

- **Pixel art**: the sprite's **outline colours** are those making up most of its silhouette's
  edge (pixels beside transparency): usually the one dark outline colour, sometimes two. From the
  tapped pixel (or the nearest pixel not of an outline colour, within 3 pixels) a flood spreads
  through pixels not of an outline colour (4 neighbours); then the outline pixels touching what it
  reached (8 neighbours) are added. So a tap inside an outlined arm takes the arm and its outline.
  Where an arm lies over the body with no outline between them, the flood runs into the body: the
  lasso's Subtract cuts it back.
- **Other pictures**: Automatic Selection at the tap (`ColourFlood`, threshold 20%), or the PC's
  segmenting when there is one (G1).

### D2. Filling behind (engine: `puppet::gap`, `puppet::fill`)

- **The gap** is what the part took out of the layer under it that the rest of that layer would
  have covered: the cut area, within the rest's silhouette once that's closed (a dilation then an
  erosion by 3 pixels on pixel art, by 1% of the picture's side otherwise) with its holes filled.
  An arm hanging beside the body leaves no gap; an arm across the chest leaves one.
- **Fill behind** gives each gap pixel the colour of the nearest pixel of the rest that isn't an
  outline colour (outline colours only where there's no other within 6 pixels), nearest first, so
  the body's own shading runs into the gap. On pixel art it uses the sprite's colours only, so
  nothing new comes in. One undo step. It's rough on purpose: the hidden part only shows when the
  part turns, and the user can touch it up or use the AI.

## E. Drawing a pose (engine: `puppet::pose_at`, `puppet::draw`)

### E1. The pose at a frame

- Between the keys before and after the frame (the last key leading round to the first, as a move
  loops; one key holds for the whole move): each part's `turn` and `shift` eased (smoothstep), its
  turn the short way round; a swap part's drawing the earlier key's.
- Each part's **place** is its parent's place, then a turn of `turn` about its rest joint, then
  `shift`: a 2 × 3 matrix chained from the root. On pixel art each part's place is rounded so its
  joint lands on a whole pixel.

### E2. Drawing

- Each part's pixels (its layer, or its swap drawing's) read once when the mode opens or the part
  changes (`read_layer`), kept on the CPU (`puppet::Parts`); drawn into one RGBA picture the size
  of the puppet's reach (the union of every part's posed box), back to front in drawing order.
- **Not turned**: copied as they are, moved by whole pixels.
- **Turned, pixel art**: **RotSprite**, our own code from its published description: the part
  scaled up ×8 by three passes of Scale2x (EPX), turned with nearest sampling about its joint, then
  brought down ×8 by taking each 8 × 8 block's middle. Scale2x and nearest sampling make no new
  colours, so a turned part keeps the sprite's colours exactly. Cached per part and angle (to 0.5°).
- **Turned, other art**: bilinear sampling, premultiplied.
- **Live**: the picture goes to the canvas as its **overlay** (`set_overlay`, as a generated
  result's preview), in canvas pixels at the reach's place; the puppet group stays hidden under
  it. Redrawn on each posing drag event and each played frame; for a 64-pixel sprite that's well
  under a millisecond, for a 1000-pixel part a few milliseconds.

## F. Baking (core: `bake_move`)

- Every frame of the move drawn (E2), within the canvas (what reaches past it is cut off), stacked
  and handed to `add_animation_frames` (the code AI animations and sprite sheet imports use): the
  frames go after the animation holding the current frame, Animation Assist turned on if it's
  off, the layers named "walk 1", "walk 2"…, the animation named after the move.
- **Baking again** replaces: if the artwork has an animation of the move's name, its frames are
  removed in the same step and the new ones put where they were. `add_animation_frames` gains a
  `replacing: Option<&str>` for this.
- One undo step. The notice says "Baked 8 frames of walk". Puppet mode stays open, so posing,
  baking and touching up a frame can go round.

## G. The PC's help (this fork, with the generation feature)

### G1. Segmenting

- When the server has `Sam2Segmentation` (kijai's `ComfyUI-segment-anything-2`, whose loader
  downloads its model by itself), a tap on a picture that isn't pixel art sends a small graph
  (`crates/diffusion/src/segment.rs`, as the depth tracer's): the picture (the layer, at most 1024
  on its longest side), `DownloadAndLoadSAM2Model`, and the tap as `coordinates_positive`
  (`[{"x":…,"y":…}]`); the mask comes back and is joined to the selection as a pick is. "Finding
  the part…" shows meanwhile. Without the node, or with no server, D1's way is used.
- The companion and `dev/starters` README name the node pack; nothing is installed for the user
  without asking.
- Pixel art keeps D1's way even with the node: outlines are a surer guide on small sprites.

### G2. Fill with AI

- **Fill with AI…** selects the gap (grown by 2 pixels), and opens the Generate popover on the
  layer under the part, as Generate's Fill on a selection does today, the prompt left as the user
  had it. On a pixel art canvas the usual snapping to its colours applies to the result.
- No new workflow: the user's current style or workflow does the fill.

## H. Milestones

- **P1, engine** (`puppet.rs`, with tests): the data and its repairs; the guesses; `pose_at`;
  `draw` with RotSprite and bilinear; `pick`; `gap` and `fill`.
- **P2, core** (`puppet.rs` in `crates/core`, exported): make, open and close (solo, overlay),
  pick, make a part, fill, joints, moves, keys, posing, play, bake with replacing; `Document`'s
  `puppets` field (the one hook into `document.rs`).
- **P3, Android**: Make a Puppet / Open Puppet in the layer menu; the puppet bar and its steps
  (`android/.../ui/PuppetUi.kt`), drags on the canvas routed to the puppet while the mode is open.
- **P4, desktop**: the same (`desktop/src/puppet.rs`).
- **P5, the PC's help**: segmenting and Fill with AI.

## I. Checks

- Engine: a puppet's repairs (a deleted part, a new layer in the group); the guesses on a drawn
  figure (body root, arms and legs hung from it, joints where they touch); `pose_at` easing,
  short way round, looping, swap holding; RotSprite turning a 1-pixel outline line keeps it one
  pixel thick and adds no colours; `pick` takes an outlined arm whole; `gap` empty for an arm beside
  the body, filled for one across it; `fill` uses only the sprite's colours.
- Core: make from a layer and from a group (one undo each); bake adds a named animation; baking
  again replaces it in place; a move that turns an arm 90° bakes the arm turned.
- On the tablet and the desktop: the turtle sheet's turtle split into shell, head and four legs,
  a walk of four keys baked to 8 frames and exported as a sprite sheet.

## J. Decisions

| Date | Decision |
|---|---|
| 2026-11-27 | Flat 2D; baked frames; joined parts; swaps in the first version; ready-made motions later (brainstorm, 06) |
| 2026-11-27 | Parts can come from an existing picture: tap to pick, Make a part, Fill behind (proposal, 07) |
| 2026-11-27 | The puppet is a hidden group plus a `puppets` entry in the document; the layers panel stays the place to reorder and remove parts |
| 2026-11-27 | Live posing draws on the CPU into the canvas overlay; RotSprite on pixel art, bilinear otherwise |
| 2026-11-27 | Posing makes or changes the key at the frame shown; baking again replaces the move's frames |
