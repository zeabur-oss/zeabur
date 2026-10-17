import { QueryClientProvider } from "@tanstack/react-query"
import { RouterProvider } from "@tanstack/react-router"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import { queryClient } from "@/api/queries"
import { followElevation } from "@/auth/follow-elevation"
import { I18nProvider, prepareI18n } from "@/i18n"
import { router } from "@/router"
import "@/styles/app.css"
import { followSystemTheme } from "@/theme"

followSystemTheme()
followElevation(queryClient)

const root = document.getElementById("root")
// A non-English catalogue loads before the first render, so it never flashes English.
if (root) {
  void prepareI18n().then(() =>
    createRoot(root).render(
      <StrictMode>
        <I18nProvider>
          <QueryClientProvider client={queryClient}>
            <RouterProvider router={router} />
          </QueryClientProvider>
        </I18nProvider>
      </StrictMode>,
    ),
  )
}
