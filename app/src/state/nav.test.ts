import { describe, expect, it, beforeEach } from 'vitest'
import { NAV_ITEMS, useNavStore, type ViewId } from './nav'

describe('nav store', () => {
  beforeEach(() => {
    useNavStore.setState({ activeView: 'setup' })
  })

  it('test_initial_view_is_setup', () => {
    expect(useNavStore.getState().activeView).toBe('setup')
  })

  it('test_set_view_changes_active_view', () => {
    useNavStore.getState().setView('lyrics')
    expect(useNavStore.getState().activeView).toBe('lyrics')

    useNavStore.getState().setView('library')
    expect(useNavStore.getState().activeView).toBe('library')
  })

  it('test_nav_items_are_unique_and_ordered', () => {
    const ids = NAV_ITEMS.map((item) => item.id)
    const uniqueIds = [...new Set(ids)]

    expect(uniqueIds).toHaveLength(ids.length)

    const expected: ViewId[] = ['setup', 'lyrics', 'audio', 'library', 'art']
    expect(ids).toEqual(expected)
  })
})
