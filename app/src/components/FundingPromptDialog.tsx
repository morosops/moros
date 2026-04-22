type FundingPromptDialogProps = {
  open: boolean
  onDeposit: () => void
  onSkip: () => void
}

export function FundingPromptDialog({ open, onDeposit, onSkip }: FundingPromptDialogProps) {
  if (!open) {
    return null
  }

  return (
    <div className="funding-prompt-backdrop" onClick={onSkip} role="presentation">
      <section
        aria-label="Deposit funds"
        className="funding-prompt"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="funding-prompt__header">
          <strong>Deposit funds</strong>
          <button
            aria-label="Skip deposit for now"
            className="funding-prompt__close"
            onClick={onSkip}
            type="button"
          >
            ×
          </button>
        </header>

        <div className="funding-prompt__body">
          <p>Select a chain and token, send funds to the issued address, and Moros will route them into STRK automatically.</p>
          <div className="funding-prompt__actions">
            <button className="button button--primary" onClick={onDeposit} type="button">
              Deposit
            </button>
            <button className="button button--ghost" onClick={onSkip} type="button">
              Skip for now
            </button>
          </div>
        </div>
      </section>
    </div>
  )
}
