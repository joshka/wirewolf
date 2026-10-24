import { AppWindow, BookOpenText, ChevronRight, Globe, Laptop, Router, Waypoints, type LucideIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import type { Hop, HopKind, Sample, Severity, Status } from "@/lib/api"
import { ms, pct } from "@/lib/format"
import { Sparkline } from "./sparkline"
import { StatusIcon, StatusPill, statusText } from "./status"

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
  if (m.rtt_ms == null && m.loss_pct == null) return hop.summary
  if ((m.loss_pct ?? 0) > 0) return m.rtt_ms == null ? `${pct(m.loss_pct)} loss` : `${ms(m.rtt_ms)} · ${pct(m.loss_pct)} loss`
  return ms(m.rtt_ms)
}

interface PathProps {
  hops: Hop[]
  blame: string | null
  severity: Severity
  samples?: Record<string, Sample[]>
  now?: number
  onSelect: (hop: Hop) => void
}

/**
 * The path from this machine to the web. A horizontal chain on wide screens and a vertical
 * list below `xl`, where six cards side by side would get too narrow.
 */
export function PathDiagram({ hops, blame, severity, samples, now = Date.now(), onSelect }: PathProps) {
  return (
    <section aria-label="Network path">
      <ol className="hidden xl:flex xl:items-stretch">
        {hops.map((hop, i) => (
          <li key={hop.id} className="flex min-w-0 flex-1 items-center">
            {i > 0 && <Connector status={hop.status} horizontal />}
            <HopCard
              hop={hop}
              blamed={hop.id === blame}
              severity={severity}
              samples={samples?.[hop.id]}
              now={now}
              onSelect={onSelect}
            />
          </li>
        ))}
      </ol>
      <ol className="flex flex-col xl:hidden">
        {hops.map((hop, i) => (
          <li key={hop.id} className="flex flex-col">
            {i > 0 && <Connector status={hop.status} />}
            <HopRow
              hop={hop}
              blamed={hop.id === blame}
              severity={severity}
              samples={samples?.[hop.id]}
              now={now}
              onSelect={onSelect}
            />
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
        horizontal ? "mx-1 h-0 w-4 border-t-2 2xl:w-6" : "ml-7 h-3 w-0 border-l-2",
        broken ? "border-dashed border-muted-foreground/40" : status === "warn" ? "border-warn/60" : "border-border",
      )}
    />
  )
}

interface HopProps {
  hop: Hop
  blamed: boolean
  severity: Severity
  samples?: Sample[]
  now: number
  onSelect: (hop: Hop) => void
}

function blameRing(blamed: boolean, severity: Severity) {
  if (!blamed) return ""
  return severity === "down"
    ? "ring-2 ring-fail/70 border-transparent"
    : "ring-2 ring-warn/70 border-transparent"
}

function BlameTag({ severity }: { severity: Severity }) {
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wider",
        severity === "down" ? "bg-fail/15 text-fail" : "bg-warn/15 text-warn",
      )}
    >
      Likely cause
    </span>
  )
}

function HopCard({ hop, blamed, severity, samples, now, onSelect }: HopProps) {
  const Icon = hopIcon[hop.kind]
  const dim = hop.status === "skip" || hop.status === "not_applicable"
  return (
    <button
      type="button"
      onClick={() => onSelect(hop)}
      aria-label={`${hop.label}: ${hop.status}. ${hop.summary}. Open details`}
      className={cn(
        "group flex h-full min-w-0 flex-1 flex-col gap-3 rounded-xl border bg-card p-4 text-left transition-[border-color,box-shadow,opacity] outline-none",
        "hover:border-foreground/20 focus-visible:ring-3 focus-visible:ring-ring",
        blameRing(blamed, severity),
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
      <div className="mt-auto">
        <div className={cn("tnum truncate text-lg font-semibold tracking-tight", hop.status !== "pass" && statusText[hop.status])} title={hop.summary}>
          {keyMetric(hop)}
        </div>
        <Sparkline
          samples={samples ?? []}
          now={now}
          label={`${hop.label} latency, last 15 minutes`}
          className={cn("mt-1 h-8 w-full", hop.status === "fail" ? "text-fail" : hop.status === "warn" ? "text-warn" : "text-brand")}
        />
      </div>
    </button>
  )
}

function HopRow({ hop, blamed, severity, samples, now, onSelect }: HopProps) {
  const Icon = hopIcon[hop.kind]
  const dim = hop.status === "skip" || hop.status === "not_applicable"
  return (
    <button
      type="button"
      onClick={() => onSelect(hop)}
      aria-label={`${hop.label}: ${hop.status}. ${hop.summary}. Open details`}
      className={cn(
        "flex items-center gap-3 rounded-xl border bg-card p-3 text-left transition-[border-color,opacity] outline-none sm:gap-4 sm:p-4",
        "hover:border-foreground/20 focus-visible:ring-3 focus-visible:ring-ring",
        blameRing(blamed, severity),
        dim && "opacity-50",
      )}
    >
      <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
        <Icon aria-hidden className="size-4" />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="font-medium">{hop.label}</span>
          {blamed ? <BlameTag severity={severity} /> : null}
        </div>
        <div className="truncate font-mono text-xs text-muted-foreground">{hop.target ?? hop.summary}</div>
      </div>
      <Sparkline
        samples={samples ?? []}
        now={now}
        label={`${hop.label} latency, last 15 minutes`}
        className={cn("hidden h-8 w-24 shrink-0 sm:block md:w-40", hop.status === "fail" ? "text-fail" : hop.status === "warn" ? "text-warn" : "text-brand")}
      />
      <div className="flex shrink-0 flex-col items-end gap-1">
        <span className={cn("tnum max-w-32 truncate text-sm font-semibold sm:max-w-48", hop.status !== "pass" && statusText[hop.status])}>
          {keyMetric(hop)}
        </span>
        <StatusPill status={hop.status} className="py-0 text-[11px]" />
      </div>
      <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground" />
    </button>
  )
}
