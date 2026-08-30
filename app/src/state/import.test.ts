import { describe, expect, it } from 'vitest'
import type { Candidate, RoleSuggestion } from '../bridge/import'
import {
  canSave,
  initialSelection,
  mappingsOf,
  roleRows,
  saveNotes,
  toggleAddress,
  type Selection,
} from './import'

function strong(address: string, nodeType = 'TextEncodeAceStepAudio1.5'): Candidate {
  return { address, node_type: nodeType, confidence: 'strong', reason: `on ${nodeType}` }
}

function possible(address: string, drives: string): Candidate {
  return {
    address,
    node_type: 'PrimitiveInt',
    confidence: 'possible',
    reason: `drives ${drives}`,
  }
}

/** ACE-Step's real shape: tags strong, duration two strong, seed only the hop. */
function aceSuggestions(): RoleSuggestion[] {
  return [
    { role: 'tags', candidates: [strong('94.tags')] },
    { role: 'lyrics', candidates: [strong('94.lyrics')] },
    {
      role: 'duration_seconds',
      candidates: [strong('94.duration'), strong('98.seconds', 'EmptyAceStep1.5LatentAudio')],
    },
    { role: 'seed', candidates: [possible('109.value', '3.seed, 94.seed')] },
    { role: 'steps', candidates: [strong('3.steps', 'KSampler')] },
  ]
}

describe('initialSelection', () => {
  /**
   * Protects: T-313c's confidence field is behaviour, not decoration.
   *
   * A `possible` candidate was reached by following a link -- `109.value` is
   * ACE-Step's seed because of the graph's shape, not because anything about
   * it says "seed". Pre-ticking it would make the app silently accept its own
   * guess as the user's mapping: they save, the profile is written, generation
   * works, and the seed they think they set is the one the app chose. Nothing
   * errors, which is exactly why this needs a test rather than a click.
   */
  it('never pre-selects a possible candidate', () => {
    const selection = initialSelection(aceSuggestions())

    expect(selection.seed).toBeUndefined()
    expect(selection.tags).toEqual(['94.tags'])
  })

  /** Protects: the one-role-many-slots case. ACE-Step's duration is two real
   * slots and both land, so both are ticked. */
  it('selects every strong candidate, including two for one role', () => {
    const selection = initialSelection(aceSuggestions())

    expect(selection.duration_seconds).toEqual(['94.duration', '98.seconds'])
  })

  it('leaves a role with no candidates unselected', () => {
    expect(initialSelection(aceSuggestions()).negative).toBeUndefined()
  })
})

describe('roleRows', () => {
  /** Protects: a role the app found nothing for is still visible. Hiding it
   * would leave a person unable to see what was not matched. */
  it('renders a row for a role with no candidates', () => {
    const rows = roleRows(aceSuggestions(), {})
    const negative = rows.find((r) => r.role === 'negative')

    expect(negative).toBeDefined()
    expect(negative?.options).toEqual([])
    expect(negative?.mapped).toBe(false)
    expect(negative?.emptyNote).toContain('Leave it unmapped')
  })

  /** Protects: the reason reaches the screen. A suggestion nobody can check is
   * one nobody should accept. */
  it('carries each candidate reason and checked state', () => {
    const rows = roleRows(aceSuggestions(), initialSelection(aceSuggestions()))
    const seed = rows.find((r) => r.role === 'seed')

    expect(seed?.options).toHaveLength(1)
    expect(seed?.options[0].checked).toBe(false)
    expect(seed?.options[0].candidate.reason).toContain('drives 3.seed')
    expect(seed?.mapped).toBe(false)
  })

  it('lists every role in reading order', () => {
    expect(roleRows([], {}).map((r) => r.role)).toEqual([
      'tags',
      'lyrics',
      'negative',
      'duration_seconds',
      'seed',
      'steps',
      'cfg',
    ])
  })
})

describe('toggleAddress', () => {
  it('adds and removes an address without disturbing other roles', () => {
    const start: Selection = { tags: ['94.tags'], steps: ['3.steps'] }

    const added = toggleAddress(start, 'seed', '109.value')
    expect(added.seed).toEqual(['109.value'])
    expect(added.tags).toEqual(['94.tags'])

    const removed = toggleAddress(added, 'seed', '109.value')
    expect(removed.seed).toBeUndefined()
    expect(removed.steps).toEqual(['3.steps'])
  })

  it('keeps a role with two addresses when only one is removed', () => {
    const both: Selection = { duration_seconds: ['94.duration', '98.seconds'] }
    expect(toggleAddress(both, 'duration_seconds', '94.duration').duration_seconds).toEqual([
      '98.seconds',
    ])
  })
})

describe('canSave', () => {
  it('needs a name and at least one mapping', () => {
    const mapped: Selection = { tags: ['94.tags'] }

    expect(canSave('My Import', mapped)).toBe(true)
    expect(canSave('', mapped)).toBe(false)
    expect(canSave('   ', mapped)).toBe(false)
    expect(canSave('My Import', {})).toBe(false)
  })

  /**
   * Protects: MCP-SURFACE 29.3, on the far side of the wire.
   *
   * T-313b already refuses to let warnings block an import in Rust. This is
   * the same rule where it would most plausibly be re-imposed by accident --
   * `canSave` takes no warnings argument at all, which is the design that
   * makes re-imposing it require a deliberate change.
   */
  it('does not consult warnings', () => {
    expect(canSave('My Import', { tags: ['94.tags'] })).toBe(true)
  })
})

describe('mappingsOf', () => {
  it('sends only mapped roles, in reading order', () => {
    const selection: Selection = { steps: ['3.steps'], tags: ['94.tags'] }
    expect(mappingsOf(selection)).toEqual([
      { role: 'tags', addresses: ['94.tags'] },
      { role: 'steps', addresses: ['3.steps'] },
    ])
  })
})

describe('saveNotes', () => {
  /**
   * Protects: the cost of the default path is stated.
   *
   * On an ACE-Step-shaped graph the seed is always the `possible` hop, so it
   * is never pre-ticked -- which means the default import has no seed, no seed
   * control, and a Variations button that queues N runs varying nothing.
   * Nothing errors, and the tracks differ anyway because ACE-Step is not
   * reproducible run-to-run, so nothing on screen would reveal it.
   */
  it('calls out an unmapped seed', () => {
    const notes = saveNotes({ tags: ['94.tags'] })

    expect(notes).toHaveLength(1)
    expect(notes[0]).toContain('Variations')
  })

  it('says nothing when the seed is mapped', () => {
    expect(saveNotes({ seed: ['109.value'] })).toEqual([])
  })

  /** Protects: advisory means advisory. Same rule as warnings. */
  it('never blocks saving', () => {
    const selected: Selection = { tags: ['94.tags'] }
    expect(saveNotes(selected)).not.toEqual([])
    expect(canSave('My Import', selected)).toBe(true)
  })
})
