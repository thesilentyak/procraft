# Pixel art in ProCraft: puppets (proposal)

Cutout (puppet) animation in ProCraft, from the [brainstorm](06-puppet-brainstorm.md): a character
drawn once in parts, joined at its joints, posed at a few moments, and baked into ordinary frames.
For pixel art first, and for ordinary 2D art too. The user is new to animation: choices are made
for him, the rare settings folded away.

Status: final (agreed 2026-11-27). The design: [08-puppet-design.md](08-puppet-design.md).

## What the user asked

- Rig 2D art and pixel art for animation in the app, natively: puppet animation.
- Flat 2D (agreed after looking at Smack Studio's 3D skeleton).
- From the brainstorm, agreed as recommended: baked frames come out (the rig for Godot later);
  parts are joined (the hand follows the arm); swaps (alternative drawings) are in the first
  version; ready-made motions are a second step.
- Parts must come from **an existing picture** too, not only drawn on layers one by one: much of
  the user's art comes from AI or imports (2026-11-27). Agreed: tap a part to pick it out, with
  Fill behind (below).

## What ProCraft has already

Layers and groups for the parts; the transform tool, crisp on pixel art; Animation Assist's frame
strip, onion skins, holds and playing; named animations; Sprite Sheet export; and the code that
adds frames as a named animation (AI animations and sprite sheet imports use it). Puppets make
frames for all of that rather than bringing a second timeline.

## Splitting a picture into parts

- **Make a Puppet** on a single layer (a finished character) opens puppet mode at its first step,
  **Parts**: the picture alone, and a bar with Make a part, Fill behind and Done.
- **Tap a part to pick it out**: a tap selects the part under the finger. On pixel art the app
  follows the outlines and colours there (clear edges make this work well). On other pictures, and
  when the PC's ComfyUI has a segmenting model (Segment Anything, through a node the starters
  name), the PC finds the whole part from the tap; without it, the outline-and-colour way is used
  everywhere. More taps add to the pick; the selection brushes tidy its edge.
- **Make a part** moves what's picked onto a layer of its own inside the puppet, named from a
  short list the app offers (head, body, arm, hand, leg, foot, tail, other).
- **The hole behind**: a part cut out leaves a gap in what was under it, which shows as soon as the
  part turns. **Fill behind** fills it: small gaps by spreading the colours round them, on the
  device; bigger ones by the PC's AI (an inpaint starter workflow), snapped to the sprite's own
  colours on pixel art. Each fill can be undone or touched up by hand.
- **Done** goes on to joints (guessed as below) and posing. Splitting can be opened again later
  to cut out another part.
- The AI help (segmenting, filling) is this fork's, with the generation feature; the rest goes to
  ProCraft with the puppets.

## The puppet

- **Making one from layers**: a layer group's menu gets **Make a Puppet** too. Each layer in the
  group becomes a **part**; a group inside it becomes a **swap part** (its layers are the part's alternative
  drawings, one showing at a time). The puppet group is hidden from the frame strip (hidden groups
  aren't frames) and stays in the artwork, ready to pose again.
- **Joints, guessed for you**: each part's joint (the point it turns about) and parent (the part
  it hangs from) are guessed when the puppet is made: the parent is the part it overlaps most
  towards the middle of the character, the joint the middle of that overlap. Dragging a joint's dot
  moves it; **Hangs from…** in a part's menu changes its parent. One part is the **root** (the
  biggest, usually the body): moving it moves the whole puppet.
- **Changing the drawing** at any time: the parts are ordinary layers, drawn on with any tool.
  Poses keep working on the new drawing; baking again uses it.

## Puppet mode

Opened by tapping the puppet group's **Pose** (or the puppet's name in the frame strip's menu). The
canvas shows the puppet alone with its joints as small dots; the usual tools stay for touching up
a part. A bar along the bottom holds:

- **Moves**: a puppet has several moves (idle, walk, attack), each its own keys. The first is made
  for you; **+** adds one, named as animations are.
- **The move's timeline**: its length in frames (8 to start; 10 frames a second on pixel art) and
  its keys as marks. Tapping a moment shows the pose there.
- **Posing makes keys**: a tap picks a part, a drag turns it about its joint (the root moves). A
  change at a moment without a key makes one there; at a key, it changes that key. Swap parts show
  their drawings to pick from. Long-pressing a key gives Copy, Paste and Delete.
- **Play** loops the move live, the in-betweens worked out as it plays.
- **Bake** makes the move's frames: added as a named animation (the move's name) after the current
  animation, Animation Assist turned on if it's off. Baking a move again replaces its frames, so
  posing, baking and touching up can go round as often as needed. One undo takes a bake away.

## In-betweens

- **Smooth** (eased in and out) between keys, each part turning the short way round. A move loops:
  the last key leads back to the first.
- **Swaps** don't blend: a swap part shows its drawing from the key before.
- **Whole pixels** on a pixel art canvas: each part's place is rounded to whole pixels, so nothing
  shimmers.

## Pixel art

- **Turning a part** uses the published RotSprite method, written for ProCraft: the part is scaled
  up ×8 with an edge-keeping pixel scaler, turned, and brought back down, so outlines stay clean
  lines instead of breaking up. Turned parts are brought to the sprite's own colours.
- Parts that only move or swap aren't changed at all: they bake pixel for pixel.

## Not in it

Bending parts (meshes and weights, as Live2D); depth (Smack Studio's 3D turning); the rig for Godot
(its Skeleton2D and AnimationPlayer); ready-made motions (Idle, Walk, Wave: the second step, once
parts can be named as a head or a leg); dragging a hand to pull the arm after it (inverse
kinematics); splitting a whole picture into parts in one go (body-part models suit realistic
people, not small sprites or creatures). Later idea: asking Gemini for a character as a parts sheet,
the hidden areas already drawn, gathered into a puppet as sprite sheets are.

## Decided for the user (he can change any of them)

- A new move is 8 frames long, loops, and eases smoothly.
- Joints and parents are guessed when the puppet is made.
- A tap picks a part by its outline and colours, or by the PC's segmenting model when there is one.
- Posing changes or makes the key at the moment shown (no separate "add key" step).
- Baking a move again replaces its earlier frames.
- On pixel art, places snap to whole pixels and turned parts keep the sprite's colours.
