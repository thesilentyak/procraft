# Pixel art in ProCraft, stage 2: sprite sheet import design

How the [stage 2 proposal](03-animation-proposal.md)'s **Import a sprite sheet** is built: a sheet
picture from anywhere (Gemini, another app, a website, an old game asset) comes in as clean,
lined-up frames of a named animation, ready to touch up. The proposal's principles hold: new files
with thin hooks, mobile first, every step done for the user and changeable afterwards.

Status: agreed and built 2026-11-27 (Android and the desktop). Written to go to ProCraft with
stages 1 and 2.

## Why

Tried 2026-11-27 on two sheets the user made in Gemini (a beet holding a balloon, 4 × 3, a JPEG on
dark grey with a stray white band; a turtle walk, 4 × 2, a JPEG on mid grey). Cleaned by hand with
the steps below, both came out as real pixel sprites (about 45 × 55 and 57 × 46 pixels, 14 and 11
colours) whose character stays the same in every frame. Gemini's one weakness showed too: the
turtle's bottom row is drawn about 3 pixels smaller. That's left for the user to touch up; the
import can't know which size is right.

## F1. Where

- **Actions › Add › Insert a sprite sheet** beside Insert a file and Insert a photo, on Android and
  the desktop, for any artwork. It opens the system's picture picker (PNG, JPEG, WebP, GIF's first
  frame).
- A picture **shared to ProCraft** or dropped on the canvas stays an ordinary insert: a sheet is
  only ever treated as a sheet when asked.

## F2. The import sheet

After picking, one small sheet shows what the app found, so a wrong guess costs nothing:

- the picture, with a numbered box round each frame it found, in reading order;
- a small preview playing the cleaned frames at the artwork's speed (10 fps on pixel art);
- **Name**: the animation's name, filled from the file's name ("turtle walk.png" → "turtle walk",
  "16418.jpg" → "imported"), up to 40 characters as elsewhere;
- **Import 8 frames** (the count in the button).

Folded under **Not right?**: **Grid** (frames across × down, for sheets whose sprites touch so
gaps can't split them) and **Background** (the colour found, with a tap to pick another from the
picture). Changing either redoes the boxes and the preview at once.

## F3. What the app does, in order

All of it in the engine (`crates/engine/src/sprite_import.rs`, no GPU, no UI), so both platforms
and the tests share it.

1. **Background.** If the picture has real transparency, that's the background (alpha at or above
   half counts as solid). Otherwise the background is every colour making up at least a tenth of
   the picture's border (so Gemini's stray white band and drawn checkerboards go too), within a
   tolerance that swallows JPEG noise. Background-coloured areas touching the edge or the gaps
   between sprites are removed; an enclosed patch of that colour is removed only when it's bigger
   than a few art pixels, so a dark grey eye stays.
2. **Frames.** Bands of rows with sprites in them, split by empty rows; each band split into frames
   by empty columns. A piece much smaller than its neighbours (a stray speck, a sparkle) joins the
   nearest frame, or goes if it's tiny. With **Grid** set, the picture is cut into equal cells
   instead. Empty cells are skipped.
3. **Pixel size.** How many picture pixels make one art pixel, from the rhythm of colour edges
   (10.7 on both Gemini sheets; 1 on a sheet of real pixels, 8 on one exported at ×8). A grid
   scores by the share of edge strength on its lines less the share of places it covers, so the
   true size beats its multiples and its fractions; a blurred edge is first gathered onto its
   peak. It's measured in each of the largest eight frames and the middle value taken (an AI
   picture's grid drifts across a whole sheet); fewer than half finding one means real pixels.
   Below 3 only an exact blow-up is believed. Each frame finds its own grid offset. At 4 and
   up, solid lines a pixel or two thin are dropped: Gemini's soft edge between two background
   colours would otherwise tie a row of sprites together.
4. **Real pixels.** Each art pixel takes the commonest colour of its block's middle, ignoring the
   blurred edges, so JPEG fuzz goes. Skipped at pixel size 1.
5. **Colours.** All frames together are brought to one small set of the sheet's own colours (near
   duplicates merged, colours on fewer than 0.4% of the pixels dropped as blur, at most 32), so
   the same green is the same green in every frame. Never forced onto the artwork's palette: the
   sprite keeps its look. The Gemini sheets came to 15 and 11 colours.
6. **Lining up.** Every frame is moved so its feet sit on one line: the bottom of its lowest pixels
   and the middle of its bottom few rows, as an engine's sprite would stand. A sheet of real
   pixels cut by **Grid** keeps each frame's place in its cell instead (its cells are exact, and
   a jump should stay a jump).
7. **Steadying.** Small shading flicker is calmed against the first frame with the steadying AI
   animations already use (`pixel::steady`).

On a canvas that isn't pixel art, steps 3 to 5 and 7 are skipped: the frames keep the picture's
own pixels, background removed and lined up.

## F4. Into the artwork

- The frames go in as the AI frames do (`add_generated_frames`'s path): Animation Assist turned on
  if it's off, new frame layers after the end of the animation holding the current frame, named
  "<name> 1", "<name> 2"…, the new animation named from **Name**, the first new frame current.
  One undo removes all the frames (a second, the canvas's growing). The code is shared with AI
  animations (`add_animation_frames`).
- **Placement**: the frames' shared box, centred on the canvas.
- **Too big**: if the sprite is bigger than the canvas, the canvas grows to fit it, existing art
  kept centred (as Crop & Resize does), and a notice says so ("Canvas made 64 × 64 to fit").
  Shrinking would ruin the pixels, and cutting would lose part of the sprite. The canvas grows to
  4 pixels more than the sprite each way that's short, by the same on both sides.

## F5. Checks

- Engine tests on sheets made in the test from a small sprite: blown up ×11 on grey with JPEG-like
  noise and a white band; real pixels on transparency; ×8 touching cells with **Grid**; a
  checkerboard background; an enclosed background patch; specks. Each checks the frames found, the
  pixel size, the colours and the feet's line.
- Core tests: frames added as a named animation after the current one, one undo, the canvas grown.
- The user's two Gemini sheets (kept out of git) through the engine: an ignored test reads raw
  RGBA files from `PROCRAFT_SHEETS`. Both came out as their 12 and 8 frames at ×10.7, matching a
  clean-up by hand, in under a second each.
- On the tablet and the desktop: the same sheets through Actions › Add.

Code: `crates/engine/src/sprite_import.rs` (reading), `crates/core/src/sprite_sheet_import.rs`
(`SpriteSheetImport`: look, insert), `android/.../ui/SpriteImportUi.kt`, `desktop/src/sprite_import.rs`.

## F6. Later, not now

- An animated GIF's or video's frames.
- Reading the Aseprite JSON beside a sheet (its frames and tags exactly).
- Sorting out a row drawn at a different size (the turtle's): left to the user's touch-ups.
