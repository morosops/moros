export function normalizeWalletError(error: unknown, fallback: string) {
  if (error instanceof Error) {
    if (isWalletSetupFundingError(error)) {
      return 'One-time Moros wallet setup needs a small STRK gas balance. Fund the execution wallet or enable sponsored wallet setup, then retry.'
    }
    if (error.message.includes('User interaction required')) {
      return 'Open wallet to continue.'
    }
    return error.message
  }
  return fallback
}

export function isInteractionRequiredError(error: unknown) {
  return error instanceof Error && error.message.includes('User interaction required')
}

export function isWalletSetupFundingError(error: unknown) {
  if (!(error instanceof Error)) {
    return false
  }

  const message = error.message.toLowerCase()
  return (
    message.includes('adddeployaccounttransaction') ||
    message.includes('deploy_account')
  ) && (
    message.includes('exceed balance') ||
    message.includes('balance (0)') ||
    message.includes('account validation failed')
  )
}

export function loginProgressLabel(step: 'CONNECTED' | 'CHECK_DEPLOYED' | 'DEPLOYING' | 'FAILED' | 'READY') {
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
