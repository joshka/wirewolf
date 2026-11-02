import type { Sample } from "./api"

export type Point = { t: number } & Record<string, number | null>

/** How a bucket's round trips are summarised. Loss is always the worst in the bucket. */
export type Agg = "avg" | "p95" | "max"

const SEC = 1e3
const MIN = 60 * SEC
/** Bucket sizes a reader can name: "avg · 2 min". */
const STEPS = [5 * SEC, 10 * SEC, 30 * SEC, MIN, 2 * MIN, 5 * MIN, 10 * MIN, 15 * MIN, 30 * MIN, 60 * MIN, 2 * 60 * MIN, 6 * 60 * MIN]

/**
 * The typical gap between one hop's samples, so buckets can't be smaller than the sampling.
 * History may be coarser than live samples, so it's measured rather than configured.
 */
function sampleInterval(samples: Sample[]): number {
  const byHop = new Map<string, number[]>()
  for (const s of samples) {
    const list = byHop.get(s.hop) ?? []
    list.push(new Date(s.at).getTime())
    byHop.set(s.hop, list)
  }
  let busiest: number[] = []
  for (const list of byHop.values()) if (list.length > busiest.length) busiest = list
  if (busiest.length < 3) return 0
  const deltas = busiest.slice(1).map((t, i) => t - busiest[i]).sort((a, b) => a - b)
  return deltas[Math.floor(deltas.length / 2)]
}

/** The bucket size for a window: a named step no smaller than the sampling or `span / points`. */
export function bucketStep(samples: Sample[], from: number, to: number, points = 300): number {
  const want = Math.max((to - from) / points, 1.5 * sampleInterval(samples))
  return STEPS.find((s) => s >= want) ?? STEPS[STEPS.length - 1]
}

const percentile = (sorted: number[], p: number) => sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))]

const summarise = (v: number[], agg: Agg): number => {
  if (agg === "avg") return v.reduce((a, b) => a + b, 0) / v.length
  const sorted = v.slice().sort((a, b) => a - b)
  return agg === "max" ? sorted[sorted.length - 1] : percentile(sorted, 0.95)
}

/**
 * Samples bucketed into fixed slots of `step` across the window. Every slot is present, and a
 * hop with no sample in a slot gets `null`, so the line breaks where data is missing rather
 * than drawing a line through an outage. Keys are `<hop>_rtt` and `<hop>_loss`.
 */
export function bucket(samples: Sample[], from: number, to: number, { points = 300, agg = "avg" as Agg, step = 0 } = {}): { data: Point[]; step: number } {
  step ||= bucketStep(samples, from, to, points)
  const start = Math.floor(from / step) * step
  const slots = new Map<number, { rtt: Record<string, number[]>; loss: Record<string, number> }>()
  const hops = new Set<string>()
  for (const s of samples) {
    const t = new Date(s.at).getTime()
    if (t < from || t > to) continue
    hops.add(s.hop)
    const k = start + Math.floor((t - start) / step) * step
    let slot = slots.get(k)
    if (!slot) slots.set(k, (slot = { rtt: {}, loss: {} }))
    if (s.rtt_ms != null) (slot.rtt[s.hop] ??= []).push(s.rtt_ms)
    if (s.loss_pct != null) slot.loss[s.hop] = Math.max(slot.loss[s.hop] ?? 0, s.loss_pct)
  }
  const data: Point[] = []
  for (let t = start; t <= to; t += step) {
    const p: Point = { t }
    const slot = slots.get(t)
    for (const hop of hops) {
      p[`${hop}_rtt`] = slot?.rtt[hop] ? summarise(slot.rtt[hop], agg) : null
      p[`${hop}_loss`] = slot?.loss[hop] ?? null
    }
    data.push(p)
  }
  return { data, step }
}

/** Last, mean, p95, worst round trip and worst loss for one hop's samples in a window. */
export interface SeriesStats {
  last: number | null
  avg: number | null
  p95: number | null
  max: number | null
  loss: number | null
  samples: number
}

export function stats(samples: Sample[], hop: string): SeriesStats {
  const mine = samples.filter((s) => s.hop === hop)
  const rtt = mine.map((s) => s.rtt_ms).filter((v): v is number => v != null)
  const sorted = rtt.slice().sort((a, b) => a - b)
  const losses = mine.map((s) => s.loss_pct).filter((v): v is number => v != null)
  return {
    last: rtt.length ? rtt[rtt.length - 1] : null,
    avg: rtt.length ? rtt.reduce((a, b) => a + b, 0) / rtt.length : null,
    p95: rtt.length ? percentile(sorted, 0.95) : null,
    max: rtt.length ? sorted[sorted.length - 1] : null,
    loss: losses.length ? losses.reduce((a, b) => a + b, 0) / losses.length : null,
    samples: mine.length,
  }
}

/** "2 min", "30 s", "1 h": a bucket size as people say it. */
export function stepLabel(step: number): string {
  if (step < MIN) return `${Math.round(step / SEC)} s`
  if (step < 60 * MIN) return `${Math.round(step / MIN)} min`
  return `${Math.round(step / (60 * MIN))} h`
}


/**
 * Splits samples at gaps (probes paused for a speed test, or the server down) so the line breaks
 * there instead of drawing a straight segment across. A gap is a step over 3× the steps either
 * side of it. Comparing with neighbours rather than one global interval matters because history
 * can be coarser than live samples.
 */
export function sparkSeries(samples: Sample[]): { data: { t: number; rtt: number | null }[]; gaps: [number, number][] } {
  const pts = samples.map((s) => ({ t: new Date(s.at).getTime(), rtt: s.rtt_ms }))
  const step = (i: number) => (i >= 1 && i < pts.length ? pts[i].t - pts[i - 1].t : undefined)
  const data: typeof pts = []
  const gaps: [number, number][] = []
  pts.forEach((p, i) => {
    const around = [step(i - 1), step(i + 1)].filter((x): x is number => x != null)
    const dt = step(i)
    if (dt != null && around.length > 0 && dt > 3 * Math.max(...around)) {
      const prev = pts[i - 1]
      gaps.push([prev.t, p.t])
      data.push({ t: prev.t + 1, rtt: null })
    }
    data.push(p)
  })
  return { data, gaps }
}
