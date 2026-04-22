import { useEffect, useRef, useState } from 'react'
import { transferAccountBalances, type BalanceAccount } from '../lib/api'
import { formatStrk } from '../lib/format'
import { useFocusTrap } from '../hooks/useFocusTrap'

type VaultModalProps = {
  open: boolean
  onClose: () => void
  userId?: string
  walletAddress?: string
  signedIn: boolean
  walletActionReady: boolean
  onboardingLabel: string
  balances?: BalanceAccount
  ensureGameplaySession: () => Promise<string>
  onSettled?: () => Promise<void> | void
}

function parseStrkInputToWei(value: string) {
  const trimmed = value.trim()
  if (!trimmed) {
    return '0'
  }

  const [wholePart, fractionalPart = ''] = trimmed.split('.')
  const whole = wholePart.replace(/[^\d]/g, '') || '0'
  const fractional = fractionalPart.replace(/[^\d]/g, '').slice(0, 18).padEnd(18, '0')
  return (BigInt(whole) * 10n ** 18n + BigInt(fractional || '0')).toString()
}

export function VaultModal({
  open,
  onClose,
  userId,
  walletAddress,
  signedIn,
  walletActionReady,
  onboardingLabel,
  balances,
  ensureGameplaySession,
  onSettled,
}: VaultModalProps) {
  const dialogRef = useRef<HTMLElement | null>(null)
  const [amount, setAmount] = useState('10')
  const [direction, setDirection] = useState<'wallet_to_vault' | 'vault_to_wallet'>('wallet_to_vault')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<string>()

  useFocusTrap(dialogRef, open, onClose)

  useEffect(() => {
    if (!open) {
      return
    }

    setMessage(undefined)
  }, [open])

  useEffect(() => {
    if (!open) {
      return
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose()
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose, open])

  async function handleConfirmTransfer() {
    if (!signedIn) {
      setMessage('Log in to move funds.')
      return
    }

    if (!walletActionReady) {
      setMessage(`${onboardingLabel}.`)
      return
    }

    if (!walletAddress || !userId) {
      setMessage('Moros wallet is still preparing. Try again.')
      return
    }

    setBusy(true)
    setMessage(undefined)
    try {
      const sessionToken = await ensureGameplaySession()
      await transferAccountBalances({
        user_id: userId,
        wallet_address: walletAddress,
        direction: direction === 'wallet_to_vault' ? 'gambling_to_vault' : 'vault_to_gambling',
        amount: parseStrkInputToWei(amount),
      }, sessionToken)
      if (onSettled) {
        await onSettled()
      }
      setMessage(direction === 'wallet_to_vault' ? 'Moved into vault.' : 'Moved into wallet balance.')
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'Vault transfer failed.')
    } finally {
      setBusy(false)
    }
  }

  if (!open) {
    return null
  }

  return (
    <div className="deposit-dialog-backdrop" onClick={onClose} role="presentation">
      <section
        aria-label="Vault"
        aria-modal="true"
        className="deposit-dialog withdraw-dialog vault-dialog"
        onClick={(event) => event.stopPropagation()}
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header className="deposit-dialog__header">
          <div className="deposit-dialog__heading">
            <strong>Vault</strong>
            <small>Move STRK between your wallet balance and vault.</small>
          </div>
          <button
            aria-label="Close vault modal"
            className="deposit-dialog__close"
            onClick={onClose}
            type="button"
          >
            ×
          </button>
        </header>

        <div className="deposit-dialog__body withdraw-dialog__body">
          <div className="withdraw-dialog__balances">
            <div className="withdraw-dialog__balance">
              <span>Wallet</span>
              <strong>{formatStrk(balances?.gambling_balance)}</strong>
            </div>
            <div className="withdraw-dialog__balance">
              <span>Vault</span>
              <strong>{formatStrk(balances?.vault_balance)}</strong>
            </div>
          </div>

          <div className="withdraw-dialog__source-toggle" role="tablist" aria-label="Vault direction">
            <button
              aria-selected={direction === 'wallet_to_vault'}
              className={direction === 'wallet_to_vault' ? 'deposit-dialog__tab deposit-dialog__tab--active' : 'deposit-dialog__tab'}
              onClick={() => setDirection('wallet_to_vault')}
              role="tab"
              type="button"
            >
              Wallet to Vault
            </button>
            <button
              aria-selected={direction === 'vault_to_wallet'}
              className={direction === 'vault_to_wallet' ? 'deposit-dialog__tab deposit-dialog__tab--active' : 'deposit-dialog__tab'}
              onClick={() => setDirection('vault_to_wallet')}
              role="tab"
              type="button"
            >
              Vault to Wallet
            </button>
          </div>

          <label className="wallet-funds__field">
            <span>Amount</span>
            <input
              className="text-input"
              inputMode="decimal"
              onChange={(event) => setAmount(event.target.value)}
              type="text"
              value={amount}
            />
          </label>

          {message ? (
            <div className="deposit-dialog__notice">
              <span>{message}</span>
            </div>
          ) : null}

          <div className="withdraw-dialog__actions">
            <button className="button button--ghost" onClick={onClose} type="button">
              Cancel
            </button>
            <button
              className="button button--primary"
              disabled={busy}
              onClick={() => void handleConfirmTransfer()}
              type="button"
            >
              {busy ? 'Moving...' : direction === 'wallet_to_vault' ? 'Move to Vault' : 'Move to Wallet'}
            </button>
          </div>
        </div>
      </section>
    </div>
  )
}
