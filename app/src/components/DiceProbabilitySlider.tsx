import { useEffect, useMemo, useRef, useState } from 'react'
import { useProfileStore } from '../store/profile'

type DiceProbabilitySliderProps = {
  thresholdBps: number
  rollOver: boolean
  resultBps?: number
  resultLabel?: string
  resultWin?: boolean
  resultKey?: number
  onChangeThresholdBps: (thresholdBps: number) => void
}

const majorTicks = [0, 25, 50, 75, 100]

function clampThreshold(thresholdBps: number, rollOver: boolean) {
  const minimum = rollOver ? 199 : 100
  const maximum = rollOver ? 9899 : 9800
  return Math.min(maximum, Math.max(minimum, thresholdBps))
}

type SnapMode = 'drag' | 'settle'

function applyMagneticSnap(rawPercent: number, mode: SnapMode = 'drag') {
  let snapped = rawPercent
  const snapLayers = mode === 'settle'
    ? [
        { step: 25, threshold: 2.8, strength: 0.96, exponent: 1.03, lockRatio: 0.76 },
        { step: 5, threshold: 1.32, strength: 0.76, exponent: 1.1, lockRatio: 0.62 },
        { step: 1, threshold: 0.52, strength: 0.34, exponent: 1.16, lockRatio: 0.42 },
      ]
    : [
        { step: 25, threshold: 2.45, strength: 0.82, exponent: 1.04, lockRatio: 0.12 },
        { step: 5, threshold: 1.08, strength: 0.54, exponent: 1.08, lockRatio: 0.1 },
        { step: 1, threshold: 0.4, strength: 0.18, exponent: 1.2, lockRatio: 0.06 },
      ]

  for (const layer of snapLayers) {
    const anchor = Math.round(snapped / layer.step) * layer.step
    const distance = Math.abs(snapped - anchor)
    if (distance <= layer.threshold) {
      if (distance <= layer.threshold * layer.lockRatio) {
        snapped = anchor
        continue
      }
      const proximity = Math.pow(1 - distance / layer.threshold, layer.exponent)
      snapped += (anchor - snapped) * layer.strength * proximity
    }
  }

  return Number(Math.min(100, Math.max(0, snapped)).toFixed(2))
}

export function DiceProbabilitySlider({
  thresholdBps,
  rollOver,
  resultBps,
  resultLabel,
  resultWin,
  resultKey,
  onChangeThresholdBps,
}: DiceProbabilitySliderProps) {
  const gameAudioVolume = useProfileStore((state) => state.gameAudioVolume)
  const trackRef = useRef<HTMLDivElement | null>(null)
  const tickAudioContextRef = useRef<AudioContext | null>(null)
  const tickBufferRef = useRef<AudioBuffer | null>(null)
  const chimeBufferRef = useRef<AudioBuffer | null>(null)
  const tickLoadPromiseRef = useRef<Promise<void> | null>(null)
  const chimeLoadPromiseRef = useRef<Promise<void> | null>(null)
  const lastAudibleStepRef = useRef(Math.round(thresholdBps / 100))
  const lastResultStepRef = useRef<number | null>(null)
  const visualThresholdRef = useRef(thresholdBps)
  const targetThresholdRef = useRef(thresholdBps)
  const [visualThresholdBps, setVisualThresholdBps] = useState(thresholdBps)
  const [isDragging, setIsDragging] = useState(false)
  const [resultPositionBps, setResultPositionBps] = useState<number | null>(resultBps ?? null)
  const [resultAnimating, setResultAnimating] = useState(false)

  async function ensureAudioReady() {
    const AudioContextCtor = window.AudioContext || (window as typeof window & {
      webkitAudioContext?: typeof AudioContext
    }).webkitAudioContext
    if (!AudioContextCtor) {
      return
    }

    if (!tickAudioContextRef.current) {
      tickAudioContextRef.current = new AudioContextCtor()
    }

    const context = tickAudioContextRef.current
    if (context.state === 'suspended') {
      await context.resume().catch(() => {})
    }

    if (!tickBufferRef.current && !tickLoadPromiseRef.current) {
      tickLoadPromiseRef.current = fetch('/tick2.wav')
        .then((response) => response.arrayBuffer())
        .then((buffer) => context.decodeAudioData(buffer.slice(0)))
        .then((decoded) => {
          tickBufferRef.current = decoded
        })
        .finally(() => {
          tickLoadPromiseRef.current = null
        })
    }

    if (!chimeBufferRef.current && !chimeLoadPromiseRef.current) {
      chimeLoadPromiseRef.current = fetch('/chime-g.wav')
        .then((response) => response.arrayBuffer())
        .then((buffer) => context.decodeAudioData(buffer.slice(0)))
        .then((decoded) => {
          chimeBufferRef.current = decoded
        })
        .finally(() => {
          chimeLoadPromiseRef.current = null
        })
    }

    await Promise.all([tickLoadPromiseRef.current, chimeLoadPromiseRef.current])
  }

  function playTickSound(count: number) {
    const context = tickAudioContextRef.current
    const buffer = tickBufferRef.current
    if (!context || !buffer || context.state !== 'running') {
      return
    }

    for (let index = 0; index < count; index += 1) {
      const source = context.createBufferSource()
      source.buffer = buffer
      source.playbackRate.value = 1.02

      const filterNode = context.createBiquadFilter()
      filterNode.type = 'lowpass'
      filterNode.frequency.value = 1850
      filterNode.Q.value = 0.45

      const gainNode = context.createGain()
      gainNode.gain.value = 0.74 * gameAudioVolume

      source.connect(filterNode)
      filterNode.connect(gainNode)
      gainNode.connect(context.destination)
      source.start(context.currentTime + index * 0.012)
    }
  }

  function playWinChime() {
    const context = tickAudioContextRef.current
    const buffer = chimeBufferRef.current
    if (!context || !buffer || context.state !== 'running') {
      return
    }

    const source = context.createBufferSource()
    source.buffer = buffer
    source.playbackRate.value = 1

    const filterNode = context.createBiquadFilter()
    filterNode.type = 'lowpass'
    filterNode.frequency.value = 2800
    filterNode.Q.value = 0.32

    const gainNode = context.createGain()
    gainNode.gain.value = 0.86 * gameAudioVolume

    source.connect(filterNode)
    filterNode.connect(gainNode)
    gainNode.connect(context.destination)
    source.start()
  }

  useEffect(() => {
    const primeAudio = () => {
      void ensureAudioReady()
    }

    window.addEventListener('pointerdown', primeAudio, { passive: true })
    window.addEventListener('keydown', primeAudio)

    return () => {
      window.removeEventListener('pointerdown', primeAudio)
      window.removeEventListener('keydown', primeAudio)
    }
  }, [])

  useEffect(() => {
    let frame = 0

    const tick = () => {
      const delta = thresholdBps - visualThresholdRef.current
      if (Math.abs(delta) <= 0.5) {
        visualThresholdRef.current = thresholdBps
        setVisualThresholdBps(thresholdBps)
        return
      }

      visualThresholdRef.current += delta * (isDragging ? 0.42 : 0.32)
      setVisualThresholdBps(visualThresholdRef.current)
      frame = window.requestAnimationFrame(tick)
    }

    frame = window.requestAnimationFrame(tick)
    return () => window.cancelAnimationFrame(frame)
  }, [isDragging, thresholdBps])

  useEffect(() => {
    const nextStep = Math.round(visualThresholdBps / 100)
    const previousStep = lastAudibleStepRef.current
    if (nextStep === previousStep) {
      return
    }

    lastAudibleStepRef.current = nextStep
    const tickCount = Math.min(6, Math.max(1, Math.abs(nextStep - previousStep)))
    playTickSound(tickCount)
  }, [gameAudioVolume, visualThresholdBps])

  useEffect(() => {
    if (resultBps === undefined || resultKey === undefined) {
      return
    }

    const start = Math.round(visualThresholdRef.current)
    const end = resultBps
    const durationMs = 520
    const startedAt = performance.now()

    setResultPositionBps(start)
    setResultAnimating(true)
    lastResultStepRef.current = Math.round(start / 100)
    void ensureAudioReady()

    let frame = 0
    const animate = (timestamp: number) => {
      const elapsed = Math.min(1, (timestamp - startedAt) / durationMs)
      const eased = 1 - Math.pow(1 - elapsed, 3)
      const next = Math.round((start + (end - start) * eased) * 100) / 100
      setResultPositionBps(next)

      const nextStep = Math.round(next / 100)
      const previousStep = lastResultStepRef.current ?? nextStep
      if (nextStep !== previousStep) {
        const tickCount = Math.min(6, Math.max(1, Math.abs(nextStep - previousStep)))
        lastResultStepRef.current = nextStep
        playTickSound(tickCount)
      }

      if (elapsed < 1) {
        frame = window.requestAnimationFrame(animate)
        return
      }

      setResultPositionBps(end)
      if (resultWin) {
        playWinChime()
      }
      window.setTimeout(() => {
        setResultAnimating(false)
      }, 80)
    }

    frame = window.requestAnimationFrame(animate)

    return () => {
      window.cancelAnimationFrame(frame)
    }
  }, [gameAudioVolume, resultBps, resultKey, resultWin])

  useEffect(() => {
    targetThresholdRef.current = thresholdBps
  }, [thresholdBps])

  function clientXToThreshold(clientX: number, mode: SnapMode = 'drag') {
    const rect = trackRef.current?.getBoundingClientRect()
    if (!rect || rect.width === 0) {
      return thresholdBps
    }

    const ratio = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
    const rawPercent = ratio * 100
    const snappedPercent = applyMagneticSnap(rawPercent, mode)
    return clampThreshold(Math.round(snappedPercent * 100), rollOver)
  }

  function commitThreshold(nextThresholdBps: number) {
    targetThresholdRef.current = nextThresholdBps
    onChangeThresholdBps(nextThresholdBps)
  }

  function settleThreshold() {
    const settledPercent = applyMagneticSnap(targetThresholdRef.current / 100, 'settle')
    const settledThreshold = clampThreshold(Math.round(settledPercent * 100), rollOver)
    commitThreshold(settledThreshold)
  }

  useEffect(() => {
    if (!isDragging) {
      return
    }

    function handlePointerMove(event: PointerEvent) {
      commitThreshold(clientXToThreshold(event.clientX, 'drag'))
    }

    function handlePointerUp() {
      setIsDragging(false)
      settleThreshold()
    }

    window.addEventListener('pointermove', handlePointerMove)
    window.addEventListener('pointerup', handlePointerUp)

    return () => {
      window.removeEventListener('pointermove', handlePointerMove)
      window.removeEventListener('pointerup', handlePointerUp)
    }
  }, [isDragging, onChangeThresholdBps, rollOver, thresholdBps])

  const handlePercent = useMemo(() => visualThresholdBps / 100, [visualThresholdBps])
  const thresholdPercent = handlePercent
  const lossStyle = rollOver
    ? { width: `${thresholdPercent}%` }
    : { left: `${thresholdPercent}%`, width: `${100 - thresholdPercent}%` }
  const winStyle = rollOver
    ? { left: `${thresholdPercent}%`, width: `${100 - thresholdPercent}%` }
    : { width: `${thresholdPercent}%` }

  return (
    <div className="dice-probability-slider">
      <div
        className={isDragging ? 'dice-probability-slider__track dice-probability-slider__track--dragging' : 'dice-probability-slider__track'}
        onPointerDown={(event) => {
          void ensureAudioReady()
          setIsDragging(true)
          event.currentTarget.setPointerCapture?.(event.pointerId)
          commitThreshold(clientXToThreshold(event.clientX, 'drag'))
        }}
        ref={trackRef}
      >
        <div className="dice-probability-slider__rail" />
        <div className="dice-probability-slider__loss" style={lossStyle} />
        <div className="dice-probability-slider__win" style={winStyle} />
        <div className="dice-probability-slider__handle" style={{ left: `${thresholdPercent}%` }}>
          <span />
        </div>

        {resultPositionBps !== null && resultLabel ? (
          <div
            className={
              resultAnimating
                ? `dice-probability-slider__result ${resultWin ? 'dice-probability-slider__result--win' : 'dice-probability-slider__result--loss'} dice-probability-slider__result--animating`
                : `dice-probability-slider__result ${resultWin ? 'dice-probability-slider__result--win' : 'dice-probability-slider__result--loss'}`
            }
            style={{ left: `${resultPositionBps / 100}%` }}
          >
            {resultLabel}
          </div>
        ) : null}
      </div>

      <div className="dice-probability-slider__scale" aria-hidden="true">
        {majorTicks.map((tick) => (
          <div className="dice-probability-slider__scale-item" key={tick} style={{ left: `${tick}%` }}>
            <i />
            <span>{tick}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
