type PlayingCardSuit = 'spades' | 'hearts' | 'diamonds' | 'clubs'

type PlayingCardProps = {
  rank?: string
  suit?: PlayingCardSuit
  faceDown?: boolean
  placeholder?: boolean
  tilt?: number
  className?: string
  ariaLabel?: string
}

const suitGlyphs: Record<PlayingCardSuit, string> = {
  spades: '♠',
  hearts: '♥',
  diamonds: '♦',
  clubs: '♣',
}

function cx(...parts: Array<string | false | null | undefined>) {
  return parts.filter(Boolean).join(' ')
}

export function PlayingCard({
  rank,
  suit = 'spades',
  faceDown = false,
  placeholder = false,
  tilt = 0,
  className,
  ariaLabel,
}: PlayingCardProps) {
  const style = {
    transform: `rotate(${tilt}deg)`,
  }

  if (placeholder) {
    return (
      <div
        aria-label={ariaLabel ?? 'Empty card slot'}
        className={cx('casino-playing-card', 'casino-playing-card--placeholder', className)}
        role="img"
        style={style}
      />
    )
  }

  if (faceDown) {
    return (
      <div
        aria-label={ariaLabel ?? 'Face-down card'}
        className={cx('casino-playing-card', 'casino-playing-card--back', className)}
        role="img"
        style={style}
      >
        <div className="casino-playing-card__back-pattern" />
        <img alt="" className="casino-playing-card__back-logo" src="/transparent.jpg?v=2" />
        <div className="casino-playing-card__back-core" />
      </div>
    )
  }

  const glyph = suitGlyphs[suit]
  const suitTone = suit === 'hearts' || suit === 'diamonds' ? 'casino-playing-card--red' : 'casino-playing-card--black'

  return (
    <div
      aria-label={ariaLabel ?? `${rank ?? 'Unknown'} of ${suit}`}
      className={cx('casino-playing-card', suitTone, className)}
      role="img"
      style={style}
    >
      <div className="casino-playing-card__corner">
        <span className="casino-playing-card__rank">{rank}</span>
        <span className="casino-playing-card__suit">{glyph}</span>
      </div>
      <div className="casino-playing-card__center">{glyph}</div>
      <div className="casino-playing-card__corner casino-playing-card__corner--bottom">
        <span className="casino-playing-card__rank">{rank}</span>
        <span className="casino-playing-card__suit">{glyph}</span>
      </div>
    </div>
  )
}

export type { PlayingCardSuit }
