# T-402c: playback + visualizer -- the Player and Visualizer components

**Depends:** T-402b (the player store) | **Crate/dir:** app/src
**Files to create/modify:**
- `app/src/components/Player.tsx` (new)
- `app/src/components/Visualizer.tsx` (new)
- `app/src/views/Library.tsx` (modify: play button per track + the player bar)
- `app/src/theme.css` (modify: append the player/visualizer rules)

## Goal

The DOM half of the player: an `<audio>` element wired to the T-402b store, transport controls
(play/pause, seek, time), and a canvas visualizer drawn from an `AnalyserNode`. The Library view
gains a Play button per track and renders the player bar at the bottom. The store's state machine
is already pure and tested; this brief is the wiring that cannot be unit-tested, so its correctness
rests on the reference code below plus the producer's click-through.

## The trap to design against

The webview review environment cannot composite frames or fire `requestAnimationFrame`
(WORKFLOW section 5), so the visualizer's *drawing* is verified by the producer's click-through,
never silently assumed. The wiring is the part that is easy to get wrong, and three of its rules
are invisible to the gate, so they are called out in the code and repeated here:

1. `createMediaElementSource` may be called **once** per audio element, and it re-routes the
   element's output -- the analyser **must** connect back to `context.destination`, or the track
   plays silently.
2. The audio element is held in **state**, not a ref, so the Visualizer can receive it: a ref
   assignment does not re-render, and passing `audioRef.current` as a prop would hand the
   Visualizer `null` forever.
3. The seek handler must set the media element's `currentTime` imperatively -- updating the store
   position alone does not move the playhead.

## Spec

### `app/src/components/Player.tsx` (new)

```tsx
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
        onError={() =>
          fail(
            'This track could not be played: its audio file is missing or unreadable. Re-generate the track to play it again.',
          )
        }
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
```

### `app/src/components/Visualizer.tsx` (new)

```tsx
import { useEffect, useRef } from 'react'

/**
 * A read-only spectrum + waveform drawn from the playing track's `AnalyserNode`.
 *
 * Zero custom DSP: `AnalyserNode` supplies the frequency and time-domain data;
 * this component only draws it (ARCHITECTURE section 9). The drawing cannot be
 * verified in the review environment -- no frame compositing, no
 * `requestAnimationFrame` (WORKFLOW section 5) -- so it is a producer
 * click-through item. The wiring below is the part that is easy to get wrong
 * and is reviewed by eye:
 *
 * - `createMediaElementSource` may be called once per audio element, so the
 *   effect runs once per element and tears down with it.
 * - `createMediaElementSource` re-routes the element's output: the analyser
 *   MUST connect back to `context.destination`, or the track goes silent.
 */
export function Visualizer({ audio }: { audio: HTMLAudioElement | null }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (audio === null || canvas === null) return

    const context = new AudioContext()
    const source = context.createMediaElementSource(audio)
    const analyser = context.createAnalyser()
    analyser.fftSize = 2048
    analyser.smoothingTimeConstant = 0.8
    source.connect(analyser)
    analyser.connect(context.destination)

    const ctx = canvas.getContext('2d')
    if (ctx === null) {
      void context.close()
      return
    }

    // Follow theme.css rather than forking --accent into canvas code.
    const accent = getComputedStyle(canvas).getPropertyValue('--accent').trim() || '#58a6ff'
    const frequencies = new Uint8Array(analyser.frequencyBinCount)
    const wave = new Uint8Array(analyser.frequencyBinCount)
    const barCount = 48

    let frame = 0
    const draw = () => {
      frame = requestAnimationFrame(draw)
      analyser.getByteFrequencyData(frequencies)
      analyser.getByteTimeDomainData(wave)

      const { width, height } = canvas
      ctx.clearRect(0, 0, width, height)

      // Spectrum: bars across the lower two thirds.
      const barWidth = width / barCount
      const barFloor = height * 0.66
      ctx.fillStyle = accent
      for (let i = 0; i < barCount; i++) {
        const value = frequencies[Math.floor((i / barCount) * frequencies.length)]
        const barHeight = (value / 255) * barFloor
        ctx.fillRect(i * barWidth, height - barHeight, Math.max(1, barWidth - 1), barHeight)
      }

      // Waveform: a time-domain line across the top third.
      ctx.strokeStyle = accent
      ctx.lineWidth = 1
      ctx.beginPath()
      for (let i = 0; i < wave.length; i++) {
        const x = (i / (wave.length - 1)) * width
        const y = (wave[i] / 255) * height * 0.33
        if (i === 0) ctx.moveTo(x, y)
        else ctx.lineTo(x, y)
      }
      ctx.stroke()
    }
    frame = requestAnimationFrame(draw)

    return () => {
      cancelAnimationFrame(frame)
      void context.close()
    }
  }, [audio])

  return <canvas ref={canvasRef} className="visualizer" width={640} height={120} />
}
```

### `app/src/views/Library.tsx` (modify: anchors, not the whole file)

Add the imports at the top (after the existing `import { EMPTY_LIBRARY, ... }` line):

```tsx
import { Player } from '../components/Player'
import { usePlayerStore } from '../state/player'
```

In `TrackCard`, change the function to add a play button. The current head is:

```tsx
function TrackCard({ row }: { row: TrackRow }) {
  return (
    <li className="panel track-row">
      <div className="track-head">
        <span className="track-name">{row.name}</span>
        <span className="track-duration">{row.duration}</span>
      </div>
```

Replace it with:

```tsx
function TrackCard({ row }: { row: TrackRow }) {
  const play = usePlayerStore((state) => state.play)
  return (
    <li className="panel track-row">
      <div className="track-head">
        <span className="track-name">{row.name}</span>
        <div className="track-head-actions">
          <button
            type="button"
            className="track-play"
            onClick={() => void play(row.id, row.name)}
          >
            Play
          </button>
          <span className="track-duration">{row.duration}</span>
        </div>
      </div>
```

Render the player bar at the bottom of the `Library` view. The view currently ends:

```tsx
      {tracks.length === 0 ? (
        <p className="library-empty">{EMPTY_LIBRARY}</p>
      ) : (
        <ul className="track-list">
          {tracks.map((row) => (
            <TrackCard key={row.id} row={row} />
          ))}
        </ul>
      )}
    </>
  )
}
```

Add `<Player />` after the conditional, before `</>`:

```tsx
      {tracks.length === 0 ? (
        <p className="library-empty">{EMPTY_LIBRARY}</p>
      ) : (
        <ul className="track-list">
          {tracks.map((row) => (
            <TrackCard key={row.id} row={row} />
          ))}
        </ul>
      )}

      <Player />
    </>
  )
}
```

The player lives in the Library view, not the shell: playback is a Library feature, and the
milestone click-through (generate -> play -> visualizer) happens entirely in this view. Cross-view
playback persistence is a later product decision, not this phase.

### `app/src/theme.css` (modify: append)

Append these rules at the end of the file. Do not change any existing rule.

```css
/* --- Player & visualizer (T-402) --- */

.player {
  display: flex;
  flex-direction: column;
  gap: var(--gap-md);
  margin-top: var(--gap-lg);
}

.player-now-playing {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--gap-md);
}

.player-track-name {
  color: var(--text);
  font-size: 15px;
  font-weight: 600;
}

.player-status {
  color: var(--text-muted);
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.4px;
}

.player-controls {
  display: flex;
  align-items: center;
  gap: var(--gap-md);
}

.player-toggle {
  padding: var(--gap-sm) var(--gap-lg);
  background: var(--accent);
  border: 1px solid var(--accent);
  border-radius: var(--radius);
  color: var(--bg);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition:
    background var(--transition),
    border-color var(--transition);
}

.player-toggle:hover:not(:disabled) {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}

.player-seek {
  flex: 1;
  min-width: 0;
  accent-color: var(--accent);
}

.player-seek:disabled {
  opacity: 0.4;
}

.player-time {
  color: var(--text-muted);
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.player-error {
  margin: 0;
  color: var(--danger);
  font-size: 13px;
}

.visualizer {
  width: 100%;
  height: 120px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
}

/* Library play affordance */

.track-head-actions {
  display: flex;
  align-items: center;
  gap: var(--gap-sm);
}

.track-play {
  padding: var(--gap-xs) var(--gap-sm);
  background: transparent;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--accent);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
  transition:
    color var(--transition),
    border-color var(--transition);
}

.track-play:hover {
  color: var(--accent-hover);
  border-color: var(--accent);
}
```

## Acceptance criteria

- [ ] `tsc -b`, `oxlint src`, `vitest run` and `vite build` green; frontend stays **351** tests (no new tests -- this brief is wiring).
- [ ] Every new className in the diff has a rule in `theme.css`, and no existing `theme.css` rule changed.
- [ ] `invoke`, `listen` and `convertFileSrc` do not appear in `components/` or `views/` (grep `@tauri-apps` across `app/src` -- it must appear only in `bridge/`).
- [ ] The audio element is in state, not a ref, and the Visualizer receives it as a prop (the ref-assignment re-render trap).
- [ ] `analyser.connect(context.destination)` is present (the silent-audio trap).

## Out of scope

- Autoplay/resume of the `AudioContext` on first interaction: playback is always user-initiated
  (a Play click), so the context is created after a gesture. Listed for the producer to confirm.
- Waveform-only or spectrum-only toggles, cross-view persistence, volume control.

## Manual verification (producer click-through -- the gate cannot check these)

1. On a **built** app (`tauri build`, not `tauri dev` -- the CSP is only injected into the HTML
   Tauri serves), open the Library and click Play on a track. Confirm the track audibly plays.
2. Confirm the visualizer's spectrum bars and waveform move while the track plays.
3. Pause, then Play: the track resumes. Let a track finish: the button shows Play; clicking it
   replays from the start.
4. Drag the seek bar: the playhead jumps and the audio follows.
5. Play a track with the asset protocol scope correct; confirm the player error text appears for a
   track whose audio file was deleted (a "say what to do next" message, not a blank failure).

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/state/player.ts --read app/src/views/Library.tsx --read app/src/components/JobQueue.tsx --file app/src/components/Player.tsx --file app/src/components/Visualizer.tsx --file app/src/views/Library.tsx --file app/src/theme.css
```
