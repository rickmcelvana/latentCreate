# CSS-TODO.md — styling debt

Presentation work that is **not** behaviour: nothing here is a bug, and nothing here blocks a
task. It exists so styling gaps found while clicking through a feature get written down at the
moment they are noticed, instead of being rediscovered at the Phase 5 polish pass.

Rules that apply to everything below: tokens from `theme.css`, no forked values, no hardcoded
hex. The branding source of truth is `latentbeats.com` — change the site first, then follow it
(PROJECT.md decisions log, 2026-08-23).

---

## Setup wizard

- **The lyric-model list's scrollbar is unstyled** (found 2026-08-27, T-301b click-through).
  The list itself performs well — the producer's QwenCloud endpoint returns **163 models** and
  it "scrolls and loads good, no lag or stuttering" — but the scrollbar is the browser default
  against the app's dark ground, so it reads as un-themed the moment the list overflows. Wants
  `scrollbar-color` / `scrollbar-width`, or a `::-webkit-scrollbar` treatment, using existing
  tokens. Applies to any list that can overflow, so it is worth solving once as a shared rule
  rather than per view.

## Lyrics Studio

- **The streamed-reasoning panel should read as reassuring, not as the app being stuck**
  (2026-08-27). It already caps and scrolls, so this is presentation only. It matters most on
  hosted reasoning models: T-302 measured **33 s before the first lyric character** on
  QwenCloud with reasoning unsuppressed, and that block is the only thing on screen for the
  whole wait (LLM-SURFACE §13.1). Whatever it becomes, it must stay obviously *secondary* to
  the lyric text — it is proof of life, not content.
