// @vitest-environment jsdom

import { MemoryRouter } from 'react-router-dom'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { VipProgressCard } from './VipProgressCard'

describe('VipProgressCard', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders the username, progress, tooltip copy, and next level link target', () => {
    render(
      <MemoryRouter>
        <VipProgressCard
          nextLevel="Bronze"
          progressPercentage={24.41}
          to="/rewards"
          username="meowless21"
        />
      </MemoryRouter>,
    )

    expect(screen.getByText('Your VIP Progress')).toBeTruthy()
    expect(screen.getByRole('link', { name: /meowless21/i }).getAttribute('href')).toBe('/rewards')
    expect(screen.getByText('24.41%')).toBeTruthy()
    expect(screen.getByText('Next level: Bronze')).toBeTruthy()
    expect(screen.getByRole('tooltip').textContent).toContain('Progress is based on wagered amount / activity')
  })
})
