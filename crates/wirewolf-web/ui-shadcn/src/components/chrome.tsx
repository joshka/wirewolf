import { Check, FlaskConical, Monitor, Moon, Sun } from "lucide-react"
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
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { api } from "@/lib/api"
import { reloadMeta, useLive } from "@/lib/live"
import { cn } from "@/lib/utils"
import { relative } from "@/lib/format"
import { useNow } from "./time"

export function ThemeToggle() {
  const { theme, setTheme } = useTheme()
  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={<Button variant="ghost" size="icon" aria-label="Theme" />}>
        <Sun aria-hidden className="dark:hidden" />
        <Moon aria-hidden className="hidden dark:block" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-36">
        <DropdownMenuRadioGroup value={theme ?? "system"} onValueChange={(v) => setTheme(String(v))}>
          <DropdownMenuRadioItem value="light">
            <Sun aria-hidden /> Light
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="dark">
            <Moon aria-hidden /> Dark
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="system">
            <Monitor aria-hidden /> System
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/** SSE connection state: a calm pulsing dot when live, a loud pill when not. */
export function LiveIndicator({ compact }: { compact?: boolean }) {
  const connection = useLive((s) => s.connection)
  const lostAt = useLive((s) => s.lostAt)
  const at = useLive((s) => s.snapshot?.at)
  const speedRunning = useLive((s) => s.speedRuns.some((r) => r.state === "running"))
  const now = useNow(1000)
  const live = connection === "live"
  const label = live ? "Live" : connection === "connecting" ? "Connecting" : "Reconnecting"
  return (
    <Tooltip>
      <TooltipTrigger
        render={<span tabIndex={0} role="status" />}
        className={cn(
          "inline-flex items-center gap-2 rounded-full border px-2.5 py-1 text-xs font-medium outline-none focus-visible:ring-3 focus-visible:ring-ring",
          !live && "border-warn/40 bg-warn/10 text-warn",
        )}
      >
        <span className="relative flex size-2">
          {live && <span className="absolute inline-flex size-full animate-ping rounded-full bg-pass opacity-60 motion-reduce:hidden" />}
          <span className={cn("relative inline-flex size-2 rounded-full", live ? "bg-pass" : "bg-warn")} />
        </span>
        {(!compact || !live) && label}
        {live && speedRunning && !compact && <span className="font-normal text-muted-foreground">· probes paused</span>}
        {!live && lostAt && <span className="hidden font-normal sm:inline">· lost {relative(new Date(lostAt), now)}</span>}
      </TooltipTrigger>
      <TooltipContent>
        {live
          ? `${speedRunning ? "Probes pause during the speed test. " : ""}Streaming updates${at ? ` · last ${relative(at, now)}` : ""}`
          : "Lost the connection to wirewolf. Retrying…"}
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
        render={<Button variant="outline" size="sm" aria-label={`Demo scenario: ${current?.name ?? "none"}`} />}
        className="gap-1.5 border-dashed"
      >
        <FlaskConical aria-hidden className="text-brand" />
        <span className="hidden sm:inline">Demo:</span>
        <span className="max-w-28 truncate">{current?.name ?? "—"}</span>
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
