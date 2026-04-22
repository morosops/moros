// @vitest-environment jsdom

import { MemoryRouter } from 'react-router-dom'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ConnectedVipProgressCard } from './ConnectedVipProgressCard'

const apiMocks = vi.hoisted(() => ({
  fetchRewardsState: vi.fn(),
}))

vi.mock('../lib/api', () => ({
  fetchRewardsState: apiMocks.fetchRewardsState,
}))

describe('ConnectedVipProgressCard', () => {
  afterEach(() => {
    cleanup()
    apiMocks.fetchRewardsState.mockReset()
  })

  it('loads VIP progress through the canonical account identity when available', async () => {
    apiMocks.fetchRewardsState.mockResolvedValue({
      wallet_address: '0xfeedface',
      vip: {
        lifetime_wager_raw: '0',
        wager_7d_raw: '0',
        wager_30d_raw: '0',
        lifetime_weighted_volume_raw: '0',
        weighted_volume_7d_raw: '0',
        weighted_volume_30d_raw: '0',
        vip_points_raw: '0',
        current_tier_level: 2,
        current_tier_name: 'Silver',
        next_tier_level: 3,
        next_tier_name: 'Gold',
        next_tier_threshold_raw: '0',
        progress_bps: 2441,
      },
      rakeback: {
        accrued_raw: '0',
        claimed_raw: '0',
        claimable_raw: '0',
        scale_bps: 10_000,
      },
      weekly: {
        accrued_raw: '0',
        claimed_raw: '0',
        claimable_raw: '0',
        scale_bps: 10_000,
      },
      level_up: {
        accrued_raw: '0',
        claimed_raw: '0',
        claimable_raw: '0',
        scale_bps: 10_000,
      },
      referral: {
        referred_users: 0,
        accrued_raw: '0',
        claimed_raw: '0',
        claimable_raw: '0',
        referral_rate_bps: 2500,
      },
      rakeback_epochs: [],
      weekly_epochs: [],
      level_up_rewards: [],
      claimable_total_raw: '0',
      config: {
        budget_share_bps: 2000,
        rakeback_share_bps: 6500,
        weekly_share_bps: 2500,
        level_up_share_bps: 1000,
        referral_rate_bps: 2500,
        max_counted_wager_per_bet_raw: '0',
        rewards_pool_cap_raw: null,
        rakeback_user_cap_raw: '0',
        weekly_user_cap_raw: '0',
        global_epoch_cap_raw: '0',
        referral_user_cap_raw: '0',
        referral_global_cap_raw: '0',
        weekly_min_weighted_volume_raw: '0',
        claim_reservation_ttl_seconds: 300,
        blackjack_reward_house_edge_bps: 50,
        dice_reward_house_edge_bps: 100,
        roulette_reward_house_edge_bps: 270,
        baccarat_reward_house_edge_bps: 120,
        tiers: [],
      },
    })

    render(
      <MemoryRouter>
        <ConnectedVipProgressCard
          to="/rewards"
          userId="11111111-1111-7111-8111-111111111111"
          username="meowless21"
          walletAddress="0xfeedface"
        />
      </MemoryRouter>,
    )

    await waitFor(() =>
      expect(apiMocks.fetchRewardsState).toHaveBeenCalledWith({
        userId: '11111111-1111-7111-8111-111111111111',
        walletAddress: '0xfeedface',
      }),
    )

    await screen.findByText('24.41%')
    expect(screen.getByRole('link', { name: /meowless21/i }).getAttribute('href')).toBe('/rewards')
    expect(screen.getByText('24.41%')).toBeTruthy()
    expect(screen.getByText('Next level: Gold')).toBeTruthy()
  })
})
