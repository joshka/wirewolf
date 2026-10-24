import type { Severity, Status } from "./api"

const nf = (digits: number) =>
  new Intl.NumberFormat(undefined, { maximumFractionDigits: digits, minimumFractionDigits: digits })

/** Milliseconds with precision that scales: `0.42 ms`, `1.4 ms`, `38 ms`. */
export function ms(v: number | null | undefined): string {
  if (v == null) return "—"
  const d = v < 1 ? 2 : v < 10 ? 1 : 0
  return `${nf(d).format(v)} ms`
}

export function pct(v: number | null | undefined): string {
  if (v == null) return "—"
  if (v === 0) return "0%"
  return `${nf(v < 10 ? 1 : 0).format(v)}%`
}

export function mbps(v: number | null | undefined): string {
  if (v == null) return "—"
  return `${nf(v < 10 ? 1 : 0).format(v)} Mbps`
}

export function bytes(v: number | null | undefined): string {
  if (v == null) return "—"
  const units = ["B", "KB", "MB", "GB"]
  let i = 0
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000
    i++
  }
  return `${nf(i === 0 ? 0 : 1).format(v)} ${units[i]}`
}

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto", style: "short" })

/** Localized "3 min ago" up to 48 hours, then a short date. */
export function relative(at: string | Date, now = Date.now()): string {
  const t = new Date(at).getTime()
  const secs = Math.round((t - now) / 1000)
  const a = Math.abs(secs)
  if (a < 60) return rtf.format(a < 10 ? 0 : secs, "second")
  if (a < 3600) return rtf.format(Math.round(secs / 60), "minute")
  if (a < 48 * 3600) return rtf.format(Math.round(secs / 3600), "hour")
  return new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" })
}

export function absolute(at: string | Date): string {
  return new Date(at).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "medium",
  })
}

export function clock(at: string | Date | number): string {
  return new Date(at).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })
}

/** "4 min", "1 h 12 min", "2 d 3 h". */
export function duration(fromIso: string, toIso: string | null, now = Date.now()): string {
  const end = toIso ? new Date(toIso).getTime() : now
  let s = Math.max(0, Math.round((end - new Date(fromIso).getTime()) / 1000))
  if (s < 60) return `${s} s`
  let m = Math.floor(s / 60)
  if (m < 60) return `${m} min`
  const h = Math.floor(m / 60)
  m %= 60
  if (h < 24) return m ? `${h} h ${m} min` : `${h} h`
  const d = Math.floor(h / 24)
  s = h % 24
  return s ? `${d} d ${s} h` : `${d} d`
}

export const statusLabel: Record<Status, string> = {
  pass: "OK",
  warn: "Warning",
  fail: "Failing",
  skip: "Skipped",
  not_applicable: "N/A",
  pending: "Pending",
}

export const severityLabel: Record<Severity, string> = {
  healthy: "Healthy",
  degraded: "Degraded",
  down: "Down",
  unknown: "Unknown",
}
