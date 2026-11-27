# Pixel art in ProCraft, stage 2: animation design

How the [stage 2 proposal](03-animation-proposal.md) is built. The proposal's principles hold:
new files with thin hooks, mobile first, every choice made for the user and changeable.

Status: agreed with the user 2026-11-26 ("let's move forward with building"); part A built
2026-11-26. Built in three parts:
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
  in view), **Frames** (whole number, 2 to 12, default 8) and **Seconds** (1 to 5, default 1.6).
- **The picture**: blown up with hard edges by the largest whole number that keeps its longest
  side within 768 pixels, then centred on white with the sides rounded up to a multiple of 32 (the
  model's step). It's the video's first and last frame, so the move loops.
- **The video**: MiniMax H3 first-and-last-frame to video with its 8-step LoRA, at 24 frames a
  second; its length from Seconds as the model's own workflow works it out (17k + 5 frames). The
  prompt is the move wrapped in the words that worked: a pixel art sprite animation, the camera
  still, the character centred at one size, a plain white background, crisp blocky pixels.
- **Frames kept**: the sprite's part of each video frame, the repeated last frame left out, every
  *n*th kept so about Frames remain.
- **Back to pixels**: the frames side by side through SpriteFusion's Pixel Snapper at the blow-up's
  cell size (one palette for all of them, white keyed out as transparent), cut back into frames at
  the canvas's size.
- **Out**: one Krita Output node with batch mode **animation** (the tooling nodes' own word for
  "these are frames"), named "Animation".
- Frames stays at 12 or below: the snapper takes at most 10 000 pixels a side, and 12 frames at 768
  is 9 216.

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
  sent (its opaque pixels, if 256 colours or fewer). An animation's frames are snapped to them as
  they're kept: every pixel either transparent (alpha under half) or the nearest of those colours,
  fully opaque. So the frames are in exactly the sprite's colours, whatever palette the snapper
  chose, and can be drawn over with them.
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

- **Pixel-friendly defaults**: Animation Assist turned on for a pixel art canvas starts at 10
  frames a second with one onion skin frame each side (instead of 15 and 12).
- **Share › Sprite sheet** (Android's Share panel and the desktop's): every played frame in one
  PNG, a frame held longer appearing once, one row per animation (part C; one row until then), at
  ×1 to ×16 with crisp pixels (×1 by default), transparent; beside it `<name>.json` in Aseprite's
  JSON array format (`frames`: each frame's place, size and duration in milliseconds; `meta`:
  the app, image name, size, scale, and `frameTags` once part C names animations). On Android
  both go out through the share sheet / Files, on the desktop to the folder chosen.
- **Crisp GIF**: Animated GIF and Animated PNG on a pixel art canvas offer the PNG's ×N scales and
  draw the frames with nearest pixels.

## C. Animations by name (to go to ProCraft)

- `Animation` gains `tags: Vec<Tag>` (`name`, `from: LayerId`), each running to the next tag's
  frame or the end. Hold a frame in the strip: **Start animation here…** (named), **Rename
  animation…**, **Remove animation**; coloured bands over the strip; playing loops the animation
  holding the current frame. Applying an AI animation starts one named after the move.
- The sprite sheet lays one row per animation; the JSON's `frameTags` lists them.
- **Animate with AI…** in the frame's hold menu (this fork): opens Generate on the Pixel animation
  card with that frame current.

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
- 2026-11-26: the workflow sees the sprite on white; a sprite that is mostly white may lose parts
  where it touches the edge (said in the starter's note; a colour choice can come if it's needed).
