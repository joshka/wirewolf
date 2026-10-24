import { useEffect, useMemo, useState } from "react"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Skeleton } from "@/components/ui/skeleton"
import { api, type Hop, type Sample, type Severity } from "@/lib/api"
import { useLive } from "@/lib/live"
import { ms, pct } from "@/lib/format"
import { cn } from "@/lib/utils"
import { bucket, HopChart } from "./charts"
import { BlameTag, hopIcon } from "./path"
import { StatusPill } from "./status"
import { useNow } from "./time"

const HOUR = 3600e3

/**
 * Details for one hop: metrics, a 1h chart, checks, and raw evidence. `live` adds incoming
 * samples to the chart; incident snapshots pass `live={false}` and an `at` to anchor the hour.
 */
export function HopSheet({
  hop,
  blamed,
  severity,
  returnFocus,
  onClose,
  at,
  live = true,
}: {
  hop: Hop | null
  blamed: boolean
  severity: Severity
  returnFocus: React.RefObject<HTMLElement | null>
  onClose: () => void
  at?: string
  live?: boolean
}) {
  const [open, setOpen] = useState(false)
  const [shown, setShown] = useState<Hop | null>(null)
  useEffect(() => {
    if (hop) {
      setShown(hop)
      setOpen(true)
    } else setOpen(false)
  }, [hop])
  const h = hop ?? shown
  return (
    <Sheet open={open} onOpenChange={(o) => !o && onClose()}>
      <SheetContent
        side="right"
        finalFocus={returnFocus}
        className="w-full gap-0 overflow-y-auto data-[side=right]:w-full data-[side=right]:sm:max-w-[520px]"
      >
        {h && <HopDetail hop={h} blamed={blamed} severity={severity} at={at} live={live} />}
      </SheetContent>
    </Sheet>
  )
}

function HopDetail({ hop, blamed, severity, at, live }: { hop: Hop; blamed: boolean; severity: Severity; at?: string; live: boolean }) {
  const Icon = hopIcon[hop.kind]
  const now = useNow(5000)
  const anchor = at ? new Date(at).getTime() + HOUR / 4 : null
  const to = anchor ?? now
  const from = to - HOUR
  const [fetched, setFetched] = useState<Sample[] | null>(null)
  useEffect(() => {
    let cancel = false
    setFetched(null)
    const until = anchor ? new Date(anchor) : undefined
    api.history({ hop: hop.id, since: new Date((anchor ?? Date.now()) - HOUR), until }).then((s) => !cancel && setFetched(s))
    return () => {
      cancel = true
    }
  }, [hop.id, anchor])
  const recent = useLive((s) => s.samples[hop.id])
  const incidents = useLive((s) => s.incidents)
  const samples = useMemo(() => {
    if (!fetched) return []
    if (!live || !recent) return fetched
    const last = fetched.length ? new Date(fetched[fetched.length - 1].at).getTime() : 0
    return [...fetched, ...recent.filter((s) => new Date(s.at).getTime() > last)]
  }, [fetched, recent, live])
  const data = useMemo(() => bucket(samples, from, to, 180), [samples, from, to])
  const hasRtt = samples.some((s) => s.rtt_ms != null)
  const m = hop.metrics
  return (
    <>
      <SheetHeader className="border-b p-5 pr-12">
        <div className="flex items-center gap-3">
          <span className="grid size-10 place-items-center rounded-lg bg-muted text-muted-foreground">
            <Icon aria-hidden className="size-5" />
          </span>
          <div className="min-w-0">
            <SheetTitle className="flex flex-wrap items-center gap-2 text-lg">
              {hop.label} <StatusPill status={hop.status} />
              {blamed && <BlameTag severity={severity} />}
            </SheetTitle>
            <SheetDescription className="truncate font-mono text-xs">{hop.target ?? hop.kind}</SheetDescription>
          </div>
        </div>
        <p className="mt-3 text-sm">{hop.summary}</p>
      </SheetHeader>

      <div className="flex flex-col gap-6 p-5">
        <dl className="grid grid-cols-3 gap-2">
          {[
            ["RTT", ms(m.rtt_ms)],
            ["Jitter", ms(m.jitter_ms)],
            ["Loss", pct(m.loss_pct)],
          ].map(([k, v]) => (
            <div key={k} className="rounded-lg border bg-muted/30 p-3">
              <dt className="text-xs text-muted-foreground">{k}</dt>
              <dd className="tnum mt-0.5 text-lg font-semibold">{v}</dd>
            </div>
          ))}
        </dl>

        <section>
          <h3 className="mb-2 text-sm font-medium">Last hour</h3>
          {!fetched ? (
            <Skeleton className="h-48 w-full" />
          ) : hasRtt ? (
            <HopChart hop={{ id: hop.id, label: hop.label }} data={data} from={from} to={to} incidents={incidents} />
          ) : (
            <p className="rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground">
              This hop has no latency series.
            </p>
          )}
        </section>

        <section>
          <h3 className="mb-2 text-sm font-medium">Checks</h3>
          <div className="rounded-lg border">
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
                      <div className="mt-0.5 text-xs text-muted-foreground">{c.detail}</div>
                    </TableCell>
                    <TableCell>
                      <StatusPill status={c.status} />
                    </TableCell>
                    <TableCell className="tnum text-right">{ms(c.rtt_ms)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </section>

        {hop.evidence.length > 0 && (
          <section>
            <h3 className="mb-2 text-sm font-medium">Evidence</h3>
            <dl className="divide-y rounded-lg border text-sm">
              {hop.evidence.map((e, i) => (
                <div key={`${i}-${e.key}`} className="flex gap-4 px-3 py-2">
                  <dt className="w-32 shrink-0 text-muted-foreground">{e.key}</dt>
                  <dd className={cn("min-w-0 break-words font-mono text-xs leading-5")}>{e.value}</dd>
                </div>
              ))}
            </dl>
          </section>
        )}
      </div>
    </>
  )
}
