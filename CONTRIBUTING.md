# Contributing to latentCreate

Thanks for helping. The most useful thing right now is **testing on machines that are not the
developer's** -- different ComfyUI installs, GPUs and operating systems -- and filing what breaks.

## Reporting a bug

Open an issue with the **Bug report** form. It asks for your ComfyUI version and kind, where it runs
and the app's `session.log`, because most problems found so far depended on exactly those. If a
workflow also fails in ComfyUI's own web UI, report it to ComfyUI instead.

## Building and running

See the README's [Requirements](README.md#requirements) and [Development](README.md#development):

```bash
npm install     # root and the app workspace, one step
npm run dev     # the desktop app with hot reload -- run from the repo root
npm run gate    # everything CI runs; must pass before a pull request
```

No test needs a running ComfyUI or LLM: the network crates test against fixtures.

## Before you write code

For anything beyond a small fix, **open an issue first** so the approach can be agreed before you
spend time on it. Work in this repo is planned as numbered tasks in [tasks/](tasks/); the
[ROADMAP](tasks/ROADMAP.md) and the current phase file say what is in flight.

Read, in this order: [ARCHITECTURE.md](ARCHITECTURE.md) for the design and its contracts,
[CONVENTIONS.md](CONVENTIONS.md) for code standards, and
[docs/COMFY-HTTP-SURFACE.md](docs/COMFY-HTTP-SURFACE.md) before touching anything that talks to
ComfyUI. Claims about ComfyUI's API are verified against a live server, not written from memory.

## Rules a change must not break

- **Ship no models.** All generation goes through the user's own ComfyUI or their own API keys.
  Nothing is bundled, and nothing is downloaded except when the user asks.
- **Never rewrite a user's words silently.** A change to prompt or lyric text needs an accept step
  that shows the diff first.
- **Deletes go to the OS trash**, never a hard delete.
- **Every generated file gets a provenance sidecar** complete enough to reproduce it.
- **Model licences are shown** wherever a model is chosen or installed.

## Dependencies and licensing

latentCreate is Apache-2.0, and contributions are accepted under the same licence (Apache-2.0
section 5). New dependencies must be permissively licensed; anything copyleft needs discussing in an
issue first. After adding or updating a dependency, run `npm run licenses` and commit the
regenerated [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) -- the gate fails until you do.

Do not copy code from other projects unless their licence allows it and the source is credited.

## Pull requests

Keep a pull request to one change. Say what it does, how you checked it, and what you clicked if it
changes the UI. The template has a short checklist.
