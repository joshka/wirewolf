import { describe, expect, it } from "vitest"
import type { Snapshot } from "./api"
import { explanationText } from "./explain"

const snapshot = {
  at: "2026-10-24T21:08:00Z",
  hops: [],
  open_incident: null,
  targets: [],
  verdict: {
    severity: "degraded",
    headline: "Packet loss on your local network",
    detail: "Gateway is dropping 15% of packets.",
    blame: "gateway",
    confidence: "medium",
    fix_hint: "Try a cable.",
    method: "16 checks across 6 hops",
    bottom_line: "The problem starts inside your home.",
    confidence_reason: "Medium: one clear signal.",
    held_cycles: 5,
    steps: [
      { hop: "gateway", status: "warn", role: "cause", question: "Can it reach your router?", subject: "192.0.2.1", did: "Sent 20 pings to the router", saw: "3 of 20 pings lost (15%)", limit: "loss under 10%", means: "It's the cause." },
      { hop: "dns", status: "skip", role: "skipped", question: "Do website names resolve?", subject: null, did: "Not run this time", saw: "Skipped", limit: null, means: "Not tested." },
    ],
  },
} as Snapshot

describe("explanationText", () => {
  it("lists each step with its role, result, limit, and meaning", () => {
    const text = explanationText(snapshot)
    expect(text).toContain("The problem starts inside your home.")
    expect(text).toContain("✗ Can it reach your router? (192.0.2.1) [cause]")
    expect(text).toContain("    Sent 20 pings to the router: 3 of 20 pings lost (15%) (limit: loss under 10%)")
    expect(text).toContain("– Do website names resolve?\n")
    expect(text).toContain("Held for 5 checks in a row.")
    expect(text).toContain("Try: Try a cable.")
  })

  it("leaves out what an older server doesn't send", () => {
    const old = { ...snapshot, verdict: { ...snapshot.verdict, steps: undefined, bottom_line: undefined, confidence_reason: undefined, held_cycles: undefined } }
    expect(explanationText(old)).not.toContain("Held for")
  })
})
