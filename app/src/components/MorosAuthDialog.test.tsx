// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { MorosAuthDialog } from './MorosAuthDialog'

vi.mock('../lib/api', () => ({
  fetchUsernameAvailability: vi.fn().mockResolvedValue({
    available: true,
    username: 'tanya',
  }),
}))

const baseProps = {
  address: undefined,
  authReady: true,
  emailState: 'idle' as const,
  error: undefined,
  mode: 'login' as const,
  needsUsername: false,
  oauthLoading: false,
  onClose: vi.fn(),
  onGoogleLogin: vi.fn().mockResolvedValue(undefined),
  onOpenWallets: vi.fn().mockResolvedValue(undefined),
  onSaveProfile: vi.fn().mockResolvedValue(undefined),
  onSendEmailCode: vi.fn().mockResolvedValue(undefined),
  onVerifyEmailCode: vi.fn().mockResolvedValue(undefined),
  onWalletLogin: vi.fn().mockResolvedValue(undefined),
  open: true,
  pendingLabel: undefined,
  privyEnabled: true,
  username: undefined,
  walletOptions: [],
  walletsLoading: false,
}

describe('MorosAuthDialog', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders the wallet-first login flow', () => {
    render(<MorosAuthDialog {...baseProps} />)

    expect(screen.getByRole('heading', { name: 'Log in' })).toBeTruthy()
    expect(screen.getByRole('button', { name: /Continue with Google/i })).toBeTruthy()
    expect(screen.getByText(/Continue with Email/i)).toBeTruthy()
    expect(screen.getByPlaceholderText('name@example.com')).toBeTruthy()
    expect(screen.getByRole('button', { name: /Continue with Wallet/i })).toBeTruthy()
    expect(screen.queryByRole('heading', { name: 'Continue with Wallet' })).toBeNull()
  }, 15000)

  it('allows Google login to lazy-load auth when the runtime is not ready yet', async () => {
    const onGoogleLogin = vi.fn().mockResolvedValue(undefined)

    render(
      <MorosAuthDialog
        {...baseProps}
        authReady={false}
        onGoogleLogin={onGoogleLogin}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: /Continue with Google/i }))

    await waitFor(() => expect(onGoogleLogin).toHaveBeenCalled())
  }, 15000)

  it('opens the detected wallet list after wallet discovery', async () => {
    const onOpenWallets = vi.fn().mockResolvedValue(undefined)

    render(
      <MorosAuthDialog
        {...baseProps}
        onOpenWallets={onOpenWallets}
        walletOptions={[
          {
            id: 'braavos',
            icon: '/icons/braavos.svg',
            name: 'Braavos',
            provider: {} as never,
          },
        ]}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: /Continue with Wallet/i }))

    await waitFor(() => expect(onOpenWallets).toHaveBeenCalled())
    expect(screen.getByRole('heading', { name: 'Continue with Wallet' })).toBeTruthy()
    expect(screen.getByRole('button', { name: /Braavos/i })).toBeTruthy()
  }, 15000)

  it('submits username save from the username step', async () => {
    const onSaveProfile = vi.fn().mockResolvedValue(undefined)

    render(
      <MorosAuthDialog
        {...baseProps}
        address="0x1234567890abcdef"
        needsUsername
        onSaveProfile={onSaveProfile}
      />,
    )

    fireEvent.change(screen.getByPlaceholderText('0x1234...cdef'), {
      target: { value: 'tanya' },
    })
    await waitFor(() => expect(screen.getByText('tanya is available.')).toBeTruthy())
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))

    await waitFor(() => expect(onSaveProfile).toHaveBeenCalledWith('tanya'))
  }, 15000)

  it('continues with the wallet address when username is left empty', async () => {
    const onSaveProfile = vi.fn().mockResolvedValue(undefined)

    render(
      <MorosAuthDialog
        {...baseProps}
        address="0x1234567890abcdef"
        needsUsername
        onSaveProfile={onSaveProfile}
      />,
    )

    expect(screen.queryByText(/Your wallet address works by default/i)).toBeNull()
    expect(screen.queryByText(/Optional. If you skip this/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))

    await waitFor(() => expect(onSaveProfile).toHaveBeenCalledWith(undefined))
  }, 15000)
})
