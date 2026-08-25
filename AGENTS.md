# AGENTS.md — orientation for any coding agent (Claude Code, Opencode, etc.)

**Read order, every session:**
1. [PROJECT.md](PROJECT.md) — current state, decisions log, open questions. **First action each session:** check it and ARCHITECTURE.md against `git log` since the last session-log entry; fix drift before new work.
2. [ARCHITECTURE.md](ARCHITECTURE.md) — system design and interface contracts. Never change silently.
3. [WORKFLOW.md](WORKFLOW.md) — how tasks are written, executed (Aider + `ollama_chat/kimi-k2.7-code:cloud`), reviewed, and merged.
4. [CONVENTIONS.md](CONVENTIONS.md) — code standards. Fed to Aider with `--read` on every run.
4b. [docs/MCP-SURFACE.md](docs/MCP-SURFACE.md) — **verified** comfy-mcp tool names, slot addresses, LoRA and template facts. Authoritative over docs/RESEARCH.md and over any model documentation. Read before touching `mcp-bridge` or writing a model profile.
4c. [docs/LLM-SURFACE.md](docs/LLM-SURFACE.md) — **verified** OpenAI-compatible wire format: the `delta.reasoning` split, the empty-`choices` usage frame, non-JSON error bodies, SSE framing rules. Read before touching `llm-bridge`.
5. `tasks/ROADMAP.md` → the current `tasks/phase-N.md` — pick up the first unfinished T-number. **Currently [tasks/phase-1.md](tasks/phase-1.md)**; T-101 through T-112d have landed and **T-113 (Phase 1 live milestone, producer-run) is next**. PROJECT.md's Snapshot carries the live state — trust it over this line if they ever disagree.

**Hard rules (summary — the linked docs are authoritative):**
- Planning-first: no code without a T-brief in the current phase file. One brief per Aider run, ≤ ~400-line diffs, commit `T-0XX: title` only after **`npm run gate`** passes (it mirrors CI). Executors run with `--no-auto-commits`; they never commit. **The architect (you) commits once the gate is green** — including for your own doc/brief work, where no Aider run is involved. Green gate is the go-ahead, not a checkpoint to ask at.
- Ship no models. All generation goes through the user's ComfyUI (Comfy MCP) or their API keys.
- Never modify user prompt/lyric text without an explicit accept step. Every generated asset gets a provenance sidecar. Deletes go to OS trash.
- Verify third-party API surfaces (rmcp, provider APIs) against live docs/source before writing briefs — not from memory. For comfy-mcp, docs/MCP-SURFACE.md holds the verified names/slots; re-check against the live server rather than trusting the cloud documentation, which names different tools.
- End every session by updating PROJECT.md's session log; doc updates land in the same commit as the behavior change they describe.

**Comfy MCP for agent sessions:** local server registers with Claude Code via `claude mcp add comfy-mcp -- comfy-mcp` (requires `pip install comfy-mcp` and a ComfyUI install). Useful for live verification from Phase 1 on; never required for unit tests (mock transports only — WORKFLOW §5).

**Sibling repos** (`../latent-mixing`, `../latent-mastering`): closed-source references for conventions and visual language. **Do not copy code from them into this open-source repo** unless PROJECT.md's decisions log records the owner explicitly relicensing that piece.
