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

/** The one upgrade fact the cluster's header and the home page show: the most severe state,
 * worded. */
export function useUpgradeBadge(
  upgrades?: Schemas.Upgrades,
): { health: Health; text: string } | null {
  const t = useT()
  const date = useDate()
  if (!upgrades) return null
  const eol = upgrades.release?.endOfLife
  for (const state of upgrades.states) {
    switch (state) {
      case "end-of-life":
        return { health: "fail", text: t("cluster.endOfLife") }
      case "maintenance":
        return { health: "warn", text: t("cluster.endOfLifeOn", eol ? date(eol) : "") }
      case "skew-unsupported":
        return { health: "fail", text: t("cluster.skewBadge") }
      case "upgrading":
        return { health: "progress", text: t("cluster.upgrading") }
      case "patch-available":
        return { health: "progress", text: t("cluster.available", upgrades.patch ?? "") }
      case "minor-available":
        return { health: "progress", text: t("cluster.available", upgrades.nextMinor ?? "") }
      case "skew-mixed":
      case "newer-than-data":
        break
    }
  }
  return null
}
