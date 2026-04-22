import { useToastStore } from '../store/toast'

export function ToastViewport() {
  const toasts = useToastStore((state) => state.toasts)
  const dismissToast = useToastStore((state) => state.dismissToast)

  return (
    <div aria-live="polite" className="toast-viewport" role="status">
      {toasts.map((toast) => (
        <div className={`toast toast--${toast.tone}`} key={toast.id}>
          <div>
            {toast.title ? <strong>{toast.title}</strong> : null}
            <span>{toast.message}</span>
          </div>
          <button aria-label="Dismiss notification" onClick={() => dismissToast(toast.id)} type="button">
            ×
          </button>
        </div>
      ))}
    </div>
  )
}
