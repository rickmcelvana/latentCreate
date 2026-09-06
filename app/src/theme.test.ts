import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * CONVENTIONS: "Single `theme.css`; every className styled there."
 *
 * Written after three bugs of one shape landed in a single day (2026-09-06):
 * `.art-gallery` had no rule at all, so the Cover Art queue and gallery ran
 * together; `.job-panel` had no bottom gap because until Cover Art existed it
 * was always the last thing on its page; and `.album-panel` kept a `margin-top`
 * from the position it held before it was moved. None of them is visible to
 * `tsc`, oxlint or vitest, and all three reached a person.
 *
 * This catches only the first kind -- a className with no rule behind it -- but
 * that is the one that produces an element with no styling whatsoever, and it
 * is the one a machine can check.
 */

const SRC = join(import.meta.dirname, '.')
const THEME = join(SRC, 'theme.css')

/**
 * Classes used in TSX that deliberately have no rule.
 *
 * Each entry needs a reason, and the reason must be that the element is styled
 * by something else -- a parent's flex layout, typically -- rather than that
 * nobody got round to it. A class that looks unstyled *on screen* belongs in
 * `theme.css`, not here.
 */
const UNSTYLED_BY_DESIGN = new Map([
  [
    'import-role',
    'a list item whose spacing comes from `.import-roles`; the row itself paints nothing',
  ],
  [
    'track-details',
    'a wrapper inside `.track-row`, which is the flex column that spaces it',
  ],
])

/** Every `.tsx` file under `src/`, recursively. */
function tsxFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) return tsxFiles(path)
    return entry.name.endsWith('.tsx') ? [path] : []
  })
}

/**
 * Every class name a file asks for, including the ones inside a template
 * literal's interpolations.
 *
 * `${selected ? 'picker-row-selected' : ''}` is where half this app's state
 * classes live, so a scanner that stripped interpolations wholesale would check
 * the least-exercised half of the stylesheet and miss the rest. A token left
 * ending in `-` after stripping is a computed suffix
 * (`` `job-item-${row.status}` ``) and is checked as a prefix instead.
 */
export function classesIn(source: string): { exact: string[]; prefixes: string[] } {
  const exact = new Set<string>()
  const prefixes = new Set<string>()

  for (const match of source.matchAll(/className=(?:"([^"]*)"|\{`([^`]*)`\})/g)) {
    const raw = match[1] ?? match[2] ?? ''

    // Harvest the string literals inside `${...}` before dropping the rest.
    for (const interpolation of raw.matchAll(/\$\{[^}]*\}/g)) {
      for (const literal of interpolation[0].matchAll(/'([^']*)'|"([^"]*)"/g)) {
        for (const token of (literal[1] ?? literal[2] ?? '').split(/\s+/)) {
          if (/^[a-zA-Z][\w-]*$/.test(token)) exact.add(token)
        }
      }
    }

    for (const token of raw.replaceAll(/\$\{[^}]*\}/g, ' ').split(/\s+/)) {
      if (/^[a-zA-Z][\w-]*-$/.test(token)) prefixes.add(token)
      else if (/^[a-zA-Z][\w-]*$/.test(token)) exact.add(token)
    }
  }

  return { exact: [...exact], prefixes: [...prefixes] }
}

/** Every class `theme.css` defines a rule for. */
export function classesDefinedIn(css: string): Set<string> {
  return new Set([...css.matchAll(/\.([a-zA-Z][\w-]*)/g)].map((m) => m[1]))
}

describe('theme.css covers every className', () => {
  const defined = classesDefinedIn(readFileSync(THEME, 'utf8'))

  it('has a rule for every class used in a view or component', () => {
    const missing: string[] = []
    for (const file of tsxFiles(SRC)) {
      const { exact } = classesIn(readFileSync(file, 'utf8'))
      for (const name of exact) {
        if (defined.has(name) || UNSTYLED_BY_DESIGN.has(name)) continue
        missing.push(`.${name} (${file.slice(SRC.length + 1)})`)
      }
    }
    expect(
      missing,
      'add a rule to theme.css, or an entry with its reason to UNSTYLED_BY_DESIGN',
    ).toEqual([])
  })

  // `` `job-item-${row.status}` `` is only safe if *some* rule answers to it.
  it('has a rule for every computed class prefix', () => {
    const orphaned: string[] = []
    for (const file of tsxFiles(SRC)) {
      const { prefixes } = classesIn(readFileSync(file, 'utf8'))
      for (const prefix of prefixes) {
        const answered = [...defined].some((name) => name.startsWith(prefix) && name !== prefix)
        if (!answered) orphaned.push(`.${prefix}* (${file.slice(SRC.length + 1)})`)
      }
    }
    expect(orphaned).toEqual([])
  })

  // Keeps the exemption list honest: an entry that outlives its class is a
  // licence for the next unstyled one to hide behind it.
  it('carries no stale exemptions', () => {
    const used = new Set(
      tsxFiles(SRC).flatMap((file) => classesIn(readFileSync(file, 'utf8')).exact),
    )
    for (const [name] of UNSTYLED_BY_DESIGN) {
      expect(used.has(name), `${name} is exempted but no longer used`).toBe(true)
      expect(defined.has(name), `${name} is exempted but theme.css now styles it`).toBe(false)
    }
  })
})
