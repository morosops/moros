type StarknetModule = typeof import('starknet')

let starknetModulePromise: Promise<StarknetModule> | null = null

async function loadStarknetHash() {
  if (!starknetModulePromise) {
    starknetModulePromise = import('starknet')
  }

  const module = await starknetModulePromise
  return module.hash
}

export const MOROS_SERVER_SEED_DOMAIN = '0x4d4f524f535f5345525645525f53454544'
export const MOROS_DICE_ROLL_DOMAIN = '0x4d4f524f535f444943455f524f4c4c'
export const MOROS_ROULETTE_SPIN_DOMAIN = '0x4d4f524f535f524f554c455454455f5350494e'
export const MOROS_BACCARAT_SHOE_DOMAIN = '0x4d4f524f535f42414343415241545f53484f45'
export const MOROS_BACCARAT_CARD_DOMAIN = '0x4d4f524f535f4241435f43415244'
export const MOROS_BACCARAT_TRANSCRIPT_DOMAIN = '0x4d4f524f535f4241435f524f4f54'

export async function computePoseidonOnElements(values: [string, ...string[]]) {
  const hash = await loadStarknetHash()
  return hash.computePoseidonHashOnElements(values)
}

export function feltToModulo(value: string, modulo: bigint) {
  return Number(BigInt(value) % modulo)
}
