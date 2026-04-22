export const GAMEPLAY_SESSION_KEY = 'moros.gameplay.session'
export const EXTERNAL_WALLET_RESTORE_HINT_KEY = 'moros.wallet.external-restore'
export const GAMEPLAY_SESSION_ONCHAIN_EXPIRY_BUFFER_SECONDS = 300

export type StoredGameplaySession = {
  walletAddress: string
  sessionToken: string
  expiresAtUnix: number
  sessionKeyAddress?: string
}

export function readStoredGameplaySession() {
  if (typeof window === 'undefined') {
    return undefined
  }

  const raw = window.localStorage.getItem(GAMEPLAY_SESSION_KEY)
  if (!raw) {
    return undefined
  }

  try {
    return JSON.parse(raw) as StoredGameplaySession
  } catch {
    window.localStorage.removeItem(GAMEPLAY_SESSION_KEY)
    return undefined
  }
}

export function writeStoredGameplaySession(session: StoredGameplaySession) {
  if (typeof window === 'undefined') {
    return
  }
  window.localStorage.setItem(GAMEPLAY_SESSION_KEY, JSON.stringify(session))
}

export function clearStoredGameplaySession() {
  if (typeof window === 'undefined') {
    return
  }
  window.localStorage.removeItem(GAMEPLAY_SESSION_KEY)
}

export function readExternalWalletRestoreHint() {
  if (typeof window === 'undefined') {
    return false
  }

  try {
    return window.localStorage.getItem(EXTERNAL_WALLET_RESTORE_HINT_KEY) === '1'
  } catch {
    return false
  }
}

export function writeExternalWalletRestoreHint(enabled: boolean) {
  if (typeof window === 'undefined') {
    return
  }

  try {
    if (enabled) {
      window.localStorage.setItem(EXTERNAL_WALLET_RESTORE_HINT_KEY, '1')
      return
    }
    window.localStorage.removeItem(EXTERNAL_WALLET_RESTORE_HINT_KEY)
  } catch {
    // Storage can be unavailable in constrained environments.
  }
}

export function gameplaySessionMatchesAddress(session: StoredGameplaySession | undefined, address?: string) {
  if (!session || !address) {
    return false
  }
  return session.walletAddress.toLowerCase() === address.toLowerCase()
}

export function gameplaySessionMatchesKey(
  session: StoredGameplaySession | undefined,
  sessionKeyAddress?: string,
) {
  if (!session) {
    return false
  }
  if (!sessionKeyAddress) {
    return true
  }
  if (!session.sessionKeyAddress) {
    return false
  }
  return session.sessionKeyAddress.toLowerCase() === sessionKeyAddress.toLowerCase()
}
