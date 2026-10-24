import { useEffect, useMemo, useState } from "react"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api, type Hop, type Sample, type Severity } from "@/lib/api"
import { useLive } from "@/lib/live"
import { ms, pct } from "@/lib/format"
import { bucket, HopChart } from "./charts"
import { hopIcon, IconTile } from "./path"
import { CauseBadge, StatusPill } from "./status"
import { useNow } from "./time"

const HOUR = 3600e3

/**
 * Details for one hop: metrics, a 1h chart, checks, and evidence. Incident snapshots pass `at`
 * to show the hour around the snapshot instead of the live hour.
 */
export function HopSheet({
  hop,
  blamed,
  severity,
  returnFocus,
  onClose,
  at,
}: {
  hop: Hop | null
  blamed: boolean
  severity: Severity
  returnFocus: React.RefObject<HTMLElement | null>
  onClose: () => void
  at?: string
}) {
  // Keep showing the last hop while the sheet animates closed.
  const [shown, setShown] = useState<Hop | null>(null)
  useEffect(() => void (hop && setShown(hop)), [hop])
  const h = hop ?? shown
  return (
    <Sheet open={!!hop} onOpenChange={(o) => !o && onClose()}>
      <SheetContent side="right" finalFocus={returnFocus} className="w-full gap-6 overflow-y-auto p-5 data-[side=right]:sm:max-w-lg">
        {h && <HopDetail hop={h} blamed={blamed} severity={severity} at={at} />}
      </SheetContent>
    </Sheet>
  )
}

function HopDetail({ hop, blamed, severity, at }: { hop: Hop; blamed: boolean; severity: Severity; at?: string }) {
  const now = useNow(5000)
  const anchor = at ? new Date(at).getTime() + HOUR / 4 : null
  const to = anchor ?? now
  const from = to - HOUR
  const [fetched, setFetched] = useState<Sample[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let cancel = false
    setFetched(null)
    setError(null)
    const end = anchor ?? Date.now()
    api
      .history({ hop: hop.id, since: new Date(end - HOUR), until: anchor ? new Date(anchor) : undefined })
      .then((s) => !cancel && setFetched(s))
      .catch((e: Error) => !cancel && setError(e.message))
    return () => {
      cancel = true
    }
  }, [hop.id, anchor])
  const recent = useLive((s) => s.samples[hop.id])
  const incidents = useLive((s) => s.incidents)
  // Live: append samples that arrived after the fetch.
  const samples = useMemo(() => {
    if (!fetched || anchor || !recent) return fetched ?? []
    const last = fetched.length ? fetched[fetched.length - 1].at : ""
    return [...fetched, ...recent.filter((s) => s.at > last)]
  }, [fetched, recent, anchor])
  const data = useMemo(() => bucket(samples, from, to, 180), [samples, from, to])
  const m = hop.metrics

  return (
    <>
      <SheetHeader className="p-0 pr-8">
        <div className="flex items-center gap-3">
          <IconTile icon={hopIcon[hop.kind]} />
          <div className="min-w-0">
            <SheetTitle className="flex flex-wrap items-center gap-2 text-lg">
              {hop.label} <StatusPill status={hop.status} />
              {blamed && <CauseBadge severity={severity} />}
            </SheetTitle>
            <SheetDescription className="truncate font-mono text-xs">{hop.target ?? hop.kind}</SheetDescription>
          </div>
        </div>
        <p className="mt-3 text-sm">{hop.summary}</p>
      </SheetHeader>

      <dl className="grid grid-cols-3 gap-2">
        {[
          ["RTT", ms(m.rtt_ms)],
          ["Jitter", ms(m.jitter_ms)],
          ["Loss", pct(m.loss_pct)],
        ].map(([k, v]) => (
          <div key={k} className="rounded-lg border p-3">
            <dt className="text-xs text-muted-foreground">{k}</dt>
            <dd className="mt-1 text-lg font-semibold tabular-nums">{v}</dd>
          </div>
        ))}
      </dl>

      <section>
        <h3 className="mb-3 font-semibold">Last hour</h3>
        {error ? (
          <p className="text-sm text-fail">Couldn't load history: {error}</p>
        ) : !fetched ? (
          <Skeleton className="h-52 w-full" />
        ) : samples.some((s) => s.rtt_ms != null) ? (
          <HopChart hop={hop} data={data} from={from} to={to} incidents={incidents} />
        ) : (
          <p className="text-sm text-muted-foreground">This hop has no latency series.</p>
        )}
      </section>

      <section>
        <h3 className="mb-3 font-semibold">Checks</h3>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Check</TableHead>
              <TableHead>Status</TableHead>
              <TableHead className="text-right">RTT</TableHead>
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
                <TableCell className="text-right tabular-nums">{ms(c.rtt_ms)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </section>

      {hop.evidence.length > 0 && (
        <section>
          <h3 className="mb-3 font-semibold">Evidence</h3>
          <dl className="divide-y text-sm">
            {hop.evidence.map((e, i) => (
              <div key={`${i}-${e.key}`} className="flex gap-4 py-2">
                <dt className="w-32 shrink-0 text-xs text-muted-foreground">{e.key}</dt>
                <dd className="min-w-0 font-mono text-xs break-words">{e.value}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}
    </>
  )
}
