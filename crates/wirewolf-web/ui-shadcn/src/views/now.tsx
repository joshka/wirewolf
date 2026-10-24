import { ArrowRight, Siren } from "lucide-react"
import { Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { SnapshotView } from "@/components/snapshot"
import { RunSpeedButton, SpeedCompact, useSpeed } from "@/components/speed"
import { SeverityPill } from "@/components/status"
import { RelTime, useNow } from "@/components/time"
import { useLive } from "@/lib/live"
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
      <SnapshotView snapshot={snapshot} since={open?.started} live samples={samples} now={now} />
      <div className="grid items-start gap-4 lg:grid-cols-2">
        <SpeedCard />
        <RecentIncidents />
      </div>
    </div>
  )
}

function SpeedCard() {
  const { running, latest } = useSpeed()
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
      <CardContent>
        {running || latest ? (
          <SpeedCompact run={running ?? latest} running={!!running} />
        ) : (
          <p className="py-4 text-sm text-muted-foreground">No tests yet. Run one to measure throughput and bufferbloat.</p>
        )}
      </CardContent>
    </Card>
  )
}

function RecentIncidents() {
  const incidents = useLive((s) => s.incidents)
  const hops = useLive((s) => s.snapshot?.hops)
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
                      {i.blame && ` · ${hops?.find((h) => h.id === i.blame)?.label ?? i.blame}`}
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
