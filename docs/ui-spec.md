# wirewolf UI spec (converged)

The three variants built from `docs/ui-brief.md` turned out to be about 80% the same design,
with small differences in each region. This spec picks one answer per region, taking the best
of each. All three variants converge on it, so the comparison is about the technology, not
about design choices. Where a row says "from X", look at that variant's screenshots in
`docs/compare/X/`.

## Differences found (self-review)

| Region | shadcn | HTMX | Topcoat | Converged choice |
|---|---|---|---|---|
| Top bar | Demo pill and theme toggle at top right, no page title | No top bar; live, demo, and theme controls in the sidebar footer | Page title, Live pill, demo select, and theme toggle | **Topcoat's top bar** |
| Sidebar footer | Live pill, host, version | Status badge, live, demo select, theme | Host, version | Host and version only (status lives in the top bar) |
| Font | Geist | System | System | **Geist, bundled** in all three |
| Hop colors | Varies | Different | Different | **One fixed palette** (below). Currently all three disagree |
| Verdict banner | Colored top edge, meta line above headline, "Try this:" | Similar, "SINCE 4 s" caps label | Severity badge in meta line, "Try next:" | **shadcn structure + Topcoat's severity badge**, label "Try next:" |
| Path cards | Big metric + sparkline in card | Status text + metric + sparkline + target at bottom | Compact: target under label, colored metric, no sparkline | **Topcoat's compact card + shadcn's "LIKELY CAUSE" badge** |
| Sparklines | Inside the cards | Inside the cards | Separate "Last 15 minutes" panel | **Topcoat's separate panel** (rows: hop, area sparkline, current value) |
| Below the path | Speed + recent incidents | Speed + recent incidents | 15-minute panel + speed | **Left: 15-minute panel. Right: speed card stacked over recent incidents** |
| Speed card | 2×2 numbers + bloat row | Single row of 4 | Big down/up + 3 stat tiles + "MB used" | **Topcoat's card** |
| Drawer order | Stats, chart, checks, evidence | Stats, checks, evidence, chart | Stats, checks, evidence, chart (loss chart cut off) | **Stats, chart, checks, evidence**, with the chart visible without scrolling |
| History charts | RTT + loss; speed shown as dotted lines | RTT + loss; speed shown as ticks | RTT + loss + a speed chart; **not bucketed (noisy)** | RTT + loss + **a separate speed chart**, all **bucketed to ~300 points** |
| Mobile path | List rows with metric and status pill | Cards with sparkline | List rows with chevron | **Topcoat's list rows** |
| Mobile nav | Bottom tab bar | Bottom tab bar | Bottom tab bar | Same: bottom tab bar |

## Tokens

- **Theme:** dark first; light mode follows the system, with a toggle. shadcn neutral
  surfaces: background `#0a0a0a`, card `#141414`, border `#262626`. Light mode uses shadcn's
  neutral light.
- **Accent:** violet `#8b5cf6`. Use it for the logo, the active nav item, the primary button,
  and focus rings.
- **Status:**

  | Status | Color |
  |---|---|
  | pass | `#22c55e` |
  | warn | `#f59e0b` |
  | fail | `#ef4444` |
  | skip / n/a | the muted foreground |

  Always pair status color with an icon or text.
- **Hop palette** (charts, legend chips, and sparklines, everywhere):

  | Hop | Color |
  |---|---|
  | gateway | `#3b82f6` |
  | isp | `#f97316` |
  | internet | `#10b981` |
  | dns | `#a855f7` |
  | web | `#ec4899` |

  Host has no series.
- **Type:** Geist Sans and Geist Mono, bundled with no CDN. Use mono for targets, addresses,
  and evidence values. Numbers use tabular figures. Units render smaller and muted after the
  value (`915` `Mbps`).
- **Shape:** radius 12px for cards and 8px for controls. Cards have a 1px border and no
  shadow.

## Shell

- **Desktop (≥1024px):** a 240px sidebar with the logo and nav (Now, History, Incidents with
  an open-count badge, Speed) and a footer with the host and version. A top bar holds the page
  title on the left, and on the right the live pill, the demo scenario select (demo mode only),
  and the theme toggle.
- **Live pill:** green "Live", amber "Reconnecting…", or muted "Probes paused" while a speed
  test runs.
- **Mobile (<1024px):** the top bar shows the logo, live pill, demo select, and theme toggle,
  and a bottom tab bar holds the 4 nav items. The content is a single column.

## Now

1. **Verdict banner.** The card is tinted by severity, with a 3px top edge in the severity
   color (healthy uses pass green and a compact layout with no fix box).
   - Meta line: severity badge · "at <Hop>" · "since <relative>" · "<Confidence> confidence".
   - Headline: about 32px semibold.
   - Detail: muted.
   - Fix box: lightbulb icon, "Try next:", and the hint.
   - Footer: the method · "checked <relative>".
2. **Path:** 6 equal cards in a row, joined by thin connector lines. Each card has:
   - an icon tile at top left and a status icon at top right;
   - the label;
   - the target (mono, truncated, with the full text in a tooltip);
   - one status-colored line with the primary metric (`1.6 ms`, `100% loss`, or the skip
     reason).

   The blamed hop gets a status-colored border and a "LIKELY CAUSE" badge. Skipped hops are
   dimmed to 50%. Clicking or pressing Enter opens the drawer. On mobile, these become list
   rows (icon, label and target, status · metric, chevron).
3. **Grid below:** two columns on desktop (left 3/5, right 2/5), one on mobile.
   - **Left:** the "Last 15 minutes" panel. One row per hop (gateway to web): the label, an
     area sparkline in the hop color with a fixed wall-clock x axis, and the current value on
     the right ("no reply" in fail color).
   - **Right:** the speed card, and below it the recent incidents card.
4. **Speed card.** The header reads "Speed test" with an outline "Run now" button. The body
   shows big down and up numbers, then 3 stat tiles (Idle, Loaded, Bufferbloat grade colored
   A+/A green, B/C amber, D/F red), and a meta line: server · relative time · MB used. While
   running, the button becomes disabled with a spinner, a progress bar shows, and the numbers
   tick up.
5. **Recent incidents card.** It shows the latest 4: a severity badge, the headline
   (truncated), and below it the relative start · duration (or "ongoing" in fail color) · hop.
   An "All incidents →" link is at the bottom.

## Hop drawer

A right-hand sheet, 520px wide (full width on mobile). It is a **modal dialog with a focus
trap**: Escape or clicking the overlay closes it, and focus returns to the card that opened
it. Content, top to bottom:

1. The icon tile, label, target in mono, status badge, and a "LIKELY CAUSE" badge if blamed.
2. The summary line.
3. Three stat tiles: RTT, Jitter, Loss.
4. "Last hour": an RTT line in the hop color, with loss as a red area on a right-hand 0–100%
   axis in the same chart. Incident bands are shaded.
5. Checks table: Check (name + detail below), Status badge, RTT.
6. Evidence: key/value rows, with values in mono.

## History

- Header: title + one-line description. Controls row: a 1h/6h/24h segmented control, then
  hop toggle chips (colored dot + label).
- Three stacked chart cards:
  - **Round-trip time:** a line per hop, mean per bucket.
  - **Packet loss:** a step line, worst per bucket.
  - **Speed tests:** download and upload lines with point markers.
- Bucket all data to about 300 points per series.
- Incident periods are shaded bands, using the severity color at about 15% alpha, on all
  three charts.
- A hover tooltip shows the time and every visible series value.
- New samples append live.

## Incidents

- **List:** a table on desktop, cards on mobile. Columns: severity badge, headline, hop, start
  (relative, with the absolute time in a tooltip), and duration or "ongoing". An open
  incident is pinned to the top with a subtle severity tint.
- **Detail:** the evidence snapshot rendered with the same banner and path components as
  Now (static), plus a "Copy report" button that copies plain text, with a fallback for
  non-secure contexts. Also a time-window line: start → end (duration).

## Speed

- **Latest result:** the same layout as the Now speed card, but larger, plus a "Run now"
  primary button.
- **History:** the chart from History, plus a table of runs (time, backend, server, down, up,
  idle, loaded, grade, data used).

## Motion and accessibility

- Value changes don't jump the layout. Progress bars and sparklines animate unless
  `prefers-reduced-motion` is set.
- Everything works by keyboard, with visible focus rings in the accent color.
- Icon buttons have labels.
