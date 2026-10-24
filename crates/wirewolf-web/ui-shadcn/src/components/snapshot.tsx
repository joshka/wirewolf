import { useRef, useState } from "react"
import type { Hop, Snapshot } from "@/lib/api"
import { HopSheet } from "./hop-sheet"
import { PathDiagram } from "./path"
import { VerdictBanner } from "./verdict"

/** Verdict plus path, with the hop drawer. Shared by Now and the incident detail. */
export function SnapshotView({ snapshot, since, live }: { snapshot: Snapshot; since?: string | null; live: boolean }) {
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
        live={live}
        at={live ? undefined : snapshot.at}
      />
    </div>
  )
}
