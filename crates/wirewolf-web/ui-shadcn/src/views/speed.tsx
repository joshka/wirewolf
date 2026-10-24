import { useMemo } from "react"
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { ChartContainer, ChartLegend, ChartLegendContent, ChartTooltip, ChartTooltipContent, type ChartConfig } from "@/components/ui/chart"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { BloatBadge, RunSpeedButton, SpeedNumbers, SpeedProgress, useSpeed } from "@/components/speed"
import { RelTime } from "@/components/time"
import { bytes, clock, mbps, ms } from "@/lib/format"

const config = {
  down: { label: "Download", color: "var(--chart-1)" },
  up: { label: "Upload", color: "var(--chart-2)" },
} satisfies ChartConfig

export function SpeedView() {
  const { runs, running, latest } = useSpeed()
  const done = useMemo(() => runs.filter((r) => r.state === "complete"), [runs])
  const data = useMemo(
    () =>
      done
        .slice()
        .reverse()
        .map((r) => ({ t: new Date(r.started).getTime(), down: r.down_mbps, up: r.up_mbps })),
    [done],
  )
  const shown = running ?? latest
  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Speed</h1>
        <p className="text-sm text-muted-foreground">Throughput and latency under load (bufferbloat).</p>
      </div>

      <Card className="overflow-hidden">
        <CardHeader>
          <CardTitle>{running ? "Running a speed test" : "Latest result"}</CardTitle>
          <CardDescription>
            {shown ? (
              <>
                <RelTime at={shown.started} /> · {shown.server ?? shown.backend}
                {shown.bytes_used != null && !running && ` · ${bytes(shown.bytes_used)} used`}
              </>
            ) : (
              "No tests yet"
            )}
          </CardDescription>
          <CardAction>
            <RunSpeedButton running={!!running} size="default" />
          </CardAction>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="flex flex-col gap-6 sm:flex-row sm:items-center">
            <div className="flex-1">
              <SpeedNumbers run={shown} large />
            </div>
            <div className="flex items-center gap-3 rounded-xl border bg-muted/30 p-4 sm:flex-col sm:items-center sm:gap-1 sm:px-6">
              <BloatBadge grade={running ? null : (latest?.bloat ?? null)} className="text-2xl sm:min-w-14 sm:py-2 sm:text-3xl" />
              <div className="text-sm sm:text-center">
                <div className="font-medium">Bufferbloat</div>
                <div className="tnum text-xs text-muted-foreground">
                  {!running && latest?.loaded_latency_ms != null && latest.idle_latency_ms != null
                    ? `+${ms(latest.loaded_latency_ms - latest.idle_latency_ms)} under load`
                    : "—"}
                </div>
              </div>
            </div>
          </div>
          {running && <SpeedProgress run={running} />}
          {latest?.state === "failed" && !running && (
            <p className="rounded-lg border border-fail/30 bg-fail/10 p-3 text-sm text-fail">Last test failed: {latest.error ?? "unknown error"}</p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Throughput history</CardTitle>
          <CardDescription>{done.length} completed tests</CardDescription>
        </CardHeader>
        <CardContent className="px-2 sm:px-4">
          <ChartContainer config={config} className="aspect-auto h-64 w-full">
            <AreaChart data={data} margin={{ left: 4, right: 12, top: 8 }} accessibilityLayer>
              <defs>
                {(["down", "up"] as const).map((k) => (
                  <linearGradient key={k} id={`fill-${k}`} x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor={`var(--color-${k})`} stopOpacity={0.35} />
                    <stop offset="95%" stopColor={`var(--color-${k})`} stopOpacity={0.02} />
                  </linearGradient>
                ))}
              </defs>
              <CartesianGrid vertical={false} />
              <XAxis dataKey="t" type="number" scale="time" domain={["dataMin", "dataMax"]} tickFormatter={clock} tickLine={false} axisLine={false} minTickGap={48} />
              <YAxis width={56} tickLine={false} axisLine={false} tickFormatter={(v: number) => `${v}`} unit="" />
              <ChartTooltip
                content={
                  <ChartTooltipContent
                    indicator="line"
                    labelFormatter={(_, p) => {
                      const t = p?.[0]?.payload?.t
                      return t ? new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : ""
                    }}
                    formatter={(value, name, item) => (
                      <div className="flex w-full items-center justify-between gap-4">
                        <span className="flex items-center gap-1.5 text-muted-foreground">
                          <span className="h-2.5 w-1 rounded-sm" style={{ background: item.color }} />
                          {config[name as keyof typeof config]?.label}
                        </span>
                        <span className="tnum font-mono font-medium text-foreground">{mbps(Number(value))}</span>
                      </div>
                    )}
                  />
                }
              />
              <Area dataKey="down" type="monotone" stroke="var(--color-down)" fill="url(#fill-down)" strokeWidth={2} isAnimationActive={false} />
              <Area dataKey="up" type="monotone" stroke="var(--color-up)" fill="url(#fill-up)" strokeWidth={2} isAnimationActive={false} />
              <ChartLegend content={<ChartLegendContent />} />
            </AreaChart>
          </ChartContainer>
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
                <TableHead className="pl-4 sm:pl-2">When</TableHead>
                <TableHead className="text-right">Down</TableHead>
                <TableHead className="text-right">Up</TableHead>
                <TableHead className="hidden text-right md:table-cell">Idle</TableHead>
                <TableHead className="hidden text-right md:table-cell">Loaded</TableHead>
                <TableHead className="pr-4 text-center sm:pr-2">Grade</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody className="tnum">
              {runs.map((r) => (
                <TableRow key={r.id}>
                  <TableCell className="pl-4 sm:pl-2">
                    <RelTime at={r.started} />
                    {r.state !== "complete" && <span className="ml-2 text-xs text-muted-foreground">{r.state}</span>}
                  </TableCell>
                  <TableCell className="text-right">{mbps(r.down_mbps)}</TableCell>
                  <TableCell className="text-right">{mbps(r.up_mbps)}</TableCell>
                  <TableCell className="hidden text-right md:table-cell">{ms(r.idle_latency_ms)}</TableCell>
                  <TableCell className="hidden text-right md:table-cell">{ms(r.loaded_latency_ms)}</TableCell>
                  <TableCell className="pr-4 text-center sm:pr-2">
                    <BloatBadge grade={r.bloat} className="text-xs" />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  )
}
