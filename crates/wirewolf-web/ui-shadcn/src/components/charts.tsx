// wirewolf's charts: the generic time-series chart with hops, incidents, and speed tests fed in.

import { ReferenceLine } from "recharts"
import { ChartLegend, ChartLegendContent } from "@/components/ui/chart"
import type { HopKind, Incident, SpeedRun } from "@/lib/api"
import { bucket, stepLabel, type Agg, type Point } from "@/lib/bucket"
import { pct } from "@/lib/format"
import { hopColor } from "@/lib/colors"
import { useLive } from "@/lib/live"
import { zoneLabel } from "@/lib/ticks"
import { ChartNote, TimeSeriesChart, type Band, type Scale, type Series } from "./time-series"

export { bucket, hopColor, stepLabel, ChartNote }
export type { Agg, Scale }

export interface HopSeries {
  id: string
  kind: HopKind
  label: string
}

/** Round trips above this are slow enough to count against the connection. */
export const SLOW_RTT_MS = 100

/** Incidents as shaded bands, clipped to the window. */
export function incidentBands(incidents: Incident[], from: number, to: number): Band[] {
  return incidents
    .map((inc) => ({ from: new Date(inc.started).getTime(), to: inc.ended ? new Date(inc.ended).getTime() : Date.now(), tone: inc.severity === "down" ? ("fail" as const) : ("warn" as const) }))
    .filter((b) => b.to >= from && b.from <= to)
}

/**
 * RTT or loss over time for several hops, with incidents shaded. `focus` picks one hop out and
 * fades the rest; `blame` makes a hop bold without fading the others.
 */
export function TimeChart({
  data,
  hops,
  metric,
  from,
  to,
  step,
  aggregation = "avg",
  scale = metric === "rtt" ? "log" : "linear",
  incidents = [],
  focus = null,
  blame = null,
  threshold = metric === "rtt",
  endLabels = true,
  syncId,
  onSelect,
  className = "aspect-auto h-64 w-full",
  label,
}: {
  data: Point[]
  hops: HopSeries[]
  metric: "rtt" | "loss"
  from: number
  to: number
  step?: number
  aggregation?: Agg
  scale?: Scale
  incidents?: Incident[]
  /** A hop id to highlight; the other lines fade so it can be followed through the overlap. */
  focus?: string | null
  /** The hop blamed for the current state: drawn bold. */
  blame?: string | null
  threshold?: boolean
  endLabels?: boolean
  syncId?: string
  onSelect?: (from: number, to: number) => void
  className?: string
  label?: string
}) {
  // Loss is zero almost always, and lines at zero hide each other, so only hops that lost
  // packets in this window are drawn (all of them if none did).
  const lossy = metric === "loss" ? hops.filter((h) => data.some((p) => (p[`${h.id}_loss`] ?? 0) > 0)) : hops
  const drawn = lossy.length > 0 ? lossy : hops
  const series: Series[] = drawn.map((h) => ({
    key: `${h.id}_${metric}`,
    label: h.label,
    color: hopColor[h.kind],
    weight: focus ? (focus === h.id ? "bold" : "faded") : blame === h.id ? "bold" : "normal",
  }))
  const unit = metric === "rtt" ? "ms" : "%"
  return (
    <TimeSeriesChart
      data={data}
      series={series}
      from={from}
      to={to}
      unit={unit}
      scale={metric === "rtt" ? scale : "linear"}
      step={step}
      aggregation={metric === "rtt" ? aggregation : "max"}
      bands={incidentBands(incidents, from, to)}
      threshold={threshold && metric === "rtt" ? { value: SLOW_RTT_MS, label: `${SLOW_RTT_MS} ms limit` } : undefined}
      curve={metric === "loss" ? "stepAfter" : "monotone"}
      yDomain={metric === "loss" ? [0, Math.max(5, Math.ceil(Math.max(0, ...data.flatMap((p) => drawn.map((h) => p[`${h.id}_loss`] ?? 0)))))] : undefined}
      endLabels={endLabels}
      syncId={syncId}
      onSelect={onSelect}
      extra={metric === "rtt" ? lossNote : undefined}
      className={className}
      label={label ?? `${metric === "rtt" ? "Round-trip time" : "Packet loss"} of ${drawn.map((h) => h.label).join(", ")}`}
    />
  )
}

/** On the RTT tooltip, a hop that lost packets in the bucket says so. */
const lossNote = (point: Point, s: Series) => {
  const loss = point[s.key.replace(/_rtt$/, "_loss")]
  return loss != null && loss > 0 ? <span className="text-fail"> · {pct(loss)} loss</span> : null
}

/** "avg · 2 min · log · AEDT": how to read a chart, for its footer. */
export function readingNote({ step, aggregation = "avg", scale }: { step?: number; aggregation?: Agg; scale?: Scale }): string[] {
  return [step ? `${aggregation} · ${stepLabel(step)}` : aggregation, scale === "log" ? "log scale" : scale === "linear" ? "linear" : "", zoneLabel()].filter(Boolean)
}

const speedSeries: Series[] = [
  { key: "down", label: "Download", color: "var(--chart-1)" },
  { key: "up", label: "Upload", color: "var(--chart-2)" },
]

/** Download and upload per completed speed test. Without `from`/`to` it spans the data. */
export function SpeedChart({
  runs,
  from,
  to,
  incidents = [],
  syncId,
  className = "aspect-auto h-56 w-full",
}: {
  runs: SpeedRun[]
  from?: number
  to?: number
  incidents?: Incident[]
  syncId?: string
  className?: string
}) {
  const plan = useLive((s) => s.plan)
  const data: Point[] = runs
    .filter((r) => r.state === "complete")
    .map((r) => ({ t: new Date(r.started).getTime(), down: r.down_mbps, up: r.up_mbps }))
    .filter((p) => (from == null || p.t >= from) && (to == null || p.t <= to))
    .sort((a, b) => a.t - b.t)
  if (data.length === 0) return <p className="py-10 text-center text-sm text-muted-foreground">No completed speed tests in this range.</p>
  const lo = from ?? data[0].t
  const hi = to ?? data[data.length - 1].t
  // Three round ticks, the same on every speed chart, so small and large charts read alike.
  const peak = Math.max(...data.flatMap((p) => [p.down ?? 0, p.up ?? 0]), plan.down ?? 0, plan.up ?? 0)
  const tick = peak > 2000 ? 1000 : peak > 400 ? 250 : peak > 80 ? 50 : 10
  const top = Math.max(tick, Math.ceil(peak / tick / 2) * tick * 2)
  return (
    <TimeSeriesChart
      data={data}
      series={speedSeries}
      from={lo}
      to={hi}
      unit="Mbps"
      yDomain={[0, top]}
      yTicks={[0, top / 2, top]}
      bands={incidentBands(incidents, lo, hi)}
      // Tests are separate measurements, so straight segments: a curve invents values between them.
      curve="linear"
      dots
      endLabels={false}
      syncId={syncId}
      className={className}
      label="Download and upload speed per test"
    >
      {/* The speeds paid for, dashed in each direction's colour. */}
      {speedSeries.map((s) => {
        const v = plan[s.key as "down" | "up"]
        return v != null ? <ReferenceLine key={s.key} y={v} stroke={s.color} strokeDasharray="4 4" strokeOpacity={0.6} ifOverflow="extendDomain" /> : null
      })}
      <ChartLegend content={<ChartLegendContent />} />
    </TimeSeriesChart>
  )
}
