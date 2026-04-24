import { useCallback } from 'react'
import { useMorosAuthRuntime } from '../components/MorosAuthProvider'
import { waitForPrivyAccessToken, waitForPrivyRequestToken } from '../lib/privy-auth'
import { ensureMorosPrivyWallet } from '../lib/privy-bridge'
import {
  GAMEPLAY_SESSION_ONCHAIN_EXPIRY_BUFFER_SECONDS,
  clearStoredGameplaySession,
  gameplaySessionMatchesAddress,
  gameplaySessionMatchesKey,
  readExternalWalletRestoreHint,
  readStoredGameplaySession,
  writeExternalWalletRestoreHint,
  writeStoredGameplaySession,
} from '../lib/gameplay-session'
import { useAccountStore } from '../store/account'
import { useWalletStore } from '../store/wallet'
import {
  createGameplaySession,
  createGameplaySessionChallenge,
  openBaccaratRoundRelayed,
  openDiceRoundRelayed,
  openRouletteSpinRelayed,
} from '../lib/api'
import { morosConfig } from '../lib/config'
import type {
  ConnectMorosWalletOptions,
  ExternalMorosWalletOption,
  OpenBaccaratRoundPayload,
  OpenDiceRoundPayload,
  OpenRouletteSpinPayload,
  SwapToStrkPayload,
} from '../lib/starkzap-types'

type StarkzapModule = typeof import('../lib/starkzap')
type StarkzapFinanceModule = typeof import('../lib/starkzap-finance')

let starkzapModulePromise: Promise<StarkzapModule> | null = null
let starkzapFinanceModulePromise: Promise<StarkzapFinanceModule> | null = null
let privyConnectPromise:
  | Promise<{ address: string; strategy: 'external' | 'privy'; walletLink?: unknown }>
  | null = null
let gameplaySessionPromise: Promise<string> | null = null
let gameplaySessionPromiseMode: 'background' | 'foreground' | null = null

async function loadStarkzap() {
  if (!starkzapModulePromise) {
    starkzapModulePromise = import('../lib/starkzap')
  }
  return starkzapModulePromise
}

async function loadStarkzapFinance() {
  if (!starkzapFinanceModulePromise) {
    starkzapFinanceModulePromise = import('../lib/starkzap-finance')
  }
  return starkzapFinanceModulePromise
}

function normalizeWalletError(error: unknown, fallback: string) {
  if (error instanceof Error) {
    if (error.message.includes('User interaction required')) {
      return 'Open wallet to continue.'
    }
    if (error.message.toLowerCase().includes('gameplay session grant')) {
      return 'Moros gameplay authorization expired. Retry the action.'
    }
    if (isPrivySignerAuthError(error)) {
      return 'Moros wallet authorization expired. Retry the action.'
    }
    return error.message
  }
  return fallback
}

function isInteractionRequiredError(error: unknown) {
  return error instanceof Error && error.message.includes('User interaction required')
}

function isPrivySignerAuthError(error: unknown) {
  if (!(error instanceof Error)) {
    return false
  }

  const message = error.message.toLowerCase()
  return (
    message.includes('invalid jwt token provided') ||
    message.includes('"code":"invalid_data"') ||
    message.includes('invalid_data') ||
    message.includes('no valid authorization keys') ||
    message.includes('user signing keys')
  )
}

function shouldReconnectWalletForError(error: unknown) {
  return isInteractionRequiredError(error) || isPrivySignerAuthError(error)
}

function loginProgressLabel(step: 'CONNECTED' | 'CHECK_DEPLOYED' | 'DEPLOYING' | 'FAILED' | 'READY') {
  switch (step) {
    case 'CONNECTED':
      return 'Wallet connected.'
    case 'CHECK_DEPLOYED':
      return 'Checking account...'
    case 'DEPLOYING':
      return 'Deploying account...'
    case 'READY':
      return 'Wallet ready.'
    case 'FAILED':
    default:
      return 'Wallet setup failed.'
  }
}

export function useMorosWallet() {
  const store = useWalletStore()
  const accountStore = useAccountStore()
  const auth = useMorosAuthRuntime()
  const loginRequiredMessage = auth.authenticated
    ? 'Moros wallet is still preparing. Try again.'
    : 'Log in to Moros to continue.'
  const accountRequiredMessage = auth.authenticated
    ? 'Moros wallet is still preparing. Try again.'
    : 'Connect a wallet before continuing.'

  const resolvePrivyIdentityToken = useCallback(async () => {
    return waitForPrivyRequestToken(auth, {
      attempts: 20,
      delayMs: 150,
    })
  }, [auth])

  const resolvePrivySigningToken = useCallback(async () => {
    return waitForPrivyAccessToken(auth, {
      attempts: 20,
      delayMs: 150,
    })
  }, [auth])

  const finalizeConnection = useCallback(
    async ({
      wallet,
      strategy,
      walletLink,
    }: {
      wallet: NonNullable<typeof store.wallet>
      strategy: 'external' | 'privy'
      walletLink?: unknown
    }) => {
      const { prepareMorosWalletForExecution, readStrkBalance } = await loadStarkzap()
      const initialBalance = await readStrkBalance(wallet)
      const cachedGameplaySession = readStoredGameplaySession()
      if (
        !gameplaySessionMatchesAddress(cachedGameplaySession, wallet.address)
        || !gameplaySessionMatchesKey(cachedGameplaySession, morosConfig.gameplaySessionKey)
      ) {
        clearStoredGameplaySession()
      }
      writeExternalWalletRestoreHint(strategy === 'external')
      store.setConnected(wallet, strategy, initialBalance.formatted, initialBalance.unit)
      accountStore.setResolvedAccount({
        walletAddress: wallet.address,
        authMethod:
          strategy === 'external'
            ? 'wallet'
            : accountStore.authMethod ?? 'privy',
      })

      void Promise.all([prepareMorosWalletForExecution(wallet), readStrkBalance(wallet)])
        .then(([, refreshed]) => {
          store.setConnected(wallet, strategy, refreshed.formatted, refreshed.unit)
        })
        .catch(() => {})

      return {
        address: wallet.address,
        strategy,
        walletLink,
      }
    },
    [accountStore, store],
  )

  const connectExternal = useCallback(
    async (provider?: ExternalMorosWalletOption['provider'], options: ConnectMorosWalletOptions = {}) => {
      store.setConnecting('Connecting wallet...')
      try {
        const { connectMorosExternalWallet } = await loadStarkzap()
        const result = await connectMorosExternalWallet(provider)
        return await finalizeConnection(result)
      } catch (error) {
        store.setError(normalizeWalletError(error, 'Wallet connection failed.'))
        throw error
      }
    },
    [finalizeConnection, store],
  )

  const connectPrivy = useCallback(
    async (idToken?: string | null, options: ConnectMorosWalletOptions = {}) => {
      if (privyConnectPromise) {
        return privyConnectPromise
      }

      const task = (async () => {
      const resolvedIdToken = idToken ?? await resolvePrivyIdentityToken()
      if (!resolvedIdToken) {
        const error = new Error('Login with Google or email before connecting the Moros wallet.')
        store.setError(error.message)
        throw error
      }
      const resolvedSigningToken = await resolvePrivySigningToken()
      if (!resolvedSigningToken) {
        const error = new Error('Moros auth is still preparing. Retry in a moment.')
        store.setError(error.message)
        throw error
      }

      store.setConnecting('Preparing wallet...')
      try {
        const walletLink = await ensureMorosPrivyWallet(resolvedIdToken)
        if (!walletLink) {
          throw new Error('Privy Starknet wallet is not ready. Retry in a moment.')
        }
        const { connectMorosPrivyWallet } = await loadStarkzap()
        const result = await connectMorosPrivyWallet({
          ...options,
          idToken: resolvedIdToken,
          signingToken: resolvedSigningToken,
          walletLink,
          resolveSigningToken: resolvePrivySigningToken,
          resolvePaymasterToken: resolvePrivySigningToken,
          signRawHash: auth.signStarknetHash,
          onProgress: (event) => {
            if (event.step === 'CONNECTED') {
              return
            }
            store.setPreparing(loginProgressLabel(event.step))
            options.onProgress?.(event)
          },
        })
        return await finalizeConnection(result)
      } catch (error) {
        store.setError(normalizeWalletError(error, 'Wallet connection failed.'))
        throw error
      }
      })()

      privyConnectPromise = task.finally(() => {
        privyConnectPromise = null
      })

      return privyConnectPromise
    },
    [
      auth.signStarknetHash,
      finalizeConnection,
      resolvePrivyIdentityToken,
      resolvePrivySigningToken,
      store,
    ],
  )

  const restore = useCallback(async () => {
    if (store.wallet && store.address) {
      return { address: store.address, strategy: store.strategy }
    }

    if (!readExternalWalletRestoreHint()) {
      return undefined
    }

    try {
      const { reconnectMorosExternalWallet } = await loadStarkzap()
      const result = await reconnectMorosExternalWallet()
      if (result) {
        return await finalizeConnection(result)
      }
      writeExternalWalletRestoreHint(false)
    } catch {
      writeExternalWalletRestoreHint(false)
      return undefined
    }

    return undefined
  }, [
    finalizeConnection,
    store,
  ])

  const connect = useCallback(async (options: ConnectMorosWalletOptions = {}) => {
    if (store.wallet && store.address && store.strategy) {
      return { address: store.address, strategy: store.strategy }
    }

    if (auth.authenticated) {
      const identityToken = await resolvePrivyIdentityToken()
      if (identityToken) {
        return connectPrivy(identityToken, options)
      }
    }

    const error = new Error(loginRequiredMessage)
    store.setError(error.message)
    throw error
  }, [
    auth.authenticated,
    connectPrivy,
    loginRequiredMessage,
    resolvePrivyIdentityToken,
    store,
  ])

  const ensureCanonicalPrivyWallet = useCallback(async () => {
    const currentState = useWalletStore.getState()
    const canonicalWalletAddress = useAccountStore.getState().walletAddress
    const canUsePrivyRuntime =
      auth.authenticated && currentState.strategy !== 'external'
    const needsPrivyReconnect = Boolean(
      canUsePrivyRuntime &&
      canonicalWalletAddress &&
      (
        !currentState.wallet ||
        !currentState.address ||
        currentState.address.toLowerCase() !== canonicalWalletAddress.toLowerCase()
      ),
    )

    if (!needsPrivyReconnect) {
      return currentState
    }

    const identityToken = await resolvePrivyIdentityToken()
    if (!identityToken) {
      throw new Error('Moros wallet is still preparing. Try again.')
    }

    await connectPrivy(identityToken)
    return useWalletStore.getState()
  }, [auth.authenticated, connectPrivy, resolvePrivyIdentityToken])

  const warmConnect = useCallback(() => {
    void loadStarkzap()
  }, [])

  const disconnect = useCallback(async () => {
    if (!store.wallet) {
      return
    }

    try {
      const { disconnectMorosWallet } = await loadStarkzap()
      await disconnectMorosWallet(store.wallet)
      clearStoredGameplaySession()
      writeExternalWalletRestoreHint(false)
      store.reset()
      accountStore.clearAccount()
    } catch (error) {
      store.setError(normalizeWalletError(error, 'Wallet disconnect failed.'))
    }
  }, [accountStore, store])

  const refreshBalance = useCallback(async () => {
    if (!store.wallet) {
      return
    }

    try {
      const { readStrkBalance } = await loadStarkzap()
      const balance = await readStrkBalance(store.wallet)
      store.setBalance(balance.formatted, balance.unit)
    } catch (error) {
      store.setError(normalizeWalletError(error, 'Balance refresh failed.'))
    }
  }, [store])

  const listExternalWallets = useCallback(async () => {
    try {
      const { listAvailableMorosExternalWallets } = await loadStarkzap()
      return await listAvailableMorosExternalWallets()
    } catch (error) {
      store.setError(normalizeWalletError(error, 'Wallet discovery failed.'))
      throw error
    }
  }, [store])

  const reconnectForInteraction = useCallback(async () => {
    if (store.strategy === 'privy') {
      const identityToken = await resolvePrivyIdentityToken()
      if (!identityToken) {
        return undefined
      }

      const result = await connectPrivy(identityToken)
      if (!store.wallet || result.address !== store.wallet.address) {
        return useWalletStore.getState().wallet
      }
      return store.wallet
    }

    const { connectMorosExternalWallet } = await loadStarkzap()
    const result = await connectMorosExternalWallet()
    await finalizeConnection(result)
    return useWalletStore.getState().wallet
  }, [connectPrivy, finalizeConnection, resolvePrivyIdentityToken, store.strategy, store.wallet])

  const signTypedData = useCallback(async (typedData: Record<string, unknown>) => {
    if (!store.wallet) {
      throw new Error(accountRequiredMessage)
    }

    let activeWallet = store.wallet

    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        const { signMorosMessage } = await loadStarkzap()
        return await signMorosMessage(activeWallet, typedData)
      } catch (error) {
        if (!shouldReconnectWalletForError(error) || attempt === 1) {
          store.setError(normalizeWalletError(error, 'Signature request failed.'))
          throw error
        }

        const reconnected = await reconnectForInteraction()
        if (!reconnected) {
          const interactionError = new Error('Open wallet to continue.')
          store.setError(interactionError.message)
          throw interactionError
        }
        activeWallet = reconnected
      }
    }

    throw new Error('Signature request failed.')
  }, [accountRequiredMessage, reconnectForInteraction, store])

  const ensureGameplaySessionInternal = useCallback(async (
    options?: {
      background?: boolean
      suppressStoreError?: boolean
      retryAfterBackgroundFailure?: boolean
    },
  ) => {
    const background = Boolean(options?.background)
    const suppressStoreError = Boolean(options?.suppressStoreError)
    const retryAfterBackgroundFailure = options?.retryAfterBackgroundFailure !== false
    let currentState = await ensureCanonicalPrivyWallet()
    const connectedWallet = currentState.wallet
    const connectedAddress = currentState.address
    if (!connectedWallet || !connectedAddress) {
      throw new Error(accountRequiredMessage)
    }

    const cachedSession = readStoredGameplaySession()
    const nowUnix = Math.floor(Date.now() / 1000)
    if (
      gameplaySessionMatchesAddress(cachedSession, connectedAddress)
      && gameplaySessionMatchesKey(cachedSession, morosConfig.gameplaySessionKey)
      && cachedSession
      && cachedSession.expiresAtUnix > nowUnix + 30
    ) {
      if (!background) {
        store.setReady()
      }
      return cachedSession.sessionToken
    }

    if (gameplaySessionPromise) {
      if (!background) {
        store.setPreparing('Authorizing gameplay...')
      }
      const sharedMode = gameplaySessionPromiseMode
      try {
        const sessionToken = await gameplaySessionPromise
        if (!background) {
          store.setReady()
        }
        return sessionToken
      } catch (error) {
        const canRetryForeground =
          !background &&
          retryAfterBackgroundFailure &&
          sharedMode === 'background'

        if (canRetryForeground) {
          gameplaySessionPromise = null
          gameplaySessionPromiseMode = null
          return ensureGameplaySessionInternal({
            background: false,
            suppressStoreError,
            retryAfterBackgroundFailure: false,
          })
        }

        if (!background && !suppressStoreError) {
          store.setError(normalizeWalletError(error, 'Gameplay authorization failed.'))
        } else if (!background) {
          store.setReady()
        }
        throw error
      }
    }

    const task = (async () => {
      if (!background) {
        store.setPreparing('Preparing wallet...')
      }
      const { prepareMorosWalletForExecution, registerMorosGameplaySession } = await loadStarkzap()
      let activeWallet = connectedWallet
      let activeAddress = connectedAddress
      for (let attempt = 0; attempt < 2; attempt += 1) {
        try {
          await prepareMorosWalletForExecution(activeWallet)
          break
        } catch (error) {
          if (!shouldReconnectWalletForError(error) || attempt === 1) {
            throw error
          }
          const reconnected = await reconnectForInteraction()
          if (!reconnected) {
            throw new Error('Open wallet to continue.')
          }
          activeWallet = reconnected
          activeAddress = useWalletStore.getState().address ?? activeAddress
        }
      }

      if (!background) {
        store.setPreparing('Authorizing gameplay...')
      }
      const challenge = await createGameplaySessionChallenge({
        wallet_address: activeAddress,
      })
      const signature = await signTypedData(challenge.typed_data)
      const session = await createGameplaySession({
        wallet_address: activeAddress,
        challenge_id: challenge.challenge_id,
        signature,
      })
      const sessionKeyAddress = morosConfig.gameplaySessionKey
      if (!sessionKeyAddress) {
        throw new Error('VITE_MOROS_GAMEPLAY_SESSION_KEY_ADDRESS is not configured.')
      }
      if (!background) {
        store.setPreparing('Enabling gameplay session...')
      }
      for (let attempt = 0; attempt < 2; attempt += 1) {
        try {
          await registerMorosGameplaySession(activeWallet, {
            sessionKeyAddress,
            maxWagerWei: morosConfig.gameplaySessionMaxWagerWei,
            expiresAtUnix:
              session.expires_at_unix + GAMEPLAY_SESSION_ONCHAIN_EXPIRY_BUFFER_SECONDS,
          })
          break
        } catch (error) {
          if (!shouldReconnectWalletForError(error) || attempt === 1) {
            throw error
          }
          const reconnected = await reconnectForInteraction()
          if (!reconnected) {
            throw new Error('Open wallet to continue.')
          }
          activeWallet = reconnected
          activeAddress = useWalletStore.getState().address ?? activeAddress
        }
      }
      writeStoredGameplaySession({
        walletAddress: session.wallet_address,
        sessionToken: session.session_token,
        expiresAtUnix: session.expires_at_unix,
        sessionKeyAddress,
      })
      if (!background) {
        store.setReady()
      }
      return session.session_token
    })()
      .catch((error) => {
        if (!background && !suppressStoreError) {
          store.setError(normalizeWalletError(error, 'Gameplay authorization failed.'))
        } else if (!background) {
          store.setReady()
        }
        throw error
      })
      .finally(() => {
        gameplaySessionPromise = null
        gameplaySessionPromiseMode = null
      })

    gameplaySessionPromise = task
    gameplaySessionPromiseMode = background ? 'background' : 'foreground'
    return task
  }, [accountRequiredMessage, ensureCanonicalPrivyWallet, signTypedData, store])

  const ensureGameplaySession = useCallback(async () => {
    return ensureGameplaySessionInternal({ suppressStoreError: true })
  }, [ensureGameplaySessionInternal])

  const prewarmGameplaySession = useCallback(async () => {
    try {
      await ensureGameplaySessionInternal({ background: true })
    } catch {
      // Foreground gameplay actions will retry with user-visible errors when needed.
    }
  }, [ensureGameplaySessionInternal])

  const runWalletExecution = useCallback(async <T,>(
    execute: (wallet: NonNullable<typeof store.wallet>) => Promise<{ hash: string; explorerUrl?: string; wait: () => Promise<T> }>,
    labels: {
      submitting: string
      awaitingApproval?: string
      confirming?: string
    },
  ) => {
    if (!store.wallet) {
      throw new Error(accountRequiredMessage)
    }

    let activeWallet = store.wallet
    let tx

    for (let attempt = 0; attempt < 2; attempt += 1) {
      const needsApprovalPrompt = attempt > 0
      store.setFunding(
        needsApprovalPrompt ? (labels.awaitingApproval ?? labels.submitting) : labels.submitting,
      )

      try {
        const { prepareMorosWalletForExecution } = await loadStarkzap()
        await prepareMorosWalletForExecution(activeWallet)
        tx = await execute(activeWallet)
        break
      } catch (error) {
        if (!shouldReconnectWalletForError(error) || attempt === 1) {
          throw new Error(normalizeWalletError(error, 'Transaction failed.'))
        }

        const reconnected = await reconnectForInteraction()
        if (!reconnected) {
          throw new Error('Open wallet to continue.')
        }
        activeWallet = reconnected
      }
    }

    if (!tx) {
      throw new Error('Open wallet to continue.')
    }

    store.setConfirming(tx.hash, tx.explorerUrl, labels.confirming)
    await tx.wait()
    await refreshBalance()
    return tx
  }, [accountRequiredMessage, reconnectForInteraction, refreshBalance, store])

  const runRelayedGameplayAction = useCallback(async <T,>(
    execute: (sessionToken: string) => Promise<T>,
    labels: {
      submitting: string
    },
  ) => {
    if (!store.wallet || !store.address) {
      throw new Error(accountRequiredMessage)
    }

    store.setFunding(labels.submitting)
    let succeeded = false
    try {
      let sessionToken = await ensureGameplaySession()

      for (let attempt = 0; attempt < 2; attempt += 1) {
        try {
          const result = await execute(sessionToken)
          succeeded = true
          return result
        } catch (error) {
          const message = normalizeWalletError(error, 'Gameplay request failed.')
          const shouldRefreshSession =
            message.includes('401') ||
            message.toLowerCase().includes('gameplay session grant')
          if (!shouldRefreshSession || attempt === 1) {
            throw new Error(message)
          }
          clearStoredGameplaySession()
          sessionToken = await ensureGameplaySession()
        }
      }

      throw new Error('Gameplay request failed.')
    } catch (error) {
      store.setReady()
      throw error
    } finally {
      if (succeeded) {
        store.setReady()
      }
    }
  }, [accountRequiredMessage, ensureGameplaySession, store])

  const fund = useCallback(async (amount: string) =>
    runWalletExecution(
      async (wallet) => {
        const { fundMorosBankroll } = await loadStarkzap()
        return fundMorosBankroll(wallet, amount)
      },
      {
        submitting: 'Preparing bankroll deposit...',
        awaitingApproval: 'Open wallet to confirm bankroll deposit...',
        confirming: 'Confirming bankroll deposit...',
      },
    ), [runWalletExecution])

  const swapIntoStrk = useCallback(async (payload: SwapToStrkPayload) =>
    runWalletExecution(
      async (wallet) => {
        const { swapToStrk } = await loadStarkzapFinance()
        return swapToStrk(wallet, payload)
      },
      {
        submitting: 'Preparing swap...',
        awaitingApproval: 'Open wallet to confirm swap...',
        confirming: 'Confirming swap...',
      },
    ), [runWalletExecution])

  const withdraw = useCallback(async (
    amount: string,
    sourceBalance: 'gambling' | 'vault' = 'vault',
    recipientAddress?: string,
  ) =>
    runWalletExecution(
      async (wallet) => {
        const { withdrawMorosBankroll } = await loadStarkzap()
        return withdrawMorosBankroll(wallet, amount, sourceBalance, recipientAddress)
      },
      {
        submitting: 'Preparing withdrawal...',
        awaitingApproval: 'Open wallet to confirm withdrawal...',
        confirming: 'Confirming withdrawal...',
      },
    ), [runWalletExecution])

  const openDiceRound = useCallback(async (payload: OpenDiceRoundPayload) =>
    runRelayedGameplayAction(
      async (sessionToken) => {
        const activeAddress = useWalletStore.getState().address ?? store.address
        if (!activeAddress) {
          throw new Error(accountRequiredMessage)
        }
        return openDiceRoundRelayed(sessionToken, {
          table_id: payload.tableId,
          player: activeAddress,
          wager: payload.wagerWei,
          target_bps: payload.targetBps,
          roll_over: payload.rollOver,
          client_seed: payload.clientSeed,
          commitment_id: payload.commitmentId,
        })
      },
      {
        submitting: 'Opening dice round...',
      },
    ), [accountRequiredMessage, runRelayedGameplayAction, store.address])

  const openRouletteSpin = useCallback(async (payload: OpenRouletteSpinPayload) =>
    runRelayedGameplayAction(
      async (sessionToken) => {
        const activeAddress = useWalletStore.getState().address ?? store.address
        if (!activeAddress) {
          throw new Error(accountRequiredMessage)
        }
        return openRouletteSpinRelayed(sessionToken, {
          table_id: payload.tableId,
          player: activeAddress,
          total_wager: payload.totalWagerWei,
          client_seed: payload.clientSeed,
          commitment_id: payload.commitmentId,
          bets: payload.bets.map((bet) => ({
            kind: bet.kind,
            selection: bet.selection,
            amount: bet.amountWei,
          })),
        })
      },
      {
        submitting: 'Opening roulette spin...',
      },
    ), [accountRequiredMessage, runRelayedGameplayAction, store.address])

  const openBaccaratRound = useCallback(async (payload: OpenBaccaratRoundPayload) =>
    runRelayedGameplayAction(
      async (sessionToken) => {
        const activeAddress = useWalletStore.getState().address ?? store.address
        if (!activeAddress) {
          throw new Error(accountRequiredMessage)
        }
        return openBaccaratRoundRelayed(sessionToken, {
          table_id: payload.tableId,
          player: activeAddress,
          wager: payload.wagerWei,
          bet_side: payload.betSide,
          client_seed: payload.clientSeed,
          commitment_id: payload.commitmentId,
        })
      },
      {
        submitting: 'Opening baccarat deal...',
      },
    ), [accountRequiredMessage, runRelayedGameplayAction, store.address])

  return {
    ...store,
    connect,
    connectExternal,
    connectPrivy,
    restore,
    disconnect,
    refreshBalance,
    fund,
    withdraw,
    swapIntoStrk,
    openDiceRound,
    openRouletteSpin,
    openBaccaratRound,
    ensureGameplaySession,
    prewarmGameplaySession,
    signTypedData,
    listExternalWallets,
    warmConnect,
  }
}
