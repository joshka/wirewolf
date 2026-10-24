import { useSyncExternalStore } from "react"
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

interface PathProps {
  hops: Hop[]
  blame: string | null
  severity: Severity
  onSelect: (hop: Hop, opener: HTMLElement) => void
}

/** The path from this machine to the web: six cards from `lg` up, list rows below. */
const DESKTOP = "(min-width: 1024px)"

/** Matches the Tailwind `lg` breakpoint. Rendering one layout keeps `data-hop` selectors unique. */
function useDesktop(): boolean {
  return useSyncExternalStore(
    (cb) => {
      const m = matchMedia(DESKTOP)
      m.addEventListener("change", cb)
      return () => m.removeEventListener("change", cb)
    },
    () => matchMedia(DESKTOP).matches,
  )
}

/** The path from this machine to the web: six cards from `lg` up, list rows below. */
export function PathDiagram({ hops, blame, severity, onSelect }: PathProps) {
  const desktop = useDesktop()
  return (
    <section aria-label="Network path">
      {desktop ? (
        <ol className="flex items-stretch">
          {hops.map((hop, i) => (
            <li key={hop.id} className="flex min-w-0 flex-1 items-center">
              {i > 0 && <Connector status={hop.status} horizontal />}
              <HopCard hop={hop} blamed={hop.id === blame} severity={severity} onSelect={onSelect} />
            </li>
          ))}
        </ol>
      ) : (
        <ol className="flex flex-col">
          {hops.map((hop, i) => (
            <li key={hop.id} className="flex flex-col">
              {i > 0 && <Connector status={hop.status} />}
              <HopRow hop={hop} blamed={hop.id === blame} severity={severity} onSelect={onSelect} />
            </li>
          ))}
        </ol>
      )}
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

const blameFrame = (blamed: boolean, severity: Severity) =>
  blamed ? (severity === "down" ? "border-fail bg-fail/[0.06]" : "border-warn bg-warn/[0.06]") : ""

/** Passing metrics are plain foreground; color only means something is wrong. */
const metricClass = (s: Status) => (s === "pass" ? "text-foreground" : s === "skip" || s === "not_applicable" ? "text-muted-foreground" : statusText[s])

export function IconTile({ icon: Icon, className }: { icon: LucideIcon; className?: string }) {
  return (
    <span className={cn("grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground", className)}>
      <Icon aria-hidden className="size-5" strokeWidth={1.75} />
    </span>
  )
}

function HopCard({ hop, blamed, severity, onSelect }: HopProps) {
  const dim = hop.status === "skip" || hop.status === "not_applicable"
  return (
    <button
      type="button"
      data-hop={hop.id}
      onClick={(e) => onSelect(hop, e.currentTarget)}
      aria-label={`${hop.label}: ${statusLabel[hop.status]}. ${hop.summary}. Open details`}
      className={cn(
        "flex min-h-[132px] min-w-0 flex-1 flex-col gap-2 self-stretch rounded-xl border bg-card p-4 text-left transition-[border-color,opacity] outline-none",
        "hover:border-foreground/25 focus-visible:ring-3 focus-visible:ring-ring",
        blameFrame(blamed, severity),
        dim && "opacity-50 hover:opacity-80",
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <IconTile icon={hopIcon[hop.kind]} />
        <StatusIcon status={hop.status} />
      </div>
      <div className="flex min-w-0 flex-col gap-1">
        <span className="text-sm font-medium">{hop.label}</span>
        {blamed && <CauseBadge severity={severity} />}
        <span className="truncate font-mono text-xs text-muted-foreground" title={hop.target ?? undefined}>
          {hop.target ?? "—"}
        </span>
      </div>
      <div className={cn("tnum mt-auto truncate text-sm font-medium", metricClass(hop.status))} title={hop.summary}>
        {keyMetric(hop)}
      </div>
    </button>
  )
}

function HopRow({ hop, blamed, severity, onSelect }: HopProps) {
  const dim = hop.status === "skip" || hop.status === "not_applicable"
  return (
    <button
      type="button"
      data-hop={hop.id}
      onClick={(e) => onSelect(hop, e.currentTarget)}
      aria-label={`${hop.label}: ${statusLabel[hop.status]}. ${hop.summary}. Open details`}
      className={cn(
        "flex min-h-14 items-center gap-3 rounded-xl border bg-card px-4 py-3 text-left transition-[border-color,opacity] outline-none",
        "hover:border-foreground/25 focus-visible:ring-3 focus-visible:ring-ring",
        blameFrame(blamed, severity),
        dim && "opacity-50",
      )}
    >
      <IconTile icon={hopIcon[hop.kind]} />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-sm font-medium">{hop.label}</span>
        {blamed && <CauseBadge severity={severity} />}
        <span className="truncate font-mono text-xs text-muted-foreground">{hop.target ?? "—"}</span>
        <span className={cn("tnum flex min-w-0 items-center gap-1.5 text-sm font-medium", metricClass(hop.status))}>
          <StatusIcon status={hop.status} className="size-3.5" />
          <span className="sr-only">{statusLabel[hop.status]}:</span>
          <span className="truncate">{keyMetric(hop)}</span>
        </span>
      </div>
      <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground" />
    </button>
  )
}
