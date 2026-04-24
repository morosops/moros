import { morosConfig } from './config'

export type MorosPrivyWalletLink = {
  wallet_id: string
  wallet_address: string
  public_key?: string
  user_id: string
}

type MorosPrivyWalletSignatureResponse = {
  signature: string
  wallet_id: string
  wallet_address: string
}

export async function ensureMorosPrivyWallet(idToken: string) {
  const response = await fetch(`${morosConfig.privyBridgeUrl}/v1/auth/privy/starknet-wallet/ensure`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({
      auth_token: idToken,
    }),
  })

  if (!response.ok) {
    throw new Error(await response.text())
  }

  return response.json() as Promise<MorosPrivyWalletLink>
}

export async function signMorosPrivyWalletHash(input: {
  idToken: string
  signingToken?: string
  walletId: string
  hash: `0x${string}`
}) {
  const response = await fetch(`${morosConfig.privyBridgeUrl}/v1/auth/privy/starknet-wallet/sign`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
    },
    body: JSON.stringify({
      auth_token: input.signingToken ?? input.idToken,
      id_token: input.idToken,
      signing_token: input.signingToken ?? input.idToken,
      wallet_id: input.walletId,
      hash: input.hash,
    }),
  })

  if (!response.ok) {
    throw new Error(await response.text())
  }

  const payload = await response.json() as MorosPrivyWalletSignatureResponse
  if (!payload?.signature || typeof payload.signature !== 'string') {
    throw new Error('Privy bridge returned an invalid Starknet signature payload.')
  }

  return payload.signature
}
