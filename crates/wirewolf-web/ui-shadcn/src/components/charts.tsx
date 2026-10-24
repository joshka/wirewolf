import { CartesianGrid, Line, LineChart, ReferenceArea, ReferenceLine, XAxis, YAxis } from "recharts"
import { ChartContainer, ChartTooltip, ChartTooltipContent, type ChartConfig } from "@/components/ui/chart"
import type { Incident, Sample, SpeedRun } from "@/lib/api"
import { clock, ms, pct } from "@/lib/format"

export const hopColor: Record<string, string> = {
  gateway: "var(--chart-1)",
  isp: "var(--chart-2)",
  internet: "var(--chart-3)",
  dns: "var(--chart-4)",
  web: "var(--chart-5)",
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
  speedRuns?: SpeedRun[]
  className?: string
}

/** RTT or loss over time for several hops, with incident bands and speed-test markers. */
export function TimeChart({ data, hops, metric, from, to, incidents = [], speedRuns = [], className }: Props) {
  const config: ChartConfig = Object.fromEntries(
    hops.map((h) => [`${h.id}_${metric}`, { label: h.label, color: hopColor[h.id] ?? "var(--chart-1)" }]),
  )
  const fmt = metric === "rtt" ? ms : pct
  const span = to - from
  const tick = (t: number) =>
    span > 2 * 86400e3
      ? new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" })
      : clock(t)
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
        {incidents.map((inc) => {
          const x1 = Math.max(from, new Date(inc.started).getTime())
          const x2 = Math.min(to, inc.ended ? new Date(inc.ended).getTime() : Date.now())
          if (x2 < from || x1 > to) return null
          return (
            <ReferenceArea
              key={inc.id}
              x1={x1}
              x2={Math.max(x2, x1 + span / 300)}
              ifOverflow="hidden"
              fill={inc.severity === "down" ? "var(--fail)" : "var(--warn)"}
              fillOpacity={0.2}
              strokeOpacity={0}
            />
          )
        })}
        {speedRuns.map((r) => {
          const x = new Date(r.started).getTime()
          if (x < from || x > to) return null
          return (
            <ReferenceLine
              key={r.id}
              x={x}
              stroke="var(--muted-foreground)"
              strokeOpacity={0.22}
              strokeDasharray="2 4"
              ifOverflow="hidden"
            />
          )
        })}
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
            type="monotone"
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
