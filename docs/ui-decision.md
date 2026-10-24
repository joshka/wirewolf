# UI decision: shadcn

On 2026-10-24 Josh chose the shadcn (React + Vite) variant over HTMX + Basecoat and Topcoat.
The losing variants were deleted. Their code and notes remain in jj/git history (search for
`feat(web): HTMX + Basecoat UI variant` and `feat(web): Topcoat UI variant`). Screenshots from
the comparison live outside the repo in `artifacts/compare/`.


These are the three web UI variants built from a shared brief (`docs/ui-brief.md`, now removed; see history) against the same demo data.
Each variant's notes and screenshots are in `docs/compare/<variant>/`. To try all three in one
binary, run `cargo run --features ui-shadcn -- serve --demo` (the shadcn variant needs Node and
pnpm), then open http://127.0.0.1:7465.

Scores are 1–5, higher is better.

| | shadcn (React + Vite) | HTMX + Basecoat (maud) | Topcoat 0.10 |
|---|---|---|---|
| **Looks** | 5: the most polished. Geist font, tight hierarchy, and a "likely cause" card that pops | 4.5: very close; it uses the system font | 4.5: clean, and its "Last 15 minutes" chart panel is the best on the Now view |
| **Responsive / mobile** | 5 | 4.5 | 5 |
| **Live feel** | 5: client store plus SSE, no layout jumps | 4.5: morphed fragments; History doesn't update live | 4.5: `live!` regions are great, but there are two channels (WebSocket + SSE) and the indicator only tracks SSE |
| **Accessibility** | 5: Base UI provides focus traps and ARIA | 4: native `select`; morphing keeps focus | 3.5: the drawer is not modal and focus isn't trapped |
| **Build/tooling** | 2: Node and pnpm needed for `--all-features`, in CI, and in a Docker stage | 5: `cargo build`, nothing else | 3: Rust 1.98, and the first build downloads the Tailwind CLI |
| **Own code** | ~2,230 TS/TSX + 1,430 copied components | ~2,170 (1,490 Rust, 326 CSS, 350 JS) | ~1,600 Rust + 400 JS + 540 copied components |
| **Dependencies** | ~450 npm packages, 888 KB JS bundle | `maud` + 368 KB of vendored, pinned JS/CSS | 182 crates (vs ~75 base) + vendored runtime JS |
| **Maturity / churn** | Mature but fast-moving: Vite, Tailwind, React, and Recharts all had majors recently, and shadcn moved Radix → Base UI | htmx 2 and uPlot are stable; Basecoat is young and single-maintainer (the main risk) | Experimental: 10 breaking minors in 3 months |
| **Maintenance burden (6 months out)** | Medium-high: two ecosystems and lockfiles, and re-syncing copied components | **Low**: nothing rots by itself. Model changes are caught at compile time; Basecoat class renames are not | High: an upgrade to ~0.16 touches every view, and the runtime JS must be re-vendored by hand |
| **Languages someone must know** | Rust + TS/React/Tailwind + Node toolchain | Rust + htmx attributes + CSS + a little JS | Rust + the Topcoat DSL and its Rust-to-JS subset + Tailwind + JS |

## Recommendation: HTMX + Basecoat

It is visually within a hair of shadcn, about as good on mobile, and it is the only option
with no extra toolchain. That matches your "avoid a node build step unless it's obviously
strongest". shadcn is the prettiest, but not by enough to justify Node in every build plus
npm churn for a home network tool.

Topcoat's live-region model fits wirewolf really well. It's the one to revisit once it
stabilizes, around 1.0: when it can embed assets and has a mount prefix.

If you pick HTMX, the polish to port from the others is:
- bundle the Geist font
- the "Last 15 minutes" per-hop chart panel from Topcoat
- the "likely cause" emphasis and the number styling from shadcn
- live updates on the History view
- a modal, focus-trapped drawer

## Current build state (until you choose)

`ui-htmx` and `ui-topcoat` are on by default, and `ui-shadcn` is opt-in because it needs Node.
After the decision, the losing variants are deleted, the winner moves to `/`, and the MSRV goes
back to 1.88 unless Topcoat wins.

## Assessment after converging (2026-10-24)

This was measured on main after all three implemented a converged spec (`docs/ui-spec.md`, now replaced by `docs/ui.md`). HTMX and Topcoat had
also applied `docs/design-language.md`; shadcn's design-language pass was still pending.
"Own code" counts code lines only and excludes vendored and copied-in component code.

| | shadcn | HTMX | Topcoat |
|---|---|---|---|
| Own code | 2,420 TS/TSX + 180 CSS + 80 Rust | 1,300 Rust + 340 JS + 320 CSS | 1,650 Rust + 580 JS + 140 CSS |
| Copied component code we now own | 1,320 | 0 | 350 |
| Extra deps | ~450 npm packages, 300 MB node_modules | +4 crates | +107 crates (182 vs 75 base) |
| Rendering mechanisms | 1 (client store from REST + SSE) | 2 (server fragments over SSE; client JS charts) | 3 (live! regions over WebSocket, signals/procedures, client JS charts over SSE) |
| Fighting the framework | Little: Base UI vs Radix docs, Recharts typing | Basecoat defaults: about 1/3 of the design-language diff was overrides, class-name collisions | The most: bypassed the asset system, vendored runtime JS, root `/_topcoat` route, edited copied components (no class merge), MutationObserver to rebuild charts after live re-renders, modal built in JS |
| Duplicated logic | None (one language on the client) | Relative time in Rust and JS | Relative time in Rust and JS |
| Fidelity ceiling | Highest: accessible primitives, animation, rich client state | High for this app; fine-grained interactions cost a server round trip or hand JS | High for live data; charts and modals are hand JS |

## Code quality review (2026-10-24)

Each variant got an independent read-only review against the same rubric. All three scored
**3.5/5** overall.

| | shadcn | HTMX | Topcoat |
|---|---|---|---|
| Structure | 4: lib / components / views, max 289 lines per file | 3: `views.rs` is a 1,184-line monolith | 4: one file per page, plus a 580-line `app.js` |
| Idiom | 4: current React 19, Tailwind v4, Base UI | 4: idiomatic maud and htmx | 4: idiomatic Topcoat |
| Types and correctness | 3: unhandled fetch rejections, SSE JSON not validated | 3: reconnect doesn't resync incidents, speed, or charts; JS state bugs | 3: a dropped WebSocket goes unnoticed; grades matched on strings |
| Duplication | 3: within TS (HopCard/HopRow, formatters) | 2.5: across Rust and JS (time, format, bucketing ×3) | 2.5: across Rust and JS (hops, formatters, theme, colours) |
| Hacks | 3: minor | 3: text scraping, `!important` | 2.5: global MutationObserver, modal glue, copied components edited in place |
| Tests | 1: none | 1.5: 2 unit tests | 2: formatting and report |
| Accessibility | 4 | 4 | 4 |

The scores tie, but the problems are of different kinds. shadcn's issues are ordinary cleanups
inside one language and can be fixed in place. In HTMX and Topcoat, the Rust/JS duplication and
the second rendering path come from the architecture and can't be refactored away.

## shadcn: what the parity rounds changed

This compares `7157e5df` (the last commit before parity work) with `e427b162` (after the spec
and design-language rounds), limited to `crates/wirewolf-web/ui-shadcn/src`. Reproduce with:

```sh
jj diff --from 7157e5dfc059 --to e427b162c34a crates/wirewolf-web/ui-shadcn/src
```

The diff is +852/−645 lines across 19 files, so the **net growth is only about 200 lines**.
The "~900" figure was churn: much of the diff swaps one hand-styled thing for another.

The verdicts below apply the "shadcn defaults first" rule: **keep** it, **revert** to the
shadcn or stock version, or **rework** it the shadcn way.

## A. Layout and features (from the spec)

| # | Change | Where | Verdict |
|---|---|---|---|
| A1 | "Last 15 minutes" panel with sparkline rows; sparklines removed from hop cards | new `recent.tsx`, `path.tsx`, `now.tsx` | Keep the panel. Rework the sparklines on shadcn `Chart`, not custom SVG |
| A2 | Now grid: panel on the left (3/5), speed and recent incidents on the right (2/5) | `now.tsx` | Keep |
| A3 | Speed card restructured: big down/up, Idle/Loaded/Bufferbloat tiles, meta line (`SpeedSummary`) | `speed.tsx` | Keep the structure. Drop the pixel sizes |
| A4 | Separate speed chart on History, replacing dotted speed-test lines; the Speed page reuses it | `charts.tsx` `SpeedChart`, `history.tsx`, `views/speed.tsx` | Keep. It removed a duplicate chart, so it's net simpler |
| A5 | Incident bands factored into `incidentBands()` | `charts.tsx` | Keep (deduplication) |
| A6 | Drawer: one `HopChart` with RTT plus loss on a right-hand axis, replacing two stock `TimeChart`s | `charts.tsx` +55, `hop-sheet.tsx` | **Revert** to the two simple charts |
| A7 | Drawer: checks table gains a Status column, the Detail column is gone, and evidence loses its frame | `hop-sheet.tsx` | Keep the Status column. Otherwise stock table |
| A8 | Incidents: table on desktop and cards on mobile (two renderings), open incidents pinned and tinted | `views/incidents.tsx` | **Rework** into one responsive list (or the table only). Keep the pinning |
| A9 | Top bar with page title; page `<h1>`s removed | `App.tsx`, views | Keep. Later, consider shadcn `Sidebar` + `SidebarInset` |
| A10 | Live pill states: Live, Probes paused, Reconnecting | `chrome.tsx` | Keep |
| A11 | Theme control: a custom cycling button replacing shadcn's dropdown with radio items | `chrome.tsx` | **Revert** to the stock dropdown (the standard shadcn pattern) |
| A12 | `useDesktop()` JS media query that renders only one path layout, just so `data-hop` is unique for the screenshot script | `path.tsx` | **Revert** to CSS `hidden`/`lg:flex`. Fix the script to click `:visible` |
| A13 | Drawer returns focus to the hop that opened it (`finalFocus` ref) | `snapshot.tsx`, `hop-sheet.tsx` | Keep (accessibility, small) |
| A14 | Incident detail: start → end (duration) line | `views/incidents.tsx` | Keep |
| A15 | Recent incidents: "All incidents →" footer; Speed card loses its "Speed history" link | `now.tsx` | Keep |

## B. Visual language (from design-language.md): mostly fighting shadcn

| # | Change | Where | Verdict |
|---|---|---|---|
| B1 | Hand-rolled `Badge` with tones and a `cause` mode; `ui/badge` unused | `status.tsx` +40 | **Rework** as `cva` variants in `ui/badge.tsx` |
| B2 | Card: `border` instead of shadcn's `ring-1 ring-foreground/10`, title 15px semibold, description 13px, footer border and background removed | `ui/card.tsx` | **Revert** to the stock card |
| B3 | Table: muted 12px header, `h-11 px-3` cells, row hover removed | `ui/table.tsx` | **Revert** to the stock table |
| B4 | Theme: tuned oklch palette replaced by hex; `--radius` 0.5rem; dark neutrals overridden with hex | `index.css` | **Revert** to shadcn neutral. Set `--primary`/`--ring` to violet. Keep the status and hop tokens |
| B5 | Separate `--brand` token, plus `bg-brand text-white` overrides on buttons, logo, and nav | `index.css`, `App.tsx`, `speed.tsx` | **Rework**: brand becomes `--primary`, so the default `Button` is violet |
| B6 | Arbitrary pixel values: `text-[11/13/15/24/30/32/40px]`, `h-[3px]`, `max-w-[720px]`, `min-h-[132px]`, `size-[18px]`, `max-w-[1400px]`, `tracking-[...]` | everywhere (31 total) | **Revert** to Tailwind scale steps |
| B7 | `strokeWidth={1.75}` added to every icon | many files | **Revert** (Lucide default) |
| B8 | Icon swaps: History→ChartLine, Siren→Flag, Waypoints→Building2, BookOpenText→AtSign | `App.tsx`, `path.tsx` | Neutral. Keep whichever reads better; no cost either way |
| B9 | Logo: favicon image replaced by a violet tile with the Activity icon | `App.tsx` | Neutral (becomes `bg-primary` after B5) |
| B10 | Nav: active state is a brand tint; incident count is a solid red 18px circle; Now's severity dot removed | `App.tsx` | **Revert** to the earlier `bg-accent` active state and a stock `Badge` count |
| B11 | Verdict banner: top stripe always shown (even when healthy), `SeverityPill` in the meta line, capitalised confidence, custom tracking | `verdict.tsx` | Keep the pill. Drop the stripe on healthy and the tracking values |
| B12 | Sparkline gradient flattened to 0.15/0.15 (now a dead gradient) | `sparkline.tsx` | Goes away with A1's rework |
| B13 | Custom relative-time format replacing `Intl.RelativeTimeFormat` | `lib/format.ts` | **Revert** to Intl. It's localized and less code |
| B14 | Status label "Pass" → "OK" | `lib/format.ts` | Neutral (wording) |
| B15 | Hop card: fixed min height, blamed frame `border + 6% tint` instead of a ring, metric line foreground when passing | `path.tsx` | Keep "colour only when not passing". Use the stock card look for the rest |

## Summary

- **Keep** (structure that makes the product better): A1–A5, A7, A9, A10, A13–A15, plus the
  "calm by default" colour rule.
- **Revert to stock shadcn:** A6, A11, A12, B2, B3, B4, B6, B7, B10, B13. These are the bulk of
  the visual churn and of the "fighting the framework".
- **Rework the shadcn way:** A8, B1, B5, and the A1 sparklines.
