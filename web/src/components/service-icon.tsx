import ClockIcon from "~icons/ph/clock"
import DatabaseIcon from "~icons/ph/database"
import GearSixIcon from "~icons/ph/gear-six"
import GlobeIcon from "~icons/ph/globe"
import LightningIcon from "~icons/ph/lightning"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import PlugsConnectedIcon from "~icons/ph/plugs-connected"
import QueueIcon from "~icons/ph/queue"

import type { Icon } from "@/lib/icon"
import { cn } from "@/lib/utils"
import type { Schemas } from "@/api/client"
import { roleOf, type Role, type Service } from "@/model/service"

const flavourIcons: Record<Schemas["DataStore"], Icon> = {
  postgres: DatabaseIcon,
  mysql: DatabaseIcon,
  mongo: DatabaseIcon,
  redis: LightningIcon,
  queue: QueueIcon,
  search: MagnifyingGlassIcon,
  other: DatabaseIcon,
}

const roleIcons: Record<Role, Icon> = {
  web: GlobeIcon,
  internal: PlugsConnectedIcon,
  worker: GearSixIcon,
  job: ClockIcon,
  data: DatabaseIcon,
}

const glyphs: Record<string, Icon> = {
  ...Object.fromEntries(Object.entries(flavourIcons).map(([k, v]) => [`flavour:${k}`, v])),
  ...Object.fromEntries(Object.entries(roleIcons).map(([k, v]) => [`role:${k}`, v])),
}

function glyphKey(service: Service): string {
  return service.dataStore ? `flavour:${service.dataStore}` : `role:${roleOf(service)}`
}

export function ServiceIcon({
  service,
  size = "md",
  className,
}: {
  service: Service
  size?: "sm" | "md" | "lg"
  className?: string
}) {
  const Glyph = glyphs[glyphKey(service)] ?? GearSixIcon
  const glyph = size === "sm" ? "size-3.5" : size === "lg" ? "size-5" : "size-4"
  return (
    <Glyph aria-hidden="true" className={cn("shrink-0 text-muted-foreground", glyph, className)} />
  )
}

export function RoleIcon({ role, className }: { role: Role; className?: string }) {
  const Glyph = roleIcons[role]
  return <Glyph aria-hidden="true" className={cn("shrink-0", className)} />
}
