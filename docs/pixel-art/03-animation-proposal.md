# Pixel art in ProCraft, stage 2: animation for game assets (proposal)

Stage 2 of the [pixel art canvas](01-proposal.md): animating sprites and getting them into a game,
as a simpler Aseprite does, with AI to help make the frames. The user is new to animation, so the
choices are made for him and the steps kept few.

Status: final (agreed 2026-11-26). The design: [04-animation-design.md](04-animation-design.md).

## What the user asked

- The abilities of a simplified Aseprite for animation.
- AI help making the frames: he doesn't animate by hand much.
- Sprite sheet export, for certain.
- AI on his own PC; paid online models only as a last resort.

## What ProCraft has already

Animation Assist (Actions › Canvas): each layer (or group) is a frame, with a frame strip along the
bottom, play and loop, onion skins (the frames either side shown faintly), frames a second, and a
frame held longer than the rest. It works on a pixel art canvas today. Stage 2 builds on it rather
than adding another timeline.

## Stage 2a: animate and export

- **Pixel-friendly from the start**: turning Animation Assist on for a pixel art canvas starts it at
  10 frames a second with one onion skin each side, easy to read on pixels. Everything stays
  changeable as now.
- **Animations by name** (Aseprite's tags): a run of frames named "idle", "walk" or "attack", so one
  artwork holds a character's moves. Made from the frame strip: hold a frame, *Start animation
  here…*, name it; it runs to the next one. Shown as coloured bands over the frame strip, and
  playing loops the animation the current frame is in.
- **Sprite sheet export** (Share › Sprite sheet): every frame in one PNG, **one row per
  animation** (row 1 idle, row 2 walk…; an artwork with none is one row), at ×1 to ×16 with crisp
  pixels, and beside it the **JSON in Aseprite's format** (each frame's place and time, each
  animation's frames), which importers for Godot, Unity, GameMaker and others read. Godot can also
  cut the PNG itself, by its rows and columns. A frame held longer keeps its time in the JSON.
- **Crisp GIF** at ×N, for sharing: Animated GIF on a pixel art canvas asks for the scale, as PNG
  does, with nearest pixels.

## Stage 2b: AI help with the frames

Making frames one at a time with an edit model drifts: the character changes a little each frame.
What works better is making **the whole move at once**, as a short video or as one sheet, so the
model keeps the character the same throughout. Stage 2b has two parts: the app turns any such
result into clean frames, and starter workflows make the results on the user's PC.

### In the app: frames from a result

- **Apply as frames**: when a workflow returns several pictures (a video's frames, or one sprite
  sheet), the results tray offers *Apply as frames* beside Apply. The app then:
  - picks the frames: from a video, evenly spaced (8 by default, changeable); from a sheet, it cuts
    the grid by finding each sprite on the plain background;
  - takes away the plain background;
  - makes each one a **real pixel sprite** at the canvas size, in the artwork's palette (the same
    pixel snapping as the Pixel art edit workflow, done in the app so every frame matches);
  - **lines them up** by the bottom centre, so the feet stay put and the sprite doesn't wobble;
  - adds them as new frames after the current one, as an animation named from the prompt
    ("walk"), each editable by hand.
- **Import a sprite sheet** (Actions › Canvas): the same cutting for a sheet picture from anywhere
  (another app, a website, an old game asset), so sheets made outside ProCraft come in too.
- **The current frame alone** goes to the AI while animating (not every frame drawn over each
  other), so a workflow sees just the sprite it's moving.

### On the PC: starter workflows (free, local)

The user's PC already has MiniMax H3's video models (reference to video, and first and last frame
to video) with their fast LoRAs, and ComfyUI ships an official H3 image-to-video workflow to build
from. Nothing new to download for the first two.

1. **Pixel animation (MiniMax H3)**, first: the current frame plus a move ("walks to the right")
   makes a short clip (about a second) on a plain background, which comes back as frames. With the
   same frame as first and last, the clip **loops**, which is what walks and idles need.
2. **In-between (MiniMax H3)**, second: the current frame and the next one (two key poses the user
   drew or made) make the frames between them, the animator's "in-betweening". It needs the app to
   send two frames; worked out in the design.
3. **Sprite sheet in one picture**, only if the video route disappoints: FLUX.2 Klein base 4B with
   a public pixel sprite sheet LoRA (Apache-2.0) draws a standard 4-direction walk sheet from one
   character picture. It's a download of a few GB.

Paid online models (Nano Banana and the like, through ComfyUI's partner nodes) aren't planned. If
the local ones can't do a move well, they stay a last resort the user can choose for himself.

**Tried first**: the first starter is built and tried on the user's PC before the app side is
designed, since how well it works decides the details (how many frames, how much cleanup).

## Not in it

Aseprite's linked cels, its ink modes, frame-by-frame layer visibility and its own file format
(reading `.aseprite` is stage 3). Tiles and tilemaps. Training a LoRA on one character.

## Decided for the user (he can change any of them)

- Sheets lay one row per animation; ×1 by default, as engines want the real pixels.
- Animations start at 10 frames a second with one onion skin each side.
- The JSON is Aseprite's, the one most game engines' importers already read.
- Apply as frames picks 8 frames from a video and lines frames up by the feet.
- Local models only; MiniMax H3 video first.
- The starters ship as workflow files only, never model weights; each model's own licence applies.
