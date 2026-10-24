import type { HopKind } from "./api"

/** Series colors by hop kind (theme tokens in index.css). Host has no series. */
export const hopColor: Record<HopKind, string> = {
  host: "var(--muted-foreground)",
  gateway: "var(--hop-gateway)",
  isp: "var(--hop-isp)",
  internet: "var(--hop-internet)",
  dns: "var(--hop-dns)",
  web: "var(--hop-web)",
  target: "var(--primary)",
}
