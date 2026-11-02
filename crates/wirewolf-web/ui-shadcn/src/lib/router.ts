// A tiny History API router. The app has a handful of routes, which doesn't justify a router library.

import { useSyncExternalStore } from "react"

export const BASE = import.meta.env.BASE_URL.replace(/\/$/, "") // "" when served from the root

/** History's view, kept in the URL so a view can be linked to and shared. */
export type HistoryQuery = Partial<Record<"range" | "hops" | "focus" | "scale" | "agg" | "from" | "to", string>>

export type Route =
  | { name: "now" }
  | { name: "history"; query?: HistoryQuery }
  | { name: "incidents" }
  | { name: "incident"; id: string }
  | { name: "speed" }
  | { name: "settings" }
  | { name: "logs"; tab?: "server" }
  | { name: "report" }
  | { name: "hop"; id: string; incident?: string } // incident: show that incident's evidence

export function parse(pathname: string, search = ""): Route {
  const p = pathname.startsWith(BASE) ? pathname.slice(BASE.length) : pathname
  const parts = p.split("/").filter(Boolean)
  switch (parts[0]) {
    case "history": {
      const query = Object.fromEntries(new URLSearchParams(search)) as HistoryQuery
      return Object.keys(query).length ? { name: "history", query } : { name: "history" }
    }
    case "incidents":
      if (parts[2] === "path" && parts[3]) return { name: "hop", id: decodeURIComponent(parts[3]), incident: decodeURIComponent(parts[1]) }
      return parts[1] ? { name: "incident", id: decodeURIComponent(parts[1]) } : { name: "incidents" }
    case "path":
      return parts[1] ? { name: "hop", id: decodeURIComponent(parts[1]) } : { name: "now" }
    case "speed":
      return { name: "speed" }
    case "settings":
      return { name: "settings" }
    case "report":
      return { name: "report" }
    case "logs":
      return parts[1] === "server" ? { name: "logs", tab: "server" } : { name: "logs" }
    default:
      return { name: "now" }
  }
}

export function href(route: Route): string {
  switch (route.name) {
    case "now":
      return `${BASE}/`
    case "history": {
      const q = new URLSearchParams(Object.entries(route.query ?? {}).filter((kv): kv is [string, string] => !!kv[1]))
      return `${BASE}/history${q.size ? `?${q}` : ""}`
    }
    case "incident":
      return `${BASE}/incidents/${encodeURIComponent(route.id)}`
    case "hop":
      return `${route.incident ? href({ name: "incident", id: route.incident }) : BASE}/path/${encodeURIComponent(route.id)}`
    case "logs":
      return `${BASE}/logs${route.tab ? `/${route.tab}` : ""}`
    default:
      return `${BASE}/${route.name}`
  }
}

const listeners = new Set<() => void>()
const notify = () => listeners.forEach((l) => l())

/** Asked before every in-app navigation; returning true keeps the user where they are. */
type Blocker = (to: string) => boolean
let blocker: Blocker | null = null
const here = () => location.pathname + location.search

/** The URL being shown, so a blocked back/forward can be undone. */
let shown = typeof location === "undefined" ? "" : here()

/** Lets a page with unsaved work intercept navigation. Pass null to stop. */
export function blockNavigation(b: Blocker | null) {
  blocker = b
}

/**
 * Goes to `to`. `force` skips the blocker, for "discard and leave". `replace` swaps the current
 * entry instead of adding one, for controls that only change the query.
 */
export function navigate(to: string, { force = false, replace = false } = {}) {
  if (to === here()) return
  if (!force && blocker?.(to)) return
  history[replace ? "replaceState" : "pushState"](null, "", to)
  shown = to
  if (!replace) window.scrollTo({ top: 0 })
  notify()
}

// Back and forward have already changed the URL by the time we hear of them, so a blocked one
// puts the shown path back. (No window in unit tests.)
globalThis.window?.addEventListener("popstate", () => {
  const to = here()
  if (to !== shown && blocker?.(to)) {
    history.pushState(null, "", shown)
    return
  }
  shown = to
  notify()
})

export function useRoute(): Route {
  const url = useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => {
        listeners.delete(l)
      }
    },
    () => here(),
  )
  const i = url.indexOf("?")
  return parse(i < 0 ? url : url.slice(0, i), i < 0 ? "" : url.slice(i))
}

/** Left-click navigation without a page load; modified clicks fall through to the browser. */
export function onLinkClick(e: React.MouseEvent<HTMLAnchorElement>) {
  if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return
  e.preventDefault()
  navigate(e.currentTarget.getAttribute("href")!)
}
