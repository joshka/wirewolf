import type { Sample, Snapshot } from "@/lib/api"
import { PathDiagram } from "./path"
import { VerdictBanner } from "./verdict"

/** Verdict plus path. Shared by Now and the incident detail, which passes its id. */
export function SnapshotView({
  snapshot,
  since,
  incident,
  samples,
  now,
}: {
  snapshot: Snapshot
  since?: string | null
  incident?: string
  samples?: Record<string, Sample[]>
  now?: number
}) {
  return (
    <div className="flex flex-col gap-4">
      <VerdictBanner snapshot={snapshot} since={since} />
      <PathDiagram hops={snapshot.hops} blame={snapshot.verdict.blame} severity={snapshot.verdict.severity} samples={samples} now={now} incident={incident} />
    </div>
  )
}
