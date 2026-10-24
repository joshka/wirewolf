import { Area, CartesianGrid, ComposedChart, Line, LineChart, ReferenceArea, XAxis, YAxis } from "recharts"
import { ChartContainer, ChartLegend, ChartLegendContent, ChartTooltip, ChartTooltipContent, type ChartConfig } from "@/components/ui/chart"
import type { HopKind, Incident, Sample, SpeedRun } from "@/lib/api"
import { clock, mbps, ms, pct } from "@/lib/format"

/** Series colors by hop kind (theme tokens in index.css). Host has no series. */
export const hopColor: Record<HopKind, string> = {
  host: "var(--muted-foreground)",
  gateway: "var(--hop-gateway)",
  isp: "var(--hop-isp)",
  internet: "var(--hop-internet)",
  dns: "var(--hop-dns)",
  web: "var(--hop-web)",
}

type Point = { t: number } & Record<string, number | null>
type Series = { id: string; kind: HopKind; label: string }

/** Buckets samples into at most `points` slots per hop: mean RTT, worst loss. */
export function bucket(samples: Sample[], from: number, to: number, points = 300): Point[] {
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

const fmtFor = (key: string) => (key.endsWith("_loss") ? pct : key.endsWith("_rtt") ? ms : mbps)

// Recharts needs its axis and tooltip elements as direct children, so these are element
// factories rather than components.
const timeAxis = (from: number | string, to: number | string, span: number) => (
  <XAxis
    dataKey="t"
    type="number"
    scale="time"
    domain={[from, to]}
    tickFormatter={(t: number) => (span > 2 * 86400e3 ? new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : clock(t))}
    tickLine={false}
    axisLine={false}
    minTickGap={48}
    allowDataOverflow
  />
)

const tooltip = (config: ChartConfig) => (
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
              {config[String(name)]?.label}
            </span>
            <span className="font-mono font-medium text-foreground tabular-nums">{fmtFor(String(name))(Number(value))}</span>
          </div>
        )}
      />
    }
  />
)

/** Shaded incident periods, clipped to the visible window. */
const incidentBands = (incidents: Incident[], from: number, to: number, yAxisId?: string) =>
  incidents.map((inc) => {
    const x1 = Math.max(from, new Date(inc.started).getTime())
    const x2 = Math.min(to, inc.ended ? new Date(inc.ended).getTime() : Date.now())
    if (x2 < from || x1 > to) return null
    return (
      <ReferenceArea
        key={inc.id}
        yAxisId={yAxisId}
        x1={x1}
        x2={Math.max(x2, x1 + (to - from) / 300)}
        ifOverflow="hidden"
        fill={inc.severity === "down" ? "var(--fail)" : "var(--warn)"}
        fillOpacity={0.15}
      />
    )
  })

const msTick = (v: number) => `${+v.toFixed(v < 10 ? 1 : 0)} ms`

/** RTT or loss over time for several hops, with incident bands. */
export function TimeChart({
  data,
  hops,
  metric,
  from,
  to,
  incidents = [],
  className = "aspect-auto h-64 w-full",
}: {
  data: Point[]
  hops: Series[]
  metric: "rtt" | "loss"
  from: number
  to: number
  incidents?: Incident[]
  className?: string
}) {
  const config: ChartConfig = Object.fromEntries(hops.map((h) => [`${h.id}_${metric}`, { label: h.label, color: hopColor[h.kind] }]))
  return (
    <ChartContainer config={config} className={className}>
      <LineChart data={data} margin={{ left: 4, right: 12, top: 8 }} accessibilityLayer>
        <CartesianGrid vertical={false} />
        {timeAxis(from, to, to - from)}
        <YAxis
          width={56}
          tickLine={false}
          axisLine={false}
          tickFormatter={metric === "rtt" ? msTick : (v: number) => `${v}%`}
          domain={metric === "loss" ? [0, (max: number) => Math.max(5, Math.ceil(max))] : [0, "auto"]}
        />
        {incidentBands(incidents, from, to)}
        {tooltip(config)}
        {hops.map((h) => (
          <Line
            key={h.id}
            dataKey={`${h.id}_${metric}`}
            type={metric === "loss" ? "stepAfter" : "monotone"}
            stroke={`var(--color-${h.id}_${metric})`}
            strokeWidth={1.75}
            dot={false}
            isAnimationActive={false}
          />
        ))}
      </LineChart>
    </ChartContainer>
  )
}

/** One hop: RTT line in the hop color, loss as a red area on a 0–100% right axis. */
export function HopChart({ hop, data, from, to, incidents }: { hop: Series; data: Point[]; from: number; to: number; incidents: Incident[] }) {
  const config: ChartConfig = {
    [`${hop.id}_rtt`]: { label: "RTT", color: hopColor[hop.kind] },
    [`${hop.id}_loss`]: { label: "Loss", color: "var(--fail)" },
  }
  return (
    <ChartContainer config={config} className="aspect-auto h-52 w-full">
      <ComposedChart data={data} margin={{ left: 4, right: 4, top: 8 }} accessibilityLayer>
        <CartesianGrid vertical={false} />
        {timeAxis(from, to, to - from)}
        <YAxis yAxisId="rtt" width={52} tickLine={false} axisLine={false} domain={[0, "auto"]} tickFormatter={msTick} />
        <YAxis yAxisId="loss" orientation="right" width={40} tickLine={false} axisLine={false} domain={[0, 100]} ticks={[0, 50, 100]} tickFormatter={(v: number) => `${v}%`} />
        {incidentBands(incidents, from, to, "rtt")}
        {tooltip(config)}
        <Area yAxisId="loss" dataKey={`${hop.id}_loss`} type="stepAfter" stroke="var(--fail)" fill="var(--fail)" fillOpacity={0.2} isAnimationActive={false} />
        <Line yAxisId="rtt" dataKey={`${hop.id}_rtt`} type="monotone" stroke={`var(--color-${hop.id}_rtt)`} strokeWidth={1.75} dot={false} isAnimationActive={false} />
      </ComposedChart>
    </ChartContainer>
  )
}

const speedConfig: ChartConfig = {
  down: { label: "Download", color: "var(--chart-1)" },
  up: { label: "Upload", color: "var(--chart-2)" },
}

/** Download and upload per completed speed test. Without `from`/`to` it spans the data. */
export function SpeedChart({ runs, from, to, incidents = [] }: { runs: SpeedRun[]; from?: number; to?: number; incidents?: Incident[] }) {
  const data = runs
    .filter((r) => r.state === "complete")
    .map((r) => ({ t: new Date(r.started).getTime(), down: r.down_mbps, up: r.up_mbps }))
    .filter((p) => (from == null || p.t >= from) && (to == null || p.t <= to))
    .sort((a, b) => a.t - b.t)
  if (data.length === 0) return <p className="py-10 text-center text-sm text-muted-foreground">No completed speed tests in this range.</p>
  const lo = from ?? data[0].t
  const hi = to ?? data[data.length - 1].t
  return (
    <ChartContainer config={speedConfig} className="aspect-auto h-56 w-full">
      <LineChart data={data} margin={{ left: 4, right: 12, top: 8 }} accessibilityLayer>
        <CartesianGrid vertical={false} />
        {timeAxis(from ?? "dataMin", to ?? "dataMax", hi - lo)}
        <YAxis width={56} tickLine={false} axisLine={false} domain={[0, "auto"]} />
        {incidentBands(incidents, lo, hi)}
        {tooltip(speedConfig)}
        {(["down", "up"] as const).map((k) => (
          <Line key={k} dataKey={k} type="monotone" stroke={`var(--color-${k})`} strokeWidth={2} dot={{ r: 2.5, fill: `var(--color-${k})`, strokeWidth: 0 }} isAnimationActive={false} />
        ))}
        <ChartLegend content={<ChartLegendContent />} />
      </LineChart>
    </ChartContainer>
  )
}
