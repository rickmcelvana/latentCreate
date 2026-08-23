# WORKFLOW.md — The latentCreate Build Loop

*Adapted from `../latent-mastering/WORKFLOW.md` (the proven loop). Differences: this repo is open source, has no DSP-math lane, and its verification pain-point is external services (ComfyUI/MCP/LLM endpoints) rather than audio math.*

## 1. Roles
- **Architect (Claude / any capable agent):** writes and refines task briefs, reviews every diff line-by-line, maintains PROJECT.md/ARCHITECTURE.md, designs interfaces, and authors any tricky reference code inside the brief (MCP transport setup, SSE parsing, Tauri event wiring).
- **Aider (executor):** implements exactly one brief per run. Default model: `ollama_chat/kimi-k2.7-code:cloud`. If a brief is ambiguous, Aider must stop and list numbered questions (footer rule), not guess.
- **You (producer):** run Aider with the provided launch command, run builds, click through the app, merge, arbitrate.

**What makes an executor run succeed (measured over T-002, T-003, T-003b):** briefs that carry **full reference code** plus, for each test, the **invariant it must protect** produced near-clean runs needing only `cargo fmt`. The one brief written as prose spec without reference code (T-002) came back not compiling. Write the code in the brief; let the executor transcribe, wire up and test it.

**Model routing:** everything here is plumbing/UI → kimi lane by default. If a task class racks up repeated failed fix-up rounds (3+ attempts on one T-number), stop and ask the producer to try a different model — record the switch in the decisions log.

## 2. The loop (per task)
```
Architect writes brief (tasks/phase-N.md, one T-number) incl. "Aider launch" command block
  → producer: copy-paste the launch command as-is
  → paste brief → Aider implements (working tree only, --no-auto-commits)
  → producer runs the green gate → architect reviews → commit "T-0XX: <title>"
  → Architect reviews `git diff` against brief (checklist §4)
  → PASS → merge; FAIL → fix-up brief (T-0XXb) with its own launch command → repeat
  → UI/integration tasks: producer click-through per the brief's manual-verify list
```
Keep runs ≤ ~400 lines of diff; bigger scope = split the brief. **Green gate:** run **`npm run gate`** from the repo root — one command that chains `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `tsc -b`, `oxlint`, `vitest`, and `vite build`, in the same order CI runs them (T-005). `npm run gate:rust` / `gate:app` run half each. Commit only on green.

**Executors do not commit (learned on T-002, 2026-08-23).** Aider's first run in
this repo auto-committed twice while `tsc -b` was failing with 6 errors, so the
"commit only on green" rule was bypassed by the tool before any human or
architect saw the diff. Every launch command therefore carries
**`--no-auto-commits`**: the executor edits the working tree, the producer runs
the gate, and the commit happens after review with a `T-0XX:` message. A red
commit costs more to unpick than it saves.

**Executors do not decide line endings either.** That same run rewrote all 11
files as CRLF, turning an 11-line CSS addition into a 336-line diff and burying
the actual change. `.gitattributes` now pins `* text=auto eol=lf`. When a diff
looks impossibly large, check `file -b` before reading it as a rewrite.

## 3. Task brief template
```markdown
# T-0XX: <title>
**Depends:** T-0YY | **Crate/dir:** crates/mcp-bridge
**Files to create/modify:** (exact paths)

## Goal
One paragraph, testable.

## Spec
Exact behavior: types, ranges, defaults, error cases. Reference ARCHITECTURE.md
sections instead of restating interfaces.

## Reference implementation (when the architect pre-derives tricky code)
```rust
// integrate verbatim, adapt naming to crate style
```

## Acceptance criteria
- [ ] named tests pass; clippy/fmt/tsc clean
- [ ] no changes outside listed files

## Out of scope
Explicit non-goals.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file <exact files>
```
```

## 4. Architect's diff review checklist
1. Diff touches only listed files; ARCHITECTURE.md interfaces unchanged.
2. Acceptance tests exist and actually assert the spec (not vacuous). **Ask of each test: would it fail if the thing it guards were broken?** T-003 shipped a seed test that round-tripped a `u64` through `serde_json` without touching the crate's own types — green, and blind to the regression it existed to catch. Briefs cause this by naming mechanics; name the **invariant** instead.
3. No `unwrap()`/`expect()` on I/O or network paths; errors typed per crate.
4. Third-party API surfaces (rmcp, Tauri, comfy-mcp tool schemas, provider APIs) were verified against real docs/source in the brief — never from model memory.
5. Frontend: every new `className` has a rule in `theme.css`; `invoke`/`listen` only inside `app/src/bridge/`.
6. Secrets: nothing key-like written to config.json, logs, or provenance sidecars, **and no Tauri command returns a secret value** — the webview learns only whether a secret exists (T-004). Secret names arriving from the frontend are checked against a closed whitelist before touching the keychain.
7. Naming/units per CONVENTIONS.md; no TODO comments (backlog goes to PROJECT.md).
8. **Run the gate yourself before believing the diff.** T-002 arrived structurally correct and did not compile; a plausible-looking diff is not evidence of a green build.
9. Frontend types: no global `JSX` namespace (React 19 removed it -- use `ReactElement`); DOM boolean attributes are booleans, not `'true'`/`'false'` strings.
10. Zustand: subscribe with a selector, never the bare store, or the component re-renders on every unrelated state change.

## 5. Verification against live services
Unit tests must not require a running ComfyUI/LLM. Rules:
- `mcp-bridge` and `llm-bridge` get **mock-transport tests** (fake MCP server speaking the protocol over stdio pipes; canned SSE fixtures for LLMs). These run in CI.
- **Live smoke checks** are producer-run at phase milestones with real local services, from a checklist in the phase file (e.g. "install ACE-Step via wizard, generate 30 s clip, sidecar fields populated"). Results pasted into PROJECT.md session log.
- The Browser/webview review environment cannot composite frames or fire rAF (sibling-repo lesson). UI claims are verified by `getBoundingClientRect`/`getComputedStyle`/store reads, or explicitly listed as unverified for the producer's click-through — never silently assumed.

## 6. Git & docs conventions
- Default branch work: direct commits fine (solo repo); tag `phase0-done` etc. at milestones.
- Commit format: `T-013: setup wizard comfy step` / `docs: session log 2026-08-30`.
- **Docs discipline (hard rule):** every session starts by checking PROJECT.md/ARCHITECTURE.md against commits since the last session-log entry, and ends by updating the session log. A merged task that changed behavior described in a doc updates that doc *in the same commit or the review fails*.

## 7. When models disagree
If Aider pushes back with a technically valid point, bring it to the architect verbatim. The architect amends brief + ARCHITECTURE.md, or explains why the original stands. Architecture never changes silently inside an Aider run.
