import { Scatter, ScatterChart, Tooltip, XAxis, YAxis } from "recharts"
import { ChartContainer } from "@/components/ui/chart"
import type { Activity, Incident, SpeedRun } from "@/lib/api"
import { mbps } from "@/lib/format"
import { timeTicks } from "@/lib/ticks"
import { AXIS_WIDTH } from "./time-series"

type Kind = "opened" | "resolved" | "speed" | "settings"

interface Marker {
  t: number
  kind: Kind
  label: string
}

const COLOR: Record<Kind, string> = {
  opened: "var(--fail)",
  resolved: "var(--pass)",
  speed: "var(--chart-1)",
  settings: "var(--muted-foreground)",
}

const NAME: Record<Kind, string> = { opened: "incident opened", resolved: "resolved", speed: "speed test", settings: "settings changed" }

const TIME = new Intl.DateTimeFormat(undefined, { weekday: "short", hour: "2-digit", minute: "2-digit" })

/** What happened when: one marker per incident edge, speed test, and settings change. */
export function eventMarkers(incidents: Incident[], runs: SpeedRun[], activity: Activity[], from: number, to: number): Marker[] {
  const out: Marker[] = []
  for (const i of incidents) {
    out.push({ t: new Date(i.started).getTime(), kind: "opened", label: i.headline })
    if (i.ended) out.push({ t: new Date(i.ended).getTime(), kind: "resolved", label: `Resolved: ${i.headline}` })
  }
  for (const r of runs) {
    if (r.state !== "complete") continue
    out.push({ t: new Date(r.started).getTime(), kind: "speed", label: `Speed test: ${mbps(r.down_mbps)} down, ${mbps(r.up_mbps)} up` })
  }
  for (const a of activity) if (a.kind === "config") out.push({ t: new Date(a.at).getTime(), kind: "settings", label: a.message })
  return out.filter((m) => m.t >= from && m.t <= to).sort((a, b) => a.t - b.t)
}

/**
 * A thin lane under the charts that puts events on the same time axis, so "latency rose at
 * 04:40" sits next to "a speed test ran at 04:40". Shares the charts' crosshair.
 */
export function EventsLane({ markers, from, to, gutter, syncId }: { markers: Marker[]; from: number; to: number; gutter: number; syncId?: string }) {
  const { ticks } = timeTicks(from, to)
  const kinds = (Object.keys(NAME) as Kind[]).filter((k) => markers.some((m) => m.kind === k))
  return (
    <div className="flex flex-col gap-1">
      <ChartContainer config={{}} className="aspect-auto h-9 w-full" aria-label="Events on the same time axis">
        <ScatterChart margin={{ left: 0, right: gutter, top: 8, bottom: 0 }} syncId={syncId} syncMethod="value">
          <XAxis dataKey="t" type="number" scale="time" domain={[from, to]} ticks={ticks} hide allowDataOverflow />
          <YAxis dataKey="y" type="number" domain={[0, 1]} width={AXIS_WIDTH} hide={false} tick={false} tickLine={false} axisLine={false} />
          <Tooltip
            cursor={{ stroke: "var(--border)", strokeWidth: 1 }}
            isAnimationActive={false}
            content={({ active, payload }) => {
              const m = payload?.[0]?.payload as Marker | undefined
              if (!active || !m) return null
              return (
                <div className="max-w-72 rounded-lg border bg-popover px-3 py-2 text-xs text-popover-foreground shadow-md">
                  <div className="text-muted-foreground">
                    {TIME.format(m.t)} · {NAME[m.kind]}
                  </div>
                  <div className="mt-1">{m.label}</div>
                </div>
              )
            }}
          />
          {kinds.map((k) => (
            <Scatter
              key={k}
              name={NAME[k]}
              data={markers.filter((m) => m.kind === k).map((m) => ({ ...m, y: 0.5 }))}
              fill={COLOR[k]}
              stroke="var(--card)"
              strokeWidth={2}
              shape={(p: { cx?: number; cy?: number }) => <circle cx={p.cx} cy={p.cy} r={5} fill={COLOR[k]} stroke="var(--card)" strokeWidth={2} />}
              isAnimationActive={false}
            />
          ))}
        </ScatterChart>
      </ChartContainer>
      <ul className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground" style={{ paddingLeft: AXIS_WIDTH }} aria-label="Event kinds">
        {kinds.length === 0 && <li>No events in this window.</li>}
        {kinds.map((k) => (
          <li key={k} className="flex items-center gap-1.5">
            <span className="size-2.5 rounded-full" style={{ background: COLOR[k] }} />
            {NAME[k]}
          </li>
        ))}
      </ul>
    </div>
  )
}
