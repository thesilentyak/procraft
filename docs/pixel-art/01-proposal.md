# Pixel art in ProCraft: proposal

A **Pixel art canvas** for ProCraft: a simpler, touch-first Aseprite inside the app, good enough to
draw sprites for fun and to make real game assets. It's written to go to ProCraft itself (an issue
first, then pull requests), not only to this fork.

Status: final, agreed with the user 2026-11-26 (stages in this order, sizes as listed, scripting
added as stage 4). Next: the design (`02-design.md`), then building in stages.

## The idea in one paragraph

Making a new artwork offers **Pixel art** beside the presets, with a size (16, 32, 64, 128, 256 or
custom). Everything else stays ProCraft: the gallery, layers, undo, animation, the colour panel, and
the AI workflows. What changes is that every tool works in **whole pixels with hard edges**, the
canvas shows each pixel as a crisp square with a grid when you zoom in, and exporting gives crisp
pictures and sprite sheets a game engine can read. There's no separate mode to manage, and no
settings to turn on: a pixel art canvas simply behaves that way.

## What ProCraft has already

Layers and groups, undo, animation (frames, onion skins, GIF/PNG/MP4 export), symmetry, shape
snapping (QuickShape), fill (ColorDrop), the eyedropper, palettes, rectangle/lasso/automatic
selections, transform with a Nearest Neighbor setting, and crisp pixels once zoomed past 100%. Most
of the work is making these behave the pixel way on a pixel art canvas, not new tools.

## Stage 1: draw

- **Pixel art canvas**: chosen when making a new artwork, at its size. It shows crisp squares at
  every zoom, a pixel grid when zoomed in close, and opens zoomed to fit.
- **Pencil and eraser**: each touch colours whole pixels, fully or not at all. 1 pixel by default,
  with a few square and round sizes (1 to 8). Lines are *pixel-perfect*: no doubled-up corners
  (Aseprite's best-loved setting, on by default). Pressure changes nothing but, optionally, size in
  whole pixels.
- **Fill**: exact colour. Fills the touching area, or every pixel of that colour (one switch).
- **Eyedropper**: the exact colour of one pixel.
- **Lines and shapes**: QuickShape's line, rectangle and ellipse drawn pixel-exact.
- **Selections**: rectangle, lasso and magic wand (automatic), all hard-edged and on the grid, no
  feather. Moving or pasting a selection moves by whole pixels.
- **Transform**: always Nearest Neighbor, moving by whole pixels. Flip and 90° turns stay clean.
- **Palette strip**: the current palette as a row of colours to tap. Palettes can be pasted as hex
  codes (as from Lospec) or taken from a picture.
- **Export**: PNG crisp at ×1, ×2, ×4, ×8 or ×16.

## Stage 2: animate for games

ProCraft's animation already has frames and onion skins. For game sprites it needs:

- **Sprite sheet export**: all frames in one PNG, laid in a row or a grid, with a JSON file in
  Aseprite's sprite sheet format, which Godot, Unity, GameMaker and most engines' importers read.
- **Tags**: name runs of frames ("idle", "walk", "attack") so a sheet says which frames are which.
- **Each frame's own time**, as Aseprite has.
- **Crisp GIF** at ×N for sharing.

## Stage 3: polish (each a small piece, in any order)

- **Shading**: a brush that paints the next lighter or darker colour of the palette over what's
  there.
- **Tile preview**: the canvas shown repeated, to make seamless tiles.
- **Replace colour**: every pixel of one colour to another, on a layer or all.
- **Outline**: a one-pixel outline round what's on a layer.
- **Aseprite files**: open `.aseprite` files (the format is published), so existing work comes in.

## Stage 4: scripts and plugins (later)

Automating the dull parts of animation and more: making the in-between frames of a move, recolouring
every frame, laying out a walk cycle, generating variations. ProCraft already has plugins whose code
can read the artwork, add layers and paint on them, and run from a frame's options in Animation
Assist. This stage gives them what pixel art needs:

- reading and writing frames, their times and their tags;
- the palette, and the pixel tools themselves (pencil, fill, selections) as functions;
- running from the canvas, a frame, a key or a pen button, each run one undo step.

Then, if it's wanted, a **plain script** you can write and run in the app without building a plugin,
as Aseprite's Lua scripts are, using the same functions.

## Not in it

Tilemaps, indexed colour mode, Aseprite's scripting and its many ink modes. Any could come later if
it's wanted. Plain (non-pixel) artworks don't change at all.

## How it fits ProCraft

- A pixel art artwork is an ordinary `.procraft` artwork with one mark saying it's pixel art; opened
  in an app without this, it's just a small artwork.
- The changes sit inside ProCraft's own tools (the brush, fill, selection, transform and display),
  each switched by that mark, so they can go to ProCraft as a few pull requests, one per stage.
  Plugins can't do this: a plugin's own kind of artwork can't use ProCraft's layers, animation or
  the AI workflows, and its brushes are still laid smoothed.
- Android tablet and phone first, then the desktop, as the rest of the app.
- Before building much, ask ProCraft's authors in an issue whether they'd take it, linking this.

## Decided

- 2026-11-26: written for everyone, to go to ProCraft. Stages in this order (drawing, animation for
  games, polish, scripts). New canvas sizes 16, 32, 64, 128, 256 and custom.
