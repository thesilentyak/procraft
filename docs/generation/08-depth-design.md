# Depth control: design

Status: **final** (agreed 2026-11-15). Turns `07-depth-proposal.md` (final) into screens, code and checks.
It adds to `03-design.md` §A6 (control layers) and changes one rule there (hidden control layers);
§A6 is brought up to date when M9 is built. Names: `names.md`.

---

# Part A — Experience

## A1. Adding a depth layer

- **In the popover**, the Control part's add row becomes three buttons: **+ Line art · + Scribble ·
  + Depth**, each a third of the row (to check on the tablet that "+ Line art" still fits on one
  line; if not, the words drop the "+ "). + Depth adds an empty layer named **"Depth"** above the current one and makes it
  current, one undo step ("New control layer"), as the others do.
- **In the Layers panel**, the layer options' **Control layer** menu gains **Depth**: None · Line art
  · Scribble · Depth.
- **Marks**: Android's caption under the layer's name reads **"Depth control"**; the desktop's badge
  left of the blend letter is **three short level lines, each shorter than the one below** (ground
  going into the distance), drawn like the other two badges.

## A2. Painting depth

- An ordinary layer: any brush, any colour, though only its lightness counts. **White is nearest,
  black furthest; what's transparent counts as black (far).** So a white blob on an empty depth
  layer means "something near here", with the rest left to the model as far away.
- Colours count by their grey (luminance), so painting in colour doesn't break it.

## A3. Tracing depth

- The row's trace button, as for the other kinds: what's visible (the whole canvas, or inside the
  selection), control layers left out, goes to the server's depth reader, and **the greys replace
  the layer's pixels there, opaque**, one undo step ("Trace control layer"). Outside a selection the
  layer keeps its own pixels.
- While it runs: the progress ring on the button, and "Tracing depth…" in the results tray (or its
  tab), as "Tracing line art…" is now.
- **No slow first trace on a companion install**: the companion downloads the depth reader (and
  the line extractors' models) when it installs, so the first trace is as quick as any (B5). On a
  server set up by hand, the add-on still fetches its model the first time, over the PC's
  internet: that one trace is slow (the 1.3 GB download first), with the usual progress.

## A4. Hidden control layers keep guiding (every kind)

The rule from the proposal (§6, as Krita):

- **The eye in the Layers panel only hides a control layer from view; it keeps guiding** Generate,
  Refine and Selections. This is now true of Line art and Scribble too.
- To stop one guiding: **×** on its row in the popover, or Control layer › None (it becomes a plain
  layer, its pixels kept), or delete the layer.
- **Its popover row stays as it is, not dimmed** (it is still in use), its caption saying so:
  "Depth · hidden", "Line art · hidden". The desktop shows hidden rows already (dimmed); Android
  leaves them out today. Both now show every control layer, undimmed.
- A hidden control layer is still left out of the picture sent and of what's traced, as now.

## A5. The popover row

- Thumbnail, name, and the caption: **"Depth · lighter is nearer"** (the hint, kept short; "Depth ·
  hidden" when hidden). Then trace and ×, then the four steps, as the other kinds.
- **The steps**: Loose · Balanced · Close · Strict, **Balanced** by default. They start with the
  same numbers as line art (strength and how far through the steps it applies: Loose 0.5 to 40 %,
  Balanced 0.6 to 60 %, Close 0.7 to 80 %, Strict 1.0 to 100 %), which the tests found already work
  for depth (Balanced moved the tower and the boulder to where the map had them). They become their
  own constants for depth, re-tuned on the dev server only if a step proves too weak or too strong.

## A6. Without the model

- With `anima-lllite-depth-1` missing, **+ Depth is greyed**, and tapping it says
  **"needs anima-lllite-depth-1.safetensors"** in the popover's error line, as the other missing
  files are named. Existing depth layers stay (their rows show the same note), and Generate runs
  without them rather than failing.
- Without the depth reader node (an old `comfyui_controlnet_aux`), the depth row's trace button
  says "needs comfyui_controlnet_aux", as line art's does without its add-on.
- The server view's feature list gains **"Depth control"** with ✓ or what's missing.

---

# Part B — Architecture

## B1. Engine (`crates/engine/src/document.rs`, a thin hook)

- `ControlKind::Depth`, serialised as `"Depth"` like the others.
- `Document::control_layers()` drops its `&& l.visible`: every control layer, shown or not (A4).
  Its other users are checked: what's left out of the picture sent (hidden layers aren't in it
  anyway) and the popover's list.
- *Known limit*: an artwork with a depth layer opened in an older ProCraft Diffusion build can't
  read the new kind. Upstream ProCraft has no control field at all, so it is unaffected (it already
  ignores the field).

## B2. `crates/diffusion`

- `request.rs`: `ControlKind::Depth`.
- `family.rs` / `family/anima.rs`: `ModelKind::DepthControl`, folder `controlnet`, pattern
  `anima*lllite*depth*`, described as `anima-lllite-depth-1.safetensors`. The control's graph picks
  the file by the control's kind (Line art and Scribble the any-test file, Depth the depth file):
  the same `ETN_control_load` / `ETN_control_apply` pair. `control_settings` takes the kind, with
  depth's own four pairs (A5).
- `picture.rs`: `Picture::control` takes the kind. Line art and Scribble as now (flattened onto
  white, turned into white lines on black). **Depth: flattened onto black, kept as it is** (greys,
  white near). A new `depth_layer()` beside `lines_alpha()` turns a traced map into opaque grey
  pixels (each pixel its luminance, alpha 255).
- `extract.rs`: Depth is `DepthAnythingV2Preprocessor` with `ckpt_name =
  "depth_anything_v2_vitl.pth"` (Large, decided), at the same resolution rule as the others (the
  shorter side rounded to 64, between 512 and 2048); to check on the dev server that 2048 isn't too
  slow or heavy for Large, and cap it lower for depth if it is.
- `requirements.rs`: a new `Feature::DepthControl`, labelled "Depth control", needing the control
  nodes, `DepthAnythingV2Preprocessor` and `ModelKind::DepthControl`. Line art and Scribble keep
  their features as they are, so a server without the depth file loses nothing else.

## B3. `crates/core`

- `layers.rs`: `LayerControl::Depth`, both ways to the engine's kind. `layer_ops.rs`: a new depth
  layer is named "Depth".
- `generation.rs`:
  - `run_controls` gathers every control layer (hidden too), and leaves out a depth layer when the
    server isn't ready for `DepthControl` (A6), rather than failing the run.
  - `control_picture` passes the layer's kind to `Picture::control`.
  - `trace_into` uses `depth_layer()` for a depth layer, `lines_alpha()` for the others.
  - The run's `extracting` state names "depth" for the tray.
  - `ControlLayerInfo.visible` stays: the apps use it for the "· hidden" caption only.

## B4. The apps

| | Android (`GenerationUi.kt`, `LayersPanel.kt`) | Desktop (`generation.rs`, `layers.rs`) |
|---|---|---|
| Add row | a third button, + Depth; greyed with its note when the file is missing | the same |
| Rows | every control layer (no more `filter { it.visible }`), undimmed; captions per A5 | rows undimmed; captions per A5 |
| Layers panel | "Depth control" caption; Depth in the Control layer menu | the depth badge (A1); Depth in the Control layer menu |
| Tray | "Tracing depth…" | `extract_name`: "depth" |

## B5. The companion

- `pins.rs`: `anima-lllite-depth-1.safetensors`, folder `controlnet`, 7,864,128 bytes, SHA-256
  `81357e51cd59874b4fbdda1eb3117aa73eafe36cc7efb9cf80a152d6bc8d0602` (checked against Hugging
  Face's), from `huggingface.co/kohya-ss/Anima-LLLite`.
- `check.rs`: a "depth control" kind with the pattern above.
- **The tracers' models, downloaded at install** (decided 2026-11-15), so no trace waits for a
  download. `comfyui_controlnet_aux` looks for them under its own `ckpts/` folder, by Hugging Face
  repository, and only downloads what isn't there; the companion puts them there first:

  | File | Under `custom_nodes/comfyui_controlnet_aux/ckpts/` | Bytes | For |
  |---|---|---|---|
  | `depth_anything_v2_vitl.pth` | `depth-anything/Depth-Anything-V2-Large/` | 1,341,395,338 | Depth |
  | `netG.pth` | `lllyasviel/Annotators/` | 217,631,959 | Line art |
  | `table5_pidinet.pth` | `lllyasviel/Annotators/` | 2,871,148 | Scribble |

  Each with its SHA-256 from Hugging Face (`a7ea19fa…9345`, `ccabdcc3…c488`, `80860ac2…486c`), from
  `huggingface.co/<repository>/resolve/main/<file>`. `ModelFile.folder` is relative to
  `ComfyUI/models/` today, so these go in a second list whose folders are relative to `ComfyUI/`;
  `install` and `models` download both lists, and `check` looks for both. The line extractors'
  models are included for the same reason (a first line art trace waits for 218 MB today).
- The paths are the add-on's own choice: when the companion's pinned `comfyui_controlnet_aux` is
  updated, these paths are checked again.

---

# Part C — Testing and plan

## C1. Testing

- **Unit** (`crates/diffusion`): depth's graph uses the depth file and its pairs; Line art and
  Scribble unchanged; the depth extractor graph names Large; `Picture::control` for depth flattens
  onto black and keeps the greys; `depth_layer()` is opaque luminance; `DepthControl` readiness
  names the file and the node; a server without the depth file is still ready for control layers.
- **Core**: a hidden control layer is sent with a run; a depth layer is left out of a run on a
  server without the file; a depth trace lands as one undo step of opaque greys; Control layer ›
  Depth sets the kind and is saved and read back with the artwork.
- **Server** (`#[ignore]`, the dev ComfyUI): a depth trace of a test picture comes back as greys;
  a generation with a painted depth layer (a scenery prompt with `scenery, no humans`) runs, and
  the steps are looked at once to confirm Balanced as the default.
- **On the tablet and the desktop**: + Depth, painting a white blob, Generate; trace, hide, Generate
  (still guided); ×; the Control layer menu; a companion install's first trace (no wait); the missing-file note.
  Test prompts are safe scenery with `scenery, no humans`; results cleared afterwards.

## C2. Work breakdown

| Milestone | Work |
|---|---|
| **M9 Depth control** | B1 engine kind and the hidden rule; B2 diffusion (kind, model, picture, extractor, feature); B3 core; B4 both apps; B5 companion; C1 tests; `03-design.md` §A6 and `names.md` brought up to date; checked on the tablet and the desktop |

## C3. M9 as built (2026-11-15)

- Built as designed. `Family::control_settings` takes the kind; the core's `depth_missing()`
  ("needs anima-lllite-depth-1.safetensors" / "needs comfyui_controlnet_aux", none when ready or
  not connected) greys + Depth and refuses a depth trace; a run leaves depth layers out when the
  server can't follow them, and only checks "Control layers" readiness when a line or scribble
  layer is sent. The companion downloads the tracers' models in `models anima` (after the licence,
  with the depth reader's own CC BY-NC 4.0 line), and `check` lists them.
- **Checked on the dev ComfyUI**: the depth reader landed at the path the companion pins
  (`ckpts/depth-anything/Depth-Anything-V2-Large/`); a cold first trace took 31 s (most of it the
  1.3 GB download), a warm one 3 s, and 2048 is no slower than 1024 for Large; a depth map traced
  from a test picture guided a base-model picture (the `#[ignore]` server test
  `a_depth_trace_comes_back_as_greys_and_guides_a_picture`).
- Tests: diffusion 83 (+5), core generation (+1 on the GPU: greys sent, clear counts as far, left
  out when not ready, the hidden rule), companion 34.

---

# Part D — Agreed choices

| # | Choice | Decided |
|---|---|---|
| 1 | Depth uses kohya's `anima-lllite-depth-1`, not the VACE model | 2026-11-15, after the tests |
| 2 | Pose waits for Claquasse's Preview-3; the editable figure stays the plan | 2026-11-15 |
| 3 | Hidden control layers keep guiding, every kind, as Krita | 2026-11-15 |
| 4 | The depth reader is Depth Anything V2 Large | 2026-11-15 (picked) |
| 5 | Transparent counts as far; a traced map is opaque greys | this design |
| 6 | The companion downloads the tracers' models at install | 2026-11-15 |
