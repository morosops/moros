type NavIconName =
  | 'api'
  | 'builders'
  | 'card'
  | 'documentation'
  | 'exchange'
  | 'help'
  | 'language'
  | 'leaderboard'
  | 'logout'
  | 'moon'
  | 'profile'
  | 'ranking'
  | 'referral'
  | 'rewards'
  | 'settings'
  | 'support'
  | 'terms'
  | 'transfer'
  | 'vault'

type NavIconProps = {
  name: NavIconName
  className?: string
  variant?: 'stroke' | 'fill'
}

function IconPath({ name }: { name: NavIconName }) {
  switch (name) {
    case 'leaderboard':
      return (
        <>
          <path d="M3.5 12.5h2.5v-4H3.5zM6.75 12.5h2.5v-7h-2.5zM10 12.5h2.5v-9h-2.5z" />
          <path d="M2.5 12.5h11" />
        </>
      )
    case 'rewards':
      return (
        <>
          <path d="m8 2.8 1.55 3.14 3.46.5-2.5 2.43.59 3.43L8 10.68 4.9 12.3l.6-3.43L3 6.44l3.45-.5Z" />
        </>
      )
    case 'api':
      return (
        <>
          <path d="M5.25 4 2.5 8l2.75 4" />
          <path d="M10.75 4 13.5 8l-2.75 4" />
          <path d="M8.9 3 7.1 13" />
        </>
      )
    case 'referral':
      return (
        <>
          <path d="M5 5.25h5.25a2 2 0 0 1 0 4H7.5" />
          <path d="m8 2.75-3 2.5 3 2.5" />
          <path d="M11 10.75H5.75a2 2 0 1 1 0-4H8.5" />
          <path d="m8 13.25 3-2.5-3-2.5" />
        </>
      )
    case 'builders':
      return (
        <>
          <path d="m5.25 9.75-2-2 2-2" />
          <path d="m10.75 5.75 2 2-2 2" />
          <path d="M6.75 12.5h2.5" />
          <path d="M8 3v6.5" />
        </>
      )
    case 'moon':
      return (
        <>
          <path d="M10.7 2.9a4.8 4.8 0 1 0 2.4 8.9 5.3 5.3 0 1 1-2.4-8.9Z" />
        </>
      )
    case 'ranking':
      return (
        <>
          <path d="M2.5 12.5h11" />
          <path d="M3.5 12.5v-5.5h3v5.5zM6.5 12.5V3.5h3v9zM9.5 12.5v-7h3v7z" />
        </>
      )
    case 'support':
      return (
        <>
          <path d="M3.5 8a4.5 4.5 0 1 1 9 0" />
          <path d="M3.5 8v2a1.5 1.5 0 0 0 1.5 1.5h.75" />
          <path d="M12.5 8v2a1.5 1.5 0 0 1-1.5 1.5h-.75" />
          <path d="M6.5 12.5h3" />
        </>
      )
    case 'documentation':
      return (
        <>
          <path d="M4 3.5h6.5a2 2 0 0 1 2 2v7H6a2 2 0 0 0-2 2Z" />
          <path d="M4 3.5v9" />
          <path d="M6.75 6.5h3.5" />
          <path d="M6.75 8.75h3.5" />
        </>
      )
    case 'help':
      return (
        <>
          <circle cx="8" cy="8" r="5.25" />
          <path d="M6.7 6.2a1.45 1.45 0 1 1 2.42 1.16c-.55.48-.87.82-.87 1.64" />
          <path d="M8 10.85h.01" />
        </>
      )
    case 'terms':
      return (
        <>
          <path d="M8 2.75 12 4.25v3.3c0 2.25-1.35 4.1-4 5.7-2.65-1.6-4-3.45-4-5.7v-3.3Z" />
          <path d="m6.2 7.9 1.2 1.2 2.4-2.55" />
        </>
      )
    case 'language':
      return (
        <>
          <circle cx="8" cy="8" r="5.25" />
          <path d="M2.75 8h10.5" />
          <path d="M8 2.75a8.55 8.55 0 0 1 0 10.5" />
          <path d="M8 2.75a8.55 8.55 0 0 0 0 10.5" />
        </>
      )
    case 'logout':
      return (
        <>
          <path d="M6 3.5H4.5a1 1 0 0 0-1 1v7a1 1 0 0 0 1 1H6" />
          <path d="M9 5.25 11.75 8 9 10.75" />
          <path d="M11.5 8H6.25" />
        </>
      )
    case 'profile':
      return (
        <>
          <circle cx="8" cy="5.6" r="2.1" />
          <path d="M4.3 12.5a3.7 3.7 0 0 1 7.4 0" />
        </>
      )
    case 'settings':
      return (
        <>
          <circle cx="8" cy="8" r="1.75" />
          <path d="M8 2.75v1.3M8 11.95v1.3M12.25 8h1.3M2.45 8h1.3M11.02 4.98l.92-.92M4.06 11.94l.92-.92M11.02 11.02l.92.92M4.06 4.06l.92.92" />
        </>
      )
    case 'transfer':
      return (
        <>
          <path d="M2.75 5.5h7.75" />
          <path d="m8.5 3 2.5 2.5L8.5 8" />
          <path d="M13.25 10.5H5.5" />
          <path d="m7.75 13-2.5-2.5L7.75 8" />
        </>
      )
    case 'exchange':
      return (
        <>
          <path d="M4 4.5h6.25a1.75 1.75 0 0 1 0 3.5H4" />
          <path d="m8.75 2.75 2 1.75-2 1.75" />
          <path d="M12 11.5H5.75a1.75 1.75 0 0 1 0-3.5H12" />
          <path d="m7.25 13.25-2-1.75 2-1.75" />
        </>
      )
    case 'card':
      return (
        <>
          <rect x="2.75" y="4" width="10.5" height="8" rx="1.4" />
          <path d="M2.75 6.6h10.5" />
          <path d="M5 10h2" />
        </>
      )
    case 'vault':
      return (
        <>
          <path d="M3.25 5.1c0-1.3 1.05-2.35 2.35-2.35h4.8c1.3 0 2.35 1.05 2.35 2.35v5.8c0 1.3-1.05 2.35-2.35 2.35H5.6c-1.3 0-2.35-1.05-2.35-2.35Z" />
          <path d="M6 7.25h4" />
          <path d="M8 5.8v2.9" />
        </>
      )
    default:
      return null
  }
}

function FilledIconPath({ name }: { name: NavIconName }) {
  switch (name) {
    case 'leaderboard':
    case 'ranking':
      return (
        <>
          <rect x="2.75" y="11.5" width="10.5" height="1.25" rx="0.6" />
          <rect x="3.25" y="7.5" width="2.35" height="4" rx="0.55" />
          <rect x="6.8" y="4.75" width="2.35" height="6.75" rx="0.55" />
          <rect x="10.35" y="2.75" width="2.35" height="8.75" rx="0.55" />
        </>
      )
    case 'referral':
      return (
        <>
          <path d="M4.1 4.2h4.25a1.95 1.95 0 0 1 0 3.9H5.25V6.85h3.1a.7.7 0 0 0 0-1.4H4.1Z" />
          <path d="m5.25 2.75-2.4 2.15 2.4 2.15z" />
          <path d="M11.9 11.8H7.65a1.95 1.95 0 0 1 0-3.9h3.1v1.25h-3.1a.7.7 0 0 0 0 1.4h4.25Z" />
          <path d="m10.75 13.25 2.4-2.15-2.4-2.15z" />
        </>
      )
    case 'support':
      return (
        <>
          <path d="M8 2.3a5.6 5.6 0 0 0-5.6 5.6v1.4c0 .83.67 1.5 1.5 1.5h1.35v-2.6H3.65v-.3a4.35 4.35 0 1 1 8.7 0v.3h-1.6v2.6h1.35c.83 0 1.5-.67 1.5-1.5V7.9A5.6 5.6 0 0 0 8 2.3Z" />
          <rect x="6.05" y="11.2" width="3.9" height="1.45" rx="0.72" />
        </>
      )
    case 'logout':
      return (
        <>
          <path d="M3.1 3.1h3.2v1.3H4.4v7.2h1.9v1.3H3.1a.95.95 0 0 1-.95-.95V4.05c0-.52.43-.95.95-.95Z" />
          <path d="M8.15 4.45 12.2 8l-4.05 3.55V9.35H5.7v-2.7h2.45Z" />
        </>
      )
    case 'profile':
      return (
        <>
          <circle cx="8" cy="5.35" r="2.15" />
          <path d="M8 8.45c-2.33 0-4.2 1.5-4.2 3.35 0 .4.32.72.72.72h7a.72.72 0 0 0 .72-.72c0-1.85-1.87-3.35-4.24-3.35Z" />
        </>
      )
    case 'vault':
      return (
        <>
          <path d="M3 5.1c0-1.44 1.16-2.6 2.6-2.6h4.8c1.44 0 2.6 1.16 2.6 2.6v5.8c0 1.44-1.16 2.6-2.6 2.6H5.6A2.6 2.6 0 0 1 3 10.9Z" />
          <rect x="4.7" y="6.35" width="6.6" height="1.25" rx="0.62" />
          <rect x="7.37" y="4.85" width="1.26" height="4.2" rx="0.63" />
        </>
      )
    default:
      return <IconPath name={name} />
  }
}

export function NavIcon({ name, className, variant = 'stroke' }: NavIconProps) {
  if (variant === 'fill') {
    return (
      <svg
        aria-hidden="true"
        className={className}
        fill="currentColor"
        viewBox="0 0 16 16"
        xmlns="http://www.w3.org/2000/svg"
      >
        <FilledIconPath name={name} />
      </svg>
    )
  }

  return (
    <svg
      aria-hidden="true"
      className={className}
      fill="none"
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
    >
      <g stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.35">
        <IconPath name={name} />
      </g>
    </svg>
  )
}

export type { NavIconName }
