// Capability names from the components catalogue, in the person's language.
import { useT } from "@/i18n"

const knownCapabilities = [
  "certificates",
  "dns",
  "event-autoscaling",
  "ingress",
  "intrusion-prevention",
  "logs",
  "metrics",
  "node-updates",
  "object-storage",
  "observability-operator",
  "postgres",
  "resource-recommendations",
  "runtime-security",
  "uptime-probes",
  "volume-backup",
  "vulnerability-scanning",
] as const
type KnownCapability = (typeof knownCapabilities)[number]

/** A capability's name in the person's language (the catalogue's id when it is new). */
export function useCapabilityLabel() {
  const t = useT()
  return (name: string) =>
    (knownCapabilities as readonly string[]).includes(name)
      ? t(`cap.${name as KnownCapability}`)
      : name
}
