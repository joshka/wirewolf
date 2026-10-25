# wirewolf dashboard

The dashboard is a React + Vite + shadcn/ui app in `crates/wirewolf-web/ui-shadcn`, embedded in
the binary and served from `/`. The data it reads is in `docs/api.md`.

## Who it's for

One person on their home network, opening wirewolf from a laptop or a phone, usually because
something feels wrong. The first second must answer: **is it broken, where, and since
when?** It works on a LAN whose internet is down, so nothing loads from a CDN.

## Pages

| Route | Page |
|---|---|
| `/` | **Now**: the verdict banner, then the path (rows with wide sparklines, or six cards with small ones; the choice is remembered), then speed test and recent incidents side by side |
| `/path/<hop>` | **Hop**: status, the stats and charts the hop actually measures (1h/6h/24h), checks, evidence, and incidents that blamed it. User targets (`target:<slug>`) use the same page |
| `/incidents` | **Incidents**: open incidents first, then newest |
| `/incidents/<id>` | **Incident**: the evidence snapshot rendered like Now, plus a copyable ISP report |
| `/incidents/<id>/path/<hop>` | **Hop, as it was** during that incident |
| `/history` | **History**: RTT, loss, and speed charts with incident bands, by range and hop |
| `/speed` | **Speed**: latest result, run now, and history |
| `/logs`, `/logs/server` | **Logs**: the activity timeline, and the server's recent log lines with level, search, and the live log filter (writers only) |
| `/settings` | **Settings**: the effective config and where each value came from, editable when the server allows writes |

The sidebar holds the logo, the theme and collapse toggles, the live state and host, the
navigation (including the path's hops), and, in demo mode, the scenario switcher. On phones
it opens as a sheet from a slim top bar.

## Visual style

One thing gets attention at a time; everything else is quiet. Design at laptop width
(1120–1440px) first, then check the phone.

- **Surfaces.** One flat page: no cards around sections, no boxes inside boxes. Hairlines
  (1px) go between rows, never around the outside. Sections are separated by whitespace and a
  bold ~16px heading with a muted subtitle on the same line. On phones a contained card is
  fine.
- **Page header.** A title, a muted subtitle under it, a segmented time chooser on the right
  (only the selected item raised), and a hairline under it.
- **Colour means status.** There's one status colour per page: every coloured mark uses the
  verdict's colour, so a red verdict never has an amber cause. No tint over a whole section,
  and no pills in a different hue. The accent stays out of content.
- **Verdict.** A faint band of the status colour (the status tint) runs edge to edge behind
  only the status line and the headline. It's there in every state, green when healthy. The
  status word is in the status colour and its metadata is muted. The headline is 26px at
  weight 650 with slight negative tracking. The lead is 15px, at most ~68 characters per line.
- **The cause row.** It gets the same faint tint, an outlined pill ("CAUSE") with a 1px border
  and text in the status colour, and a title one step bolder (650 vs 500). No side bars,
  solid fills, or filled pills.
- **Type and density.** Body text is 14px. Secondary text is 12.5–13px and muted.
  Identifiers are 12px mono in the dimmest text colour. Rows get 10px vertical padding.
  Hierarchy comes from the muted and dim greys, not from extra sizes or boxes.
- **Actions.** One neutral primary button (light fill, dark text, in dark mode) for the most
  useful next step, which has to suit the state. Every other action is a muted text link. No
  rows of equal bordered buttons.
- **Check lists** (Explain and similar) have two columns: the check (the question in bold, its
  target in mono under it) | what it saw, with what that tells you muted underneath. No
  uppercase column headers. When everything passes, collapse the list to "All N checks
  passed · Show checks". Skipped checks get a hollow dot and muted text.
- **Layout.** Before building, state the rules: section rows, row rhythm, control sizing. Rows
  fill the width, with no lopsided whitespace.

## Tokens

- **Accent:** violet, for focus rings, selection, and navigation. The primary button is
  neutral (see Visual style).
- **Status:** pass `#22c55e`, warn `#f59e0b`, fail `#ef4444`, skip and n/a in muted
  foreground. Always pair status color with an icon or a word.
- **Status tint:** the status colour at 10% alpha (`bg-fail/10`, `bg-warn/10`,
  `bg-pass/10`), for the verdict band and the cause row.
- **Hop palette** (chart series and legend dots only):

  | Hop | Color |
  |---|---|
  | gateway | `#3b82f6` |
  | isp | `#f97316` |
  | internet | `#10b981` |
  | dns | `#a855f7` |
  | web | `#ec4899` |

- **Type:** Geist Sans and Geist Mono, bundled. Mono is for targets, addresses, and
  evidence values. Numbers use tabular figures.

## Developing

```sh
cargo run -- serve --demo --listen 127.0.0.1:7471 &   # API with scripted scenarios
cd crates/wirewolf-web/ui-shadcn && pnpm install && pnpm dev   # hot reload, proxies /api
pnpm test                                            # vitest
```
