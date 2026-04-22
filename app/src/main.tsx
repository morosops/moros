import '@fontsource/space-grotesk/400.css'
import '@fontsource/space-grotesk/500.css'
import '@fontsource/space-grotesk/700.css'
import '@fontsource/ibm-plex-mono/400.css'
import React from 'react'
import ReactDOM from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router-dom'
import { App } from './App'
import { MorosAuthProvider } from './components/MorosAuthProvider'
import './styles.css'

if (
  import.meta.env.VITE_PRIVY_APP_ID &&
  typeof window !== 'undefined' &&
  window.location.hostname === '127.0.0.1'
) {
  const redirectedUrl = new URL(window.location.href)
  redirectedUrl.hostname = 'localhost'
  window.location.replace(redirectedUrl.toString())
}

const queryClient = new QueryClient()

function BootstrapApp() {
  const [ready, setReady] = React.useState(false)

  React.useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      setReady(true)
    })

    return () => window.cancelAnimationFrame(frame)
  }, [])

  if (!ready) {
    return (
      <div className="app-loading-screen" role="status" aria-live="polite">
        <img alt="" className="app-loading-screen__logo" src="/transparent.png" />
      </div>
    )
  }

  return <App />
}

try {
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <MorosAuthProvider>
          <BrowserRouter>
            <BootstrapApp />
          </BrowserRouter>
        </MorosAuthProvider>
      </QueryClientProvider>
    </React.StrictMode>,
  )
} catch (error) {
  const runtime = window as Window & {
    __morosBootstrapErrors?: Array<Record<string, string | number | undefined>>
  }
  runtime.__morosBootstrapErrors ??= []
  runtime.__morosBootstrapErrors.push({
    type: 'render',
    message: error instanceof Error ? error.message : String(error),
  })
  throw error
}
