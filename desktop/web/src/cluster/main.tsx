import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "@/styles/app.css"
import { I18nProvider } from "@/i18n"
import { followSystemTheme } from "@/theme"

import { ClusterPage } from "./page"

const queryClient = new QueryClient()

followSystemTheme()
document.documentElement.classList.add("bg-surface-page", "overscroll-none")

const root = document.getElementById("root")
if (root) {
  createRoot(root).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <I18nProvider>
          <ClusterPage />
        </I18nProvider>
      </QueryClientProvider>
    </StrictMode>,
  )
}
