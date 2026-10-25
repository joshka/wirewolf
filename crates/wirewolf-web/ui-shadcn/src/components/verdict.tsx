import type { ReactNode } from "react"
import { cn } from "@/lib/utils"
import type { Severity, Snapshot } from "@/lib/api"
import { Lasting } from "./time"
import { SeverityPill } from "./status"
import { Explanation } from "./explain"

/** The faint status band behind the status line and headline, one colour per verdict. */
const band: Record<Severity, string> = {
  down: "bg-fail/10",
  degraded: "bg-warn/10",
  healthy: "bg-pass/10",
  unknown: "bg-muted/40",
}

/**
 * The answer to "is it broken, where, and since when?", then the checks behind it.
 *
 * No card: a status band (in every state, so only its colour changes) holds the status line and
 * headline; the lead and the checks sit on the page surface below it. Band text, lead, check
 * rows, and footer share one 16px left edge.
 */
export function VerdictBanner({
  snapshot,
  since,
  recent,
  action,
}: {
  snapshot: Snapshot
  /** When the current state began: the open incident's start, or the last one's end when healthy. */
  since?: string | null
  /** Incidents in the last 24 hours, shown when healthy so "good now" isn't mistaken for "stable". */
  recent?: number
  action?: ReactNode
}) {
  const v = snapshot.verdict
  const healthy = v.severity === "healthy"
  const lead = v.bottom_line ?? v.detail
  return (
    <section aria-label="Verdict">
      <div className={cn("rounded-lg px-4 py-3.5", band[v.severity])}>
        <div className="flex items-start gap-3">
          <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-1.5 gap-y-1 text-[13px] text-muted-foreground">
            <SeverityPill severity={v.severity} />
            {since && (
              <span>
                · <Lasting since={since} />
              </span>
            )}
            <span>· {v.confidence} confidence</span>
            {healthy && recent != null && (
              <span>· {recent === 0 ? "no incidents" : recent === 1 ? "1 incident" : `${recent} incidents`} in the last 24 h</span>
            )}
          </div>
          {action && <div className="shrink-0">{action}</div>}
        </div>
        <h1 aria-live="polite" className="mt-1.5 text-[26px] leading-tight font-[650] tracking-[-0.015em] text-balance">
          {v.headline}
        </h1>
      </div>
      <p className="mt-3 max-w-[68ch] px-4 text-[15px] text-foreground/80 text-pretty">{lead}</p>
      {v.severity !== "unknown" && <Explanation snapshot={snapshot} />}
    </section>
  )
}
