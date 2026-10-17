// Settings sections that modules add to a service's Settings tab, after the built-in ones
// (compute, health checks, storage). A module that manages something about a service (TLS from
// cert-manager, autoscaling from KEDA) lists a section here; its form turns input into plan
// changes of the change types its provider adds on the server, and every write still goes
// through the plan dialog. The list is explicit, like the server's module list: a module is
// compiled in by adding its entry, and the core never imports a module.
import type { ComponentType } from "react"

import type { PlanAsk } from "@/components/plan/planner"
import type { Service } from "@/model/service"

/** @public What a module's section receives; no module is compiled in yet. */
export interface ServiceSettingsProps {
  project: string
  service: Service
  /** Opens the plan dialog for the section's changes. */
  plan: (ask: PlanAsk) => void
}

export interface ServiceSettingsSection {
  /** The module's id, as `engawa-modules` names it (`network-policy`). */
  module: string
  /** The section's heading key in the module's own strings. */
  title: string
  /** Whether the section applies to this service (a kind, a detected provider). */
  applies: (service: Service) => boolean
  /** The section, loaded lazily by its module. */
  Section: ComponentType<ServiceSettingsProps>
}

/** Every module section, in the order they show. None yet. */
export const serviceSettingsSections: readonly ServiceSettingsSection[] = []
