import type { ReactNode } from "react"

/**
 * A page's title and one-line description, in the same style as the Settings header. Now has
 * no header: its verdict headline is the page's h1.
 */
export function PageHeader({ title, description, children }: { title: string; description: ReactNode; children?: ReactNode }) {
  return (
    <header className="flex flex-col gap-1 pb-2">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {children}
      </div>
      <p className="text-sm text-muted-foreground">{description}</p>
    </header>
  )
}
