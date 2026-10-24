import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { SpeedChart } from "@/components/charts"
import { BloatBadge, RunSpeedButton, SpeedSummary, useSpeed } from "@/components/speed"
import { RelTime } from "@/components/time"
import { useLive } from "@/lib/live"
import { bytes, mbps, ms } from "@/lib/format"

export function SpeedView() {
  const { runs, running, latest, failed } = useSpeed()
  const incidents = useLive((s) => s.incidents)
  const done = runs.filter((r) => r.state === "complete").length
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">Throughput, and latency under load (bufferbloat).</p>

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
          </CardDescription>
        </CardHeader>
        <CardContent className="px-2 sm:px-4">
          <SpeedChart runs={runs} incidents={incidents} />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>All tests</CardTitle>
        </CardHeader>
        <CardContent className="px-0 sm:px-4">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="pl-4 sm:pl-2">Time</TableHead>
                <TableHead className="hidden lg:table-cell">Backend</TableHead>
                <TableHead className="hidden lg:table-cell">Server</TableHead>
                <TableHead className="text-right">Down</TableHead>
                <TableHead className="text-right">Up</TableHead>
                <TableHead className="hidden text-right md:table-cell">Idle</TableHead>
                <TableHead className="hidden text-right md:table-cell">Loaded</TableHead>
                <TableHead className="text-center">Grade</TableHead>
                <TableHead className="hidden pr-4 text-right sm:table-cell sm:pr-2">Data</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody className="tabular-nums">
              {runs.map((r) => (
                <TableRow key={r.id}>
                  <TableCell className="pl-4 sm:pl-2">
                    <RelTime at={r.started} />
                    {r.state !== "complete" && <span className="ml-2 text-xs text-muted-foreground">{r.state}</span>}
                  </TableCell>
                  <TableCell className="hidden text-muted-foreground lg:table-cell">{r.backend}</TableCell>
                  <TableCell className="hidden text-muted-foreground lg:table-cell">{r.server ?? "—"}</TableCell>
                  <TableCell className="text-right">{mbps(r.down_mbps)}</TableCell>
                  <TableCell className="text-right">{mbps(r.up_mbps)}</TableCell>
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
        </CardContent>
      </Card>
    </div>
  )
}
