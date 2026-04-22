import clsx from 'clsx'

export function StatusPill({
  label,
  tone = 'neutral',
}: {
  label: string
  tone?: 'neutral' | 'good' | 'warn'
}) {
  return <span className={clsx('status-pill', `status-pill--${tone}`)}>{label}</span>
}
