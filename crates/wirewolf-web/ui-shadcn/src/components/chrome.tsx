import { Check, ChevronDown, Monitor, Moon, Sun } from "lucide-react"
import { useTheme } from "next-themes"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { api } from "@/lib/api"
import { reloadMeta, useLive } from "@/lib/live"
import { cn } from "@/lib/utils"
import { relative } from "@/lib/format"
import { useNow } from "./time"

const THEMES = ["system", "light", "dark"] as const
const themeIcon = { system: Monitor, light: Sun, dark: Moon }

/** Shows the current mode; each click cycles system → light → dark. */
export function ThemeToggle() {
  const { theme = "system", setTheme } = useTheme()
  const current = (THEMES as readonly string[]).includes(theme) ? (theme as (typeof THEMES)[number]) : "system"
  const next = THEMES[(THEMES.indexOf(current) + 1) % THEMES.length]
  const Icon = themeIcon[current]
  return (
    <Tooltip>
      <TooltipTrigger
        render={<Button variant="ghost" size="icon" aria-label={`Theme: ${current}. Switch to ${next}`} onClick={() => setTheme(next)} />}
      >
        <Icon aria-hidden strokeWidth={1.75} />
      </TooltipTrigger>
      <TooltipContent>Theme: {current}</TooltipContent>
    </Tooltip>
  )
}

/** SSE connection state: green "Live", amber "Reconnecting…", muted "Probes paused". */
export function LiveIndicator() {
  const connection = useLive((s) => s.connection)
  const lostAt = useLive((s) => s.lostAt)
  const at = useLive((s) => s.snapshot?.at)
  const paused = useLive((s) => s.speedRuns.some((r) => r.state === "running"))
  const now = useNow(1000)
  const live = connection === "live"
  const state = !live ? "reconnecting" : paused ? "paused" : "live"
  const label = { live: "Live", paused: "Probes paused", reconnecting: connection === "connecting" ? "Connecting…" : "Reconnecting…" }[state]
  return (
    <Tooltip>
      <TooltipTrigger
        render={<span tabIndex={0} role="status" />}
        className={cn(
          "inline-flex h-6 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium outline-none focus-visible:ring-3 focus-visible:ring-ring",
          state === "live" && "text-foreground",
          state === "paused" && "text-muted-foreground",
          state === "reconnecting" && "border-warn/40 bg-warn/10 text-warn",
        )}
      >
        <span className="relative flex size-1.5">
          {state === "live" && <span className="absolute inline-flex size-full animate-ping rounded-full bg-pass opacity-60 motion-reduce:hidden" />}
          <span className={cn("relative inline-flex size-1.5 rounded-full", { live: "bg-pass", paused: "bg-muted-foreground", reconnecting: "bg-warn" }[state])} />
        </span>
        <span>{label}</span>
        {state === "reconnecting" && lostAt && <span className="hidden font-normal sm:inline">· lost {relative(new Date(lostAt), now)}</span>}
      </TooltipTrigger>
      <TooltipContent>
        {state === "reconnecting"
          ? "Lost the connection to wirewolf. Retrying…"
          : `${state === "paused" ? "Probes pause while a speed test runs. " : ""}Streaming updates${at ? ` · last snapshot ${relative(at, now)}` : ""}`}
      </TooltipContent>
    </Tooltip>
  )
}

/** Demo-only scenario picker. Hidden in live mode. */
export function ScenarioSwitcher() {
  const meta = useLive((s) => s.meta)
  if (!meta || meta.mode !== "demo") return null
  const pick = async (id: string) => {
    const status = await api.setScenario(id)
    if (status !== 202) toast.error(`Couldn't switch scenario (HTTP ${status})`)
    await reloadMeta()
  }
  const current = meta.scenarios.find((s) => s.id === meta.scenario)
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={<Button variant="outline" aria-label={`Demo scenario: ${current?.name ?? "none"}`} />}
        className="h-8 gap-1 px-2.5 text-[13px] font-medium"
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
  )
}
