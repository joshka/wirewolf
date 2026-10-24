import type { Hop, Interface, Status } from "./api"

/** A port's status: `warn` when one of the hop's checks names it, `pass` when linked, else unplugged. */
export function portStatus(hop: Hop, port: Interface): Status {
  if (hop.checks.some((c) => c.status === "warn" && c.name.split(" ").slice(0, -1).includes(port.name))) return "warn"
  return port.link ? "pass" : "not_applicable"
}

/** `1 Gbps full`, `100 Mbps half`, or `—` with no link. */
export function linkSpeed(port: Interface): string {
  if (!port.link || port.speed_mbps == null) return "—"
  const speed = port.speed_mbps >= 1000 ? `${port.speed_mbps / 1000} Gbps` : `${port.speed_mbps} Mbps`
  return port.duplex ? `${speed} ${port.duplex}` : speed
}

/** The sum of two counters, or `null` when the OS reports neither. */
export function total(a: number | null, b: number | null): number | null {
  return a == null && b == null ? null : (a ?? 0) + (b ?? 0)
}
