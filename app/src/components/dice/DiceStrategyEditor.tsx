import { useRef, type CSSProperties } from 'react'
import {
  type ConditionAction,
  type ConditionBlock,
  type ConditionProfitComparator,
  type ConditionTriggerContext,
  type ConditionTriggerMode,
  type StrategyEditorStep,
  conditionActionNeedsValue,
  conditionActionValueUnit,
  conditionSummary,
} from '../../lib/dice-strategy'
import { useFocusTrap } from '../../hooks/useFocusTrap'

const strategyPanelIconStyle = {
  '--icon-url': 'url(/icons/panel.svg)',
} as CSSProperties
const strategyCloseIconStyle = {
  '--icon-url': 'url(/icons/close.svg)',
} as CSSProperties

type DiceStrategyEditorProps = {
  conditionDraft: ConditionBlock[]
  error?: string
  name: string
  onAddCondition: () => void
  onClose: () => void
  onDeleteCondition: (conditionId: string) => void
  onFocusCondition: (conditionId: string) => void
  onNameChange: (value: string) => void
  onSave: () => void
  onStart: () => void
  onStepConditionCount: (conditionId: string, delta: number) => void
  onUpdateCondition: (conditionId: string, updates: Partial<ConditionBlock>) => void
  selectedConditionId: string
  step: StrategyEditorStep
}

export function DiceStrategyEditor({
  conditionDraft,
  error,
  name,
  onAddCondition,
  onClose,
  onDeleteCondition,
  onFocusCondition,
  onNameChange,
  onSave,
  onStart,
  onStepConditionCount,
  onUpdateCondition,
  selectedConditionId,
  step,
}: DiceStrategyEditorProps) {
  const dialogRef = useRef<HTMLDivElement | null>(null)
  useFocusTrap(dialogRef, true, onClose)

  return (
    <div className="dice-condition-overlay" onClick={onClose} role="presentation">
      <div
        aria-label="Advanced Bet"
        aria-modal="true"
        className="dice-condition-modal"
        onClick={(event) => event.stopPropagation()}
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <div className="dice-condition-modal__header">
          <div className="dice-strategy-modal__title">
            <span aria-hidden="true" className="dice-strategy-modal__icon-mask" style={strategyPanelIconStyle} />
            <div>
              <strong>Advanced Bet</strong>
            </div>
          </div>
          <button aria-label="Close" className="dice-condition-modal__close" onClick={onClose} type="button">
            <span aria-hidden="true" className="dice-condition-modal__close-icon" style={strategyCloseIconStyle} />
          </button>
        </div>

        {step === 'start' ? (
          <div className="dice-strategy-create">
            <label className="stack-field">
              <span>Strategy Name *</span>
              <input
                autoCapitalize="none"
                className="text-input text-input--large"
                onChange={(event) => onNameChange(event.target.value)}
                spellCheck={false}
                type="text"
                value={name}
              />
            </label>
            {error ? <p className="stack-note stack-note--error">{error}</p> : null}
            <button className="button button--primary button--wide" onClick={onStart} type="button">
              Get Started
            </button>
          </div>
        ) : (
          <>
            <div className="dice-strategy-identity">
              <strong>{name.trim()}</strong>
            </div>

            <div className="dice-condition-modal__body">
              <div className="dice-condition-builder">
                {conditionDraft.length ? (
                  conditionDraft.map((block, index) => (
                    <div
                      className={
                        selectedConditionId === block.id
                          ? 'dice-condition-card dice-condition-card--selected'
                          : 'dice-condition-card'
                      }
                      key={block.id}
                    >
                      {block.collapsed ? (
                        <div className="dice-condition-card__summary-row">
                          <div className="dice-condition-card__summary-copy">
                            <span className="dice-condition-card__summary-title">Condition {index + 1}</span>
                            <strong>{conditionSummary(block)}</strong>
                          </div>
                          <div className="dice-condition-card__summary-actions">
                            <button
                              aria-label={`Expand condition ${index + 1}`}
                              className="dice-condition-icon-button"
                              onClick={() => onFocusCondition(block.id)}
                              type="button"
                            >
                              ▾
                            </button>
                            <button
                              aria-label={`Edit condition ${index + 1}`}
                              className="dice-condition-icon-button"
                              onClick={() => onFocusCondition(block.id)}
                              type="button"
                            >
                              ✎
                            </button>
                            <button
                              aria-label={`Delete condition ${index + 1}`}
                              className="dice-condition-icon-button"
                              onClick={() => onDeleteCondition(block.id)}
                              type="button"
                            >
                              ✕
                            </button>
                          </div>
                        </div>
                      ) : (
                        <>
                          <div className="dice-condition-card__header">
                            <span>Condition {index + 1}</span>
                          </div>

                          <div className="dice-condition-card__body">
                            <div className="dice-condition-section">
                              <span className="dice-condition-section__label">Condition Type</span>
                              <div className="dice-condition-type-toggle">
                                <button
                                  className={block.type === 'bet' ? 'dice-condition-type-toggle__button dice-condition-type-toggle__button--active' : 'dice-condition-type-toggle__button'}
                                  onClick={() => onUpdateCondition(block.id, { type: 'bet' })}
                                  type="button"
                                >
                                  <span aria-hidden="true" className="dice-condition-type-toggle__indicator" />
                                  <span>Bet Condition</span>
                                </button>
                                <button
                                  className={block.type === 'profit' ? 'dice-condition-type-toggle__button dice-condition-type-toggle__button--active' : 'dice-condition-type-toggle__button'}
                                  onClick={() => onUpdateCondition(block.id, { type: 'profit' })}
                                  type="button"
                                >
                                  <span aria-hidden="true" className="dice-condition-type-toggle__indicator" />
                                  <span>Profit Condition</span>
                                </button>
                              </div>
                            </div>

                            <div className="dice-condition-section">
                              <span className="dice-condition-section__label">On</span>
                              {block.type === 'bet' ? (
                                <div className="dice-condition-grid dice-condition-grid--trigger">
                                  <select
                                    className="table-select"
                                    onChange={(event) => onUpdateCondition(block.id, { triggerMode: event.target.value as ConditionTriggerMode })}
                                    value={block.triggerMode}
                                  >
                                    <option value="every">Every</option>
                                    <option value="after">After</option>
                                  </select>
                                  <div className="dice-condition-stepper-field">
                                    <input
                                      className="text-input"
                                      inputMode="numeric"
                                      min="1"
                                      onChange={(event) => onUpdateCondition(block.id, { triggerCount: Math.max(1, Number.parseInt(event.target.value, 10) || 1) })}
                                      type="number"
                                      value={block.triggerCount}
                                    />
                                    <div className="dice-stepper dice-stepper--inline dice-stepper--stacked">
                                      <button aria-label={`Increase trigger count for condition ${index + 1}`} onClick={() => onStepConditionCount(block.id, 1)} type="button">▲</button>
                                      <button aria-label={`Decrease trigger count for condition ${index + 1}`} onClick={() => onStepConditionCount(block.id, -1)} type="button">▼</button>
                                    </div>
                                  </div>
                                  <select
                                    className="table-select"
                                    onChange={(event) => onUpdateCondition(block.id, { triggerContext: event.target.value as ConditionTriggerContext })}
                                    value={block.triggerContext}
                                  >
                                    <option value="bets">Bets</option>
                                    <option value="wins">Wins</option>
                                    <option value="losses">Losses</option>
                                  </select>
                                </div>
                              ) : (
                                <div className="dice-condition-grid dice-condition-grid--profit">
                                  <div className="dice-condition-static-field">Profit</div>
                                  <select
                                    className="table-select"
                                    onChange={(event) => onUpdateCondition(block.id, { profitComparator: event.target.value as ConditionProfitComparator })}
                                    value={block.profitComparator}
                                  >
                                    <option value="gt">Greater than</option>
                                    <option value="gte">Greater than or equal to</option>
                                    <option value="lt">Lower than</option>
                                    <option value="lte">Lower than or equal to</option>
                                  </select>
                                  <label className="dice-condition-value-input">
                                    <input
                                      className="text-input"
                                      inputMode="decimal"
                                      onChange={(event) => onUpdateCondition(block.id, { profitValue: event.target.value })}
                                      type="text"
                                      value={block.profitValue}
                                    />
                                    <small className="dice-condition-value-input__unit">STRK</small>
                                  </label>
                                </div>
                              )}
                            </div>

                            <div className="dice-condition-section">
                              <span className="dice-condition-section__label">Do</span>
                              <div className="dice-condition-grid dice-condition-grid--action">
                                <select
                                  className="table-select"
                                  onChange={(event) => onUpdateCondition(block.id, { action: event.target.value as ConditionAction })}
                                  value={block.action}
                                >
                                  <option value="increase_bet_amount">Increase bet amount</option>
                                  <option value="decrease_bet_amount">Decrease bet amount</option>
                                  <option value="increase_win_chance">Increase win chance</option>
                                  <option value="decrease_win_chance">Decrease win chance</option>
                                  <option value="add_to_bet_amount">Add to bet amount</option>
                                  <option value="subtract_from_bet_amount">Subtract from bet amount</option>
                                  <option value="add_to_win_chance">Add to win chance</option>
                                  <option value="subtract_from_win_chance">Subtract from win chance</option>
                                  <option value="set_bet_amount">Set bet amount</option>
                                  <option value="set_win_chance">Set win chance</option>
                                  <option value="switch_over_under">Switch over/under</option>
                                  <option value="reset_bet_amount">Reset bet amount</option>
                                  <option value="reset_win_chance">Reset win chance</option>
                                  <option value="stop_autobet">Stop autobet</option>
                                </select>
                                {conditionActionNeedsValue(block.action) ? (
                                  <label className="dice-condition-value-input">
                                    <input
                                      className="text-input"
                                      inputMode="decimal"
                                      onChange={(event) => onUpdateCondition(block.id, { actionValue: event.target.value })}
                                      type="text"
                                      value={block.actionValue}
                                    />
                                    <small>{conditionActionValueUnit(block.action)}</small>
                                  </label>
                                ) : (
                                  <div className="dice-condition-static-field dice-condition-static-field--muted">No value</div>
                                )}
                              </div>
                            </div>
                          </div>

                          <div className="dice-condition-card__footer">
                            <button className="button button--ghost button--compact" onClick={() => onUpdateCondition(block.id, { collapsed: true })} type="button">
                              Minimize
                            </button>
                            <button className="button button--ghost button--compact" onClick={() => onDeleteCondition(block.id)} type="button">
                              Delete
                            </button>
                          </div>
                        </>
                      )}
                    </div>
                  ))
                ) : (
                  <p className="stack-note">Add a condition block to start building automation rules.</p>
                )}
              </div>
            </div>

            <div className="dice-strategy-modal__footer">
              <button className="button button--secondary button--wide" onClick={onAddCondition} type="button">
                Add Condition Block
              </button>
              <p className="stack-note dice-condition-builder__note">Conditions will be executed in a top down order.</p>
              {error ? <p className="stack-note stack-note--error">{error}</p> : null}
              <button className="button button--primary button--wide" onClick={onSave} type="button">
                Save Strategy
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
