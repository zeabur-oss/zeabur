import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "@/styles/app.css"
import { followSystemTheme } from "@/theme"

import { I18nProvider } from "@/i18n"

import { App } from "./app"
import { keepContextMenuForText } from "./menu"

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })

followSystemTheme()
document.documentElement.classList.add("bg-surface-page", "overscroll-none")
keepContextMenuForText()

const root = document.getElementById("root")
if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <I18nProvider>
          <App />
        </I18nProvider>
      </QueryClientProvider>
    </StrictMode>,
  )
}
