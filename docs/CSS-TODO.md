# CSS-TODO.md — styling debt

Presentation work that is **not** behaviour: nothing here is a bug, and nothing here blocks a
task. It exists so styling gaps found while clicking through a feature get written down at the
moment they are noticed, instead of being rediscovered at the Phase 5 polish pass.

Rules that apply to everything below: tokens from `theme.css`, no forked values, no hardcoded
hex. The branding source of truth is `latentbeats.com` — change the site first, then follow it
(PROJECT.md decisions log, 2026-08-23).

---

## Lyrics Studio

- **The streamed-reasoning panel should read as reassuring, not as the app being stuck**
  (2026-08-27). It already caps and scrolls, so this is presentation only. It matters most on
  hosted reasoning models: T-302 measured **33 s before the first lyric character** on
  QwenCloud with reasoning unsuppressed, and that block is the only thing on screen for the
  whole wait (LLM-SURFACE §13.1). Whatever it becomes, it must stay obviously *secondary* to
  the lyric text — it is proof of life, not content.
