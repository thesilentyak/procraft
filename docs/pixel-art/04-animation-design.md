# Pixel art in ProCraft, stage 2: animation design

How the [stage 2 proposal](03-animation-proposal.md) is built. The proposal's principles hold:
new files with thin hooks, mobile first, every choice made for the user and changeable.

Status: agreed with the user 2026-11-26 ("let's move forward with building"); part A built
2026-11-26, parts B and C 2026-11-27. Built in three parts:
**A** (AI animation, the user's first want), then **B** (sprite sheets and export), then **C**
(animations by name).

The fork's half (the starter and the results tray) lives in the generation code and never goes to
ProCraft; the export and the animations by name are written to go to ProCraft with stage 1.

## A. AI animation (this fork)

The flow: make a sprite (the RD Flow card, or by hand), pick the **Pixel animation** card, type the
move, Generate. About a minute later the tray holds one result that plays; **Apply** adds its frames
to the frame strip.

### A1. The starter: "ProCraft - Pixel animation (MiniMax H3)"

Made by `dev/starters/make_starters.py` with the others, from the workflow tried on the user's PC
(2026-11-26: a 64 × 64 fox, a usable 8-frame loop in about a minute on a 16 GB card).

- **From the app**: the canvas (its picture and seed), and three parameters: **Prompt** (the move,
  in view), **Frames** (whole number, 2 to 24, default 8) and **Seconds** (1 to 5, default 1.6).
- **The picture**: blown up with hard edges by a whole number, aiming for its longest side at 8
  times the canvas's (so each canvas pixel gets 8 × 8 video pixels), kept between 768 and 1280
  pixels; then centred on white with the sides rounded up to a multiple of 32 (the model's step).
  It's the video's first and last frame, so the move loops. A 64 canvas gets ×12 (768), a 128
  canvas ×8 (1024), a 250 canvas ×5 (1280; about 4 minutes on a 16 GB card, against 1 at 768).
- **The video**: MiniMax H3 first-and-last-frame to video with its 8-step LoRA, at 24 frames a
  second; its length from Seconds as the model's own workflow works it out (17k + 5 frames). The
  prompt is the move wrapped in the words that worked: a pixel art sprite animation, the camera
  still, the character centred at one size, a plain background, crisp blocky pixels.
- **Frames kept**: the sprite's part of each video frame, the repeated last frame left out, every
  *n*th kept so about Frames remain.
- **Back to pixels**: each frame through SpriteFusion's Pixel Snapper at the blow-up's cell size,
  at the canvas's size, white keyed out as transparent. (Their palettes may differ a little; the
  app puts every frame on the sprite's own colours anyway. Laying them side by side for one shared
  palette hit the snapper's 10 000-pixel side once videos grew past 768.)
- **Out**: one Krita Output node with batch mode **animation** (the tooling nodes' own word for
  "these are frames"), named "Animation".
- Tried 2026-11-27 on a 250 × 249 dancer shrunk from a GIF the user likes ("sways side to side",
  "cheers with both fists"): the same face and colours throughout, crisp, 50 to 90% of each frame
  kept as drawn; the dark offset shadow behind her mostly lost while she moves, and fists raised
  past the canvas's top cut off (leave room round a sprite for its move).

### A2. What the canvas sends

- **The current frame alone** while Animation Assist is on (not Page Assist): the picture any run
  reads is drawn as playing shows the current frame (its background and foreground frames with it,
  no onion skins), not every frame stacked. Done by setting the canvas's play frame to the current
  frame while it reads the picture, as the GIF export draws its frames. Applies to every run, as
  generating "this frame" is what anyone animating means.
- **On white**: on a pixel art canvas a workflow gets the picture flattened on white where it's
  transparent, so a sprite on a hidden background reaches the model as a sprite on a plain
  background (now it arrives on black, which outlines run into). Styles already flatten it.

### A3. An animation result

- **The workflow side** (`crates/diffusion`): an Output node's batch mode is read when the workflow
  is prepared (`OutputPlace::animation`), and each picture it sends keeps it.
- **Kept as one result**: `take_made` gathers the pictures one run's animation output sent into a
  single result with `frames` > 1. Its pixels are the frames stacked top to bottom (the area's
  width, `frames` × its height), so it is still one buffer and one PNG in the artwork's
  `generation/` folder; its thumbnail PNG stacks the frames' thumbnails the same way. The index
  keeps `frames` (missing: 1), so older indexes read as before.
- **The reference's colours**: on a pixel art canvas a run keeps the colours of the picture it
  sent (its opaque pixels, if 4 096 colours or fewer). An animation's frames are snapped to them as
  they're kept: every pixel either transparent (alpha under half) or the nearest of those colours,
  fully opaque. So the frames are in exactly the sprite's colours, whatever palette the snapper
  chose, and can be drawn over with them.
- **Steadied**: the run keeps the picture it sent too, and wherever a frame's pixel and the eight
  around it barely changed from it (a weighted colour difference under 24 of 255), the sprite's
  own pixels are put back. The parts that don't move stay exactly as drawn, with no shimmer or
  soft speckle from the video; only the moving parts come from the video. (Tried on a 128 × 128
  goblin: 91 to 99% of each frame came back as drawn; checking only the pixel itself, without its
  neighbours, pasted ghosts of the balloon's old outline back.)
- **`ResultInfo.frames`**: the frames' thumbnails (empty for a picture), for the tray to play.
- **Shown**: the overlay shows its first frame.

### A4. The tray and Apply

- **The thumbnail plays** at 10 frames a second (Android and desktop), with a small "8 frames"
  badge. Everything else in the tray is as for a picture: tap shows it, hold deletes, Again.
- **Apply** (`apply_result`, one undo step):
  - turns Animation Assist on if it's off (with the pixel art defaults of part B on a pixel art
    canvas);
  - adds each frame as a new top-level layer, in order, just above the current frame (at the top
    of the stack when there is none), named "<name> 1", "<name> 2"… after the prompt;
  - makes the first of them the current layer, so the strip shows where they went;
  - says "Added 8 frames" (or "No room for 8 more layers").
- When more frames are added than the canvas has room for layers, nothing is added.

### A5. Not in part A

The frame strip's **Animate with AI…** (part C); in-betweening between two frames (it needs two
pictures sent; later); the sheet and the named animation the frames make (parts B and C).

## B. Sprite sheets and export (to go to ProCraft)

Built 2026-11-27 (`crates/core/src/sprite_sheet.rs`).

- **Pixel-friendly defaults**: Animation Assist turned on for a pixel art canvas starts at 10
  frames a second with one onion skin frame each side (instead of 15 and 12).
- **Share › Share Layers › Sprite Sheet** (Android's Share panel and the desktop's): every frame
  that plays in one PNG, left to right in one row (a row per animation once part C names them), a
  frame held longer appearing once, transparent where the background was, at the artwork's own
  pixels (the core takes ×1 to ×16; the menus ask for ×1, what engines want). Beside it
  `<name> sheet.json` in Aseprite's JSON array format: `frames` (each frame's layer name, place,
  size and duration in milliseconds: 1000 / fps, times one more for each frame it's held) and
  `meta` (the app, the PNG's file name, its size, the scale, and `frameTags`, empty until part C).
  On Android both go to the share sheet together (Save to device asks for a folder); on the
  desktop the PNG goes where it's saved and the JSON beside it. A sheet wider than 32 768 pixels
  isn't made.
- **Crisp GIF and APNG**: on a pixel art canvas their frames are scaled up by a whole number with
  each pixel a square, to about 512 pixels at full size (a 64 sprite ×8, a 128 ×4, a 250 ×2) or
  within the web-ready size, instead of a speck of the canvas's own size. No new setting.

## C. Animations by name (to go to ProCraft)

Built 2026-11-27 (`crates/engine/src/animation.rs`'s tags, `crates/core/src/animation_names.rs`).

- `Animation` gains `tags: Vec<Tag>` (`name`, `from: LayerId`), kept in the artwork's JSON. Each
  runs from its frame to the next tag's frame, or to the last frame that plays; frames before the
  first are an unnamed animation. A tag whose frame is gone is dropped (its frames join the one
  before).
- **Frame options** (tap the current frame) gains a group: **Start Animation Here…** (asks for a
  name, "walk" offered), and on a named animation's frames **Rename "walk"…** and **Remove
  "walk"**; each one undo step. On the desktop the name is typed in the same panel.
- **The strip**: a named animation's frames have a band of its colour along their bottom (six
  colours in turn), its name on its first frame.
- **Playing** loops the animation holding the current frame (all the frames without names).
- **AI animations** (part A's Apply) go after the end of the animation holding the current frame,
  so one already there stays whole, and start an animation named after the move.
- **The sprite sheet** lays a row per animation (a short row's end left clear); the JSON's
  `frameTags` lists the named ones (`name`, `from`, `to`, `direction` "forward"), each frame's
  `frame` its place in the grid.
- **A sheet for each named animation** too (2026-11-27, the user's ask: Godot reads Aseprite's
  JSON only with an add-on such as Aseprite Wizard or Aseprite Animation Importer): one row each,
  `<artwork> <name> (<n> frames).png` beside the combined sheet and JSON, so Godot's SpriteFrames
  editor can cut it by its frame count without an add-on. Names are made safe for files; two with
  one name are numbered.
- **Animate with AI…** (this fork, Android: the desktop has no workflow cards): in Frame options,
  opens Generate on the workflow card that makes animations (`WorkflowCard::animates`: an Output
  node with batch mode "animation"), a kept card first, else the live one, with that frame
  current; without one it says "Keep the Pixel animation workflow first".

## D. Checks

- Rust tests: the animation output read from a workflow; results from one run gathered into one;
  stacking and the index's `frames`; snapping to the reference's colours; Apply adding frames in
  order above the current one and turning Animation Assist on; the current frame alone sent.
- The starter's API fixture parses, its parameters are Prompt, Frames and Seconds, and preparing
  it links the canvas everywhere it's used.
- On the phone: the fox from the RD Flow card, animated, applied, played, a frame touched up.

## E. Decisions

- 2026-11-26: one result per animation, its frames stacked in one PNG (no change to how results
  are stored, saved or read, beyond a count).
- 2026-11-26: frames snapped to the reference picture's own colours, not the palette strip: the
  strip may hold a starting palette the sprite doesn't use.
- 2026-11-27: the video sized for 8 × 8 video pixels a canvas pixel (768 to 1280), and up to 24
  frames, after a 128 canvas came back soft at ×6.
- 2026-11-27: frames steadied against the sprite and snapped to its colours up to 4 096 of them
  (the user saw soft, fuzzy frames from a 268-colour RD Flow goblin on a 128 canvas: the snapping
  had been skipped over 256 colours, and the video's softness spans pixels when the blow-up is
  only ×6).
- 2026-11-26: the workflow sees the sprite on white; a sprite that is mostly white may lose parts
  where it touches the edge (said in the starter's note; a colour choice can come if it's needed).
