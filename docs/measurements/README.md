# measurements

Raw data from live producer runs. Kept so a later run can be **compared** rather than re-argued.

## `t-314-vram-1hz.csv`

VRAM during T-314's full-length runs, 2026-08-30. RTX 5060 Ti, 15.93 GiB.

Sampled `GET /system_stats` once per second: **139 samples over 165 s, no gap wider than 2 s**.
Covers two 200 s ACE-Step generations (`1e9ca8a5`, `6c02fb79`).

Columns are raw bytes as ComfyUI reported them. `epoch_s` is Unix seconds.

What it shows:

| | `vram_free` | used |
|---|---|---|
| idle | 14.37 GiB | 1.56 GiB |
| plateau (~22 s, the sampler) | 4.68 GiB | 11.25 GiB |
| **peak** (decode spike) | **0.44 GiB** | **15.49 GiB** |

**The peak is not a VRAM requirement.** ComfyUI expands to fill free VRAM, so this measures the
card, not the model — see [MCP-SURFACE 30.3](../MCP-SURFACE.md). T-317 re-runs this under
`--reserve-vram` to get a real floor; keep its CSV here too and compare.

Two rows read `vram_free` **greater** than `vram_total` (17,104,398,164 vs 17,102,733,312), and
`torch_vram_total` reads 16.31 GiB on a 15.93 GiB card. Allocator accounting, not physical memory —
subtracting these unsigned will underflow (MCP-SURFACE 30.4).
