import { useEffect, useMemo, useRef, useState } from 'react'
import { createAccountWithdrawal, type BalanceAccount } from '../lib/api'
import { formatStrk, formatUsd as formatUsdAmount } from '../lib/format'
import { useFocusTrap } from '../hooks/useFocusTrap'

type WithdrawModalProps = {
  open: boolean
  onClose: () => void
  userId?: string
  walletAddress?: string
  signedIn: boolean
  walletActionReady: boolean
  onboardingLabel: string
  balanceFormatted?: string
  balances?: BalanceAccount
  ensureGameplaySession: () => Promise<string>
  withdraw: (
    amount: string,
    sourceBalance?: 'gambling' | 'vault',
    recipientAddress?: string,
  ) => Promise<{ hash: string }>
  onSettled?: () => Promise<void> | void
}

function parseFormattedStrk(value?: string) {
  if (!value) {
    return 0
  }

  const parsed = Number.parseFloat(value.replace(/[^\d.]/g, ''))
  return Number.isFinite(parsed) ? parsed : 0
}

function formatUsd(value?: number) {
  if (value === undefined || !Number.isFinite(value)) {
    return 'Moros Balance: USD feed unavailable'
  }

  return `Moros Balance: ${formatUsdAmount(value)}`
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

export function WithdrawModal({
  open,
  onClose,
  userId,
  walletAddress,
  signedIn,
  walletActionReady,
  onboardingLabel,
  balanceFormatted,
  balances,
  ensureGameplaySession,
  withdraw,
  onSettled,
}: WithdrawModalProps) {
  const dialogRef = useRef<HTMLElement | null>(null)
  const [amount, setAmount] = useState('25')
  const [destination, setDestination] = useState(walletAddress ?? '')
  const [sourceBalance, setSourceBalance] = useState<'vault' | 'gambling'>('vault')
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<string>()
  const [strkUsdPrice, setStrkUsdPrice] = useState<number>()

  useFocusTrap(dialogRef, open, onClose)

  const usdBalance = useMemo(() => {
    if (strkUsdPrice === undefined) {
      return undefined
    }
    return parseFormattedStrk(balanceFormatted) * strkUsdPrice
  }, [balanceFormatted, strkUsdPrice])

  useEffect(() => {
    if (!open) {
      return
    }

    setMessage(undefined)
    setDestination((current) => current || walletAddress || '')
  }, [open, walletAddress])

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

  useEffect(() => {
    if (!open) {
      return
    }

    let cancelled = false

    async function loadPrice() {
      try {
        const response = await fetch(
          'https://api.coingecko.com/api/v3/simple/price?ids=starknet&vs_currencies=usd',
        )
        if (!response.ok) {
          throw new Error('USD feed unavailable')
        }
        const payload = (await response.json()) as { starknet?: { usd?: number } }
        if (!cancelled) {
          setStrkUsdPrice(typeof payload.starknet?.usd === 'number' ? payload.starknet.usd : undefined)
        }
      } catch {
        if (!cancelled) {
          setStrkUsdPrice(undefined)
        }
      }
    }

    void loadPrice()
    return () => {
      cancelled = true
    }
  }, [open])

  async function handleConfirmWithdraw() {
    if (!signedIn) {
      setMessage('Log in to withdraw funds.')
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

    if (!destination.trim()) {
      setMessage('Enter a destination address.')
      return
    }

    setBusy(true)
    setMessage(undefined)
    try {
      setMessage('Open wallet to confirm withdrawal.')
      const tx = await withdraw(amount, sourceBalance, destination.trim())
      setMessage('Withdrawal confirmed onchain. Recording it in Moros history...')
      const sessionToken = await ensureGameplaySession()
      await createAccountWithdrawal({
        user_id: userId,
        wallet_address: walletAddress,
        source_balance: sourceBalance,
        amount: parseStrkInputToWei(amount),
        destination_address: destination.trim(),
        destination_chain_key: 'starknet',
        destination_asset_symbol: 'STRK',
        destination_tx_hash: tx.hash,
      }, sessionToken)
      if (onSettled) {
        await onSettled()
      }
      setMessage('Withdrawal completed from your on-chain vault.')
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'Withdrawal failed.')
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
        aria-label="Withdraw"
        aria-modal="true"
        className="deposit-dialog withdraw-dialog"
        onClick={(event) => event.stopPropagation()}
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header className="deposit-dialog__header">
          <div className="deposit-dialog__heading">
            <strong>Withdraw</strong>
            <small>{formatUsd(usdBalance)}</small>
          </div>
          <button
            aria-label="Close withdraw modal"
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

          <div className="withdraw-dialog__source-toggle" role="tablist" aria-label="Withdraw source">
            <button
              aria-selected={sourceBalance === 'vault'}
              className={sourceBalance === 'vault' ? 'deposit-dialog__tab deposit-dialog__tab--active' : 'deposit-dialog__tab'}
              onClick={() => setSourceBalance('vault')}
              role="tab"
              type="button"
            >
              Vault
            </button>
            <button
              aria-selected={sourceBalance === 'gambling'}
              className={sourceBalance === 'gambling' ? 'deposit-dialog__tab deposit-dialog__tab--active' : 'deposit-dialog__tab'}
              onClick={() => setSourceBalance('gambling')}
              role="tab"
              type="button"
            >
              Wallet
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

          <label className="wallet-funds__field">
            <span>Destination address</span>
            <input
              className="text-input"
              onChange={(event) => setDestination(event.target.value)}
              placeholder={walletAddress ?? '0x...'}
              type="text"
              value={destination}
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
              onClick={() => void handleConfirmWithdraw()}
              type="button"
            >
              {busy ? 'Withdrawing...' : 'Withdraw'}
            </button>
          </div>
        </div>
      </section>
    </div>
  )
}
