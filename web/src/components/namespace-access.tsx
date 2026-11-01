// RBAC cannot narrow a patch to one label, so without the chart's admission policy the right also
// covers the Pod Security Admission labels: https://kubernetes.io/docs/concepts/security/pod-security-admission/#pod-security-admission-labels-for-namespaces
import { useQuery } from "@tanstack/react-query"
import WarningFillIcon from "~icons/ph/warning-fill"

import { whoamiQuery } from "@/api/queries"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { kubectlLabel, patchRole } from "@/model/project"

/** `manage`: the change also needs `zeabur-manage` (a label, where the policy guards labels).
 * `guarded`: the policy keeps a plain patch to the project's details. */
export function NamespaceAccess({
  namespace,
  adopted,
  manage,
  guarded,
}: {
  namespace: string
  adopted: boolean
  manage: boolean
  guarded: boolean
}) {
  const t = useT()
  const whoami = useQuery(whoamiQuery)
  const user = whoami.data?.username || t("access.you")
  const commands = kubectlLabel(namespace)
  return (
    <div className="flex min-w-0 flex-col gap-3 text-sm">
      <p>{manage ? t("access.askManage", namespace) : t("access.ask", namespace)}</p>
      {guarded && !manage ? (
        <p className="text-muted-foreground">{t("access.detailsOnly")}</p>
      ) : (
        <Alert variant="warn" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertDescription>{t("access.alsoLabels")}</AlertDescription>
        </Alert>
      )}
      <p className="text-muted-foreground">{t("access.role")}</p>
      <YamlBlock yaml={patchRole(namespace, user, manage)} label={t("access.roleLabel")} />
      <p className="text-muted-foreground">{t("access.kubectl")}</p>
      <YamlBlock
        yaml={`${adopted ? commands.unadopt : commands.adopt}\n`}
        label={t("access.kubectlLabel")}
      />
    </div>
  )
}
