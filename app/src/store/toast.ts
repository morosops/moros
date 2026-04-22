import { create } from 'zustand'

export type ToastTone = 'neutral' | 'success' | 'error' | 'warn'

export type Toast = {
  id: string
  message: string
  tone: ToastTone
  title?: string
}

type ToastInput = Omit<Toast, 'id' | 'tone'> & {
  id?: string
  tone?: ToastTone
}

type ToastState = {
  toasts: Toast[]
  dismissToast: (id: string) => void
  pushToast: (toast: ToastInput) => string
}

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  dismissToast: (id) => set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) })),
  pushToast: (toast) => {
    const id = toast.id ?? crypto.randomUUID()
    set((state) => ({
      toasts: [
        ...state.toasts.filter((current) => current.id !== id),
        {
          id,
          message: toast.message,
          title: toast.title,
          tone: toast.tone ?? 'neutral',
        },
      ].slice(-4),
    }))

    window.setTimeout(() => {
      set((state) => ({ toasts: state.toasts.filter((current) => current.id !== id) }))
    }, 4200)
    return id
  },
}))
