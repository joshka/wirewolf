import { ArrowRight, Siren } from "lucide-react"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { SnapshotView } from "@/components/snapshot"
import { BloatBadge, RunSpeedButton, SpeedMeta, SpeedNumbers, SpeedProgress, useSpeed } from "@/components/speed"
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
    <div className="flex flex-col gap-6">
      <SnapshotView snapshot={snapshot} since={open?.started} samples={samples} now={now} live />
      <div className="grid gap-4 md:grid-cols-2">
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
        <CardTitle>Speed</CardTitle>
        <CardDescription>{running ? "Test in progress" : latest ? <SpeedMeta run={latest} /> : "No tests yet"}</CardDescription>
        <CardAction>
          <RunSpeedButton running={!!running} />
        </CardAction>
      </CardHeader>
      <CardContent className="space-y-4">
        <SpeedNumbers run={running ?? latest} />
        {running ? (
          <SpeedProgress run={running} />
        ) : (
          latest && (
            <div className="flex items-center justify-between border-t pt-3 text-sm">
              <span className="text-muted-foreground">Bufferbloat</span>
              <BloatBadge grade={latest.bloat} />
            </div>
          )
        )}
        <a href={href({ name: "speed" })} onClick={onLinkClick} className="inline-flex items-center gap-1 text-sm text-brand hover:underline">
          Speed history <ArrowRight aria-hidden className="size-3.5" />
        </a>
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
        <CardDescription>{incidents.length ? `${incidents.length} in history` : "Nothing recorded"}</CardDescription>
      </CardHeader>
      <CardContent>
        {recent.length === 0 ? (
          <div className="flex flex-col items-center gap-2 py-6 text-sm text-muted-foreground">
            <Siren aria-hidden className="size-5" /> No incidents. Nice.
          </div>
        ) : (
          <ul className="-mx-2 flex flex-col">
            {recent.map((i) => (
              <li key={i.id}>
                <a
                  href={href({ name: "incident", id: i.id })}
                  onClick={onLinkClick}
                  className="flex items-center gap-3 rounded-lg px-2 py-2 outline-none hover:bg-accent focus-visible:ring-3 focus-visible:ring-ring"
                >
                  <SeverityPill severity={i.severity} className="shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{i.headline}</div>
                    <div className="text-xs text-muted-foreground">
                      <RelTime at={i.started} plain /> · {i.ended ? duration(i.started, i.ended) : <span className="text-fail">ongoing {duration(i.started, null, now)}</span>}
                      {i.blame && ` · ${hops?.find((h) => h.id === i.blame)?.label ?? i.blame}`}
                    </div>
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

function NowSkeleton() {
  return (
    <div className="flex flex-col gap-4" aria-busy="true" aria-label="Loading">
      <Skeleton className="h-36 w-full rounded-xl" />
      <div className="grid gap-2 xl:grid-cols-6">
        {Array.from({ length: 6 }, (_, i) => (
          <Skeleton key={i} className="h-16 rounded-xl xl:h-40" />
        ))}
      </div>
    </div>
  )
}
