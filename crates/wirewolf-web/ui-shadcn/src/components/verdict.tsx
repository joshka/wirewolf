import { CircleHelp, Lightbulb } from "lucide-react"
import { cn } from "@/lib/utils"
import type { Snapshot } from "@/lib/api"
import { RelTime } from "./time"
import { SeverityPill, severityStatus, statusIcon, statusText } from "./status"

/**
 * The answer to "is it broken, where, and since when?". Loud when something is wrong, compact
 * when healthy.
 */
export function VerdictBanner({ snapshot, since }: { snapshot: Snapshot; since?: string | null }) {
  const v = snapshot.verdict
  const status = severityStatus[v.severity]
  const Icon = v.severity === "unknown" ? CircleHelp : statusIcon[status]
  const blamed = snapshot.hops.find((h) => h.id === v.blame)
  const healthy = v.severity === "healthy"
  return (
    <section
      aria-label="Verdict"
      className={cn(
        "relative overflow-hidden rounded-xl border",
        "bg-card p-4 sm:p-5",
        v.severity === "degraded" && "border-warn/40 bg-warn/10",
        v.severity === "down" && "border-fail/40 bg-fail/10",
        v.severity === "unknown" && "bg-muted/40",
      )}
    >
      <div
        aria-hidden
        className={cn(
          "pointer-events-none absolute inset-x-0 top-0 h-1",
          v.severity === "down" ? "bg-fail" : v.severity === "degraded" ? "bg-warn" : v.severity === "healthy" ? "bg-pass" : "bg-skip",
        )}
      />
      <div className="flex items-start gap-4">
        <div
          className={cn(
            "grid size-11 shrink-0 place-items-center rounded-lg",
            healthy && "bg-pass/12",
            v.severity === "degraded" && "bg-warn/15",
            v.severity === "down" && "bg-fail/15",
            v.severity === "unknown" && "bg-muted",
          )}
        >
          <Icon aria-hidden className={cn("size-6", statusText[status])} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-1 gap-y-1 text-xs text-muted-foreground">
            <SeverityPill severity={v.severity} />
            {blamed && <span> · at {blamed.label}</span>}
            {since && (
              <span>
                · since <RelTime at={since} className="text-foreground" />
              </span>
            )}
            <span>· {v.confidence[0].toUpperCase() + v.confidence.slice(1)} confidence</span>
          </div>
          <h1
            aria-live="polite"
            className={cn(
              "mt-2 font-semibold tracking-tight text-balance",
              healthy ? "text-xl sm:text-2xl" : "text-2xl sm:text-3xl",
            )}
          >
            {v.headline}
          </h1>
          <p className="mt-2 max-w-2xl text-sm text-muted-foreground text-pretty">{v.detail}</p>
          {v.fix_hint && !healthy && (
            <div className="mt-4 flex max-w-2xl items-start gap-2 rounded-lg border bg-background p-3 text-sm">
              <Lightbulb aria-hidden className="mt-0.5 size-4 shrink-0 text-primary" />
              <p>
                <span className="font-medium">Try next: </span>
                {v.fix_hint}
              </p>
            </div>
          )}
          <p className="mt-3 text-xs text-muted-foreground">
            {v.method} · checked <RelTime at={snapshot.at} />
          </p>
        </div>
      </div>
    </section>
  )
}
