import { Area, CartesianGrid, ComposedChart, Line, LineChart, ReferenceArea, XAxis, YAxis } from "recharts"
import { ChartContainer, ChartLegend, ChartLegendContent, ChartTooltip, ChartTooltipContent, type ChartConfig } from "@/components/ui/chart"
import type { Incident, Sample, SpeedRun } from "@/lib/api"
import { clock, mbps, ms, pct } from "@/lib/format"

export const hopColor: Record<string, string> = {
  gateway: "var(--hop-gateway)",
  isp: "var(--hop-isp)",
  internet: "var(--hop-internet)",
  dns: "var(--hop-dns)",
  web: "var(--hop-web)",
  host: "var(--muted-foreground)",
}

type Point = { t: number } & Record<string, number | null>

/**
 * Buckets samples into at most `points` time slots per hop: mean RTT, max loss. Keeps 24h of
 * per-minute data (8k+ samples) light enough to redraw.
 */
export function bucket(samples: Sample[], from: number, to: number, points = 240): Point[] {
  const step = Math.max(1, (to - from) / points)
  const slots = new Map<number, { rtt: Record<string, number[]>; loss: Record<string, number> }>()
  for (const s of samples) {
    const t = new Date(s.at).getTime()
    if (t < from || t > to) continue
    const k = from + Math.floor((t - from) / step) * step
    let slot = slots.get(k)
    if (!slot) slots.set(k, (slot = { rtt: {}, loss: {} }))
    if (s.rtt_ms != null) (slot.rtt[s.hop] ??= []).push(s.rtt_ms)
    if (s.loss_pct != null) slot.loss[s.hop] = Math.max(slot.loss[s.hop] ?? 0, s.loss_pct)
  }
  return [...slots.entries()]
    .sort(([a], [b]) => a - b)
    .map(([t, slot]) => {
      const p: Point = { t }
      for (const [hop, v] of Object.entries(slot.rtt)) p[`${hop}_rtt`] = v.reduce((a, b) => a + b, 0) / v.length
      for (const [hop, v] of Object.entries(slot.loss)) p[`${hop}_loss`] = v
      return p
    })
}

interface Props {
  data: Point[]
  hops: { id: string; label: string }[]
  metric: "rtt" | "loss"
  from: number
  to: number
  incidents?: Incident[]
  /** Draw loss as a step line (History) rather than a smooth line. */
  step?: boolean
  className?: string
}

/** RTT or loss over time for several hops, with incident bands and speed-test markers. */
export function TimeChart({ data, hops, metric, from, to, incidents = [], step, className }: Props) {
  const config: ChartConfig = Object.fromEntries(
    hops.map((h) => [`${h.id}_${metric}`, { label: h.label, color: hopColor[h.id] ?? "var(--chart-1)" }]),
  )
  const fmt = metric === "rtt" ? ms : pct
  const tick = timeTick(to - from)
  return (
    <ChartContainer config={config} className={className ?? "aspect-auto h-64 w-full"}>
      <LineChart data={data} margin={{ left: 4, right: 12, top: 8, bottom: 0 }} accessibilityLayer>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="t"
          type="number"
          scale="time"
          domain={[from, to]}
          tickFormatter={tick}
          tickLine={false}
          axisLine={false}
          minTickGap={48}
          allowDataOverflow
        />
        <YAxis
          width={56}
          tickLine={false}
          axisLine={false}
          tickFormatter={(v: number) => (metric === "rtt" ? `${+v.toFixed(v < 10 ? 1 : 0)} ms` : `${+v.toFixed(0)}%`)}
          domain={metric === "loss" ? [0, (max: number) => Math.max(5, Math.ceil(max))] : [0, "auto"]}
        />
        {incidentBands(incidents, from, to)}
        <ChartTooltip
          cursor={{ strokeDasharray: "3 3" }}
          content={
            <ChartTooltipContent
              indicator="line"
              labelFormatter={(_, p) => {
                const t = p?.[0]?.payload?.t
                return t ? new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : ""
              }}
              formatter={(value, name, item) => (
                <div className="flex w-full items-center justify-between gap-4">
                  <span className="flex items-center gap-1.5 text-muted-foreground">
                    <span className="h-2.5 w-1 rounded-sm" style={{ background: item.color }} />
                    {config[String(name)]?.label}
                  </span>
                  <span className="tnum font-mono font-medium text-foreground">{fmt(Number(value))}</span>
                </div>
              )}
            />
          }
        />
        {hops.map((h) => (
          <Line
            key={h.id}
            dataKey={`${h.id}_${metric}`}
            type={step ? "stepAfter" : "monotone"}
            stroke={`var(--color-${h.id}_${metric})`}
            strokeWidth={1.75}
            dot={false}
            connectNulls={false}
            isAnimationActive={false}
          />
        ))}
      </LineChart>
    </ChartContainer>
  )
}

/** Shaded incident periods, clipped to the visible window. Used by every time chart. */
export function incidentBands(incidents: Incident[], from: number, to: number, yAxisId?: string) {
  const span = to - from
  return incidents.map((inc) => {
    const x1 = Math.max(from, new Date(inc.started).getTime())
    const x2 = Math.min(to, inc.ended ? new Date(inc.ended).getTime() : Date.now())
    if (x2 < from || x1 > to) return null
    return (
      <ReferenceArea
        key={inc.id}
        yAxisId={yAxisId}
        x1={x1}
        x2={Math.max(x2, x1 + span / 300)}
        ifOverflow="hidden"
        fill={inc.severity === "down" ? "var(--fail)" : "var(--warn)"}
        fillOpacity={0.15}
        strokeOpacity={0}
      />
    )
  })
}

const timeTick = (span: number) => (t: number) =>
  span > 2 * 86400e3 ? new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : clock(t)

/** One hop's last hour: RTT line in the hop color, loss as a red area on a 0–100% right axis. */
export function HopChart({
  hop,
  data,
  from,
  to,
  incidents,
}: {
  hop: { id: string; label: string }
  data: Point[]
  from: number
  to: number
  incidents: Incident[]
}) {
  const config: ChartConfig = {
    [`${hop.id}_rtt`]: { label: "RTT", color: hopColor[hop.id] ?? "var(--chart-1)" },
    [`${hop.id}_loss`]: { label: "Loss", color: "var(--fail)" },
  }
  return (
    <ChartContainer config={config} className="aspect-auto h-52 w-full">
      <ComposedChart data={data} margin={{ left: 4, right: 4, top: 8, bottom: 0 }} accessibilityLayer>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="t" type="number" scale="time" domain={[from, to]} tickFormatter={timeTick(to - from)} tickLine={false} axisLine={false} minTickGap={48} allowDataOverflow />
        <YAxis yAxisId="rtt" width={52} tickLine={false} axisLine={false} domain={[0, "auto"]} tickFormatter={(v: number) => `${+v.toFixed(v < 10 ? 1 : 0)} ms`} />
        <YAxis yAxisId="loss" orientation="right" width={40} tickLine={false} axisLine={false} domain={[0, 100]} ticks={[0, 50, 100]} tickFormatter={(v: number) => `${v}%`} />
        {incidentBands(incidents, from, to, "rtt")}
        <ChartTooltip
          cursor={{ strokeDasharray: "3 3" }}
          content={
            <ChartTooltipContent
              indicator="line"
              labelFormatter={(_, p) => {
                const t = p?.[0]?.payload?.t
                return t ? new Date(t).toLocaleTimeString(undefined, { timeStyle: "short" }) : ""
              }}
              formatter={(value, name, item) => (
                <div className="flex w-full items-center justify-between gap-4">
                  <span className="flex items-center gap-1.5 text-muted-foreground">
                    <span className="h-2.5 w-1 rounded-sm" style={{ background: item.color }} />
                    {config[String(name)]?.label}
                  </span>
                  <span className="tnum font-mono font-medium text-foreground">
                    {String(name).endsWith("_loss") ? pct(Number(value)) : ms(Number(value))}
                  </span>
                </div>
              )}
            />
          }
        />
        <Area yAxisId="loss" dataKey={`${hop.id}_loss`} type="stepAfter" stroke="var(--fail)" strokeOpacity={0.6} fill="var(--fail)" fillOpacity={0.2} isAnimationActive={false} />
        <Line yAxisId="rtt" dataKey={`${hop.id}_rtt`} type="monotone" stroke={`var(--color-${hop.id}_rtt)`} strokeWidth={1.75} dot={false} isAnimationActive={false} />
      </ComposedChart>
    </ChartContainer>
  )
}

const speedConfig = {
  down: { label: "Download", color: "var(--chart-1)" },
  up: { label: "Upload", color: "var(--chart-2)" },
} satisfies ChartConfig

/** Download and upload per completed speed test, with point markers. */
export function SpeedChart({
  runs,
  from,
  to,
  incidents = [],
  className,
}: {
  runs: SpeedRun[]
  from?: number
  to?: number
  incidents?: Incident[]
  className?: string
}) {
  const data = runs
    .filter((r) => r.state === "complete")
    .map((r) => ({ t: new Date(r.started).getTime(), down: r.down_mbps, up: r.up_mbps }))
    .filter((p) => (from == null || p.t >= from) && (to == null || p.t <= to))
    .sort((a, b) => a.t - b.t)
  const lo = from ?? data[0]?.t ?? Date.now()
  const hi = to ?? data[data.length - 1]?.t ?? Date.now()
  if (data.length === 0)
    return (
      <p className="grid h-40 place-items-center rounded-lg border border-dashed text-sm text-muted-foreground">
        No completed speed tests in this range.
      </p>
    )
  return (
    <ChartContainer config={speedConfig} className={className ?? "aspect-auto h-56 w-full"}>
      <LineChart data={data} margin={{ left: 4, right: 12, top: 8, bottom: 0 }} accessibilityLayer>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="t"
          type="number"
          scale="time"
          domain={from != null ? [lo, hi] : ["dataMin", "dataMax"]}
          tickFormatter={timeTick(hi - lo)}
          tickLine={false}
          axisLine={false}
          minTickGap={48}
          allowDataOverflow
        />
        <YAxis width={56} tickLine={false} axisLine={false} domain={[0, "auto"]} tickFormatter={(v: number) => `${v}`} />
        {incidentBands(incidents, lo, hi)}
        <ChartTooltip
          content={
            <ChartTooltipContent
              indicator="line"
              labelFormatter={(_, p) => {
                const t = p?.[0]?.payload?.t
                return t ? new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : ""
              }}
              formatter={(value, name, item) => (
                <div className="flex w-full items-center justify-between gap-4">
                  <span className="flex items-center gap-1.5 text-muted-foreground">
                    <span className="h-2.5 w-1 rounded-sm" style={{ background: item.color }} />
                    {speedConfig[name as keyof typeof speedConfig]?.label}
                  </span>
                  <span className="tnum font-mono font-medium text-foreground">{mbps(Number(value))}</span>
                </div>
              )}
            />
          }
        />
        {(["down", "up"] as const).map((k) => (
          <Line
            key={k}
            dataKey={k}
            type="monotone"
            stroke={`var(--color-${k})`}
            strokeWidth={2}
            dot={{ r: 2.5, fill: `var(--color-${k})`, strokeWidth: 0 }}
            activeDot={{ r: 4 }}
            isAnimationActive={false}
          />
        ))}
        <ChartLegend content={<ChartLegendContent />} />
      </LineChart>
    </ChartContainer>
  )
}
