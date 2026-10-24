import { Fragment, useEffect, useMemo, useState, type ReactNode } from "react"
import { Lock, Pencil, Plus, RotateCcw, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { cn } from "@/lib/utils"
import {
  api,
  type Config,
  type ConfigSource,
  type ConfigView,
  type FieldError,
  type Limits,
  type UserTargetConfig,
} from "@/lib/api"
import { blockNavigation, navigate } from "@/lib/router"
import { signIn, useSession, writeBlocked } from "@/lib/session"

/** Everything one setting control needs: its value, whether it can change, and its error. */
interface Ctx {
  draft: Config
  view: ConfigView
  /** Built-in values, to mark and reset what's changed. Null on servers before API 1.4. */
  defaults: Config | null
  /** Show only settings that differ from the default or have unsaved edits. */
  changedOnly: boolean
  canEdit: boolean
  errors: Record<string, string>
  set: (key: string, value: unknown) => void
}

// Layout (see docs/design-language.md, flat sections):
// - Each section is one full-width row: what it controls on the left, its fields on the right.
//   Rows always fill the width, so sections line up and leave no holes.
// - Every field is the same 56px row: label and hint left, control right, hairlines between.
//   A control's width says what goes in it: a switch, a short value with its unit, a choice, or
//   a long URL.
// - A setting that differs from its default gets a dot before its label, "Default …" in its hint,
//   and a reset button. An unsaved edit says "Unsaved · was …" instead. Section titles count
//   their changes, and "Changed" hides everything still at its default.
// - Save lives in a bar that appears only with unsaved changes and stays in view while scrolling.

/** The effective config, editable when the server and session allow it. */
export function SettingsView() {
  const [view, setView] = useState<ConfigView | null | undefined>(undefined)
  const session = useSession()
  const [draft, setDraft] = useState<Config | null>(null)
  const [errors, setErrors] = useState<FieldError[]>([])
  const [saving, setSaving] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [changedOnly, setChangedOnly] = useState(false)
  useEffect(() => {
    api.config().then(
      (v) => {
        setView(v)
        setDraft(v ? structuredClone(v.config) : null)
      },
      (e: Error) => setLoadError(e.message),
    )
  }, [])
  const dirty = useMemo(() => !!view && !!draft && JSON.stringify(view.config) !== JSON.stringify(draft), [view, draft])
  /** Where the user tried to go while there were unsaved changes. */
  const [leavingTo, setLeavingTo] = useState<string | null>(null)
  useEffect(() => {
    if (!dirty) return
    blockNavigation((to) => {
      setLeavingTo(to)
      return true
    })
    // Reloads, closing the tab, and typed URLs only allow the browser's own prompt.
    const unload = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      e.returnValue = "" // older Chromium and Safari only prompt when this is set
    }
    window.addEventListener("beforeunload", unload)
    return () => {
      blockNavigation(null)
      window.removeEventListener("beforeunload", unload)
    }
  }, [dirty])
  if (loadError) return <p className="text-sm text-fail">Couldn't load the config: {loadError}</p>
  if (view === undefined || (view && !draft)) return <Skeleton className="h-96 w-full rounded-xl" />
  if (view === null || !draft) return <p className="text-muted-foreground">This server doesn't report its configuration.</p>

  const blocked = writeBlocked(session, "change settings")
  const canEdit = !!view.editable && !blocked
  const byKey = Object.fromEntries(errors.filter((e) => e.key).map((e) => [e.key, e.message]))
  const general = errors.filter((e) => !e.key || !hasControl(e.key))
  const ctx: Ctx = {
    draft,
    view,
    defaults: view.defaults ?? null,
    changedOnly,
    canEdit,
    errors: byKey,
    set: (key, value) => {
      const next = structuredClone(draft)
      setPath(next, key, value)
      setDraft(next)
      setErrors((es) => es.filter((e) => e.key !== key))
    },
  }
  const save = async (): Promise<boolean> => {
    setSaving(true)
    const res = await api.saveConfig(draft, session?.csrf).catch((e: Error) => ({ ok: false as const, status: 0, errors: [{ key: "", message: e.message }] }))
    setSaving(false)
    if (res.ok) {
      setView(res.view)
      setDraft(structuredClone(res.view.config))
      setErrors([])
      toast.success("Settings saved", { description: "Probes use them from the next cycle." })
      return true
    }
    setErrors(res.errors)
    toast.error(res.errors.length === 1 ? "One setting needs fixing" : `${res.errors.length} settings need fixing`)
    return false
  }
  const leave = () => {
    const to = leavingTo
    setLeavingTo(null)
    if (to) navigate(to, { force: true })
  }
  const changedKeys = ctx.defaults ? ALL_KEYS.filter((k) => differs(draft, ctx.defaults, k)) : []
  const unsaved = ALL_KEYS.filter((k) => differs(draft, view.config, k)).length

  return (
    <div className="flex flex-col">
      <header className="flex flex-col gap-1 pb-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
          {ctx.defaults && (
            <ToggleGroup
              variant="outline"
              size="sm"
              value={[changedOnly ? "changed" : "all"]}
              onValueChange={(v: string[]) => v[0] && setChangedOnly(v[0] === "changed")}
              aria-label="Which settings to show"
            >
              <ToggleGroupItem value="all">All</ToggleGroupItem>
              <ToggleGroupItem value="changed">Changed ({changedKeys.length})</ToggleGroupItem>
            </ToggleGroup>
          )}
        </div>
        <p className="text-sm text-muted-foreground">
          {view.path ? <code className="font-mono">{view.path}</code> : "No config file yet"}
          {ctx.defaults && (
            <>
              {" · "}
              {changedKeys.length === 0 ? "everything is at its default" : `${changedKeys.length} ${changedKeys.length === 1 ? "setting differs" : "settings differ"} from the default`}
            </>
          )}
        </p>
        <p className="text-sm text-muted-foreground">
          {canEdit ? (
            "Changes are written to the file, keeping its comments, and apply from the next probe cycle."
          ) : blocked && view.editable ? (
            <>
              {blocked}.{" "}
              {session?.login_url && (
                <Button variant="link" className="h-auto p-0" onClick={() => signIn(session.login_url!)}>
                  Sign in
                </Button>
              )}
            </>
          ) : (
            "Read-only on this server. Edit the file and restart to change these."
          )}
        </p>
      </header>
      {general.length > 0 && (
        <div role="alert" className="mb-6 flex flex-col gap-1 text-sm text-destructive">
          {general.map((e, i) => (
            <p key={i}>
              {e.key && <code className="font-mono">{e.key}: </code>}
              {e.message}
            </p>
          ))}
        </div>
      )}

      <Section ctx={ctx} keys={["targets"]} title="Targets" description="Your own things to watch. A failing critical target degrades the verdict and can open an incident.">
        <TargetsTable ctx={ctx} />
      </Section>
      <Section ctx={ctx} keys={KEYS.probe} title="Probing" description="How often the path is probed, and which extras run.">
        <Rows>
          <Field ctx={ctx} k="probe.interval" label="Interval" hint="Time between cycles, at least 5s">
            <Text ctx={ctx} k="probe.interval" width="short" placeholder="30s" />
          </Field>
          <Field ctx={ctx} k="probe.ipv6" label="IPv6" hint="Auto tests IPv6 when this host has a global address">
            <Choice ctx={ctx} k="probe.ipv6" options={{ auto: "Auto", on: "On", off: "Off" }} />
          </Field>
          <Field ctx={ctx} k="probe.path_mtu" label="Path MTU" hint="Don't-fragment pings toward the internet; needs ICMP">
            <Toggle ctx={ctx} k="probe.path_mtu" />
          </Field>
        </Rows>
      </Section>
      <Section ctx={ctx} keys={KEYS.incidents} title="Incidents" description="How many cycles in a row it takes to open or close an incident. Higher values ignore short blips.">
        <Rows>
          <Field ctx={ctx} k="incidents.open_after" label="Open after" hint="Unhealthy cycles in a row">
            <Num ctx={ctx} k="incidents.open_after" unit="cycles" />
          </Field>
          <Field ctx={ctx} k="incidents.close_after" label="Close after" hint="Healthy cycles in a row">
            <Num ctx={ctx} k="incidents.close_after" unit="cycles" />
          </Field>
        </Rows>
      </Section>
      <Section ctx={ctx} keys={KEYS.thresholds} title="Thresholds" description="Above any of these limits a hop is degraded. Not every hop uses every limit.">
        <ThresholdsTable ctx={ctx} />
      </Section>
      <Section ctx={ctx} keys={KEYS.speed} title="Speed tests" description="Scheduled throughput tests. You can always start one from the Speed page.">
        <Rows>
          <Field ctx={ctx} k="speed.backend" label="Backend">
            <Choice ctx={ctx} k="speed.backend" options={{ cloudflare: "Cloudflare", ookla: "Ookla" }} />
          </Field>
          <Field ctx={ctx} k="speed.interval" label="Interval" hint="0 turns scheduled tests off">
            <Text ctx={ctx} k="speed.interval" width="short" placeholder="1h" />
          </Field>
          <Field ctx={ctx} k="speed.server" label="Ookla server" hint="host:port; empty picks the fastest nearby">
            <Text ctx={ctx} k="speed.server" width="medium" placeholder="auto" />
          </Field>
          <Field ctx={ctx} k="speed.streams" label="Streams" hint="Parallel connections per direction">
            <Num ctx={ctx} k="speed.streams" />
          </Field>
          <Field ctx={ctx} k="speed.duration" label="Duration" hint="How long each direction runs">
            <Text ctx={ctx} k="speed.duration" width="short" placeholder="10s" />
          </Field>
        </Rows>
      </Section>
      <Section ctx={ctx} keys={KEYS.dns} title="DNS" description="The resolvers compared against this host's own, and the names asked of each. The first name is the headline check.">
        <Rows>
          <Field ctx={ctx} k="dns.system" label="System resolvers" hint="Test the resolvers this host uses">
            <Toggle ctx={ctx} k="dns.system" />
          </Field>
          <Field ctx={ctx} k="dns.record_types" label="Record types">
            <RecordTypes ctx={ctx} />
          </Field>
        </Rows>
        <List
          ctx={ctx}
          k="dns.resolvers"
          title="Resolvers"
          cols={[
            { key: "name", label: "Name", placeholder: "Cloudflare" },
            { key: "address", label: "Address", placeholder: "1.1.1.1", mono: true },
          ]}
          blank={{ name: "", address: "" }}
        />
        <List
          ctx={ctx}
          k="dns.queries"
          title="Query names"
          cols={[
            { key: "name", label: "Name", placeholder: "example.com", mono: true },
            { key: "expect", label: "Expected answers", placeholder: "any", list: true, mono: true },
          ]}
          blank={{ name: "", expect: [] }}
        />
      </Section>
      <Section ctx={ctx} keys={KEYS.internet} title="Internet" description="TCP handshakes, and ICMP pings where allowed, to anycast addresses that answer from nearby.">
        <Rows>
          <Field ctx={ctx} k="internet.port" label="TCP port">
            <Num ctx={ctx} k="internet.port" />
          </Field>
          <Field ctx={ctx} k="internet.icmp" label="ICMP" hint="Also ping each target; informational only">
            <Toggle ctx={ctx} k="internet.icmp" />
          </Field>
        </Rows>
        <List
          ctx={ctx}
          k="internet.targets"
          title="Targets"
          cols={[
            { key: "name", label: "Name", placeholder: "Cloudflare" },
            { key: "v4", label: "IPv4", placeholder: "none", nullable: true, mono: true },
            { key: "v6", label: "IPv6", placeholder: "none", nullable: true, mono: true },
          ]}
          blank={{ name: "", v4: null, v6: null }}
        />
      </Section>
      <Section ctx={ctx} keys={KEYS.web} title="Web" description="Captive portal detection, the TLS handshake, and HTTPS sites that must answer.">
        <Rows>
          <Field ctx={ctx} k="web.captive_portal" label="Captive portal" hint="Must answer 204 over plain HTTP">
            <Text ctx={ctx} k="web.captive_portal" width="long" />
          </Field>
          <Field ctx={ctx} k="web.tls_host" label="TLS host" hint="Handshake and certificate check">
            <Text ctx={ctx} k="web.tls_host" width="medium" />
          </Field>
        </Rows>
        <List ctx={ctx} k="web.sites" title="HTTPS sites" cols={[{ key: "", label: "URL", placeholder: "https://example.com", mono: true }]} blank="" />
      </Section>
      <Section ctx={ctx} keys={KEYS.server} title="Server and storage" description="Read once at startup. Edit the file and restart wirewolf to change these.">
        <Rows>
          <Field ctx={ctx} k="server.listen" label="Listen address">
            <Text ctx={ctx} k="server.listen" width="medium" />
          </Field>
          <Field ctx={ctx} k="storage.db" label="History file">
            <Text ctx={ctx} k="storage.db" width="long" placeholder="memory only" />
          </Field>
          <Field ctx={ctx} k="storage.retention" label="Keep history">
            <Text ctx={ctx} k="storage.retention" width="short" />
          </Field>
        </Rows>
      </Section>

      <Dialog open={!!leavingTo} onOpenChange={(open) => !open && setLeavingTo(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Leave without saving?</DialogTitle>
            <DialogDescription>
              You have {unsaved} unsaved {unsaved === 1 ? "change" : "changes"} to the settings. They'll be lost if you leave now.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setLeavingTo(null)}>
              Stay
            </Button>
            <Button variant="outline" onClick={leave}>
              Discard and leave
            </Button>
            {canEdit && (
              <Button
                disabled={saving}
                onClick={async () => {
                  // On a validation error, stay so the field errors are visible.
                  if (await save()) leave()
                  else setLeavingTo(null)
                }}
              >
                {saving ? "Saving…" : "Save and leave"}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {canEdit && dirty && (
        <div className="sticky bottom-0 -mx-4 mt-6 flex items-center justify-end gap-2 border-t bg-background/95 px-4 py-3 backdrop-blur sm:-mx-6 sm:px-6">
          <p className="mr-auto text-sm text-muted-foreground">
            {unsaved} unsaved {unsaved === 1 ? "change" : "changes"}
          </p>
          <Button variant="outline" disabled={saving} onClick={() => (setDraft(structuredClone(view.config)), setErrors([]))}>
            Discard
          </Button>
          <Button disabled={saving} onClick={save}>
            {saving ? "Saving…" : "Save changes"}
          </Button>
        </div>
      )}
    </div>
  )
}

/** One full-width row of the page: what the section controls on the left, its fields on the right. */
function Section({ ctx, keys, title, description, children }: { ctx: Ctx; keys: string[]; title: string; description: string; children: ReactNode }) {
  const changed = ctx.defaults ? keys.filter((k) => differs(ctx.draft, ctx.defaults, k)).length : 0
  if (ctx.changedOnly && changed === 0 && !keys.some((k) => differs(ctx.draft, ctx.view.config, k))) return null
  return (
    <section className="grid gap-4 border-t py-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,2fr)] lg:gap-8">
      <div className="flex flex-col gap-1">
        <h2 className="flex items-baseline gap-2 text-base font-semibold">
          {title}
          {keys[0] === "targets" ? (
            <span className="text-xs font-normal text-muted-foreground">{ctx.draft.targets.length} configured</span>
          ) : (
            changed > 0 && <span className="text-xs font-normal text-muted-foreground">{changed} changed</span>
          )}
        </h2>
        <p className="text-sm text-pretty text-muted-foreground">{description}</p>
      </div>
      <div className="flex min-w-0 flex-col gap-6">{children}</div>
    </section>
  )
}

function Rows({ children }: { children: ReactNode }) {
  return <div className="flex flex-col divide-y">{children}</div>
}

/** Why a setting can't be edited here, or null if it can. */
function lockOf(ctx: Ctx, key: string): { label: string; why: string } | null {
  const source = sourceOf(key, ctx.view.sources)
  if (source === "env" || source === "flag")
    return { label: source, why: `Set by ${source === "env" ? "an environment variable" : "a command line flag"}, which overrides the file` }
  if (ctx.view.restart_keys?.includes(key)) return { label: "restart", why: "Takes effect after a restart" }
  return null
}

function editable(ctx: Ctx, key: string) {
  return ctx.canEdit && !lockOf(ctx, key)
}

function LockBadge({ lock }: { lock: { label: string; why: string } | null }) {
  if (!lock) return null
  return (
    <Badge variant="outline" title={lock.why}>
      <Lock /> {lock.label}
    </Badge>
  )
}

/** A setting row: label and hint on the left, the control on the right, its error below. */
function Field({ ctx, k, label, hint, children }: { ctx: Ctx; k: string; label: string; hint?: string; children: ReactNode }) {
  const lock = lockOf(ctx, k)
  const error = ctx.errors[k]
  const state = changeOf(ctx, k)
  if (ctx.changedOnly && !state.changed && !state.unsaved) return null
  return (
    <div className="flex min-h-14 flex-col gap-2 py-3 first:pt-0 last:pb-0 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
      <div className="flex min-w-0 flex-col gap-0.5">
        <Label htmlFor={k} className="relative flex items-center gap-2">
          <ChangedDot changed={state.changed} />
          {label}
          <LockBadge lock={lock} />
        </Label>
        {(lock || hint) && <p className="text-xs text-muted-foreground">{lock?.why ?? hint}</p>}
        <ChangeNote ctx={ctx} k={k} />
        {error && <p className="text-xs text-destructive">{error}</p>}
      </div>
      <div className="flex shrink-0 items-center justify-start gap-1 sm:justify-end">
        {children}
        <ResetButton ctx={ctx} k={k} label={label} />
      </div>
    </div>
  )
}

/** Control widths, by what goes in them. */
const WIDTH = { short: "sm:w-28", medium: "sm:w-56", long: "sm:w-96" } as const

/** Hides the browser's number spinners; the values are typed, not nudged. */
const NO_SPIN = "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"

function Text({ ctx, k, placeholder, width }: { ctx: Ctx; k: string; placeholder?: string; width: keyof typeof WIDTH }) {
  return (
    <Input
      id={k}
      value={String(getPath(ctx.draft, k) ?? "")}
      placeholder={placeholder}
      disabled={!editable(ctx, k)}
      aria-invalid={!!ctx.errors[k]}
      className={cn("w-full font-mono", WIDTH[width])}
      onChange={(e) => ctx.set(k, e.target.value)}
    />
  )
}

function NumInput({ ctx, k, label, unit, className }: { ctx: Ctx; k: string; label?: string; unit?: string; className?: string }) {
  return (
    <div className={cn("relative", className)}>
      <Input
        id={label ? undefined : k}
        aria-label={label}
        type="number"
        step="any"
        inputMode="decimal"
        value={String(getPath(ctx.draft, k) ?? "")}
        disabled={!editable(ctx, k)}
        aria-invalid={!!ctx.errors[k]}
        className={cn("w-full tabular-nums", NO_SPIN, unit && (unit.length > 2 ? "pr-14" : "pr-8"))}
        onChange={(e) => ctx.set(k, e.target.value === "" ? "" : Number(e.target.value))}
      />
      {unit && <span className="pointer-events-none absolute inset-y-0 right-2.5 flex items-center text-xs text-muted-foreground">{unit}</span>}
    </div>
  )
}

function Num({ ctx, k, unit }: { ctx: Ctx; k: string; unit?: string }) {
  return <NumInput ctx={ctx} k={k} unit={unit} className={cn("w-full", WIDTH.short)} />
}

function Toggle({ ctx, k }: { ctx: Ctx; k: string }) {
  return <Switch id={k} checked={!!getPath(ctx.draft, k)} disabled={!editable(ctx, k)} onCheckedChange={(v) => ctx.set(k, v)} />
}

function Choice({ ctx, k, options }: { ctx: Ctx; k: string; options: Record<string, string> }) {
  const value = String(getPath(ctx.draft, k))
  return (
    <Select value={value} disabled={!editable(ctx, k)} onValueChange={(v) => ctx.set(k, v)}>
      <SelectTrigger id={k} className={cn("w-full", WIDTH.short)} aria-invalid={!!ctx.errors[k]}>
        <SelectValue>{options[value] ?? value}</SelectValue>
      </SelectTrigger>
      <SelectContent>
        {Object.entries(options).map(([v, label]) => (
          <SelectItem key={v} value={v}>
            {label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function RecordTypes({ ctx }: { ctx: Ctx }) {
  const k = "dns.record_types"
  const types = ctx.draft.dns.record_types
  return (
    <div className="flex gap-6">
      {(["A", "AAAA"] as const).map((t) => (
        <Label key={t} className="flex items-center gap-2 font-mono text-xs">
          <Switch
            checked={types.includes(t)}
            disabled={!editable(ctx, k)}
            onCheckedChange={(on) => ctx.set(k, (["A", "AAAA"] as const).filter((x) => (x === t ? on : types.includes(x))))}
          />
          {t}
        </Label>
      ))}
    </div>
  )
}

interface Col {
  /** Field of each row; `""` when rows are plain strings. */
  key: string
  label: string
  placeholder?: string
  /** A comma-separated list. */
  list?: boolean
  /** Empty means `null`. */
  nullable?: boolean
  mono?: boolean
}

/** An editable list such as `[[dns.resolvers]]`: a titled block, column labels once, then rows. */
function List({ ctx, k, title, cols, blank }: { ctx: Ctx; k: string; title: string; cols: Col[]; blank: unknown }) {
  const state = changeOf(ctx, k)
  if (ctx.changedOnly && !state.changed && !state.unsaved) return null
  const rows = (getPath(ctx.draft, k) as unknown[]) ?? []
  const can = editable(ctx, k)
  const cellKey = (i: number, c: Col) => (c.key ? `${k}.${i}.${c.key}` : `${k}.${i}`)
  const grid = { gridTemplateColumns: `repeat(${cols.length}, minmax(0, 1fr))${can ? " 2rem" : ""}` }
  return (
    <div className="flex flex-col gap-2">
      <div className="flex h-8 items-center justify-between">
        <h3 className="relative flex items-center gap-2 text-sm font-medium">
          <ChangedDot changed={changeOf(ctx, k).changed} />
          {title}
          <LockBadge lock={lockOf(ctx, k)} />
        </h3>
        <div className="flex items-center gap-1">
          <ResetButton ctx={ctx} k={k} label={title} />
          {can && (
            <Button variant="ghost" size="sm" onClick={() => ctx.set(k, [...rows, structuredClone(blank)])}>
              <Plus /> Add
            </Button>
          )}
        </div>
      </div>
      <ChangeNote ctx={ctx} k={k} list />
      {ctx.errors[k] && <p className="text-xs text-destructive">{ctx.errors[k]}</p>}
      {rows.length === 0 ? (
        <p className="text-sm text-muted-foreground">None.</p>
      ) : (
        <div className="grid items-center gap-x-2 gap-y-2" style={grid}>
          {cols.map((c) => (
            <span key={c.key} className="text-xs text-muted-foreground">
              {c.label}
            </span>
          ))}
          {can && <span />}
          {rows.map((row, i) => (
            <RowCells key={i} ctx={ctx} title={title} i={i} row={row} cols={cols} can={can} cellKey={cellKey} onRemove={() => ctx.set(k, rows.filter((_, j) => j !== i))} />
          ))}
        </div>
      )}
    </div>
  )
}

function RowCells({
  ctx,
  title,
  i,
  row,
  cols,
  can,
  cellKey,
  onRemove,
}: {
  ctx: Ctx
  title: string
  i: number
  row: unknown
  cols: Col[]
  can: boolean
  cellKey: (i: number, c: Col) => string
  onRemove: () => void
}) {
  const errors = cols.map((c) => ctx.errors[cellKey(i, c)]).filter(Boolean)
  return (
    <>
      {cols.map((c) => {
        const raw = c.key ? (row as Record<string, unknown>)[c.key] : row
        const shown = c.list ? (raw as string[]).join(", ") : String(raw ?? "")
        return (
          <Input
            key={c.key}
            aria-label={`${title} ${i + 1} ${c.label}`}
            placeholder={c.placeholder}
            value={shown}
            disabled={!can}
            aria-invalid={!!ctx.errors[cellKey(i, c)]}
            className={cn("min-w-0", c.mono && "font-mono")}
            onChange={(e) => {
              const v = e.target.value
              const value = c.list ? v.split(",").map((s) => s.trim()).filter(Boolean) : c.nullable && v.trim() === "" ? null : v
              ctx.set(cellKey(i, c), value)
            }}
          />
        )
      })}
      {can && (
        <Button variant="ghost" size="icon" className="text-muted-foreground" aria-label={`Remove ${title} ${i + 1}`} onClick={onRemove}>
          <Trash2 />
        </Button>
      )}
      {errors.length > 0 && <p className="col-span-full -mt-1 text-xs text-destructive">{errors.join(" · ")}</p>}
    </>
  )
}

const HOPS = ["gateway", "isp", "internet", "dns", "targets"] as const
const LIMITS: [keyof Limits, string, string][] = [
  ["rtt_warn_ms", "RTT", "ms"],
  ["jitter_warn_ms", "Jitter", "ms"],
  ["loss_warn_pct", "Loss", "%"],
]

function ThresholdsTable({ ctx }: { ctx: Ctx }) {
  const bad = HOPS.flatMap((h) => LIMITS.map(([l]) => `thresholds.${h}.${l}`)).filter((k) => ctx.errors[k])
  return (
    <div className="flex flex-col gap-2">
      <div className="grid grid-cols-[auto_repeat(3,minmax(0,1fr))_2rem] items-center gap-x-2 gap-y-2 sm:gap-x-4">
        <span className="text-xs text-muted-foreground">Hop</span>
        {LIMITS.map(([, label]) => (
          <span key={label} className="text-xs text-muted-foreground">
            {label}
          </span>
        ))}
        <span />
        {HOPS.map((h) => (
          <Fragment key={h}>
            <span className="relative flex items-center gap-2 font-mono text-xs">
              <ChangedDot changed={changeOf(ctx, `thresholds.${h}`).changed} />
              {h}
            </span>
            {LIMITS.map(([l, label, unit]) => (
              <NumInput key={l} ctx={ctx} k={`thresholds.${h}.${l}`} label={`${h} ${label}`} unit={unit} />
            ))}
            <ResetButton ctx={ctx} k={`thresholds.${h}`} label={`${h} thresholds`} />
          </Fragment>
        ))}
      </div>
      {ctx.defaults &&
        HOPS.filter((h) => changeOf(ctx, `thresholds.${h}`).changed).map((h) => (
          <p key={h} className="text-xs text-muted-foreground">
            <span className="font-mono">{h}</span> default: {LIMITS.map(([l, label, unit]) => `${label} ${getPath(ctx.defaults, `thresholds.${h}.${l}`)}${unit === "%" ? "%" : ` ${unit}`}`).join(" · ")}
          </p>
        ))}
      {bad.map((k) => (
        <p key={k} className="text-xs text-destructive">
          <code className="font-mono">{k.slice("thresholds.".length)}</code> {ctx.errors[k]}
        </p>
      ))}
    </div>
  )
}

const BLANK_TARGET: UserTargetConfig = { name: "", check: "", critical: false, verify_tls: true }

/** `[[targets]]`: a table with add, edit (in a dialog), and remove. */
function TargetsTable({ ctx }: { ctx: Ctx }) {
  const [editing, setEditing] = useState<{ index: number | null; target: UserTargetConfig } | null>(null)
  const targets = ctx.draft.targets
  const can = editable(ctx, "targets")
  const rowError = (i: number) => ["name", "check"].map((f) => ctx.errors[`targets.${i}.${f}`]).find(Boolean)
  return (
    <div className="flex flex-col gap-2">
      {targets.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          None yet. Try a NAS (<code className="font-mono">tcp://nas.local:445</code>), the router's admin page, a printer, or a Pi-hole.
        </p>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="pl-0">Name</TableHead>
              <TableHead>Check</TableHead>
              <TableHead>Critical</TableHead>
              {can && <TableHead className="w-20 pr-0" />}
            </TableRow>
          </TableHeader>
          <TableBody>
            {targets.map((t, i) => (
              <TableRow key={i}>
                <TableCell className="pl-0">
                  {t.name}
                  {rowError(i) && <p className="text-xs text-destructive">{rowError(i)}</p>}
                </TableCell>
                <TableCell className="font-mono text-xs whitespace-normal break-all text-muted-foreground">
                  {t.check}
                  {t.check.startsWith("https://") && !t.verify_tls && <span className="font-sans"> · certificate not checked</span>}
                </TableCell>
                <TableCell>
                  <Switch aria-label={`${t.name} critical`} checked={t.critical} disabled={!can} onCheckedChange={(v) => ctx.set(`targets.${i}.critical`, v)} />
                </TableCell>
                {can && (
                  <TableCell className="pr-0 text-right">
                    <Button variant="ghost" size="icon" className="text-muted-foreground" aria-label={`Edit ${t.name}`} onClick={() => setEditing({ index: i, target: { ...t } })}>
                      <Pencil />
                    </Button>
                    <Button variant="ghost" size="icon" className="text-muted-foreground" aria-label={`Remove ${t.name}`} onClick={() => ctx.set("targets", targets.filter((_, j) => j !== i))}>
                      <Trash2 />
                    </Button>
                  </TableCell>
                )}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
      {can && (
        <div>
          <Button variant="outline" size="sm" onClick={() => setEditing({ index: null, target: { ...BLANK_TARGET } })}>
            <Plus /> Add target
          </Button>
        </div>
      )}
      <TargetDialog
        key={editing ? (editing.index ?? "new") : "closed"}
        editing={editing}
        onClose={() => setEditing(null)}
        onDone={(t) => {
          if (!editing) return
          const next = [...targets]
          if (editing.index === null) next.push(t)
          else next[editing.index] = t
          ctx.set("targets", next)
          setEditing(null)
        }}
      />
    </div>
  )
}

function TargetDialog({
  editing,
  onClose,
  onDone,
}: {
  editing: { index: number | null; target: UserTargetConfig } | null
  onClose: () => void
  onDone: (t: UserTargetConfig) => void
}) {
  const [t, setT] = useState<UserTargetConfig>(editing?.target ?? BLANK_TARGET)
  const schemeOk = /^(tcp|https?|ping|icmp|dns):\/\/./.test(t.check)
  return (
    <Dialog open={!!editing} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{editing?.index === null ? "Add target" : "Edit target"}</DialogTitle>
          <DialogDescription>The check is validated by the server when you save.</DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault()
            onDone({ ...t, name: t.name.trim(), check: t.check.trim() })
          }}
        >
          <div className="flex flex-col gap-2">
            <Label htmlFor="target-name">Name</Label>
            <Input id="target-name" value={t.name} placeholder="NAS" autoFocus onChange={(e) => setT({ ...t, name: e.target.value })} />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="target-check">Check</Label>
            <Input id="target-check" className="font-mono" value={t.check} placeholder="tcp://nas.local:445" onChange={(e) => setT({ ...t, check: e.target.value })} />
            <p className="text-xs text-muted-foreground">
              <code className="font-mono">tcp://host:port</code>, <code className="font-mono">http(s)://…</code>, <code className="font-mono">ping://host</code>, or{" "}
              <code className="font-mono">dns://server</code>
            </p>
          </div>
          <Label className="flex items-center justify-between gap-4">
            <span className="flex flex-col gap-1">
              Critical
              <span className="text-xs font-normal text-muted-foreground">A failure degrades the verdict and can open an incident</span>
            </span>
            <Switch checked={t.critical} onCheckedChange={(v) => setT({ ...t, critical: v })} />
          </Label>
          {t.check.startsWith("https://") && (
            <Label className="flex items-center justify-between gap-4">
              <span className="flex flex-col gap-1">
                Verify TLS
                <span className="text-xs font-normal text-muted-foreground">Turn off for self-signed admin pages</span>
              </span>
              <Switch checked={t.verify_tls} onCheckedChange={(v) => setT({ ...t, verify_tls: v })} />
            </Label>
          )}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={!t.name.trim() || !schemeOk}>
              {editing?.index === null ? "Add" : "Done"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** The settings in each section, as dotted keys. Lists and threshold rows count as one. */
const KEYS = {
  probe: ["probe.interval", "probe.ipv6", "probe.path_mtu"],
  incidents: ["incidents.open_after", "incidents.close_after"],
  thresholds: ["gateway", "isp", "internet", "dns", "targets"].map((h) => `thresholds.${h}`),
  speed: ["speed.backend", "speed.interval", "speed.server", "speed.streams", "speed.duration"],
  dns: ["dns.system", "dns.record_types", "dns.resolvers", "dns.queries"],
  internet: ["internet.port", "internet.icmp", "internet.targets"],
  web: ["web.captive_portal", "web.tls_host", "web.sites"],
  server: ["server.listen", "storage.db", "storage.retention"],
}
const ALL_KEYS = ["targets", ...Object.values(KEYS).flat()]

function differs(a: unknown, b: unknown, key: string) {
  return JSON.stringify(getPath(a, key)) !== JSON.stringify(getPath(b, key))
}

/** Whether a setting differs from its default, and whether it has an unsaved edit. */
function changeOf(ctx: Ctx, key: string) {
  return { changed: !!ctx.defaults && differs(ctx.draft, ctx.defaults, key), unsaved: differs(ctx.draft, ctx.view.config, key) }
}

/** A value as a short phrase, for "Default …" and "was …". */
function show(v: unknown): string {
  if (v === null || v === undefined || v === "") return "empty"
  if (typeof v === "boolean") return v ? "on" : "off"
  if (Array.isArray(v)) {
    if (v.length === 0) return "none"
    return v.map((x) => (typeof x === "object" && x !== null ? String((x as { name?: unknown }).name ?? JSON.stringify(x)) : String(x))).join(", ")
  }
  return String(v)
}

/** Marks a setting that differs from its default. Hangs in the gutter so labels and hints share an edge. */
function ChangedDot({ changed }: { changed: boolean }) {
  if (!changed) return null
  return (
    <span
      aria-label={changed ? "Changed from default" : undefined}
      title={changed ? "Changed from the default" : undefined}
      className="absolute top-1/2 -left-3.5 size-1.5 -translate-y-1/2 rounded-full bg-primary"
    />
  )
}

/** "Unsaved · was …" for an edit, or "Default …" for a saved change. */
function ChangeNote({ ctx, k, list }: { ctx: Ctx; k: string; list?: boolean }) {
  const { changed, unsaved } = changeOf(ctx, k)
  if (unsaved) return <p className="text-xs text-primary">Unsaved · was {show(getPath(ctx.view.config, k))}</p>
  if (changed && ctx.defaults) return <p className={cn("text-xs text-muted-foreground", list && "-mt-1")}>Default: {show(getPath(ctx.defaults, k))}</p>
  return null
}

/** Puts one setting back to its default. Holds its space when hidden so controls line up. */
function ResetButton({ ctx, k, label }: { ctx: Ctx; k: string; label: string }) {
  if (!ctx.canEdit || !ctx.defaults) return null
  const show = changeOf(ctx, k).changed && editable(ctx, k)
  return (
    <Button
      variant="ghost"
      size="icon"
      aria-label={`Reset ${label} to default`}
      title="Reset to default"
      className={cn("text-muted-foreground", !show && "invisible")}
      onClick={() => ctx.set(k, structuredClone(getPath(ctx.defaults, k)))}
    >
      <RotateCcw />
    </Button>
  )
}

/** Keys with a control on this page; errors for anything else go in the banner. */
function hasControl(key: string) {
  return /^(probe|incidents|speed|thresholds|dns|internet|web|server|storage|targets)\./.test(key) || key === "targets"
}

function getPath(obj: unknown, key: string): unknown {
  return key.split(".").reduce<unknown>((o, k) => (o == null ? undefined : (o as Record<string, unknown>)[k]), obj)
}

function setPath(obj: unknown, key: string, value: unknown) {
  const parts = key.split(".")
  const last = parts.pop()!
  const parent = parts.reduce<Record<string, unknown>>((o, k) => o[k] as Record<string, unknown>, obj as Record<string, unknown>)
  parent[last] = value
}

/** A setting's source, or its nearest set parent (a whole table set in the file). */
function sourceOf(key: string, sources: Record<string, ConfigSource>): ConfigSource {
  for (let k = key; k; k = k.includes(".") ? k.slice(0, k.lastIndexOf(".")) : "") if (sources[k]) return sources[k]
  return "default"
}
