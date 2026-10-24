import { describe, expect, it } from "vitest"
import type { Incident, Sample } from "./api"
import { bucket } from "./bucket"
import { duration, mbps, ms, pct, relative } from "./format"
import { reportText } from "./report"
import { href, parse } from "./router"

const at = (min: number) => new Date(Date.UTC(2026, 9, 24, 0, min)).toISOString()
const sample = (min: number, hop: string, rtt: number | null, loss: number | null = 0): Sample => ({
  at: at(min),
  hop,
  status: "pass",
  rtt_ms: rtt,
  jitter_ms: null,
  loss_pct: loss,
})

describe("format", () => {
  it("scales precision", () => {
    expect(ms(0.42)).toBe("0.42 ms")
    expect(ms(1.44)).toBe("1.4 ms")
    expect(ms(38.2)).toBe("38 ms")
    expect(ms(null)).toBe("—")
    expect(pct(0)).toBe("0%")
    expect(pct(9.62)).toBe("9.6%")
    expect(mbps(912.4)).toBe("912 Mbps")
  })
  it("formats relative times", () => {
    const now = Date.parse(at(0))
    expect(relative(at(0), now + 5e3)).toBe("just now")
    expect(relative(at(0), now + 38e3)).toBe("38s ago")
    expect(relative(at(0), now + 3 * 60e3)).toBe("3 min ago")
    expect(relative(at(0), now + 20 * 3600e3)).toBe("20 h ago")
    expect(relative(at(0), now + 72 * 3600e3)).not.toMatch(/ago/)
  })
  it("formats durations", () => {
    expect(duration(at(0), at(4))).toBe("4 min")
    expect(duration(at(0), at(72))).toBe("1 h 12 min")
  })
})

describe("bucket", () => {
  it("averages RTT and keeps the worst loss per slot", () => {
    const from = Date.parse(at(0))
    const to = Date.parse(at(10))
    const pts = bucket([sample(0, "gw", 2, 0), sample(1, "gw", 4, 50), sample(6, "gw", 10, 0), sample(20, "gw", 99)], from, to, 2)
    expect(pts).toEqual([
      { t: from, gw_rtt: 3, gw_loss: 50 },
      { t: from + 5 * 60e3, gw_rtt: 10, gw_loss: 0 },
    ])
  })
  it("skips missing RTT", () => {
    const from = Date.parse(at(0))
    expect(bucket([sample(0, "dns", null, 100)], from, from + 60e3, 1)).toEqual([{ t: from, dns_loss: 100 }])
  })
})

describe("router", () => {
  it("round-trips routes", () => {
    for (const r of [{ name: "now" }, { name: "history" }, { name: "incidents" }, { name: "speed" }, { name: "incident", id: "inc 3/x" }] as const) {
      expect(parse(href(r))).toEqual(r)
    }
  })
  it("falls back to now", () => expect(parse("/nope")).toEqual({ name: "now" }))
})

describe("reportText", () => {
  it("summarizes an incident for an ISP", () => {
    const inc: Incident = {
      id: "inc-1",
      started: at(0),
      ended: at(6),
      severity: "down",
      headline: "Your internet provider's connection is down",
      blame: "isp",
      evidence: {
        at: at(3),
        open_incident: null,
        verdict: { severity: "down", headline: "", detail: "The ISP stopped answering.", blame: "isp", confidence: "high", fix_hint: null, method: "12 probes" },
        hops: [
          {
            id: "isp",
            kind: "isp",
            label: "ISP",
            target: "100.64.0.1",
            status: "fail",
            summary: "no reply",
            metrics: { rtt_ms: null, jitter_ms: null, loss_pct: 100 },
            evidence: [],
            checks: [{ name: "First ISP hop", status: "fail", detail: "timed out", rtt_ms: null }],
          },
        ],
      },
    }
    const text = reportText(inc, null)
    expect(text).toContain("Problem:   Your internet provider's connection is down")
    expect(text).toContain("Duration:  6 min")
    expect(text).toContain("Likely at: ISP")
    expect(text).toContain("[FAIL] ISP        100.64.0.1")
    expect(text).toContain("- First ISP hop: timed out")
  })
})
