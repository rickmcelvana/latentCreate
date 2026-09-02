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
 * - The canvas backing store follows the element's real size x
 *   `devicePixelRatio`, so a canvas that CSS stretches wide (full screen) is
 *   drawn at native resolution rather than upscaled from a fixed bitmap -- the
 *   blur T-503 fixes. A `ResizeObserver` keeps the two in step across window
 *   and full-screen changes, and the draw code works in CSS pixels.
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

    // Autoplay policy may start the context suspended: it is created here, after
    // the Play click's gesture, so resume it the moment the element plays. The
    // element playing is user-initiated, which is what allows the resume.
    const resume = () => void context.resume()
    audio.addEventListener('play', resume)

    const ctx = canvas.getContext('2d')
    if (ctx === null) {
      void context.close()
      return
    }

    // Follow theme.css rather than forking the palette into canvas code.
    const styles = getComputedStyle(canvas)
    const accent = styles.getPropertyValue('--accent').trim() || '#58a6ff'
    const accentDim = styles.getPropertyValue('--accent-dim').trim() || accent
    const frequencies = new Uint8Array(analyser.frequencyBinCount)
    const wave = new Uint8Array(analyser.frequencyBinCount)
    const barCount = 64

    // `width`/`height` are the logical (CSS-pixel) size the draw code works in;
    // the backing store is that x dpr. `fit` resyncs both whenever the element
    // resizes (window, full screen), and the context transform is reset so one
    // drawn unit is one CSS pixel regardless of dpr.
    let width = 0
    let height = 0
    const fit = () => {
      const dpr = window.devicePixelRatio || 1
      const rect = canvas.getBoundingClientRect()
      width = rect.width
      height = rect.height
      canvas.width = Math.round(width * dpr)
      canvas.height = Math.round(height * dpr)
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    }
    fit()
    const observer = new ResizeObserver(fit)
    observer.observe(canvas)

    let frame = 0
    const draw = () => {
      frame = requestAnimationFrame(draw)
      if (width === 0 || height === 0) return
      analyser.getByteFrequencyData(frequencies)
      analyser.getByteTimeDomainData(wave)

      ctx.clearRect(0, 0, width, height)

      // Spectrum: gradient bars growing from the floor, brightest at the base.
      const gradient = ctx.createLinearGradient(0, height, 0, 0)
      gradient.addColorStop(0, accent)
      gradient.addColorStop(1, accentDim)
      ctx.fillStyle = gradient
      const barWidth = width / barCount
      for (let i = 0; i < barCount; i++) {
        const value = frequencies[Math.floor((i / barCount) * frequencies.length)] / 255
        const barHeight = Math.max(2, value * height * 0.9)
        const w = Math.max(1, barWidth - 2)
        const r = Math.min(w / 2, 3)
        ctx.beginPath()
        ctx.roundRect(i * barWidth, height - barHeight, w, barHeight, [r, r, 0, 0])
        ctx.fill()
      }

      // Waveform: a centered oscilloscope line over the bars. 128 is the
      // silent midpoint of time-domain byte data, so it maps to the centre.
      ctx.strokeStyle = accent
      ctx.globalAlpha = 0.7
      ctx.lineWidth = 2
      ctx.lineJoin = 'round'
      ctx.beginPath()
      const mid = height / 2
      for (let i = 0; i < wave.length; i++) {
        const x = (i / (wave.length - 1)) * width
        const y = mid + ((wave[i] - 128) / 128) * (height * 0.4)
        if (i === 0) ctx.moveTo(x, y)
        else ctx.lineTo(x, y)
      }
      ctx.stroke()
      ctx.globalAlpha = 1
    }
    frame = requestAnimationFrame(draw)

    return () => {
      audio.removeEventListener('play', resume)
      observer.disconnect()
      cancelAnimationFrame(frame)
      void context.close()
    }
  }, [audio])

  return <canvas ref={canvasRef} className="visualizer" />
}
