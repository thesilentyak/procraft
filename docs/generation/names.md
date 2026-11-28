# Names

What we call the parts of image generation, so the app, the docs, the code and our conversations
use the same words. **Bold** is the name; "quoted" is the exact text shown in the app. Add a line
when something gets a name, and change it here first when a name changes.

## On the canvas screen

| Name | What it is | Shown as |
|---|---|---|
| **AI button** | The top-bar button that opens the Generate popover; a progress ring goes round it while a run is going. Desktop: G. | the "AI" icon |
| **Generate popover** | The popover under the AI button: style, prompt, control layers, strength, resolution, seed, count, Generate. | "Generate" |
| **Server view** | The popover's own settings page: the server address, what it has, NSFW, Largest picture. Opened from the server button in the popover's title. | the server's address |
| **Results tray** | The bar at the bottom with this artwork's results: thumbnails, Again, Apply, ··· and ⌄. Tucks away into its tab, never closes. *(Named 2026-11-06; was "the results bar". Being redesigned: `mockups/results-tray.html`.)* | — |
| **Tray tab** | The small tab the results tray tucks into, at the bottom centre: the shown result's thumbnail and the count, or the progress while generating. | "Results · 4", "Generating · 63%" |
| **Preview** | A result shown over the canvas (not yet a layer), from a tap on its thumbnail. | — |

## In the Generate popover

| Name | What it is | Shown as |
|---|---|---|
| **Style** | A card at the top: a named set of model, prompt words and settings. | e.g. "Anima" |
| **Prompt** | The text field; tags or plain words. | "Prompt" |
| **Suggestions** (the **suggestion strip**) | The chips under the prompt: tags for the word being typed, or recent prompts when it's empty. | — |
| **Recent prompts** | The last 20 prompts, offered when the prompt is empty. | — |
| **Control part** | The popover's Control section: a row per control layer, then the add buttons. | "Control" |
| **Control layer** | A layer the picture follows rather than shows. Three kinds (Depth from M9). Hidden, it still guides. | "Line art", "Scribble", "Depth" |
| **Depth** (a control layer's kind) | Greys that set what's near (white) and far (black); transparent counts as far. | "Depth", "Depth control", "lighter is nearer" |
| **Depth reader** (docs only) | The server's model that traces depth from a picture: Depth Anything V2 Large. Never named in the app. | — |
| **Trace button** | On each control row: fills that layer with lines traced from what's visible. | a line with a sparkle |
| **Steps** | How closely a control layer is followed. | "Loose", "Balanced", "Close", "Strict" |
| **Strength** | How much a run changes: 100 % makes a new picture; less refines what's there. | "Strength", "New image" |
| **Generation resolution** | How big the picture is made. | "Draft", "Normal", "Quality" |
| **Mode** (selection mode) | What a run does inside a selection. | "Fill", "Add", "Remove", "Expand" |
| **Seed** | The number a run starts from; the same seed and settings give the same picture. | "Seed" |

## In the results tray

| Name | What it is | Shown as |
|---|---|---|
| **Thumbnail** | One result. Tap: preview it (again: hide it). Hold: its Delete. | — |
| **Again** | Runs the same settings with a new seed (the fixed one if locked). | ↻ |
| **Apply** | Adds the shown result as a new layer at the top. | "Apply" |
| **More menu** | ···: put the shown result's seed in the popover (locked), clear all results. | "Use the seed 18234", "Clear all results…" |
| **Tuck** | Puts the tray away into its tab (also a tap on the canvas, or Esc). | ⌄ |

## Styles (Phase 2: `05-styles-proposal.md`, `06-styles-design.md`)

| Name | What it is | Shown as |
|---|---|---|
| **Built-in style** | A style that comes with the app (Anima, Anima Turbo): read-only, can be duplicated. | — |
| **Style Studio** | The full-screen editor for a style, as Brush Studio is for brushes. Opened by holding a style card → Edit. | "Style Studio" |
| **Family badge** | The small badge on a style card and a LoRA naming the model family it's for. | "Anima" |
| **Best size** | A style's override for the size its model makes pictures at (Krita's "Preferred Resolution"): off by default, then one side in pixels. In a style's Advanced section. | "Best size", "Override" |
| **LoRA** | A small add-on model a style uses, with a strength, to push a look or a character. | "LoRAs" |
| **Trigger words** | Words a LoRA needs to take effect, kept with the LoRA and added to the prompt whenever a style uses it. | "Trigger words" |
| **Advanced** | The Studio's section with the rarer settings (scheduler, rescale CFG, VAE, text encoder, best size). | "Advanced" |
| **Preview prompt** | The prompt the Studio's Try it uses, typed by the user. | — |
| **Try it** | The Studio's button that makes one small preview picture with the style as it is. | "Try it" |
| **Tries** | The Studio's last four Try it pictures, as thumbnails under the big one, to compare a change with the one before. Kept until the Studio closes. | — |
| **Filled in from the file** | Trigger words the app read from a LoRA's own training notes, for a LoRA without any kept on the server; the user checks them. | "Filled in from the file: check them" |
| **Tag chips** | The LoRA's other most used training tags under its Trigger words field, added with a tap. | — |
| **Export** | Hands one of the user's style files to another app or a folder (share sheet, save dialog). | "Export…" |
| **Import** | Copies a style file from elsewhere into the styles folder, after checking it reads. | "Import" |
| **Plus card** | The "+" card at the end of the style strip: a new style, opened in the Studio. | "+", "New style" |
| **Styles folder** | The folder a person can open where styles are kept as `.toml` files: Documents › ProCraft Diffusion › Styles on Android, the app's data folder on the desktop. Changed in the server view's Styles folder row. | "Styles folder", "Open styles folder" |

## Settings and set-up

| Name | What it is | Shown as |
|---|---|---|
| **NSFW switch** | Whether NSFW tags are suggested. Never changes what is generated. | "Show NSFW tags" |
| **Largest picture** | The most megapixels a picture is made at, from the GPU's memory unless changed. | "Largest picture" |
| **Companion** | The CLI that installs and runs a ComfyUI for the app (M5). | `procraft-companion` |
| **Dev ComfyUI** | The ComfyUI on this machine used for development (port 8189). Not a user-facing name. | — |

## Workflows from ComfyUI (`09-workflows-proposal.md`, `10-workflows-design.md`)

| Name | What it is | Shown as |
|---|---|---|
| **Workflow** | A ComfyUI workflow with Krita marker nodes, run from the Generate popover instead of a style. | the "Workflow" badge |
| **Live workflow** | The newest workflow the PC's ComfyUI tab sent; its card follows every change there. | "ComfyUI (live)" |
| **Kept workflow** | A workflow saved as a file in the styles folder's Workflows folder: kept from the live one, or imported. | its file's name |
| **Marker nodes** (docs only) | The add-on's nodes that mark where the app's inputs go and results come out: Krita Canvas, Krita Selection, Krita Output, Parameter, Krita Image Layer, Krita Mask Layer. | — |
| **Parameters** (the **Parameters part**) | The popover's part with a row per Parameter or layer node of the chosen workflow. | "Parameters" |
| **Keep** | Saves the live workflow as a kept one, under a name. | "Keep…" |

## Pixel art animation (`docs/pixel-art/`)

| Name | What it is | Shown as |
|---|---|---|
| **Sprite sheet** (export) | Share Layers' sheet of the frames, a row per animation, with Aseprite's JSON beside it. | "Sprite Sheet" |
| **Animation** (named) | A run of frames with a name, from Frame options or an import; a coloured band on the strip. | "Start Animation Here…" |
| **Insert a sprite sheet** | Actions › Add: a sheet picture from anywhere read into clean, lined-up frames as a named animation. | "Insert a sprite sheet" |
| **Import sheet** (docs only) | Its window: the sheet with each frame boxed, the frames playing, Name, and Not right? (Grid, Background). | "Import 8 frames" |
| **In-between with AI** | Frame options: fills the middle frames between this frame and the next using MiniMax H3 First & Last Frame. | "In-between with AI…" |
