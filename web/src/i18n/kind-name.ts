import type { Translate } from "@/i18n"

/** The plain word for a Kubernetes kind people work with; any other kind as it is. */
export function kindName(t: Translate, kind: string): string {
  switch (kind) {
    case "Pod":
      return t("kind.Pod")
    case "PersistentVolumeClaim":
      return t("kind.PersistentVolumeClaim")
    case "Secret":
      return t("kind.Secret")
    case "ConfigMap":
      return t("kind.ConfigMap")
    default:
      return kind
  }
}
