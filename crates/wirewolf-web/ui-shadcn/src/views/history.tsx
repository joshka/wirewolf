import { useEffect, useMemo, useState } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { bucket, hopColor, SpeedChart, TimeChart } from "@/components/charts"
import { api, type Sample } from "@/lib/api"
import { useLive } from "@/lib/live"
import { PageHeader } from "@/components/page-header"

export const RANGES = { "1h": 3600e3, "6h": 6 * 3600e3, "24h": 24 * 3600e3 } as const
export type Range = keyof typeof RANGES

export function HistoryView() {
  const [range, setRange] = useState<Range>("6h")
  const allHops = useLive((s) => s.snapshot?.hops)
  const hops = useMemo(() => (allHops ?? []).filter((h) => h.kind !== "host"), [allHops])
  const [hidden, setHidden] = useState<string[]>([])
  const [focus, setFocus] = useState<string | null>(null)
  const shown = hops.filter((h) => !hidden.includes(h.id))
  const incidents = useLive((s) => s.incidents)
  const speedRuns = useLive((s) => s.speedRuns)

  const [samples, setSamples] = useState<Sample[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [to, setTo] = useState(() => Date.now())
  useEffect(() => {
    let cancel = false
    const load = () => {
      const t = Date.now()
      api
        .history({ since: new Date(t - RANGES[range]) })
        .then((s) => {
          if (cancel) return
          setSamples(s)
          setTo(t)
          setError(null)
        })
        .catch((e: Error) => !cancel && setError(e.message))
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
  const data = useMemo(() => (samples ? bucket(samples, from, to) : []), [samples, from, to])

  const chart = (title: string, description: string, body: React.ReactNode) => (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent className="px-2 sm:px-4">
        {error ? <p className="py-10 text-center text-sm text-fail">Couldn't load history: {error}</p> : samples && hops.length ? body : <Skeleton className="h-56 w-full" />}
      </CardContent>
    </Card>
  )

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="History" description="Round-trip time, packet loss, and speed tests over time, with incidents shaded." />
      <div className="flex flex-wrap items-center justify-between gap-3">
        <ToggleGroup
          variant="outline"
          spacing={1}
          className="flex-wrap"
          multiple
          value={shown.map((h) => h.id)}
          onValueChange={(v) => setHidden(hops.filter((h) => !v.includes(h.id)).map((h) => h.id))}
          aria-label="Hops"
        >
          {hops.map((h) => (
            <ToggleGroupItem
              key={h.id}
              value={h.id}
              className="gap-2 px-3"
              onMouseEnter={() => setFocus(h.id)}
              onMouseLeave={() => setFocus(null)}
              onFocus={() => setFocus(h.id)}
              onBlur={() => setFocus(null)}
            >
              <span className="size-2 rounded-full" style={{ background: hopColor[h.kind] }} />
              {h.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <ToggleGroup variant="outline" value={[range]} onValueChange={(v) => v[0] && setRange(v[0] as Range)} aria-label="Time range">
          {(Object.keys(RANGES) as Range[]).map((r) => (
            <ToggleGroupItem key={r} value={r} className="px-3">
              {r}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      {chart("Round-trip time", "Mean per interval · shaded bands are incidents · hover a hop above to pick out its line", <TimeChart data={data} hops={shown} metric="rtt" from={from} to={to} incidents={incidents} focus={focus} />)}
      {chart("Packet loss", "Worst per interval · only hops that lost packets are drawn", <TimeChart data={data} hops={shown} metric="loss" from={from} to={to} incidents={incidents} focus={focus} className="aspect-auto h-44 w-full" />)}
      {chart("Speed tests", "Download and upload per test", <SpeedChart runs={speedRuns} from={from} to={to} incidents={incidents} />)}
    </div>
  )
}
