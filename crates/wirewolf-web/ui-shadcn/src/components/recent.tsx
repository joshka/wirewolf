import { Area, AreaChart, XAxis, YAxis } from "recharts"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { ChartContainer } from "@/components/ui/chart"
import type { Hop, Sample } from "@/lib/api"
import { SPARK_WINDOW_MS } from "@/lib/live"
import { ms, pct } from "@/lib/format"
import { cn } from "@/lib/utils"
import { hopColor } from "./charts"
import { statusText } from "./status"

/** "Last 15 minutes": an RTT sparkline per measured hop, in the hop colors. */
export function RecentPanel({ hops, samples, now }: { hops: Hop[]; samples: Record<string, Sample[]>; now: number }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Last 15 minutes</CardTitle>
        <CardDescription>Round-trip time per hop</CardDescription>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col divide-y">
          {hops
            .filter((h) => h.kind !== "host")
            .map((hop) => {
              const data = (samples[hop.id] ?? []).map((s) => ({ t: new Date(s.at).getTime(), rtt: s.rtt_ms }))
              const loss = hop.metrics.loss_pct ?? 0
              return (
                <li key={hop.id} className="flex items-center gap-3 py-2 first:pt-0 last:pb-0">
                  <span className="w-20 shrink-0 text-sm">{hop.label}</span>
                  <ChartContainer config={{ rtt: { label: hop.label, color: hopColor[hop.kind] } }} className="aspect-auto h-8 min-w-0 flex-1">
                    <AreaChart data={data} margin={{ top: 2, bottom: 0, left: 0, right: 0 }}>
                      <XAxis dataKey="t" type="number" domain={[now - SPARK_WINDOW_MS, now]} hide />
                      <YAxis domain={[0, "auto"]} hide />
                      <Area dataKey="rtt" stroke="var(--color-rtt)" fill="var(--color-rtt)" fillOpacity={0.15} strokeWidth={1.5} isAnimationActive={false} />
                    </AreaChart>
                  </ChartContainer>
                  <span className={cn("w-32 shrink-0 truncate text-right text-sm tabular-nums", hop.status !== "pass" && statusText[hop.status])}>
                    {hop.status === "fail" ? "no reply" : hop.status === "skip" ? "skipped" : ms(hop.metrics.rtt_ms)}
                    {hop.status !== "fail" && loss > 0 && ` · ${pct(loss)} loss`}
                  </span>
                </li>
              )
            })}
        </ul>
      </CardContent>
    </Card>
  )
}
