import { create } from 'zustand'
import type { DiceRoundView, SettleDiceCommitmentResponse } from '../lib/api'

type GameState = {
  diceHistory: DiceRoundView[]
  diceLastRoll?: SettleDiceCommitmentResponse
  diceRound?: DiceRoundView
  resetDice: () => void
  setDiceResult: (result: SettleDiceCommitmentResponse) => void
}

export const useGameStore = create<GameState>((set) => ({
  diceHistory: [],
  resetDice: () => set({ diceHistory: [], diceLastRoll: undefined, diceRound: undefined }),
  setDiceResult: (result) =>
    set((state) => ({
      diceHistory: [result.round, ...state.diceHistory].slice(0, 12),
      diceLastRoll: result,
      diceRound: result.round,
    })),
}))
