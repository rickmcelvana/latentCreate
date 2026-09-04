import { useEffect } from 'react'
import { GenerateBar } from '../components/GenerateBar'
import { ImportWorkflow } from '../components/ImportWorkflow'
import { JobQueue } from '../components/JobQueue'
import { LoraStack } from '../components/LoraStack'
import { ParamPanel } from '../components/ParamPanel'
import { ProfilePickerRow } from '../components/ProfilePickerRow'
import { useConfigStore } from '../state/config'
import { useJobsStore } from '../state/jobs'
import { useLoraPanelStore } from '../state/loraPanel'
import { useModelsStore } from '../state/models'
import { useParamPanelStore } from '../state/paramPanel'
import {
  effectiveProfileId,
  pickable,
  profileRow,
  selectedProfile,
} from '../state/profiles'

export function AudioStudio() {
  const startListening = useJobsStore((state) => state.startListening)
  const view = useModelsStore((state) => state.view)
  const refresh = useModelsStore((state) => state.refresh)
  const config = useConfigStore((state) => state.config)
  const save = useConfigStore((state) => state.save)
  const load = useParamPanelStore((state) => state.load)
  const loadLoras = useLoraPanelStore((state) => state.load)

  useEffect(() => {
    void startListening()
  }, [startListening])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const effectiveId = effectiveProfileId(config)
  const selected = selectedProfile(view, config)
  const rows = pickable(view, 'music')
  const names = Object.fromEntries(rows.map((p) => [p.id, p.display_name]))

  useEffect(() => {
    void load(effectiveId)
  }, [effectiveId, load])

  useEffect(() => {
    void loadLoras(effectiveId)
  }, [effectiveId, loadLoras])

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
              group="profile"
              onSelect={() => void save({ default_profile_id: profile.id })}
            />
          ))}
        </ul>

        <ImportWorkflow />
      </section>

      <ParamPanel store={useParamPanelStore} />

      <LoraStack />

      <GenerateBar />

      <JobQueue names={names} />
    </>
  )
}
