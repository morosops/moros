import { useId } from 'react'
import { Link } from 'react-router-dom'

type VipProgressCardProps = {
  username: string
  progressPercentage: number
  nextLevel: string
  to?: string
  title?: string
  compact?: boolean
  className?: string
}

function formatPercentage(value: number) {
  if (value > 0 && value < 0.01) {
    return '<0.01%'
  }
  if (value > 0 && value < 1) {
    return `${value.toFixed(3).replace(/\.?0+$/, '')}%`
  }
  return `${value.toFixed(2).replace(/\.?0+$/, '')}%`
}

function clampPercentage(value: number) {
  if (!Number.isFinite(value)) {
    return 0
  }
  return Math.min(100, Math.max(0, value))
}

export function VipProgressCard({
  username,
  progressPercentage,
  nextLevel,
  to = '/rewards',
  title = 'Your VIP Progress',
  compact = false,
  className,
}: VipProgressCardProps) {
  const tooltipId = useId()
  const progressValue = clampPercentage(progressPercentage)
  const cardClassName = [
    'vip-progress-panel',
    compact ? 'vip-progress-panel--compact' : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <div className={cardClassName}>
      <span className="vip-progress-panel__title">{title}</span>

      <div className="vip-progress-card">
        <Link className="vip-progress-card__user-row" to={to}>
          <span className="vip-progress-card__username">{username}</span>
          <span aria-hidden="true" className="vip-progress-card__chevron">
            &gt;
          </span>
        </Link>

        <div className="vip-progress-card__progress-row">
          <strong className="vip-progress-card__percentage">{formatPercentage(progressValue)}</strong>
          <span className="vip-progress-card__info-wrap">
            <button
              aria-describedby={tooltipId}
              aria-label="How VIP progress is calculated"
              className="vip-progress-card__info"
              type="button"
            >
              i
            </button>
            <span className="vip-progress-card__tooltip" id={tooltipId} role="tooltip">
              Progress is based on wagered amount / activity
            </span>
          </span>
        </div>

        <div aria-hidden="true" className="vip-progress-card__bar">
          <div
            className="vip-progress-card__bar-fill"
            style={{ width: `${progressValue.toFixed(2)}%` }}
          />
        </div>

        <span className="vip-progress-card__next-level">Next level: {nextLevel}</span>
      </div>
    </div>
  )
}

export type { VipProgressCardProps }
