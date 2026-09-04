# CLAUDE.md

**Follow [AGENTS.md](AGENTS.md).** It defines the session ritual, the read order, the Aider-based
build loop, and the hard rules, and it is authoritative over this file. Do not duplicate its
guidance here.

**This file holds no project state.** Nothing about which phase is open, which task is next, or what
has landed belongs here — that is [PROJECT.md](PROJECT.md)'s Snapshot, which is updated every
session. A status line copied into an auto-loaded file goes stale silently, which is the one kind of
drift the session-start `git log` check is worst at catching.

## Start of every session

1. Read [PROJECT.md](PROJECT.md) — Snapshot first, then the last session-log entry.
2. Check PROJECT.md and [ARCHITECTURE.md](ARCHITECTURE.md) against `git log` since that entry. **Fix
   drift before starting new work.**
3. Read [AGENTS.md](AGENTS.md) for the rest of the read order and the hard rules.

## The three rules a session must not get wrong

Summarised here because a mistake in any of them is unrecoverable, and because a session that reads
only its auto-loaded file should still not make one. [AGENTS.md](AGENTS.md) and
[CONVENTIONS.md](CONVENTIONS.md) are authoritative.

- **Ship no models.** All generation goes through the user's own ComfyUI (via Comfy MCP) or their
  own API keys. Nothing is bundled, and nothing is downloaded except at the user's request.
- **Never rewrite a user's words silently.** Prompt and lyric text changes need an explicit accept
  step with the diff shown first.
- **Deletes go to the OS trash, never a hard delete** — and the trasher is injected as a parameter
  so `cargo test` cannot fill the developer's Recycle Bin.

## Build gate

`npm run gate` is the pre-commit check and mirrors CI. **Run it before every commit**; a green gate
is the go-ahead to commit, not a checkpoint to ask permission at.
