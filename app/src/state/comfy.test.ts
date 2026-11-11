import { describe, expect, it } from 'vitest'
import type { ComfyStatus } from '../bridge/comfy'
import { formatVram, pillFor } from './comfy'

const DEFAULTS = ['http://127.0.0.1:8188', 'http://127.0.0.1:8000']

/** Every state the backend can report, so the sweep below cannot go stale. */
const ALL_STATES: ComfyStatus[] = [
  { state: 'not_found', tried: DEFAULTS, configured: false },
  { state: 'not_found', tried: ['http://gpu-box:8188'], configured: true },
  { state: 'unreachable', url: 'http://127.0.0.1:8000', detail: 'That is not a ComfyUI.' },
  {
    state: 'ready',
    url: 'http://127.0.0.1:8188',
    vram_bytes: 17102733312,
    comfyui_version: '0.45.2',
    local: true,
  },
]

describe('pillFor', () => {
  /**
   * Protects a product rule, not a rendering detail: CONVENTIONS requires
   * user-facing errors to say what to do next, not just what failed. Adding a
   * degraded state without a next step fails here.
   */
  it('gives every degraded state a next step', () => {
    for (const status of ALL_STATES) {
      const pill = pillFor(status)
      if (status.state === 'ready') {
        expect(pill.nextStep).toBeNull()
        expect(pill.tone).toBe('ok')
      } else {
        expect(pill.nextStep, `${status.state} must say what to do next`).not.toBeNull()
        expect(pill.nextStep).not.toBe('')
        expect(pill.tone).toBe('warn')
      }
    }
  })

  /**
   * Protects: the state the owner's other PCs were in. Nothing listening names
   * every address tried and says to start ComfyUI -- or where to type its
   * address when it lives somewhere else.
   */
  it('names every address tried when nothing answered', () => {
    const pill = pillFor(ALL_STATES[0])
    expect(pill.label).toBe('ComfyUI not found')
    for (const url of DEFAULTS) expect(pill.nextStep).toContain(url)
    expect(pill.nextStep).toContain('Start ComfyUI')
    expect(pill.nextStep).toContain('address')
  })

  /** Protects: a configured address that does not answer says to check it,
   * not to start ComfyUI on this machine. */
  it('points at the configured address when only it was tried', () => {
    const pill = pillFor(ALL_STATES[1])
    expect(pill.nextStep).toContain('http://gpu-box:8188')
    expect(pill.nextStep).toContain('address is right')
  })

  /** Protects: the reason a connection failed is not swallowed. */
  it('carries the failure detail when unreachable', () => {
    const pill = pillFor(ALL_STATES[2])
    expect(pill.nextStep).toContain('That is not a ComfyUI.')
  })

  /** Protects: the ready pill names where ComfyUI is, so a user running two
   * installs can tell which one answered. */
  it('shows the url when ready', () => {
    const pill = pillFor(ALL_STATES[3])
    expect(pill.label).toContain('http://127.0.0.1:8188')
    expect(pill.tone).toBe('ok')
  })

  /** Protects: the pre-check state is neutral, not a failure. A red pill
   * before the first check has even returned reads as broken. */
  it('is neutral before the first check returns', () => {
    const pill = pillFor(null)
    expect(pill.tone).toBe('neutral')
    expect(pill.nextStep).toBeNull()
  })
})

describe('formatVram', () => {
  /**
   * Protects: unknown stays unknown. A CPU-only ComfyUI reports no GPU, and
   * rendering that absence as `0.0 GiB` puts a hardware warning on a perfectly
   * working machine.
   */
  it('returns null for unknown or zero, never a zero reading', () => {
    expect(formatVram(null)).toBeNull()
    expect(formatVram(0)).toBeNull()
  })

  /** Protects: the unit. The captured card reports 17102733312 bytes, which
   * every GPU tool calls 16 GB and is 15.9 GiB -- so the number shown must
   * match what the user sees elsewhere. */
  it('formats the captured card as GiB', () => {
    expect(formatVram(17102733312)).toBe('15.9 GiB VRAM')
  })
})
