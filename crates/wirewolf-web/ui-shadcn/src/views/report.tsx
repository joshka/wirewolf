import { useEffect, useState } from "react"
import { ClipboardCopy, Printer } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { PageHeader } from "@/components/page-header"
import { SeverityPill } from "@/components/status"
import { UptimeStrip } from "@/components/uptime"
import { useNow } from "@/components/time"
import type { Incident } from "@/lib/api"
import { copyText } from "@/lib/clipboard"
import { duration } from "@/lib/format"
import { useLive } from "@/lib/live"
import { periodReport, periodReportText } from "@/lib/report"
import { href, onLinkClick } from "@/lib/router"

const PERIODS = { "7": "7 days", "30": "30 days" } as const
type Period = keyof typeof PERIODS

const msSpan = (v: number) => duration(new Date(0).toISOString(), new Date(v).toISOString())
const when = (t: string | number) => new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })

/**
 * A printable summary of the connection over a period, for sending to an ISP: outages on their
 * side first (blamed on the ISP or internet hop), then speed against the plan, then everything
 * else. "Copy as text" gives the same summary for an email or a support chat.
 */
export function ReportView() {
  const [period, setPeriod] = useState<Period>("30")
  const incidents = useLive((s) => s.incidents)
  const runs = useLive((s) => s.speedRuns)
  const plan = useLive((s) => s.plan)
  const meta = useLive((s) => s.meta)
  const now = useNow(60_000)
  const days = Number(period)
  const r = periodReport(incidents, runs, plan, now, days)
  const share = (v: number | null, p: number | null) => (v != null && p != null ? ` · ${Math.round((v / p) * 100)}% of plan` : "")
  usePrintInLight()
  const copy = async () => {
    await copyText(periodReportText(r, plan, meta))
    toast.success("Report copied", { description: "Paste it into an email or your ISP's support chat." })
  }
  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title="Connection report"
        description={`${when(r.from)} to ${when(r.to)}${meta ? ` · wirewolf ${meta.version} on ${meta.host}` : ""}`}
      >
        <div className="flex flex-wrap items-center gap-2 print:hidden">
          <ToggleGroup variant="outline" size="sm" value={[period]} onValueChange={(v) => v[0] && setPeriod(v[0] as Period)} aria-label="Period">
            {(Object.keys(PERIODS) as Period[]).map((p) => (
              <ToggleGroupItem key={p} value={p} className="px-3">
                {PERIODS[p]}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <Button variant="outline" size="sm" onClick={copy}>
            <ClipboardCopy aria-hidden />
            Copy as text
          </Button>
          <Button size="sm" onClick={() => window.print()}>
            <Printer aria-hidden />
            Print
          </Button>
        </div>
      </PageHeader>

      <dl className="grid gap-4 sm:grid-cols-3">
        <Stat label="Provider-side outages" value={String(r.provider.length)} detail={r.providerDown > 0 ? `${msSpan(r.providerDown)} down` : "no downtime"} />
        <Stat label="All incidents" value={String(r.incidents.length)} detail={r.down > 0 ? `${msSpan(r.down)} down in total` : "no downtime"} />
        <Stat
          label="Median speed"
          value={r.tests.length ? `${Math.round(r.medianDown ?? 0)} / ${Math.round(r.medianUp ?? 0)} Mbps` : "—"}
          detail={
            r.tests.length
              ? `down / up over ${r.tests.length} tests${share(r.medianDown, plan.down)}${plan.down != null || plan.up != null ? ` · ${r.underPlan} under 80%` : ""}`
              : "no speed tests in this period"
          }
        />
      </dl>

      <UptimeStrip incidents={incidents} since={meta?.monitoring_since} now={now} days={days} />

      <IncidentTable
        title="Outages on the provider's side"
        empty="None. Every problem in this period was on your own network, or there were none."
        incidents={r.provider}
        to={r.to}
      />
      <IncidentTable
        title="Everything else"
        empty="Nothing else."
        incidents={r.incidents.filter((i) => !r.provider.includes(i))}
        to={r.to}
      />

      <p className="text-xs text-muted-foreground">
        Provider-side means wirewolf blamed the ISP's network or the internet beyond it; the router, Wi-Fi, DNS, and web checks
        count as your side. Each incident keeps the evidence from its worst moment on its own page.
      </p>
    </div>
  )
}

/**
 * Prints in the light theme. Browsers drop background colours when printing, so the dark
 * theme's light text would come out pale grey on white paper.
 */
function usePrintInLight() {
  useEffect(() => {
    const root = document.documentElement
    let wasDark = false
    const before = () => {
      wasDark = root.classList.contains("dark")
      root.classList.remove("dark")
    }
    const after = () => wasDark && root.classList.add("dark")
    window.addEventListener("beforeprint", before)
    window.addEventListener("afterprint", after)
    return () => {
      window.removeEventListener("beforeprint", before)
      window.removeEventListener("afterprint", after)
    }
  }, [])
}

function Stat({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div className="border-l pl-3">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="mt-1 text-2xl font-semibold tracking-tight tabular-nums">{value}</dd>
      <dd className="text-xs text-muted-foreground">{detail}</dd>
    </div>
  )
}

function IncidentTable({ title, empty, incidents, to }: { title: string; empty: string; incidents: Incident[]; to: number }) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-sm font-medium">{title}</h2>
      {incidents.length === 0 ? (
        <p className="text-sm text-muted-foreground">{empty}</p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Started</TableHead>
              <TableHead>Duration</TableHead>
              <TableHead>Severity</TableHead>
              <TableHead>What happened</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody className="tabular-nums">
            {incidents.map((i) => (
              <TableRow key={i.id}>
                <TableCell>{when(i.started)}</TableCell>
                <TableCell>
                  {duration(i.started, i.ended, to)}
                  {!i.ended && " (ongoing)"}
                </TableCell>
                <TableCell>
                  <SeverityPill severity={i.severity} />
                </TableCell>
                <TableCell>
                  <a href={href({ name: "incident", id: i.id })} onClick={onLinkClick} className="hover:underline">
                    {i.headline}
                  </a>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </section>
  )
}
