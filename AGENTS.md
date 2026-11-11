# AGENTS.md — orientation for any coding agent (Claude Code, Opencode, etc.)

**Read order, every session:**
1. [PROJECT.md](PROJECT.md) — current state, decisions log, open questions. **First action each session:** check it and ARCHITECTURE.md against `git log` since the last session-log entry; fix drift before new work.
2. [ARCHITECTURE.md](ARCHITECTURE.md) — system design and interface contracts. Never change silently.
3. [WORKFLOW.md](WORKFLOW.md) — how tasks are written, executed (Aider + `ollama_chat/kimi-k2.7-code:cloud`), reviewed, and merged.
4. [CONVENTIONS.md](CONVENTIONS.md) — code standards. Fed to Aider with `--read` on every run.
4b. [docs/COMFY-HTTP-SURFACE.md](docs/COMFY-HTTP-SURFACE.md) — **verified** ComfyUI HTTP routes, the API workflow format, job/cancel/output semantics and the download-destination rule. Read before touching `comfy-bridge` or writing a model profile. [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) is the **historical** comfy-mcp record (retired 2026-11-11, T-519): its findings about ComfyUI and the models still hold, its tool facts do not.
4c. [docs/LLM-SURFACE.md](docs/LLM-SURFACE.md) — **verified** OpenAI-compatible wire format: the `delta.reasoning` split, the empty-`choices` usage frame, non-JSON error bodies, SSE framing rules. Read before touching `llm-bridge`.
5. `tasks/ROADMAP.md` -> the current `tasks/phase-N.md` -- pick up the first unfinished T-number. **Phases 0-4 are complete** and their phase files are records, not to-do lists: 0/1/2 are git-tagged (`phase0-done`, `phase1-done` 2026-08-25, `phase2-done` 2026-08-26); 3 and 4 were closed docs-only (2026-08-30 and 2026-09-02). **Phase 5 is open** -- [tasks/phase-5.md](tasks/phase-5.md) is the only phase file to work from.

**Hard rules (summary — the linked docs are authoritative):**
- Planning-first: no code without a T-brief in the current phase file. One brief per Aider run, ≤ ~400-line diffs, commit `T-0XX: title` only after **`npm run gate`** passes (it mirrors CI). Executors run with `--no-auto-commits`; they never commit. **The architect (you) commits once the gate is green** — including for your own doc/brief work, where no Aider run is involved. Green gate is the go-ahead, not a checkpoint to ask at.
- Ship no models. All generation goes through the user's own ComfyUI (its HTTP API) or their API keys.
- Never modify user prompt/lyric text without an explicit accept step. Every generated asset gets a provenance sidecar. Deletes go to OS trash.
- Verify third-party API surfaces (ComfyUI routes, provider APIs) against a live server or real source before writing briefs — not from memory. For ComfyUI, docs/COMFY-HTTP-SURFACE.md holds what was observed; re-check against the live server, and capture a fixture into `testdata/comfy-http/` for anything new.
- **Briefs are written one at a time**, each after the previous lands and its review is done. A phase file lists the lanes; it does not pre-write them.
- **This file carries no per-task state, deliberately.** Item 5 names the open phase and nothing more -- it went stale twice by accreting status a session at a time, which is the failure the session-start drift check exists to catch and the one it is worst at catching. **PROJECT.md's Snapshot is the live state.**
- End every session by updating PROJECT.md's session log; doc updates land in the same commit as the behavior change they describe.
- **Verify a doc's claim before building on it.** A doc that names a file, a test, a gate or a count is making a claim about the repo, and the session-start `git log` check cannot catch one that was never true. Open the file. Grep for the behaviour. Prefer the most recently dated number over the oldest. WORKFLOW §6 has the rule and what it has cost twice.
- **Aider is a token-saving device, nothing else.** Its only job is to keep the architect's context free so a session runs longer. Work that is already written and verified does not go through it — WORKFLOW §1.

**Live ComfyUI for agent sessions:** any running ComfyUI on `127.0.0.1:8188` is enough -- `curl` its routes for live checks. To convert a gallery template to API format, open the ComfyUI web UI in a browser and run `await app.graphToPrompt()` after `app.loadGraphData(template)` (the Export (API) code path; ARCHITECTURE §5). Never required for unit tests (fake transport + fixtures only — WORKFLOW §5).

**Sibling repos** (`../latent-mixing`, `../latent-mastering`): closed-source references for conventions and visual language. **Do not copy code from them into this open-source repo** unless PROJECT.md's decisions log records the owner explicitly relicensing that piece.
