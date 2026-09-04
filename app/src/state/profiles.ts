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
 * The image profile Cover Art is working against, or `null` when none is chosen.
 *
 * **No default, deliberately.** `effectiveProfileId` can fall back to
 * `DEFAULT_PROFILE_ID` because `ace-step-1.5-turbo` ships; the app ships no image
 * profile at all, so there is nothing to fall back to and inventing one would
 * generate with a model the user never picked. `null` is the view's cue to say
 * so and point at the Setup catalog.
 */
export function effectiveImageProfileId(config: Config | null): string | null {
  const stored = config?.default_image_profile_id ?? null
  if (stored === null) return null
  const trimmed = stored.trim()
  return trimmed === '' ? null : trimmed
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

/**
 * The chosen image profile, when it is still one of the loaded ones.
 *
 * `null` while the list has not loaded, when nothing is chosen, and when the
 * configured id names a profile that is no longer there -- a deleted or renamed
 * user profile. The caller says which, rather than substituting another model.
 *
 * **Resolved against the image profiles, not all of them.**
 * `default_image_profile_id` is its own field, and `config.json` is a file a
 * user can open: an id naming a *music* profile must read as "not one of these"
 * rather than as a chosen model, or Cover Art would show that model's panel and
 * only `generate_image`'s kind guard (T-506b) would stop it, at submit.
 */
export function selectedImageProfile(
  view: ModelsView | null,
  config: Config | null,
): ProfileStatus | null {
  const id = effectiveImageProfileId(config)
  if (id === null) return null
  return pickable(view, 'image').find((p) => p.id === id) ?? null
}

/**
 * What Cover Art can say for itself right now.
 *
 * `loading` -- the profile list has not come back.
 * `no-profiles` -- it came back with no image profiles at all; nothing to pick.
 * `none-chosen` -- image profiles exist and the user has not chosen one. There
 *   is no default to fall back on (see `effectiveImageProfileId`), so this is a
 *   real state rather than a moment before one.
 * `missing` -- an id is configured that no loaded profile answers to: a user
 *   profile deleted from disk, or renamed. Named rather than silently
 *   re-picked, the same rule the Audio Studio's fallback note follows.
 * `ready` -- a chosen profile is loaded.
 */
export type ImageStudioState = 'loading' | 'no-profiles' | 'none-chosen' | 'missing' | 'ready'

export function imageStudioState(view: ModelsView | null, config: Config | null): ImageStudioState {
  if (view === null) return 'loading'
  if (pickable(view, 'image').length === 0) return 'no-profiles'
  const chosen = selectedImageProfile(view, config)
  if (chosen === null) {
    const id = effectiveImageProfileId(config)
    return id === null ? 'none-chosen' : 'missing'
  }
  return 'ready'
}

/**
 * The sentence for a state, or `null` when there is nothing to say.
 *
 * `id` is the configured id, needed only by `missing` -- naming it is what lets
 * a user find the profile they renamed.
 */
export function imageStudioNote(state: ImageStudioState, id: string | null): string | null {
  switch (state) {
    case 'loading':
      return null
    case 'no-profiles':
      return 'No image model profile yet. Bring one in from the model catalog in Setup.'
    case 'none-chosen':
      return 'Pick an image model to start.'
    case 'missing':
      return `The configured image profile ${id} is not among the loaded profiles. Pick one below to continue.`
    case 'ready':
      return null
  }
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
