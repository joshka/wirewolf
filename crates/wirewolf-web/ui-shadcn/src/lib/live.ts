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
  /** When the connection was lost, for the "disconnected since" message. */
  lostAt: number | null
}

let state: LiveState = {
  meta: null,
  snapshot: null,
  samples: {},
  speedRuns: [],
  incidents: [],
  connection: "connecting",
  lostAt: null,
}

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
  set({ meta, samples, speedRuns, incidents })
}

let source: EventSource | null = null
let retry = 0

function connect() {
  source?.close()
  source = new EventSource(api.eventsUrl)
  source.onopen = () => {
    retry = 0
    set({ connection: "live", lostAt: null })
    refresh().catch(() => {})
  }
  source.onerror = () => {
    // EventSource retries on its own, but gives up for good on some errors; own the retry loop.
    source?.close()
    source = null
    set({ connection: "reconnecting", lostAt: state.lostAt ?? Date.now() })
    const delay = Math.min(15000, 1000 * 2 ** retry++)
    setTimeout(connect, delay)
  }
  source.addEventListener("snapshot", (e) => set({ snapshot: JSON.parse(e.data) }))
  source.addEventListener("sample", (e) => addSample(JSON.parse(e.data)))
  source.addEventListener("speed", (e) => set({ speedRuns: upsertById(state.speedRuns, JSON.parse(e.data)) }))
  source.addEventListener("incident", (e) => set({ incidents: upsertById(state.incidents, JSON.parse(e.data)) }))
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

