import { useEffect, useState } from 'react'
import { formatTime, statusLabel, usePlayerStore } from '../state/player'
import { Visualizer } from './Visualizer'

/** The transport bar: audio element, play/pause, seek, time, and the visualizer. */
export function Player() {
  const track = usePlayerStore((state) => state.track)
  const status = usePlayerStore((state) => state.status)
  const position = usePlayerStore((state) => state.position)
  const duration = usePlayerStore((state) => state.duration)
  const error = usePlayerStore((state) => state.error)
  const toggle = usePlayerStore((state) => state.toggle)
  const seek = usePlayerStore((state) => state.seek)
  const reportTime = usePlayerStore((state) => state.reportTime)
  const reportDuration = usePlayerStore((state) => state.reportDuration)
  const ended = usePlayerStore((state) => state.ended)
  const fail = usePlayerStore((state) => state.fail)

  // The element itself, held in state so the Visualizer can receive it: a ref
  // assignment does not re-render, so a ref alone would hand it `null` forever.
  const [audioEl, setAudioEl] = useState<HTMLAudioElement | null>(null)

  // Push the store's status into the media element. `audio.ended` is the only
  // signal that a replay must start from zero rather than resuming at the tail.
  useEffect(() => {
    if (audioEl === null) return
    if (status === 'playing') {
      if (audioEl.ended) audioEl.currentTime = 0
      void audioEl.play()
    } else {
      audioEl.pause()
    }
  }, [status, track?.url, audioEl])

  if (track === null) return null

  const max = duration ?? 0

  return (
    <section className="panel player">
      <audio
        ref={setAudioEl}
        src={track.url}
        onTimeUpdate={(event) => reportTime(event.currentTarget.currentTime)}
        onLoadedMetadata={(event) => reportDuration(event.currentTarget.duration)}
        onEnded={ended}
        onError={() => fail('This track could not be played.')}
      />

      <div className="player-now-playing">
        <span className="player-track-name">{track.name}</span>
        <span className="player-status">{statusLabel(status)}</span>
      </div>

      <div className="player-controls">
        <button
          type="button"
          className="player-toggle"
          onClick={toggle}
          aria-label={status === 'playing' ? 'Pause' : 'Play'}
        >
          {status === 'playing' ? 'Pause' : 'Play'}
        </button>
        <span className="player-time">{formatTime(position)}</span>
        <input
          type="range"
          className="player-seek"
          min={0}
          max={max}
          step={0.1}
          value={Math.min(position, max)}
          onChange={(event) => {
            const value = Number(event.target.value)
            if (audioEl !== null) audioEl.currentTime = value
            seek(value)
          }}
          disabled={max === 0}
          aria-label="Seek"
        />
        <span className="player-time">{formatTime(duration ?? 0)}</span>
      </div>

      {error !== null ? <p className="player-error">{error}</p> : null}

      <Visualizer audio={audioEl} />
    </section>
  )
}
