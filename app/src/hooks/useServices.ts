import { useQueries } from '@tanstack/react-query'
import {
  fetchCoordinatorHealth,
  fetchDepositRouterHealth,
  fetchIndexerHealth,
  fetchRelayerHealth,
} from '../lib/api'

export function useServiceHealth() {
  const results = useQueries({
    queries: [
      { queryKey: ['service', 'coordinator'], queryFn: fetchCoordinatorHealth, retry: 1 },
      { queryKey: ['service', 'relayer'], queryFn: fetchRelayerHealth, retry: 1 },
      { queryKey: ['service', 'indexer'], queryFn: fetchIndexerHealth, retry: 1 },
      { queryKey: ['service', 'deposit-router'], queryFn: fetchDepositRouterHealth, retry: 1 },
    ],
  })

  return {
    coordinator: results[0],
    relayer: results[1],
    indexer: results[2],
    depositRouter: results[3],
  }
}
