# wirewolf design language

`docs/ui-spec.md` says *what* is on each page. This file says *how it looks*: spacing, frames,
type, color use, badges, and icons. After the three variants converged on the spec, the
remaining differences were almost all at this level. These rules are the tie-breakers, and
every variant follows them exactly.

Compare variants with `docs/compare/shots.cjs`. It takes identical screenshots of every
variant against one demo server.

## Principles

1. **Calm by default, loud only when something's wrong.** A healthy page should be almost
   monochrome. Color is information. If everything is green, nothing stands out.
2. **One frame per idea.** Don't nest bordered boxes more than one level deep inside a card.
   Stat tiles inside a card are the only allowed nested frame.
3. **Numbers are the content.** Values are bigger and brighter than their labels. Units are
   smaller and muted.

## Spacing

Use a 4px base. Only these values: 4, 8, 12, 16, 20, 24, 32.

| Use | Value |
|---|---|
| Page padding (desktop / mobile) | 24 / 16 |
| Gap between cards and sections | 16 |
| Card padding | 20 (16 on mobile) |
| Card title to content | 16 |
| Within a group (label to value, icon to text) | 8 |
| Tight inline (badge to text, meta items) | 4–8 |
| Stat tile padding | 12 |
| Table cell padding | 12 horizontal, row height 44 |

## Frames

- **Card:** 1px `border` (`#262626` dark, `#e5e5e5` light), background `card`, radius 12, and
  no shadow. Cards never have a colored background, except the verdict banner and the blamed
  hop.
- **Verdict banner:** the card plus a 10% tint of the severity color, a 1px border at 40% of
  the severity color, and a 3px top edge in the severity color. The healthy banner has no tint
  and a green top edge only.
- **Blamed hop card:** a 1px border in the severity color plus a 6% tint. No glow and no
  thicker border.
- **Stat tile:** 1px border, transparent background, radius 8.
- **Fix box** ("Try next"): 1px border and a `background` fill (darker than the card), radius
  8, width up to 720px.
- **Tables:** inside a card with no outer border of their own. Rows are separated by 1px
  borders, with no zebra striping. The header row is 12px muted text, not bold.
- **Drawer:** full height with a 1px left border. Its sections are separated by a 24px gap,
  not by borders.

## Type

Geist Sans, plus Geist Mono for targets, addresses, and evidence values.

| Role | Size / weight | Color |
|---|---|---|
| Page title (top bar) | 16 / 600 | foreground |
| Verdict headline | 30 / 600, tracking −0.02em (24 on mobile) | foreground |
| Card title | 15 / 600 | foreground |
| Card subtitle | 13 / 400 | muted |
| Big number (speed) | 40 / 600, tabular | foreground |
| Stat value | 18 / 600, tabular | foreground |
| Body / list item | 14 / 400 (headline-like items 500) | foreground |
| Label (stat label, table header, meta) | 12 / 400 | muted |
| Unit | 0.6 of the value size / 400 | muted |
| Mono (target) | 12 / 400 | muted |

Avoid all-caps, except for the `LIKELY CAUSE` badge.

## Color use

- **Status color is only for non-pass states.** A passing hop's metric is shown in
  `foreground`, not green. Green appears in only three places: the pass status icon, the live
  dot, and A+/A grades. Warn and fail use their color on the icon, the metric line, and the
  border (blamed hop only).
- **Accent violet** is used for the logo tile, the active nav item (icon + 10% tint
  background), focus rings, and **one** primary button per page. That's "Run now" on the
  Speed page. On Now, "Run now" is an outline button.
- **Hop palette** (see the spec) is only for chart series and the legend dots, never for
  text.
- **Muted text** is about 60% of the foreground. Don't stack two muted levels.

## Badges

There is one badge component in three tones.

- **Shape:** 20px tall, radius 6, horizontal padding 6, 12/500 text, and an optional 12px icon
  with a 4px gap.
- **Status tone:** 12% tinted background plus 1px border at 30% of the status color, with
  text in the status color. Used for Degraded, Down, Warning, Failing, OK (OK rarely appears,
  because of the calm-by-default rule), and grades.
- **Cause tone:** the `LIKELY CAUSE` badge. 11/600 uppercase with 0.04em tracking, in the
  status tone. It is always on its own line under the hop label, both in cards and in mobile
  rows.
- **Count tone:** the nav incident count. A solid fail-red circle, 18px, white 11/600 text.
  Shown only when an incident is open.

## Icons

Use [Lucide](https://lucide.dev) everywhere, 16px with a 1.75 stroke in text, 20px inside icon
tiles. The icon names are fixed:

| Thing | Icon |
|---|---|
| Logo | `activity` in a violet rounded tile (until a wolf mark exists) |
| Nav: Now / History / Incidents / Speed | `activity` / `chart-line` / `flag` / `gauge` |
| Hop: host / gateway / isp / internet / dns / web | `laptop` / `router` / `building-2` / `globe` / `at-sign` / `app-window` |
| Status: pass / warn / fail / skip | `circle-check` / `triangle-alert` / `circle-x` / `circle-minus` |
| Fix hint | `lightbulb` |
| Run now | `play` |
| Download / upload | `arrow-down` / `arrow-up` |
| Theme | `sun` / `moon` / `monitor` (shows the current mode; click cycles) |
| Drawer close | `x` |
| Row affordance (mobile) | `chevron-right` |

- **Icon tiles** (hop cards, banner, drawer header): 36px square, radius 8, `muted`
  background, 20px icon in muted foreground. The banner tile is 44px and uses the severity
  color.
- A status icon appears **once** per hop: at the top right of the card, or in the metric line
  on mobile. Never next to the label as well.

## Component specifics

- **Hop card** (desktop), with a fixed 132px height so the row aligns. The rows inside are:
  1. the icon tile and status icon;
  2. the label, with the `LIKELY CAUSE` badge on its own line below when blamed;
  3. the target (mono, truncated);
  4. the metric line, 14/500 (foreground when passing, status color otherwise).
- **Mobile hop row:** a 56px minimum height. The icon tile sits on the left; next to it the
  label (and the badge line when blamed), then the target, then the metric line (status icon +
  metric); a chevron is on the right. No separate status text like "OK ·".
- **Sparkline row:** the label is 14/400 and has no dot. The sparkline is 32px tall, with a
  1.5px line and a 15% area fill. The value on the right is 13/400 tabular (status color only
  when not passing), and loss is appended as "· 13% loss" when it's above 0.
- **Meta lines** (banner, speed, incidents): items are separated by ` · ` (a middle dot with
  spaces). Don't mix separators.
- **Relative times:** "just now", "38s ago", "3 min ago", "2 h ago", "20 h ago", or "Oct 21"
  beyond 48h. Use the same format everywhere, with the absolute time in a `title`.
- **Buttons:** 32px tall (36 on mobile), radius 8, 13/500 text. The outline button uses the
  card border.
- **Top bar:** 56px tall with a bottom border. The demo select is 32px tall with the label
  "Demo: <scenario>". The live pill is the count badge's shape at 24px tall, with a 6px dot.
- **Sidebar:** 240px wide, with nav items 36px tall at radius 8, 14/500 text, and a 16px icon.
  The footer shows the host in mono and the version on the next line, 12px muted.
