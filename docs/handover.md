# ProCraft Diffusion: handover

A synopsis of what's been built, where everything lives, how to get a new build onto the tablet,
and how another AI (Gemini, or a later Claude) should pick the work up. Written 2026-11-27.

---

## 1. What this is, in one paragraph

**ProCraft Diffusion** is a fork of **ProCraft** (an open-source, Procreate-like painting app for
Android tablets and desktop, written in Rust with a Kotlin/Compose Android front end and an egui
desktop front end). The fork adds AI image generation through **ComfyUI running on the user's
Windows PC**, plus a full **pixel art and game-sprite toolset**: a pixel art canvas,
AI sprite generation and animation, named animations, sprite sheet export and import. The next
feature, designed but not built, is **puppet animation** (rigging a character's parts and posing
it).

The user is not a programmer. He wants maximum functionality with few options: sensible defaults chosen for him,
rare settings folded away, steps done for him.

---

## 2. Where everything is

| What | Where |
|---|---|
| **The user's repository** (work here) | https://github.com/thesilentyak/procraft |
| **The branch to use** | `main-diffusion` (the default). Never work on `main`. |
| The fork it came from | https://github.com/Hekel1989/procraft (remote `hekel`) |
| ProCraft itself (upstream) | https://github.com/artcraft-android/procraft (remote `upstream`) |
| Agent instructions (read first) | `CLAUDE.md` in the repo root (`AGENTS.md` is an identical copy) |
| All design documents | `docs/generation/` (AI generation) and `docs/pixel-art/` (pixel art and animation) |
| Names of every part | `docs/generation/names.md` |
| The user's plain guide to upstream syncing | `docs/fork-guide.md` |
| ComfyUI workflow starters | `dev/starters/*.json`, made by `dev/starters/make_starters.py` |
| The PC's ComfyUI | ComfyUI Desktop on Windows, `http://127.0.0.1:8188` on the PC |
| The tablet app | package `io.github.hekel1989.procraftdiffusion` (`.debug` for debug builds) |

---

## 3. Getting a new build onto the tablet

**Every push to `main-diffusion` builds the Android app automatically on GitHub.**

1. Push a commit to `main-diffusion` (or to any branch named `update/…`, for a test build that
   doesn't touch the main one). Pushes that change only `docs/` or `.md` files don't build.
2. On GitHub: the repository → **Actions** tab → **Build APK** → the newest run. It takes about
   7 minutes. A green tick means it built.
3. Open the run, scroll to **Artifacts**, download `procraft-diffusion-arm64-<commit>`, unzip it,
   and install the `.apk` on the tablet. This works from the tablet's own browser too.
4. Every build is signed with the same key (`dev/fork-debug.keystore`, a public debug key), so a
   new one **installs over the old one and keeps the artworks**. Never uninstall first: that
   deletes the artworks.

**To build without a new commit:** Actions → Build APK → **Run workflow** → choose the branch.

**From a command line** (an AI with GitHub's `gh` tool):

```bash
gh run list -R thesilentyak/procraft -L 5                 # the latest builds
gh run watch <run-id> -R thesilentyak/procraft            # wait for one
gh run download <run-id> -R thesilentyak/procraft         # fetch the APK
gh workflow run "Build APK" -R thesilentyak/procraft --ref main-diffusion
```

**Going back to an older version:** each commit is a saved point. Build artifacts are kept for
30 days (download an older run's APK from Actions). For anything older, make a branch at that
commit and push it as `update/<name>`; GitHub builds it:

```bash
git switch -c update/back-to-xyz <commit-hash>
git push origin update/back-to-xyz
```

---

## 4. For the next AI: how to work on the code

### 4.1 Read first

- `CLAUDE.md`: the rules, the layout, the build commands, the status list. Its rules apply to
  any AI, not only Claude.
- The relevant `docs/…` design document for whatever is being built.

### 4.2 The working rules (the user's)

- **Process: brainstorm → proposal → design → code.** Each step is agreed with the user before
  the next. Don't code something that has no agreed design.
- **Keep it simple.** Prefer the plain solution that works.
- **The user decides genuine choices**: offer two or three options with a recommendation, in
  plain words with an example. When the answer is obvious, pick it and say so.
- **Mobile first**: the Android tablet is the main target; it must also work on the desktop.
- **Commit and push only when the user asks.** (So far he has agreed to a push after each built
  step, which also makes the test APK.)
- **Keep hooks into ProCraft's own files thin**: new code goes in new files, connected by a line
  or two, so ProCraft's updates merge cleanly.
- **Never put personal details in tracked files**: the repository is public.

### 4.3 Hard rules

- **Test generations use explicitly safe subjects** (landscapes, objects, animals), with
  `scenery, no humans` added. Never an empty or vague prompt.
- Never copy code from krita-ai-diffusion (GPL-3.0). Never host or ship model weights (Anima's
  are non-commercial).
- The NSFW switch only changes tag suggestions, never what's generated.
- On the tablet: ask before uninstalling the app (it deletes the artworks) or changing security
  settings.
- No iPad support, ever.

### 4.4 Build and check (from `CLAUDE.md`)

```bash
cargo fmt --all -- --check
cargo clippy -p procraft-diffusion -p procraft-core --features procraft-core/generation --all-targets -- -D warnings
cargo clippy -p procraft-desktop --all-targets -- -D warnings
cargo test -p procraft-diffusion
cargo test -p procraft-core --features generation --lib     # slow: run a filtered subset, e.g. `-- sprite_sheet`
cargo run --release -p procraft-desktop
```

- **Lints deny** `unwrap`, `expect`, `panic` and indexing (`a[i]`) outside tests: use `.get()`.
- `cargo clippy --fix --allow-dirty -p <crate> --all-targets` fixes most "collapsible if" lints.
- rustfmt width is 120. Doc comments are plain prose.
- Commits are semantic (`feat(pixel-art): …`, `fix(…)`, `docs(…)`, `tune(…)`).
- **Kotlin isn't compiled in a cloud session without the Android SDK**: GitHub's Build APK run
  is the Kotlin check. Push, then watch the run.
- Disk space in a cloud session can be tight: build with `CARGO_INCREMENTAL=0`.

### 4.5 Known test failures that were there before (not bugs from our work)

- Desktop: `gallery::tests::a_narrow_window_puts_the_buttons_under_more_beside_a_whole_title`,
  `input::tests::a_1000_hz_pen_reaches_the_stroke_whole_once_a_frame`,
  `tests::the_title_is_looked_at_again_each_second_until_saved_and_sets_only_changes`.
- Core: `a_time_lapse_frame_waits_for_an_ended_strokes_end` (a 1-second limit a software GPU
  misses) and `tests::a_transform_s_box_is_found`.
- `ui::tests::an_artwork_left_is_held_by_nothing_in_the_interface` can time out under a full
  parallel run; it passes alone.

### 4.6 Code layout

| Part | Where |
|---|---|
| Engine (pure, no GPU or UI) | `crates/engine/src/` (e.g. `pixel.rs`, `animation.rs`, `sprite_import.rs`) |
| GPU renderer | `crates/gpu/src/canvas.rs` |
| Core (exported to Kotlin via UniFFI, used directly by desktop) | `crates/core/src/` |
| ComfyUI client and workflows | `crates/diffusion/src/` |
| Android UI | `android/app/src/main/java/io/github/artcraftandroid/procraft/` (generation UI in `android/app/src/generation/`) |
| Desktop UI | `desktop/src/` |

---

## 5. What's been built (synopsis)

### 5.1 AI generation (Phase 1 and early Phase 2; `docs/generation/`)

- **M1–M4**: the ComfyUI client; Generate on the tablet and desktop (an "AI" button opens the
  Generate popover); Draft/Normal/Quality sizes; selections (Fill, Add, Remove, Expand); control
  layers (Line art, Scribble) with tracing; tag autocomplete.
- **M5**: a companion CLI that installs and runs ComfyUI (`companion/`).
- **M6–M8**: styles as files with LoRAs, the Style Studio, trigger words, import and export.
- **M9**: depth control layers.
- **Results tray**: results kept with each artwork, a tray that tucks into a tab, Again, Apply.

### 5.2 This fork's additions (`thesilentyak/procraft`)

- **ProCraft v0.0.8-beta merged in.**
- **Custom ComfyUI workflows** (`docs/generation/09-…`, `10-…`): any ComfyUI workflow with the
  Krita marker nodes runs from the Generate popover. "ComfyUI (live)" follows the open ComfyUI
  tab; workflows can be kept. Starters (in `dev/starters/` and saved in the user's ComfyUI):
  - Z-Image new picture, Z-Image paint over
  - MiniMax new picture, MiniMax edit
  - Pixel art (RD Flow 64); **Pixel art (Anima)**, the best for base sprites (Anima asked for
    pixel art on green, then SpriteFusion's Pixel Snapper)
  - Pixel art edit (MiniMax)
  - **Pixel animation (MiniMax H3)**: a sprite becomes a looping video, snapped back to pixels,
    up to 24 frames and 5 seconds
- **Pixel art canvas, stage 1** (`docs/pixel-art/01-…`, `02-…`): New canvas › Pixel art; crisp
  pixels, pixel grid, checkerboard; the normal Brush and Eraser become a pixel pencil and eraser
  on it (sizes 1–8, pixel-perfect lines, QuickShape in pixels); exact selections, fill and
  eyedropper; a palette strip; PNG export at ×N.
- **Animation, stage 2** (`docs/pixel-art/03-…`, `04-…`):
  - **Part A**: animate a sprite with AI. The tray plays an animation result; Apply adds its frames.
  - **Part B**: Share Layers › **Sprite Sheet** (PNG plus Aseprite JSON, one row per animation,
    and a separate sheet per named animation for Godot); crisp GIFs on pixel art.
  - **Part C**: **named animations** (Frame options › Start Animation Here…, Rename, Remove;
    coloured bands on the frame strip) and **Animate with AI…** in Frame options.
- **Sprite sheet import** (`docs/pixel-art/05-sprite-import-design.md`, built 2026-11-27):
  Actions › Add › **Insert a sprite sheet**. It removes the background, finds the frames, works
  out the pixel size and makes real pixels, gives all frames one set of colours, lines up the
  feet, and adds them as a named animation, growing the canvas if needed. Tested on the user's
  Gemini sheets (a beet with a balloon, a turtle walk) through the code; **not yet tried on the
  tablet**.
- **Gemini findings**: Gemini makes very consistent sprite sheets of
  a character; real motion improved a lot with a precise prompt. Its weak spots were rows drawn
  at slightly different sizes and a shuffly leg order.

### 5.3 Latest commit

`5cb9ec9a feat(pixel-art): insert a sprite sheet as a named animation`, pushed; its APK built
successfully.

---

## 6. Where things stand now: puppet animation

**Not built yet.** Three documents were written on 2026-11-27 and are **not yet committed** to the
repository (they're in the working copy of the cloud session):

- `docs/pixel-art/06-puppet-brainstorm.md`: agreed
- `docs/pixel-art/07-puppet-proposal.md`: **final**, agreed
- `docs/pixel-art/08-puppet-design.md`: **written, waiting for the user's OK**

**Agreed decisions:**
- Flat 2D (not Smack Studio's 3D; that approach is patented).
- Baked frames come out; the rig for Godot later.
- Joined parts; swap parts (alternative drawings) in the first version.
- Ready-made motions (Idle, Walk, Wave) later.
- Parts can be **split out of an existing picture**: tap a part to pick it out, Make a part,
  **Fill behind** for the hole it leaves (with an AI option).

**The design in brief:**
- **Make a Puppet** in the layer menu opens **Puppet mode**, with three steps: **Parts · Joints ·
  Pose**.
- **Parts**: tap to pick a part. On pixel art the pick follows the outline. On other pictures, the
  PC's Segment Anything (kijai's `ComfyUI-segment-anything-2`, node `Sam2Segmentation`) is used if
  installed.
- **Joints** are guessed from where parts touch; dots can be dragged to fix them.
- **Pose**: moves (idle, walk…) with 8-frame timelines. Dragging a part turns it, and each change
  saves a key. Play, then **Bake** into named frames; baking again replaces them.
- Turned pixel-art parts use **RotSprite** (Scale2x ×3, turn, back down), written from its
  published description, so outlines stay clean and no new colours appear.
- The puppet is a hidden layer group plus a `puppets` list in the document. Live posing is drawn
  on the CPU into the canvas overlay.
- **Build order:** P1 engine (`crates/engine/src/puppet.rs`, with tests) → P2 core → P3 Android →
  P4 desktop → P5 the PC's help.
- **First real test:** split the turtle into shell, head and four legs, pose a walk, bake 8
  frames, export a sprite sheet.

**Open ideas for later:**
- A Gemini "parts sheet" (a character drawn already in parts) gathered into a puppet.
- Scripts: pasting Lua scripts written in a chat, with a "Copy guide for AI" button. Brainstormed
  only; the user likes pasting scripts made in his own chat.
- Desktop parity for a few pixel art bits (palette strip, Contiguous, Custom size).

---

## 7. A prompt to start another AI with

> You're continuing work on ProCraft Diffusion, the GitHub repository `thesilentyak/procraft`,
> branch `main-diffusion`. Read `CLAUDE.md` (its rules apply to you), then this handover, then
> `docs/pixel-art/07-puppet-proposal.md` and `08-puppet-design.md`. The user is not a programmer:
> explain things in plain words, offer choices with a recommendation, and don't write code for
> anything without an agreed design. Commit and push only when he asks; every push to
> `main-diffusion` builds a test APK on GitHub Actions (Build APK). Start by asking whether he
> agrees the puppet design, then build milestone P1 (the engine, with tests).
