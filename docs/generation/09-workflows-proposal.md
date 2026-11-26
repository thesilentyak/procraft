# Workflows from ComfyUI: proposal

Status: **final** (agreed 2026-11-26: "it should effectively work like Krita's version").
Decided: the **live link** first (a list of the workflows saved in ComfyUI later, as a second
step), the **Krita marker nodes** (the ones krita-ai-diffusion's custom graphs use), and Krita's
**import of workflow files** and **layer nodes** in this first part. The design
(`10-workflows-design.md`) turns it into screens and APIs. Names: `names.md`.

---

## 1. Summary

Any ComfyUI workflow, with any model, becomes something the Generate popover can run. In ComfyUI
on the PC you add a few **marker nodes** to a workflow: where the canvas goes in, which values the
app may set, and where the result comes out. The tooling add-on the app already needs
(comfyui-tooling-nodes) sends the open workflow to the app as soon as it has a **Krita Output**
node, and again on every change. The app shows it as a style card, **From ComfyUI**, with a
control for each of its parameters, and runs it like any other style: results land in the results
tray and are applied as layers.

## 2. Goals

1. **Any model, any workflow**: SDXL, Flux, Z-Image, Qwen, upscalers, whatever the PC runs, without
   waiting for the app to learn each model family.
2. **Edit on the PC, paint on the tablet**: change the workflow in ComfyUI and the app has the new
   version a moment later, with nothing to export or copy.
3. **The same painting loop**: the same popover, results tray, Apply, Again and undo; a selection
   still limits where a result goes.
4. **Krita's workflows work as they are**: the same marker nodes, so custom graphs made for
   krita-ai-diffusion run in ProCraft too.
5. **Nothing new on the server**: the marker nodes and the sending are in the add-on we already
   require.

## 3. Non-goals for now

- **A list of the workflows saved in ComfyUI** to pick from in the app. It needs the app to turn
  ComfyUI's saved format into the one it runs, which is the hard, fragile part; the live link gets
  it for free. Second step.
- Editing a workflow in the app (that's ComfyUI's job).
- The rarer marker nodes: Krita Style (the app's style as model and prompt) and Send Text (text
  shown in the app); the Output node's batch modes and "resize canvas". Later, one by one.
- Regions, Live painting, animation.
- A phone layout for the popover: its own project after this (the user finds it cramped on a phone).

## 4. What the user gets

**On the PC**, in a workflow in ComfyUI:

- **Krita Output** (required): what reaches it comes back to the app as results.
- **Krita Canvas** (optional): gives the workflow the visible picture, its width and height, and
  the seed.
- **Krita Selection** (optional): the selection as a mask, so the result is placed inside it.
- **Parameter** (optional, any number): a value the app sets: text, a prompt, a number with a
  range, a switch, or a choice from a list.
- **Krita Image Layer** and **Krita Mask Layer** (optional): a layer of the artwork, picked in the
  app, as a picture or a mask (a reference picture, a hand-painted mask).
- The Output node's **name** becomes the applied layer's name, and its **offset** places the result.

As in Krita, a workflow can also be **exported from ComfyUI as a file** ("Export (API)") and
**imported** into the app, and shared that way.

**In the app**:

- A **From ComfyUI** card among the style cards once a workflow has arrived. It holds the newest
  one received and keeps it after ComfyUI's tab is closed or the app restarts. Before any has
  arrived, the card says how to send one (open a workflow with a Krita Output node in ComfyUI).
- **Keep…** on the card's hold menu saves the workflow as it is now under a name, as a card of its
  own, so a few favourites stay at hand while the live card follows whatever is open on the PC.
- **Import…** on the + card takes a workflow file (ComfyUI's API format) as a kept card too, and a
  kept card's hold menu has **Export** to share it.
- With the card chosen, the popover's middle shows **the workflow's parameters** instead of the
  style's own settings: a prompt field (with tag suggestions) for each prompt parameter, a text
  field, slider, switch or list for the rest, in the order of their names. Count and seed stay.
- **Generate** runs it. Progress, previews, cancel, the results tray, Apply and Again work as now.
- If the server lacks a node or a model the workflow uses, ComfyUI's own message is shown, naming it.

### An example

A Z-Image Turbo workflow on the PC: Krita Canvas into the latent size, a "Prompt" parameter into
the text encoder, Krita Output at the end. In the app: choose **From ComfyUI**, type "koi pond in
the rain", Generate; four results in the tray, one applied as a layer.

## 5. How it works (outline for the design)

- **Receiving**: on connecting, the app asks the add-on to send it workflows
  (`/api/etn/workflow/subscribe` with its WebSocket client id) and listens for
  `etn_workflow_published`, which carries the workflow already in the form ComfyUI runs.
- **Running**: before queuing, the app replaces Krita Canvas with the picture and numbers (the same
  base64 image node Anima's graphs use), Krita Selection with the mask, sets each Parameter's value,
  and turns Krita Output into the image-sending node it already listens to. All of it in a new
  module of `crates/diffusion`; the popover's parameter controls in the generation UI files on both
  platforms, keeping the hooks into ProCraft's own files thin.
- **Licence**: krita-ai-diffusion and the add-on are GPL-3.0. We copy neither; we only follow their
  documented behaviour and talk to the add-on over the network, as the rest of the app does.

## 6. Settled for the design

- Kept workflows are stored **on the device**, beside the styles (simplest; Export and Import move
  them between devices).
- A "choice" parameter's list comes from the input it feeds, which ComfyUI describes.
