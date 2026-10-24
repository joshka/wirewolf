import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import type { Hop, Sample } from "@/lib/api"
import { ms } from "@/lib/format"
import { cn } from "@/lib/utils"
import { hopColor } from "./charts"
import { Sparkline } from "./sparkline"

/** "Last 15 minutes": one sparkline row per measured hop, in the fixed hop colors. */
export function RecentPanel({ hops, samples, now }: { hops: Hop[]; samples: Record<string, Sample[]>; now: number }) {
  const rows = hops.filter((h) => h.kind !== "host")
  return (
    <Card>
      <CardHeader>
        <CardTitle>Last 15 minutes</CardTitle>
        <CardDescription>Round-trip time per hop; red ticks are packet loss</CardDescription>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col divide-y">
          {rows.map((hop) => {
            const list = samples[hop.id] ?? []
            const last = list[list.length - 1]
            const noReply = hop.status === "fail" || (last && last.rtt_ms == null && (last.loss_pct ?? 0) >= 100)
            return (
              <li key={hop.id} className="flex items-center gap-3 py-2.5 first:pt-0 last:pb-0">
                <span className="flex w-20 shrink-0 items-center gap-2 text-sm font-medium">
                  <span aria-hidden className="size-2 rounded-full" style={{ background: hopColor[hop.id] }} />
                  {hop.label}
                </span>
                {list.filter((x) => x.rtt_ms != null).length < 3 ? (
                  <span className="flex h-8 min-w-0 flex-1 items-center border-b border-dashed text-xs text-muted-foreground/70">
                    collecting samples…
                  </span>
                ) : (
                <Sparkline
                  samples={list}
                  now={now}
                  label={`${hop.label} latency, last 15 minutes`}
                  className="h-8 min-w-0 flex-1"
                  color={hopColor[hop.id]}
                />
                )}
                <span className={cn("tnum w-20 shrink-0 text-right text-sm font-medium", noReply && "text-fail", hop.status === "skip" && "text-muted-foreground")}>
                  {hop.status === "skip" ? "skipped" : noReply ? "no reply" : ms(hop.metrics.rtt_ms)}
                </span>
              </li>
            )
          })}
        </ul>
      </CardContent>
    </Card>
  )
}
