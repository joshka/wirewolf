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
import { Badge } from "@/components/ui/badge"
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

/**
 * Icon plus label, so status never relies on color alone. Pass `icon={false}` where a status
 * icon is already shown right next to it.
 */
export function StatusPill({
  status,
  label,
  icon = true,
  className,
}: {
  status: Status
  label?: string
  icon?: boolean
  className?: string
}) {
  const Icon = label && status === "pending" ? CircleHelp : statusIcon[status]
  const variant = status === "pass" || status === "warn" || status === "fail" ? status : "muted"
  return (
    <Badge variant={variant} className={className}>
      {icon && <Icon aria-hidden />}
      {label ?? statusLabel[status]}
    </Badge>
  )
}

export const SeverityPill = ({ severity, icon, className }: { severity: Severity; icon?: boolean; className?: string }) => (
  <StatusPill status={severityStatus[severity]} label={severityLabel[severity]} icon={icon} className={className} />
)

/** The LIKELY CAUSE badge, always on its own line under the hop label. */
export function CauseBadge({ severity }: { severity: Severity }) {
  return (
    <Badge variant={severity === "down" ? "fail" : "warn"} caps>
      Likely cause
    </Badge>
  )
}

