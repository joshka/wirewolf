// Who is signed in and what they may do (`GET /api/v1/session`).

import { useSyncExternalStore } from "react"
import type { Session } from "@/lib/api"

/** Servers without `/session` predate sign-in: everything is allowed. */
const OPEN: Session = { methods: [], user: null, can_write: true, reads_require_login: false, csrf: null, login_url: null, logout_url: null }

let session: Session | null = null
const listeners = new Set<() => void>()

export function currentSession(): Session | null {
  return session
}

export async function loadSession() {
  try {
    const res = await fetch("/api/v1/session")
    session = res.status === 404 ? OPEN : res.ok ? ((await res.json()) as Session) : session
  } catch {
    // Keep what we had; the live indicator already reports the connection.
  }
  listeners.forEach((l) => l())
}

export function useSession(): Session | null {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => session,
  )
}

export function signIn(loginUrl: string) {
  const here = location.pathname + location.search
  location.assign(`${loginUrl}?return_to=${encodeURIComponent(here)}`)
}

export async function signOut(s: Session) {
  if (!s.logout_url) return
  await fetch(s.logout_url, { method: "POST", headers: s.csrf ? { "x-wirewolf-csrf": s.csrf } : undefined })
  await loadSession()
}

/** Why a write is unavailable, phrased for a button hint; `null` if it's allowed. */
export function writeBlocked(s: Session | null, action: string): string | null {
  if (!s || s.can_write) return null
  if (s.login_url) return `Sign in to ${action}`
  if (s.methods.includes("header")) return `Sign in through your proxy to ${action}`
  return `Changes are turned off on this server (see [auth] in wirewolf.toml)`
}
