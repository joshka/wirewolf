import { useState } from "react"
import { ArrowDown, ArrowUp, Gauge, Loader2, Play } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { api, type BloatGrade, type SpeedRun } from "@/lib/api"
import { useLive } from "@/lib/live"
import { cn } from "@/lib/utils"
import { RelTime } from "./time"

export function useSpeed() {
  const runs = useLive((s) => s.speedRuns)
  const running = runs.find((r) => r.state === "running") ?? null
  const latest = runs.find((r) => r.state !== "running") ?? null
  return { runs, running, latest }
}

export const gradeClass = (g: BloatGrade | null) =>
  g == null
    ? "bg-muted text-muted-foreground"
    : g === "A+" || g === "A"
      ? "bg-pass/15 text-pass"
      : g === "B" || g === "C"
        ? "bg-warn/15 text-warn"
        : "bg-fail/15 text-fail"

export function BloatBadge({ grade, className }: { grade: BloatGrade | null; className?: string }) {
  return (
    <span
      className={cn("inline-grid min-w-8 place-items-center rounded-md px-1.5 py-0.5 text-sm font-bold", gradeClass(grade), className)}
      aria-label={`Bufferbloat grade ${grade ?? "unknown"}`}
    >
      {grade ?? "—"}
    </span>
  )
}

export function RunSpeedButton({ running, size = "sm" }: { running: boolean; size?: "sm" | "default" | "lg" }) {
  const [busy, setBusy] = useState(false)
  const start = async () => {
    setBusy(true)
    try {
      const status = await api.runSpeed()
      if (status === 409) toast.info("A speed test is already running")
      else if (status !== 202) toast.error(`Couldn't start a speed test (HTTP ${status})`)
    } catch {
      toast.error("Couldn't reach wirewolf to start a speed test")
    } finally {
      setBusy(false)
    }
  }
  const active = running || busy
  return (
    <Button size={size} onClick={start} disabled={active} className="bg-brand text-white hover:bg-brand/90">
      {active ? <Loader2 className="animate-spin" aria-hidden /> : <Play aria-hidden />}
      {running ? "Running…" : "Run now"}
    </Button>
  )
}

export function SpeedProgress({ run }: { run: SpeedRun }) {
  const p = Math.round((run.progress ?? 0) * 100)
  const phase = p < 10 ? "Measuring idle latency" : p < 55 ? "Testing download" : p < 95 ? "Testing upload" : "Finishing"
  return (
    <div className="space-y-2" aria-live="polite">
      <div className="flex items-center justify-between text-sm">
        <span className="text-muted-foreground">{phase}…</span>
        <span className="tnum font-medium">{p}%</span>
      </div>
      <Progress value={p} aria-label="Speed test progress" />
    </div>
  )
}

/** Down, up, and latency in a compact row. Uses whatever a running test has measured so far. */
export function SpeedNumbers({ run, large }: { run: SpeedRun | null; large?: boolean }) {
  const num = (v: number | null | undefined, digits = 0) => (v == null ? "—" : v.toFixed(digits))
  const items = [
    { icon: ArrowDown, label: "Download", value: num(run?.down_mbps), unit: "Mbps" },
    { icon: ArrowUp, label: "Upload", value: num(run?.up_mbps), unit: "Mbps" },
    { icon: Gauge, label: "Idle latency", value: num(run?.idle_latency_ms, 1), unit: "ms" },
    { icon: Gauge, label: "Loaded latency", value: num(run?.loaded_latency_ms, 1), unit: "ms" },
  ]
  return (
    <dl className={cn("grid grid-cols-2 gap-x-4 gap-y-3", large && "sm:grid-cols-4")}>
      {items.map(({ icon: Icon, label, value, unit }) => (
        <div key={label} className="min-w-0">
          <dt className="flex items-center gap-1 text-xs text-muted-foreground">
            <Icon aria-hidden className="size-3.5" />
            {label}
          </dt>
          <dd className="tnum mt-0.5 flex items-baseline gap-1">
            <span className={cn("font-semibold tracking-tight", large ? "text-3xl sm:text-4xl" : "text-2xl")}>{value}</span>
            <span className="text-xs text-muted-foreground">{unit}</span>
          </dd>
        </div>
      ))}
    </dl>
  )
}

export function SpeedMeta({ run }: { run: SpeedRun }) {
  return (
    <p className="text-xs text-muted-foreground">
      <RelTime at={run.started} /> · {run.server ?? run.backend}
    </p>
  )
}
