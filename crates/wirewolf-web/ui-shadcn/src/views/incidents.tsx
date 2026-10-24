import { useEffect, useState } from "react"
import { ArrowLeft, Check, ChevronRight, ClipboardCopy, Siren } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { SnapshotView } from "@/components/snapshot"
import { SeverityPill } from "@/components/status"
import { RelTime, useNow } from "@/components/time"
import { api, type Incident, type Meta } from "@/lib/api"
import { useLive } from "@/lib/live"
import { absolute, duration, ms, pct } from "@/lib/format"
import { href, navigate, onLinkClick } from "@/lib/router"
import { cn } from "@/lib/utils"

export function IncidentsView() {
  const all = useLive((s) => s.incidents)
  const meta = useLive((s) => s.meta)
  const now = useNow()
  // Open incidents first, then newest first.
  const incidents = [...all].sort((a, b) => Number(!!a.ended) - Number(!!b.ended) || b.started.localeCompare(a.started))
  const hopLabel = (i: Incident) => i.evidence.hops.find((h) => h.id === i.blame)?.label ?? "—"
  const when = (i: Incident) =>
    i.ended ? duration(i.started, i.ended) : <span className="font-medium text-fail">ongoing · {duration(i.started, null, now)}</span>
  const tint = (i: Incident) => !i.ended && (i.severity === "down" ? "bg-fail/5" : "bg-warn/5")
  const go = (i: Incident) => navigate(href({ name: "incident", id: i.id }))
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">Periods when the verdict wasn't healthy. Open incidents are pinned to the top.</p>
      {!meta ? (
        <Skeleton className="h-64 w-full rounded-xl" />
      ) : incidents.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center gap-2 py-16 text-muted-foreground">
            <Siren aria-hidden className="size-6" />
            No incidents recorded.
          </CardContent>
        </Card>
      ) : (
        <>
          {/* Desktop: table. The headline is the row's link; the whole row is clickable too. */}
          <div className="hidden overflow-hidden rounded-xl border bg-card md:block">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="pl-4">Severity</TableHead>
                  <TableHead>Headline</TableHead>
                  <TableHead>Hop</TableHead>
                  <TableHead>Started</TableHead>
                  <TableHead className="pr-4 text-right">Duration</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {incidents.map((i) => (
                  <TableRow key={i.id} onClick={() => go(i)} className={cn("cursor-pointer", tint(i))}>
                    <TableCell className="pl-4">
                      <SeverityPill severity={i.severity} />
                    </TableCell>
                    <TableCell className="max-w-md whitespace-normal">
                      <a
                        href={href({ name: "incident", id: i.id })}
                        onClick={(e) => {
                          e.stopPropagation()
                          onLinkClick(e)
                        }}
                        className="font-medium outline-none hover:underline focus-visible:ring-3 focus-visible:ring-ring rounded-sm"
                      >
                        {i.headline}
                      </a>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{hopLabel(i)}</TableCell>
                    <TableCell>
                      <RelTime at={i.started} />
                    </TableCell>
                    <TableCell className="tabular-nums pr-4 text-right">{when(i)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
          {/* Mobile: cards. */}
          <ol className="flex flex-col gap-2 md:hidden">
            {incidents.map((i) => (
              <li key={i.id}>
                <a
                  href={href({ name: "incident", id: i.id })}
                  onClick={onLinkClick}
                  className={cn(
                    "flex items-center gap-3 rounded-xl border bg-card p-3.5 outline-none focus-visible:ring-3 focus-visible:ring-ring",
                    tint(i),
                  )}
                >
                  <div className="min-w-0 flex-1">
                    <SeverityPill severity={i.severity} />
                    <div className="mt-1.5 font-medium text-pretty">{i.headline}</div>
                    <div className="tabular-nums mt-1 text-xs text-muted-foreground">
                      <RelTime at={i.started} plain /> · {when(i)} · {hopLabel(i)}
                    </div>
                  </div>
                  <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground" />
                </a>
              </li>
            ))}
          </ol>
        </>
      )}
    </div>
  )
}

export function IncidentView({ id }: { id: string }) {
  const live = useLive((s) => s.incidents.find((i) => i.id === id))
  const meta = useLive((s) => s.meta)
  const [fetched, setFetched] = useState<Incident | null | undefined>(undefined)
  useEffect(() => {
    if (live) return
    api.incident(id).then(setFetched).catch(() => setFetched(null))
  }, [id, live])
  const incident = live ?? fetched
  const now = useNow()

  const back = (
    <a href={href({ name: "incidents" })} onClick={onLinkClick} className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
      <ArrowLeft aria-hidden className="size-4" /> All incidents
    </a>
  )
  if (incident === undefined) return <div className="flex flex-col gap-4">{back}<Skeleton className="h-96 w-full rounded-xl" /></div>
  if (incident === null)
    return (
      <div className="flex flex-col gap-4">
        {back}
        <p className="text-muted-foreground">Incident “{id}” wasn't found.</p>
      </div>
    )

  const blamed = incident.evidence.hops.find((h) => h.id === incident.blame)
  return (
    <div className="flex flex-col gap-5">
      {back}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <SeverityPill severity={incident.severity} />
            {!incident.ended && <span className="rounded-full bg-fail/15 px-2 py-0.5 text-xs font-semibold text-fail">Ongoing</span>}
          </div>
          <h1 className="mt-2 text-2xl font-semibold tracking-tight text-balance">{incident.headline}</h1>
          <p className="tabular-nums mt-1 text-sm text-muted-foreground">
            {absolute(incident.started)} → {incident.ended ? absolute(incident.ended) : "ongoing"} ({duration(incident.started, incident.ended, now)})
          </p>
        </div>
        <CopyReport incident={incident} meta={meta} />
      </div>
      <dl className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        {[
          ["Started", <RelTime key="s" at={incident.started} />],
          ["Ended", incident.ended ? <RelTime key="e" at={incident.ended} /> : <span key="e" className="text-fail">Ongoing</span>],
          ["Duration", duration(incident.started, incident.ended, now)],
          ["Blamed hop", blamed?.label ?? "—"],
        ].map(([k, v]) => (
          <div key={String(k)} className="rounded-lg border bg-card p-3">
            <dt className="text-xs text-muted-foreground">{k}</dt>
            <dd className="tabular-nums mt-0.5 font-medium">{v}</dd>
          </div>
        ))}
      </dl>
      <div>
        <h2 className="mb-2 text-sm font-medium text-muted-foreground">
          Evidence at the worst point · <RelTime at={incident.evidence.at} />
        </h2>
        <SnapshotView snapshot={incident.evidence} live={false} />
      </div>
    </div>
  )
}

/** A plain-text summary to paste into an ISP support chat or email. */
export function reportText(i: Incident, meta: Meta | null): string {
  const v = i.evidence.verdict
  const lines = [
    `Network incident report (wirewolf${meta ? ` ${meta.version} on ${meta.host}` : ""})`,
    "",
    `Problem:   ${i.headline}`,
    `Severity:  ${i.severity}`,
    `Started:   ${absolute(i.started)} (${i.started})`,
    `Ended:     ${i.ended ? `${absolute(i.ended)} (${i.ended})` : "ongoing"}`,
    `Duration:  ${duration(i.started, i.ended)}${i.ended ? "" : " so far"}`,
    `Likely at: ${i.evidence.hops.find((h) => h.id === i.blame)?.label ?? "unknown"}`,
    "",
    `Diagnosis: ${v.detail}`,
    `Method:    ${v.method} (${v.confidence} confidence)`,
    "",
    "Path at the worst point:",
  ]
  for (const h of i.evidence.hops) {
    const m = h.metrics
    const metrics = [m.rtt_ms != null && `rtt ${ms(m.rtt_ms)}`, m.loss_pct != null && `loss ${pct(m.loss_pct)}`].filter(Boolean).join(", ")
    lines.push(`  [${h.status.toUpperCase().padEnd(4)}] ${h.label.padEnd(10)} ${h.target ?? ""}`.trimEnd())
    lines.push(`         ${h.summary}${metrics ? ` (${metrics})` : ""}`)
    for (const c of h.checks.filter((c) => c.status === "fail" || c.status === "warn")) {
      lines.push(`         - ${c.name}: ${c.detail}`)
    }
  }
  return lines.join("\n")
}

function CopyReport({ incident, meta }: { incident: Incident; meta: Meta | null }) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    const text = reportText(incident, meta)
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      // Plain HTTP on a LAN address isn't a secure context, so the async clipboard API is
      // missing. Fall back to a hidden textarea.
      const ta = document.createElement("textarea")
      ta.value = text
      ta.style.position = "fixed"
      ta.style.opacity = "0"
      document.body.append(ta)
      ta.select()
      document.execCommand("copy")
      ta.remove()
    }
    setCopied(true)
    toast.success("Report copied", { description: "Paste it into your ISP's support chat or email." })
    setTimeout(() => setCopied(false), 2000)
  }
  return (
    <Button variant="outline" onClick={copy}>
      {copied ? <Check aria-hidden /> : <ClipboardCopy aria-hidden />}
      {copied ? "Copied" : "Copy report"}
    </Button>
  )
}
