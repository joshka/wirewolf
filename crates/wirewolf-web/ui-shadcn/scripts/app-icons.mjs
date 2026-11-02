// Renders the desktop app's icons from the wolf mark (the same path as the sidebar logo in
// App.tsx) into crates/wirewolf-app/icons: the 1024px app icon, which `cargo tauri icon` turns
// into the sizes the bundle needs, and the menu bar status icons at 2x.
// Usage: `pnpm app-icons`, then from the repo root
// `cargo tauri icon crates/wirewolf-app/icons/icon.png -o crates/wirewolf-app/icons`.
import { mkdirSync } from "node:fs"
import { chromium } from "playwright"

const OUT = new URL("../../../wirewolf-app/icons/", import.meta.url).pathname
mkdirSync(OUT, { recursive: true })

const WOLF = "M4 3 12 9H20L28 3 26 13H30L27 22 16 30 5 22 2 13H6ZM8 14 10 19 14 17ZM24 14 18 17 22 19Z"
const wolf = (size, fill) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="${size}" height="${size}" fill="${fill}"><path fill-rule="evenodd" d="${WOLF}"/></svg>`

// macOS app icons are a rounded square on an 824px grid inside the 1024px canvas, with a soft
// shadow. The wolf is white on the sidebar's violet.
const appIcon = `<!doctype html><body style="margin:0;width:1024px;height:1024px;background:transparent">
<div style="position:absolute;left:100px;top:100px;width:824px;height:824px;border-radius:185px;
  background:linear-gradient(165deg,#a78bfa 0%,#7c3aed 55%,#4c1d95 100%);
  box-shadow:0 12px 36px rgba(0,0,0,.3), inset 0 2px 0 rgba(255,255,255,.25);
  display:flex;align-items:center;justify-content:center">${wolf(560, "#fff")}</div></body>`

// Status icons are 22pt, rendered at 2x. The healthy one is a template (black plus alpha, so
// macOS tints it); the others carry their color.
const trayIcon = (fill) =>
  `<!doctype html><body style="margin:0;width:44px;height:44px;background:transparent;display:flex;align-items:center;justify-content:center">${wolf(36, fill)}</body>`

const browser = await chromium.launch()
const render = async (html, size, file) => {
  const page = await browser.newPage({ viewport: { width: size, height: size }, deviceScaleFactor: 1 })
  await page.setContent(html)
  await page.screenshot({ path: OUT + file, omitBackground: true })
  await page.close()
  console.log(`wrote ${file}`)
}
await render(appIcon, 1024, "icon.png")
await render(trayIcon("#000"), 44, "tray.png")
await render(trayIcon("#d97706"), 44, "tray-warn.png")
await render(trayIcon("#dc2626"), 44, "tray-fail.png")
await browser.close()
