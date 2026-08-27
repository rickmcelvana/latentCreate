import type { Config } from '../bridge/config'
import type { ModelsView, ProfileStatus } from '../bridge/models'
import { curatedFirst, rowFor, type RowView } from './models'

/** The profile used when none has been chosen -- the app's default model. */
export const DEFAULT_PROFILE_ID = 'ace-step-1.5-turbo'

/**
 * Which profile the studios are working against.
 *
 * The configured id wins. The default is what a user who has never opened the
 * picker gets, not a value they are stuck with.
 */
export function effectiveProfileId(config: Config | null): string {
  const stored = config?.default_profile_id ?? null
  return stored !== null && stored.trim() !== '' ? stored : DEFAULT_PROFILE_ID
}

/**
 * The profiles this picker offers, in the order it offers them.
 *
 * `kind` is a parameter rather than a hardcoded `'music'` because CoverArt
 * (Phase 5) wants the same list filtered the other way, and two nearly
 * identical filters is how they drift.
 */
export function pickable(view: ModelsView | null, kind: ProfileStatus['kind']): ProfileStatus[] {
  return curatedFirst((view?.profiles ?? []).filter((p) => p.kind === kind))
}

/**
 * The configured profile, when it is still one of the loaded ones.
 *
 * `null` while the list has not loaded, and also when the configured id
 * names a profile that is no longer there -- a user profile deleted from disk,
 * or a shipped one renamed. The caller must say so rather than quietly
 * substituting the default: silently swapping the model a user chose is the
 * same fault as carrying a stale verified fact (T-302b), one level up.
 */
export function selectedProfile(
  view: ModelsView | null,
  config: Config | null,
): ProfileStatus | null {
  const id = effectiveProfileId(config)
  return (view?.profiles ?? []).find((p) => p.id === id) ?? null
}

/** One picker row, as the view renders it. */
export interface ProfileRow {
  id: string
  displayName: string
  /** Never null. Users ship these tracks commercially (T-111, CONVENTIONS). */
  license: string
  licenseNotes: string | null
  /** "Shipped" / "Yours" -- a user profile is otherwise indistinguishable. */
  origin: string
  /** The profile's own claim, worded as a claim. Null when undeclared. */
  vramClaim: string | null
  readiness: RowView
}

/**
 * Describe one row.
 *
 * A selector rather than JSX because it is the only way the licence rule is
 * testable here: this repo runs vitest in `node` with no DOM, so "assert the
 * licence reached the screen" is not a test that can be written (T-301b
 * learned this the expensive way). Putting the fields in a value moves the
 * rule somewhere a test reaches, and leaves the view a dumb renderer.
 */
export function profileRow(profile: ProfileStatus): ProfileRow {
  return {
    id: profile.id,
    displayName: profile.display_name,
    license: profile.license,
    licenseNotes: profile.license_notes,
    origin: profile.source === 'shipped' ? 'Shipped' : 'Yours',
    vramClaim:
      profile.vram_gb_min === null ? null : `Profile states ${profile.vram_gb_min} GB VRAM`,
    readiness: rowFor(profile.readiness),
  }
}
