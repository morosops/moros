// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  fetchBetFeed,
  fetchCoordinatorTableState,
} from './api'

describe('api URL construction', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({}),
      }),
    )
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('builds coordinator feed URLs correctly when service roots are relative', async () => {
    await fetchBetFeed({ userId: 'moros:user_1', walletAddress: '0xabc' })

    expect(fetch).toHaveBeenCalledWith(
      'http://localhost:3000/coordinator/v1/bets?user_id=moros%3Auser_1&player=0xabc',
      undefined,
    )
  })

  it('builds coordinator table-state URLs correctly when service roots are relative', async () => {
    await fetchCoordinatorTableState(1, '0xabc')

    expect(fetch).toHaveBeenCalledWith(
      'http://localhost:3000/coordinator/v1/tables/1/state?player=0xabc',
      undefined,
    )
  })
})
