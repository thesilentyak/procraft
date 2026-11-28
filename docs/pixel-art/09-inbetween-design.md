# Pixel art in ProCraft, stage 2: AI in-betweening (design)

How the [stage 2 proposal](03-animation-proposal.md)'s **In-between (MiniMax H3)** is built: two
frames from the canvas (a **First frame** and a **Last frame**) go to MiniMax H3's first-and-last-frame
video model on the user's PC, and the generated middle frames land right between them in the same
animation. Works on pixel art canvases (snapped back to crisp pixels and the two frames' shared
colours) and on regular 2D canvases alike.

Status: agreed, tested live on ComfyUI (MiniMax H3, RTX 5070 Ti), and built 2026-11-28 (starters,
engine, core, Android). Fork-only (uses the generation feature and ComfyUI workflows; nothing here
touches upstream ProCraft's own files beyond one row in Android's Frame options).

---

## A. How it feels in the app

1. **Two ways to start:**
   - **From the frame strip (fastest):** tap a frame (when there's a next frame after it) →
     **In-between with AI…** (right below *Animate with AI…*). It picks the in-between workflow card,
     fills **First frame** with the current frame and **Last frame** with the next frame, and opens
     the Generate popover ready to run.
   - **From the Generate popover:** pick the **Pixel in-between (MiniMax H3)** card (or
     **In-between (MiniMax H3)** on painted art). **First frame** and **Last frame** sit right below
     the Prompt field (not hidden under More), defaulting to the current frame and the next frame,
     and can be changed to any two frames or layers.
2. **Optional prompt & settings:**
   - **Prompt** is optional (a short move hint like `"fast sword slash"` or `"hops forward"`; left
     blank, the workflow still produces smooth motion between the two poses).
   - Folded under **More**: **Frames** (middle frames to generate, 1 to 20, default **6**) and
     **Seconds** (clip duration, 0.9 to 5.0, default **1.6** — tested 2026-11-28: 1.6 s / 39 video
     frames gave noticeably smoother pacing and anticipation than 0.9 s / 22 frames, for only ~15 s
     more).
3. **Preview & Apply:**
   - The results tray plays the generated middle frames as one animated result, just like loop
     animations.
   - **Apply** (one undo step) turns Animation Assist on if it's off and inserts the middle frames
     **immediately after the First frame** (between First and Last), **keeping them inside the same
     animation** (no duplicate endpoints, no new animation tag). The first inserted frame becomes
     current so pressing Play shows the full move (`First → middle frames → Last`) right away.
   - If the First frame was deleted before Apply, the frames fall back to inserting after the
     current frame.

---

## B. Starter workflows on the PC (`dev/starters/make_starters.py`)

Both use the exact MiniMax H3 first-and-last-frame models already on the user's PC (no new
downloads) and standard Krita marker nodes (`ETN_KritaCanvas`, `ETN_KritaImageLayer`,
`ETN_Parameter`, `ETN_KritaOutput` with `batch_mode: "animation"`).

### B1. `ProCraft - Pixel in-between (MiniMax H3)`

- **Inputs:**
  - `ETN_KritaCanvas` (canvas width, height, seed).
  - `ETN_KritaImageLayer` **"First frame"** and `ETN_KritaImageLayer` **"Last frame"**.
  - `ETN_Parameter` **"Prompt"** (positive prompt, default `""`).
  - `ETN_Parameter` **"Frames"** (integer, 1–20, default `6`).
  - `ETN_Parameter` **"Seconds"** (number, 0.9–5.0, default `1.6`).
- **Blow-up & video:**
  - Both First frame and Last frame are scaled up with `ImageScale` (`nearest-exact`, using exact
    `w * k × h * k` rather than `ImageScaleBy` which caps at `8.0`) by the same whole factor `k`
    targeting `8 × max(w, h)` within `768..1280` pixels, centred on white padded to multiples of 32.
  - Fed into `MiniMaxH3ImageToVideo` (`first_frame` and `last_frame`) with its 8-step LoRA and
    video length `17k + 5` (at 24 fps, 39 frames at 1.6 s).
  - Prompt wrapped in crisp pixel-art motion phrasing (stationary camera, same character size, plain
    background, crisp blocky pixels, no motion blur).
- **Symmetric middle-frame sampling:**
  - Crops back to the sprite area, drops frame `0` (First) and frame `L - 1` (Last), and picks
    `N = min(Frames, L - 2)` evenly spaced frames symmetrically centred in `1 .. L - 1`:
    - `step = max(1, floor((L - 2) / N))`
    - `start = 1 + floor(((L - 2) - (N - 1) * step) / 2)`
    - `count = min(L - 1 - start, N * step)`
    - `GetImageRangeFromBatch(start, count)` → `RebatchImages(step)` → `ImageFromBatch(0, 1)` →
      `ImageListToBatch+`.
  - Produces the exact requested middle-frame count (e.g. 6 frames out of 22 picks indices
    `3, 6, 9, 12, 15, 18` — even gaps from both endpoints).
- **Back to pixels:**
  - `SpriteFusionPixelSnapper` at cell size `k`, exact canvas size, `#FFFFFF` connected chroma key
    → `JoinImageWithAlpha` → `ETN_KritaOutput` (`batch_mode: "animation"`).

### B2. `ProCraft - In-between (MiniMax H3)` (regular 2D art)

- Same inputs (**First frame**, **Last frame**, **Prompt**, **Frames**, **Seconds**) and same
  symmetric middle-frame sampling.
- Scales First and Last smoothly (`lanczos`) to fit within `768..1280` (multiples of 32), runs
  MiniMax H3 with a smooth 2D animation prompt template (no pixel-art phrasing, no white chroma key,
  no Pixel Snapper), resizes back to canvas dimensions, and outputs via `ETN_KritaOutput`
  (`batch_mode: "animation"`).

---

## C. Engine & Core changes

### C1. Recognizing in-between workflows (`crates/diffusion/src/workflow.rs`, `crates/core/src/generation_workflows.rs`)

- `Workflow::inbetweens(&self) -> bool`: true when `self.animates()` and the graph has at least one
  `ETN_KritaImageLayer` node.
- `WorkflowCard` gains `#[uniffi(default = false)] pub inbetweens: bool`.
- `ParameterInfo::in_view` becomes true for `ParameterKind::Prompt`, `ParameterKind::ImageLayer`,
  and `ParameterKind::MaskLayer`, so frame/layer pickers are immediately visible without opening
  *More*.

### C2. Reading frames cleanly (`crates/core/src/generation.rs`, `crates/core/src/generation_workflows.rs`)

- A frame in Animation Assist may be a single paint layer **or a layer group**.
  `CanvasSession::layer_picture` is updated so if `id` is a group, it renders that frame cleanly on
  the GPU (transparent background on pixel art, canvas background on painted art) instead of
  returning `None`.
- For animation workflows (`workflow.animates()`), picked `ImageLayer` pictures on a **pixel art**
  canvas keep their transparent straight-alpha copy for reference (colour snapping + steadying) and
  send a white-flattened copy (`on_white`) to ComfyUI, matching how `workflow_canvas` already sends
  the canvas. Non-animation workflows continue receiving untouched straight RGBA + alpha mask.
- Smart defaults when a layer parameter isn't explicitly chosen:
  - `"First frame"` (case-insensitive) defaults to the **current top-level frame** (or current layer
    when Animation Assist is off).
  - `"Last frame"` defaults to the **next playable frame** after the current one (falling back to
    the current frame if on the last frame).

### C3. Combined colours & two-frame steadying (`crates/engine/src/pixel.rs`, `crates/core/src/generation.rs`)

- On a pixel art canvas, an in-between run keeps:
  - `colours`: the combined unique solid colours of **both** First frame and Last frame (up to
    `MOST_SNAPPED_COLOURS = 4096`), so colours introduced in Last frame (e.g. a sword flash or opened
    eyes) are never lost.
  - `reference` (First frame) and `reference_end` (Last frame).
- New pure function `pixel::steady_between(frame, first, last, width, height)`:
  - Puts back `first`'s pixel only where **both** `first` vs `last` barely differ (within
    `STEADY_WITHIN` across the 3×3 neighbourhood) **and** `frame` vs `first` barely differ.
  - Static parts (e.g. a still torso or hat) stay rock-solid with zero shimmer, while moving parts
    are completely free to transition toward `last`.

### C4. Inserting in-between frames on Apply (`crates/core/src/animation_names.rs`, `crates/core/src/generation.rs`)

- `Running`, `Kept`, `WriteResult`, and `SavedResult` gain an optional `insert_after: Option<u64>`
  (`#[serde(default)]` in `index.json` for full backwards compatibility).
- Set to `Some(first_frame_layer_id)` for in-between workflow runs (`None` for loop animations and
  single pictures).
- `CanvasSession::insert_inbetween_frames(step, area, pixels, frames, after, layer_name)`:
  - One undo step (`self.change`).
  - Turns Animation Assist on if it's off.
  - Finds the top-level frame `after` (falling back to the current frame if `after` was deleted),
    sets it current, and adds the `frames` new layers directly above it—**without** adding a new
    `Tag`, so they stay inside the same named animation.
  - Uploads each frame's pixels to its new layer, selects the first inserted frame, and refreshes.

---

## D. Android UI (`GenerationWorkflows.kt`, `GenerationUi.kt`, `AnimationUi.kt`)

- **Fix `LayerPicker` ID lookup:** match picked layers by **index/ID** instead of by layer name
  (fixing duplicate layer names), and when Animation Assist is on, list top-level frames clearly
  (e.g. `Frame 1 · Pose A`).
- **Frame defaults in the popover:** when an in-between card is selected and no manual pick has
  been made for the current artwork, show the current frame for `"First frame"` and the next frame
  for `"Last frame"`.
- **Frame options › `In-between with AI…`** (`AnimationUi.kt` + `GenerationUi.inbetweenFrame`):
  - Shown right below `Animate with AI…` when `index < last` (there is a next frame).
  - Picks the best matching `inbetweens` card (preferring a pixel-art card on a pixel art canvas and
    a non-pixel card on a regular canvas; kept card first, else live), sets `First frame` = current
    frame ID and `Last frame` = next frame ID, and opens Generate.
  - `animateFrame` (existing `Animate with AI…`) filters to `animates && !inbetweens` so it always
    picks the loop card.

---

## E. Checks

- **Engine (`pixel.rs`):** `steady_between` locks static pixels shared by both references and leaves
  pixels that moved between First and Last untouched.
- **Diffusion (`workflow.rs`):** both new starter API fixtures parse, report `animates() == true`
  and `inbetweens() == true`, expose `First frame`, `Last frame`, `Prompt`, `Frames`, `Seconds`,
  and prepare cleanly with both layer images wired into `MiniMaxH3ImageToVideo`.
- **Core (`generation.rs` / `animation_names.rs`):**
  - `layer_picture` renders a layer group cleanly.
  - Combined colours from First + Last preserve a colour present only in Last.
  - `insert_inbetween_frames` places new frames right between Frame 1 and Frame 2 inside the same
    named animation, with 1 undo removing them all.
- **Live on PC + tablet:** run `ProCraft - Pixel in-between (MiniMax H3)` between two poses of a
  sprite (safe subject, e.g. a walking turtle or bouncing ball), verify smooth transition, crisp
  pixels, and Apply inserting right between the two frames.

---

## F. Decisions

| Date | Decision |
|---|---|
| 2026-11-28 | Apply inserts only the middle frames right between First and Last, staying inside the same animation |
| 2026-11-28 | Two ways to pick: Frame options › **In-between with AI…** (current + next) plus visible First/Last pickers in Generate |
| 2026-11-28 | Two new starters: **Pixel in-between (MiniMax H3)** and **In-between (MiniMax H3)** for regular 2D art |
| 2026-11-28 | Kept separate from the loop animation card so each card does one clear job |
