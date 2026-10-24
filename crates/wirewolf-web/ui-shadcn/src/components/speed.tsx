import { useState } from "react"
import { ArrowDown, ArrowUp, Loader2, Play } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { api, type BloatGrade, type SpeedRun } from "@/lib/api"
import { ofPlan, PLAN_SHORTFALL, useLive } from "@/lib/live"
import { useSession, writeBlocked } from "@/lib/session"
import { Gated } from "./chrome"
import { cn } from "@/lib/utils"
import { bytes } from "@/lib/format"
import { RelTime } from "./time"
import { Badge } from "@/components/ui/badge"

export function useSpeed() {
  const runs = useLive((s) => s.speedRuns)
  const running = runs.find((r) => r.state === "running") ?? null
  // A failed test has no numbers, so it mustn't hide the last good result. It's reported
  // separately, and only while it's newer than that result.
  const latest = runs.find((r) => r.state === "complete") ?? null
  const finished = runs.find((r) => r.state !== "running") ?? null
  const failed = finished?.state === "failed" ? finished : null
  return { runs, running, latest, failed }
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

export function RunSpeedButton({ running, primary }: { running: boolean; primary?: boolean }) {
  const [busy, setBusy] = useState(false)
  const start = async () => {
    setBusy(true)
    try {
      const status = await api.runSpeed()
      if (status === 409) toast.info("A speed test is already running")
      else if (status === 401 || status === 403) toast.error("Sign in to run a speed test")
      else if (status !== 202) toast.error(`Couldn't start a speed test (HTTP ${status})`)
    } catch {
      toast.error("Couldn't reach wirewolf to start a speed test")
    } finally {
      setBusy(false)
    }
  }
  const down = useLive((s) => s.snapshot?.verdict.severity === "down")
  const confirmed = () =>
    !down || window.confirm("The connection is down, so a speed test will almost certainly fail. Run it anyway?")
  const active = running || busy
  const blocked = writeBlocked(useSession(), "run a speed test")
  return (
    <Gated reason={blocked}>
      <Button variant={primary ? "default" : "outline"} onClick={() => confirmed() && start()} disabled={active || !!blocked}>
        {active ? <Loader2 className="animate-spin" aria-hidden /> : <Play aria-hidden />}
        {running ? "Running…" : "Run now"}
      </Button>
    </Gated>
  )
}

/**
 * What a running test is measuring. The API reports only overall progress, but the phases fill
 * in their numbers in order: latency, then download, then upload.
 */
export function speedPhase(run: SpeedRun): string {
  if (run.up_mbps != null) return "Upload"
  if (run.down_mbps != null) return "Download"
  return "Latency"
}

export function SpeedProgress({ run }: { run: SpeedRun }) {
  const p = Math.round((run.progress ?? 0) * 100)
  return (
    <div className="flex items-center gap-3 text-sm">
      <span className="w-20 text-muted-foreground" aria-live="polite">
        {speedPhase(run)}
      </span>
      <Progress value={p} aria-label="Speed test progress" className="flex-1" />
      <span className="w-10 text-right tabular-nums">{p}%</span>
    </div>
  )
}

/**
 * "92% of plan" under a result, in the warn colour below 80%. Nothing when no plan is set, or
 * while the test is still running (a partial figure would read as a shortfall).
 */
function PlanShare({ value, plan, running }: { value: number | null | undefined; plan: number | null; running: boolean }) {
  const share = ofPlan(value, plan)
  if (share == null || running) return null
  return (
    <div className={cn("text-xs tabular-nums", share < PLAN_SHORTFALL ? "text-warn" : "text-muted-foreground")}>
      {Math.round(share * 100)}% of {plan} Mbps plan
    </div>
  )
}

/** A note for a test that failed after `run` (the last good result) was measured. */
function FailedNote({ failed }: { failed: SpeedRun }) {
  return (
    <p role="status" className="text-sm text-fail">
      Last attempt failed: {failed.error ?? "unknown error"} · <RelTime at={failed.started} />
    </p>
  )
}

/**
 * The speed result body (spec: big down/up, then Idle / Loaded / Bufferbloat tiles, then a meta
 * line). Shows whatever a running test has measured so far.
 */
export function SpeedSummary({ run, running, failed }: { run: SpeedRun | null; running: boolean; failed?: SpeedRun | null }) {
  const num = (v: number | null | undefined, digits = 0) => (v == null ? "—" : v.toFixed(digits))
  const big = "text-3xl sm:text-4xl"
  const plan = useLive((s) => s.plan)
  return (
    <div className="flex flex-col gap-4">
      <dl className="grid grid-cols-2 gap-4">
        {[
          { icon: ArrowDown, label: "Download", value: num(run?.down_mbps), raw: run?.down_mbps, of: plan.down },
          { icon: ArrowUp, label: "Upload", value: num(run?.up_mbps), raw: run?.up_mbps, of: plan.up },
        ].map(({ icon: Icon, label, value, raw, of }) => (
          <div key={label} className="min-w-0">
            <dt className="flex items-center gap-1 text-xs text-muted-foreground">
              <Icon aria-hidden className="size-3.5" />
              {label}
            </dt>
            <dd className="tabular-nums mt-2 flex items-baseline gap-1">
              <span className={cn("font-semibold tracking-tight", big)}>{value}</span>
              <span className="text-2xl text-muted-foreground">Mbps</span>
            </dd>
            <PlanShare value={raw} plan={of} running={running} />
          </div>
        ))}
      </dl>
      <dl className="grid grid-cols-3 gap-2">
        {[
          ["Idle", num(run?.idle_latency_ms, 1), "ms"],
          ["Loaded", num(run?.loaded_latency_ms, 1), "ms"],
        ].map(([k, v, u]) => (
          <div key={k} className="border-l pl-3">
            <dt className="text-xs text-muted-foreground">{k}</dt>
            <dd className="tabular-nums mt-1 flex items-baseline gap-1">
              <span className="text-lg font-semibold">{v}</span>
              <span className="text-xs text-muted-foreground">{u}</span>
            </dd>
          </div>
        ))}
        <div className="border-l pl-3">
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
      {failed && !running && <FailedNote failed={failed} />}
    </div>
  )
}

/**
 * One row of numbers for the Now page: down, up, loaded latency, and grade. The Speed page has
 * the full breakdown.
 */
export function SpeedCompact({ run, running, failed }: { run: SpeedRun | null; running: boolean; failed?: SpeedRun | null }) {
  const num = (v: number | null | undefined, digits = 0) => (v == null ? "—" : v.toFixed(digits))
  const plan = useLive((s) => s.plan)
  const stats = [
    { label: "Down", value: num(run?.down_mbps), unit: "Mbps", raw: run?.down_mbps, of: plan.down },
    { label: "Up", value: num(run?.up_mbps), unit: "Mbps", raw: run?.up_mbps, of: plan.up },
    { label: "Loaded", value: num(run?.loaded_latency_ms), unit: "ms", raw: null, of: null },
  ]
  return (
    <div className="flex flex-col gap-3">
      <dl className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        {stats.map(({ label, value, unit, raw, of }) => (
          <div key={label} className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="tabular-nums flex items-baseline gap-1">
              <span className="text-3xl font-semibold tracking-tight">{value}</span>
              <span className="text-xs text-muted-foreground">{unit}</span>
            </dd>
            <PlanShare value={raw} plan={of} running={running} />
          </div>
        ))}
        <div>
          <dt className="text-xs text-muted-foreground">Bloat</dt>
          <dd className="mt-1">
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
          </p>
        )
      )}
      {failed && !running && <FailedNote failed={failed} />}
    </div>
  )
}
