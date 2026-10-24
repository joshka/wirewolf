import { useEffect } from "react"
import { Activity, ChartLine, Flag, Gauge, type LucideIcon } from "lucide-react"
import { ThemeProvider } from "next-themes"
import { Badge } from "@/components/ui/badge"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"
import { LiveIndicator, ScenarioSwitcher, ThemeToggle } from "@/components/chrome"
import { start, useLive } from "@/lib/live"
import { href, onLinkClick, useRoute, type Route } from "@/lib/router"
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
        <SidebarProvider>
          <AppSidebar />
          <Main />
        </SidebarProvider>
        <Toaster position="top-center" />
      </TooltipProvider>
    </ThemeProvider>
  )
}

function AppSidebar() {
  const route = useRoute()
  const meta = useLive((s) => s.meta)
  const open = useLive((s) => s.incidents.filter((i) => !i.ended).length)
  const { setOpenMobile } = useSidebar()
  const go = (e: React.MouseEvent<HTMLAnchorElement>) => {
    onLinkClick(e)
    setOpenMobile(false)
  }
  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" render={<a href={href({ name: "now" })} onClick={go} />}>
              <span className="grid size-8 place-items-center rounded-lg bg-primary text-primary-foreground">
                <Activity aria-hidden className="size-4" />
              </span>
              <span className="font-semibold">wirewolf</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarMenu>
            {NAV.map((n) => (
              <SidebarMenuItem key={n.label}>
                <SidebarMenuButton
                  isActive={isActive(n.route, route)}
                  tooltip={n.label}
                  render={<a href={href(n.route)} onClick={go} aria-current={isActive(n.route, route) ? "page" : undefined} />}
                >
                  <n.icon aria-hidden />
                  <span>{n.label}</span>
                  {n.route.name === "incidents" && open > 0 && (
                    <Badge variant="count" className="ml-auto" aria-label={`${open} open`}>
                      {open}
                    </Badge>
                  )}
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>
      {meta && (
        <SidebarFooter className="px-4 text-xs text-muted-foreground group-data-[collapsible=icon]:hidden">
          <span className="font-mono">{meta.host}</span>
          <span>
            wirewolf v{meta.version} · {meta.mode}
          </span>
        </SidebarFooter>
      )}
    </Sidebar>
  )
}

function Main() {
  const route = useRoute()
  const severity = useLive((s) => s.snapshot?.verdict.severity)
  const error = useLive((s) => s.error)
  const title = route.name === "incident" ? "Incident" : (NAV.find((n) => isActive(n.route, route))?.label ?? "wirewolf")
  useEffect(() => {
    const sev = severity && severity !== "healthy" ? `⚠ ${severity} · ` : ""
    document.title = `${sev}${title} · wirewolf`
  }, [title, severity])
  return (
    <SidebarInset>
      <header className="sticky top-0 z-30 flex h-14 items-center gap-2 border-b bg-background/80 px-4 backdrop-blur">
        <SidebarTrigger aria-label="Toggle navigation" />
        <h1 className="hidden font-semibold sm:block">{title}</h1>
        <div className="ml-auto flex items-center gap-2">
          <LiveIndicator />
          <ScenarioSwitcher />
          <ThemeToggle />
        </div>
      </header>
      <main id="main" className="mx-auto w-full max-w-7xl flex-1 p-4 sm:p-6">
        {error && <p className="mb-4 rounded-lg border border-fail/30 bg-fail/10 p-3 text-sm text-fail">{error}</p>}
        {route.name === "now" && <NowView />}
        {route.name === "history" && <HistoryView />}
        {route.name === "incidents" && <IncidentsView />}
        {route.name === "incident" && <IncidentView id={route.id} />}
        {route.name === "speed" && <SpeedView />}
      </main>
    </SidebarInset>
  )
}
