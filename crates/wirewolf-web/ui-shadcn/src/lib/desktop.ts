// The desktop app (crates/wirewolf-app) shows this dashboard in a native window. It sets
// `window.__WIREWOLF_DESKTOP__` before the page loads, and sends the menu bar's commands as
// Tauri events. The Tauri modules load only inside the app, so the web bundle doesn't carry them.

import { toast } from "sonner"
import { api } from "@/lib/api"
import { copyText } from "@/lib/clipboard"

declare global {
  interface Window {
    __WIREWOLF_DESKTOP__?: boolean
  }
}

/** True inside the desktop app. */
export const isDesktop = typeof window !== "undefined" && window.__WIREWOLF_DESKTOP__ === true

export interface DesktopHandlers {
  navigate(path: string): void
  toggleSidebar(): void
}

/** Wires the menu bar's events to the dashboard. Returns a function that undoes it. */
export async function connectDesktop(handlers: DesktopHandlers): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event")
  const stops = await Promise.all([
    listen<string>("navigate", (e) => handlers.navigate(e.payload)),
    listen("toggle-sidebar", () => handlers.toggleSidebar()),
    listen("reload", () => location.reload()),
    listen<string>("download", (e) => location.assign(e.payload)),
    listen("run-speed", async () => {
      handlers.navigate("/speed")
      const status = await api.runSpeed()
      if (status === 202) toast.success("Speed test started")
      else if (status === 409) toast.info("A speed test is already running")
      else toast.error(`Couldn't start a speed test (HTTP ${status})`)
    }),
    listen("copy-brief", async () => {
      try {
        await copyText(await api.briefPrompt({ redact: true }))
        toast.success("Copied the brief. Paste it into an AI assistant.")
      } catch {
        toast.error("Couldn't build the brief. Is wirewolf still measuring?")
      }
    }),
  ])
  return () => stops.forEach((stop) => stop())
}

/** Tells the window the dashboard's theme, so its own chrome matches. */
export async function syncDesktopTheme(theme: string | undefined): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core")
  await invoke("set_theme", { theme: theme ?? "system" })
}
