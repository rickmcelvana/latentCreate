import { useEffect, useState } from 'react'
import { GenerateArtBar } from '../components/GenerateArtBar'
import { JobQueue } from '../components/JobQueue'
import { ParamPanel } from '../components/ParamPanel'
import { ProfilePickerRow } from '../components/ProfilePickerRow'
import { useConfigStore } from '../state/config'
import { useArtStore, EMPTY_ART, type ArtRow } from '../state/art'
import { useJobsStore } from '../state/jobs'
import { useModelsStore } from '../state/models'
import { useNavStore } from '../state/nav'
import { useArtPanelStore } from '../state/paramPanel'
import {
  effectiveImageProfileId,
  imageStudioNote,
  imageStudioState,
  pickable,
  profileRow,
} from '../state/profiles'

export function CoverArt() {
  const startListeningJobs = useJobsStore((state) => state.startListening)
  const view = useModelsStore((state) => state.view)
  const refresh = useModelsStore((state) => state.refresh)
  const config = useConfigStore((state) => state.config)
  const save = useConfigStore((state) => state.save)
  const loadArt = useArtStore((state) => state.load)
  const startListeningArt = useArtStore((state) => state.startListening)
  const loadPanel = useArtPanelStore((state) => state.load)

  useEffect(() => {
    void startListeningJobs()
  }, [startListeningJobs])

  useEffect(() => {
    void refresh()
  }, [refresh])

  useEffect(() => {
    void loadArt()
  }, [loadArt])

  useEffect(() => {
    void startListeningArt()
  }, [startListeningArt])

  const state = imageStudioState(view, config)
  const chosenId = effectiveImageProfileId(config)
  const note = imageStudioNote(state, chosenId)
  const rows = pickable(view, 'image')
  const names = Object.fromEntries(rows.map((p) => [p.id, p.display_name]))

  useEffect(() => {
    if (chosenId !== null) {
      void loadPanel(chosenId)
    }
  }, [chosenId, loadPanel])

  return (
    <>
      <h1 className="view-title">Cover Art</h1>
      <p className="view-subtitle">
        Artwork for singles and albums, from the same ComfyUI.
      </p>

      <section className="panel profile-picker">
        <h2 className="profile-picker-title">Image model</h2>

        {note !== null ? (
          <p className="profile-picker-fallback">{note}</p>
        ) : null}

        {view !== null && !view.inventory_available ? (
          <p className="profile-picker-disclaimer">
            Readiness could not be checked because ComfyUI is not running.
          </p>
        ) : null}

        {state === 'no-profiles' ? (
          <button
            type="button"
            className="profile-picker-setup"
            onClick={() => useNavStore.getState().setView('setup')}
          >
            Open Setup
          </button>
        ) : null}

        <ul className="profile-list">
          {rows.map((profile) => (
            <ProfilePickerRow
              key={profile.id}
              row={profileRow(profile)}
              selected={profile.id === chosenId}
              group="image-profile"
              onSelect={() => void save({ default_image_profile_id: profile.id })}
            />
          ))}
        </ul>
      </section>

      {state === 'ready' ? <ParamPanel store={useArtPanelStore} /> : null}
      {state === 'ready' ? <GenerateArtBar /> : null}

      <JobQueue names={names} />

      <ArtGallery />
    </>
  )
}

function ArtGallery() {
  const art = useArtStore((state) => state.art)
  const error = useArtStore((state) => state.error)
  const warnings = useArtStore((state) => state.warnings)
  const load = useArtStore((state) => state.load)

  return (
    <section className="panel art-gallery">
      {error !== null ? (
        <p className="library-error">
          {error}
          <button
            type="button"
            className="library-retry"
            onClick={() => void load()}
          >
            Retry
          </button>
        </p>
      ) : null}

      {warnings !== null ? <p className="library-warning">{warnings}</p> : null}

      {art.length === 0 ? (
        <p className="library-empty">{EMPTY_ART}</p>
      ) : (
        <ul className="art-grid">
          {art.map((row) => (
            <ArtTile key={row.id} row={row} />
          ))}
        </ul>
      )}
    </section>
  )
}

function ArtTile({ row }: { row: ArtRow }) {
  const [broken, setBroken] = useState(false)

  // Clear the failure when the store reloads. `artRows` builds fresh row
  // objects on every load, so `row` changes identity exactly when the gallery
  // is re-read and never on an unrelated re-render -- which is the signal
  // wanted here. Without it a tile that failed once stays "not found" for the
  // life of the mount, including after the file it names comes back.
  useEffect(() => {
    setBroken(false)
  }, [row])

  return (
    <li className="art-tile">
      {row.url !== null && !broken ? (
        <img
          className="art-thumb"
          src={row.url}
          alt={row.name}
          onError={() => setBroken(true)}
        />
      ) : (
        <div className="art-missing">Image file not found.</div>
      )}

      <span className="art-name">{row.name}</span>

      <dl className="art-facts">
        <div className="art-fact">
          <dt>Model</dt>
          <dd>{row.model}</dd>
        </div>
        <div className="art-fact">
          <dt>Licence</dt>
          <dd>{row.license}</dd>
        </div>
        <div className="art-fact">
          <dt>Size</dt>
          <dd>{row.size}</dd>
        </div>
        <div className="art-fact">
          <dt>Seed</dt>
          <dd>{row.seed}</dd>
        </div>
        <div className="art-fact">
          <dt>Created</dt>
          <dd>{row.created}</dd>
        </div>
      </dl>
    </li>
  )
}
