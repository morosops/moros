import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import {
  fetchCoordinatorTableState,
  type CoordinatorTableStateResponse,
} from '../lib/api'
import { ConnectedVipProgressCard } from '../components/ConnectedVipProgressCard'
import { useMorosAuthRuntime } from '../components/MorosAuthProvider'
import { LeaderboardPanel } from '../components/LeaderboardPanel'
import { useMorosWallet } from '../hooks/useMorosWallet'
import { deriveMorosAccountState } from '../lib/account-state'
import { morosGames } from '../lib/game-config'
import { useAccountStore } from '../store/account'
import { useProfileStore } from '../store/profile'

export function LobbyPage() {
  const auth = useMorosAuthRuntime()
  const { address } = useMorosWallet()
  const accountUserId = useAccountStore((state) => state.userId)
  const accountWalletAddress = useAccountStore((state) => state.walletAddress)
  const username = useProfileStore((state) => state.username)
  const [tableStates, setTableStates] = useState<Record<number, CoordinatorTableStateResponse>>({})
  const { resolvedWalletAddress, signedIn } = deriveMorosAccountState({
    authReady: auth.ready,
    authenticated: auth.authenticated,
    accountUserId,
    accountWalletAddress,
    runtimeWalletAddress: address,
  })

  useEffect(() => {
    let cancelled = false
    void Promise.all(
      morosGames.map(async (game) => [game.tableId, await fetchCoordinatorTableState(game.tableId)] as const),
    )
      .then((states) => {
        if (cancelled) return
        setTableStates(Object.fromEntries(states))
      })
      .catch(() => {
        if (!cancelled) setTableStates({})
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <section className="page page--lobby">
      <div className="lobby-overview">
        <header className="page__header">
          <div>
            <h1>Casino</h1>
          </div>
        </header>

        {signedIn && resolvedWalletAddress ? (
          <ConnectedVipProgressCard
            className="lobby-overview__vip"
            to="/rewards"
            userId={accountUserId}
            username={username}
            walletAddress={resolvedWalletAddress}
          />
        ) : null}
      </div>

      <div className="lobby-game-grid">
        {morosGames.map((game) => {
          const response = tableStates[game.tableId]
          const livePlayers = response?.live_players ?? 0
          const liveLabel = `${livePlayers} ${livePlayers === 1 ? 'playing' : 'playing'}`
          return (
            <Link className={`lobby-game-card lobby-game-card--${game.slug}`} key={game.title} to={game.route}>
              <div className="lobby-game-card__topline">
                <span className="lobby-game-card__pill">{game.title.toUpperCase()}</span>
              </div>
              <img
                alt={`${game.title} table artwork`}
                className="lobby-game-card__image"
                loading="lazy"
                src={game.image}
              />
              <div className="lobby-game-card__live">
                <span className="lobby-game-card__live-dot" aria-hidden="true" />
                <span>{liveLabel}</span>
              </div>
            </Link>
          )
        })}
      </div>

      <div className="section-header">
        <h2>Leaderboard</h2>
      </div>

      <LeaderboardPanel initialTab="all_bets" />
    </section>
  )
}
