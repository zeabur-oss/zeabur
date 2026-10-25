// The warning for a component K3s and RKE2 also package (Traefik, metrics-server): the packaged
// copy must be turned off first, and that removes what it installed. Read from the guide's
// `replacesK3sPackaged`, never from the texts. https://docs.k3s.io/installation/packaged-components,
// https://docs.rke2.io/advanced, https://docs.rke2.io/networking/networking_services
import type { Translate } from "@/i18n"

/** One paragraph per packaged component: how to turn it off on K3s and on RKE2, then what that
 * removes. */
export function k3sPackagedNotes(addons: readonly string[], t: Translate): string[] {
  return addons.map((addon) =>
    t("common.sentences", [
      t("install.k3sPackaged", addon),
      addon === "traefik"
        ? t("install.rke2PackagedIngress")
        : t("install.rke2Packaged", `rke2-${addon}`),
      addon === "traefik"
        ? t("install.k3sPackagedTraefik")
        : addon === "metrics-server"
          ? t("install.k3sPackagedMetrics")
          : t("install.k3sPackagedOther"),
    ]),
  )
}
