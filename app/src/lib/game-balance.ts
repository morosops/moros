import { fetchAccountBalancesByWalletAddress } from './api'

function parseBalanceWei(value?: string | null) {
  if (!value) {
    return 0n
  }

  try {
    return BigInt(value)
  } catch {
    return 0n
  }
}

export async function resolveEffectiveMorosBalanceWei(
  walletAddress?: string | null,
  tableBalanceWei?: string | null,
) {
  const liveTableBalance = parseBalanceWei(tableBalanceWei)
  if (!walletAddress) {
    return liveTableBalance.toString()
  }

  try {
    const account = await fetchAccountBalancesByWalletAddress(walletAddress)
    const accountBalance = parseBalanceWei(account.gambling_balance)
    return (accountBalance > liveTableBalance ? accountBalance : liveTableBalance).toString()
  } catch {
    return liveTableBalance.toString()
  }
}
