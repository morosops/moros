type StarkzapModule = typeof import('./starkzap')
type StarkzapFinanceModule = typeof import('./starkzap-finance')

let starkzapModulePromise: Promise<StarkzapModule> | null = null
let starkzapFinanceModulePromise: Promise<StarkzapFinanceModule> | null = null

export async function loadStarkzap() {
  if (!starkzapModulePromise) {
    starkzapModulePromise = import('./starkzap')
  }
  return starkzapModulePromise
}

export async function loadStarkzapFinance() {
  if (!starkzapFinanceModulePromise) {
    starkzapFinanceModulePromise = import('./starkzap-finance')
  }
  return starkzapFinanceModulePromise
}
