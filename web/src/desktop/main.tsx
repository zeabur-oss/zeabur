import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "@/styles/app.css"
import { followSystemTheme } from "@/theme"

import { I18nProvider, prepareI18n } from "@/i18n"

import { App } from "./app"

followSystemTheme()
document.documentElement.classList.add("bg-surface-page")

const root = document.getElementById("root")
if (root) {
  void prepareI18n().then(() =>
    createRoot(root).render(
      <StrictMode>
        <I18nProvider>
          <App />
        </I18nProvider>
      </StrictMode>,
    ),
  )
}
