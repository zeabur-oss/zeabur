import type { Schemas } from "@/api/client"
import type { Translate } from "@/i18n"

/** Where the cluster's name comes from, in one short line. */
export function nameSourceOf(cluster: Schemas.Cluster, t: Translate): string {
  switch (cluster.nameSource) {
    case "setting":
      return t("shell.nameFrom.setting")
    case "install":
      return t("shell.nameFrom.install")
    case "address":
      return t("shell.nameFrom.address")
    case "kubeconfig":
      return t("shell.nameFrom.kubeconfig")
  }
}
