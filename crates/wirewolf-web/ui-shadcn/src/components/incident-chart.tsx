import { useEffect, useMemo, useState } from "react"
import { ArrowRight } from "lucide-react"
import { Skeleton } from "@/components/ui/skeleton"
import { useNow } from "@/components/time"
import { api, type Incident, type Sample } from "@/lib/api"
import { bucket } from "@/lib/bucket"
import { href, onLinkClick } from "@/lib/router"
import { ChartNote, readingNote, TimeChart } from "./charts"
import { labelGutter } from "./time-series"

const PAD = 15 * 60e3

/** The incident on the chart: every hop from 15 minutes before it opened to 15 minutes after it closed. */
export default function IncidentChart({ incident }: { incident: Incident }) {
  const hops = incident.evidence.hops.filter((h) => h.kind !== "host")
  // Rounded to the minute, so a still-open incident doesn't refetch on every clock tick.
  const now = Math.floor(useNow(60_000) / 60e3) * 60e3
  const from = new Date(incident.started).getTime() - PAD
  const to = Math.min(now, (incident.ended ? new Date(incident.ended).getTime() : now) + PAD)
  const [samples, setSamples] = useState<Sample[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let cancel = false
    api
      .history({ since: new Date(from), until: new Date(to) })
      .then((s) => !cancel && setSamples(s))
      .catch((e: Error) => !cancel && setError(e.message))
    return () => {
      cancel = true
    }
  }, [from, to])
  const { data, step } = useMemo(() => bucket(samples ?? [], from, to, { points: 240 }), [samples, from, to])
  const gutter = labelGutter(hops.map((h) => h.label))
  const sync = `incident-${incident.id}`
  const history = href({ name: "history", query: { from: String(from), to: String(to), focus: incident.blame ?? undefined } })
  return (
    <section aria-label="The incident over time" className="flex flex-col gap-2">
      <h2 className="text-sm font-medium text-muted-foreground">
        Around the incident <span className="font-normal">· round-trip time and loss, every hop</span>
      </h2>
      {error ? (
        <p className="py-6 text-center text-sm text-fail">Couldn't load history: {error}</p>
      ) : !samples ? (
        <Skeleton className="h-64 w-full" />
      ) : samples.length === 0 ? (
        <p className="py-6 text-center text-sm text-muted-foreground">No samples were kept for this period.</p>
      ) : (
        <>
          <TimeChart data={data} hops={hops} metric="rtt" from={from} to={to} step={step} incidents={[incident]} blame={incident.blame} syncId={sync} gutter={gutter} className="aspect-auto h-56 w-full" />
          <TimeChart data={data} hops={hops} metric="loss" from={from} to={to} step={step} incidents={[incident]} blame={incident.blame} syncId={sync} gutter={gutter} className="aspect-auto h-28 w-full" />
        </>
      )}
      <ChartNote items={readingNote({ step, scale: "log" })}>
        <a href={history} onClick={onLinkClick} className="inline-flex items-center gap-1 hover:text-foreground hover:underline">
          Open in History <ArrowRight aria-hidden className="size-3.5" />
        </a>
      </ChartNote>
    </section>
  )
}
