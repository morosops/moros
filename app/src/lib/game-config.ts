import baccaratCard from '../assets/cards/baccarat.jpg'
import blackjackCard from '../assets/cards/blackjack.jpg'
import diceCard from '../assets/cards/dice.jpg'
import rouletteCard from '../assets/cards/roulette.jpg'
import { morosConfig } from './config'
export {
  BACCARAT_MAX_SHOE_DRAW_ATTEMPTS,
  FAIRNESS_ARTIFACT_POLL_MS,
  GAME_BALANCE_POLL_MS,
  GAME_TABLE_STATE_POLL_MS,
  ROULETTE_MAX_BET_SPOTS,
} from './game-rules'

export type MorosGameSlug = 'dice' | 'blackjack' | 'roulette' | 'baccarat'

export type MorosGameDefinition = {
  slug: MorosGameSlug
  title: string
  route: string
  navIcon: string
  tableId: number
  tag: string
  subtitle: string
  image: string
}

export const morosGames: MorosGameDefinition[] = [
  {
    slug: 'dice',
    title: 'Dice',
    route: '/tables/dice',
    navIcon: '/icons/dice.svg',
    tableId: morosConfig.diceTableId,
    tag: 'Instant STRK',
    subtitle: 'Commit-reveal over / under',
    image: diceCard,
  },
  {
    slug: 'blackjack',
    title: 'Blackjack',
    route: '/tables/blackjack',
    navIcon: '/icons/blackjack.svg',
    tableId: morosConfig.blackjackTableId,
    tag: 'Moros table',
    subtitle: 'Hybrid hidden-deck runtime',
    image: blackjackCard,
  },
  {
    slug: 'roulette',
    title: 'Roulette',
    route: '/tables/roulette',
    navIcon: '/icons/roulette.svg',
    tableId: morosConfig.rouletteTableId,
    tag: 'European 0',
    subtitle: 'Single-zero wheel and outside bets',
    image: rouletteCard,
  },
  {
    slug: 'baccarat',
    title: 'Baccarat',
    route: '/tables/baccarat',
    navIcon: '/icons/baccarat.svg',
    tableId: morosConfig.baccaratTableId,
    tag: 'Player / Banker',
    subtitle: 'Commit-reveal dealing',
    image: baccaratCard,
  },
]

export function morosGameBySlug(slug: MorosGameSlug) {
  return morosGames.find((game) => game.slug === slug)
}
