# Pixel art in ProCraft: design (draft)

How the [proposal](01-proposal.md) is built. Stage 1 (drawing) is designed in full; stages 2 to 4
are outlined, each to be designed before it's built. Written to go to ProCraft as pull requests, so
it changes ProCraft's own files, but each change is a few lines switched by one mark, with the work
in new files of its own.

Status: agreed with the user 2026-11-26; stage 1 (P1 to P5) built 2026-11-26.

## A. Principles

- **One mark, no settings.** An artwork is pixel art or it isn't (`Document::pixel_art`). Every
  pixel behaviour follows from it; nothing else to switch. A plain artwork behaves exactly as now.
- **Pixels are set on the CPU, not stamped.** ProCraft's brushes are GPU stamps with soft rims; bent
  to whole pixels they would still be sampled and blended. A pixel art canvas is small (at most
  1024 × 1024), so its tools work out which pixels change on the CPU and write them with the
  canvas's existing `write_layer_part`, which undo already records. Exact by construction, and the
  brush engine isn't touched.
- **Masks are all or nothing.** Every selection, fill and eyedropper on a pixel art canvas is
  hardened: a pixel is in or out (`Mask::harden`, at half).
- **New files, thin hooks.** The work goes in `crates/engine/src/pixel.rs` (the pure parts: lines,
  footprints, pixel-perfect, fills, hardening, scaling) and `crates/core/src/pixel_art.rs` (the
  session side), with a line or two in the files it hooks into.
- **Mobile first**, Android then desktop, as the rest of ProCraft.

## B. The mark and the canvas

- `Document` gains `pixel_art: bool` (`#[serde(default, skip_serializing_if = "is_false")]`), so
  older files open as plain artworks and plain artworks save as before. An app without this opens
  a pixel art artwork as a small plain one.
- **New canvas**: the gallery's New canvas panel gets a **Pixel art** row (beside the presets, as
  plugins' kinds are) opening a short list: 16, 32, 64, 128, 256 (square) and Custom (width and
  height 1 to 1024). It makes the artwork with the mark set and the background hidden (transparent,
  as Aseprite starts), and opens it zoomed to fit.
- **Display**: on a pixel art canvas `crisp_from` is 0, so pixels are squares at every zoom (now
  they're crisp only from 100%).
- **Pixel grid**: a one-pixel grid drawn over the canvas once a canvas pixel is 8 screen pixels or
  more, faint, fading in over the next few steps of zoom. It is not the drawing guide and isn't
  saved; the drawing guide stays available for larger grids (tiles of 8 or 16, say).

## C. Stage 1 tools

### C1. Pencil and eraser

- **Where**: `stroke_begin`, `stroke_add`, `stroke_end` and `stroke_cancel` in `crates/core/src/lib.rs`
  already let a plugin's own artwork take its strokes first (`plugin_touch`). Beside that, on a
  pixel art canvas with the paint or erase tool, `pixel_art::touch` takes the stroke. Smudge stays
  as it is (it's rarely wanted in pixel art; it can be left off the pixel canvas's tools later).
- **Size and shape**: the brush size slider gives 1 to 8 pixels on a pixel art canvas. Size 1 and 2
  are squares; 3 and up are round (a disc of whole pixels, as Aseprite's round brush) unless the
  chosen brush is a square one: the pixel canvas reads only the brush's roundness (`roundness < 1`
  or a square shape means square), so the brush library needs no pixel brushes of its own. Pressure
  changes nothing by default.
- **Lines**: each new sample's pixel (`floor` of its canvas point) joins the last by a Bresenham
  line, each pixel of it stamped with the footprint.
- **Pixel-perfect** (size 1): an L-shaped corner's middle pixel is dropped as the line goes (when
  pixel *i − 1* touches both *i − 2* and *i* and those two touch diagonally, *i − 1* is put back as
  it was). Always on: it's what pixel artists want from a 1-pixel pencil.
- **Colour**: the current colour, its alpha the brush's opacity. Each pixel is set once a stroke,
  from what it was before the stroke, so going over it again doesn't build up. The eraser sets
  pixels clear (or, below full opacity, takes that much away).
- **Respects**: the selection (hardened), the layer's alpha lock and lock, the drawing guide's
  symmetry (vertical, horizontal, both; each pixel mirrored about the canvas's middle line, exact
  on whole pixels), and Animation Assist (it paints the current frame's layer, as now).
- **Undo**: `mark_steps` before the stroke, one step after.
- **Speed**: the layer is read once at the stroke's start (`read_layer`, at most 4 MB), changed on
  the CPU, and only the rectangle around each batch's new pixels is written.
- **QuickShape**: holding at the end snaps as now. On the pixel canvas the snapped line, rectangle,
  ellipse or arc is drawn again as pixels: a Bresenham line, a rectangle of four lines, and a
  midpoint ellipse, in place of the freehand pixels.

### C2. Fill

- ColorDrop and the fill tool on a pixel art canvas: the flood at threshold 0 by default (exact
  colour), its mask hardened, poured at full strength (no `under_edges` spreading, no fade).
- A **Contiguous** switch in ColorDrop's threshold bar, on by default: off fills every pixel of that
  colour on the layer (Aseprite's "contiguous" checkbox).
- The threshold still slides, for fills over near colours.

### C3. Eyedropper

- The colour of the one pixel under the point, from the current layer or the whole picture as the
  eyedropper's setting says now, not an average.

### C4. Selections

- **Rectangle and ellipse**: their corners snap to pixel corners; the mask is hardened.
- **Freehand and polygon**: hardened (a pixel is in if at least half of it is).
- **Automatic** (magic wand): threshold 0 by default (exact colour), hardened. Its slide still works.
- **Feather** on a pixel art canvas is left out of the selection bar.
- **Copy, paste, cut and move** keep their pixels exact (pasted at whole-pixel places).

### C5. Transform

- On a pixel art canvas the interpolation is Nearest and isn't offered. Moves go by whole pixels
  (ProCraft already rounds an unscaled move to whole pixels; on the pixel canvas every move rounds).
- Flip and the 90° turns are exact. Free rotation and scaling stay available, nearest.

### C6. Palette strip

- A row of swatches along the colour panel's bottom on a pixel art canvas: the colours of the palette
  chosen for the artwork (ProCraft's palettes, kept with the artwork as which one it is).
- Tapping a swatch takes its colour; holding gives Add the current colour and Remove.
- Its menu: **Paste hex codes** (as Lospec gives them), **From the picture** (the colours used on
  the canvas, by ProCraft's palette-from-picture), and choosing another of ProCraft's palettes.

### C7. Export

- Share › PNG on a pixel art canvas asks for a scale: ×1, ×2, ×4, ×8, ×16, each pixel a square of
  that many (nearest), as the canvas's own pixels are.
- The `.procraft` package and the other formats are as now.

### C8. Generation (this fork's, and only where it's on)

- Workflows already get the canvas's own pixels, so the pixel art workflows work pixel for pixel on
  it. A result is applied as it is (nearest, never smoothed) when the artwork is pixel art.

## D. Later stages (each designed before it's built)

- **Stage 2, animation for games**: sprite sheet export (row or grid, PNG plus Aseprite's sprite
  sheet JSON, `frames` and `meta.frameTags`), tags on runs of Animation Assist's frames, each
  frame's own time, crisp GIF at ×N. Most of it extends `crates/engine/src/animation.rs` and the
  export menu.
- **Stage 3, polish**: shading ink (the palette's next lighter or darker colour, by the palette's
  order), tile preview (the canvas repeated round itself), replace colour, a 1-pixel outline, and
  reading `.aseprite` files (the published format, RGBA and indexed, layers and frames).
- **Stage 4, scripts**: plugin functions for frames, tags, the palette and the pixel tools, then
  perhaps plain scripts run in the app with the same functions.

## E. Milestones (stage 1)

Each is checked on an Android tablet or phone and on the desktop before the next.

| | What | Done when |
|---|---|---|
| P1 | The mark, New canvas › Pixel art, crisp display, the pixel grid, PNG export ×N | A 32 × 32 artwork made, shown crisp with its grid, exported at ×8 |
| P2 | Pencil and eraser: footprints, lines, pixel-perfect, opacity, selection, alpha lock, symmetry, undo, QuickShape | Sprites drawn by hand with clean 1-pixel lines |
| P3 | Fill (with Contiguous) and the eyedropper | Fills exact, eyedropper exact |
| P4 | Selections hardened and snapped, the wand exact, transform nearest by whole pixels | Cut, move and flip a part with no soft pixels |
| P5 | The palette strip | A Lospec palette pasted and used |

The engine's parts (`pixel.rs`) get unit tests: lines, pixel-perfect corners, footprints, hardening,
exact fills, scaling. The session's get tests as ProCraft's `tests.rs` has for strokes.

## F. Going to ProCraft

1. An issue first (text below), asking whether they'd take it and how they'd like it split.
2. Then one pull request per milestone, each small, each with its tests, from a branch kept off
   this fork's own changes (no generation code in them).

Issue text, for the user to post:

> **Pixel art canvas (a simpler, touch-first Aseprite)**
>
> I'd like to add a pixel art kind of artwork: chosen in New canvas at 16 to 256 pixels (or custom),
> it behaves the pixel way with no settings: crisp pixels at every zoom with a pixel grid, a pencil
> and eraser that set whole pixels (1-pixel lines pixel-perfect), exact fill and eyedropper,
> hard-edged selections, nearest transforms by whole pixels, a palette strip, and PNG export at ×N.
> Later: sprite sheets with Aseprite's JSON for game engines, frame tags and times, shading, tile
> preview, opening .aseprite files, and plugin functions for scripting animation.
>
> It would be one `pixel_art` mark on the document, with the tools' pixel behaviour in new files
> and a few lines where they hook in; plain artworks wouldn't change. Pixels are set on the CPU and
> written with `write_layer_part`, so the brush engine isn't touched. A full write-up is here:
> (link). Would you take it, and would you like it as one pull request per milestone?

## G. Decisions

| Date | Decision |
|---|---|
| 2026-11-26 | A pixel art canvas inside ProCraft, not a separate editor or a plugin (plugins' own artworks can't use layers, animation or generation, and their brushes are laid smoothed) |
| 2026-11-26 | Pixels set on the CPU and written with `write_layer_part`, not stamped by the brush engine |
| 2026-11-26 | Pixel-perfect always on for size 1; pressure doesn't change pixel strokes by default |
| 2026-11-26 | Clear pixels show a checkerboard of 8-pixel squares lined up with the pixels (the surround's own grid showed through beside the pixel grid, looking doubled) |
| 2026-11-26 | The pencil's shape follows its width alone for now (squares to 2, discs from 3); a square brush's own shape can come later |
| 2026-11-26 | QuickShape on the pixel pencil: the held line snaps as it does elsewhere, and the shape's outline is drawn again as pixel lines, following the pen until it lifts (no edit handles afterwards) |
| 2026-11-26 | Rectangle and ellipse selections are upright on the canvas with their corners on pixel corners, whatever the view's turn |
| 2026-11-26 | A freehand, rectangle or ellipse selection being drawn on a pixel art canvas shows the pixels it takes as it goes (joined to the selection before it, kept as one step only when it closes or is let go), not only once it's made (the user: cleaner to work with) |
| 2026-11-26 | A pixel art canvas keeps its own ColorDrop threshold, starting at 0% (others start at 50%); its fill takes only pixels wholly within it, never across a diagonal; Contiguous sits in ColorDrop's band (Android), on as it starts, off filling every pixel of that colour on the layer |
| 2026-11-26 | The palette strip lives as a small square under the side bar (the user's idea), unfolding a two-column strip of swatches beside the bar; + adds the current colour, ⋯ pastes hex codes from the clipboard, takes the picture's colours, or clears; holding a swatch removes it. New pixel art canvases start with a 16-colour palette, and are sRGB (hex codes and exported PNGs as game engines read them) |
| 2026-11-26 | The eyedropper on a pixel art canvas reads the visible picture's pixel exactly: a see-through pixel's colour as where it's solid, a clear one leaving the colour as it was |
