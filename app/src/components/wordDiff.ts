/**
 * The word diff behind `<PromptDiff>`.
 *
 * Pure and DOM-free so it can be tested without a renderer -- this repo has no
 * jsdom, and component rendering is producer click-through (WORKFLOW 5).
 *
 * Words, not lines. The texts being compared are assembled briefs
 * (`create-core::lyrics::assemble_user_message`), where an optimizer typically
 * rewrites the middle of a line and leaves its label alone; a line diff would
 * report every touched line as wholly replaced and show the user nothing about
 * what actually changed.
 */

/** What happened to one run of text. */
export type DiffKind = 'same' | 'added' | 'removed'

/** One run of text, and whether it survived the rewrite. */
export interface DiffSpan {
  kind: DiffKind
  text: string
}

/**
 * Token ceiling for the quadratic pass.
 *
 * A brief is well under a hundred tokens, so this is a guard against a pasted
 * novel rather than a limit anyone should meet. The table is
 * `(n + 1) * (m + 1)` cells; at this cap that is about 9 MB of `Uint32Array`,
 * which is the reason a ceiling exists at all.
 */
const MAX_TOKENS = 1500

/**
 * Split text into words and whitespace runs.
 *
 * Whitespace is kept as its own token so the spans can be concatenated back
 * into the original text -- the rendered diff is the text, not a summary of it.
 */
export function tokenize(text: string): string[] {
  return text.match(/\s+|\S+/g) ?? []
}

/**
 * Diff two texts into a single ordered span list.
 *
 * Longest-common-subsequence over tokens: `same` runs appear once, and a
 * rewrite shows as `removed` followed by `added`. Reading the `same`+`removed`
 * spans reconstructs `original` exactly; `same`+`added` reconstructs `revised`.
 *
 * Above [`MAX_TOKENS`] the result degrades to one whole-text replacement rather
 * than allocating a table nobody asked for. That is a worse diff, not a wrong
 * one: both panes still show their own text in full.
 */
export function wordDiff(original: string, revised: string): DiffSpan[] {
  const a = tokenize(original)
  const b = tokenize(revised)

  if (a.length > MAX_TOKENS || b.length > MAX_TOKENS) {
    return merge([
      { kind: 'removed', text: original },
      { kind: 'added', text: revised },
    ])
  }

  const n = a.length
  const m = b.length
  const width = m + 1
  // Length of the longest common subsequence of a[i..] and b[j..].
  const lcs = new Uint32Array((n + 1) * width)
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i * width + j] =
        a[i] === b[j]
          ? lcs[(i + 1) * width + (j + 1)] + 1
          : Math.max(lcs[(i + 1) * width + j], lcs[i * width + (j + 1)])
    }
  }

  const spans: DiffSpan[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      spans.push({ kind: 'same', text: a[i] })
      i++
      j++
    } else if (lcs[(i + 1) * width + j] >= lcs[i * width + (j + 1)]) {
      spans.push({ kind: 'removed', text: a[i] })
      i++
    } else {
      spans.push({ kind: 'added', text: b[j] })
      j++
    }
  }
  while (i < n) {
    spans.push({ kind: 'removed', text: a[i] })
    i++
  }
  while (j < m) {
    spans.push({ kind: 'added', text: b[j] })
    j++
  }

  return merge(spans)
}

/** The spans that make up the original text: what was kept, plus what went. */
export function originalSpans(spans: DiffSpan[]): DiffSpan[] {
  return spans.filter((span) => span.kind !== 'added')
}

/** The spans that make up the revised text: what was kept, plus what arrived. */
export function revisedSpans(spans: DiffSpan[]): DiffSpan[] {
  return spans.filter((span) => span.kind !== 'removed')
}

/**
 * Whether the rewrite changed anything at all.
 *
 * A model that hands back the brief verbatim is a normal outcome, and one the
 * UI should name -- an all-`same` diff otherwise reads as a broken view.
 */
export function hasChanges(spans: DiffSpan[]): boolean {
  return spans.some((span) => span.kind !== 'same')
}

/** Join neighbouring spans of the same kind, so a rewritten phrase is one run. */
function merge(spans: DiffSpan[]): DiffSpan[] {
  const merged: DiffSpan[] = []
  for (const span of spans) {
    if (span.text === '') continue
    const last = merged[merged.length - 1]
    if (last !== undefined && last.kind === span.kind) {
      last.text += span.text
    } else {
      merged.push({ ...span })
    }
  }
  return merged
}
