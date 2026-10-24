import { ArrowRight, Siren } from "lucide-react"
import { Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { SnapshotView } from "@/components/snapshot"
import { SpeedChart } from "@/components/charts"
import { RunSpeedButton, SpeedCompact, useSpeed } from "@/components/speed"
import { SeverityPill } from "@/components/status"
import { RelTime, useNow } from "@/components/time"
import { useLive } from "@/lib/live"
import { findHop } from "@/lib/targets"
import type { Target } from "@/lib/api"
import { Badge } from "@/components/ui/badge"
import { StatusIcon } from "@/components/status"
import { ms } from "@/lib/format"
import { duration } from "@/lib/format"
import { href, onLinkClick } from "@/lib/router"

export function NowView() {
  const snapshot = useLive((s) => s.snapshot)
  const samples = useLive((s) => s.samples)
  const incidents = useLive((s) => s.incidents)
  const now = useNow(2000)
  if (!snapshot) return <NowSkeleton />
  const open = incidents.find((i) => i.id === snapshot.open_incident)
  return (
    <div className="flex flex-col gap-4">
      <SnapshotView snapshot={snapshot} since={open?.started} samples={samples} now={now} />
      <TargetsCard targets={snapshot.targets ?? []} />
      <div className="grid gap-4 lg:grid-cols-2">
        <SpeedCard />
        <RecentIncidents />
      </div>
    </div>
  )
}

/** Your own targets (`[[targets]]`), each linking to its page. */
function TargetsCard({ targets }: { targets: Target[] }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Targets</CardTitle>
        <CardAction>
          <a href={href({ name: "settings" })} onClick={onLinkClick} className="text-sm text-muted-foreground hover:underline">
            Configure
          </a>
        </CardAction>
      </CardHeader>
      <CardContent>
        {targets.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No targets yet. Add a <code className="font-mono">[[targets]]</code> entry to <code className="font-mono">wirewolf.toml</code> to watch a NAS, a router page, or a Pi-hole.
          </p>
        ) : (
          <ul className="-mx-2 grid gap-1 sm:grid-cols-2 xl:grid-cols-4">
            {targets.map((t) => (
              <li key={t.id}>
                <a
                  href={href({ name: "hop", id: t.id })}
                  onClick={onLinkClick}
                  className="flex items-start gap-3 rounded-lg px-2 py-2 outline-none hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring"
                >
                  <StatusIcon status={t.status} className="mt-0.5 shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 text-sm font-medium">
                      <span className="truncate">{t.name}</span>
                      {t.critical && <Badge variant="outline">critical</Badge>}
                      <span className="ml-auto text-xs text-muted-foreground tabular-nums">{ms(t.rtt_ms)}</span>
                    </div>
                    <div className="truncate font-mono text-xs text-muted-foreground">{t.check}</div>
                    <div className="truncate text-xs text-muted-foreground">{t.detail}</div>
                  </div>
                </a>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  )
}

function SpeedCard() {
  const { runs, running, latest } = useSpeed()
  const now = useNow(60_000)
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          <a href={href({ name: "speed" })} onClick={onLinkClick} className="hover:underline">
            Speed test
          </a>
        </CardTitle>
        <CardAction>
          <RunSpeedButton running={!!running} />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-4">
        {running || latest ? (
          <>
            <SpeedCompact run={running ?? latest} running={!!running} />
            <SpeedChart runs={runs} from={now - 24 * 3600e3} to={now} className="aspect-auto min-h-28 w-full flex-1" />
          </>
        ) : (
          <p className="py-4 text-sm text-muted-foreground">No tests yet. Run one to measure throughput and bufferbloat.</p>
        )}
      </CardContent>
    </Card>
  )
}

function RecentIncidents() {
  const incidents = useLive((s) => s.incidents)
  const snapshot = useLive((s) => s.snapshot)
  const now = useNow()
  const recent = incidents.slice(0, 4)
  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent incidents</CardTitle>
      </CardHeader>
      <CardContent>
        {recent.length === 0 ? (
          <div className="flex flex-col items-center gap-2 py-4 text-sm text-muted-foreground">
            <Siren aria-hidden className="size-5" /> No incidents recorded.
          </div>
        ) : (
          <ul className="-mx-2 flex flex-col">
            {recent.map((i) => (
              <li key={i.id}>
                <a
                  href={href({ name: "incident", id: i.id })}
                  onClick={onLinkClick}
                  className="flex items-start gap-3 rounded-lg px-2 py-2 outline-none hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring"
                >
                  <SeverityPill severity={i.severity} className="mt-0.5 shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{i.headline}</div>
                    <div className="text-xs text-muted-foreground">
                      <RelTime at={i.started} /> ·{" "}
                      {i.ended ? duration(i.started, i.ended) : <span className="text-fail">ongoing {duration(i.started, null, now)}</span>}
                      {i.blame && ` · ${findHop(snapshot, i.blame)?.label ?? i.blame}`}
                    </div>
                  </div>
                </a>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
      <CardFooter>
        <a href={href({ name: "incidents" })} onClick={onLinkClick} className="inline-flex items-center gap-1 text-sm text-primary hover:underline">
          All incidents <ArrowRight aria-hidden className="size-3.5" />
        </a>
      </CardFooter>
    </Card>
  )
}

function NowSkeleton() {
  return (
    <div className="flex flex-col gap-4" aria-busy="true" aria-label="Loading">
      <Skeleton className="h-36 w-full rounded-xl" />
      <Skeleton className="h-80 w-full rounded-xl" />
    </div>
  )
}
