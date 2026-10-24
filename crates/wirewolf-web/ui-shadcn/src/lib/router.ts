// A tiny History API router. The app has five routes, which doesn't justify a router library.

import { useSyncExternalStore } from "react"

export const BASE = import.meta.env.BASE_URL.replace(/\/$/, "") // "/ui/shadcn"

export type Route =
  | { name: "now" }
  | { name: "history" }
  | { name: "incidents" }
  | { name: "incident"; id: string }
  | { name: "speed" }

export function parse(pathname: string): Route {
  const p = pathname.startsWith(BASE) ? pathname.slice(BASE.length) : pathname
  const parts = p.split("/").filter(Boolean)
  switch (parts[0]) {
    case "history":
      return { name: "history" }
    case "incidents":
      return parts[1] ? { name: "incident", id: decodeURIComponent(parts[1]) } : { name: "incidents" }
    case "speed":
      return { name: "speed" }
    default:
      return { name: "now" }
  }
}

export function href(route: Route): string {
  switch (route.name) {
    case "now":
      return `${BASE}/`
    case "incident":
      return `${BASE}/incidents/${encodeURIComponent(route.id)}`
    default:
      return `${BASE}/${route.name}`
  }
}

const listeners = new Set<() => void>()
window.addEventListener("popstate", () => listeners.forEach((l) => l()))

export function navigate(to: string) {
  if (to === location.pathname) return
  history.pushState(null, "", to)
  window.scrollTo({ top: 0 })
  listeners.forEach((l) => l())
}

export function useRoute(): Route {
  const path = useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => location.pathname,
  )
  return parse(path)
}

/** Left-click navigation without a page load; modified clicks fall through to the browser. */
export function onLinkClick(e: React.MouseEvent<HTMLAnchorElement>) {
  if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return
  e.preventDefault()
  navigate(e.currentTarget.getAttribute("href")!)
}
