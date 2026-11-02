import { ArrowRight } from "lucide-react"
import type { Incident, Sample, Snapshot } from "@/lib/api"
import { bucket } from "@/lib/bucket"
import { LIVE_WINDOW_MS } from "@/lib/live"
import { href, onLinkClick } from "@/lib/router"
import { ChartNote, readingNote, TimeChart } from "./charts"

/**
 * The last hour on one chart: every hop's round-trip time, the blamed hop bold, incidents shaded.
 * It answers "is it OK, whose fault, since when" and has no controls; its link opens History on
 * the same window for anything deeper.
 */
export default function NowChart({ snapshot, samples, incidents, now }: { snapshot: Snapshot; samples: Record<string, Sample[]>; incidents: Incident[]; now: number }) {
  const hops = snapshot.hops.filter((h) => h.kind !== "host")
  const all = Object.values(samples).flat()
  const to = now
  const from = to - LIVE_WINDOW_MS
  const { data, step } = bucket(all, from, to, { points: 120 })
  const blame = snapshot.verdict.blame
  const history = href({ name: "history", query: { range: "1h", focus: blame ?? undefined } })
  return (
    <section aria-label="Last hour" className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-medium text-muted-foreground">
          Last hour <span className="font-normal">· round-trip time</span>
        </h2>
        <a href={history} onClick={onLinkClick} className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground hover:underline">
          Investigate in History <ArrowRight aria-hidden className="size-3.5" />
        </a>
      </div>
      {all.length === 0 ? (
        <p className="py-10 text-center text-sm text-muted-foreground">Collecting the first measurements…</p>
      ) : (
        <TimeChart data={data} hops={hops} metric="rtt" from={from} to={to} step={step} incidents={incidents} blame={blame} className="aspect-auto h-56 w-full" label="Round-trip time of every hop over the last hour" />
      )}
      <ChartNote items={readingNote({ step, scale: "log" })} />
    </section>
  )
}
