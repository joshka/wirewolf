import { AppWindow, BookOpenText, ChevronRight, Globe, Laptop, Router, Waypoints, type LucideIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import type { Hop, HopKind, Severity, Status } from "@/lib/api"
import { ms, pct, statusLabel } from "@/lib/format"
import { StatusIcon, statusText } from "./status"

export const hopIcon: Record<HopKind, LucideIcon> = {
  host: Laptop,
  gateway: Router,
  isp: Waypoints,
  internet: Globe,
  dns: BookOpenText,
  web: AppWindow,
}

/** The one number that matters for a hop, or its summary if it has none. */
export function keyMetric(hop: Hop): string {
  const m = hop.metrics
  if (hop.status === "skip" || (m.rtt_ms == null && m.loss_pct == null)) return hop.summary
  if ((m.loss_pct ?? 0) > 0) return m.rtt_ms == null ? `${pct(m.loss_pct)} loss` : `${ms(m.rtt_ms)} · ${pct(m.loss_pct)} loss`
  return ms(m.rtt_ms)
}

interface PathProps {
  hops: Hop[]
  blame: string | null
  severity: Severity
  onSelect: (hop: Hop, opener: HTMLElement) => void
}

/** The path from this machine to the web: six cards from `lg` up, list rows below. */
export function PathDiagram({ hops, blame, severity, onSelect }: PathProps) {
  return (
    <section aria-label="Network path">
      <ol className="hidden lg:flex lg:items-stretch">
        {hops.map((hop, i) => (
          <li key={hop.id} className="flex min-w-0 flex-1 items-center">
            {i > 0 && <Connector status={hop.status} horizontal />}
            <HopCard hop={hop} blamed={hop.id === blame} severity={severity} onSelect={onSelect} />
          </li>
        ))}
      </ol>
      <ol className="flex flex-col lg:hidden">
        {hops.map((hop, i) => (
          <li key={hop.id} className="flex flex-col">
            {i > 0 && <Connector status={hop.status} />}
            <HopRow hop={hop} blamed={hop.id === blame} severity={severity} onSelect={onSelect} />
          </li>
        ))}
      </ol>
    </section>
  )
}

function Connector({ status, horizontal }: { status: Status; horizontal?: boolean }) {
  const broken = status === "fail" || status === "skip"
  return (
    <div
      aria-hidden
      className={cn(
        "shrink-0",
        horizontal ? "h-0 w-3 border-t xl:w-4" : "ml-7 h-2.5 w-0 border-l",
        broken ? "border-dashed border-muted-foreground/40" : status === "warn" ? "border-warn/60" : "border-border",
      )}
    />
  )
}

interface HopProps {
  hop: Hop
  blamed: boolean
  severity: Severity
  onSelect: (hop: Hop, opener: HTMLElement) => void
}

const blameBorder = (blamed: boolean, severity: Severity) =>
  blamed ? (severity === "down" ? "border-fail ring-1 ring-fail/40" : "border-warn ring-1 ring-warn/40") : ""

export function BlameTag({ severity }: { severity: Severity }) {
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-[10px] font-semibold tracking-wider uppercase",
        severity === "down" ? "bg-fail/15 text-fail" : "bg-warn/15 text-warn",
      )}
    >
      Likely cause
    </span>
  )
}

const metricClass = (s: Status) => (s === "pass" ? "text-foreground" : statusText[s])

function HopCard({ hop, blamed, severity, onSelect }: HopProps) {
  const Icon = hopIcon[hop.kind]
  const dim = hop.status === "skip" || hop.status === "not_applicable"
  return (
    <button
      type="button"
      onClick={(e) => onSelect(hop, e.currentTarget)}
      aria-label={`${hop.label}: ${statusLabel[hop.status]}. ${hop.summary}. Open details`}
      className={cn(
        "flex h-full min-w-0 flex-1 flex-col gap-2 rounded-xl border bg-card p-3.5 text-left transition-[border-color,opacity] outline-none",
        "hover:border-foreground/25 focus-visible:ring-3 focus-visible:ring-ring",
        blameBorder(blamed, severity),
        dim && "opacity-50 hover:opacity-80",
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="grid size-8 place-items-center rounded-lg bg-muted text-muted-foreground">
          <Icon aria-hidden className="size-4" />
        </span>
        {blamed ? <BlameTag severity={severity} /> : <StatusIcon status={hop.status} />}
      </div>
      <div className="min-w-0">
        <div className="flex items-center gap-1.5 font-medium">
          {hop.label}
          {blamed && <StatusIcon status={hop.status} className="size-3.5" />}
        </div>
        <div className="truncate font-mono text-xs text-muted-foreground" title={hop.target ?? undefined}>
          {hop.target ?? "—"}
        </div>
      </div>
      <div className={cn("tnum mt-auto truncate text-sm font-semibold", metricClass(hop.status))} title={hop.summary}>
        {keyMetric(hop)}
      </div>
    </button>
  )
}

function HopRow({ hop, blamed, severity, onSelect }: HopProps) {
  const Icon = hopIcon[hop.kind]
  const dim = hop.status === "skip" || hop.status === "not_applicable"
  return (
    <button
      type="button"
      onClick={(e) => onSelect(hop, e.currentTarget)}
      aria-label={`${hop.label}: ${statusLabel[hop.status]}. ${hop.summary}. Open details`}
      className={cn(
        "flex items-center gap-3 rounded-xl border bg-card p-3 text-left transition-[border-color,opacity] outline-none",
        "hover:border-foreground/25 focus-visible:ring-3 focus-visible:ring-ring",
        blameBorder(blamed, severity),
        dim && "opacity-50",
      )}
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
        <Icon aria-hidden className="size-4" />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="font-medium">{hop.label}</span>
          {blamed && <BlameTag severity={severity} />}
        </div>
        <div className="truncate font-mono text-xs text-muted-foreground">{hop.target ?? "—"}</div>
      </div>
      <div className={cn("tnum flex min-w-0 max-w-[45%] shrink-0 items-center gap-1.5 text-sm font-medium", metricClass(hop.status))}>
        <StatusIcon status={hop.status} className="size-3.5" />
        <span className="truncate">
          <span className="sr-only">{statusLabel[hop.status]} · </span>
          {keyMetric(hop)}
        </span>
      </div>
      <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground" />
    </button>
  )
}
