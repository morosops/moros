import { useEffect, useMemo, useState } from 'react'
import { fetchUsernameAvailability } from '../lib/api'

export type UsernameAvailabilityStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'taken'
  | 'invalid'
  | 'error'

export function normalizeUsernameDraft(value: string) {
  return value.trim().toLowerCase()
}

export function isValidUsernameDraft(value: string) {
  return /^[a-z0-9_]{3,16}$/.test(value)
}

export function useUsernameAvailability(draft: string, currentUsername?: string) {
  const [status, setStatus] = useState<UsernameAvailabilityStatus>('idle')
  const [message, setMessage] = useState<string>()

  const normalizedDraft = useMemo(() => normalizeUsernameDraft(draft), [draft])
  const normalizedCurrent = useMemo(() => normalizeUsernameDraft(currentUsername ?? ''), [currentUsername])

  useEffect(() => {
    if (!normalizedDraft) {
      setStatus('idle')
      setMessage('Leave this blank to use your wallet address publicly.')
      return
    }

    if (!isValidUsernameDraft(normalizedDraft)) {
      setStatus('invalid')
      setMessage('Use 3-16 lowercase letters, digits, or underscores.')
      return
    }

    if (normalizedDraft === normalizedCurrent) {
      setStatus('idle')
      setMessage(`Current public username: @${normalizedDraft}`)
      return
    }

    let cancelled = false
    setStatus('checking')
    setMessage('Checking availability...')

    const timer = window.setTimeout(() => {
      void fetchUsernameAvailability(normalizedDraft)
        .then((result) => {
          if (cancelled) {
            return
          }

          if (result.available) {
            setStatus('available')
            setMessage(`@${normalizedDraft} is available.`)
            return
          }

          setStatus('taken')
          setMessage(`@${normalizedDraft} is already taken.`)
        })
        .catch((error) => {
          if (cancelled) {
            return
          }
          setStatus('error')
          setMessage(error instanceof Error ? error.message : 'Could not verify username availability.')
        })
    }, 220)

    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [normalizedCurrent, normalizedDraft])

  return {
    normalizedDraft,
    status,
    message,
  }
}
