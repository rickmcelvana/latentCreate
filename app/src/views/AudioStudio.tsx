import { useEffect } from 'react'
import { JobQueue } from '../components/JobQueue'
import { ParamPanel } from '../components/ParamPanel'
import { useConfigStore } from '../state/config'
import { useJobsStore } from '../state/jobs'
import { useModelsStore } from '../state/models'
import { useParamPanelStore } from '../state/paramPanel'
import {
  effectiveProfileId,
  pickable,
  profileRow,
  selectedProfile,
  type ProfileRow,
} from '../state/profiles'

export function AudioStudio() {
  const startListening = useJobsStore((state) => state.startListening)
  const view = useModelsStore((state) => state.view)
  const refresh = useModelsStore((state) => state.refresh)
  const config = useConfigStore((state) => state.config)
  const save = useConfigStore((state) => state.save)
  const load = useParamPanelStore((state) => state.load)

  useEffect(() => {
    void startListening()
  }, [startListening])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const effectiveId = effectiveProfileId(config)
  const selected = selectedProfile(view, config)
  const rows = pickable(view, 'music')

  useEffect(() => {
    void load(effectiveId)
  }, [effectiveId, load])

  return (
    <>
      <h1 className="view-title">Audio</h1>
      <p className="view-subtitle">
        Style tags, lyrics, and the settings worth changing.
      </p>

      <section className="panel profile-picker">
        <h2 className="profile-picker-title">Model profile</h2>

        {/* No fallback happens here, and the wording must not promise one:
            `effectiveProfileId` returns the configured id whether or not a
            profile answers to it, so generation would fail on that id rather
            than quietly using another model. Saying "falling back" would
            describe behaviour the app does not have. */}
        {selected === null && view !== null ? (
          <p className="profile-picker-fallback">
            The configured profile <code>{effectiveId}</code> is not among the loaded
            profiles. Pick one below to continue.
          </p>
        ) : null}

        {view !== null && !view.inventory_available ? (
          <p className="profile-picker-disclaimer">
            Readiness could not be checked because ComfyUI is not running.
          </p>
        ) : null}

        <ul className="profile-list">
          {rows.map((profile) => (
            <ProfilePickerRow
              key={profile.id}
              row={profileRow(profile)}
              selected={profile.id === effectiveId}
              onSelect={() => void save({ default_profile_id: profile.id })}
            />
          ))}
        </ul>
      </section>

      <ParamPanel />

      <JobQueue />
    </>
  )
}

function ProfilePickerRow({
  row,
  selected,
  onSelect,
}: {
  row: ProfileRow
  selected: boolean
  onSelect: () => void
}) {
  return (
    <li className={`profile-row ${selected ? 'profile-row-selected' : ''}`}>
      <label className="profile-row-pick">
        <input
          type="radio"
          name="profile"
          checked={selected}
          onChange={onSelect}
        />
        <span className="profile-row-name">{row.displayName}</span>
      </label>

      <div className="profile-row-meta">
        <span className={`status-pill status-pill-${row.readiness.tone}`}>
          {row.readiness.label}
        </span>
        <span className="profile-row-origin">{row.origin}</span>
        {row.vramClaim !== null ? (
          <span className="profile-row-vram">{row.vramClaim}</span>
        ) : null}
      </div>

      <p className="profile-row-license">
        <span className="profile-row-license-name">{row.license}</span>
        {row.licenseNotes !== null ? ` -- ${row.licenseNotes}` : null}
      </p>
    </li>
  )
}
