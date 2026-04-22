import type { PlayingCardSuit } from '../components/PlayingCard'

export function rankLabel(card?: number | null) {
  if (!card) {
    return '—'
  }

  if (card === 1) {
    return 'A'
  }

  if (card === 11) {
    return 'J'
  }

  if (card === 12) {
    return 'Q'
  }

  if (card === 13) {
    return 'K'
  }

  return String(card)
}

const displaySuits: PlayingCardSuit[] = ['spades', 'hearts', 'diamonds', 'clubs']

export function displaySuit(rank: string, lane: string, index: number): PlayingCardSuit {
  const seed = `${lane}-${rank}-${index}`
  let total = 0
  for (const char of seed) {
    total += char.charCodeAt(0)
  }
  return displaySuits[total % displaySuits.length]
}

export function cardTilt(index: number, count: number) {
  if (count <= 1) {
    return 0
  }
  const center = (count - 1) / 2
  return (index - center) * 1.6
}

export function seatOutcomeTone(outcome?: string | null) {
  if (outcome === 'win' || outcome === 'blackjack') {
    return 'blackjack-player-seat blackjack-player-seat--win'
  }

  if (outcome === 'push') {
    return 'blackjack-player-seat blackjack-player-seat--push'
  }

  if (outcome) {
    return 'blackjack-player-seat blackjack-player-seat--loss'
  }

  return 'blackjack-player-seat'
}

export function badgeOutcomeTone(outcome?: string | null) {
  if (outcome === 'win' || outcome === 'blackjack') {
    return 'blackjack-total-badge blackjack-total-badge--win'
  }

  if (outcome === 'loss') {
    return 'blackjack-total-badge blackjack-total-badge--loss'
  }

  if (outcome === 'push') {
    return 'blackjack-total-badge blackjack-total-badge--push'
  }

  return 'blackjack-total-badge'
}

export function formatBlackjackTotal(total?: number | null, soft?: boolean | null) {
  if (total === undefined || total === null) {
    return '—'
  }

  if (!soft || total < 12) {
    return String(total)
  }

  return `${total - 10}/${total}`
}

export function actionLabel(action: string) {
  switch (action) {
    case 'hit':
      return 'Hit'
    case 'stand':
      return 'Stand'
    case 'split':
      return 'Split'
    case 'double':
      return 'Double'
    case 'take_insurance':
      return 'Insurance'
    case 'decline_insurance':
      return 'No Insurance'
    default:
      return action
  }
}
