# T-314 — Phase 3 milestone verification (live)

**Lane: producer.** The architect's part is this checklist, the measurement plan, and recording
what comes back. The run itself is a person at the app.

## What is already discharged

The ROADMAP's five milestone lines have **all** been verified, each by its own dated click-through.
Listed with evidence so this task does not re-run work that is already recorded:

| line | discharged | evidence |
|---|---|---|
| tags + lyrics → queued job → track with a complete sidecar | 2026-08-29 | T-311 click-through, all five steps |
| a two-LoRA ACE-Step run reproduces from its sidecar alone | 2026-08-29 | MCP-SURFACE 27: *"the milestone bar is met and was checked against the engine rather than against our own tests"* |
| output is **lossless, not MP3** | 2026-08-28 | MCP-SURFACE 20 — template ships `mp3`/`V0`, executed prompt carries `flac`; re-confirmed in the T-315 API capture |
| an imported user workflow generates successfully | 2026-08-30 | T-313f click-through, step 5 |
| kill ComfyUI mid-job → clean failed state + retry | 2026-08-29 | T-315 + MCP-SURFACE 28.4, observed twice — once as the defect, once as the fix |

**So this task is not the checklist.** It is the two extras the phase file named, plus one thing the
table above cannot cover.

## The thing the table cannot cover

**T-313a changed the first step of `build_and_submit` for every profile, not just imported ones.**
The gallery-template arm now runs through `place_working_copy` rather than calling `fetch_template`
inline. Its tests were unchanged and pass — but **the only live generation since that refactor used
the imported path** (T-313f step 5). A shipped profile has not been generated from since the shared
code under it moved.

That is a small risk and a cheap check, and it is exactly the kind of gap a dated table hides:
every line above is true, and none of them was run against today's code.

## The runs

### 1. A shipped profile still generates (2 minutes)

ACE-Step or MiniMax, ordinary short generation, from the app. It should queue, run, and land in the
Library exactly as before. **This is a regression check on T-313a, not a milestone line.**

### 2. The full-length run

~~Every generation this project has ever made has been ~10 seconds.~~ **Corrected 2026-08-30 by
the producer: false.** The session log only ever recorded *app-driven* generations, and this brief
restated that as a fact about the project. The producer had been testing 2-minute-plus songs
routinely; `ACE_Step1.5_xl_turbo_00010.flac`, a **120 s** track from 05:44 that morning, was
sitting in the output folder while the brief claimed otherwise. What was true: **no full-length run
had been recorded, measured, or driven end-to-end through the app.** Set **duration to a real song
length — 180 s or more** — on a shipped ACE-Step profile, with lyrics attached, and let it run.

Watch for, and report:

- **Wall-clock time.** Nothing in the repo knows what a real generation costs.
- Whether the queue panel's elapsed clock stays sensible over minutes rather than seconds.
- Whether the track lands with the right duration in the Library (`duration_s` is read from the
  file itself, T-311a, so a mismatch would mean the encode disagreed with the request).
- Anything that degrades with length: progress reporting, the pump's poll interval, memory.

### 3. VRAM, and settling `vram_gb_min`

**The oldest open question in the repo.** `ace-step-1.5-turbo.json` declares `vram_gb_min: 8`; the
XL turbo DiT alone is 9.3 GiB, so the number has been suspect since Phase 1 and no run has ever
measured it.

**Method.** `system_stats` is read-only and safe to poll, so the architect polls it during run 2 and
records the **minimum `vram_free`** seen. Baseline captured 2026-08-30 before any run:

```
vram_total  17,102,733,312  (15.93 GiB)
vram_free   15,429,016,404  (14.37 GiB idle)
```

so roughly **1.56 GiB is already resident at idle**, before this app asks for anything.

**Two honesty limits on the number, and they decide how it is used:**

1. **Polling can miss the true peak** between samples, so the observed figure is a **lower bound**
   on peak usage. A minimum-VRAM requirement derived from a lower bound must therefore be **rounded
   up**, never taken as exact.
2. `vram_free` reflects the caching allocator's reservations (`cudaMallocAsync`), not live tensors
   alone. It answers "how much of the card was unavailable", which is the right question for a
   floor, but it is not "how much the model needs".

**What gets written down.** Peak observed usage, the card, and the run that produced it — then
`vram_gb_min` is set from it with a comment saying it is an observed floor on a 15.93 GiB card, or
**left at 8 with a recorded reason** if the run shows 8 is defensible. What must not happen is the
number changing on argument rather than measurement, which is how it got here.

## Out of scope

- **Re-running the five discharged lines.** Their evidence is dated and specific; run 1 is the only
  regression check this task carries.
- **Fixing whatever runs 1–3 find.** Fix-ups get their own numbers, as T-315 and T-313g did.
- **A second card.** One machine's number, honestly labelled, beats a general claim.

---

# Results — 2026-08-30

Full evidence: [docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md) §30.

## Run 1 — a shipped profile still generates: **PASS**

The gallery-template arm generates after T-313a moved `build_and_submit`'s first step to
`place_working_copy`. No regression.

## Run 2 — the full-length run: **PASS**

Four submissions, read from `GET /history` because `get_logs` was unusable (§30.5):

| prompt_id | wall clock | asked | delivered |
|---|---|---|---|
| `504652c6` | 0 s | 185 s | — `execution_cached`, never ran (§30.6) |
| `1609b9a5` | 36 s | 185 s | 185.00 s |
| `1e9ca8a5` | 40 s | 200 s | 200.00 s |
| `6c02fb79` | 39 s | 200 s | 200.00 s |

- **A 200 s song costs ~39 s** — about 5x realtime. The repo had no such number before.
- **Duration is exact.** Every sidecar's `duration_s` equals the STREAMINFO duration of its own
  FLAC, checked independently of the app. 48 kHz / 16-bit / stereo; the lossless swap holds on
  post-T-313a code.
- Nothing degraded with length. No stalls, no poll-interval trouble, no memory growth across four
  runs.

## Run 3 — VRAM: **measured, and the number is deliberately not changed**

1 Hz sampling of `GET /system_stats`, 139 samples over 165 s, no gap wider than 2 s. Both 200 s
runs traced the same curve: ~3 s ramp → ~22 s plateau at **11.25 GiB** → ~5 s decode spike → full
release.

**Peak: 15.49 GiB used of 15.93 GiB — the card at 97% full.**

**`vram_gb_min` stays at 8.** This brief's two honesty limits both made the figure a conservative
*lower bound* on the floor. A third, read off ComfyUI's startup banner, breaks that direction:
`DynamicVRAM`, `NORMAL_VRAM` and async weight offloading mean ComfyUI **expands to fill free VRAM**,
so an unconstrained run measures **the card, not the model**. The peak neither justifies raising the
number nor refutes 8. Changing it on this evidence would be exactly the "changing on argument rather
than measurement" this brief forbids. The constrained bisect that *can* settle it is **T-317**.

## Found on the way, both outside the runs

- **`get_logs` served a three-day-old log** for a different ComfyUI version while reporting
  `source: "explicit_port"` and `port_mismatch: false` — the two signals its own docs call
  trustworthy (§30.5).
- **An unchanged resubmission is cached and filed as a new track** — byte-identical audio, sidecars
  differing only in `created_at` and `prompt_id`. Not a provenance defect; a fresh Generate does not
  re-roll the seed. **T-316** (§30.6).
- **Three stale doc comments**, corrected with this entry: `profile.rs` called the import format
  API-format (T-313/§29 disproved that) and claimed `vram_gb_min` warns "before a doomed run";
  `health.rs` claimed `vram_bytes` is "the number a profile's `vram_gb_min` is checked against".
  Nothing compares them — the field is display text only.
