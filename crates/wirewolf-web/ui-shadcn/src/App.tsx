import { useEffect } from "react"
import { Activity, ChartLine, Flag, Gauge, type LucideIcon } from "lucide-react"
import { ThemeProvider } from "next-themes"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { LiveIndicator, ScenarioSwitcher, ThemeToggle } from "@/components/chrome"
import { start, useLive } from "@/lib/live"
import { href, onLinkClick, useRoute, type Route } from "@/lib/router"
import { cn } from "@/lib/utils"
import { NowView } from "@/views/now"
import { HistoryView } from "@/views/history"
import { IncidentsView, IncidentView } from "@/views/incidents"
import { SpeedView } from "@/views/speed"

const NAV: { route: Route; label: string; icon: LucideIcon }[] = [
  { route: { name: "now" }, label: "Now", icon: Activity },
  { route: { name: "history" }, label: "History", icon: ChartLine },
  { route: { name: "incidents" }, label: "Incidents", icon: Flag },
  { route: { name: "speed" }, label: "Speed", icon: Gauge },
]

const isActive = (nav: Route, cur: Route) =>
  nav.name === cur.name || (nav.name === "incidents" && cur.name === "incident")

export default function App() {
  useEffect(start, [])
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem disableTransitionOnChange>
      <TooltipProvider delay={300}>
        <Shell />
        <Toaster position="top-center" />
      </TooltipProvider>
    </ThemeProvider>
  )
}

function Logo() {
  return (
    <a href={href({ name: "now" })} onClick={onLinkClick} className="flex items-center gap-2 rounded-md font-semibold tracking-tight outline-none focus-visible:ring-3 focus-visible:ring-ring">
      <span className="grid size-7 place-items-center rounded-lg bg-brand text-white">
        <Activity aria-hidden className="size-4" strokeWidth={2.25} />
      </span>
      wirewolf
    </a>
  )
}

function Shell() {
  const route = useRoute()
  const meta = useLive((s) => s.meta)
  const severity = useLive((s) => s.snapshot?.verdict.severity)
  const openIncidents = useLive((s) => s.incidents.filter((i) => !i.ended).length)
  const title = route.name === "incident" ? "Incident" : (NAV.find((n) => isActive(n.route, route))?.label ?? "wirewolf")
  useEffect(() => {
    const nav = NAV.find((n) => isActive(n.route, route))
    const sev = severity && severity !== "healthy" ? `⚠ ${severity} · ` : ""
    document.title = `${sev}${nav?.label ?? "wirewolf"} · wirewolf`
  }, [route, severity])

  return (
    <div className="min-h-dvh lg:grid lg:grid-cols-[15rem_1fr]">
      <a href="#main" className="sr-only z-50 rounded-md bg-background p-2 focus:not-sr-only focus:fixed focus:top-2 focus:left-2">
        Skip to content
      </a>

      {/* Desktop sidebar */}
      <aside className="sticky top-0 hidden h-dvh flex-col border-r bg-sidebar/50 p-4 lg:flex">
        <div className="px-2 py-1">
          <Logo />
        </div>
        <nav aria-label="Main" className="mt-6 flex flex-col gap-1">
          {NAV.map((n) => {
            const active = isActive(n.route, route)
            return (
              <a
                key={n.label}
                href={href(n.route)}
                onClick={onLinkClick}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "flex h-9 items-center gap-2 rounded-lg px-3 text-sm font-medium text-muted-foreground transition-colors outline-none hover:bg-accent hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring",
                  active && "bg-brand/10 text-foreground hover:bg-brand/10",
                )}
              >
                <n.icon aria-hidden strokeWidth={1.75} className={cn("size-4", active && "text-brand")} />
                {n.label}
                {n.label === "Incidents" && openIncidents > 0 && (
                  <span
                    className="ml-auto grid size-[18px] place-items-center rounded-full bg-fail text-[11px] font-semibold text-white"
                    aria-label={`${openIncidents} open`}
                  >
                    {openIncidents}
                  </span>
                )}
              </a>
            )
          })}
        </nav>
        {meta && (
          <p className="mt-auto px-3 text-xs text-muted-foreground">
            <span className="font-mono">{meta.host}</span>
            <br />
            wirewolf v{meta.version} · {meta.mode}
          </p>
        )}
      </aside>

      <div className="flex min-w-0 flex-col">
        <header className="sticky top-0 z-30 flex h-14 items-center gap-2 border-b bg-background/80 px-4 backdrop-blur supports-backdrop-filter:bg-background/60 sm:px-6">
          <div className="lg:hidden">
            <Logo />
          </div>
          <h1 className="hidden text-base font-semibold lg:block">{title}</h1>
          <div className="ml-auto flex items-center gap-2">
            <LiveIndicator />
            <ScenarioSwitcher />
            <ThemeToggle />
          </div>
        </header>

        <main id="main" className="mx-auto w-full max-w-[1400px] flex-1 px-4 pt-4 pb-28 sm:px-6 sm:pt-6 lg:pb-10">
          {route.name === "now" && <NowView />}
          {route.name === "history" && <HistoryView />}
          {route.name === "incidents" && <IncidentsView />}
          {route.name === "incident" && <IncidentView id={route.id} />}
          {route.name === "speed" && <SpeedView />}
        </main>
      </div>

      {/* Mobile tab bar */}
      <nav
        aria-label="Main"
        className="fixed inset-x-0 bottom-0 z-30 grid grid-cols-4 border-t bg-background/90 pb-[env(safe-area-inset-bottom)] backdrop-blur lg:hidden"
      >
        {NAV.map((n) => {
          const active = isActive(n.route, route)
          const alert = n.label === "Incidents" && openIncidents > 0
          return (
            <a
              key={n.label}
              href={href(n.route)}
              onClick={onLinkClick}
              aria-current={active ? "page" : undefined}
              className={cn(
                "relative flex flex-col items-center gap-1 py-2.5 text-[11px] font-medium text-muted-foreground outline-none focus-visible:bg-accent",
                active && "text-foreground",
              )}
            >
              <span className="relative">
                <n.icon aria-hidden strokeWidth={1.75} className={cn("size-5", active && "text-brand")} />
                {alert && (
                  <span
                    aria-label={`${openIncidents} open`}
                    className="absolute -top-1.5 -right-2.5 grid size-[18px] place-items-center rounded-full bg-fail text-[11px] font-semibold text-white ring-2 ring-background"
                  >
                    {openIncidents}
                  </span>
                )}
              </span>
              {n.label}
            </a>
          )
        })}
      </nav>
    </div>
  )
}
