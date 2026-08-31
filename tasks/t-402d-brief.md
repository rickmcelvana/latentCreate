# T-402d: playback fix -- CORS-clean audio element + AudioContext resume

**Depends:** T-402c (the components) | **Crate/dir:** app/src
**Files to modify:**
- `app/src/components/Player.tsx` (modify: `crossOrigin` on the `<audio>` element)
- `app/src/components/Visualizer.tsx` (modify: resume the `AudioContext` on `play`)

## Goal

The T-402c click-through found the silent-audio trap the brief named but could not check in the
gate: the track plays (the playhead advances, pause stops it) but nothing is audible and the
spectrum is flat. Fix the two browser-level causes, both invisible to `vite build`:

1. `createMediaElementSource` on a **cross-origin** media element emits silence, and the asset
   protocol is cross-origin by construction.
2. The `AudioContext` is created outside the click's gesture window, so autoplay policy may start
   it `suspended`.

## Why it failed (verified against the Tauri 2.11.5 source, not from memory)

The asset protocol serves audio from a different origin than the page. On Windows the app frontend
is `http://tauri.localhost` while `convertFileSrc` produces `http://asset.localhost/...` (on
macOS/Linux it is `tauri://localhost` vs `asset://localhost`). A `<video>`/`<audio>` element whose
`src` is cross-origin is **not** CORS-clean unless it requests CORS, and the Web Audio spec says a
`MediaElementAudioSourceNode` over a non-CORS-clean element outputs silence. That is exactly the
observed symptom: the element's `currentTime` still advances (so the seek bar moves and pause stops
it), but the audio re-routed into the graph is silent, so nothing reaches `context.destination` and
the analyser reads all zeros (flat spectrum).

The fix is to mark the element `crossOrigin="anonymous"` so the browser sends an `Origin` header
and treats the response as CORS-clean. The asset protocol already answers CORS: every response is
built with `Access-Control-Allow-Origin: <window_origin>` (tauri 2.11.5 `src/protocol/asset.rs`),
and range responses additionally expose `content-range`. So the media request with
`crossOrigin="anonymous"` is approved, not just tainted.

The second cause is latent rather than observed, and the T-402c brief already flagged it as a
producer-confirm item: the `AudioContext` is created in the Visualizer's effect, which runs after
the Play click has resolved the URL -- outside the click's transient gesture. On a browser with a
strict autoplay policy the context starts `suspended` and never resumes. The playhead would still
move (the media element plays independently), which makes this indistinguishable from cause 1 by
symptom alone. Resume the context whenever the element actually starts playing, which is the same
user-initiated path that is allowed to resume it.

## Spec

### `app/src/components/Player.tsx` (modify: one attribute)

Add `crossOrigin` to the audio element. The `ref`, `src` and every handler stay exactly as they
are.

```tsx
      <audio
        ref={setAudioEl}
        src={track.url}
        crossOrigin="anonymous"
        onTimeUpdate={(event) => reportTime(event.currentTarget.currentTime)}
        onLoadedMetadata={(event) => reportDuration(event.currentTarget.duration)}
        onEnded={ended}
        onError={() => fail('This track could not be played.')}
      />
```

### `app/src/components/Visualizer.tsx` (modify: resume on play)

After `analyser.connect(context.destination)`, resume the context when the element plays, and clean
the listener up with the context. The existing `createMediaElementSource` once-per-element and
`connect(destination)` wiring is unchanged.

```tsx
    const context = new AudioContext()
    const source = context.createMediaElementSource(audio)
    const analyser = context.createAnalyser()
    analyser.fftSize = 2048
    analyser.smoothingTimeConstant = 0.8
    source.connect(analyser)
    analyser.connect(context.destination)

    // Autoplay policy may start the context suspended: it is created here, after
    // the Play click's gesture, so resume it the moment the element plays. The
    // element playing is user-initiated, which is what allows the resume.
    const resume = () => void context.resume()
    audio.addEventListener('play', resume)
```

and in the effect cleanup, remove the listener before closing the context:

```tsx
    return () => {
      audio.removeEventListener('play', resume)
      cancelAnimationFrame(frame)
      void context.close()
    }
```

## Acceptance criteria

- [ ] `tsc -b`, `oxlint src`, `vitest run` and `vite build` green; frontend stays **355** tests (no new tests -- this brief is browser wiring the gate cannot exercise).
- [ ] The `<audio>` element carries `crossOrigin="anonymous"` and nothing else about `Player.tsx` changed.
- [ ] The Visualizer resumes the context on the element's `play` event and removes that listener in cleanup, and the `createMediaElementSource` / `analyser.connect(context.destination)` lines are untouched.
- [ ] `invoke`, `listen` and `convertFileSrc` still appear only in `bridge/` (grep `@tauri-apps` across `app/src`).

## Out of scope

- Volume control, cross-view persistence, waveform/spectrum toggles (still T-402c's scope line).

## Manual verification (producer click-through on a built app)

1. `tauri build`, open the Library, click Play: the track is **audible**.
2. The spectrum bars and waveform move while it plays.
3. Pause/resume, replay-from-zero after end, and seek still behave as T-402c verified.

## If unclear

Do not guess. Output a numbered list of questions and stop.
