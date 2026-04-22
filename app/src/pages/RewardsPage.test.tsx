// @vitest-environment jsdom

import { MemoryRouter } from 'react-router-dom'
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { RewardsPage } from './RewardsPage'

describe('RewardsPage', () => {
  it('renders the current placeholder state', () => {
    render(
      <MemoryRouter>
        <RewardsPage />
      </MemoryRouter>,
    )

    expect(screen.getByText('Coming Soon')).toBeTruthy()
  })
})
