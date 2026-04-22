import { Suspense, lazy, type ReactNode } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'

const AppShell = lazy(() => import('./components/AppShell').then((module) => ({ default: module.AppShell })))
const BaccaratPage = lazy(() => import('./pages/BaccaratPage').then((module) => ({ default: module.BaccaratPage })))
const BlackjackPage = lazy(() => import('./pages/BlackjackPage').then((module) => ({ default: module.BlackjackPage })))
const DicePage = lazy(() => import('./pages/DicePage').then((module) => ({ default: module.DicePage })))
const LeaderboardPage = lazy(() => import('./pages/LeaderboardPage').then((module) => ({ default: module.LeaderboardPage })))
const LobbyPage = lazy(() => import('./pages/LobbyPage').then((module) => ({ default: module.LobbyPage })))
const PublicProfilePage = lazy(() => import('./pages/PublicProfilePage').then((module) => ({ default: module.PublicProfilePage })))
const RewardsPage = lazy(() => import('./pages/RewardsPage').then((module) => ({ default: module.RewardsPage })))
const RoulettePage = lazy(() => import('./pages/RoulettePage').then((module) => ({ default: module.RoulettePage })))

function RouteFallback() {
  return (
    <div className="app-loading-screen" role="status" aria-live="polite">
      <img alt="" className="app-loading-screen__logo" src="/transparent.png" />
    </div>
  )
}

function withSuspense(node: ReactNode) {
  return <Suspense fallback={<RouteFallback />}>{node}</Suspense>
}

export function App() {
  return (
    <Routes>
      <Route element={withSuspense(<AppShell />)}>
        <Route path="/" element={withSuspense(<LobbyPage />)} />
        <Route path="/leaderboard" element={withSuspense(<LeaderboardPage />)} />
        <Route path="/tables/blackjack" element={withSuspense(<BlackjackPage />)} />
        <Route path="/tables/baccarat" element={withSuspense(<BaccaratPage />)} />
        <Route path="/tables/dice" element={withSuspense(<DicePage />)} />
        <Route path="/tables/roulette" element={withSuspense(<RoulettePage />)} />
        <Route path="/profile/:profileId" element={withSuspense(<PublicProfilePage />)} />
        <Route path="/rewards" element={withSuspense(<RewardsPage />)} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  )
}
