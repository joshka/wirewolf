import { AppWindow, AtSign, Building2, ChevronRight, Globe, Laptop, Router, type LucideIcon } from "lucide-react"
import { Area, AreaChart, XAxis, YAxis } from "recharts"
import { Card } from "@/components/ui/card"
import { ChartContainer } from "@/components/ui/chart"
import { cn } from "@/lib/utils"
import type { Hop, HopKind, Sample, Severity, Status } from "@/lib/api"
import { SPARK_WINDOW_MS } from "@/lib/live"
import { ms, pct, statusLabel } from "@/lib/format"
import { hopColor } from "./charts"
import { CauseBadge, StatusIcon, statusText } from "./status"

export const hopIcon: Record<HopKind, LucideIcon> = {
  host: Laptop,
  gateway: Router,
  isp: Building2,
  internet: Globe,
  dns: AtSign,
  web: AppWindow,
}

/** The one number that matters for a hop, or its summary if it has none. */
export function keyMetric(hop: Hop): string {
  const m = hop.metrics
  if (hop.status === "skip" || (m.rtt_ms == null && m.loss_pct == null)) return hop.summary
  if ((m.loss_pct ?? 0) > 0) return m.rtt_ms == null ? `${pct(m.loss_pct)} loss` : `${ms(m.rtt_ms)} · ${pct(m.loss_pct)} loss`
  return ms(m.rtt_ms)
}

const dimmed = (s: Status) => s === "skip" || s === "not_applicable"

export function IconTile({ icon: Icon }: { icon: LucideIcon }) {
  return (
    <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
      <Icon aria-hidden className="size-5" />
    </span>
  )
}

/** RTT over the last 15 minutes, in the hop's colour. */
function Sparkline({ hop, samples, now }: { hop: Hop; samples: Sample[]; now: number }) {
  const data = samples.map((s) => ({ t: new Date(s.at).getTime(), rtt: s.rtt_ms }))
  return (
    <ChartContainer
      config={{ rtt: { label: hop.label, color: hopColor[hop.kind] } }}
      className="hidden aspect-auto h-8 min-w-0 flex-1 sm:block"
      aria-label={`${hop.label} round-trip time, last 15 minutes`}
    >
      <AreaChart data={data} margin={{ top: 2, bottom: 0, left: 0, right: 0 }}>
        <XAxis dataKey="t" type="number" domain={[now - SPARK_WINDOW_MS, now]} hide />
        <YAxis domain={[0, "auto"]} hide />
        <Area dataKey="rtt" stroke="var(--color-rtt)" fill="var(--color-rtt)" fillOpacity={0.15} strokeWidth={1.5} isAnimationActive={false} />
      </AreaChart>
    </ChartContainer>
  )
}

/**
 * The path from this machine to the web: one row per hop with its target, a 15-minute RTT
 * sparkline (when samples are given), and its key metric. Rows open the hop drawer.
 */
export function PathDiagram({
  hops,
  blame,
  severity,
  samples,
  now,
  onSelect,
}: {
  hops: Hop[]
  blame: string | null
  severity: Severity
  samples?: Record<string, Sample[]>
  now?: number
  onSelect: (hop: Hop, opener: HTMLElement) => void
}) {
  return (
    <Card className="py-2 sm:py-2">
      <ol aria-label="Network path" className="flex flex-col divide-y">
        {hops.map((hop) => {
          const blamed = hop.id === blame
          return (
            <li key={hop.id}>
              <button
                type="button"
                data-hop={hop.id}
                onClick={(e) => onSelect(hop, e.currentTarget)}
                aria-label={`${hop.label}: ${statusLabel[hop.status]}. ${hop.summary}. Open details`}
                className={cn(
                  "flex w-full items-center gap-3 px-4 py-2.5 text-left transition outline-none hover:bg-accent/50 focus-visible:ring-3 focus-visible:ring-ring focus-visible:ring-inset sm:px-5",
                  blamed && (severity === "down" ? "bg-fail/5" : "bg-warn/5"),
                  dimmed(hop.status) && "opacity-50",
                )}
              >
                <IconTile icon={hopIcon[hop.kind]} />
                <div className="flex w-40 min-w-0 shrink-0 flex-col gap-0.5 sm:w-56">
                  <span className="flex items-center gap-2 text-sm font-medium">
                    {hop.label}
                    {blamed && <CauseBadge severity={severity} />}
                  </span>
                  <span className="truncate font-mono text-xs text-muted-foreground" title={hop.target ?? undefined}>
                    {hop.target ?? "—"}
                  </span>
                </div>
                {samples && now != null && hop.kind !== "host" ? <Sparkline hop={hop} samples={samples[hop.id] ?? []} now={now} /> : <span className="flex-1" />}
                <span
                  className={cn(
                    "w-28 truncate text-right text-sm font-medium tabular-nums sm:w-40",
                    hop.status === "pass" ? "text-foreground" : dimmed(hop.status) ? "text-muted-foreground" : statusText[hop.status],
                  )}
                  title={hop.summary}
                >
                  {keyMetric(hop)}
                </span>
                <StatusIcon status={hop.status} />
                <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground" />
              </button>
            </li>
          )
        })}
      </ol>
    </Card>
  )
}
