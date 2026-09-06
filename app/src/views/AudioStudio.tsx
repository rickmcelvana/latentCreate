import { useEffect } from 'react'
import { ApprovedLyricOffer } from '../components/ApprovedLyricOffer'
import { GenerateBar } from '../components/GenerateBar'
import { JobQueue } from '../components/JobQueue'
import { LoraStack } from '../components/LoraStack'
import { ParamPanel } from '../components/ParamPanel'
import { QuickSwap } from '../components/QuickSwap'
import { useConfigStore } from '../state/config'
import { useJobsStore } from '../state/jobs'
import { useLoraPanelStore } from '../state/loraPanel'
import { useModelsStore } from '../state/models'
import { useParamPanelStore } from '../state/paramPanel'
import {
  effectiveProfileId,
  installedOptions,
  optionsNote,
  pickable,
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

      {/* A menu, not a list: the model is chosen and installed on Setup, and
          this is the fast swap between what is already installed. Every fact
          that decides a choice -- licence, VRAM, readiness, install -- stays
          on Setup rather than being half-repeated here. */}
      <section className="panel quick-swap-panel">
        <QuickSwap
          label="Model"
          value={selected === null ? null : effectiveId}
          options={installedOptions(view, 'music', effectiveId)}
          onChange={(id) => void save({ default_profile_id: id })}
          note={
            selected === null && view !== null
              ? `The configured profile ${effectiveId} is not among the loaded profiles. Pick one here to continue.`
              : optionsNote(view, 'music', effectiveId)
          }
          emptyLabel="No music model installed"
        />
      </section>

      <ParamPanel store={useParamPanelStore} lyricAccessory={<ApprovedLyricOffer />} />

      <LoraStack />

      <GenerateBar />

      <JobQueue names={names} />
    </>
  )
}
