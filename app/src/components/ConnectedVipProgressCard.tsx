import { useEffect, useState } from 'react'
import { fetchRewardsState, type RewardsState } from '../lib/api'
import { shortAddress as formatShortAddress } from '../lib/format'
import { VipProgressCard } from './VipProgressCard'

const VIP_PROGRESS_POLL_MS = 15_000

type ConnectedVipProgressCardProps = {
  userId?: string
  walletAddress?: string
  username?: string
  to?: string
  compact?: boolean
  className?: string
}

function shortAddress(address?: string) {
  return formatShortAddress(address) ?? 'Moros account'
}

export function ConnectedVipProgressCard({
  userId,
  walletAddress,
  username,
  to,
  compact = false,
  className,
}: ConnectedVipProgressCardProps) {
  const [rewards, setRewards] = useState<RewardsState>()
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!walletAddress) {
      setRewards(undefined)
      setLoading(false)
      return
    }

    let cancelled = false
    let interval: number | undefined

    async function loadRewards(showLoading: boolean) {
      if (showLoading) {
        setLoading(true)
      }

      try {
        const response = await fetchRewardsState({
          userId,
          walletAddress,
        })
        if (!cancelled) {
          setRewards(response)
        }
      } catch {
        if (!cancelled && showLoading) {
          setRewards(undefined)
        }
      } finally {
        if (!cancelled && showLoading) {
          setLoading(false)
        }
      }
    }

    void loadRewards(true)
    interval = window.setInterval(() => {
      void loadRewards(false)
    }, VIP_PROGRESS_POLL_MS)

    return () => {
      cancelled = true
      if (interval !== undefined) {
        window.clearInterval(interval)
      }
    }
  }, [userId, walletAddress])

  if (!walletAddress) {
    return null
  }

  const nextLevel = loading
    ? 'Loading...'
    : rewards?.vip.next_tier_name ?? (rewards ? 'Top tier' : 'Rewards')

  return (
    <VipProgressCard
      className={className}
      compact={compact}
      nextLevel={nextLevel}
      progressPercentage={(rewards?.vip.progress_bps ?? 0) / 100}
      to={to}
      username={username ?? shortAddress(walletAddress)}
    />
  )
}

export type { ConnectedVipProgressCardProps }
