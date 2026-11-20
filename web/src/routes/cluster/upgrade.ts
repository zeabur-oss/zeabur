// The cluster's upgrade state in one pill, shared by the cluster page and the home page.
import type { Schemas } from "@/api/client"
import { useI18n, useT } from "@/i18n"
import type { Health } from "@/model/service"

/** A `YYYY-MM-DD` date in the person's locale. */
export function useDate() {
  const { locale } = useI18n()
  return (date: string) =>
    new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeZone: "UTC" }).format(
      new Date(`${date}T00:00:00Z`),
    )
}

/** The one upgrade fact the cluster's header and the home page show: the server's badge,
 * worded. */
export function useUpgradeBadge(
  upgrades?: Schemas.Upgrades,
): { health: Health; text: string } | null {
  const t = useT()
  const date = useDate()
  const badge = upgrades?.badge
  if (!upgrades || !badge) return null
  const eol = upgrades.release?.endOfLifeAt
  const words = {
    "end-of-life": () => t("cluster.endOfLife"),
    maintenance: () => t("cluster.endOfLifeOn", eol ? date(eol) : ""),
    "skew-unsupported": () => t("cluster.skewBadge"),
    upgrading: () => t("cluster.upgrading"),
    "patch-available": () => t("cluster.available", upgrades.patch ?? ""),
    "minor-available": () => t("cluster.available", upgrades.nextMinor ?? ""),
    "skew-mixed": () => null,
    "newer-than-data": () => null,
  }
  // A state newer than this build shows no pill.
  const text = Object.hasOwn(words, badge.state) ? words[badge.state]() : null
  return text === null ? null : { health: badge.health, text }
}
