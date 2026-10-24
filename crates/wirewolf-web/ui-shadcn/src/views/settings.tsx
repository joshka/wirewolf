import { useEffect, useMemo, useState, type ReactNode } from "react"
import { Lock, Pencil, Plus, RotateCcw, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import {
  api,
  type Config,
  type ConfigSource,
  type ConfigView,
  type FieldError,
  type Limits,
  type Session,
  type UserTargetConfig,
} from "@/lib/api"

/** Everything one setting control needs: its value, whether it can change, and its error. */
interface Ctx {
  draft: Config
  view: ConfigView
  canEdit: boolean
  errors: Record<string, string>
  set: (key: string, value: unknown) => void
}

/** The effective config, editable when the server and session allow it. */
export function SettingsView() {
  const [view, setView] = useState<ConfigView | null | undefined>(undefined)
  const [session, setSession] = useState<Session | null>(null)
  const [draft, setDraft] = useState<Config | null>(null)
  const [errors, setErrors] = useState<FieldError[]>([])
  const [saving, setSaving] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)
  useEffect(() => {
    api.config().then(
      (v) => {
        setView(v)
        setDraft(v ? structuredClone(v.config) : null)
      },
      (e: Error) => setLoadError(e.message),
    )
    // Servers without the auth layer have no /session; their writes aren't gated by login.
    api.session().then(setSession, () => setSession(null))
  }, [])
  const dirty = useMemo(() => !!view && !!draft && JSON.stringify(view.config) !== JSON.stringify(draft), [view, draft])
  if (loadError) return <p className="text-sm text-fail">Couldn't load the config: {loadError}</p>
  if (view === undefined || (view && !draft)) return <Skeleton className="h-96 w-full rounded-xl" />
  if (view === null || !draft) return <p className="text-muted-foreground">This server doesn't report its configuration.</p>

  const signedOut = session !== null && !session.can_write
  const canEdit = !!view.editable && !signedOut
  const byKey = Object.fromEntries(errors.filter((e) => e.key).map((e) => [e.key, e.message]))
  const general = errors.filter((e) => !e.key || !hasControl(e.key))
  const ctx: Ctx = {
    draft,
    view,
    canEdit,
    errors: byKey,
    set: (key, value) => {
      const next = structuredClone(draft)
      setPath(next, key, value)
      setDraft(next)
      setErrors((es) => es.filter((e) => e.key !== key))
    },
  }
  const save = async () => {
    setSaving(true)
    const res = await api.saveConfig(draft, session?.csrf).catch((e: Error) => ({ ok: false as const, status: 0, errors: [{ key: "", message: e.message }] }))
    setSaving(false)
    if (res.ok) {
      setView(res.view)
      setDraft(structuredClone(res.view.config))
      setErrors([])
      toast.success("Settings saved", { description: "Probes use them from the next cycle." })
    } else {
      setErrors(res.errors)
      toast.error(res.errors.length === 1 ? "One setting needs fixing" : `${res.errors.length} settings need fixing`)
    }
  }
  const set = Object.keys(view.sources).length

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
          <p className="text-sm text-muted-foreground">
            {view.path ? (
              <>
                From <code className="font-mono">{view.path}</code>, {set} {set === 1 ? "setting" : "settings"} changed from the defaults.
              </>
            ) : (
              "No config file yet: everything is a default."
            )}{" "}
            {canEdit ? (
              <>
                Saving writes <code className="font-mono">{view.save_path}</code>, keeping its comments, and applies from the next probe cycle.
              </>
            ) : signedOut ? (
              "Sign in to change them."
            ) : (
              <>
                Read-only on this server. Edit the file and restart to change them.
              </>
            )}
          </p>
        </div>
        {canEdit && (
          <div className="flex gap-2">
            <Button variant="outline" disabled={!dirty || saving} onClick={() => (setDraft(structuredClone(view.config)), setErrors([]))}>
              <RotateCcw /> Discard
            </Button>
            <Button disabled={!dirty || saving} onClick={save}>
              {saving ? "Saving…" : "Save"}
            </Button>
          </div>
        )}
      </div>
      {general.length > 0 && (
        <div role="alert" className="rounded-lg border border-destructive/50 px-4 py-3 text-sm text-destructive">
          {general.map((e, i) => (
            <p key={i}>
              {e.key && <code className="font-mono">{e.key}: </code>}
              {e.message}
            </p>
          ))}
        </div>
      )}
      <TargetsCard ctx={ctx} />
      <div className="grid gap-4 xl:grid-cols-2">
        <Section title="Probing" description="How often the path is probed, and which extras run">
          <Field ctx={ctx} k="probe.interval" label="Interval" hint="Time between cycles, at least 5s">
            <Text ctx={ctx} k="probe.interval" placeholder="30s" />
          </Field>
          <Field ctx={ctx} k="probe.ipv6" label="IPv6" hint="Auto tests IPv6 when this host has a global address">
            <Choice ctx={ctx} k="probe.ipv6" options={{ auto: "Auto", on: "On", off: "Off" }} />
          </Field>
          <Field ctx={ctx} k="probe.path_mtu" label="Path MTU" hint="Don't-fragment pings toward the internet (needs ICMP)">
            <Toggle ctx={ctx} k="probe.path_mtu" />
          </Field>
        </Section>
        <Section title="Incidents" description="How many cycles in a row open or close an incident">
          <Field ctx={ctx} k="incidents.open_after" label="Open after" hint="Unhealthy cycles in a row">
            <Num ctx={ctx} k="incidents.open_after" />
          </Field>
          <Field ctx={ctx} k="incidents.close_after" label="Close after" hint="Healthy cycles in a row">
            <Num ctx={ctx} k="incidents.close_after" />
          </Field>
        </Section>
        <Section title="Speed tests" description="Scheduled throughput tests">
          <Field ctx={ctx} k="speed.backend" label="Backend">
            <Choice ctx={ctx} k="speed.backend" options={{ cloudflare: "Cloudflare", ookla: "Ookla (speedtest.net)" }} />
          </Field>
          <Field ctx={ctx} k="speed.interval" label="Interval" hint="0 turns scheduled tests off">
            <Text ctx={ctx} k="speed.interval" placeholder="1h" />
          </Field>
          <Field ctx={ctx} k="speed.server" label="Ookla server" hint="host:port; empty picks the fastest nearby">
            <Text ctx={ctx} k="speed.server" placeholder="auto" />
          </Field>
          <Field ctx={ctx} k="speed.streams" label="Streams" hint="Parallel connections per direction">
            <Num ctx={ctx} k="speed.streams" />
          </Field>
          <Field ctx={ctx} k="speed.duration" label="Duration" hint="How long each direction runs">
            <Text ctx={ctx} k="speed.duration" placeholder="10s" />
          </Field>
        </Section>
        <ThresholdsCard ctx={ctx} />
        <Section title="DNS" description="Resolvers compared and the names asked of each">
          <Field ctx={ctx} k="dns.system" label="System resolvers" hint="Test the resolvers this host uses">
            <Toggle ctx={ctx} k="dns.system" />
          </Field>
          <Field ctx={ctx} k="dns.record_types" label="Record types">
            <RecordTypes ctx={ctx} />
          </Field>
          <Rows
            ctx={ctx}
            k="dns.resolvers"
            title="Resolvers"
            cols={[
              { key: "name", label: "Name", placeholder: "Cloudflare" },
              { key: "address", label: "Address", placeholder: "1.1.1.1" },
            ]}
            blank={{ name: "", address: "" }}
          />
          <Rows
            ctx={ctx}
            k="dns.queries"
            title="Query names"
            cols={[
              { key: "name", label: "Name", placeholder: "example.com" },
              { key: "expect", label: "Expect (comma-separated)", placeholder: "optional", list: true },
            ]}
            blank={{ name: "", expect: [] }}
          />
        </Section>
        <Section title="Internet" description="TCP handshakes (and ICMP pings) to anycast targets">
          <Field ctx={ctx} k="internet.port" label="TCP port">
            <Num ctx={ctx} k="internet.port" />
          </Field>
          <Field ctx={ctx} k="internet.icmp" label="ICMP" hint="Also ping each target; informational only">
            <Toggle ctx={ctx} k="internet.icmp" />
          </Field>
          <Rows
            ctx={ctx}
            k="internet.targets"
            title="Targets"
            cols={[
              { key: "name", label: "Name", placeholder: "Cloudflare" },
              { key: "v4", label: "IPv4", placeholder: "1.1.1.1", nullable: true },
              { key: "v6", label: "IPv6", placeholder: "optional", nullable: true },
            ]}
            blank={{ name: "", v4: null, v6: null }}
          />
        </Section>
        <Section title="Web" description="Captive portal, TLS, and HTTPS reachability">
          <Field ctx={ctx} k="web.captive_portal" label="Captive portal" hint="Must answer 204 over plain HTTP">
            <Text ctx={ctx} k="web.captive_portal" />
          </Field>
          <Field ctx={ctx} k="web.tls_host" label="TLS host" hint="Handshake and certificate check">
            <Text ctx={ctx} k="web.tls_host" />
          </Field>
          <Rows ctx={ctx} k="web.sites" title="HTTPS sites" cols={[{ key: "", label: "URL", placeholder: "https://example.com" }]} blank="" />
        </Section>
        <Section title="Server and storage" description="Read at startup: edit the file and restart to change these">
          {(["server.listen", "storage.db", "storage.retention"] as const).map((k) => (
            <Field key={k} ctx={ctx} k={k} label={{ "server.listen": "Listen address", "storage.db": "History file", "storage.retention": "Keep history" }[k]}>
              <Text ctx={ctx} k={k} />
            </Field>
          ))}
        </Section>
      </div>
    </div>
  )
}

function Section({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">{children}</CardContent>
    </Card>
  )
}

/** Why a setting can't be edited here, or null if it can. */
function lockOf(ctx: Ctx, key: string): { label: string; why: string } | null {
  const source = sourceOf(key, ctx.view.sources)
  if (source === "env" || source === "flag")
    return { label: source, why: `Set by ${source === "env" ? "an environment variable" : "a command line flag"}, which overrides the file.` }
  if (ctx.view.restart_keys?.includes(key)) return { label: "restart", why: "Takes effect only after a restart." }
  return null
}

function editable(ctx: Ctx, key: string) {
  return ctx.canEdit && !lockOf(ctx, key)
}

/** A labelled setting row: label and hint on the left, control on the right, error below. */
function Field({ ctx, k, label, hint, children }: { ctx: Ctx; k: string; label: string; hint?: string; children: ReactNode }) {
  const lock = lockOf(ctx, k)
  const error = ctx.errors[k]
  return (
    <div className="grid gap-x-4 gap-y-1 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] sm:items-center">
      <div className="flex flex-col">
        <Label htmlFor={k} className="flex items-center gap-2">
          {label}
          {lock && (
            <Badge variant="secondary" title={lock.why}>
              <Lock /> {lock.label}
            </Badge>
          )}
          {!lock && <SourceBadge source={sourceOf(k, ctx.view.sources)} />}
        </Label>
        {(hint || lock) && <p className="text-xs text-muted-foreground">{lock?.why ?? hint}</p>}
      </div>
      <div>{children}</div>
      {error && <p className="text-xs text-destructive sm:col-start-2">{error}</p>}
    </div>
  )
}

function Text({ ctx, k, placeholder }: { ctx: Ctx; k: string; placeholder?: string }) {
  return (
    <Input
      id={k}
      value={String(getPath(ctx.draft, k) ?? "")}
      placeholder={placeholder}
      disabled={!editable(ctx, k)}
      aria-invalid={!!ctx.errors[k]}
      className="font-mono"
      onChange={(e) => ctx.set(k, e.target.value)}
    />
  )
}

function Num({ ctx, k, step }: { ctx: Ctx; k: string; step?: number }) {
  return (
    <Input
      id={k}
      type="number"
      step={step ?? 1}
      value={String(getPath(ctx.draft, k) ?? "")}
      disabled={!editable(ctx, k)}
      aria-invalid={!!ctx.errors[k]}
      onChange={(e) => ctx.set(k, e.target.value === "" ? "" : Number(e.target.value))}
    />
  )
}

function Toggle({ ctx, k }: { ctx: Ctx; k: string }) {
  return <Switch id={k} checked={!!getPath(ctx.draft, k)} disabled={!editable(ctx, k)} onCheckedChange={(v) => ctx.set(k, v)} />
}

function Choice({ ctx, k, options }: { ctx: Ctx; k: string; options: Record<string, string> }) {
  const value = String(getPath(ctx.draft, k))
  return (
    <Select value={value} disabled={!editable(ctx, k)} onValueChange={(v) => ctx.set(k, v)}>
      <SelectTrigger id={k} className="w-full" aria-invalid={!!ctx.errors[k]}>
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
    <div className="flex gap-4">
      {(["A", "AAAA"] as const).map((t) => (
        <Label key={t} className="flex items-center gap-2 font-mono">
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
}

/** An editable list of rows, such as `[[dns.resolvers]]`, with add and remove. */
function Rows({ ctx, k, title, cols, blank }: { ctx: Ctx; k: string; title: string; cols: Col[]; blank: unknown }) {
  const rows = (getPath(ctx.draft, k) as unknown[]) ?? []
  const can = editable(ctx, k)
  const lock = lockOf(ctx, k)
  const cellKey = (i: number, c: Col) => (c.key ? `${k}.${i}.${c.key}` : `${k}.${i}`)
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <Label className="flex items-center gap-2">
          {title}
          {lock ? (
            <Badge variant="secondary" title={lock.why}>
              <Lock /> {lock.label}
            </Badge>
          ) : (
            <SourceBadge source={sourceOf(k, ctx.view.sources)} />
          )}
        </Label>
        {can && (
          <Button variant="ghost" size="sm" onClick={() => ctx.set(k, [...rows, structuredClone(blank)])}>
            <Plus /> Add
          </Button>
        )}
      </div>
      {ctx.errors[k] && <p className="text-xs text-destructive">{ctx.errors[k]}</p>}
      {rows.length === 0 && <p className="text-sm text-muted-foreground">None.</p>}
      {rows.map((row, i) => (
        <div key={i} className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
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
                  className="min-w-0 flex-1 font-mono"
                  onChange={(e) => {
                    const v = e.target.value
                    const value = c.list ? v.split(",").map((s) => s.trim()).filter(Boolean) : c.nullable && v.trim() === "" ? null : v
                    ctx.set(cellKey(i, c), value)
                  }}
                />
              )
            })}
            {can && (
              <Button variant="ghost" size="icon" aria-label={`Remove ${title} ${i + 1}`} onClick={() => ctx.set(k, rows.filter((_, j) => j !== i))}>
                <Trash2 />
              </Button>
            )}
          </div>
          {cols.map((c) => ctx.errors[cellKey(i, c)] && <p key={c.key} className="text-xs text-destructive">{ctx.errors[cellKey(i, c)]}</p>)}
        </div>
      ))}
    </div>
  )
}

const HOPS = ["gateway", "isp", "internet", "dns", "targets"] as const
const LIMITS: [keyof Limits, string][] = [
  ["rtt_warn_ms", "RTT ms"],
  ["jitter_warn_ms", "Jitter ms"],
  ["loss_warn_pct", "Loss %"],
]

function ThresholdsCard({ ctx }: { ctx: Ctx }) {
  const bad = HOPS.flatMap((h) => LIMITS.map(([l]) => `thresholds.${h}.${l}`)).filter((k) => ctx.errors[k])
  return (
    <Section title="Thresholds" description="Above any of these a hop is degraded">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Hop</TableHead>
            {LIMITS.map(([, label]) => (
              <TableHead key={label}>{label}</TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {HOPS.map((h) => (
            <TableRow key={h}>
              <TableCell className="font-mono text-xs">{h}</TableCell>
              {LIMITS.map(([l, label]) => {
                const k = `thresholds.${h}.${l}`
                return (
                  <TableCell key={l}>
                    <Input
                      type="number"
                      step="any"
                      aria-label={`${h} ${label}`}
                      value={String(getPath(ctx.draft, k) ?? "")}
                      disabled={!editable(ctx, k)}
                      aria-invalid={!!ctx.errors[k]}
                      onChange={(e) => ctx.set(k, e.target.value === "" ? "" : Number(e.target.value))}
                    />
                  </TableCell>
                )
              })}
            </TableRow>
          ))}
        </TableBody>
      </Table>
      {bad.map((k) => (
        <p key={k} className="text-xs text-destructive">
          <code className="font-mono">{k.slice("thresholds.".length)}</code> {ctx.errors[k]}
        </p>
      ))}
    </Section>
  )
}

const BLANK_TARGET: UserTargetConfig = { name: "", check: "", critical: false, verify_tls: true }

/** `[[targets]]`: a table with add, edit (in a dialog), and remove. */
function TargetsCard({ ctx }: { ctx: Ctx }) {
  const [editing, setEditing] = useState<{ index: number | null; target: UserTargetConfig } | null>(null)
  const targets = ctx.draft.targets
  const can = editable(ctx, "targets")
  const rowError = (i: number) => ["name", "check"].map((f) => ctx.errors[`targets.${i}.${f}`]).find(Boolean)
  return (
    <Card>
      <CardHeader>
        <CardTitle>Targets</CardTitle>
        <CardDescription>Your own things to watch. A failing critical target degrades the verdict and can open an incident.</CardDescription>
        {can && (
          <CardAction>
            <Button variant="outline" onClick={() => setEditing({ index: null, target: { ...BLANK_TARGET } })}>
              <Plus /> Add target
            </Button>
          </CardAction>
        )}
      </CardHeader>
      <CardContent>
        {targets.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            None yet. Add a NAS (<code className="font-mono">tcp://nas.local:445</code>), a router page, a printer, or a Pi-hole.
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Check</TableHead>
                <TableHead>Critical</TableHead>
                <TableHead>Verify TLS</TableHead>
                {can && <TableHead className="text-right">Actions</TableHead>}
              </TableRow>
            </TableHeader>
            <TableBody>
              {targets.map((t, i) => (
                <TableRow key={i}>
                  <TableCell>
                    {t.name}
                    {rowError(i) && <p className="text-xs text-destructive">{rowError(i)}</p>}
                  </TableCell>
                  <TableCell className="font-mono text-xs whitespace-normal break-all">{t.check}</TableCell>
                  <TableCell>
                    <Switch
                      aria-label={`${t.name} critical`}
                      checked={t.critical}
                      disabled={!can}
                      onCheckedChange={(v) => ctx.set(`targets.${i}.critical`, v)}
                    />
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">{t.check.startsWith("https://") ? (t.verify_tls ? "yes" : "no") : "—"}</TableCell>
                  {can && (
                    <TableCell className="text-right">
                      <Button variant="ghost" size="icon" aria-label={`Edit ${t.name}`} onClick={() => setEditing({ index: i, target: { ...t } })}>
                        <Pencil />
                      </Button>
                      <Button variant="ghost" size="icon" aria-label={`Remove ${t.name}`} onClick={() => ctx.set("targets", targets.filter((_, j) => j !== i))}>
                        <Trash2 />
                      </Button>
                    </TableCell>
                  )}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
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
    </Card>
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

function SourceBadge({ source }: { source: ConfigSource }) {
  if (source === "default") return null
  return <Badge variant="outline">{source}</Badge>
}
