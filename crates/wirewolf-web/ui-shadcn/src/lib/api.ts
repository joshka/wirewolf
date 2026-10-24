// Wire types for the wirewolf API v1 (docs/api.md, crates/wirewolf-core/src/model.rs).

export type Status = "pass" | "warn" | "fail" | "skip" | "not_applicable" | "pending"
export type Severity = "healthy" | "degraded" | "down" | "unknown"
export type Confidence = "low" | "medium" | "high"
/** `target` is UI-only: a user target shown as a hop (see `lib/targets.ts`). */
export type HopKind = "host" | "gateway" | "isp" | "internet" | "dns" | "web" | "target"
export type BloatGrade = "A+" | "A" | "B" | "C" | "D" | "F"

export interface Metrics {
  rtt_ms: number | null
  jitter_ms: number | null
  loss_pct: number | null
}

export interface Hop {
  id: string
  kind: HopKind
  label: string
  target: string | null
  status: Status
  summary: string
  metrics: Metrics
  evidence: { key: string; value: string }[]
  checks: { name: string; status: Status; detail: string; rtt_ms: number | null }[]
}

export interface Verdict {
  severity: Severity
  headline: string
  detail: string
  blame: string | null
  confidence: Confidence
  fix_hint: string | null
  method: string
}

export interface Snapshot {
  at: string
  verdict: Verdict
  hops: Hop[]
  open_incident: string | null
  /** User targets from `[[targets]]` (API 1.1; missing from older servers). */
  targets?: Target[]
}

export interface Target {
  /** `target:<slug>`; also the history series id. */
  id: string
  name: string
  check: string
  kind: "tcp" | "http" | "https" | "ping" | "dns" | string
  critical: boolean
  status: Status
  detail: string
  rtt_ms: number | null
  loss_pct: number | null
}

export type ConfigSource = "default" | "file" | "env" | "flag"

/** `GET /config`: the effective `wirewolf.toml`, as JSON. */
export interface ConfigView {
  path: string | null
  config: Record<string, unknown>
  sources: Record<string, ConfigSource>
}

export interface Sample extends Metrics {
  at: string
  hop: string
  status: Status
}

export interface SpeedRun {
  id: string
  started: string
  state: "running" | "complete" | "failed"
  backend: string
  server: string | null
  down_mbps: number | null
  up_mbps: number | null
  idle_latency_ms: number | null
  loaded_latency_ms: number | null
  bloat: BloatGrade | null
  bytes_used: number | null
  progress: number | null
  error: string | null
}

export interface Incident {
  id: string
  started: string
  ended: string | null
  severity: Severity
  headline: string
  blame: string | null
  evidence: Snapshot
}

export interface Meta {
  version: string
  api_version: string
  mode: "demo" | "live" | string
  host: string
  scenarios: { id: string; name: string; description: string }[]
  scenario: string | null
}

const BASE = "/api/v1"

async function get<T>(path: string): Promise<T | null> {
  const res = await fetch(BASE + path)
  if (res.status === 204 || res.status === 404) return null
  if (!res.ok) throw new Error(`GET ${path}: ${res.status}`)
  return res.json() as Promise<T>
}

async function post(path: string, body?: unknown): Promise<number> {
  const res = await fetch(BASE + path, {
    method: "POST",
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  return res.status
}

export const api = {
  meta: () => get<Meta>("/meta"),
  now: () => get<Snapshot>("/now"),
  config: () => get<ConfigView>("/config"),
  history: (q: { hop?: string; since?: Date; until?: Date } = {}) => {
    const p = new URLSearchParams()
    if (q.hop) p.set("hop", q.hop)
    if (q.since) p.set("since", q.since.toISOString())
    if (q.until) p.set("until", q.until.toISOString())
    return get<Sample[]>(`/history?${p}`).then((x) => x ?? [])
  },
  speed: () => get<SpeedRun[]>("/speed").then((x) => x ?? []),
  runSpeed: () => post("/speed/run"),
  incidents: () => get<Incident[]>("/incidents").then((x) => x ?? []),
  incident: (id: string) => get<Incident>(`/incidents/${encodeURIComponent(id)}`),
  setScenario: (id: string) => post("/demo/scenario", { id }),
  eventsUrl: `${BASE}/events`,
}
