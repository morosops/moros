import { morosConfig } from './config'

export type MorosPrivyWalletLink = {
  wallet_id: string
  wallet_address: string
  public_key?: string
  user_id: string
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
