import { useEffect, useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api, type ConfigSource, type ConfigView } from "@/lib/api"

/** What each section of `wirewolf.toml` controls. */
const SECTIONS: Record<string, string> = {
  server: "Where the dashboard listens",
  probe: "How often the path is probed, and which extras run",
  internet: "TCP handshakes (and ICMP pings) to anycast targets",
  dns: "Resolvers compared and the names asked of each",
  web: "Captive portal, TLS, and HTTPS reachability",
  thresholds: "Above these limits a hop is degraded",
  incidents: "How many cycles in a row open or close an incident",
  speed: "Scheduled throughput tests",
  storage: "History on disk",
  targets: "Your own things to watch",
}

type Json = null | boolean | number | string | Json[] | { [k: string]: Json }

/** Read-only: the effective config, and where each value came from. */
export function SettingsView() {
  const [view, setView] = useState<ConfigView | null | undefined>(undefined)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    api.config().then(setView, (e: Error) => setError(e.message))
  }, [])
  if (error) return <p className="text-sm text-fail">Couldn't load the config: {error}</p>
  if (view === undefined) return <Skeleton className="h-96 w-full rounded-xl" />
  if (view === null) return <p className="text-muted-foreground">This server doesn't report its configuration.</p>
  const set = Object.keys(view.sources).length
  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <p className="text-sm text-muted-foreground">
          {view.path ? (
            <>
              From <code className="font-mono">{view.path}</code>, {set} {set === 1 ? "setting" : "settings"} changed from the defaults.
            </>
          ) : (
            "No config file: everything is a default."
          )}{" "}
          Edit the file and restart to change them. <code className="font-mono">wirewolf config default</code> prints a commented starting point.
        </p>
      </div>
      <div className="grid gap-4 xl:grid-cols-2">
        {Object.entries(view.config as Record<string, Json>).map(([section, value]) => (
          <Card key={section} className={section === "targets" ? "xl:col-span-2" : undefined}>
            <CardHeader>
              <CardTitle className="font-mono">{section === "targets" ? "[[targets]]" : `[${section}]`}</CardTitle>
              <CardDescription>{SECTIONS[section]}</CardDescription>
            </CardHeader>
            <CardContent>
              <Section path={section} value={value} sources={view.sources} />
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  )
}

function Section({ path, value, sources }: { path: string; value: Json; sources: Record<string, ConfigSource> }) {
  if (Array.isArray(value)) return <ListTable path={path} rows={value} sources={sources} />
  const rows = flatten(value, path)
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Setting</TableHead>
          <TableHead>Value</TableHead>
          <TableHead className="text-right">From</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map(([key, v]) =>
          Array.isArray(v) && v.some((x) => typeof x === "object" && x !== null) ? (
            <TableRow key={key}>
              <TableCell colSpan={3} className="px-0">
                <div className="mb-1 flex items-center justify-between px-2">
                  <span className="font-mono text-xs">{key.slice(path.length + 1)}</span>
                  <SourceBadge source={sources[key]} />
                </div>
                <ListTable path={key} rows={v} sources={sources} bare />
              </TableCell>
            </TableRow>
          ) : (
            <TableRow key={key}>
              <TableCell className="font-mono text-xs">{key.slice(path.length + 1)}</TableCell>
              <TableCell className="font-mono text-xs whitespace-normal break-all">{show(v)}</TableCell>
              <TableCell className="text-right">
                <SourceBadge source={sourceOf(key, sources)} />
              </TableCell>
            </TableRow>
          ),
        )}
      </TableBody>
    </Table>
  )
}

/** An array of tables, such as `[[targets]]`, as one table. */
function ListTable({ path, rows, sources, bare }: { path: string; rows: Json[]; sources: Record<string, ConfigSource>; bare?: boolean }) {
  if (rows.length === 0)
    return (
      <p className="text-sm text-muted-foreground">
        None.{path === "targets" && " Add entries like { name = \"NAS\", check = \"tcp://nas.local:445\" } to wirewolf.toml."}
      </p>
    )
  const objects = rows.filter((r): r is { [k: string]: Json } => typeof r === "object" && r !== null && !Array.isArray(r))
  const cols = [...new Set(objects.flatMap((r) => Object.keys(r)))]
  return (
    <div className="flex flex-col gap-2">
      {!bare && (
        <div className="flex justify-end">
          <SourceBadge source={sources[path]} />
        </div>
      )}
      <Table>
        <TableHeader>
          <TableRow>
            {cols.map((c) => (
              <TableHead key={c}>{c}</TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {objects.map((r, i) => (
            <TableRow key={i}>
              {cols.map((c) => (
                <TableCell key={c} className="font-mono text-xs whitespace-normal break-all">
                  {show(r[c] ?? null)}
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}

function flatten(value: Json, path: string): [string, Json][] {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return [[path, value]]
  return Object.entries(value).flatMap(([k, v]) => flatten(v, `${path}.${k}`))
}

/** A setting's source, or its nearest set parent (a whole table set in the file). */
function sourceOf(key: string, sources: Record<string, ConfigSource>): ConfigSource {
  for (let k = key; k; k = k.includes(".") ? k.slice(0, k.lastIndexOf(".")) : "") if (sources[k]) return sources[k]
  return "default"
}

function show(v: Json): string {
  if (v === null) return "—"
  if (typeof v === "string") return v === "" ? '""' : v
  if (Array.isArray(v)) return v.length ? v.map(show).join(", ") : "[]"
  if (typeof v === "object") return JSON.stringify(v)
  return String(v)
}

function SourceBadge({ source = "default" }: { source?: ConfigSource }) {
  const label = { default: "default", file: "file", env: "env", flag: "flag" }[source]
  return <Badge variant={source === "default" ? "outline" : "secondary"}>{label}</Badge>
}
