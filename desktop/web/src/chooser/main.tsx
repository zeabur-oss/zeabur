import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "@/styles/app.css"
import { followSystemTheme } from "@/theme"

import { I18nProvider } from "@/i18n"

import { App } from "./app"
import { keepContextMenuForText } from "./menu"

followSystemTheme()
document.documentElement.classList.add("bg-surface-page", "overscroll-none")
keepContextMenuForText()

const root = document.getElementById("root")
if (root) {
  createRoot(root).render(
    <StrictMode>
      <I18nProvider>
        <App />
      </I18nProvider>
    </StrictMode>,
  )
}
