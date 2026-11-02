import { useEffect, useMemo, useRef, useState } from "react"
import { ChevronLeft, ChevronRight, Radio } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { bucket, ChartNote, hopColor, readingNote, SpeedChart, TimeChart, type Agg, type Scale } from "@/components/charts"
import { EventsLane, eventMarkers } from "@/components/events-lane"
import { labelGutter } from "@/components/time-series"
import { CsvLink, PageHeader } from "@/components/page-header"
import { api, type Hop, type Sample } from "@/lib/api"
import { stats, stepLabel } from "@/lib/bucket"
import { ms, pct } from "@/lib/format"
import { useLive } from "@/lib/live"
import { href, navigate, type HistoryQuery } from "@/lib/router"
import { cn } from "@/lib/utils"

export const RANGES = { "15m": 15 * 60e3, "1h": 3600e3, "6h": 6 * 3600e3, "24h": 24 * 3600e3, "2d": 48 * 3600e3 } as const
export type Range = keyof typeof RANGES
const AGGS: Agg[] = ["avg", "p95", "max"]
const SYNC = "history"

/** The History view is a function of its URL, so any view can be linked to and shared. */
function readQuery(q: HistoryQuery) {
  const range = (q.range && q.range in RANGES ? q.range : "6h") as Range
  const from = Number(q.from)
  const to = Number(q.to)
  return {
    range,
    zoom: from > 0 && to > from ? { from, to } : null,
    scale: (q.scale === "linear" ? "linear" : "log") as Scale,
    agg: AGGS.includes(q.agg as Agg) ? (q.agg as Agg) : "avg",
    shown: q.hops ? q.hops.split(",").filter(Boolean) : null,
    focus: q.focus ?? null,
  }
}

const TIME = new Intl.DateTimeFormat(undefined, { weekday: "short", hour: "2-digit", minute: "2-digit" })

/**
 * The investigation tool: "what happened, how often, and can I prove it?". Range presets,
 * drag to zoom, avg/p95/max, log or linear, a shared crosshair across the stacked charts, a
 * stats table that doubles as the legend, and an events lane. All of it lives in the URL.
 */
export function HistoryView({ query = {} }: { query?: HistoryQuery }) {
  const view = readQuery(query)
  const set = (patch: HistoryQuery) => navigate(href({ name: "history", query: { ...query, ...patch } }), { replace: true })

  const allHops = useLive((s) => s.snapshot?.hops)
  const hops = useMemo(() => (allHops ?? []).filter((h) => h.kind !== "host"), [allHops])
  const shownIds = view.shown ?? hops.map((h) => h.id)
  const shown = hops.filter((h) => shownIds.includes(h.id))
  const incidents = useLive((s) => s.incidents)
  const speedRuns = useLive((s) => s.speedRuns)
  const activity = useLive((s) => s.activity)
  const [hover, setHover] = useState<string | null>(null)

  // Live follows the clock; a zoom pins the window until "Back to live".
  const live = !view.zoom
  const [clock, setClock] = useState(() => Date.now())
  useEffect(() => {
    if (!live) return
    const timer = setInterval(() => setClock(Date.now()), view.range === "15m" ? 5e3 : view.range === "1h" ? 10e3 : 60e3)
    return () => clearInterval(timer)
  }, [live, view.range])
  const to = view.zoom?.to ?? clock
  const from = view.zoom?.from ?? to - RANGES[view.range]

  // Samples are fetched for the window and kept while a reload runs, so the charts never
  // flash to a skeleton: they hold at reduced opacity until the new data lands.
  const [loaded, setLoaded] = useState<{ since: number; samples: Sample[] } | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const inflight = useRef(0)
  const needSince = Math.floor(from / 60e3) * 60e3
  // A live view refetches on every clock tick; a pinned one only when its window moves.
  const tick = live ? clock : 0
  useEffect(() => {
    const id = ++inflight.current
    setLoading(true)
    api
      .history({ since: new Date(needSince) })
      .then((samples) => {
        if (id !== inflight.current) return
        setLoaded({ since: needSince, samples })
        setError(null)
      })
      .catch((e: Error) => id === inflight.current && setError(e.message))
      .finally(() => id === inflight.current && setLoading(false))
  }, [needSince, tick])

  const samples = loaded?.samples ?? null
  const inWindow = useMemo(() => (samples ?? []).filter((s) => {
    const t = new Date(s.at).getTime()
    return t >= from && t <= to
  }), [samples, from, to])
  const { data, step } = useMemo(() => bucket(inWindow, from, to, { agg: view.agg }), [inWindow, from, to, view.agg])
  const gutter = labelGutter(hops.map((h) => h.label))
  const markers = useMemo(() => eventMarkers(incidents, speedRuns, activity, from, to), [incidents, speedRuns, activity, from, to])

  const zoomTo = (a: number, b: number) => set({ from: String(Math.round(a)), to: String(Math.round(b)) })
  const pan = (dir: -1 | 1) => zoomTo(from + dir * (to - from) * 0.5, to + dir * (to - from) * 0.5)
  const backToLive = () => set({ from: undefined, to: undefined })
  useEffect(() => {
    if (live) return
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement | null)?.closest("input, textarea, select, [contenteditable]")) return
      if (e.key === "ArrowLeft") pan(-1)
      else if (e.key === "ArrowRight") pan(1)
      else if (e.key === "Escape") backToLive()
      else return
      e.preventDefault()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  })

  const focus = hover ?? view.focus
  const body = (node: React.ReactNode, height: string) =>
    error ? <p className="py-10 text-center text-sm text-fail">Couldn't load history: {error}</p> : samples && hops.length ? <div className={cn("transition-opacity", loading && "opacity-60")}>{node}</div> : <Skeleton className={cn("w-full", height)} />

  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="History" description="What happened, how often, and the evidence: round-trip time, packet loss, speed tests, and events on one time axis. Drag on a chart to zoom in.">
        <CsvLink what="history" query={`since=${new Date(from).toISOString()}&until=${new Date(to).toISOString()}`} />
      </PageHeader>

      {/* One filter row scopes everything below it. */}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <ToggleGroup variant="segmented" value={live ? [view.range] : []} onValueChange={(v) => v[0] && set({ range: v[0], from: undefined, to: undefined })} aria-label="Time range">
          {(Object.keys(RANGES) as Range[]).map((r) => (
            <ToggleGroupItem key={r} value={r} className="px-3">
              {r}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {live ? (
          <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
            <Radio aria-hidden className="size-3.5" /> live
          </span>
        ) : (
          <div className="flex items-center gap-1">
            <Button variant="ghost" size="icon-sm" onClick={() => pan(-1)} aria-label="Earlier">
              <ChevronLeft aria-hidden />
            </Button>
            <span className="text-xs text-muted-foreground tabular-nums">
              {TIME.format(from)} – {TIME.format(to)}
            </span>
            <Button variant="ghost" size="icon-sm" onClick={() => pan(1)} aria-label="Later">
              <ChevronRight aria-hidden />
            </Button>
            <Button variant="outline" size="sm" onClick={backToLive}>
              Back to live
            </Button>
          </div>
        )}
        <div className="ml-auto flex flex-wrap items-center gap-2">
          <ToggleGroup variant="segmented" value={[view.agg]} onValueChange={(v) => v[0] && set({ agg: v[0] })} aria-label="Summary per bucket">
            {AGGS.map((a) => (
              <ToggleGroupItem key={a} value={a} className="px-3">
                {a}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <ToggleGroup variant="segmented" value={[view.scale]} onValueChange={(v) => v[0] && set({ scale: v[0] })} aria-label="Round-trip scale">
            <ToggleGroupItem value="log" className="px-3">
              log
            </ToggleGroupItem>
            <ToggleGroupItem value="linear" className="px-3">
              linear
            </ToggleGroupItem>
          </ToggleGroup>
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Round-trip time</CardTitle>
          <CardDescription>Shaded bands are incidents · the dashed line is the 100 ms limit · a break in a line is a hop that stopped answering</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 px-2 sm:px-4">
          {body(
            <>
              <TimeChart data={data} hops={shown} metric="rtt" from={from} to={to} step={step} aggregation={view.agg} scale={view.scale} incidents={incidents} focus={focus} syncId={SYNC} onSelect={zoomTo} gutter={gutter} className="aspect-auto h-72 w-full" />
              <ChartNote items={readingNote({ step, aggregation: view.agg, scale: view.scale })} className="px-2" />
              <SeriesTable hops={hops} shown={shownIds} samples={inWindow} onHover={setHover} onShown={(ids) => set({ hops: ids.length === hops.length ? undefined : ids.join(",") })} />
            </>,
            "h-96",
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Packet loss</CardTitle>
          <CardDescription>Worst per {stepLabel(step || 60e3)} · only hops that lost packets are drawn</CardDescription>
        </CardHeader>
        <CardContent className="px-2 sm:px-4">{body(<TimeChart data={data} hops={shown} metric="loss" from={from} to={to} step={step} incidents={incidents} focus={focus} syncId={SYNC} onSelect={zoomTo} gutter={gutter} className="aspect-auto h-40 w-full" />, "h-40")}</CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Speed tests</CardTitle>
          <CardDescription>Download and upload per test · dashed lines are the plan you pay for</CardDescription>
        </CardHeader>
        <CardContent className="px-2 sm:px-4">{body(<SpeedChart runs={speedRuns} from={from} to={to} incidents={incidents} syncId={SYNC} gutter={gutter} className="aspect-auto h-48 w-full" />, "h-48")}</CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Events</CardTitle>
          <CardDescription>Incidents, speed tests, and settings changes on the same axis · hover one to read it</CardDescription>
        </CardHeader>
        <CardContent className="px-2 sm:px-4">{body(<EventsLane markers={markers} from={from} to={to} gutter={gutter} syncId={SYNC} />, "h-16")}</CardContent>
      </Card>
    </div>
  )
}

/**
 * The legend with numbers: one row per hop with last, average, p95, worst and loss over the
 * window. Hover a row to pick its line out, tick it to show or hide it, "only" to see it alone.
 */
function SeriesTable({ hops, shown, samples, onHover, onShown }: { hops: Hop[]; shown: string[]; samples: Sample[]; onHover: (id: string | null) => void; onShown: (ids: string[]) => void }) {
  const toggle = (id: string) => onShown(shown.includes(id) ? shown.filter((x) => x !== id) : hops.filter((h) => h.id === id || shown.includes(h.id)).map((h) => h.id))
  const num = "text-right tabular-nums"
  return (
    <Table className="text-sm">
      <TableHeader>
        <TableRow>
          <TableHead>Hop</TableHead>
          <TableHead className={num}>last</TableHead>
          <TableHead className={num}>avg</TableHead>
          <TableHead className={num}>p95</TableHead>
          <TableHead className={num}>worst</TableHead>
          <TableHead className={num}>loss</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {hops.map((h) => {
          const on = shown.includes(h.id)
          const s = stats(samples, h.id)
          return (
            <TableRow key={h.id} className={cn("group", !on && "text-muted-foreground")} onMouseEnter={() => on && onHover(h.id)} onMouseLeave={() => onHover(null)}>
              <TableCell className="py-1.5">
                <div className="flex items-center gap-3">
                  <label className="flex cursor-pointer items-center gap-2">
                    <input type="checkbox" checked={on} onChange={() => toggle(h.id)} className="sr-only" aria-label={`Show ${h.label}`} />
                    <span aria-hidden className="size-2.5 rounded-full ring-2 ring-card" style={{ background: on ? hopColor[h.kind] : "var(--border)" }} />
                    <span className={cn(on ? "font-medium" : "line-through decoration-border")}>{h.label}</span>
                  </label>
                  <button type="button" onClick={() => onShown([h.id])} onFocus={() => onHover(h.id)} onBlur={() => onHover(null)} className="text-xs text-muted-foreground opacity-0 transition hover:text-foreground hover:underline focus-visible:opacity-100 group-hover:opacity-100">
                    only
                  </button>
                </div>
              </TableCell>
              <TableCell className={num}>{ms(s.last)}</TableCell>
              <TableCell className={num}>{ms(s.avg)}</TableCell>
              <TableCell className={num}>{ms(s.p95)}</TableCell>
              <TableCell className={num}>{ms(s.max)}</TableCell>
              <TableCell className={cn(num, (s.loss ?? 0) > 0 && on && "text-fail")}>{pct(s.loss)}</TableCell>
            </TableRow>
          )
        })}
      </TableBody>
    </Table>
  )
}
