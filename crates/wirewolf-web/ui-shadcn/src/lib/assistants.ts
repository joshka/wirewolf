/** Chat assistants the "Ask an AI" dialog can open, and how each takes a prefilled prompt. */
export interface Assistant {
  name: string
  /** The new-chat page. */
  home: string
  /** Prefix for a prefilled prompt; the text is appended URL-encoded. Missing if unsupported. */
  prefill?: string
}

export const ASSISTANTS: Assistant[] = [
  { name: "ChatGPT", home: "https://chatgpt.com/", prefill: "https://chatgpt.com/?q=" },
  { name: "Claude", home: "https://claude.ai/new", prefill: "https://claude.ai/new?q=" },
  { name: "Gemini", home: "https://gemini.google.com/app" },
  { name: "Perplexity", home: "https://www.perplexity.ai/", prefill: "https://www.perplexity.ai/search?q=" },
  { name: "Grok", home: "https://grok.com/", prefill: "https://grok.com/?q=" },
]

/** Longer URLs get cut off or refused, so past this the user pastes instead. */
const MAX_URL = 8000

/** The URL to open for `text`: prefilled when the assistant supports it and it fits. */
export function assistantUrl(a: Assistant, text: string): { url: string; prefilled: boolean } {
  if (a.prefill) {
    const url = a.prefill + encodeURIComponent(text)
    if (url.length <= MAX_URL) return { url, prefilled: true }
  }
  return { url: a.home, prefilled: false }
}
