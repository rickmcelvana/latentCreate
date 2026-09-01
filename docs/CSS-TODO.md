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

## Library

- **The player is below the fold, so playing a track from the top of the list shows nothing**
  (2026-09-01). `<Player />` renders last in the Library view (`Library.tsx`), after the whole
  track list and the album panel. Click Play on a track near the top and its visualizer is all
  the way at the bottom — with the producer's 20 tracks you must scroll to find it, which most
  people will not do, so the app looks like it did nothing. Same lesson as the T-402 player
  error, the T-315 crash copy, and the T-408a refusal message, all moved to where the user is
  already looking: **a response to an interaction has to appear near the interaction.** Likely
  fixes are a sticky/persistent player bar or scrolling it into view on play; the choice is a
  layout decision worth a small task rather than a blind reposition. Behaviour is fine — the
  audio plays and the visualizer runs — so this is findability, not a bug.
