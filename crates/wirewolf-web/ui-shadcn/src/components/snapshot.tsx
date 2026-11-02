import type { ReactNode } from "react"
import type { Snapshot } from "@/lib/api"
import { PathDiagram } from "./path"
import { VerdictBanner } from "./verdict"

/** Verdict, chart, path. Shared by Now and the incident detail, which passes its id. */
export function SnapshotView({
  snapshot,
  since,
  recent,
  incident,
  chart,
  action,
}: {
  snapshot: Snapshot
  since?: string | null
  recent?: number
  incident?: string
  /** The chart of the period, between the verdict and the path. */
  chart?: ReactNode
  /** Shown in the verdict banner's corner, for example "Ask an AI". */
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col gap-4">
      <VerdictBanner snapshot={snapshot} since={since} recent={recent} action={action} />
      {chart}
      <PathDiagram hops={snapshot.hops} blame={snapshot.verdict.blame} severity={snapshot.verdict.severity} incident={incident} />
    </div>
  )
}
