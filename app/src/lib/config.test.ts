// @vitest-environment jsdom

import { describe, expect, it } from 'vitest'
import { resolveMorosServicePath, resolveMorosServiceUrl, resolveMorosServiceUrlForOrigin } from './config'

describe('config service URL resolution', () => {
  it('resolves relative service roots against the current origin', () => {
    expect(resolveMorosServiceUrl('/coordinator')).toBe('http://localhost:3000/coordinator')
    expect(resolveMorosServicePath('/coordinator', '/v1/accounts/resolve')).toBe(
      'http://localhost:3000/coordinator/v1/accounts/resolve',
    )
  })

  it('preserves absolute service roots', () => {
    expect(resolveMorosServiceUrl('http://127.0.0.1:18081')).toBe('http://127.0.0.1:18081/')
    expect(resolveMorosServicePath('http://127.0.0.1:18081', '/v1/accounts/resolve')).toBe(
      'http://127.0.0.1:18081/v1/accounts/resolve',
    )
  })

  it('can fall back to same-origin service paths for public browser deployments', () => {
    expect(resolveMorosServiceUrlForOrigin('http://127.0.0.1:18081', 'https://moros.bet', '/coordinator')).toBe(
      'https://moros.bet/coordinator',
    )
  })
})
