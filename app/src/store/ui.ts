import { create } from 'zustand'

type UiState = {
  darkModeEnabled: boolean
  lowMotionEnabled: boolean
  settingsDrawerOpen: boolean
  settingsDrawerTab: 'profile' | 'security' | 'preferences'
  selectedTable: string
  sidebarCollapsed: boolean
  setDarkModeEnabled: (enabled: boolean) => void
  setLowMotionEnabled: (enabled: boolean) => void
  openSettingsDrawer: (tab?: 'profile' | 'security' | 'preferences') => void
  closeSettingsDrawer: () => void
  setSettingsDrawerTab: (tab: 'profile' | 'security' | 'preferences') => void
  setSelectedTable: (table: string) => void
  toggleSidebar: () => void
}

const LOW_MOTION_STORAGE_KEY = 'moros-low-motion'

function readDarkModePreference() {
  if (typeof window === 'undefined') {
    return true
  }
  try {
    return window.localStorage.getItem('moros-dark-mode') !== '0'
  } catch {
    return true
  }
}

function writeDarkModePreference(enabled: boolean) {
  if (typeof window === 'undefined') {
    return
  }
  try {
    window.localStorage.setItem('moros-dark-mode', enabled ? '1' : '0')
  } catch {
    // Ignore storage failures in private browsing or constrained runtimes.
  }
}

function readLowMotionPreference() {
  if (typeof window === 'undefined') {
    return false
  }
  try {
    return window.localStorage.getItem(LOW_MOTION_STORAGE_KEY) === '1'
  } catch {
    return false
  }
}

function writeLowMotionPreference(enabled: boolean) {
  if (typeof window === 'undefined') {
    return
  }
  try {
    window.localStorage.setItem(LOW_MOTION_STORAGE_KEY, enabled ? '1' : '0')
  } catch {
    // Ignore storage failures in constrained runtimes.
  }
}

export const useUiStore = create<UiState>((set) => ({
  darkModeEnabled: readDarkModePreference(),
  lowMotionEnabled: readLowMotionPreference(),
  settingsDrawerOpen: false,
  settingsDrawerTab: 'profile',
  selectedTable: 'blackjack-main-floor',
  sidebarCollapsed: true,
  setDarkModeEnabled: (darkModeEnabled) => {
    writeDarkModePreference(darkModeEnabled)
    set({ darkModeEnabled })
  },
  setLowMotionEnabled: (lowMotionEnabled) => {
    writeLowMotionPreference(lowMotionEnabled)
    set({ lowMotionEnabled })
  },
  openSettingsDrawer: (settingsDrawerTab = 'profile') => set({ settingsDrawerOpen: true, settingsDrawerTab }),
  closeSettingsDrawer: () => set({ settingsDrawerOpen: false }),
  setSettingsDrawerTab: (settingsDrawerTab) => set({ settingsDrawerTab }),
  setSelectedTable: (selectedTable) => set({ selectedTable }),
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
}))
