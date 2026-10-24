import { useState } from "react"
import { ArrowDown, ArrowUp, Loader2, Play } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { api, type BloatGrade, type SpeedRun } from "@/lib/api"
import { useLive } from "@/lib/live"
import { cn } from "@/lib/utils"
import { bytes } from "@/lib/format"
import { RelTime } from "./time"
import { Badge } from "@/components/ui/badge"

export function useSpeed() {
  const runs = useLive((s) => s.speedRuns)
  const running = runs.find((r) => r.state === "running") ?? null
  const latest = runs.find((r) => r.state !== "running") ?? null
  return { runs, running, latest }
}

const gradeTone = (g: BloatGrade | null) =>
  g == null ? "muted" : g === "A+" || g === "A" ? "pass" : g === "B" || g === "C" ? "warn" : "fail"

export function BloatBadge({ grade, className }: { grade: BloatGrade | null; className?: string }) {
  return (
    <span aria-label={`Bufferbloat grade ${grade ?? "unknown"}`}>
      <Badge variant={gradeTone(grade)} className={cn("min-w-7", className)}>
        {grade ?? "—"}
      </Badge>
    </span>
  )
}

export function RunSpeedButton({
  running,
  size = "sm",
  primary,
}: {
  running: boolean
  size?: "sm" | "default" | "lg"
  primary?: boolean
}) {
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
    <Button
      size={size}
      variant={primary ? "default" : "outline"}
      onClick={start}
      disabled={active}
    >
      {active ? <Loader2 className="animate-spin" aria-hidden /> : <Play aria-hidden />}
      {running ? "Running…" : "Run now"}
    </Button>
  )
}

export function SpeedProgress({ run }: { run: SpeedRun }) {
  const p = Math.round((run.progress ?? 0) * 100)
  return (
    <div className="flex items-center gap-3 text-sm">
      <Progress value={p} aria-label="Speed test progress" className="flex-1" />
      <span className="w-10 text-right tabular-nums">{p}%</span>
    </div>
  )
}

/**
 * The speed result body (spec: big down/up, then Idle / Loaded / Bufferbloat tiles, then a meta
 * line). Shows whatever a running test has measured so far.
 */
export function SpeedSummary({ run, running }: { run: SpeedRun | null; running: boolean }) {
  const num = (v: number | null | undefined, digits = 0) => (v == null ? "—" : v.toFixed(digits))
  const big = "text-3xl sm:text-4xl"
  return (
    <div className="flex flex-col gap-4">
      <dl className="grid grid-cols-2 gap-4">
        {[
          { icon: ArrowDown, label: "Download", value: num(run?.down_mbps) },
          { icon: ArrowUp, label: "Upload", value: num(run?.up_mbps) },
        ].map(({ icon: Icon, label, value }) => (
          <div key={label} className="min-w-0">
            <dt className="flex items-center gap-1 text-xs text-muted-foreground">
              <Icon aria-hidden className="size-3.5" />
              {label}
            </dt>
            <dd className="tabular-nums mt-2 flex items-baseline gap-1">
              <span className={cn("font-semibold tracking-tight", big)}>{value}</span>
              <span className="text-2xl text-muted-foreground">Mbps</span>
            </dd>
          </div>
        ))}
      </dl>
      <dl className="grid grid-cols-3 gap-2">
        {[
          ["Idle", num(run?.idle_latency_ms, 1), "ms"],
          ["Loaded", num(run?.loaded_latency_ms, 1), "ms"],
        ].map(([k, v, u]) => (
          <div key={k} className="rounded-lg border p-3">
            <dt className="text-xs text-muted-foreground">{k}</dt>
            <dd className="tabular-nums mt-1 flex items-baseline gap-1">
              <span className="text-lg font-semibold">{v}</span>
              <span className="text-xs text-muted-foreground">{u}</span>
            </dd>
          </div>
        ))}
        <div className="rounded-lg border p-3">
          <dt className="text-xs text-muted-foreground">Bufferbloat</dt>
          <dd className="mt-1.5">
            <BloatBadge grade={running ? null : (run?.bloat ?? null)} />
          </dd>
        </div>
      </dl>
      {running && run ? (
        <SpeedProgress run={run} />
      ) : (
        run && (
          <p className="text-xs text-muted-foreground">
            {run.server ?? run.backend} · <RelTime at={run.started} />
            {run.bytes_used != null && ` · ${bytes(run.bytes_used)} used`}
          </p>
        )
      )}
      {run?.state === "failed" && !running && (
        <p className="rounded-lg border border-fail/30 bg-fail/10 p-3 text-sm text-fail">Last test failed: {run.error ?? "unknown error"}</p>
      )}
    </div>
  )
}
