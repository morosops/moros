import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useFocusTrap } from '../hooks/useFocusTrap'
import { clearStoredGameplaySession, readStoredGameplaySession } from '../lib/gameplay-session'
import { shortAddress } from '../lib/format'
import { useProfileStore } from '../store/profile'
import { useToastStore } from '../store/toast'
import { useUsernameAvailability } from '../hooks/useUsernameAvailability'

type SettingsDrawerProps = {
  authProvider?: string
  onClose: () => void
  onSaveUsername: (username?: string) => Promise<void>
  open: boolean
  userId?: string
  walletAddress?: string
}

function formatSessionExpiry(expiresAtUnix?: number) {
  if (!expiresAtUnix) {
    return 'No active gameplay session'
  }

  const date = new Date(expiresAtUnix * 1000)
  return new Intl.DateTimeFormat('en-US', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(date)
}

function formatSessionRemaining(expiresAtUnix?: number, nowUnix?: number) {
  if (!expiresAtUnix || !nowUnix) {
    return 'Authorize gameplay when you place a live wager.'
  }

  const remainingSeconds = Math.max(0, expiresAtUnix - nowUnix)
  if (remainingSeconds <= 0) {
    return 'Expired'
  }

  const minutes = Math.floor(remainingSeconds / 60)
  const seconds = remainingSeconds % 60
  if (minutes > 0) {
    return `${minutes}m ${seconds.toString().padStart(2, '0')}s remaining`
  }
  return `${seconds}s remaining`
}

function formatProviderLabel(authProvider?: string) {
  switch (authProvider) {
    case 'wallet':
      return 'External Starknet wallet'
    default:
      return 'Moros account'
  }
}

export function SettingsDrawer({
  authProvider,
  onClose,
  onSaveUsername,
  open,
  userId,
  walletAddress,
}: SettingsDrawerProps) {
  const drawerRef = useRef<HTMLElement | null>(null)
  const username = useProfileStore((state) => state.username)
  const usernameDraft = useProfileStore((state) => state.usernameDraft)
  const setUsernameDraft = useProfileStore((state) => state.setUsernameDraft)
  const resetUsernameDraft = useProfileStore((state) => state.resetUsernameDraft)
  const leaderboardPrivacyEnabled = useProfileStore((state) => state.leaderboardPrivacyEnabled)
  const setLeaderboardPrivacyEnabled = useProfileStore((state) => state.setLeaderboardPrivacyEnabled)
  const pushToast = useToastStore((state) => state.pushToast)
  const [sessionNowUnix, setSessionNowUnix] = useState(() => Math.floor(Date.now() / 1000))
  const [sessionSnapshot, setSessionSnapshot] = useState(() => readStoredGameplaySession())
  const [usernameState, setUsernameState] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle')
  const [usernameFeedback, setUsernameFeedback] = useState<string>()
  const saveTimerRef = useRef<number | undefined>(undefined)
  const userEditedRef = useRef(false)
  const { normalizedDraft, status: usernameStatus, message: usernameMessage } = useUsernameAvailability(usernameDraft, username)
  const deleteAccountHref = useMemo(() => {
    const subject = encodeURIComponent('Moros delete account request')
    const body = encodeURIComponent(
      [
        'Please delete my Moros account.',
        '',
        `User ID: ${userId ?? 'unknown'}`,
        `Wallet: ${walletAddress ?? 'not linked'}`,
        `Login method: ${formatProviderLabel(authProvider)}`,
      ].join('\n'),
    )
    return `mailto:admin@moros.bet?subject=${subject}&body=${body}`
  }, [authProvider, userId, walletAddress])
  const usernameMetaMessage =
    usernameFeedback ?? (normalizedDraft ? usernameMessage : undefined)

  useFocusTrap(drawerRef, open, onClose)

  useEffect(() => {
    if (!open) {
      return
    }

    resetUsernameDraft()
    userEditedRef.current = false
    setUsernameState('idle')
    setUsernameFeedback(undefined)
  }, [open, resetUsernameDraft])

  useEffect(() => {
    if (!open) {
      return
    }

    const syncSession = () => {
      setSessionNowUnix(Math.floor(Date.now() / 1000))
      setSessionSnapshot(readStoredGameplaySession())
    }

    syncSession()
    const interval = window.setInterval(syncSession, 1000)
    return () => window.clearInterval(interval)
  }, [open])

  useEffect(() => {
    if (!open) {
      return
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null
      if (target && drawerRef.current && !drawerRef.current.contains(target)) {
        onClose()
      }
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose()
      }
    }

    document.addEventListener('pointerdown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [onClose, open])

  useEffect(() => {
    if (!open || !userEditedRef.current) {
      return
    }

    if (saveTimerRef.current !== undefined) {
      window.clearTimeout(saveTimerRef.current)
      saveTimerRef.current = undefined
    }

    if (usernameStatus === 'checking') {
      setUsernameState('idle')
      setUsernameFeedback('Checking availability...')
      return
    }

    if (usernameStatus === 'invalid' || usernameStatus === 'taken' || usernameStatus === 'error') {
      setUsernameState('error')
      setUsernameFeedback(usernameMessage)
      return
    }

    const currentUsername = (username ?? '').trim().toLowerCase()
    const nextUsername = normalizedDraft ?? ''

    if (nextUsername === currentUsername) {
      setUsernameState('saved')
      setUsernameFeedback(nextUsername ? `Saved as @${nextUsername}.` : 'Using wallet address as your public profile.')
      return
    }

    saveTimerRef.current = window.setTimeout(() => {
      void (async () => {
        try {
          setUsernameState('saving')
          setUsernameFeedback(nextUsername ? `Saving @${nextUsername}...` : 'Switching back to wallet-address mode...')
          await onSaveUsername(nextUsername || undefined)
          setUsernameState('saved')
          setUsernameFeedback(nextUsername ? `Saved as @${nextUsername}.` : 'Using wallet address as your public profile.')
        } catch (error) {
          setUsernameState('error')
          setUsernameFeedback(error instanceof Error ? error.message : 'Profile update failed.')
        }
      })()
    }, 700)

    return () => {
      if (saveTimerRef.current !== undefined) {
        window.clearTimeout(saveTimerRef.current)
        saveTimerRef.current = undefined
      }
    }
  }, [normalizedDraft, onSaveUsername, open, username, usernameMessage, usernameStatus])

  const handleRevokeSession = useCallback(() => {
    clearStoredGameplaySession()
    setSessionSnapshot(undefined)
    pushToast({
      message: 'Gameplay session revoked on this browser.',
      tone: 'success',
      title: 'Session revoked',
    })
  }, [pushToast])

  const handleUsernameInput = useCallback((value: string) => {
    userEditedRef.current = true
    setUsernameDraft(value)
  }, [setUsernameDraft])

  if (!open) {
    return null
  }

  return (
    <div className="settings-drawer-shell" role="presentation">
      <aside
        aria-label="Profile"
        aria-modal="false"
        className="settings-drawer settings-drawer--edge"
        ref={drawerRef}
        role="dialog"
        tabIndex={-1}
      >
        <header className="settings-drawer__header">
          <div>
            <span className="wallet-funds__section-label">Profile</span>
            <strong>{username ? `@${username}` : shortAddress(walletAddress) ?? 'Moros account'}</strong>
            <small>{formatProviderLabel(authProvider)}</small>
          </div>
          <button aria-label="Close profile" className="settings-drawer__close" onClick={onClose} type="button">
            ×
          </button>
        </header>

        <div className="settings-drawer__body">
          <div className="settings-drawer__stack">
            <section className="settings-drawer__panel">
              <div className="settings-drawer__panel-head">
                <strong>Username</strong>
              </div>
              <label className="wallet-funds__field">
                <span>Username</span>
                <input
                  autoCapitalize="none"
                  autoComplete="username"
                  className="text-input"
                  onChange={(event) => handleUsernameInput(event.target.value)}
                  placeholder={walletAddress ?? 'wallet address'}
                  spellCheck={false}
                  type="text"
                  value={usernameDraft}
                />
              </label>
              {usernameMetaMessage ? (
                <div className="wallet-funds__inline-meta">
                  <span>{usernameState === 'saving' ? usernameFeedback : usernameMetaMessage}</span>
                </div>
              ) : null}
            </section>

            <section className="settings-drawer__panel">
              <label className="settings-drawer__toggle-row">
                <div>
                  <strong>Leaderboard privacy</strong>
                </div>
                <button
                  aria-pressed={leaderboardPrivacyEnabled}
                  className={leaderboardPrivacyEnabled ? 'profile-dropdown__switch profile-dropdown__switch--active' : 'profile-dropdown__switch'}
                  onClick={() => setLeaderboardPrivacyEnabled(!leaderboardPrivacyEnabled)}
                  type="button"
                >
                  <span className="profile-dropdown__switch-thumb" />
                </button>
              </label>
            </section>

            <section className="settings-drawer__panel">
              <div className="settings-drawer__panel-head">
                <strong>Session</strong>
              </div>
              <div className="settings-drawer__identity-list">
                <div className="settings-drawer__identity-row">
                  <span>Status</span>
                  <strong>{sessionSnapshot ? 'Active' : 'Not active'}</strong>
                </div>
                <div className="settings-drawer__identity-row">
                  <span>Expires</span>
                  <strong>{formatSessionExpiry(sessionSnapshot?.expiresAtUnix)}</strong>
                </div>
                <div className="settings-drawer__identity-row">
                  <span>Remaining</span>
                  <strong>{formatSessionRemaining(sessionSnapshot?.expiresAtUnix, sessionNowUnix)}</strong>
                </div>
              </div>
              <div className="settings-drawer__actions">
                <button
                  className="button button--ghost"
                  disabled={!sessionSnapshot}
                  onClick={handleRevokeSession}
                  type="button"
                >
                  Revoke session
                </button>
              </div>
            </section>

            <section className="settings-drawer__panel settings-drawer__panel--danger">
              <div className="settings-drawer__panel-head">
                <strong>Account</strong>
              </div>
              <div className="settings-drawer__identity-list">
                <div className="settings-drawer__identity-row">
                  <span>Login method</span>
                  <strong>{formatProviderLabel(authProvider)}</strong>
                </div>
                <div className="settings-drawer__identity-row">
                  <span>Execution wallet</span>
                  <strong>{walletAddress ? shortAddress(walletAddress) ?? walletAddress : 'Not linked yet'}</strong>
                </div>
                {userId ? (
                  <div className="settings-drawer__identity-row">
                    <span>Moros account</span>
                    <strong>{shortAddress(userId) ?? userId}</strong>
                  </div>
                ) : null}
              </div>
              <div className="settings-drawer__actions">
                <a className="button button--ghost settings-drawer__danger-link" href={deleteAccountHref}>
                  Delete account
                </a>
              </div>
            </section>
          </div>
        </div>
      </aside>
    </div>
  )
}
