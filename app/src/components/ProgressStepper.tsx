type ProgressStepStatus = 'pending' | 'active' | 'complete' | 'warning'

type ProgressStepperProps = {
  label: string
  steps: Array<{
    label: string
    status: ProgressStepStatus
  }>
}

export function ProgressStepper({ label, steps }: ProgressStepperProps) {
  return (
    <div aria-label={label} className="progress-stepper">
      {steps.map((step, index) => (
        <div className={`progress-step progress-step--${step.status}`} key={`${step.label}-${index}`}>
          <span className="progress-step__dot" />
          <span>{step.label}</span>
        </div>
      ))}
    </div>
  )
}
