// What to ask for when the person may not patch a namespace: the right (patch on that one
// Namespace, limited with `resourceNames`), what else that right allows (RBAC cannot narrow a
// patch to one label, so it covers the Pod Security Admission labels too:
// https://kubernetes.io/docs/concepts/security/pod-security-admission/#pod-security-admission-labels-for-namespaces),
// a ClusterRole and binding an administrator can apply, and the kubectl command that does what
// engawa's plan would.
import { useQuery } from "@tanstack/react-query"
import WarningFillIcon from "~icons/ph/warning-fill"

import { whoamiQuery } from "@/api/queries"
import { Alert, AlertDescription } from "@engawahq/design-system/components/alert"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { kubectlLabel, patchRole } from "@/model/project"

export function NamespaceAccess({ namespace, adopted }: { namespace: string; adopted: boolean }) {
  const t = useT()
  const whoami = useQuery(whoamiQuery)
  const user = whoami.data?.username || t("access.you")
  const commands = kubectlLabel(namespace)
  return (
    <div className="flex min-w-0 flex-col gap-3 text-sm">
      <p>{t("access.ask", namespace)}</p>
      <Alert variant="warn" role="note">
        <WarningFillIcon aria-hidden="true" />
        <AlertDescription>{t("access.alsoLabels")}</AlertDescription>
      </Alert>
      <p className="text-muted-foreground">{t("access.role")}</p>
      <YamlBlock yaml={patchRole(namespace, user)} label={t("access.roleLabel")} />
      <p className="text-muted-foreground">{t("access.kubectl")}</p>
      <YamlBlock
        yaml={`${adopted ? commands.unadopt : commands.adopt}\n`}
        label={t("access.kubectlLabel")}
      />
    </div>
  )
}
