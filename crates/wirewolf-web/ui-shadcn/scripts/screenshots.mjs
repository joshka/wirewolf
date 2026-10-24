// Captures the comparison screenshots into docs/compare/ui-shadcn/.
// Usage: start `wirewolf serve --demo --listen 127.0.0.1:7471`, then `pnpm screenshots`.
import { mkdirSync } from "node:fs"
import { chromium } from "playwright"

const BASE = process.env.WIREWOLF_URL ?? "http://127.0.0.1:7471"
const UI = `${BASE}/ui/shadcn`
const OUT = new URL("../../../../docs/compare/ui-shadcn/", import.meta.url).pathname
mkdirSync(OUT, { recursive: true })

const scenario = async (id) => {
  await fetch(`${BASE}/api/v1/demo/scenario`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id }),
  })
  await new Promise((r) => setTimeout(r, 4500)) // two demo cycles
}

const browser = await chromium.launch()
const page = async (width, scheme) => {
  const ctx = await browser.newContext({
    viewport: { width, height: width > 600 ? 1000 : 844 },
    deviceScaleFactor: width > 600 ? 1 : 2,
    colorScheme: scheme,
    reducedMotion: "reduce",
  })
  return ctx.newPage()
}
const shot = async (p, name, full = true) => {
  await p.waitForTimeout(600)
  await p.screenshot({ path: `${OUT}${name}.png`, fullPage: full })
  console.log(name)
}

for (const sc of ["healthy", "dns_down", "wifi_loss"]) {
  await scenario(sc)
  for (const scheme of ["dark", "light"]) {
    for (const w of [1440, 390]) {
      const p = await page(w, scheme)
      await p.goto(`${UI}/`)
      await p.getByRole("region", { name: "Verdict" }).waitFor()
      await shot(p, `now-${sc}-${w}-${scheme}`)
      await p.context().close()
    }
  }
}

await scenario("wifi_loss")
const p = await page(1440, "dark")
await p.goto(`${UI}/`)
await p.getByRole("button", { name: /^Gateway:/ }).first().click()
await p.getByRole("dialog").waitFor()
await p.waitForTimeout(800)
await shot(p, "hop-drawer-1440-dark", false)
await p.keyboard.press("Escape")

const m = await page(390, "dark")
await m.goto(`${UI}/`)
await m.getByRole("button", { name: /^Gateway:/ }).last().click()
await m.waitForTimeout(800)
await shot(m, "hop-drawer-390-dark", false)

await p.goto(`${UI}/history`)
await p.waitForSelector(".recharts-line")
await p.getByRole("radio", { name: "24h" }).or(p.getByRole("button", { name: "24h" })).first().click()
await p.waitForTimeout(1200)
await shot(p, "history-1440-dark")
await m.goto(`${UI}/history`)
await m.waitForSelector(".recharts-line")
await shot(m, "history-390-dark")

await p.goto(`${UI}/incidents`)
await p.getByRole("table").waitFor()
await shot(p, "incidents-1440-dark")
await m.goto(`${UI}/incidents`)
await shot(m, "incidents-390-dark")
await p.getByRole("link", { name: /DNS is failing/ }).first().click()
await p.waitForTimeout(300)
await p.getByRole("button", { name: "Copy report" }).waitFor()
await shot(p, "incident-detail-1440-dark")

await scenario("healthy")
await p.goto(`${UI}/speed`)
await p.getByRole("button", { name: "Run now" }).waitFor()
await shot(p, "speed-idle-1440-dark")
await p.getByRole("button", { name: "Run now" }).click()
await p.waitForTimeout(3500)
await shot(p, "speed-running-1440-dark")
const l = await page(390, "light")
await l.goto(`${UI}/speed`)
await l.waitForTimeout(1500)
await shot(l, "speed-running-390-light")

await browser.close()
