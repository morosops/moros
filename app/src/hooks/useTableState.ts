import { useCallback, useEffect, useState } from 'react'
import {
  fetchCoordinatorTableState,
  type BlackjackTableState,
} from '../lib/api'
import { GAME_TABLE_STATE_POLL_MS } from '../lib/game-rules'

export function tableStateQueryKey(tableId: number, player?: string | null) {
  return ['table-state', tableId, player?.toLowerCase() ?? 'anonymous'] as const
}

export function useTableState(tableId: number, player?: string | null) {
  const [tableState, setTableState] = useState<BlackjackTableState>()
  const [livePlayers, setLivePlayers] = useState<number>()
  const [isLoading, setIsLoading] = useState(false)
  const [lastLoadedAt, setLastLoadedAt] = useState<number>()
  const [error, setError] = useState<unknown>()

  const refreshTableState = useCallback(async (nextPlayer = player ?? undefined) => {
    const response = await fetchCoordinatorTableState(tableId, nextPlayer)
    setTableState(response.state)
    setLivePlayers(response.live_players)
    setLastLoadedAt(Date.now())
    setError(undefined)
    return response
  }, [player, tableId])

  useEffect(() => {
    if (tableId <= 0) {
      setTableState(undefined)
      return
    }

    let cancelled = false
    let interval: number | undefined

    async function load() {
      try {
        setIsLoading(true)
        const response = await fetchCoordinatorTableState(tableId, player ?? undefined)
        if (!cancelled) {
          setTableState(response.state)
          setLivePlayers(response.live_players)
          setLastLoadedAt(Date.now())
          setError(undefined)
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError)
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false)
        }
      }
    }

    void load()
    interval = window.setInterval(() => {
      void load()
    }, GAME_TABLE_STATE_POLL_MS)

    return () => {
      cancelled = true
      if (interval !== undefined) {
        window.clearInterval(interval)
      }
    }
  }, [player, tableId])

  return {
    error,
    isLoading,
    lastLoadedAt,
    livePlayers,
    refreshTableState,
    tableState,
  }
}
