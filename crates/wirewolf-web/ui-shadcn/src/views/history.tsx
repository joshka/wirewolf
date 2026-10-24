import { useEffect, useMemo, useState } from "react"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { bucket, hopColor, TimeChart } from "@/components/charts"
import { api, type Sample } from "@/lib/api"
import { useLive } from "@/lib/live"
import { cn } from "@/lib/utils"

const RANGES = { "1h": 3600e3, "6h": 6 * 3600e3, "24h": 24 * 3600e3 } as const
type Range = keyof typeof RANGES

const DEFAULT_HOPS = [
  { id: "gateway", label: "Gateway" },
  { id: "isp", label: "ISP" },
  { id: "internet", label: "Internet" },
  { id: "dns", label: "DNS" },
  { id: "web", label: "Web" },
]

export function HistoryView() {
  const [range, setRange] = useState<Range>("6h")
  const snapshotHops = useLive((s) => s.snapshot?.hops)
  const hops = useMemo(
    () => (snapshotHops ? snapshotHops.filter((h) => h.kind !== "host").map((h) => ({ id: h.id, label: h.label })) : DEFAULT_HOPS),
    [snapshotHops],
  )
  const [hidden, setHidden] = useState<string[]>([])
  const shown = hops.filter((h) => !hidden.includes(h.id))
  const incidents = useLive((s) => s.incidents)
  const speedRuns = useLive((s) => s.speedRuns)

  const [samples, setSamples] = useState<Sample[] | null>(null)
  const [to, setTo] = useState(() => Date.now())
  useEffect(() => {
    let cancel = false
    const load = () => {
      const t = Date.now()
      api.history({ since: new Date(t - RANGES[range]) }).then((s) => {
        if (cancel) return
        setSamples(s)
        setTo(t)
      })
    }
    setSamples(null)
    load()
    const timer = setInterval(load, range === "1h" ? 10e3 : 60e3)
    return () => {
      cancel = true
      clearInterval(timer)
    }
  }, [range])
  const from = to - RANGES[range]
  const data = useMemo(() => (samples ? bucket(samples, from, to, range === "1h" ? 180 : 288) : []), [samples, from, to, range])
  const inRange = incidents.filter((i) => (i.ended ? new Date(i.ended).getTime() : to) >= from)

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">History</h1>
          <p className="text-sm text-muted-foreground">Latency and loss per hop. Shaded bands are incidents; dotted lines are speed tests.</p>
        </div>
        <ToggleGroup
          value={[range]}
          onValueChange={(v) => v[0] && setRange(v[0] as Range)}
          variant="outline"
          aria-label="Time range"
        >
          {(Object.keys(RANGES) as Range[]).map((r) => (
            <ToggleGroupItem key={r} value={r} className="tnum px-3">
              {r}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      <div role="group" aria-label="Hops" className="flex flex-wrap gap-2">
        {hops.map((h) => {
          const on = !hidden.includes(h.id)
          return (
            <button
              key={h.id}
              type="button"
              aria-pressed={on}
              onClick={() => setHidden((x) => (on ? [...x, h.id] : x.filter((id) => id !== h.id)))}
              className={cn(
                "inline-flex items-center gap-2 rounded-full border px-3 py-1 text-sm transition-colors outline-none hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring",
                !on && "text-muted-foreground opacity-60",
              )}
            >
              <span className="size-2.5 rounded-full" style={{ background: on ? hopColor[h.id] : "transparent", border: `2px solid ${hopColor[h.id]}` }} />
              {h.label}
            </button>
          )
        })}
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Latency</CardTitle>
          <CardDescription>Mean round-trip time per interval</CardDescription>
          <CardAction className="text-xs text-muted-foreground">
            {inRange.length} incident{inRange.length === 1 ? "" : "s"} in range
          </CardAction>
        </CardHeader>
        <CardContent className="px-2 sm:px-4">
          {samples ? (
            <TimeChart data={data} hops={shown} metric="rtt" from={from} to={to} incidents={incidents} speedRuns={speedRuns} className="aspect-auto h-64 w-full sm:h-80" />
          ) : (
            <Skeleton className="h-64 w-full sm:h-80" />
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Packet loss</CardTitle>
          <CardDescription>Worst loss per interval</CardDescription>
        </CardHeader>
        <CardContent className="px-2 sm:px-4">
          {samples ? (
            <TimeChart data={data} hops={shown} metric="loss" from={from} to={to} incidents={incidents} className="aspect-auto h-44 w-full" />
          ) : (
            <Skeleton className="h-44 w-full" />
          )}
        </CardContent>
      </Card>
    </div>
  )
}
