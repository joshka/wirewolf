import type { Incident, Severity } from "./api"

/** One local calendar day of the uptime strip. */
export interface Day {
  start: number
  /** Milliseconds wirewolf was watching this day (0 before monitoring began, or in the future). */
  watched: number
  /** Milliseconds inside a Down incident. */
  down: number
  /** Milliseconds inside a Degraded incident. */
  degraded: number
  /** The worst severity seen, or null when nothing went wrong. */
  worst: Severity | null
  incidents: number
}

export interface Uptime {
  days: Day[]
  watched: number
  down: number
  incidents: number
  /** Share of watched time not inside a Down incident, or null with nothing watched. */
  ratio: number | null
}

const overlap = (a0: number, a1: number, b0: number, b1: number) => Math.max(0, Math.min(a1, b1) - Math.max(a0, b0))

/**
 * Uptime over the last `count` local days, oldest first. Time counts as down only inside a Down
 * incident; degraded time is tracked separately and still counts as up. Days before `since` (the
 * oldest measurement) aren't watched, so they read "no data" rather than "no problems".
 */
export function uptime(incidents: Incident[], since: string | null | undefined, now: number, count = 30): Uptime {
  const today = new Date(now)
  today.setHours(0, 0, 0, 0)
  const from = since ? new Date(since).getTime() : now
  const days: Day[] = []
  for (let i = count - 1; i >= 0; i--) {
    const d = new Date(today)
    d.setDate(d.getDate() - i)
    const start = d.getTime()
    d.setDate(d.getDate() + 1)
    const end = d.getTime()
    const day: Day = { start, watched: overlap(start, end, from, now), down: 0, degraded: 0, worst: null, incidents: 0 }
    for (const inc of incidents) {
      const s = new Date(inc.started).getTime()
      const e = inc.ended ? new Date(inc.ended).getTime() : now
      const t = overlap(start, end, s, e)
      if (t === 0 && !(s >= start && s < end)) continue
      day.incidents++
      if (inc.severity === "down") {
        day.down += t
        day.worst = "down"
      } else if (inc.severity === "degraded") {
        day.degraded += t
        if (day.worst !== "down") day.worst = "degraded"
      }
    }
    days.push(day)
  }
  const watched = days.reduce((a, d) => a + d.watched, 0)
  const down = days.reduce((a, d) => a + Math.min(d.down, d.watched), 0)
  const ids = new Set(incidents.filter((i) => days.some((d) => overlap(d.start, d.start + 86400e3, new Date(i.started).getTime(), i.ended ? new Date(i.ended).getTime() : now) > 0)).map((i) => i.id))
  return { days, watched, down, incidents: ids.size, ratio: watched > 0 ? 1 - down / watched : null }
}
