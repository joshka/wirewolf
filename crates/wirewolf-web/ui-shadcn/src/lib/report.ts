import type { Incident, Meta } from "./api"
import { absolute, duration, ms, pct } from "./format"

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
