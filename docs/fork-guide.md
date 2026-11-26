# Keeping ProCraft Diffusion working: a plain guide

ProCraft Diffusion is ProCraft with AI generation added. ProCraft keeps getting better, and we
want those improvements too, without breaking our own features. This page is what you need to
know, and the few things you might need to do.

## The two branches (think of them as two copies)

- **`main-diffusion`**: ours. ProCraft plus the AI features. It's the default on GitHub and
  it's what you build and use.
- **`main`**: a plain copy of ProCraft, untouched. We don't work on it.

## What happens by itself

We follow ProCraft's **releases** (the versions it publishes every few days, like v0.0.4-beta),
not every change it makes in between: a release is a tested point, while ProCraft's day-to-day
work moves too fast to keep up with.

Every morning at **08:00 (UK time)**, a GitHub robot (GitHub is busy then, so it often starts
an hour or so late, and very occasionally skips a day):

1. looks for ProCraft's latest release; if we already have it, it stops there, all fine,
2. otherwise takes our app and mixes that release in,
3. checks it: Rust tests, lint, and an Android build.

(It has to look every day: GitHub can't wake it up when ProCraft publishes a release.)

It **only checks**. It never changes our app. You find out the result like this:

| What you see | What it means | What to do |
|---|---|---|
| Nothing (no email) | All fine: either no new release, or a new one that fits | Nothing, or ask the agent to bring it in when you like |
| An email from GitHub saying the run failed | ProCraft's new release clashes with ours, or a check broke | Ask the agent: "the upstream sync failed, have a look" |

To see the results yourself: GitHub, your repository, **Actions** tab, **Upstream sync**. Click
a run to read its summary, e.g. "Ready: ProCraft's v0.0.5-beta (26 commits) merges cleanly and
passes".

## Bringing in ProCraft's new things

Nothing comes in until you ask. When you want to catch up, tell the agent:

> Bring in ProCraft's latest release.

The agent mixes it in on your computer, runs all the checks, tries the app if needed, and
pushes when you say so. Bringing in each release soon after it comes out keeps the clashes small
and easy.

## Things not to do

- **Don't press "Sync fork" on `main-diffusion`** on GitHub. If it offers "Discard commits",
  that throws away all our AI work. On `main` it's harmless.
- **Don't uninstall the app on the tablet** without thinking: it deletes the artworks in it.
  The old ProCraft app and ProCraft Diffusion are separate apps with separate artworks.
- **Don't put personal details in the repository**: it's public. They go in `dev/local.md`,
  which stays on your computer only.

## Things worth remembering

- **No email isn't always good news**: GitHub pauses the morning robot if the repository has had
  no pushes for 60 days. Every month or so, glance at the Actions tab: if Upstream sync says it's
  disabled, a button there turns it back on.
- **Running the check now**: Actions, Upstream sync, **Run workflow**. It takes about half
  an hour.
- **Starting a new agent session**: the notes in `CLAUDE.md` (and `AGENTS.md`, the same text
  for other tools) tell any agent how this all works, so you don't need to explain it again.
- **The old private history**: the GitHub Support request to purge old copies is drafted in
  the folder above this repository (`github-support-request.md`, not in git). Once GitHub
  confirms it's done, ask the agent to delete the local backups of the old history.
