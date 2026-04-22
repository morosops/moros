export function randomClientSeed() {
  const bytes = new Uint32Array(4)
  crypto.getRandomValues(bytes)
  let value = 0n
  for (const byte of bytes) {
    value = (value << 32n) + BigInt(byte)
  }
  return value.toString()
}

export function sameFelt(left?: string, right?: string) {
  return Boolean(left && right && BigInt(left) === BigInt(right))
}
