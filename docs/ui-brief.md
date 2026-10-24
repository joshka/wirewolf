# wirewolf UI brief

Every web UI variant builds the same thing from this brief against the demo API
(`docs/api.md`). We then compare the variants side by side and keep one. Build a real,
shippable UI, not a mockup.

## Who and where

The user is one person on their home network, opening wirewolf from a laptop or a phone,
usually because something feels wrong. The first second must answer: **is it broken, where,
and since when?** The UI must look modern and polished, and it must be responsive. This
matters more than anything else in the brief.

## Look and feel

- Dark mode first, with a light mode that follows the system plus a toggle. Use a
  shadcn-style aesthetic: neutral surfaces, subtle borders, generous spacing, one accent
  color, and status colors (pass green, warn amber, fail red, skip/NA muted) used sparingly
  and paired with icons or text, never color alone.
- The layout works from 360px phones up to wide desktops. Use a sidebar or top nav on desktop
  and a bottom or top tab bar on mobile.
- Live updates should feel live: values tick without layout jumps, sparklines append, and a
  subtle "live" indicator shows the SSE connection state. When the connection drops, say so
  and reconnect.
- Numbers use tabular figures and sensible precision (`1.4 ms`, `912 Mbps`, `0% loss`).
  Show relative times ("3 min ago") with the absolute time on hover.

## Views

1. **Now** (home)
   - **Verdict banner**: severity, headline, detail, fix hint, confidence, and "since" (the
     open incident start). It dominates when something is wrong and stays calm when
     everything is healthy.
   - **Path diagram**: the hops as a horizontal chain (vertical on mobile). Each hop shows its
     label, target, status, and key metric. The blamed hop is highlighted, and collateral
     `skip` hops are dimmed.
   - **Sparklines**: recent RTT/loss per hop from `sample` events, seeded from
     `/history?since=<now-15min>`.
   - **Latest speed test** card, with a "Run now" button and live progress.
   - **Hop detail**: clicking a hop opens a drawer or sheet with its checks table, evidence
     key/values, and a 1h RTT/loss chart.
2. **History**: RTT and loss over time per hop. Use a range picker (1h / 6h / 24h) and hop
   toggles. Overlay incident periods as shaded bands. Overlay speed-run markers or show them
   as a second chart.
3. **Incidents**: a list or timeline with severity, headline, blamed hop, start, and
   duration (or "ongoing"). The incident detail shows the evidence snapshot rendered like the
   Now view, plus **"Copy report"**: a plain-text summary suitable for pasting to an ISP.
4. **Speed**: the latest result as large numbers (down, up, idle and loaded latency,
   bufferbloat grade), "Run now" with live progress, and a history chart and table.
5. **Demo scenario switcher**: only when `meta.mode == "demo"`. A small, unobtrusive control
   (for example in the header or footer) that posts to `/api/v1/demo/scenario`. It is how we
   screenshot every state.

## Constraints

- Serve the UI from the wirewolf binary (embedded or server-rendered). The final container
  has no Node runtime, so if a Node build exists it runs at build time only.
- Mount your variant under `/ui/<variant>/` during the comparison. `/` stays the chooser page.
  The API stays at `/api/v1`.
- Avoid external CDNs at runtime. wirewolf must work on a LAN with a broken internet
  connection, which is exactly when people open it.
- Accessibility: keyboard navigable, visible focus, labels on icon buttons, and respects
  `prefers-reduced-motion`.

## Deliverables (per variant)

- The variant, working against `cargo run -- serve --demo`.
- Screenshots of the Now view at 1440px and 390px, in dark and light mode, for the
  `healthy`, `dns_down`, and `wifi_loss` scenarios. Also screenshot History, Incidents (list
  and detail), Speed (idle and running), and the hop drawer. Save them in
  `docs/compare/<variant>/`.
- `docs/compare/<variant>/notes.md` with honest notes for the scorecard:
  - What was easy and what fought you.
  - Build steps and tooling needed.
  - Lines of code (your own, excluding generated or vendored code).
  - Dependency count and maturity, and how often the main dependencies churn.
  - **Maintenance burden**: what breaks when you come back in 6 months, and how many
    languages and ecosystems someone must know.
  - Gaps against this brief.
