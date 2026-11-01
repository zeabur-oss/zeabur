import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { App } from "@/app"
import { I18nProvider, prepareI18n } from "@/i18n"
import "@/styles/app.css"
import { followSystemTheme } from "@/theme"
import { TooltipProvider } from "@zeabur-oss/design-system/components/tooltip"

followSystemTheme()

const root = document.getElementById("root")
// A non-English catalogue loads before the first render, so it never flashes English.
if (root) {
  void prepareI18n().then(() =>
    createRoot(root).render(
      <StrictMode>
        <I18nProvider>
          <TooltipProvider>
            <App />
          </TooltipProvider>
        </I18nProvider>
      </StrictMode>,
    ),
  )
}
