import type { ReactNode } from "react"
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
  action,
}: {
  snapshot: Snapshot
  since?: string | null
  incident?: string
  samples?: Record<string, Sample[]>
  now?: number
  /** Shown in the verdict banner's corner, for example "Ask an AI". */
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col gap-4">
      <VerdictBanner snapshot={snapshot} since={since} action={action} />
      <PathDiagram hops={snapshot.hops} blame={snapshot.verdict.blame} severity={snapshot.verdict.severity} samples={samples} now={now} incident={incident} />
    </div>
  )
}
