import { ChevronDown, Copy, FileText } from "lucide-react"
import { toast } from "sonner"
import { cn } from "@/lib/utils"
import type { Snapshot, Step } from "@/lib/api"
import { copyText } from "@/lib/clipboard"
import { explanationText } from "@/lib/explain"
import { href, onLinkClick } from "@/lib/router"
import { Button, buttonVariants } from "./ui/button"
import { StatusIcon } from "./status"

export function ExplainToggle({ open, onChange }: { open: boolean; onChange: (open: boolean) => void }) {
  return (
    <Button variant="outline" size="sm" aria-expanded={open} aria-controls="verdict-explanation" onClick={() => onChange(!open)}>
      {open ? "Hide explanation" : "Explain"}
      <ChevronDown aria-hidden className={cn("size-4 transition-transform", open && "rotate-180")} />
    </Button>
  )
}

const tag: Partial<Record<Step["role"], { label: string; className: string }>> = {
  cause: { label: "Cause", className: "bg-fail text-white" },
  collateral: { label: "Knock-on", className: "border border-border text-muted-foreground" },
  excused: { label: "Excused", className: "border border-pass/50 text-pass" },
}

/**
 * The checks behind the verdict, in path order, each saying what was done, what came back, the
 * limit, and what it means. Four columns from `md` up, stacked on phones.
 */
export function Explanation({ snapshot }: { snapshot: Snapshot }) {
  const v = snapshot.verdict
  const steps = v.steps ?? []
  const warnCause = v.severity === "degraded"
  async function copy() {
    await copyText(explanationText(snapshot))
    toast.success("Explanation copied", { description: "Paste it into a support chat or an email." })
  }
  if (steps.length === 0) {
    return <p id="verdict-explanation" className="mt-4 text-sm text-muted-foreground">This server doesn't send an explanation yet (it needs wirewolf with API 1.8).</p>
  }
  return (
    <div id="verdict-explanation" className="mt-4">
      {v.bottom_line && (
        <p className={cn("rounded-r-md border-l-4 bg-background/60 px-3 py-2 text-sm sm:text-base", v.severity === "down" ? "border-fail" : v.severity === "degraded" ? "border-warn" : "border-pass")}>
          <span className="font-semibold">Bottom line: </span>
          {v.bottom_line}
        </p>
      )}
      <div role="table" aria-label="Checks behind the verdict" className="mt-4 text-sm">
        <div role="row" className="hidden gap-4 px-3 pb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground md:grid md:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)_minmax(0,1.1fr)_minmax(0,1.4fr)]">
          <span role="columnheader">Check, in path order</span>
          <span role="columnheader">What wirewolf did</span>
          <span role="columnheader">What it saw</span>
          <span role="columnheader">What that tells you</span>
        </div>
        {steps.map((s) => {
          const t = tag[s.role]
          const quiet = s.role === "collateral" || s.role === "skipped"
          return (
            <div
              key={s.hop}
              role="row"
              className={cn(
                "grid gap-1 border-t px-3 py-2.5 md:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)_minmax(0,1.1fr)_minmax(0,1.4fr)] md:gap-4",
                s.role === "cause" && (warnCause ? "rounded-md bg-warn/15" : "rounded-md bg-fail/15"),
                quiet && "text-muted-foreground",
              )}
            >
              <div role="cell" className="flex gap-2.5">
                <StatusIcon status={s.status} className="mt-0.5 size-4 shrink-0" />
                <div className="min-w-0">
                  <div className="font-medium">{s.question}</div>
                  {s.subject && <div className="truncate font-mono text-xs text-muted-foreground" title={s.subject}>{s.subject}</div>}
                </div>
              </div>
              <div role="cell" className="pl-6.5 md:pl-0"><span className="mr-1 text-xs font-medium uppercase tracking-wide text-muted-foreground md:hidden">Did </span>{s.did}</div>
              <div role="cell" className="pl-6.5 tabular-nums md:pl-0">
                <span className="mr-1 text-xs font-medium uppercase tracking-wide text-muted-foreground md:hidden">Saw </span><span className={cn(s.role === "cause" && "font-semibold")}>{s.saw}</span>
                {s.limit && <div className="text-xs text-muted-foreground">Limit: {s.limit}</div>}
              </div>
              <div role="cell" className="pl-6.5 md:pl-0">
                {!t && (<span className="mr-1 text-xs font-medium uppercase tracking-wide text-muted-foreground md:hidden">Means </span>)}
                {t && <span className={cn("mr-1.5 rounded px-1.5 py-px text-xs font-semibold", s.role === "cause" && warnCause ? "bg-warn text-black" : t.className)}>{t.label}</span>}
                {s.means}
              </div>
            </div>
          )
        })}
      </div>
      <div className="mt-2 grid gap-4 border-t px-3 pt-3 text-sm md:grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)_auto]">
        <p>
          <span className="font-medium">How sure: </span>
          {v.confidence_reason ?? v.confidence}
          {v.held_cycles != null && v.held_cycles > 1 && ` It has held for ${v.held_cycles} checks in a row.`}
        </p>
        <p>{v.fix_hint && v.severity !== "healthy" && <><span className="font-medium">What to try: </span>{v.fix_hint}</>}</p>
        <div className="flex flex-wrap items-start gap-2 md:flex-col md:items-stretch">
          <Button variant="outline" size="sm" onClick={copy}>
            <Copy aria-hidden className="size-4" /> Copy explanation
          </Button>
          {v.severity !== "healthy" && (
            <a href={href({ name: "report" })} onClick={onLinkClick} className={buttonVariants({ variant: "outline", size: "sm" })}>
              <FileText aria-hidden className="size-4" /> Open ISP report
            </a>
          )}
        </div>
      </div>
    </div>
  )
}
