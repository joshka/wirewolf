// Axis ticks that land on times people can read: :00, :15, midnight. Recharts spaces ticks
// evenly, which gives 04:11, 04:47 (recharts/recharts#5816), so the chart passes these in.

const SEC = 1e3
const MIN = 60 * SEC
const HOUR = 60 * MIN
export const DAY = 24 * HOUR

const STEPS = [MIN, 2 * MIN, 5 * MIN, 10 * MIN, 15 * MIN, 30 * MIN, HOUR, 2 * HOUR, 3 * HOUR, 6 * HOUR, 12 * HOUR, DAY, 2 * DAY, 7 * DAY]

const offset = (t: number) => -new Date(t).getTimezoneOffset() * MIN

/** The first multiple of `step` at or after `t`, counted in local time. */
function alignUp(t: number, step: number): number {
  if (step < DAY) {
    const local = t + offset(t)
    return Math.ceil(local / step) * step - offset(t)
  }
  const d = new Date(t)
  d.setHours(0, 0, 0, 0)
  if (d.getTime() < t) d.setDate(d.getDate() + 1)
  return d.getTime()
}

function next(t: number, step: number): number {
  if (step < DAY) return alignUp(t + 1, step)
  const d = new Date(t)
  d.setDate(d.getDate() + step / DAY)
  return d.getTime()
}

/** Round clock-time ticks between `from` and `to`, at most `max` of them, and the step used. */
export function timeTicks(from: number, to: number, max = 6): { ticks: number[]; step: number } {
  const span = Math.max(1, to - from)
  const step = STEPS.find((s) => span / s <= max) ?? STEPS[STEPS.length - 1]
  const ticks: number[] = []
  for (let t = alignUp(from, step); t <= to && ticks.length < 100; t = next(t, step)) ticks.push(t)
  return { ticks, step }
}

const isMidnight = (t: number) => {
  const d = new Date(t)
  return d.getHours() === 0 && d.getMinutes() === 0
}

const dayLabel = (t: number) => new Date(t).toLocaleDateString(undefined, { month: "short", day: "numeric" })

/** "09:40" within a day, with the date where a day starts or when ticks are a day apart. */
export function tickLabel(t: number, step: number): string {
  if (step >= DAY || isMidnight(t)) return dayLabel(t)
  return new Date(t).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })
}

/** The viewer's timezone as people write it: "AEDT", "PST", or "GMT+2" where there's no name. */
export function zoneLabel(t = Date.now()): string {
  const parts = new Intl.DateTimeFormat(undefined, { timeZoneName: "short" }).formatToParts(new Date(t))
  return parts.find((p) => p.type === "timeZoneName")?.value ?? "local time"
}

/** Log-axis ticks: 1, 2, 5 × 10ⁿ within the domain, thinned to decades when there are too many. */
export function logTicks(lo: number, hi: number, max = 6): number[] {
  const all: number[] = []
  for (let e = Math.floor(Math.log10(lo)); e <= Math.ceil(Math.log10(hi)); e++) {
    for (const m of [1, 2, 5]) {
      const v = m * 10 ** e
      if (v >= lo && v <= hi) all.push(v)
    }
  }
  if (all.length <= max) return all
  const decades = all.filter((v) => Number.isInteger(Math.log10(v)))
  return decades.length ? decades : all.slice(0, max)
}

/** A log-scale domain covering the values, with a floor since log(0) is undefined. */
export function logDomain(values: number[], floor = 0.5): [number, number] {
  const finite = values.filter((v) => Number.isFinite(v))
  const min = finite.length ? Math.min(...finite) : floor
  const max = finite.length ? Math.max(...finite) : floor * 100
  const lo = Math.max(floor, 10 ** Math.floor(Math.log10(Math.max(min, floor))))
  // The top is the next 1, 2 or 5 × 10ⁿ above the data, so a chart isn't two-thirds empty.
  const top = Math.max(max, lo * 10)
  const e = 10 ** Math.floor(Math.log10(top))
  const hi = [1, 2, 5, 10].map((m) => m * e).find((v) => v >= top) ?? top
  return [lo, hi]
}

/** Pushes crowded label positions apart (top to bottom) so none sit closer than `gap`. */
export function spread(positions: number[], gap: number, min = -Infinity, max = Infinity): number[] {
  const order = positions.map((y, i) => [y, i] as const).sort((a, b) => a[0] - b[0])
  const out = positions.slice()
  let last = -Infinity
  for (const [y, i] of order) {
    const v = Math.max(y, last + gap, min)
    out[i] = v
    last = v
  }
  // Pushing down can run past the bottom; shove the pile back up.
  const over = last - max
  if (over > 0) {
    let prev = Infinity
    for (let k = order.length - 1; k >= 0; k--) {
      const i = order[k][1]
      out[i] = Math.min(out[i] - over, prev - gap)
      prev = out[i]
    }
  }
  return out
}
