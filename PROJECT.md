# PROJECT.md — latentCreate (living document)

> Load this file at the start of every session (Claude Code, Opencode, any agent). Update it at the end of every session. **Session-start rule: verify this file and ARCHITECTURE.md agree with `git log` since the last session entry; fix drift before new work.**

## Snapshot
- **Project:** latentCreate — open-source, desktop-only (Tauri 2) AI music creation front-end. Orchestrates user-provided ComfyUI (via Comfy MCP) for audio/image generation and a user-provided LLM for lyrics. Ships no models. Complements closed-source siblings `../latent-mixing` and `../latent-mastering` (send-to targets) and the in-development latentPlayer.
- **Phase:** Planning complete (2026-08-23). Next: Phase 0 scaffold — see [tasks/ROADMAP.md](tasks/ROADMAP.md) and [tasks/phase-0.md](tasks/phase-0.md).
- **Next up:** T-001 (repo scaffold). Prereq for the human: none for T-001; Comfy MCP registration for Claude Code (`claude mcp add comfy-mcp -- comfy-mcp`) is only needed from Phase 1 verification onward.
- **Stack:** Rust (workspace crates) + Tauri 2.x, React 19 + TS strict + Vite + Zustand, plain CSS single-theme file. Matches sibling repos' versions where sensible (Tauri ~2.11, React ~19.2 as of planning).

## How work happens
- WORKFLOW.md defines the Claude(architect)/Aider(executor)/human(producer) loop, adapted from latent-mastering. This repo is almost entirely plumbing/UI → default executor `ollama_chat/kimi-k2.7-code:cloud`. No DSP lane exists here (the visualizer is AnalyserNode + canvas, not custom math).
- Tasks live in `tasks/phase-N.md` as T-numbered briefs with ready-to-paste Aider launch commands. One brief per Aider run, ≤ ~400-line diffs, commit only on green tests.

## Key decisions log
- **2026-08-23 — MCP-first Comfy integration.** App embeds an MCP *client* (rmcp, stdio to local `comfy-mcp`; HTTP to Comfy Cloud) rather than ComfyUI's raw HTTP API. Rationale: model search/download, templates, validation, and job tools come free; local/cloud is one trait, two transports (ARCHITECTURE §1, §3). Raw API fallback deliberately deferred (OQ-3).
- **2026-08-23 — Model capability profiles as JSON data** (ARCHITECTURE §5). Supporting a new music model = a profile file, not code. Default model: ACE-Step 1.5 (Apache-2.0, lyrics+vocals, consumer-GPU fast). Also profiled: Stable Audio Open, MusicGen, YuE (advanced), DiffRhythm.
- **2026-08-23 — Prompt optimization is consent-gated.** Optimizer output always shown as a diff; user accepts/edits/reverts; provenance records the flag. Never silently rewrite user text (owner requirement).
- **2026-08-23 — Provenance sidecars** for every generated asset (full inputs+seed+model). JSON store, no DB.
- **2026-08-23 — Clean-room visualizer.** Sibling repos are closed-source; default is reimplementing the small viz layer against AnalyserNode rather than porting, to keep this repo unencumbered (owner may explicitly relicense pieces they own — record it here if so).
- **2026-08-23 — Delete = OS trash**, never hard delete (safety rule, also matches suite behavior).
- **2026-08-23 — License: Apache-2.0** (owner decision, closes OQ-1). Verbatim `LICENSE` + `NOTICE` in place. Rationale: patent grant, matches the ACE-Step/permissive model ecosystem, compatible with the closed-source siblings consuming outputs. Consequence for briefs: **permissive-only dependencies**; any copyleft dep needs its own decisions-log entry, and no code may be copied in from `../latent-mixing` / `../latent-mastering` without a recorded relicensing note. NOTICE holder is currently "latentCreate contributors" — swap in a legal name/entity if the owner prefers.
- **2026-08-23 — Lyric-LLM recommendations** (closes OQ-2). From the owner's hands-on use across many local models: **Gemma 4 12B is the standout at its size** for lyric writing, and **Gemma 4 26B–31B perform well for users with the VRAM to run them**. These become the app's *suggested* models in the setup wizard's LLM step and in docs — suggestions only, never a gate: any OpenAI-compatible endpoint remains first-class (ARCHITECTURE §4). Not benchmarked in-repo; recorded as owner experience so agents don't re-litigate it. Full list with sizing: docs/MODELS.md.
- **2026-08-23 — Send-to sequencing** (closes OQ-4 for this repo): the real handoff mechanism is being built on the mixing/mastering side **before** latentCreate reaches Phase 4. latentCreate therefore builds only the v1 link-out + reveal-file behavior and adopts whatever protocol those repos define when it exists — this repo does not design the handshake. Re-check the mixing/mastering repos' state at the start of Phase 4.

## Open questions (owner to decide)
- **OQ-3 Raw ComfyUI API fallback.** Build a second `ComfyBackend` impl against `/prompt`+websocket if comfy-mcp proves limiting (e.g. arbitrary node-input introspection)? Deferred until Phase 3 evidence exists.
- **OQ-5 App identity — parked, do not force a decision.** `latentbeats.com` is the umbrella for the whole suite; "latentCreate" is the working name and is fine to ship in docs/UI for now. Final product name comes out of a dedicated brainstorming session the owner will schedule. **Agents: do not propose or apply branding changes unprompted**; keep the name in a small number of places (README title, `package.json`/`tauri.conf.json` product name, window title) so a later rename is cheap.

*Resolved: OQ-1 (Apache-2.0), OQ-2 (lyric-LLM guidance), OQ-4 (send-to owned by mixing/mastering) — all in the decisions log above.*

## Backlog (accepted, not yet scheduled)
- Album lists → bulk send-to-mastering once mastering's bulk import lands (owner-stated future feature).
- latentPlayer integration (library hand-off) once player matures.
- Audio-to-audio flows (cover/remix/extend) for models that support it — profiles already leave room via `inputs`.
- Community profile sharing (import a model profile JSON from URL).
- In-app "what's new in models" surface — periodic `search_models` diff against catalog. Graceful upgrade UX sketched in ARCHITECTURE §10 step 2.

## Session log
- **2026-08-23 — Planning session (Claude Fable).** Researched Comfy MCP tool surface (docs.comfy.org/agent-tools/mcp) and 2026 open music-model landscape (docs/RESEARCH.md). Authored README, ARCHITECTURE, CONVENTIONS, WORKFLOW, AGENTS/CLAUDE, docs/RESEARCH.md, docs/MODELS.md, tasks/ROADMAP.md, tasks/phase-0.md. Owner resolved OQ-1 (Apache-2.0 — LICENSE/NOTICE added) and OQ-4 (send-to handled by mixing/mastering first). Committed as the repo's initial commit; no code yet. Follow-up commit closed OQ-2 (Gemma 4 lyric-LLM guidance) and parked OQ-5 (naming — umbrella stays `latentbeats.com`, working name stays "latentCreate"). **Next session:** owner will have `comfy-mcp` registered with Claude Code — start by verifying it responds (`tools/list`), then either begin T-001 (scaffold, producer+architect) or run the Phase 1 schema enumeration early to de-risk `mcp-bridge` (RESEARCH §1 verification item).
