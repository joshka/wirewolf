import { useEffect, useMemo } from "react"
import { Activity, ChartLine, Flag, Gauge, Settings, type LucideIcon } from "lucide-react"
import { ThemeProvider } from "next-themes"
import { Badge } from "@/components/ui/badge"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
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
import { hopIcon } from "@/components/path"
import { StatusIcon } from "@/components/status"
import { AccountControl, LiveIndicator, ScenarioSwitcher, ThemeToggle } from "@/components/chrome"
import { loadSession, signIn, useSession } from "@/lib/session"
import { Button } from "@/components/ui/button"
import { start, useLive } from "@/lib/live"
import { href, onLinkClick, useRoute, type Route } from "@/lib/router"
import { NowView } from "@/views/now"
import { HistoryView } from "@/views/history"
import { IncidentsView, IncidentView } from "@/views/incidents"
import { SpeedView } from "@/views/speed"
import { SettingsView } from "@/views/settings"
import { findHop, targetHop } from "@/lib/targets"
import type { Hop } from "@/lib/api"
import { HopView } from "@/views/hop"

const NAV: { route: Route; label: string; icon: LucideIcon }[] = [
  { route: { name: "now" }, label: "Now", icon: Activity },
  { route: { name: "history" }, label: "History", icon: ChartLine },
  { route: { name: "incidents" }, label: "Incidents", icon: Flag },
  { route: { name: "speed" }, label: "Speed", icon: Gauge },
  { route: { name: "settings" }, label: "Settings", icon: Settings },
]

const isActive = (nav: Route, cur: Route) =>
  nav.name === cur.name || (nav.name === "incidents" && (cur.name === "incident" || (cur.name === "hop" && !!cur.incident)))

export default function App() {
  useEffect(() => {
    loadSession()
    start()
  }, [])
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
  const hops = useLive((s) => s.snapshot?.hops)
  const rawTargets = useLive((s) => s.snapshot?.targets)
  const targets = useMemo(() => rawTargets?.map(targetHop), [rawTargets])
  const { setOpenMobile } = useSidebar()
  const go = (e: React.MouseEvent<HTMLAnchorElement>) => {
    onLinkClick(e)
    setOpenMobile(false)
  }
  return (
    <Sidebar collapsible="icon">
      <SidebarHeader className="gap-3 p-4 group-data-[collapsible=icon]:p-2">
        <div className="flex items-center gap-2 group-data-[collapsible=icon]:flex-col">
          <a
            href={href({ name: "now" })}
            onClick={go}
            aria-label="wirewolf home"
            className="flex min-w-0 flex-1 items-center gap-2 rounded-md outline-none focus-visible:ring-2 focus-visible:ring-sidebar-ring group-data-[collapsible=icon]:flex-none"
          >
            <svg viewBox="0 0 32 32" fill="currentColor" aria-hidden="true" className="size-8 shrink-0 text-violet-500 group-data-[collapsible=icon]:size-6">
              <path fillRule="evenodd" d="M4 3 12 9H20L28 3 26 13H30L27 22 16 30 5 22 2 13H6ZM8 14 10 19 14 17ZM24 14 18 17 22 19Z" />
            </svg>
            <span className="truncate text-xl font-semibold tracking-tight group-data-[collapsible=icon]:hidden">wirewolf</span>
          </a>
          <div className="flex items-center gap-0.5 group-data-[collapsible=icon]:flex-col">
            <ThemeToggle />
            <SidebarTrigger aria-label="Toggle navigation" className="hidden md:inline-flex" />
          </div>
        </div>
        <div className="flex min-w-0 items-center gap-2 group-data-[collapsible=icon]:hidden">
          <LiveIndicator />
          {meta && <span className="min-w-8 truncate font-mono text-[11px] text-muted-foreground" title={meta.host}>{meta.host}</span>}
          <div className="ml-auto flex min-w-0 justify-end">
            <AccountControl />
          </div>
        </div>
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
        {[["Path", hops], ["Targets", targets]].map(([label, list]) =>
          list && list.length > 0 ? (
            <SidebarGroup key={label as string}>
              <SidebarGroupLabel>{label as string}</SidebarGroupLabel>
              <SidebarMenu>
                {(list as Hop[]).map((h) => {
                  const active = route.name === "hop" && !route.incident && route.id === h.id
                  const Icon = hopIcon[h.kind]
                  return (
                    <SidebarMenuItem key={h.id}>
                      <SidebarMenuButton
                        isActive={active}
                        tooltip={h.label}
                        render={<a href={href({ name: "hop", id: h.id })} onClick={go} aria-current={active ? "page" : undefined} />}
                      >
                        <Icon aria-hidden />
                        <span>{h.label}</span>
                        <StatusIcon status={h.status} className="ml-auto group-data-[collapsible=icon]:hidden" />
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  )
                })}
              </SidebarMenu>
            </SidebarGroup>
          ) : null,
        )}
      </SidebarContent>
      <SidebarFooter className="items-start gap-2 px-4 pb-4 group-data-[collapsible=icon]:hidden">
        <ScenarioSwitcher />
        {meta && (
          <span className="text-xs text-muted-foreground">wirewolf v{meta.version}</span>
        )}
      </SidebarFooter>
    </Sidebar>
  )
}

function Main() {
  const route = useRoute()
  const severity = useLive((s) => s.snapshot?.verdict.severity)
  const error = useLive((s) => s.error)
  const session = useSession()
  const locked = !!session?.reads_require_login && !session.user
  const snapshotHop = useLive((s) => (route.name === "hop" ? findHop(s.snapshot, route.id)?.label : undefined))
  const title = route.name === "incident" ? "Incident" : route.name === "hop" ? (snapshotHop ?? route.id) : (NAV.find((n) => isActive(n.route, route))?.label ?? "wirewolf")
  useEffect(() => {
    const sev = severity && severity !== "healthy" ? `⚠ ${severity} · ` : ""
    document.title = `${sev}${title} · wirewolf`
  }, [title, severity])
  return (
    <SidebarInset>
      {/* Phones only: the sidebar is a sheet there, so it needs a trigger. */}
      <header className="sticky top-0 z-30 flex h-12 items-center gap-2 border-b bg-background/80 px-3 backdrop-blur md:hidden">
        <SidebarTrigger aria-label="Open navigation" />
        <span className="font-semibold">wirewolf</span>
        <div className="ml-auto flex items-center gap-2">
          <AccountControl />
          <LiveIndicator />
        </div>
      </header>
      <main id="main" className="mx-auto w-full max-w-7xl flex-1 p-4 sm:p-6">
        {locked ? (
          <div className="mx-auto mt-24 max-w-sm space-y-3 text-center">
            <h1 className="text-xl font-semibold">Sign in to see this network</h1>
            <p className="text-sm text-muted-foreground">This wirewolf only shows its dashboard to signed-in users.</p>
            {session.login_url ? (
              <Button onClick={() => signIn(session.login_url!)}>Sign in</Button>
            ) : (
              <p className="text-sm text-muted-foreground">Sign in through the proxy in front of wirewolf.</p>
            )}
          </div>
        ) : (
          <>
            {error && <p className="mb-4 rounded-lg border border-fail/30 bg-fail/10 p-3 text-sm text-fail">{error}</p>}
            {route.name === "now" && <NowView />}
            {route.name === "history" && <HistoryView />}
            {route.name === "incidents" && <IncidentsView />}
            {route.name === "incident" && <IncidentView id={route.id} />}
            {route.name === "speed" && <SpeedView />}
            {route.name === "settings" && <SettingsView />}
            {route.name === "hop" && <HopView key={`${route.incident}/${route.id}`} id={route.id} incident={route.incident} />}
          </>
        )}
      </main>
    </SidebarInset>
  )
}
