import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import "./index.css"
import App from "./App.tsx"

// Save the rendered page for index.html to show while the next load starts up.
addEventListener("pagehide", () => {
  try {
    const html = document.getElementById("root")!.innerHTML
    sessionStorage.setItem("wirewolf.shell", JSON.stringify({ url: location.href, savedAt: Date.now(), html }))
  } catch {
    // Quota or privacy mode: the saved page is only an optimisation.
  }
})

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
