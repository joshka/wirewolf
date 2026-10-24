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
  return <Icon aria-hidden strokeWidth={1.75} className={cn("size-4 shrink-0", statusText[status], className)} />
}

type Tone = "pass" | "warn" | "fail" | "muted"

const toneClass: Record<Tone, string> = {
  pass: "border-pass/30 bg-pass/12 text-pass",
  warn: "border-warn/30 bg-warn/12 text-warn",
  fail: "border-fail/30 bg-fail/12 text-fail",
  muted: "border-border bg-muted/60 text-muted-foreground",
}

/**
 * The one badge (docs/design-language.md): 20px tall, radius 6, 12/500 text, optional 12px
 * icon. `cause` is the uppercase LIKELY CAUSE variant.
 */
export function Badge({
  tone,
  icon: Icon,
  cause,
  className,
  children,
}: {
  tone: Tone
  icon?: LucideIcon
  cause?: boolean
  className?: string
  children: React.ReactNode
}) {
  return (
    <span
      className={cn(
        "inline-flex h-5 w-fit shrink-0 items-center gap-1 rounded-md border px-1.5 whitespace-nowrap",
        cause ? "text-[11px] font-semibold tracking-[0.04em] uppercase" : "text-xs font-medium",
        toneClass[tone],
        className,
      )}
    >
      {Icon && <Icon aria-hidden className="size-3" strokeWidth={2} />}
      {children}
    </span>
  )
}

const statusTone: Record<Status, Tone> = {
  pass: "pass",
  warn: "warn",
  fail: "fail",
  skip: "muted",
  not_applicable: "muted",
  pending: "muted",
}

/** Icon plus label, so status never relies on color alone. */
export function StatusPill({ status, className }: { status: Status; className?: string }) {
  return (
    <Badge tone={statusTone[status]} icon={statusIcon[status]} className={className}>
      {statusLabel[status]}
    </Badge>
  )
}

export function SeverityPill({ severity, className }: { severity: Severity; className?: string }) {
  const status = severityStatus[severity]
  return (
    <Badge tone={statusTone[status]} icon={severity === "unknown" ? CircleHelp : statusIcon[status]} className={className}>
      {severityLabel[severity]}
    </Badge>
  )
}

/** The LIKELY CAUSE badge, always on its own line under the hop label. */
export function CauseBadge({ severity }: { severity: Severity }) {
  return (
    <Badge tone={severity === "down" ? "fail" : "warn"} cause>
      Likely cause
    </Badge>
  )
}
