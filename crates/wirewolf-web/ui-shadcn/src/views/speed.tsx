import { useState } from "react"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { SpeedChart } from "@/components/charts"
import { BloatBadge, RunSpeedButton, SpeedSummary, useSpeed } from "@/components/speed"
import { RelTime } from "@/components/time"
import { PLAN_SHORTFALL, useLive } from "@/lib/live"
import { bytes, mbps, ms } from "@/lib/format"
import { CsvLink, PageHeader } from "@/components/page-header"
import { Button } from "@/components/ui/button"
import type { SpeedRun } from "@/lib/api"
import { cn } from "@/lib/utils"

export function SpeedView() {
  const { runs, running, latest, failed } = useSpeed()
  const plan = useLive((s) => s.plan)
  const incidents = useLive((s) => s.incidents)
  const done = runs.filter((r) => r.state === "complete").length
  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Speed" description="Throughput, and latency under load (bufferbloat).">
        <CsvLink what="speed" />
      </PageHeader>

      <Card>
        <CardHeader>
          <CardTitle>{running ? "Running a speed test" : "Latest result"}</CardTitle>
          <CardAction>
            <RunSpeedButton running={!!running} primary />
          </CardAction>
        </CardHeader>
        <CardContent>
          {running || latest || failed ? (
            <div className="max-w-3xl">
              <SpeedSummary run={running ?? latest} running={!!running} failed={failed} />
            </div>
          ) : (
            <p className="py-6 text-sm text-muted-foreground">No tests yet. Run one to measure throughput and bufferbloat.</p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>History</CardTitle>
          <CardDescription>
            {done} completed test{done === 1 ? "" : "s"}
            {(plan.down != null || plan.up != null) && " · the dashed line is your plan"}
          </CardDescription>
        </CardHeader>
        <CardContent className="px-2 sm:px-4">
          <SpeedChart runs={runs} incidents={incidents} />
        </CardContent>
      </Card>

      <RunsTable runs={runs} />
    </div>
  )
}

/** Runs shown before "Show all". */
const RECENT_RUNS = 10

/**
 * Every run, newest first: the latest ten, then the rest on request. A column whose values are
 * all the same (one backend, one server) moves into the description instead of repeating on
 * every row, and slow speeds are coloured: under 80% of the plan if one is set, otherwise under
 * half the median.
 */
function RunsTable({ runs }: { runs: SpeedRun[] }) {
  const [all, setAll] = useState(false)
  const done = runs.filter((r) => r.state === "complete")
  const median = (xs: number[]) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)]
  const mid = { down: median(done.map((r) => r.down_mbps ?? 0)), up: median(done.map((r) => r.up_mbps ?? 0)) }
  const plan = useLive((s) => s.plan)
  // Against the plan when one is set, otherwise against this network's own median.
  const slow = (v: number | null, m: number | undefined, of: number | null) =>
    v != null && (of != null ? v < of * PLAN_SHORTFALL : m != null && v < m / 2)
  const same = (f: (r: SpeedRun) => string | null) => new Set(runs.map(f)).size <= 1
  const oneBackend = same((r) => r.backend)
  const oneServer = same((r) => r.server)
  const shown = all ? runs : runs.slice(0, RECENT_RUNS)
  const common = [oneBackend && runs[0]?.backend, oneServer && runs[0]?.server].filter(Boolean).join(" · ")
  return (
    <Card>
      <CardHeader>
        <CardTitle>All tests</CardTitle>
        {common && <CardDescription>Every test used {common}</CardDescription>}
      </CardHeader>
      <CardContent className="px-0 sm:px-4">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="pl-4 sm:pl-2">Time</TableHead>
              {!oneBackend && <TableHead className="hidden lg:table-cell">Backend</TableHead>}
              {!oneServer && <TableHead className="hidden lg:table-cell">Server</TableHead>}
              <TableHead className="text-right">Down</TableHead>
              <TableHead className="text-right">Up</TableHead>
              <TableHead className="hidden text-right md:table-cell">Idle</TableHead>
              <TableHead className="hidden text-right md:table-cell">Loaded</TableHead>
              <TableHead className="text-center">Grade</TableHead>
              <TableHead className="hidden pr-4 text-right sm:table-cell sm:pr-2">Data</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody className="tabular-nums">
            {shown.map((r) => (
              <TableRow key={r.id}>
                <TableCell className="pl-4 sm:pl-2">
                  <RelTime at={r.started} />
                  {r.state !== "complete" && <span className="ml-2 text-xs text-muted-foreground">{r.state}</span>}
                </TableCell>
                {!oneBackend && <TableCell className="hidden text-muted-foreground lg:table-cell">{r.backend}</TableCell>}
                {!oneServer && <TableCell className="hidden text-muted-foreground lg:table-cell">{r.server ?? "—"}</TableCell>}
                <TableCell className={cn("text-right", slow(r.down_mbps, mid.down, plan.down) && "text-warn")}>{mbps(r.down_mbps)}</TableCell>
                <TableCell className={cn("text-right", slow(r.up_mbps, mid.up, plan.up) && "text-warn")}>{mbps(r.up_mbps)}</TableCell>
                <TableCell className="hidden text-right md:table-cell">{ms(r.idle_latency_ms)}</TableCell>
                <TableCell className="hidden text-right md:table-cell">{ms(r.loaded_latency_ms)}</TableCell>
                <TableCell className="text-center">
                  <BloatBadge grade={r.bloat} className="text-xs" />
                </TableCell>
                <TableCell className="hidden pr-4 text-right text-muted-foreground sm:table-cell sm:pr-2">{bytes(r.bytes_used)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        {runs.length > RECENT_RUNS && (
          <Button variant="ghost" size="sm" className="mt-2 ml-2" onClick={() => setAll(!all)}>
            {all ? "Show the latest 10" : `Show all ${runs.length}`}
          </Button>
        )}
      </CardContent>
    </Card>
  )
}
