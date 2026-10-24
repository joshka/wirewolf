import { useEffect, useState } from "react"
import { absolute, clock, duration, relative } from "@/lib/format"

/** The current time, re-rendering every `everyMs`. */
export function useNow(everyMs = 15000): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), everyMs)
    return () => clearInterval(t)
  }, [everyMs])
  return now
}

/** "3 min ago", with the absolute time in the title. */
export function RelTime({ at, className }: { at: string; className?: string }) {
  const now = useNow()
  return (
    <time dateTime={at} title={absolute(at)} className={className}>
      {relative(at, now)}
    </time>
  )
}

/**
 * "for 12 min (since 09:41)": how long a state has lasted, which answers "since when?" better
 * than a relative start time ("since now" reads oddly while it's fresh).
 */
export function Lasting({ since, className }: { since: string; className?: string }) {
  const now = useNow()
  return (
    <span className={className}>
      for{" "}
      <time dateTime={since} title={absolute(since)} className="text-foreground">
        {duration(since, null, now)}
      </time>{" "}
      (since {clock(since)})
    </span>
  )
}
