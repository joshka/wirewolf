import {
  CircleCheck,
  CircleDashed,
  CircleHelp,
  CircleMinus,
  CircleSlash,
  CircleX,
  TriangleAlert,
  type LucideIcon,
} from "lucide-react"
import { cn } from "@/lib/utils"
import type { Severity, Status } from "@/lib/api"
import { severityLabel, statusLabel } from "@/lib/format"

export const statusIcon: Record<Status, LucideIcon> = {
  pass: CircleCheck,
  warn: TriangleAlert,
  fail: CircleX,
  skip: CircleMinus,
  not_applicable: CircleSlash,
  pending: CircleDashed,
}

/** Text color class for a status. */
export const statusText: Record<Status, string> = {
  pass: "text-pass",
  warn: "text-warn",
  fail: "text-fail",
  skip: "text-skip",
  not_applicable: "text-skip",
  pending: "text-muted-foreground",
}

export const severityStatus: Record<Severity, Status> = {
  healthy: "pass",
  degraded: "warn",
  down: "fail",
  unknown: "pending",
}

export function StatusIcon({ status, className }: { status: Status; className?: string }) {
  const Icon = statusIcon[status]
  return <Icon aria-hidden className={cn("size-4 shrink-0", statusText[status], className)} />
}

/** Icon plus label, so status never relies on color alone. */
export function StatusPill({ status, className }: { status: Status; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs font-medium",
        status === "pass" && "border-pass/25 bg-pass/10",
        status === "warn" && "border-warn/30 bg-warn/10",
        status === "fail" && "border-fail/30 bg-fail/10",
        (status === "skip" || status === "not_applicable" || status === "pending") && "bg-muted/50",
        className,
      )}
    >
      <StatusIcon status={status} className="size-3.5" />
      {statusLabel[status]}
    </span>
  )
}

export function SeverityPill({ severity, className }: { severity: Severity; className?: string }) {
  const status = severityStatus[severity]
  const Icon = severity === "unknown" ? CircleHelp : statusIcon[status]
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs font-medium",
        severity === "healthy" && "border-pass/25 bg-pass/10",
        severity === "degraded" && "border-warn/30 bg-warn/10",
        severity === "down" && "border-fail/30 bg-fail/10",
        severity === "unknown" && "bg-muted/50",
        className,
      )}
    >
      <Icon aria-hidden className={cn("size-3.5", statusText[status])} />
      {severityLabel[severity]}
    </span>
  )
}
