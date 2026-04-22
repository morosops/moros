export const STRK_DECIMALS = 18n
export const STRK_UNIT = 10n ** STRK_DECIMALS

type ParseStrkOptions = {
  allowZero?: boolean
  label?: string
  maxDecimals?: number
}

export function parseStrkInputToWei(value: string, options: ParseStrkOptions = {}) {
  const { allowZero = true, label = 'STRK amount', maxDecimals = 4 } = options
  const normalized = value.trim()
  const expression = new RegExp(`^\\d+(\\.\\d{0,${maxDecimals}})?$`)
  if (!expression.test(normalized)) {
    throw new Error(`Enter a ${label} with up to ${maxDecimals} decimal places.`)
  }

  const [whole, fraction = ''] = normalized.split('.')
  const parsed = BigInt(whole) * STRK_UNIT + BigInt(fraction.padEnd(Number(STRK_DECIMALS), '0'))
  if (!allowZero && parsed === 0n) {
    throw new Error(`${label.charAt(0).toUpperCase()}${label.slice(1)} must be greater than zero.`)
  }
  return parsed
}

export function parseStrkInput(value: string, options: ParseStrkOptions = {}) {
  return parseStrkInputToWei(value, options).toString()
}

export function parseOptionalStrkInput(value: string, options: Omit<ParseStrkOptions, 'allowZero'> = {}) {
  if (!value.trim()) {
    return undefined
  }
  return parseStrkInputToWei(value, { ...options, allowZero: true })
}

export function formatStrk(wei?: string | bigint | null, maxFractionDigits = 4) {
  if (wei === undefined || wei === null) {
    return '0 STRK'
  }

  const value = typeof wei === 'bigint' ? wei : BigInt(wei)
  const whole = value / STRK_UNIT
  const fraction = (value % STRK_UNIT)
    .toString()
    .padStart(Number(STRK_DECIMALS), '0')
    .slice(0, maxFractionDigits)
    .replace(/0+$/, '')
  return fraction ? `${whole}.${fraction} STRK` : `${whole} STRK`
}

export function formatWagerInput(wei: bigint, maxFractionDigits = 4) {
  const whole = wei / STRK_UNIT
  const fraction = (wei % STRK_UNIT)
    .toString()
    .padStart(Number(STRK_DECIMALS), '0')
    .slice(0, maxFractionDigits)
    .replace(/0+$/, '')
  return fraction ? `${whole}.${fraction}` : whole.toString()
}

export function formatUsd(value?: number) {
  if (value === undefined || Number.isNaN(value)) {
    return '$0.00'
  }
  return new Intl.NumberFormat('en-US', {
    currency: 'USD',
    maximumFractionDigits: 2,
    minimumFractionDigits: 2,
    style: 'currency',
  }).format(value)
}

export function formatDecimal(value: number, precision = 2) {
  return value.toFixed(precision).replace(/0+$/, '').replace(/\.$/, '')
}

export function formatPercentBps(bps: number) {
  return `${(bps / 100).toFixed(2)}%`
}

export function formatMultiplierBps(multiplierBps: number) {
  return `${(multiplierBps / 10000).toFixed(4).replace(/0+$/, '').replace(/\.$/, '')}x`
}

export function shortAddress(address?: string | null, leading = 6, trailing = 4) {
  if (!address) {
    return undefined
  }
  if (address.length <= leading + trailing + 3) {
    return address
  }
  return `${address.slice(0, leading)}...${address.slice(-trailing)}`
}
