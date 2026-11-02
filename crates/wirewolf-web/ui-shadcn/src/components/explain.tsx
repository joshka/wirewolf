import { useState } from "react"
import { toast } from "sonner"
import { cn } from "@/lib/utils"
import type { Severity, Snapshot, Step } from "@/lib/api"
import { copyText } from "@/lib/clipboard"
import { explanationText } from "@/lib/explain"
import { href, onLinkClick } from "@/lib/router"
import { Button, buttonVariants } from "./ui/button"
import { StatusIcon } from "./status"
import { RelTime } from "./time"

/** The cause row's tint and its outlined pill, in the verdict's colour. */
const tint: Record<Severity, string> = { down: "bg-fail/10", degraded: "bg-warn/10", healthy: "", unknown: "" }
const pill: Record<Severity, string> = { down: "border-fail text-fail", degraded: "border-warn text-warn", healthy: "", unknown: "" }

const link = "text-muted-foreground underline decoration-border underline-offset-4 hover:text-foreground"

/**
 * The checks behind the verdict, in path order. Two columns: the check (question, target in mono)
 * and what it saw, with what that tells you under it. Hairlines between rows only.
 *
 * Collapsed to one line when everything passes; open when there's something to show.
 */
export function Explanation({ snapshot }: { snapshot: Snapshot }) {
  const v = snapshot.verdict
  const steps = v.steps ?? []
  const healthy = v.severity === "healthy"
  const [open, setOpen] = useState(!healthy)
  const reportable = v.blame === "isp" || v.blame === "internet"

  async function copy() {
    await copyText(explanationText(snapshot))
    toast.success("Explanation copied", { description: "Paste it into a support chat or an email." })
  }

  const footer = (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 pt-3 text-[13px]">
      {!healthy && (reportable ? (
        <a href={href({ name: "report" })} onClick={onLinkClick} className={buttonVariants({ size: "sm" })}>
          Open ISP report
        </a>
      ) : (
        <Button size="sm" onClick={copy}>
          Copy explanation
        </Button>
      ))}
      {(healthy || reportable) && <button type="button" className={link} onClick={copy}>Copy explanation</button>}
      <button type="button" className={link} aria-expanded={open} aria-controls="verdict-checks" onClick={() => setOpen(!open)}>
        {open ? "Hide checks" : "Show checks"}
      </button>
      <span className="ml-auto text-muted-foreground">{v.method} · checked <RelTime at={snapshot.at} /></span>
    </div>
  )

  if (steps.length === 0) {
    return v.fix_hint && !healthy ? <p className="mt-3 max-w-[68ch] px-4 text-sm"><span className="font-medium">Try: </span>{v.fix_hint}</p> : null
  }
  if (!open) {
    const bad = steps.filter((s) => s.status !== "pass").length
    return (
      <div className="mt-4 flex flex-wrap items-center gap-x-4 gap-y-2 border-y px-4 py-2.5 text-sm">
        <StatusIcon status={healthy ? "pass" : "warn"} className="size-4 shrink-0" />
        <span className="-ml-2">{healthy ? `All ${steps.length} checks passed` : `${bad} of ${steps.length} checks need attention`}</span>
        <button type="button" className={link} aria-expanded={false} aria-controls="verdict-checks" onClick={() => setOpen(true)}>Show checks</button>
        <button type="button" className={link} onClick={copy}>Copy explanation</button>
        <span className="ml-auto text-[13px] text-muted-foreground">{v.method} · checked <RelTime at={snapshot.at} /></span>
      </div>
    )
  }
  return (
    <div className="mt-4">
      <div id="verdict-checks" role="table" aria-label="Checks behind the verdict" className="divide-y border-y text-sm">
        {steps.map((s) => <Row key={s.hop} step={s} severity={v.severity} />)}
      </div>
      <div className="grid gap-x-8 gap-y-2 px-4 pt-3 text-[13px] md:grid-cols-2">
        {v.fix_hint && !healthy && (
          <p><span className="font-medium">Try: </span>{v.fix_hint}</p>
        )}
        <p className="text-muted-foreground">
          <span className="font-medium text-foreground">How sure: </span>
          {v.confidence_reason ?? v.confidence}
          {v.held_cycles != null && v.held_cycles > 1 && ` Held for ${v.held_cycles} checks in a row.`}
        </p>
      </div>
      {footer}
    </div>
  )
}

function Row({ step: s, severity }: { step: Step; severity: Severity }) {
  const cause = s.role === "cause"
  const quiet = s.role === "collateral" || s.role === "skipped"
  return (
    <div role="row" className={cn("grid gap-x-8 gap-y-1 px-4 py-2.5 md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]", cause && tint[severity], quiet && "text-muted-foreground")}>
      <div role="cell" className="flex min-w-0 gap-2.5">
        <StatusIcon status={s.status} className="mt-0.5 size-4 shrink-0" />
        <div className="min-w-0">
          <div className={cn(cause ? "font-[650]" : "font-medium")}>
            {s.question}
            {cause && <span className={cn("ml-2 rounded border px-1.5 py-px align-[1px] text-[10.5px] font-semibold tracking-wide", pill[severity])}>CAUSE</span>}
            {s.role === "excused" && <span className="ml-2 text-xs font-normal text-muted-foreground">excused</span>}
          </div>
          {s.subject && <div className="truncate font-mono text-xs text-muted-foreground/70" title={s.subject}>{s.subject}</div>}
        </div>
      </div>
      <div role="cell" className="min-w-0 pl-6.5 md:pl-0">
        <div className="tabular-nums">
          <span className={cn(cause && "font-[650]")}>{s.saw}</span>
          {s.limit && <span className="text-muted-foreground"> · limit {s.limit}</span>}
        </div>
        <div className="mt-0.5 text-[13px] text-muted-foreground">
          {s.role === "skipped" ? s.means : `${s.did}. ${s.means}`}
        </div>
      </div>
    </div>
  )
}
