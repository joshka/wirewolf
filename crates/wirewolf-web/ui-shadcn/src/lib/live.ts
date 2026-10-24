// One SSE connection feeding a small external store. Components subscribe with `useLive`.

import { useSyncExternalStore } from "react"
import { api, type Incident, type Meta, type Sample, type Snapshot, type SpeedRun } from "./api"

export type Connection = "connecting" | "live" | "reconnecting"

/** How much recent history the Now view keeps for sparklines. */
export const SPARK_WINDOW_MS = 15 * 60 * 1000

export interface LiveState {
  meta: Meta | null
  snapshot: Snapshot | null
  /** Recent samples per hop, oldest first, trimmed to SPARK_WINDOW_MS. */
  samples: Record<string, Sample[]>
  speedRuns: SpeedRun[]
  incidents: Incident[]
  connection: Connection
  /** The last failure to load data outside the SSE stream. */
  error: string | null
}

const empty: LiveState = {
  meta: null,
  snapshot: null,
  samples: {},
  speedRuns: [],
  incidents: [],
  connection: "connecting",
  error: null,
}

// A reload (Cmd+R) would otherwise paint skeletons until the API answers, which reads as a flash.
// Keep the last state in this tab's sessionStorage and render it immediately; live data replaces
// it within a moment. Stale copies (older than a few minutes) are ignored.
const CACHE_KEY = "wirewolf.live"
const CACHE_MAX_AGE_MS = 5 * 60 * 1000

function restore(): LiveState {
  try {
    const raw = sessionStorage.getItem(CACHE_KEY)
    if (!raw) return empty
    const { savedAt, ...cached } = JSON.parse(raw) as Partial<LiveState> & { savedAt: number }
    if (Date.now() - savedAt > CACHE_MAX_AGE_MS) return empty
    return { ...empty, ...cached }
  } catch {
    return empty
  }
}

function save() {
  try {
    const { meta, snapshot, samples, speedRuns, incidents } = state
    sessionStorage.setItem(CACHE_KEY, JSON.stringify({ savedAt: Date.now(), meta, snapshot, samples, speedRuns, incidents: incidents.slice(0, 50) }))
  } catch {
    // Quota or privacy mode: the cache is only an optimisation.
  }
}

let state: LiveState = restore()
if (typeof window !== "undefined") window.addEventListener("pagehide", save)

const listeners = new Set<() => void>()

function set(patch: Partial<LiveState>) {
  state = { ...state, ...patch }
  listeners.forEach((l) => l())
}

function trim(list: Sample[]): Sample[] {
  const cutoff = Date.now() - SPARK_WINDOW_MS
  let i = 0
  while (i < list.length && new Date(list[i].at).getTime() < cutoff) i++
  return i ? list.slice(i) : list
}

function addSample(s: Sample) {
  const prev = state.samples[s.hop] ?? []
  set({ samples: { ...state.samples, [s.hop]: trim([...prev, s]) } })
}

function upsertById<T extends { id: string }>(list: T[], item: T): T[] {
  const i = list.findIndex((x) => x.id === item.id)
  if (i === -1) return [item, ...list]
  const next = list.slice()
  next[i] = item
  return next
}

/** Reload everything that SSE doesn't replay: meta, recent history, speed runs, incidents. */
async function refresh() {
  const since = new Date(Date.now() - SPARK_WINDOW_MS)
  const [meta, history, speedRuns, incidents] = await Promise.all([
    api.meta(),
    api.history({ since }),
    api.speed(),
    api.incidents(),
  ])
  const samples: Record<string, Sample[]> = {}
  for (const s of history) (samples[s.hop] ??= []).push(s)
  set({ meta, samples, speedRuns, incidents, error: null })
}

let source: EventSource | null = null
let retry = 0

function connect() {
  source?.close()
  source = new EventSource(api.eventsUrl)
  source.onopen = () => {
    retry = 0
    set({ connection: "live" })
    refresh().catch((e: Error) => set({ error: `Couldn't load data: ${e.message}` }))
  }
  source.onerror = () => {
    // EventSource retries on its own, but gives up for good on some errors; own the retry loop.
    source?.close()
    source = null
    set({ connection: "reconnecting" })
    const delay = Math.min(15000, 1000 * 2 ** retry++)
    setTimeout(connect, delay)
  }
  on("snapshot", (x): x is Snapshot => Array.isArray(x.hops) && typeof x.verdict?.severity === "string", (x) => set({ snapshot: x }))
  on("sample", (x): x is Sample => typeof x.hop === "string" && typeof x.at === "string", addSample)
  on("speed", (x): x is SpeedRun => typeof x.id === "string" && typeof x.state === "string", (x) =>
    set({ speedRuns: upsertById(state.speedRuns, x) }),
  )
  on("incident", (x): x is Incident => typeof x.id === "string" && Array.isArray(x.evidence?.hops), (x) =>
    set({ incidents: upsertById(state.incidents, x) }),
  )
}

/** Subscribes to one SSE event type, dropping payloads that don't have the expected shape. */
function on<T>(name: string, valid: (x: any) => x is T, handle: (x: T) => void) {
  source?.addEventListener(name, (e) => {
    try {
      const x = JSON.parse((e as MessageEvent).data)
      if (x && typeof x === "object" && valid(x)) return handle(x)
    } catch {
      // fall through
    }
    console.warn(`wirewolf: ignored malformed "${name}" event`)
  })
}

let started = false
export function start() {
  if (started) return
  started = true
  connect()
}

export function useLive<T>(select: (s: LiveState) => T): T {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => select(state),
  )
}

/** Re-read meta now (after switching the demo scenario) instead of waiting for a reconnect. */
export async function reloadMeta() {
  set({ meta: await api.meta() })
}

