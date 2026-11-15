# Phase 2: brainstorm

*Started 2026-11-06. Phase 2's first part is **style editing** (agreed order after Phase 1). The
rest of Phase 2 (proposal §8, the brainstorm's feature table: depth and pose control, upscaling,
and so on) gets its own section here when we come to it. Brainstorm first, then a proposal, then a
design, then code. Sketches: `mockups/style-editor.html`. Names: `names.md`.*

## Style editing

### Where it starts

A style is a preset the user picks from the cards at the top of the Generate popover: a name, a
model (checkpoint), a prompt template (`masterpiece, best quality, {prompt}`), a negative prompt, a
sampler, a scheduler, steps and CFG (`crates/diffusion/styles/anima.json`). Phase 1 ships two,
Anima and Anima Turbo, and lists one more for each other Anima model the server has. None can be
changed in the app, and there are no LoRAs.

What makes krita-ai-diffusion's styles worth having (the user's words: "this is what makes krita
ai diffusion great") is that everything can be customised, LoRAs above all. Its styles are edited
in a window of their own and kept as files.

### Decided (2026-11-06)

| Question | Answer |
|---|---|
| What can be edited | **Everything Krita's style editor has that Anima uses**: name, model, **LoRAs (each with a strength)**, prompt template with `{prompt}`, negative prompt, sampler, scheduler, steps, CFG, rescale CFG, VAE, text encoder, best size. Left out: clip skip, v-prediction and the live-painting sampler, which Anima doesn't use. (A seed was listed here at first, by mistake: Krita's styles have none, and the seed stays in the popover) |
| Where it's edited | **B, a full-screen Style Studio**, as Brush Studio is for brushes: someone working on a style doesn't need the canvas, and Krita edits styles in a window of its own. (A, a page inside the popover, was quicker but a long scrolling column with no room for a preview) |
| How you get there | **Hold a style card** in the Generate popover: your styles offer Edit, Duplicate, Delete; built-in ones only Duplicate |
| Built-in styles | **Read-only**; Duplicate makes an editable copy, so the defaults can always be gone back to and can improve in later versions |
| Simple or full settings | **The real settings, shown as they are**: steps is a number of steps, CFG a number, the sampler by name. ("Detail: Fast · Balanced · Fine", a plain-words stand-in for steps, was dropped: with a full Studio there's room for the real thing.) The rarer ones (scheduler, rescale CFG, VAE, text encoder, best size) sit in an **Advanced** section |
| Where styles live | **App-wide** (every artwork sees them), kept as small files, so they can be copied between the tablet and desktop |
| Krita's style files | **Import later, maybe** (reading its style files is reading data, not copying code) |
| The preview | **The user types their own preview prompt** in the Studio (it starts with a safe scene, "a lighthouse on a cliff, scenery, no humans", and is remembered) |
| A style's description | **None: the name is enough**, as in Krita; the people who make a style know what it's for. No About section |
| LoRA trigger words | **As Krita**: a LoRA can carry its own **trigger words** (and a default strength), kept with the LoRA, not the style. Whenever a style uses that LoRA, its trigger words are added **at the end of the user's prompt** (where Krita adds them; we put a comma before them, since Anima reads tags). Set in the Studio, on the LoRA's row |
| Where trigger words are kept | **On the server**, in ComfyUI's own storage for small files (its `userdata`), not in the app: one entry serves the tablet and the desktop and lives beside the LoRAs. (Krita keeps them on the one computer, in its own settings) |
| Filling them in | **Automatically where the file allows**: when a LoRA is added to a style, the app reads the file's metadata from the server (ComfyUI's `view_metadata`). A trigger phrase, if there is one, is filled in; otherwise the training tags it lists (kohya's trainer writes them, with how often each was used) show as a few chips to tap. The user checks and keeps them. A LoRA with no metadata (merges, some trainers) has them typed once. Asked only when a LoRA is first used, never for the whole folder up front |
| Filling many at once | **Later**: the companion, which runs beside the files, could look each LoRA up on Civitai by its file's fingerprint and fill in their trigger words in one go (needs the internet and Civitai's public lookup) |
| Sharing a style | **Export and Import of one style file, now if it's easy.** It looks easy: a style is one small file, and both apps already export and import brushes, so the same paths serve (Android's share sheet and file picker, the desktop's dialogs). If the design finds otherwise, it moves later |
| Desktop | **The same Studio**, full-window, as the desktop's Brush Studio |
| Best size | **Kept as Krita has it** (its "Preferred Resolution"), in Advanced: an **Override** switch, off by default, and one side in pixels (up to 2048). Off: the planner's own sizes for the family (for Anima, one pass up to 4.2 MP). On, say 1024: every picture is made at about 1024 × 1024 worth of pixels, then scaled up (with a second pass) or down to the target. For fine-tunes trained at another size than their family's; rarely needed for Anima, needed when more families come (which they will) |
| Family badge | **As Krita**: a small badge on each style card names the model family it's for (Anima today; SDXL, Illustrious, Flux and others when they come), and the same badge on each LoRA, so a LoRA for another family stands out. Our own badges, not Krita's icons |
| The file format | **TOML**: plain `name = value` lines a person can read and edit, with comments; one file per style, named after it (below) |
| Where the files are | **Where a person can see them** (an open-source app hides nothing). Android: **Documents › ProCraft Diffusion › Styles**, made by the app and confirmed once in Android's folder picker. Desktop: the app's own data folder, beside `brushes/`, with an **Open styles folder** button in the Studio. Both can be changed in a "Styles folder" setting (below) |

### The Style Studio, as sketched

- A screen of its own: Cancel and Done at the top, **sections on the left**, the chosen section in
  the middle, and a **preview** on the right.
- **LoRAs**: the style's LoRAs, each with a strength slider and ×; "+ Add LoRA"; and the LoRAs the
  server has, to add with a tap.
- **Preview**: the user's own preview prompt and **Try it**, which makes one small picture with
  the style as it is now, so a change (a LoRA's strength, say) can be seen without leaving.
- **Sections**: Model, LoRAs (with each LoRA's trigger words), Prompts, Sampling (sampler, steps,
  CFG), Advanced. No About.
- **Try it** makes the picture at **Draft** size. The seed stays the same between tries, so the
  change made is the only difference, with a **dice** beside Try it for a new seed (when a LoRA
  spoils the picture at that seed). Real testing happens on the canvas; Try it is a quick look.
- **An imported style whose model or LoRAs aren't on the server is kept**, and the Studio says what's
  missing.

### What real Anima LoRAs carry (checked 2026-11-06)

Three of the user's LoRAs, read through ComfyUI's `view_metadata`:

| LoRA | Metadata | The tag on every training picture |
|---|---|---|
| dark_art_style | kohya's trainer, 99 pictures | `dark_art_style` (99 of 99) |
| dragon_ball_classic | kohya's trainer, 285 pictures | `@dragonball` (285 of 285) |
| fire_emblem_3h | none | — |

None has a separate trigger-phrase field. But kohya's metadata says how many training pictures
there were (`ss_dataset_dirs`) and how often each tag was used (`ss_tag_frequency`), and **the
trigger word is the tag used on every picture**; ordinary tags (`1girl`, `solo`) are on fewer.
So the suggestion is exact where kohya's metadata is there: the tags on (nearly) every picture,
pre-filled, with the next most used as chips. A LoRA with no metadata gets its words typed once
(or, later, from Civitai through the companion).

### The files, readable by a person (decided 2026-11-06)

**Format: TOML**, a plain text file of `name = value` lines, which a person can read and edit in
any text editor, with `#` comments allowed. (JSON, which Krita uses, needs quotes and braces
everywhere and allows no comments.) A style:

```toml
# A ProCraft Diffusion style
name = "Moonlit"
model = "anima-base-v1.0.safetensors"

prompt = "masterpiece, best quality, {prompt}, night, moonlight"
negative = "worst quality, low quality, blurry"

sampler = "euler_ancestral"
steps = 24
cfg = 4.0

[[lora]]
file = "anima/dark_art_style_(zura_janai).safetensors"
strength = 0.7

[[lora]]
file = "anima/dragon_ball_classic_(_GAi).safetensors"
strength = 0.5

# Advanced, only when changed from the model's own
# scheduler = "normal"
# vae = "qwen_image_vae.safetensors"
```

One style, one file, named after it: **`Moonlit.toml`**. Export gives the same file; Import takes
one.

**Where the styles are kept**, in a folder a person can open:

| Platform | Folder |
|---|---|
| Android | **`Documents/ProCraft Diffusion/Styles`**, in the tablet's shared storage: seen in the Files app and from a PC over USB, kept after an uninstall, and usable by sync tools. On first use the app makes the folder and opens Android's folder picker inside it; the user taps "Use this folder" and "Allow" once (Android always asks; no manifest permission is needed, the same picker as ProCraft's Save to device). If the folder is later removed or moved, the app asks again. *To check on the tablet: that the picker opens inside the folder on the OnePlus Pad* |
| Linux | `~/.local/share/procraft-diffusion/styles/` (next to `brushes/`, as ProCraft keeps them) |
| Windows | `%APPDATA%\ProCraft Diffusion\styles\` (the same, as the app keeps `brushes\` there) |

On the desktop the folder is hidden by default, so the Studio has an **Open styles folder** button
that opens it in the file manager. A **Styles folder** setting on both platforms picks another
folder (one a sync tool watches, say). Export and Import of one style stay, for sending a style
to someone.

**The trigger words**, on the server: one file for all LoRAs, in ComfyUI's own user folder,
**`ComfyUI/user/default/procraft-diffusion/loras.toml`**, written and read through ComfyUI's
`userdata` API, so the app never needs the server's disk:

```toml
# Trigger words and default strengths for LoRAs, kept by ProCraft Diffusion

["anima/dark_art_style_(zura_janai).safetensors"]
trigger = "dark_art_style"
strength = 0.8

["anima/dragon_ball_classic_(_GAi).safetensors"]
trigger = "@dragonball"
```

### Next

The brainstorm for style editing is settled. Its proposal, `05-styles-proposal.md`, and its design,
`06-styles-design.md`, are final; next is M6.

## Depth and pose

*Started 2026-11-08. Two more kinds of control layer, beside Line art and Scribble.*

### Where it starts

- **Control layers today**: Line art and Scribble, added from the popover's Control part, each
  followed at one of four steps (Loose, Balanced, Close, Strict). Each row's **trace button** makes
  the layer from what's visible on the canvas, with a preprocessor on the server. The layer is an
  ordinary layer, so it can be painted on.
- **How they're applied**: Anima's LLLite control models, through the tooling add-on's nodes
  (`comfyui-tooling-nodes`). Line art and Scribble both use `anima-lllite-any-test-like-v2`.
- **The preprocessors are already installed**: `comfyui_controlnet_aux` has
  `DepthAnythingV2Preprocessor` (depth) and `DWPreprocessor` (pose: body, hands and face). Each
  downloads its own small model the first time it's used.
- **What a depth layer is**: a grey picture, **white near and black far**. A depth layer fixes
  the picture's shapes and space (a road going into the distance, a rock in front of a tree)
  without fixing its lines or colours.
- **What a pose layer is**: a coloured stick figure on black, one colour per limb, with dots for
  the face and hands (DWPose's own drawing). It fixes where a figure's head, arms and legs go.
- **Krita** (behaviour only): Depth and Pose are kinds of control layer there too, made from the
  picture with the same "from the image" button. Its Pose layer is a **vector layer**: the figure's
  joints can be dragged afterwards and new figures added.

### What models there are (researched 2026-11-08)

Two for each. All four carry Anima's non-commercial licence, like Anima itself; we never ship
them, so nothing changes there.

**Depth**

| | kohya `anima-lllite-depth-1` | TaihoC Anima-ControlNet-VACE-Depth |
|---|---|---|
| What it is | an LLLite: a small add-on to the model's attention, 8 MB | a VACE ControlNet: copies of some of the model's blocks, 596 MB |
| Trained on | Anima **Preview3**, 2,000 pictures; works on 1.0 "with somewhat reduced quality" | Anima **1.0**, 20,000 steps at 1024², a mix of depth sources |
| Runs with | what we already use (the tooling add-on's control nodes, as Line art) | one person's fork of ComfyUI-Advanced-ControlNet, on a side branch, its Anima nodes undocumented (7 stars) |
| Input | white near, black far (Depth Anything V2) | Depth Anything V2 suggested |
| Said of it | "intentionally moderate"; one user: "works 50/50" | users: "much better than LLLite", "works so great"; no numbers |

**Pose**

| | kohya `anima-lllite-pose-1` | Claquasse Anima Control Pose Preview-2 |
|---|---|---|
| What it is | an LLLite, 23 MB | a control-LoRA: the skeleton goes in beside the picture, plus a LoRA on the model, 75 MB |
| Trained on | Anima **Preview3**, 1,544 pairs | Anima **1.0**, more than 3,900 pairs, at 512, 768 and 1024 |
| Runs with | what we already use | two node folders copied from its Hugging Face page (no GitHub), plus the usual LoRA loader; its photo detector adds Python packages and a 330 MB download, but we'd only need the apply node if we draw the skeleton ourselves |
| Input | DWPose's drawing: coloured limbs, white face points, hand points | a "thin" skeleton, hands and face left out |
| Said of it | by its author: "noticeably weaker", a soft hint, not a lock | measured by its author: a held-out pose followed **83 %** of the time at 1024 (37 % with it off); misses dynamic poses (running, sitting) and mushes busy hands; strength 0–2; "experimental", Preview-3 coming |

In short: the kohya pair costs nothing to add and is what Krita uses, but was made for an older
Anima and is weak, pose especially. The newer pair is made for Anima 1.0 and is said to be
clearly better, but each brings someone else's add-on that the companion would have to install
for everyone, and both are one person's experimental work.

### Questions

1. **Which models.** To be settled by **trying them on the dev ComfyUI** with test pictures
   (below), since only the newer pose model has any numbers. The plan: the kohya pair first
   (nothing to install); then Claquasse's pose model (its apply node only), since pose is where
   the gap looks widest and an editable figure is worth building only for a control that holds;
   the VACE depth only if kohya's depth disappoints, its fork being the riskiest add-on.
2. **How a depth layer is made.** Traced from what's visible (the trace button, as Line art), and,
   since it's an ordinary layer, **painted on in greys** (lighter is nearer). **Picked**, with a
   short hint in the row ("lighter is nearer").
3. **How a pose layer is made: decided 2026-11-08, as Krita**: an editable figure, joints dragged
   with a finger, figures added and removed, as well as traced from what's visible. Since the app
   keeps the joints, not a picture, it draws the skeleton itself in whichever style the chosen
   model wants (DWPose's colours for kohya's, the thin one for Claquasse's).
4. **Test pictures for pose: decided 2026-11-08**: **a robot** ("a red robot standing, full body,
   simple background"), the **base model, not Turbo**, the results cleared afterwards and not
   looked at beyond what the test needs. The pose to follow is a plain stick figure.
5. **Where they're added**: the Control part's add menu gains **Depth** and **Pose** beside Line
   art and Scribble, with the same four steps (each kind tuned on its own). **Picked.**

### Checked again after ProCraft 0.0.5 and 0.0.6 (2026-11-15)

The two merges change nothing on the server side, so the models and the test plan stand. They add
two things the pose layer's design can lean on:

- **A layer can keep data of its own beside its pixels** (`PluginLayer`: saved as
  `layers/<id>.layerdata`, and brought back with the pixels on undo). It's made for plugins' kinds
  of layer, such as the new vector layers, whose items are brush strokes, not joints, so a pose
  layer wouldn't be a plugin layer. But a pose layer can keep its figures the same way: the joints
  as its data, the skeleton drawn from them into its pixels.
- **Nodes dragged on the canvas**: QuickShape's curves now show their anchors and handles to drag,
  on both apps. The pose figure's joints can look and handle the same way.

### The tests (2026-11-15)

On the dev ComfyUI: the base model, 1024 × 1024, 30 steps, two seeds each, against the same seeds
with no control. The control pictures were drawn by a script, not traced, so no preprocessor was
needed: a depth map (a near boulder low on the left, a tower in the middle distance on the right,
ground growing nearer towards the bottom), and a stick figure on the left third with its arms up in
a Y and its legs wide, drawn in each model's own style (DWPose's colours for kohya's, the thin one
for Claquasse's). Claquasse's model was added as a LoRA (`models/loras/`) with its apply node in
`custom_nodes/anima_control_lora`.

| Model | Result |
|---|---|
| kohya depth-1, Balanced (0.6, to 60 %) | **follows the map**: the tower moves to where the map has it, and the boulder down to the left; the picture stays as good as with none |
| kohya depth-1, Close and Strict | the same, a little closer (the boulder's size and place) |
| kohya pose-1, Balanced and Strict | **no visible effect**: arms down, legs together, as with none |
| Claquasse Preview-2, strength 1.0 | **the legs wide** as drawn; the arms not raised (the robot held a spear across instead) |
| Claquasse Preview-2, strength 1.5 | legs wide and **arms raised** on one seed, half raised on the other, with a stranger body |

Neither pose model put the figure on the left third, where it was drawn: both centred it.

What it means for question 1: **kohya's depth model is good enough** (no need to try the VACE one
and its fork), and **kohya's pose model is not worth a button**. Claquasse's pose model is the only
one that does something, though it holds a pose only partly, and needs its apply node installed by
the companion. An open question for the proposal: whether a partial pose control is worth building
the editable figure for now, or whether pose waits for Claquasse's Preview-3.

**Decided 2026-11-15: depth now, pose later.** Depth goes ahead with kohya's depth-1. Pose waits
for Claquasse's Preview-3, tested the same way when it comes; the editable figure (question 3) is
still how it'll be made. Claquasse's Preview-2 and its apply node stay on the dev ComfyUI for that.

### Next

The depth proposal, `07-depth-proposal.md`.
