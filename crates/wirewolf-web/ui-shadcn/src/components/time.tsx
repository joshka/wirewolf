import { useEffect, useState } from "react"
import { absolute, relative } from "@/lib/format"

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
