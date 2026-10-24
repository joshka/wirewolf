import { useState } from "react"
import type { Hop, Sample, Snapshot } from "@/lib/api"
import { HopSheet } from "./hop-sheet"
import { PathDiagram } from "./path"
import { VerdictBanner } from "./verdict"

/** Verdict plus path, with the hop drawer. Shared by Now and the incident detail. */
export function SnapshotView({
  snapshot,
  since,
  samples,
  now,
  live,
}: {
  snapshot: Snapshot
  since?: string | null
  samples?: Record<string, Sample[]>
  now?: number
  live: boolean
}) {
  const [selected, setSelected] = useState<string | null>(null)
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
        onSelect={(h) => setSelected(h.id)}
      />
      <HopSheet hop={hop} onClose={() => setSelected(null)} live={live} at={live ? undefined : snapshot.at} />
    </div>
  )
}
