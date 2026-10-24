# ui-shadcn: React + Vite + Tailwind v4 + shadcn/ui

Mounted at `/ui/shadcn/` behind cargo feature `ui-shadcn`. Vite builds a static SPA at
compile time (`crates/wirewolf-web/build.rs`), `rust-embed` puts it in the binary, and an
`index.html` fallback makes deep links like `/ui/shadcn/incidents/inc-past-3` reload correctly.
There's no Node at runtime and no CDN; fonts (Geist) are bundled.

```sh
cargo run --features ui-shadcn -- serve --demo     # needs node + pnpm on PATH
# UI dev loop with hot reload, proxying /api to a demo server on :7471:
cargo run -- serve --demo --listen 127.0.0.1:7471 &
cd crates/wirewolf-web/ui-shadcn && pnpm install && pnpm dev
pnpm screenshots                                   # regenerates everything in this folder
```

## Screenshots

| | |
|---|---|
| Now, healthy / dns_down / wifi_loss, 1440 + 390, dark + light | `now-<scenario>-<width>-<scheme>.png` (12 files) |
| Hop drawer | `hop-drawer-1440-dark.png`, `hop-drawer-390-dark.png` |
| History (24h, incident bands, speed markers) | `history-1440-dark.png`, `history-390-dark.png` |
| Incidents list and detail | `incidents-*.png`, `incident-detail-1440-dark.png` |
| Speed, idle and running | `speed-idle-1440-dark.png`, `speed-running-1440-dark.png`, `speed-running-390-light.png` |

## Choices

- **React over Svelte**: shadcn/ui's canonical implementation is React. The coordinator chose
  this.
- **Charts: Recharts, through the shadcn `chart` component.** It's the shadcn default, so it
  themes with the same CSS variables, and the tooltip and legend match the design system for
  free. It also supports `ReferenceArea` and `ReferenceLine` for incident bands and speed
  markers. The cost is bundle size, since Recharts is most of the ~270 KB of gzipped JS. uPlot
  would be about 10× smaller and faster on 24h data, but it isn't themed and would need its
  own tooltip and legend. For the live sparklines on the Now view I skipped both libraries and
  used a ~60-line SVG component, which is cheap to redraw every 2 seconds. History data is
  bucketed client-side to about 300 points.
- **No router library, no state library.** A 70-line History API router and a 120-line
  `useSyncExternalStore` store fed by one `EventSource` cover the app's needs. The store
  retries SSE itself with backoff, because the browser gives up after some errors.
- shadcn's current default style (`base-nova`) is built on **Base UI**, not Radix. It uses the
  `render` prop instead of `asChild`, and most examples online are still written for Radix.

## What was easy

- Making it look good. The shadcn defaults (spacing, borders, Geist, the dark palette) got
  most of the way to "modern and polished" with no design work. I added one accent (violet),
  status tokens (`pass`/`warn`/`fail`/`skip` as Tailwind colors), and per-hop chart colors.
- Responsive layout with Tailwind breakpoints: a sidebar from `lg` up and a bottom tab bar
  below it. The path is a horizontal chain from `xl` up and a vertical list below.
- The Sheet, DropdownMenu, Tooltip, ToggleGroup, and Sonner components gave focus trapping,
  Escape to close, keyboard menus, and ARIA for free.
- Live updates. React re-renders keyed hop cards in place, numbers use `tabular-nums`, and
  nothing jumps.

## What fought me

- Generated component code didn't pass the template's own `tsc` strictness
  (`noUnusedLocals` on `scroll-area.tsx`). I deleted the components I didn't use.
- The `shadcn` CLI package ended up as a **runtime dependency** because `index.css` imports
  `shadcn/tailwind.css`. It's a big package to keep for one CSS file.
- Base UI vs. Radix API differences, as above. The docs and LLM training data skew toward
  Radix.
- Recharts typing for custom tooltip formatters is fiddly, and axis tick text wraps unless you
  pick widths by hand.
- Clipboard: `navigator.clipboard` needs a secure context, but wirewolf is usually opened over
  plain HTTP on a LAN IP. "Copy report" falls back to `execCommand("copy")`.

## Build and tooling

- Build time: Node ≥ 20 and pnpm. `build.rs` runs `pnpm install --frozen-lockfile` (if
  `node_modules` is missing) and then `pnpm build`, which takes about 3 seconds. Without the
  feature it does nothing, so plain `cargo build` never touches Node.
  `WIREWOLF_UI_SHADCN_PREBUILT=1` embeds an existing `dist/`, for a container stage that
  builds the UI separately.
- `cargo build --all-features` now needs Node and pnpm. Every workstream and CI will hit this.
- Debug builds of `rust-embed` read `dist/` from disk, so running `vite build --watch` with
  the server shows changes on reload.
- Runtime: nothing beyond the binary. Assets are fingerprinted and served with
  `immutable` caching. The JS is 888 KB raw / 269 KB gzipped (one chunk), and the CSS is 74 KB
  raw / 13 KB gzipped.

## Size

- Own code: about 2,230 lines of TS/TSX in `src/` (excluding `components/ui`), plus 190 lines
  of CSS, 96 lines of Rust (`build.rs` and `ui_shadcn.rs`), and a 94-line screenshot script.
- Generated or vendored shadcn components: about 1,430 lines in `src/components/ui/`. These
  are copied into the repo and owned by us, not imported.

## Dependencies

- Direct: 14 runtime and 8 dev (`react`, `react-dom`, `@base-ui/react`, `recharts`,
  `tailwindcss`, `@tailwindcss/vite`, `lucide-react`, `next-themes`, `sonner`, `cn`,
  `class-variance-authority`, `tw-animate-css`, the Geist fonts, `shadcn`; plus `vite`,
  `typescript`, `@vitejs/plugin-react`, `oxlint`, `playwright`, and `@types/*`).
- About 450 packages in the full tree. `node_modules` is about 300 MB, mostly Playwright and
  the shadcn CLI.
- Churn is high. Vite (now 8), Tailwind (v4 changed config completely), React (19), and
  Recharts (3) each had a major release in the last 1–2 years. shadcn moved from Radix to
  Base UI and changed its CSS setup between releases. `lucide-react` releases weekly.
  Everything is mature and widely used; it's just fast-moving.

## Maintenance burden

- **Languages and ecosystems: Rust plus TypeScript, React, Tailwind, and the Node/pnpm
  toolchain.** That's two ecosystems, with two lockfiles and two sets of dependency updates.
- **In 6 months**: expect `pnpm install` to work (lockfile pinned) but `pnpm outdated` to show
  majors, likely in Vite, TypeScript, and possibly Recharts or Base UI. Updating shadcn
  components means re-running the CLI and diffing over our local edits, because the code was
  copied into the repo. Node LTS will move. Nothing breaks at runtime, since the output is
  static files in the binary.
- **Contributor friction**: everyone who runs `--all-features` needs Node. CI needs a Node
  setup step and a pnpm cache. The container needs a Node build stage (or the `PREBUILT`
  path).
- Upside: the most familiar stack of the three variants. There's a huge pool of examples, and
  any frontend developer (or LLM) can work on it immediately.

## Gaps against the brief

- None of the deliverables are missing. Known rough edges:
  - The History speed markers are dotted reference lines rather than a second chart. The
    Speed view has the throughput chart.
  - The incident detail doesn't chart the hop's history around the incident window. The hop
    drawer in incident detail does show the hour around the evidence snapshot.
  - Full-page screenshots at 390px show the fixed bottom tab bar part-way down the page. It's
    pinned to the viewport in a real browser.
  - In the screenshots the desktop sidebar stops at the viewport height because it's sticky,
    which is also fine in a real browser.
- Single 888 KB JS chunk. Route-level code splitting (lazy-load Recharts) would bring the
  Now view's first load down a lot, and is a small change.

## Against live probes

Checked against real probes (`serve` without `--demo`) with no UI changes needed for the data
shapes. See `live-now-*.png`, `live-hop-drawer-dns-1440-dark.png`, and
`live-speed-{running,done}-1440-dark.png`. The scenario switcher hides itself in live mode.
Sparse-data fixes: sparklines wait for 3 points (live samples every 30s), and the speed chart
shows an empty state with no runs and dots with one or two. The live indicator says "probes
paused" while a speed test runs.

## Converging to the spec

Brought in line with `docs/ui-spec.md`. The screenshots above were retaken with the same
filenames. The change touched 16 files under `src/`, with **+640 / −465 lines**; I didn't
edit any vendored `components/ui` file. It took about an hour, and most of that was layout
rather than fighting the stack.

**Easy, because it's mostly class names and moving JSX:**
- Tokens. The fixed hop palette, accent, status colors, and radius became about 30 lines of
  CSS variables. Tailwind picked them up as utilities (`text-pass`, `bg-hop-dns`).
- The top bar with page title and live pill, the sidebar footer, and the verdict meta line
  with the severity badge and "Try next:".
- The compact path cards (deleting code), the 3/5 + 2/5 grid, the "Last 15 minutes" panel
  (reusing the existing SVG sparkline), and the speed card's tiles.
- Moving the incidents list from a timeline to a table plus cards, pinning open incidents,
  and adding the speed table columns.
- Focus return from the drawer. Base UI's `finalFocus` takes a ref, so it was one prop.

**Harder:**
- The drawer chart with RTT and loss on two y axes in one chart. This needed a Recharts
  `ComposedChart` with `yAxisId` on every series and on the incident bands. Getting the
  y-axis tick widths and the tooltip formatting per series right took trial and error.
- When I replaced the token block, I lost the `--chart-1/2` overrides and the speed chart
  quietly fell back to shadcn's grey palette. That's easy to miss with CSS-variable theming.
- A 390px horizontal overflow from the new grid. The fix was `min-w-0` on the grid children,
  a classic CSS grid trap. I added a check script that reports `scrollWidth` per route.

**Deviations from the spec:**
- In light mode the status colors are one shade darker (`#16a34a`, `#d97706`, `#dc2626`).
  The spec's amber and green on white fail text contrast.
- Page titles live in the top bar only. The views keep their one-line descriptions without a
  second `h1`.
- The demo control is a dropdown menu with descriptions, not a native `<select>`.
- Live mode: the 15-minute panel shows "collecting samples…" until a hop has 3 points,
  because live probes sample every 30 seconds.

**Maintenance signal:** design changes in this stack are cheap and local: classes and JSX in
the views, CSS variables for tokens. The costly parts are Recharts' API surface (multi-axis
charts) and CSS layout subtleties, not React or shadcn. Verified with the demo screenshots,
plus live probes on my own port (Now, the DNS hop drawer, and a real Cloudflare speed test),
with no console errors and no horizontal overflow at 390px on any route.

## Applying the design language

Followed `docs/dev/design-language.md`. The change touched 15 files, **+265 / −232** lines. This
time it included the vendored `components/ui` files, because the defaults themselves had to
change:
- `card.tsx`: a border instead of a ring, 20px padding (16 on mobile), a 16px title gap, a
  15/600 title, and a 13px subtitle.
- `table.tsx`: 12px muted header, 12px cell padding, 44px rows, and no hover tint.

That's the shadcn way: you own the component code. It also means future `shadcn add` updates
to those files will conflict with these edits.

Other changes:
- One `Badge` component in status, cause, and count tones replaced the ad hoc pills.
- `LIKELY CAUSE` now sits on its own line, and the status icon appears once per hop.
- Lucide icons now match the fixed table: `activity` logo tile, `chart-line`, `flag`,
  `building-2`, and `at-sign`.
- Passing metrics and the live pill are plain foreground; green only shows on icons, the live
  dot, and A grades.
- The nav count is a solid 18px red circle.
- The theme button shows the current mode and cycles on click.
- The demo control reads "Demo: <scenario>", and the relative-time format is unified.
- The path renders only the layout that matches the viewport (`matchMedia`), rather than both
  layouts hidden with CSS, so `[data-hop]` selectors are unique.

Awkward in React/shadcn:
- The duplicated desktop/mobile path markup (Tailwind's usual `hidden lg:flex` pattern) broke
  the shared `data-hop` selector. It needed a `useSyncExternalStore` media-query hook.
- The spacing scale had to be enforced by hand. Tailwind's scale has 2px steps and shadcn's
  defaults use them (`gap-1.5`, `py-0.5`), so "4px base only" needs review rather than
  tooling.
- The spec's "unit = 0.6 of the value" makes `Mbps` 24px next to a 40px number, which looks
  heavy. I followed it anyway.

## De-tweak: shadcn defaults first

This pass followed the "shadcn defaults first" rule in `docs/dev/design-language.md`, with **less
own code** as the main goal. The before/after Now screenshots are in `detweak/`
(`before-now-{desk,mob,desk-light}.png` vs `after-now-*.png`).

**Own code** (everything outside `components/ui`, `hooks`, and tests):

| | Before | After |
|---|---|---|
| TS/TSX | 2,441 lines | **1,992** lines |
| Own CSS (on top of the generated shadcn theme) | ~55 lines, including hex overrides of the neutral palette | **39** lines: violet primary, status, and hop tokens |
| Arbitrary Tailwind values (`text-[13px]`, `min-h-[132px]`, …) | 23 in own code + 2 in edited `ui/` | **0** |

- Diff in own code: **+745 / −1,091**. That includes +102 lines of new tests and 58 lines
  moved into `lib/` for testability.
- Diff in `components/ui`: **+726 / −3**, almost all the stock `sidebar.tsx` added by the
  shadcn CLI, plus the badge variants.

**Swapped for stock shadcn:**
- The `Sidebar` (collapsible to icons; a sheet on mobile) replaces the hand-rolled sidebar
  and the bottom tab bar, which drew the nav twice.
- `ui/badge` `cva` variants (pass, warn, fail, muted, count, plus `caps` for LIKELY CAUSE)
  replace the hand-rolled badge.
- Chart (Recharts area) sparklines replace the custom SVG sparkline.
- `ToggleGroup` handles the hop toggles as well as the range picker.
- Violet is `--primary`, `--ring`, and `--sidebar-primary`, so the default `Button` is the
  primary button and `--brand` is gone.

**Simplified:**
- One responsive hop component: a row on mobile, a card from `lg` up. It replaces
  `HopCard`, `HopRow`, the `matchMedia` hook, and the connectors.
- One responsive incidents table (columns hide on small screens) replaces the desktop
  table plus mobile cards.
- Shared chart axis, tooltip, and incident-band helpers.

**Review cleanup:**
- History and the hop drawer show errors, and refresh errors surface in a banner instead of
  being swallowed.
- SSE payloads are shape-checked, and malformed events are dropped with a console warning.
- Hop colors are keyed by kind, and `DEFAULT_HOPS` is gone.
- Live regions now announce only stable text: the verdict headline and the live-pill label,
  not the ticking timestamps.
- `build.rs` reinstalls when `pnpm-lock.yaml` is newer than `node_modules`.
- `pnpm test` runs 8 vitest tests over bucketing, formatting, the router, and the
  copy-report text.

**Spec details dropped** (they cost custom code for pixel parity):
- The mobile bottom tab bar. Mobile now opens the sidebar from the header trigger, and the
  open-incident count shows in the sidebar.
- The connector lines between hop cards.
- Red loss ticks on sparklines. Loss is still in the value text ("· 13% loss").
- The guessed phase labels on the speed progress bar. It shows the percentage only.
- Tooltips on relative times. A `title` attribute shows the absolute time instead.
- The 4 summary tiles on the incident detail page. Their facts are in the time-window line.
- Separate mobile incident cards.
- Exact pixel sizes, which are now Tailwind steps. The card radius is shadcn's default
  (about 14px rather than 12), units are `text-2xl` next to `text-4xl` numbers, and the
  drawer is `max-w-lg` (512px rather than 520).
