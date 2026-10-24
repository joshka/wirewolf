import { Area, AreaChart, XAxis, YAxis } from "recharts"
import { ChartContainer } from "@/components/ui/chart"
import type { Hop, Sample } from "@/lib/api"
import { sparkSeries } from "@/lib/bucket"
import { hopColor } from "@/lib/colors"
import { SPARK_WINDOW_MS } from "@/lib/live"
import { cn } from "@/lib/utils"
import { dimmed } from "./path"

/** Fewer samples than this can't show a trend, so the sparkline says it's still collecting. */
const MIN_SPARK_POINTS = 3

/**
 * RTT over the last 15 minutes, in the hop's colour. A hop that isn't being measured right now
 * (skipped) draws its old line grey and dashed, so stale data can't pass for healthy.
 */
export default function Sparkline({ hop, samples, now, className }: { hop: Hop; samples: Sample[]; now: number; className?: string }) {
  const { data } = sparkSeries(samples)
  if (samples.filter((s) => s.rtt_ms != null).length < MIN_SPARK_POINTS) {
    return <span className={cn("self-center text-xs text-muted-foreground", className)}>collecting…</span>
  }
  const stale = dimmed(hop.status)
  // Until there's a full window of samples (e.g. just after a restart), stretch what there is
  // across the width instead of leaving most of the line empty.
  const first = data.find((p) => p.rtt != null)?.t ?? now - SPARK_WINDOW_MS
  const from = Math.max(now - SPARK_WINDOW_MS, first)
  return (
    <ChartContainer
      config={{ rtt: { label: hop.label, color: stale ? "var(--muted-foreground)" : hopColor[hop.kind] } }}
      className={cn("aspect-auto h-8 min-w-0", className)}
      aria-label={`${hop.label} round-trip time, last 15 minutes`}
    >
      <AreaChart data={data} margin={{ top: 2, bottom: 0, left: 0, right: 0 }} accessibilityLayer={false}>
        <XAxis dataKey="t" type="number" domain={[from, now]} hide />
        <YAxis domain={[0, "auto"]} hide />
        <Area
          dataKey="rtt"
          stroke="var(--color-rtt)"
          fill="var(--color-rtt)"
          fillOpacity={stale ? 0.05 : 0.15}
          strokeWidth={1.5}
          strokeDasharray={stale ? "3 3" : undefined}
          isAnimationActive={false}
        />
      </AreaChart>
    </ChartContainer>
  )
}
