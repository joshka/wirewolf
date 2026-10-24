import { Check, ChevronDown, LogIn, LogOut, Moon, Sun, UserRound } from "lucide-react"
import { useTheme } from "next-themes"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { api } from "@/lib/api"
import { reloadMeta, useLive } from "@/lib/live"
import { signIn, signOut, useSession, writeBlocked } from "@/lib/session"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"

export function ThemeToggle() {
  const { theme, setTheme } = useTheme()
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="ghost" size="icon" aria-label="Theme" />}>
        <Sun aria-hidden className="dark:hidden" />
        <Moon aria-hidden className="hidden dark:block" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup value={theme} onValueChange={(v) => setTheme(String(v))}>
          <DropdownMenuRadioItem value="light">Light</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="dark">Dark</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="system">System</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/** SSE connection state: "Live", "Reconnecting…", or "Probes paused" during a speed test. */
export function LiveIndicator() {
  const connection = useLive((s) => s.connection)
  const paused = useLive((s) => s.speedRuns.some((r) => r.state === "running"))
  const state = connection !== "live" ? "reconnecting" : paused ? "paused" : "live"
  return (
    // Only the stable label is in the status region, so screen readers hear state changes, not ticks.
    <span
      role="status"
      title={state === "paused" ? "Probes pause while a speed test runs" : state === "reconnecting" ? "Lost the connection to wirewolf; retrying" : "Streaming updates"}
      className={cn(
        "inline-flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full border px-2.5 text-xs font-medium",
        state === "paused" && "text-muted-foreground",
        state === "reconnecting" && "border-warn/40 bg-warn/10 text-warn",
      )}
    >
      <span className={cn("size-1.5 rounded-full", { live: "bg-pass", paused: "bg-muted-foreground", reconnecting: "bg-warn" }[state])} />
      {{ live: "Live", paused: "Probes paused", reconnecting: connection === "connecting" ? "Connecting…" : "Reconnecting…" }[state]}
    </span>
  )
}

/** Demo-only scenario picker. Hidden in live mode. */
export function ScenarioSwitcher() {
  const meta = useLive((s) => s.meta)
  const session = useSession()
  if (!meta || meta.mode !== "demo") return null
  const pick = async (id: string) => {
    const status = await api.setScenario(id)
    if (status !== 202) toast.error(`Couldn't switch scenario (HTTP ${status})`)
    await reloadMeta()
  }
  const current = meta.scenarios.find((s) => s.id === meta.scenario)
  const blocked = writeBlocked(session, "switch scenarios")
  return (
    <Gated reason={blocked}>
      <DropdownMenu>
        <DropdownMenuTrigger
          disabled={!!blocked}
          render={<Button variant="outline" aria-label={`Demo scenario: ${current?.name ?? "none"}`} />}
          className="h-8 gap-1 px-2.5 text-sm font-medium"
        >
          <span className="text-muted-foreground">Demo:</span>
          <span className="max-w-24 truncate sm:max-w-32">{current?.name ?? "—"}</span>
          <ChevronDown aria-hidden className="size-3.5 text-muted-foreground" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-72">
          <DropdownMenuGroup>
            <DropdownMenuLabel>Demo scenario</DropdownMenuLabel>
            {meta.scenarios.map((s) => (
              <DropdownMenuItem key={s.id} onClick={() => pick(s.id)} className="items-start">
                <Check aria-hidden className={cn("mt-0.5", s.id !== meta.scenario && "invisible")} />
                <div>
                  <div className="font-medium">{s.name}</div>
                  <div className="text-xs text-muted-foreground">{s.description}</div>
                </div>
              </DropdownMenuItem>
            ))}
          </DropdownMenuGroup>
          <DropdownMenuSeparator />
          <p className="px-2 py-1.5 text-xs text-muted-foreground">Only shown in demo mode.</p>
        </DropdownMenuContent>
      </DropdownMenu>
    </Gated>
  )
}

/** Wraps a control that may be disabled, explaining why on hover and focus. */
export function Gated({ reason, children }: { reason: string | null; children: React.ReactNode }) {
  if (!reason) return <>{children}</>
  return (
    <Tooltip>
      <TooltipTrigger render={<span tabIndex={0} className="inline-flex rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring" />}>
        {children}
      </TooltipTrigger>
      <TooltipContent>{reason}</TooltipContent>
    </Tooltip>
  )
}

/** Sign in or out. Hidden when the server has no sign-in configured. */
export function AccountControl() {
  const session = useSession()
  if (!session || session.methods.length === 0) return null
  const user = session.user
  if (!user) {
    if (!session.login_url) return null
    return (
      <Button variant="outline" size="sm" className="h-6 shrink-0 gap-1 px-2 text-xs" onClick={() => signIn(session.login_url!)}>
        <LogIn aria-hidden className="size-3.5" />
        Sign in
      </Button>
    )
  }
  const label = user.name ?? user.email ?? user.id
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button variant="ghost" size="sm" aria-label={`Signed in as ${label}`} />}
        className="h-6 min-w-0 max-w-full gap-1 px-1.5 text-xs"
      >
        <UserRound aria-hidden className="size-3.5 shrink-0" />
        <span className="truncate">{label}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuGroup>
          <DropdownMenuLabel>
            <div className="truncate font-medium text-foreground">{label}</div>
            {user.email && user.email !== label && <div className="truncate text-xs font-normal">{user.email}</div>}
          </DropdownMenuLabel>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        {session.logout_url ? (
          <DropdownMenuItem onClick={() => signOut(session)}>
            <LogOut aria-hidden />
            Sign out
          </DropdownMenuItem>
        ) : (
          <p className="px-2 py-1.5 text-xs text-muted-foreground">Signed in by your proxy. Sign out there.</p>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
