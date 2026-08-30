# T-316 — a fresh Generate re-rolls the seed (or says why it does not)

**Lane: owner decision first, then a small frontend change.** Evidence:
[docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md) §30.6.

## What was observed

T-314's producer clicked Generate a second time without changing anything. ComfyUI returned
`execution_cached` in **0 s** — `execution_start == execution_success` — and re-served the previous
output. The app ingested that as a new Library track.

The two tracks are byte-identical:

```
tr-0017.flac  6bc4fbef34d752ab27231f652557a1ca
tr-0018.flac  6bc4fbef34d752ab27231f652557a1ca
```

and their sidecars differ in **exactly two fields**, `created_at` and `prompt_id`. Same seed, same
inputs, same resolved slots.

## What this is *not*

- **Not a provenance defect.** The sidecar promises that these inputs produced this waveform. They
  did. Reproducing from either sidecar yields this file.
- **Not a contradiction of MCP-SURFACE 17.3** (two ACE-Step runs differ in 98.1% of bytes). Nothing
  re-executed, so there was no second run to differ from. Under 17.3 the identical bytes are the
  *proof* the cache path was taken.
- **Not a ComfyUI bug.** Caching an unchanged graph is correct and it is why the click cost 0 s.

## What it is

**A fresh submission does not re-roll the seed.** T-312 gave each track *within a batch* its own
seed; two separate Generate clicks were never covered by any task. The result is a duplicate
Library entry for zero GPU time, which surprised the producer enough to report it — which is the
signal that the current behaviour is not the intended one.

## The decision (owner)

Three defensible options, and this task should not pick one silently:

1. **Re-roll on every submit unless the user pinned the seed.** Matches the expectation that
   Generate generates. Costs the ability to re-run an identical render by clicking twice.
2. **Keep the seed, and refuse the duplicate** — detect that nothing changed and tell the user so,
   rather than writing a second track.
3. **Keep the seed, ingest it, and label it** — mark the row as a cached repeat of an existing
   track.

Option 1 is the recommendation: the seed control already exists for anyone who wants determinism,
and the current behaviour makes the *default* path the surprising one. But 2 and 3 preserve the
"click twice, get the same render" property, which is worth something for a lyric the user is
iterating on.

## Scope once decided

Frontend only, most likely — the seed is resolved into the spec before `generate_audio`. Whichever
option is chosen, **the pre-existing seed control must keep working**: a user who typed a seed gets
that seed. Add a test that two consecutive submissions with no input change produce whatever the
decision says they produce, because nothing in the suite covers a second click today.

## Out of scope

- Deduplicating the Library retrospectively. `tr-0018` stays; it is real evidence.
- Anything about VRAM — that is T-317.
