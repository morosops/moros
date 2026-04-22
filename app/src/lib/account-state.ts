import type { MorosEmailState } from '../components/moros-auth-context'
import type { WalletConnectionState } from '../store/wallet'

export type MorosAccountStateInput = {
  authReady: boolean
  authenticated: boolean
  authLoading?: boolean
  oauthLoading?: boolean
  emailState?: MorosEmailState
  accountUserId?: string
  accountWalletAddress?: string
  runtimeWalletAddress?: string
  walletStatus?: WalletConnectionState
}

export type MorosOnboardingStage =
  | 'signed_out'
  | 'signing_in'
  | 'resolving_account'
  | 'preparing_wallet'
  | 'ready'

export type MorosAccountState = {
  resolvedWalletAddress?: string
  signedIn: boolean
  depositReady: boolean
  walletActionReady: boolean
  accountPending: boolean
  accountSyncPending: boolean
  walletPending: boolean
  authPending: boolean
  onboardingBusy: boolean
  onboardingStage: MorosOnboardingStage
  onboardingLabel: string
}

function isMorosAuthPending(input: Pick<MorosAccountStateInput, 'authLoading' | 'oauthLoading' | 'emailState'>) {
  return Boolean(
    input.authLoading ||
    input.oauthLoading ||
    input.emailState === 'sending-code' ||
    input.emailState === 'awaiting-code' ||
    input.emailState === 'verifying',
  )
}

function onboardingLabelFor(stage: MorosOnboardingStage) {
  switch (stage) {
    case 'signing_in':
      return 'Signing in'
    case 'resolving_account':
      return 'Resolving account'
    case 'preparing_wallet':
      return 'Preparing wallet'
    case 'ready':
      return 'Ready'
    case 'signed_out':
    default:
      return 'Sign in'
  }
}

export function deriveMorosAccountState({
  authReady,
  authenticated,
  authLoading,
  oauthLoading,
  emailState,
  accountUserId,
  accountWalletAddress,
  runtimeWalletAddress,
  walletStatus,
}: MorosAccountStateInput): MorosAccountState {
  const authResolved = authReady && authenticated
  const resolvedWalletAddress = accountWalletAddress ?? runtimeWalletAddress
  const walletResolved = Boolean(resolvedWalletAddress)
  const runtimeWalletReady = Boolean(runtimeWalletAddress)
  const signedIn = Boolean(authResolved || accountUserId || walletResolved)
  const authPending = isMorosAuthPending({ authLoading, oauthLoading, emailState })
  const executionWalletKnown = walletResolved || runtimeWalletReady
  const accountPending = authResolved && !accountUserId && !walletResolved
  const accountSyncPending = signedIn && !accountUserId && executionWalletKnown
  const walletPending = Boolean(accountUserId) && !executionWalletKnown
  const walletPreparing =
    Boolean(accountUserId) &&
    executionWalletKnown &&
    (!runtimeWalletReady || walletStatus === 'connecting' || walletStatus === 'preparing')

  let onboardingStage: MorosOnboardingStage
  if (!signedIn) {
    onboardingStage = authPending
      ? 'signing_in'
      : accountPending
        ? 'resolving_account'
        : 'signed_out'
  } else if (accountPending) {
    onboardingStage = 'resolving_account'
  } else if (walletPending || walletPreparing) {
    onboardingStage = 'preparing_wallet'
  } else {
    onboardingStage = 'ready'
  }

  return {
    resolvedWalletAddress,
    signedIn,
    depositReady: executionWalletKnown,
    walletActionReady: onboardingStage === 'ready',
    accountPending,
    accountSyncPending,
    walletPending,
    authPending,
    onboardingBusy: onboardingStage !== 'signed_out' && onboardingStage !== 'ready',
    onboardingStage,
    onboardingLabel: onboardingLabelFor(onboardingStage),
  }
}

type MorosPrimaryActionLabelInput = {
  accountState: Pick<MorosAccountState, 'signedIn' | 'onboardingLabel' | 'onboardingStage'>
  pendingLabel?: string
  readyLabel: string
  signedOutLabel?: string
  walletBusy?: boolean
}

export function resolveMorosPrimaryActionLabel({
  accountState,
  pendingLabel,
  readyLabel,
  signedOutLabel = 'Login',
  walletBusy = false,
}: MorosPrimaryActionLabelInput) {
  if (accountState.onboardingStage !== 'signed_out' && accountState.onboardingStage !== 'ready') {
    if (walletBusy && pendingLabel) {
      return pendingLabel
    }
    return accountState.onboardingLabel
  }

  if (!accountState.signedIn) {
    return signedOutLabel
  }

  if (walletBusy) {
    return pendingLabel ?? accountState.onboardingLabel
  }

  if (accountState.onboardingStage !== 'ready') {
    return accountState.onboardingLabel
  }

  return readyLabel
}
