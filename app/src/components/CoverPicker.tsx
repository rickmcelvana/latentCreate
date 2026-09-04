import { useEffect, useState } from 'react'
import type { CoverView } from '../state/covers'

export function CoverPicker({
  view,
  choices,
  disabled,
  onChange,
}: {
  view: CoverView
  choices: { id: string | null; label: string }[]
  disabled?: boolean
  onChange: (cover: string | null) => void
}) {
  const [broken, setBroken] = useState(false)

  useEffect(() => {
    setBroken(false)
  }, [view])

  const value = view.state === 'none' ? '' : view.id

  return (
    <div className="cover-picker">
      {view.state === 'shown' ? (
        view.url !== null && !broken ? (
          <>
            <img
              className="cover-thumb"
              src={view.url}
              alt={view.name}
              onError={() => setBroken(true)}
            />
            <span className="cover-name">{view.name}</span>
          </>
        ) : (
          <div className="cover-missing">Image file not found.</div>
        )
      ) : null}

      {view.state === 'missing' ? (
        <div className="cover-missing">
          Artwork {view.id} is no longer in this project.
        </div>
      ) : null}

      {view.state === 'none' ? <div className="cover-none">No cover</div> : null}

      {choices.length === 1 ? (
        <span className="muted">Generate cover art to use it here.</span>
      ) : (
        <select
          className="cover-select"
          value={value}
          disabled={disabled}
          onChange={(event) => {
            const next = event.target.value
            onChange(next === '' ? null : next)
          }}
        >
          {choices.map((choice) => (
            <option key={choice.id ?? ''} value={choice.id ?? ''}>
              {choice.label}
            </option>
          ))}
        </select>
      )}
    </div>
  )
}
