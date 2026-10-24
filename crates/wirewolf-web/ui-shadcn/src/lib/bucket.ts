import type { Sample } from "./api"

export type Point = { t: number } & Record<string, number | null>

/** Buckets samples into at most `points` slots per hop: mean RTT, worst loss. */
export function bucket(samples: Sample[], from: number, to: number, points = 300): Point[] {
  const step = Math.max(1, (to - from) / points)
  const slots = new Map<number, { rtt: Record<string, number[]>; loss: Record<string, number> }>()
  for (const s of samples) {
    const t = new Date(s.at).getTime()
    if (t < from || t > to) continue
    const k = from + Math.floor((t - from) / step) * step
    let slot = slots.get(k)
    if (!slot) slots.set(k, (slot = { rtt: {}, loss: {} }))
    if (s.rtt_ms != null) (slot.rtt[s.hop] ??= []).push(s.rtt_ms)
    if (s.loss_pct != null) slot.loss[s.hop] = Math.max(slot.loss[s.hop] ?? 0, s.loss_pct)
  }
  return [...slots.entries()]
    .sort(([a], [b]) => a - b)
    .map(([t, slot]) => {
      const p: Point = { t }
      for (const [hop, v] of Object.entries(slot.rtt)) p[`${hop}_rtt`] = v.reduce((a, b) => a + b, 0) / v.length
      for (const [hop, v] of Object.entries(slot.loss)) p[`${hop}_loss`] = v
      return p
    })
}
