# T-317 — settle `vram_gb_min` by starving the card, not by watching it

**Lane: producer run, then a one-line profile edit.** Evidence:
[docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md) §30.2–30.3.

## Why T-314 could not settle this

T-314 measured a 200 s ACE-Step run at 1 Hz and found a **peak of 15.49 GiB used of 15.93 GiB** —
the card at 97% full. That figure cannot become `vram_gb_min`.

The T-314 brief carried two honesty limits, both saying the measurement is a **conservative lower
bound** on the floor. A third limit, read off ComfyUI's own startup banner, breaks the direction of
both:

```
Set vram state to: NORMAL_VRAM
Using async weight offloading with 2 streams
DynamicVRAM support detected and enabled
Model ACEStep15 prepared for dynamic VRAM loading. 9510MB Staged.
```

ComfyUI **stages models and expands to fill whatever VRAM is free**, offloading to host RAM when it
is not. So an unconstrained run measures **the card, not the model**: on a 12 GiB card the same run
would likely report a peak near 12. The observed 15.49 GiB neither supports raising `vram_gb_min`
nor refutes the existing 8.

`ace-step-1.5-turbo.json` therefore **stays at 8**, with that reason recorded, until this task runs.
It is the oldest open question in the repo and it has now survived one honest attempt.

## The measurement

Starve the process and see what still completes. `--reserve-vram <GB>` makes ComfyUI hold back that
much, so the effective budget is roughly `15.93 - reserve`.

1. Relaunch ComfyUI with `--reserve-vram 8` → effective budget ≈ 8 GiB. Run a **200 s** ACE-Step
   generation from the app.
2. Record: does it complete, what is the wall clock (offload is slow — expect minutes, not 39 s),
   and does the FLAC still land at 200.00 s.
3. If it completes, tighten (`--reserve-vram 10`, ≈ 6 GiB) until it fails. If it fails at 8, loosen
   until it passes.
4. The last budget that **completes a 200 s run** is the measured floor. Round **up** to the next
   whole GiB.

Poll `GET /system_stats` at 1 Hz throughout, as T-314 did, and keep the CSV in
`docs/measurements/`. T-314's is already there --
[`t-314-vram-1hz.csv`](../docs/measurements/t-314-vram-1hz.csv), the unconstrained baseline this
run is compared against.

## What to write down

The floor, the card, the reserve setting that produced it, and the wall clock at that budget — then
set `vram_gb_min` from it. **A number that only works at 6x the wall clock is worth recording as
such**, because the field's job is to tell a person whether to bother, and "runs, but takes eleven
minutes" is a different answer from "will not run".

## Before writing any gate on this number

`vram_gb_min` currently **gates nothing** — it renders as `Profile states N GB VRAM`
(`app/src/state/profiles.ts:77`) and no code compares it to `vram_bytes`. If this task adds a check,
note MCP-SURFACE §30.4: after a job releases, `vram_free` has been observed **larger than
`vram_total`**, so `vram_total - vram_free` in unsigned arithmetic underflows.

Also settle `minimax-music-3.json`, which declares **16** on a card of 15.93 GiB it has generated on
repeatedly — so at least one declared number is already known to be wrong.

## Out of scope

- A second card. One machine's number, honestly labelled.
- The seed/duplicate question — that is T-316.

---

# Results — 2026-08-30

Full evidence: [docs/MCP-SURFACE.md](../docs/MCP-SURFACE.md) §31; CSVs in
`docs/measurements/t317-vram-reserve{8,10,12,14,15}.csv`.

| reserve | effective budget | peak used | wall clock | completed |
|---|---|---|---|---|
| 8 | ~8 GiB | 9.03 GiB | 259 s | yes |
| 10 | ~6 GiB | 7.03 GiB | 443 s | yes |
| 12 | ~4 GiB | 5.00 GiB | 546 s | yes |
| 14 | ~2 GiB | 4.64 GiB | 698 s | yes |
| 15 | ~1 GiB | 2.94 GiB | 702 s | yes |

**The finding: ACE-Step never fails — it offloads and slows down.** Every budget down to an
effective ~1 GiB completed a full 200 s run. The wall clock climbs monotonically (259 → 702 s,
~2.7x) as the card is starved, but there is no hard floor: `DynamicVRAM` + async weight offloading
mean the model streams weights from host RAM and keeps going. The peak *used* figure **falls** as
the budget tightens (9.03 → 2.94 GiB), which is the allocator obeying the reserve, not the model
needing less.

**`vram_gb_min` stays at 8**, and its meaning is now measured rather than assumed: it is a
**comfort floor** — below it the model still runs, but at a wall clock that climbs toward 12
minutes for a 200 s song. The brief's own caveat ("runs, but takes eleven minutes" is a different
answer from "will not run") turned out to be the whole answer.

**`minimax-music-3.json` declares `vram_gb_min: 16` on a 15.93 GiB card it has generated on
repeatedly** — so that number is already known to be wrong in the *other* direction, and this
bisect does not touch it. Both fields remain display text only; nothing gates on them.
