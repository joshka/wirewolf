import { CartesianGrid, Line, LineChart, ReferenceArea, ReferenceLine, XAxis, YAxis } from "recharts"
import { ChartContainer, ChartLegend, ChartLegendContent, ChartTooltip, ChartTooltipContent, type ChartConfig } from "@/components/ui/chart"
import type { HopKind, Incident, SpeedRun } from "@/lib/api"
import { bucket, type Point } from "@/lib/bucket"
import { clock, mbps, ms, pct } from "@/lib/format"
import { hopColor } from "@/lib/colors"
import { useLive } from "@/lib/live"

export { bucket }
export { hopColor }
type Series = { id: string; kind: HopKind; label: string }


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
const incidentBands = (incidents: Incident[], from: number, to: number) =>
  incidents.map((inc) => {
    const x1 = Math.max(from, new Date(inc.started).getTime())
    const x2 = Math.min(to, inc.ended ? new Date(inc.ended).getTime() : Date.now())
    if (x2 < from || x1 > to) return null
    return (
      <ReferenceArea
        key={inc.id}
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
  focus = null,
  className = "aspect-auto h-64 w-full",
}: {
  data: Point[]
  hops: Series[]
  metric: "rtt" | "loss"
  from: number
  to: number
  incidents?: Incident[]
  /** A hop id to highlight; the other lines fade so it can be followed through the overlap. */
  focus?: string | null
  className?: string
}) {
  // CSS variable names can't hold the `:` in target ids, so the config keys swap it out.
  const key = (h: Series) => `${h.id.replace(/[^\w-]/g, "-")}_${metric}`
  const config: ChartConfig = Object.fromEntries(hops.map((h) => [key(h), { label: h.label, color: hopColor[h.kind] }]))
  // Loss is zero almost always, and lines at zero hide each other, so only hops that lost
  // packets in this window are drawn (all of them if none did).
  const lossy = metric === "loss" ? hops.filter((h) => data.some((p) => (p[`${h.id}_loss`] ?? 0) > 0)) : hops
  const drawn = lossy.length > 0 ? lossy : hops
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
        {drawn.map((h) => (
          <Line
            key={h.id}
            dataKey={`${h.id}_${metric}`}
            type={metric === "loss" ? "stepAfter" : "monotone"}
            name={key(h)}
            stroke={`var(--color-${key(h)})`}
            strokeOpacity={focus && focus !== h.id ? 0.15 : 1}
            strokeWidth={focus === h.id ? 2.5 : 1.75}
            dot={false}
            isAnimationActive={false}
          />
        ))}
      </LineChart>
    </ChartContainer>
  )
}

const speedConfig: ChartConfig = {
  down: { label: "Download", color: "var(--chart-1)" },
  up: { label: "Upload", color: "var(--chart-2)" },
}

/** Download and upload per completed speed test. Without `from`/`to` it spans the data. */
export function SpeedChart({
  runs,
  from,
  to,
  incidents = [],
  className = "aspect-auto h-56 w-full",
}: {
  runs: SpeedRun[]
  from?: number
  to?: number
  incidents?: Incident[]
  className?: string
}) {
  const plan = useLive((s) => s.plan)
  const data = runs
    .filter((r) => r.state === "complete")
    .map((r) => ({ t: new Date(r.started).getTime(), down: r.down_mbps, up: r.up_mbps }))
    .filter((p) => (from == null || p.t >= from) && (to == null || p.t <= to))
    .sort((a, b) => a.t - b.t)
  if (data.length === 0) return <p className="py-10 text-center text-sm text-muted-foreground">No completed speed tests in this range.</p>
  const lo = from ?? data[0].t
  const hi = to ?? data[data.length - 1].t
  // Three round ticks, the same on every speed chart, so small and large charts read alike.
  const peak = Math.max(...data.flatMap((p) => [p.down ?? 0, p.up ?? 0]), plan.down ?? 0, plan.up ?? 0)
  const step = peak > 2000 ? 1000 : peak > 400 ? 250 : peak > 80 ? 50 : 10
  const top = Math.max(step, Math.ceil(peak / step / 2) * step * 2)
  return (
    <ChartContainer config={speedConfig} className={className}>
      <LineChart data={data} margin={{ left: 4, right: 12, top: 12 }} accessibilityLayer>
        <CartesianGrid vertical={false} />
        {timeAxis(from ?? "dataMin", to ?? "dataMax", hi - lo)}
        <YAxis width={84} tickLine={false} axisLine={false} domain={[0, top]} ticks={[0, top / 2, top]} interval={0} tickFormatter={(v: number) => `${v} Mbps`} />
        {incidentBands(incidents, lo, hi)}
        {/* The speeds paid for, dashed in each direction's colour. */}
        {(["down", "up"] as const).map((k) =>
          plan[k] != null ? <ReferenceLine key={k} y={plan[k]!} stroke={`var(--color-${k})`} strokeDasharray="4 4" strokeOpacity={0.6} ifOverflow="extendDomain" /> : null,
        )}
        {tooltip(speedConfig)}
        {/* Tests are separate measurements, so straight segments: a curve invents values between them. */}
        {(["down", "up"] as const).map((k) => (
          <Line key={k} dataKey={k} type="linear" stroke={`var(--color-${k})`} strokeWidth={2} dot={{ r: 2.5, fill: `var(--color-${k})`, strokeWidth: 0 }} isAnimationActive={false} />
        ))}
        <ChartLegend content={<ChartLegendContent />} />
      </LineChart>
    </ChartContainer>
  )
}
