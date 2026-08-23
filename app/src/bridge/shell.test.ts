import { describe, expect, it } from 'vitest'
import { isTauri } from './shell'

describe('bridge/shell', () => {
  it('test_is_tauri_false_outside_webview', () => {
    // The test runner is plain Node: no Tauri internals are injected, so the
    // guard that keeps browser builds from calling invoke must report false.
    expect(isTauri()).toBe(false)
  })
})
