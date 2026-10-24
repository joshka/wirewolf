// The Logs page: what wirewolf did (Activity), and its recent server log lines (Server logs).
//
// Layout: one toolbar row (tabs on the left, the tab's filters on the right, all h-8 controls),
// then a full-width card. Activity is a table like Incidents. Server logs are a dense mono list
// that scrolls inside the card and follows the tail until you scroll up. A bar under it says
// whether the stream is live, and offers a jump back to the newest line when scrolled up.

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
// The colors tracing-subscriber uses on a terminal: red, yellow, green, blue, purple.
const LEVEL_TONE: Record<string, string> = {
  error: "text-fail",
  warn: "text-warn",
  info: "text-pass",
  debug: "text-blue-600 dark:text-blue-400",
  trace: "text-purple-600 dark:text-purple-400",
}

/** Local `HH:MM:SS.mmm`. */
function clock(at: string): string {
  const d = new Date(at)
  return `${d.toLocaleTimeString([], { hour12: false })}.${String(d.getMilliseconds()).padStart(3, "0")}`
}

/**
 * Starting points for the log filter. Plain `debug` and `trace` are mostly DNS, TLS, and HTTP
 * library internals (thousands of lines a minute), so these turn up wirewolf first.
 */
export const PRESETS: { filter: string; label: string; hint: string }[] = [
  { filter: "info", label: "Normal", hint: "What wirewolf does: startup, verdict changes, speed tests, problems" },
  { filter: "warn", label: "Quiet", hint: "Only warnings and errors" },
  { filter: "info,wirewolf=debug", label: "wirewolf detail", hint: "Adds a summary of every probe cycle" },
  { filter: "info,wirewolf=debug,hickory_resolver=debug", label: "DNS detail", hint: "Adds each DNS query and which server answered" },
  { filter: "info,wirewolf=debug,reqwest=debug,rustls=debug", label: "HTTP and TLS detail", hint: "Adds web checks, connections, and handshakes" },
  { filter: "trace", label: "Everything", hint: "Every library's internals; thousands of lines a minute" },
]

/** Field names in italics with a dim `=`, as tracing-subscriber prints `key=value`. */
function Message({ text }: { text: string }) {
  const parts = text.split(/(?<=^|\s)([A-Za-z_][\w.]*)=/)
  return (
    <>
      {parts.map((p, i) =>
        i % 2 === 1 ? (
          <span key={i}>
            <i className="text-muted-foreground">{p}</i>
            <span className="text-muted-foreground/60">=</span>
          </span>
        ) : (
          p
        ),
      )}
    </>
  )
}

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

  const [stream, setStream] = useState<Stream>("connecting")

  useEffect(() => {
    if (blocked) return
    let source: EventSource | null = null
    let closed = false
    const merge = (incoming: LogLine[]) =>
      setLines((prev) => {
        const list = prev ?? []
        const last = list.length ? list[list.length - 1].seq : 0
        const fresh = incoming.filter((l) => l.seq > last)
        if (!fresh.length) return list
        const next = [...list, ...fresh]
        return next.length > KEEP ? next.slice(next.length - KEEP) : next
      })
    api
      .logs()
      .then((page) => {
        if (closed || !page) return setLines((l) => l ?? [])
        merge(page.lines)
        setFilter(page.filter)
        setDraft(page.filter)
        let opened = false
        source = new EventSource(api.logEventsUrl)
        source.onopen = () => {
          setStream("live")
          // After a reconnect, fetch what arrived while the stream was down.
          if (opened) api.logs().then((p) => p && merge(p.lines)).catch(() => {})
          opened = true
        }
        // EventSource reconnects on its own; this only reports it.
        source.onerror = () => setStream(source?.readyState === EventSource.CLOSED ? "closed" : "reconnecting")
        source.addEventListener("log", (e) => {
          try {
            merge([JSON.parse((e as MessageEvent).data) as LogLine])
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

  const apply = async (to = draft) => {
    const r = await api.setLogFilter(to)
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
          <Select value={PRESETS.find((p) => p.filter === filter)?.filter ?? ""} onValueChange={(v) => v && void apply(String(v))}>
            <SelectTrigger className="h-8 w-48" aria-label="Preset">
              <SelectValue>{PRESETS.find((p) => p.filter === filter)?.label ?? "Custom"}</SelectValue>
            </SelectTrigger>
            <SelectContent className="w-80">
              {PRESETS.map((p) => (
                <SelectItem key={p.filter} value={p.filter}>
                  <div className="flex flex-col gap-0.5">
                    <span>{p.label}</span>
                    <span className="text-xs text-muted-foreground">{p.hint}</span>
                  </div>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Input id="log-filter" value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="info,wirewolf_probe=debug" className="h-8 min-w-0 flex-1 font-mono text-xs sm:max-w-md" />
          <Button type="submit" size="sm" variant="outline" disabled={!draft.trim() || draft.trim() === filter}>
            Apply
          </Button>
          <span className="w-full text-xs text-muted-foreground sm:ml-auto sm:w-auto">
            Decides which lines the server keeps, here and in its output. Saved as <code className="font-mono">log.filter</code>.
          </span>
        </form>
        <CardContent className="px-0">
          {lines === null ? <Skeleton className="m-4 h-64 rounded-lg" /> : <LogList lines={shown} total={lines.length} stream={stream} />}
        </CardContent>
      </Card>
    </>
  )
}

type Stream = "connecting" | "live" | "reconnecting" | "closed"

/** Whether new lines are arriving, and whether the list is following them. */
function StreamBar({ stream, following, behind, onJump }: { stream: Stream; following: boolean; behind: number; onJump: () => void }) {
  const paused = stream === "live" && !following
  const label = { connecting: "Connecting…", live: paused ? "Live · paused" : "Live", reconnecting: "Reconnecting…", closed: "Disconnected · reload the page" }[stream]
  return (
    <div className="flex h-10 items-center gap-3 border-t px-4 text-xs text-muted-foreground">
      <span role="status" className="inline-flex items-center gap-1.5 font-medium">
        <span
          className={cn(
            "size-1.5 rounded-full",
            stream === "live" ? (paused ? "bg-muted-foreground" : "animate-pulse bg-pass") : stream === "closed" ? "bg-fail" : "bg-warn",
          )}
        />
        <span className={cn(stream === "live" && !paused && "text-foreground", stream === "reconnecting" && "text-warn", stream === "closed" && "text-fail")}>{label}</span>
      </span>
      <span className="hidden sm:inline">{paused ? "Scrolled up, so the list holds still. New lines keep arriving below." : "New lines appear at the bottom as the server logs them."}</span>
      {paused && (
        <Button size="sm" variant="outline" className="ml-auto h-7" onClick={onJump}>
          Jump to latest{behind > 0 ? ` (${behind} new)` : ""}
        </Button>
      )}
    </div>
  )
}

function LogList({ lines, total, stream }: { lines: LogLine[]; total: number; stream: Stream }) {
  const box = useRef<HTMLDivElement>(null)
  const [following, setFollowing] = useState(true)
  // The newest line seen while following, to count what arrived since.
  const [seenSeq, setSeenSeq] = useState(0)
  const newest = lines.length ? lines[lines.length - 1].seq : 0
  useEffect(() => {
    const el = box.current
    if (el && following) el.scrollTop = el.scrollHeight
    if (following) setSeenSeq(newest)
  }, [lines, following, newest])
  const behind = following ? 0 : lines.filter((l) => l.seq > seenSeq).length
  const jump = () => {
    setFollowing(true)
    const el = box.current
    if (el) el.scrollTop = el.scrollHeight
  }
  const bar = <StreamBar stream={stream} following={following} behind={behind} onJump={jump} />
  if (lines.length === 0) {
    return (
      <>
        <p className="py-16 text-center text-sm text-muted-foreground">{total ? "No lines match." : "No log lines yet."}</p>
        {bar}
      </>
    )
  }
  return (
    <>
      <div
        ref={box}
        onScroll={(e) => {
          const el = e.currentTarget
          setFollowing(el.scrollHeight - el.scrollTop - el.clientHeight < 24)
        }}
        className="h-[65vh] overflow-auto py-2 font-mono text-xs leading-5"
        role="log"
        aria-live="off"
      >
        {lines.map((l) => (
          // Laid out like tracing's console output: `time LEVEL target: message k=v`, with
          // wrapped lines hanging under the message.
          <div key={l.seq} className="grid grid-cols-[auto_auto_1fr] gap-x-2 px-4 hover:bg-muted/50">
            <time dateTime={l.at} className="text-muted-foreground tabular-nums" title={l.at}>
              {clock(l.at)}
            </time>
            <span className={cn("w-[5ch] text-right font-semibold uppercase", LEVEL_TONE[l.level])}>{l.level}</span>
            <span className={cn("min-w-0 break-words whitespace-pre-wrap", l.level === "error" && "text-fail", l.level === "warn" && "text-warn")}>
              <span className="text-muted-foreground/70">{l.target}:</span> <Message text={l.message} />
            </span>
          </div>
        ))}
      </div>
      {bar}
    </>
  )
}
