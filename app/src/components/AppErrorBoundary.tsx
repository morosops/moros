import { Component, type PropsWithChildren, type ReactNode } from 'react'

type AppErrorBoundaryProps = PropsWithChildren<{
  resetKey: string
}>

type AppErrorBoundaryState = {
  error?: string
}

export class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = {}

  static getDerivedStateFromError(error: unknown): AppErrorBoundaryState {
    return {
      error: error instanceof Error ? error.message : 'Moros hit an unexpected client error.',
    }
  }

  componentDidUpdate(previousProps: AppErrorBoundaryProps) {
    if (previousProps.resetKey !== this.props.resetKey && this.state.error) {
      this.setState({ error: undefined })
    }
  }

  componentDidCatch(error: unknown) {
    const runtime = window as Window & {
      __morosRuntimeErrors?: Array<{ message: string; timestamp: number }>
    }
    runtime.__morosRuntimeErrors ??= []
    runtime.__morosRuntimeErrors.push({
      message: error instanceof Error ? error.message : String(error),
      timestamp: Date.now(),
    })
  }

  render(): ReactNode {
    if (!this.state.error) {
      return this.props.children
    }

    return (
      <section className="empty-state" role="alert">
        <h1>Moros could not render this view.</h1>
        <p>{this.state.error}</p>
        <button className="button button--primary" onClick={() => this.setState({ error: undefined })} type="button">
          Retry
        </button>
      </section>
    )
  }
}
