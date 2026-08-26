import { describe, expect, it } from 'vitest'
import {
  hasChanges,
  originalSpans,
  revisedSpans,
  tokenize,
  wordDiff,
  type DiffSpan,
} from './wordDiff'

/** The text one pane renders, reassembled from its spans. */
function render(spans: DiffSpan[]): string {
  return spans.map((span) => span.text).join('')
}

describe('wordDiff', () => {
  /**
   * Protects: whitespace survives tokenizing. The spans are concatenated back
   * into the rendered text, so a tokenizer that dropped the gaps would show the
   * user a brief with every line run together.
   */
  it('test_tokenize_keeps_whitespace_as_its_own_token', () => {
    expect(tokenize('a night\ndrive')).toEqual(['a', ' ', 'night', '\n', 'drive'])
    expect(tokenize('')).toEqual([])
    expect(tokenize('   ')).toEqual(['   '])
  })

  /**
   * The invariant the whole component rests on: the two panes are the two
   * texts. If either side does not reassemble exactly, the diff is showing the
   * user something other than the prompt they are being asked to accept -- and
   * they would be accepting the text, not the picture of it.
   */
  it('test_panes_reassemble_both_texts_exactly', () => {
    const original =
      'Theme: A night drive out of a city you are leaving for good\nMood: bittersweet, hopeful\nTarget duration: 120 seconds\n'
    const revised =
      'Theme: A rain-slick night drive out of a coastal city you are leaving for good\nMood: bittersweet, hopeful, resigned\nTarget duration: 120 seconds\n'

    const spans = wordDiff(original, revised)
    expect(render(originalSpans(spans))).toBe(original)
    expect(render(revisedSpans(spans))).toBe(revised)
  })

  /**
   * Protects: an unchanged word is not reported as changed. A diff that marked
   * whole lines would highlight the settings lines the optimizer was told to
   * leave alone, and the user would stop trusting the highlighting exactly
   * where it matters most.
   */
  it('test_untouched_words_stay_same_and_only_the_edit_is_marked', () => {
    const spans = wordDiff('Mood: bittersweet, hopeful', 'Mood: bittersweet, resigned')
    expect(spans.filter((span) => span.kind === 'removed')).toEqual([
      { kind: 'removed', text: 'hopeful' },
    ])
    expect(spans.filter((span) => span.kind === 'added')).toEqual([
      { kind: 'added', text: 'resigned' },
    ])
    expect(render(spans.filter((span) => span.kind === 'same'))).toBe('Mood: bittersweet, ')
  })

  /**
   * Protects: identical texts produce no change at all, which is what the UI
   * uses to say the model returned the brief unchanged rather than render an
   * empty-looking diff.
   */
  it('test_identical_texts_have_no_changes', () => {
    const spans = wordDiff('Theme: a night drive', 'Theme: a night drive')
    expect(hasChanges(spans)).toBe(false)
    expect(spans).toEqual([{ kind: 'same', text: 'Theme: a night drive' }])
  })

  /** Protects: an empty side is a whole-text insertion or deletion, not a crash. */
  it('test_an_empty_side_is_a_whole_text_change', () => {
    expect(wordDiff('', 'Theme: a night drive')).toEqual([
      { kind: 'added', text: 'Theme: a night drive' },
    ])
    expect(wordDiff('Theme: a night drive', '')).toEqual([
      { kind: 'removed', text: 'Theme: a night drive' },
    ])
    expect(wordDiff('', '')).toEqual([])
  })

  /**
   * Protects: neighbouring spans of one kind are merged. Unmerged spans render
   * as separate highlight boxes with visible seams between the words of a
   * single rewritten phrase.
   */
  it('test_adjacent_spans_of_one_kind_are_merged', () => {
    const spans = wordDiff('Mood: hopeful', 'Mood: quietly hopeful and tired')
    expect(spans).toEqual([
      { kind: 'same', text: 'Mood: ' },
      { kind: 'added', text: 'quietly ' },
      { kind: 'same', text: 'hopeful' },
      { kind: 'added', text: ' and tired' },
    ])
  })

  /**
   * Protects: the token ceiling degrades to a whole-text replacement instead of
   * allocating a table sized by the paste. Both panes must still show their own
   * text in full -- a worse diff is acceptable, a missing one is not.
   */
  it('test_a_text_over_the_token_ceiling_falls_back_to_a_whole_text_change', () => {
    const original = 'word '.repeat(2000)
    const revised = `${original}tail`
    const spans = wordDiff(original, revised)

    expect(spans).toEqual([
      { kind: 'removed', text: original },
      { kind: 'added', text: revised },
    ])
    expect(render(originalSpans(spans))).toBe(original)
    expect(render(revisedSpans(spans))).toBe(revised)
  })
})
