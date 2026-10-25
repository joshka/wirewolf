import type { Snapshot, Step } from "./api"

const mark: Record<Step["role"], string> = { cause: "✗", collateral: "·", excused: "✓", pass: "✓", skipped: "–" }
const roleText: Partial<Record<Step["role"], string>> = { cause: "cause", collateral: "knock-on", excused: "excused" }

/** The verdict and its steps as plain text, for pasting into a support chat or an email. */
export function explanationText(s: Snapshot): string {
  const v = s.verdict
  const lines = [`wirewolf · ${new Date(s.at).toLocaleString()}`, v.headline, v.detail]
  if (v.bottom_line) lines.push("", v.bottom_line)
  lines.push("")
  for (const step of v.steps ?? []) {
    const tag = roleText[step.role] ? ` [${roleText[step.role]}]` : ""
    lines.push(`${mark[step.role]} ${step.question}${step.subject ? ` (${step.subject})` : ""}${tag}`)
    lines.push(`    ${step.did}: ${step.saw}${step.limit ? ` (limit: ${step.limit})` : ""}`)
    lines.push(`    ${step.means}`)
  }
  lines.push("")
  if (v.confidence_reason) lines.push(`How sure: ${v.confidence_reason}`)
  if (v.held_cycles) lines.push(`Held for ${v.held_cycles} ${v.held_cycles === 1 ? "check" : "checks"} in a row.`)
  if (v.fix_hint && v.severity !== "healthy") lines.push(`Try: ${v.fix_hint}`)
  return lines.join("\n")
}
