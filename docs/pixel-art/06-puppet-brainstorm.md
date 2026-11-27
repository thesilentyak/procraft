# Pixel art in ProCraft: puppets (brainstorm)

Draw a character once, in parts, join the parts at their joints, pose it at a few moments, and
let the app make the frames in between: cutout (puppet) animation, as Spine, DragonBones and
Godot's cutout animation do it, for pixel art and for ordinary 2D art. Agreed 2026-11-27; the
proposal: [07-puppet-proposal.md](07-puppet-proposal.md).

## Agreed so far

- **Flat 2D** (2026-11-27): parts turn only in the screen's plane, like a paper puppet. Smack
  Studio's 3D skeleton (parts turning towards the viewer, its pixels redrawn) was looked at: it's
  patented and a much bigger job, and flat 2D covers side-view games' walks, idles, attacks and
  waves. Depth can be looked at again later.

## The idea

1. **Parts.** The character drawn in pieces, each on its own layer: head, body, upper arm, lower
   arm, hand, legs. ProCraft's own layers and groups, nothing new to learn.
2. **Rig.** Each part gets a **joint** (its pivot: the shoulder, the elbow) and a **parent** (the
   lower arm hangs off the upper arm). Moving or turning a part takes everything hanging off it
   along.
3. **Pose.** A tap picks a part; a drag turns it about its joint (or, on its root, moves the whole
   puppet). A pose at a moment on the timeline is a **key**.
4. **In-betweens.** Between keys the app turns each part smoothly (eased), so two or three keys
   make a whole move. The puppet plays live while posing.
5. **Bake.** The keys become ordinary frames in the frame strip, as a named animation (as AI
   animations and imported sheets come in). Then everything already there works on them:
   touch-ups, onion skins, holds, playing, Sprite Sheet export, GIFs, Godot through the sheet.

## Where it could live

- **The puppet is a layer group** marked as a puppet. Hidden, it isn't a frame (Animation Assist's
  frames are the visible top-level layers), so it can stay in the artwork beside its baked frames,
  ready to pose again.
- **Puppet mode**: opening the puppet shows it alone with its joints, a small bar (Key, Play,
  Bake, the keys on a short timeline) and leaves the drawing tools for touching up a part. A
  part's drawing can be changed at any time; baking again uses the new drawing.
- On the tablet everything is a tap or a drag; the desktop the same with the mouse.

## Pixel art

- **Turning pixel art** normally turns it to mush. The well-known way round it (RotSprite, a
  published method) scales a part up with an edge-keeping pixel scaler, turns it, and brings it
  back down, so outlines stay clean lines. Baked frames are also brought to the sprite's own
  colours, as AI frames are.
- **Shifting and swapping** need no turning at all, and they're how much pixel art moves: a part
  nudged by whole pixels (a bob, a step), or swapped for another drawing (an open hand and a fist,
  eyes open and shut for a blink). Swaps could be a part with alternative drawings (a group of
  them), a key choosing which shows.
- Positions snap to whole pixels on a pixel art canvas, so nothing shimmers.

## Choices (agreed 2026-11-27 as recommended)

1. **What comes out**: (a) baked frames, which work everywhere; or (b) also the rig itself for
   Godot (its Skeleton2D and AnimationPlayer), so the game moves the parts live. Recommended:
   **(a)** first; (b) later if the game needs it.
2. **Joints or loose parts**: joined parts (the hand follows the arm) or loose ones moved one by
   one. Recommended: **joined**, or posing a limb is tedious.
3. **Ready-made motions**: one-tap Idle (a breathing bob), Walk or Wave on a set-up puppet, then
   tweaked. They need the app to know the parts (a head, a leg), so parts would be named from a
   short list. Recommended: **a second step**, after posing by hand works.
4. **Swaps** (alternative drawings for a part): in the first version or later. Recommended: **in
   the first**: a blink and a fist are a big part of pixel art's life, and they're cheap.

## Later, not now

- Bending parts (meshes and weights, as Live2D or Spine's meshes): a different, much bigger tool,
  and it suits pixel art poorly.
- Depth (Smack Studio's 3D turning).
- Splitting a finished drawing into parts for you (an AI picking out the arm): the selection tools
  and Cut and Paste to a new layer do it by hand today.
- Ready-made motions (choice 3), the rig for Godot (choice 1b).
