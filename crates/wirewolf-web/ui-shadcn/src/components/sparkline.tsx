import { useId } from "react"
import type { Sample } from "@/lib/api"
import { SPARK_WINDOW_MS } from "@/lib/live"

/**
 * A fixed-window RTT sparkline (plain SVG, no chart library: cheap to re-render every 2s).
 * Loss shows as red ticks along the bottom. The x axis is wall time, so new points slide in.
 */
export function Sparkline({
  samples,
  now,
  className,
  label,
  color,
}: {
  samples: Sample[]
  now: number
  className?: string
  label: string
  color?: string
}) {
  const id = useId()
  const W = 120
  const H = 32
  const pts = samples.filter((s) => s.rtt_ms != null)
  // Live mode samples every 30s; a line from one or two points just looks like a glitch.
  if (pts.length < 3) {
    return <div className={className} aria-hidden />
  }
  const t0 = now - SPARK_WINDOW_MS
  const max = Math.max(1, ...pts.map((s) => s.rtt_ms!)) * 1.15
  const x = (at: string) => ((new Date(at).getTime() - t0) / SPARK_WINDOW_MS) * W
  const y = (v: number) => H - 2 - (v / max) * (H - 6)
  const line = pts.map((s, i) => `${i ? "L" : "M"}${x(s.at).toFixed(1)},${y(s.rtt_ms!).toFixed(1)}`).join("")
  const area = pts.length ? `${line}L${x(pts[pts.length - 1].at).toFixed(1)},${H}L${x(pts[0].at).toFixed(1)},${H}Z` : ""
  const lossy = samples.filter((s) => (s.loss_pct ?? 0) > 0)
  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      className={className}
      style={color ? { color } : undefined}
      role="img"
      aria-label={label}
    >
      <defs>
        <linearGradient id={id} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="currentColor" stopOpacity="0.25" />
          <stop offset="100%" stopColor="currentColor" stopOpacity="0" />
        </linearGradient>
      </defs>
      {area && <path d={area} fill={`url(#${id})`} />}
      {line && <path d={line} fill="none" stroke="currentColor" strokeWidth="1.5" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />}
      {lossy.map((s) => (
        <rect key={s.at} x={x(s.at) - 0.6} y={H - Math.min(H, 3 + (s.loss_pct! / 100) * H)} width="1.2" height={Math.min(H, 3 + (s.loss_pct! / 100) * H)} className="fill-fail" />
      ))}
    </svg>
  )
}
