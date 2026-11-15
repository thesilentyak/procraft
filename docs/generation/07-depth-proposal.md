# Depth control: proposal

Status: **final** (agreed 2026-11-15). Built from the depth-and-pose part of `04-phase2-brainstorm.md`
(depth now, pose later: decided 2026-11-15, after the tests there). The design turns it into
screens and APIs. No product code before the design. Names: `names.md`.

---

## 1. Summary

A third kind of control layer, **Depth**, beside Line art and Scribble. A depth layer is a grey
picture, **lighter nearer and darker further**, that fixes the picture's space (what stands in
front of what, where the ground goes) without fixing its lines or colours. It's made the way the
other kinds are: added empty from the popover's Control part and painted in greys, or **traced**
from what's visible with the row's trace button. It uses kohya's `anima-lllite-depth-1`, applied
with the nodes Line art already uses, so the server needs one more small file (8 MB) and nothing
else.

## 2. Goals

1. **Shape the space, not the lines**: block in a near rock and a far tower with a few grey
   strokes, or keep a painting's layout while the picture is made again.
2. **The same as the other control layers**: the same rows, trace button, four steps and menus;
   nothing new to learn. A hidden control layer of any kind keeps guiding (§6).
3. **Paintable**: an ordinary layer, painted with any brush in greys.
4. **Nothing new on the server** but the model file (and the tracer's own model, fetched once).

## 3. Non-goals for now

- **Pose**: waits for Claquasse's Preview-3 (brainstorm, "The tests"). When it comes it gets its
  own proposal, with the editable figure as decided.
- The VACE depth model and its add-on (kohya's is good enough, as tested).
- Depth from 3D scenes, or tools to paint depth (a "nearer / further" brush): greys and ProCraft's
  own brushes are enough to start.

## 4. What the user gets

- **+ Depth** in the popover's Control part, after + Line art and + Scribble: an empty layer named
  "Depth", made current, ready to paint on. **Depth** also in the Layers panel's Control layer menu
  (None · Line art · Scribble · Depth), for a layer already painted.
- **Painting depth**: white is nearest, black furthest; **what's transparent counts as far** (as
  black), so a single white blob on an empty depth layer means "something near here".
- **Trace**: the row's trace button reads depth from what's visible (the whole canvas, or inside
  the selection), with Depth Anything V2, and the greys replace the layer's pixels there, one undo
  step, as for the other kinds.
- **The four steps** (Loose · Balanced · Close · Strict), **Balanced** by default. In the tests,
  Balanced already moved a tower and a boulder to where the map put them, with no loss of quality;
  the steps' numbers are tuned for depth on their own if needed.
- **A short hint** in the row the first times: "lighter is nearer".
- **Without the file**, + Depth says what's missing ("needs anima-lllite-depth-1"), as other
  missing models do; the companion downloads it with the rest.

## 5. How it's built (outline)

- `ControlKind::Depth` in the engine (saved with the artwork as the others are) and in
  `crates/diffusion`; the model pattern `anima*lllite*depth*`; the tracer
  `DepthAnythingV2Preprocessor` in `extract.rs`, its greys kept as they come (not turned into
  alpha, as line art's are).
- The graph: the same `ETN_control_load` / `ETN_control_apply` pair, with the depth file.
- Both apps: the + Depth button, the menu row, the badge or caption ("Depth control").
- The requirements check and the companion's model list gain the file.

## 6. Decided

1. **Hidden control layers still guide, as in Krita** (decided 2026-11-15, option A). A traced
   depth layer is opaque greys over the whole canvas, so it's hidden to see the painting; hiding
   it must not switch it off. So, for **every kind** (Line art and Scribble too): the eye in the
   Layers panel only hides a control layer from view, and it keeps guiding. To stop one guiding,
   × in the popover (or Control layer › None) makes it a plain layer again. This changes what
   Phase 1 built (`03-design.md` §A6: "Hidden control layers are ignored"); the design says how
   the popover's row shows a hidden one. (Turned down: B, depth only, where the eye would mean
   different things for different kinds; C, a traced depth layer at low opacity, a grey veil over
   the painting.)
2. **The tracer: Depth Anything V2 Large** (picked). The server reads depth with a small AI model
   of its own, downloaded once (1.3 GB) the first time anyone traces a depth layer; the smaller
   ones (99 MB, 390 MB) give rougher maps, and kohya trained the depth model on this one's maps.

## 7. Milestone

**M9 Depth control**: the kind in the engine, `crates/diffusion` and both apps; trace; the steps
tuned on the dev server; the requirements check and the companion's list; checked on the tablet
and the desktop.
