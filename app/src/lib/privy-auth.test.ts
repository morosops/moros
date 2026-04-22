// @vitest-environment jsdom

import { describe, expect, it, vi } from 'vitest'
import { resolvePrivyAuthSubject, waitForPrivyAccessToken, waitForPrivyRequestToken } from './privy-auth'
import type { MorosAuthRuntime } from '../components/moros-auth-context'

function createRuntime(overrides: Partial<MorosAuthRuntime> = {}): MorosAuthRuntime {
  return {
    enabled: true,
    loaded: true,
    loading: false,
    ready: true,
    authenticated: true,
    emailState: 'idle',
    oauthLoading: false,
    ensureLoaded: vi.fn(),
    getAccessToken: vi.fn().mockResolvedValue(null),
    sendEmailCode: vi.fn(),
    verifyEmailCode: vi.fn(),
    loginWithGoogle: vi.fn(),
    getIdentityToken: vi.fn().mockResolvedValue(null),
    logout: vi.fn(),
    ...overrides,
  }
}

describe('resolvePrivyAuthSubject', () => {
  it('prefers the runtime userId when present', async () => {
    const subject = await resolvePrivyAuthSubject(createRuntime({
      userId: 'did:privy:user_123',
    }))

    expect(subject).toBe('did:privy:user_123')
  })

  it('falls back to the JWT sub claim when userId is missing', async () => {
    const payload = { sub: 'did:privy:user_from_token' }
    const token = `header.${btoa(JSON.stringify(payload))}.signature`

    const subject = await resolvePrivyAuthSubject(createRuntime({
      userId: undefined,
      identityToken: token,
    }))

    expect(subject).toBe('did:privy:user_from_token')
  })

  it('polls until a request token becomes available', async () => {
    let attempts = 0
    const token = await waitForPrivyRequestToken(
      createRuntime({
        getAccessToken: vi.fn().mockImplementation(async () => {
          attempts += 1
          return attempts >= 3 ? 'privy-access-token' : null
        }),
        getIdentityToken: vi.fn().mockResolvedValue(null),
      }),
      {
        attempts: 3,
        delayMs: 0,
      },
    )

    expect(token).toBe('privy-access-token')
  })

  it('prefers the identity token over the access token when both exist', async () => {
    const token = await waitForPrivyRequestToken(
      createRuntime({
        identityToken: 'privy-identity-token',
        accessToken: 'privy-access-token',
        getAccessToken: vi.fn().mockResolvedValue('privy-access-token'),
        getIdentityToken: vi.fn().mockResolvedValue('privy-identity-token'),
      }),
      {
        attempts: 1,
        delayMs: 0,
      },
    )

    expect(token).toBe('privy-identity-token')
  })

  it('uses the access token for Privy wallet signing requests', async () => {
    const token = await waitForPrivyAccessToken(
      createRuntime({
        identityToken: 'privy-identity-token',
        accessToken: 'privy-access-token',
      }),
      {
        attempts: 1,
        delayMs: 0,
      },
    )

    expect(token).toBe('privy-access-token')
  })
})
