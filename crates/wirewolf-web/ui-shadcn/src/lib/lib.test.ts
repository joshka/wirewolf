import { describe, expect, it } from "vitest"
import type { Config, Hop, Incident, Interface, Sample, Snapshot } from "./api"
import { bucket, bucketStep, stats, stepLabel } from "./bucket"
import { logDomain, logTicks, spread, timeTicks } from "./ticks"
import { duration, mbps, ms, pct, relative } from "./format"
import { linkSpeed, portStatus, total } from "./ports"
import { isPeak, methodology, periodReport, periodReportText, reportText } from "./report"
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
  const from = Date.parse(at(0))
  const to = Date.parse(at(10))
  it("averages RTT and keeps the worst loss per slot", () => {
    const { data, step } = bucket([sample(0, "gw", 2, 0), sample(1, "gw", 4, 50), sample(6, "gw", 10, 0), sample(20, "gw", 99)], from, to, { step: 5 * 60e3 })
    expect(step).toBe(5 * 60e3)
    expect(data).toEqual([
      { t: from, gw_rtt: 3, gw_loss: 50 },
      { t: from + 5 * 60e3, gw_rtt: 10, gw_loss: 0 },
      { t: from + 10 * 60e3, gw_rtt: null, gw_loss: null },
    ])
  })
  it("leaves a null where a hop has no sample, so the line breaks", () => {
    const { data } = bucket([sample(0, "gw", 2), sample(0, "isp", 8), sample(5, "gw", 3)], from, to, { step: 5 * 60e3 })
    expect(data[1]).toEqual({ t: from + 5 * 60e3, gw_rtt: 3, gw_loss: 0, isp_rtt: null, isp_loss: null })
  })
  it("summarises as p95 or max", () => {
    const s = [1, 2, 3, 4, 100].map((v, i) => sample(i, "gw", v))
    expect(bucket(s, from, to, { step: 10 * 60e3, agg: "max" }).data[0].gw_rtt).toBe(100)
    expect(bucket(s, from, to, { step: 10 * 60e3, agg: "p95" }).data[0].gw_rtt).toBe(100)
    expect(bucket(s, from, to, { step: 10 * 60e3, agg: "avg" }).data[0].gw_rtt).toBe(22)
  })
  it("picks a named step no finer than the sampling", () => {
    const every30s = Array.from({ length: 20 }, (_, i) => ({ ...sample(0, "gw", 1), at: new Date(from + i * 30e3).toISOString() }))
    expect(bucketStep(every30s, from, from + 10 * 60e3, 300)).toBe(60e3)
    expect(bucketStep(every30s, from, from + 24 * 3600e3, 300)).toBe(5 * 60e3)
    expect(bucketStep([], from, from + 3600e3, 300)).toBe(30e3)
  })
  it("computes stats per hop", () => {
    const s = [1, 2, 3, 4, 100].map((v, i) => sample(i, "gw", v, i === 4 ? 50 : 0))
    expect(stats(s, "gw")).toEqual({ last: 100, avg: 22, p95: 100, max: 100, loss: 10, samples: 5 })
    expect(stats(s, "isp").last).toBeNull()
  })
  it("names the step", () => {
    expect(stepLabel(30e3)).toBe("30 s")
    expect(stepLabel(2 * 60e3)).toBe("2 min")
    expect(stepLabel(2 * 3600e3)).toBe("2 h")
  })
})

describe("ticks", () => {
  // Local time matters here; the tests avoid a fixed zone by reading it back.
  const minutes = (t: number) => new Date(t).getMinutes()
  it("lands on round clock times", () => {
    const from = Date.UTC(2026, 9, 24, 3, 7)
    const { ticks, step } = timeTicks(from, from + 3600e3)
    expect(step).toBe(10 * 60e3)
    expect(ticks.length).toBe(6)
    for (const t of ticks) expect(minutes(t) % 10).toBe(0)
    expect(ticks[0]).toBeGreaterThanOrEqual(from)
  })
  it("steps by whole days on long ranges", () => {
    const from = Date.UTC(2026, 9, 20, 3, 7)
    const { ticks, step } = timeTicks(from, from + 7 * 86400e3)
    expect(step).toBe(2 * 86400e3)
    for (const t of ticks) expect(new Date(t).getHours()).toBe(0)
  })
  it("chooses log ticks", () => {
    expect(logTicks(1, 100)).toEqual([1, 2, 5, 10, 20, 50, 100].slice(0, 7).filter((v) => [1, 10, 100].includes(v)))
    expect(logTicks(1, 10)).toEqual([1, 2, 5, 10])
    expect(logDomain([1.2, 35, 80])).toEqual([1, 100])
    expect(logDomain([1.2, 35, 120])).toEqual([1, 200])
    expect(logDomain([0.02, 3])).toEqual([0.5, 5])
  })
  it("spreads crowded labels apart", () => {
    expect(spread([10, 12, 50], 13)).toEqual([10, 23, 50])
    expect(spread([95, 97], 13, 0, 100)).toEqual([87, 100])
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

  it("splits tests into peak and off-peak hours and finds the speed 80% reached", () => {
    // A Wednesday evening: 20:00 local is peak, 09:00 is not.
    const peak = new Date(2026, 10, 4, 20, 0).toISOString()
    const day = new Date(2026, 10, 4, 9, 0).toISOString()
    expect(isPeak(peak)).toBe(true)
    expect(isPeak(day)).toBe(false)
    expect(isPeak(new Date(2026, 10, 7, 20, 0))).toBe(false) // Saturday
    const at = (iso: string, down: number) => ({ ...run(0, down, 100), id: iso + down, started: iso })
    const end = new Date(2026, 10, 4, 23, 0).getTime()
    const r = periodReport([], [at(peak, 300), at(peak, 320), at(day, 900), at(day, 950), at(day, 940)], { down: 1000, up: null }, end, 1)
    expect(r.peak).toEqual({ tests: 2, medianDown: 310, medianUp: 100 })
    expect(r.offPeak.tests).toBe(3)
    expect(r.offPeak.medianDown).toBe(940)
    // Sorted downs: 300 320 900 940 950; the value 80% reach is the second lowest.
    expect(r.p20Down).toBe(320)
    const text = periodReportText(r, { down: 1000, up: null }, null, ["Measured carefully."])
    expect(text).toContain("Peak hours (weekdays 7–11 pm): 2 tests, median 310 Mbps down (31% of the 1000 Mbps plan)")
    expect(text).toContain("80% of tests reached at least 320 Mbps down")
    expect(text).toContain("How this was measured:\n  Measured carefully.")
  })
})

describe("methodology", () => {
  it("describes the machine, the probes, the incident rule, and the speed tests", () => {
    const config = {
      probe: { interval: "30s", ipv6: "auto", path_mtu: true },
      internet: { port: 443, icmp: true, targets: [{ name: "Cloudflare", v4: "1.1.1.1", v6: null }, { name: "Quad9", v4: "9.9.9.9", v6: null }] },
      incidents: { open_after: 2, close_after: 2 },
      speed: { backend: "cloudflare", interval: "1h", server: "", streams: 8, duration: "10s" },
    } as unknown as Config
    const port = { name: "eth0", via: null, default_route: true, link: true, wireless: false, speed_mbps: 1000, duplex: "full", addresses: [], rx_errors: null, tx_errors: null, rx_dropped: null, tx_dropped: null, carrier_changes: null }
    const snapshot = { hops: [{ id: "host", interfaces: [port] }] } as unknown as Snapshot
    const lines = methodology(config, snapshot)
    expect(lines[0]).toBe("wirewolf ran on a machine connected over a wired 1 Gbps port (eth0).")
    expect(lines[1]).toContain("Every 30s it pinged the router")
    expect(lines[1]).toContain("Cloudflare, Quad9")
    expect(lines[2]).toContain("opens after 2 failing checks in a row and closes after 2 good ones")
    expect(lines[3]).toBe("Speed tests ran every 1h against Cloudflare, with 8 parallel streams for 10s in each direction.")
    expect(lines.at(-1)).toContain("Every test is counted")
    // Scheduled tests off: no speed line, and no machine line without a snapshot.
    const off = methodology({ ...config, speed: { ...config.speed, interval: "0" } }, null)
    expect(off.some((l) => l.startsWith("Speed tests"))).toBe(false)
    expect(off[0]).toContain("Every 30s")
  })
})
