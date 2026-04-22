// @vitest-environment jsdom

import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DepositModal } from './DepositModal'

describe('DepositModal', () => {
  afterEach(() => {
    cleanup()
  })

  it('does not ask a signed-in user to log in again while the Moros account is still provisioning', () => {
    render(
      <DepositModal
        onClose={vi.fn()}
        open
        signedIn
      />,
    )

    expect(
      screen.getByText(
        'Preparing your Moros deposit routes.',
      ),
    ).toBeTruthy()
    expect(screen.queryByText('Log in to generate a deposit route address.')).toBeNull()
  })

  it('shows the login prompt only when the user is actually signed out', () => {
    render(
      <DepositModal
        onClose={vi.fn()}
        open
        signedIn={false}
      />,
    )

    expect(screen.getByText('Log in to generate a deposit route address.')).toBeTruthy()
  })
})
