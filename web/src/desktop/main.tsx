import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "@/styles/app.css"
import { followSystemTheme } from "@/theme"

import { App } from "./app"
import { I18n, systemLang } from "./i18n"

followSystemTheme()
document.documentElement.classList.add("bg-surface-page")

const root = document.getElementById("root")
if (root) {
  createRoot(root).render(
    <StrictMode>
      <I18n initial={systemLang()}>
        <App />
      </I18n>
    </StrictMode>,
  )
}
