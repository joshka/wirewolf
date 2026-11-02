// A time-series line chart on top of the shadcn chart and Recharts, with nothing wirewolf-specific
// in it: a real clock axis, log or linear scale with units, gaps where data is missing, incident
// bands, a threshold, end-of-line labels, a tooltip that lists every series, and drag to zoom.

import { useState, type ReactNode } from "react"
import { CartesianGrid, Line, LineChart, ReferenceArea, ReferenceLine, XAxis, YAxis, usePlotArea, useXAxisScale, useYAxisScale } from "recharts"
import { ChartContainer, ChartTooltip, type ChartConfig } from "@/components/ui/chart"
import { stepLabel, type Point } from "@/lib/bucket"
import { logDomain, logTicks, spread, tickLabel, timeTicks } from "@/lib/ticks"
import { cn } from "@/lib/utils"

export type Unit = "ms" | "%" | "Mbps"
export type Scale = "log" | "linear"

export interface Series {
  /** The key in each point. */
  key: string
  label: string
  color: string
  /** Bold is the series the chart is about; faded ones stay readable but recede. */
  weight?: "bold" | "faded" | "normal"
}

export interface Band {
  from: number
  to: number
  tone: "fail" | "warn"
}

export interface Threshold {
  value: number
  label: string
}

/** Axis ticks: whole units, one decimal under 10. */
export const unitTick: Record<Unit, (v: number) => string> = {
  ms: (v) => `${+v.toFixed(v < 10 ? 1 : 0)} ms`,
  "%": (v) => `${+v.toFixed(v < 10 && v !== 0 ? 1 : 0)}%`,
  Mbps: (v) => `${Math.round(v)} Mbps`,
}

/** Tooltip values: enough precision to compare hops. */
export const unitValue: Record<Unit, (v: number) => string> = {
  ms: (v) => `${+v.toFixed(v < 1 ? 2 : v < 10 ? 1 : 0)} ms`,
  "%": (v) => (v === 0 ? "0%" : `${+v.toFixed(v < 10 ? 1 : 0)}%`),
  Mbps: (v) => `${+v.toFixed(v < 10 ? 1 : 0)} Mbps`,
}

const LOG_FLOOR = 0.5
const safe = (key: string) => key.replace(/[^\w-]/g, "-")

const TOOLTIP_TIME = new Intl.DateTimeFormat(undefined, { weekday: "short", hour: "2-digit", minute: "2-digit" })
const TOOLTIP_DATETIME = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" })

export function TimeSeriesChart({
  data,
  series,
  from,
  to,
  unit,
  scale = "linear",
  step,
  aggregation,
  bands = [],
  threshold,
  curve = "monotone",
  dots = false,
  endLabels = true,
  yDomain,
  yTicks,
  syncId,
  onSelect,
  extra,
  className = "aspect-auto h-64 w-full",
  label,
  children,
}: {
  data: Point[]
  series: Series[]
  from: number
  to: number
  unit: Unit
  scale?: Scale
  /** The bucket size, named in the tooltip ("avg over 2 min"). */
  step?: number
  aggregation?: "avg" | "p95" | "max"
  bands?: Band[]
  threshold?: Threshold
  curve?: "monotone" | "stepAfter" | "linear"
  dots?: boolean
  endLabels?: boolean
  yDomain?: [number, number]
  yTicks?: number[]
  /** Charts with the same id share a crosshair. */
  syncId?: string
  /** Set to let the reader drag a range; called with the chosen window. */
  onSelect?: (from: number, to: number) => void
  /** Extra text for a series' tooltip row, for example its loss. */
  extra?: (point: Point, series: Series) => ReactNode
  className?: string
  /** For screen readers: what the chart shows. */
  label?: string
  children?: ReactNode
}) {
  // CSS variable names can't hold the `:` in target ids, so the config keys are sanitised.
  const config: ChartConfig = Object.fromEntries(series.map((s) => [safe(s.key), { label: s.label, color: s.color }]))
  const { ticks, step: tickStep } = timeTicks(from, to)
  const log = scale === "log"
  const values = log ? data.flatMap((p) => series.map((s) => p[s.key]).filter((v): v is number => v != null)) : []
  // A threshold gets headroom, so the limit line reads as a line and not as the chart's top edge.
  const domain: [number, number] | [number, string] = yDomain ?? (log ? logDomain(threshold ? [...values, threshold.value * 2] : values, LOG_FLOOR) : [0, "auto"])
  // log(0) is undefined, so values under the floor sit on it rather than vanishing.
  const drawn = log ? data.map((p) => Object.fromEntries(Object.entries(p).map(([k, v]) => [k, k !== "t" && v != null && v < domain[0] ? domain[0] : v])) as Point) : data
  const gutter = endLabels ? Math.max(...series.map((s) => s.label.length), 4) * 6.4 + 12 : 12

  const [drag, setDrag] = useState<{ x1: number; x2: number } | null>(null)
  const at = (e: { activeLabel?: string | number }) => (e.activeLabel == null ? null : Number(e.activeLabel))
  const select = onSelect
    ? {
        onMouseDown: (e: { activeLabel?: string | number }) => {
          const x = at(e)
          if (x != null) setDrag({ x1: x, x2: x })
        },
        onMouseMove: (e: { activeLabel?: string | number }) => {
          const x = at(e)
          if (drag && x != null) setDrag({ x1: drag.x1, x2: x })
        },
        onMouseUp: () => {
          if (drag) {
            const [a, b] = [Math.min(drag.x1, drag.x2), Math.max(drag.x1, drag.x2)]
            if (b - a >= (step ?? 0) * 2 && b - a > (to - from) / 100) onSelect(a, b)
          }
          setDrag(null)
        },
        onMouseLeave: () => setDrag(null),
      }
    : {}

  return (
    <ChartContainer config={config} className={cn(onSelect && "cursor-crosshair select-none", className)} aria-label={label}>
      <LineChart data={drawn} margin={{ left: 0, right: gutter, top: 12, bottom: 0 }} accessibilityLayer syncId={syncId} syncMethod="value" {...select}>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="t"
          type="number"
          scale="time"
          domain={[from, to]}
          ticks={ticks}
          tickFormatter={(t: number) => tickLabel(t, tickStep)}
          tickLine={false}
          axisLine={false}
          minTickGap={24}
          allowDataOverflow
        />
        <YAxis
          width={unit === "Mbps" ? 76 : unit === "ms" ? 64 : 48}
          tickLine={false}
          axisLine={false}
          scale={log ? "log" : "linear"}
          domain={domain}
          ticks={yTicks ?? (log ? logTicks(domain[0] as number, domain[1] as number) : undefined)}
          interval={yTicks || log ? 0 : undefined}
          allowDataOverflow={log}
          tickFormatter={unitTick[unit]}
        />
        {bands.map((b, i) => {
          const x1 = Math.max(from, b.from)
          const x2 = Math.min(to, b.to)
          if (x2 < from || x1 > to) return null
          return <ReferenceArea key={i} x1={x1} x2={Math.max(x2, x1 + (to - from) / 300)} ifOverflow="hidden" fill={`var(--${b.tone})`} fillOpacity={0.12} />
        })}
        {threshold && (
          <ReferenceLine
            y={threshold.value}
            ifOverflow="hidden"
            stroke="var(--chart-threshold)"
            strokeDasharray="4 3"
            strokeOpacity={0.7}
            label={{ value: threshold.label, position: "insideTopLeft", fill: "var(--chart-threshold)", fontSize: 11, dy: -6, dx: 2 }}
          />
        )}
        {drag && drag.x1 !== drag.x2 && <ReferenceArea x1={Math.min(drag.x1, drag.x2)} x2={Math.max(drag.x1, drag.x2)} fill="var(--primary)" fillOpacity={0.1} stroke="var(--primary)" strokeOpacity={0.5} />}
        <ChartTooltip
          cursor={{ stroke: "var(--border)", strokeWidth: 1 }}
          isAnimationActive={false}
          content={<SeriesTooltip series={series} unit={unit} step={step} aggregation={aggregation} extra={extra} />}
        />
        {series.map((s) => (
          <Line
            key={s.key}
            dataKey={s.key}
            name={safe(s.key)}
            type={curve}
            stroke={s.color}
            strokeWidth={s.weight === "bold" ? 2.5 : s.weight === "faded" ? 1.25 : 2}
            strokeOpacity={s.weight === "faded" ? 0.35 : 1}
            strokeLinejoin="round"
            strokeLinecap="round"
            connectNulls={false}
            dot={dots ? { r: 3, fill: s.color, stroke: "var(--card)", strokeWidth: 2 } : false}
            activeDot={{ r: 4, fill: s.color, stroke: "var(--card)", strokeWidth: 2 }}
            isAnimationActive={false}
          />
        ))}
        {endLabels && <EndLabels data={drawn} series={series} />}
        <GapLabel data={data} series={series} />
        {children}
      </LineChart>
    </ChartContainer>
  )
}

/**
 * Each series' name at the end of its line, in text colour: the line is the colour key. Labels
 * that would overlap are spread apart, with a hairline back to the line they belong to.
 */
function EndLabels({ data, series }: { data: Point[]; series: Series[] }) {
  const x = useXAxisScale()
  const y = useYAxisScale()
  const area = usePlotArea()
  if (!x || !y || !area) return null
  const ends = series.flatMap((s) => {
    for (let i = data.length - 1; i >= 0; i--) {
      const v = data[i][s.key]
      if (v != null) return [{ s, t: data[i].t, v }]
    }
    return []
  })
  if (ends.length === 0) return null
  const wanted = ends.map((e) => y(e.v) ?? area.y)
  const ys = spread(wanted, 13, area.y + 6, area.y + area.height - 4)
  const lx = area.x + area.width + 6
  return (
    <g aria-hidden>
      {ends.map((e, i) => (
        <g key={e.s.key} opacity={e.s.weight === "faded" ? 0.5 : 1}>
          {Math.abs(ys[i] - wanted[i]) > 2 && <line x1={x(e.t) ?? lx} y1={wanted[i]} x2={lx - 2} y2={ys[i]} stroke={e.s.color} strokeWidth={1} strokeOpacity={0.6} />}
          <text x={lx} y={ys[i]} dominantBaseline="middle" fontSize={11} fontWeight={e.s.weight === "bold" ? 600 : 400} className={e.s.weight === "bold" ? "fill-foreground" : "fill-muted-foreground"}>
            {e.s.label}
          </text>
        </g>
      ))}
    </g>
  )
}

/** Names the longest stretch with no data for the bold series: missing data is evidence. */
function GapLabel({ data, series }: { data: Point[]; series: Series[] }) {
  const x = useXAxisScale()
  const area = usePlotArea()
  const bold = series.find((s) => s.weight === "bold")
  if (!x || !area || !bold || data.length < 6) return null
  let best: [number, number] | null = null
  let run: number | null = null
  const seen = data.some((p) => p[bold.key] != null)
  if (!seen) return null
  for (let i = 0; i < data.length; i++) {
    const missing = data[i][bold.key] == null
    if (missing && run == null) run = i
    if ((!missing || i === data.length - 1) && run != null) {
      const end = missing ? i : i - 1
      // Only a gap inside the series counts; the leading stretch is just "not started yet".
      if (run > 0 && (!best || end - run > best[1] - best[0])) best = [run, end]
      run = null
    }
  }
  if (!best || best[1] - best[0] + 1 < Math.max(3, data.length / 20)) return null
  const left = x(data[best[0]].t)
  const right = x(data[best[1]].t)
  if (left == null || right == null || right - left < 56) return null
  return (
    <text aria-hidden x={(left + right) / 2} y={area.y + area.height / 2} textAnchor="middle" dominantBaseline="middle" fontSize={11} className="fill-chart-nodata">
      no replies
    </text>
  )
}

function SeriesTooltip({
  active,
  payload,
  label,
  series,
  unit,
  step,
  aggregation,
  extra,
}: {
  active?: boolean
  payload?: { payload?: Point }[]
  label?: string | number
  series: Series[]
  unit: Unit
  step?: number
  aggregation?: string
  extra?: (point: Point, series: Series) => ReactNode
}) {
  const point = payload?.[0]?.payload
  if (!active || !point || label == null) return null
  const t = Number(label)
  const rows = series
    .map((s) => ({ s, v: point[s.key] }))
    .sort((a, b) => {
      if (a.v == null || b.v == null) return a.v == null ? 1 : -1
      return unit === "Mbps" ? a.v - b.v : b.v - a.v
    })
  return (
    <div className="grid min-w-44 gap-1.5 rounded-lg border bg-popover px-3 py-2 text-xs text-popover-foreground shadow-md">
      <div className="text-muted-foreground">
        {step ? TOOLTIP_TIME.format(t) : TOOLTIP_DATETIME.format(t)}
        {step && aggregation && <span> · {aggregation} over {stepLabel(step)}</span>}
      </div>
      {rows.map(({ s, v }) => (
        <div key={s.key} className={cn("flex items-center justify-between gap-4", s.weight === "faded" && "opacity-60", s.weight === "bold" && "font-semibold")}>
          <span className="flex items-center gap-1.5">
            <span className="h-0.5 w-3 rounded-full" style={{ background: s.color }} />
            {s.label}
          </span>
          <span className="font-mono tabular-nums">
            {v == null ? <span className="text-chart-nodata">no data</span> : unitValue[unit](v)}
            {v != null && extra?.(point, s)}
          </span>
        </div>
      ))}
    </div>
  )
}

/** The line under a chart saying how to read it: "avg · 2 min · log · AEDT", with room for a link. */
export function ChartNote({ items, children, className }: { items: (string | false | null | undefined)[]; children?: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-wrap items-center justify-between gap-x-4 gap-y-1 text-xs text-muted-foreground", className)}>
      <span>{items.filter(Boolean).join(" · ")}</span>
      {children}
    </div>
  )
}
