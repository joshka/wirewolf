import { useEffect, useState } from "react"
import { Check, ChevronDown, ClipboardCopy, Download, ExternalLink, Sparkles } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { api } from "@/lib/api"
import { ASSISTANTS, assistantUrl, type Assistant } from "@/lib/assistants"
import { copyText, downloadText } from "@/lib/clipboard"

/**
 * "Ask an AI": shows the prompt plus brief (`GET /brief/prompt`) to copy into any chat assistant,
 * or download. Nothing is sent anywhere by wirewolf. With `incident`, the brief describes that
 * incident's evidence instead of now.
 */
export function AskAi({ incident, size }: { incident?: string; size?: "sm" | "default" }) {
  const [open, setOpen] = useState(false)
  const [redact, setRedact] = useState(true)
  const [text, setText] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!open) return
    let current = true
    api
      .briefPrompt({ incident, redact })
      .then((t) => {
        if (!current) return
        setText(t)
        setFailed(false)
      })
      .catch(() => current && setFailed(true))
    return () => {
      current = false
    }
  }, [open, redact, incident])

  const copy = async () => {
    if (!text) return
    await copyText(text)
    setCopied(true)
    toast.success("Copied", { description: "Paste it into ChatGPT, Claude, Gemini, or any chat assistant." })
    setTimeout(() => setCopied(false), 2000)
  }
  // Copy too, so a site that ignores the prefill (or a prompt too long for a URL) is one paste away.
  const openIn = (a: Assistant) => {
    if (!text) return
    const { url, prefilled } = assistantUrl(a, text)
    window.open(url, "_blank", "noopener,noreferrer")
    void copyText(text)
    toast.success(`Opened ${a.name}`, { description: prefilled ? "The prompt is filled in, and also copied in case it isn't." : "The prompt is copied: paste it into the chat." })
  }
  const download = () => text && downloadText(text, `wirewolf-${incident ?? "brief"}-${new Date().toISOString().slice(0, 16).replace(":", "")}.md`)

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button variant="outline" size={size} />}>
        <Sparkles aria-hidden />
        Ask an AI
      </DialogTrigger>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Ask an AI about {incident ? "this incident" : "your network"}</DialogTitle>
          <DialogDescription>
            Copy this into any chat assistant, or open one with it filled in. It's a compact summary of what wirewolf measured, with a request to explain it. wirewolf doesn't send it
            anywhere; if your internet is down, paste it from your phone.
          </DialogDescription>
        </DialogHeader>
        <textarea
          readOnly
          aria-label="Prompt and brief"
          value={failed ? "Couldn't build the brief. Is the server still measuring?" : (text ?? "Loading…")}
          onFocus={(e) => e.currentTarget.select()}
          className="h-80 w-full resize-none rounded-lg border bg-muted/40 p-3 font-mono text-xs leading-relaxed outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
        />
        <Label className="flex items-center justify-between gap-4">
          <span className="flex flex-col gap-1">
            Hide public IP addresses
            <span className="text-xs font-normal text-muted-foreground">Local, provider-internal, and well-known resolver addresses stay: they help the diagnosis</span>
          </span>
          <Switch checked={redact} onCheckedChange={setRedact} />
        </Label>
        <DialogFooter>
          <Button variant="outline" onClick={download} disabled={!text}>
            <Download aria-hidden /> Download
          </Button>
          <div className="flex">
            <Button onClick={copy} disabled={!text} className="rounded-r-none">
              {copied ? <Check aria-hidden /> : <ClipboardCopy aria-hidden />}
              {copied ? "Copied" : "Copy"}
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button disabled={!text} aria-label="Open in a chat assistant" className="rounded-l-none border-l border-primary-foreground/25 px-2" />}>
                <ChevronDown aria-hidden />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuGroup>
                  <DropdownMenuLabel>Open in</DropdownMenuLabel>
                  {ASSISTANTS.map((a) => (
                    <DropdownMenuItem key={a.name} onClick={() => openIn(a)}>
                      <ExternalLink aria-hidden />
                      {a.name}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
