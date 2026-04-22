import type { MorosAuthRuntime } from '../components/moros-auth-context'

function decodeJwtPayload(token: string) {
  const segments = token.split('.')
  if (segments.length < 2 || !segments[1]) {
    return undefined
  }

  try {
    const normalized = segments[1].replace(/-/g, '+').replace(/_/g, '/')
    const padded = normalized.padEnd(normalized.length + ((4 - (normalized.length % 4 || 4)) % 4), '=')
    if (typeof globalThis.atob !== 'function') {
      return undefined
    }
    const json = globalThis.atob(padded)
    return JSON.parse(json) as Record<string, unknown>
  } catch {
    return undefined
  }
}

export async function resolvePrivyRequestToken(auth: MorosAuthRuntime) {
  if (!auth.enabled || !auth.authenticated) {
    return undefined
  }

  if (auth.identityToken) {
    return auth.identityToken
  }

  if (auth.accessToken) {
    return auth.accessToken
  }

  if (auth.enabled && !auth.loaded) {
    await auth.ensureLoaded().catch(() => undefined)
  }

  if (!auth.ready) {
    return undefined
  }

  try {
    const identityToken = await auth.getIdentityToken()
    if (identityToken) {
      return identityToken
    }
  } catch {
    // fall through to access-token fallback
  }

  try {
    return (await auth.getAccessToken()) ?? undefined
  } catch {
    return undefined
  }
}

export async function resolvePrivyAccessToken(auth: MorosAuthRuntime) {
  if (!auth.enabled || !auth.authenticated) {
    return undefined
  }

  if (auth.accessToken) {
    return auth.accessToken
  }

  if (auth.enabled && !auth.loaded) {
    await auth.ensureLoaded().catch(() => undefined)
  }

  if (!auth.ready) {
    return undefined
  }

  try {
    return (await auth.getAccessToken()) ?? undefined
  } catch {
    return undefined
  }
}

function delay(ms: number) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}

export async function waitForPrivyRequestToken(
  auth: MorosAuthRuntime,
  options?: {
    attempts?: number
    delayMs?: number
  },
) {
  const attempts = Math.max(1, options?.attempts ?? 12)
  const delayMs = Math.max(0, options?.delayMs ?? 150)

  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const token = await resolvePrivyRequestToken(auth)
    if (token) {
      return token
    }

    if (attempt < attempts - 1 && delayMs > 0) {
      await delay(delayMs)
    }
  }

  return undefined
}

export async function waitForPrivyAccessToken(
  auth: MorosAuthRuntime,
  options?: {
    attempts?: number
    delayMs?: number
  },
) {
  const attempts = Math.max(1, options?.attempts ?? 12)
  const delayMs = Math.max(0, options?.delayMs ?? 150)

  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const token = await resolvePrivyAccessToken(auth)
    if (token) {
      return token
    }

    if (attempt < attempts - 1 && delayMs > 0) {
      await delay(delayMs)
    }
  }

  return undefined
}

export async function resolvePrivyAuthSubject(auth: MorosAuthRuntime) {
  if (auth.userId) {
    return auth.userId
  }

  const token = await resolvePrivyRequestToken(auth)
  if (!token) {
    return undefined
  }

  const payload = decodeJwtPayload(token)
  const subject = payload?.sub
  return typeof subject === 'string' && subject.trim() ? subject.trim() : undefined
}
