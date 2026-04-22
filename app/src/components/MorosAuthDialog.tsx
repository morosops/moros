import { useEffect, useState } from 'react'
import { fetchUsernameAvailability } from '../lib/api'
import type { ExternalMorosWalletOption } from '../lib/starkzap-types'
import type { MorosEmailState } from './MorosAuthProvider'

type MorosAuthDialogProps = {
  open: boolean
  mode: 'login' | 'signup'
  address?: string
  username?: string
  needsUsername: boolean
  privyEnabled: boolean
  authReady: boolean
  emailState: MorosEmailState
  oauthLoading: boolean
  pendingLabel?: string
  error?: string
  walletOptions: ExternalMorosWalletOption[]
  walletsLoading: boolean
  onGoogleLogin: () => Promise<void>
  onOpenWallets: () => Promise<void>
  onSendEmailCode: (email: string) => Promise<void>
  onVerifyEmailCode: (code: string) => Promise<void>
  onWalletLogin: (provider?: ExternalMorosWalletOption['provider']) => Promise<void>
  onSaveProfile: (username?: string) => Promise<void>
  onClose: () => void
}

type DialogStep = 'methods' | 'wallets' | 'username'
type UsernameStatus = 'idle' | 'checking' | 'available' | 'taken' | 'invalid' | 'error'

function normalizeUsername(value: string) {
  return value.trim().toLowerCase()
}

function isValidUsername(value: string) {
  return /^[a-z0-9_]{3,16}$/.test(value)
}

function shortAddress(address?: string) {
  if (!address) {
    return undefined
  }
  return `${address.slice(0, 6)}...${address.slice(-4)}`
}

export function MorosAuthDialog({
  open,
  mode,
  address,
  username,
  needsUsername,
  privyEnabled,
  authReady: _authReady,
  emailState,
  oauthLoading,
  pendingLabel,
  error,
  walletOptions,
  walletsLoading,
  onGoogleLogin,
  onOpenWallets,
  onSendEmailCode,
  onVerifyEmailCode,
  onWalletLogin,
  onSaveProfile,
  onClose,
}: MorosAuthDialogProps) {
  const [step, setStep] = useState<DialogStep>('methods')
  const [email, setEmail] = useState('')
  const [code, setCode] = useState('')
  const [usernameInput, setUsernameInput] = useState('')
  const [usernameStatus, setUsernameStatus] = useState<UsernameStatus>('idle')
  const [usernameMessage, setUsernameMessage] = useState<string>()
  const [busyAction, setBusyAction] = useState<string>()

  useEffect(() => {
    if (!open) {
      setStep('methods')
      setEmail('')
      setCode('')
      setBusyAction(undefined)
      setUsernameStatus('idle')
      setUsernameMessage(undefined)
      return
    }

    if (needsUsername) {
      setStep('username')
      return
    }

    setStep('methods')
  }, [needsUsername, open])

  useEffect(() => {
    if (needsUsername && !usernameInput && !username) {
      setUsernameInput('')
    }
  }, [needsUsername, username, usernameInput])

  useEffect(() => {
    if (!needsUsername) {
      return
    }

    const normalized = normalizeUsername(usernameInput)
    if (!normalized) {
      setUsernameStatus('idle')
      setUsernameMessage(undefined)
      return
    }

    if (!isValidUsername(normalized)) {
      setUsernameStatus('invalid')
      setUsernameMessage('Use 3-16 lowercase letters, digits, or underscores.')
      return
    }

    let cancelled = false
    setUsernameStatus('checking')
    setUsernameMessage('Checking availability...')

    const timeout = window.setTimeout(() => {
      void fetchUsernameAvailability(normalized)
        .then((result) => {
          if (cancelled) {
            return
          }
          if (result.available) {
            setUsernameStatus('available')
            setUsernameMessage(`${normalized} is available.`)
            return
          }
          setUsernameStatus('taken')
          setUsernameMessage(`${normalized} is already taken.`)
        })
        .catch(() => {
          if (cancelled) {
            return
          }
          setUsernameStatus('error')
          setUsernameMessage('Could not verify username availability.')
        })
    }, 220)

    return () => {
      cancelled = true
      window.clearTimeout(timeout)
    }
  }, [needsUsername, usernameInput])

  if (!open) {
    return null
  }

  async function runAction(label: string, action: () => Promise<void>) {
    setBusyAction(label)
    try {
      await action()
    } finally {
      setBusyAction(undefined)
    }
  }

  async function handleSaveUsername() {
    const normalized = normalizeUsername(usernameInput)
    if (!normalized) {
      await onSaveProfile(undefined)
      return
    }

    if (!isValidUsername(normalized)) {
      setUsernameStatus('invalid')
      setUsernameMessage('Use 3-16 lowercase letters, digits, or underscores.')
      return
    }

    if (usernameStatus === 'taken') {
      return
    }

    await onSaveProfile(normalized)
  }

  async function handleOpenWallets() {
    setStep('wallets')
    await onOpenWallets()
  }

  const emailAwaitingCode = emailState === 'awaiting-code' || emailState === 'verifying'
  const authTitle = step === 'username'
    ? 'Choose username'
    : step === 'wallets'
      ? 'Continue with Wallet'
      : mode === 'signup'
        ? 'Create account'
        : 'Log in'

  return (
    <div className="login-overlay" role="presentation" onClick={onClose}>
      <div
        aria-label={authTitle}
        aria-modal="true"
        className="login-sheet login-sheet--polymarket"
        onClick={(event) => event.stopPropagation()}
        role="dialog"
      >
        <div className="login-sheet__header">
          <div>
            <h2>{authTitle}</h2>
          </div>
          <button aria-label="Close login" className="login-sheet__close" onClick={onClose} type="button">
            <span aria-hidden="true">×</span>
          </button>
        </div>

        {step === 'methods' ? (
          <div className="moros-auth-stack">
            <div className="moros-auth-section">
              <strong className="moros-auth-section__title">Choose how to continue</strong>
              <div className="login-methods">
                <button
                  className="login-method"
                  disabled={!privyEnabled || oauthLoading || Boolean(busyAction)}
                  onClick={() => void runAction('google', onGoogleLogin)}
                  type="button"
                >
                  <strong>{oauthLoading && busyAction === 'google' ? 'Opening Google...' : 'Continue with Google'}</strong>
                </button>

                <div className="login-method login-method--email-auth">
                  <strong>Continue with Email</strong>
                  <div className="moros-auth-inline">
                    <input
                      autoComplete={emailAwaitingCode ? 'one-time-code' : 'email'}
                      className="text-input"
                      inputMode={emailAwaitingCode ? 'numeric' : undefined}
                      onChange={(event) =>
                        emailAwaitingCode ? setCode(event.target.value) : setEmail(event.target.value)
                      }
                      placeholder={emailAwaitingCode ? '123456' : 'name@example.com'}
                      type={emailAwaitingCode ? 'text' : 'email'}
                      value={emailAwaitingCode ? code : email}
                    />
                    <button
                      className="button button--primary"
                      disabled={
                        !privyEnabled ||
                        Boolean(busyAction) ||
                        (emailAwaitingCode
                          ? !code.trim() || emailState === 'verifying'
                          : !email.trim() || emailState === 'sending-code')
                      }
                      onClick={() =>
                        void runAction(
                          emailAwaitingCode ? 'email-verify' : 'email-send',
                          () => (emailAwaitingCode ? onVerifyEmailCode(code.trim()) : onSendEmailCode(email.trim())),
                        )
                      }
                      type="button"
                    >
                      {emailAwaitingCode
                        ? emailState === 'verifying'
                          ? 'Verifying...'
                          : 'Continue'
                        : emailState === 'sending-code'
                          ? 'Sending...'
                          : 'Continue'}
                    </button>
                  </div>
                </div>

                <button
                  className="login-method"
                  disabled={walletsLoading || Boolean(busyAction)}
                  onClick={() => void runAction('wallets-open', handleOpenWallets)}
                  type="button"
                >
                  <strong>{walletsLoading && busyAction === 'wallets-open' ? 'Scanning wallets...' : 'Continue with Wallet'}</strong>
                </button>
              </div>
            </div>
          </div>
        ) : null}

        {step === 'wallets' ? (
          <div className="moros-auth-stack">
            <div className="moros-auth-section">
              <strong className="moros-auth-section__title">Detected Starknet wallets</strong>
              {walletsLoading ? (
                <p className="stack-note">Scanning installed wallets…</p>
              ) : null}

              {!walletsLoading && walletOptions.length ? (
                <div className="moros-auth-wallet-list">
                  {walletOptions.map((option) => (
                    <button
                      className="login-method moros-auth-wallet-button"
                      disabled={Boolean(busyAction)}
                      key={option.id}
                      onClick={() => void runAction(`wallet:${option.id}`, () => onWalletLogin(option.provider))}
                      type="button"
                    >
                      <span className="moros-auth-wallet">
                        <img alt="" className="moros-auth-wallet__icon" src={option.icon} />
                        <span className="moros-auth-wallet__copy">
                          <strong>{option.name}</strong>
                          <span>Connect the wallet and continue into Moros.</span>
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              ) : null}

              {!walletsLoading && !walletOptions.length ? (
                <div className="moros-auth-empty">
                  <strong>No Starknet wallet detected.</strong>
                  <span>Install Braavos or Argent X, or use Google/email to open a Moros wallet.</span>
                </div>
              ) : null}
            </div>

            <button className="button button--ghost" onClick={() => setStep('methods')} type="button">
              Back
            </button>
          </div>
        ) : null}

        {step === 'username' ? (
          <div className="moros-auth-stack">
            <div className="moros-auth-section">
              <strong className="moros-auth-section__title">
                Connected as {shortAddress(address) ?? 'wallet'}
              </strong>
              <label className="stack-field">
                <span>Username</span>
                <input
                  autoCapitalize="none"
                  autoComplete="username"
                  className="text-input text-input--large"
                  onChange={(event) => setUsernameInput(event.target.value)}
                  placeholder={shortAddress(address) ?? 'moros'}
                  spellCheck={false}
                  type="text"
                  value={usernameInput}
                />
              </label>
              {usernameMessage ? (
                <p
                  className={
                    usernameStatus === 'taken' || usernameStatus === 'invalid' || usernameStatus === 'error'
                      ? 'stack-note stack-note--error'
                      : 'stack-note'
                  }
                >
                  {usernameMessage}
                </p>
              ) : null}
            </div>

            <div className="moros-auth-actions">
              <button
                className="button button--primary"
                disabled={
                  busyAction === 'save-username' ||
                  usernameStatus === 'taken' ||
                  usernameStatus === 'invalid' ||
                  usernameStatus === 'checking'
                }
                onClick={() => void runAction('save-username', handleSaveUsername)}
                type="button"
              >
                {busyAction === 'save-username' ? 'Saving...' : 'Continue'}
              </button>
            </div>
          </div>
        ) : null}

        {pendingLabel ? <p className="stack-note">{pendingLabel}</p> : null}
        {error ? <p className="stack-note stack-note--error">{error}</p> : null}
      </div>
    </div>
  )
}
