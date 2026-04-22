import { LeaderboardPanel } from '../components/LeaderboardPanel'

export function LeaderboardPage() {
  return (
    <section className="page page--leaderboard">
      <header className="page__header">
        <div>
          <h1>Leaderboard</h1>
        </div>
        <p className="page__summary">Settled bets, high rollers, and the current race table.</p>
      </header>

      <LeaderboardPanel initialTab="all_bets" />
    </section>
  )
}
