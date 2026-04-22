import { ProgressStepper } from './ProgressStepper'

export type OriginalsFairnessStage = 'idle' | 'commit' | 'open' | 'reveal' | 'verify'

type OriginalsFairnessStepperProps = {
  committed: boolean
  label: string
  opened: boolean
  verified: boolean
  warning?: boolean
  phase?: OriginalsFairnessStage
}

export function OriginalsFairnessStepper({
  committed,
  label,
  opened,
  verified,
  warning = false,
  phase,
}: OriginalsFairnessStepperProps) {
  if (phase) {
    const steps = [
      {
        label: 'Commit',
        status: phase === 'commit' ? 'active' as const : ['open', 'reveal', 'verify'].includes(phase) ? 'complete' as const : 'pending' as const,
      },
      {
        label: 'Open',
        status: phase === 'open' ? 'active' as const : ['reveal', 'verify'].includes(phase) ? 'complete' as const : 'pending' as const,
      },
      {
        label: 'Reveal',
        status: phase === 'reveal' ? 'active' as const : phase === 'verify' ? 'complete' as const : 'pending' as const,
      },
      {
        label: 'Verify',
        status: verified ? 'complete' as const : warning ? 'warning' as const : phase === 'verify' ? 'active' as const : 'pending' as const,
      },
    ]

    return <ProgressStepper label={label} steps={steps} />
  }

  const steps = [
    {
      label: 'Commit',
      status: committed ? 'complete' as const : opened ? 'active' as const : 'pending' as const,
    },
    {
      label: 'Open',
      status: opened ? 'complete' as const : committed ? 'active' as const : 'pending' as const,
    },
    {
      label: 'Reveal',
      status: verified ? 'complete' as const : opened ? 'active' as const : 'pending' as const,
    },
    {
      label: 'Verify',
      status: verified ? 'complete' as const : warning ? 'warning' as const : opened ? 'active' as const : 'pending' as const,
    },
  ]

  return <ProgressStepper label={label} steps={steps} />
}
