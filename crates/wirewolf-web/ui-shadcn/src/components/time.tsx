import { useEffect, useState } from "react"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
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

/** "3 min ago", with the absolute time on hover and focus. */
export function RelTime({ at, className, plain }: { at: string; className?: string; plain?: boolean }) {
  const now = useNow()
  // Inside links and buttons a focusable tooltip would nest interactive elements; use title.
  if (plain)
    return (
      <time dateTime={at} title={absolute(at)} className={className}>
        {relative(at, now)}
      </time>
    )
  return (
    <Tooltip>
      <TooltipTrigger
        render={<time dateTime={at} tabIndex={0} />}
        className={"cursor-default underline decoration-dotted decoration-muted-foreground/40 underline-offset-4 " + (className ?? "")}
      >
        {relative(at, now)}
      </TooltipTrigger>
      <TooltipContent>{absolute(at)}</TooltipContent>
    </Tooltip>
  )
}
