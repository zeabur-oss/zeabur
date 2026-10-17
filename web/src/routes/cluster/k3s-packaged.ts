// The warning for a component k3s also packages (Traefik, metrics-server): on k3s the packaged
// copy must be disabled first, and disabling it removes what it installed. Read from the guide's
// `replacesK3sPackaged`, never from the texts. https://docs.k3s.io/installation/packaged-components
import type { Translate } from "@/i18n"

/** One sentence per packaged component: how to disable it, then what that removes. */
export function k3sPackagedNotes(addons: readonly string[], t: Translate): string[] {
  return addons.map(
    (addon) =>
      `${t("install.k3sPackaged", addon)} ${
        addon === "traefik"
          ? t("install.k3sPackagedTraefik")
          : addon === "metrics-server"
            ? t("install.k3sPackagedMetrics")
            : t("install.k3sPackagedOther")
      }`,
  )
}
