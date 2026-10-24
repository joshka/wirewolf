import type { Incident, Meta, SpeedRun } from "./api"
import { absolute, duration, mbps, ms, pct } from "./format"
import { PLAN_SHORTFALL, type Plan } from "./live"

/** A plain-text summary to paste into an ISP support chat or email. */
export function reportText(i: Incident, meta: Meta | null): string {
  const v = i.evidence.verdict
  const lines = [
    `Network incident report (wirewolf${meta ? ` ${meta.version} on ${meta.host}` : ""})`,
    "",
    `Problem:   ${i.headline}`,
    `Severity:  ${i.severity}`,
    `Started:   ${absolute(i.started)} (${i.started})`,
    `Ended:     ${i.ended ? `${absolute(i.ended)} (${i.ended})` : "ongoing"}`,
    `Duration:  ${duration(i.started, i.ended)}${i.ended ? "" : " so far"}`,
    `Likely at: ${i.evidence.hops.find((h) => h.id === i.blame)?.label ?? "unknown"}`,
    "",
    `Diagnosis: ${v.detail}`,
    `Method:    ${v.method} (${v.confidence} confidence)`,
    "",
    "Path at the worst point:",
  ]
  for (const h of i.evidence.hops) {
    const m = h.metrics
    const metrics = [m.rtt_ms != null && `rtt ${ms(m.rtt_ms)}`, m.loss_pct != null && `loss ${pct(m.loss_pct)}`].filter(Boolean).join(", ")
    lines.push(`  [${h.status.toUpperCase().padEnd(4)}] ${h.label.padEnd(10)} ${h.target ?? ""}`.trimEnd())
    lines.push(`         ${h.summary}${metrics ? ` (${metrics})` : ""}`)
    for (const c of h.checks.filter((c) => c.status === "fail" || c.status === "warn")) {
      lines.push(`         - ${c.name}: ${c.detail}`)
    }
  }
  return lines.join("\n")
}

/** Hops on the provider's side of the router: what an ISP report is about. */
const PROVIDER = new Set(["isp", "internet"])

export interface PeriodReport {
  from: number
  to: number
  /** Incidents overlapping the period, oldest first. */
  incidents: Incident[]
  /** Those blamed on the provider's side (the ISP or internet hop). */
  provider: Incident[]
  /** Milliseconds of Down incidents on the provider's side, within the period. */
  providerDown: number
  /** Milliseconds of any Down incident, within the period. */
  down: number
  /** Completed speed tests in the period, oldest first. */
  tests: SpeedRun[]
  medianDown: number | null
  medianUp: number | null
  /** Tests under 80% of the plan in either direction (0 without a plan). */
  underPlan: number
}

const median = (xs: number[]) => {
  if (xs.length === 0) return null
  const s = [...xs].sort((a, b) => a - b)
  const m = Math.floor(s.length / 2)
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}

/** Everything the connection report shows, for the `days` before `now`. */
export function periodReport(incidents: Incident[], runs: SpeedRun[], plan: Plan, now: number, days: number): PeriodReport {
  const from = now - days * 86400e3
  const end = (i: Incident) => (i.ended ? new Date(i.ended).getTime() : now)
  const inside = (i: Incident) => Math.max(0, Math.min(end(i), now) - Math.max(new Date(i.started).getTime(), from))
  const inPeriod = incidents.filter((i) => end(i) > from && new Date(i.started).getTime() < now).sort((a, b) => a.started.localeCompare(b.started))
  const provider = inPeriod.filter((i) => i.blame != null && PROVIDER.has(i.blame))
  const downTime = (list: Incident[]) => list.filter((i) => i.severity === "down").reduce((a, i) => a + inside(i), 0)
  const tests = runs
    .filter((r) => r.state === "complete" && new Date(r.started).getTime() >= from && new Date(r.started).getTime() <= now)
    .sort((a, b) => a.started.localeCompare(b.started))
  const under = (v: number | null, p: number | null) => v != null && p != null && v < p * PLAN_SHORTFALL
  return {
    from,
    to: now,
    incidents: inPeriod,
    provider,
    providerDown: downTime(provider),
    down: downTime(inPeriod),
    tests,
    medianDown: median(tests.flatMap((r) => (r.down_mbps == null ? [] : [r.down_mbps]))),
    medianUp: median(tests.flatMap((r) => (r.up_mbps == null ? [] : [r.up_mbps]))),
    underPlan: tests.filter((r) => under(r.down_mbps, plan.down) || under(r.up_mbps, plan.up)).length,
  }
}

const msSpan = (v: number) => duration(new Date(0).toISOString(), new Date(v).toISOString())

/** The connection report as plain text, for an email or a support chat. */
export function periodReportText(r: PeriodReport, plan: Plan, meta: Meta | null): string {
  const date = (t: number | string) => new Date(t).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })
  const lines = [
    `Connection report (wirewolf${meta ? ` ${meta.version} on ${meta.host}` : ""})`,
    `Period: ${date(r.from)} to ${date(r.to)}`,
    "",
    `Provider-side outages: ${r.provider.length}, down for ${msSpan(r.providerDown)} in total.`,
    `All incidents: ${r.incidents.length}, down for ${msSpan(r.down)} in total.`,
  ]
  if (r.tests.length > 0) {
    const share = (v: number | null, p: number | null) => (v != null && p != null ? ` (${Math.round((v / p) * 100)}% of the ${p} Mbps plan)` : "")
    lines.push(
      `Speed: ${r.tests.length} tests, median ${mbps(r.medianDown)} down${share(r.medianDown, plan.down)} and ${mbps(r.medianUp)} up${share(r.medianUp, plan.up)}.`,
    )
    if (plan.down != null || plan.up != null) lines.push(`Tests under 80% of the plan: ${r.underPlan} of ${r.tests.length}.`)
  }
  if (r.provider.length > 0) {
    lines.push("", "Provider-side outages:")
    for (const i of r.provider) {
      lines.push(`  ${date(i.started)}  ${duration(i.started, i.ended, r.to).padEnd(10)} ${i.severity.padEnd(8)} ${i.headline}`)
    }
  }
  return lines.join("\n")
}
