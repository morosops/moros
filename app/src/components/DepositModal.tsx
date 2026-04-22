import { useEffect, useMemo, useState } from 'react'
import { DepositRouterPanel } from './DepositRouterPanel'

type DepositModalTab = 'crypto' | 'cash'

type DepositModalProps = {
  open: boolean
  onClose: () => void
  walletAddress?: string
  idToken?: string
  resolveIdToken?: () => Promise<string | null | undefined>
  balanceFormatted?: string
  signedIn?: boolean
  preparing?: boolean
  error?: string
  onRetry?: () => void
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

  return `Moros Balance: ${new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: value >= 1000 ? 0 : 2,
    maximumFractionDigits: value >= 1000 ? 0 : 2,
  }).format(value)}`
}

export function DepositModal({
  open,
  onClose,
  walletAddress,
  idToken,
  resolveIdToken,
  balanceFormatted,
  signedIn = false,
  preparing = false,
  error,
  onRetry,
}: DepositModalProps) {
  const [activeTab, setActiveTab] = useState<DepositModalTab>('crypto')
  const [strkUsdPrice, setStrkUsdPrice] = useState<number>()

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

  if (!open) {
    return null
  }

  return (
    <div className="deposit-dialog-backdrop" onClick={onClose} role="presentation">
      <section
        aria-label="Deposit"
        className="deposit-dialog"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="deposit-dialog__header">
          <div className="deposit-dialog__heading">
            <strong>Deposit</strong>
            <small>{formatUsd(usdBalance)}</small>
          </div>
          <button
            aria-label="Close deposit modal"
            className="deposit-dialog__close"
            onClick={onClose}
            type="button"
          >
            ×
          </button>
        </header>

        <div className="deposit-dialog__tabs" role="tablist" aria-label="Deposit options">
          <button
            aria-selected={activeTab === 'crypto'}
            className={activeTab === 'crypto' ? 'deposit-dialog__tab deposit-dialog__tab--active' : 'deposit-dialog__tab'}
            onClick={() => setActiveTab('crypto')}
            role="tab"
            type="button"
          >
            Use Crypto
          </button>
          <button
            aria-selected={activeTab === 'cash'}
            className={activeTab === 'cash' ? 'deposit-dialog__tab deposit-dialog__tab--active' : 'deposit-dialog__tab'}
            onClick={() => setActiveTab('cash')}
            role="tab"
            type="button"
          >
            Use Cash
          </button>
        </div>

        <div className="deposit-dialog__body">
          {activeTab === 'crypto' ? (
            <div className="deposit-dialog__detail">
              {walletAddress || idToken || resolveIdToken ? (
                <DepositRouterPanel
                  idToken={idToken}
                  resolveIdToken={resolveIdToken}
                  walletAddress={walletAddress}
                />
              ) : (
                <div className="deposit-dialog__notice">
                  <span>
                    {preparing
                      ? 'Preparing your Moros deposit routes.'
                      : error ?? (
                        signedIn
                          ? 'Preparing your Moros deposit routes.'
                          : 'Log in to generate a deposit route address.'
                      )}
                  </span>
                  {error && onRetry ? (
                    <button className="button button--primary button--compact" onClick={onRetry} type="button">
                      Retry
                    </button>
                  ) : null}
                </div>
              )}
            </div>
          ) : (
            <>
              <div className="deposit-dialog__notice">
                <span>Card deposits are not enabled yet. Use Crypto to generate a deposit route address.</span>
              </div>
            </>
          )}
        </div>
      </section>
    </div>
  )
}
