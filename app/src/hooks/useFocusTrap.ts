import { useEffect, type RefObject } from 'react'

const focusableSelector = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

export function useFocusTrap(ref: RefObject<HTMLElement | null>, enabled: boolean, onEscape?: () => void) {
  useEffect(() => {
    if (!enabled) {
      return
    }

    const root = ref.current
    if (!root) {
      return
    }

    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined
    const focusable = Array.from(root.querySelectorAll<HTMLElement>(focusableSelector))
    ;(focusable[0] ?? root).focus()

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onEscape?.()
        return
      }
      if (event.key !== 'Tab') {
        return
      }

      const currentFocusable = Array.from(root.querySelectorAll<HTMLElement>(focusableSelector))
      if (!currentFocusable.length) {
        event.preventDefault()
        root.focus()
        return
      }

      const first = currentFocusable[0]
      const last = currentFocusable[currentFocusable.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }

    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      previousFocus?.focus()
    }
  }, [enabled, onEscape, ref])
}
