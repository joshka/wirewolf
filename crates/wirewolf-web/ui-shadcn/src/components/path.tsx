import { lazy, Suspense, useState } from "react"
import { AppWindow, AtSign, Building2, ChevronRight, Columns3, Globe, Laptop, Crosshair, Router, Rows3, type LucideIcon } from "lucide-react"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { href, onLinkClick } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Hop, HopKind, Sample, Severity, Status } from "@/lib/api"
import { ms, pct, statusLabel } from "@/lib/format"
import { CauseBadge, StatusIcon, statusText } from "./status"

export const hopIcon: Record<HopKind, LucideIcon> = {
  host: Laptop,
  gateway: Router,
  isp: Building2,
  internet: Globe,
  dns: AtSign,
  web: AppWindow,
  target: Crosshair,
}

/** The one number that matters for a hop, or its summary if it has none. */
export function keyMetric(hop: Hop): string {
  const m = hop.metrics
  if (hop.status === "skip" || (m.rtt_ms == null && m.loss_pct == null)) return hop.summary
  if ((m.loss_pct ?? 0) > 0) return m.rtt_ms == null ? `${pct(m.loss_pct)} loss` : `${ms(m.rtt_ms)} · ${pct(m.loss_pct)} loss`
  return ms(m.rtt_ms)
}

// The chart library is loaded after first paint, so the verdict and path render without it.
const Sparkline = lazy(() => import("./sparkline"))

export const dimmed = (s: Status) => s === "skip" || s === "not_applicable"

const metricTone = (s: Status) => (s === "pass" ? "text-foreground" : dimmed(s) ? "text-muted-foreground" : statusText[s])

export function IconTile({ icon: Icon }: { icon: LucideIcon }) {
  return (
    <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
      <Icon aria-hidden className="size-5" />
    </span>
  )
}

type Layout = "rows" | "cards"
const LAYOUT_KEY = "wirewolf.pathLayout"

function useLayout(): [Layout, (l: Layout) => void] {
  const [layout, setLayout] = useState<Layout>(() => (localStorage.getItem(LAYOUT_KEY) === "cards" ? "cards" : "rows"))
  return [
    layout,
    (l) => {
      localStorage.setItem(LAYOUT_KEY, l)
      setLayout(l)
    },
  ]
}

/**
 * The path from this machine to the web, as rows (one per hop, with a wide sparkline) or as six
 * cards across (from `lg` up, with small sparklines). The choice is remembered per browser; phones
 * always get rows. Each hop links to its page.
 */
export function PathDiagram({
  hops,
  blame,
  severity,
  samples,
  now,
  incident,
}: {
  hops: Hop[]
  blame: string | null
  severity: Severity
  samples?: Record<string, Sample[]>
  now?: number
  incident?: string
}) {
  const [layout, setLayout] = useLayout()
  const cards = layout === "cards"
  return (
    <section aria-label="Network path" className="flex flex-col gap-2">
      <div className="hidden items-center justify-between lg:flex">
        <h2 className="text-sm font-medium text-muted-foreground">
          Path
          {samples && !cards && <span className="font-normal"> · round-trip time, last 15 min</span>}
        </h2>
        <ToggleGroup variant="outline" size="sm" value={[layout]} onValueChange={(v) => v[0] && setLayout(v[0] as Layout)} aria-label="Path layout">
          <ToggleGroupItem value="rows" aria-label="Rows">
            <Rows3 aria-hidden />
          </ToggleGroupItem>
          <ToggleGroupItem value="cards" aria-label="Cards">
            <Columns3 aria-hidden />
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
      <ol className={cn("grid gap-0 border-y", cards && "lg:grid-cols-6 lg:gap-3 lg:overflow-visible lg:border-0 lg:bg-transparent")}>
        {hops.map((hop, i) => {
          const blamed = hop.id === blame
          const spark = samples && now != null && hop.kind !== "host"
          return (
            <li key={hop.id} className={cn("flex min-w-0", i > 0 && "border-t", cards && "lg:border-t-0")}>
              <a
                href={href({ name: "hop", id: hop.id, incident })}
                onClick={onLinkClick}
                data-hop={hop.id}
                aria-label={`${hop.label}: ${statusLabel[hop.status]}. ${hop.summary}. Open details`}
                className={cn(
                  "flex w-full min-w-0 items-center gap-3 px-0 py-2.5 text-left transition outline-none hover:bg-accent/50 focus-visible:ring-3 focus-visible:ring-ring focus-visible:ring-inset sm:px-0",
                  cards && "lg:flex-col lg:items-stretch lg:gap-2 lg:rounded-xl lg:border lg:bg-card lg:p-4",
                  blamed && (severity === "down" ? "bg-fail/5" : "bg-warn/5"),
                  blamed && cards && (severity === "down" ? "lg:border-fail" : "lg:border-warn"),
                  dimmed(hop.status) && "opacity-50",
                )}
              >
                <div className={cn("flex items-start justify-between", !cards && "contents")}>
                  <IconTile icon={hopIcon[hop.kind]} />
                  {cards && <StatusIcon status={hop.status} className="hidden lg:block" />}
                </div>
                <div className={cn("flex min-w-0 flex-1 flex-col gap-0.5 sm:w-56 sm:flex-none", cards && "lg:w-auto")}>
                  <span className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm font-medium">
                    {hop.label}
                    {blamed && <CauseBadge severity={severity} />}
                  </span>
                  <span className="truncate font-mono text-xs text-muted-foreground" title={hop.target ?? undefined}>
                    {hop.target ?? "—"}
                  </span>
                  {/* Phones put the metric under the target, so a long summary can't push the row wider. */}
                  <span className={cn("truncate text-sm font-medium tabular-nums sm:hidden", metricTone(hop.status))}>{keyMetric(hop)}</span>
                </div>
                {spark ? (
                  <Suspense fallback={<span className={cn("hidden h-8 flex-1 sm:block", cards && "lg:order-last lg:mt-auto lg:h-10 lg:flex-none")} />}>
                    <Sparkline hop={hop} samples={samples[hop.id] ?? []} now={now} className={cn("hidden flex-1 sm:block", cards && "lg:order-last lg:mt-auto lg:h-10 lg:flex-none")} />
                  </Suspense>
                ) : (
                  <span className={cn("hidden flex-1 sm:block", cards && "lg:hidden")} />
                )}
                <span
                  className={cn(
                    "hidden shrink-0 truncate text-right text-sm font-medium tabular-nums sm:block sm:w-40",
                    cards && "lg:w-auto lg:text-left",
                    metricTone(hop.status),
                  )}
                  title={hop.summary}
                >
                  {keyMetric(hop)}
                </span>
                <StatusIcon status={hop.status} className={cn(cards && "lg:hidden")} />
                <ChevronRight aria-hidden className={cn("size-4 shrink-0 text-muted-foreground", cards && "lg:hidden")} />
              </a>
            </li>
          )
        })}
      </ol>
    </section>
  )
}
