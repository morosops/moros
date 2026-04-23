import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  createAuthenticatedDepositChannel,
  createDepositChannel,
  fetchDepositStatus,
  fetchDepositSupportedAssets,
  type CreateDepositChannelResponse,
  type DepositRecovery,
  type DepositRiskFlag,
  type DepositRouteJob,
  type DepositStatusResponse,
  type DepositSupportedAsset,
  type DepositTransfer,
} from '../lib/api'

type DepositRouterPanelProps = {
  walletAddress?: string
  idToken?: string
  resolveIdToken?: () => Promise<string | null | undefined>
}

type ChainOption = {
  chainKey: string
  label: string
  network: string
}

const CHAIN_PRIORITY: Record<string, number> = {
  'starknet-sepolia': 0,
  'starknet-mainnet': 0,
  'ethereum-mainnet': 1,
  'solana-testnet': 2,
  'solana-mainnet': 2,
}

const ENABLED_DEPOSIT_MATRIX = new Set([
  'starknet-sepolia:strk',
  'starknet-sepolia:eth',
  'starknet-sepolia:usdc',
  'starknet-mainnet:strk',
  'starknet-mainnet:eth',
  'starknet-mainnet:usdc',
  'ethereum-mainnet:eth',
  'ethereum-mainnet:usdc',
  'solana-testnet:sol',
  'solana-mainnet:sol',
])

const MAX_AUTH_PREPARATION_RETRIES = 4
const DEPOSIT_STATUS_POLL_MS = 4_000
const AUTH_PREPARATION_RETRY_MS = 250

function formatRouteError(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback
}

function humanizePart(value: string) {
  return value
    .split(/[-_]/g)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function formatChainLabel(asset: DepositSupportedAsset) {
  const metadataLabel = asset.metadata?.chain_label
  if (typeof metadataLabel === 'string' && metadataLabel.trim()) {
    return metadataLabel.trim()
  }

  const [family, network] = asset.chain_key.split('-')
  if (!network) {
    return humanizePart(asset.chain_key)
  }

  return `${humanizePart(family)} ${humanizePart(network)}`
}

function formatAssetLabel(asset: DepositSupportedAsset) {
  const metadataLabel = asset.metadata?.label
  if (typeof metadataLabel === 'string' && metadataLabel.trim()) {
    return metadataLabel.trim()
  }

  return `${asset.asset_symbol} on ${formatChainLabel(asset)}`
}

function formatWalletAddress(value?: string | null) {
  if (!value) {
    return 'Unknown sender'
  }
  return `${value.slice(0, 6)}...${value.slice(-4)}`
}

function formatTimestamp(value?: string | null) {
  if (!value) {
    return undefined
  }
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) {
    return value
  }
  return new Intl.DateTimeFormat('en-US', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(parsed)
}

function formatUnits(raw: string, decimals: number, symbol: string) {
  try {
    const value = BigInt(raw)
    const divisor = 10n ** BigInt(decimals)
    const whole = value / divisor
    const fraction = (value % divisor).toString().padStart(decimals, '0').slice(0, 6)
    const trimmed = fraction.replace(/0+$/, '')
    return trimmed ? `${whole}.${trimmed} ${symbol}` : `${whole} ${symbol}`
  } catch {
    return `${raw} ${symbol}`
  }
}

function summarizeTransferStatus(transfer?: DepositTransfer | null) {
  if (!transfer) {
    return {
      badge: 'Awaiting deposit',
      detail: 'Send funds to this fixed route address. Moros will detect the transfer, convert it to STRK, and credit your gambling balance after confirmation.',
    }
  }

  switch (transfer.status) {
    case 'DEPOSIT_DETECTED':
      return {
        badge: 'Deposit detected',
        detail: `${transfer.confirmations}/${transfer.required_confirmations} confirmations observed on the source chain.`,
      }
    case 'ORIGIN_CONFIRMED':
      return {
        badge: 'Confirmed on source chain',
        detail: 'The deposit is confirmed and queued for STRK conversion.',
      }
    case 'PROCESSING':
      return {
        badge: 'Routing to STRK',
        detail: 'Bridge and swap execution is in progress.',
      }
    case 'COMPLETED':
      return {
        badge: 'Credited',
        detail: 'STRK has been credited to your Moros balance.',
      }
    case 'FLAGGED':
      return {
        badge: 'Held for review',
        detail: 'This deposit is under review.',
      }
    case 'RECOVERY_REQUIRED':
      return {
        badge: 'Recovery required',
        detail: 'This deposit needs manual recovery before it can be credited.',
      }
      default:
      return {
        badge: transfer.status.replace(/_/g, ' '),
        detail: 'Deposit status updated.',
      }
  }
}

function summarizeRouterState({
  transfer,
  latestRouteJob,
  latestRiskFlag,
  latestRecovery,
}: {
  transfer?: DepositTransfer | null
  latestRouteJob?: DepositRouteJob | null
  latestRiskFlag?: DepositRiskFlag | null
  latestRecovery?: DepositRecovery | null
}) {
  if (latestRecovery && latestRecovery.status === 'open') {
    return {
      badge: 'Recovery required',
      detail: latestRecovery.reason
        ? `Manual recovery is open: ${latestRecovery.reason.replace(/_/g, ' ')}.`
        : 'This deposit needs manual recovery before it can be credited.',
    }
  }

  if (latestRiskFlag && latestRiskFlag.resolution_status === 'open') {
    return {
      badge: 'Held for review',
      detail: latestRiskFlag.description || 'This deposit is under review.',
    }
  }

  if (latestRouteJob) {
    const stage =
      latestRouteJob.response && typeof latestRouteJob.response.stage === 'string'
        ? latestRouteJob.response.stage
        : undefined

    switch (latestRouteJob.status) {
      case 'queued':
        return {
          badge: 'Queued for routing',
          detail: 'The deposit is confirmed and queued for STRK conversion.',
        }
      case 'dispatching':
        return {
          badge: 'Routing started',
          detail: 'Moros is starting the STRK conversion flow.',
        }
      case 'processing':
        if (stage === 'starknet_fee_topup_submitted') {
          return {
            badge: 'Preparing route',
            detail: 'Moros is preparing the STRK conversion flow.',
          }
        }
        if (stage === 'source_transfer_submitted') {
          return {
            badge: 'Processing deposit',
            detail: 'Your deposit is being moved into the STRK conversion flow.',
          }
        }
        if (stage === 'bridge_submitted' || stage === 'solana_bridge_submitted') {
          return {
            badge: 'Bridge submitted',
            detail: 'Your deposit is bridging to the STRK settlement path.',
          }
        }
        if (stage === 'swap_submitted') {
          return {
            badge: 'Swapping to STRK',
            detail: 'Your deposit arrived and is being converted to STRK.',
          }
        }
        if (stage === 'bankroll_credit_submitted') {
          return {
            badge: 'Crediting balance',
            detail: 'STRK is being added to your Moros balance.',
          }
        }
        return {
          badge: 'Routing to STRK',
          detail: 'Your deposit is being converted to STRK.',
        }
      case 'retryable':
        return {
          badge: 'Retrying',
          detail: latestRouteJob.last_error
            ? `The last processing attempt failed: ${latestRouteJob.last_error}`
            : 'The last processing attempt can be retried.',
        }
      case 'failed':
        return {
          badge: 'Processing failed',
          detail: latestRouteJob.last_error
            ? `Processing failed: ${latestRouteJob.last_error}`
            : 'This deposit failed and needs support review.',
        }
      case 'completed':
        return {
          badge: 'Credited',
          detail: 'STRK has been credited to your Moros balance.',
        }
      default:
        break
    }
  }

  return summarizeTransferStatus(transfer)
}

export function DepositRouterPanel({
  walletAddress,
  idToken,
  resolveIdToken,
}: DepositRouterPanelProps) {
  const [assets, setAssets] = useState<DepositSupportedAsset[]>([])
  const [assetsLoading, setAssetsLoading] = useState(false)
  const [assetsError, setAssetsError] = useState<string>()
  const [selectedChainKey, setSelectedChainKey] = useState('')
  const [selectedAssetId, setSelectedAssetId] = useState('')
  const [channelResponse, setChannelResponse] = useState<CreateDepositChannelResponse>()
  const [statusResponse, setStatusResponse] = useState<DepositStatusResponse>()
  const [channelLoading, setChannelLoading] = useState(false)
  const [statusLoading, setStatusLoading] = useState(false)
  const [panelMessage, setPanelMessage] = useState<string>()
  const [copyMessage, setCopyMessage] = useState<string>()
  const [qrDataUrl, setQrDataUrl] = useState<string>()
  const [channelRetryKey, setChannelRetryKey] = useState(0)

  const chainOptions = useMemo<ChainOption[]>(() => {
    const seen = new Set<string>()
    return assets
      .filter((asset) => asset.status === 'enabled')
      .filter((asset) => ENABLED_DEPOSIT_MATRIX.has(`${asset.chain_key}:${asset.id}`))
      .filter((asset) => {
        if (seen.has(asset.chain_key)) {
          return false
        }
        seen.add(asset.chain_key)
        return true
      })
      .map((asset) => ({
        chainKey: asset.chain_key,
        label: formatChainLabel(asset),
        network: asset.network,
      }))
      .sort((left, right) => {
        const leftRank = CHAIN_PRIORITY[left.chainKey] ?? 50
        const rightRank = CHAIN_PRIORITY[right.chainKey] ?? 50
        if (leftRank !== rightRank) {
          return leftRank - rightRank
        }
        return left.label.localeCompare(right.label)
      })
  }, [assets])

  const selectedChainAssets = useMemo(
    () =>
      assets.filter(
        (asset) =>
          asset.chain_key === selectedChainKey &&
          asset.status === 'enabled' &&
          ENABLED_DEPOSIT_MATRIX.has(`${asset.chain_key}:${asset.id}`),
      ),
    [assets, selectedChainKey],
  )

  const selectedAsset = useMemo(
    () =>
      selectedChainAssets.find((asset) => asset.id === selectedAssetId) ??
      selectedChainAssets[0],
    [selectedAssetId, selectedChainAssets],
  )

  const latestTransfer = statusResponse?.transfers?.[0]
  const latestRouteJob = statusResponse?.route_jobs?.[0]
  const latestRiskFlag = statusResponse?.risk_flags?.[0]
  const latestRecovery = statusResponse?.recoveries?.[0]
  const transferSummary = summarizeRouterState({
    transfer: latestTransfer,
    latestRouteJob,
    latestRiskFlag,
    latestRecovery,
  })

  const loadAssets = useCallback(async () => {
    setAssetsLoading(true)
    setAssetsError(undefined)
    try {
      const response = await fetchDepositSupportedAssets()
      setAssets(response)
    } catch (error) {
      setAssetsError(formatRouteError(error, 'Could not load deposit routes.'))
    } finally {
      setAssetsLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadAssets()
  }, [loadAssets])

  useEffect(() => {
    if (!chainOptions.length) {
      setSelectedChainKey('')
      return
    }

    if (!chainOptions.some((option) => option.chainKey === selectedChainKey)) {
      setSelectedChainKey(chainOptions[0].chainKey)
    }
  }, [chainOptions, selectedChainKey])

  useEffect(() => {
    if (!selectedChainAssets.length) {
      setSelectedAssetId('')
      return
    }

    if (!selectedChainAssets.some((asset) => asset.id === selectedAssetId)) {
      setSelectedAssetId(selectedChainAssets[0].id)
    }
  }, [selectedAssetId, selectedChainAssets])

  useEffect(() => {
    if ((!walletAddress && !idToken && !resolveIdToken) || !selectedAsset) {
      setChannelResponse(undefined)
      setStatusResponse(undefined)
      setQrDataUrl(undefined)
      setPanelMessage(undefined)
      return
    }

    let cancelled = false
    let retryTimer: number | undefined
    setChannelResponse(undefined)
    setStatusResponse(undefined)
    setQrDataUrl(undefined)
    setChannelLoading(true)
    setPanelMessage(undefined)

    const createChannel = async () => {
      if (walletAddress) {
        return createDepositChannel({
          wallet_address: walletAddress,
          asset_id: selectedAsset.id,
          chain_key: selectedAsset.chain_key,
        })
      }

      let requestToken = idToken

      if (!requestToken && resolveIdToken) {
        requestToken = (await resolveIdToken()) ?? undefined
      }

      if (!requestToken) {
        setChannelResponse(undefined)
        setStatusResponse(undefined)
        setQrDataUrl(undefined)
        if (channelRetryKey >= MAX_AUTH_PREPARATION_RETRIES) {
          const allowedOrigin = typeof window === 'undefined'
            ? 'this origin'
            : window.location.origin
          setPanelMessage(
            `Privy did not return a Moros auth token. Check that ${allowedOrigin} is allowed in Privy and that the session is still valid, then retry.`,
          )
          return
        }
        setPanelMessage('Preparing your Moros deposit routes.')
        retryTimer = window.setTimeout(() => {
          if (!cancelled) {
            setChannelRetryKey((current) => current + 1)
          }
        }, AUTH_PREPARATION_RETRY_MS)
        return
      }

      const requestBody = {
        asset_id: selectedAsset.id,
        chain_key: selectedAsset.chain_key,
      }

      try {
        return await createAuthenticatedDepositChannel(requestToken, requestBody)
      } catch (error) {
        if (requestToken && resolveIdToken) {
          const refreshedToken = (await resolveIdToken()) ?? undefined
          if (refreshedToken && refreshedToken !== requestToken) {
            return createAuthenticatedDepositChannel(refreshedToken, requestBody)
          }
        }
        throw error
      }
    }

    void createChannel()
      .then((response) => {
        if (cancelled || !response) {
          return
        }
        setChannelResponse(response)
      })
      .catch((error) => {
        if (!cancelled) {
          setChannelResponse(undefined)
          setStatusResponse(undefined)
          setQrDataUrl(undefined)
          setPanelMessage(formatRouteError(error, 'Could not create a deposit route address.'))
        }
      })
      .finally(() => {
        if (!cancelled) {
          setChannelLoading(false)
        }
      })

    return () => {
      cancelled = true
      if (retryTimer) {
        window.clearTimeout(retryTimer)
      }
    }
  }, [channelRetryKey, idToken, resolveIdToken, selectedAsset, walletAddress])

  const refreshStatus = useCallback(async () => {
    const depositAddress = channelResponse?.channel.deposit_address
    if (!depositAddress) {
      return
    }

    setStatusLoading(true)
    try {
      const response = await fetchDepositStatus(depositAddress)
      setStatusResponse(response)
      setPanelMessage(undefined)
    } catch (error) {
      setPanelMessage(formatRouteError(error, 'Could not refresh deposit status.'))
    } finally {
      setStatusLoading(false)
    }
  }, [channelResponse?.channel.deposit_address])

  useEffect(() => {
    if (!channelResponse?.channel.deposit_address) {
      setStatusResponse(undefined)
      return
    }

    let cancelled = false

    const load = async () => {
      try {
        const response = await fetchDepositStatus(channelResponse.channel.deposit_address)
        if (!cancelled) {
          setStatusResponse(response)
          setPanelMessage(undefined)
        }
      } catch (error) {
        if (!cancelled) {
          setPanelMessage(formatRouteError(error, 'Could not refresh deposit status.'))
        }
      } finally {
        if (!cancelled) {
          setStatusLoading(false)
        }
      }
    }

    setStatusLoading(true)
    void load()
    const interval = window.setInterval(() => {
      void load()
    }, DEPOSIT_STATUS_POLL_MS)

    return () => {
      cancelled = true
      window.clearInterval(interval)
    }
  }, [channelResponse?.channel.deposit_address])

  useEffect(() => {
    const payload = channelResponse?.channel.qr_payload
    if (!payload) {
      setQrDataUrl(undefined)
      return
    }

    let cancelled = false
    void import('qrcode')
      .then((module) =>
        module.toDataURL(payload, {
          errorCorrectionLevel: 'M',
          margin: 1,
          width: 240,
          color: {
            dark: '#f3f5fb',
            light: '#0f1218',
          },
        }),
      )
      .then((dataUrl) => {
        if (!cancelled) {
          setQrDataUrl(dataUrl)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setQrDataUrl(undefined)
        }
      })

    return () => {
      cancelled = true
    }
  }, [channelResponse?.channel.qr_payload])

  const copyValue = useCallback(async (value: string, successLabel: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setCopyMessage(successLabel)
    } catch {
      setCopyMessage('Copy failed. Copy the value manually.')
    }
  }, [])

  return (
    <div className="wallet-funds__panel-body deposit-router">
      <div className="deposit-router__selectors">
        <label className="wallet-funds__field">
          <span>Chain</span>
          <select
            className="text-input"
            disabled={assetsLoading || !chainOptions.length}
            onChange={(event) => setSelectedChainKey(event.target.value)}
            value={selectedChainKey}
          >
            {chainOptions.map((option) => (
              <option key={option.chainKey} value={option.chainKey}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <label className="wallet-funds__field">
          <span>Token</span>
          <select
            className="text-input"
            disabled={assetsLoading || !selectedChainAssets.length}
            onChange={(event) => setSelectedAssetId(event.target.value)}
            value={selectedAsset?.id ?? ''}
          >
            {selectedChainAssets.map((asset) => (
              <option key={`${asset.chain_key}:${asset.id}`} value={asset.id}>
                {asset.asset_symbol}
              </option>
            ))}
          </select>
        </label>
      </div>

      {selectedAsset ? (
        <div className="wallet-funds__inline-meta">
          <span>{formatAssetLabel(selectedAsset)}</span>
          <span>
            Limits: {formatUnits(selectedAsset.min_amount, selectedAsset.asset_decimals, selectedAsset.asset_symbol)} min
            {' '}·{' '}
            {formatUnits(selectedAsset.max_amount, selectedAsset.asset_decimals, selectedAsset.asset_symbol)} max
          </span>
          <span>{selectedAsset.confirmations_required} source-chain confirmations required</span>
        </div>
      ) : null}

      {assetsLoading ? <div className="wallet-funds__inline-meta"><span>Loading deposit routes…</span></div> : null}
      {assetsError ? <div className="wallet-funds__inline-meta"><span>{assetsError}</span></div> : null}
      {!assetsLoading && !assetsError && !assets.length ? (
        <div className="wallet-funds__inline-meta">
          <span>No deposit routes are configured on this Moros deployment.</span>
        </div>
      ) : null}
      {panelMessage && !channelResponse ? (
        <div className="wallet-funds__inline-meta">
          <span>{panelMessage}</span>
          <button
            className="wallet-funds__inline-button"
            onClick={() => setChannelRetryKey((value) => value + 1)}
            type="button"
          >
            Retry
          </button>
        </div>
      ) : null}

      {selectedAsset && channelResponse ? (
        <div className="deposit-router__layout">
          <div className="deposit-router__address-panel">
            <div className="deposit-router__address-card">
              <span className="wallet-funds__section-label">Funding route address</span>
              <div className="deposit-router__address-row">
                <input
                  className="text-input"
                  readOnly
                  type="text"
                  value={channelResponse.channel.deposit_address}
                />
                <button
                  className="button button--ghost"
                  onClick={() => void copyValue(channelResponse.channel.deposit_address, 'Funding route address copied.')}
                  type="button"
                >
                  Copy
                </button>
              </div>
            </div>

            <div className="deposit-router__address-card">
              <span className="wallet-funds__section-label">Status</span>
              <div className="deposit-router__status-head">
                <strong>{transferSummary.badge}</strong>
                <button className="wallet-funds__inline-button" onClick={() => void refreshStatus()} type="button">
                  {statusLoading ? 'Refreshing…' : 'Refresh'}
                </button>
              </div>
              <p className="deposit-router__status-copy">{transferSummary.detail}</p>
              <div className="wallet-funds__inline-meta">
                <span>This address stays fixed for this chain.</span>
                <span>Supported tokens on the same chain reuse this address. Moros detects the asset from the incoming transfer.</span>
                <span>This is a Moros routing endpoint for the selected chain, not the final Starknet vault destination.</span>
                <span>Confirmed deposits are routed into STRK automatically and then credited to your Moros gambling balance.</span>
                {channelLoading ? <span>Refreshing deposit channel…</span> : null}
                {copyMessage ? <span>{copyMessage}</span> : null}
                {panelMessage ? <span>{panelMessage}</span> : null}
              </div>
            </div>
          </div>

          <div className="deposit-router__qr-panel">
            <div className="deposit-router__qr-frame">
              {qrDataUrl ? <img alt="Deposit QR code" src={qrDataUrl} /> : <span>QR loading…</span>}
            </div>
            <div className="deposit-router__qr-actions">
              <button
                className="button button--ghost"
                onClick={() => void copyValue(channelResponse.channel.qr_payload, 'Deposit QR payload copied.')}
                type="button"
              >
                Copy QR payload
              </button>
              <span>Send only {selectedAsset.asset_symbol} on {formatChainLabel(selectedAsset)}.</span>
            </div>
          </div>
        </div>
      ) : null}

      {channelResponse && statusResponse?.transfers.length ? (
        <div className="deposit-router__history">
          <div className="deposit-router__history-head">
            <span className="wallet-funds__section-label">Recent deposits</span>
            <span>{statusResponse.transfers.length} tracked</span>
          </div>
          <div className="deposit-router__history-list">
            {statusResponse.transfers.slice(0, 4).map((transfer) => (
              <div className="deposit-router__history-row" key={transfer.transfer_id}>
                <div>
                  <strong>{transfer.amount_display} {transfer.asset_symbol}</strong>
                  <small>
                    {formatWalletAddress(transfer.sender_address)} · {formatTimestamp(transfer.detected_at) ?? 'Just detected'}
                  </small>
                </div>
                <div className="deposit-router__history-meta">
                  <span>{transfer.status.replace(/_/g, ' ')}</span>
                  <small>
                    {transfer.status === 'COMPLETED'
                      ? formatWalletAddress(transfer.destination_tx_hash)
                      : `${transfer.confirmations}/${transfer.required_confirmations} conf`}
                  </small>
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : channelResponse ? (
        <div className="wallet-funds__inline-meta">
          <span>No source transfer detected yet for this route address.</span>
        </div>
      ) : null}
    </div>
  )
}
