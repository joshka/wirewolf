# wirewolf dashboard

The dashboard is a React + Vite + shadcn/ui app in `crates/wirewolf-web/ui-shadcn`, embedded in
the binary and served from `/`. How it looks is in `docs/design-language.md`. The data it
reads is in `docs/api.md`. Why it's shadcn is in `docs/ui-decision.md`.

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
| `/settings` | **Settings**: the effective config and where each value came from, editable when the server allows writes |

The sidebar holds the logo, the theme and collapse toggles, the live state and host, the
navigation (including the path's hops), and, in demo mode, the scenario switcher. On phones
it opens as a sheet from a slim top bar.

## Tokens

- **Accent:** violet, as shadcn `--primary`.
- **Status:** pass `#22c55e`, warn `#f59e0b`, fail `#ef4444`, skip and n/a in muted
  foreground. Always pair status color with an icon or a word.
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
