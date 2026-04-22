import { create } from 'zustand'

type ProfileState = {
  gameAudioVolume: number
  leaderboardPrivacyEnabled: boolean
  uiAudioVolume: number
  username?: string
  usernameDraft: string
  authProvider?: string
  setProfile: (profile: { username?: string | null; authProvider?: string }) => void
  setGameAudioVolume: (value: number) => void
  setLeaderboardPrivacyEnabled: (enabled: boolean) => void
  setUiAudioVolume: (value: number) => void
  setUsernameDraft: (value: string) => void
  resetUsernameDraft: () => void
  clearProfile: () => void
}

const GAME_AUDIO_VOLUME_STORAGE_KEY = 'moros.game-audio-volume'
const UI_AUDIO_VOLUME_STORAGE_KEY = 'moros.ui-audio-volume'
const LEADERBOARD_PRIVACY_STORAGE_KEY = 'moros.leaderboard-privacy'

function readStoredNumber(key: string, fallback: number) {
  if (typeof window === 'undefined') {
    return fallback
  }

  try {
    const raw = window.localStorage.getItem(key)
    if (!raw) {
      return fallback
    }
    const parsed = Number.parseFloat(raw)
    return Number.isFinite(parsed) ? Math.min(1, Math.max(0, parsed)) : fallback
  } catch {
    return fallback
  }
}

function writeStoredNumber(key: string, value: number) {
  if (typeof window === 'undefined') {
    return
  }

  try {
    window.localStorage.setItem(key, String(Math.min(1, Math.max(0, value))))
  } catch {
    // Ignore storage failures in constrained runtimes.
  }
}

function readStoredBoolean(key: string, fallback = false) {
  if (typeof window === 'undefined') {
    return fallback
  }

  try {
    const raw = window.localStorage.getItem(key)
    if (raw === null) {
      return fallback
    }
    return raw === '1'
  } catch {
    return fallback
  }
}

function writeStoredBoolean(key: string, value: boolean) {
  if (typeof window === 'undefined') {
    return
  }

  try {
    window.localStorage.setItem(key, value ? '1' : '0')
  } catch {
    // Ignore storage failures in constrained runtimes.
  }
}

export const useProfileStore = create<ProfileState>((set) => ({
  gameAudioVolume: readStoredNumber(GAME_AUDIO_VOLUME_STORAGE_KEY, 0.82),
  leaderboardPrivacyEnabled: readStoredBoolean(LEADERBOARD_PRIVACY_STORAGE_KEY, false),
  uiAudioVolume: readStoredNumber(UI_AUDIO_VOLUME_STORAGE_KEY, 0.6),
  username: undefined,
  usernameDraft: '',
  authProvider: undefined,
  setProfile: ({ username, authProvider }) =>
    set(() => ({
      username: username ?? undefined,
      usernameDraft: username ?? '',
      authProvider,
    })),
  setGameAudioVolume: (gameAudioVolume) => {
    writeStoredNumber(GAME_AUDIO_VOLUME_STORAGE_KEY, gameAudioVolume)
    set({ gameAudioVolume: Math.min(1, Math.max(0, gameAudioVolume)) })
  },
  setLeaderboardPrivacyEnabled: (leaderboardPrivacyEnabled) => {
    writeStoredBoolean(LEADERBOARD_PRIVACY_STORAGE_KEY, leaderboardPrivacyEnabled)
    set({ leaderboardPrivacyEnabled })
  },
  setUiAudioVolume: (uiAudioVolume) => {
    writeStoredNumber(UI_AUDIO_VOLUME_STORAGE_KEY, uiAudioVolume)
    set({ uiAudioVolume: Math.min(1, Math.max(0, uiAudioVolume)) })
  },
  setUsernameDraft: (usernameDraft) => set({ usernameDraft }),
  resetUsernameDraft: () =>
    set((state) => ({
      usernameDraft: state.username ?? '',
    })),
  clearProfile: () => set({ username: undefined, usernameDraft: '', authProvider: undefined }),
}))
