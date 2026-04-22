import { useEffect, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import {
  fetchPlayerProfile,
  fetchPlayerProfileByUsername,
  type PlayerProfile,
} from '../lib/api'
import { shortAddress as formatShortAddress } from '../lib/format'

function shortAddress(address?: string | null) {
  return formatShortAddress(address) ?? 'Moros user'
}

function formatTimestamp(value?: string) {
  if (!value) {
    return 'Unknown'
  }

  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) {
    return value
  }

  return new Intl.DateTimeFormat('en-US', {
    dateStyle: 'medium',
  }).format(parsed)
}

export function PublicProfilePage() {
  const { profileId = '' } = useParams()
  const [profile, setProfile] = useState<PlayerProfile>()
  const [error, setError] = useState<string>()

  useEffect(() => {
    let cancelled = false
    setError(undefined)
    setProfile(undefined)

    const lookup = profileId.startsWith('@')
      ? fetchPlayerProfileByUsername(profileId.slice(1))
      : fetchPlayerProfile(profileId)

    void lookup
      .then((response) => {
        if (!cancelled) {
          setProfile(response)
        }
      })
      .catch((lookupError) => {
        if (!cancelled) {
          setError(lookupError instanceof Error ? lookupError.message : 'Profile not found.')
        }
      })

    return () => {
      cancelled = true
    }
  }, [profileId])

  const displayName = profile?.username ? `@${profile.username}` : shortAddress(profile?.wallet_address)

  return (
    <section className="page page--profile">
      <div className="profile-page">
        <header className="profile-page__header">
          <span className="wallet-funds__section-label">Profile</span>
          <strong>{displayName}</strong>
          <small>{profile?.wallet_address ?? 'No execution wallet linked yet'}</small>
        </header>

        {error ? (
          <div className="wallet-funds__inline-meta">
            <span>{error}</span>
          </div>
        ) : null}

        {profile ? (
          <>
            <div className="profile-page__grid">
              <div className="profile-page__row">
                <span>Username</span>
                <strong>{profile.username ? `@${profile.username}` : 'Using wallet address'}</strong>
              </div>
              <div className="profile-page__row">
                <span>Wallet</span>
                <strong>{profile.wallet_address ?? 'No execution wallet linked yet'}</strong>
              </div>
              <div className="profile-page__row">
                <span>Auth provider</span>
                <strong>{profile.auth_provider}</strong>
              </div>
              <div className="profile-page__row">
                <span>Created</span>
                <strong>{formatTimestamp(profile.created_at)}</strong>
              </div>
            </div>

            <div className="wallet-funds__inline-meta">
              {profile.wallet_address ? (
                <span>Wallet profile URL: /profile/{profile.wallet_address}</span>
              ) : (
                <span>This profile does not expose a wallet address yet.</span>
              )}
              {profile.username ? <span>Username profile URL: /profile/@{profile.username}</span> : null}
            </div>
          </>
        ) : null}

        <div className="profile-page__actions">
          <Link className="button button--ghost" to="/">
            Back to lobby
          </Link>
        </div>
      </div>
    </section>
  )
}
