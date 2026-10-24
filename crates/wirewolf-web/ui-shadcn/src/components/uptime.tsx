import type { Incident } from "@/lib/api"
import { duration } from "@/lib/format"
import { uptime, type Day } from "@/lib/uptime"
import { cn } from "@/lib/utils"

const DAY_MS = 86400e3

/** "14 min" from milliseconds, via the shared duration format. */
const span = (ms: number) => duration(new Date(0).toISOString(), new Date(ms).toISOString())

const dayLabel = (d: Day) => new Date(d.start).toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })

function describe(d: Day): string {
  if (d.watched === 0) return `${dayLabel(d)}: no data`
  if (d.incidents === 0) return `${dayLabel(d)}: no incidents`
  const parts = [d.down > 0 && `${span(d.down)} down`, d.degraded > 0 && `${span(d.degraded)} degraded`].filter(Boolean)
  return `${dayLabel(d)}: ${parts.join(", ") || "brief problems"} · ${d.incidents} incident${d.incidents === 1 ? "" : "s"}`
}

/**
 * The last 30 days as a strip of bars, like a status page: red for a day with downtime, amber
 * for degraded only, green otherwise, and muted where wirewolf wasn't watching yet. Above it,
 * uptime over the watched time.
 */
export function UptimeStrip({ incidents, since, now, days = 30 }: { incidents: Incident[]; since?: string | null; now: number; days?: number }) {
  const u = uptime(incidents, since, now, days)
  const pct = u.ratio == null ? null : u.ratio * 100
  const summary = [
    pct == null ? "No data yet" : `${pct >= 99.95 ? pct.toFixed(2) : pct.toFixed(1)}% up`,
    `${u.incidents} incident${u.incidents === 1 ? "" : "s"}`,
    u.down > 0 && `${span(u.down)} down`,
    u.watched < days * DAY_MS && since && `watching since ${new Date(since).toLocaleDateString(undefined, { month: "short", day: "numeric" })}`,
  ].filter(Boolean)
  return (
    <section aria-label={`Last ${days} days`} className="flex flex-col gap-2">
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h2 className="text-sm font-medium">Last {days} days</h2>
        <p className="text-sm text-muted-foreground tabular-nums">{summary.join(" · ")}</p>
      </div>
      <ol className="flex h-8 gap-0.5">
        {u.days.map((d) => (
          <li
            key={d.start}
            title={describe(d)}
            aria-label={describe(d)}
            className={cn(
              "flex-1 rounded-sm",
              d.watched === 0 ? "bg-muted" : d.worst === "down" ? "bg-fail" : d.worst === "degraded" ? "bg-warn" : "bg-pass/60",
            )}
          />
        ))}
      </ol>
      <div className="flex justify-between text-xs text-muted-foreground">
        <span>{days} days ago</span>
        <span>Today</span>
      </div>
    </section>
  )
}
