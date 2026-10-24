import { CircleHelp, Lightbulb } from "lucide-react"
import { cn } from "@/lib/utils"
import type { Snapshot } from "@/lib/api"
import { RelTime } from "./time"
import { severityStatus, statusIcon, statusText } from "./status"

/**
 * The answer to "is it broken, where, and since when?". Loud when something is wrong, quiet
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
      aria-live="polite"
      aria-label="Verdict"
      className={cn(
        "relative overflow-hidden rounded-xl border p-5 sm:p-6",
        healthy && "bg-card",
        v.severity === "degraded" && "border-warn/40 bg-warn/[0.07]",
        v.severity === "down" && "border-fail/40 bg-fail/[0.08]",
        v.severity === "unknown" && "bg-muted/40",
      )}
    >
      {!healthy && (
        <div
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-x-0 top-0 h-1",
            v.severity === "down" ? "bg-fail" : v.severity === "degraded" ? "bg-warn" : "bg-skip",
          )}
        />
      )}
      <div className="flex items-start gap-4">
        <div
          className={cn(
            "grid shrink-0 place-items-center rounded-full",
            healthy ? "size-10 bg-pass/10" : "size-12 sm:size-14",
            v.severity === "degraded" && "bg-warn/15",
            v.severity === "down" && "bg-fail/15",
            v.severity === "unknown" && "bg-muted",
          )}
        >
          <Icon aria-hidden className={cn(healthy ? "size-5" : "size-6 sm:size-7", statusText[status])} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
            <span className={cn("font-semibold uppercase tracking-wider", statusText[status])}>{v.severity}</span>
            {since && (
              <span>
                since <RelTime at={since} className="text-foreground" />
              </span>
            )}
            {blamed && <span>at {blamed.label}</span>}
            <span>{v.confidence} confidence</span>
          </div>
          <h1
            className={cn(
              "mt-1 font-semibold tracking-tight text-balance",
              healthy ? "text-xl sm:text-2xl" : "text-2xl sm:text-3xl",
            )}
          >
            {v.headline}
          </h1>
          <p className="mt-1.5 max-w-3xl text-sm text-muted-foreground text-pretty sm:text-base">{v.detail}</p>
          {v.fix_hint && (
            <div className="mt-4 flex max-w-3xl items-start gap-2.5 rounded-lg border bg-background/60 p-3 text-sm">
              <Lightbulb aria-hidden className="mt-0.5 size-4 shrink-0 text-brand" />
              <p>
                <span className="font-medium">Try this: </span>
                {v.fix_hint}
              </p>
            </div>
          )}
          <p className="mt-3 text-xs text-muted-foreground/80">{v.method}</p>
        </div>
      </div>
    </section>
  )
}
