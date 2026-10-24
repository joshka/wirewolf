import { useRef, useState } from "react"
import type { Hop, Sample, Snapshot } from "@/lib/api"
import { HopSheet } from "./hop-sheet"
import { PathDiagram } from "./path"
import { VerdictBanner } from "./verdict"

/** Verdict plus path, with the hop drawer. Shared by Now and the incident detail. */
export function SnapshotView({
  snapshot,
  since,
  live,
  samples,
  now,
}: {
  snapshot: Snapshot
  since?: string | null
  live: boolean
  samples?: Record<string, Sample[]>
  now?: number
}) {
  const [selected, setSelected] = useState<string | null>(null)
  const opener = useRef<HTMLElement | null>(null)
  // Look the hop up by id so the drawer shows fresh values as snapshots arrive.
  const hop: Hop | null = snapshot.hops.find((h) => h.id === selected) ?? null
  return (
    <div className="flex flex-col gap-4">
      <VerdictBanner snapshot={snapshot} since={since} />
      <PathDiagram
        hops={snapshot.hops}
        blame={snapshot.verdict.blame}
        severity={snapshot.verdict.severity}
        samples={samples}
        now={now}
        onSelect={(h, el) => {
          opener.current = el
          setSelected(h.id)
        }}
      />
      <HopSheet
        hop={hop}
        blamed={hop?.id === snapshot.verdict.blame}
        severity={snapshot.verdict.severity}
        returnFocus={opener}
        onClose={() => setSelected(null)}
        at={live ? undefined : snapshot.at}
      />
    </div>
  )
}
