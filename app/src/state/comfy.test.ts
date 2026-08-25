import { describe, expect, it } from 'vitest'
import type { ComfyStatus } from '../bridge/comfy'
import { formatVram, pillFor } from './comfy'

/** Every state the backend can report, so the sweep below cannot go stale. */
const ALL_STATES: ComfyStatus[] = [
  { state: 'not_installed', install_command: 'pip install comfy-mcp' },
  { state: 'unreachable', detail: 'connection closed.' },
  { state: 'server_down', workspace: 'C:/Comfy/ComfyUI' },
  {
    state: 'ready',
    url: 'http://127.0.0.1:8188',
    vram_bytes: 17102733312,
    workspace: 'C:/Comfy/ComfyUI',
    comfy_cli_version: '1.16.0',
    update_available: true,
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

  /** Protects: the install command reaches the user verbatim, so it is copyable. */
  it('quotes the install command when comfy-mcp is missing', () => {
    const pill = pillFor({ state: 'not_installed', install_command: 'pip install comfy-mcp' })
    expect(pill.nextStep).toContain('pip install comfy-mcp')
  })

  /** Protects: the reason a connection failed is not swallowed. */
  it('carries the failure detail when unreachable', () => {
    const pill = pillFor({ state: 'unreachable', detail: 'spawn failed.' })
    expect(pill.nextStep).toContain('spawn failed.')
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
   * Protects: unknown stays unknown. comfy-cli does not always report a GPU,
   * and rendering that absence as `0.0 GiB` puts a hardware warning on a
   * perfectly working machine.
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
