// User targets ride along in the snapshot. Shown as hops, they get the same drill-down page,
// charts, and incident list as the path.

import type { Hop, Snapshot, Target } from "./api"

export function targetHop(t: Target): Hop {
  return {
    id: t.id,
    kind: "target",
    label: t.name,
    target: t.check,
    status: t.status,
    summary: t.detail,
    metrics: { rtt_ms: t.rtt_ms, jitter_ms: null, loss_pct: t.loss_pct },
    evidence: [
      { key: "check", value: t.check },
      { key: "kind", value: t.kind },
      { key: "critical", value: t.critical ? "yes: failing degrades the verdict and opens incidents" : "no: shown, never an incident" },
    ],
    checks: [{ name: t.name, status: t.status, detail: t.detail, rtt_ms: t.rtt_ms }],
  }
}

/** A hop or target by id. */
export function findHop(snapshot: Snapshot | null | undefined, id: string): Hop | undefined {
  if (!snapshot) return undefined
  const hop = snapshot.hops.find((h) => h.id === id)
  if (hop) return hop
  const t = snapshot.targets?.find((x) => x.id === id)
  return t && targetHop(t)
}
