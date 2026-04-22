// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { PublicProfilePage } from './PublicProfilePage'

const apiMocks = vi.hoisted(() => ({
  fetchPlayerProfile: vi.fn(),
  fetchPlayerProfileByUsername: vi.fn(),
}))

vi.mock('../lib/api', () => ({
  fetchPlayerProfile: apiMocks.fetchPlayerProfile,
  fetchPlayerProfileByUsername: apiMocks.fetchPlayerProfileByUsername,
}))

describe('PublicProfilePage', () => {
  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  it('resolves wallet-address profile URLs', async () => {
    apiMocks.fetchPlayerProfile.mockResolvedValue({
      wallet_address: '0xabc123',
      username: null,
      auth_provider: 'wallet',
      created_at: '2026-04-20T00:00:00.000Z',
    })

    render(
      <MemoryRouter initialEntries={['/profile/0xabc123']}>
        <Routes>
          <Route element={<PublicProfilePage />} path="/profile/:profileId" />
        </Routes>
      </MemoryRouter>,
    )

    await waitFor(() => {
      expect(apiMocks.fetchPlayerProfile).toHaveBeenCalledWith('0xabc123')
    })
    expect(apiMocks.fetchPlayerProfileByUsername).not.toHaveBeenCalled()
    expect(await screen.findByText('Using wallet address')).toBeTruthy()
    expect(screen.getByText('Wallet profile URL: /profile/0xabc123')).toBeTruthy()
  })

  it('resolves username profile URLs with @ handles', async () => {
    apiMocks.fetchPlayerProfileByUsername.mockResolvedValue({
      wallet_address: '0xabc123',
      username: 'tanya',
      auth_provider: 'google',
      created_at: '2026-04-20T00:00:00.000Z',
    })

    render(
      <MemoryRouter initialEntries={['/profile/@tanya']}>
        <Routes>
          <Route element={<PublicProfilePage />} path="/profile/:profileId" />
        </Routes>
      </MemoryRouter>,
    )

    await waitFor(() => {
      expect(apiMocks.fetchPlayerProfileByUsername).toHaveBeenCalledWith('tanya')
    })
    expect(apiMocks.fetchPlayerProfile).not.toHaveBeenCalled()
    await screen.findByText('Username profile URL: /profile/@tanya')
    expect(screen.getAllByText('@tanya').length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText('Username profile URL: /profile/@tanya')).toBeTruthy()
  })
})
