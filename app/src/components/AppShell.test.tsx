// @vitest-environment jsdom

import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AppShell } from './AppShell'
import { useAccountStore } from '../store/account'
import { useProfileStore } from '../store/profile'
import { useUiStore } from '../store/ui'

const authMocks = vi.hoisted(() => ({
  useMorosAuthRuntime: vi.fn(),
}))

const walletMocks = vi.hoisted(() => ({
  useMorosWallet: vi.fn(),
}))

const apiMocks = vi.hoisted(() => ({
  createMorosAccountResolveChallenge: vi.fn(),
  fetchAccountBalancesByWalletAddress: vi.fn(),
  fetchProfileClaimChallenge: vi.fn(),
  fetchPlayerProfile: vi.fn(),
  resolveMorosPrivyAccount: vi.fn(),
  resolveMorosWalletAccount: vi.fn(),
  upsertPlayerProfile: vi.fn(),
}))

vi.mock('../hooks/useMorosWallet', () => ({
  useMorosWallet: walletMocks.useMorosWallet,
}))

vi.mock('../lib/api', () => ({
  createMorosAccountResolveChallenge: apiMocks.createMorosAccountResolveChallenge,
  fetchAccountBalancesByWalletAddress: apiMocks.fetchAccountBalancesByWalletAddress,
  fetchProfileClaimChallenge: apiMocks.fetchProfileClaimChallenge,
  fetchPlayerProfile: apiMocks.fetchPlayerProfile,
  resolveMorosPrivyAccount: apiMocks.resolveMorosPrivyAccount,
  resolveMorosWalletAccount: apiMocks.resolveMorosWalletAccount,
  upsertPlayerProfile: apiMocks.upsertPlayerProfile,
}))

vi.mock('./MorosAuthProvider', () => ({
  useMorosAuthRuntime: authMocks.useMorosAuthRuntime,
}))

describe('AppShell', () => {
  beforeEach(() => {
    walletMocks.useMorosWallet.mockReturnValue({
      address: undefined,
      connectPrivy: vi.fn(),
      connectExternal: vi.fn(),
      disconnect: vi.fn(),
      ensureGameplaySession: vi.fn(),
      listExternalWallets: vi.fn().mockResolvedValue([]),
      pendingLabel: undefined,
      restore: vi.fn(),
      signTypedData: vi.fn(),
      status: 'idle',
      strategy: undefined,
      warmConnect: vi.fn(),
      withdraw: vi.fn(),
    })

    authMocks.useMorosAuthRuntime.mockReturnValue({
      enabled: true,
      loaded: true,
      loading: false,
      ready: true,
      authenticated: false,
      userId: 'did:privy:user_1',
      identityToken: undefined,
      emailState: 'idle',
      oauthLoading: false,
      ensureLoaded: vi.fn(),
      sendEmailCode: vi.fn(),
      verifyEmailCode: vi.fn(),
      loginWithGoogle: vi.fn(),
      getAccessToken: vi.fn().mockResolvedValue(null),
      getIdentityToken: vi.fn().mockResolvedValue(null),
      logout: vi.fn(),
    })

    apiMocks.fetchPlayerProfile.mockRejectedValue(new Error('not found'))
    apiMocks.fetchAccountBalancesByWalletAddress.mockResolvedValue({
      user_id: 'did:privy:user_1',
      gambling_balance: '7000000000000000000',
      gambling_reserved: '0',
      vault_balance: '2000000000000000000',
      updated_at: '2026-04-22T00:00:00Z',
    })

    const storage = new Map<string, string>()
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: {
        getItem: vi.fn((key: string) => storage.get(key) ?? null),
        setItem: vi.fn((key: string, value: string) => {
          storage.set(key, value)
        }),
        removeItem: vi.fn((key: string) => {
          storage.delete(key)
        }),
        clear: vi.fn(() => {
          storage.clear()
        }),
      },
    })

    act(() => {
      useAccountStore.setState({
        userId: 'did:privy:user_1',
        walletAddress: undefined,
        authMethod: 'google',
      })
      useProfileStore.setState({
        username: undefined,
        authProvider: undefined,
      })
      useUiStore.setState({
        sidebarCollapsed: true,
      })
    })
  })

  afterEach(() => {
    cleanup()
  })

  it('shows Deposit immediately once a canonical Moros account has a resolved wallet address', async () => {
    authMocks.useMorosAuthRuntime.mockReturnValue({
      enabled: true,
      loaded: true,
      loading: false,
      ready: true,
      authenticated: true,
      userId: 'did:privy:user_1',
      identityToken: undefined,
      emailState: 'idle',
      oauthLoading: false,
      ensureLoaded: vi.fn(),
      sendEmailCode: vi.fn(),
      verifyEmailCode: vi.fn(),
      loginWithGoogle: vi.fn(),
      getAccessToken: vi.fn(() => new Promise(() => {})),
      getIdentityToken: vi.fn(() => new Promise(() => {})),
      logout: vi.fn(),
    })

    act(() => {
      useAccountStore.setState({
        userId: 'did:privy:user_1',
        walletAddress: '0xfeedface',
        authMethod: 'google',
      })
    })

    render(
      <MemoryRouter initialEntries={['/']}>
        <Routes>
          <Route element={<AppShell />} path="/">
            <Route element={<div>home</div>} index />
          </Route>
        </Routes>
      </MemoryRouter>,
    )

    const depositButton = screen.getByRole('button', { name: 'Deposit' })
    expect(depositButton).toBeTruthy()
    expect(depositButton.getAttribute('disabled')).toBeNull()
    expect(screen.queryByText('Ready')).toBeNull()

    const walletButton = await screen.findByRole('button', { name: /open wallet, balance 7 strk/i })
    expect(walletButton).toBeTruthy()
  })

  it('flips auth-only Privy sessions into the signed-in shell while Moros account resolution finishes', () => {
    act(() => {
      useAccountStore.setState({
        userId: undefined,
        walletAddress: undefined,
        authMethod: undefined,
      })
    })

    authMocks.useMorosAuthRuntime.mockReturnValue({
      enabled: true,
      loaded: true,
      loading: false,
      ready: true,
      authenticated: true,
      userId: 'did:privy:user_1',
      identityToken: undefined,
      emailState: 'idle',
      oauthLoading: false,
      ensureLoaded: vi.fn(),
      sendEmailCode: vi.fn(),
      verifyEmailCode: vi.fn(),
      loginWithGoogle: vi.fn(),
      getAccessToken: vi.fn(() => new Promise(() => {})),
      getIdentityToken: vi.fn(() => new Promise(() => {})),
      logout: vi.fn(),
    })

    render(
      <MemoryRouter initialEntries={['/']}>
        <Routes>
          <Route element={<AppShell />} path="/">
            <Route element={<div>home</div>} index />
          </Route>
        </Routes>
      </MemoryRouter>,
    )

    const walletButton = screen.getByRole('button', { name: /open wallet, balance 0 strk/i })
    expect(walletButton).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Deposit' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Sign up' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Login' })).toBeNull()
  })

  it('opens wallet and profile menus on click with the restored actions', async () => {
    apiMocks.fetchPlayerProfile.mockResolvedValue({
      wallet_address: '0xfeedface',
      username: 'tanya',
      auth_provider: 'google',
      auth_subject: 'did:privy:user_1',
      created_at: '2026-04-22T00:00:00Z',
      updated_at: '2026-04-22T00:00:00Z',
    })

    act(() => {
      useAccountStore.setState({
        userId: 'did:privy:user_1',
        walletAddress: '0xfeedface',
        authMethod: 'google',
      })
      useProfileStore.setState({
        username: 'tanya',
        authProvider: 'google',
      })
    })

    walletMocks.useMorosWallet.mockReturnValue({
      address: '0xfeedface',
      connectPrivy: vi.fn(),
      connectExternal: vi.fn(),
      disconnect: vi.fn(),
      ensureGameplaySession: vi.fn(),
      listExternalWallets: vi.fn().mockResolvedValue([]),
      pendingLabel: undefined,
      restore: vi.fn(),
      signTypedData: vi.fn(),
      status: 'connected',
      strategy: 'privy',
      warmConnect: vi.fn(),
      withdraw: vi.fn(),
    })

    authMocks.useMorosAuthRuntime.mockReturnValue({
      enabled: true,
      loaded: true,
      loading: false,
      ready: true,
      authenticated: true,
      userId: 'did:privy:user_1',
      identityToken: undefined,
      emailState: 'idle',
      oauthLoading: false,
      ensureLoaded: vi.fn(),
      sendEmailCode: vi.fn(),
      verifyEmailCode: vi.fn(),
      loginWithGoogle: vi.fn(),
      getAccessToken: vi.fn(() => new Promise(() => {})),
      getIdentityToken: vi.fn(() => new Promise(() => {})),
      logout: vi.fn(),
    })

    render(
      <MemoryRouter initialEntries={['/']}>
        <Routes>
          <Route element={<AppShell />} path="/">
            <Route element={<div>home</div>} index />
          </Route>
        </Routes>
      </MemoryRouter>,
    )

    fireEvent.click(await screen.findByRole('button', { name: /open wallet, balance 7 strk/i }))
    expect(screen.getByRole('dialog', { name: 'Wallet' })).toBeTruthy()
    expect(screen.getByText('Withdraw')).toBeTruthy()
    expect(screen.getAllByText('Deposit')).toHaveLength(2)

    fireEvent.click(screen.getByTitle('tanya'))
    expect(screen.getByRole('button', { name: 'Profile' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Vault' })).toBeTruthy()
    const support = screen.getByRole('link', { name: 'Support' })
    expect(support.getAttribute('href')).toBe('mailto:admin@moros.bet')
  })
})
