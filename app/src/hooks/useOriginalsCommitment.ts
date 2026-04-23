import { useCallback, useEffect, useRef, useState } from 'react'
import type { CreateDiceCommitmentResponse } from '../lib/api'

type CreateCommitment = () => Promise<CreateDiceCommitmentResponse>

type UseOriginalsCommitmentOptions = {
  enabled?: boolean
}

export function useOriginalsCommitment(
  createCommitment: CreateCommitment,
  options: UseOriginalsCommitmentOptions = {},
) {
  const enabled = options.enabled ?? true
  const cachedCommitmentRef = useRef<CreateDiceCommitmentResponse | undefined>(undefined)
  const inFlightRef = useRef<Promise<CreateDiceCommitmentResponse> | null>(null)
  const mountedRef = useRef(true)
  const [readyCommitmentId, setReadyCommitmentId] = useState<number>()

  useEffect(() => {
    return () => {
      mountedRef.current = false
    }
  }, [])

  const warmCommitment = useCallback(async () => {
    if (!enabled) {
      return undefined
    }

    if (cachedCommitmentRef.current) {
      return cachedCommitmentRef.current
    }

    if (inFlightRef.current) {
      return inFlightRef.current
    }

    const task = createCommitment()
      .then((response) => {
        cachedCommitmentRef.current = response
        if (mountedRef.current) {
          setReadyCommitmentId(response.commitment.commitment_id)
        }
        return response
      })
      .finally(() => {
        inFlightRef.current = null
      })

    inFlightRef.current = task
    return task
  }, [createCommitment, enabled])

  const takeCommitment = useCallback(async () => {
    const readyCommitment = cachedCommitmentRef.current ?? await warmCommitment()
    if (!readyCommitment) {
      throw new Error('Server commitment is not ready yet.')
    }

    cachedCommitmentRef.current = undefined
    if (mountedRef.current) {
      setReadyCommitmentId(undefined)
    }

    if (enabled) {
      void warmCommitment()
    }

    return readyCommitment
  }, [enabled, warmCommitment])

  useEffect(() => {
    if (!enabled) {
      cachedCommitmentRef.current = undefined
      if (mountedRef.current) {
        setReadyCommitmentId(undefined)
      }
      return
    }

    void warmCommitment()
  }, [enabled, warmCommitment])

  return {
    commitmentReady: readyCommitmentId !== undefined,
    readyCommitmentId,
    takeCommitment,
    warmCommitment,
  }
}
