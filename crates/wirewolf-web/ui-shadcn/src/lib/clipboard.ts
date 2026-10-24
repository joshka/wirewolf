/** Copies text, falling back to a hidden textarea where the async clipboard API is missing. */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    // Plain HTTP on a LAN address isn't a secure context, so the async clipboard API is
    // missing. Fall back to a hidden textarea.
    const ta = document.createElement("textarea")
    ta.value = text
    ta.style.position = "fixed"
    ta.style.opacity = "0"
    document.body.append(ta)
    ta.select()
    document.execCommand("copy")
    ta.remove()
  }
}

/** Saves text as a file through a temporary link. */
export function downloadText(text: string, filename: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: "text/markdown;charset=utf-8" }))
  const a = document.createElement("a")
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}
