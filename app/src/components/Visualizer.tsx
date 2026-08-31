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
