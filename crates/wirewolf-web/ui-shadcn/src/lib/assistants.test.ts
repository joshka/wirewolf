import { describe, expect, it } from "vitest"
import { ASSISTANTS, assistantUrl } from "./assistants"

describe("assistantUrl", () => {
  const chatgpt = ASSISTANTS.find((a) => a.name === "ChatGPT")!
  const gemini = ASSISTANTS.find((a) => a.name === "Gemini")!
  it("prefills when supported and short enough", () => {
    expect(assistantUrl(chatgpt, "a b&c")).toEqual({ url: "https://chatgpt.com/?q=a%20b%26c", prefilled: true })
  })
  it("falls back to the home page", () => {
    expect(assistantUrl(gemini, "x")).toEqual({ url: gemini.home, prefilled: false })
    expect(assistantUrl(chatgpt, "x".repeat(9000)).prefilled).toBe(false)
  })
})
