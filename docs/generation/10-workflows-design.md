# Workflows from ComfyUI: design

Status: **final** (agreed 2026-11-26). From `09-workflows-proposal.md` (final).
Names: `names.md`. Built as milestones W1–W6 (Part C).

---

# Part A — Experience

## A0. The rule for this feature

The user (2026-11-26): "maximum functionality without a million different options to select
through". So: **everything works with nothing chosen**. Each choice has a sensible default picked
for the user; only the prompt is in view; anything rarer is folded away. A workflow that needs
setting up before it can run is a bug in the workflow, not a question for the user.

## A1. In the Generate popover

- **Workflow cards** sit in the style row, after the styles and before the + card, each with a
  small **Workflow** badge (where a style card has its family badge):
  - **ComfyUI (live)**: the newest workflow the PC sent. Always shown once a server is set. Until
    one arrives its card reads "Open a workflow with a Krita Output node in ComfyUI"; tapped, the
    popover's middle says the same, with the three steps (add Krita Output, open it, wait a moment).
  - **Kept workflows**: one card per workflow file in the styles folder's **Workflows** folder,
    named by the file.
- **Hold menu**: on the live card **Keep…** (asks for a name, writes the file, makes its card).
  On a kept card **Rename…**, **Export**, **Delete…** (the same sheets the style cards use).
- **+ card menu**: **Import…** also takes a `.json` workflow file in ComfyUI's API format (the
  format check decides: a style is TOML, a workflow JSON). It lands as a kept card.
- **With a workflow card chosen**, the popover keeps its title, the card row, **Count**, **Seed**
  and **Generate**. Prompt, strength, resolution, the Control part and the selection mode are
  replaced by the **Parameters** part, which shows **only the prompt** (each prompt parameter,
  positive first). Every other row (numbers, switches, lists, layer pickers, a negative prompt) is
  folded under **More**, closed by default and in the order of the nodes' names (Krita's order),
  labelled with the node's name. A workflow with no prompt parameter shows just Generate.
- **Starter workflows** come ready in the Workflows folder (W7), made for the models on the user's
  PC, so there is something to pick before any workflow is built in ComfyUI.

## A2. Parameter rows

| Node and type | Row |
|---|---|
| Parameter, `prompt (positive)` | The prompt field, with the suggestion strip and recent prompts |
| Parameter, `prompt (negative)` | A plain two-line text field |
| Parameter, `text` | A one-line text field |
| Parameter, `number` | A slider when min and max are both set (the node's defaults ±1e10 mean "not set"), else a number field; the value can be typed, as in the Studio |
| Parameter, `number (integer)` | The same, in whole steps |
| Parameter, `toggle` | A switch |
| Parameter, `choice` | A list of the options of the input it feeds (from the server's node info) |
| Parameter, `auto` | Decided by the input it feeds: INT, FLOAT, BOOLEAN, a list, or text (multiline text: a prompt) |
| Krita Image Layer | A layer picker: the layer with the node's name if there is one, else the current layer |
| Krita Mask Layer | The same picker, the layer's alpha used as the mask |

- Each row starts at the node's default. Values are **remembered per workflow** (by the workflow's
  name, the live one as "live") across restarts, as the popover's prompt and style are.
- Layer pickers choose for the user: the layer named like the node, else the current layer.
- **Back to defaults**, at the bottom of More, puts every row back.

## A3. A run

- **Generate** runs the workflow Count times, the seed going up by one each time (Krita Canvas's
  seed output gets it). Progress, sampling previews, cancel and the results tray work as now.
- **What goes in**:
  - Krita Canvas: the visible picture (control layers left out, as for Anima), its width, height,
    the seed, and the selection as its mask (all white without one).
  - Krita Selection: the selection's mask, whether there is one, and its offset, following the
    node's `context` (entire image, mask bounds, automatic) and `padding`.
  - Image and Mask Layer: the picked layer at canvas size.
- **What comes out**: every image reaching a Krita Output is a result, in the order the server
  sends them. **Apply** makes a layer named by the Output node's `name` (else the first prompt
  parameter, as now), placed at its `x`, `y` offset; with a selection, masked to it as now.
- **Errors**: the server lacks the tooling add-on → "The server needs comfyui-tooling-nodes";
  the workflow uses a node or model the server hasn't got → ComfyUI's own message, naming it,
  in the popover's error line (as for a style's missing files).

## A4. Desktop

The same cards, rows and menus in the egui popover. Import and Export through the system's file
dialogs, as for styles.

---

# Part B — Architecture

## B1. Pieces

| Where | What |
|---|---|
| `crates/diffusion/src/workflow.rs` (new) | `Workflow` (name, API-format JSON), `parameters()`, `prepare()`: everything about marker nodes, no UI, no GPU |
| `crates/diffusion/src/comfy.rs` | `subscribe_workflows(client_id)`; the socket reads `etn_workflow_published` |
| `crates/diffusion/src/worker.rs` | Keeps a socket open while connected to receive workflows; `submit_workflow()`; a run can return several pictures |
| `crates/core/src/generation_workflows.rs` (new) | The UniFFI API: workflow cards, parameters, kept files, runs |
| `android/.../generation/GenerationWorkflows.kt` (new) | The cards and Parameters part in Compose |
| `desktop/src/generation_workflows.rs` (new) | The same in egui |

Hooks into existing files stay a line or two: the style row draws the workflow cards after the
styles; the popover asks which part to show; `lib.rs` lists the new modules.

## B2. Receiving the live workflow

- On connecting, the worker opens its WebSocket (client id as now) and posts
  `/api/etn/workflow/subscribe {client_id}`. The add-on answers at once with any workflow it holds,
  then sends `etn_workflow_published {publisher, workflow}` on every change in ComfyUI's tab.
- Between runs the worker reads that socket; during a run it keeps reading it as now, and a
  workflow event in the middle of a run is kept, not dropped.
- `ServerState` gains `live_workflow: Option<(u64 /* revision */, serde_json::Value)>`. Core turns
  a new revision into the live card and hands its text to the app, which keeps it in its private
  storage (`live-workflow.json`) and gives it back on start (`set_live_workflow`), so the card works
  before the PC sends anything again.
- If the server has no `/api/etn/workflow/subscribe` (old add-on), the live card says the add-on
  needs updating; kept and imported workflows still run.

## B3. `workflow.rs`

```rust
pub struct Workflow { pub name: String, graph: serde_json::Map<String, Value> }
pub enum ParamKind { Prompt, NegativePrompt, Text, Number { min, max, integer: bool }, Toggle, Choice(Vec<String>), ImageLayer, MaskLayer }
pub struct Param { pub node: String, pub name: String, pub kind: ParamKind, pub default: Value }
pub enum ParamValue { Text(String), Number(f64), Toggle(bool), Layer(Picture) }
pub struct Inputs { picture: Picture, selection: Option<SelectionInput>, seed: u64, values: BTreeMap<String, ParamValue> }
pub struct Output { pub node: String, pub name: String, pub offset: (i32, i32) }

impl Workflow {
    pub fn parse(name: &str, text: &str) -> Result<Workflow, Error>;      // API format; refuses the UI format with a clear message
    pub fn parameters(&self, info: &NodeInfo) -> Vec<Param>;              // sorted by name; `auto` and `choice` resolved from `info`
    pub fn outputs(&self) -> Vec<Output>;
    pub fn prepare(&self, inputs: &Inputs) -> Result<Graph, Error>;       // the graph to queue
}
```

`prepare` copies the graph and:

- **Krita Canvas** → an `ETN_LoadImageBase64` and an `ETN_LoadMaskBase64` node; every input linked
  to its width, height or seed output gets the number itself (the API format allows a value where
  a link was).
- **Krita Selection** → `ETN_LoadMaskBase64` (the mask cut to the node's context and padding);
  `active`, `x`, `y` become values the same way.
- **Parameter** → its value, typed as the parameter's kind (a number, `true`/`false`, text), written
  straight into every input linked to it; the node is left with nothing to feed and doesn't run.
  (The add-on's own Parameter node passes a switch on as the text "true", which a BOOLEAN input
  can't take, so it can't be left to do it.)
- **Image Layer** → `ETN_LoadImageBase64` + `ETN_LoadMaskBase64` (its alpha); **Mask Layer** →
  `ETN_LoadMaskBase64`.
- **Krita Output** → `ETN_SendImageWebSocket` (PNG) on the same images input.
- A workflow is numbered again from 1 when read (`Graph::from_api`, links renumbered with it), so
  the nodes the app adds take the next ids and never collide.
- With a selection and a Krita Selection node not set to "entire image", the workflow's canvas is
  the selection's bounds plus the node's padding (Canvas's picture, width and height are that
  part), and results are placed there.

## B4. Runs with several pictures

`follow` gathers every PNG the socket sends until the prompt finishes (today it takes one). A
workflow run's pictures are matched to Output nodes by their order (ComfyUI runs output nodes in
id order; the add-on sends each node's images in turn). Anima runs keep taking the first, as now.

## B5. The core API (UniFFI)

```rust
pub struct WorkflowCard { id: String, name: String, live: bool, ready: bool, reason: Option<String> }
pub struct ParameterInfo { key: String, name: String, kind: ParameterKind, default: String, min: Option<f64>, max: Option<f64>, choices: Vec<String> }
pub struct WorkflowRun { workflow: String, values: Vec<ParameterValue>, seed: Option<u64>, count: u32 }

impl Generation {
    fn workflow_cards(&self) -> Vec<WorkflowCard>;
    fn workflow_parameters(&self, workflow: String) -> Vec<ParameterInfo>;
    fn set_workflow_files(&self, files: Vec<WorkflowFileText>);           // the Workflows folder, read by the app
    fn set_live_workflow(&self, text: Option<String>);                   // from the app's storage at start
    fn live_workflow_text(&self) -> Option<String>;                      // to store when it changes
    fn keep_workflow(&self, name: String) -> Result<WorkflowFileText, GenerationError>;
    fn check_workflow_file(&self, text: String) -> Result<String, GenerationError>; // Import: the name, or why not
    fn generate_workflow(&self, session: &CanvasSession, run: WorkflowRun) -> Result<u64, GenerationError>;
}
```

The app writes, renames and deletes the files, as for styles. `GenerationState` reports results
the same way; a result carries its layer name and offset.

---

# Part C — Plan

## C1. Milestones

| | What | Checked by |
|---|---|---|
| W1 | `workflow.rs`: parse, parameters, prepare, outputs (**built 2026-11-26**) | Unit tests on fixtures (a Z-Image text-to-image graph, an inpaint graph with Selection, one with each parameter type) |
| W2 | Worker: subscribe, the live workflow, runs with several pictures (**built 2026-11-26**) | Unit tests on recorded socket messages; the user's server |
| W3 | Core API, kept files, the live workflow kept between starts (**built 2026-11-26**) | Core tests |
| W4 | Desktop popover | The desktop app on the user's PC |
| W5 | Android popover (**built 2026-11-26**; kept workflows are `.json` files in the styles folder itself, beside the styles, not a Workflows folder below it; the popover's prompt field is the workflow's first prompt) | The APK from GitHub Actions on the user's phone |
| W6 | Import and Export, layer pickers (**built on Android 2026-11-26**) | Both apps |
| W7 | Starter workflows for the user's own models (text to image, refine, selection fill), with only a prompt in view (**first two 2026-11-26**: Z-Image Turbo "new picture" and "paint over", in `dev/starters/`, made by `make_starters.py` at about 1 MP, the app scaling the result to the canvas; then MiniMax H3 "new picture" and "edit"; and "Pixel art (RD Flow 64)": Retro Diffusion's 64 × 64 model and node pack, Qwen3-VL 4B writing the long description it wants from the prompt, the sprite scaled up with hard edges and padded, transparent round it) | The user's server and phone |

## C2. Testing

- Fixtures are API-format graphs exported from the user's ComfyUI, checked into
  `crates/diffusion/tests/workflows/` (no personal paths in them).
- Each milestone passes `Building and checking` in `CLAUDE.md` and the GitHub build.
- Test generations use safe subjects (`scenery, no humans`), as the hard rules say.

## C3. Decisions

| Date | Decision |
|---|---|
| 2026-11-26 | Live link first; saved-workflow list later (proposal) |
| 2026-11-26 | Krita's marker nodes, file import and layer nodes, so Krita workflows run as they are |
| 2026-11-26 | Kept workflows are files in the styles folder's Workflows folder, beside the styles |
| 2026-11-26 | Parameter values are written into the inputs a Parameter feeds, typed (its node passes switches on as text) |
| 2026-11-26 | The seed is fitted to each node's own range (wrapping round), as some custom nodes take seeds only up to 2³¹ - 1 |
| 2026-11-26 | Only the prompt in view; the rest folded under More; starter workflows included (the user: maximum functionality, few options) |
