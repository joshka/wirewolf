import { AppWindow, AtSign, Building2, ChevronRight, Globe, Laptop, Router, type LucideIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import type { Hop, HopKind, Severity, Status } from "@/lib/api"
import { ms, pct, statusLabel } from "@/lib/format"
import { CauseBadge, StatusIcon, statusText } from "./status"

export const hopIcon: Record<HopKind, LucideIcon> = {
  host: Laptop,
  gateway: Router,
  isp: Building2,
  internet: Globe,
  dns: AtSign,
  web: AppWindow,
}

/** The one number that matters for a hop, or its summary if it has none. */
export function keyMetric(hop: Hop): string {
  const m = hop.metrics
  if (hop.status === "skip" || (m.rtt_ms == null && m.loss_pct == null)) return hop.summary
  if ((m.loss_pct ?? 0) > 0) return m.rtt_ms == null ? `${pct(m.loss_pct)} loss` : `${ms(m.rtt_ms)} · ${pct(m.loss_pct)} loss`
  return ms(m.rtt_ms)
}

const dimmed = (s: Status) => s === "skip" || s === "not_applicable"

export function IconTile({ icon: Icon }: { icon: LucideIcon }) {
  return (
    <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
      <Icon aria-hidden className="size-5" />
    </span>
  )
}

/**
 * The path from this machine to the web. One element per hop: a list row on small screens,
 * a card in a six-column row from `lg` up.
 */
export function PathDiagram({
  hops,
  blame,
  severity,
  onSelect,
}: {
  hops: Hop[]
  blame: string | null
  severity: Severity
  onSelect: (hop: Hop, opener: HTMLElement) => void
}) {
  return (
    <ol aria-label="Network path" className="grid gap-2 lg:grid-cols-6 lg:gap-3">
      {hops.map((hop) => {
        const blamed = hop.id === blame
        return (
          <li key={hop.id} className="flex">
            <button
              type="button"
              data-hop={hop.id}
              onClick={(e) => onSelect(hop, e.currentTarget)}
              aria-label={`${hop.label}: ${statusLabel[hop.status]}. ${hop.summary}. Open details`}
              className={cn(
                "flex w-full items-center gap-3 rounded-xl border bg-card p-3 text-left transition outline-none lg:min-h-32 lg:flex-col lg:items-stretch lg:p-4",
                "hover:border-foreground/25 focus-visible:ring-3 focus-visible:ring-ring",
                blamed && (severity === "down" ? "border-fail bg-fail/5" : "border-warn bg-warn/5"),
                dimmed(hop.status) && "opacity-50",
              )}
            >
              <div className="flex items-start justify-between">
                <IconTile icon={hopIcon[hop.kind]} />
                <StatusIcon status={hop.status} className="hidden lg:block" />
              </div>
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <span className="text-sm font-medium">{hop.label}</span>
                {blamed && <CauseBadge severity={severity} />}
                <span className="truncate font-mono text-xs text-muted-foreground" title={hop.target ?? undefined}>
                  {hop.target ?? "—"}
                </span>
                <span
                  className={cn(
                    "flex min-w-0 items-center gap-1.5 text-sm font-medium tabular-nums lg:mt-auto",
                    hop.status === "pass" ? "text-foreground" : dimmed(hop.status) ? "text-muted-foreground" : statusText[hop.status],
                  )}
                >
                  <StatusIcon status={hop.status} className="size-3.5 lg:hidden" />
                  <span className="truncate" title={hop.summary}>
                    {keyMetric(hop)}
                  </span>
                </span>
              </div>
              <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground lg:hidden" />
            </button>
          </li>
        )
      })}
    </ol>
  )
}
