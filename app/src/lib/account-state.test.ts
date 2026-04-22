import { describe, expect, it } from 'vitest'
import { deriveMorosAccountState, resolveMorosPrimaryActionLabel } from './account-state'

describe('deriveMorosAccountState', () => {
  it('treats authenticated sessions as signed in while Moros account resolution finishes', () => {
    const state = deriveMorosAccountState({
      authReady: true,
      authenticated: true,
    })

    expect(state.signedIn).toBe(true)
    expect(state.depositReady).toBe(false)
    expect(state.walletActionReady).toBe(false)
    expect(state.accountPending).toBe(true)
    expect(state.accountSyncPending).toBe(false)
    expect(state.walletPending).toBe(false)
    expect(state.onboardingStage).toBe('resolving_account')
    expect(state.onboardingLabel).toBe('Resolving account')
  })

  it('treats canonical wallets without an active runtime wallet as deposit-ready but not action-ready', () => {
    const state = deriveMorosAccountState({
      authReady: true,
      authenticated: true,
      accountUserId: 'did:privy:user_1',
      accountWalletAddress: '0xfeedface',
    })

    expect(state.signedIn).toBe(true)
    expect(state.depositReady).toBe(true)
    expect(state.walletActionReady).toBe(false)
    expect(state.accountPending).toBe(false)
    expect(state.accountSyncPending).toBe(false)
    expect(state.walletPending).toBe(false)
    expect(state.onboardingStage).toBe('preparing_wallet')
    expect(state.onboardingLabel).toBe('Preparing wallet')
  })

  it('lets canonical auth-only accounts open deposits while wallet actions prepare', () => {
    const state = deriveMorosAccountState({
      authReady: true,
      authenticated: true,
      accountUserId: 'did:privy:user_1',
    })

    expect(state.signedIn).toBe(true)
    expect(state.depositReady).toBe(false)
    expect(state.walletActionReady).toBe(false)
    expect(state.accountPending).toBe(false)
    expect(state.accountSyncPending).toBe(false)
    expect(state.walletPending).toBe(true)
    expect(state.onboardingStage).toBe('preparing_wallet')
    expect(state.onboardingLabel).toBe('Preparing wallet')
  })

  it('does not block a resolved runtime wallet on background account sync', () => {
    const state = deriveMorosAccountState({
      authReady: true,
      authenticated: true,
      runtimeWalletAddress: '0xfeedface',
    })

    expect(state.signedIn).toBe(true)
    expect(state.depositReady).toBe(true)
    expect(state.walletActionReady).toBe(true)
    expect(state.accountPending).toBe(false)
    expect(state.accountSyncPending).toBe(true)
    expect(state.onboardingStage).toBe('ready')
  })

  it('treats resolved runtime wallets as fully ready', () => {
    const state = deriveMorosAccountState({
      authReady: true,
      authenticated: true,
      accountUserId: 'did:privy:user_1',
      runtimeWalletAddress: '0xfeedface',
    })

    expect(state.signedIn).toBe(true)
    expect(state.depositReady).toBe(true)
    expect(state.walletActionReady).toBe(true)
    expect(state.accountSyncPending).toBe(false)
    expect(state.onboardingStage).toBe('ready')
    expect(state.onboardingLabel).toBe('Ready')
  })

  it('maps active auth flows to signing in before the account exists', () => {
    const state = deriveMorosAccountState({
      authReady: false,
      authenticated: false,
      authLoading: true,
      oauthLoading: true,
    })

    expect(state.signedIn).toBe(false)
    expect(state.authPending).toBe(true)
    expect(state.onboardingStage).toBe('signing_in')
    expect(state.onboardingLabel).toBe('Signing in')
  })
})

describe('resolveMorosPrimaryActionLabel', () => {
  it('returns onboarding labels until the account is ready', () => {
    expect(
      resolveMorosPrimaryActionLabel({
        accountState: {
          signedIn: true,
          onboardingStage: 'resolving_account',
          onboardingLabel: 'Resolving account',
        },
        readyLabel: 'Bet',
      }),
    ).toBe('Resolving account')
  })

  it('returns the signed out label when no auth flow is active', () => {
    expect(
      resolveMorosPrimaryActionLabel({
        accountState: {
          signedIn: false,
          onboardingStage: 'signed_out',
          onboardingLabel: 'Sign in',
        },
        readyLabel: 'Bet',
      }),
    ).toBe('Login')
  })

  it('surfaces resolving-account copy even before the signed-in shell flips', () => {
    expect(
      resolveMorosPrimaryActionLabel({
        accountState: {
          signedIn: true,
          onboardingStage: 'resolving_account',
          onboardingLabel: 'Resolving account',
        },
        readyLabel: 'Bet',
      }),
    ).toBe('Resolving account')
  })
})
