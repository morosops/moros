import { useMemo, useState } from 'react'
import { formatDecimal } from '../lib/format'

export function clampChance(value: number) {
  return Math.min(9800, Math.max(100, Math.round(value)))
}

export function targetFromChance(chanceBps: number, rollOver: boolean) {
  return rollOver ? 10000 - chanceBps - 1 : chanceBps
}

export function clampThresholdBps(thresholdBps: number, rollOver: boolean) {
  const minimum = rollOver ? 199 : 100
  const maximum = rollOver ? 9899 : 9800
  return Math.min(maximum, Math.max(minimum, thresholdBps))
}

export function chanceFromThresholdBps(thresholdBps: number, rollOver: boolean) {
  const clampedThreshold = clampThresholdBps(thresholdBps, rollOver)
  return clampChance(rollOver ? 9999 - clampedThreshold : clampedThreshold)
}

export function quotedDicePayout(wagerWei: string, multiplierBps: number) {
  return ((BigInt(wagerWei) * BigInt(multiplierBps)) / 10000n).toString()
}

export function useDiceGame(initialChanceBps = 4950, initialRollOver = true) {
  const [chanceBps, setChanceBps] = useState(initialChanceBps)
  const [rollOver, setRollOver] = useState(initialRollOver)
  const targetBps = useMemo(() => targetFromChance(chanceBps, rollOver), [chanceBps, rollOver])
  const multiplierBps = Math.floor((9900 * 10000) / chanceBps)
  const thresholdDisplay = useMemo(() => formatDecimal(targetBps / 100, 2), [targetBps])
  const chanceDisplay = useMemo(() => formatDecimal(chanceBps / 100, 2), [chanceBps])
  const multiplierDisplay = useMemo(() => formatDecimal(multiplierBps / 10000, 4), [multiplierBps])

  function updateThreshold(nextThresholdBps: number) {
    setChanceBps(chanceFromThresholdBps(nextThresholdBps, rollOver))
  }

  function stepMultiplier(delta: number) {
    const current = multiplierBps / 10000
    const next = Math.min(99, Math.max(1.0101, current + delta))
    setChanceBps(clampChance(Math.round(9900 / next)))
  }

  function toggleRollDirection() {
    const nextRollOver = !rollOver
    setRollOver(nextRollOver)
    setChanceBps(chanceFromThresholdBps(targetBps, nextRollOver))
  }

  return {
    chanceBps,
    chanceDisplay,
    multiplierBps,
    multiplierDisplay,
    rollOver,
    setChanceBps,
    setRollOver,
    stepMultiplier,
    targetBps,
    thresholdDisplay,
    toggleRollDirection,
    updateThreshold,
  }
}
