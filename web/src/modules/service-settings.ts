// A module is compiled in by adding its entry here; the core never imports a module.
import type { ComponentType } from "react"

import type { PlanAsk } from "@/components/plan/planner"
import type { Service } from "@/model/service"

/** @public No module is compiled in yet. */
export interface ServiceSettingsProps {
  project: string
  service: Service
  plan: (ask: PlanAsk) => void
}

export interface ServiceSettingsSection {
  /** As `zeabur-modules` names it (`network-policy`). */
  module: string
  /** A key in the module's own strings. */
  title: string
  applies: (service: Service) => boolean
  Section: ComponentType<ServiceSettingsProps>
}

export const serviceSettingsSections: readonly ServiceSettingsSection[] = []
