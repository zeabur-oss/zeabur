import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router"

import type { Use } from "@/api"
import { useWizard, WizardPage } from "@/app"
import { AccessStep } from "@/steps/access"
import { ComponentsStep } from "@/steps/components"
import { HostStep } from "@/steps/host"
import { InstallStep } from "@/steps/install"
import { ReviewStep } from "@/steps/review"
import { SettingsStep } from "@/steps/settings"

const rootRoute = createRootRoute({
  validateSearch: (search: Record<string, unknown>): { use?: Use } =>
    search.use === "personal" || search.use === "team" ? { use: search.use } : {},
  component: WizardPage,
})

const hostRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => <HostStep wizard={useWizard()} />,
})
const accessRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/access",
  component: () => <AccessStep wizard={useWizard()} />,
})
const componentsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/components",
  component: () => <ComponentsStep wizard={useWizard()} />,
})
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: () => <SettingsStep wizard={useWizard()} />,
})
const reviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/review",
  component: () => <ReviewStep wizard={useWizard()} />,
})
const installRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/install",
  component: () => <InstallStep wizard={useWizard()} />,
})

export const router = createRouter({
  routeTree: rootRoute.addChildren([
    hostRoute,
    accessRoute,
    componentsRoute,
    settingsRoute,
    reviewRoute,
    installRoute,
  ]),
})

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}
