import { Fragment, useEffect, useMemo, useState } from "react"
import { Breadcrumb, BreadcrumbItem, BreadcrumbLink, BreadcrumbList, BreadcrumbPage, BreadcrumbSeparator } from "@/components/ui/breadcrumb"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { bucket, TimeChart } from "@/components/charts"
import { hopIcon, IconTile } from "@/components/path"
import { CauseBadge, SeverityPill, StatusPill } from "@/components/status"
import { RelTime, useNow } from "@/components/time"
import { api, type Hop, type Incident, type Sample, type Severity } from "@/lib/api"
import { useLive } from "@/lib/live"
import { findHop } from "@/lib/targets"
import { duration, ms, pct } from "@/lib/format"
import { href, onLinkClick, type Route } from "@/lib/router"
import { RANGES, type Range } from "./history"
import { useIncident } from "./incidents"

const HOUR = RANGES["1h"]
// Stat tiles keep one width however many the hop measures.
const TILE_GRID = ["", "grid-cols-1 lg:w-36", "grid-cols-2 lg:w-72", "grid-cols-3 lg:w-md"]

/** Whether the hop measures round trips, unlike the host, which only reports its setup. */
const measuresRtt = (hop: Hop) => [hop.metrics.rtt_ms, hop.metrics.jitter_ms, hop.metrics.loss_pct].some((v) => v != null) || hop.checks.some((c) => c.rtt_ms != null)

/**
 * One hop's page: live at `/path/<hop>`, or as an incident's evidence at
 * `/incidents/<id>/path/<hop>`, where the charts show the hour around the snapshot.
 */
export function HopView({ id, incident: incidentId }: { id: string; incident?: string }) {
  const snapshot = useLive((s) => s.snapshot)
  const incident = useIncident(incidentId)
  const source = incidentId ? incident?.evidence : snapshot
  const hop = findHop(source, id)

  const crumbs: [string, Route][] = incident
    ? [["Incidents", { name: "incidents" }], [incident.headline, { name: "incident", id: incident.id }]]
    : [["Now", { name: "now" }]]
  const breadcrumb = (
    <Breadcrumb>
      <BreadcrumbList>
        {crumbs.map(([label, route]) => (
          <Fragment key={label}>
            <BreadcrumbItem className="max-w-64 truncate">
              <BreadcrumbLink render={<a href={href(route)} onClick={onLinkClick} />}>{label}</BreadcrumbLink>
            </BreadcrumbItem>
            <BreadcrumbSeparator />
          </Fragment>
        ))}
        <BreadcrumbItem>
          <BreadcrumbPage>{hop?.label ?? id}</BreadcrumbPage>
        </BreadcrumbItem>
      </BreadcrumbList>
    </Breadcrumb>
  )

  if (!source)
    return (
      <div className="flex flex-col gap-4">
        {breadcrumb}
        {incident === null ? <p className="text-muted-foreground">Incident “{incidentId}” wasn't found.</p> : <Skeleton className="h-96 w-full rounded-xl" />}
      </div>
    )
  if (!hop)
    return (
      <div className="flex flex-col gap-4">
        {breadcrumb}
        <p className="text-muted-foreground">There's no hop or target “{id}” here.</p>
      </div>
    )

  const blamed = hop.id === source.verdict.blame
  return (
    <div className="flex flex-col gap-5">
      {breadcrumb}
      <Header hop={hop} blamed={blamed} severity={source.verdict.severity} at={incident ? source.at : undefined} />
      <HopCharts hop={hop} at={incident ? source.at : undefined} />
      <div className="grid gap-4 lg:grid-cols-2">
        <Checks hop={hop} />
        <HopIncidents hop={hop} />
      </div>
    </div>
  )
}

function Header({ hop, blamed, severity, at }: { hop: Hop; blamed: boolean; severity: Severity; at?: string }) {
  const m = hop.metrics
  const tiles = (
    [
      ["RTT", m.rtt_ms, ms],
      ["Jitter", m.jitter_ms, ms],
      ["Loss", m.loss_pct, pct],
    ] as const
  )
    .filter(([, v]) => v != null)
    .map(([k, v, f]) => [k, f(v)])
  return (
    <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
      <div className="flex min-w-0 flex-col gap-4">
        <div className="flex items-center gap-3">
          <IconTile icon={hopIcon[hop.kind]} />
          <div className="min-w-0">
            <h1 className="flex flex-wrap items-center gap-2 text-2xl font-semibold tracking-tight">
              {hop.label} <StatusPill status={hop.status} />
              {blamed && <CauseBadge severity={severity} />}
            </h1>
            <p className="truncate font-mono text-xs text-muted-foreground">
              {hop.target ?? hop.kind}
              {at && (
                <>
                  {" · evidence from "}
                  <RelTime at={at} />
                </>
              )}
            </p>
          </div>
        </div>
        <p className="text-sm">{hop.summary}</p>
      </div>
      {tiles.length > 0 && (
        <dl className={`grid shrink-0 gap-2 ${TILE_GRID[tiles.length]}`}>
          {tiles.map(([k, v]) => (
            <div key={k} className="rounded-lg border bg-card p-3">
              <dt className="text-xs text-muted-foreground">{k}</dt>
              <dd className="mt-1 text-lg font-semibold tabular-nums">{v}</dd>
            </div>
          ))}
        </dl>
      )}
    </div>
  )
}

/** RTT and loss over a chosen range, or over the hour around `at` for incident evidence. */
function HopCharts({ hop, at }: { hop: Hop; at?: string }) {
  const [range, setRange] = useState<Range>("1h")
  const span = at ? HOUR : RANGES[range]
  const now = useNow(5000)
  const anchor = at ? new Date(at).getTime() + HOUR / 4 : null
  const to = anchor ?? now
  const from = to - span
  const [fetched, setFetched] = useState<Sample[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let cancel = false
    setFetched(null)
    setError(null)
    const end = anchor ?? Date.now()
    api
      .history({ hop: hop.id, since: new Date(end - span), until: anchor ? new Date(anchor) : undefined })
      .then((s) => !cancel && setFetched(s))
      .catch((e: Error) => !cancel && setError(e.message))
    return () => {
      cancel = true
    }
  }, [hop.id, anchor, span])
  const recent = useLive((s) => s.samples[hop.id])
  const incidents = useLive((s) => s.incidents)
  // Live: append samples that arrived after the fetch.
  const samples = useMemo(() => {
    if (!fetched || anchor || !recent) return fetched ?? []
    const last = fetched.length ? fetched[fetched.length - 1].at : ""
    return [...fetched, ...recent.filter((s) => s.at > last)]
  }, [fetched, recent, anchor])
  const data = useMemo(() => bucket(samples, from, to, 180), [samples, from, to])
  // A hop that measures nothing now gets a chart only if its history does, so a hop
  // skipped during an outage still shows what it measured before.
  const hasRtt = samples.some((s) => s.rtt_ms != null)
  const hasLoss = samples.some((s) => s.loss_pct != null)
  const hasSeries = hasRtt || hasLoss
  if (!measuresRtt(hop) && !hasSeries) return null

  return (
    <Card>
      <CardHeader>
        <CardTitle>{hasRtt && hasLoss ? "Round-trip time and loss" : hasLoss ? "Loss" : "Round-trip time"}</CardTitle>
        <CardDescription>{at ? "The hour around the evidence" : "Shaded bands are incidents"}</CardDescription>
        {!at && (
          <ToggleGroup variant="outline" value={[range]} onValueChange={(v) => v[0] && setRange(v[0] as Range)} aria-label="Time range" className="col-start-2 row-span-2 row-start-1 self-start justify-self-end">
            {(Object.keys(RANGES) as Range[]).map((r) => (
              <ToggleGroupItem key={r} value={r} className="px-3">
                {r}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        )}
      </CardHeader>
      <CardContent className="px-2 sm:px-4">
        {error ? (
          <p className="py-10 text-center text-sm text-fail">Couldn't load history: {error}</p>
        ) : !fetched ? (
          <Skeleton className="h-72 w-full" />
        ) : hasSeries ? (
          <div className="flex flex-col gap-2">
            {hasRtt && <TimeChart data={data} hops={[hop]} metric="rtt" from={from} to={to} incidents={incidents} className="aspect-auto h-48 w-full" />}
            {hasLoss && <TimeChart data={data} hops={[hop]} metric="loss" from={from} to={to} incidents={incidents} className="aspect-auto h-28 w-full" />}
          </div>
        ) : (
          <p className="py-10 text-center text-sm text-muted-foreground">This hop has no latency series.</p>
        )}
      </CardContent>
    </Card>
  )
}

function Checks({ hop }: { hop: Hop }) {
  const rtt = hop.checks.some((c) => c.rtt_ms != null)
  // Hops that don't measure round trips (the host) lead with their facts.
  const lead = !measuresRtt(hop)
  const evidence = hop.evidence.length > 0 && (
    <dl className={lead ? "divide-y text-sm" : "divide-y border-t text-sm"}>
      {hop.evidence.map((e, i) => (
        <div key={`${i}-${e.key}`} className="flex gap-4 py-2">
          <dt className="w-32 shrink-0 text-xs text-muted-foreground">{e.key}</dt>
          <dd className="min-w-0 font-mono text-xs break-words">{e.value}</dd>
        </div>
      ))}
    </dl>
  )
  return (
    <Card>
      <CardHeader>
        <CardTitle>Checks</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {lead && evidence}
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Check</TableHead>
              <TableHead>Status</TableHead>
              {rtt && <TableHead className="text-right">RTT</TableHead>}
            </TableRow>
          </TableHeader>
          <TableBody>
            {hop.checks.map((c, i) => (
              <TableRow key={`${i}-${c.name}`}>
                <TableCell className="whitespace-normal">
                  <div className="font-medium">{c.name}</div>
                  <div className="text-xs text-muted-foreground">{c.detail}</div>
                </TableCell>
                <TableCell>
                  <StatusPill status={c.status} />
                </TableCell>
                {rtt && <TableCell className="text-right tabular-nums">{ms(c.rtt_ms)}</TableCell>}
              </TableRow>
            ))}
          </TableBody>
        </Table>
        {!lead && evidence}
      </CardContent>
    </Card>
  )
}

function HopIncidents({ hop }: { hop: Hop }) {
  const all = useLive((s) => s.incidents)
  const now = useNow()
  const incidents: Incident[] = all.filter((i) => i.blame === hop.id).sort((a, b) => b.started.localeCompare(a.started))
  return (
    <Card>
      <CardHeader>
        <CardTitle>Incidents blamed on {hop.label}</CardTitle>
      </CardHeader>
      <CardContent>
        {incidents.length === 0 ? (
          <p className="text-sm text-muted-foreground">None recorded.</p>
        ) : (
          <ul className="-mx-2 flex flex-col">
            {incidents.map((i) => (
              <li key={i.id}>
                <a href={href({ name: "incident", id: i.id })} onClick={onLinkClick} className="flex items-center gap-3 rounded-md px-2 py-2 hover:bg-accent/50">
                  <SeverityPill severity={i.severity} />
                  <span className="min-w-0 flex-1 truncate text-sm">{i.headline}</span>
                  <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                    <RelTime at={i.started} /> · {duration(i.started, i.ended, now)}
                  </span>
                </a>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  )
}
