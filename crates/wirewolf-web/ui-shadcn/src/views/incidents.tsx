import { useEffect, useState } from "react"
import { ArrowLeft, Check, ClipboardCopy } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { AskAi } from "@/components/ask-ai"
import { SnapshotView } from "@/components/snapshot"
import { SeverityPill } from "@/components/status"
import { RelTime, useNow } from "@/components/time"
import { api, type Incident, type Meta } from "@/lib/api"
import { useLive } from "@/lib/live"
import { absolute, duration } from "@/lib/format"
import { copyText } from "@/lib/clipboard"
import { reportText } from "@/lib/report"
import { href, onLinkClick } from "@/lib/router"
import { cn } from "@/lib/utils"
import { CsvLink, PageHeader } from "@/components/page-header"

export function IncidentsView() {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader title="Incidents" description="Every period the network was degraded or down, open ones first. Each keeps the evidence from its worst moment.">
        <CsvLink what="incidents" />
      </PageHeader>
      <IncidentList />
    </div>
  )
}

function IncidentList() {
  const all = useLive((s) => s.incidents)
  const meta = useLive((s) => s.meta)
  const now = useNow()
  // Open incidents first, then newest first.
  const incidents = [...all].sort((a, b) => Number(!!a.ended) - Number(!!b.ended) || b.started.localeCompare(a.started))
  if (!meta) return <Skeleton className="h-64 w-full rounded-xl" />
  if (incidents.length === 0) return <p className="py-16 text-center text-muted-foreground">No incidents recorded.</p>
  return (
    <Card className="py-2">
      <CardContent className="px-2">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Incident</TableHead>
              <TableHead className="hidden md:table-cell">Hop</TableHead>
              <TableHead className="hidden sm:table-cell">Started</TableHead>
              <TableHead className="text-right">Duration</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {incidents.map((i) => (
              <TableRow key={i.id} className={cn(!i.ended && "bg-fail/5")}>
                <TableCell className="whitespace-normal">
                  <a
                    href={href({ name: "incident", id: i.id })}
                    onClick={onLinkClick}
                    className="flex flex-col items-start gap-1 rounded-sm outline-none hover:underline focus-visible:ring-3 focus-visible:ring-ring sm:flex-row sm:items-center sm:gap-3"
                  >
                    <SeverityPill severity={i.severity} />
                    <span className="font-medium">{i.headline}</span>
                  </a>
                </TableCell>
                <TableCell className="hidden text-muted-foreground md:table-cell">
                  {i.evidence.hops.find((h) => h.id === i.blame)?.label ?? "—"}
                </TableCell>
                <TableCell className="hidden sm:table-cell">
                  <RelTime at={i.started} />
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {i.ended ? duration(i.started, i.ended) : <span className="text-fail">ongoing · {duration(i.started, null, now)}</span>}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  )
}

/** An incident from the live list, else fetched. `undefined` while loading, `null` if missing. */
export function useIncident(id: string | undefined): Incident | null | undefined {
  const live = useLive((s) => s.incidents.find((i) => i.id === id))
  const [fetched, setFetched] = useState<Incident | null | undefined>(undefined)
  useEffect(() => {
    if (live || !id) return
    api.incident(id).then(setFetched).catch(() => setFetched(null))
  }, [id, live])
  return live ?? fetched
}

export function IncidentView({ id }: { id: string }) {
  const meta = useLive((s) => s.meta)
  const incident = useIncident(id)
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
          <SeverityPill severity={incident.severity} />
          <h1 className="mt-2 text-2xl font-semibold tracking-tight text-balance">{incident.headline}</h1>
          <p className="tabular-nums mt-1 text-sm text-muted-foreground">
            {absolute(incident.started)} → {incident.ended ? absolute(incident.ended) : "ongoing"} ({duration(incident.started, incident.ended, now)})
            {blamed && ` · ${blamed.label}`}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <CopyReport incident={incident} meta={meta} />
          <AskAi incident={incident.id} />
        </div>
      </div>
      <div>
        <h2 className="mb-2 text-sm font-medium text-muted-foreground">
          Evidence at the worst point · <RelTime at={incident.evidence.at} />
        </h2>
        <SnapshotView snapshot={incident.evidence} incident={incident.id} />
      </div>
    </div>
  )
}

function CopyReport({ incident, meta }: { incident: Incident; meta: Meta | null }) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    await copyText(reportText(incident, meta))
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
