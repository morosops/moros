import { memo } from 'react'
import type { DiceRoundView } from '../../lib/api'
import { formatPercentBps, formatStrk } from '../../lib/format'

type DiceHistoryPanelProps = {
  history: DiceRoundView[]
}

export const DiceHistoryPanel = memo(function DiceHistoryPanel({ history }: DiceHistoryPanelProps) {
  return (
    <section className="dice-history-panel">
      <div className="dice-panel__header">
        <span>Recent rounds</span>
        <span>{history.length} tracked</span>
      </div>

      {history.length === 0 ? (
        <p className="stack-note">Your settled rounds will appear here after the first bet.</p>
      ) : (
        <table className="dice-history-table">
          <thead>
            <tr>
              <th>Round</th>
              <th>Roll</th>
              <th>Chance</th>
              <th>Mode</th>
              <th>Payout</th>
            </tr>
          </thead>
          <tbody>
            {history.map((entry) => (
              <tr key={entry.round_id}>
                <td>#{entry.round_id}</td>
                <td>{(entry.roll_bps / 100).toFixed(2)}</td>
                <td>{formatPercentBps(entry.chance_bps)}</td>
                <td>{entry.roll_over ? 'Over' : 'Under'}</td>
                <td className={entry.win ? 'profit profit--positive' : 'profit profit--negative'}>
                  {formatStrk(entry.payout)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
})
