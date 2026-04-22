import { useEffect, useState, type CSSProperties, type MouseEvent } from 'react'

type UtilityStat = {
  label: string
  value: string
}

type FairnessField = {
  label: string
  value: string
  sensitive?: boolean
}

type FairnessStatusTone = 'neutral' | 'good' | 'warn'

type GameUtilityBarProps = {
  fairnessFields: FairnessField[]
  fairnessSummary: string
  fairnessStatus?: {
    label: string
    tone?: FairnessStatusTone
  }
  liveStats?: UtilityStat[]
  settingsStats?: UtilityStat[]
  theatreMode?: boolean
  onToggleTheatre?: (enabled: boolean) => void
  onRegenerate?: () => void
  regenerateLabel?: string
}

const settingsIconStyle = {
  '--icon-url': 'url(/icons/settings.svg)',
} as CSSProperties

const theatreIconStyle = {
  '--icon-url': 'url(/icons/theatre.svg)',
} as CSSProperties

const statsIconStyle = {
  '--icon-url': 'url(/icons/stats.svg)',
} as CSSProperties

function maskValue(value: string) {
  if (!value || value === '—' || value === 'Pending' || value === 'Unavailable') {
    return value
  }

  if (value.includes(' ')) {
    return value
  }

  if (value.length <= 14) {
    return `${value.slice(0, 4)}••••${value.slice(-2)}`
  }

  return `${value.slice(0, 10)}••••${value.slice(-6)}`
}

function cx(...parts: Array<string | false | null | undefined>) {
  return parts.filter(Boolean).join(' ')
}

function CheckIcon() {
  return (
    <svg aria-hidden="true" fill="none" viewBox="0 0 20 20">
      <path d="m5 10 3.2 3.2L15 6.6" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  )
}

export function GameUtilityBar({
  fairnessFields,
  fairnessSummary,
  fairnessStatus,
  liveStats = [],
  settingsStats = [],
  theatreMode = false,
  onToggleTheatre,
  onRegenerate,
  regenerateLabel = 'Regenerate seed',
}: GameUtilityBarProps) {
  const [fairnessOpen, setFairnessOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [statsOpen, setStatsOpen] = useState(false)
  const [revealSensitive, setRevealSensitive] = useState(false)
  const [copiedLabel, setCopiedLabel] = useState<string | null>(null)
  const hasLiveStats = liveStats.length > 0
  const hasSettings = settingsStats.length > 0 || Boolean(onToggleTheatre)

  useEffect(() => {
    if (!copiedLabel) {
      return
    }

    const timer = window.setTimeout(() => setCopiedLabel(null), 1200)
    return () => window.clearTimeout(timer)
  }, [copiedLabel])

  useEffect(() => {
    if (!onToggleTheatre) {
      return
    }

    function handleFullscreenChange() {
      if (!document.fullscreenElement && theatreMode) {
        onToggleTheatre?.(false)
      }
    }

    document.addEventListener('fullscreenchange', handleFullscreenChange)
    return () => {
      document.removeEventListener('fullscreenchange', handleFullscreenChange)
    }
  }, [onToggleTheatre, theatreMode])

  async function handleCopy(label: string, value: string) {
    try {
      await navigator.clipboard.writeText(value)
      setCopiedLabel(label)
    } catch {
      setCopiedLabel(null)
    }
  }

  async function handleTheatreToggle(event: MouseEvent<HTMLButtonElement>) {
    const nextTheatreMode = !theatreMode
    const pageElement = event.currentTarget.closest('.page') as HTMLElement | null

    if (!nextTheatreMode) {
      onToggleTheatre?.(false)
      if (document.fullscreenElement) {
        await document.exitFullscreen().catch(() => {})
      }
      return
    }
    onToggleTheatre?.(true)
    if (pageElement?.requestFullscreen) {
      try {
        await pageElement.requestFullscreen()
      } catch {
        onToggleTheatre?.(false)
      }
    }
  }

  const displayedFields = fairnessFields.filter((field) => field.value)
  const toneClass = fairnessStatus?.tone === 'good'
    ? 'game-utility-bar__status game-utility-bar__status--good'
    : fairnessStatus?.tone === 'warn'
      ? 'game-utility-bar__status game-utility-bar__status--warn'
      : 'game-utility-bar__status'

  return (
    <div className="game-utility-bar">
      <div className="game-utility-bar__row">
        <div className="game-utility-bar__group">
          {hasSettings ? (
            <button
              aria-label="Game settings"
              className={settingsOpen ? 'game-utility-button game-utility-button--active' : 'game-utility-button'}
              onClick={() => {
                setSettingsOpen((open) => !open)
                setStatsOpen(false)
                setFairnessOpen(false)
              }}
              type="button"
            >
              <span aria-hidden="true" className="game-utility-icon-mask" style={settingsIconStyle} />
            </button>
          ) : null}
          <button
            aria-label="Enable theatre mode"
            className={theatreMode ? 'game-utility-button game-utility-button--active' : 'game-utility-button'}
            onClick={(event) => void handleTheatreToggle(event)}
            type="button"
          >
            <span aria-hidden="true" className="game-utility-icon-mask" style={theatreIconStyle} />
          </button>
          {hasLiveStats ? (
            <button
              aria-label="Open live stats"
              className={statsOpen ? 'game-utility-button game-utility-button--active' : 'game-utility-button'}
              onClick={() => {
                setStatsOpen((open) => !open)
                setSettingsOpen(false)
                setFairnessOpen(false)
              }}
              type="button"
            >
              <span aria-hidden="true" className="game-utility-icon-mask" style={statsIconStyle} />
            </button>
          ) : null}
        </div>

        <div className="game-utility-bar__center">
          <img alt="" className="game-utility-bar__brand" src="/transparent.jpg?v=2" />
        </div>

        <div className="game-utility-bar__group game-utility-bar__group--right">
          <button
            className={fairnessOpen ? 'game-fairness-button game-fairness-button--active' : 'game-fairness-button'}
            onClick={() => {
              setFairnessOpen((open) => !open)
              setSettingsOpen(false)
              setStatsOpen(false)
            }}
            type="button"
          >
            <CheckIcon />
            <span>Fairness</span>
          </button>
        </div>
      </div>

      {settingsOpen && hasSettings ? (
        <div className="game-utility-popover game-utility-popover--left">
          <div className="game-utility-popover__header">
            <span>Game settings</span>
          </div>
          <div className="game-utility-grid">
            <div>
              <span>Theatre mode</span>
              <strong>{theatreMode ? 'Enabled' : 'Disabled'}</strong>
            </div>
            {settingsStats.map((entry) => (
              <div key={entry.label}>
                <span>{entry.label}</span>
                <strong>{entry.value}</strong>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {statsOpen && hasLiveStats ? (
        <div className="game-utility-popover game-utility-popover--left">
          <div className="game-utility-popover__header">
            <span>Live stats</span>
          </div>
          <div className="game-utility-grid">
            {liveStats.map((entry) => (
              <div key={entry.label}>
                <span>{entry.label}</span>
                <strong>{entry.value}</strong>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {fairnessOpen ? (
        <div className="game-utility-popover game-utility-popover--right">
          <div className="game-utility-popover__header">
            <span>Fairness</span>
            <div className="game-utility-popover__actions">
              <button className="game-utility-inline-button" onClick={() => setRevealSensitive((open) => !open)} type="button">
                {revealSensitive ? 'Mask' : 'Reveal'}
              </button>
              <button
                className="game-utility-inline-button"
                disabled={!onRegenerate}
                onClick={() => onRegenerate?.()}
                type="button"
              >
                {regenerateLabel}
              </button>
            </div>
          </div>

          {fairnessStatus ? <div className={toneClass}>{fairnessStatus.label}</div> : null}

          <div className="game-utility-grid">
            {displayedFields.map((field) => {
              const visibleValue = field.sensitive && !revealSensitive ? maskValue(field.value) : field.value

              return (
                <div className="game-utility-field" key={field.label}>
                  <span>{field.label}</span>
                  <div className="game-utility-field__value">
                    <strong>{visibleValue}</strong>
                    <button className="game-utility-copy" onClick={() => void handleCopy(field.label, field.value)} type="button">
                      {copiedLabel === field.label ? 'Copied' : 'Copy'}
                    </button>
                  </div>
                </div>
              )
            })}
          </div>

          <p className="game-utility-note">{fairnessSummary}</p>
        </div>
      ) : null}
    </div>
  )
}
