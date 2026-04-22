import { useEffect, useState } from 'react'
import { useMorosAuthRuntime } from './MorosAuthProvider'
import { fetchBetFeed, type BetFeedItem, type BetFeedResponse } from '../lib/api'
import { deriveMorosAccountState } from '../lib/account-state'
import { useMorosWallet } from '../hooks/useMorosWallet'
import { useAccountStore } from '../store/account'

const betTabs = [
  { key: 'my_bets', label: 'My Bets' },
  { key: 'all_bets', label: 'All Bets' },
  { key: 'high_rollers', label: 'High Rollers' },
  { key: 'race_leaderboard', label: 'Race Leaderboard' },
] as const

export type BetTabKey = (typeof betTabs)[number]['key']

function formatStrk(wei?: string | null) {
  if (!wei) return '0 STRK'
  const value = BigInt(wei)
  const whole = value / 10n ** 18n
  const fraction = (value % 10n ** 18n).toString().padStart(18, '0').slice(0, 4)
  return fraction === '0000' ? `${whole} STRK` : `${whole}.${fraction.replace(/0+$/, '')} STRK`
}

function formatMultiplier(multiplierBps: string) {
  const value = Number(multiplierBps) / 10_000
  if (!Number.isFinite(value)) {
    return '0x'
  }
  return `${value.toFixed(2).replace(/0+$/, '').replace(/\.$/, '')}x`
}

function shortUser(user: string) {
  const looksLikeAddress = user.startsWith('0x')
  if (!looksLikeAddress) {
    return user
  }
  if (user.length <= 14) {
    return user
  }
  return `${user.slice(0, 6)}...${user.slice(-4)}`
}

function formatProfit(item: BetFeedItem) {
  const profit = BigInt(item.payout) - BigInt(item.bet_amount)
  const sign = profit > 0n ? '+' : profit < 0n ? '-' : ''
  const absolute = profit < 0n ? -profit : profit
  return `${sign}${formatStrk(absolute.toString())}`
}

function emptyTextForTab(tab: BetTabKey, options: { signedIn: boolean; pendingAccount: boolean }) {
  if (tab === 'my_bets') {
    if (options.pendingAccount) {
      return 'Loading your settled bets...'
    }
    return options.signedIn ? 'No settled bets for this account yet.' : 'Login to show your settled bets.'
  }
  if (tab === 'high_rollers') {
    return 'No high-roller bets indexed yet.'
  }
  if (tab === 'race_leaderboard') {
    return 'No race entries indexed yet.'
  }
  return 'No settled bets indexed yet.'
}

export function LeaderboardPanel({ initialTab = 'all_bets' }: { initialTab?: BetTabKey }) {
  const auth = useMorosAuthRuntime()
  const { address } = useMorosWallet()
  const accountUserId = useAccountStore((state) => state.userId)
  const accountWalletAddress = useAccountStore((state) => state.walletAddress)
  const { accountPending, resolvedWalletAddress, signedIn: accountSignedIn } = deriveMorosAccountState({
    authReady: auth.ready,
    authenticated: auth.authenticated,
    accountUserId,
    accountWalletAddress,
    runtimeWalletAddress: address,
  })
  const [feed, setFeed] = useState<BetFeedResponse>()
  const [activeBetTab, setActiveBetTab] = useState<BetTabKey>(initialTab)

  useEffect(() => {
    let cancelled = false
    void fetchBetFeed({
      userId: accountUserId,
      walletAddress: resolvedWalletAddress,
    })
      .then((nextFeed) => {
        if (!cancelled) setFeed(nextFeed)
      })
      .catch(() => {
        if (!cancelled) {
          setFeed({
            my_bets: [],
            all_bets: [],
            high_rollers: [],
            race_leaderboard: [],
          })
        }
      })
    return () => {
      cancelled = true
    }
  }, [accountUserId, resolvedWalletAddress])

  const items = feed?.[activeBetTab]

  return (
    <section className="leaderboard-panel" id="leaderboard">
      <div className="leaderboard-tabs" role="tablist" aria-label="Bet feed">
        {betTabs.map((tab) => (
          <button
            aria-selected={activeBetTab === tab.key}
            className={activeBetTab === tab.key ? 'leaderboard-tab leaderboard-tab--active' : 'leaderboard-tab'}
            key={tab.key}
            onClick={() => setActiveBetTab(tab.key)}
            role="tab"
            type="button"
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="leaderboard-table-wrap">
        {items?.length ? (
          <table className="leaderboard-table">
            <thead>
              <tr>
                <th>User</th>
                <th>Game</th>
                <th>Bet Amount</th>
                <th>Multiplier</th>
                <th>Profit</th>
              </tr>
            </thead>
            <tbody>
              {items.map((item) => {
                const profit = BigInt(item.payout) - BigInt(item.bet_amount)
                return (
                  <tr key={`${activeBetTab}-${item.game}-${item.user}-${item.settled_at}-${item.tx_hash ?? ''}`}>
                    <td>{activeBetTab === 'my_bets' && !accountSignedIn ? 'Connect wallet' : shortUser(item.user)}</td>
                    <td>{item.game}</td>
                    <td>{formatStrk(item.bet_amount)}</td>
                    <td>{formatMultiplier(item.multiplier_bps)}</td>
                    <td className={profit >= 0n ? 'profit profit--positive' : 'profit profit--negative'}>{formatProfit(item)}</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        ) : (
          <p className="empty-state leaderboard-empty-state">
            {items
              ? emptyTextForTab(activeBetTab, {
                  signedIn: accountSignedIn,
                  pendingAccount: activeBetTab === 'my_bets' && accountPending,
                })
              : 'Loading settled bets from the coordinator.'}
          </p>
        )}
      </div>
    </section>
  )
}
