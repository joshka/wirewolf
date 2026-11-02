import type { Config, Incident, Meta, Snapshot, SpeedRun } from "./api"
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
  /** Tests in peak hours (weekdays 7–11 pm local time) and the rest. */
  peak: SpeedSummary
  offPeak: SpeedSummary
  /** The download speed 80% of tests reached (the 20th percentile). */
  p20Down: number | null
  p20Up: number | null
}

/** Median speeds over a subset of tests. */
export interface SpeedSummary {
  tests: number
  medianDown: number | null
  medianUp: number | null
}

/**
 * Peak hours as the FCC's Measuring Broadband America programme defines them: weekdays from
 * 7 pm to 11 pm local time, when a shared line is busiest.
 */
export function isPeak(at: string | number | Date): boolean {
  const d = new Date(at)
  const day = d.getDay()
  const hour = d.getHours()
  return day >= 1 && day <= 5 && hour >= 19 && hour < 23
}

const median = (xs: number[]) => {
  if (xs.length === 0) return null
  const s = [...xs].sort((a, b) => a - b)
  const m = Math.floor(s.length / 2)
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2
}

/** The value `share` of the samples reach or exceed (nearest rank, so always a real result). */
const reachedBy = (xs: number[], share: number) => {
  if (xs.length === 0) return null
  const s = [...xs].sort((a, b) => a - b)
  // A hair of slack: (1 - 0.8) * 5 is 0.9999… in floating point.
  return s[Math.min(s.length - 1, Math.floor((1 - share) * s.length + 1e-9))]
}

const downs = (tests: SpeedRun[]) => tests.flatMap((r) => (r.down_mbps == null ? [] : [r.down_mbps]))
const ups = (tests: SpeedRun[]) => tests.flatMap((r) => (r.up_mbps == null ? [] : [r.up_mbps]))
const summarize = (tests: SpeedRun[]): SpeedSummary => ({ tests: tests.length, medianDown: median(downs(tests)), medianUp: median(ups(tests)) })

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
    medianDown: median(downs(tests)),
    medianUp: median(ups(tests)),
    underPlan: tests.filter((r) => under(r.down_mbps, plan.down) || under(r.up_mbps, plan.up)).length,
    peak: summarize(tests.filter((r) => isPeak(r.started))),
    offPeak: summarize(tests.filter((r) => !isPeak(r.started))),
    p20Down: reachedBy(downs(tests), 0.8),
    p20Up: reachedBy(ups(tests), 0.8),
  }
}

/**
 * How the numbers were measured, in the order a support agent will question them: the machine,
 * the probes, the incident rule, and the speed tests. Only what the config and snapshot say;
 * nothing is claimed that wasn't done.
 */
export function methodology(config: Config | null, snapshot: Snapshot | null): string[] {
  const lines: string[] = []
  const port = snapshot?.hops.find((h) => h.id === "host")?.interfaces?.find((i) => i.default_route)
  if (port) {
    const link = port.wireless ? "over Wi-Fi" : port.speed_mbps ? `over a wired ${port.speed_mbps >= 1000 ? `${port.speed_mbps / 1000} Gbps` : `${port.speed_mbps} Mbps`} port` : "over a wired port"
    lines.push(`wirewolf ran on a machine connected ${link} (${port.name}).`)
  }
  if (config) {
    const targets = config.internet.targets.map((t) => t.name).join(", ")
    lines.push(
      `Every ${config.probe.interval} it pinged the router and the provider's first router, made TCP connections to ${targets} over IPv4 and IPv6, and checked DNS and HTTPS.`,
    )
    const { open_after, close_after } = config.incidents
    lines.push(
      `An incident opens after ${open_after} failing ${open_after === 1 ? "check" : "checks in a row"} and closes after ${close_after} good ${close_after === 1 ? "one" : "ones"}. It is blamed on the provider only when the router itself answers normally.`,
    )
    const sp = config.speed
    if (sp.interval !== "0" && sp.interval !== "0s") {
      const where = sp.backend === "ookla" ? (sp.server ? `speedtest.net server ${sp.server}` : "the nearest speedtest.net server") : "Cloudflare"
      lines.push(`Speed tests ran every ${sp.interval} against ${where}, with ${sp.streams} parallel streams for ${sp.duration} in each direction.`)
    }
  }
  lines.push("Peak hours are weekdays 7–11 pm local time, as in the FCC's Measuring Broadband America reports. Every test is counted; none were discarded.")
  return lines
}

const msSpan = (v: number) => duration(new Date(0).toISOString(), new Date(v).toISOString())

/** The connection report as plain text, for an email or a support chat. */
export function periodReportText(r: PeriodReport, plan: Plan, meta: Meta | null, method: string[] = []): string {
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
    if (r.peak.tests > 0 && r.offPeak.tests > 0) {
      lines.push(
        `Peak hours (weekdays 7–11 pm): ${r.peak.tests} tests, median ${mbps(r.peak.medianDown)} down${share(r.peak.medianDown, plan.down)} and ${mbps(r.peak.medianUp)} up. Other times: ${r.offPeak.tests} tests, median ${mbps(r.offPeak.medianDown)} down and ${mbps(r.offPeak.medianUp)} up.`,
      )
    }
    lines.push(`80% of tests reached at least ${mbps(r.p20Down)} down and ${mbps(r.p20Up)} up.`)
  }
  if (r.provider.length > 0) {
    lines.push("", "Provider-side outages:")
    for (const i of r.provider) {
      lines.push(`  ${date(i.started)}  ${duration(i.started, i.ended, r.to).padEnd(10)} ${i.severity.padEnd(8)} ${i.headline}`)
    }
  }
  if (method.length > 0) {
    lines.push("", "How this was measured:", ...method.map((l) => `  ${l}`))
  }
  return lines.join("\n")
}
