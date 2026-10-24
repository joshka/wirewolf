import { describe, expect, it } from "vitest"
import type { Hop, Incident, Interface, Sample } from "./api"
import { bucket, sparkSeries } from "./bucket"
import { duration, mbps, ms, pct, relative } from "./format"
import { linkSpeed, portStatus, total } from "./ports"
import { periodReport, reportText } from "./report"
import { uptime } from "./uptime"
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
    const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto", style: "short" })
    expect(relative(at(0), now + 5e3)).toBe(rtf.format(0, "second"))
    expect(relative(at(0), now + 38e3)).toBe(rtf.format(-38, "second"))
    expect(relative(at(0), now + 3 * 60e3)).toBe(rtf.format(-3, "minute"))
    expect(relative(at(0), now + 20 * 3600e3)).toBe(rtf.format(-20, "hour"))
    expect(relative(at(0), now + 72 * 3600e3)).toBe(new Date(at(0)).toLocaleDateString(undefined, { month: "short", day: "numeric" }))
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
    for (const r of [{ name: "now" }, { name: "history" }, { name: "incidents" }, { name: "speed" }, { name: "logs" }, { name: "logs", tab: "server" }, { name: "incident", id: "inc 3/x" }, { name: "hop", id: "isp" }, { name: "hop", id: "dns/1", incident: "inc 3" }] as const) {
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

describe("sparkSeries", () => {
  it("breaks the line at a gap of more than 3× the usual interval", () => {
    const s = [0, 1, 2, 3, 9, 10].map((m) => sample(m, "gateway", 2))
    const { data, gaps } = sparkSeries(s)
    expect(gaps).toEqual([[new Date(at(3)).getTime(), new Date(at(9)).getTime()]])
    expect(data.filter((p) => p.rtt == null)).toHaveLength(1)
  })

  it("doesn't treat a change from coarse history to fine live samples as a gap", () => {
    const s = [0, 5, 10, 15, 16, 17, 18].map((m) => sample(m, "gateway", 2))
    expect(sparkSeries(s).gaps).toEqual([])
  })

  it("leaves an evenly spaced series alone", () => {
    const { data, gaps } = sparkSeries([0, 1, 2].map((m) => sample(m, "gateway", 2)))
    expect(gaps).toEqual([])
    expect(data).toHaveLength(3)
  })
})

describe("ports", () => {
  const port = (name: string, link: boolean, speed: number | null = link ? 1000 : null): Interface => ({
    name, via: null, default_route: false, link, wireless: false, speed_mbps: speed, duplex: link ? "full" : null,
    addresses: [], rx_errors: 0, tx_errors: 0, rx_dropped: 0, tx_dropped: 0, carrier_changes: 0,
  })
  const hop = (checks: [string, "pass" | "warn"][]): Hop => ({
    id: "host", kind: "host", label: "This host", target: null, status: "pass", summary: "",
    metrics: { rtt_ms: null, jitter_ms: null, loss_pct: null }, evidence: [],
    checks: checks.map(([name, status]) => ({ name, status, detail: "", rtt_ms: null })),
  })
  it("marks the port a warning names", () => {
    const h = hop([["eth0 speed", "warn"], ["Interface up", "pass"]])
    expect(portStatus(h, port("eth0", true))).toBe("warn")
    expect(portStatus(h, port("eth1", true))).toBe("pass")
    expect(portStatus(h, port("eth2", false))).toBe("not_applicable")
    expect(portStatus(hop([["eth0 eth1 subnet", "warn"]]), port("eth1", true))).toBe("warn")
  })
  it("formats speed", () => {
    expect(linkSpeed(port("eth0", true))).toBe("1 Gbps full")
    expect(linkSpeed(port("eth0", true, 100))).toBe("100 Mbps full")
    expect(linkSpeed(port("eth0", false))).toBe("—")
    expect(total(null, null)).toBeNull()
    expect(total(2, null)).toBe(2)
  })
})

describe("uptime", () => {
  const inc = (startMin: number, endMin: number | null, severity: "down" | "degraded"): Incident => ({
    id: `i${startMin}`,
    started: at(startMin),
    ended: endMin == null ? null : at(endMin),
    severity,
    headline: "",
    blame: null,
    evidence: {} as Incident["evidence"],
  })
  const now = new Date(at(24 * 60)).getTime()

  it("counts only down time against uptime, and only while watching", () => {
    const u = uptime([inc(60, 90, "down"), inc(120, 180, "degraded")], at(0), now, 3)
    expect(u.watched).toBe(24 * 3600e3)
    expect(u.down).toBe(30 * 60e3)
    expect(u.incidents).toBe(2)
    expect(u.ratio).toBeCloseTo(1 - 30 / (24 * 60))
  })

  it("marks days before monitoring began as unwatched", () => {
    const u = uptime([], at(12 * 60), now, 3)
    expect(u.days[0].watched).toBe(0)
    expect(u.watched).toBe(12 * 3600e3)
  })

  it("has no ratio with nothing watched", () => {
    expect(uptime([], null, now, 3).ratio).toBeNull()
  })
})

describe("periodReport", () => {
  const now = new Date(at(24 * 60)).getTime()
  const inc = (id: string, startMin: number, endMin: number, blame: string, severity: "down" | "degraded" = "down"): Incident => ({
    id,
    started: at(startMin),
    ended: at(endMin),
    severity,
    headline: id,
    blame,
    evidence: {} as Incident["evidence"],
  })
  const run = (min: number, down: number, up: number) => ({ id: `s${min}`, started: at(min), state: "complete" as const, backend: "demo", server: null, down_mbps: down, up_mbps: up, idle_latency_ms: 9, loaded_latency_ms: 15, bloat: "A" as const, bytes_used: 1, progress: null, error: null })

  it("separates provider-side outages and clips downtime to the period", () => {
    const r = periodReport([inc("isp", 60, 70, "isp"), inc("wifi", 100, 130, "gateway"), inc("old", -120, 30, "internet")], [], { down: null, up: null }, now, 1)
    expect(r.provider.map((i) => i.id)).toEqual(["old", "isp"])
    expect(r.providerDown).toBe((30 + 10) * 60e3)
    expect(r.down).toBe((30 + 10 + 30) * 60e3)
  })

  it("counts tests under 80% of the plan", () => {
    const r = periodReport([], [run(10, 900, 900), run(20, 700, 900), run(30, 950, 100)], { down: 1000, up: 1000 }, now, 1)
    expect(r.medianDown).toBe(900)
    expect(r.underPlan).toBe(2)
  })
})
