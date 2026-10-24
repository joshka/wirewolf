// The Logs page: what wirewolf did (Activity), and its recent server log lines (Server logs).
//
// Layout: one toolbar row (tabs on the left, the tab's filters on the right, all h-8 controls),
// then a full-width card. Activity is a table like Incidents. Server logs are a dense mono list
// that scrolls inside the card and follows the tail until you scroll up.

import { useEffect, useMemo, useRef, useState } from "react"
import { toast } from "sonner"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { RelTime } from "@/components/time"
import { api, type Activity, type LogLine } from "@/lib/api"
import { useLive } from "@/lib/live"
import { href, navigate, onLinkClick } from "@/lib/router"
import { useSession, writeBlocked } from "@/lib/session"
import { cn } from "@/lib/utils"

type Tab = "activity" | "server"

export function LogsView({ tab }: { tab: Tab }) {
  return tab === "server" ? <ServerLogs /> : <ActivityLog />
}

function Tabs({ tab, children }: { tab: Tab; children?: React.ReactNode }) {
  return (
    <div className="mb-4 flex flex-wrap items-center gap-2">
      <ToggleGroup
        variant="outline"
        value={[tab]}
        onValueChange={(v) => v[0] && navigate(href(v[0] === "server" ? { name: "logs", tab: "server" } : { name: "logs" }))}
        aria-label="Log"
      >
        <ToggleGroupItem value="activity" className="px-3">
          Activity
        </ToggleGroupItem>
        <ToggleGroupItem value="server" className="px-3">
          Server logs
        </ToggleGroupItem>
      </ToggleGroup>
      <div className="ml-auto flex flex-wrap items-center gap-2">{children}</div>
    </div>
  )
}

const KINDS: Record<string, string> = {
  all: "Everything",
  verdict: "Verdict",
  incident: "Incidents",
  speed: "Speed tests",
  config: "Settings",
  auth: "Sign-ins",
  probe: "Probes",
}

function ActivityLog() {
  const all = useLive((s) => s.activity)
  const meta = useLive((s) => s.meta)
  const [kind, setKind] = useState("all")
  const entries = kind === "all" ? all : all.filter((a) => a.kind === kind)
  const showActor = all.some((a) => a.actor)
  return (
    <>
      <Tabs tab="activity">
        <Select value={kind} onValueChange={(v) => setKind(String(v))}>
          <SelectTrigger className="w-40" aria-label="Show">
            <SelectValue>{KINDS[kind]}</SelectValue>
          </SelectTrigger>
          <SelectContent>
            {Object.entries(KINDS).map(([v, label]) => (
              <SelectItem key={v} value={v}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Tabs>
      {!meta ? (
        <Skeleton className="h-64 w-full rounded-xl" />
      ) : entries.length === 0 ? (
        <p className="py-16 text-center text-muted-foreground">Nothing recorded yet.</p>
      ) : (
        <Card className="py-2">
          <CardContent className="px-2">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-28">When</TableHead>
                  <TableHead className="hidden w-28 sm:table-cell">Kind</TableHead>
                  <TableHead>What happened</TableHead>
                  {showActor && <TableHead className="hidden md:table-cell">Who</TableHead>}
                </TableRow>
              </TableHeader>
              <TableBody>
                {entries.map((a) => (
                  <TableRow key={a.id}>
                    <TableCell className="text-muted-foreground">
                      <RelTime at={a.at} />
                    </TableCell>
                    <TableCell className="hidden sm:table-cell">
                      <Badge variant="muted">{KINDS[a.kind] ?? a.kind}</Badge>
                    </TableCell>
                    <TableCell className="whitespace-normal">
                      <Subject a={a} />
                    </TableCell>
                    {showActor && <TableCell className="hidden text-muted-foreground md:table-cell">{a.actor ?? "—"}</TableCell>}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </>
  )
}

/** The message, linked to the incident it's about. */
function Subject({ a }: { a: Activity }) {
  if (a.kind === "incident" && a.subject) {
    return (
      <a href={href({ name: "incident", id: a.subject })} onClick={onLinkClick} className="hover:underline">
        {a.message}
      </a>
    )
  }
  return <>{a.message}</>
}

const LEVELS: Record<string, string> = { error: "Errors", warn: "Warnings", info: "Info", debug: "Debug", trace: "All levels" }
const RANK: Record<string, number> = { error: 0, warn: 1, info: 2, debug: 3, trace: 4 }
const LEVEL_TONE: Record<string, string> = { error: "text-fail", warn: "text-warn", info: "text-foreground", debug: "text-muted-foreground", trace: "text-muted-foreground" }

/** Lines kept in the browser; the server keeps its own ring. */
const KEEP = 5000

function ServerLogs() {
  const session = useSession()
  const blocked = writeBlocked(session, "see server logs")
  const [lines, setLines] = useState<LogLine[] | null>(null)
  const [filter, setFilter] = useState("")
  const [draft, setDraft] = useState("")
  const [denied, setDenied] = useState(false)
  const [level, setLevel] = useState("trace")
  const [query, setQuery] = useState("")

  useEffect(() => {
    if (blocked) return
    let source: EventSource | null = null
    let closed = false
    api
      .logs()
      .then((page) => {
        if (closed || !page) return setLines((l) => l ?? [])
        setLines(page.lines)
        setFilter(page.filter)
        setDraft(page.filter)
        source = new EventSource(api.logEventsUrl)
        source.addEventListener("log", (e) => {
          try {
            const line = JSON.parse((e as MessageEvent).data) as LogLine
            setLines((prev) => {
              const list = prev ?? []
              if (list.length && list[list.length - 1].seq >= line.seq) return list
              const next = [...list, line]
              return next.length > KEEP ? next.slice(next.length - KEEP) : next
            })
          } catch {
            // ignore malformed lines
          }
        })
      })
      .catch((e: { status?: number }) => (e.status === 403 ? setDenied(true) : setLines([])))
    return () => {
      closed = true
      source?.close()
    }
  }, [blocked])

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase()
    return (lines ?? []).filter((l) => (RANK[l.level] ?? 4) <= RANK[level] && (!q || l.message.toLowerCase().includes(q) || l.target.toLowerCase().includes(q)))
  }, [lines, level, query])

  const apply = async () => {
    const r = await api.setLogFilter(draft)
    if (!r.ok) return toast.error(`Log filter not changed: ${r.message}`)
    setFilter(r.filter)
    setDraft(r.filter)
    toast.success(r.saved ? `Log filter set to ${r.filter} and saved` : `Log filter set to ${r.filter} until restart`)
  }

  if (blocked || denied) {
    return (
      <>
        <Tabs tab="server" />
        <p className="py-16 text-center text-muted-foreground">
          {blocked ?? "You can't see server logs on this server."} Logs name hosts and addresses on your network, so only people who can change settings see them.
        </p>
      </>
    )
  }
  return (
    <>
      <Tabs tab="server">
        <Select value={level} onValueChange={(v) => setLevel(String(v))}>
          <SelectTrigger className="w-32" aria-label="Least severe level shown">
            <SelectValue>{LEVELS[level]}</SelectValue>
          </SelectTrigger>
          <SelectContent>
            {Object.entries(LEVELS).map(([v, label]) => (
              <SelectItem key={v} value={v}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search text or module" aria-label="Search" className="h-8 w-56" />
      </Tabs>
      <Card className="gap-0 py-0">
        <form
          className="flex flex-wrap items-center gap-2 border-b px-4 py-3"
          onSubmit={(e) => {
            e.preventDefault()
            void apply()
          }}
        >
          <label htmlFor="log-filter" className="text-sm font-medium">
            Filter
          </label>
          <Input id="log-filter" value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="info,wirewolf_probe=debug" className="h-8 min-w-0 flex-1 font-mono text-xs sm:max-w-md" />
          <Button type="submit" size="sm" variant="outline" disabled={!draft.trim() || draft.trim() === filter}>
            Apply
          </Button>
          <span className="w-full text-xs text-muted-foreground sm:ml-auto sm:w-auto">
            Decides which lines the server keeps, here and in its output. Saved as <code className="font-mono">log.filter</code>.
          </span>
        </form>
        <CardContent className="px-0">
          {lines === null ? <Skeleton className="m-4 h-64 rounded-lg" /> : <LogList lines={shown} total={lines.length} />}
        </CardContent>
      </Card>
    </>
  )
}

function LogList({ lines, total }: { lines: LogLine[]; total: number }) {
  const box = useRef<HTMLDivElement>(null)
  const follow = useRef(true)
  useEffect(() => {
    const el = box.current
    if (el && follow.current) el.scrollTop = el.scrollHeight
  }, [lines])
  if (lines.length === 0) {
    return <p className="py-16 text-center text-sm text-muted-foreground">{total ? "No lines match." : "No log lines yet."}</p>
  }
  return (
    <div
      ref={box}
      onScroll={(e) => {
        const el = e.currentTarget
        follow.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24
      }}
      className="h-[65vh] overflow-auto py-2 font-mono text-xs leading-5"
      role="log"
      aria-live="off"
    >
      {lines.map((l) => (
        <div key={l.seq} className="flex gap-3 px-4 hover:bg-muted/50">
          <time dateTime={l.at} className="shrink-0 text-muted-foreground tabular-nums" title={l.at}>
            {new Date(l.at).toLocaleTimeString([], { hour12: false })}
          </time>
          <span className={cn("w-10 shrink-0 uppercase", LEVEL_TONE[l.level])}>{l.level}</span>
          <span className="hidden w-48 shrink-0 truncate text-muted-foreground lg:inline" title={l.target}>
            {l.target}
          </span>
          <span className="min-w-0 break-words whitespace-pre-wrap">{l.message}</span>
        </div>
      ))}
    </div>
  )
}
