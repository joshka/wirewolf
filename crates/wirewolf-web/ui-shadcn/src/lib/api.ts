// Wire types for the wirewolf API v1 (docs/api.md, crates/wirewolf-core/src/model.rs).

import { currentSession } from "@/lib/session"

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

export interface Limits {
  rtt_warn_ms: number
  jitter_warn_ms: number
  loss_warn_pct: number
}

export interface UserTargetConfig {
  name: string
  check: string
  critical: boolean
  verify_tls: boolean
}

/** `wirewolf.toml` as JSON. Durations are strings such as `"30s"`. */
export interface Config {
  server: { listen: string }
  probe: { interval: string; ipv6: "auto" | "on" | "off"; path_mtu: boolean }
  internet: { port: number; icmp: boolean; targets: { name: string; v4: string | null; v6: string | null }[] }
  dns: {
    system: boolean
    resolvers: { name: string; address: string }[]
    queries: { name: string; expect: string[] }[]
    record_types: ("A" | "AAAA")[]
  }
  web: { captive_portal: string; tls_host: string; sites: string[] }
  thresholds: Record<"gateway" | "isp" | "internet" | "dns" | "targets", Limits>
  incidents: { open_after: number; close_after: number }
  speed: { backend: string; interval: string; server: string; streams: number; duration: string }
  storage: { db: string; retention: string }
  targets: UserTargetConfig[]
  [section: string]: unknown
}

/** `GET /config`: the effective `wirewolf.toml`, as JSON. */
export interface ConfigView {
  path: string | null
  config: Config
  sources: Record<string, ConfigSource>
  /** `PUT /config` accepts edits (1.2). */
  editable?: boolean
  /** Where edits are written (1.2). */
  save_path?: string | null
  /** Settings that only change on restart (1.2). */
  restart_keys?: string[]
  /** The built-in defaults, same shape as `config` (1.4). */
  defaults?: Config
}

/** One rejected setting from `PUT /config`. `key` is a dotted path; `""` is the whole config. */
export interface FieldError {
  key: string
  message: string
}

export type SaveResult = { ok: true; view: ConfigView } | { ok: false; status: number; errors: FieldError[] }

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

/** `GET /session`: who is signed in and what they may do. */
export interface Session {
  /** `oidc`, `header`; empty when the server has no sign-in configured. */
  methods: string[]
  user: { id: string; name: string | null; email: string | null; via: string } | null
  can_write: boolean
  reads_require_login: boolean
  /** Sent as `X-Wirewolf-CSRF` on writes. */
  csrf: string | null
  login_url: string | null
  logout_url: string | null
}

const BASE = "/api/v1"

async function get<T>(path: string): Promise<T | null> {
  const res = await fetch(BASE + path)
  if (res.status === 204 || res.status === 404) return null
  if (!res.ok) throw new Error(`GET ${path}: ${res.status}`)
  return res.json() as Promise<T>
}

async function post(path: string, body?: unknown): Promise<number> {
  const headers: Record<string, string> = {}
  if (body !== undefined) headers["content-type"] = "application/json"
  const csrf = currentSession()?.csrf
  if (csrf) headers["x-wirewolf-csrf"] = csrf
  const res = await fetch(BASE + path, {
    method: "POST",
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  return res.status
}

async function putConfig(config: Config, csrf?: string | null): Promise<SaveResult> {
  const headers: Record<string, string> = { "content-type": "application/json" }
  if (csrf) headers["x-wirewolf-csrf"] = csrf
  const res = await fetch(BASE + "/config", { method: "PUT", headers, body: JSON.stringify(config) })
  if (res.ok) return { ok: true, view: (await res.json()) as ConfigView }
  const body = (await res.json().catch(() => null)) as { errors?: FieldError[] } | null
  const fallback = res.status === 403 ? "You can't change settings on this server." : `Saving failed (${res.status}).`
  return { ok: false, status: res.status, errors: body?.errors ?? [{ key: "", message: fallback }] }
}

export const api = {
  meta: () => get<Meta>("/meta"),
  now: () => get<Snapshot>("/now"),
  config: () => get<ConfigView>("/config"),
  saveConfig: putConfig,
  session: () => get<Session>("/session"),
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
