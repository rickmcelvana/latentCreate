# AGENTS.md — orientation for any coding agent (Claude Code, Opencode, etc.)

**Read order, every session:**
1. [PROJECT.md](PROJECT.md) — current state, decisions log, open questions. **First action each session:** check it and ARCHITECTURE.md against `git log` since the last session-log entry; fix drift before new work.
2. [ARCHITECTURE.md](ARCHITECTURE.md) — system design and interface contracts. Never change silently.
3. [WORKFLOW.md](WORKFLOW.md) — how tasks are written, executed (Aider + `ollama_chat/kimi-k2.7-code:cloud`), reviewed, and merged.
4. [CONVENTIONS.md](CONVENTIONS.md) — code standards. Fed to Aider with `--read` on every run.
4b. [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) — **verified** comfy-mcp tool names, slot addresses, LoRA and template facts. Authoritative over docs/RESEARCH.md and over any model documentation. Read before touching `mcp-bridge` or writing a model profile.
4c. [docs/LLM-SURFACE.md](docs/LLM-SURFACE.md) — **verified** OpenAI-compatible wire format: the `delta.reasoning` split, the empty-`choices` usage frame, non-JSON error bodies, SSE framing rules. Read before touching `llm-bridge`.
5. `tasks/ROADMAP.md` → the current `tasks/phase-N.md` — pick up the first unfinished T-number. **Phases 0, 1 and 2 are complete**, tagged `phase0-done`, `phase1-done` (2026-08-25) and `phase2-done` (2026-08-26); their phase files are records, not to-do lists. **Phase 3 is complete** (closed 2026-08-30, T-301 … T-317 all landed): the pipeline generates
audio live on both shipped profiles, a finished track is ingested into the Library with a
provenance sidecar checked field-for-field against the graph ComfyUI actually executed, one click
queues N variations by seed, and a fresh Generate re-rolls the seed unless the user pinned it
(T-316). T-315 landed 2026-08-29 and passed its click-through (the crash path's error copy — one sentence with a next step, diagnostics moved to `session.log`; this also discharges T-314's kill-mid-job check). **T-313 is complete (a-g) and click-through passed.** A person can import their own ComfyUI workflow, confirm what the app guessed about it, save it as a profile indistinguishable from a shipped one, and generate from it -- the milestone line "an imported user workflow generates successfully" is **discharged**, as is the kill-mid-job line (T-315). Import takes the **frontend** format, not API (MCP-SURFACE 29, which corrected ARCHITECTURE 5b), and an imported workflow is **copied**, not referenced, so sidecars cannot go stale. **T-314 ran live 2026-08-30** (the first recorded full-length generations — 185/200 s of audio at
~5x realtime) and **T-317 settled `vram_gb_min: 8` by measurement**: a comfort floor, not a gate —
ComfyUI offloads rather than fails (MCP-SURFACE 31). **Phase 4 (Library & Player) is in progress**
— [tasks/phase-4.md](tasks/phase-4.md) opened 2026-08-30 with its phase-start check done (the
mixing/mastering repos are web-first; Send-to stays the v1 link-out) and two owner decisions
(projects become first-class; milestone-first ordering). **T-401 (projects become first-class) is complete** — T-401a (backend seam) and T-401b (the
picker) landed 2026-08-30 and the click-through passed: a track generated with a second project
selected lands in `projects/<slug>/tracks/` and the Library shows it under that project. **T-402
(playback + visualizer) is complete** — click-through passed, the first Phase 4 milestone line
discharged. **T-403 (album lists) is complete** — `library::albums`, the six `albums_*` commands,
the `state/albums` store (18 tests) and the Library album panel; **the producer click-through
passed 2026-08-31 and the second Phase 4 milestone line ("album list") is discharged.** **A planning pass on 2026-09-01** added two tasks and set the remaining order --
**T-404 (Send-to) -> T-405 (track actions) -> T-408 (delete for every kind of created content) ->
T-409 (the song title, carried) -> T-406 (provenance inspector)** -- and confirmed the sibling
apps still have no import surface, so T-404 stays the v1 link-out. **T-404 is complete (a/b) and
its click-through passed 2026-09-01**, discharging the third and last Phase 4 milestone line:
**the phase's milestone check is met in full, and the phase is not finished** -- T-405, T-408,
T-409 and T-406 are the half of its scope the milestone line never covered. Briefs are written one at a
time, each after the previous lands. **PROJECT.md's Snapshot is the live state and this line is a summary of it** — if they disagree, this line is stale and fixing it is part of the session, not something to read past.

**Hard rules (summary — the linked docs are authoritative):**
- Planning-first: no code without a T-brief in the current phase file. One brief per Aider run, ≤ ~400-line diffs, commit `T-0XX: title` only after **`npm run gate`** passes (it mirrors CI). Executors run with `--no-auto-commits`; they never commit. **The architect (you) commits once the gate is green** — including for your own doc/brief work, where no Aider run is involved. Green gate is the go-ahead, not a checkpoint to ask at.
- Ship no models. All generation goes through the user's ComfyUI (Comfy MCP) or their API keys.
- Never modify user prompt/lyric text without an explicit accept step. Every generated asset gets a provenance sidecar. Deletes go to OS trash.
- Verify third-party API surfaces (rmcp, provider APIs) against live docs/source before writing briefs — not from memory. For comfy-mcp, docs/MCP-SURFACE.md holds the verified names/slots; re-check against the live server rather than trusting the cloud documentation, which names different tools.
- End every session by updating PROJECT.md's session log; doc updates land in the same commit as the behavior change they describe.
- **Verify a doc's claim before building on it.** A doc that names a file, a test, a gate or a count is making a claim about the repo, and the session-start `git log` check cannot catch one that was never true. Open the file. Grep for the behaviour. Prefer the most recently dated number over the oldest. WORKFLOW §6 has the rule and what it has cost twice.
- **Aider is a token-saving device, nothing else.** Its only job is to keep the architect's context free so a session runs longer. Work that is already written and verified does not go through it — WORKFLOW §1.

**Comfy MCP for agent sessions:** local server registers with Claude Code via `claude mcp add comfy-mcp -- comfy-mcp` (requires `pip install comfy-mcp` and a ComfyUI install). Useful for live verification from Phase 1 on; never required for unit tests (mock transports only — WORKFLOW §5).

**Sibling repos** (`../latent-mixing`, `../latent-mastering`): closed-source references for conventions and visual language. **Do not copy code from them into this open-source repo** unless PROJECT.md's decisions log records the owner explicitly relicensing that piece.
